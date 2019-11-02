#![no_std]

pub mod hal_i2c;
pub mod hal_time;


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
