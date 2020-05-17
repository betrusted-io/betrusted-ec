#![no_std]

extern crate gyro_sys;
extern crate c_types;
extern crate betrusted_hal;
extern crate betrusted_pac;
pub extern crate gyro_bindings;

pub mod hal_gyro;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
