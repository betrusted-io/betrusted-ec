#![no_std]
#![forbid(unsafe_code)]
//! This crate provides a minimalist IP stack with packet filtering.
//!
//! The code here is a significant departure from POSIX style networking APIs you may be
//! familiar with. Guiding principle of this design is to provide minimalist connectivity
//! for updates and chat while keeping attack surface as small as possible.
//!
//! Goals:
//! 1. Provide transport layer support for downloading software updates
//! 2. Provide transport layer support for Matrix chat
//! 3. Keep binary size small enough for EC (UP5K) to run a packet filter, dropping
//!    broadcast chatter, port scans, and other noise, so that SoC (XC7S) can sleep more
//!    to make battery last longer
//! 4. Implement IP standards well enough to interoperate reliably, blend in with other
//!    hosts, and avoid irritating the local network admins
//! 5. Take advantage of Rust's safety features: In particular, whenever possible, do
//!    packet parsing and assembly in modules protected by `#[forbid(unsafe_code)]`
//!
//! Anti-Goals:
//! 1. Supporting web browsing is not a priority
//! 2. Strict implementation of all "required" features in RFC 791 (Internet Protocol) and
//!    RFC 1122 (Host Requirements) is not a priority. Guiding principle is to find a
//!    balance between the goals of interoperating smoothly and limiting attack surface.
//!    In particular, the packet filter drops packets with IPv4 fragments or options.
//!
//! Other Limits of Scope:
//! 1. Focus of this crate is on safely parsing and assembling Ethernet II frames and
//!    maintaining related Internet protocol state machines.
//! 2. Things that require `unsafe`, such as static mut struct instances or network driver
//!    C FFI calls, should go elsewhere (see sw/src/main.rs and sw/wfx_rs)
//! 3. Code specific to details of a particular network interface device should probably
//!    go elsewhere (see sw/wfx_rs). Ethernet II MAC header stuff is sort of an exception.
//! 4. API code to manage link layer connections should go elsewhere (see sw/src/wlan.rs)
//!
//! Fuzzy Edges:
//! 1. Necessary level of support for DNS is unclear. Using a manually managed contact
//!    list of static IP addresses might be good enough for communicating with update
//!    server or proof of concept chat client using a Raspberry Pi on private LAN hosting
//!    a Matrix homeserver. Might be possible to bridge DNS Ethernet frames across COM bus
//!    to smoltcp running in Xous.
//! 2. How to do transport layer for software updates is unclear. Signing and verification
//!    of binary image files works, so encrypted transport is not strictly required. Might
//!    still be desirable though.
//! 3. How to do transport layer for Matrix chat is unclear. Normal clients do Olm double
//!    ratchet on client, then use https for transport to Matrix server. Maybe we can use
//!    smoltcp and rustls in Xous with bridging of TCP Ethernet frames across COM bus.
//!
//! Priority 1 Features to Support Factory Test ([x]=works, [-]=partial, [ ]=todo):
//! - [x] Ethernet frame RX and protocol handler dispatch
//! - [x] Packet filter: drop multicast, unsuported protocol, failed checksum, etc.
//! - [-] Diagnostic stats event counters with COM bus API
//! - [-] Ethernet frame TX
//! - [-] DHCP client
//! - [ ] ARP Responder
//! - [ ] ARP cache
//! - [ ] TX Packet routing to local MAC or gateway using ARP cache and IP/netmask
//! - [ ] ICMP echo responder and COM bus ping API
//! - [ ] Connectivity check after DHCP ACK (ping gateway)
//! - [ ] Handle ICMP destination unreachable
//!
//! Priority 2 Features:
//! - [ ] UDP sockets with COM bus API
//! - [ ] TCP sockets with COM bus API (maybe pass TCP frames to Xous + smoltcp?)
//!
use debug;
use debug::{log, logln, sprint, sprintln, LL};

pub mod dhcp;
pub mod filter;
pub mod hostname;
pub mod prng;
use dhcp::DhcpClient;
use filter::{FilterBin, FilterStats};
use prng::NetPrng;

// Configure Log Level (used in macro expansions)
const LOG_LEVEL: LL = LL::Debug;

// Expected Ethernet frame header sizes
const MAC_HEADER_LEN: usize = 14;
const ARP_FRAME_LEN: usize = MAC_HEADER_LEN + 28;
const IPV4_MIN_HEADER_LEN: usize = 20;
const IPV4_MIN_FRAME_LEN: usize = MAC_HEADER_LEN + IPV4_MIN_HEADER_LEN;
const UDP_HEADER_LEN: usize = 8;
const MIN_UDP_FRAME_LEN: usize = IPV4_MIN_FRAME_LEN + UDP_HEADER_LEN;

// Ethertypes for Ethernet MAC header
const ETHERTYPE_IPV4: &[u8] = &[0x08, 0x00];
const ETHERTYPE_ARP: &[u8] = &[0x08, 0x06];

/// Holds network stack state such as DHCP client state, addresses, and diagnostic stats
pub struct NetState {
    pub mac: [u8; 6],
    pub filter_stats: FilterStats,
    pub prng: NetPrng,
    pub dhcp: DhcpClient,
}
impl NetState {
    /// Initialize a new NetState struct
    pub const fn new() -> NetState {
        NetState {
            mac: [0u8; 6],
            filter_stats: FilterStats::new_all_zero(),
            prng: NetPrng::new_from(&[0x55u16; 8]),
            dhcp: DhcpClient::new(),
        }
    }

    /// Set the source MAC address to use for building outbound Ethernet frames
    pub fn set_mac(&mut self, mac: &[u8; 6]) {
        self.mac.clone_from_slice(mac);
    }

    /// Dump current state to the debug log
    pub fn log_state(&self) {
        log!(LL::Debug, "MAC ");
        log_hex(&self.mac);
        logln!(
            LL::Debug,
            "\r\nDropNoise {:X}",
            self.filter_stats.drop_noise
        );
        logln!(LL::Debug, "DropEType {:X}", self.filter_stats.drop_etype);
        logln!(LL::Debug, "DropMulti {:X}", self.filter_stats.drop_multi);
        logln!(LL::Debug, "DropProto {:X}", self.filter_stats.drop_proto);
        logln!(LL::Debug, "DropFrag {:X}", self.filter_stats.drop_frag);
        logln!(LL::Debug, "DropIpCk {:X}", self.filter_stats.drop_ipck);
        logln!(LL::Debug, "DropUdpCk {:X}", self.filter_stats.drop_udpck);
        logln!(LL::Debug, "ArpReq {:X}", self.filter_stats.arp_req);
        logln!(LL::Debug, "ArpReply {:X}", self.filter_stats.arp_reply);
        logln!(LL::Debug, "Icmp {:X}", self.filter_stats.icmp);
        logln!(LL::Debug, "Dhcp {:X}", self.filter_stats.dhcp);
        logln!(LL::Debug, "Udp {:X}", self.filter_stats.udp);
    }
}

/// Handle an inbound Ethernet II frame
///
/// This was written for the 14-byte "Ethernet II frame header" format described in
/// Silicon Labs WF200 documentation: {6-byte dest MAC, 6-byte src MAC, 2-byte ethertype},
/// with no preamble nor trailing checksum. Ethernet II is similar to, and largely
/// compatible with, the newer 802.3 MAC headers, but 802.3 brings the possibility of a
/// variable length MAC header due to tags (VLAN, etc).
///
/// See: https://docs.silabs.com/wifi/wf200/rtos/latest/group-w-f-m-g-r-o-u-p-c-o-n-c-e-p-t-s#WFM-CONCEPT-PACKET
///
/// Here we expect to see a fixed 14 byte MAC header and, as a defensive precaution, drop
/// frames with EtherType values indicating a long 802.3 style MAC header containing tags.
/// If you want to repurpose this code for use with other network interfaces, particularly
/// with wired Ethernet, keep in mind that code from this crate was originally written
/// assuming a fixed-length Ethernet II MAC header at the start of each frame.
///
pub fn handle_frame(mut net_state: &mut NetState, data: &[u8]) -> FilterBin {
    if data.len() < MAC_HEADER_LEN {
        // Drop frames that are too short to contain an Ethernet MAC header
        let bin = FilterBin::DropNoise;
        net_state.filter_stats.inc_count_for(bin);
        return bin;
    }
    const MAC_MULTICAST: &[u8] = &[0x01, 0x00, 0x5E, 0x00, 0x00, 0xFB]; // Frequently seen for mDNS
    let dest_mac = &data[..6];
    if dest_mac == MAC_MULTICAST {
        // Drop mDNS
        let bin = FilterBin::DropMulti;
        net_state.filter_stats.inc_count_for(bin);
        return bin;
    }
    let ethertype = &data[12..14]; // ipv4=0x0800, ipv6=0x86DD, arp=0x0806, vlan=0x8100
    let filter_bin = match ethertype {
        ETHERTYPE_IPV4 => handle_ipv4_frame(&mut net_state, data),
        ETHERTYPE_ARP => handle_arp_frame(data),
        _ => FilterBin::DropEType,
    };
    net_state.filter_stats.inc_count_for(filter_bin);
    return filter_bin;
}

fn log_hex(s: &[u8]) {
    for i in s {
        log!(LL::Debug, "{:02X}", *i);
    }
    log!(LL::Debug, " ");
}

/// Calculate one's complement IPv4 header checksum according to RFC 1071 & RFC 791
fn ipv4_checksum(data: &[u8]) -> u16 {
    let pre_checksum_it = data[14..24].chunks_exact(2);
    let post_checksum_it = data[26..34].chunks_exact(2);
    let header_it = pre_checksum_it.chain(post_checksum_it);
    let mut sum: u16 = 0;
    for c in header_it {
        let x = ((c[0] as u16) << 8) | (c[1] as u16);
        sum = match sum.overflowing_add(x) {
            (n, true) => n + 1,
            (n, false) => n,
        };
    }
    !sum
}

/// Calculate one's complement UDP checksum according to RFC 1071 & RFC 768
/// UDP checksum includes IP pseudo-header {src,dst,zero,protocol,UDP_len} and the whole UDP datagram
fn ipv4_udp_checksum(data: &[u8]) -> u16 {
    const ZERO_PAD_PROTO_UDP: u16 = 0x0011;
    let udp = &data[IPV4_MIN_FRAME_LEN..];
    // Build a chained iterator over the IP pseudoheader and UDP datagram
    let ip_src_dst_it = data[26..34].chunks_exact(2);
    let udp_length_it = udp[4..6].chunks_exact(2);
    let pseudo_header_it = ip_src_dst_it.chain(udp_length_it);
    let udp_pre_checksum_it = udp[..6].chunks_exact(2);
    let udp_post_checksum_it = udp[8..].chunks_exact(2);
    let udp_remainder = udp_post_checksum_it.remainder();
    let udp_tail = match udp_remainder.is_empty() {
        true => [0, 0],
        // If the UDP datagram length was not an integer multiple of 16-bits, pad it
        false => [udp_remainder[0], 0],
    };
    let udp_tail_it = udp_tail.chunks_exact(2);
    let datagram_it = udp_pre_checksum_it
        .chain(udp_post_checksum_it)
        .chain(udp_tail_it);
    // Putting the UDP protocol code in an iterator would be silly since it's a constant
    let mut sum: u16 = ZERO_PAD_PROTO_UDP;
    // Do the math
    for c in pseudo_header_it.chain(datagram_it) {
        let x = ((c[0] as u16) << 8) | (c[1] as u16);
        sum = match sum.overflowing_add(x) {
            (n, true) => n + 1,
            (n, false) => n,
        };
    }
    !sum
}

fn handle_ipv4_frame(net_state: &mut NetState, data: &[u8]) -> FilterBin {
    if data.len() < IPV4_MIN_FRAME_LEN {
        // Drop frames that are too short to hold an IPV4 header
        return FilterBin::DropNoise;
    }
    let ip_ver_ihl = &data[14..15];
    let ip_flags_frag = &data[20..22];
    let ip_proto = &data[23..24];
    let ip_checksum = &data[24..26];
    const VER4_LEN5: u8 = 0x4_5;
    if ip_ver_ihl[0] != VER4_LEN5 {
        // Drop frames with IP version field not 4 or IP header length longer than minimum (5*32-bits=20 bytes)
        // The main effect of this is to drop frames with IP header options. Dropping
        // packets with IP options is apparently common practice and probably mostly fine?
        // For additional context, see RFC 7126: Filtering of IP-Optioned Packets
        return FilterBin::DropNoise;
    }
    const IGNORE_DF_MASK: u8 = 0b101_11111;
    if (ip_flags_frag[0] & IGNORE_DF_MASK != 0) || (ip_flags_frag[1] != 0) {
        // Drop frames that are part of a fragmented IP packet
        return FilterBin::DropFrag;
    }
    let csum = ipv4_checksum(data);
    if csum != u16::from_be_bytes([ip_checksum[0], ip_checksum[1]]) {
        return FilterBin::DropIpCk;
    }
    const PROTO_UDP: u8 = 0x11;
    const PROTO_ICMP: u8 = 0x01;
    match ip_proto[0] {
        PROTO_UDP => handle_udp_frame(net_state, data),
        PROTO_ICMP => handle_icmp_frame(data),
        _ => FilterBin::DropProto,
    }
}

fn log_mac_header(data: &[u8]) {
    if data.len() < MAC_HEADER_LEN {
        return;
    }
    let dest_mac = &data[..6];
    let src_mac = &data[6..12];
    let ethertype = &data[12..14];
    log_hex(dest_mac);
    log_hex(src_mac);
    log_hex(ethertype);
}

fn log_ipv4_header(data: &[u8]) {
    if data.len() < IPV4_MIN_FRAME_LEN {
        return;
    }
    let ip_ver_ihl = &data[14..15];
    let ip_dcsp_ecn = &data[15..16];
    let ip_length = &data[16..18];
    let ip_id = &data[18..20];
    let ip_flags_frag = &data[20..22];
    let ip_ttl = &data[22..23];
    let ip_proto = &data[23..24];
    let ip_checksum = &data[24..26];
    let ip_src = &data[26..30];
    let ip_dst = &data[30..34];
    log_hex(ip_ver_ihl);
    log_hex(ip_dcsp_ecn);
    log_hex(ip_length);
    log!(LL::Debug, " ");
    log_hex(ip_id);
    log_hex(ip_flags_frag);
    log!(LL::Debug, " ");
    log_hex(ip_ttl);
    log_hex(ip_proto);
    log_hex(ip_checksum);
    log!(LL::Debug, " ");
    log_hex(ip_src);
    log!(LL::Debug, " ");
    log_hex(ip_dst);
}

fn log_udp_header(data: &[u8]) {
    if data.len() < MIN_UDP_FRAME_LEN {
        return;
    }
    let udp = &data[IPV4_MIN_FRAME_LEN..];
    let src_port = &udp[0..2];
    let dst_port = &udp[2..4];
    let length = &udp[4..6];
    let checksum = &udp[6..8];
    log_hex(src_port);
    log_hex(dst_port);
    log!(LL::Debug, " ");
    log_hex(length);
    log_hex(checksum);
}

fn handle_icmp_frame(data: &[u8]) -> FilterBin {
    if data.len() < IPV4_MIN_FRAME_LEN {
        return FilterBin::DropNoise;
    }
    log!(LL::Debug, "RxICMP ");
    log_mac_header(data);
    log!(LL::Debug, "\r\n  ");
    log_ipv4_header(data);
    log!(LL::Debug, "\r\n  ");
    log_hex(&data[IPV4_MIN_FRAME_LEN..]);
    logln!(LL::Debug, "");
    return FilterBin::Icmp;
}

fn handle_udp_frame(mut net_state: &mut NetState, data: &[u8]) -> FilterBin {
    if data.len() < MIN_UDP_FRAME_LEN {
        // Drop if frame is too short for a minimal well formed UDP datagram
        return FilterBin::DropNoise;
    }
    let udp = &data[IPV4_MIN_FRAME_LEN..];
    let checksum = &udp[6..8];
    if u16::from_be_bytes([checksum[0], checksum[1]]) != ipv4_udp_checksum(data) {
        // Drop if UDP checksum validation fails
        return FilterBin::DropUdpCk;
    }
    let dst_port = u16::from_be_bytes([udp[2], udp[3]]);
    let payload = &udp[8..];
    const DHCP_CLIENT: u16 = 68;
    match dst_port {
        DHCP_CLIENT => return dhcp::handle_dhcp_frame(&mut net_state, data),
        _ => {
            log!(LL::Debug, "RxUDP ");
            log_mac_header(data);
            log!(LL::Debug, "\r\n  ");
            log_ipv4_header(data);
            log!(LL::Debug, "\r\n  ");
            log_udp_header(data);
            log!(LL::Debug, "\r\n  ");
            log_hex(payload);
            logln!(LL::Debug, "");
            return FilterBin::Udp;
        }
    };
}

/// Handle received Ethernet frame of type ARP (0x0806)
///
/// |-------- Ethernet MAC Header --------|----------------------------- ARP --------------------------------------|
/// | DEST_MAC     SRC_MAC      ETHERTYPE | HTYPE PTYPE HLEN PLEN OPER SHA          SPA      THA          TPA      |
/// | FFFFFFFFFFFF ------------ 0806      | 0001  0800  06   04   0001 ------------ 0A000101 000000000000 0A000102 |
/// | ------------ ------------ 0806      | 0001  0800  06   04   0002 ------------ 0A000102 ------------ 0A000101 |
///
fn handle_arp_frame(data: &[u8]) -> FilterBin {
    if data.len() < ARP_FRAME_LEN {
        // Drop malformed (too short) ARP packet
        return FilterBin::DropNoise;
    }
    let dest_mac = &data[..6];
    let src_mac = &data[6..12];
    log!(LL::Debug, "RxARP ");
    log_hex(dest_mac);
    log_hex(src_mac);
    // ARP header for Ethernet + IPv4:
    //  {htype=0x0001 (Ethernet), ptype=0x0800 (IPv4), hlen=0x06 (6 bytes), plen=0x04 (4 bytes)}
    const ARP_FOR_ETHERNET_IPV4: &[u8] = &[0, 1, 8, 0, 6, 4];
    let htype_ptype_hlen_plen = &data[14..20];
    if htype_ptype_hlen_plen != ARP_FOR_ETHERNET_IPV4 {
        // Drop ARP packets that do not match the format for IPv4 over Ethernet
        return FilterBin::DropNoise;
    }
    let arp_oper = &data[20..22];
    let arp_sha = &data[22..28];
    let arp_spa = &data[28..32];
    let arp_tha = &data[32..38];
    let arp_tpa = &data[38..42];
    let mut filter_bin = FilterBin::DropNoise;
    if arp_oper == &[0, 1] {
        // ARP Request
        filter_bin = FilterBin::ArpReq;
        log!(LL::Debug, "who has ");
        log_hex(arp_tpa);
        log!(LL::Debug, "tell ");
        log_hex(arp_sha);
        log_hex(arp_spa);
    } else if arp_oper == &[0, 2] {
        // ARP Reply
        filter_bin = FilterBin::ArpReply;
        log_hex(arp_spa);
        log!(LL::Debug, "is at ");
        log_hex(arp_sha);
        log!(LL::Debug, "-> ");
        log_hex(arp_tha);
        log_hex(arp_tpa);
    }
    if arp_sha != src_mac {
        // If Ethernet source MAC does not match the ARP sender hardware
        // address, something weird is happening. Possible that the sending
        // host has two network interfaces attached to the same LAN?
        log!(LL::Debug, "WeirdSender");
    }
    logln!(LL::Debug, "");
    return filter_bin;
}
