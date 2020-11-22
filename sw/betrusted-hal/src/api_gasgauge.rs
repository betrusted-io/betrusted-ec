#![allow(dead_code)]

use crate::hal_hardi2c::Hardi2c;
use crate::hal_time::delay_ms;

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
const GG_EXT_DCAP_MSB    :  u8 = 0x3D;  // design capacity MSB
const GG_EXT_DCAP_LSB    :  u8 = 0x3C;  // design capacity LSB

// control command codes
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

    while i2c.i2c_controller(BQ27421_ADDR, Some(&txbuf), None, GG_TIMEOUT_MS) != 0 {}
}

fn gg_set_byte(i2c: &mut Hardi2c, cmd_code: u8, val: u8) {
    let txbuf: [u8; 2] = [cmd_code, val];

    while i2c.i2c_controller(BQ27421_ADDR, Some(&txbuf), None, GG_TIMEOUT_MS) != 0 {}
}

fn gg_get(i2c: &mut Hardi2c, cmd_code: u8) -> i16 {
    let txbuf: [u8; 1] = [cmd_code];
    let mut rxbuf: [u8; 2] = [0, 0];

    while i2c.i2c_controller(BQ27421_ADDR, Some(&txbuf), Some(&mut rxbuf), GG_TIMEOUT_MS) != 0 {}

    // don't do the sign conversion untl after the bytes are composited, sign extension of
    // of i8's would be inappropriate for this application
    (rxbuf[0] as u16 | (rxbuf[1] as u16) << 8) as i16
}

fn gg_get_byte(i2c: &mut Hardi2c, cmd_code: u8) -> u8 {
    let txbuf: [u8; 1] = [cmd_code];
    let mut rxbuf: [u8; 2] = [0, 0];

    while i2c.i2c_controller(BQ27421_ADDR, Some(&txbuf), Some(&mut rxbuf), GG_TIMEOUT_MS) != 0 {}
    rxbuf[0]
}

pub fn gg_start(i2c: &mut Hardi2c) { gg_set(i2c, GG_CMD_CNTL, GG_CODE_CLR_HIB);  }
pub fn gg_set_hibernate(i2c: &mut Hardi2c) { gg_set(i2c, GG_CMD_CNTL, GG_CODE_SET_HIB); }
pub fn gg_voltage(i2c: &mut Hardi2c) -> i16 { gg_get(i2c, GG_CMD_VOLT) }
pub fn gg_avg_current(i2c: &mut Hardi2c) -> i16  { gg_get(i2c, GG_CMD_AVGCUR) }
pub fn gg_avg_power(i2c: &mut Hardi2c) -> i16  { gg_get(i2c, GG_CMD_AVGPWR) }
pub fn gg_remaining_capacity(i2c: &mut Hardi2c) -> i16  { gg_get(i2c, GG_CMD_RM) }
pub fn gg_full_capacity(i2c: &mut Hardi2c) -> i16 { gg_get(i2c, GG_CMD_FCC) }
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
pub fn gg_set_design_capacity(i2c: &mut Hardi2c, mah: Option<u16>) -> u16 {
    let design_capacity: u16;
    if mah.is_some() {
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

        /*
        This is the desired result:
            00: 00 00 00 00 00 81 0e db
            08: 0e a8 04 4c 13 60 05 3c
            10: 0c 80 00 c8 00 32 00 14
            18: 03 e8 01 00 64 10 04 00
            20: dd
        */
        if true {
            // this targets all the bytes

            // read the existing data block, extract design capacity, then update and writeback
            let mut blockdata: [u8; 33] = [0; 33];
            for i in 0..33 {
                blockdata[i] = gg_get_byte(i2c, GG_EXT_BLKDATABSE + i as u8);
            }
            /*
            for i in 0..33 {
                if (i % 8) == 0 {
                    sprint!("\n\r{:02x}: ", i)
                }
                sprint!("{:02x} ", blockdata[i]);
            }
            sprintln!("");*/

            design_capacity = (blockdata[11] as u16) | ((blockdata[10] as u16) << 8);

            let newcap = mah.unwrap();
            blockdata[11] = (newcap & 0xFF) as u8;
            blockdata[10] = ((newcap >> 8) & 0xFF) as u8;
            blockdata[32] = compute_checksum(&blockdata);
            delay_ms(2); // some delay seems to be needed
            for i in 0..33 {
                gg_set_byte(i2c, GG_EXT_BLKDATABSE + i as u8, blockdata[i]);
            }
            delay_ms(2); // some delay seems to be needed
            /*
            for i in 0..33 {
                if (i % 8) == 0 {
                    sprint!("\n\r{:02x}: ", i)
                }
                sprint!("{:02x} ", blockdata[i]);
            }
            sprintln!("");*/
        } else {
            // this targets just the capacity bytes per bq27421-G1 technical reference
            let old_csum = gg_get_byte(i2c, GG_EXT_BLKDATABSE + 0x20);
            let dc_msb = gg_get_byte(i2c, GG_EXT_BLKDATABSE + 0xA);
            let dc_lsb = gg_get_byte(i2c, GG_EXT_BLKDATABSE + 0xB);
            design_capacity = ((dc_msb as u16) << 8) | dc_lsb as u16;
            let newcap = mah.unwrap();
            gg_set_byte(i2c, GG_EXT_BLKDATABSE + 0xA, ((newcap >> 8) & 0xff) as u8);
            gg_set_byte(i2c, GG_EXT_BLKDATABSE + 0xB, (newcap & 0xff) as u8);
            let temp = 255 - old_csum - dc_msb - dc_lsb;
            let new_csum = 255 - (temp + (newcap & 0xff) as u8 + ((newcap >> 8) & 0xff) as u8);
            gg_set_byte(i2c, GG_EXT_BLKDATABSE + 0x20, new_csum);
        }

        // reset the gasguage to get the new data to take hold
        gg_set(i2c, GG_CMD_CNTL, GG_CODE_RESET);

        loop {
            let flags : i16 = gg_get(i2c, GG_CMD_FLAG);
            if (flags & 0x10) != 0 { break; }
        }

        // seal the gas gauge
        gg_set(i2c, GG_CMD_CNTL, GG_CODE_SEAL);
    } else {
        design_capacity = ((gg_get_byte(i2c, GG_EXT_DCAP_MSB) as u16) << 8) | gg_get_byte(i2c, GG_EXT_DCAP_LSB) as u16;
    }

    design_capacity
}
