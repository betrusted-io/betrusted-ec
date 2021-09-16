#![no_std]
#![forbid(unsafe_code)]
use debug;
use debug::{log, logln, sprint, sprintln, LL};

pub mod dhcp;
pub mod filter;
pub mod prng;
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

/// Context maintains network stack state such as addresses and diagnostic stats
pub struct NetState {
    pub mac: [u8; 6],
    pub filter_stats: FilterStats,
    pub prng: NetPrng,
}
impl NetState {
    /// Initialize a new NetState struct
    pub const fn new() -> NetState {
        NetState {
            mac: [0u8; 6],
            filter_stats: FilterStats::new_all_zero(),
            prng: NetPrng::new_from(&[0x55u16; 8]),
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

/// Handle an inbound Ethernet frame
pub fn handle_frame(net_state: &mut NetState, data: &[u8]) -> FilterBin {
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
    let ethertype = &data[12..14]; // ipv4=0x0800, ipv6=0x86DD, arp=0x0806
    let filter_bin = match ethertype {
        ETHERTYPE_IPV4 => handle_ipv4_frame(data),
        ETHERTYPE_ARP => handle_arp_frame(data),
        _ => FilterBin::DropEType,
    };
    net_state.filter_stats.inc_count_for(filter_bin);
    return filter_bin;
}

/// Populate the MAC header portion of an Ethernet frame buffer
// fn fill_ethernet_mac_header(
//     src_mac: &[u8; 6],
//     dst_mac: &[u8; 6],
//     frame: &mut [u8],
// ) -> Result<(), ()> {
//     if frame.len() < MAC_HEADER_LEN {
//         return Err(());
//     }
//     let dst_mac_it = dst_mac.iter();
//     let src_mac_it = src_mac.iter();
//     let ethertype_it = ETHERTYPE_ARP.iter();
//     let mac_header_it = dst_mac_it.chain(src_mac_it).chain(ethertype_it);
//     for (dst, src) in frame.iter_mut().zip(mac_header_it) {
//         *dst = *src;
//     }
//     return Ok(());
// }

/// Populate the IP header portion of an Ethernet frame buffer
// fn fill_ip_header(
//     src_ip: &[u8; 4],
//     dst_ip: &[u8; 4],
//     protocol: u8,
//     frame: &mut [u8],
// ) -> Result<(), ()> {
//     if frame.len() < IPV4_MIN_FRAME_LEN {
//         return Err(());
//     }
//     let ver_ihl: u8 = 0x4_5; // ver=IPv4, IPv4 Header Length: 5 * 32-bits = 20 bytes
//     let dcsp_ecn: u8 = 0b000000_00; // Standard service class, Default forwarding, Non ECN-Capable transport
//     let length: u16 = IPV4_MIN_HEADER_LEN as u16; // Protocol layer will need to update this
//     let id: u16 = 0xABCD; // TODO: set this with PRNG
//     let flags_offset: u8 = 0b0_0_0_00000; // Reserved=0, DF=0, MF=0, Offset=0
//     let ttl: u8 = 0xFF; // Max TTL
//     let checksum = 0; // Protocol layer needs to update this once length is known; See RFC1071 (checksum)

//     // TODO: Implement this
//     return Err(());
// }

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

fn handle_ipv4_frame(data: &[u8]) -> FilterBin {
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
        PROTO_UDP => handle_udp_frame(data),
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

fn handle_udp_frame(data: &[u8]) -> FilterBin {
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
    let dst_port = &udp[2..4];
    let _length = &udp[4..6];
    let payload = &udp[8..];
    match dst_port {
        &[0, 67] | &[0, 68] => return dhcp::handle_dhcp_frame(data),
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
