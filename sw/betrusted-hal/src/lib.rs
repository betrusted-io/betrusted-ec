#![no_std]

extern crate bitflags;
extern crate volatile;
extern crate utralib;
extern crate riscv;

pub mod hal_hardi2c;
pub mod hal_i2c;
pub mod hal_time;
pub mod api_gasgauge;
pub mod api_charger;
pub mod api_lm3509;
pub mod api_lsm6ds3;
pub mod api_bq25618;
pub mod api_tusb320;
pub mod mem_locs;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
