#![no_main]
#![no_std]

use core::panic::PanicInfo;
use riscv_rt::entry;

extern crate betrusted_hal;

const CONFIG_CLOCK_FREQUENCY: u32 = 12_000_000;
const I2C_TRANS_TIMEOUT: u32 = 10;

const BQ24157_ADDR: u8 = 0x6a;
const BQ24157_ID_ADR: u8 = 3;

#[panic_handler]
fn panic(_panic: &PanicInfo<'_>) -> ! {
    loop {}
}

#[entry]
fn main() -> ! {
    use betrusted_hal::hal_i2c::hal_i2c::*;

    i2c_init(CONFIG_CLOCK_FREQUENCY / 1_000_000);

    let peripherals = betrusted_pac::Peripherals::take().unwrap();

    // flash an LED!
    let mut delay: u32 = 0;
    let mut counter: u32 = 42;
    loop { 
        let txbuf: [u8; 1] = [BQ24157_ID_ADR];
        let mut rxbuf: [u8; 1] = [0];

        i2c_master( BQ24157_ADDR, &txbuf, &mut rxbuf, I2C_TRANS_TIMEOUT);
        
        unsafe{peripherals.RGB.raw.write( |w| {w.bits(5)}); }
        while delay < 500_000 {
            delay = delay + 1;
        }

        counter = counter + 1;

        unsafe{peripherals.RGB.raw.write( |w| {w.bits(0)}); }
        while delay > 1 {
            delay = delay - 1;
        }
    }
}