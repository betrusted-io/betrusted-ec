use crate::{
    log_hex, log_ipv4_header, log_mac_header, log_udp_header, FilterBin, MIN_UDP_FRAME_LEN,
};
use debug::{log, logln, sprint, sprintln, LL};

// Configure Log Level (used in macro expansions)
const LOG_LEVEL: LL = LL::Debug;

const DHCP_HEADER_LEN: usize = 241; // op field -> one byte past options magic cookie
const MIN_DHCP_FRAME_LEN: usize = MIN_UDP_FRAME_LEN + DHCP_HEADER_LEN;

pub fn handle_dhcp_frame(data: &[u8]) -> FilterBin {
    if data.len() < MIN_DHCP_FRAME_LEN {
        return FilterBin::DropNoise;
    }
    let dhcp = &data[MIN_UDP_FRAME_LEN..];
    let op: &[u8] = &dhcp[0..1];
    let htype: &[u8] = &dhcp[1..2];
    let hlen: &[u8] = &dhcp[2..3];
    let hops: &[u8] = &dhcp[3..4];
    let xid: &[u8] = &dhcp[4..8];
    let secs: &[u8] = &dhcp[8..10];
    let flags: &[u8] = &dhcp[10..12];
    let ciaddr: &[u8] = &dhcp[12..16];
    let yiaddr: &[u8] = &dhcp[16..20];
    let siaddr: &[u8] = &dhcp[20..24];
    let giaddr: &[u8] = &dhcp[24..28];
    let chaddr: &[u8] = &dhcp[28..44];
    let sname: &[u8] = &dhcp[44..108];
    let file: &[u8] = &dhcp[108..236];
    let option_mc: &[u8] = &dhcp[236..240]; // Options magic cookie
    let options: &[u8] = &dhcp[240..]; // First option -> ...
    log!(LL::Debug, "RxDHCP ");
    log_mac_header(data);
    log!(LL::Debug, "\r\n  ");
    log_ipv4_header(data);
    log!(LL::Debug, "\r\n  ");
    log_udp_header(data);
    log!(LL::Debug, "\r\n  ");
    log_hex(op);
    log_hex(htype);
    log_hex(hlen);
    log_hex(hops);
    log!(LL::Debug, " ");
    log_hex(xid);
    log!(LL::Debug, " ");
    log_hex(secs);
    log_hex(flags);
    log!(LL::Debug, "\r\n  ");
    log_hex(ciaddr);
    log_hex(yiaddr);
    log_hex(siaddr);
    log_hex(giaddr);
    log!(LL::Debug, "\r\n  ");
    log_hex(chaddr);
    log!(LL::Debug, "\r\n  ");
    log_hex(sname);
    log!(LL::Debug, "\r\n  ");
    log_hex(file);
    log!(LL::Debug, "\r\n  ");
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
    let mut end_mode = false;
    for _ in 0..options.len() {
        // Log the option's tag
        if let Some(tag_u8) = options_it.next() {
            if end_mode {
                // End mode parsing: log bytes without trying to interpret them as options
                log!(LL::Debug, " {:X}", tag_u8);
                continue;
            } else {
                // Normal parsing with End & Pad special case checks
                log!(LL::Debug, "\r\n {:X}", tag_u8);
                let tag = tag_from(*tag_u8);
                log_tag_label(tag);
                match tag {
                    Tag::End => {
                        log!(LL::Debug, "\r\n");
                        end_mode = true;
                        continue;
                    }
                    Tag::Pad => continue,
                    _ => (),
                }
            }
        } else {
            break;
        }
        // Then log the length and data
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
    Pad,           //   0: length=0 (implicit from RFC; no length octet)
    End,           // 255: length=0, RFC says subsequent options should be Pads
    SubnetMask,    //   1: Subnet Mask; length=4
    GatewayList,   //   3: Gateway IP addresses; length=n*4 for n>=1
    DnsList,       //   6: DNS servers; length=n*4 for n>=1
    Hostname,      //  12: Client's hostname
    DomainName,    //  15: Name of local domain
    RequestedIp,   //  50: Client's requested IP (DISCOVER); length=4
    IpLeaseTime,   //  51: Client's requested IP lease time (s) (DISCOVER or REQUEST); length=4 (u32)
    MsgType,       //  53: One of {1:Discover, 2:Offer, 3:Request, ...}; length=1, range=1-7
    ServerId,      //  54: IP of server for OFFER and REQUEST (optional for ACK/NACK); length=4
    ParamReqList,  //  55: Config parameters client wants server to include in reply
    ErrMsg,        //  56: ASCII error message for use in NACK or DECLINE
    MaxMsgSize,    //  57: Maximum DHCP message size client is willing to accept; length=2
    RenewalTime,   //  58: Interval (s) from IP assignment until enter RENEWING; length=4 (u32)
    RebindingTime, //  59: Interval (s) from IP assignment until enter REBINDING; length=4 (u32)
    ClassId,       //  60: Clients can use this to inform server of hardware configuration
    ClientId,      //  61: Unique client ID, normally {hardware_type, MAC_address}
    UserClass,     //  77: See RFC 3004
    Wpad,          // 252: WPAD Proxy config URL
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
        77 => Tag::UserClass,
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
        Tag::UserClass => &"UserClass",
        Tag::Wpad => &"Wpad",
        Tag::Other(_) => &"?",
    };
    log!(LL::Debug, " ({})", label);
}
