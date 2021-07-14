extern crate betrusted_hal;
extern crate utralib;
extern crate volatile;
use betrusted_hal::api_bq25618::BtCharger;
use betrusted_hal::api_gasgauge::{
    gg_avg_current, gg_set_hibernate, gg_state_of_charge, gg_voltage,
};
use betrusted_hal::api_lm3509::BtBacklight;
use betrusted_hal::api_tusb320::BtUsbCc;
use betrusted_hal::hal_hardi2c::Hardi2c;
use betrusted_hal::hal_time::{delay_ms, get_time_ms, get_time_ticks, set_msleep_target_ticks};
use utralib::generated::{utra, CSR};

// This is the voltage that we hard shut down the device to avoid battery damage
const BATTERY_PANIC_VOLTAGE: i16 = 3500;

// This is the reserve voltage where we attempt to shut off the SoC so that BBRAM keys, RTC are preserved
const BATTERY_LOW_VOLTAGE: i16 = 3575;

// == 2021-07-14 REFACTOR IN PROGRESS =========================================
// TODO: Fix the comments and argument passing of charger_handler().
//
// This is mostly a copy & paste from the event loop in main.rs::main(). The
// exceptions are reduced indentation and dereferencing of mutable reference
// arguments. For now, there are various things about comments and code in
// charger_handler() that don't make sense when removed from the context of
// main(). I'm trying to avoid changing too much at once so it's easier to read
// through the commit history and review the changes step by step.
// ============================================================================

/// This function wraps a tricky chunk of I2C driver code that manages battery
/// and power subsystem stuff, including:
/// - Activating ship-mode
/// - USB-C charge cable detection
/// - Battery voltage and state of charge
/// - Adjusting battery charge current
/// - Low voltage panic shutdown
///
/// DANGER! DANGER! Be careful about changing this code. The EC's I2C interface
/// uses the SB_I2C hard IP block on the Lattice iCE40 UP5K. SB_I2C is
/// generally a bit weird and also very picky about timing. Also, this code is
/// important for safe shipping and overall battery health.
///
pub fn charger_handler(
    power_csr: &mut CSR<u32>,
    charger: &mut BtCharger,
    mut i2c: &mut Hardi2c,
    last_run_time: &mut u32,
    loopcounter: &mut u32,
    voltage: &mut i16,
    last_voltage: &mut i16,
    current: &mut i16,
    stby_current: &mut i16,
    soc_was_on: &mut bool,
    battery_panic: &mut bool,
    voltage_glitch: &mut bool,
    usb_cc_event: &mut bool,
    usb_cc: &mut BtUsbCc,
    backlight: &mut BtBacklight,
) {
    // I2C can't happen inside an interrupt routine, so we do it in the main loop
    // real time response is also not critical; note this runs "lazily", only if the COM loop is idle
    if get_time_ms() - *last_run_time > 1000 {
        *last_run_time = get_time_ms();
        *loopcounter += 1;

        // routine pings & housekeeping; split i2c traffic across two phases to even the CPU load
        if *loopcounter % 2 == 0 {
            charger.chg_keepalive_ping(&mut i2c);
            if !(*usb_cc_event) {
                *usb_cc_event = usb_cc.check_event(&mut i2c);
                if usb_cc.status[1] & 0xC0 == 0x80 {
                    // Attached.SNK transition
                    charger.chg_start(&mut i2c);
                }
            }
        } else {
            *voltage = gg_voltage(&mut i2c);
            if *voltage < 0 {
                // there are monitoring glitches during charge mode transitions, try to catch and
                // filter them out
                *voltage = *last_voltage;
                *voltage_glitch = true;
            }
            *last_voltage = *voltage;
            if *voltage < BATTERY_PANIC_VOLTAGE {
                let cursoc = gg_state_of_charge(&mut i2c);
                if cursoc < 5 && *battery_panic {
                    // in case of a cold boot, give the charger a few seconds to recognize charging
                    // and raise the voltage also don't attempt to go shipmode if the charger is
                    // indicating it is trying to charge
                    if get_time_ticks() > 8000
                        && !charger.chg_is_charging(&mut i2c, false)
                        && gg_voltage(&mut i2c) < BATTERY_PANIC_VOLTAGE
                    {
                        // put the device into "shipmode" which disconnects the battery from the system
                        // NOTE: this may cause the loss of volatile keys
                        backlight.set_brightness(&mut i2c, 0, 0); // make sure the backlight is off

                        charger.set_shipmode(&mut i2c);
                        gg_set_hibernate(&mut i2c);
                        let power = power_csr.ms(utra::power::POWER_SELF, 1)
                            | power_csr.ms(utra::power::POWER_DISCHARGE, 1);
                        power_csr.wo(utra::power::POWER, power);
                        set_msleep_target_ticks(500);
                        delay_ms(16_000); // 15s max time for ship mode to kick in, add 1s just to be safe
                    }
                } else if cursoc < 5 {
                    // require a second check before shutting things down, to rule out temporary
                    // glitches in measurement
                    *battery_panic = true;
                }
            } else if *voltage < BATTERY_LOW_VOLTAGE {
                // TODO: warn the SoC that power is about to go away using the COM_IRQ feature...
                // siginficantly: shutting down the SoC without its consent is not possible. so this
                // needs to be refactored once Xous gets to a state where it can handle a power
                // state request for now just make a NOP

                // NOTE: this should probably get more aggressive about shutting down wifi, etc.
                /*
                if gg_state_of_charge(&mut i2c) < 10 {
                    backlight.set_brightness(&mut i2c, 0, 0); // make sure the backlight is off
                    let power =
                    power_csr.ms(utra::power::POWER_SELF, 1)
                    | power_csr.ms(utra::power::POWER_DISCHARGE, 1);
                    power_csr.wo(utra::power::POWER, power);

                    set_msleep_target_ticks(500); // extend next service so we can discharge

                    pd_loop_timer = get_time_ms();
                }
                */
            } else {
                *battery_panic = false;
            }
            if power_csr.rf(utra::power::STATS_STATE) == 1 {
                *current = gg_avg_current(&mut i2c);
            } else if power_csr.rf(utra::power::STATS_STATE) == 0 && !(*soc_was_on) {
                // only sample if the last state was also powered off, so we aren't averaging in ~1s
                // worth of "power on" current while this loop triggers
                *stby_current = gg_avg_current(&mut i2c);
            }
            if power_csr.rf(utra::power::STATS_STATE) == 1 {
                *soc_was_on = true;
            } else {
                *soc_was_on = false;
            }
        }

        // check if we should turn the SoC on or not based on power status change events
        if charger.chg_is_charging(&mut i2c, false) {
            // sprintln!("charger insert or soc on event!");
            let power = power_csr.ms(utra::power::POWER_SELF, 1)
                | power_csr.ms(utra::power::POWER_SOC_ON, 1)
                | power_csr.ms(utra::power::POWER_DISCHARGE, 0);
            power_csr.wo(utra::power::POWER, power); // turn off discharge if the soc is up
        }
    }
}
