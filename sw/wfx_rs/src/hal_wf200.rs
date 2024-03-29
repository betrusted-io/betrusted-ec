#![allow(non_upper_case_globals)]
use crate::betrusted_hal::hal_time::delay_ms;
use crate::betrusted_hal::hal_time::get_time_ms;
use crate::betrusted_hal::mem_locs::*;
use crate::wfx_bindings;
use core::slice;
use core::str;
use com_rs::ConnectResult;
use net::dhcp::{self, PacketNeeded};
use utralib::generated::{utra, CSR, HW_WIFI_BASE};
use com_rs::LinkState;

mod bt_wf200_pds;

use crate::pkt_buf::{PktBuf, MAX_PKTS};
use bt_wf200_pds::PDS_DATA;
use com_rs::serdes::Ipv4Conf;
use debug;
use debug::{log, loghex, loghexln, logln, LL};
use net::{self, filter::FilterBin};

// The mixed case constants here are the reason for the `allow(non_upper_case_globals)` above
pub use wfx_bindings::{
    sl_status_t, sl_wfx_buffer_type_t, sl_wfx_confirmations_ids_e_SL_WFX_CONNECT_CNF_ID,
    sl_wfx_confirmations_ids_e_SL_WFX_DISCONNECT_CNF_ID,
    sl_wfx_confirmations_ids_e_SL_WFX_SEND_FRAME_CNF_ID,
    sl_wfx_confirmations_ids_e_SL_WFX_SET_ARP_IP_ADDRESS_CNF_ID,
    sl_wfx_confirmations_ids_e_SL_WFX_START_SCAN_CNF_ID,
    sl_wfx_confirmations_ids_e_SL_WFX_STOP_SCAN_CNF_ID, sl_wfx_connect_ind_t, sl_wfx_context_t,
    sl_wfx_data_write, sl_wfx_disable_device_power_save, sl_wfx_disconnect_ind_t,
    sl_wfx_enable_device_power_save, sl_wfx_error_ind_t, sl_wfx_exception_ind_t,
    sl_wfx_fmac_status_e_WFM_STATUS_CONNECTION_ABORTED,
    sl_wfx_fmac_status_e_WFM_STATUS_CONNECTION_AUTH_FAILURE,
    sl_wfx_fmac_status_e_WFM_STATUS_CONNECTION_REJECTED_BY_AP,
    sl_wfx_fmac_status_e_WFM_STATUS_CONNECTION_TIMEOUT,
    sl_wfx_fmac_status_e_WFM_STATUS_NO_MATCHING_AP, sl_wfx_fmac_status_e_WFM_STATUS_SUCCESS,
    sl_wfx_general_confirmations_ids_e_SL_WFX_CONFIGURATION_CNF_ID,
    sl_wfx_general_confirmations_ids_e_SL_WFX_PTA_SETTINGS_CNF_ID,
    sl_wfx_general_indications_ids_e_SL_WFX_ERROR_IND_ID,
    sl_wfx_general_indications_ids_e_SL_WFX_EXCEPTION_IND_ID,
    sl_wfx_general_indications_ids_e_SL_WFX_GENERIC_IND_ID,
    sl_wfx_general_indications_ids_e_SL_WFX_STARTUP_IND_ID, sl_wfx_generic_ind_t,
    sl_wfx_generic_indication_type_e_SL_WFX_GENERIC_INDICATION_TYPE_RX_STATS,
    sl_wfx_generic_indication_type_e_SL_WFX_GENERIC_INDICATION_TYPE_STRING,
    sl_wfx_generic_message_t, sl_wfx_get_signal_strength, sl_wfx_host_bus_transfer_type_t,
    sl_wfx_host_bus_transfer_type_t_SL_WFX_BUS_READ, sl_wfx_indication_data_u,
    sl_wfx_indications_ids_e_SL_WFX_CONNECT_IND_ID,
    sl_wfx_indications_ids_e_SL_WFX_DISCONNECT_IND_ID,
    sl_wfx_indications_ids_e_SL_WFX_RECEIVED_IND_ID,
    sl_wfx_indications_ids_e_SL_WFX_SCAN_COMPLETE_IND_ID,
    sl_wfx_indications_ids_e_SL_WFX_SCAN_RESULT_IND_ID, sl_wfx_init,
    sl_wfx_interface_t_SL_WFX_STA_INTERFACE, sl_wfx_mac_address_t,
    sl_wfx_pm_mode_e_WFM_PM_MODE_ACTIVE, sl_wfx_pm_mode_e_WFM_PM_MODE_PS, sl_wfx_receive_frame,
    sl_wfx_received_ind_body_s, sl_wfx_received_ind_t, sl_wfx_register_address_t,
    sl_wfx_requests_ids_e_SL_WFX_GET_SIGNAL_STRENGTH_REQ_ID, sl_wfx_rx_stats_s,
    sl_wfx_scan_complete_ind_t, sl_wfx_scan_mode_e_WFM_SCAN_MODE_ACTIVE,
    sl_wfx_scan_result_ind_body_t, sl_wfx_scan_result_ind_t, sl_wfx_send_configuration,
    sl_wfx_send_ethernet_frame, sl_wfx_send_frame_req_t, sl_wfx_send_scan_command,
    sl_wfx_set_arp_ip_address, sl_wfx_set_power_mode, sl_wfx_ssid_def_t,
    sl_wfx_state_t_SL_WFX_STA_INTERFACE_CONNECTED, u_int32_t, SL_STATUS_ALLOCATION_FAILED,
    SL_STATUS_IO_TIMEOUT, SL_STATUS_OK, SL_STATUS_WIFI_SLEEP_GRANTED, SL_STATUS_WIFI_WRONG_STATE,
    SL_WFX_CONT_NEXT_LEN_MASK, SL_WFX_EXCEPTION_DATA_SIZE_MAX,
    sl_wfx_reg_read_32, sl_wfx_register_address_t_SL_WFX_CONFIG_REG_ID,
    sl_wfx_reg_read_16, sl_wfx_register_address_t_SL_WFX_CONTROL_REG_ID,
    sl_wfx_ext_auth_req_t,
};

// Configure Log Level (used in macro expansions)
const LOG_LEVEL: LL = LL::Debug;

// This is defined in wfx-fullMAC-driver/wfx_fmac_driver/firmware/sl_wfx_general_error_api.h in the enum
// typedef for sl_wfx_error_t. For some reason that I don't care to hunt down at the moment, this is not
// included in wfx_bindings. Whatever. Here it is:
const SL_WFX_HIF_BUS_ERROR: u32 = 0xf;

pub const WIFI_EVENT_WIRQ: u32 = 0x1;

// SSID scan state variables
static mut SSID_SCAN_UPDATE: bool = false;
static mut SSID_SCAN_FINISHED: bool = false;
pub const SSID_ARRAY_SIZE: usize = 8;
// format: [dbm as u8] [len as u8] [ssid as storage in [u8; 32]]
static mut SSID_ARRAY: [[u8; 34]; SSID_ARRAY_SIZE] = [[0; 34]; SSID_ARRAY_SIZE];
static mut SSID_INDEX: usize = 0;
static mut SSID_BEST_RSSI: Option<u8> = None;

// event state variables
pub const WIFI_MTU: usize = 1500;

// NOTE this assumption:
// once a packet is lodged into the PACKET_PENDING, it cannot cange
// until it has been read out. Thus all new incoming packets must be dropped.
// If the packet changes, then the read length reported to the SOC could change
// before the read happens. That would be Bad.

static mut PACKET_BUF: PktBuf = PktBuf {
    ptr_storage: [None; MAX_PKTS],
    enqueue_index: None,
    dequeue_index: None,
    was_polled: false,
    was_init: false,
};

static mut PACKETS_DROPPED: u32 = 0;
static mut DROPPED_UPDATED: bool = false;

pub fn init_pkt_buf() {
    unsafe {
        PACKET_BUF.init();
    }
}

pub fn drop_packet() {
    unsafe {
        PACKETS_DROPPED += 1;
        DROPPED_UPDATED = true;
    }
}
pub fn get_packets_dropped() -> u32 {
    unsafe {
        DROPPED_UPDATED = false;
        PACKETS_DROPPED
    }
}
pub fn poll_new_dropped() -> bool {
    unsafe {
        if DROPPED_UPDATED {
            DROPPED_UPDATED = false;
            true
        } else {
            false
        }
    }
}

pub fn peek_get_packet() -> Option<&'static [u8]> {
    unsafe { PACKET_BUF.peek_dequeue_slice() }
}
pub fn dequeue_packet() {
    unsafe {
        PACKET_BUF.dequeue();
    }
}
pub fn poll_new_avail() -> Option<u16> {
    unsafe { PACKET_BUF.poll_new_avail() }
}
pub fn poll_disconnect_pending() -> bool {
    unsafe {
        let ret = DISCONNECT_PENDING;
        DISCONNECT_PENDING = false;
        ret
    }
}
pub fn poll_connect_result() -> ConnectResult {
    unsafe {
        let ret = CONNECT_RESULT;
        CONNECT_RESULT = ConnectResult::Pending;
        ret
    }
}
pub fn poll_wfx_err_pending() -> bool {
    unsafe {
        let ret = WFX_ERR_PENDING;
        WFX_ERR_PENDING = false;
        ret
    }
}
/// Current link layer connection state
static mut CURRENT_STATUS: LinkState = LinkState::Unknown;
static mut DISCONNECT_PENDING: bool = false;
static mut CONNECT_RESULT: ConnectResult = ConnectResult::Pending;
static mut WFX_ERR_PENDING: bool = false;

/// Internet layer connection state
static mut NET_STATE: net::NetState = net::NetState::new();

/// WFX driver (link layer) context
static mut WIFI_CONTEXT: sl_wfx_context_t = sl_wfx_context_t {
    event_payload_buffer: [0; 512usize],
    firmware_build: 0,
    firmware_minor: 0,
    firmware_major: 0,
    data_frame_id: 0,
    used_buffers: 0,
    wfx_opn: [0; 14usize],
    mac_addr_0: sl_wfx_mac_address_t { octet: [0; 6usize] },
    mac_addr_1: sl_wfx_mac_address_t { octet: [0; 6usize] },
    state: 0,
};

// DANGER! DANGER! DANGER!
// The math for these PBUF_* constants, and related buffer slicing using them, determines
// the correctness of casting PBUF to a `*mut sl_wfx_send_frame_req_t` as required for
// calling sl_wfx_send_ethernet_frame(). Be wary of any code involving PBUF_* constants.
pub const PBUF_HEADER_SIZE: usize = core::mem::size_of::<sl_wfx_send_frame_req_t>();
const PBUF_DATA_SIZE: usize = WIFI_MTU;
pub const PBUF_SIZE: usize = PBUF_HEADER_SIZE + PBUF_DATA_SIZE;
/// Packet buffer for building outbound Ethernet II frames
static mut PBUF: [u8; PBUF_SIZE] = [0; PBUF_SIZE];

pub fn interface_status() -> LinkState {
    unsafe{CURRENT_STATUS}
}
/// Return string tag describing status of WF200
pub fn interface_status_tag() -> &'static str {
    match unsafe { CURRENT_STATUS } {
        LinkState::Unknown => "E1",
        LinkState::ResetHold => "off",
        LinkState::Uninitialized => "busy1",
        LinkState::Initializing => "busy2",
        LinkState::Disconnected => "down",
        LinkState::Connecting => "busy3",
        LinkState::Connected => "up",
        LinkState::WFXError => "E2",
    }
}

/// Export an API to retrieve net state for COM reporting
pub fn com_ipv4_config() -> Ipv4Conf {
    Ipv4Conf {
        dhcp: dhcp_get_state(),
        mac: unsafe { NET_STATE.mac },
        addr: match unsafe { NET_STATE.dhcp.ip } {
            Some(ip) => ip.to_be_bytes(),
            None => [0, 0, 0, 0],
        },
        gtwy: match unsafe { NET_STATE.dhcp.gateway } {
            Some(gw) => gw.to_be_bytes(),
            None => [0, 0, 0, 0],
        },
        mask: match unsafe { NET_STATE.dhcp.subnet } {
            Some(mask) => mask.to_be_bytes(),
            None => [0, 0, 0, 0],
        },
        dns1: match unsafe { NET_STATE.dhcp.dns } {
            Some(dns) => dns.to_be_bytes(),
            None => [0, 0, 0, 0],
        },
        dns2: [0; 4],
    }
}

#[allow(dead_code)]
fn log_hex(s: &[u8]) {
    for i in s {
        log!(LL::Debug, "{:02X}", *i);
    }
    log!(LL::Debug, " ");
}

pub fn send_net_packet(pkt: &mut [u8]) -> Result<(), ()> {
    unsafe {
        // Convert the byte buffer to a struct pointer for the sl_wfx API
        let frame_req_ptr: *mut sl_wfx_send_frame_req_t =
            pkt.as_mut_ptr() as *mut _ as *mut sl_wfx_send_frame_req_t;
        // Send the frame
        let result = sl_wfx_send_ethernet_frame(
            frame_req_ptr,
            pkt.len() as u32,
            sl_wfx_interface_t_SL_WFX_STA_INTERFACE,
            0,
        );
        match result {
            SL_STATUS_OK => Ok(()),
            e => {
                loghexln!(LL::Debug, "SendFrameErr ", e);
                Err(())
            }
        }
    }
}

/// Export an API for the main event loop to trigger a log dump of packet filter stats, etc.
pub fn log_net_state() {
    logln!(LL::Debug, "WF200Status {}", interface_status_tag());
    unsafe { NET_STATE.log_state() };
}

/// Export an API for the main event loop to reseed the network stack's PRNG
pub fn reseed_net_prng(seed: &[u16; 8]) {
    unsafe { NET_STATE.prng.reseed(seed) };
}

/// Export an API for access to the prng (because this one gets a TRNG seed from Xous at boot)
pub fn net_prng_rand() -> u32 {
    unsafe { NET_STATE.prng.next() }
}

/// Allow main event loop to disconnect the COM bus net bridge
pub fn set_com_net_bridge_enable(enable: bool) {
    unsafe { NET_STATE.set_com_net_bridge_enable(enable) };
}

/// Return dBm (positive) of strongest RSSI seen during all previous SSID scans
pub fn get_best_ssid_scan_rssi() -> Option<u8> {
    unsafe { SSID_BEST_RSSI }
}

/// Return RSSI of last packet received.
///
/// See Silicon Labs WFX API docs at:
/// https://docs.silabs.com/wifi/wf200/rtos/latest/group-f-u-l-l-m-a-c-d-r-i-v-e-r-a-p-i#ga38f335d89c3af730ea08e8d82e873d39
///
pub fn get_rssi() -> Result<u32, u8> {
    if unsafe { CURRENT_STATUS != LinkState::Connected } {
        return Err(0x20);
    }
    let mut rcpi: u32 = 0;
    let status: sl_status_t;
    status = unsafe { sl_wfx_get_signal_strength(&mut rcpi) };
    match status {
        SL_STATUS_OK => {
            // API docs say rcp range is 0 to 220; 0 means -110 dBm; 220 means 0 dBm; increment is 0.5 dBm
            let dbm = rcpi >> 1;
            Ok(dbm)
        }
        e => {
            loghexln!(LL::Debug, "GetRssiErr ", e);
            Err(0x01)
        }
    }
}

/// Enable WF200 ARP response offloading
/// See https://docs.silabs.com/wifi/wf200/rtos/latest/group-f-u-l-l-m-a-c-d-r-i-v-e-r-a-p-i#gabc2fb642a325bfda29d120f81f8df5f8
pub fn arp_begin_offloading() {
    if let Some(mut ip) = unsafe { NET_STATE.dhcp.ip } {
        let size = 1u8;
        match unsafe { sl_wfx_set_arp_ip_address(&mut ip, size) } {
            SL_STATUS_OK => logln!(LL::Debug, "ArpBegin"),
            err => loghexln!(LL::Debug, "ArpBeginErr ", err),
        };
    }
}

/// Disable WF200 ARP response offloading (docs say to set address to 0)
pub fn arp_stop_offloading() {
    let mut off = 0u32;
    let size = 1u8;
    match unsafe { sl_wfx_set_arp_ip_address(&mut off, size) } {
        SL_STATUS_OK => logln!(LL::Debug, "ArpEnd0"),
        // This happens after a WLAN_OFF command from COM (means radio is in reset)
        SL_STATUS_WIFI_WRONG_STATE => logln!(LL::Debug, "ArpEnd1"),
        err => loghexln!(LL::Debug, "ArpStopErr ", err),
    };
}

/// Return current state of DHCP state machine.
/// This is intended as a way for event loop to monitor DHCP handshake progress and detect slowness.
pub fn dhcp_get_state() -> com_rs::DhcpState {
    unsafe { NET_STATE.dhcp.get_state() }
}

/// Get a description of the DHCP state suitable for including in status string
pub fn dhcp_get_state_tag() -> &'static str {
    unsafe { NET_STATE.dhcp.get_state_tag() }
}

/// Check for notification of DHCP state changes to Bound or Halted
pub fn dhcp_pop_and_ack_change_event() -> Option<dhcp::DhcpEvent> {
    unsafe { NET_STATE.dhcp.pop_and_ack_change_event() }
}

/// Reset DHCP client state machine to start at INIT state with new random hostname
pub fn dhcp_reset() -> Result<(), u8> {
    let mut entropy = [0u32; 5];
    for dst in entropy.iter_mut() {
        *dst = unsafe { NET_STATE.prng.next() };
    }
    unsafe { NET_STATE.dhcp.begin_at_init(entropy) };
    let hostname = unsafe { NET_STATE.dhcp.hostname.as_str() };
    match unsafe { NET_STATE.dhcp.xid } {
        Some(xid) => {
            logln!(LL::Debug, "DhcpReset x:{:08X} h:{}", xid, hostname);
        }
        _ => return Err(0x01),
    }
    Ok(())
}

/// Inform DHCP state machine that the network link dropped
pub fn dhcp_handle_link_drop() {
    unsafe { NET_STATE.dhcp.handle_link_drop() };
}

/// Send a DHCP request
pub fn dhcp_do_next() -> Result<(), u8> {
    // Make sure the link is active before we try to use it
    if unsafe { CURRENT_STATUS != LinkState::Connected } {
        return Err(0x20);
    }
    // DANGER! DANGER! DANGER!
    //
    // The wfx driver API for sending frames takes an argument of a C struct with a zero
    // length array (aka flexible array member). Zero length arrays are dangerous because
    // they extend into memory beyond the size of the struct that declares them. You can't
    // just define a sl_wfx_send_frame_req_t and use it. Rather, you have to define a
    // buffer big enough to hold the sl_wfx_send_frame_req_t (header), plus however much
    // data goes in the frame (perhaps up to 1500 bytes), then cast a pointer to the buffer
    // into a sl_wfx_send_frame_req_t reference.
    //
    // The following code does those things. Be wary of this stuff. It is dangerous.
    //
    let src_mac: [u8; 6] = unsafe { NET_STATE.mac.clone() };
    let ip_id: u16 = unsafe { NET_STATE.prng.next() } as u16;
    unsafe {
        // CAUTION: PBUF is not zeroed between outbound packets, so old packet data may be
        // present in PBUF[data_length..PBUF_SIZE]. As long as the math for data_length
        // correctly specifies the length of the newly generated frame data, all should be
        // well when sl_wfx_send_ethernet_frame(..., data_length, ...) is called.
        let data_length: u32;
        // Clock the DHCP state machine and, depending on what it returns, maybe send a packet
        match NET_STATE.dhcp.cycle_clock() {
            PacketNeeded::Discover => {
                data_length = NET_STATE.dhcp.build_discover_frame(
                    &mut PBUF[PBUF_HEADER_SIZE..],
                    &src_mac,
                    ip_id,
                )?;
            }
            PacketNeeded::Request => {
                data_length = NET_STATE.dhcp.build_request_frame(
                    &mut PBUF[PBUF_HEADER_SIZE..],
                    &src_mac,
                    dhcp::RequestType::Discover,
                    ip_id,
                )?;
            }
            PacketNeeded::Renew => {
                data_length = NET_STATE.dhcp.build_request_frame(
                    &mut PBUF[PBUF_HEADER_SIZE..],
                    &src_mac,
                    dhcp::RequestType::Renew,
                    ip_id,
                )?;
            }
            PacketNeeded::Rebind => {
                data_length = NET_STATE.dhcp.build_request_frame(
                    &mut PBUF[PBUF_HEADER_SIZE..],
                    &src_mac,
                    dhcp::RequestType::Rebind,
                    ip_id,
                )?;
            }
            PacketNeeded::None => return Ok(()),
        }
        // Convert the byte buffer to a struct pointer for the sl_wfx API
        let frame_req_ptr: *mut sl_wfx_send_frame_req_t =
            PBUF.as_mut_ptr() as *mut _ as *mut sl_wfx_send_frame_req_t;
        // Send the frame
        let result = sl_wfx_send_ethernet_frame(
            frame_req_ptr,
            data_length,
            sl_wfx_interface_t_SL_WFX_STA_INTERFACE,
            0,
        );
        match result {
            SL_STATUS_OK => Ok(()),
            e => {
                loghexln!(LL::Debug, "SendFrameErr ", e);
                Err(0x21)
            }
        }
    }
}

/// Note -- PDS spec says max PDS size is 256 bytes, so let's just pin the buffer at that
/// returns true if send was OK
pub fn wf200_send_pds(data: [u8; 256], length: u16) -> bool {
    if length >= 256 {
        return false;
    }
    let pds_data: *const c_types::c_char = (&data).as_ptr() as *const c_types::c_char;
    let result = unsafe { sl_wfx_send_configuration(pds_data, length as u_int32_t) };
    if result == SL_STATUS_OK {
        true
    } else {
        loghexln!(LL::Debug, "SendPdsErr ", result);
        false
    }
}

pub fn wf200_ssid_get_list(ssid_list: &mut [[u8; 34]; SSID_ARRAY_SIZE]) {
    unsafe {
        for (dst, src) in ssid_list.iter_mut().zip(SSID_ARRAY.iter()) {
            for (d, s) in (*dst).iter_mut().zip(src.iter()) {
                *d = *s;
            }
        }
        // clear the array so we don't end up in limit cycles if we happen to have exactly 6 or 7 APs in range
        SSID_ARRAY = [[0; 34]; SSID_ARRAY_SIZE];
        SSID_INDEX = 0;
    }
}

/// a non-official structure that's baked into the sl_wfx_host.c file, and
/// is used to pass data between various functions within the driver
#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct host_context {
    pub sl_wfx_firmware_download_progress: u32,
    pub waited_event_id: u8,
    pub posted_event_id: u8,
}
static mut HOST_CONTEXT: host_context = host_context {
    sl_wfx_firmware_download_progress: 0,
    waited_event_id: 0,
    posted_event_id: 0,
};

trait Empty<T> {
    fn empty() -> T;
}
impl Empty<sl_wfx_mac_address_t> for sl_wfx_mac_address_t {
    fn empty() -> sl_wfx_mac_address_t {
        sl_wfx_mac_address_t { octet: [0; 6usize] }
    }
}
impl Empty<sl_wfx_context_t> for sl_wfx_context_t {
    fn empty() -> sl_wfx_context_t {
        sl_wfx_context_t {
            event_payload_buffer: [0; 512usize],
            firmware_build: 0,
            firmware_minor: 0,
            firmware_major: 0,
            data_frame_id: 0,
            used_buffers: 0,
            wfx_opn: [0; 14usize],
            mac_addr_0: sl_wfx_mac_address_t::empty(),
            mac_addr_1: sl_wfx_mac_address_t::empty(),
            state: 0,
        }
    }
}

pub fn wf200_fw_build() -> u8 {
    unsafe { WIFI_CONTEXT.firmware_build }
}
pub fn wf200_fw_minor() -> u8 {
    unsafe { WIFI_CONTEXT.firmware_minor }
}
pub fn wf200_fw_major() -> u8 {
    unsafe { WIFI_CONTEXT.firmware_major }
}

pub fn wfx_init() -> sl_status_t {
    unsafe {
        CURRENT_STATUS = LinkState::Initializing;
        // use this to drive porting of the wfx library
        let status = sl_wfx_init(&mut WIFI_CONTEXT);
        // Copy the MAC address for use by net module so it can remain blissfully unaware of the
        // sl_wfx_* APIs. The mac_addr_0 field the STA MAC address for the WFx station interface.
        // See https://docs.silabs.com/wifi/wf200/rtos/latest/structsl-wfx-context-t
        NET_STATE.set_mac(&(WIFI_CONTEXT.mac_addr_0.octet as [u8; 6]));
        return status;
    }
}

pub fn wfx_config() -> u32 {
    let mut config: u32 = 0;
    let config_ptr = &mut config as *mut u32;
    unsafe{ sl_wfx_reg_read_32(
        sl_wfx_register_address_t_SL_WFX_CONFIG_REG_ID,
        config_ptr);}
    config
}

pub fn wfx_control() -> u16 {
    let mut control: u16 = 0;
    let control_ptr = &mut control as *mut u16;
    unsafe{ sl_wfx_reg_read_16(
        sl_wfx_register_address_t_SL_WFX_CONTROL_REG_ID,
        control_ptr
    );}
    control
}

// above is glue to Rust subsystem
// //////////////////////////////////////////////////////////////////////////////////
// //////////////////////////////////////////////////////////////////////////////////
// //////////////////////////////////////////////////////////////////////////////////
// //////////////////////////////////////////////////////////////////////////////////
// //////////////////////////////////////////////////////////////////////////////////
// //////////////////////////////////////////////////////////////////////////////////
// below is glue to wfx host drivers

#[export_name = "sl_wfx_host_spi_cs_assert"]
pub unsafe extern "C" fn sl_wfx_host_spi_cs_assert() -> sl_status_t {
    let mut wifi_csr = CSR::new(HW_WIFI_BASE as *mut u32);
    wifi_csr.wfo(utra::wifi::CS_CS, 1);
    SL_STATUS_OK
}

#[export_name = "sl_wfx_host_spi_cs_deassert"]
pub unsafe extern "C" fn sl_wfx_host_spi_cs_deassert() -> sl_status_t {
    let mut wifi_csr = CSR::new(HW_WIFI_BASE as *mut u32);
    wifi_csr.wfo(utra::wifi::CS_CS, 0);
    SL_STATUS_OK
}

#[export_name = "sl_wfx_host_deinit_bus"]
pub unsafe extern "C" fn sl_wfx_host_deinit_bus() -> sl_status_t {
    CURRENT_STATUS = LinkState::Uninitialized;
    let mut wifi_csr = CSR::new(HW_WIFI_BASE as *mut u32);
    wifi_csr.wo(utra::wifi::CONTROL, 0);
    wifi_csr.wo(utra::wifi::WIFI, 0);
    SL_STATUS_OK
}

#[export_name = "sl_wfx_host_enable_platform_interrupt"]
pub unsafe extern "C" fn sl_wfx_host_enable_platform_interrupt() -> sl_status_t {
    // NOP -- we're doing polling for now
    SL_STATUS_OK
}

#[export_name = "sl_wfx_host_disable_platform_interrupt"]
pub unsafe extern "C" fn sl_wfx_host_disable_platform_interrupt() -> sl_status_t {
    // NOP -- we're doing polling for now
    SL_STATUS_OK
}

#[export_name = "sl_wfx_host_init_bus"]
pub unsafe extern "C" fn sl_wfx_host_init_bus() -> sl_status_t {
    let mut wifi_csr = CSR::new(HW_WIFI_BASE as *mut u32);
    wifi_csr.wo(utra::wifi::CONTROL, 0);
    wifi_csr.wo(utra::wifi::WIFI, 0);
    SL_STATUS_OK
}

#[export_name = "sl_wfx_host_reset_chip"]
pub unsafe extern "C" fn sl_wfx_host_reset_chip() -> sl_status_t {
    let mut wifi_csr = CSR::new(HW_WIFI_BASE as *mut u32);
    wifi_csr.wfo(utra::wifi::WIFI_RESET, 1);
    delay_ms(10);
    wifi_csr.wfo(utra::wifi::WIFI_RESET, 0);
    delay_ms(10);

    // TODO: marshall all these state variables into a single object so we don't lose track of them.
    WFX_ERR_PENDING = false;

    // clear "mallocs"
    WFX_PTR_LIST = [0; WFX_MAX_PTRS];

    // reset dhcp state and net state
    dhcp_handle_link_drop();
    CURRENT_STATUS = LinkState::Uninitialized;
    DISCONNECT_PENDING = false;
    DROPPED_UPDATED = false;
    NET_STATE = net::NetState::new();
    CONNECT_RESULT = ConnectResult::Pending;

    // clear ssid scan state
    SSID_SCAN_UPDATE = false;
    SSID_SCAN_FINISHED = false;
    // I think it's OK to keep these "stale" values around because the SSID environment is external to the driver
    //SSID_INDEX = 0;
    //SSID_BEST_RSSI = None;
    //SSID_ARRAY = [[0; 34]; SSID_ARRAY_SIZE];

    // clear the packet buf
    PACKET_BUF.init();

    // reset FFI contexts
    HOST_CONTEXT = host_context {
        sl_wfx_firmware_download_progress: 0,
        waited_event_id: 0,
        posted_event_id: 0,
    };
    WIFI_CONTEXT = sl_wfx_context_t {
        event_payload_buffer: [0; 512usize],
        firmware_build: 0,
        firmware_minor: 0,
        firmware_major: 0,
        data_frame_id: 0,
        used_buffers: 0,
        wfx_opn: [0; 14usize],
        mac_addr_0: sl_wfx_mac_address_t { octet: [0; 6usize] },
        mac_addr_1: sl_wfx_mac_address_t { octet: [0; 6usize] },
        state: 0,
    };

    SL_STATUS_OK
}

#[export_name = "sl_wfx_host_hold_in_reset"]
pub unsafe extern "C" fn sl_wfx_host_hold_in_reset() -> sl_status_t {
    let mut wifi_csr = CSR::new(HW_WIFI_BASE as *mut u32);
    wifi_csr.wfo(utra::wifi::WIFI_RESET, 1);
    // Allow a little time for reset signal to take effect before returning
    delay_ms(1);
    CURRENT_STATUS = LinkState::ResetHold;
    SSID_SCAN_UPDATE = false;
    SSID_SCAN_FINISHED = false;
    SL_STATUS_OK
}

#[export_name = "sl_wfx_host_wait"]
pub unsafe extern "C" fn sl_wfx_host_wait(wait_ms: u32) -> sl_status_t {
    delay_ms(wait_ms);
    SL_STATUS_OK
}

#[export_name = "sl_wfx_host_set_wake_up_pin"]
pub unsafe extern "C" fn sl_wfx_host_set_wake_up_pin(state: u8) -> sl_status_t {
    let mut wifi_csr = CSR::new(HW_WIFI_BASE as *mut u32);
    if state == 0 {
        wifi_csr.rmwf(utra::wifi::WIFI_WAKEUP, 0);
    } else {
        wifi_csr.rmwf(utra::wifi::WIFI_WAKEUP, 1);
    }
    SL_STATUS_OK
}

/// no locking because we're single threaded and one process only to drive all of this
#[export_name = "sl_wfx_host_lock"]
pub unsafe extern "C" fn sl_wfx_host_lock() -> sl_status_t {
    // NOP -- no interrupts or multi-threading for now
    // TODO: maybe revisit this
    SL_STATUS_OK
}
#[export_name = "sl_wfx_host_unlock"]
pub unsafe extern "C" fn sl_wfx_host_unlock() -> sl_status_t {
    // NOP -- no interrupts or multi-threading for now
    // TODO: maybe revisit this
    SL_STATUS_OK
}

static mut DEEP_DEBUG: bool = false;
pub fn set_deep_debug(state: bool) {
    unsafe{DEEP_DEBUG = state};
}
#[doc = " @brief Send data on the SPI bus"]
#[doc = ""]
#[doc = " @param type is the type of bus action (see ::sl_wfx_host_bus_transfer_type_t)"]
#[doc = " @param header is a pointer to the header data"]
#[doc = " @param header_length is the length of the header data"]
#[doc = " @param buffer is a pointer to the buffer data"]
#[doc = " @param buffer_length is the length of the buffer data"]
#[doc = " @returns Returns SL_STATUS_OK if successful, SL_STATUS_FAIL otherwise"]
#[export_name = "sl_wfx_host_spi_transfer_no_cs_assert"]
pub unsafe extern "C" fn sl_wfx_host_spi_transfer_no_cs_assert(
    type_: sl_wfx_host_bus_transfer_type_t,
    header: *mut u8,
    header_length: u16,
    buffer: *mut u8,
    buffer_length: u16,
) -> sl_status_t {
    let mut wifi_csr = CSR::new(HW_WIFI_BASE as *mut u32);

    let mut hdr_buf = [0u16; 16];
    let mut hdr_index = 0;
    let mut hdr_printed = false;
    let mut suppress = false;
    {
        // we do "MTU" in case header_len is odd. should never be but...this is their API
        let mut header_len_mtu = header_length / 2;
        let mut header_pos: usize = 0;
        let headeru16: *mut u16 = header as *mut u16;
        while header_len_mtu > 0 {
            //let word: u16 = ((header.add(header_pos).read() as u16) << 8) | (header.add(header_pos + 1).read() as u16);
            let word: u16 = headeru16.add(header_pos).read();
            hdr_buf[hdr_index] = word;
            hdr_index += 1;

            wifi_csr.wo(utra::wifi::TX, word as u32);
            header_len_mtu -= 1;
            header_pos += 1;

            wifi_csr.wfo(utra::wifi::CONTROL_GO, 1);
            while wifi_csr.rf(utra::wifi::STATUS_TIP) == 1 {}
            wifi_csr.wfo(utra::wifi::CONTROL_GO, 0);
        }
        if type_ == sl_wfx_host_bus_transfer_type_t_SL_WFX_BUS_READ {
            let mut buffer_len_mtu = buffer_length / 2;
            let mut buffer_pos: usize = 0;
            let bufferu16: *mut u16 = buffer as *mut u16;
            while buffer_len_mtu > 0 {
                // transmit a dummy word to get the rx data
                wifi_csr.wo(utra::wifi::TX, 0);
                wifi_csr.wfo(utra::wifi::CONTROL_GO, 1);
                while wifi_csr.rf(utra::wifi::STATUS_TIP) == 1 {}
                wifi_csr.wfo(utra::wifi::CONTROL_GO, 0);

                let word: u16 = wifi_csr.rf(utra::wifi::RX_RX) as u16;
                if DEEP_DEBUG {
                    if !hdr_printed {
                        hdr_printed = true;
                        if hdr_buf[0] == 0x0190 && word == 0x3000 {
                            suppress = true;
                        } else {
                            log!(LL::Debug, "HDR ");
                            for &w in hdr_buf[..hdr_index].iter() {
                                log!(LL::Debug, "{:04x}", w);
                            }
                            log!(LL::Debug, "\n\rREAD ");
                            log!(LL::Debug, "{:04x} ", word);
                        }
                    } else {
                        log!(LL::Debug, "{:04x} ", word);
                    }
                }
                bufferu16.add(buffer_pos).write(word);
                //buffer.add(buffer_pos).write((word >> 8) as u8);
                //buffer.add(buffer_pos+1).write((word & 0xff) as u8);
                buffer_len_mtu -= 1;
                buffer_pos += 1;
            }
        } else {
            if DEEP_DEBUG {
                log!(LL::Debug, "HDR ");
                for &w in hdr_buf[..hdr_index].iter() {
                    log!(LL::Debug, "{:04x}", w);
                }
                log!(LL::Debug, "\n\rWRITE ");
            }
            // transmit the buffer
            let buffer_len_mtu: usize = buffer_length as usize / 2;
            let mut buffer_pos: usize = 0;
            let bufferu16: *mut u16 = buffer as *mut u16;
            while buffer_pos < buffer_len_mtu {
                //let word: u16 = ((buffer.add(buffer_pos).read() as u16) << 8) | (buffer.add(buffer_pos+1).read() as u16);
                let word: u16 = bufferu16.add(buffer_pos).read();
                if DEEP_DEBUG {
                    log!(LL::Debug, "{:04x} ", word);
                }
                wifi_csr.wo(utra::wifi::TX, word as u32);
                //                buffer_len_mtu -= 1;
                buffer_pos += 1;

                wifi_csr.wfo(utra::wifi::CONTROL_GO, 1);
                while wifi_csr.rf(utra::wifi::STATUS_TIP) == 1 {}
                wifi_csr.wfo(utra::wifi::CONTROL_GO, 0);
            }
        }
        if DEEP_DEBUG && !suppress {
            logln!(LL::Debug, "");
        }
    }
    SL_STATUS_OK
}

// crappy alloc constants
static mut WFX_RAM_ALLOC: usize = WFX_RAM_OFFSET;
pub const WFX_MAX_PTRS: usize = 4;
static mut WFX_PTR_LIST: [usize; WFX_MAX_PTRS] = [0; WFX_MAX_PTRS];
pub const WFX_ALLOC_MAXLEN: usize = WFX_RAM_LENGTH / WFX_MAX_PTRS;
static mut WFX_OVERSIZE_COUNT: usize = 0;
static mut WFX_ALLOC_FAILS: usize = 0;

pub unsafe fn alloc_free_count() -> usize {
    let mut count = 0;
    for ptr in WFX_PTR_LIST {
        if ptr == 0 {
            count += 1;
        }
    }
    count
}
pub unsafe fn alloc_oversize_count() -> usize { WFX_OVERSIZE_COUNT }
pub unsafe fn alloc_fail_count() -> usize { WFX_ALLOC_FAILS }

#[doc = " @brief Called when the driver wants to allocate memory"]
#[doc = ""]
#[doc = " @param buffer is a pointer to the data"]
#[doc = " @param type is the type of buffer to allocate (see ::sl_wfx_buffer_type_t)"]
#[doc = " @param buffer_size represents the amount of memory to allocate"]
#[doc = " @returns Returns SL_STATUS_OK if successful, SL_STATUS_FAIL otherwise"]
#[doc = ""]
#[doc = " @note Called by the driver every time it needs memory"]
#[export_name = "sl_wfx_host_allocate_buffer"]
pub unsafe extern "C" fn sl_wfx_host_allocate_buffer(
    buffer: *mut *mut c_types::c_void,
    _type_: sl_wfx_buffer_type_t,
    buffer_size: u32,
) -> sl_status_t {
    if buffer_size as usize > WFX_ALLOC_MAXLEN {
        logln!(
            LL::Error,
            "Alloc {} larger than max of {}!",
            buffer_size,
            WFX_ALLOC_MAXLEN
        );
        WFX_OVERSIZE_COUNT += 1;
        return SL_STATUS_ALLOCATION_FAILED;
    }

    // find the first "0" entry in the pointer list
    let mut i = 0;
    while (WFX_PTR_LIST[i] != 0) && (i < WFX_MAX_PTRS as usize) {
        i += 1;
    }
    if i == WFX_MAX_PTRS {
        WFX_ALLOC_FAILS += 1;
        logln!(LL::Debug, "AllocFailNoPtr");
        return SL_STATUS_ALLOCATION_FAILED;
    }
    WFX_PTR_LIST[i] = WFX_RAM_ALLOC + i * WFX_ALLOC_MAXLEN;
    *buffer = WFX_PTR_LIST[i] as *mut c_types::c_void;

    logln!(LL::Trace, "Alloc [{}]:{}", i, buffer_size);

    SL_STATUS_OK
}

#[doc = " @brief Called when the driver wants to free memory"]
#[doc = ""]
#[doc = " @param buffer is the pointer to the memory to free"]
#[doc = " @param type is the type of buffer to free (see ::sl_wfx_buffer_type_t)"]
#[doc = " @returns Returns SL_STATUS_OK if successful, SL_STATUS_FAIL otherwise"]
#[export_name = "sl_wfx_host_free_buffer"]
pub unsafe extern "C" fn sl_wfx_host_free_buffer(
    buffer: *mut c_types::c_void,
    _type_: sl_wfx_buffer_type_t,
) -> sl_status_t {
    let mut i = 0;
    let addr: usize = (buffer as *mut c_types::c_uint) as usize;
    while (WFX_PTR_LIST[i] != addr) && (i < WFX_MAX_PTRS as usize) {
        i = i + 1;
    }
    if i == WFX_MAX_PTRS {
        logln!(LL::Debug, "FreeFail");
        return SL_STATUS_ALLOCATION_FAILED;
    }
    logln!(LL::Trace, "DeAlloc [{}]", i);
    WFX_PTR_LIST[i] = 0;
    SL_STATUS_OK
}

/// clear the shitty allocator list if we're re-initializing the driver
/// also clear all the static muts (e.g. "C globals") that the driver depends upon
#[export_name = "sl_wfx_host_init"]
pub unsafe extern "C" fn sl_wfx_host_init() -> sl_status_t {
    WFX_RAM_ALLOC = WFX_RAM_OFFSET;
    WFX_PTR_LIST = [0; WFX_MAX_PTRS];
    HOST_CONTEXT.sl_wfx_firmware_download_progress = 0;
    //    HOST_CONTEXT.waited_event_id = 0;  // this is apparently side-effected elsewhere
    HOST_CONTEXT.posted_event_id = 0;
    WIFI_CONTEXT = sl_wfx_context_t {
        event_payload_buffer: [0; 512usize],
        firmware_build: 0,
        firmware_minor: 0,
        firmware_major: 0,
        data_frame_id: 0,
        used_buffers: 0,
        wfx_opn: [0; 14usize],
        mac_addr_0: sl_wfx_mac_address_t { octet: [0; 6usize] },
        mac_addr_1: sl_wfx_mac_address_t { octet: [0; 6usize] },
        state: 0,
    };
    SL_STATUS_OK
}

#[export_name = "sl_wfx_host_deinit"]
pub unsafe extern "C" fn sl_wfx_host_deinit() -> sl_status_t {
    WFX_RAM_ALLOC = WFX_RAM_OFFSET;
    WFX_PTR_LIST = [0; WFX_MAX_PTRS];
    SL_STATUS_OK
}

#[doc = " @brief Called when the driver is waiting for a confirmation"]
#[doc = ""]
#[doc = " @param confirmation_id is the ID to be waited"]
#[doc = " @param timeout_ms is the time before the command times out"]
#[doc = " @param event_payload_out is a pointer to the data returned by the"]
#[doc = " confirmation"]
#[doc = " @returns Returns SL_STATUS_OK if successful, SL_STATUS_FAIL otherwise"]
#[doc = ""]
#[doc = " @note Called every time a API command is called"]
#[export_name = "sl_wfx_host_wait_for_confirmation"]
pub unsafe extern "C" fn sl_wfx_host_wait_for_confirmation(
    confirmation_id: u8,
    timeout_ms: u32,
    event_payload_out: *mut *mut c_types::c_void,
) -> sl_status_t {
    let start_time = get_time_ms();
    while (get_time_ms() - start_time) < timeout_ms {
        let mut control_register: u16 = 0;
        loop {
            sl_wfx_receive_frame(&mut control_register);
            if (control_register & SL_WFX_CONT_NEXT_LEN_MASK as u16) == 0 {
                break;
            }
        }
        if confirmation_id == HOST_CONTEXT.posted_event_id {
            HOST_CONTEXT.posted_event_id = 0;
            if event_payload_out
                != (::core::ptr::null::<c_types::c_void> as *mut *mut c_types::c_void)
            {
                *event_payload_out =
                    WIFI_CONTEXT.event_payload_buffer.as_ptr() as *mut c_types::c_void;
            }
            return SL_STATUS_OK;
        } else {
            delay_ms(1);
        }
    }
    logln!(LL::Debug, "hostWaitTimeout");
    logln!(LL::Debug, "cur {}", get_time_ms());
    logln!(LL::Debug, "sta {}", start_time);
    logln!(LL::Debug, "out {}", timeout_ms);
    SL_STATUS_IO_TIMEOUT
}

#[doc = " @brief Set up the next event that the driver will wait"]
#[doc = ""]
#[doc = " @param event_id is the ID to be waited"]
#[doc = " @returns Returns SL_STATUS_OK if successful, SL_STATUS_FAIL otherwise"]
#[doc = ""]
#[doc = " @note Called every time a API command is called"]
#[export_name = "sl_wfx_host_setup_waited_event"]
pub unsafe extern "C" fn sl_wfx_host_setup_waited_event(event_id: u8) -> sl_status_t {
    HOST_CONTEXT.waited_event_id = event_id;
    SL_STATUS_OK
}

#[doc = " @brief Called when the driver sends a frame to the WFx chip"]
#[doc = ""]
#[doc = " @param frame is a pointer to the frame data"]
#[doc = " @param frame_len is size of the frame"]
#[doc = " @returns Returns SL_STATUS_OK if successful, SL_STATUS_FAIL otherwise"]
#[export_name = "sl_wfx_host_transmit_frame"]
pub unsafe extern "C" fn sl_wfx_host_transmit_frame(
    frame: *mut c_types::c_void,
    frame_len: u32,
) -> sl_status_t {
    let ret: sl_status_t;
    ret = sl_wfx_data_write(frame, frame_len);
    if ret != SL_STATUS_OK {
        loghexln!(LL::Debug, "TxFrmErr ", ret);
    }
    ret
}

#[doc = " @brief Driver hook to retrieve the firmware size"]
#[doc = ""]
#[doc = " @param firmware_size is a pointer to the firmware size value"]
#[doc = " @returns Returns SL_STATUS_OK if successful, SL_STATUS_FAIL otherwise"]
#[doc = ""]
#[doc = " @note Called once during the driver initialization phase"]
#[export_name = "sl_wfx_host_get_firmware_size"]
pub unsafe extern "C" fn sl_wfx_host_get_firmware_size(firmware_size: *mut u32) -> sl_status_t {
    *firmware_size = WFX_FIRMWARE_SIZE as u32;
    SL_STATUS_OK
}

#[doc = " @brief Driver hook to retrieve a firmware chunk"]
#[doc = ""]
#[doc = " @param data is a pointer to the firmware data"]
#[doc = " @param data_size is the size of data requested by the driver"]
#[doc = " @returns Returns SL_STATUS_OK if successful, SL_STATUS_FAIL otherwise"]
#[doc = ""]
#[doc = " @note Called multiple times during the driver initialization phase"]
#[export_name = "sl_wfx_host_get_firmware_data"]
pub unsafe extern "C" fn sl_wfx_host_get_firmware_data(
    data: *mut *const u8,
    data_size: u32,
) -> sl_status_t {
    *data = (WFX_FIRMWARE_OFFSET + HOST_CONTEXT.sl_wfx_firmware_download_progress as usize)
        as *const u8;
    HOST_CONTEXT.sl_wfx_firmware_download_progress += data_size;
    SL_STATUS_OK
}

#[doc = " @brief Called when the driver is considering putting the WFx in"]
#[doc = " sleep mode"]
#[doc = ""]
#[doc = " @param type is the type of the message sent"]
#[doc = " @param address is the address of the message sent"]
#[doc = " @param length is the length of the message to be sent"]
#[doc = " @returns Returns SL_STATUS_WIFI_SLEEP_GRANTED to let the WFx go to sleep,"]
#[doc = " SL_STATUS_WIFI_SLEEP_NOT_GRANTED otherwise"]
#[doc = ""]
#[doc = " @note The parameters are given as information for the host to take a decision"]
#[doc = " on whether or not the WFx is put back to sleep mode."]
#[export_name = "sl_wfx_host_sleep_grant"]
pub unsafe extern "C" fn sl_wfx_host_sleep_grant(
    _type_: sl_wfx_host_bus_transfer_type_t,
    _address: sl_wfx_register_address_t,
    _length: u32,
) -> sl_status_t {
    SL_STATUS_WIFI_SLEEP_GRANTED
}

#[doc = " @brief Called once the WFx chip is waking up"]
#[doc = " @returns Returns SL_STATUS_OK if successful, SL_STATUS_FAIL otherwise"]
#[doc = ""]
#[doc = " @note Called if the sleep mode is enabled. The function waits for the WFx"]
#[doc = " interruption"]
#[export_name = "sl_wfx_host_wait_for_wake_up"]
pub unsafe extern "C" fn sl_wfx_host_wait_for_wake_up() -> sl_status_t {
    delay_ms(2); // don't ask me, this is literally the reference vendor code!
    SL_STATUS_OK
}

#[export_name = "strlen"]
pub unsafe extern "C" fn strlen(__s: *const c_types::c_char) -> c_types::c_ulong {
    let mut len: usize = 0;

    while (__s).add(len).read() != 0 {
        len += 1;
    }

    len as c_types::c_ulong
}

/// this is a hyper-targeted implementation of strtoul for the instance where it is called in
/// referenced by sl_wfx.c:1527 (wfx-fullMAC-driver/wfx_fmac_driver/sl_wfx.c:1527):
/// endptr is NULL, base is 16
#[export_name = "strtoul"]
pub unsafe extern "C" fn strtoul(
    __nptr: *const c_types::c_char,
    __endptr: *mut *mut c_types::c_char,
    __base: c_types::c_int,
) -> c_types::c_ulong {
    // check this is according to the specs we anticipate
    //
    // DANGER! DANGER! DANGER!
    //
    // These asserts could cause problems both in terms of panicking and linking extra code.
    // TODO: Consider if these can be removed
    //
    assert!(__base == 16 as c_types::c_int);
    assert!(__endptr == ::core::ptr::null::<c_types::c_void> as *mut *mut c_types::c_char);
    let mut length: usize = 0;
    while (__nptr).add(length).read() != 0 {
        length += 1;
    }
    let s = str::from_utf8(slice::from_raw_parts(__nptr as *const u8, length))
        .expect("unable to parse string");
    usize::from_str_radix(s.trim_start_matches("0x"), 16).expect("unable to parse num")
        as c_types::c_ulong
}

#[doc = " @brief Driver hook to retrieve a PDS line"]
#[doc = ""]
#[doc = " @param pds_data is a pointer to the PDS data"]
#[doc = " @param index is the index of the line requested by the driver"]
#[doc = " @returns Returns SL_STATUS_OK if successful, SL_STATUS_FAIL otherwise"]
#[doc = ""]
#[doc = " @note Called multiple times during the driver initialization phase"]
#[export_name = "sl_wfx_host_get_pds_data"]
pub unsafe extern "C" fn sl_wfx_host_get_pds_data(
    pds_data: *mut *const c_types::c_char,
    index: u16,
) -> sl_status_t {
    // pds should be static data so it will not go out of scope when this function terminates
    // so weird! suspicious bunnie is suspicious.
    *pds_data = (&PDS_DATA[index as usize]).as_ptr() as *const c_types::c_char;
    SL_STATUS_OK
}

#[doc = " @brief Driver hook to get the number of lines of the PDS"]
#[doc = ""]
#[doc = " @param pds_size is a pointer to the PDS size value"]
#[doc = " @returns Returns SL_STATUS_OK if successful, SL_STATUS_FAIL otherwise"]
#[doc = ""]
#[doc = " @note Called once during the driver initialization phase"]
#[export_name = "sl_wfx_host_get_pds_size"]
pub unsafe extern "C" fn sl_wfx_host_get_pds_size(pds_size: *mut u16) -> sl_status_t {
    *pds_size = PDS_DATA.len() as u16;
    SL_STATUS_OK
}

fn sl_wfx_connect_callback(_mac: [u8; 6usize], status: u32) {
    let mut new_status = LinkState::Disconnected;
    match status {
        sl_wfx_fmac_status_e_WFM_STATUS_SUCCESS => {
            unsafe{CONNECT_RESULT = ConnectResult::Success;}
            logln!(LL::Debug, "ConnSuccess");
            new_status = LinkState::Connected;
            unsafe {
                NET_STATE.filter_stats.reset();
                WIFI_CONTEXT.state |= sl_wfx_state_t_SL_WFX_STA_INTERFACE_CONNECTED;
                // TODO: configure power saving features
                //sl_wfx_set_power_mode(sl_wfx_pm_mode_e_WFM_PM_MODE_PS, 0);
                //sl_wfx_enable_device_power_save();
            }
        }
        sl_wfx_fmac_status_e_WFM_STATUS_NO_MATCHING_AP => {
            unsafe{CONNECT_RESULT = ConnectResult::NoMatchingAp;}
            logln!(LL::Debug, "ConnNoMatchAp");
        }
        sl_wfx_fmac_status_e_WFM_STATUS_CONNECTION_ABORTED => {
            unsafe{CONNECT_RESULT = ConnectResult::Aborted;}
            logln!(LL::Debug, "ConnAbort");
        }
        sl_wfx_fmac_status_e_WFM_STATUS_CONNECTION_TIMEOUT => {
            unsafe{CONNECT_RESULT = ConnectResult::Timeout;}
            logln!(LL::Debug, "ConnTimeout");
        }
        sl_wfx_fmac_status_e_WFM_STATUS_CONNECTION_REJECTED_BY_AP => {
            unsafe{CONNECT_RESULT = ConnectResult::Reject;}
            logln!(LL::Debug, "ConnReject");
        }
        sl_wfx_fmac_status_e_WFM_STATUS_CONNECTION_AUTH_FAILURE => {
            unsafe{CONNECT_RESULT = ConnectResult::AuthFail;}
            logln!(LL::Debug, "ConnAuthFail");
        }
        _ => {
            unsafe{CONNECT_RESULT = ConnectResult::Error;}
            loghexln!(LL::Debug, "ConnErr ", status);
        }
    }
    unsafe {
        CURRENT_STATUS = new_status;
    }
}

fn sl_wfx_disconnect_callback(_mac: [u8; 6usize], reason: u16) {
    loghexln!(LL::Debug, "WfxDisconn ", reason);
    unsafe {
        CURRENT_STATUS = LinkState::Disconnected;
        DISCONNECT_PENDING = true;
        WIFI_CONTEXT.state &= !sl_wfx_state_t_SL_WFX_STA_INTERFACE_CONNECTED;
    }
    dhcp_handle_link_drop();
    unsafe{PACKET_BUF.init()} // reset the packet buffer to a known state
}

fn sl_wfx_host_received_frame_callback(rx_buffer: *const sl_wfx_received_ind_t) {
    let body: &sl_wfx_received_ind_body_s;
    unsafe {
        if rx_buffer.is_null() {
            logln!(LL::Warn, "WfxRxFr Null");
            return;
        }
        body = &(*rx_buffer).body;
    }
    let _frame_type: u8 = body.frame_type;
    let padding = body.frame_padding as usize;
    let length = body.frame_length as usize;
    let data = unsafe { &body.frame.as_slice(length + padding)[padding..] };

    // This will give the EC's DHCP client and packet filter first dibs on the packet
    let filter_bin = net::handle_frame(unsafe { &mut NET_STATE }, data);

    // If the filter bin came back as ComFwd, that means the EC wants the packet to be
    // forwarded up the COM bus net bridge. The com_net_bridge_enable check relates to an
    // AT command in the EC main event loop
    if (filter_bin == FilterBin::ComFwd) && unsafe { NET_STATE.com_net_bridge_enable } {
        if let Some(_) = unsafe { NET_STATE.dhcp.ip } {
            let maybe_pkt = unsafe { PACKET_BUF.get_enqueue_slice(data.len()) };
            if let Some(pkt) = maybe_pkt {
                log!(LL::Debug, "R"); // RX packet for COM bus
                for (&src, dst) in data.iter().zip(pkt.iter_mut()) {
                    *dst = src;
                }
            } else {
                log!(LL::Debug, "D"); // Drop RX packet because COM bus congested
                drop_packet();
            }
        }
    }
}

unsafe fn sl_wfx_scan_result_callback(scan_result: *const sl_wfx_scan_result_ind_body_t) {
    let sr = &*scan_result;
    if sr.ssid_def.ssid_length == 0 || sr.ssid_def.ssid[0] == 0 {
        // Silently ignore scan results for hidden SSIDs since they're of no use to us
        return;
    }
    let ssid = match str::from_utf8(slice::from_raw_parts(&sr.ssid_def.ssid as *const u8, sr.ssid_def.ssid_length as usize)) {
        Ok(s) => s,
        _ => "",
    };
    // Debug print the SSID result
    let dbm = 32768 - ((sr.rcpi - 220) / 2);
    /*
    let channel = core::ptr::addr_of!(sr.channel).read_unaligned();
    log!(LL::Debug, "ssid {:X} -{}", channel, dbm);
    for i in sr.mac.iter() {
        loghex!(LL::Debug, " ", *i);
    }
    logln!(LL::Debug, " {}", ssid);*/
    // Update the scan result log
    if SSID_INDEX >= SSID_ARRAY_SIZE {
        SSID_INDEX = 0;
    }
    let _mac = sr.mac;
    let dbm = dbm;
    SSID_BEST_RSSI = match SSID_BEST_RSSI {
        Some(best) if (dbm as u8) < best => Some(dbm as u8),
        Some(best) => Some(best),
        _ => Some(dbm as u8),
    };
    let _chan = sr.channel as u8;
    for (dst_ssid, &src_ssid) in SSID_ARRAY[SSID_INDEX][2..]
        .iter_mut()
        .zip(ssid.as_bytes().iter())
    {
        *dst_ssid = src_ssid
    }
    SSID_ARRAY[SSID_INDEX][1] = sr.ssid_def.ssid_length as u8;
    SSID_ARRAY[SSID_INDEX][0] = if dbm < 256 { dbm as u8 } else { 255 };
    // This is like `n = (n+1) % m`, but % is slow on the EC's minimal RV32I core
    SSID_INDEX += 1;
    if SSID_INDEX >= SSID_ARRAY_SIZE {
        SSID_INDEX = 0;
    }
    SSID_SCAN_UPDATE = true;
}

pub fn wfx_start_scan() -> sl_status_t {
    let result: sl_status_t;
    unsafe {
        SSID_INDEX = 0;
        for ssid in SSID_ARRAY.iter_mut() {
            ssid[0] = 0; // set the length field on each entry to 0 as a proxy for clearing the array
        }
        result = sl_wfx_send_scan_command(
            sl_wfx_scan_mode_e_WFM_SCAN_MODE_ACTIVE as u16,
            0 as *const u8,
            0,
            0 as *const sl_wfx_ssid_def_t,
            0,
            0 as *const u8,
            0,
            0 as *const u8,
        );
    }
    result
}

fn sl_wfx_scan_complete_callback(_status: u32) {
    logln!(LL::Debug, "scan complete");
    unsafe {
        SSID_SCAN_FINISHED = true;
    }
}

pub fn poll_scan_updated() -> bool {
    unsafe {
        let ret = SSID_SCAN_UPDATE;
        SSID_SCAN_UPDATE = false;
        ret
    }
}
pub fn poll_scan_finished() -> bool {
    unsafe {
        let ret = SSID_SCAN_FINISHED;
        SSID_SCAN_FINISHED = false;
        ret
    }
}

pub fn wfx_handle_event() -> sl_status_t {
    let mut control_register: u16 = 0;
    let result: sl_status_t;
    unsafe {
        result = sl_wfx_receive_frame(&mut control_register);
    }
    result
}

/// Handle frames that may be pending in the WF200's queue.
pub fn wfx_drain_event_queue(limit: usize) {
    let mut result: sl_status_t;
    for _ in 0..limit {
        result = wfx_handle_event();
        if result != SL_STATUS_OK {
            loghexln!(LL::Debug, "DrainEventErr ", result);
            break;
        }
    }
}

#[doc = " @brief Called when a message is received from the WFx chip"]
#[doc = ""]
#[doc = " @param event_payload is a pointer to the data received"]
#[doc = " @returns Returns SL_STATUS_OK if successful, SL_STATUS_FAIL otherwise"]
#[doc = ""]
#[doc = " @note Called by ::sl_wfx_receive_frame function"]
#[export_name = "sl_wfx_host_post_event"]
pub unsafe extern "C" fn sl_wfx_host_post_event(
    event_payload: *mut sl_wfx_generic_message_t,
) -> sl_status_t {
    let msg_type: u32 = (*event_payload).header.id as u32;
    match msg_type {
        sl_wfx_indications_ids_e_SL_WFX_CONNECT_IND_ID => {
            let connect_indication: sl_wfx_connect_ind_t =
                *(event_payload as *const sl_wfx_connect_ind_t);
            sl_wfx_connect_callback(connect_indication.body.mac, connect_indication.body.status);
        }
        sl_wfx_indications_ids_e_SL_WFX_DISCONNECT_IND_ID => {
            let disconnect_indication: sl_wfx_disconnect_ind_t =
                *(event_payload as *const sl_wfx_disconnect_ind_t);
            sl_wfx_disconnect_callback(
                disconnect_indication.body.mac,
                disconnect_indication.body.reason,
            );
        }
        sl_wfx_indications_ids_e_SL_WFX_RECEIVED_IND_ID => {
            let ethernet_frame: *const sl_wfx_received_ind_t =
                event_payload as *const sl_wfx_received_ind_t;
            if (*ethernet_frame).body.frame_type == 0 {
                sl_wfx_host_received_frame_callback(ethernet_frame);
            }
        }
        sl_wfx_indications_ids_e_SL_WFX_SCAN_RESULT_IND_ID => {
            let scan_result: *const sl_wfx_scan_result_ind_t =
                event_payload as *const sl_wfx_scan_result_ind_t;
            sl_wfx_scan_result_callback(&(*scan_result).body);
        }
        sl_wfx_indications_ids_e_SL_WFX_SCAN_COMPLETE_IND_ID => {
            let scan_complete: *const sl_wfx_scan_complete_ind_t =
                event_payload as *const sl_wfx_scan_complete_ind_t;
            sl_wfx_scan_complete_callback((*scan_complete).body.status);
        }
        sl_wfx_general_indications_ids_e_SL_WFX_GENERIC_IND_ID => {
            let generic_ind: *const sl_wfx_generic_ind_t =
                event_payload as *const sl_wfx_generic_ind_t;
            let ind_type = (*generic_ind).body.indication_type;
            loghexln!(LL::Debug, "WfxGeneric ", ind_type);
        }
        sl_wfx_general_indications_ids_e_SL_WFX_EXCEPTION_IND_ID => {
            let exception_ind: *const sl_wfx_exception_ind_t =
                event_payload as *const sl_wfx_exception_ind_t;
            let reason = core::ptr::addr_of!((*exception_ind).body.reason).read_unaligned();
            loghexln!(LL::Warn, "WfxException ", reason);
        }
        sl_wfx_general_indications_ids_e_SL_WFX_ERROR_IND_ID => {
            let firmware_error: *const sl_wfx_error_ind_t =
                event_payload as *const sl_wfx_error_ind_t;
            let error = core::ptr::addr_of!((*firmware_error).body.type_).read_unaligned();
            CURRENT_STATUS = LinkState::WFXError;
            CONNECT_RESULT = ConnectResult::Error; // this will also trip an interrupt to inform the host that there's an error
            // SL_WFX_HIF_BUS_ERROR means something got messed up on the SPI bus between the UP5K and the
            // WF200. The one instance I've seen of that happened because of using some weird pointer casting stuff on a
            // the control register argument to wf_receive_frame(). Using `let cr: u16 = 0; wfx_receive_frame(&mut cr);`
            // fixed the problem.
            log!(LL::Debug, "WfxError: ");
            match error {
                SL_WFX_HIF_BUS_ERROR => {
                    use core::convert::TryInto;
                    let maybe_err = u32::from_be_bytes(core::ptr::addr_of!((*firmware_error).body.data).read_unaligned().as_slice(4).try_into().unwrap());
                    loghex!(LL::Debug, "WfxHifBusErr: ", maybe_err);
                    let mut config: u32 = 0;
                    let config_ptr = &mut config as *mut u32;
                    sl_wfx_reg_read_32(
                        sl_wfx_register_address_t_SL_WFX_CONFIG_REG_ID,
                        config_ptr);
                    loghexln!(LL::Debug, " WfxConfig: ", config);
                },
                _ => loghexln!(LL::Debug, "", error),
            }
            WFX_ERR_PENDING = true;
            /*
            let mut cr: u16 = 0;
            let s = sl_wfx_receive_frame(&mut cr);
            loghexln!(LL::Debug, "Kick s: ", s);
            loghexln!(LL::Debug, "Kick cr: ", cr);
            */
        }
        sl_wfx_general_indications_ids_e_SL_WFX_STARTUP_IND_ID => {
            logln!(LL::Debug, "WfxStartup");
            CURRENT_STATUS = LinkState::Disconnected;
        }
        sl_wfx_general_confirmations_ids_e_SL_WFX_CONFIGURATION_CNF_ID => {
            // this occurs during configuration, and is handled specially
        }
        sl_wfx_confirmations_ids_e_SL_WFX_START_SCAN_CNF_ID => {
            logln!(LL::Debug, "WfxStartScan");
        }
        sl_wfx_confirmations_ids_e_SL_WFX_STOP_SCAN_CNF_ID => {
            logln!(LL::Debug, "WfxStopScan");
        }
        sl_wfx_confirmations_ids_e_SL_WFX_CONNECT_CNF_ID => {
            logln!(LL::Debug, "WfxConnCnf");
            let ext_auth: *const sl_wfx_ext_auth_req_t =
                event_payload as *const sl_wfx_ext_auth_req_t;
                loghexln!(LL::Debug, "ExtAuthType ", (*ext_auth).body.auth_data_type);
                loghexln!(LL::Debug, "ExtAuthLen ", (*ext_auth).body.auth_data_length);
                let data = (*ext_auth).body.auth_data.as_slice((*ext_auth).body.auth_data_length as usize);
                for &d in data {
                    loghex!(LL::Debug, "", d);
                }
                logln!(LL::Debug, "");
            }
        sl_wfx_confirmations_ids_e_SL_WFX_DISCONNECT_CNF_ID => {
            logln!(LL::Debug, "WfxDisconCnf");
        }
        sl_wfx_requests_ids_e_SL_WFX_GET_SIGNAL_STRENGTH_REQ_ID => {
            logln!(LL::Debug, "WfxGetSigStr");
        }
        sl_wfx_confirmations_ids_e_SL_WFX_SET_ARP_IP_ADDRESS_CNF_ID => {
            // This happens when you set an IP address for ARP offloading
        }
        sl_wfx_confirmations_ids_e_SL_WFX_SEND_FRAME_CNF_ID => {
            // This happens when a frame gets sent.
            // TODO: maybe increment a counter of packets sent?
        }
        0 => {
            // Whatever... I guess this is fine?
            // Seems like this branch gets hit with a `0` value if there are no events pending
            // That happens a lot if the control loop polls, so ignore this
        }
        _ => {
            loghexln!(LL::Warn, "WfxUnhandled ", msg_type);
        }
    }

    if HOST_CONTEXT.waited_event_id == (*event_payload).header.id {
        if (*event_payload).header.length < 512usize as u16 {
            for i in 0..(*event_payload).header.length {
                WIFI_CONTEXT.event_payload_buffer[i as usize] =
                    (event_payload as *const u8).add(i as usize).read();
            }
            HOST_CONTEXT.posted_event_id = (*event_payload).header.id;
        }
    }
    SL_STATUS_OK
}

/// Return current WF200 power and connection status
pub fn get_status() -> LinkState {
    unsafe { CURRENT_STATUS }
}
