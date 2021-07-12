//! Serial debug for wishbone-bridge crossover UART.
//!
//! To enable serial debug printing:
//!  1. Build and flash EC gateware with `debugonly = True`
//!  2. In ../Cargo.toml, enable the "debug_uart" feature for this crate
//!

#[cfg(feature = "debug_uart")]
use utralib::generated::*;

use crate::hal_time::delay_ms;

/// The flow control timeout determines how long putc() waits to decide if the
/// wishbone-bridge connection is down before dropping characters.
///
const FLOW_CONTROL_TIMEOUT_MS: usize = 1000;

pub struct Uart {}

#[cfg(feature = "debug_uart")]
impl Uart {
    /// Write to UART with TX buffer flow control to allow for intermittent
    /// wishbone-bridge connection.
    ///
    /// Goal of flow control strategy is to provide non-blocking IO with
    /// timeout limited CSR polling in order to:
    ///  1. Avoid DoS of wishbone bus
    ///  2. Avoid starving main control loop and COM handlers for CPU cycles
    ///
    /// The tradeoff for control loop responsiveness is that some debug
    /// characters may be dropped. Timeout delay is calibrated so that, when a
    /// wishbone-tool serial connection is established promptly after reset,
    /// dropped characters are unlikely.
    ///
    pub fn putc(&self, c: u8) {
        static mut MUTED: bool = false;
        let mut uart_csr = CSR::new(HW_UART_BASE as *mut u32);
        let tx_buffer_empty = uart_csr.rf(utra::uart::TXEMPTY_TXEMPTY) == 1;
        if unsafe { MUTED } && !tx_buffer_empty {
            // Looks like connection is still down... Drop this character.
            return;
        } else if tx_buffer_empty {
            // Yay... looks like wishbone-bridge connection is back!
            unsafe {
                MUTED = false;
            }
        } else {
            if uart_csr.rf(utra::uart::TXFULL_TXFULL) == 1 {
                // Hmm... TX buffer is newly full... Watch to see if it drains...
                for _ in 0..FLOW_CONTROL_TIMEOUT_MS {
                    delay_ms(1);
                    if uart_csr.rf(utra::uart::TXFULL_TXFULL) == 0 {
                        break;
                    }
                }
                if uart_csr.rf(utra::uart::TXFULL_TXFULL) == 1 {
                    // Boo... Nope. Looks like the connection just dropped...
                    // 1. Mute to avoid pointlessly calling delay_ms() while there is
                    //    no active wishbone-bridge connection to drain the TX buffer
                    // 2. Begin dropping characters, starting with this one
                    unsafe {
                        MUTED = true;
                    }
                    return;
                }
            }
        }
        // Since the flow control checks all passed, send a character
        uart_csr.wfo(utra::uart::RXTX_RXTX, c as u32);
    }
}

#[cfg(not(feature = "debug_uart"))]
impl Uart {
    pub fn putc(&self, _c: u8) {}
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

// Note to people tempted to change "\r\n" to "\n" in the macros below:
// Wishbone-tool's serial bridge expects CRLF style line termination. If you do
// LF only, it will print your text in diagonal cascades instead of columns.
//
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
