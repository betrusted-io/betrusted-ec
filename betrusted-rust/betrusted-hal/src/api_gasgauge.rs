const BQ24157_ADDR : u8 = 0x55;

// word-width commands                                                                                               |
const GG_CMD_CNTL     :  u8 = 0x00;
const GG_CMD_TEMP     :  u8 = 0x02;     // 0.1 degrees K, of battery
const GG_CMD_VOLT     :  u8 = 0x04;     // (mV)
const GG_CMD_FLAG     :  u8 = 0x06;
const GG_CMD_NOM_CAP  :  u8 = 0x08;  // nominal available capacity (mAh)
const GG_CMD_FULL_CAP :  u8 = 0x0A;  // full available capacity (mAh)
const GG_CMD_RM       :  u8 = 0x0C;  // remaining capacity (mAh)
const GG_CMD_FCC      :  u8 = 0x0E;  // full charge capacity (mAh)
const GG_CMD_AVGCUR   :  u8 = 0x10;  // (mA)
const GG_CMD_SBYCUR   :  u8 = 0x12;  // standby current (mA)
const GG_CMD_MAXCUR   :  u8 = 0x14;  // max load current (mA)
const GG_CMD_AVGPWR   :  u8 = 0x18;  // average power (mW)
const GG_CMD_SOC      :  u8 = 0x1C;  // state of charge in %
const GG_CMD_INTTEMP  :  u8 = 0x1E;  // temperature of gg IC
const GG_CMD_SOH      :  u8 = 0x20;  // state of health, num / %

// single-byte extended commands
const GG_EXT_BLKDATACTL  :  u8 = 0x61;  // block data control
const GG_EXT_BLKDATACLS  :  u8 = 0x3E;  // block data class
const GG_EXT_BLKDATAOFF  :  u8 = 0x3F;  // block data offset
const GG_EXT_BLKDATACHK  :  u8 = 0x60;  // block data checksum
const GG_EXT_BLKDATABSE  :  u8 = 0x40;  // block data base

// control command codes\
const GG_CODE_CTLSTAT :  u16 = 0x0000;
const GG_CODE_DEVTYPE :  u16 = 0x0001;
const GG_CODE_UNSEAL  :  u16 = 0x8000;
const GG_CODE_SEAL    :  u16 = 0x0020;
const GG_CODE_CFGUPDATE :  u16 = 0x0013;
const GG_CODE_RESET   :  u16 = 0x0042;
const GG_CODE_SET_HIB :  u16 = 0x0011;
const GG_CODE_CLR_HIB :  u16 = 0x0012;

const GG_UPDATE_INTERVAL_MS : u32 = 1000;
const GG_TIMEOUT_MS: u32 = 5;

pub mod api_gasgauge {

    fn gg_set(p: &betrusted_pac::Peripherals, cmd_code: u8, val: u16) {
        let txbuf: [u8; 3] = [cmd_code, (val & 0xff) as u8, ((val >> 8) & 0xff) as u8];

        hal_i2c::hal_i2c::i2c_master(p, BQ24157_ADDR, Some(&txbuf), None, GG_TIMEOUT_MS);
    }

    fn gg_set_byte(p: &betrusted_pac::Peripherals, cmd_code: u8, val: u16) {
        let txbuf: [u8; 2] = [cmd_code, (val & 0xff) as u8];

        hal_i2c::hal_i2c::i2c_master(p, BQ24157_ADDR, Some(&txbuf), None, GG_TIMEOUT_MS);
    }

    fn gg_get(p: &betrusted_pac::Peripherals, cmd_code: u8) -> u16 {
        let txbuf: [u8; 1] = [cmd_code];
        let rxbuf: [u8; 2] = [0, 0];

        hal_i2c::hal_i2c::i2c_master(p, BQ24157_ADDR, Some(&txbuf), Some(&mut rxbuf), GG_TIMEOUT_MS);

        rxbuf[0] as u16 | (rx[1] as u16 << 8)
    }   

    fn gg_get_byte(p: &betrusted_pac::Peripherals, cmd_code: u8) -> u8 {
        let txbuf: [u8; 1] = [cmd_code];
        let rxbuf: [u8; 1] = [0];

        hal_i2c::hal_i2c::i2c_master(p, BQ24157_ADDR, Some(&txbuf), Some(&mut rxbuf), GG_TIMEOUT_MS);

        rxbuf[0]
    }   

    pub fn gg_start(p: &betrusted_pac::Peripherals) { gg_set(p, GG_CMD_CNTL, GG_CODE_CLR_HIB);  }
    pub fn gg_set_hibernate(p: &betrusted_pac::Peripherals) { gg_set(p, GG_CMD_CNTL, GG_CODE_SET_HIB); }

    pub fn gg_voltage(p: &betrusted_pac::Peripherals) -> u16 {
        gg_get(p, GG_CMD_VOLT)
    }
}