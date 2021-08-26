use bitflags::*;

use crate::hal_i2c::Hardi2c;

const BQ25618_ADDR: u8 = 0x6A;


pub const BQ25618_00_ILIM: usize = 0x00;
bitflags! {
    pub struct InputCurrentLimit: u8 {
        const BATSNS_DIS     = 0b0010_0000;
        const TS_IGNORE      = 0b0100_0000;
        const EN_HIZ         = 0b1000_0000;
    }
}
const IINDPM_LSB_MA: u32 = 100;
const IINDMP_OFFSET_MA: u32 = 100;
const IINDPM_MASK: u32    = 0b0001_1111;
const IINDPM_BITPOS: u32  = 0;

pub const BQ25618_01_CHG_CTL: usize = 0x01;
bitflags! {
    pub struct ChargeControl: u8 {
        const MIN_VBAT_SEL    = 0b0000_0001;
        const SYS_MIN_2600MV  = 0b0000_0000;
        const SYS_MIN_2800MV  = 0b0000_0010;
        const SYS_MIN_3000MV  = 0b0000_0100;
        const SYS_MIN_3200MV  = 0b0000_0110;
        const SYS_MIN_3400MV  = 0b0000_1000;
        const SYS_MIN_3500MV  = 0b0000_1010;
        const SYS_MIN_3600MV  = 0b0000_1100;
        const SYS_MIN_3700MV  = 0b0000_1110;
        const CHARGE_ON       = 0b0001_0000;
        const BOOST_ON        = 0b0010_0000;
        const BOOST_OFF       = 0b0000_0000;
        const WD_RST          = 0b0100_0000;
        const PFM_DIS         = 0b1000_0000;
    }
}

pub const BQ25618_02_CHG_ILIM: usize = 0x02;
bitflags! {
    pub struct ChargeCurrentLimit: u8 {
        const ICHG_MASK       = 0b0011_1111;
        const Q1_FULLON       = 0b0100_0000;
    }
}
const ICHG_BITPOS: u32 = 0;
const ICHG_LSB_MA: u32 = 20;


pub const BQ25618_03_PRE_TERM: usize = 0x03;
const IPRECHG_MASK  :u32  = 0b1111_0000;
const IPRECHG_BITPOS:u32  = 4;
const IPRECHG_OFFSET_MA: u32 = 20;
const IPRECHG_LSB_MA: u32 = 20;

const ITERM_MASK    :u32  = 0b0000_1111;
const ITERM_BITPOS  :u32  = 0;
const ITERM_OFFSET_MA: u32 = 20;
const ITERM_LSB_MA: u32 = 20;


pub const BQ25618_04_VOLT_LIM: usize = 0x04;
bitflags! {
    pub struct BatteryVoltageLimit: u8 {
        const VRECHG_THRESH_120MV = 0b00000_000;
        const VRECHG_THRESH_210MV = 0b00000_001;
        const TOPOFF_TIME_DISABLE = 0b00000_000;
        const TOPOFF_TIME_15MIN   = 0b00000_010;
        const TOPOFF_TIME_30MIN   = 0b00000_100;
        const TOPOFF_TIME_45MIN   = 0b00000_110;
        const VREG_3504MV         = 0b00000_000;
        const VREG_3600MV         = 0b00001_000;
        const VREG_3696MV         = 0b00010_000;
        const VREG_3800MV         = 0b00011_000;
        const VREG_3904MV         = 0b00100_000;
        const VREG_4000MV         = 0b00101_000;
        const VREG_4100MV         = 0b00110_000;
        const VREG_4150MV         = 0b00111_000;
        const VREG_4200MV         = 0b01000_000;
        const VREG_4300MV         = 0b01001_000;
        const VREG_4310MV         = 0b01010_000;
        const VREG_4320MV         = 0b01011_000;
        const VREG_4330MV         = 0b01100_000;
        const VREG_4340MV         = 0b01101_000;
        const VREG_4350MV         = 0b01110_000;
        const VREG_4360MV         = 0b01111_000;
        const VREG_4370MV         = 0b10000_000;
        const VREG_4380MV         = 0b10001_000;
        const VREG_4390MV         = 0b10010_000;
        const VREG_4400MV         = 0b10011_000;
        const VREG_4410MV         = 0b10100_000;
        const VREG_4420MV         = 0b10101_000;
        const VREG_4430MV         = 0b10110_000;
        const VREG_4440MV         = 0b10111_000;
        const VREG_4450MV         = 0b11000_000;
        const VREG_4460MV         = 0b11001_000;
        const VREG_4470MV         = 0b11010_000;
        const VREG_4480MV         = 0b11011_000;
        const VREG_4490MV         = 0b11100_000;
        const VREG_4500MV         = 0b11101_000;
        const VREG_4510MV         = 0b11110_000;
        const VREG_4520MV         = 0b11111_000;
    }
}

pub const BQ25618_05_CHG_CTL1: usize = 0x05;
bitflags! {
    pub struct ChargeControl1: u8 {
        const JEITA_VSET_4100MV        = 0b0000_0000;
        const JEITA_VSET_VREG          = 0b0000_0001; // takes the VREG value from BQ25618_04_VOLT_LIM
        const TREG_90C                 = 0b0000_0000;
        const TREG_110C                = 0b0000_0010;
        const CHG_TIMER_10HRS          = 0b0000_0000;
        const CHG_TIMER_20HRS          = 0b0000_0100;
        const SAFETY_TIMER_EN          = 0b0000_1000;
        const SAFETY_TIMER_DIS         = 0b0000_0000;
        const WATCHDOG_DISABLE         = 0b0000_0000;
        const WATCHDOG_40S             = 0b0001_0000;
        const WATCHDOG_80S             = 0b0010_0000;
        const WATCHDOG_160S            = 0b0011_0000;
        const CHG_TERM_ENABLE          = 0b1000_0000;
        const CHG_TERM_DISABLE         = 0b0000_0000;
    }
}

pub const BQ25618_06_CHG_CTL2: usize = 0x06;
bitflags! {
    pub struct ChargeControl2: u8 {
        const BOOSTV_4600MV           = 0b0000_0000;
        const BOOSTV_4750MV           = 0b0001_0000;
        const BOOSTV_5000MV           = 0b0010_0000;
        const BOOSTV_5150MV           = 0b0011_0000;
        const OVP_5850MV              = 0b0000_0000;
        const OVP_6400MV              = 0b0100_0000;
        const OVP_11000MV             = 0b1000_0000;
        const OVP_14200MV             = 0b1100_0000;
    }
}
// VINDPM is the level at which DPM kicks in (e.g. droop on input voltage detected)
// actual VINDPM is the greater of VINDPM here or VINDPM_TRACK
const VINDPM_MASK    :u32      = 0b0000_1111;
const VINDPM_BITOFF  :u32      = 0;
const VINDPM_OFFSET_MV: u32 = 3900;
const VINDPM_LSB_MV: u32 = 100;


pub const BQ25618_07_CHG_CTL3: usize = 0x07;
bitflags! {
    pub struct ChargeControl3: u8 {
        const VINDPM_TRACK_DISABLE = 0b0000_0000;
        const VINDPM_TRACK_200MV   = 0b0000_0001; // VBAT + 200mV = VINDPM
        const VINDPM_TRACK_250MV   = 0b0000_0010; // VBAT + 250mV
        const VINDPM_TRACK_300MV   = 0b0000_0011; // VBAT + 300mV
        const BATFET_RST_EN        = 0b0000_0100; // disconnect battery forcibly
        const BATFET_DLY_ZERO      = 0b0000_0000; // turn off immediately
        const BATFET_DLY_10S       = 0b0000_1000; // wait 10s to turn off batfet
        const BATFET_RST_WITH_VBUS = 0b0001_0000; // do batfet reset even with VBUS present
        const BATFET_RST_WAIT_VBUS = 0b0000_0000; // wait for VBUS to disappear before doing batfet reset
        const BATFET_OFF_ALLOW     = 0b0010_0000; // needs to be set along with RST_EN to turn off the system
        const BATFET_OFF_IGNORE    = 0b0000_0000; // selecting this causes settings to be ignored
        const TMR2X_EN             = 0b0100_0000; // slow safety timer by 2x during DPM
        const IINDET_EN            = 0b1000_0000; // force input current limit detection when VBUS present
    }
}

pub const BQ25618_08_CHG_STAT0: usize = 0x08;
bitflags! {
    pub struct ChargerStatus0: u8 {
        const VSYS_STAT        = 0b00000_001;
        const THERM_STAT       = 0b00000_010;
        const PWRGOOD_STA      = 0b00000_100;

        const CHG_MASK         = 0b00011_000;
        const CHG_NOT_CHARGING = 0b00000_000;
        const CHG_PRECHARGING  = 0b00001_000;
        const CHG_FASTCHARGING = 0b00010_000;
        const CHG_CHARGETERM   = 0b00011_000;

        const VBUS_MASK        = 0b11100_000;
        const VBUS_NOINPUT     = 0b00000_000;
        const VBUS_HOST_500MA  = 0b00100_000;
        const VBUS_ADAPTER_2A  = 0b01100_000;
        const VBUS_BOOSTMODE   = 0b11100_000;
    }
}

pub const BQ25618_09_CHG_STAT1: usize = 0x09;
bitflags! {
    pub struct ChargerStatus1: u8 {
        const NTC_NORMAL       = 0b00_00_0_000;
        const NTC_WARM         = 0b00_00_0_010;
        const NTC_COOL         = 0b00_00_0_011;
        const NTC_COLD         = 0b00_00_0_101;
        const NTC_HOT          = 0b00_00_0_110;
        const BAT_OVERVOLT     = 0b00_00_1_000;
        const CHG_NORMAL       = 0b00_00_0_000;
        const CHG_INPUT_FAULT  = 0b00_01_0_000;
        const CHG_THERM_FAULT  = 0b00_10_0_000;
        const CHG_TIMEOUT      = 0b00_11_0_000;
        const BOOST_FAULT      = 0b01_00_0_000;
        const WATCHDOG_FAULT   = 0b10_00_0_000;
    }
}

pub const BQ25618_0A_CHG_STAT2: usize = 0x0A;
bitflags! {
    pub struct ChargerStatus2: u8 {
        const IINDPM_INT_MASK = 0b0000_0001;
        const VINDPM_INT_MASK = 0b0000_0010;
        const ACOV_STAT       = 0b0000_0100;
        const TOPOFF_ACTIVE   = 0b0000_1000;
        const BATSNS_ERROR    = 0b0001_0000;
        const IINDPM_STAT     = 0b0010_0000;
        const VINDPM_STAT     = 0b0100_0000;
        const VBUS_GOOD       = 0b1000_0000;
    }
}

pub const BQ25618_0B_ID_RESET: usize = 0x0B;
bitflags! {
    pub struct PartIDReset: u8 {
        const RESERVED_MASK = 0b0_0000_001;
        const ID_VALUE      = 0b0_0101_000;
        const ID_MASK       = 0b0_1111_000;
        const RESET_CHIP    = 0b1_0000_000;
    }
}

pub const BQ25618_0C_JEITA: usize = 0x0C;
bitflags! {
    pub struct JEITAControl: u8 {
        const JEITA_VT3_40C0        = 0b00_00_00_00;  // hot temp, e.g. 40.0C
        const JEITA_VT3_44C5        = 0b00_00_00_01;
        const JEITA_VT3_50C5        = 0b00_00_00_10;
        const JEITA_VT3_54C5        = 0b00_00_00_11;
        const JEITA_VT2_5C5         = 0b00_00_00_00;  // cold temp, e.g. 5.5C
        const JEITA_VT2_10C0        = 0b00_00_01_00;
        const JEITA_VT2_15C0        = 0b00_00_10_00;
        const JEITA_VT2_20C0        = 0b00_00_11_00;
        const JEITA_WARM_NOCHARGE   = 0b00_00_00_00;  // don't charge when warm
        const JEITA_WARM_20         = 0b00_01_00_00;  // 20% of ICHG
        const JEITA_WARM_50         = 0b00_10_00_00;  // 50% of ICHG
        const JEITA_WARM_100        = 0b00_11_00_00;  // 100% of ICHG when warm
        const JEITA_COOL_NOCHARGE   = 0b00_00_00_00;  // don't charge when cool
        const JEITA_COOL_20         = 0b01_00_00_00;  // 20% of ICHG
        const JEITA_COOL_50         = 0b10_00_00_00;  // 50% of ICHG
        const JEITA_COOL_100        = 0b11_00_00_00;  // 100% of ICHG when cool
    }
}

const CHG_TIMEOUT_MS: u32 = 1;

#[derive(Debug)]
pub struct BtCharger {
    pub registers: [u8; 0xC],
}

impl BtCharger {
    pub fn new() -> Self {
        BtCharger { registers: [0; 0xC] }
    }

    pub fn update_regs(&mut self, i2c: &mut Hardi2c) -> &mut Self {
        let mut rxbuf: [u8; 2] = [0, 0];
        let mut txbuf: [u8; 1] = [0];

        for i in 0..0xC {
            txbuf[0] = i as u8;
            while i2c.i2c_controller(BQ25618_ADDR, Some(&txbuf), Some(&mut rxbuf), CHG_TIMEOUT_MS) != 0 {}
            self.registers[i] = rxbuf[0] as u8;
        }
        self
    }

    pub fn set_shipmode(&mut self, i2c: &mut Hardi2c) {
        let txbuf: [u8; 2] = [BQ25618_07_CHG_CTL3 as u8,
        (ChargeControl3::BATFET_DLY_10S |
            ChargeControl3::BATFET_RST_WAIT_VBUS |
            ChargeControl3::BATFET_OFF_ALLOW |
            ChargeControl3::BATFET_RST_EN).bits() as u8];

        i2c.i2c_controller(BQ25618_ADDR, Some(&txbuf), None, CHG_TIMEOUT_MS);
    }

    pub fn chg_is_charging(&mut self, i2c: &mut Hardi2c, use_cached: bool) -> bool {
        let txbuf: [u8; 1] = [BQ25618_08_CHG_STAT0 as u8];
        let mut rxbuf: [u8; 2] = [0, 0];

        let chgstat0: u8;
        if !use_cached {
            while i2c.i2c_controller(BQ25618_ADDR, Some(&txbuf), Some(&mut rxbuf), CHG_TIMEOUT_MS) != 0 {}
            chgstat0 = rxbuf[0];
        } else {
            chgstat0 = self.registers[BQ25618_08_CHG_STAT0];
        }
        if (chgstat0 & ChargerStatus0::CHG_MASK.bits()) == ChargerStatus0::CHG_NOT_CHARGING.bits() {
            false
        } else if (chgstat0 & ChargerStatus0::CHG_MASK.bits()) == ChargerStatus0::CHG_PRECHARGING.bits() {
            true
        } else if (chgstat0 & ChargerStatus0::CHG_MASK.bits()) == ChargerStatus0::CHG_FASTCHARGING.bits() {
            true
        } else if (chgstat0 & ChargerStatus0::CHG_MASK.bits()) == ChargerStatus0::CHG_CHARGETERM.bits() {
            false
        } else {
            false
        }
    }

    pub fn chg_keepalive_ping(&mut self, i2c: &mut Hardi2c) {
        let txbuf: [u8; 1] = [BQ25618_01_CHG_CTL as u8];
        let mut rxbuf: [u8; 2] = [0, 0];
        while i2c.i2c_controller(BQ25618_ADDR, Some(&txbuf), Some(&mut rxbuf), CHG_TIMEOUT_MS) != 0 {}

        let txbuf: [u8; 2] = [BQ25618_01_CHG_CTL as u8, rxbuf[0] | ChargeControl::WD_RST.bits()];
        while i2c.i2c_controller(BQ25618_ADDR, Some(&txbuf), None, CHG_TIMEOUT_MS) != 0 {}
    }

    pub fn chg_set_autoparams(&mut self, i2c: &mut Hardi2c) {
        self.registers[BQ25618_00_ILIM] =
            InputCurrentLimit::TS_IGNORE.bits() |
            ((((1500 - IINDMP_OFFSET_MA) / IINDPM_LSB_MA) << IINDPM_BITPOS) & IINDPM_MASK) as u8;
        self.registers[BQ25618_01_CHG_CTL] =
           (ChargeControl::WD_RST |
            ChargeControl::CHARGE_ON |
            ChargeControl::BOOST_OFF |
            ChargeControl::SYS_MIN_3400MV)
            .bits();
        self.registers[BQ25618_02_CHG_ILIM] =
            ((500 / ICHG_LSB_MA) << ICHG_BITPOS) as u8; // 500mA fast charge setting
        self.registers[BQ25618_03_PRE_TERM] =
            ((((40 - IPRECHG_OFFSET_MA) / IPRECHG_LSB_MA) << IPRECHG_BITPOS) & IPRECHG_MASK) as u8 |
            ((((60 - ITERM_OFFSET_MA) / ITERM_LSB_MA) << ITERM_BITPOS) & ITERM_MASK) as u8;
        self.registers[BQ25618_04_VOLT_LIM] =
           (BatteryVoltageLimit::VRECHG_THRESH_120MV |
            BatteryVoltageLimit::TOPOFF_TIME_30MIN |
            BatteryVoltageLimit::VREG_4200MV)
            .bits();
        self.registers[BQ25618_05_CHG_CTL1] =
           (ChargeControl1::CHG_TERM_ENABLE |
            ChargeControl1::WATCHDOG_40S |
            ChargeControl1::CHG_TIMER_10HRS |
            ChargeControl1::SAFETY_TIMER_EN |
            ChargeControl1::TREG_110C |
            ChargeControl1::JEITA_VSET_4100MV)
            .bits();
        self.registers[BQ25618_06_CHG_CTL2] =
           ((((4500 - VINDPM_OFFSET_MV) / VINDPM_LSB_MV) << VINDPM_BITOFF) & VINDPM_MASK) as u8 |
           (ChargeControl2::BOOSTV_5000MV |
            ChargeControl2::OVP_14200MV)
            .bits();
        self.registers[BQ25618_07_CHG_CTL3] =
           (ChargeControl3::TMR2X_EN |
            ChargeControl3::BATFET_DLY_10S |
            ChargeControl3::VINDPM_TRACK_300MV)
            .bits();
        // charger status 2 -- no bits set, allow interrupts
        // JEITA registers left at default

        // now commit registers 0-7
        let mut txbuf: [u8; 9] = [0; 9];
        txbuf[0] = BQ25618_00_ILIM as u8; // 0-index address
        for i in 0..8 {
            txbuf[i+1] = self.registers[i];
        }
        while i2c.i2c_controller(BQ25618_ADDR, Some(&txbuf), None, CHG_TIMEOUT_MS) != 0 {}
    }

    // this will override ilim, to attempt charge to run at full current
    pub fn chg_start(&mut self, i2c: &mut Hardi2c) {
        self.registers[BQ25618_00_ILIM] =
            InputCurrentLimit::TS_IGNORE.bits() |
            ((((1500 - IINDMP_OFFSET_MA) / IINDPM_LSB_MA) << IINDPM_BITPOS) & IINDPM_MASK) as u8;
        self.registers[BQ25618_01_CHG_CTL] =
            (ChargeControl::WD_RST |
             ChargeControl::CHARGE_ON |
             ChargeControl::BOOST_OFF |
             ChargeControl::SYS_MIN_3400MV)
             .bits();
         self.registers[BQ25618_02_CHG_ILIM] =
            ((500 / ICHG_LSB_MA) << ICHG_BITPOS) as u8; // 500mA fast charge setting

        // now commit registers 0-2
        let mut txbuf: [u8; 3] = [0; 3];
        txbuf[0] = BQ25618_00_ILIM as u8; // 0-index address
        for i in 0..2 {
            txbuf[i+1] = self.registers[i];
        }
        while i2c.i2c_controller(BQ25618_ADDR, Some(&txbuf), None, CHG_TIMEOUT_MS) != 0 {}
    }

    pub fn chg_boost(&mut self, i2c: &mut Hardi2c) {
        // make sure BATFET_DIS is 0
        self.registers[BQ25618_07_CHG_CTL3] =
           (ChargeControl3::TMR2X_EN |
            ChargeControl3::BATFET_DLY_10S |
            ChargeControl3::VINDPM_TRACK_300MV |
            ChargeControl3::BATFET_OFF_IGNORE)
            .bits();
        let txbuf: [u8; 2] = [BQ25618_07_CHG_CTL3 as u8, self.registers[BQ25618_07_CHG_CTL3]];
        while i2c.i2c_controller(BQ25618_ADDR, Some(&txbuf), None, CHG_TIMEOUT_MS) != 0 {}

        // CHG_CONFIG = 0, BST_CONFIG = 1
        self.registers[BQ25618_01_CHG_CTL] =
           (ChargeControl::WD_RST |
            ChargeControl::BOOST_ON |
            ChargeControl::SYS_MIN_3200MV)
            .bits();
        let txbuf: [u8; 2] = [BQ25618_01_CHG_CTL as u8, self.registers[BQ25618_01_CHG_CTL]];
        while i2c.i2c_controller(BQ25618_ADDR, Some(&txbuf), None, CHG_TIMEOUT_MS) != 0 {}

        // set boost target voltage to 5V
        self.registers[BQ25618_06_CHG_CTL2] =
           ((((4500 - VINDPM_OFFSET_MV) / VINDPM_LSB_MV) << VINDPM_BITOFF) & VINDPM_MASK) as u8 |
           (ChargeControl2::BOOSTV_5150MV |
            ChargeControl2::OVP_14200MV)
            .bits();
        let txbuf: [u8; 2] = [BQ25618_06_CHG_CTL2 as u8, self.registers[BQ25618_06_CHG_CTL2]];
        while i2c.i2c_controller(BQ25618_ADDR, Some(&txbuf), None, CHG_TIMEOUT_MS) != 0 {}
    }

    pub fn chg_boost_off(&mut self, i2c: &mut Hardi2c) {
        self.registers[BQ25618_01_CHG_CTL] =
           (ChargeControl::WD_RST |
            ChargeControl::CHARGE_ON |
            ChargeControl::BOOST_OFF |
            ChargeControl::SYS_MIN_3400MV)
            .bits();
        let txbuf: [u8; 2] = [BQ25618_01_CHG_CTL as u8, self.registers[BQ25618_01_CHG_CTL]];
        while i2c.i2c_controller(BQ25618_ADDR, Some(&txbuf), None, CHG_TIMEOUT_MS) != 0 {}
    }

    pub fn chg_set_safety(&mut self, _i2c: &mut Hardi2c) {
        // function does nothing in this implementation
    }
}


