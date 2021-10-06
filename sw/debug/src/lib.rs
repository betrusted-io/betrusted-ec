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
    pub fn putc(c: u8) {
        let mut uart_csr = CSR::new(HW_UART_BASE as *mut u32);
        // Allow TX buffer to drain if it's full
        // TX buffer is currently 256 bytes (see betrusted-ec/betrusted_ec.py)
        // Baud rate is 115200 with 8N1. So...
        // Time to send one byte: 1000ms / (115200 / 10) = 0.087ms
        // Bytes sent per ms: 1ms / 0.087ms = 11.5
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
            Self::putc(c);
        }
        Ok(())
    }
}

// The sprint* macros are for some of the older code that does not use
// LOG_LEVEL filtering.

#[macro_export]
macro_rules! sprint {
    () => {
        ()
    };
    ($string_literal:literal) => {
        (debug::log_str($string_literal))
    };
    ($($e:expr),*) => {
        use core::fmt::Write;
        let _ = write!(crate::debug::Uart {}, $($e),*);
    }
}

#[macro_export]
macro_rules! sprintln {
    () => {
        (debug::newline())
    };
    ($string_literal:literal) => {
        debug::log_str_ln($string_literal);
        debug::newline();
    };
    ($($e:expr),*) => {
        use core::fmt::Write;
        let _ = write!(crate::debug::Uart {}, $($e),*);
        debug::newline();
    }
}

// These log* macros exist to allow for granular logging using a LOG_LEVEL
// const defined per-file. The idea is to make it possible to include a lot of
// trace level log messages that will normally compile down to nothing because
// of the log level checks. Using macro_rules match arms that make a function
// call for common calling patterns helps to avoid code bloat from a lot of
// inlined macro expansions.

/// Log a fmt expression *without* a newline at the end
#[macro_export]
macro_rules! log {
    () => {
        ()
    };
    ($level:expr, $string_literal:literal) => {
        if LOG_LEVEL <= $level {
            (debug::log_str($string_literal))
        }
    };
    ($level:expr, $($e:expr),*) => {
        if LOG_LEVEL <= $level {
            use core::fmt::Write;
            let _ = write!(crate::debug::Uart {}, $($e),*);
        }
    }
}

/// Log a fmt expression with a newline appended at the end
#[macro_export]
macro_rules! logln {
    () => {
        if LOG_LEVEL <= $level {
            (debug::newline())
        }
    };
    ($level:expr, $string_literal:literal) => {
        if LOG_LEVEL <= $level {
            debug::log_str_ln($string_literal);
        }
    };
    ($level:expr, $($e:expr),*) => {
        if LOG_LEVEL <= $level {
            use core::fmt::Write;
            let _ = write!(crate::debug::Uart {}, $($e),*);
            debug::newline();
        }
    }
}

/// Log a hex formatted number with a string prefix
/// Using this macro should compile smaller than using `log!(LL::..., "prefix {:X}", n)`
#[macro_export]
macro_rules! loghex {
    ($level:expr, $prefix:literal, $number:expr) => {
        if LOG_LEVEL <= $level {
            debug::log_hex($prefix, ($number as u32));
        }
    };
}

/// Log a hex formatted number with a string prefix and a newline at the end
/// Using this macro should compile smaller than using `logln!(LL::..., "prefix {:X}", n)`
#[macro_export]
macro_rules! loghexln {
    ($level:expr, $prefix:literal, $number:expr) => {
        if LOG_LEVEL <= $level {
            debug::log_hex_ln($prefix, ($number as u32));
        }
    };
}

/// Write a newline to the UART
pub fn newline() {
    Uart::putc(b'\r');
    Uart::putc(b'\n');
}

/// Write a string literal to the UART
pub fn log_str(s: &str) {
    let _ = Uart {}.write_str(s);
}

/// Write a string literal to the UART, appending a newline at the end
pub fn log_str_ln(s: &str) {
    let _ = Uart {}.write_str(s);
    newline();
}

/// Write a string prefix followed by a hex formatted number to the UART
pub fn log_hex(prefix: &str, n: u32) {
    let _ = write!(Uart {}, "{}{:X}", prefix, n);
}

/// Write a string prefix followed by a hex formatted number and a newline to the UART
pub fn log_hex_ln(prefix: &str, n: u32) {
    log_hex(prefix, n);
    newline();
}
