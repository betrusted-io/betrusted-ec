#![no_std]
#![forbid(unsafe_code)]
use debug;
use debug::{log, logln, sprint, sprintln, LL};

pub mod filter;
use filter::{FilterBin, FilterStats};

// ==========================================================
// ===== Configure Log Level (used in macro expansions) =====
// ==========================================================
const LOG_LEVEL: LL = LL::Debug;
// ==========================================================

// Expected Ethernet frame header sizes
const MAC_HEADER_LEN: usize = 14;
const ARP_FRAME_LEN: usize = MAC_HEADER_LEN + 28;
const IPV4_FRAME_LEN: usize = MAC_HEADER_LEN + 20;

// Ethertypes for Ethernet MAC header
const ETHERTYPE_IPV4: &[u8] = &[0x08, 0x00];
const ETHERTYPE_ARP: &[u8] = &[0x08, 0x06];

/// Context maintains network stack state such as addresses and diagnostic stats
pub struct NetState {
    pub mac: [u8; 6],
    pub filter_stats: FilterStats,
}
impl NetState {
    /// Initialize a new NetState struct
    pub const fn new() -> NetState {
        NetState {
            mac: [0u8; 6],
            filter_stats: FilterStats::new_all_zero(),
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
        logln!(LL::Debug, "\r\nDropNoise {:X}", self.filter_stats.drop_noise);
        logln!(LL::Debug, "DropEType {:X}", self.filter_stats.drop_etype);
        logln!(LL::Debug, "DropMulti {:X}", self.filter_stats.drop_multi);
        logln!(LL::Debug, "DropProto {:X}", self.filter_stats.drop_proto);
        logln!(LL::Debug, "DropFrag {:X}", self.filter_stats.drop_frag);
        logln!(LL::Debug, "ArpReq {:X}", self.filter_stats.arp_req);
        logln!(LL::Debug, "ArpReply {:X}", self.filter_stats.arp_reply);
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

// TODO: Expand on this with something to make an ARP request (intent: trigger ARP reply to this MAC)
fn set_ethernet_mac_header(
    src_mac: &[u8; 6],
    dst_mac: &[u8; 6],
    frame: &mut [u8],
) -> Result<(), ()> {
    if frame.len() < MAC_HEADER_LEN {
        return Err(());
    }
    let dst_mac_it = dst_mac.iter();
    let src_mac_it = src_mac.iter();
    let ethertype_it = ETHERTYPE_ARP.iter();
    let mac_header_it = dst_mac_it.chain(src_mac_it).chain(ethertype_it);
    for (dst, src) in frame.iter_mut().zip(mac_header_it) {
        *dst = *src;
    }
    return Ok(());
}

fn log_hex(s: &[u8]) {
    for i in s {
        log!(LL::Debug, "{:02X}", *i);
    }
    log!(LL::Debug, " ");
}

fn handle_ipv4_frame(data: &[u8]) -> FilterBin {
    if data.len() < IPV4_FRAME_LEN {
        // Drop frames that are too short to hold an IPV4 header
        return FilterBin::DropNoise;
    }
    let dest_mac = &data[..6];
    let src_mac = &data[6..12];
    let ethertype = &data[12..14];
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
    const PROTO_UDP: &[u8] = &[0x11];
    if ip_proto != PROTO_UDP {
        // Drop frames that are not UDP
        return FilterBin::DropProto;
    }
    const IGNORE_DF_MASK: u8 = 0b101_11111;
    if (ip_flags_frag[0] & IGNORE_DF_MASK != 0) || (ip_flags_frag[1] != 0) {
        // Drop frames that are part of a fragmented IP packet
        return FilterBin::DropFrag;
    }
    const VERSION_MASK: u8 = 0xF0;
    if ip_ver_ihl[0] & VERSION_MASK != 0x40 {
        // Drop frames with IP version field not equal to 4
        return FilterBin::DropNoise;
    }
    log!(LL::Debug, "RxUDP ");
    log_hex(dest_mac);
    log_hex(src_mac);
    log_hex(ethertype);
    log_hex(ip_ver_ihl);
    log_hex(ip_dcsp_ecn);
    log!(LL::Debug, "len:");
    log_hex(ip_length);
    log_hex(ip_id);
    log_hex(ip_flags_frag);
    log_hex(ip_ttl);
    log!(LL::Debug, "proto:");
    log_hex(ip_proto);
    log_hex(ip_checksum);
    log_hex(ip_src);
    log_hex(ip_dst);
    logln!(LL::Debug, "");
    return FilterBin::Udp;
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
