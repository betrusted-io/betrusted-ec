use utralib::generated::*;
use crate::hal_time::get_time_ms;

pub struct Hardi2c {
    csr: CSR::<u32>,
}

impl Hardi2c {
    pub fn new() -> Self {
        Hardi2c {
            csr: CSR::new(HW_I2C_BASE as *mut u32),
        }
    }
    pub fn i2c_init(&mut self, clock_hz: u32) {
        let clkcode: u32 = clock_hz / (5 * 100_000) - 1;
        // set the prescale assuming 100MHz cpu operation: 100MHz / ( 5 * 100kHz ) - 1 = 199
        self.csr.wfo(utra::i2c::PRESCALE_PRESCALE, clkcode);

        // enable the block
        self.csr.wfo(utra::i2c::CONTROL_EN, 1);
    }
    // [FIXME] this is a stupid polled implementation of I2C transmission. Once we have
    // threads and interurpts, this should be refactored to be asynchronous
    /// Wait until a transaction in progress ends. [FIXME] would be good to yield here once threading is enabled."
    fn i2c_tip_wait(&mut self, timeout_ms: u32) -> u32 {
        let starttime: u32 = get_time_ms();

        // wait for TIP to go high
        loop {
            if self.csr.rf(utra::i2c::STATUS_TIP) == 1 {
                break;
            }
            if get_time_ms() > starttime + timeout_ms {
                self.csr.wo(utra::i2c::COMMAND, 0);
                return 1;
            }
        }

        // wait for tip to go low
        loop {
            if self.csr.rf(utra::i2c::STATUS_TIP) == 0 {
                break;
            }
            if get_time_ms() > starttime + timeout_ms {
                self.csr.wo(utra::i2c::COMMAND, 0);
                return 1;
            }
        }
        self.csr.wo(utra::i2c::COMMAND, 0);

        0
    }
    /// The primary I2C interface call. This version currently blocks until the transaction is done.
    pub fn i2c_controller(&mut self, addr: u8, txbuf: Option<&[u8]>, rxbuf: Option<&mut [u8]>, timeout_ms: u32) -> u32 {
        let mut ret: u32 = 0;

        // write half
        if txbuf.is_some() {
            let txbuf_checked : &[u8] = txbuf.unwrap();
            self.csr.wo(utra::i2c::TXR, (addr << 1 | 0) as u32);
            self.csr.wo(utra::i2c::COMMAND,
                self.csr.ms(utra::i2c::COMMAND_STA, 1)
                | self.csr.ms(utra::i2c::COMMAND_WR, 1)
            );

            ret += self.i2c_tip_wait(timeout_ms);

            for i in 0..txbuf_checked.len() {
                if self.csr.rf(utra::i2c::STATUS_RXACK) == 1 {
                    ret += 1;
                }
                self.csr.wo(utra::i2c::TXR, (txbuf_checked[i]) as u32);
                if (i == (txbuf_checked.len() - 1)) && rxbuf.is_none() {
                    self.csr.wo(utra::i2c::COMMAND,
                        self.csr.ms(utra::i2c::COMMAND_STO, 1)
                        | self.csr.ms(utra::i2c::COMMAND_WR, 1)
                    );
                } else {
                    self.csr.wfo(utra::i2c::COMMAND_WR, 1);
                }
                ret += self.i2c_tip_wait(timeout_ms);
            }
            if self.csr.rf(utra::i2c::STATUS_RXACK) == 1 {
                ret += 1;
            }
        }

        // read half
        if rxbuf.is_some() {
            let rxbuf_checked : &mut [u8] = rxbuf.unwrap();
            self.csr.wo(utra::i2c::TXR, (addr << 1 | 1) as u32);
            self.csr.wo(utra::i2c::COMMAND,
                self.csr.ms(utra::i2c::COMMAND_STA, 1)
                | self.csr.ms(utra::i2c::COMMAND_WR, 1)
            );

            ret += self.i2c_tip_wait(timeout_ms);

            for i in 0..rxbuf_checked.len() {
                if i == (rxbuf_checked.len() - 1) {
                    self.csr.wo(utra::i2c::COMMAND,
                        self.csr.ms(utra::i2c::COMMAND_STO, 1)
                        | self.csr.ms(utra::i2c::COMMAND_RD, 1)
                        | self.csr.ms(utra::i2c::COMMAND_ACK, 1)
                    );
                } else {
                    self.csr.wfo(utra::i2c::COMMAND_RD, 1);
                }
                ret += self.i2c_tip_wait(timeout_ms);
                rxbuf_checked[i] = self.csr.r(utra::i2c::RXR) as u8;
            }
        }
        ret
    }
}
