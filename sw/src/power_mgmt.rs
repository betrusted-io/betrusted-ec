extern crate betrusted_hal;
extern crate utralib;
extern crate volatile;
use betrusted_hal::api_bq25618::BtCharger;
use betrusted_hal::api_gasgauge::{
    gg_avg_current, gg_set_hibernate, gg_state_of_charge, gg_voltage,
};
use betrusted_hal::api_lm3509::BtBacklight;
use betrusted_hal::api_tusb320::BtUsbCc;
use betrusted_hal::hal_i2c::Hardi2c;
use betrusted_hal::hal_time::{delay_ms, get_time_ms, get_time_ticks, set_msleep_target_ticks};
use utralib::generated::{utra, CSR};

// This is the voltage that we hard shut down the device to avoid battery damage
const BATTERY_PANIC_VOLTAGE: i16 = 3500;

// This is the reserve voltage where we attempt to shut off the SoC so that BBRAM keys, RTC are preserved
const BATTERY_LOW_VOLTAGE: i16 = 3575;

/// Variables to track Precursor's I2C power management subsystem
pub struct PowerState {
    pub voltage: i16,
    pub last_voltage: i16,
    pub current: i16,
    pub stby_current: i16,
    pub soc_was_on: bool,
    pub battery_panic: bool,
    pub voltage_glitch: bool,
    pub usb_cc_event: bool,
}

pub struct PowerHardware {
    pub power_csr: CSR<u32>,
    pub charger: BtCharger,
    pub usb_cc: BtUsbCc,
    pub backlight: BtBacklight,
}

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
    mut hw: &mut PowerHardware,
    mut i2c: &mut Hardi2c,
    last_run_time: &mut u32,
    loopcounter: &mut u32,
    mut pd_loop_timer: &mut u32,
    mut pow: &mut PowerState,
) {
    // I2C can't happen inside an interrupt routine, so we do it in the main loop
    // real time response is also not critical; note this runs "lazily", only if the COM loop is idle
    if get_time_ms() - *last_run_time > 1000 {
        *last_run_time = get_time_ms();
        *loopcounter += 1;

        // routine pings & housekeeping; split i2c traffic across two phases to even the CPU load
        if *loopcounter % 2 == 0 {
            charge_cable_ping_and_update_status(&mut hw, &mut i2c, &mut pow);
        } else {
            battery_update_voltage(&mut i2c, &mut pow);
            if pow.voltage < BATTERY_PANIC_VOLTAGE {
                handle_low_voltage_panic_event(&mut hw, &mut i2c, &mut pow);
            } else if pow.voltage < BATTERY_LOW_VOLTAGE {
                // TODO: warn the SoC that power is about to go away using the COM_IRQ feature...
                // siginficantly: shutting down the SoC without its consent is not possible. so this
                // needs to be refactored once Xous gets to a state where it can handle a power
                // state request for now just make a NOP

                handle_low_voltage_event(&mut hw, &mut i2c, &mut pd_loop_timer);
            } else {
                pow.battery_panic = false;
            }
            if hw.power_csr.rf(utra::power::STATS_STATE) == 1 {
                pow.current = gg_avg_current(&mut i2c);
            } else if hw.power_csr.rf(utra::power::STATS_STATE) == 0 && !(pow.soc_was_on) {
                // only sample if the last state was also powered off, so we aren't averaging in ~1s
                // worth of "power on" current while this loop triggers
                pow.stby_current = gg_avg_current(&mut i2c);
            }
            pow.soc_was_on = hw.power_csr.rf(utra::power::STATS_STATE) == 1;
        }

        // check if we should turn the SoC on or not based on power status change events
        if hw.charger.chg_is_charging(&mut i2c, false) {
            // sprintln!("charger insert or soc on event!");
            let power = hw.power_csr.ms(utra::power::POWER_SELF, 1)
                | hw.power_csr.ms(utra::power::POWER_SOC_ON, 1);
                //| hw.power_csr.ms(utra::power::POWER_DISCHARGE, 0);
            hw.power_csr.wo(utra::power::POWER, power); // turn off discharge if the soc is up
        }
    }
}

pub fn charge_cable_ping_and_update_status(
    hw: &mut PowerHardware,
    mut i2c: &mut Hardi2c,
    pow: &mut PowerState,
) {
    hw.charger.chg_keepalive_ping(&mut i2c);
    if !(pow.usb_cc_event) {
        pow.usb_cc_event = hw.usb_cc.check_event(&mut i2c);
        if hw.usb_cc.status[1] & 0xC0 == 0x80 {
            // Attached.SNK transition
            hw.charger.chg_start(&mut i2c);
        }
    }
}

pub fn battery_update_voltage(mut i2c: &mut Hardi2c, pow: &mut PowerState) {
    pow.voltage = gg_voltage(&mut i2c);
    if pow.voltage < 0 {
        // There are monitoring glitches during charge mode transitions, try to catch and filter
        // them out
        pow.voltage = pow.last_voltage;
        pow.voltage_glitch = true;
    }
    pow.last_voltage = pow.voltage;
}

pub fn handle_low_voltage_panic_event(
    hw: &mut PowerHardware,
    mut i2c: &mut Hardi2c,
    pow: &mut PowerState,
) {
    let cursoc = gg_state_of_charge(&mut i2c);
    if cursoc < 5 && pow.battery_panic {
        // in case of a cold boot, give the charger a few seconds to recognize charging
        // and raise the voltage also don't attempt to go shipmode if the charger is
        // indicating it is trying to charge
        if get_time_ticks() > 8000
            && !hw.charger.chg_is_charging(&mut i2c, false)
            && gg_voltage(&mut i2c) < BATTERY_PANIC_VOLTAGE
        {
            // put the device into "shipmode" which disconnects the battery from the system
            // NOTE: this may cause the loss of volatile keys
            hw.backlight.set_brightness(&mut i2c, 0, 0); // make sure the backlight is off

            hw.charger.set_shipmode(&mut i2c);
            gg_set_hibernate(&mut i2c);
            let power = hw.power_csr.ms(utra::power::POWER_SELF, 1);
               // | hw.power_csr.ms(utra::power::POWER_DISCHARGE, 1);
            hw.power_csr.wo(utra::power::POWER, power);
            set_msleep_target_ticks(500);
            delay_ms(16_000); // 15s max time for ship mode to kick in, add 1s just to be safe
        }
    } else if cursoc < 5 {
        // require a second check before shutting things down, to rule out temporary
        // glitches in measurement
        pow.battery_panic = true;
    }
}

/// This is currently useless (TODO: make this less useless)
#[allow(unused_variables, unused_mut)]
pub fn handle_low_voltage_event(
    hw: &mut PowerHardware,
    mut i2c: &mut Hardi2c,
    pd_loop_timer: &mut u32,
) {
    // NOTE: this should probably get more aggressive about shutting down wifi, etc.
    /*
    if gg_state_of_charge(&mut i2c) < 10 {
        hw.backlight.set_brightness(&mut i2c, 0, 0); // make sure the backlight is off
        let power = hw.power_csr.ms(utra::power::POWER_SELF, 1)
            | hw.power_csr.ms(utra::power::POWER_DISCHARGE, 1);
        hw.power_csr.wo(utra::power::POWER, power);
        set_msleep_target_ticks(500); // extend next service so we can discharge
        *pd_loop_timer = get_time_ms();
    }
    */
}
