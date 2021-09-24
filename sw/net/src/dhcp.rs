//! This module implements a DHCP client.
//!
//! The code here provides a state machine and functions to build and parse Ethernet
//! frames for a simple DHCP client to obtain an IPv4 address, netmask, gateway, and DNS
//! server.
//!
//! CAUTION: The Ethernet frame building and parsing functions use header field offset
//! constants based on the assumption of a network interface that expects a fixed size
//! 14-byte Ethernet II MAC frame header. This works fine with the WF200's wfx fullMAC
//! driver. But, for example, using this code for wired 802.3 Ethernet with variable
//! header size (VLAN tags) would require modifications.
//!
use crate::{FilterBin, NetState, MIN_UDP_FRAME_LEN};
use debug::{logln, sprint, sprintln, LL};

// Configure Log Level (used in macro expansions)
const LOG_LEVEL: LL = LL::Debug;

const DHCP_HEADER_LEN: usize = 241; // op field -> one byte past options magic cookie
const MIN_DHCP_FRAME_LEN: usize = MIN_UDP_FRAME_LEN + DHCP_HEADER_LEN;
const DHCP_FRAME_LEN: usize = 342;

/// Build a DHCP discover packet by filling in a template of byte arrays.
/// Returns Ok(data_length), where data_length is the number of bytes of pbuf.len()
/// that were used to hold the packet data
pub fn build_discover_frame<'a>(
    mut pbuf: &'a mut [u8],
    src_mac: &[u8; 6],
    ip_id: u16,
    dhcp_xid: u32,
    seconds: u16,
    hostname: &str,
) -> Result<u32, u8> {
    if pbuf.len() < DHCP_FRAME_LEN {
        return Err(0x03);
    }
    // Buffer might be a full MTU, so only use what we need. (this determines number of loop iterations below)
    pbuf = &mut pbuf[..DHCP_FRAME_LEN];
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
    // Do the checksum fixup. Note how these checksum offsets assume the minimum MAC and
    // IP header size. On some networks (VLAN?), that assumption might cause problems.
    let ip_csum: u16 = crate::ipv4_checksum(&pbuf);
    for (dst, src) in pbuf[24..26].iter_mut().zip(ip_csum.to_be_bytes().iter()) {
        *dst = *src;
    }
    let udp_csum: u16 = crate::ipv4_udp_checksum(&pbuf);
    for (dst, src) in pbuf[40..42].iter_mut().zip(udp_csum.to_be_bytes().iter()) {
        *dst = *src;
    }
    return Ok(pbuf.len() as u32);
}

/// Parse a DHCP reply and update the DHCP client state machine
pub fn handle_dhcp_frame(net_state: &mut NetState, data: &[u8]) -> FilterBin {
    if data.len() < MIN_DHCP_FRAME_LEN {
        return FilterBin::DropNoise;
    }
    // DHCP fields from RFC 2131 (start offset in UDP payload; size in bytes):
    //  op     (  0;   1):  Opcode: 1=BOOTREQUEST, 2=BOOTREPLY
    //  htype  (  1;   1):  Hardware address type; 1 for Ethernet
    //  hlen   (  2;   1):  Hardware address length; 6 for Etherent
    //  hops   (  3;   1):  Client sets this to 0
    //  xid    (  4;   4):  Transaction ID: picked randomly by client
    //  secs   (  8;   2):  Seconds since start of exchange: filled in by client
    //  flags  ( 10;   2):  Flags: clients set to 0 unless they cannot handle unicast
    //  ciaddr ( 12;   4):  Client IP address filled in by client for BOUND, RENEW, or REBINDING
    //  yiaddr ( 16;   4):  Your (client) IP address filled in by server
    //  siaddr ( 20;   4):  [ignore] IP of next server for bootstrap flow (BOOTP)
    //  giaddr ( 24;   4):  [ignore] IP of relay agent for booting with relay agent (BOOTP)
    //  chaddr ( 28;  16):  Client hardware address filled in by client (Ethernet MAC + null pad)
    //  sname  ( 44;  64):  [ignore] Server host name (BOOTP)
    //  file   (108; 128):  [ignore] Boot file name (BOOTP)
    //  option (236; ...):  Variable length options field starting with 4-byte magic cookie
    let dhcp = &data[MIN_UDP_FRAME_LEN..];
    let op_htype_hlen: &[u8] = &dhcp[0..3];
    const REPLY: u8 = 2;
    const ETHERNET: u8 = 1;
    const ETHERNET_MAC_LEN: u8 = 6;
    if op_htype_hlen != &[REPLY, ETHERNET, ETHERNET_MAC_LEN] {
        // Drop frames that are not replies configured for Ethernet
        return FilterBin::DropNoise;
    }
    let xid: u32 = u32::from_be_bytes([dhcp[4], dhcp[5], dhcp[6], dhcp[7]]);
    let yiaddr: u32 = u32::from_be_bytes([dhcp[16], dhcp[17], dhcp[18], dhcp[19]]);
    // CAUTION: ignoring client hardware address (chaddr)... is that bad?
    let option_mc: &[u8] = &dhcp[236..240];
    if option_mc != &[0x63, 0x82, 0x53, 0x63] {
        // Drop frames that don't have the correct options magic cookie
        return FilterBin::DropNoise;
    }
    // Slice the options block and parse it
    let options: &[u8] = &dhcp[240..];
    match parse_options(options) {
        Ok(opts) => {
            const DHCPOFFER: u8 = 2;
            const DHCPACK: u8 = 5;
            const DHCPNAK: u8 = 6;
            match (
                opts.msg_type,
                opts.server_id,
                opts.gateway,
                opts.subnet,
                opts.dns,
            ) {
                (Some(DHCPOFFER), Some(sid), Some(gw), Some(sn), Some(dns)) => {
                    net_state.dsm.handle_offer(xid, sid, yiaddr, gw, sn, dns);
                    return FilterBin::Dhcp;
                }
                (Some(DHCPACK), Some(sid), _, _, _) => {
                    net_state.dsm.handle_ack(xid, sid);
                    return FilterBin::Dhcp;
                }
                (Some(DHCPNAK), Some(sid), _, _, _) => {
                    net_state.dsm.handle_nak(xid, sid);
                    return FilterBin::Dhcp;
                }
                // Responses missing any of the required options will match here and get dropped
                _ => return FilterBin::DropNoise,
            }
        }
        Err(err_code) => {
            logln!(LL::Debug, "RxDHCP optsErr {:}", err_code);
            return FilterBin::DropNoise;
        }
    }
}

/// State Machine to control DHCP client
pub struct DhcpStateMachine {
    pub state: State,
    pub xid: Option<u32>,
    pub sid: Option<u32>,
    pub ip: Option<u32>,
    pub subnet: Option<u32>,
    pub gateway: Option<u32>,
    pub dns: Option<u32>,
}
impl DhcpStateMachine {
    pub const fn new() -> Self {
        Self {
            state: State::Init,
            xid: None,
            sid: None,
            ip: None,
            subnet: None,
            gateway: None,
            dns: None,
        }
    }

    pub fn set_xid(&mut self, xid: u32) {
        self.xid = Some(xid);
    }

    /// Handle DHCPOFFER event: transaction ID, server ID, IP, gateway IP, subnet mask, DNS server
    pub fn handle_offer(&mut self, xid: u32, sid: u32, ip: u32, gw: u32, sn: u32, dns: u32) {
        logln!(LL::Debug, "DHCPOFFER  xid: {:08X}  sid: {:08X}", xid, sid);
        logln!(LL::Debug, " IP      {:08X}", ip);
        logln!(LL::Debug, " Gateway {:08X}", gw);
        logln!(LL::Debug, " Subnet  {:08X}", sn);
        logln!(LL::Debug, " DNS     {:08X}", dns);
        if self.state != State::Selecting {
            return;
        }
        self.sid = Some(sid);
        self.ip = Some(ip);
        self.gateway = Some(gw);
        self.subnet = Some(sn);
        self.dns = Some(dns);
        self.state = State::Requesting;
    }

    /// Handle DHCPACK event: transaction ID, server ID
    pub fn handle_ack(&mut self, xid: u32, sid: u32) {
        logln!(LL::Debug, "DHCPACK  xid: {:08X}  sid: {:08X}", xid, sid);
    }

    /// Handle DHCPNAK event: transaction ID, server ID
    pub fn handle_nak(&mut self, xid: u32, sid: u32) {
        logln!(LL::Debug, "DHCNAK  xid: {:08X}  sid: {:08X}", xid, sid);
    }
}

/// DHCP Client States
#[allow(dead_code)]
#[derive(Copy, Clone, PartialEq)]
pub enum State {
    Init,
    Selecting,
    Requesting,
    InitReboot,
    Rebooting,
    Bound,
    Renewing,
    Rebinding,
}

/// Parse the DHCP options field, which is a hot mess.
///
/// For option tag meanings see:
/// - RFC 1533: DHCP Options and BOOTP Vendor Extensions
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
fn parse_options(options: &[u8]) -> Result<DhcpOption, u8> {
    let mut d = DhcpOption::new();
    // Since each iteration of the for loop here may consume multiple options bytes,
    // options.len() is just a safe upper bound to avoid using while-true. The for loop
    // should end with a break. Also, I tried doing this with Iterators, but trying to
    // dispatch function calls that each have a &mut Iterator reference inside a `for...{
    // match...{...} }` seems to be a major problem for the borrow checker. I give up.
    // Borrow checker wins. I'll just use C-style indexing.
    let mut i: usize = 0;
    for _ in 0..options.len() {
        // Next byte should be an option tag, so decide how to handle it
        let tag: u8 = match options[i] {
            // End option means we're done, so ignore the rest
            O_END => break,
            // Pad is NOP with no length byte or data after it
            O_PAD => {
                i += 1;
                continue;
            }
            // All other tags should have a length and data
            n => n,
        };
        i += 1;
        if i >= options.len() {
            // Malformed packet: Options bytes ran out too early
            return Err(0x01);
        }
        let len: u8 = options[i];
        i += 1;
        // CAUTION: Potential off by one error here. I'm 99.9% sure it's right... but that last 0.1, hmm...
        let i_plus_data_len = i + (len as usize);
        if i_plus_data_len > options.len() {
            // Malformed packet: Options bytes aren't long enough for specified data length
            return Err(0x02);
        }
        let data = &options[i..i_plus_data_len];
        match tag {
            // These options are interesting, parse and store their data
            O_MSG_TYPE => d.parse_msg_type(data, 0x11)?,
            O_SERVER_ID => d.parse_server_id(data, 0x12)?,
            O_IP_LEASE_TIME => d.parse_ip_lease_time(data, 0x13)?,
            O_SUBNET_MASK => d.parse_subnet(data, 0x14)?,
            O_GATEWAY_LIST => d.parse_gateway(data, 0x15)?,
            O_DNS_LIST => d.parse_dns(data, 0x16)?,
            // Ignore data for other options
            _ => (),
        };
        i = i_plus_data_len;
        if i == options.len() {
            // This is the normal loop exit point for a well-formed options block
            break;
        } else if i > options.len() {
            // This should never happen
            return Err(0x03);
        }
    }
    return Ok(d);
}

/// Struct to accumulate options that may be present in a DHCP response.
struct DhcpOption {
    pub msg_type: Option<u8>,
    pub server_id: Option<u32>,
    pub ip_lease_time: Option<u32>,
    pub subnet: Option<u32>,
    pub gateway: Option<u32>,
    pub dns: Option<u32>,
}
impl DhcpOption {
    /// Return a new empty DhcpOption struct instance
    pub fn new() -> Self {
        DhcpOption {
            msg_type: None,
            server_id: None,
            ip_lease_time: None,
            subnet: None,
            gateway: None,
            dns: None,
        }
    }

    /// Parse the first big-endian u32 off a list of data bytes that should be a non-zero multiple of 4 long.
    /// Return None if the length is not valid.
    fn parse_first_be_u32(data: &[u8], e: u8) -> Result<u32, u8> {
        if (data.len() == 0) || ((data.len() & 3) != 0) {
            // Data is not a valid length
            return Err(e);
        }
        // Convert first 4 bytes into a u32, potentially ignoring additional list items
        return Ok(u32::from_be_bytes([data[0], data[1], data[2], data[3]]));
    }

    /// Parse message type (Discover, Offer, Request, Ack, Nack, etc)
    ///  1 DHCPDISCOVER
    ///  2 DHCPOFFER
    ///  3 DHCPREQUEST
    ///  4 DHCPDECLINE
    ///  5 DHCPACK
    ///  6 DHCPNAK
    ///  7 DHCPRELEASE
    ///  8 DHCPINFORM
    pub fn parse_msg_type(&mut self, data: &[u8], e: u8) -> Result<(), u8> {
        if data.len() == 1 {
            self.msg_type = match data[0] {
                t @ 1..=8 => Some(t),
                _ => return Err(e),
            };
            return Ok(());
        }
        Err(e)
    }

    /// Parse server id option (usually server's IP address)
    pub fn parse_server_id(&mut self, data: &[u8], e: u8) -> Result<(), u8> {
        self.server_id = Some(Self::parse_first_be_u32(data, e)?);
        Ok(())
    }

    /// Parse IP lease time option
    pub fn parse_ip_lease_time(&mut self, data: &[u8], e: u8) -> Result<(), u8> {
        self.ip_lease_time = Some(Self::parse_first_be_u32(data, e)?);
        Ok(())
    }

    /// Parse subnet mask option
    pub fn parse_subnet(&mut self, data: &[u8], e: u8) -> Result<(), u8> {
        self.subnet = Some(Self::parse_first_be_u32(data, e)?);
        Ok(())
    }

    /// Parse _only_the_first_ gateway from a list of one or more gateway IP addresses
    /// CAUTION: Ignoring possibility of more than one gateway might cause trouble some day.
    pub fn parse_gateway(&mut self, data: &[u8], e: u8) -> Result<(), u8> {
        self.gateway = Some(Self::parse_first_be_u32(data, e)?);
        Ok(())
    }

    /// Parse _only_the_first_ DNS server from a list of one or more DNS server IP addresses
    /// CAUTION: Ignoring possibility of more than one DNS server might cause trouble some day.
    pub fn parse_dns(&mut self, data: &[u8], e: u8) -> Result<(), u8> {
        self.dns = Some(Self::parse_first_be_u32(data, e)?);
        Ok(())
    }
}

// DHCP Option tag constants
const O_END: u8 = 255;
const O_PAD: u8 = 0;
const O_MSG_TYPE: u8 = 53;
const O_SERVER_ID: u8 = 54;
const O_IP_LEASE_TIME: u8 = 51;
const O_SUBNET_MASK: u8 = 1;
const O_GATEWAY_LIST: u8 = 3;
const O_DNS_LIST: u8 = 6;

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
// DHCP Option Tags:
// 255 => End
//   0 => Pad
//   1 => SubnetMask
//   3 => GatewayList
//   6 => DnsList
//  12 => Hostname
//  15 => DomainName
//  50 => RequestedIp
//  51 => IpLeaseTime
//  53 => MsgType
//  54 => ServerId
//  55 => ParamReqList
//  56 => ErrMsg
//  57 => MaxMsgSize
//  58 => RenewalTime
//  59 => RebindingTime
//  60 => ClassId
//  61 => ClientId
// 119 => DomainSearch
// 121 => ClasslessRoute
// 252 => Wpad
