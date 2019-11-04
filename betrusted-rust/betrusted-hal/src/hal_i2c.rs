pub mod hal_i2c {
    use crate::hal_time::hal_time::get_time_ms;

    pub fn i2c_init(p: &betrusted_pac::Peripherals, clockmhz: u32) {
        let clkcode: u32 = (clockmhz * 1_000_000) / (5 * 100_000) - 1;

        // set the prescale assuming 100MHz cpu operation: 100MHz / ( 5 * 100kHz ) - 1 = 199
        unsafe{p.I2C.prescale0.write( |w| {w.bits(clkcode & 0xFF)}); }
        unsafe{p.I2C.prescale1.write( |w| {w.bits((clkcode >> 8) & 0xFF)}); }

        // enable the block
        p.I2C.control.write( |w| {w.en().bit(true)});
    }

    fn i2c_tip_wait(p: &betrusted_pac::Peripherals, timeout_ms: u32) -> u32 {
        let starttime: u32 = get_time_ms(p);

        // wait for TIP to go high
        loop {
            if p.I2C.status.read().tip().bit() == true {
                break;
            }
            if get_time_ms(p) > starttime + timeout_ms {
                unsafe{p.I2C.command.write( |w| {w.bits(0)}); }
                return 1;
            }
        }

        // wait for tip to go low
        loop {
            if p.I2C.status.read().tip().bit() == false {
                break;
            }
            if get_time_ms(p) > starttime + timeout_ms {
                unsafe{p.I2C.command.write( |w| {w.bits(0)}); }
                return 1;
            }
        }
        unsafe{p.I2C.command.write( |w| {w.bits(0)}); }

        0
    }

    pub fn i2c_master(p: &betrusted_pac::Peripherals, addr: u8, txbuf: &[u8], rxbuf: &mut [u8], timeout_ms: u32) -> u32 {
        let mut ret: u32 = 0;

        // write half
        if txbuf.len() > 0 {
            unsafe{ p.I2C.txr.write( |w| {w.bits( (addr << 1 | 0) as u32 )}); }
            p.I2C.command.write( |w| {w.sta().bit(true).wr().bit(true)});

            ret += i2c_tip_wait(p, timeout_ms);

            let mut i: usize = 0;
            loop {
                if i == txbuf.len() as usize {
                    break;
                }
                if p.I2C.status.read().rx_ack().bit() {
                    ret += 1;
                }
                unsafe{ p.I2C.txr.write( |w| {w.bits( (txbuf[i]) as u32 )}); }
                if i == txbuf.len() - 1 && rxbuf.len() == 0 {
                    p.I2C.command.write( |w| {w.wr().bit(true).sto().bit(true)});
                } else {
                    p.I2C.command.write( |w| {w.wr().bit(true)});
                }
                ret += i2c_tip_wait(p, timeout_ms);
                i += 1;
            }
            if p.I2C.status.read().rx_ack().bit() {
                ret += 1;
            }
        }

        // read half
        if rxbuf.len() > 0 {
            unsafe{ p.I2C.txr.write( |w| {w.bits( (addr << 1 | 1) as u32 )}); }
            p.I2C.command.write( |w| {w.sta().bit(true).wr().bit(true)});

            ret += i2c_tip_wait(p, timeout_ms);

            let mut i: usize = 0;
            loop {
                if i == rxbuf.len() as usize {
                    break;
                }
                if i == rxbuf.len() - 1 {
                    p.I2C.command.write( |w| {w.rd().bit(true).ack().bit(true).sto().bit(true)});
                } else {
                    p.I2C.command.write( |w| {w.rd().bit(true)});
                }
                ret += i2c_tip_wait(p, timeout_ms);
                rxbuf[i] = p.I2C.rxr.read().bits() as u8;
                i += 1;
            }
        }

        ret
    }
}