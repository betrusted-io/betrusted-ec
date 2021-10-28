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
extern crate betrusted_hal;
use crate::timers::{Countdown, CountdownStatus, RetryStatus, RetryTimer, Stopwatch, StopwatchErr};
use crate::{hostname::Hostname, FilterBin, MIN_UDP_FRAME_LEN};
use debug::{loghexln, logln, LL};

// Configure Log Level (used in macro expansions)
const LOG_LEVEL: LL = LL::Debug;

const DHCP_HEADER_LEN: usize = 241; // op field -> one byte past options magic cookie
const MIN_DHCP_FRAME_LEN: usize = MIN_UDP_FRAME_LEN + DHCP_HEADER_LEN;
const DHCP_FRAME_LEN: usize = 342;

/// DHCP Client States
///
/// Note that InitReboot and Rebooting were intentionally omitted. Also, Halted is for
/// power-up or receiving a DHCPNAK while in Renewing or Rebinding.
///
#[derive(Copy, Clone, PartialEq)]
pub enum State {
    Halted,
    Init,
    Selecting,
    Requesting,
    Bound,
    Renewing,
    Rebinding,
}

/// Packet types that may need to be sent for a state transition or timer event
#[derive(Copy, Clone, PartialEq)]
pub enum PacketNeeded {
    Discover,
    Request,
    Renew,
    Rebind,
    None,
}

/// State transition notification latch for polling by event loop
#[derive(Copy, Clone, PartialEq)]
pub enum DhcpEvent {
    ChangedToBound,
    ChangedToHalted,
}

/// The three types of DHCP request packets that require slightly different MAC or DHCP options
#[derive(Copy, Clone, PartialEq)]
pub enum RequestType {
    Discover,
    Renew,
    Rebind,
}

/// State Machine for DHCP client
pub struct DhcpClient {
    entropy: [u32; 2],
    state_change_event_latch: Option<DhcpEvent>,
    timer_t1: Countdown,
    timer_t2: Countdown,
    timer_lease: Countdown,
    pub hostname: Hostname,
    pub state: State,
    pub secs: Stopwatch,
    pub retry: RetryTimer,
    pub xid: Option<u32>,
    pub sid: Option<u32>,
    pub ip: Option<u32>,
    pub subnet: Option<u32>,
    pub gateway: Option<u32>,
    pub gateway_mac: Option<[u8; 6]>,
    pub lease_sec: Option<u32>,
    pub dns: Option<u32>,
}
impl DhcpClient {
    pub const fn new() -> Self {
        Self {
            entropy: [0; 2],
            state_change_event_latch: None,
            timer_t1: Countdown::new(),
            timer_t2: Countdown::new(),
            timer_lease: Countdown::new(),
            hostname: Hostname::new_blank(),
            state: State::Halted,
            secs: Stopwatch::new(),
            retry: RetryTimer::new_halted(),
            xid: None,
            sid: None,
            ip: None,
            subnet: None,
            gateway: None,
            gateway_mac: None,
            lease_sec: None,
            dns: None,
        }
    }

    /// Return current state machine state
    pub fn get_state(&self) -> State {
        self.state
    }

    /// Check for notification of Bind/Halt state change event with implicit ACK
    pub fn pop_and_ack_change_event(&mut self) -> Option<DhcpEvent> {
        match self.state_change_event_latch {
            sce @ Some(_) => {
                self.state_change_event_latch = None;
                sce
            }
            None => None,
        }
    }

    /// Return string tag describing current state machine state
    pub fn get_state_tag(&self) -> &str {
        match self.state {
            State::Halted => "dhcpHalt",
            State::Init => "dhcpInit",
            State::Selecting => "dhcpSelect",
            State::Requesting => "dhcpRequest",
            State::Bound => "dhcpBound",
            State::Renewing => "dhcpRenew",
            State::Rebinding => "dhcpRebind",
        }
    }

    /// Clear all bindings that get populated from a DHCPOFFER
    fn reset_bindings(&mut self) {
        self.sid = None;
        self.ip = None;
        self.subnet = None;
        self.gateway = None;
        self.lease_sec = None;
        self.dns = None;
        self.timer_t1.clear();
        self.timer_t2.clear();
        self.timer_lease.clear();
    }

    /// Reset to refelct a state transition to halted (this means something went wrong)
    fn halt_and_reset(&mut self) {
        self.state = State::Halted;
        self.state_change_event_latch = Some(DhcpEvent::ChangedToHalted);
        self.secs.reset();
        self.reset_bindings();
        logln!(LL::Debug, "DhcpHalt");
    }

    /// Handle network link drop
    pub fn handle_link_drop(&mut self) {
        self.halt_and_reset();
    }

    /// Feed the state machine some entropy so it can start at INIT with new random hostname and xid.
    /// Also, save some entropy for generating randomized exponential backoff delays for retries.
    pub fn begin_at_init(&mut self, entropy: [u32; 5]) {
        self.entropy = [entropy[0], entropy[1]];
        self.hostname.randomize_if_unset(entropy[2], entropy[3]);
        self.state = State::Init;
        self.secs.reset();
        self.retry = RetryTimer::new_halted();
        self.xid = Some(entropy[4]);
        self.reset_bindings();
    }

    /// Update the state machine and return what packet type, if any, needs to be sent.
    ///
    /// This is weirdly sliced up because of the need to interoperate with sl_wfx_* C FFI
    /// code. Receiving a DHCP server packet happens by way of the main EC event loop
    /// calling into the FFI code to receive a frame, then the FFI code calls into Rust
    /// code in wfx_rs/src/hal_wf200.rs which implements the sl_wfx_* host api functions.
    /// In order to make that work, a bunch of the important network interface state has
    /// to exist as static mut in wfx_rs/src/hal_wf200.rs. Sending a DHCP packet works by
    /// way of the EC main event loop calling functions in hal_wf200.rs, because that's
    /// where the static mut DhcpClient necessarily resides.
    ///
    /// Given all that, in order to keep the Rust borrow checker happy, and to limit the
    /// extent of unsafe code, we do a dance where hal_wf200.rs code hides all the unsafe
    /// stuff from DhcpClient.
    ///
    /// The hal_wf200.rs DHCP state machine clocking wrapper function:
    ///  1. Calls this function to let DhcpClient update its state and respond with what
    ///     type of packet needs to be send next, if any
    ///  2. Does dangerous unsafe stuff to prepare a packet buffer
    ///  3. Calls a DhcpClient function to fill the packet buffer with the proper frame
    ///  4. Does dangerous unsafe stuff to cast the packet buffer into the right type
    ///     of C struct and call the sl_wfx FFI code to send the frame
    ///
    pub fn cycle_clock(&mut self) -> PacketNeeded {
        // See state transition diagram at RFC 2131 § 4.4 DHCP client behavior
        // InitRebooting and Rebooting are intentionally omitted.
        // Halted is power-up state or result of DHCPNAK from Renewing or Rebinding
        match self.state {
            State::Halted => PacketNeeded::None,
            State::Init => {
                self.secs.start();
                self.retry = RetryTimer::new_first_random(self.entropy[0]);
                self.state = State::Selecting;
                PacketNeeded::Discover
            }
            State::Selecting => {
                match self.ip {
                    // Matching Some(_) means we've received a valid DHCPOFFER
                    Some(_) => {
                        self.state = State::Requesting;
                        self.retry = RetryTimer::new_first_random(self.entropy[1]);
                        PacketNeeded::Request
                    }
                    _ => {
                        match self.retry.status() {
                            RetryStatus::Halted => {
                                self.halt_and_reset();
                                // TODO: notify main event loop of DHCP connection problem
                                PacketNeeded::None
                            }
                            RetryStatus::TimerRunning => PacketNeeded::None,
                            RetryStatus::TimerExpired => {
                                self.retry.schedule_next(self.entropy[0]);
                                PacketNeeded::Discover
                            }
                        }
                    }
                }
            }
            State::Requesting => match self.retry.status() {
                RetryStatus::Halted => {
                    self.halt_and_reset();
                    PacketNeeded::None
                }
                RetryStatus::TimerRunning => PacketNeeded::None,
                RetryStatus::TimerExpired => {
                    self.retry.schedule_next(self.entropy[1]);
                    PacketNeeded::Request
                }
            },
            State::Bound => match self.timer_t1.status() {
                CountdownStatus::Done => {
                    self.timer_t1.clear();
                    self.state = State::Renewing;
                    self.retry = RetryTimer::new_first_random_renew(self.entropy[1]);
                    self.secs.start();
                    logln!(LL::Debug, "DhcpRenew");
                    PacketNeeded::Renew
                }
                CountdownStatus::NotDone => PacketNeeded::None,
                CountdownStatus::NotStarted => PacketNeeded::None,
            },
            State::Renewing => match self.timer_t2.status() {
                CountdownStatus::Done => {
                    self.timer_t2.clear();
                    self.state = State::Rebinding;
                    self.retry = RetryTimer::new_first_random_renew(self.entropy[1]);
                    self.secs.start();
                    logln!(LL::Debug, "DhcpRebind");
                    PacketNeeded::Rebind
                }
                _ => match self.retry.status() {
                    RetryStatus::Halted | RetryStatus::TimerRunning => PacketNeeded::None,
                    RetryStatus::TimerExpired => {
                        logln!(LL::Debug, "DhcpRenewRetry");
                        self.retry.schedule_next_renew(self.entropy[1]);
                        PacketNeeded::Renew
                    }
                },
            },
            State::Rebinding => {
                match self.timer_lease.status() {
                    CountdownStatus::Done => {
                        // This is bad. Lease is up. Unable to get a new one.
                        self.reset_bindings();
                        self.state = State::Halted;
                        self.state_change_event_latch = Some(DhcpEvent::ChangedToHalted);
                        logln!(LL::Debug, "DhcpLeaseExpire");
                        PacketNeeded::None
                    }
                    _ => match self.retry.status() {
                        RetryStatus::Halted | RetryStatus::TimerRunning => PacketNeeded::None,
                        RetryStatus::TimerExpired => {
                            logln!(LL::Debug, "DhcpRebindRetry");
                            self.retry.schedule_next_renew(self.entropy[1]);
                            PacketNeeded::Rebind
                        }
                    },
                }
            }
        }
    }

    /// Handle DHCPOFFER event: transaction ID, server ID, IP, gateway IP, subnet mask, DNS server
    pub fn handle_offer(
        &mut self,
        sid: u32,
        ip: u32,
        gw: u32,
        gwm: &[u8; 6],
        ls: u32,
        sn: u32,
        dns: u32,
    ) {
        logln!(LL::Debug, "DhcpOffer");
        match self.state {
            State::Halted => (),
            State::Init => (),
            State::Selecting => {
                logln!(LL::Debug, "DhcpSelect");
                self.sid = Some(sid);
                self.ip = Some(ip);
                self.gateway = Some(gw);
                self.gateway_mac = Some(*gwm);
                self.lease_sec = Some(ls);
                self.subnet = Some(sn);
                self.dns = Some(dns);
                // Print results to the log
                self.log_bindings();
            }
            State::Requesting => (),
            State::Bound => (),
            State::Renewing => (),
            State::Rebinding => (),
        }
    }

    /// Print {IP, gateway, netmask, DNS} bindings to debug log
    pub fn log_bindings(&self) {
        match (self.ip, self.gateway, self.lease_sec, self.subnet, self.dns) {
            (Some(ip), Some(gateway), Some(lease), Some(subnet), Some(dns)) => {
                logln!(LL::Debug, " IP    {:08X}", ip);
                logln!(LL::Debug, " Gtwy  {:08X}", gateway);
                logln!(LL::Debug, " Lease {:08X}", lease);
                logln!(LL::Debug, " Mask  {:08X}", subnet);
                logln!(LL::Debug, " DNS   {:08X}", dns);
            }
            _ => (),
        };
    }

    /// Handle DHCPACK event: transaction ID, server ID
    pub fn handle_ack(&mut self, lease_sec: u32) {
        logln!(LL::Debug, "DhcpACK");
        match self.state {
            State::Halted => (),
            State::Init => (),
            State::Selecting => (),
            State::Requesting | State::Renewing | State::Rebinding => {
                // See RFC 2131 § 4.4.5 "Reacquisition and expiration" for rules on
                // calculating T1 and T2 timers. TL;DR: T1=0.5*lease, T2=0.875*lease.
                self.lease_sec = Some(lease_sec);
                // Set T1 timer for lease_sec * 0.5
                let t1 = lease_sec >> 1;
                self.timer_t1.start_s(t1);
                // Set T2 timer for approximately lease_sec * 0.875.
                // (8/7=0.875 and >>3 is equivalent to integer /8)
                let t2 = ((lease_sec as u64 * 7) >> 3) as u32;
                self.timer_t2.start_s(t2);
                // Set lease timer for 0.937 of the full lease interval (allow margin for possibly slow clock)
                let lease = ((lease_sec as u64 * 15) >> 4) as u32;
                self.timer_lease.start_s(lease);
                self.state = State::Bound;
                self.state_change_event_latch = Some(DhcpEvent::ChangedToBound);
                logln!(LL::Debug, "DhcpBound");
            }
            State::Bound => (),
        }
    }

    /// Handle DHCPNAK event: transaction ID, server ID
    pub fn handle_nak(&mut self) {
        logln!(LL::Debug, "DhcpNAK");
        match self.state {
            State::Halted => (),
            State::Init => (),
            State::Selecting => (),
            State::Requesting => {
                self.reset_bindings();
                self.state = State::Init;
                logln!(LL::Debug, "DhcpInit");
            }
            State::Bound => (),
            State::Renewing | State::Rebinding => {
                // This is bad. DHCP servers have probably assigned all their available leases.
                self.reset_bindings();
                self.state = State::Halted;
                self.state_change_event_latch = Some(DhcpEvent::ChangedToHalted);
                logln!(LL::Debug, "DhcpHalted");
            }
        }
    }

    /// Fill in the DHCP packet headers for MAC, IP, UDP, and BOOTP
    fn build_dhcp_headers<'a>(
        &mut self,
        pbuf: &'a mut [u8],
        src_mac: &[u8; 6],
        dst_mac: &[u8; 6],
        ciaddr: u32,
        ip_id: u16,
        ip_src: u32,
        ip_dst: u32,
    ) -> Result<usize, u8> {
        if pbuf.len() < DHCP_FRAME_LEN {
            return Err(0x03);
        }
        let xid = match self.xid {
            Some(x) => x,
            None => return Err(0x04), // This means state machine was not initialized properly
        };
        // Ethernet MAC header
        let ethertype = [8u8, 0];
        let mac_it = dst_mac.iter().chain(src_mac.iter()).chain(ethertype.iter());
        // IP header (checksum starts 0x0000 and gets updated later)
        let ip_vihl_tos_len = [0x45 as u8, 0x00, 0x01, 0x48];
        let ip_id_flagfrag = [(ip_id >> 8) as u8, ip_id as u8, 0x00, 0x00];
        let ip_ttl_proto_csum = [255u8, 17, 0, 0];
        let ip_src_bytes = ip_src.to_be_bytes();
        let ip_dst_bytes = ip_dst.to_be_bytes();
        // UDP header
        let udp_srcp_dstp_len_csum = [0, 68, 0, 67, 0x01, 0x34, 0, 0];
        let ip_udp_it = ip_vihl_tos_len
            .iter()
            .chain(ip_id_flagfrag.iter())
            .chain(ip_ttl_proto_csum.iter())
            .chain(ip_src_bytes.iter())
            .chain(ip_dst_bytes.iter())
            .chain(udp_srcp_dstp_len_csum.iter());
        // DHCP
        let zero = [0u8];
        let xid_bytes = xid.to_be_bytes();
        let dhcp_op_ht_hl_hop_s = [1u8, 1, 6, 0];
        let secs = match self.secs.elapsed_s() {
            Ok(s) => s as u16,
            Err(StopwatchErr::Overflow) => return Err(0x05),
            Err(StopwatchErr::Underflow) => return Err(0x06),
            Err(StopwatchErr::NotStarted) => return Err(0x07),
        };
        let dhcp_secs_flags = [(secs >> 8) as u8, secs as u8, 0, 0];
        let ciaddr_bytes = ciaddr.to_be_bytes();
        let dhcp_ci_yi_si_gi = ciaddr_bytes.iter().chain(zero.iter().cycle().take(12));
        let dhcp_chaddr = src_mac.iter().chain(zero.iter().cycle().take(10));
        let dhcp_sname_file = zero.iter().cycle().take(64 + 128);
        let dhcp_it = dhcp_op_ht_hl_hop_s
            .iter()
            .chain(xid_bytes.iter())
            .chain(dhcp_secs_flags.iter())
            .chain(dhcp_ci_yi_si_gi)
            .chain(dhcp_chaddr)
            .chain(dhcp_sname_file);
        let src_it = mac_it.chain(ip_udp_it).chain(dhcp_it);
        let mut header_bytes: usize = 0;
        for (dst, src) in pbuf.iter_mut().zip(src_it) {
            *dst = *src;
            header_bytes += 1;
        }
        Ok(header_bytes)
    }

    /// Build a DHCP discover packet by filling in a template of byte arrays.
    /// Returns Ok(data_length), where data_length is the number of bytes of pbuf.len()
    /// that were used to hold the packet data
    pub fn build_discover_frame<'a>(
        &mut self,
        mut pbuf: &'a mut [u8],
        src_mac: &[u8; 6],
        ip_id: u16,
    ) -> Result<u32, u8> {
        if pbuf.len() < DHCP_FRAME_LEN {
            return Err(0x08);
        }
        // Buffer might be a full MTU, so only use what we need.
        // (this determines number of loop iterations below)
        pbuf = &mut pbuf[..DHCP_FRAME_LEN];
        // Fill in the MAC, IP, UDP, and BOOTP headers for a DHCP packet
        let dst_mac = [255u8, 255, 255, 255, 255, 255];
        let ciaddr = 0u32;
        let ip_src = 0u32;
        let ip_dst = 0xffffffffu32;
        let header_bytes =
            self.build_dhcp_headers(&mut pbuf, src_mac, &dst_mac, ciaddr, ip_id, ip_src, ip_dst)?;

        let zero = [0u8];
        // DHCP options part 1: magic cookie, 53_type, 55_paramRequestList, 57_maxMsgSize, 61_clientId
        let dopt1 = [
            0x63 as u8, 0x82, 0x53, 0x63, 53, 1, 1, 55, 7, 1, 121, 3, 6, 15, 119, 252, 57, 2, 0x05,
            0xdc, 61, 7, 1,
        ];
        // Part 2: chain source MAC as Client ID to finish option 61
        let dopt2 = src_mac.iter();
        // Part 3: 51_IpLeaseTime, 12_hostname
        let dopt3 = [
            51u8,
            4,
            0x00,
            0x76,
            0xa7,
            0x00,
            12,
            self.hostname.len() as u8,
        ];
        // Part 4: chain hostname to finish option 12
        let dopt4 = self.hostname.as_bytes().iter();
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
        for (dst, src) in pbuf[header_bytes..].iter_mut().zip(dhcp_opts_it) {
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

    /// Build a DHCP request packet by filling in a template of byte arrays.
    /// Returns Ok(data_length), where data_length is the number of bytes of pbuf.len()
    /// that were used to hold the packet data
    pub fn build_request_frame<'a>(
        &mut self,
        mut pbuf: &'a mut [u8],
        src_mac: &[u8; 6],
        request_type: RequestType,
        ip_id: u16,
    ) -> Result<u32, u8> {
        if pbuf.len() < DHCP_FRAME_LEN {
            return Err(0x09);
        }
        // Buffer might be a full MTU, so only use what we need.
        // (this determines number of loop iterations below)
        pbuf = &mut pbuf[..DHCP_FRAME_LEN];
        // Fill in the MAC, IP, UDP, and BOOTP headers for a DHCP packet
        let mut dst_mac: [u8; 6] = [255; 6];
        let mut ciaddr: u32 = 0;
        let mut ip_src: u32 = 0;
        let mut ip_dst: u32 = 0xffffffff;
        match (self.gateway_mac, self.ip, self.sid) {
            (Some(gateway_mac), Some(ip), Some(sid)) => match request_type {
                RequestType::Renew => {
                    // RFC 2131 says Request packet for Renewing must be unicast
                    dst_mac = gateway_mac;
                    ip_src = ip;
                    ip_dst = sid;
                    ciaddr = ip;
                }
                RequestType::Rebind => {
                    // RFC 2131 says Request packet for Rebinding must be broadcast
                    ciaddr = ip;
                }
                _ => (),
            },
            _ => return Err(0x0A),
        };
        let header_bytes =
            self.build_dhcp_headers(&mut pbuf, src_mac, &dst_mac, ciaddr, ip_id, ip_src, ip_dst)?;

        let zero = [0u8];
        // DHCP options part 1: magic cookie, 53_type, 55_paramRequestList, 57_maxMsgSize, 61_clientId
        let dopt1 = [
            0x63 as u8, 0x82, 0x53, 0x63, 53, 1, 3, 55, 7, 1, 121, 3, 6, 15, 119, 252, 57, 2, 0x05,
            0xdc, 61, 7, 1,
        ];
        // Part 2: chain source MAC as Client ID to finish option 61
        let dopt2 = src_mac.iter();
        // Part 3: 50_RequestedIp, 54_ServerID
        let ri = match self.ip {
            Some(ip) => ip.to_be_bytes(),
            None => return Err(0x0B),
        };
        let sid = match self.sid {
            Some(sid) => sid.to_be_bytes(),
            None => return Err(0x0C),
        };
        let dopt3 = [
            50u8, 4, ri[0], ri[1], ri[2], ri[3], 54, 4, sid[0], sid[1], sid[2], sid[3],
        ];
        // Part 4: 12_hostname
        let dopt4 = [12, self.hostname.len() as u8];
        // Part 5: chain hostname to finish option 12
        let dopt5 = self.hostname.as_bytes().iter();
        // Part 6: 255_end
        let dopt6 = [255u8];
        let pad = zero.iter().cycle();
        let dhcp_opts_it = match request_type {
            // According to RFC 2131, Request packets in the Renewing or Rebinding state
            // "MUST NOT" fill in the requested IP or server ID options.
            RequestType::Renew | RequestType::Rebind => dopt1
                .iter()
                .chain(dopt2)
                .chain([].iter()) // omit part 3 (requested IP & server ID) to follow RFC 2131
                .chain(dopt4.iter())
                .chain(dopt5)
                .chain(dopt6.iter())
                .chain(pad),
            RequestType::Discover => dopt1
                .iter()
                .chain(dopt2)
                .chain(dopt3.iter())
                .chain(dopt4.iter())
                .chain(dopt5)
                .chain(dopt6.iter())
                .chain(pad),
        };
        for (dst, src) in pbuf[header_bytes..].iter_mut().zip(dhcp_opts_it) {
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
    ///
    /// DHCP fields from RFC 2131 (start offset in UDP payload; size in bytes):
    ///  op     (  0;   1):  Opcode: 1=BOOTREQUEST, 2=BOOTREPLY
    ///  htype  (  1;   1):  Hardware address type; 1 for Ethernet
    ///  hlen   (  2;   1):  Hardware address length; 6 for Etherent
    ///  hops   (  3;   1):  Client sets this to 0
    ///  xid    (  4;   4):  Transaction ID: picked randomly by client
    ///  secs   (  8;   2):  Seconds since start of exchange: filled in by client
    ///  flags  ( 10;   2):  Flags: clients set to 0 unless they cannot handle unicast
    ///  ciaddr ( 12;   4):  Client IP address filled in by client for BOUND, RENEW, or REBINDING
    ///  yiaddr ( 16;   4):  Your (client) IP address filled in by server
    ///  siaddr ( 20;   4):  [ignore] IP of next server for bootstrap flow (BOOTP)
    ///  giaddr ( 24;   4):  [ignore] IP of relay agent for booting with relay agent (BOOTP)
    ///  chaddr ( 28;  16):  Client hardware address filled in by client (Ethernet MAC + null pad)
    ///  sname  ( 44;  64):  [ignore] Server host name (BOOTP)
    ///  file   (108; 128):  [ignore] Boot file name (BOOTP)
    ///  option (236; ...):  Variable length options field starting with 4-byte magic cookie
    ///
    pub fn handle_frame(&mut self, data: &[u8]) -> FilterBin {
        if data.len() < MIN_DHCP_FRAME_LEN {
            return FilterBin::DropDhcp;
        }
        match self.state {
            State::Selecting | State::Requesting | State::Renewing | State::Rebinding => (),
            // No need to parse frame if state machine is not in state that expects a server response
            _ => return FilterBin::DropDhcp,
        };
        let dhcp = &data[MIN_UDP_FRAME_LEN..];
        let op_htype_hlen: &[u8] = &dhcp[0..3];
        const REPLY: u8 = 2;
        const ETHERNET: u8 = 1;
        const ETHERNET_MAC_LEN: u8 = 6;
        if op_htype_hlen != &[REPLY, ETHERNET, ETHERNET_MAC_LEN] {
            // Drop frames that are not replies configured for Ethernet
            return FilterBin::DropDhcp;
        }
        let xid: u32 = u32::from_be_bytes([dhcp[4], dhcp[5], dhcp[6], dhcp[7]]);
        match self.xid {
            Some(expected_xid) if xid == expected_xid => (),
            _ => return FilterBin::DropDhcp,
        };
        let yiaddr: u32 = u32::from_be_bytes([dhcp[16], dhcp[17], dhcp[18], dhcp[19]]);
        // CAUTION: ignoring client hardware address (chaddr)... is that bad?
        let option_mc: &[u8] = &dhcp[236..240];
        if option_mc != &[0x63, 0x82, 0x53, 0x63] {
            // Drop frames that don't have the correct options magic cookie
            return FilterBin::DropDhcp;
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
                    opts.ip_lease_time,
                    opts.subnet,
                    opts.dns,
                ) {
                    (Some(DHCPOFFER), Some(sid), Some(gw), Some(ilt), Some(sn), Some(dns)) => {
                        let mut gateway_mac: [u8; 6] = [0; 6];
                        for (dst, src) in gateway_mac.iter_mut().zip(&data[6..12]) {
                            *dst = *src;
                        }
                        self.handle_offer(sid, yiaddr, gw, &gateway_mac, ilt, sn, dns);
                        return FilterBin::Dhcp;
                    }
                    (Some(DHCPACK), _, _, Some(ilt), _, _) => {
                        self.handle_ack(ilt);
                        return FilterBin::Dhcp;
                    }
                    (Some(DHCPNAK), _, _, _, _, _) => {
                        self.handle_nak();
                        return FilterBin::Dhcp;
                    }
                    // Responses missing any of the required options will match here and get dropped
                    _ => return FilterBin::DropDhcp,
                }
            }
            Err(err_code) => {
                loghexln!(LL::Debug, "RXDHCP optsErr ", err_code);
                return FilterBin::DropDhcp;
            }
        }
    }
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
