use bitflags::*;

use crate::hal_hardi2c::Hardi2c;
use utralib::generated::*;

const TUSB320LAI_ADDR: u8 = 0x47;


pub const TUSB320LAI_00_ID: usize = 0x00;
pub const TUSB320LAI_IDSTRING: [u8; 8] = [0x30, 0x32, 0x33, 0x42, 0x53, 0x55, 0x54, 0x00];

pub const TUSB320LAI_08_CSR0: usize = 0x08;
bitflags! {
    pub struct ConfigStatus0: u8 {
        const ACTIVE_CABLE_DETECTED     = 0b0000_0001;
        const ACCESSORY_NOT             = 0b0000_0000;
        const ACCESSORY_AUDIO           = 0b0000_1000;
        const ACCESSORY_AUDIO_CHARGE    = 0b0000_1010;
        const ACCESSORY_DEBUG_DFP       = 0b0000_1100;
        const ACCESSORY_DEBUG_UFP       = 0b0000_1110;
        const ACCESSORY_MASK            = 0b0000_1110;
        const CURRENT_MODE_DEFAULT      = 0b0000_0000;
        const CURRENT_MODE_MEDIUM       = 0b0001_0000;
        const CURRENT_MODE_AUDIO_500MA  = 0b0010_0000;
        const CURRENT_MODE_HIGH         = 0b0011_0000;
        const CURRENT_ADVERTISE_500MA   = 0b0000_0000;
        const CURRENT_ADVERTISE_1500MA  = 0b0100_0000;
        const CURRENT_ADVERTISE_3000MA  = 0b1000_0000;
    }
}

pub const TUSB320LAI_09_CSR1: usize = 0x09;
bitflags! {
    pub struct ConfigStatus1: u8 {
        const DISABLE_UFP_ACCESSORY       = 0b0000_0001;
        const ENABLE_UFP_ACCESSORY        = 0b0000_0000;
        const DRP_ADVERT_DUTYCYCLE_30PCT  = 0b0000_0000;
        const DRP_ADVERT_DUTYCYCLE_40PCT  = 0b0000_0010;
        const DRPVERT_AD_DUTYCYCLE_50PCT  = 0b0000_0100;
        const DRPVERT_AD_DUTYCYCLE_60PCT  = 0b0000_0110;
        const REGCHANGE_INTERRUPT         = 0b0001_0000;
        const CABLE_DIR_CC1               = 0b0000_0000;
        const CABLE_DIR_CC2               = 0b0010_0000;
        const NOT_ATTACHED                = 0b0000_0000;
        const ATTACHED_SRC_DFP            = 0b0100_0000;
        const ATTACHED_SNK_UFP            = 0b1000_0000;
        const ATTACHED_ACCESSORY          = 0b1100_0000;
    }
}

pub const TUSB320LAI_0A_CSR2: usize = 0x0A;
bitflags! {
    pub struct ConfigStatus2: u8 {
        const DISABLE_CC_TERM          = 0b0000_0001;
        const ENABLE_CC_TERM           = 0b0000_0000;
        const SOURCE_PREF_DRP_STANDARD = 0b0000_0000;
        const SOURCE_PREF_DRP_TRY_SNK  = 0b0000_0010;
        const SOURCE_PREF_DRP_TRY_SRC  = 0b0000_0100;
        const SOFT_RESET               = 0b0000_1000;
        const MODE_BY_PORT_PIN         = 0b0000_0000;
        const MODE_UFP_UNATTACHED_SNK  = 0b0001_0000;
        const MODE_DFP_UNATTACHED_SRC  = 0b0010_0000;
        const MODE_DRP_AS_UNATTACH_SNK = 0b0011_0000;
        const DEBOUNCE_CC_168MS        = 0b0000_0000;
        const DEBOUNCE_CC_118MS        = 0b0100_0000;
        const DEBOUNCE_CC_134MS        = 0b1000_0000;
        const DEBOUNCE_CC_152MS        = 0b1100_0000;
    }
}

pub const TUSB320LAI_45_RDRP: usize = 0x45;
bitflags! {
    pub struct DisableRdRp: u8 {
        const DISABLE_RD_RP   = 0b100;
        const NORMAL          = 0b000;
    }
}

pub const TUSB320LAI_A0_REV: usize = 0xA0;
pub const TUSB320LAI_REVISION_EXPECTED: u8 = 0x02;
pub const TUSB320LAI_REVISION_EXPECTED_ALT: u8 = 0x06;


const TUSB320_TIMEOUT_MS: u32 = 1;

pub struct BtUsbCc {
    pub id: [u8; 8],
    pub status: [u8; 3],
}

impl BtUsbCc {
    pub fn new() -> Self {
        BtUsbCc { id: [0; 8], status: [0; 3] }
    }

    pub fn init(&mut self, i2c: &mut Hardi2c) -> u8 {
        let mut txbuf: [u8; 1] = [TUSB320LAI_00_ID as u8];
        let mut rxbuf: [u8; 8] = [0; 8];

        while i2c.i2c_controller(TUSB320LAI_ADDR, Some(&txbuf), Some(&mut rxbuf), TUSB320_TIMEOUT_MS) != 0 {}
        for i in 0..8 {
            self.id[i] = rxbuf[i];
            // maybe should do something smarter than an assert here, huh.
            assert!(self.id[i] == TUSB320LAI_IDSTRING[i]);
        }
        // check revision
        txbuf = [TUSB320LAI_A0_REV as u8];
        let mut rxrev: [u8; 1] = [0; 1];
        while i2c.i2c_controller(TUSB320LAI_ADDR, Some(&txbuf), Some(&mut rxrev), TUSB320_TIMEOUT_MS) != 0 {}
        if cfg!(feature = "debug_uart") {
            sprintln!("tusb320lai_rev: {:08x}", rxrev[0]);
            crate::hal_time::delay_ms(50);
        }
        let rev = rxrev[0];
        // assert!((rxrev[0] == TUSB320LAI_REVISION_EXPECTED) || (rxrev[0] == TUSB320LAI_REVISION_EXPECTED_ALT));

        // fill in other parameter inits
        // we want to initially look like a UFP, advertising 500mA current
        let mut txwrbuf: [u8; 2] = [TUSB320LAI_09_CSR1 as u8,
           (ConfigStatus1::DISABLE_UFP_ACCESSORY | ConfigStatus1::DRP_ADVERT_DUTYCYCLE_30PCT).bits()];
        while i2c.i2c_controller(TUSB320LAI_ADDR, Some(&txwrbuf), None, TUSB320_TIMEOUT_MS) != 0 {}

        // set us up for UFP mode -- once we get host support, need to change to allow DRP mode!!
        txwrbuf = [TUSB320LAI_0A_CSR2 as u8,
           (ConfigStatus2::MODE_UFP_UNATTACHED_SNK | ConfigStatus2::SOURCE_PREF_DRP_TRY_SNK).bits()];
        while i2c.i2c_controller(TUSB320LAI_ADDR, Some(&txwrbuf), None, TUSB320_TIMEOUT_MS) != 0 {}

        txbuf = [TUSB320LAI_08_CSR0 as u8];
        let mut status_regs: [u8; 3] = [0; 3];
        while i2c.i2c_controller(TUSB320LAI_ADDR, Some(&txbuf), Some(&mut status_regs), TUSB320_TIMEOUT_MS) != 0 {}
        for i in 0..3 {
            self.status[i] = status_regs[i];
        }

        // enable the regchange event
        let mut i2c_csr = CSR::new(HW_I2C_BASE as *mut u32);
        i2c_csr.wfo(utra::i2c::EV_ENABLE_USBCC_INT, 1);

        // clear the REGCHANGE_INTERRUPT bit before returning, in case it was set
        let txwrbuf: [u8; 2] = [TUSB320LAI_09_CSR1 as u8,
           (ConfigStatus1::DISABLE_UFP_ACCESSORY | ConfigStatus1::DRP_ADVERT_DUTYCYCLE_30PCT | ConfigStatus1::REGCHANGE_INTERRUPT).bits()];
        while i2c.i2c_controller(TUSB320LAI_ADDR, Some(&txwrbuf), None, TUSB320_TIMEOUT_MS) != 0 {}

        rev
    }

    pub fn check_event(&mut self, i2c: &mut Hardi2c) -> bool {
        let mut i2c_csr = CSR::new(HW_I2C_BASE as *mut u32);
        if i2c_csr.rf(utra::i2c::EV_PENDING_USBCC_INT) != 0 {
            let txbuf = [TUSB320LAI_08_CSR0 as u8];
            let mut status_regs: [u8; 3] = [0; 3];
            while i2c.i2c_controller(TUSB320LAI_ADDR, Some(&txbuf), Some(&mut status_regs), TUSB320_TIMEOUT_MS) != 0 {}
            for i in 0..3 {
                self.status[i] = status_regs[i];
            }

            // clear the REGCHANGE_INTERRUPT bit by writing a `1` to it
            let txwrbuf: [u8; 2] = [TUSB320LAI_09_CSR1 as u8,
            (ConfigStatus1::DISABLE_UFP_ACCESSORY | ConfigStatus1::DRP_ADVERT_DUTYCYCLE_30PCT | ConfigStatus1::REGCHANGE_INTERRUPT).bits()];
            while i2c.i2c_controller(TUSB320LAI_ADDR, Some(&txwrbuf), None, TUSB320_TIMEOUT_MS) != 0 {}

            // clear the interrupt in the CPU by writing a 1 to the pending bit
            i2c_csr.wfo(utra::i2c::EV_PENDING_USBCC_INT, 1);
            true
        } else {
            false
        }
    }
}
