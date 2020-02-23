#![allow(unused)]

use crate::betrusted_hal::hal_time::delay_ms;
use crate::betrusted_hal::hal_time::get_time_ms;
use crate::betrusted_pac;
use crate::wfx_bindings;
use xous_nommu::syscalls::*;
use core::slice;
use core::str;

#[macro_use]
mod debug;

pub use wfx_bindings::*;

static mut WF200_EVENT: bool = false;
pub const WIFI_EVENT_SPI: u32 = 0x1;
pub const WIFI_EVENT_WIRQ: u32 = 0x2;

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
    unsafe { betrusted_pac::Peripherals::steal().WIFI.cs.write(|w| w.cs().bit(true)); }
    SL_STATUS_OK
}

#[export_name = "sl_wfx_host_spi_cs_deassert"]
pub unsafe extern "C" fn sl_wfx_host_spi_cs_deassert() -> sl_status_t {
    unsafe{ betrusted_pac::Peripherals::steal().WIFI.cs.write(|w| w.cs().bit(false)); }
    SL_STATUS_OK
}

#[export_name = "sl_wfx_host_deinit_bus"]
pub unsafe extern "C" fn sl_wfx_host_deinit_bus()-> sl_status_t { 
    unsafe{ betrusted_pac::Peripherals::steal().WIFI.control.write(|w| w.bits(0)); }
    unsafe{ betrusted_pac::Peripherals::steal().WIFI.wifi.write(|w| w.bits(0)); }
    SL_STATUS_OK 
}

#[export_name = "sl_wfx_host_enable_platform_interrupt"]
pub unsafe extern "C" fn sl_wfx_host_enable_platform_interrupt() -> sl_status_t {
   sys_interrupt_claim(betrusted_pac::Interrupt::WIFI as usize, |_| {
       wf200_event_set();
        // clear the interrupt
        unsafe{ betrusted_pac::Peripherals::steal().WIFI.ev_pending.write(|w| w.bits(WIFI_EVENT_WIRQ)); }
    })
    .unwrap();
    unsafe{ betrusted_pac::Peripherals::steal().WIFI.ev_enable.write(|w| unsafe{w.bits(WIFI_EVENT_WIRQ)} ); }
    SL_STATUS_OK    
}

#[export_name = "sl_wfx_host_disable_platform_interrupt"]
pub unsafe extern "C" fn sl_wfx_host_disable_platform_interrupt() -> sl_status_t {
    unsafe{ betrusted_pac::Peripherals::steal().WIFI.ev_enable.write(|w| unsafe{w.bits(0)} ); }
    sys_interrupt_free(betrusted_pac::Interrupt::WIFI as usize);
    SL_STATUS_OK
}

#[export_name = "sl_wfx_host_init_bus"]
pub unsafe extern "C" fn sl_wfx_host_init_bus()-> sl_status_t {
    unsafe {
        betrusted_pac::Peripherals::steal().WIFI.control.write(|w| unsafe{w.bits(0)});
        betrusted_pac::Peripherals::steal().WIFI.wifi.write(|w| unsafe{w.bits(0)});
    }
    SL_STATUS_OK
}

#[export_name = "sl_wfx_host_reset_chip"]
pub unsafe extern "C" fn sl_wfx_host_reset_chip() -> sl_status_t {
    betrusted_pac::Peripherals::steal().WIFI.wifi.write(|w| unsafe{w.reset().bit(true)});
    delay_ms(&betrusted_pac::Peripherals::steal(), 10);
    betrusted_pac::Peripherals::steal().WIFI.wifi.write(|w| unsafe{w.reset().bit(false)});
    delay_ms(&betrusted_pac::Peripherals::steal(), 10);
    SL_STATUS_OK
}

#[export_name = "sl_wfx_host_hold_in_reset"]
pub unsafe extern "C" fn sl_wfx_host_hold_in_reset() -> sl_status_t {
    betrusted_pac::Peripherals::steal().WIFI.wifi.write(|w| unsafe{w.reset().bit(true)});
    SL_STATUS_OK
}

#[export_name = "sl_wfx_host_wait"]
pub unsafe extern "C" fn sl_wfx_host_wait(wait_ms: u32) -> sl_status_t {
    delay_ms(&betrusted_pac::Peripherals::steal(), wait_ms);
    SL_STATUS_OK
}

#[export_name = "sl_wfx_host_set_wake_up_pin"]
pub unsafe extern "C" fn sl_wfx_host_set_wake_up_pin(state: u8) -> sl_status_t {
    if state == 0 {
        betrusted_pac::Peripherals::steal().WIFI.wifi.modify(|_,w| w.wakeup().clear_bit());
    } else {
        betrusted_pac::Peripherals::steal().WIFI.wifi.modify(|_,w| w.wakeup().set_bit());
    }
    SL_STATUS_OK
}

/// no locking because we're single threaded and one process only to drive all of this
#[export_name = "sl_wfx_host_lock"]
pub unsafe extern "C" fn sl_wfx_host_lock() -> sl_status_t { SL_STATUS_OK }
#[export_name = "sl_wfx_host_unlock"]
pub unsafe extern "C" fn sl_wfx_host_unlock() -> sl_status_t { SL_STATUS_OK }

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
    unsafe {
        let mut header_len_mtu = header_length / 2; // we do "MTU" in case header_len is odd. should never be but...this is their API
        let mut header_pos: usize = 0;
        while header_len_mtu > 0 {
            let word: u16 = ((header.add(header_pos + 1).read() as u16) << 8) | (header.add(header_pos).read() as u16);
            betrusted_pac::Peripherals::steal().WIFI.tx.write(|w| w.bits(word as u32));
            header_len_mtu -= 1;
            header_pos += 2;

            betrusted_pac::Peripherals::steal().WIFI.control.write(|w| w.go().bit(true));
            while betrusted_pac::Peripherals::steal().WIFI.status.read().tip().bit_is_set() {}
        }
        if type_ == sl_wfx_host_bus_transfer_type_t_SL_WFX_BUS_READ {
            let mut buffer_len_mtu = buffer_length / 2;
            let mut buffer_pos: usize = 0;
            while buffer_len_mtu > 0 {
                // transmit a dummy word to get the rx data
                betrusted_pac::Peripherals::steal().WIFI.tx.write(|w| w.bits(0));
                betrusted_pac::Peripherals::steal().WIFI.control.write(|w| w.go().bit(true));
                while betrusted_pac::Peripherals::steal().WIFI.status.read().tip().bit_is_set() {}

                let word: u16 = betrusted_pac::Peripherals::steal().WIFI.rx.read().bits() as u16;
                buffer.add(buffer_pos + 1).write((word >> 8) as u8);
                buffer.add(buffer_pos).write((word & 0xff) as u8);
                buffer_len_mtu -= 1;
                buffer_pos += 2;
            }
        } else {
            // transmit the buffer
            let mut buffer_len_mtu = buffer_length / 2;
            let mut buffer_pos: usize = 0;
            while buffer_len_mtu > 0 {
                let word: u16 = ((buffer.add(buffer_pos + 1).read() as u16) << 8) | (buffer.add(buffer_pos).read() as u16);
                betrusted_pac::Peripherals::steal().WIFI.tx.write(|w| w.bits(word as u32));
                buffer_len_mtu -= 1;
                buffer_pos += 2;
    
                betrusted_pac::Peripherals::steal().WIFI.control.write(|w| w.go().bit(true));
                while betrusted_pac::Peripherals::steal().WIFI.status.read().tip().bit_is_set() {}
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
#[export_name = "sl_wfx_host_init"]
pub unsafe extern "C" fn sl_wfx_host_init() -> sl_status_t {
    unsafe {
        WFX_RAM_ALLOC = WFX_RAM_OFFSET;
        WFX_PTR_COUNT = 0;
        WFX_PTR_LIST = [0; WFX_MAX_PTRS];
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
    let start_time = get_time_ms(unsafe{&betrusted_pac::Peripherals::steal()});
    while (get_time_ms(unsafe{&betrusted_pac::Peripherals::steal()}) - start_time) < timeout_ms {
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
            delay_ms(unsafe{&betrusted_pac::Peripherals::steal()}, 1);
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
    
    let rframe: &str = unsafe {str::from_utf8(slice::from_raw_parts(frame as *const u8, frame_len as usize)).expect("unable to create string from parts") };
    let u8frame: *const u8 = rframe.as_ptr();
    sprint!("TX> {:02x}{:02x} {:02x}{:02x} ", u8frame.add(0).read(), u8frame.add(1).read(), u8frame.add(2).read(), u8frame.add(3).read());
    for i in 4..frame_len {
        sprint!("{:02x} ", u8frame.add(i as usize).read());
    }
    sprint!("\r\n");
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
    delay_ms(&betrusted_pac::Peripherals::steal(), 2); // don't ask me, this is literally the reference vendor code!
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
    while(__nptr).add(length).read() != 0 {
        length += 1;
    }
    let s = unsafe { str::from_utf8(slice::from_raw_parts(__nptr as *const u8, length)).expect("unable to parse string") };
    usize::from_str_radix(s.trim_start_matches("0x"), 16).expect("unable to parse num") as c_types::c_ulong
}

