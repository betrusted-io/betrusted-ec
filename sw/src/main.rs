#![no_main]
#![no_std]

use core::panic::PanicInfo;
use riscv_rt::entry;

extern crate betrusted_hal;
extern crate volatile;

extern crate wfx_sys;
extern crate wfx_rs;
extern crate wfx_bindings;

extern crate xous_nommu;
use wfx_rs::hal_wf200::wfx_init;
use wfx_bindings::*;

#[macro_use]
mod debug;

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
    GasGauge,
}

#[entry]
fn main() -> ! {
    use betrusted_hal::hal_hardi2c::*;
    use betrusted_hal::hal_time::*;
    use betrusted_hal::api_gasgauge::*;
    use betrusted_hal::api_charger::*;
    use betrusted_hal::api_lm3509::*;

    // Initialize the no-MMU version of Xous, which will give us
    // basic access to tasks and interrupts.
    xous_nommu::init();

    let p = betrusted_pac::Peripherals::take().unwrap();
    let mut i2c = Hardi2c::new();

    time_init(&p);

    i2c.i2c_init(CONFIG_CLOCK_FREQUENCY);
    // this needs to be one of the first things called after I2C comes up
    chg_set_safety(&mut i2c);

    gg_start(&mut i2c);

    chg_set_autoparams(&mut i2c);
    chg_start(&mut i2c);

    use volatile::Volatile;
    let com_ptr: *mut u32 = 0xD000_0000 as *mut u32; 
    let com_fifo = com_ptr as *mut Volatile<u32>;
    let com_rd_ptr: *mut u32 = 0xD000_0000 as *mut u32;
    let com_rd = com_rd_ptr as *mut Volatile<u32>;

    unsafe{ (*com_fifo).write(2020); } // load a dummy entry in so we can respond on the first txrx
    
    let mut last_time : u32 = get_time_ms(&p);
    let mut last_state : bool = false;
    let mut charger : BtCharger = BtCharger::new();
    let mut voltage : i16 = 0;
    let mut current: i16 = 0;
    let mut stby_current: i16 = 0;
    let mut linkindex : usize = 0;
    let mut comstate: ComState = ComState::Idle;
    let mut pd_time: u32 = 0;
    let mut pd_interval: u32 = 0;
    let mut soc_on: bool = true;
    let mut backlight : BtBacklight = BtBacklight::new();
    let mut com_sentinel: u16 = 0;
    backlight.set_brightness(&mut i2c, 0); // make sure the backlight is off on boot

    sprintln!("hello world!");
    
    let mut start_time: u32 = get_time_ms(&p);
    let mut wifi_ready: bool = false;

    loop { 
        if (get_time_ms(&p) - start_time > 5000) && !wifi_ready {
            sprintln!("initializing wifi!");
            delay_ms(&p, 250); // let the message print
            // init the wifi interface
            if wfx_init() == SL_STATUS_OK {
                sprintln!("Wifi ready");
                wifi_ready = true;
            } else {
                sprintln!("Wifi init failed");
            }
            start_time = get_time_ms(&p);
        }
        if wfx_rs::hal_wf200::wf200_event_get() {
            // first thing -- clear the event. So that if we get another event
            // while handling this packet, we have a chance of detecting that.
            // we lack mutexes, so we need to think about this behavior very carefully.
            wfx_rs::hal_wf200::wf200_event_clear();

            // handle the Rx packet

        }
        if get_time_ms(&p) - last_time > 1000 {
            last_time = get_time_ms(&p);
            if last_state {
                chg_keepalive_ping(&mut i2c);
                charger.update_regs(&mut i2c);
                // sprintln!("registers: {:?}", charger);
            } else {
                // once every second run these routines
                voltage = gg_voltage(&mut i2c);
                current = gg_avg_current(&mut i2c);

                // soc turns on automatically if the charger comes up
                if chg_is_charging(&mut i2c) || (p.POWER.stats.read().state().bits() & 0x2 != 0) {
                    soc_on = true;
                }

                if !soc_on {
                    stby_current = gg_avg_current(&mut i2c);
                }

            }
            last_state = ! last_state;
        }

        // monitor the keyboard inputs if the soc is in the off state
        // FIXME: thar be dragins in the code down here
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

        while p.COM.status.read().rx_avail().bit_is_set() {
            let rx: u16;
            unsafe{ rx = (*com_rd).read() as u16; }

            let mut tx: u16 = 0;
            match rx {
                0x6800..=0x681F => {
                        let bl_level: u8 = (rx & 0x1F) as u8;
                        backlight.set_brightness(&mut i2c, bl_level);
                    },
                0x7000 => {linkindex = 0; comstate = ComState::GasGauge;},
                0x8000 => {linkindex = 0; comstate = ComState::Stat;},
                0x9000..=0x90FF => {
                    linkindex = 0;
                    comstate = ComState::Power;
                    // ignore rapid, successive power down requests
                    if get_time_ms(&p) - pd_time > 2000 {
                        unsafe{ p.POWER.power.write(|w| w.bits(0x5 as u32)); } 
                        pd_time = get_time_ms(&p);
                        pd_interval = get_time_ms(&p);
                        soc_on = false;
                    }
                },
                0xF0F0 => {
                    // this a "read continuation" command, in other words, return read data
                    // based on the current state. Do nothing here, check "comstate".
                }
                0xFFFF => {
                    // reset link command, when received, empty all the FIFOs, and prime Tx with dummy data
                    tx = 0;
                    while p.COM.status.read().rx_avail().bit_is_set() {
                        unsafe{ tx += (*com_rd).read() as u16; }
                    }
                    while !p.COM.status.read().tx_empty().bit_is_set() {
                        p.COM.control.write( |w| w.pump().bit(true));
                    }
                    p.COM.control.write( |w| w.clrerr().bit(true) ); // clear all error flags
                    unsafe { p.COM.control.write( |w| w.bits(0) ); } // reset the bits.
                    com_sentinel = com_sentinel + 1; // for debugging only, right now no output
                    unsafe{ (*com_fifo).write(tx as u32); }
                    continue;
                }
                _ => {
                    tx = rx;
                    unsafe{ (*com_fifo).write(tx as u32); }                    
                    continue;
                },
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
                ComState::GasGauge => {
                    if linkindex == 0 {
                        tx = current as u16;
                    } else if linkindex == 1 {
                        tx = stby_current as u16;
                    } else if linkindex == 2 {
                        tx = voltage as u16;
                    } else if linkindex == 3 {
                        tx = p.POWER.power.read().bits() as u16;
                    } else {
                        comstate = ComState::Idle;
                    }
                }
                _ => tx = 8888,
            }
            linkindex = linkindex + 1;
            unsafe{ (*com_fifo).write(tx as u32); }
        }
        
    }
}