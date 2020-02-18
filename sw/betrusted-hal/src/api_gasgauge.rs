#![allow(dead_code)]

use crate::hal_hardi2c::Hardi2c;

const BQ27421_ADDR : u8 = 0x55;

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
const GG_TIMEOUT_MS: u32 = 2;

fn gg_set(i2c: &mut Hardi2c, cmd_code: u8, val: u16) {
    let txbuf: [u8; 3] = [cmd_code, (val & 0xff) as u8, ((val >> 8) & 0xff) as u8];

    i2c.i2c_master(BQ27421_ADDR, Some(&txbuf), None, GG_TIMEOUT_MS);
}

fn gg_set_byte(i2c: &mut Hardi2c, cmd_code: u8, val: u8) {
    let txbuf: [u8; 2] = [cmd_code, val];

    i2c.i2c_master(BQ27421_ADDR, Some(&txbuf), None, GG_TIMEOUT_MS);
}

fn gg_get(i2c: &mut Hardi2c, cmd_code: u8) -> i16 {
    let txbuf: [u8; 1] = [cmd_code];
    let mut rxbuf: [u8; 2] = [0, 0];

    i2c.i2c_master(BQ27421_ADDR, Some(&txbuf), Some(&mut rxbuf), GG_TIMEOUT_MS);

    // don't do the sign conversion untl after the bytes are composited, sign extension of
    // of i8's would be inappropriate for this application
    (rxbuf[0] as u16 | (rxbuf[1] as u16) << 8) as i16
}   

fn gg_get_byte(i2c: &mut Hardi2c, cmd_code: u8) -> u8 {
    let txbuf: [u8; 1] = [cmd_code];
    let mut rxbuf: [u8; 1] = [0];

    i2c.i2c_master(BQ27421_ADDR, Some(&txbuf), Some(&mut rxbuf), GG_TIMEOUT_MS);

    rxbuf[0]
}   

pub fn gg_start(i2c: &mut Hardi2c) { gg_set(i2c, GG_CMD_CNTL, GG_CODE_CLR_HIB);  }
pub fn gg_set_hibernate(i2c: &mut Hardi2c) { gg_set(i2c, GG_CMD_CNTL, GG_CODE_SET_HIB); }
pub fn gg_voltage(i2c: &mut Hardi2c) -> i16 { gg_get(i2c, GG_CMD_VOLT) }
pub fn gg_avg_current(i2c: &mut Hardi2c) -> i16  { gg_get(i2c, GG_CMD_AVGCUR) }
pub fn gg_avg_power(i2c: &mut Hardi2c) -> i16  { gg_get(i2c, GG_CMD_AVGPWR) }
pub fn gg_remaining_capacity(i2c: &mut Hardi2c) -> i16  { gg_get(i2c, GG_CMD_RM) }
pub fn gg_state_of_charge(i2c: &mut Hardi2c) -> i16  { gg_get(i2c, GG_CMD_SOC) }

fn compute_checksum(blockdata: &[u8]) -> u8 {
    let mut checksum: u8 = 0;
    for i in 0..32 {
        checksum += blockdata[i];
    }

    255 - checksum
}

pub fn gg_device_type(i2c: &mut Hardi2c) -> i16 {
    gg_set(i2c, GG_CMD_CNTL, GG_CODE_DEVTYPE);
    gg_get(i2c, GG_CMD_CNTL)
}

pub fn gg_control_status(i2c: &mut Hardi2c) -> i16 {
    gg_set(i2c, GG_CMD_CNTL, GG_CODE_CTLSTAT);
    gg_get(i2c, GG_CMD_CNTL)
}

#[doc = "Set the design capacity of the battery. Returns previously assigned capacity."]
pub fn gg_set_design_capacity(i2c: &mut Hardi2c, mah: u16) -> u16 {
    // unseal the gasguage by writing the unseal command twice
    gg_set(i2c, GG_CMD_CNTL, GG_CODE_UNSEAL);
    gg_set(i2c, GG_CMD_CNTL, GG_CODE_UNSEAL);

    // set configuraton update command
    gg_set(i2c, GG_CMD_CNTL, GG_CODE_CFGUPDATE);

    loop {
        let flags : i16 = gg_get(i2c, GG_CMD_FLAG);
        if (flags & 0x10) != 0 { break; }
    }
    
    gg_set_byte(i2c, GG_EXT_BLKDATACTL, 0x0);    // enable block data memory control
    gg_set_byte(i2c, GG_EXT_BLKDATACLS, 0x52);   // set data class to 0x52 -- state subclass
    gg_set_byte(i2c, GG_EXT_BLKDATAOFF, 0x00);  // specify block data offset

    // read the existing data block, extract design capacity, then update and writeback
    let mut blockdata: [u8; 33] = [0; 33];
    for i in 0..32 { // skip checksum as we don't check it
        blockdata[i] = gg_get_byte(i2c, GG_EXT_BLKDATABSE + i as u8);
    }

    let design_capacty: u16 = (blockdata[11] as u16) | ((blockdata[10] as u16) << 8);

    blockdata[11] = (mah & 0xFF) as u8;
    blockdata[10] = ((mah >> 8) & 0xFF) as u8;
    blockdata[32] = compute_checksum(&blockdata);
    for i in 0..33 {
        gg_set_byte(i2c, GG_EXT_BLKDATABSE + i as u8, blockdata[i]);
    }

    // reset the gasguage to get the new data to take hold
    gg_set(i2c, GG_CMD_CNTL, GG_CODE_RESET);

    loop {
        let flags : i16 = gg_get(i2c, GG_CMD_FLAG);
        if (flags & 0x10) != 0 { break; }
    }

    // seal the gas gauge
    gg_set(i2c, GG_CMD_CNTL, GG_CODE_SEAL);

    design_capacty
}
