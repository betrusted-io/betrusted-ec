#![no_main]
#![no_std]

// note: to get vscode to reload the PAC, do shift-ctrl-p, 'reload window'. developer:Reload window

use core::panic::PanicInfo;
use riscv_rt::entry;

extern crate betrusted_hal;
extern crate utralib;
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
use wfx_rs::hal_wf200::wf200_ssid_get_list;
use wfx_rs::hal_wf200::wf200_ssid_updated;
use wfx_rs::hal_wf200::SsidResult;
use wfx_bindings::*;

use gyro_rs::hal_gyro::BtGyro;

#[macro_use]
mod debug;

const CONFIG_CLOCK_FREQUENCY: u32 = 18_000_000;

// allocate a global, unsafe static string for debug output
#[used]  // This is necessary to keep DBGSTR from being optimized out
static mut DBGSTR: [u32; 4] = [0, 0, 0, 0];

#[panic_handler]
fn panic(_panic: &PanicInfo<'_>) -> ! {
    loop {}
}

use utralib::generated::*;

enum ComState {
    Idle,
    Stat,
    Power,
    GasGauge,
    LoopTest,
    ReadChargeState,
    Error,
    Pass,
    GyroRead,
    PollUsbCc,
    SsidCheck,
    SsidFetch,
}

const POWER_MASK_FPGA_ON: u32 = 0b10;
const POWER_MASK_SHUTDOWN_OK: u32 = 0b01;

pub fn debug_power() {
    let mut power_csr = CSR::new(HW_POWER_BASE as *mut u32);
    sprintln!("power: 0x{:04x}", power_csr.r(utra::power::POWER));
}

fn com_int_handler(_irq_no: usize) {
    let mut com_csr = CSR::new(utra::com::HW_COM_BASE as *mut u32);
    let avail_pending = com_csr.rf(utra::com::EV_PENDING_SPI_AVAIL);

    // handle interrupt here

    com_csr.wfo(utra::com::EV_PENDING_SPI_AVAIL, 1); // clear the pending
}

#[entry]
fn main() -> ! {
    use betrusted_hal::hal_hardi2c::*;
    use betrusted_hal::hal_time::*;
    use betrusted_hal::api_gasgauge::*;
    use betrusted_hal::api_lm3509::*;
    use betrusted_hal::api_bq25618::*;
    use betrusted_hal::api_tusb320::*;

    let mut power_csr = CSR::new(HW_POWER_BASE as *mut u32);
    let mut com_csr = CSR::new(HW_COM_BASE as *mut u32);

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

    let mut usb_cc = BtUsbCc::new();
    usb_cc.init(&mut i2c, &p);

    let mut gyro: BtGyro = BtGyro::new();
    gyro.init();

    use volatile::Volatile;
    let com_ptr: *mut u32 = utralib::HW_COM_MEM as *mut u32;
    let com_fifo = com_ptr as *mut Volatile<u32>;
    let com_rd_ptr: *mut u32 = utralib::HW_COM_MEM as *mut u32;
    let com_rd = com_rd_ptr as *mut Volatile<u32>;

    unsafe{ (*com_fifo).write(2020); } // load a dummy entry in so we can respond on the first txrx

    let mut last_run_time : u32 = get_time_ms(&p);
    let mut loopcounter: u32 = 0; // in seconds, so this will last ~125 years
    let mut voltage : i16 = 0;
    let mut current: i16 = 0;
    let mut stby_current: i16 = 0;
    let mut linkindex : usize = 0;
    let mut comstate: ComState = ComState::Idle;
    let mut pd_loop_timer: u32 = 0;
    let mut pd_discharge_timer: u32 = 0;
    let mut soc_on: bool = true;
    let mut backlight : BtBacklight = BtBacklight::new();
    let mut com_sentinel: u16 = 0;

    backlight.set_brightness(&mut i2c, 0, 0); // make sure the backlight is off on boot

    let mut start_time: u32 = get_time_ms(&p);
    let mut wifi_ready: bool = false;
    let mut ssid_list: [SsidResult; 6] = [SsidResult::default(); 6];

    charger.update_regs(&mut i2c);

    let mut usb_cc_event = false;

    let use_wifi: bool = true;
    let do_power: bool = false;

    xous_nommu::syscalls::sys_interrupt_claim(utra::com::COM_IRQ, com_int_handler).unwrap();
    com_csr.wfo(utra::com::EV_PENDING_SPI_AVAIL, 1); // clear the pending signal just in case
    com_csr.wfo(utra::com::EV_ENABLE_SPI_AVAIL, 1); // enable interrupts on SPI fifo not empty

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
            if get_time_ms(&p) - start_time > 20_000 {
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

        if get_time_ms(&p) - last_run_time > 1000 {
            last_run_time = get_time_ms(&p);
            loopcounter += 1;

            // routine pings & housekeeping
            if loopcounter % 2 == 0 {
                charger.chg_keepalive_ping(&mut i2c);
                if !usb_cc_event {
                    usb_cc_event = usb_cc.check_event(&mut i2c, &p);
                }
            } else {
                voltage = gg_voltage(&mut i2c);
                if soc_on {
                    current = gg_avg_current(&mut i2c);
                } else {
                    // TODO: need more fine control over this
                    // at the moment, system can power on for 1 full second prior to getting this reading
                    stby_current = gg_avg_current(&mut i2c);
                }
            }

            // check if we should turn the SoC on or not
            if charger.chg_is_charging(&mut i2c, false) || (power_csr.rf(utra::power::STATS_STATE) == 1) && do_power {
                soc_on = true;
                sprintln!("charger insert or soc on event!");
                let power =
                    power_csr.ms(utra::power::POWER_SELF, 1)
                    | power_csr.ms(utra::power::POWER_SOC_ON, 1)
                    | power_csr.ms(utra::power::POWER_DISCHARGE, 0);
                power_csr.wo(utra::power::POWER, power); // turn off discharge if the soc is up
            } else if charger.chg_is_charging(&mut i2c, false) {
                soc_on = true;
                sprintln!("charger charging!");
                let power =
                    power_csr.ms(utra::power::POWER_SELF, 1)
                    | power_csr.ms(utra::power::POWER_SOC_ON, 1)
                    | power_csr.ms(utra::power::POWER_DISCHARGE, 0);
                power_csr.wo(utra::power::POWER, power);
            }
        }

        // fast-monitor the keyboard inputs if the soc is in the off state
        if !soc_on {
            if get_time_ms(&p) - pd_discharge_timer < 2000 {
                // wait 2 seconds after PD before checking anything
            } else {
                if get_time_ms(&p) - pd_loop_timer > 50 { // every 50ms check key state
                    pd_loop_timer = get_time_ms(&p);
                    // drive sense for keyboard
                    let power =
                    power_csr.ms(utra::power::POWER_SELF, 1)
                    | power_csr.ms(utra::power::POWER_DISCHARGE, 1)
                    | power_csr.ms(utra::power::POWER_KBDDRIVE, 1);
                    power_csr.wo(utra::power::POWER, power);

                    if power_csr.rf(utra::power::STATS_MONKEY) == 3 { // both keys have to be hit
                        sprintln!("detect power up event!");
                        // power on the SOC
                        let power =
                        power_csr.ms(utra::power::POWER_SELF, 1)
                        | power_csr.ms(utra::power::POWER_SOC_ON, 1);
                        power_csr.wo(utra::power::POWER, power);
                        soc_on = true;

                        debug_power();
                    } else {
                        // re-engage discharge fets, disable keyboard drive
                        let power =
                        power_csr.ms(utra::power::POWER_SELF, 1)
                        | power_csr.ms(utra::power::POWER_KBDDRIVE, 0)
                        | power_csr.ms(utra::power::POWER_DISCHARGE, 1);
                        power_csr.wo(utra::power::POWER, power);
                    }
                }
            }
            debug_power();
        }

        // p.WIFI.ev_enable.write(|w| unsafe{w.bits(0)} ); // disable wifi interrupts, entering a critical section
        // unsafe{ betrusted_pac::Peripherals::steal().WIFI.ev_pending.write(|w| w.bits(0x1)); }
        while p.COM.status.read().rx_avail().bit_is_set() {
            let rx: u16;
            unsafe{ rx = (*com_rd).read() as u16; }

            let mut tx: u16 = 0;
            // first parse rx and try to stuff a tx response as fast as possible
            match rx {
                0x2000 => {
                    linkindex = 0;
                    comstate = ComState::SsidCheck;
                },
                0x2100 => {
                    linkindex = 0;
                    comstate = ComState::SsidFetch;
                },
                0x4000 => {
                    linkindex = 0;
                    comstate = ComState::LoopTest;
                    com_sentinel = 0;
                },
                0x7000 => {linkindex = 0; comstate = ComState::GasGauge;},
                0x8000 => {linkindex = 0; comstate = ComState::Stat;},
                0x9000 => {comstate = ComState::Power;},
                0x9100 => {comstate = ComState::ReadChargeState},
                // 0x9200 shipmode
                // 0xA000 fetch latest gyro XYZ data
                0xA100 => {linkindex = 0; comstate = ComState::GyroRead},
                0xB000 => {linkindex = 0; comstate = ComState::PollUsbCc},
                0xF0F0 => {
                    // this a "read continuation" command, in other words, return read data
                    // based on the current ComState
                },
                0xFFFF => {
                    // reset link command, when received, empty all the FIFOs, and prime Tx with dummy data
                    p.COM.control.write( |w| w.reset().bit(true) ); // reset fifos
                    p.COM.control.write( |w| w.clrerr().bit(true) ); // clear all error flags
                    com_sentinel = com_sentinel + 1;
                    unsafe{ (*com_fifo).write(com_sentinel as u32); }
                    comstate = ComState::Error;
                    continue;
                },
                _ => {
                    comstate = ComState::Pass;
                },
            }

            // these responses should all be "on-hand", e.g. not requiring an I2C transaction to retrieve
            match comstate {
                ComState::SsidCheck => {
                    if wf200_ssid_updated() {
                        tx = 1;
                    } else {
                        tx = 0;
                    }
                },
                ComState::SsidFetch => {
                    if linkindex < 16 * 6 {
                        tx = ssid_list[linkindex / 16].ssid[(linkindex % 16)*2] as u16 |
                        ((ssid_list[linkindex / 16].ssid[(linkindex % 16)*2+1] as u16) << 8);
                    } else {
                        tx = 0;
                        comstate = ComState::Idle;
                    }
                }
                ComState::Stat => {
                    if linkindex == 0 {
                        tx = 0x8888; // first response is just to the initial command
                    } else if linkindex > 0 && linkindex < 0xD {
                        tx = charger.registers[linkindex - 1] as u16;
                    } else if linkindex == 0xE {
                        tx = voltage as u16;
                    } else if linkindex == 0xF {
                        tx = stby_current as u16;
                    } else if linkindex == 0x10 {
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
                ComState::ReadChargeState => {
                    if charger.chg_is_charging(&mut i2c, true) { // use "cached" version so this is safe in a fast loop
                        tx = 1;
                    } else {
                        tx = 0;
                    }
                },
                ComState::GyroRead => {
                    match linkindex {
                        0 => tx = gyro.x,
                        1 => tx = gyro.y,
                        2 => tx = gyro.z,
                        3 => tx = gyro.id as u16,
                        _ => tx = 0xEEEE,
                    }
                },
                ComState::PollUsbCc => {
                    match linkindex {
                        0 => { if usb_cc_event { tx = 1 } else { tx = 0 } usb_cc_event = false; }, // clear the usb_cc_event pending flag as its been checked
                        1 => tx = usb_cc.status[0] as u16,
                        2 => tx = usb_cc.status[1] as u16,
                        3 => tx = usb_cc.status[2] as u16,
                        _ => tx= 0xEEEE,
                    }
                }
                ComState::Error => {
                    tx = 0xEEEE;
                },
                ComState::Pass => {
                    tx = 0x1111;
                }
                _ => tx = 8888,
            }
            linkindex = linkindex + 1;
            unsafe{ (*com_fifo).write(tx as u32); }

            // now that TX has been handled, go deeper into rx codes and run things that can take longer to respond to
            // pub const WIFI_EVENT_WIRQ: u32 = 0x1;
            // p.WIFI.ev_enable.write(|w| unsafe{w.bits(0x1)} ); // re-enable wifi interrupts

            match rx {
                0x2100 => { // ssid fetch
                    if linkindex == 1 { // only grab it on the initial command request
                        ssid_list = wf200_ssid_get_list();
                    }
                }
                0x5A00 => { // charging mode
                    charger.chg_start(&mut i2c);
                },
                0x5ABB => { // boost on
                    charger.chg_boost(&mut i2c);
                },
                0x5AFE => { // boost off
                    charger.chg_boost_off(&mut i2c);
                },
                0x6800..=0x6BFF => {
                        let main_bl_level: u8 = (rx & 0x1F) as u8;
                        let sec_bl_level: u8 = ((rx >> 5) & 0x1F) as u8;
                        backlight.set_brightness(&mut i2c, main_bl_level, sec_bl_level);
                    },
                0x8000 => {charger.update_regs(&mut i2c);},
                0x9000 => {
                    sprintln!("got power down request from soc!");
                    linkindex = 0;
                    // ignore rapid, successive power down requests
                    if get_time_ms(&p) - pd_loop_timer > 1500 {
                        let power =
                        power_csr.ms(utra::power::POWER_SELF, 1)
                        | power_csr.ms(utra::power::POWER_DISCHARGE, 1);
                        power_csr.wo(utra::power::POWER, power);

                        pd_loop_timer = get_time_ms(&p);
                        pd_discharge_timer = get_time_ms(&p);
                        soc_on = false;
                        debug_power();
                    }
                },
                0x9200 => {
                    charger.set_shipmode(&mut i2c);
                    let power =
                    power_csr.ms(utra::power::POWER_SELF, 1)
                    | power_csr.ms(utra::power::POWER_DISCHARGE, 1);
                    power_csr.wo(utra::power::POWER, power);

                    pd_loop_timer = get_time_ms(&p);
                    pd_discharge_timer = get_time_ms(&p);
                    soc_on = false;
                },
                0xA000 => {
                    gyro.update_xyz();
                },
                _ => {},
            }
        }

    }
}