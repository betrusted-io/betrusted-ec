#![allow(dead_code)]

use crate::hal_hardi2c::Hardi2c;


const BQ24157_ADDR: u8 = 0x6a; 

const  BQ24157_STAT_ADR : u8 = 0;
const  BQ24157_CTRL_ADR : u8 = 1;
const  BQ24157_BATV_ADR : u8 = 2;
const  BQ24157_ID_ADR   : u8 = 3;
const  BQ24157_IBAT_ADR : u8 = 4;
const  BQ24157_SPCHG_ADR : u8 = 5;
const  BQ24157_SAFE_ADR : u8 = 6;

const CHG_TIMEOUT_MS: u32 = 1;

#[derive(Debug)]
pub struct BtCharger {
    pub registers: [u8; 7],
}

impl BtCharger {
    pub fn new() -> Self {
        BtCharger { registers: [0; 7] }
    }

    pub fn update_regs(&mut self, i2c: &mut Hardi2c) {
        let mut rxbuf: [u8; 2] = [0, 0];
        let mut txbuf: [u8; 1] = [0];

        for i in 0..7 {
            txbuf[0] = i as u8;
            while i2c.i2c_controller(BQ24157_ADDR, Some(&txbuf), Some(&mut rxbuf), CHG_TIMEOUT_MS) != 0 {}
            self.registers[i] = rxbuf[0] as u8;
        }
    }

    pub fn chg_is_charging(&mut self, i2c: &mut Hardi2c) -> bool {
        let txbuf: [u8; 1] = [BQ24157_STAT_ADR];
        let mut rxbuf: [u8; 2] = [0, 0];

        while i2c.i2c_controller(BQ24157_ADDR, Some(&txbuf), Some(&mut rxbuf), CHG_TIMEOUT_MS) != 0 {}
        match (rxbuf[0] >> 4) & 0x3 {
            0 => false,
            1 => true,
            2 => false,
            3 => false,
            _ => false,
        }
    }

    pub fn chg_keepalive_ping(&mut self, i2c: &mut Hardi2c) {
        let txbuf: [u8; 2] = [BQ24157_STAT_ADR, 0x80]; // 32 sec timer reset, enable stat pin
        while i2c.i2c_controller(BQ24157_ADDR, Some(&txbuf), None, CHG_TIMEOUT_MS) != 0 {}
    }

    pub fn chg_set_safety(&mut self, i2c: &mut Hardi2c) {
        // 56 mOhm current sense resistor
        // (37.4mV + 54.4mV * Vmchrg[3] + 27.2mV * Vmchrg[2] + 13.6mV * Vmchrg[1] + 6.8mV * Vmchrg[0]) / 0.056ohm = I charge
        // 0xB0 => 1639 max current (limited by IC), 4.22V max regulation voltage
        // as current:
        //    971mA | 485mA | 242mA | 121 mA, plus offset of 667mA
        // 0x70 = 1.515A & 4.2V limits
        let txbuf: [u8; 2] = [BQ24157_SAFE_ADR, 0x70];
        while i2c.i2c_controller(BQ24157_ADDR, Some(&txbuf), None, CHG_TIMEOUT_MS) != 0 {}
    }

    pub fn chg_boost(&mut self, i2c: &mut Hardi2c) {
        let txbuf: [u8; 2] = [BQ24157_CTRL_ADR, 0xB5]; // turn on boost
        while i2c.i2c_controller(BQ24157_ADDR, Some(&txbuf), None, CHG_TIMEOUT_MS) != 0 {}
    }

    // 50 F8 8E 51 6B 03 70 - dump from known good charging system
    pub fn chg_set_autoparams(&mut self, i2c: &mut Hardi2c) {
        // set battery voltage
        // 0.64V | 0.32V | 0.16V | 0.08V | 0.04V | 0.02V | + 3.5V offset
        // 0x8C = 0.64 + 0.04 + 0.02 + 3.5 = 4.2V charging voltage
        // + 0x2 = OTG boost not enabled
        // address 2
        let txbuf: [u8; 2] = [BQ24157_BATV_ADR, 0x8E];
        while i2c.i2c_controller(BQ24157_ADDR, Some(&txbuf), None, CHG_TIMEOUT_MS) != 0 {}

        // set special charger voltage, e.g. threshold to reduce charging current due to bad cables
        // address 5
        // 0.32V | 0.16V | 0.08V | + 4.2V = 4.44V DPM threshold
        // normal charge current, special charger voltage = 4.2V
        let txbuf2: [u8; 2] = [BQ24157_SPCHG_ADR, 0x3];
        while i2c.i2c_controller(BQ24157_ADDR, Some(&txbuf2), None, CHG_TIMEOUT_MS) != 0 {}

        // set target charge current + termination current
        // 1.55A target current.
        // 56 mOhm resistor
        // (37.4mV + 27.2mV * Vichrg[3] + 13.6mV * Vichrg[2] + 6.8mV * Vichrg[1]) / 0.056ohm = I charge
        // termination current offset is 3.4mV, +3.4mV/LSB
        // 485mA | 242mA | 121 mA + 667mA offset  => 0x1 = 788mA charger sense target
        // 242mA | 121mA | 60mA +  60mA offset => 0x1 = 120mA termination
        // address 4
        let txbuf3: [u8; 2] = [BQ24157_IBAT_ADR, 0x11];
        while i2c.i2c_controller(BQ24157_ADDR, Some(&txbuf3), None, CHG_TIMEOUT_MS) != 0 {}
    }

    /// This forces the start of charging. It's a bit of a hammer, maybe refine it down the road. [FIXME]
    pub fn chg_start(&mut self, i2c: &mut Hardi2c) {
        // 10 11 0000   => 800mA current limit, weak battery 4.0V, no charge current term, enable charging
        // address 1
        let txbuf: [u8; 2] = [BQ24157_CTRL_ADR, 0xB0];  // 0x78 previous value
        // charge mode, not hiZ, charger enabled, enable charge current termination, weak battery==3.7V, Iin limit = no limit
        while i2c.i2c_controller(BQ24157_ADDR, Some(&txbuf), None, CHG_TIMEOUT_MS) != 0 {}
    }

}

