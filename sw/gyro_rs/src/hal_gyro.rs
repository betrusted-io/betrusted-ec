#![allow(unused)]
#![allow(nonstandard_style)]

use crate::betrusted_hal::hal_time::delay_ms;
use crate::betrusted_hal::hal_time::get_time_ms;
use crate::betrusted_pac;
use crate::betrusted_hal::hal_hardi2c::Hardi2c;
use crate::gyro_bindings;
use xous_nommu::syscalls::*;
use core::slice;
use core::str;

#[macro_use]
use core::include_bytes;

pub use gyro_bindings::*;

const GYRO_TIMEOUT_MS: u32 = 1;

static mut GYRO_CONTEXT: stmdev_ctx_t = stmdev_ctx_t {
    write_reg: Some(betrusted_lsm6ds3_write_reg),
    read_reg: Some(betrusted_lsm6ds3_read_reg),
    handle: ::core::ptr::null::<c_types::c_void> as *mut c_types::c_void,
};

#[export_name = "betrusted_lsm6ds3_read_reg"]
pub unsafe extern "C" fn betrusted_lsm6ds3_read_reg(ctx: *mut core::ffi::c_void, reg: u8, data: *mut u8, len: u16) -> i32 {
    let mut i2c = Hardi2c::new();
    let mut rxbuf: &mut [u8] = slice::from_raw_parts_mut(data, len as usize);
    i2c.i2c_master_read_ffi((LSM6DS3_I2C_ADD_H >> 1) as u8, reg, &mut rxbuf, GYRO_TIMEOUT_MS) as i32
}
#[export_name = "betrusted_lsm6ds3_write_reg"]
pub unsafe extern "C" fn betrusted_lsm6ds3_write_reg (ctx: *mut core::ffi::c_void, reg: u8, data: *mut u8, len: u16) -> i32 {
    let mut i2c = Hardi2c::new();
    let databuf: &[u8] = slice::from_raw_parts(data, len as usize);

    i2c.i2c_master_write_ffi((LSM6DS3_I2C_ADD_H >> 1) as u8, reg, &databuf, GYRO_TIMEOUT_MS) as i32
}

pub struct BtGyro {
    pub context: stmdev_ctx_t,
    pub id: u8,
    pub x: u16,
    pub y: u16,
    pub z: u16,
}

impl BtGyro {
    pub fn new() -> Self {
        unsafe {
            BtGyro{ context: GYRO_CONTEXT, id: 0, x: 0, y: 0, z: 0 }
        }
    }

    pub fn init(&mut self) -> bool {
        let mut id: u8 = 0;
        unsafe{ lsm6ds3_device_id_get(&mut self.context, &mut id); }
        self.id = id;
        unsafe {
        //    lsm6ds3_reset_set(&mut self.context, PROPERTY_ENABLE as u8);
        //    let mut rst: u8 = 1;
        //    while rst != 0 {
        //        lsm6ds3_reset_get(&mut self.context, &mut rst);
        //    }
        //   lsm6ds3_block_data_update_set(&mut self.context, PROPERTY_ENABLE as u8);

        //    lsm6ds3_xl_full_scale_set(&mut self.context, lsm6ds3_xl_fs_t_LSM6DS3_2g);
        //    lsm6ds3_gy_full_scale_set(&mut self.context, lsm6ds3_fs_g_t_LSM6DS3_2000dps);

        //    lsm6ds3_xl_data_rate_set(&mut self.context, lsm6ds3_odr_xl_t_LSM6DS3_XL_ODR_12Hz5);
        //    lsm6ds3_gy_data_rate_set(&mut self.context, lsm6ds3_odr_g_t_LSM6DS3_GY_ODR_12Hz5);
        }

        // the ffi calls from the example code is bunk. hard code some sane defaults, debug FFI some other day.
        let mut i2c = Hardi2c::new();
        // reset ctrl3 to sane defaults
        let txbuf: [u8; 2] = [0x12, 0x4];
        i2c.i2c_master((LSM6DS3_I2C_ADD_H >> 1) as u8, Some(&txbuf), None, GYRO_TIMEOUT_MS);

        // reset ctrl1 to sane defaults
        let txbuf: [u8; 2] = [0x10, 0x10];
        i2c.i2c_master((LSM6DS3_I2C_ADD_H >> 1) as u8, Some(&txbuf), None, GYRO_TIMEOUT_MS);

        // turn off XL_HM_MODE
        let txbuf: [u8; 2] = [0x15, 0x01];
        i2c.i2c_master((LSM6DS3_I2C_ADD_H >> 1) as u8, Some(&txbuf), None, GYRO_TIMEOUT_MS);

        // turn on XL
        let txbuf: [u8; 2] = [0x18, 0x3c];
        i2c.i2c_master((LSM6DS3_I2C_ADD_H >> 1) as u8, Some(&txbuf), None, GYRO_TIMEOUT_MS);
        /*
    */
        true
    }

    pub fn update_xyz(&mut self) -> bool {
        //let mut data: [u16; 3] = [3, 2, 1];
        //unsafe {
        //   lsm6ds3_acceleration_raw_get(&mut self.context, data.as_ptr() as *mut u8);
        //}
        let mut data: [u8; 6] = [0xff; 6];
        let mut i2c = Hardi2c::new();
        i2c.i2c_master_read_ffi((LSM6DS3_I2C_ADD_H >> 1) as u8, 0x28, &mut data, GYRO_TIMEOUT_MS);

        self.x = data[0] as u16 | ((data[1] as u16) << 8);
        self.y = data[2] as u16 | ((data[3] as u16) << 8);
        self.z = data[4] as u16 | ((data[5] as u16) << 8);
        true
    }
}