use debug::{log, logln, sprint, sprintln, LL};

use crate::{
    log_hex, log_ipv4_header, log_mac_header, log_udp_header, FilterBin, MIN_UDP_FRAME_LEN,
};

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
    log_hex(options);
    logln!(LL::Debug, "");
    return FilterBin::Dhcp;
}
