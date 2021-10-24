#![no_std]
#![forbid(unsafe_code)]
//! This crate provides a minimalist IP stack with packet filtering.
//!
//! Priority 1 Features to Support Factory Test ([x]=works, [-]=partial, [ ]=todo):
//! - [x] Ethernet frame RX and protocol handler dispatch
//! - [x] Packet filter: drop multicast, unsuported protocol, failed checksum, etc.
//! - [x] Diagnostic stats event counters with UP5K UART debug command API
//! - [x] Ethernet frame TX
//! - [x] DHCP client Discover/Offer/Request/Ack binding flow
//! - [x] Remember best RSSI from SSID scan
//! - [x] Check RSSI from most recent packet (or SSID scan if link down) during wlan status
//! - [x] Encode {RSSI, AP join, DHCP bind} results in WLAN_STATUS response
//!
use debug;
use debug::{log, loghexln, logln, LL};

pub mod dhcp;
pub mod filter;
pub mod hostname;
pub mod prng;
pub mod timers;

use dhcp::DhcpClient;
use filter::{FilterBin, FilterStats};
use prng::NetPrng;

// Configure Log Level (used in macro expansions)
const LOG_LEVEL: LL = LL::Debug;

// Expected Ethernet frame header sizes
const MAC_HEADER_LEN: usize = 14;
#[allow(dead_code)]
const ARP_FRAME_LEN: usize = MAC_HEADER_LEN + 28;
const IPV4_MIN_HEADER_LEN: usize = 20;
const IPV4_MIN_FRAME_LEN: usize = MAC_HEADER_LEN + IPV4_MIN_HEADER_LEN;
const UDP_HEADER_LEN: usize = 8;
const MIN_UDP_FRAME_LEN: usize = IPV4_MIN_FRAME_LEN + UDP_HEADER_LEN;

// Ethertypes for Ethernet MAC header
const ETHERTYPE_IPV4: &[u8] = &[0x08, 0x00];
#[allow(dead_code)]
const ETHERTYPE_ARP: &[u8] = &[0x08, 0x06];

/// Holds network stack state such as DHCP client state, addresses, and diagnostic stats
pub struct NetState {
    pub mac: [u8; 6],
    pub filter_stats: FilterStats,
    pub prng: NetPrng,
    pub dhcp: DhcpClient,
    pub com_net_bridge_enable: bool,
}
impl NetState {
    /// Initialize a new NetState struct
    pub const fn new() -> NetState {
        NetState {
            mac: [0u8; 6],
            filter_stats: FilterStats::new_all_zero(),
            prng: NetPrng::new_from(&[0x55u16; 8]),
            dhcp: DhcpClient::new(),
            com_net_bridge_enable: true,
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
        logln!(LL::Debug, "");
        logln!(LL::Debug, "{}", self.dhcp.get_state_tag());
        self.dhcp.log_bindings();
        loghexln!(LL::Debug, "DropNoise ", self.filter_stats.drop_noise);
        loghexln!(LL::Debug, "DropEType ", self.filter_stats.drop_etype);
        loghexln!(LL::Debug, "DropDhcp ", self.filter_stats.drop_dhcp);
        loghexln!(LL::Debug, "DropMulti ", self.filter_stats.drop_multi);
        loghexln!(LL::Debug, "DropProto ", self.filter_stats.drop_proto);
        loghexln!(LL::Debug, "DropFrag ", self.filter_stats.drop_frag);
        loghexln!(LL::Debug, "DropIpCk ", self.filter_stats.drop_ipck);
        loghexln!(LL::Debug, "DropUdpCk ", self.filter_stats.drop_udpck);
        loghexln!(LL::Debug, "Arp ", self.filter_stats.arp);
        loghexln!(LL::Debug, "Icmp ", self.filter_stats.icmp);
        loghexln!(LL::Debug, "Dhcp ", self.filter_stats.dhcp);
        loghexln!(LL::Debug, "Udp ", self.filter_stats.udp);
        loghexln!(LL::Debug, "ComFwd ", self.filter_stats.com_fwd);
    }

    pub fn set_com_net_bridge_enable(&mut self, enable: bool) {
        self.com_net_bridge_enable = enable;
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
        ETHERTYPE_ARP => handle_arp_frame(&net_state, data),
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
    const PROTO_TCP: u8 = 0x06;
    const PROTO_ICMP: u8 = 0x01;
    match ip_proto[0] {
        PROTO_UDP => handle_udp_frame(net_state, data),
        PROTO_ICMP => handle_icmp_frame(data),
        PROTO_TCP => FilterBin::ComFwd,
        _ => FilterBin::DropProto,
    }
}

fn handle_icmp_frame(data: &[u8]) -> FilterBin {
    if data.len() < IPV4_MIN_FRAME_LEN {
        return FilterBin::DropNoise;
    }
    // Forward ICMP up the COM bus
    return FilterBin::ComFwd;
}

fn handle_udp_frame(net_state: &mut NetState, data: &[u8]) -> FilterBin {
    if data.len() < MIN_UDP_FRAME_LEN {
        // Drop if frame is too short for a minimal well formed UDP datagram
        return FilterBin::DropNoise;
    }
    let udp = &data[IPV4_MIN_FRAME_LEN..];
    let dst_port = u16::from_be_bytes([udp[2], udp[3]]);
    const DHCP_CLIENT: u16 = 68;
    if dst_port != DHCP_CLIENT {
        // Return early for non-DHCP UDP to skip the checksum check on packets
        // that will get forwarded up the COM bus net bridge to smoltcp
        return FilterBin::ComFwd;
    }
    // If we make it here, packet is for the DHCP client, so check the checksum
    let checksum = &udp[6..8];
    if u16::from_be_bytes([checksum[0], checksum[1]]) != ipv4_udp_checksum(data) {
        return FilterBin::DropUdpCk;
    }
    return net_state.dhcp.handle_frame(data);
}

/// Handle received Ethernet frame of type ARP (0x0806)
fn handle_arp_frame(net_state: &NetState, data: &[u8]) -> FilterBin {
    if data.len() < ARP_FRAME_LEN {
        // Drop malformed (too short) ARP packet
        return FilterBin::DropNoise;
    }
    // Determine whether an IP address is bound to our network interface (if not, this ARP is not for us)
    if net_state.dhcp.get_state() != dhcp::State::Bound {
        return FilterBin::DropNoise;
    }
    let my_ip4: u32 = match net_state.dhcp.ip {
        Some(ip4) => ip4,
        _ => return FilterBin::DropNoise,
    };
    // ARP header for Ethernet + IPv4:
    //  {htype=0x0001 (Ethernet), ptype=0x0800 (IPv4), hlen=0x06 (6 bytes), plen=0x04 (4 bytes)}
    const ARP_FOR_ETHERNET_IPV4: &[u8] = &[0, 1, 8, 0, 6, 4];
    let htype_ptype_hlen_plen = &data[14..20];
    if htype_ptype_hlen_plen != ARP_FOR_ETHERNET_IPV4 {
        // Drop ARP packets that do not match the format for IPv4 over Ethernet
        return FilterBin::DropNoise;
    }
    // Handle replies, and requests that are addressed to us
    let oper = u16::from_be_bytes([data[20], data[21]]);
    //let _sha = &data[22..28];
    //let _spa = u32::from_be_bytes([data[28], data[29], data[30], data[31]]);
    let tpa = u32::from_be_bytes([data[38], data[39], data[40], data[41]]);
    if (oper == 1) && (tpa == my_ip4) {
        // ARP Request
        return FilterBin::Arp;
    } else if oper == 2 {
        // ARP Reply: these should be passed up the COM bus net bridge
        log!(LL::Debug, "A");
        return FilterBin::ComFwd;
    }
    return FilterBin::DropNoise;
}
