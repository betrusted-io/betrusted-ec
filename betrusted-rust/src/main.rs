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
#[used]  // test to see if the "used" attribute means we no longer need to call debugcommit() <<< 
static mut DBGSTR: [u32; 4] = [0, 0, 0, 0];

#[panic_handler]
fn panic(_panic: &PanicInfo<'_>) -> ! {
    loop {}
}

// debug simply forces DBGSTR to be committed to an unsafe bbbbb
fn debugcommit(p: &betrusted_pac::Peripherals) {
    for i in 0..4 {
        unsafe{&p.RGB.raw.write( |w| {w.bits(DBGSTR[i] as u32)}); }
    }
}

#[entry]
fn main() -> ! {
    use betrusted_hal::hal_i2c::hal_i2c::*;
    use betrusted_hal::hal_time::hal_time::*;

    let p = betrusted_pac::Peripherals::take().unwrap();

    i2c_init(&p, CONFIG_CLOCK_FREQUENCY / 1_000_000);
    time_init(&p);

    // flash an LED!
    loop { 
        let txbuf: [u8; 1] = [BQ24157_ID_ADR];
        let mut rxbuf: [u8; 1] = [0];

        unsafe{p.RGB.raw.write( |w| {w.bits(5)}); }
        delay_ms(&p, 500);

        i2c_master(&p, BQ24157_ADDR, Some(&txbuf), Some(&mut rxbuf), I2C_TRANS_TIMEOUT);
        unsafe{ DBGSTR[0] = rxbuf[0] as u32;}
        debugcommit(&p);
        
        unsafe{p.RGB.raw.write( |w| {w.bits(0)}); }
        delay_ms(&p, 500);
    }
}