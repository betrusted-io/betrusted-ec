#![no_main]
#![no_std]

use core::panic::PanicInfo;
use riscv_rt::entry;

extern crate betrusted_hal;

const CONFIG_CLOCK_FREQUENCY: u32 = 12_000_000;

// allocate a global, unsafe static string for debug output
#[used]  // This is necessary to keep DBGSTR from being optimized out
static mut DBGSTR: [u32; 4] = [0, 0, 0, 0];

#[panic_handler]
fn panic(_panic: &PanicInfo<'_>) -> ! {
    loop {}
}

#[entry]
fn main() -> ! {
    use betrusted_hal::hal_i2c::*;
    use betrusted_hal::hal_time::*;
    use betrusted_hal::api_gasgauge::*;
    use betrusted_hal::api_charger::*;

    let p = betrusted_pac::Peripherals::take().unwrap();

    time_init(&p);

    i2c_init(&p, CONFIG_CLOCK_FREQUENCY / 1_000_000);
    // this needs to be one of the first things called after I2C comes up
    chg_set_safety(&p);

    gg_start(&p);

    chg_set_autoparams(&p);
    chg_start(&p);

    // flash an LED!
    let mut last_time : u32 = get_time_ms(&p);
    let mut last_state : bool = false;
    let mut charger : BtCharger = BtCharger::new();
    let mut voltage : i16 = 0;
    let mut current: i16 = 0;
    let mut linkindex : usize = 0;
    loop { 
        if get_time_ms(&p) - last_time > 1000 {
            last_time = get_time_ms(&p);
            if last_state {
                unsafe{p.RGB.raw.write( |w| {w.bits(5)}); }
                chg_keepalive_ping(&p);
                charger.update_regs(&p);
            } else {
                // once every second run these routines
                voltage = gg_voltage(&p);
                current = gg_avg_current(&p);

                if chg_is_charging(&p) {
                    unsafe{p.RGB.raw.write( |w| {w.bits(2)}); }
                } else {
                    unsafe{p.RGB.raw.write( |w| {w.bits(0)}); }
                }
            }
            last_state = ! last_state;
        }

        // simple test routine to loopback Rx data to the Tx on the COM port
        if p.COM.status.read().rxfull().bit_is_set() { 
            // read the rx data, then add a constant to it and fold it back into the tx register
            let rx: u16 = (p.COM.rx0.read().bits() as u16) | ((p.COM.rx1.read().bits() as u16) << 8);
            while p.COM.status.read().rxfull().bit_is_set() {} // this should clear before going on

            let mut tx: u16 = 0;
            match rx {
                0x8000 => linkindex = 0,
                _ => tx = voltage as u16,
            }

            if linkindex < 7 {
                tx = charger.registers[linkindex] as u16;
            } else if linkindex == 7 {
                tx = voltage as u16;
            } else if linkindex == 8 {
                tx = current as u16;
            }
            linkindex = linkindex + 1;
            unsafe{ p.COM.tx0.write(|w| w.bits( (tx & 0xFF) as u32 )); }
            unsafe{ p.COM.tx1.write(|w| w.bits( ((tx >> 8) & 0xFF) as u32 )); }
        }
        
    }
}