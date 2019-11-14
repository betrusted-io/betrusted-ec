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
    chg_set_autoparams(&p);

    chg_start(&p);
    gg_start(&p);

    unsafe{ DBGSTR[0] = gg_device_type(&p) as u32; }

    // flash an LED!
    let mut last_time : u32 = get_time_ms(&p);
    let mut last_state : bool = false;
    let mut charger : BtCharger = BtCharger::new();
    let mut voltage : i16 = 0;
    let mut linkindex : usize = 0;
    loop { 
        if get_time_ms(&p) - last_time > 500 {
            last_time = get_time_ms(&p);
            if last_state {
                unsafe{p.RGB.raw.write( |w| {w.bits(5)}); }
            } else {
                // once every second run these routines
                voltage = gg_voltage(&p);
                unsafe{ DBGSTR[1] = voltage as u32; }
                if chg_is_charging(&p) {
                    unsafe{ DBGSTR[2] = 1;}
                    unsafe{p.RGB.raw.write( |w| {w.bits(2)}); }
                } else {
                    unsafe{ DBGSTR[2] = 0;}
                    unsafe{p.RGB.raw.write( |w| {w.bits(0)}); }
                }
                chg_keepalive_ping(&p);
                charger.update_regs(&p);
            }
            last_state = ! last_state;
        }

        // simple test routine to loopback Rx data to the Tx on the COM port
        if p.COM.status.read().rxfull().bit_is_set() { 
            // read the rx data, then add a constant to it and fold it back into the tx register
            let mut rx: u16 = (p.COM.rx0.read().bits() as u16) | ((p.COM.rx1.read().bits() as u16) << 8);
            match rx {
                0x8000 => linkindex = 0,
                _ => rx = voltage as u16,
            }

            if linkindex < 7 {
                rx = charger.registers[linkindex] as u16;
                linkindex +=1;
            }
            unsafe{ p.COM.tx0.write(|w| w.bits( (rx & 0xFF) as u32 )); }
            unsafe{ p.COM.tx1.write(|w| w.bits( ((rx >> 8) & 0xFF) as u32 )); }
    }
        
    }
}