#![no_std]

extern crate bitflags;
extern crate volatile;

pub mod hal_hardi2c;
pub mod hal_time;
pub mod api_gasgauge;
pub mod api_charger;
pub mod api_lm3509;
pub mod api_bq25618;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
