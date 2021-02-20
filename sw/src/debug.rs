#[allow(dead_code)]
use utralib::generated::*;

pub struct Uart {
}

#[cfg(feature = "debug_uart")]
impl Uart {
    pub fn putc(&self, c: u8) {
        let mut uart_csr = CSR::new(HW_UART_BASE as *mut u32);
        // Wait until TXFULL is `0`
        while uart_csr.rf(utra::uart::TXFULL_TXFULL) != 0 {}
        uart_csr.wfo(utra::uart::RXTX_RXTX, c as u32)
    }
}

#[cfg(not(feature = "debug_uart"))]
impl Uart {
    pub fn putc(&self, _c: u8) {
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

