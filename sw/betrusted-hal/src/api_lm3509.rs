#![allow(dead_code)]

use crate::hal_hardi2c::Hardi2c;

const LM3509_ADDR: u8 = 0x36;

const LM3509_GP_ADR: u8 = 0x10;
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

    pub fn set_brightness(&mut self, i2c: &mut Hardi2c, level: u8) {
        let mut level_local: u8 = level;

        // turn on main, sub, and unison mode
        let mut txbuf: [u8; 2] = [LM3509_GP_ADR, 0xC7];

        if level == 0 {
            // first set the brightness control to 0
            txbuf[0] = LM3509_BMAIN_ADR;
            txbuf[1] = level | 0xE0;
            while i2c.i2c_master(LM3509_ADDR, Some(&txbuf), None, BL_TIMEOUT_MS) != 0 {}
    
            // then put the string into shutdown mode
            txbuf[0] = LM3509_GP_ADR;
            txbuf[1] = 0xC0 | ((self.rate_of_change & 0x3) << 3);
            while i2c.i2c_master(LM3509_ADDR, Some(&txbuf), None, BL_TIMEOUT_MS) != 0 {}

            return
        } else {
            // activate BMAIN, BSUB and set ramp value
            txbuf[1] = ((self.rate_of_change & 0x3) << 3) | 0xC7;
            while i2c.i2c_master(LM3509_ADDR, Some(&txbuf), None, BL_TIMEOUT_MS) != 0 {}

            // clamp brightness level to 31
            if level_local > 31 {
                level_local = 31;
            }

            txbuf[0] = LM3509_BMAIN_ADR;
            txbuf[1] = level_local | 0xE0;

            while i2c.i2c_master(LM3509_ADDR, Some(&txbuf), None, BL_TIMEOUT_MS) != 0 {}
        }
    }

}

