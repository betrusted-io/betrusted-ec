#![no_std]

pub mod hal_i2c;
pub mod hal_time;
pub mod api_gasgauge;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
