#![no_std]

extern crate utralib;
extern crate wfx_sys;
extern crate c_types;
extern crate debug;
extern crate betrusted_hal;
pub extern crate wfx_bindings;

pub mod hal_wf200;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
