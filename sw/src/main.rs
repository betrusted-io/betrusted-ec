#![no_main]
#![no_std]

// note: to get vscode to reload file, do shift-ctrl-p, 'reload window'. developer:Reload window

extern crate betrusted_hal;
extern crate com_rs;
extern crate utralib;
extern crate volatile;
extern crate wfx_bindings;
extern crate wfx_rs;
extern crate wfx_sys;
extern crate xous_nommu;

use betrusted_hal::api_bq25618::BtCharger;
use betrusted_hal::api_gasgauge::{
    gg_full_capacity, gg_remaining_capacity, gg_set_design_capacity, gg_set_hibernate, gg_start,
    gg_state_of_charge,
};
use betrusted_hal::api_lm3509::BtBacklight;
use betrusted_hal::api_lsm6ds3::Imu;
use betrusted_hal::api_tusb320::BtUsbCc;
//use betrusted_hal::hal_hardi2c::Hardi2c;
use betrusted_hal::hal_i2c::Hardi2c;
use betrusted_hal::hal_time::{
    get_time_ms, get_time_ticks, set_msleep_target_ticks, time_init, TimeMs,
};
use betrusted_hal::mem_locs::*;
use com_rs::serdes::{StringSer, STR_64_U8_SIZE, STR_64_WORDS};
use com_rs::ComState;
use wfx_rs::hal_wf200::WIFI_MTU;
use core::panic::PanicInfo;
use debug;
use debug::{log, loghex, loghexln, logln, LL};
use net::timers::{Countdown, CountdownStatus, Stopwatch};
use riscv_rt::entry;
use utralib::generated::{
    utra, CSR, HW_COM_BASE, HW_CRG_BASE, HW_GIT_BASE, HW_POWER_BASE, HW_TICKTIMER_BASE,
};
use volatile::Volatile;

// Modules from this crate
mod com_bus;
mod power_mgmt;
mod spi;
mod str_buf;
mod uart;
mod wifi;
mod wlan;
use com_bus::{com_rx, com_tx};
use power_mgmt::charger_handler;
use spi::{spi_erase_region, spi_program_page, spi_standby};
use str_buf::StrBuf;
use wlan::WlanState;

// Configure Log Level (used in macro expansions)
const LOG_LEVEL: LL = LL::Debug;

// Constants
const CONFIG_CLOCK_FREQUENCY: u32 = 18_000_000;

/// Infinite loop panic handler (TODO: fix this to use less power)
#[panic_handler]
fn panic(_panic: &PanicInfo<'_>) -> ! {
    loop {}
}

fn ticktimer_int_handler(_irq_no: usize) {
    let mut ticktimer_csr = CSR::new(HW_TICKTIMER_BASE as *mut u32);
    let mut crg_csr = CSR::new(HW_CRG_BASE as *mut u32);
    let mut power_csr = CSR::new(HW_POWER_BASE as *mut u32);

    // disarm the watchdog
    crg_csr.wfo(utra::crg::WATCHDOG_RESET_CODE, 0x600d);
    crg_csr.wfo(utra::crg::WATCHDOG_RESET_CODE, 0xc0de);

    // fast-monitor the keyboard wakeup inputs if the soc is in the off state
    if (power_csr.rf(utra::power::POWER_SOC_ON) == 0)
        && (power_csr.rf(utra::power::STATS_STATE) == 0)
    {
        // drive sense for keyboard
        let power =
            power_csr.ms(utra::power::POWER_SELF, 1) | power_csr.ms(utra::power::POWER_KBDDRIVE, 1);
        power_csr.wo(utra::power::POWER, power);

        if power_csr.rf(utra::power::STATS_MONKEY) == 3 {
            // both keys have to be hit
            // power on the SOC
            let power = power_csr.ms(utra::power::POWER_SELF, 1)
                | power_csr.ms(utra::power::POWER_SOC_ON, 1);
            power_csr.wo(utra::power::POWER, power);
        } else {
            let power = power_csr.ms(utra::power::POWER_SELF, 1)
                | power_csr.ms(utra::power::POWER_KBDDRIVE, 0);
            power_csr.wo(utra::power::POWER, power);
        }
    }

    set_msleep_target_ticks(50); // resetting this will also clear the alarm

    ticktimer_csr.wfo(utra::ticktimer::EV_PENDING_ALARM, 1);
}

/// This logs a time comparison of many short shifts vs same number of long shifts.
/// The point is to verify that the CPU is using single cycle shifts.
fn shift_speed_test() {
    let count = 50_000;
    let mut a: u32 = wfx_rs::hal_wf200::net_prng_rand();
    let mut b: u32 = a;
    let mut sw = Stopwatch::new();
    sw.start();
    for _ in 0..count {
        a = (a >> 1) ^ (a << 3) ^ (a << 5);
    }
    // Time for short shifts (distance 8 left)
    let short_shift_ms = sw.elapsed_ms().unwrap_or(0);
    sw.start();
    for _ in 0..count {
        b = (b >> 1) ^ (b << 30) ^ (b << 23);
    }
    // Time for long shifts (distance 53 left)
    let long_shift_ms = sw.elapsed_ms().unwrap_or(0);
    let x = (a ^ b) & 15;
    loghex!(LL::Debug, "ShiftSpeed _:", x);
    loghex!(LL::Debug, ", short_shift:", short_shift_ms);
    loghexln!(LL::Debug, ", long_shift:", long_shift_ms);
}

fn stack_check() {
    // check the stack usage
    let stack: &[u32] = unsafe{core::slice::from_raw_parts(STACK_END as *const u32, (STACK_LEN as usize / core::mem::size_of::<u32>()) as usize)};
    let mut unused_stack_words = 0;
    for &word in stack.iter() {
        if word != STACK_CANARY {
            break;
        }
        unused_stack_words += 1;
    }
    logln!(LL::Debug, "{} bytes used of {}", STACK_LEN - unused_stack_words, STACK_LEN);
}

#[entry]
fn main() -> ! {
    logln!(LL::Info, "\r\n====UP5K==0A");
    let mut com_csr = CSR::new(HW_COM_BASE as *mut u32);
    let mut crg_csr = CSR::new(HW_CRG_BASE as *mut u32);
    let mut ticktimer_csr = CSR::new(HW_TICKTIMER_BASE as *mut u32);
    let git_csr = CSR::new(HW_GIT_BASE as *mut u32);
    let mut uart_state: uart::RxState = uart::RxState::BypassOnAwaitA;

    let com_rd_ptr: *mut u32 = utralib::HW_COM_MEM as *mut u32;
    let com_rd = com_rd_ptr as *mut Volatile<u32>;

    let mut loopcounter: u32 = 0; // in seconds, so this will last ~125 years
    let mut pd_loop_timer: u32 = 0;
    let mut soc_off_delay_timer: u32 = 0;

    let mut i2c = Hardi2c::new();
    let mut hw = power_mgmt::PowerHardware {
        power_csr: CSR::new(HW_POWER_BASE as *mut u32),
        charger: BtCharger::new(),
        usb_cc: BtUsbCc::new(),
        backlight: BtBacklight::new(),
    };
    let mut pow = power_mgmt::PowerState {
        voltage: 0,
        last_voltage: 0,
        current: 0,
        stby_current: 0,
        soc_was_on: hw.power_csr.rf(utra::power::STATS_STATE) == 1,
        battery_panic: false,
        voltage_glitch: false,
        usb_cc_event: false,
    };
    let mut last_run_time: u32;
    let mut com_sentinel: u16 = 0; // for link debugging mostly
    let mut flash_update_lock = false;

    let mut use_wifi: bool = true;
    let mut wifi_ready: bool = false;

    // State vars for WPA2 auth credentials for Wifi AP
    let mut wlan_state = WlanState::new();

    // Initialize the no-MMU version of 'Xous' (an extremely old branch of it), which will give us
    // basic access to tasks and interrupts.
    logln!(LL::Trace, "pre-nommu");
    xous_nommu::init();

    time_init();
    logln!(LL::Debug, "time");
    let mut uptime = Stopwatch::new();
    uptime.start();
    last_run_time = get_time_ms();
    const DHCP_POLL_MS: u32 = 101;
    let mut dhcp_oneshot = Countdown::new();

    logln!(LL::Debug, "i2c...");
    i2c.i2c_init(CONFIG_CLOCK_FREQUENCY);
    // this needs to be one of the first things called after I2C comes up
    hw.charger.chg_set_safety(&mut i2c);
    gg_start(&mut i2c);
    hw.charger.chg_set_autoparams(&mut i2c);
    hw.charger.chg_start(&mut i2c);
    let tusb320_rev = hw.usb_cc.init(&mut i2c);
    loghexln!(LL::Debug, "tusb320_rev ", tusb320_rev);
    // Initialize the IMU, note special handling for debug logging of init result
    let mut tap_check_phase: u32 = 0;
    match Imu::init(&mut i2c) {
        Ok(who_am_i_reg) => loghexln!(LL::Debug, "ImuInitOk ", who_am_i_reg), // Should be 0x6A
        Err(n) => loghexln!(LL::Debug, "ImuInitErr ", n),
    }
    // make sure the backlight is off on boot
    hw.backlight.set_brightness(&mut i2c, 0, 0);
    hw.charger.update_regs(&mut i2c);
    logln!(LL::Debug, "...i2c OK");

    spi_standby(); // make sure the OE's are off, no spurious power consumption

    let _ = xous_nommu::syscalls::sys_interrupt_claim(
        utra::ticktimer::TICKTIMER_IRQ,
        ticktimer_int_handler,
    );
    set_msleep_target_ticks(50);
    ticktimer_csr.wfo(utra::ticktimer::EV_PENDING_ALARM, 1); // clear the pending signal just in case
    ticktimer_csr.wfo(utra::ticktimer::EV_ENABLE_ALARM, 1); // enable the interrupt

    logln!(LL::Warn, "**WATCHDOG ON**");
    crg_csr.wfo(utra::crg::WATCHDOG_ENABLE, 1); // 1 = enable the watchdog reset

    // Drain the UART RX buffer
    uart::drain_rx_buf();

    // Reset & Init WF200 before starting the main loop
    if use_wifi {
        logln!(LL::Info, "wifi...");
        wifi::wf200_reset_and_init(&mut use_wifi, &mut wifi_ready);
    } else {
        wifi_ready = false;
        wifi::wf200_reset_hold();
        logln!(LL::Info, "wifi off: holding reset");
    }

    // interrupt manager for COM interface
    let mut com_int_mgr = com_bus::ComInterrupts::new();
    let mut was_connected = false;
    let mut was_scanning = false;
    let mut tx_errs: u32 = 0;

    //////////////////////// MAIN LOOP ------------------
    logln!(LL::Info, "main loop");
    loop {
        if !flash_update_lock {
            //////////////////////// WIFI HANDLER BLOCK ---------
            if use_wifi && wifi_ready {
                wifi::handle_event();
                // update interrupt vectors
                let scanning = wifi::ssid_scan_in_progress();
                if was_scanning && !scanning { // trigger interrupt when scan is done
                    com_int_mgr.set_ssid_update();
                }
                was_scanning = scanning;

                if wfx_rs::hal_wf200::new_pending() {
                    com_int_mgr.set_rx_ready(wfx_rs::hal_wf200::get_packet_len());
                } else if wfx_rs::hal_wf200::was_dropped() {
                    com_int_mgr.set_rx_error();
                }
                // Clock the DHCP state machine using its oneshot countdown
                // timer for rate limiting
                match dhcp_oneshot.status() {
                    CountdownStatus::NotStarted => dhcp_oneshot.start(DHCP_POLL_MS),
                    CountdownStatus::NotDone => (),
                    CountdownStatus::Done => {
                        wifi::dhcp_clock_state_machine();
                        dhcp_oneshot.start(DHCP_POLL_MS);

                        // fire an interrupt whenever we enter or leave the connected state
                        let connected =
                            (wfx_rs::hal_wf200::get_status() == wfx_rs::hal_wf200::State::Connected) &&
                            (wfx_rs::hal_wf200::dhcp_get_state() == net::dhcp::State::Bound);
                        if connected != was_connected {
                            com_int_mgr.set_ipconf_update();
                            was_connected = connected;
                        }
                    }
                }
            }
            //////////////////////// ---------------------------

            //////////////////////// CHARGER HANDLER BLOCK -----
            charger_handler(
                &mut hw,
                &mut i2c,
                &mut last_run_time,
                &mut loopcounter,
                &mut pd_loop_timer,
                &mut pow,
            );
            //////////////////////// ---------------------------

            //////////////////////// IMU TAP HANDLER BLOCK --------
            if tap_check_phase == 1 {
                // Clear any pending out of phase latched tap interrupt
                // TODO: Tune the tap timing parameters or otherwise find a better way to debounce this
                let _ = Imu::get_single_tap(&mut i2c);
            }
            if tap_check_phase == 0 {
                if Ok(true) == Imu::get_single_tap(&mut i2c) {
                    logln!(LL::Debug, "ImuTap");
                    tap_check_phase = 1000;
                    // Log packet filter stats, etc. when tap is detected
                    wfx_rs::hal_wf200::log_net_state();
                }
            } else {
                tap_check_phase = tap_check_phase.saturating_sub(1);
            }
            //////////////////////// ------------------------------

            ///////////////////////////// DEBUG UART RX HANDLER BLOCK ----------
            // Uart starts in bypass mode, so this won't start returning bytes
            // until after it sees the "AT\n" wake sequence (or "AT\r")
            let mut show_help = false;
            if let Some(b) = uart::rx_byte(&mut uart_state) {
                match b {
                    0x1B => {
                        // In case of ANSI escape sequences (arrow keys, etc.) turn UART bypass mode
                        // on to avoid the hassle of having to parse the escape sequences or deal
                        // with whatever unintended commands they might accidentally trigger
                        uart_state = uart::RxState::BypassOnAwaitA;
                        logln!(LL::Debug, "UartRx off");
                    }
                    b'h' | b'H' | b'?' => show_help = true,
                    b'1' => wfx_rs::hal_wf200::log_net_state(),
                    b'2' => shift_speed_test(),
                    b'3' => match uptime.elapsed_ms() {
                        Ok(ms) => loghexln!(LL::Debug, "UptimeMs ", ms),
                        Err(_) => logln!(LL::Debug, "UptimeMsErr"),
                    },
                    b'4' => match uptime.elapsed_s() {
                        Ok(s) => loghexln!(LL::Debug, "UptimeS ", s),
                        Err(_) => logln!(LL::Debug, "UptimeSErr"),
                    },
                    b'5' => {
                        let now = TimeMs::now();
                        loghex!(LL::Debug, "NowMs ", now.ms_high_word());
                        loghexln!(LL::Debug, " ", now.ms_low_word());
                    },
                    b'6' => stack_check(),
                    _ => (),
                }
            } else if uart_state == uart::RxState::Waking {
                logln!(LL::Debug, "UartRx on");
                uart_state = uart::RxState::BypassOff;
                show_help = true;
            }
            if show_help {
                log!(
                    LL::Debug,
                    concat!(
                        "UartRx Help:\r\n",
                        " h => Help\r\n",
                        " 1 => Network stats\r\n",
                        " 2 => Shift speed test\r\n",
                        " 3 => Uptime ms\r\n",
                        " 4 => Uptime s\r\n",
                        " 5 => Now ms\r\n",
                        " 6 => Peak stack usage\r\n",
                    )
                );
            }
            ///////////////////////////// --------------------------------------
        }

        //////////////////////// COM HANDLER BLOCK ---------
        if hw.power_csr.rf(utra::power::STATS_STATE) == 0 {
            com_csr.wfo(utra::com::CONTROL_RESET, 1); // reset fifos
            com_csr.wfo(utra::com::CONTROL_CLRERR, 1); // clear all error flags
            soc_off_delay_timer = get_time_ms();
            continue;
        } else {
            if get_time_ms() < soc_off_delay_timer + 100 {
                // assert reset slightly after the SoC comes up, to throw away any power-on transition noise
                com_csr.wfo(utra::com::CONTROL_RESET, 1);
                com_csr.wfo(utra::com::CONTROL_CLRERR, 1);
                continue;
            }
        }
        while com_csr.rf(utra::com::STATUS_RX_AVAIL) == 1 {
            // We know the SoC is alive, so let it control its own power state
            hw.power_csr.rmwf(utra::power::POWER_SOC_ON, 0);
            // note: this line is occasionally re-asserted whenever the charger is detected as present

            let rx: u16;
            unsafe {
                rx = (*com_rd).read() as u16;
            }
            loghexln!(LL::Trace, "rx: ", rx);

            if rx == ComState::SSID_CHECK.verb {
                logln!(LL::Debug, "CSsidChk");
                // TODO: Get rid of this when the ssid scan shellchat command is revised.
                match wifi::ssid_scan_in_progress() {
                    true => com_tx(0),
                    false => com_tx(1),
                }
            } else if rx == ComState::SSID_FETCH.verb {
                logln!(LL::Debug, "CSsidFetch...");
                let mut ssid_list: [[u8; 32]; wifi::SSID_ARRAY_SIZE] =
                    [[0; 32]; wifi::SSID_ARRAY_SIZE];
                wifi::ssid_get_list(&mut ssid_list);
                // This gets consumed by xous-core/services/com/main.rs::xmain() in the match branch for
                // `Some(Opcode::SsidFetchAsString) => {...}` which is expecting to unpack an array of six
                // SSIDs of 32 bytes each in the format of 6 * (16 * u16 COM bus words). That code divides
                // its RX buffer into 6 * (32 byte) arrays. It expects unused characters to be nulls.
                for i in 0..wifi::SSID_ARRAY_SIZE {
                    for j in 0..16 {
                        let n = j << 1;
                        let lsb = ssid_list[i][n] as u16;
                        let msb = ssid_list[i][n + 1] as u16;
                        com_tx(lsb | (msb << 8));
                    }
                }
                {
                    // Debug dump the list of SSIDs
                    for i in 0..wifi::SSID_ARRAY_SIZE {
                        for j in 0..32 {
                            match ssid_list[i][j] as u8 {
                                0 => log!(LL::Debug, "."),
                                c => log!(LL::Debug, "{}", c as char),
                            }
                        }
                        log!(LL::Debug, "\r\n");
                    }
                    logln!(LL::Debug, "...fetchDone");
                }
            } else if rx == ComState::LOOP_TEST.verb {
                logln!(LL::Debug, "CLoop");
                com_tx((rx & 0xFF) | ((com_sentinel as u16 & 0xFF) << 8));
                com_sentinel += 1;
            } else if rx == ComState::GAS_GAUGE.verb {
                logln!(LL::Trace, "CGg"); // This gets polled frequently
                com_tx(pow.current as u16);
                com_tx(pow.stby_current as u16);
                com_tx(pow.voltage as u16);
                com_tx(hw.power_csr.r(utra::power::POWER) as u16);
            } else if rx == ComState::GG_FACTORY_CAPACITY.verb {
                logln!(LL::Debug, "CGgFacCap");
                let mut error = false;
                let mut capacity: u16 = 1100;
                match com_rx(250) {
                    Ok(result) => capacity = result,
                    _ => error = true,
                }
                if !error {
                    // some manual "sanity checks" so we really don't bork the
                    // gas guage in case of a protocol error
                    if capacity >= 1900 {
                        capacity = 1100;
                    }
                    if capacity <= 600 {
                        capacity = 1100;
                    }
                    let old_capacity = gg_set_design_capacity(&mut i2c, Some(capacity));
                    com_tx(old_capacity);
                } else {
                    com_tx(ComState::ERROR.verb); // return an erroneous former capacity
                }
            } else if rx == ComState::GG_GET_CAPACITY.verb {
                logln!(LL::Debug, "CGgCap");
                let old_capacity = gg_set_design_capacity(&mut i2c, None);
                com_tx(old_capacity);
            } else if rx == ComState::GG_SOC.verb {
                logln!(LL::Trace, "CGgSoc"); // This gets polled frequently
                com_tx(gg_state_of_charge(&mut i2c) as u16);
            } else if rx == ComState::GG_REMAINING.verb {
                logln!(LL::Trace, "CGgRem"); // This gets polled frequently
                com_tx(gg_remaining_capacity(&mut i2c) as u16);
            } else if rx == ComState::GG_FULL_CAPACITY.verb {
                logln!(LL::Debug, "CGgFullCap");
                com_tx(gg_full_capacity(&mut i2c) as u16);
            } else if rx == ComState::GG_DEBUG.verb {
                logln!(LL::Debug, "CGgDebug");
                if pow.voltage_glitch {
                    com_tx(1);
                } else {
                    com_tx(0);
                }
                pow.voltage_glitch = false;
            } else if rx == ComState::STAT.verb {
                logln!(LL::Debug, "CStat");
                com_tx(0x8888); // first is just a response to the initial command
                hw.charger.update_regs(&mut i2c);
                for i in 0..0xC {
                    com_tx(hw.charger.registers[i] as u16);
                }
                com_tx(pow.voltage as u16);
                com_tx(pow.stby_current as u16);
                com_tx(pow.current as u16);
            } else if rx == ComState::POWER_OFF.verb {
                com_tx(hw.power_csr.r(utra::power::POWER) as u16);
                // ignore rapid, successive power down requests
                hw.backlight.set_brightness(&mut i2c, 0, 0); // make sure the backlight is off
                if get_time_ms() - pd_loop_timer > 1500 {
                    hw.power_csr.wfo(utra::power::POWER_SELF, 1); // only leave myself on, turn off everything else
                    pd_loop_timer = get_time_ms();
                }
            } else if rx == ComState::POWER_SHIPMODE.verb {
                hw.backlight.set_brightness(&mut i2c, 0, 0); // make sure the backlight is off
                hw.charger.set_shipmode(&mut i2c);
                gg_set_hibernate(&mut i2c);
                hw.power_csr.wfo(utra::power::POWER_SELF, 1); // only leave myself on, turn off everything else
                pd_loop_timer = get_time_ms();
            } else if rx == ComState::POWER_CHARGER_STATE.verb {
                logln!(LL::Debug, "CPowChgState");
                if hw.charger.chg_is_charging(&mut i2c, false) {
                    com_tx(1);
                } else {
                    com_tx(0);
                }
            } else if rx == ComState::GYRO_UPDATE.verb {
                // TODO: deprecate this because a) it's a NOP, and b) "gyro" is inaccurate
                logln!(LL::Debug, "CGyroUp");
            } else if rx == ComState::GYRO_READ.verb {
                // TODO: Deprecate this verb and replace with something related to accelerometer
                logln!(LL::Debug, "CGyroRd");
                let x = Imu::get_accel_x(&mut i2c);
                let y = Imu::get_accel_y(&mut i2c);
                let z = Imu::get_accel_z(&mut i2c);
                let id = Imu::get_who_am_i(&mut i2c);
                com_tx(x.unwrap_or(0));
                com_tx(y.unwrap_or(0));
                com_tx(z.unwrap_or(0));
                com_tx(id.unwrap_or(0) as u16);
            } else if rx == ComState::POLL_USB_CC.verb {
                logln!(LL::Debug, "CPollUsbCC");
                if pow.usb_cc_event {
                    com_tx(1)
                } else {
                    com_tx(0)
                }
                pow.usb_cc_event = false; // clear the usb_cc_event pending flag as its been checked
                for i in 0..3 {
                    com_tx(hw.usb_cc.status[i] as u16);
                }
                com_tx(tusb320_rev as u16);
            } else if rx == ComState::CHG_START.verb {
                logln!(LL::Debug, "CChgStart");
                // charging mode
                hw.charger.chg_start(&mut i2c);
            } else if rx == ComState::CHG_BOOST_ON.verb {
                logln!(LL::Debug, "CBoost1");
                // boost on
                hw.charger.chg_boost(&mut i2c);
            } else if rx == ComState::CHG_BOOST_OFF.verb {
                // boost off
                hw.charger.chg_boost_off(&mut i2c);
                logln!(LL::Debug, "CBoost0");
            } else if rx >= ComState::BL_START.verb && rx <= ComState::BL_END.verb {
                logln!(LL::Debug, "CBklt");
                let main_bl_level: u8 = (rx & 0x1F) as u8;
                let sec_bl_level: u8 = ((rx >> 5) & 0x1F) as u8;
                hw.backlight
                    .set_brightness(&mut i2c, main_bl_level, sec_bl_level);
            } else if rx == ComState::LINK_READ.verb {
                // this a "read continuation" command, in other words, return read data
                // based on the current ComState
                logln!(LL::Trace, "CRL");
            } else if rx == ComState::LINK_SYNC.verb {
                logln!(LL::Debug, "CLSync");
                // sync link command, when received, empty all the FIFOs, and prime Tx with dummy data
                com_csr.wfo(utra::com::CONTROL_RESET, 1); // reset fifos
                com_csr.wfo(utra::com::CONTROL_CLRERR, 1); // clear all error flags
            } else if rx == ComState::FLASH_ERASE.verb {
                let mut error = false;
                let mut address: u32 = 0;
                let mut len: u32 = 0;
                // receive address in "network order" (big endian)
                match com_rx(100) {
                    Ok(result) => address = (result as u32) << 16,
                    _ => error = true,
                }
                match com_rx(100) {
                    Ok(result) => address |= (result as u32) & 0xFFFF,
                    _ => error = true,
                }
                // receive len, in bytes
                match com_rx(100) {
                    Ok(result) => len = (result as u32) << 16,
                    _ => error = true,
                }
                match com_rx(100) {
                    Ok(result) => len |= (result as u32) & 0xFFFF,
                    _ => error = true,
                }
                if !error {
                    logln!(LL::Debug, "Erasing {} bytes from 0x{:08x}", len, address);
                    spi_erase_region(address, len);
                }
            } else if rx == ComState::FLASH_PP.verb {
                let mut error = false;
                let mut address: u32 = 0;
                let mut page: [u8; 256] = [0; 256];
                // receive address in "network order" (big endian)
                match com_rx(100) {
                    Ok(result) => address = (result as u32) << 16,
                    _ => error = true,
                }
                match com_rx(100) {
                    Ok(result) => address |= (result as u32) & 0xFFFF,
                    _ => error = true,
                }
                for i in 0..128 {
                    match com_rx(200) {
                        Ok(result) => {
                            let b = result.to_le_bytes();
                            page[i * 2] = b[0];
                            page[i * 2 + 1] = b[1];
                        }
                        _ => error = true,
                    }
                }
                if !error {
                    // logln!(LL::Debug, "Programming 256 bytes to 0x{:08x}", address);
                    spi_program_page(address, &mut page);
                }
            } else if rx == ComState::FLASH_LOCK.verb {
                flash_update_lock = true;
                wifi::wf200_irq_disable();
            } else if rx == ComState::FLASH_UNLOCK.verb {
                flash_update_lock = false;
                wifi::wf200_irq_enable();
            } else if rx == ComState::FLASH_WAITACK.verb {
                com_tx(ComState::FLASH_ACK.verb);
            } else if rx == ComState::WFX_RXSTAT_GET.verb {
                logln!(LL::Debug, "CWfxRXStat!!!");
                // TODO: determine if this verb can be removed. It appears unused by Xous.
                // Send null response of the previously implemented 376 byte size
                for _ in 0..(376 / 2) {
                    com_tx(0 as u16);
                }
            } else if rx == ComState::WFX_PDS_LINE_SET.verb {
                logln!(LL::Debug, "CWfxPdsSet...");
                // set one line of the PDS record (up to 256 bytes length)
                let mut error = false;
                let mut pds_data: [u8; 256] = [0; 256];
                let mut pds_length: u16 = 0;
                match com_rx(500) {
                    Ok(result) => pds_length = result,
                    _ => error = true,
                }
                if pds_length >= 256 {
                    // length is in BYTES not words
                    error = true;
                }
                // even if length error, do receive, because we have to clear the rx queue for proper operation
                for i in 0..128 as usize {
                    // ALWAYS expect 128 pds data elements, even if length < 256
                    match com_rx(500) {
                        Ok(result) => {
                            let b = result.to_le_bytes();
                            pds_data[i * 2] = b[0];
                            pds_data[i * 2 + 1] = b[1];
                        }
                        _ => error = true,
                    }
                }
                if !error {
                    wifi::send_pds(pds_data, pds_length);
                }
                com_csr.wfo(utra::com::CONTROL_RESET, 1); // reset fifos
                com_csr.wfo(utra::com::CONTROL_CLRERR, 1); // clear all error flags
                logln!(LL::Debug, "...PdsDone");
            } else if rx == ComState::WFX_FW_REV_GET.verb {
                logln!(LL::Debug, "CWfxFwRev");
                com_tx(wifi::fw_major() as u16);
                com_tx(wifi::fw_minor() as u16);
                com_tx(wifi::fw_build() as u16);
            } else if rx == ComState::EC_GIT_REV.verb {
                logln!(LL::Debug, "CECGitRev");
                com_tx((git_csr.rf(utra::git::GITREV_GITREV) >> 16) as u16);
                com_tx((git_csr.rf(utra::git::GITREV_GITREV) & 0xFFFF) as u16);
                com_tx(git_csr.rf(utra::git::DIRTY_DIRTY) as u16);
            } else if rx == ComState::WF200_RESET.verb {
                log!(LL::Debug, "CWF200Reset ");
                match com_rx(250) {
                    Ok(result) => {
                        if result == 0 {
                            logln!(LL::Debug, "momentary");
                            wifi::wf200_reset_and_init(&mut use_wifi, &mut wifi_ready);
                        } else {
                            logln!(LL::Debug, "hold");
                            wifi_ready = false;
                            use_wifi = false;
                            wifi::wf200_reset_hold();
                        }
                    }
                    _ => {
                        // default to a normal reset
                        logln!(LL::Debug, "default");
                        wifi::wf200_reset_and_init(&mut use_wifi, &mut wifi_ready);
                    }
                }
            } else if rx == ComState::UPTIME.verb {
                log!(LL::Debug, "CUptime");
                let mut time = get_time_ticks();
                for _ in 0..4 {
                    com_tx(time as u16);
                    time >>= 16;
                }
            } else if rx == ComState::TRNG_SEED.verb {
                logln!(LL::Debug, "CTrngSeed");
                let mut entropy: [u16; 8] = [0; 8];
                let mut error = false;
                for e in entropy.iter_mut() {
                    match com_rx(200) {
                        Ok(result) => {
                            *e = result;
                        }
                        _ => error = true,
                    }
                }
                if !error {
                    wfx_rs::hal_wf200::reseed_net_prng(&entropy);
                } else {
                    logln!(LL::Debug, "CTrngSeedErr");
                }
            } else if rx == ComState::SSID_SCAN_ON.verb {
                logln!(LL::Debug, "CSsidScan1");
                wifi::start_scan(); // turn this off for FCC testing
            } else if rx == ComState::SSID_SCAN_OFF.verb {
                logln!(LL::Debug, "CSssidScan0");
                // TODO: Get rid of this when the ssid scan shellchat command is revised.
                // This is a NOP because the WF200 scan ends on its own
            } else if rx == ComState::WLAN_ON.verb {
                logln!(LL::Debug, "CWlanOn");
                if !wifi_ready {
                    wifi::wf200_reset_and_init(&mut use_wifi, &mut wifi_ready);
                }
            } else if rx == ComState::WLAN_OFF.verb {
                logln!(LL::Debug, "CWlanOff");
                // TODO: Make graceful shutdown procedure instead of this immediate reset
                wifi_ready = false;
                wifi::wf200_reset_hold();
                logln!(LL::Debug, "holding WF200 reset")
            } else if rx == ComState::WLAN_SET_SSID.verb {
                logln!(LL::Debug, "CWlanSetS");
                match wlan::set_ssid(&mut wlan_state) {
                    #[allow(unused_variables)]
                    Ok(ssid) => logln!(LL::Debug, "ssid = {}", ssid),
                    _ => logln!(LL::Debug, "set_ssid fail"),
                };
            } else if rx == ComState::WLAN_SET_PASS.verb {
                logln!(LL::Debug, "CWlanSetP");
                match wlan::set_pass(&mut wlan_state) {
                    Ok(_) => logln!(LL::Debug, "SetPassOk"),
                    _ => logln!(LL::Debug, "SetPassFail"),
                };
            } else if rx == ComState::WLAN_JOIN.verb {
                logln!(LL::Debug, "CWlanJoin");
                wifi::ap_join_wpa2(&wlan_state);
            } else if rx == ComState::WLAN_LEAVE.verb {
                logln!(LL::Debug, "CWlanLeave");
                wifi::ap_leave();
            } else if rx == ComState::WLAN_STATUS.verb {
                logln!(LL::Debug, "CWStatus");
                let mut buf = StrBuf::<STR_64_U8_SIZE>::new();
                wifi::append_status_str(&mut buf, &wlan_state);
                let mut error = true;
                if let Ok(status) = buf.as_str() {
                    let mut str_ser = StringSer::<STR_64_WORDS>::new();
                    if let Ok(tx) = str_ser.encode(&status) {
                        for w in tx.iter() {
                            com_tx(*w);
                        }
                        error = false;
                    }
                }
                if error {
                    // Even if something went wrong with the string encoding, still need to send the
                    // proper number of response words over the COM bus to maintain protocol sync.
                    logln!(LL::Debug, "CWStatusErr");
                    for _ in 0..STR_64_WORDS {
                        com_tx(0);
                    }
                }
            } else if rx == ComState::WLAN_GET_IPV4_CONF.verb {
                logln!(LL::Debug, "CWIpConf");
                let conf = wfx_rs::hal_wf200::com_ipv4_config().encode_u16();
                for &w in conf.iter() {
                    com_tx(w);
                }
            } else if rx == ComState::LINK_GET_INTMASK.verb {
                logln!(LL::Debug, "CLGetIMsk");
                com_tx(com_int_mgr.get_mask());
            } else if rx == ComState::LINK_SET_INTMASK.verb {
                logln!(LL::Debug, "CLSetIMsk");
                match com_rx(500) {
                    Ok(result) => {
                        com_int_mgr.set_mask(result)
                    }
                    _ => (),                }
            } else if rx == ComState::LINK_ACK_INTERRUPT.verb {
                logln!(LL::Debug, "CLAckInt");
                match com_rx(500) {
                    Ok(result) => {
                        com_int_mgr.ack(result)
                    }
                    _ => (),
                }
            } else if rx == ComState::LINK_GET_INTERRUPT.verb {
                logln!(LL::Debug, "CLGetInt");
                let int_vect = com_int_mgr.get_state();
                for &w in int_vect.iter() {
                    com_tx(w);
                }
            } else if rx == ComState::WLAN_GET_ERRCOUNTS.verb {
                logln!(LL::Debug, "CWGetErrs");
                com_tx(tx_errs as u16);
                com_tx((tx_errs >> 16) as u16);
                let drops = wfx_rs::hal_wf200::get_packets_dropped();
                com_tx(drops as u16);
                com_tx((drops >> 16) as u16);
            } else if rx >= ComState::NET_FRAME_FETCH_0.verb && rx <= ComState::NET_FRAME_FETCH_7FF.verb {
                logln!(LL::Debug, "CLNetFetch");
                let expected_bytes = rx & 0x7FF;
                let expected_words =
                    if expected_bytes % 2 == 0 {
                        expected_bytes / 2
                    } else {
                        expected_bytes / 2 + 1
                    };
                let packet = wfx_rs::hal_wf200::get_packet_data();
                let packet_words =
                    if packet.len() % 2 == 0 {
                        packet.len() / 2
                    } else {
                        packet.len() / 2 + 1
                    };
                if expected_words != packet_words as u16 {
                    // flag an error, but we still need to stuff the FIFO with something to avoid a link error
                    logln!(LL::Error, "CLNetFetch: len mismatch");
                }
                let mut words_sent = 0;
                while words_sent < packet_words {
                    // use MSB order
                    if words_sent * 2 <= packet.len() - 2 {
                        com_tx(
                            ((packet[(words_sent * 2) as usize] as u16) << 8) |
                            packet[(words_sent * 2) as usize + 1] as u16
                        );
                    } else if words_sent * 2 < packet.len() {
                        com_tx(
                            ((packet[(words_sent * 2) as usize] as u16) << 8) |
                            0x00
                        )
                    } else {
                        com_tx(0xDEAD);
                    }
                    words_sent += 1;
                }
                com_int_mgr.ack_rx_ready();
            } else if rx >= ComState::NET_FRAME_SEND_0.verb && rx <= ComState::NET_FRAME_SEND_7FF.verb {
                logln!(LL::Debug, "CLNetSend");
                /*
                    Code usage note: making this array 1500 bytes causes 4.2k of code to be generated.
                    Ironically, if the array is 2048 bytes, the code size is smaller. There is something
                    weird about the array accessor code such that non-power-of-2 arrays pull in a lot more code.
                */
                let mut txbuf_backing: [u8; WIFI_MTU] = [0; WIFI_MTU];
                let num_bytes = rx & 0x7ff;
                let num_words =
                    if num_bytes % 2 == 0 {
                        num_bytes / 2
                    } else {
                        num_bytes / 2 + 1
                    };
                let mut error = false;
                let mut words_received = 0;
                // num_words can be bigger than the MTU, in which case, we fill up to our
                // buffer, discard the rest, and then ignore the packet (as FIFO must always be drained).
                while words_received < num_words {
                    match com_rx(200) {
                        Ok(result) => {
                            if words_received * 2 + 1 < WIFI_MTU as u16 {
                                let be_bytes = result.to_be_bytes();
                                txbuf_backing[words_received as usize * 2] = be_bytes[0];
                                txbuf_backing[words_received as usize * 2 + 1] = be_bytes[1];
                            } else {
                                error = true;
                            }
                        }
                        _ => error = true,
                    }
                    words_received += 1;
                }
                if !error {
                    logln!(LL::Debug, "Sending packet");
                    match wfx_rs::hal_wf200::send_net_packet(&mut txbuf_backing[..num_bytes as usize]) {
                        Err(_) => {
                            tx_errs += 1;
                            com_int_mgr.set_tx_error();
                        },
                        _ => (),
                    }
                } else {
                    logln!(LL::Error, "Send packet error!");
                }
            } else {
                loghexln!(LL::Debug, "ComError ", rx);
                com_tx(ComState::ERROR.verb);
            }
        }
        // update the state of the irq pin after all the potential ACKs have been handled above
        com_int_mgr.update_irq_pin();

        //////////////////////// ---------------------------
        // unsafe { riscv::asm::wfi() }; // potential for power savings? unfortunately WFI seems broken
    }
}
