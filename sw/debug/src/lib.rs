#![no_std]
//! Serial debug logger.
//!
//! In the past, this used Wishbone-bridge, but now it's just a plain UART.

use utralib::generated::*;
extern crate betrusted_hal;
use crate::betrusted_hal::hal_time::delay_ms;

/// Flow control timeout limits how long putc() waits to drain a full TX buffer
const FLOW_CONTROL_TIMEOUT_MS: usize = 5;

#[derive(PartialOrd, PartialEq)]
#[allow(dead_code)]
pub enum LL {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
    Fatal = 5,
}

static mut LOG_LEVEL: LL = LL::Info;

pub fn set_log_level(level: LL) {
    unsafe {
        LOG_LEVEL = level;
    }
}

pub struct Uart {}
impl Uart {
    /// Write to UART with TX buffer flow control
    pub fn putc(&self, c: u8) {
        let mut uart_csr = CSR::new(HW_UART_BASE as *mut u32);
        // Allow TX buffer to drain if it's full
        for _ in 0..FLOW_CONTROL_TIMEOUT_MS {
            if uart_csr.rf(utra::uart::TXFULL_TXFULL) == 1 {
                delay_ms(1);
            } else {
                break;
            }
        }
        // Send a character
        uart_csr.wfo(utra::uart::RXTX_RXTX, c as u32);
    }
}

use core::fmt::{Error, Write};
impl Write for Uart {
    fn write_str(&mut self, s: &str) -> Result<(), Error> {
        for c in s.bytes() {
            self.putc(c);
        }
        Ok(())
    }
}

#[macro_export]
macro_rules! sprint
{
	($($args:tt)+) => ({
			use core::fmt::Write;
			let _ = write!(crate::debug::Uart {}, $($args)+);
	});
}

#[macro_export]
macro_rules! sprintln
{
	() => ({
		sprint!("\r\n")
	});
	($fmt:expr) => ({
		sprint!(concat!($fmt, "\r\n"))
	});
	($fmt:expr, $($args:tt)+) => ({
		sprint!(concat!($fmt, "\r\n"), $($args)+)
	});
}

#[macro_export]
macro_rules! log {
    ($level:expr, $($e:expr),+) => {
        if LOG_LEVEL <= $level {
            sprint!($($e),+)
        }
    }
}

#[macro_export]
macro_rules! logln {
    ($level:expr, $($e:expr),*) => {
        if LOG_LEVEL <= $level {
            sprintln!($($e),*)
        }
    }
}
