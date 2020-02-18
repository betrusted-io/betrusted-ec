use bitflags::*;
use volatile::*;
use crate::hal_time::get_time_ticks_trunc;
use crate::hal_time::delay_ms;

#[used]  // This is necessary to keep DBGSTR from being optimized out
static mut I2C_DBGSTR: [u32; 8] = [0; 8];
//  print/x betrusted_hal::hal_hardi2c::I2C_DBGSTR

// wishbone bus width is natively 32-bits, and to simplify
// implementation we just throw away the top 24 bits and stride
// the hard I2C register bank on word boundaries. Thus address
// offset should be multiplied by 4 to match this implementation point.
pub const HARDI2C_CONTROL:      usize = 0b1000 * 4;
pub const HARDI2C_COMMAND:      usize = 0b1001 * 4;
pub const HARDI2C_PRESCALE_LSB: usize = 0b1010 * 4;
pub const HARDI2C_PRESCALE_MSB: usize = 0b1011 * 4;
pub const HARDI2C_STATUS:       usize = 0b1100 * 4;
pub const HARDI2C_TXD:          usize = 0b1101 * 4;
pub const HARDI2C_RXD:          usize = 0b1110 * 4;
pub const HARDI2C_GENCALL:      usize = 0b1111 * 4;
pub const HARDI2C_IRQEN:        usize = 0b0111 * 4;
pub const HARDI2C_IRQSTAT:      usize = 0b0110 * 4;

pub const HARDI2C_BASE: usize = 0xB000_0000;

bitflags! {
    pub struct Control: u32 {
        const I2CEN             = 0b1000_0000;
        const GCEN              = 0b0100_0000;
        const WKUPEN            = 0b0010_0000;
        const SDA_DEL_SEL_0NS   = 0b0000_1100;
        const SDA_DEL_SEL_75NS  = 0b0000_1000;
        const SDA_DEL_SEL_150NS = 0b0000_0100;
        const SDA_DEL_SEL_300NS = 0b0000_0000;
    }
}

bitflags! {
    pub struct IrqMask: u32 {
        const IRQINTCLREN      = 0b1000_0000;
        const IRQINTFRC        = 0b0100_0000;
        const IRQARBLEN        = 0b0000_1000;
        const IRQTRRDYEN       = 0b0000_0100;
        const IRQTROEEN        = 0b0000_0010;
        const IRQHGCEN         = 0b0000_0001;
    }
}

bitflags! {
    pub struct Status: u32 {
        const TIP     = 0b1000_0000; // transmit in progress
        const BUSY    = 0b0100_0000; // busy -- flags only valid if this is set
        const RARC    = 0b0010_0000; // received an ACK
        const SRW     = 0b0001_0000; // if set, we are a slave
        const ARBL    = 0b0000_1000; // arbitration lost
        const TRRDY   = 0b0000_0100; // tx or rx registers ready
        const TROE    = 0b0000_0010; // tx or rx overrun, or NACK
        const HGC     = 0b0000_0001; // hardware general call received
    }
}

bitflags! {
    pub struct Command: u32 {
        const STA     = 0b1000_0000;
        const STO     = 0b0100_0000;
        const RD      = 0b0010_0000;
        const WR      = 0b0001_0000;
        const ACK     = 0b0000_1000;
        const CKSDIS  = 0b0000_0100;
        const RBUFDIS = 0b0000_0010;
    }
}

bitflags! {
    pub struct IrqStat: u32 {
        const IRQARBL  = 0b1000;
        const IRQTRRDY = 0b0100;
        const IRQTROE  = 0b0010;
        const IRQHGC   = 0b0001;
    }
}

pub struct Hardi2c {
    p: betrusted_pac::Peripherals,
    control: *mut Volatile <u32>,
    prescale_lsb: *mut Volatile <u32>,
    prescale_msb: *mut Volatile <u32>,
    irqen: *mut Volatile <u32>,
    status: *mut Volatile <u32>,
    command: *mut Volatile <u32>,
    txd: *mut Volatile <u32>,
    rxd: *mut Volatile <u32>,
    irqstat: *mut Volatile <u32>,
}

impl Hardi2c {
    pub fn new() -> Self {
        unsafe {
            Hardi2c {
                p: betrusted_pac::Peripherals::steal(),
                control: ((HARDI2C_BASE + HARDI2C_CONTROL) as *mut u32) as *mut Volatile <u32>,
                prescale_lsb: ((HARDI2C_BASE + HARDI2C_PRESCALE_LSB) as *mut u32) as *mut Volatile <u32>,
                prescale_msb: ((HARDI2C_BASE + HARDI2C_PRESCALE_MSB) as *mut u32) as *mut Volatile <u32>,
                irqen: ((HARDI2C_BASE + HARDI2C_IRQEN) as *mut u32) as *mut Volatile <u32>,
                status: ((HARDI2C_BASE + HARDI2C_STATUS) as *mut u32) as *mut Volatile <u32>,
                command: ((HARDI2C_BASE + HARDI2C_COMMAND) as *mut u32) as *mut Volatile <u32>,
                txd: ((HARDI2C_BASE + HARDI2C_TXD) as *mut u32) as *mut Volatile <u32>,
                rxd: ((HARDI2C_BASE + HARDI2C_RXD) as *mut u32) as *mut Volatile <u32>,
                irqstat: ((HARDI2C_BASE + HARDI2C_IRQSTAT) as *mut u32) as *mut Volatile <u32>,
            }
        }
    }

    // clock_hz is clock specified in Hz
    pub fn i2c_init(&mut self, clock_hz: u32) {
        // writes to PRESCALE_MSB causes a core reset, and the prescale value to be latched
        // the clock setting is equal to sysclock / (4 * I2C_PRESCALE)
        let clock_code = (clock_hz / 100_000) / 4;
    
        // write the LSB first, as the MSB triggers the core reset
        // presumably this *loads* the prescaler values, and doesn't clear it -- need to check with oscope
        unsafe{ (*self.prescale_lsb).write( clock_code & 0xFF ); }
        unsafe{ (*self.prescale_msb).write( (clock_code >> 8) & 0x3 ); }
    
        // enable the block
        unsafe{ (*self.control).write((Control::I2CEN | Control::SDA_DEL_SEL_0NS).bits()); }
        // disable interrupts
        unsafe{ (*self.irqen).write(0); }
        // clear irqstat
        unsafe{ (*self.irqstat).write((IrqStat::IRQARBL | IrqStat::IRQTRRDY | IrqStat::IRQTROE | IrqStat::IRQHGC).bits()); }
    }

    /// Wait for trrdy or srw to go true. trrdy = false => wait for srw [FIXME] make this interrupt driven, not polled
    fn i2c_wait(&mut self, flag: u32, timeout_ms: u32) -> u32 {
        let starttime: u32 = get_time_ticks_trunc(&self.p);

        while (unsafe{ (*self.status).read() } & flag) == 0 {
            let curtime: u32 = get_time_ticks_trunc(&self.p);

            if curtime >= starttime {
                if (curtime - starttime) > timeout_ms {
                    unsafe{ (*self.command).write(Command::CKSDIS.bits()); }
                    unsafe{ I2C_DBGSTR[7] = 42; }
                    return 1;
                }
            } else {  // deal with roll-over
                if (curtime + (0xFFFF_FFFF - starttime)) > timeout_ms {
                    unsafe{ (*self.command).write(Command::CKSDIS.bits()); }
                    unsafe{ I2C_DBGSTR[7] = 42; }
                    return 1;
                }
            }
        }
        0
    }

    /// opposite polarity as above; don't generalize because the extra code can hurt wait loop timing
    fn i2c_wait_n(&mut self, flag: u32, timeout_ms: u32) -> u32 {
        let starttime: u32 = get_time_ticks_trunc(&self.p);

        while (unsafe{ (*self.status).read() } & flag) != 0 {
            let curtime: u32 = get_time_ticks_trunc(&self.p);

            if curtime >= starttime {
                if (curtime - starttime) > timeout_ms {
                    //unsafe{ (*self.command).write(Command::CKSDIS.bits()); }
                    unsafe{ I2C_DBGSTR[7] = 42; }
                    return 1;
                }
            } else {  // deal with roll-over
                if (curtime + (0xFFFF_FFFF - starttime)) > timeout_ms {
                    //unsafe{ (*self.command).write(Command::CKSDIS.bits()); }
                    unsafe{ I2C_DBGSTR[7] = 42; }
                    return 1;
                }
            }
        }
        0
    }

    /// The primary I2C interface call. This version currently blocks until the transaction is done.
    pub fn i2c_master(&mut self, addr: u8, txbuf: Option<&[u8]>, rxbuf: Option<&mut [u8]>, timeout_ms: u32) -> u32 {
        let mut ret: u32 = 0;
        
        // hoist these up to optimize performance a bit
        let do_rx: bool = rxbuf.is_some();
        let rxbuf_checked : &mut [u8] = rxbuf.unwrap();
    
        // write half
        if txbuf.is_some() {
            let txbuf_checked : &[u8] = txbuf.unwrap();

            unsafe{ (*self.txd).write((addr << 1 | 0) as u32); }
            // trrdy should drop when data is accepted
            ret += self.i2c_wait_n(Status::TRRDY.bits(), timeout_ms);
            unsafe{ I2C_DBGSTR[3] = (*self.status).read(); }
            // issue write+start
            unsafe{ (*self.command).write((Command::STA | Command::WR | Command::CKSDIS).bits()); }
            
            for i in 0..txbuf_checked.len() {
                // when trrdy goes high again, it's ready to accept the next datum
                ret += self.i2c_wait((Status::TRRDY | Status::BUSY).bits(), timeout_ms);
                ret += self.i2c_wait_n(Status::TIP.bits(), timeout_ms); // wait until the transaction in progress is done
                
                // write data
                unsafe{ (*self.txd).write(txbuf_checked[i] as u32); }
                
                // wait for trrdy to go low to indicate data was written
                //ret += self.i2c_wait_n(Status::TRRDY.bits(), timeout_ms);

                // now issue the write command
                unsafe{ (*self.command).write((Command::WR | Command::CKSDIS).bits()); }
                /*
                for _ in 0..20 {
                    unsafe{ I2C_DBGSTR[1] += (*self.status).read(); }
                    unsafe{ I2C_DBGSTR[6] = I2C_DBGSTR[6] + 1; }
                }*/

                if (i == (txbuf_checked.len() - 1)) && !do_rx {
                    // trrdy going high indicates command was accepted
                    ret += self.i2c_wait((Status::TRRDY | Status::BUSY).bits(), timeout_ms);
                    // now issue 'stop' command
                    unsafe{ (*self.command).write((Command::STO | Command::CKSDIS).bits()); }
                    // wait until busy drops, indicates we are done with write-phase
                    unsafe{ I2C_DBGSTR[2] = (*self.status).read(); }
                    ret += self.i2c_wait_n(Status::BUSY.bits(), timeout_ms);
                }
            }
        }

        // read half
        if do_rx {
            ret += self.i2c_wait((Status::TRRDY).bits(), timeout_ms);
            unsafe{ (*self.txd).write((addr << 1 | 1) as u32); }
            unsafe{ (*self.command).write((Command::STA | Command::WR | Command::CKSDIS).bits()); }
    
            ret += self.i2c_wait(Status::SRW.bits(), timeout_ms);
            //ret += self.i2c_wait_n(Status::TIP.bits(), timeout_ms);
            unsafe{ (*self.command).write((Command::RD).bits()); }
    
            for i in 0..rxbuf_checked.len() {
                if i == (rxbuf_checked.len() - 1) {
                    /*
                    if rxbuf_checked.len() == 1 {
                        // real time delay requirement if only one byte read
                        // 2 * tSCL min, 7 * tSCL max: 20-70 microseconds, e.g. 240 cycles @ 12MHz
                        // 1 tick is one tSCL, but with overhead, we should be > 2
                        delay_ticks(&self.p, 2);
                    } */
                    //ret += self.i2c_wait_n(Status::TIP.bits(), timeout_ms);
                    unsafe{ (*self.command).write((Command::RD | Command::STO | Command::ACK | Command::CKSDIS).bits()); }
                    ret += self.i2c_wait(Status::TRRDY.bits(), timeout_ms);
                    //ret += self.i2c_wait_n(Status::BUSY.bits(), timeout_ms);
                    rxbuf_checked[i] = unsafe{ (*self.rxd).read() } as u8;
                } else {
                    ret += self.i2c_wait(Status::TRRDY.bits(), timeout_ms);
                    rxbuf_checked[i] = unsafe{ (*self.rxd).read() } as u8;
                    // RD command implicitly repeats
                }
            }
        }
    
        ret
    }    
}
