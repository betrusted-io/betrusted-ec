#![allow(unused)]

use crate::betrusted_hal::hal_time::delay_ms;
use crate::betrusted_pac;
use crate::wfx_bindings;

pub use wfx_bindings::*;

#[export_name = "sl_wfx_host_spi_cs_assert"]
pub unsafe extern "C" fn sl_wfx_host_spi_cs_assert() -> sl_status_t {
    unsafe { betrusted_pac::Peripherals::steal().WIFI.cs.write(|w| w.cs().bit(true)); }
    SL_STATUS_OK
}

pub struct Wfx200 {
    p: betrusted_pac::Peripherals,
}

impl Wfx200 {
    pub fn new() -> Self {
        Wfx200 {
            p: unsafe{ betrusted_pac::Peripherals::steal() },
        }
    }
    pub fn sl_wfx_host_spi_cs_deassert(&mut self) -> sl_status_t {
        self.p.WIFI.cs.write(|w| w.cs().bit(false));
        SL_STATUS_OK
    }
    pub fn sl_wfx_host_enable_platform_interrupt(&mut self) -> sl_status_t {
        self.p.WIFI.ev_enable.write(|w| unsafe{w.bits(1)} );
        SL_STATUS_OK
    }
    pub fn sl_wfx_host_disable_platform_interrupt(&mut self) -> sl_status_t {
        self.p.WIFI.ev_enable.write(|w| unsafe{w.bits(0)} );
        SL_STATUS_OK
    }
    pub fn sl_wfx_host_init_bus(&mut self)-> sl_status_t {
        self.p.WIFI.control.write(|w| unsafe{w.bits(0)});
        self.p.WIFI.wifi.write(|w| unsafe{w.bits(0)});
        SL_STATUS_OK
    }
    pub fn sl_wfx_host_deinit_bus(&mut self)-> sl_status_t { 
        self.p.WIFI.control.write(|w| unsafe{w.bits(0)});
        self.p.WIFI.wifi.write(|w| unsafe{w.bits(0)});
        SL_STATUS_OK 
    }
    pub fn sl_wfx_host_reset_chip(&mut self) -> sl_status_t {
        self.p.WIFI.wifi.write(|w| unsafe{w.reset().bit(true)});
        delay_ms(&self.p, 1);
        self.p.WIFI.wifi.write(|w| unsafe{w.reset().bit(false)});
        SL_STATUS_OK
    }
    pub fn sl_wfx_host_hold_in_reset(&mut self) -> sl_status_t {
        self.p.WIFI.wifi.write(|w| unsafe{w.reset().bit(true)});
        SL_STATUS_OK
    }
    pub fn sl_wfx_host_wait(&mut self, wait_ms: u32) -> sl_status_t {
        delay_ms(&self.p, wait_ms);
        SL_STATUS_OK
    }
    pub fn sl_wfx_host_set_wake_up_pin(&mut self, state: u8) -> sl_status_t {
        if state == 0 {
            self.p.WIFI.wifi.modify(|_,w| w.wakeup().clear_bit());
        } else {
            self.p.WIFI.wifi.modify(|_,w| w.wakeup().set_bit());
        }
        SL_STATUS_OK
    }
    /// no locking because we're single threaded and one process only to drive all of this
    pub fn sl_wfx_host_lock() -> sl_status_t { SL_STATUS_OK }
    pub fn sl_wfx_host_unlock() -> sl_status_t { SL_STATUS_OK }

    #[doc = " @brief Send data on the SPI bus"]
    #[doc = ""]
    #[doc = " @param type is the type of bus action (see ::sl_wfx_host_bus_transfer_type_t)"]
    #[doc = " @param header is a pointer to the header data"]
    #[doc = " @param header_length is the length of the header data"]
    #[doc = " @param buffer is a pointer to the buffer data"]
    #[doc = " @param buffer_length is the length of the buffer data"]
    #[doc = " @returns Returns SL_STATUS_OK if successful, SL_STATUS_FAIL otherwise"]
    pub fn sl_wfx_host_spi_transfer_no_cs_assert(
        &mut self,
        type_: sl_wfx_host_bus_transfer_type_t,
        header: *mut u8,
        header_length: u16,
        buffer: *mut u8,
        buffer_length: u16,
    ) -> sl_status_t {
        
        SL_STATUS_OK
    }
}
