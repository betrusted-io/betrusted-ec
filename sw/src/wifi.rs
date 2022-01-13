use core::fmt::Write;

use crate::str_buf::StrBuf;
use crate::wlan::WlanState;
use debug::{loghexln, logln, LL};
use wfx_bindings::{
    sl_status_t, sl_wfx_host_hold_in_reset, sl_wfx_host_reset_chip,
    sl_wfx_security_mode_e_WFM_SECURITY_MODE_WPA2_PSK, sl_wfx_send_disconnect_command,
    sl_wfx_send_join_command, SL_STATUS_OK,
};
use wfx_rs::hal_wf200;
use wfx_rs::hal_wf200::{
    wf200_fw_build, wf200_fw_major, wf200_fw_minor, wf200_send_pds, wf200_ssid_get_list,
    wfx_drain_event_queue, wfx_handle_event, wfx_init, wfx_ssid_scan_in_progress, wfx_start_scan,
};

// Configure Log Level (used in macro expansions)
const LOG_LEVEL: LL = LL::Debug;

pub const SSID_ARRAY_SIZE: usize = wfx_rs::hal_wf200::SSID_ARRAY_SIZE;

/// Connect to an access point using WPA2 with SSID and password.
/// References:
/// - Silicon Laboratories API docs for sl_wfx_send_join_command():
///   docs.silabs.com/wifi/wf200/rtos/latest/group-f-u-l-l-m-a-c-d-r-i-v-e-r-a-p-i#ga2fd76ed31e48be10ab6b7fb9d4bc454d
/// - Rust FFI bindings for sl_wfx API: ../wfx_bindings/src/lib.rs
/// - Protected management frame explanation: en.wikipedia.org/wiki/IEEE_802.11w-2009
///
pub fn ap_join_wpa2(ws: &WlanState) {
    let prevent_roaming: u8 = 0;
    let management_frame_protection: u16 = 1;
    let ie_data: *const u8 = core::ptr::null();
    let ie_data_length: u16 = 0;
    let ssid = match ws.ssid() {
        Ok(s) => s,
        #[allow(unused_variables)]
        Err(e) => {
            logln!(LL::Debug, "SsidErr {}", e as u8);
            &""
        }
    };
    let pass = match ws.pass() {
        Ok(p) => p,
        #[allow(unused_variables)]
        Err(e) => {
            logln!(LL::Debug, "PassErr {}", e as u8);
            &""
        }
    };
    let result: sl_status_t = unsafe {
        sl_wfx_send_join_command(
            ssid.as_ptr(),
            ssid.len() as u32,
            core::ptr::null(),
            0 as u16,
            sl_wfx_security_mode_e_WFM_SECURITY_MODE_WPA2_PSK,
            prevent_roaming,
            management_frame_protection,
            pass.as_ptr(),
            pass.len() as u16,
            ie_data,
            ie_data_length,
        )
    };
    match result {
        SL_STATUS_OK => {
            logln!(LL::Debug, "joinOk");
            dhcp_init();
        }
        _ => logln!(LL::Debug, "joinFail"),
    }
}

/// Initialize DHCP to INIT state (forget bindings, but be ready to DISCOVER on wifi connect)
pub fn dhcp_init() {
    match hal_wf200::dhcp_reset() {
        Ok(_) => (),
        Err(e) => loghexln!(LL::Debug, "DhcpResetErr ", e),
    };
}

/// Clock the DHCP state machine
pub fn dhcp_clock_state_machine() {
    let link = hal_wf200::get_status();
    let dhcp = hal_wf200::dhcp_get_state();
    match link {
        com_rs::LinkState::Connected => match hal_wf200::dhcp_do_next() {
            Ok(_) => (),
            Err(e) => loghexln!(LL::Debug, "DhcpNextErr ", e),
        },
        com_rs::LinkState::Disconnected if dhcp == com_rs::DhcpState::Bound => {
            hal_wf200::dhcp_handle_link_drop();
        }
        _ => (),
    };
}

/// Leave an access point.
/// References:
/// - Silicon Laboratories API docs for sl_wfx_send_disconnect_command():
///   docs.silabs.com/wifi/wf200/rtos/latest/group-f-u-l-l-m-a-c-d-r-i-v-e-r-a-p-i#gae4ae713ea9406b5c18ec278886dcf654
/// - Rust FFI bindings for sl_wfx API: ../wfx_bindings/src/lib.rs
///
pub fn ap_leave() {
    let result: sl_status_t = unsafe { sl_wfx_send_disconnect_command() };
    match result {
        SL_STATUS_OK => logln!(LL::Debug, "leaveOk"),
        _ => logln!(LL::Debug, "leaveFail"),
    }
}

pub fn wf200_reset_momentary() {
    let _ = unsafe { sl_wfx_host_reset_chip() };
}

// TODO: Find a way to turn the WF200 off by using the API... maybe `sl_wfx_host_deinit()`?

/// Turn WF200 off the lazy way by holding reset low (sub-optimal because of pullup current)
pub fn wf200_reset_hold() {
    let _ = unsafe { sl_wfx_host_hold_in_reset() };
}

/// Initialize the WF200, returning true means success
pub fn wf200_init() -> bool {
    match wfx_init() {
        SL_STATUS_OK => true,
        _ => false,
    }
}

/// Shorthand function to encapsulate a sequence used multiple times in main.rs::main()
pub fn wf200_reset_and_init(use_wifi: &mut bool, wifi_ready: &mut bool) {
    *use_wifi = true;
    wf200_reset_momentary();
    *wifi_ready = wf200_init();
    match *wifi_ready {
        true => logln!(LL::Debug, "Wifi ready"),
        false => logln!(LL::Debug, "Wifi init fail"),
    };
}

pub fn wf200_irq_disable() {
    //let mut wifi_csr = CSR::new(HW_WIFI_BASE as *mut u32);
    //wifi_csr.wfo(utra::wifi::EV_ENABLE_WIRQ, 0);
}

pub fn wf200_irq_enable() {
    //let mut wifi_csr = CSR::new(HW_WIFI_BASE as *mut u32);
    //wifi_csr.wfo(utra::wifi::EV_ENABLE_WIRQ, 1);
}

pub fn start_scan() {
    // This call initiates an aysnc scan that takes about 800ms in active scan
    // mode or about 1500ms in passive scan mode. The scan is a one-shot thing
    // that ends automatically. You can start another scan once the first one
    // ends.
    //
    // Precautions (see samblenny/wfx_docs):
    // 1. Scanning seems to work better if, before starting a scan, you drain
    //    the WF200 received frames queue by calling sl_wfx_receive_frame()
    //    until it stops returning SL_STATUS_OK
    // 2. Starting a second scan before getting a SL_WFX_SCAN_COMPLETE_IND_ID
    //    is not good idea.
    //
    // Assuming you set up a control loop task to poll the WF200 WIRQ output
    // and call sl_wfx_receive_frame() when it's asserted, scan results will
    // appear as arguments to sl_wfx_host_post_event():
    // 1. Each new SSID gets posted as an event with event payload header ID of
    //    SL_WFX_SCAN_RESULT_IND_ID
    // 2. Post of SL_WFX_SCAN_COMPLETE_IND_ID event marks end of scan. At that
    //    point, the scan is done and you can start another one if you want.
    //
    let limit = 32;
    wfx_drain_event_queue(limit);
    wfx_start_scan();
}

pub fn ssid_scan_in_progress() -> bool {
    wfx_ssid_scan_in_progress()
}

pub fn ssid_get_list(mut ssid_list: &mut [[u8; 34]; SSID_ARRAY_SIZE]) {
    wf200_ssid_get_list(&mut ssid_list);
}

pub fn fw_build() -> u8 {
    wf200_fw_build()
}

pub fn fw_major() -> u8 {
    wf200_fw_major()
}

pub fn fw_minor() -> u8 {
    wf200_fw_minor()
}

pub fn send_pds(data: [u8; 256], length: u16) -> bool {
    wf200_send_pds(data, length)
}

pub fn handle_event() -> u32 {
    wfx_handle_event()
}

/// Append string describing WF200 power and connection status to u8 buffer iterator
///
/// Format:
///   line1: rssi interface_status dhcp_state
///   line2: ssid
///
/// The rssi value is in dBm from either the last packet recieved (if connected), or
/// from the strongest scan result seen during ssid scan commands.
///
pub fn append_status_str(mut buf: &mut StrBuf<64>, ws: &WlanState) {
    // RSSI
    let rssi_result: Result<u32, u8> = hal_wf200::get_rssi();
    match rssi_result {
        Ok(rssi) => {
            logln!(LL::Debug, "RxRssi -{}", rssi);
            let _ = write!(&mut buf, "-{} ", rssi);
        }
        Err(e) => {
            loghexln!(LL::Debug, "RxRssiErr ", e);
            match hal_wf200::get_best_ssid_scan_rssi() {
                Some(rssi) => {
                    logln!(LL::Debug, "ScanRssi -{}", rssi);
                    let _ = write!(&mut buf, "-{} ", rssi);
                }
                _ => {
                    let _ = write!(&mut buf, "-- ");
                }
            }
        }
    };
    // Interface status; changes mainly in response to wlan {on,off,join,leave}
    let ifce_tag = hal_wf200::interface_status_tag();
    // DHCP sate updates after a `wlan join`
    let dhcp_tag = hal_wf200::dhcp_get_state_tag();
    // SSID updates after a `wlan setssid ...`
    let ssid = match ws.ssid() {
        Ok(ssid) => ssid,
        _ => " ",
    };
    let _ = write!(buf, "{} {} \n{}", ifce_tag, dhcp_tag, ssid);
}
