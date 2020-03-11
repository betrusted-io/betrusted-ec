pub struct Uart {
    pub base: *mut u32,
}

pub const CROSSOVER_UART: Uart = Uart {
    base: 0xE000_1800 as *mut u32,
};

#[cfg(feature = "debug_uart")]
impl Uart {
    pub fn putc(&self, c: u8) {
        unsafe {
            // Wait until TXFULL is `0`
            while self.base.add(1).read_volatile() != 0 {
                ()
            }
            self.base.add(0).write_volatile(c as u32)
        };
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
			let _ = write!(crate::hal_wf200::debug::CROSSOVER_UART, $($args)+);
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

