use crate::{
    log_hex, log_ipv4_header, log_mac_header, log_udp_header, FilterBin, MIN_UDP_FRAME_LEN,
};
use debug::{log, logln, sprint, sprintln, LL};

// Configure Log Level (used in macro expansions)
const LOG_LEVEL: LL = LL::Debug;

const DHCP_HEADER_LEN: usize = 241; // op field -> one byte past options magic cookie
const MIN_DHCP_FRAME_LEN: usize = MIN_UDP_FRAME_LEN + DHCP_HEADER_LEN;

/// Build a DHCP discover packet by filling in a template of byte arrays
pub fn build_discover_frame(
    pbuf: &mut [u8],
    src_mac: &[u8; 6],
    ip_id: u16,
    dhcp_xid: u32,
    seconds: u16,
    hostname: &str,
) -> Result<(), u8> {
    if pbuf.len() < MIN_DHCP_FRAME_LEN {
        return Err(0x03);
    }
    // Ethernet MAC header
    let dst_mac = [255u8, 255, 255, 255, 255, 255];
    let ethertype = [8u8, 0];
    let mac_it = dst_mac.iter().chain(src_mac.iter()).chain(ethertype.iter());
    // IP header (checksum starts 0x0000 and gets updated later)
    let ip_vihl_tos_len = [0x45 as u8, 0x00, 0x01, 0x48];
    let ip_id_flagfrag = [(ip_id >> 8) as u8, ip_id as u8, 0x00, 0x00];
    let ip_ttl_proto_csum = [255u8, 17, 0, 0];
    let ip_src_dst = [0u8, 0, 0, 0, 255, 255, 255, 255];
    // UDP header
    let udp_srcp_dstp_len_csum = [0, 68, 0, 67, 0x01, 0x34, 0, 0];
    let ip_udp_it = ip_vihl_tos_len
        .iter()
        .chain(ip_id_flagfrag.iter())
        .chain(ip_ttl_proto_csum.iter())
        .chain(ip_src_dst.iter())
        .chain(udp_srcp_dstp_len_csum.iter());
    // DHCP
    let zero = [0u8];
    let dhcp_op_ht_hl_hop_s = [1u8, 1, 6, 0];
    let xid = dhcp_xid.to_be_bytes();
    let dhcp_secs_flags = [(seconds >> 8) as u8, seconds as u8, 0, 0];
    let dhcp_ci_yi_si_gi = zero.iter().cycle().take(16);
    let dhcp_chaddr = src_mac.iter().chain(zero.iter().cycle().take(10));
    let dhcp_sname_file = zero.iter().cycle().take(64 + 128);
    let dhcp_it = dhcp_op_ht_hl_hop_s
        .iter()
        .chain(xid.iter())
        .chain(dhcp_secs_flags.iter())
        .chain(dhcp_ci_yi_si_gi)
        .chain(dhcp_chaddr)
        .chain(dhcp_sname_file);
    // DHCP options part 1: magic cookie, 53_type, 55_paramRequestList, 57_maxMsgSize, 61_clientId
    let dopt1 = [
        0x63 as u8, 0x82, 0x53, 0x63, 53, 1, 1, 55, 7, 1, 121, 3, 6, 15, 119, 252, 57, 2, 0x05,
        0xdc, 61, 7, 1,
    ];
    // Part 2: chain source MAC as Client ID to finish option 61
    let dopt2 = src_mac.iter();
    // Part 3: 51_IpLeaseTime, 12_hostname
    let dopt3 = [51u8, 4, 0x00, 0x76, 0xa7, 0x00, 12, hostname.len() as u8];
    // Part 4: chain hostname to finish option 12
    let dopt4 = hostname.as_bytes().iter();
    // Part 5: 255_end
    let dopt5 = [255u8];
    let pad = zero.iter().cycle();
    let dhcp_opts_it = dopt1
        .iter()
        .chain(dopt2)
        .chain(dopt3.iter())
        .chain(dopt4)
        .chain(dopt5.iter())
        .chain(pad);
    let src_it = mac_it.chain(ip_udp_it).chain(dhcp_it).chain(dhcp_opts_it);
    for (dst, src) in pbuf.iter_mut().zip(src_it) {
        *dst = *src;
    }
    // Checksum fixup
    let ip_csum: u16 = crate::ipv4_checksum(&pbuf);
    for (dst, src) in pbuf[24..26].iter_mut().zip(ip_csum.to_be_bytes().iter()) {
        *dst = *src;
    }
    let udp_csum: u16 = crate::ipv4_udp_checksum(&pbuf);
    for (dst, src) in pbuf[34+6..34+8].iter_mut().zip(udp_csum.to_be_bytes().iter()) {
        *dst = *src;
    }
    return Ok(());
}

pub fn handle_dhcp_frame(data: &[u8]) -> FilterBin {
    if data.len() < MIN_DHCP_FRAME_LEN {
        return FilterBin::DropNoise;
    }
    let dhcp = &data[MIN_UDP_FRAME_LEN..];
    let op: &[u8] = &dhcp[0..1];
    let htype: &[u8] = &dhcp[1..2];
    let hlen: &[u8] = &dhcp[2..3];
    let hops: &[u8] = &dhcp[3..4];
    let transaction_id: &[u8] = &dhcp[4..8];
    let seconds_elapsed: &[u8] = &dhcp[8..10];
    let flags: &[u8] = &dhcp[10..12];
    // let _ciaddr: &[u8] = &dhcp[12..16];
    let your_ip_addr: &[u8] = &dhcp[16..20];
    // let _siaddr: &[u8] = &dhcp[20..24];
    // let _giaddr: &[u8] = &dhcp[24..28];
    let client_hw_addr: &[u8] = &dhcp[28..44];
    // let _sname: &[u8] = &dhcp[44..108];
    // let _file: &[u8] = &dhcp[108..236];
    let option_mc: &[u8] = &dhcp[236..240]; // Options magic cookie
    let options: &[u8] = &dhcp[240..]; // First option -> ...
    log!(LL::Debug, "RxDHCP\r\n ");
    log_mac_header(data);
    log!(LL::Debug, "\r\n ");
    log_ipv4_header(data);
    log!(LL::Debug, "\r\n ");
    log_udp_header(data);
    log!(LL::Debug, "\r\n ");
    log_hex(op);
    log_hex(htype);
    log_hex(hlen);
    log_hex(hops);
    log!(LL::Debug, " ");
    log_hex(transaction_id);
    log!(LL::Debug, " ");
    log_hex(seconds_elapsed);
    log_hex(flags);
    log!(LL::Debug, "\r\n ...");
    log_hex(your_ip_addr);
    log!(LL::Debug, "...\r\n ");
    log_hex(client_hw_addr);
    log!(LL::Debug, "\r\n ...");
    log!(LL::Debug, "\r\n ");
    log_hex(option_mc);
    handle_all_options(options)
}

/// Parse the DHCP options field, which is a hot mess.
///
/// For option tag meanings see:
/// - RFC 1533: DHCP Options and BOOTP Vendor Extensions
/// - RFC 3004: The User Class Option for DHCP
/// - RFC 1497: BOOTP Vendor Information Extensions
/// - IETF Draft draft-ietf-wrec-wpad-01 (Web Proxy Auto-Discovery Protocol)
///
/// Quick RFC 1533 Summary:
/// 1. Options 0 and 255 are exceptional because they are fixed length (1 octet long)
/// 2. All other options are "variable length" and have the format:
///    | tag_octet | length_octet=n | ... (n octets of data) |
/// 3. Length octet applies to data, so does not include tag_octet or the length_octet
/// 4. Some "variable length" fields are actually expected to always be a constant
///    length, but they are expected to still include a length_octet.
///
fn handle_all_options(options: &[u8]) -> FilterBin {
    let mut options_it = options.iter();
    // Note that options_it.next() is usually called more than once per iteration of
    // the for loop, so options.len() is an upper bound (safer than using while-true)
    for _ in 0..options.len() {
        // Log the option's tag
        if let Some(tag_u8) = options_it.next() {
            log!(LL::Debug, "\r\n {:X}", tag_u8);
            let tag = tag_from(*tag_u8);
            log_tag_label(tag);
            match tag {
                Tag::End => {
                    // Ignore whatever might follow the End option
                    log!(LL::Debug, "\r\n");
                    break;
                }
                Tag::Pad => continue,
                _ => (),
            }
        } else {
            break;
        }
        // Then log the option data length and option data
        if let Some(length) = options_it.next() {
            log!(LL::Debug, " {:X}", length);
            for _ in 0..(*length as usize) {
                if let Some(data) = options_it.next() {
                    log!(LL::Debug, " {:X}", data);
                }
            }
        } else {
            break;
        }
    }
    logln!(LL::Debug, "");
    return FilterBin::Dhcp;
}

#[derive(Copy, Clone, PartialEq)]
#[allow(dead_code)]
enum Tag {
    Pad,            // 0: length=0 (implicit from RFC; no length octet)
    End,            // 255: length=0, RFC says subsequent options should be Pads
    SubnetMask,     // 1: Subnet Mask; length=4
    GatewayList,    // 3: Gateway IP addresses; length=n*4 for n>=1
    DnsList,        // 6: DNS servers; length=n*4 for n>=1
    Hostname,       // 12: Client's hostname
    DomainName,     // 15: Name of local domain
    RequestedIp,    // 50: Client's requested IP (DISCOVER); length=4
    IpLeaseTime, // 51: Client's requested IP lease time (s) (DISCOVER or REQUEST); length=4 (u32)
    MsgType,     // 53: One of {1:Discover, 2:Offer, 3:Request, ...}; length=1, range=1-7
    ServerId,    // 54: IP of server for OFFER and REQUEST (optional for ACK/NACK); length=4
    ParamReqList, // 55: Config parameters client wants server to include in reply
    ErrMsg,      // 56: ASCII error message for use in NACK or DECLINE
    MaxMsgSize,  // 57: Maximum DHCP message size client is willing to accept; length=2
    RenewalTime, // 58: Interval (s) from IP assignment until enter RENEWING; length=4 (u32)
    RebindingTime, // 59: Interval (s) from IP assignment until enter REBINDING; length=4 (u32)
    ClassId,     // 60: Clients can use this to inform server of hardware configuration
    ClientId,    // 61: Unique client ID, normally {hardware_type, MAC_address}
    DomainSearch, // 119: See RFC 3397
    ClasslessRoute, // 121: See RFC 3442
    Wpad,        // 252: WPAD Proxy config URL
    Other(u8),
}

const fn tag_from(n: u8) -> Tag {
    match n {
        0 => Tag::Pad,
        255 => Tag::End,
        1 => Tag::SubnetMask,
        3 => Tag::GatewayList,
        6 => Tag::DnsList,
        12 => Tag::Hostname,
        15 => Tag::DomainName,
        50 => Tag::RequestedIp,
        51 => Tag::IpLeaseTime,
        53 => Tag::MsgType,
        54 => Tag::ServerId,
        55 => Tag::ParamReqList,
        56 => Tag::ErrMsg,
        57 => Tag::MaxMsgSize,
        58 => Tag::RenewalTime,
        59 => Tag::RebindingTime,
        60 => Tag::ClassId,
        61 => Tag::ClientId,
        119 => Tag::DomainSearch,
        121 => Tag::ClasslessRoute,
        252 => Tag::Wpad,
        n => Tag::Other(n),
    }
}

fn log_tag_label(tag: Tag) {
    let label = match tag {
        Tag::Pad => &"Pad",
        Tag::End => &"End",
        Tag::SubnetMask => &"SubnetMask",
        Tag::GatewayList => &"GatewayList",
        Tag::DnsList => &"DnsList",
        Tag::Hostname => &"Hostname",
        Tag::DomainName => &"DomainName",
        Tag::RequestedIp => &"RequestedIP",
        Tag::IpLeaseTime => &"IpLeaseTime",
        Tag::MsgType => &"MsgType",
        Tag::ServerId => &"ServerId",
        Tag::ParamReqList => &"ParamReqList",
        Tag::ErrMsg => &"ErrMsg",
        Tag::MaxMsgSize => &"MaxMsgSize",
        Tag::RenewalTime => &"RenewalTime",
        Tag::RebindingTime => &"RebindingTime",
        Tag::ClassId => &"ClassId",
        Tag::ClientId => &"ClientId",
        Tag::DomainSearch => &"DomainSearch",
        Tag::ClasslessRoute => &"ClasslessRoute",
        Tag::Wpad => &"Wpad",
        Tag::Other(_) => &"?",
    };
    log!(LL::Debug, " ({})", label);
}
