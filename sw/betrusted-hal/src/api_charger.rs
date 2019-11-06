#![allow(dead_code)]

pub mod api_charger {
    use crate::hal_i2c::hal_i2c::i2c_master;

    const BQ24157_ADDR: u8 = 0x6a; 

    const  BQ24157_STAT_ADR : u8 = 0;
    const  BQ24157_CTRL_ADR : u8 = 1;
    const  BQ24157_BATV_ADR : u8 = 2;
    const  BQ24157_ID_ADR   : u8 = 3;
    const  BQ24157_IBAT_ADR : u8 = 4;
    const  BQ24157_SPCHG_ADR : u8 = 5;
    const  BQ24157_SAFE_ADR : u8 = 6;

    const CHG_TIMEOUT_MS: u32 = 5;

    pub fn chg_is_charging(p: &betrusted_pac::Peripherals) -> bool {
        let txbuf: [u8; 1] = [BQ24157_ADDR];
        let mut rxbuf: [u8; 1] = [0];

        i2c_master(p, BQ24157_ADDR, Some(&txbuf), Some(&mut rxbuf), CHG_TIMEOUT_MS);
        match (rxbuf[0] >> 4) & 0x3 {
            0 => false,
            1 => true,
            2 => false,
            3 => false,
            _ => false,
        }
    }

    pub fn chg_keepalive_ping(p: &betrusted_pac::Peripherals) {
        let txbuf: [u8; 2] = [BQ24157_STAT_ADR, 0xC0]; // 32 sec timer reset, enable stat pin
        i2c_master(p, BQ24157_ADDR, Some(&txbuf), None, CHG_TIMEOUT_MS);
    }

    pub fn chg_set_safety(p: &betrusted_pac::Peripherals) {
        // 56 mOhm current sense resistor
        // (37.4mV + 54.4mV * Vmchrg[3] + 27.2mV * Vmchrg[2] + 13.6mV * Vmchrg[1] + 6.8mV * Vmchrg[0]) / 0.056ohm = I charge
        // 0xB0 => 1639 max current (limited by IC), 4.22V max regulation voltage
        let txbuf: [u8; 2] = [BQ24157_SAFE_ADR, 0xB0];
        i2c_master(p, BQ24157_ADDR, Some(&txbuf), None, CHG_TIMEOUT_MS);
    }

    pub fn chg_set_autoparams(p: &betrusted_pac::Peripherals) {
        { // set battery voltage
            // 4.2V target regulation. 3.5V offset = 0.7V coded. 0.64 + 0.04 + 0.02 = 10_0011 = 0x23
            let txbuf: [u8; 2] = [BQ24157_BATV_ADR, 0x23 << 2 | 2]; // 2 = disable otg & OTG enabled when pin is high
            i2c_master(p, BQ24157_ADDR, Some(&txbuf), None, CHG_TIMEOUT_MS);
        }
        { // set special charger voltage, e.g. threshold to reduce charging current due to bad cables
            let txbuf: [u8; 2] = [BQ24157_SPCHG_ADR, 0x3]; // 4.44V DPM thresh, normal charge current sense voltage for IBAT
            i2c_master(p, BQ24157_ADDR, Some(&txbuf), None, CHG_TIMEOUT_MS);
        }
        { // set target charge current + termination current
            // 1.55A target current.
            // 56 mOhm resistor
            // (37.4mV + 27.2mV * Vichrg[3] + 13.6mV * Vichrg[2] + 6.8mV * Vichrg[1]) / 0.056ohm = I charge
            // termination current offset is 3.4mV, +3.4mV/LSB
            let txbuf: [u8; 2] = [BQ24157_IBAT_ADR, 0x6 << 4 | 0x3]; // 1.51A charge rate, (1*6.8mV + 1*3.4mV + 3.4mV)/0.056 = 242mA termination
            i2c_master(p, BQ24157_ADDR, Some(&txbuf), None, CHG_TIMEOUT_MS);
        }
    }

    #[doc = "This forces the start of charging. It's a bit of a hammer, maybe refine it down the road. [FIXME]"]
    pub fn chg_start(p: &betrusted_pac::Peripherals) {
        let txbuf: [u8; 2] = [BQ24157_CTRL_ADR, 0x3 << 6 | 0x3 << 4 | 0x8];
        // charge mode, not hiZ, charger enabled, enable charge current termination, weak battery==3.7V, Iin limit = no limit
        i2c_master(p, BQ24157_ADDR, Some(&txbuf), None, CHG_TIMEOUT_MS); 
    }
}