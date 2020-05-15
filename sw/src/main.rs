#![no_main]
#![no_std]

// note: to get vscode to reload the PAC, do shift-ctrl-p, 'reload window'. developer:Reload window

use core::panic::PanicInfo;
use riscv_rt::entry;

extern crate betrusted_hal;
extern crate volatile;

extern crate wfx_sys;
extern crate wfx_rs;
extern crate wfx_bindings;

extern crate xous_nommu;
use wfx_rs::hal_wf200::wfx_init;
use wfx_rs::hal_wf200::wfx_scan_ongoing;
use wfx_rs::hal_wf200::wfx_start_scan;
use wfx_rs::hal_wf200::wfx_handle_event;
use wfx_rs::hal_wf200::wf200_mutex_get;
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
    LoopTest,
    Error,
}

#[entry]
fn main() -> ! {
    use betrusted_hal::hal_hardi2c::*;
    use betrusted_hal::hal_time::*;
    use betrusted_hal::api_gasgauge::*;
    use betrusted_hal::api_lm3509::*;
//    use betrusted_hal::api_charger::*;    // for EVT
    use betrusted_hal::api_bq25618::*;  // for DVT

    // Initialize the no-MMU version of Xous, which will give us
    // basic access to tasks and interrupts.
    xous_nommu::init();

    let p = betrusted_pac::Peripherals::take().unwrap();
    let mut i2c = Hardi2c::new();

    time_init(&p);

    i2c.i2c_init(CONFIG_CLOCK_FREQUENCY);

    let mut charger: BtCharger = BtCharger::new();

    // this needs to be one of the first things called after I2C comes up
    charger.chg_set_safety(&mut i2c);

    gg_start(&mut i2c);

    charger.chg_set_autoparams(&mut i2c);
    charger.chg_start(&mut i2c);

    use volatile::Volatile;
    let com_ptr: *mut u32 = 0xD000_0000 as *mut u32;
    let com_fifo = com_ptr as *mut Volatile<u32>;
    let com_rd_ptr: *mut u32 = 0xD000_0000 as *mut u32;
    let com_rd = com_rd_ptr as *mut Volatile<u32>;

    unsafe{ (*com_fifo).write(2020); } // load a dummy entry in so we can respond on the first txrx

    let mut last_time : u32 = get_time_ms(&p);
    let mut last_state : bool = false;
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

    let mut start_time: u32 = get_time_ms(&p);
    let mut wifi_ready: bool = false;

    //let mut chg_reset_time: u32 = get_time_ms(&p);
    charger.update_regs(&mut i2c);
    // sprintln!("registers: {:?}", charger);
    let use_wifi: bool = true;
    loop {
        if !use_wifi && (get_time_ms(&p) - start_time > 1500) {
            delay_ms(&p, 250); // force just a delay, so requests queue up
            start_time = get_time_ms(&p);
        }
        // slight delay to allow for wishbone-tool to connect for debuggening
        if (get_time_ms(&p) - start_time > 1500) && !wifi_ready && use_wifi {
            sprintln!("initializing wifi!");
            // delay_ms(&p, 250); // let the message print
            // init the wifi interface
            if wfx_init() == SL_STATUS_OK {
                sprintln!("Wifi ready");
                wifi_ready = true;
            } else {
                sprintln!("Wifi init failed");
            }
            start_time = get_time_ms(&p);
        }
        if wifi_ready && use_wifi {
            if get_time_ms(&p) - start_time > 600_000 {
                sprintln!("starting ssid scan");
                wfx_start_scan();
                start_time = get_time_ms(&p);
            }
        }
        if wfx_rs::hal_wf200::wf200_event_get() && use_wifi {
            // first thing -- clear the event. So that if we get another event
            // while handling this packet, we have a chance of detecting that.
            // we lack mutexes, so we need to think about this behavior very carefully.

            if wf200_mutex_get() { // don't process events while the driver has locked us out
                wfx_rs::hal_wf200::wf200_event_clear();

                // handle the Rx packet
                if wfx_scan_ongoing() {
                    wfx_handle_event();
                }
            }
        }

        // workaround: for some reason the charger is leaving its maintenance state
        //if get_time_ms(&p) - chg_reset_time > 1000 * 60 * 30 {  // every half hour reset the charger
        //    chg_reset_time = get_time_ms(&p);
        //    charger.chg_set_autoparams(&mut i2c);
        //    charger.chg_start(&mut i2c);
        //}

        if get_time_ms(&p) + 10 - last_time > 1000 {
            last_time = get_time_ms(&p);
            if last_state {
                charger.chg_keepalive_ping(&mut i2c);
            } else {
                // once every second run these routines
                voltage = gg_voltage(&mut i2c);
                current = gg_avg_current(&mut i2c);

                // soc turns on automatically if the charger comes up
                if charger.chg_is_charging(&mut i2c) || (p.POWER.stats.read().state().bits() & 0x2 != 0) {
                    soc_on = true;
                    unsafe{ p.POWER.power.write(|w| w.self_().bit(true).soc_on().bit(true).discharge().bit(false).kbdscan().bits(0) ); } // turn off discharge if the soc is up
                }

                if !soc_on {
                    stby_current = gg_avg_current(&mut i2c);
                }

            }
            last_state = ! last_state;
        }

        // monitor the keyboard inputs if the soc is in the off state
        // FIXME: isolate FPGA inputs on powerdown
        if !soc_on {
            if get_time_ms(&p) - pd_time > 2000 { // delay for power-off to settle

                if get_time_ms(&p) - pd_interval > 50 { // every 50ms check key state
                    pd_interval = get_time_ms(&p);

                    // check one key
                    unsafe{ p.POWER.power.write(|w| w.self_().bit(true).discharge().bit(true).soc_on().bit(false).kbdscan().bits(0)); }
                    if p.POWER.stats.read().monkey().bits() == 0 {
                        // power on the SOC
                        unsafe{ p.POWER.power.write(|w| w.self_().bit(true).soc_on().bit(false).kbdscan().bits(0)); } // first disengage discharge
                        soc_on = true;
                        unsafe{ p.POWER.power.write(|w| w.self_().bit(true).soc_on().bit(true).kbdscan().bits(0)); } // then try to power on the SoC
                        pd_time = get_time_ms(&p);
                    }

                    if !soc_on {
                        // turn off scan, revert discharge to true
                        unsafe{ p.POWER.power.write(|w| w.self_().bit(true).discharge().bit(true).soc_on().bit(false).kbdscan().bits(0)); }
                    }
                }
            }
        }

        // p.WIFI.ev_enable.write(|w| unsafe{w.bits(0)} ); // disable wifi interrupts, entering a critical section
        // unsafe{ betrusted_pac::Peripherals::steal().WIFI.ev_pending.write(|w| w.bits(0x1)); }
        while p.COM.status.read().rx_avail().bit_is_set() {
            let rx: u16;
            unsafe{ rx = (*com_rd).read() as u16; }

            let mut tx: u16 = 0;
            match rx {
                0x4000 => {
                    linkindex = 0;
                    comstate = ComState::LoopTest;
                    com_sentinel = 0;
                },
                0x5A00 => { // charging mode
                    charger.chg_start(&mut i2c);
                },
                0x5AFE => { // boost mode
                    charger.chg_boost(&mut i2c);
                },
                0x6800..=0x681F => {
                        let bl_level: u8 = (rx & 0x1F) as u8;
                        backlight.set_brightness(&mut i2c, bl_level);
                    },
                0x7000 => {linkindex = 0; comstate = ComState::GasGauge;},
                0x8000 => {charger.update_regs(&mut i2c);
                    linkindex = 0; comstate = ComState::Stat;},
                0x9000..=0x90FF => {
                    linkindex = 0;
                    comstate = ComState::Power;
                    // ignore rapid, successive power down requests
                    if get_time_ms(&p) - pd_time > 2000 {
                        unsafe{ p.POWER.power.write(|w| w.self_().bit(true).discharge().bit(true).soc_on().bit(false).kbdscan().bits(0)); }
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
                    p.COM.control.write( |w| w.reset().bit(true) ); // reset fifos
                    p.COM.control.write( |w| w.clrerr().bit(true) ); // clear all error flags
                    com_sentinel = com_sentinel + 1;
                    unsafe{ (*com_fifo).write(com_sentinel as u32); }
                    continue;
                }
                _ => {
                    comstate = ComState::Error;
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
                },
                ComState::LoopTest => {
                    tx = (rx & 0xFF) | ((linkindex as u16 & 0xFF) << 8);
                },
                ComState::Error => {
                    tx = 0xEEEE;
                },
                _ => tx = 8888,
            }
            linkindex = linkindex + 1;
            unsafe{ (*com_fifo).write(tx as u32); }
        }
        // pub const WIFI_EVENT_WIRQ: u32 = 0x1;
        // p.WIFI.ev_enable.write(|w| unsafe{w.bits(0x1)} ); // re-enable wifi interrupts

    }
}