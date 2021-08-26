#![allow(dead_code)]

use crate::hal_i2c::Hardi2c;
use bitflags::*;

const LM3509_ADDR: u8 = 0x36;

const LM3509_GP_ADR: u8 = 0x10;
bitflags! {
    pub struct Config: u8 {
        const  MAIN_ENABLE      = 0b0000_0001;
        const  SEC_ENABLE       = 0b0000_0010;
        const  UNISON_ENABLE    = 0b0000_0100;
        const  UNISON_DISABLE   = 0b0000_0000;
        const  RAMP_51US_STEP   = 0b0000_0000;
        const  RAMP_26MS_STEP   = 0b0000_1000;
        const  RAMP_13MS_STEP   = 0b0001_0000;
        const  RAMP_52MS_STEP   = 0b0001_1000;
        const  OLED_MODE        = 0b0010_0000;
    }
}

const LM3509_BMAIN_ADR: u8 = 0xA0;
const LM3509_BSUB_ADR: u8 = 0xB0;
const LM3509_GPIO_ADR: u8 = 0x80;

const BL_TIMEOUT_MS: u32 = 2;

pub struct BtBacklight {
    /// number from 0-3 which specifies how fast the brightness level changes (0 is fastest, 3 is slowest)
    rate_of_change: u8,
}

impl BtBacklight {
    pub fn new() -> Self {
        BtBacklight {
                rate_of_change: 3,
        }
    }

    pub fn set_rate_of_change(&mut self, roc: u8) {
        let mut roc_local: u8 = roc;
        if roc_local > 3 {
            roc_local = 3;
        }
        self.rate_of_change = roc_local;
    }

    pub fn max_brightness() -> u8 {
        31 as u8
    }

    // note: this is coded for now to only allow SEC to work
    // once the actual backlight is available
    pub fn set_brightness(&mut self, i2c: &mut Hardi2c, main_level: u8, sub_level: u8) {
        let mut main_level_local: u8 = main_level;
        let mut sub_level_local: u8 = sub_level;
        let mut txbuf: [u8; 2] = [0;2];

        if main_level_local == 0 && sub_level_local == 0 {
            // first set the brightness control to 0
            txbuf[0] = LM3509_BMAIN_ADR;
            txbuf[1] = 0xE0;
            while i2c.i2c_controller(LM3509_ADDR, Some(&txbuf), None, BL_TIMEOUT_MS) != 0 {}
            txbuf[0] = LM3509_BSUB_ADR;
            txbuf[1] = 0xE0;
            while i2c.i2c_controller(LM3509_ADDR, Some(&txbuf), None, BL_TIMEOUT_MS) != 0 {}

            // then put the string into shutdown mode
            txbuf[0] = LM3509_GP_ADR;
            txbuf[1] = 0xC0 | ((self.rate_of_change & 0x3) << 3);
            while i2c.i2c_controller(LM3509_ADDR, Some(&txbuf), None, BL_TIMEOUT_MS) != 0 {}

            return
        } else {
            // turn sub-only, and DISABLE unison mode (BL not installed! testing only!)
            txbuf[0] = LM3509_GP_ADR;
            txbuf[1] = ((self.rate_of_change & 0x3) << 3) | 0xC0 | (Config::SEC_ENABLE | Config::MAIN_ENABLE).bits();
            while i2c.i2c_controller(LM3509_ADDR, Some(&txbuf), None, BL_TIMEOUT_MS) != 0 {}

            if true {  // set to false only if main backlight is not installed
                // clamp brightness level to 31
                if main_level_local > 31 {
                    main_level_local = 31;
                }

                txbuf[0] = LM3509_BMAIN_ADR;
                txbuf[1] = main_level_local | 0xE0;
                while i2c.i2c_controller(LM3509_ADDR, Some(&txbuf), None, BL_TIMEOUT_MS) != 0 {}
            }

            // clamp brightness level to 31
            if sub_level_local > 31 {
                sub_level_local = 31;
            }

            txbuf[0] = LM3509_BSUB_ADR;
            txbuf[1] = sub_level_local | 0xE0;

            while i2c.i2c_controller(LM3509_ADDR, Some(&txbuf), None, BL_TIMEOUT_MS) != 0 {}
        }
    }

}

