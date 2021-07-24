use betrusted_hal::hal_time::delay_ms;
use utralib::generated::{utra, CSR, HW_WIFI_BASE};
use wfx_bindings::SL_STATUS_OK;
use wfx_rs::hal_wf200::{
    wf200_fw_build, wf200_fw_major, wf200_fw_minor, wf200_send_pds, wf200_ssid_get_list,
    wfx_drain_event_queue, wfx_handle_event, wfx_init, wfx_ssid_scan_in_progress, wfx_start_scan,
};
use crate::debug::LL;

// ==========================================================
// ===== Configure Log Level (used in macro expansions) =====
// ==========================================================
const LOG_LEVEL: LL = LL::Debug;
// ==========================================================

pub const SSID_ARRAY_SIZE: usize = wfx_rs::hal_wf200::SSID_ARRAY_SIZE;

pub fn wf200_reset_momentary() {
    let mut wifi_csr = CSR::new(HW_WIFI_BASE as *mut u32);
    wifi_csr.rmwf(utra::wifi::WIFI_RESET, 1);
    delay_ms(10);
    wifi_csr.rmwf(utra::wifi::WIFI_RESET, 0);
    delay_ms(10);
}

pub fn wf200_reset_hold() {
    let mut wifi_csr = CSR::new(HW_WIFI_BASE as *mut u32);
    wifi_csr.rmwf(utra::wifi::WIFI_RESET, 1);
}

/// Initialize the WF200, returning of true means success
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

pub fn ssid_get_list(mut ssid_list: &mut [[u8; 32]; SSID_ARRAY_SIZE]) {
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
