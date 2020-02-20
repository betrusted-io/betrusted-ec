#![allow(unused)]

use bitflags::*;
use volatile::*;

mod wfx_bindings;
use wfx_bindings::*;

pub struct Wfx200 {
    p: betrusted_pac::Peripherals,
}

impl Wfx200 {
    pub fn new() -> Self {
        Wfx200 {
            p: unsafe{ betrusted_pac::Peripherals::steal() },
        }
    }

    pub fn sl_wfx_host_spi_cs_assert(&mut self) -> sl_status_t {
        self.p.WIFI.cs.write(|w| w.cs().bit(true));

        SL_STATUS_OK
    }
    pub fn sl_wfx_host_spi_cs_deassert(&mut self) -> sl_status_t {
        self.p.WIFI.cs.write(|w| w.cs().bit(false));

        SL_STATUS_OK
    }
}
