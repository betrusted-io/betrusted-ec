#![allow(unused)]
#![allow(nonstandard_style)]

use crate::betrusted_hal::hal_time::delay_ms;
use crate::betrusted_hal::hal_time::get_time_ms;
use crate::wfx_bindings;
use xous_nommu::syscalls::*;
use core::slice;
use core::str;

use utralib::generated::*;

pub const DEBUGGING: bool = false;
pub const DEBUGGING2: bool = false;  // more verbose debugging

#[macro_use]
mod debug;

mod bt_wf200_pds;
use bt_wf200_pds::*;

#[macro_use]
use core::include_bytes;

pub use wfx_bindings::*;

static mut WF200_EVENT: bool = false;
pub const WIFI_EVENT_WIRQ: u32 = 0x1;

// locate firmware at SPI top minus 400kiB. Datasheet says reserve at least 350kiB for firmwares.
pub const WFX_FIRMWARE_OFFSET: usize = 0x2000_0000 + 1024*1024 - 400*1024; // 0x2009_C000
pub const WFX_FIRMWARE_SIZE: usize = 290896; // version C0, as burned to ROM

/// make a very shitty, temporary malloc that can hold up to 16 entries in the 32k space
/// this is all to avoid including the "alloc" crate, which is "nightly" and not "stable"
// reserve top 32kiB for WFX FFI RAM buffers
pub const WFX_RAM_LENGTH: usize = 32*1024;
pub const WFX_RAM_OFFSET: usize = 0x1000_0000 + 128*1024 - WFX_RAM_LENGTH; // 1001_8000
static mut WFX_RAM_ALLOC: usize = WFX_RAM_OFFSET;
pub const WFX_MAX_PTRS: usize = 16;
static mut WFX_PTR_COUNT: u8 = 0;
static mut WFX_PTR_LIST: [usize; WFX_MAX_PTRS] = [0; WFX_MAX_PTRS];

pub fn wf200_event_set() { unsafe{ WF200_EVENT = true; } }
pub fn wf200_event_get() -> bool { unsafe{ WF200_EVENT } }
pub fn wf200_event_clear() { unsafe{ WF200_EVENT = false; } }

/// TODO: totally wrong way to do this, fix later
static mut WF200_MUTEX: bool = false;
pub fn wf200_mutex_get() -> bool { unsafe{ WF200_MUTEX } }
pub fn wf200_mutex_lock() { unsafe{ WF200_MUTEX = true; } }
pub fn wf200_mutex_unlock() { unsafe{ WF200_MUTEX = false; } }

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
            ssid: [0; 32],
            rssi: 0,
            channel: 0,
        }
    }
}

// can't use initializer because calls in statics aren't allowed. :-/ that was a waste of time
static mut SSID_ARRAY: [SsidResult; 6] = [
    SsidResult{mac: [0;6], ssid: [0; 32], rssi: 0, channel: 0},
    SsidResult{mac: [0;6], ssid: [0; 32], rssi: 0, channel: 0},
    SsidResult{mac: [0;6], ssid: [0; 32], rssi: 0, channel: 0},
    SsidResult{mac: [0;6], ssid: [0; 32], rssi: 0, channel: 0},
    SsidResult{mac: [0;6], ssid: [0; 32], rssi: 0, channel: 0},
    SsidResult{mac: [0;6], ssid: [0; 32], rssi: 0, channel: 0},
    ];
static mut SSID_INDEX: usize = 0;
static mut SSID_UPDATED: bool = false;

pub fn wf200_ssid_updated() -> bool {
    unsafe{ SSID_UPDATED }
}

pub fn wf200_ssid_get_list() -> [SsidResult; 6] {
    unsafe{
        SSID_UPDATED = false;
        SSID_ARRAY
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
static mut HOST_CONTEXT: host_context = host_context{ sl_wfx_firmware_download_progress: 0, waited_event_id: 0, posted_event_id: 0 };

pub const MAX_SCAN_RESULTS: usize = 50;

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct scan_result_list_t {
    pub ssid_def: sl_wfx_ssid_def_t,
    pub mac: [u8; 6usize],
    pub channel: u16,
//    pub security_mode: sl_wfx_security_mode_bitmask_t,
    pub rcpi: u16,
}

pub struct scan_data {
    pub scan_list: [scan_result_list_t; MAX_SCAN_RESULTS],
    pub scan_count: u8,
    pub scan_count_web: u8,
    pub scan_ongoing: bool,
}

static mut SCAN_LIST: scan_data = scan_data {
    scan_list: [
        scan_result_list_t {
            ssid_def: sl_wfx_ssid_def_s { ssid_length: 0, ssid: [0; 32usize]},
            mac: [0; 6usize],
            channel: 0,
//            security_mode: sl_wfx_security_mode_bitmask_s { _bitfield_1: sl_wfx_capabilities_s::new_bitfield_1(0,0) },
            rcpi: 0,
        } ; MAX_SCAN_RESULTS ],
    scan_count: 0,
    scan_count_web: 0,
    scan_ongoing: false,
};

static mut SCAN_ONGOING: bool = false;

trait Empty<T> {
    fn empty() -> T;
}
impl Empty<sl_wfx_mac_address_t> for sl_wfx_mac_address_t {
    fn empty() -> sl_wfx_mac_address_t {
        sl_wfx_mac_address_t {
            octet: [0; 6usize],
        }
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
    mac_addr_0: sl_wfx_mac_address_t{ octet: [0; 6usize]},
    mac_addr_1: sl_wfx_mac_address_t{ octet: [0; 6usize]},
    state: 0,
};

pub fn wfx_init() -> sl_status_t {
    unsafe{ sl_wfx_init(&mut WIFI_CONTEXT) }  // use this to drive porting of the wfx library
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
pub unsafe extern "C" fn sl_wfx_host_deinit_bus()-> sl_status_t {
    let mut wifi_csr = CSR::new(HW_WIFI_BASE as *mut u32);
    if DEBUGGING2 { sprintln!("deinit_bus"); }
    wifi_csr.wo(utra::wifi::CONTROL, 0);
    wifi_csr.wo(utra::wifi::WIFI, 0);
    SL_STATUS_OK
}

pub fn wfx_int_handler(_irq_no: usize) {
    let mut wifi_csr = CSR::new(HW_WIFI_BASE as *mut u32);
    let ev: u32 = wifi_csr.r(utra::wifi::EV_PENDING);
    wf200_event_set();
    // clear the interrupt
    wifi_csr.wo(utra::wifi::EV_PENDING, ev);
}
#[export_name = "sl_wfx_host_enable_platform_interrupt"]
pub unsafe extern "C" fn sl_wfx_host_enable_platform_interrupt() -> sl_status_t {
    let mut wifi_csr = CSR::new(HW_WIFI_BASE as *mut u32);
    sys_interrupt_claim(utra::wifi::WIFI_IRQ as usize, wfx_int_handler)
    .unwrap();
    sprintln!("enabling interrupt: mask {} channel {}", WIFI_EVENT_WIRQ, utra::wifi::WIFI_IRQ as u8);
    wifi_csr.wfo(utra::wifi::EV_ENABLE_WIRQ, 1);
    SL_STATUS_OK
}

#[export_name = "sl_wfx_host_disable_platform_interrupt"]
pub unsafe extern "C" fn sl_wfx_host_disable_platform_interrupt() -> sl_status_t {
    let mut wifi_csr = CSR::new(HW_WIFI_BASE as *mut u32);
    wifi_csr.wo(utra::wifi::EV_ENABLE, 0);
    sys_interrupt_free(utra::wifi::WIFI_IRQ as usize);
    SL_STATUS_OK
}

#[export_name = "sl_wfx_host_init_bus"]
pub unsafe extern "C" fn sl_wfx_host_init_bus()-> sl_status_t {
    let mut wifi_csr = CSR::new(HW_WIFI_BASE as *mut u32);
    wifi_csr.wo(utra::wifi::CONTROL, 0);
    wifi_csr.wo(utra::wifi::WIFI, 0);
    if DEBUGGING2 { sprintln!("init_bus"); }
    SL_STATUS_OK
}

#[export_name = "sl_wfx_host_reset_chip"]
pub unsafe extern "C" fn sl_wfx_host_reset_chip() -> sl_status_t {
    let mut wifi_csr = CSR::new(HW_WIFI_BASE as *mut u32);
    if DEBUGGING2 { sprintln!("reset_chip"); }
    wifi_csr.wfo(utra::wifi::WIFI_RESET, 1);
    delay_ms(10);
    wifi_csr.wfo(utra::wifi::WIFI_RESET, 0);
    delay_ms(10);
    SL_STATUS_OK
}

#[export_name = "sl_wfx_host_hold_in_reset"]
pub unsafe extern "C" fn sl_wfx_host_hold_in_reset() -> sl_status_t {
    let mut wifi_csr = CSR::new(HW_WIFI_BASE as *mut u32);
    wifi_csr.wfo(utra::wifi::WIFI_RESET, 1);
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
    wf200_mutex_lock();
    SL_STATUS_OK
}
#[export_name = "sl_wfx_host_unlock"]
pub unsafe extern "C" fn sl_wfx_host_unlock() -> sl_status_t {
    wf200_mutex_unlock();
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

    unsafe {
        let mut header_len_mtu = header_length / 2; // we do "MTU" in case header_len is odd. should never be but...this is their API
        let mut header_pos: usize = 0;
        if DEBUGGING { sprintln!("headerlen: {}", header_length); }
        let headeru16: *mut u16 = header as *mut u16;
        while header_len_mtu > 0 {
            //let word: u16 = ((header.add(header_pos).read() as u16) << 8) | (header.add(header_pos + 1).read() as u16);
            let word: u16 = headeru16.add(header_pos).read();
            wifi_csr.wo(utra::wifi::TX, word as u32);
            if DEBUGGING { sprintln!("header: {:02x} {:02x}", word >> 8, word & 0xff); }
            header_len_mtu -= 1;
            header_pos += 1;

            wifi_csr.wfo(utra::wifi::CONTROL_GO, 1);
            while wifi_csr.rf(utra::wifi::STATUS_TIP) == 1 {}
            wifi_csr.wfo(utra::wifi::CONTROL_GO, 0);
        }
        if type_ == sl_wfx_host_bus_transfer_type_t_SL_WFX_BUS_READ {
            if DEBUGGING { sprintln!("rxlen: {}", buffer_length); }
            let mut buffer_len_mtu = buffer_length / 2;
            let mut buffer_pos: usize = 0;
            let mut bufferu16: *mut u16 = buffer as *mut u16;
            while buffer_len_mtu > 0 {
                // transmit a dummy word to get the rx data
                wifi_csr.wo(utra::wifi::TX, 0);
                wifi_csr.wfo(utra::wifi::CONTROL_GO, 1);
                while wifi_csr.rf(utra::wifi::STATUS_TIP) == 1 {}
                wifi_csr.wfo(utra::wifi::CONTROL_GO, 0);

                let word: u16 = wifi_csr.rf(utra::wifi::RX_RX) as u16;
                if DEBUGGING { sprintln!("rx: {:02x} {:02x}", word >> 8, word & 0xff); }
                bufferu16.add(buffer_pos).write(word);
                //buffer.add(buffer_pos).write((word >> 8) as u8);
                //buffer.add(buffer_pos+1).write((word & 0xff) as u8);
                buffer_len_mtu -= 1;
                buffer_pos += 1;
            }
        } else {
            if DEBUGGING { sprintln!("txlen: {}", buffer_length); }
            // transmit the buffer
            let mut buffer_len_mtu: usize = buffer_length as usize / 2;
            let mut buffer_pos: usize = 0;
            let bufferu16: *mut u16 = buffer as *mut u16;
            while buffer_pos < buffer_len_mtu {
                //let word: u16 = ((buffer.add(buffer_pos).read() as u16) << 8) | (buffer.add(buffer_pos+1).read() as u16);
                let word: u16 = bufferu16.add(buffer_pos).read();
                wifi_csr.wo(utra::wifi::TX, word as u32);
                if DEBUGGING { sprintln!("tx: {:02x} {:02x}", word >> 8, word & 0xff); }
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
    type_: sl_wfx_buffer_type_t,
    buffer_size: u32,
) -> sl_status_t {
    if (WFX_RAM_ALLOC + buffer_size as usize) < (WFX_RAM_LENGTH + WFX_RAM_OFFSET) as usize &&
        WFX_PTR_COUNT < WFX_MAX_PTRS as u8 {
        *buffer = WFX_RAM_ALLOC as *mut c_types::c_void;
        unsafe{ WFX_PTR_LIST[WFX_PTR_COUNT as usize] = WFX_RAM_ALLOC; }

        unsafe{ WFX_PTR_COUNT += 1; }
        unsafe{ WFX_RAM_ALLOC += buffer_size as usize };

        SL_STATUS_OK
    } else {
        SL_STATUS_ALLOCATION_FAILED
    }
}

#[doc = " @brief Called when the driver wants to free memory"]
#[doc = ""]
#[doc = " @param buffer is the pointer to the memory to free"]
#[doc = " @param type is the type of buffer to free (see ::sl_wfx_buffer_type_t)"]
#[doc = " @returns Returns SL_STATUS_OK if successful, SL_STATUS_FAIL otherwise"]
#[export_name = "sl_wfx_host_free_buffer"]
pub unsafe extern "C" fn sl_wfx_host_free_buffer(
    buffer: *mut c_types::c_void,
    type_: sl_wfx_buffer_type_t,
) -> sl_status_t {
    // copy the list of pointers to a temp struct, omitting the one we are looking to free
    // reset the ALLOC pointer to the last element.
    let mut temp_ptr_list: [usize; WFX_MAX_PTRS] = [0; WFX_MAX_PTRS];
    let mut temp_ptr = 0;
    let mut found = false;
    for ptr in 0..WFX_MAX_PTRS {
        if WFX_PTR_LIST[ptr] == buffer as usize {
            found = true;
            if buffer as usize != 0 {
                unsafe{ WFX_PTR_COUNT -= 1; } // decrement the list
            }
            continue; // skip copying
        } else {
            temp_ptr_list[temp_ptr] = WFX_PTR_LIST[ptr];
            temp_ptr += 1;
        }
    }

    // fail if we didn't find anything, or if somehow ptr_count wrapped around
    if found == false || WFX_PTR_COUNT > WFX_MAX_PTRS as u8 {
        SL_STATUS_FAIL
    } else {
        // copy the temp list to the master list
        let mut top_mem: usize = 0;
        for ptr in 0..WFX_MAX_PTRS {
            unsafe{ WFX_PTR_LIST[ptr] = temp_ptr_list[ptr]; }
            if temp_ptr_list[ptr] > top_mem {
                top_mem = temp_ptr_list[ptr];
            }
        }
        // if no entries in list, top_mem is 0 and should be reset to base of RAM
        if top_mem == 0 {
            top_mem = WFX_RAM_OFFSET;
        }
        // sanity check top_mem
        if top_mem < WFX_RAM_OFFSET || top_mem > (WFX_RAM_OFFSET + WFX_RAM_LENGTH) {
            SL_STATUS_FAIL
        } else {
            unsafe{ WFX_RAM_ALLOC = top_mem };
            SL_STATUS_OK
        }
    }
}

/// clear the shitty allocator list if we're re-initializing the driver
/// also clear all the static muts (e.g. "C globals") that the driver depends upon
#[export_name = "sl_wfx_host_init"]
pub unsafe extern "C" fn sl_wfx_host_init() -> sl_status_t {
    unsafe {
        WFX_RAM_ALLOC = WFX_RAM_OFFSET;
        WFX_PTR_COUNT = 0;
        WFX_PTR_LIST = [0; WFX_MAX_PTRS];
    }
    unsafe {
        HOST_CONTEXT.sl_wfx_firmware_download_progress = 0;
//        HOST_CONTEXT.waited_event_id = 0;  // this is apparently side-effected elsewhere
        HOST_CONTEXT.posted_event_id = 0;
    }
    unsafe {
        WF200_EVENT = false;
    }
    unsafe {
        WIFI_CONTEXT = sl_wfx_context_t {
            event_payload_buffer: [0; 512usize],
            firmware_build: 0,
            firmware_minor: 0,
            firmware_major: 0,
            data_frame_id: 0,
            used_buffers: 0,
            wfx_opn: [0; 14usize],
            mac_addr_0: sl_wfx_mac_address_t{ octet: [0; 6usize]},
            mac_addr_1: sl_wfx_mac_address_t{ octet: [0; 6usize]},
            state: 0,
        };
    }
    SL_STATUS_OK
}
#[export_name = "sl_wfx_host_deinit"]
pub unsafe extern "C" fn sl_wfx_host_deinit() -> sl_status_t {
    unsafe {
        WFX_RAM_ALLOC = WFX_RAM_OFFSET;
        WFX_PTR_COUNT = 0;
        WFX_PTR_LIST = [0; WFX_MAX_PTRS];
    }
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
            unsafe{ sl_wfx_receive_frame(&mut control_register); }
            if (control_register & SL_WFX_CONT_NEXT_LEN_MASK as u16) == 0 {
                break;
            }
        }
        if confirmation_id == HOST_CONTEXT.posted_event_id {
            unsafe{ HOST_CONTEXT.posted_event_id = 0; }
            if event_payload_out != (::core::ptr::null::<c_types::c_void> as *mut *mut c_types::c_void) {
                *event_payload_out = WIFI_CONTEXT.event_payload_buffer.as_ptr() as *mut c_types::c_void;
            }
            return SL_STATUS_OK;
        } else {
            if DEBUGGING{ sprintln!("confid: {}", HOST_CONTEXT.posted_event_id); }
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
    unsafe{ HOST_CONTEXT.waited_event_id = event_id; }

    SL_STATUS_OK
}

#[doc = " @brief Called when the driver sends a frame to the WFx chip"]
#[doc = ""]
#[doc = " @param frame is a pointer to the frame data"]
#[doc = " @param frame_len is size of the frame"]
#[doc = " @returns Returns SL_STATUS_OK if successful, SL_STATUS_FAIL otherwise"]
#[export_name = "sl_wfx_host_transmit_frame"]
pub unsafe extern "C" fn sl_wfx_host_transmit_frame(frame: *mut c_types::c_void, frame_len: u32) -> sl_status_t {
    let mut ret: sl_status_t = SL_STATUS_OK;
    if DEBUGGING {
        let u8frame: *const u8 = frame as *const u8;
        sprint!("TX> 0x{:x}: ", frame as u32);
        for i in 0 .. frame_len {
            if i < 4 {
                sprint!("{:02x}", u8frame.add(i as usize).read());
            } else if i >= 4 && i < 6 {
                sprint!(" {:02x} ", u8frame.add(i as usize).read());
            } else {
                if u8frame.add(i as usize).read() != 0 {
                    sprint!("{}", u8frame.add(i as usize).read() as char);
                } else {
                    sprint!("NULL");
                }
            }
        }
        sprintln!("");
    }
    unsafe{ ret = sl_wfx_data_write( frame, frame_len ); }
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
    unsafe{ *firmware_size = WFX_FIRMWARE_SIZE as u32; }
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
pub unsafe extern "C" fn sl_wfx_host_get_firmware_data(data: *mut *const u8, data_size: u32) -> sl_status_t {
    unsafe{
        *data = (WFX_FIRMWARE_OFFSET + HOST_CONTEXT.sl_wfx_firmware_download_progress as usize) as *const u8;
        HOST_CONTEXT.sl_wfx_firmware_download_progress += data_size;
    }

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
    type_: sl_wfx_host_bus_transfer_type_t,
    address: sl_wfx_register_address_t,
    length: u32,
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

#[export_name = "bt_ffi_dbg"]
pub unsafe extern "C" fn bt_ffi_dbg(dbgstr: *const c_types::c_char) {
    let mut length: usize = 0;
    while(dbgstr).add(length).read() != 0 {
        length += 1;
    }
    let s = unsafe{ str::from_utf8(slice::from_raw_parts(dbgstr as *const u8, length)).expect("unable to parse")};
    sprintln!("***dbg: {}", s);
}
#[export_name = "bt_ffi_dbg_u16"]
pub unsafe extern "C" fn bt_ffi_dbg_u16(dbgstr: *const c_types::c_char, val: u16) {
    let mut length: usize = 0;
    while(dbgstr).add(length).read() != 0 {
        length += 1;
    }
    let s = unsafe{ str::from_utf8(slice::from_raw_parts(dbgstr as *const u8, length)).expect("unable to parse")};
    sprintln!("***dbg: {}: 0x{:04x}", s, val);
}
#[export_name = "bt_ffi_dbg_u32"]
pub unsafe extern "C" fn bt_ffi_dbg_u32(dbgstr: *const c_types::c_char, val: u32) {
    let mut length: usize = 0;
    while(dbgstr).add(length).read() != 0 {
        length += 1;
    }
    let s = unsafe{ str::from_utf8(slice::from_raw_parts(dbgstr as *const u8, length)).expect("unable to parse")};
    sprintln!("***dbg: {}: 0x{:08x}", s, val);
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
    while(__nptr).add(length).read() != 0 {
        length += 1;
    }
    let s = unsafe { str::from_utf8(slice::from_raw_parts(__nptr as *const u8, length)).expect("unable to parse string") };
    usize::from_str_radix(s.trim_start_matches("0x"), 16).expect("unable to parse num") as c_types::c_ulong
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
    //let pds = include_bytes!("bt-wf200-pds.in");
    //*pds_data = (&pds).as_ptr().add(0) as *const c_types::c_char;
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

fn sl_wfx_connect_callback(mac: [u8; 6usize], status: u32) {
    match status {
        sl_wfx_fmac_status_e_WFM_STATUS_SUCCESS => {
            sprintln!("Connected");
            unsafe{ WIFI_CONTEXT.state |= sl_wfx_state_t_SL_WFX_STA_INTERFACE_CONNECTED; }
            // TODO: callback to lwip_set_sta_link_up -- setup the IP link
            if unsafe{(WIFI_CONTEXT.state & sl_wfx_state_t_SL_WFX_AP_INTERFACE_UP)} == 0 {
                unsafe { // wrap FFI C calls in unsafe
                    sl_wfx_set_power_mode(sl_wfx_pm_mode_e_WFM_PM_MODE_PS, 0);
                    sl_wfx_enable_device_power_save();
                }
            }
        }
        sl_wfx_fmac_status_e_WFM_STATUS_NO_MATCHING_AP => {
            sprintln!("Connection failed, access point not found.")
        }
        sl_wfx_fmac_status_e_WFM_STATUS_CONNECTION_ABORTED => {
            sprintln!("Connectiona aborted.")
        }
        sl_wfx_fmac_status_e_WFM_STATUS_CONNECTION_TIMEOUT => {
            sprintln!("Connection timeout.")
        }
        sl_wfx_fmac_status_e_WFM_STATUS_CONNECTION_REJECTED_BY_AP => {
            sprintln!("Connection rejected by the access point.")
        }
        sl_wfx_fmac_status_e_WFM_STATUS_CONNECTION_AUTH_FAILURE => {
            sprintln!("Connection authenication failure.")
        }
        _ => {
            sprintln!("Connection attempt error.")
        }
    }
}

fn sl_wfx_disconnect_callback(mac: [u8; 6usize], reason: u16) {
    sprintln!("Disconnected");
    unsafe{ WIFI_CONTEXT.state &= !sl_wfx_state_t_SL_WFX_STA_INTERFACE_CONNECTED; }
    // TODO: callback to lwip_set_sta_link_down -- teardown the IP link
}

fn sl_wfx_start_ap_callback(status: u32) {
    if status == 0 {
        sprintln!("AP started");
        unsafe{ WIFI_CONTEXT.state |= sl_wfx_state_t_SL_WFX_AP_INTERFACE_UP; }
        // TODO: callback to lwip_set_ap_link_up() -- if we are to be an AP!!!
        unsafe { // wrap FFI C calls in unsafe
            sl_wfx_set_power_mode(sl_wfx_pm_mode_e_WFM_PM_MODE_ACTIVE, 0);
            sl_wfx_disable_device_power_save();
        }
    } else {
        sprintln!("AP start failed");
    }
}

fn sl_wfx_stop_ap_callback() {
    // TODO: stop the DHCP server
    sprintln!("SoftAP stopped.");
    unsafe{ WIFI_CONTEXT.state &= !sl_wfx_state_t_SL_WFX_AP_INTERFACE_UP; }
    // TODO: lwip_set_ap_link_down -- bring the AP link down

    if unsafe{ WIFI_CONTEXT.state & sl_wfx_state_t_SL_WFX_STA_INTERFACE_CONNECTED } != 0 {
        unsafe { // wrap FFI C calls in unsafe
            sl_wfx_set_power_mode(sl_wfx_pm_mode_e_WFM_PM_MODE_PS, 0);
            sl_wfx_enable_device_power_save();
        }
    }
}

fn sl_wfx_host_received_frame_callback(rx_buffer: *const sl_wfx_received_ind_t) {
    // TODO: do something with received ethernet frames!
}

fn sl_wfx_scan_result_callback(scan_result: *const sl_wfx_scan_result_ind_body_t) {
    let ssid = unsafe { str::from_utf8(slice::from_raw_parts(&(*scan_result).ssid_def.ssid as *const u8, 32)).expect("unable to parse ssid") };
    unsafe { // because raw pointer dereferences
        sprintln!("scan-- ch:{} str:-{} mac:{:02x}{:02x}{:02x}{:02x}{:02x}{:02x} ssid:{}",
            (*scan_result).channel,
            32768 - (((*scan_result).rcpi - 220) / 2),
            (*scan_result).mac[0], (*scan_result).mac[1],
            (*scan_result).mac[2], (*scan_result).mac[3],
            (*scan_result).mac[4], (*scan_result).mac[5],
            ssid
        );
        if SSID_INDEX < SSID_ARRAY.len() {
            SSID_ARRAY[SSID_INDEX] = SsidResult {
                mac: [(*scan_result).mac[0], (*scan_result).mac[1],
                (*scan_result).mac[2], (*scan_result).mac[3],
                (*scan_result).mac[4], (*scan_result).mac[5]],
                rssi: (*scan_result).rcpi,
                channel: (*scan_result).channel as u8,
                ssid: [0; 32]
            };
            for i in 0..32 {
                SSID_ARRAY[SSID_INDEX].ssid[i] = (*scan_result).ssid_def.ssid[i];
            }
        }
        SSID_INDEX += 1;
    }
}

pub fn wfx_start_scan() {
    unsafe {
        let result = sl_wfx_send_scan_command(sl_wfx_scan_mode_e_WFM_SCAN_MODE_ACTIVE as u16,
            0 as *const u8, 0, 0 as *const sl_wfx_ssid_def_t, 0, 0 as *const u8, 0, 0 as *const u8);

        if result == SL_STATUS_OK || result == SL_STATUS_WIFI_WARNING {
            SCAN_ONGOING = true;
        } else{
            SCAN_ONGOING = false;
        }
    }
}

pub fn wfx_scan_ongoing() -> bool {
    unsafe{ SCAN_ONGOING }
}
fn sl_wfx_scan_start_flag() {
    unsafe{ SCAN_ONGOING = true; }
}
fn sl_wfx_scan_complete_callback(status: u32) {
    sprintln!("scan completed");
    unsafe{ SSID_UPDATED = true; }
    // nothing for now
    unsafe{ SCAN_ONGOING = false; }
}

pub fn wfx_handle_event() -> sl_status_t {
    let control_register: *mut u16 = 0 as *mut u16;
    let mut cr: u16 = 0;
    let mut result: sl_status_t = SL_STATUS_OK;
    loop {
        unsafe {
            result = sl_wfx_receive_frame(control_register);
            cr = *control_register;
        }
        sprintln!("event cr: 0x{:x}", cr);
        if (cr & (SL_WFX_CONT_NEXT_LEN_MASK as u16)) == 0 {
            break;
        }
    }
    result
}

#[doc = " @brief Called when a message is received from the WFx chip"]
#[doc = ""]
#[doc = " @param event_payload is a pointer to the data received"]
#[doc = " @returns Returns SL_STATUS_OK if successful, SL_STATUS_FAIL otherwise"]
#[doc = ""]
#[doc = " @note Called by ::sl_wfx_receive_frame function"]
#[export_name = "sl_wfx_host_post_event"]
pub unsafe extern "C" fn sl_wfx_host_post_event(event_payload: *mut sl_wfx_generic_message_t) -> sl_status_t {
    let msg_type: u32 = (*event_payload).header.id as u32;

    if DEBUGGING {
        sprintln!("msg_type: 0x{:x}", msg_type);
    }
    match msg_type {
        sl_wfx_indications_ids_e_SL_WFX_CONNECT_IND_ID => {
            let connect_indication: sl_wfx_connect_ind_t = *(event_payload as *const sl_wfx_connect_ind_t);
            sl_wfx_connect_callback(connect_indication.body.mac, connect_indication.body.status);
        },
        sl_wfx_indications_ids_e_SL_WFX_DISCONNECT_IND_ID => {
            let disconnect_indication: sl_wfx_disconnect_ind_t = *(event_payload as *const sl_wfx_disconnect_ind_t);
            sl_wfx_disconnect_callback(disconnect_indication.body.mac, disconnect_indication.body.reason);
        },
        sl_wfx_indications_ids_e_SL_WFX_START_AP_IND_ID => {
            let start_ap_indication: sl_wfx_start_ap_ind_t = *(event_payload as *const sl_wfx_start_ap_ind_t);
            sl_wfx_start_ap_callback(start_ap_indication.body.status);
        },
        sl_wfx_indications_ids_e_SL_WFX_STOP_AP_IND_ID => {
            sl_wfx_stop_ap_callback();
        },
        sl_wfx_indications_ids_e_SL_WFX_RECEIVED_IND_ID => {
            let ethernet_frame: *const sl_wfx_received_ind_t = event_payload as *const sl_wfx_received_ind_t;
            if (*ethernet_frame).body.frame_type == 0 {
                sl_wfx_host_received_frame_callback( ethernet_frame );
            }
        },
        sl_wfx_indications_ids_e_SL_WFX_SCAN_RESULT_IND_ID => {
            let scan_result: *const sl_wfx_scan_result_ind_t = event_payload as *const sl_wfx_scan_result_ind_t;
            sl_wfx_scan_result_callback(&(*scan_result).body);
        },
        sl_wfx_indications_ids_e_SL_WFX_SCAN_COMPLETE_IND_ID => {
            let scan_complete: *const sl_wfx_scan_complete_ind_t = event_payload as *const sl_wfx_scan_complete_ind_t;
            sl_wfx_scan_complete_callback((*scan_complete).body.status);
        },
        sl_wfx_indications_ids_e_SL_WFX_AP_CLIENT_CONNECTED_IND_ID => {
            unimplemented!();
        },
        sl_wfx_indications_ids_e_SL_WFX_AP_CLIENT_REJECTED_IND_ID => {
            unimplemented!();
        },
        sl_wfx_indications_ids_e_SL_WFX_AP_CLIENT_DISCONNECTED_IND_ID => {
            unimplemented!();
        },
        sl_wfx_general_indications_ids_e_SL_WFX_GENERIC_IND_ID => {
            // nothing to do here, huh.
        },
        sl_wfx_general_indications_ids_e_SL_WFX_EXCEPTION_IND_ID => {
            sprintln!("Firmware exception");
            let firmware_exception: *const sl_wfx_exception_ind_t = event_payload as *const sl_wfx_exception_ind_t;
            sprintln!("Exeption data = ");
            for i in 0..SL_WFX_EXCEPTION_DATA_SIZE {
                sprint!("{:02x} ", (*firmware_exception).body.data[i as usize]);
            }
            sprintln!("End dump.");
        },
        sl_wfx_general_indications_ids_e_SL_WFX_ERROR_IND_ID => {
            sprintln!("Firmware error");
            let firmware_error: *const sl_wfx_error_ind_t = event_payload as *const sl_wfx_error_ind_t;
            sprintln!("Error type = {}", (*firmware_error).body.type_);
        },
        sl_wfx_general_indications_ids_e_SL_WFX_STARTUP_IND_ID => {
            sprintln!("wf200 started!");
        },
        sl_wfx_general_confirmations_ids_e_SL_WFX_CONFIGURATION_CNF_ID => {
            // this occurs during configuration, and is handled specially
        },
        sl_wfx_confirmations_ids_e_SL_WFX_START_SCAN_CNF_ID => {
            sprintln!("scan start confirmation.");
            SSID_INDEX = 0;
        },
        sl_wfx_confirmations_ids_e_SL_WFX_STOP_SCAN_CNF_ID => {
            sprintln!("scan stop confirmation.");
        },
        _ => {
            sprintln!("Unhandled return code from wfx200: {}", msg_type);
        },
    }

    if HOST_CONTEXT.waited_event_id == (*event_payload).header.id {
        if (*event_payload).header.length < 512usize as u16 {
            unsafe {
                for i in 0..(*event_payload).header.length {
                    WIFI_CONTEXT.event_payload_buffer[i as usize] = (event_payload as *const u8).add(i as usize).read();
                }
                HOST_CONTEXT.posted_event_id = (*event_payload).header.id;
            }
        }
    }
    SL_STATUS_OK
}
