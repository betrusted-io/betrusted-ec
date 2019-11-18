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

enum ComState {
    Idle,
    Stat,
    Power,
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

    unsafe{p.RGB.raw.write( |w| {w.bits(0)});}  // turn off all the LEDs

    let mut last_time : u32 = get_time_ms(&p);
    let mut last_state : bool = false;
    let mut charger : BtCharger = BtCharger::new();
    let mut voltage : i16 = 0;
    let mut current: i16 = 0;
    let mut stby_current: i16 = 0;
    let mut linkindex : usize = 0;
    let mut ledbits: u32 = 0;
    let mut comstate: ComState = ComState::Idle;
    let mut pd_time: u32 = 0;
    let mut pd_interval: u32 = 0;
    let mut soc_on: bool = true;
    loop { 
        if get_time_ms(&p) - last_time > 1000 {
            last_time = get_time_ms(&p);
            if last_state {
                ledbits = 5;
                chg_keepalive_ping(&p);
                charger.update_regs(&p);
            } else {
                // once every second run these routines
                voltage = gg_voltage(&p);
                current = gg_avg_current(&p);

                // soc turns on automatically if the charger comes up
                //if chg_is_charging(&p) || (p.POWER.stats.read().state().bits() & 0x2 != 0) {
                //    soc_on = true;
                //}

                if !soc_on {
                    stby_current = gg_avg_current(&p);
                }

                if chg_is_charging(&p) {
                    ledbits = 2;
                } else {
                    ledbits = 0;
                }
            }
            last_state = ! last_state;

            unsafe {
                if DBGSTR[0] != 0 { // only update the LED bits if called for via DBGSTR
                    p.RGB.raw.write( |w| {w.bits(ledbits)});
                }
            }

        }

        // monitor the keyboard inputs if the soc is in the off state
        if !soc_on {
            if get_time_ms(&p) - pd_time > 2000 { // delay for power-off to settle
                
                if get_time_ms(&p) - pd_interval > 50 { // every 50ms check key state
                    // briefly turn on scan, while keeping discharge and self on
                    unsafe{ p.POWER.power.write(|w| w.bits(0xd)); } // 0xd
                    pd_interval = get_time_ms(&p);
                    
                    if (p.POWER.stats.read().monkey().bits() & 0x2) != 0 { // MON1 key is high/pressed
                        // power on the SOC
                        unsafe{ p.POWER.power.write(|w| w.bits(0x1)); } // first disengage discharge
                        soc_on = true;
                        unsafe{ p.POWER.power.write(|w| w.bits(0x3)); } // then try to power on the SoC
                        pd_time = get_time_ms(&p);
                    }

                    if !soc_on {
                        // turn off scan, revert discharge to true
                        unsafe { p.POWER.power.write(|w| w.bits(0x5)); }
                    }
                }
            }
        }
        // simple test routine to loopback Rx data to the Tx on the COM port
        if p.COM.status.read().rxfull().bit_is_set() { 
            // read the rx data, then add a constant to it and fold it back into the tx register
            let rx: u16 = (p.COM.rx0.read().bits() as u16) | ((p.COM.rx1.read().bits() as u16) << 8);
            while p.COM.status.read().rxfull().bit_is_set() {} // this should clear before going on

            let mut tx: u16 = 0;
            match rx {
                0x8000 => {linkindex = 0; comstate = ComState::Stat;},
                0x9000..=0x90FF => {
                    linkindex = 0;
                    comstate = ComState::Power;
                    if get_time_ms(&p) - pd_time > 2000 {
                        unsafe{ p.POWER.power.write(|w| w.bits((rx & 0xFF) as u32)); } 
                        pd_time = get_time_ms(&p);
                        pd_interval = get_time_ms(&p);
                        soc_on = false;
                    }
                },
                _ => tx = voltage as u16,
            }

            match comstate {
                ComState::Stat => {
                    if linkindex < 7 {
                        tx = charger.registers[linkindex] as u16;
                    } else if linkindex == 7 {
                        tx = voltage as u16;
                    } else if linkindex == 8 {
                        tx = stby_current as u16;
                    } else if linkindex == 9 {
                        tx = current as u16;
                    } else {
                        comstate = ComState::Idle;
                    }
                },
                ComState::Power => {
                    if linkindex == 0 {
                        tx = p.POWER.power.read().bits() as u16;
                    } else {
                        comstate = ComState::Idle;
                    }
                },
                _ => tx = voltage as u16,
            }
            linkindex = linkindex + 1;
            unsafe{ p.COM.tx0.write(|w| w.bits( (tx & 0xFF) as u32 )); }
            unsafe{ p.COM.tx1.write(|w| w.bits( ((tx >> 8) & 0xFF) as u32 )); }
        }
        
    }
}