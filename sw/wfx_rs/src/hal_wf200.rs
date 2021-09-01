#![allow(nonstandard_style)]

use crate::betrusted_hal::hal_time::delay_ms;
use crate::betrusted_hal::hal_time::get_time_ms;
use crate::wfx_bindings;
use core::slice;
use core::str;
use utralib::generated::{utra, CSR, HW_WIFI_BASE};

mod bt_wf200_pds;

use bt_wf200_pds::PDS_DATA;
use debug;
use debug::{log, logln, sprint, sprintln, LL};

// The mixed case constants here are the reason for the `allow(nonstandard_style)` above
pub use wfx_bindings::{
    sl_status_t, sl_wfx_buffer_type_t, sl_wfx_confirmations_ids_e_SL_WFX_CONNECT_CNF_ID,
    sl_wfx_confirmations_ids_e_SL_WFX_DISCONNECT_CNF_ID,
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
    sl_wfx_generic_message_t, sl_wfx_host_bus_transfer_type_t,
    sl_wfx_host_bus_transfer_type_t_SL_WFX_BUS_READ, sl_wfx_indication_data_u,
    sl_wfx_indications_ids_e_SL_WFX_CONNECT_IND_ID,
    sl_wfx_indications_ids_e_SL_WFX_DISCONNECT_IND_ID,
    sl_wfx_indications_ids_e_SL_WFX_RECEIVED_IND_ID,
    sl_wfx_indications_ids_e_SL_WFX_SCAN_COMPLETE_IND_ID,
    sl_wfx_indications_ids_e_SL_WFX_SCAN_RESULT_IND_ID, sl_wfx_init, sl_wfx_mac_address_t,
    sl_wfx_pm_mode_e_WFM_PM_MODE_ACTIVE, sl_wfx_pm_mode_e_WFM_PM_MODE_PS, sl_wfx_receive_frame,
    sl_wfx_received_ind_body_s, sl_wfx_received_ind_t, sl_wfx_register_address_t,
    sl_wfx_rx_stats_s, sl_wfx_scan_complete_ind_t, sl_wfx_scan_mode_e_WFM_SCAN_MODE_ACTIVE,
    sl_wfx_scan_result_ind_body_t, sl_wfx_scan_result_ind_t, sl_wfx_send_configuration,
    sl_wfx_send_scan_command, sl_wfx_set_power_mode, sl_wfx_ssid_def_t,
    sl_wfx_state_t_SL_WFX_STA_INTERFACE_CONNECTED, u_int32_t, SL_STATUS_ALLOCATION_FAILED,
    SL_STATUS_IO_TIMEOUT, SL_STATUS_OK, SL_STATUS_WIFI_SLEEP_GRANTED, SL_WFX_CONT_NEXT_LEN_MASK,
    SL_WFX_EXCEPTION_DATA_SIZE_MAX,
};

// ==========================================================
// ===== Configure Log Level (used in macro expansions) =====
// ==========================================================
#[allow(dead_code)]
const LOG_LEVEL: LL = LL::Debug;
// ==========================================================

// This is defined in wfx-fullMAC-driver/wfx_fmac_driver/firmware/sl_wfx_general_error_api.h in the enum
// typedef for sl_wfx_error_t. For some reason that I don't care to hunt down at the moment, this is not
// included in wfx_bindings. Whatever. Here it is:
const SL_WFX_HIF_BUS_ERROR: u32 = 0xf;

pub const WIFI_EVENT_WIRQ: u32 = 0x1;

// locate firmware at SPI top minus 400kiB. Datasheet says reserve at least 350kiB for firmwares.
pub const WFX_FIRMWARE_OFFSET: usize = 0x2000_0000 + 1024 * 1024 - 400 * 1024; // 0x2009_C000

//pub const WFX_FIRMWARE_SIZE: usize = 290896; // version C0, as burned to ROM v3.3.2
pub const WFX_FIRMWARE_SIZE: usize = 305232; // version C0, as burned to ROM v3.12.1. Also applicable for v3.12.3.

/// make a very shitty, temporary malloc that can hold up to 16 entries in the 32k space
/// this is all to avoid including the "alloc" crate, which is "nightly" and not "stable"
// reserve top 32kiB for WFX FFI RAM buffers
pub const WFX_RAM_LENGTH: usize = 32 * 1024;
pub const WFX_RAM_OFFSET: usize = 0x1000_0000 + 128 * 1024 - WFX_RAM_LENGTH; // 1001_8000
static mut WFX_RAM_ALLOC: usize = WFX_RAM_OFFSET;
pub const WFX_MAX_PTRS: usize = 8;
static mut WFX_PTR_COUNT: u8 = 0;
static mut WFX_PTR_LIST: [usize; WFX_MAX_PTRS] = [0; WFX_MAX_PTRS];

static mut SSID_SCAN_IN_PROGRESS: bool = false;

#[derive(Copy, Clone)]
pub enum State {
    Unknown,
    ResetHold,
    Uninitialized,
    Initializing,
    Disconnected,
    Connecting,
    Connected,
    WFXError,
}

static mut CURRENT_STATUS: State = State::Unknown;

#[derive(Copy, Clone)]
pub struct SsidResult {
    pub mac: [u8; 6],
    pub ssid: [u8; 32],
    pub rssi: u16,
    pub channel: u8,
}

impl Default for SsidResult {
    fn default() -> SsidResult {
        SsidResult {
            mac: [0; 6],
            ssid: ['.' as u8; 32],
            rssi: 0,
            channel: 0,
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
    if unsafe { sl_wfx_send_configuration(pds_data, length as u_int32_t) } == SL_STATUS_OK {
        true
    } else {
        false
    }
}

// can't use initializer because calls in statics aren't allowed. :-/ that was a waste of time
pub const SSID_ARRAY_SIZE: usize = 6;
static mut SSID_ARRAY: [SsidResult; SSID_ARRAY_SIZE] = [
    SsidResult {
        mac: [0; 6],
        ssid: [0; 32],
        rssi: 0,
        channel: 0,
    },
    SsidResult {
        mac: [0; 6],
        ssid: [0; 32],
        rssi: 0,
        channel: 0,
    },
    SsidResult {
        mac: [0; 6],
        ssid: [0; 32],
        rssi: 0,
        channel: 0,
    },
    SsidResult {
        mac: [0; 6],
        ssid: [0; 32],
        rssi: 0,
        channel: 0,
    },
    SsidResult {
        mac: [0; 6],
        ssid: [0; 32],
        rssi: 0,
        channel: 0,
    },
    SsidResult {
        mac: [0; 6],
        ssid: [0; 32],
        rssi: 0,
        channel: 0,
    },
];
static mut SSID_INDEX: usize = 0;

pub fn wf200_ssid_get_list(ssid_list: &mut [[u8; 32]; SSID_ARRAY_SIZE]) {
    unsafe {
        for i in 0..SSID_ARRAY_SIZE {
            for j in 0..32 {
                ssid_list[i][j] = SSID_ARRAY[i].ssid[j];
            }
        }
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
        CURRENT_STATUS = State::Initializing;
    }
    unsafe { sl_wfx_init(&mut WIFI_CONTEXT) } // use this to drive porting of the wfx library
}

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
    CURRENT_STATUS = State::Uninitialized;
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
    CURRENT_STATUS = State::Uninitialized;
    SSID_SCAN_IN_PROGRESS = false;
    SL_STATUS_OK
}

#[export_name = "sl_wfx_host_hold_in_reset"]
pub unsafe extern "C" fn sl_wfx_host_hold_in_reset() -> sl_status_t {
    let mut wifi_csr = CSR::new(HW_WIFI_BASE as *mut u32);
    wifi_csr.wfo(utra::wifi::WIFI_RESET, 1);
    // Allow a little time for reset signal to take effect before returning
    delay_ms(1);
    CURRENT_STATUS = State::ResetHold;
    SSID_SCAN_IN_PROGRESS = false;
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

    {
        let mut header_len_mtu = header_length / 2; // we do "MTU" in case header_len is odd. should never be but...this is their API
        let mut header_pos: usize = 0;
        let headeru16: *mut u16 = header as *mut u16;
        while header_len_mtu > 0 {
            //let word: u16 = ((header.add(header_pos).read() as u16) << 8) | (header.add(header_pos + 1).read() as u16);
            let word: u16 = headeru16.add(header_pos).read();
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
                bufferu16.add(buffer_pos).write(word);
                //buffer.add(buffer_pos).write((word >> 8) as u8);
                //buffer.add(buffer_pos+1).write((word & 0xff) as u8);
                buffer_len_mtu -= 1;
                buffer_pos += 1;
            }
        } else {
            // transmit the buffer
            let buffer_len_mtu: usize = buffer_length as usize / 2;
            let mut buffer_pos: usize = 0;
            let bufferu16: *mut u16 = buffer as *mut u16;
            while buffer_pos < buffer_len_mtu {
                //let word: u16 = ((buffer.add(buffer_pos).read() as u16) << 8) | (buffer.add(buffer_pos+1).read() as u16);
                let word: u16 = bufferu16.add(buffer_pos).read();
                wifi_csr.wo(utra::wifi::TX, word as u32);
                //                buffer_len_mtu -= 1;
                buffer_pos += 1;

                wifi_csr.wfo(utra::wifi::CONTROL_GO, 1);
                while wifi_csr.rf(utra::wifi::STATUS_TIP) == 1 {}
                wifi_csr.wfo(utra::wifi::CONTROL_GO, 0);
            }
        }
    }
    SL_STATUS_OK
}

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
    _buffer_size: u32,
) -> sl_status_t {
    // DANGER! DANGER! This code appears to work, but it does not check the buffer size argument!
    // TODO: Check the requested buffer size argument

    // find the first "0" entry in the pointer list
    let mut i = 0;
    while (WFX_PTR_LIST[i] != 0) && (i < WFX_MAX_PTRS as usize) {
        i += 1;
    }
    if i == WFX_MAX_PTRS {
        return SL_STATUS_ALLOCATION_FAILED;
    }
    WFX_PTR_LIST[i] = WFX_RAM_ALLOC + i * (WFX_RAM_LENGTH / WFX_MAX_PTRS);
    *buffer = WFX_PTR_LIST[i] as *mut c_types::c_void;
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
        return SL_STATUS_ALLOCATION_FAILED;
    }
    WFX_PTR_LIST[i] = 0;
    SL_STATUS_OK
}

/// clear the shitty allocator list if we're re-initializing the driver
/// also clear all the static muts (e.g. "C globals") that the driver depends upon
#[export_name = "sl_wfx_host_init"]
pub unsafe extern "C" fn sl_wfx_host_init() -> sl_status_t {
    WFX_RAM_ALLOC = WFX_RAM_OFFSET;
    WFX_PTR_COUNT = 0;
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
    WFX_PTR_COUNT = 0;
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
    log!(LL::Debug, "ConnectCallback");
    let mut new_status = State::Disconnected;
    match status {
        sl_wfx_fmac_status_e_WFM_STATUS_SUCCESS => {
            logln!(LL::Debug, "WFM_STATUS_SUCCESS");
            new_status = State::Connected;
            unsafe {
                WIFI_CONTEXT.state |= sl_wfx_state_t_SL_WFX_STA_INTERFACE_CONNECTED;
                // TODO: callback to lwip_set_sta_link_up -- setup the IP link
                //sl_wfx_set_power_mode(sl_wfx_pm_mode_e_WFM_PM_MODE_PS, 0);
                //sl_wfx_enable_device_power_save();
            }
        }
        sl_wfx_fmac_status_e_WFM_STATUS_NO_MATCHING_AP => {
            logln!(LL::Debug, "NoMatchingAP");
        }
        sl_wfx_fmac_status_e_WFM_STATUS_CONNECTION_ABORTED => {
            logln!(LL::Debug, "ConnectAborted");
        }
        sl_wfx_fmac_status_e_WFM_STATUS_CONNECTION_TIMEOUT => {
            logln!(LL::Debug, "ConnectTimeout");
        }
        sl_wfx_fmac_status_e_WFM_STATUS_CONNECTION_REJECTED_BY_AP => {
            logln!(LL::Debug, "ConnectRejected");
        }
        sl_wfx_fmac_status_e_WFM_STATUS_CONNECTION_AUTH_FAILURE => {
            logln!(LL::Debug, "AuthFailure");
        }
        _ => {
            logln!(LL::Debug, "Error {:X}", status);
        }
    }
    unsafe {
        CURRENT_STATUS = new_status;
    }
}

fn sl_wfx_disconnect_callback(_mac: [u8; 6usize], _reason: u16) {
    sprintln!("Disconnected");
    unsafe {
        CURRENT_STATUS = State::Disconnected;
        WIFI_CONTEXT.state &= !sl_wfx_state_t_SL_WFX_STA_INTERFACE_CONNECTED;
    }
    // TODO: callback to lwip_set_sta_link_down -- teardown the IP link
}

// Expected Ethernet frame header sizes
const MAC_HEADER_LEN: usize = 14;
const ARP_FRAME_LEN: usize = MAC_HEADER_LEN + 28;
const IPV4_FRAME_LEN: usize = MAC_HEADER_LEN + 20;
// Ethertypes for Ethernet MAC header
const ETHERTYPE_IPV4: &[u8] = &[0x08, 0x00];
const ETHERTYPE_ARP: &[u8] = &[0x08, 0x06];

// TODO: Expand on this with something to make an ARP request (intent: trigger ARP reply to this MAC)
fn set_ethernet_mac_header(dest_mac: &[u8; 6], frame: &mut [u8]) -> Result<(), ()> {
    if frame.len() < MAC_HEADER_LEN {
        return Err(());
    }
    let dest_mac_it = dest_mac.iter();
    // sl_wfx_context_t.mac_addr_0 is the STA MAC address for the WFx station interface
    // See https://docs.silabs.com/wifi/wf200/rtos/latest/structsl-wfx-context-t
    let src_mac = unsafe { WIFI_CONTEXT.mac_addr_0.octet as [u8; 6] };
    let src_mac_it = src_mac.iter();
    let ethertype_it = ETHERTYPE_ARP.iter();
    let mac_header_it = dest_mac_it.chain(src_mac_it).chain(ethertype_it);
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

fn handle_ipv4_frame(data: &[u8]) {
    if data.len() < IPV4_FRAME_LEN {
        // Drop frames that are too short to hold an IPV4 header
        return;
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
        return;
    }
    const IGNORE_DF_MASK: u8 = 0b101_11111;
    if (ip_flags_frag[0] & IGNORE_DF_MASK != 0) || (ip_flags_frag[1] != 0) {
        // Drop frames that are part of a fragmented IP packet
        return;
    }
    const VERSION_MASK: u8 = 0xF0;
    if ip_ver_ihl[0] & VERSION_MASK != 0x40 {
        // Drop frames with IP version field not equal to 4
        return;
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
}

/// Handle received Ethernet frame of type ARP (0x0806)
///
/// |-------- Ethernet MAC Header --------|----------------------------- ARP --------------------------------------|
/// | DEST_MAC     SRC_MAC      ETHERTYPE | HTYPE PTYPE HLEN PLEN OPER SHA          SPA      THA          TPA      |
/// | FFFFFFFFFFFF ------------ 0806      | 0001  0800  06   04   0001 ------------ 0A000101 000000000000 0A000102 |
/// | ------------ ------------ 0806      | 0001  0800  06   04   0002 ------------ 0A000102 ------------ 0A000101 |
///
fn handle_arp_frame(data: &[u8]) {
    if data.len() < ARP_FRAME_LEN {
        // Drop malformed (too short) ARP packet
        return;
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
        return;
    }
    let arp_oper = &data[20..22];
    let arp_sha = &data[22..28];
    let arp_spa = &data[28..32];
    let arp_tha = &data[32..38];
    let arp_tpa = &data[38..42];
    if arp_oper == &[0, 1] {
        // ARP Request
        log!(LL::Debug, "who has ");
        log_hex(arp_tpa);
        log!(LL::Debug, "tell ");
        log_hex(arp_sha);
        log_hex(arp_spa);
    } else if arp_oper == &[0, 2] {
        // ARP Reply
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
    const MAC_HEADER_LEN: usize = 14;
    if length < MAC_HEADER_LEN {
        // Drop frames that are too short to contain an Ethernet MAC header
        return;
    }
    const MAC_MULTICAST: &[u8] = &[0x01, 0x00, 0x5E, 0x00, 0x00, 0xFB]; // Frequently seen for mDNS
    let dest_mac = &data[..6];
    if dest_mac == MAC_MULTICAST {
        // Drop mDNS
        return;
    }
    let ethertype = &data[12..14]; // ipv4=0x0800, ipv6=0x86DD, arp=0x0806
    match ethertype {
        ETHERTYPE_IPV4 => handle_ipv4_frame(data),
        ETHERTYPE_ARP => handle_arp_frame(data),
        _ => { /* Drop IPv6 and all the rest */ }
    };
}

unsafe fn sl_wfx_scan_result_callback(scan_result: *const sl_wfx_scan_result_ind_body_t) {
    let sr = &*scan_result;
    if sr.ssid_def.ssid_length == 0 || sr.ssid_def.ssid[0] == 0 {
        // Silently ignore scan results for hidden SSIDs since they're of no use to us
        return;
    }
    let ssid = match str::from_utf8(slice::from_raw_parts(&sr.ssid_def.ssid as *const u8, 32)) {
        Ok(s) => s,
        _ => "",
    };
    if true {
        // Debug print the SSID result
        let channel = core::ptr::addr_of!(sr.channel).read_unaligned();
        let dbm = 32768 - ((sr.rcpi - 220) / 2);
        sprint!("ssid {:2} -{} {:02X}", channel, dbm, sr.mac[0],);
        for i in 1..=5 {
            sprint!(":{:02X}", sr.mac[i]);
        }
        sprintln!(" {}", ssid);
    }
    if SSID_INDEX >= SSID_ARRAY_SIZE {
        SSID_INDEX = 0;
    }
    SSID_ARRAY[SSID_INDEX] = SsidResult {
        mac: [
            sr.mac[0], sr.mac[1], sr.mac[2], sr.mac[3], sr.mac[4], sr.mac[5],
        ],
        rssi: sr.rcpi,
        channel: sr.channel as u8,
        ssid: [0; 32],
    };
    for i in 0..32 {
        // Filter nulls to '.' to bypass `ssid scan` shellchat command's broken filter
        SSID_ARRAY[SSID_INDEX].ssid[i] = match sr.ssid_def.ssid[i] {
            0 => '.' as u8,
            c => c,
        };
    }
    // This is like `n = (n+1) % m`, but % is slow on the EC's minimal RV32I core
    SSID_INDEX += 1;
    if SSID_INDEX >= SSID_ARRAY_SIZE {
        SSID_INDEX = 0;
    }
}

pub fn wfx_start_scan() -> sl_status_t {
    let result: sl_status_t;
    unsafe {
        SSID_SCAN_IN_PROGRESS = true;
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
    sprintln!("scan complete");
    unsafe {
        SSID_SCAN_IN_PROGRESS = false;
    }
}

pub fn wfx_ssid_scan_in_progress() -> bool {
    unsafe { SSID_SCAN_IN_PROGRESS }
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
            sprintln!("WFX_GENERIC_IND {:X}", ind_type);
        }
        sl_wfx_general_indications_ids_e_SL_WFX_EXCEPTION_IND_ID => {
            let exception_ind: *const sl_wfx_exception_ind_t =
                event_payload as *const sl_wfx_exception_ind_t;
            sprintln!("WFX_EXCEPTION_IND:");
            for i in 0..SL_WFX_EXCEPTION_DATA_SIZE_MAX {
                sprint!(
                    "{:02X} ",
                    (*exception_ind)
                        .body
                        .data
                        .as_slice(SL_WFX_EXCEPTION_DATA_SIZE_MAX as usize)[i as usize]
                );
            }
        }
        sl_wfx_general_indications_ids_e_SL_WFX_ERROR_IND_ID => {
            let firmware_error: *const sl_wfx_error_ind_t =
                event_payload as *const sl_wfx_error_ind_t;
            let error = core::ptr::addr_of!((*firmware_error).body.type_).read_unaligned();
            CURRENT_STATUS = State::WFXError;
            // SL_WFX_HIF_BUS_ERROR means something got messed up on the SPI bus between the UP5K and the
            // WF200. The one instance I've seen of that happened because of using some weird pointer casting stuff on a
            // the control register argument to wf_receive_frame(). Using `let cr: u16 = 0; wfx_receive_frame(&mut cr);`
            // fixed the problem.
            sprint!("WFX_ERROR_IND: ");
            match error {
                SL_WFX_HIF_BUS_ERROR => sprintln!("WFX_HIF_BUS_ERROR"),
                _ => sprintln!("{:X}", error),
            }
        }
        sl_wfx_general_indications_ids_e_SL_WFX_STARTUP_IND_ID => {
            sprintln!("WFX_STARTUP");
            CURRENT_STATUS = State::Disconnected;
        }
        sl_wfx_general_confirmations_ids_e_SL_WFX_CONFIGURATION_CNF_ID => {
            // this occurs during configuration, and is handled specially
        }
        sl_wfx_confirmations_ids_e_SL_WFX_START_SCAN_CNF_ID => {
            sprintln!("WFX_START_SCAN");
        }
        sl_wfx_confirmations_ids_e_SL_WFX_STOP_SCAN_CNF_ID => {
            sprintln!("WFX_STOP_SCAN");
        }
        sl_wfx_confirmations_ids_e_SL_WFX_CONNECT_CNF_ID => {
            logln!(LL::Debug, "WFX_CONNECT_CNF");
        }
        sl_wfx_confirmations_ids_e_SL_WFX_DISCONNECT_CNF_ID => {
            logln!(LL::Debug, "WFX_DISCONNECT_CNF");
        }
        0 => {
            // Whatever... I guess this is fine?
            // Seems like this branch gets hit with a `0` value if there are no events pending
            // That happens a lot if the control loop polls, so ignore this
        }
        _ => {
            sprintln!("WFX Unhandled Event: {:X}", msg_type);
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
pub fn get_status() -> State {
    unsafe { CURRENT_STATUS }
}
