use betrusted_hal::hal_time::{delay_ms, get_time_ms, set_msleep_target_ticks, time_init};
use utralib::generated::{
    utra, CSR, HW_COM_BASE, HW_CRG_BASE, HW_GIT_BASE, HW_POWER_BASE, HW_SPIFLASH_MEM,
    HW_TICKTIMER_BASE, HW_WIFI_BASE,
};
use wfx_bindings::SL_STATUS_OK;
use wfx_rs::hal_wf200::{
    wf200_fw_build, wf200_fw_major, wf200_fw_minor, wf200_get_rx_stats_raw, wf200_mutex_get,
    wf200_send_pds, wf200_ssid_get_list, wf200_ssid_updated, wfx_handle_event, wfx_init,
    wfx_scan_ongoing, wfx_start_scan,
};


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

pub fn wf200_irq_disable() {
    let mut wifi_csr = CSR::new(HW_WIFI_BASE as *mut u32);
    wifi_csr.wfo(utra::wifi::EV_ENABLE_WIRQ, 0);
}

pub fn wf200_irq_enable() {
    let mut wifi_csr = CSR::new(HW_WIFI_BASE as *mut u32);
    wifi_csr.wfo(utra::wifi::EV_ENABLE_WIRQ, 1);
}
