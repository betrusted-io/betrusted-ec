#![no_main]
#![no_std]

use core::panic::PanicInfo;
use riscv_rt::entry;

extern crate betrusted_hal;

const CONFIG_CLOCK_FREQUENCY: u32 = 12_000_000;
const I2C_TRANS_TIMEOUT: u32 = 10;

const BQ24157_ADDR: u8 = 0x6a;
const BQ24157_ID_ADR: u8 = 3;

// allocate a global, unsafe static string for debug output
static mut DBGSTR: [u32; 4] = [0, 0, 0, 0];

#[panic_handler]
fn panic(_panic: &PanicInfo<'_>) -> ! {
    loop {}
}

fn debug(peripherals: &betrusted_pac::Peripherals) {
    for i in 0..4 {
        unsafe{&peripherals.RGB.raw.write( |w| {w.bits(DBGSTR[i] as u32)}); }
    }
}

#[entry]
fn main() -> ! {
    use betrusted_hal::hal_i2c::hal_i2c::*;
    use betrusted_hal::hal_time::hal_time::*;

    let peripherals = betrusted_pac::Peripherals::take().unwrap();

    i2c_init(&peripherals, CONFIG_CLOCK_FREQUENCY / 1_000_000);
    time_init(&peripherals);

    // flash an LED!
    loop { 
        let txbuf: [u8; 1] = [BQ24157_ID_ADR];
        let mut rxbuf: [u8; 1] = [0];

        unsafe{peripherals.RGB.raw.write( |w| {w.bits(5)}); }
        delay_ms(&peripherals, 500);

        i2c_master(&peripherals, BQ24157_ADDR, &txbuf, &mut rxbuf, I2C_TRANS_TIMEOUT);
        unsafe{ DBGSTR[0] = rxbuf[0] as u32;}
        debug(&peripherals);
        
        unsafe{peripherals.RGB.raw.write( |w| {w.bits(0)}); }
        delay_ms(&peripherals, 500);
    }
}