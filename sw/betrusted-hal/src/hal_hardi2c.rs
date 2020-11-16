use bitflags::*;
use volatile::*;
use crate::hal_time::get_time_ticks_trunc;

#[used]  // This is necessary to keep DBGSTR from being optimized out
static mut I2C_DBGSTR: [u32; 8] = [0; 8];
//  print/x betrusted_hal::hal_hardi2c::I2C_DBGSTR

/// Hard I2C block driver
/// The good news:
///  - Going from pure RTL to SB_I2C saves about 180 LC on an ICE40 UP5K (3.5%). There's actually
///    quite a few extra LC that could be shaven out I think by letting the top 24 bits of the data
///    bus go undefined.
///
/// The bad news:
///  - The vendor docs http://www.latticesemi.com/-/media/LatticeSemi/Documents/ApplicationNotes/AD/AdvancediCE40SPII2CHardenedIPUsageGuide.ashx?document_id=50117 
///    are only semi-accurate
///  - There are some significant limitations in using the SB_I2C block
///
/// The TL;DR is I would only recommend using the SB_I2C block if and only if you are really
/// out of gates and this is the only way to optimize a few LC out of the design.
///
/// The flow chart and timing diagrams on page 24-30 of the PDF file linked above are pretty
/// handy, except they are wrong.
///
/// The biggest trick in driving the block is that the block requires the host to intervene in
/// real-time to guide the I2C transaction. In other words:
///
/// 1. The current command in the command register is sticky. Once you have set a command
///    in the register, that command is repeatedly issued until it is disabled or changed.
/// 2. It takes some time for new commands and parameters to be accepted by the block.
///    In other words, it is not a valid strategy to simply jam a "write" command into
///    the block for one cycle, and then clear it work around (1). The "write" command needs
///    to exist for somewhere longer than one I2C bus cycle, and also needs to be cleared
///    within about one I2C bus cycle of the command's completion to avoid it issuing again.
///
/// This means you have race conditions on the "fast" side (making sure the commands are around
/// long enough to be accepted by the I2C block), and "slow" side (making sure the command are
/// cleared soon enough to avoid issuing a second command by accident).
///
/// The upshot is that if you're driving this with a Vexriscv block running at 12MHz,
/// you don't actually have a lot of margin to play with. Interrupts may not be serviced
/// in time to meet the "slow" constraint; but at 12MHz, it's "fast" enough that you can't
/// simply fire and forget.
///
/// Thus the driver in essence requires a lot of polling to be done to make sure everything
/// occurs in exact lock-step.
///
/// The documentation in the PDF would hint that TRRDY is the one register to watch to
/// synchronize things. The TRRDY bit indicates that the Rx or Tx register (depending on the mode)
/// has been copied into the I2C hard IP block, and the host is now safe to update the contents
/// (or read it for Rx).
///
/// If you implement the flowchart on page 24 exactly as shown, what you end up with is just the
/// very first cycle of either a read or write being produced, and all subsequent cycles are skipped.
///
/// For writes, not only do you have to wait until TRRDY is asserted, you need to wait until
/// the initial slave address transaction (concurrent with the "STA" bit) indicates completion
/// (by monitoring the TIP bit). If you simply load the next value into Txd and issue a write
/// command upon TRRDY, the system will ignore it.
///
/// However, once you have completed that, you can now monitor TRRDY and issue WR commands
/// to issue successive writes.
///
/// In other words, the flow chart needs to be modified to say "Wait for TIP to clear" after the
/// initial "TXDR/CMDR" box in order to be correct.
///
/// I have found that "repeated-start" commands also don't work. A "repeated-start" leads to (iirc)
/// the read data cycles following the "read start" command to disappear. Thus, the work-around is
/// to always conclude every write phase with a "full stop", before moving onto the read phase.
/// Minor performance loss, no biggie.
///
/// For the read side, the document does accurately specify to wait for "SRW" instead of "TRRDY"
/// after the initial "STA+R" command. The read side flow chart is otherwise mostly correct except
/// that I couldn't get the "1 byte read" condition to work. Because of the double-buffering they
/// put on the I2C read side, there is an even stricter race condition imposed, where you must
/// issue the "RD+NACK+STOP" command within a 2*tSCL to 7*tSCL window, or else things blow up
/// (usually ends up with a weird runt cycle or the I2C block gets hung clocking SCL forever).
///
/// I was unable to find a way to reliably hit the 2*tSCL to 7*tSCL window. I had increased the
/// hardware timer precision and tried various combinations off of that, but it always seemed I
/// was either too fast or too slow. If your system uses caching, does XIP from SPI, or has interrupts,
/// that would also cause this to blow up.
///
/// Thus, my final driver implementation works around this by simply not allowing single-byte reads.
/// I have an "assert" in the code to catch that, but another valid way to deal with it might be to
/// simply issue two reads even if a single read is requested. In many cases, this is harmless for
/// the I2C device to read an extra byte, and the main impact would be e.g. if you were relying on
/// the position of the read address pointer to increment by only one byte in the target device.
/// Fortunately for my application, this is not the case so I didn't have to solve this last detail.
///
/// I will note that there is a "RBUFDIS" function that is not well documented that might solve
/// the above problem. In the flow chart examples, they always set CKSDIS but don't explain why;
/// I just do it in my code because that's what they recommended. I imagine that if you set the
///  RBUFDIS signal, you would no longer have that weird race condition anymore on the RD+STO+NACK
/// cycle, but instead you'd have another race condition timing when to read the data out of the
/// Rxd register. I didn't want to find out which was worse, but this foot note is here for anyone who decides they absolutely must have the ability to read a single byte from a slave device using this hard IP block.
///
/// Finally, I put some diagnostics in my code to check how often we hit time-outs at places I
/// wouldn't expect them, and I also explicitly wait for things like TRRDY to go "not ready"
/// even though the flow chart doesn't call for it to ensure proper interlocking. Despite these
/// measures, a small fraction of the I2C operations still trigger time-outs and fail.
/// Therefore, all the calls to the I2C API in my implementation now check the return code and
/// retry the operation if there is a failure. I did not go into why I had the rare time-outs,
/// or what causes them, because my targets all support stateless read/write, e.g., I can afford
/// to just keep on retrying the read or write until it works, but not all I2C targets are like this.
///
/// There is a mysterious "SDA delay" parameter, and apparently there is some mention of a
/// "glitch filter" elsewhere that is a hard IP block that seems like it was meant to be used
/// with the SB_I2C and it may be instantiated by the proprietary tool and perhaps adding these
/// or tuning these parameters would solve the reliability problems, but the docs are sparse on this.
///
/// Note that ironically, many of these software limitations could be worked around by wrapping some more
/// gates around the SB_I2C block, but by the time you do that, you lose that narrow 180 LC margin over
/// a pure-RTL implementation. So, basically, going to this block is to be done only as a last resort,
/// when you really need to wring a few gates out of a design, and you don't mind taking some
/// significant caveats on I2C functionality.

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
        Hardi2c {
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
        unsafe{ (*self.control).write((Control::I2CEN | Control::SDA_DEL_SEL_75NS).bits()); }
        // disable interrupts
        unsafe{ (*self.irqen).write(0); }
        // clear irqstat
        unsafe{ (*self.irqstat).write((IrqStat::IRQARBL | IrqStat::IRQTRRDY | IrqStat::IRQTROE | IrqStat::IRQHGC).bits()); }
    }

    /// Wait for trrdy or srw to go true. trrdy = false => wait for srw [FIXME] make this interrupt driven, not polled
    fn i2c_wait(&mut self, flag: u32, timeout_ms: u32) -> u32 {
        let starttime: u32 = get_time_ticks_trunc();

        while (unsafe{ (*self.status).read() } & flag) == 0 {
            let curtime: u32 = get_time_ticks_trunc();

            if curtime >= starttime {
                if (curtime - starttime) > timeout_ms {
                    unsafe{ I2C_DBGSTR[6] += 1; }
                    return 1;
                }
            } else {  // deal with roll-over
                if (curtime + (0xFFFF_FFFF - starttime)) > timeout_ms {
                    unsafe{ I2C_DBGSTR[6] += 1; }
                    return 1;
                }
            }
        }
        0
    }

    /// opposite polarity as above; don't generalize because the extra code can hurt wait loop timing
    fn i2c_wait_n(&mut self, flag: u32, timeout_ms: u32) -> u32 {
        let starttime: u32 = get_time_ticks_trunc();

        while (unsafe{ (*self.status).read() } & flag) != 0 {
            let curtime: u32 = get_time_ticks_trunc();

            if curtime >= starttime {
                if (curtime - starttime) > timeout_ms {
                    unsafe{ I2C_DBGSTR[7] += 1; }
                    return 1;
                }
            } else {  // deal with roll-over
                if (curtime + (0xFFFF_FFFF - starttime)) > timeout_ms {
                    unsafe{ I2C_DBGSTR[7] += 1; }
                    return 1;
                }
            }
        }
        0
    }

    /// The primary I2C interface call. This version currently blocks until the transaction is done.
    /// Due to a limitation of the hardware, rxbuf should either be None, or have a length >= 2!!
    /// So, for single-byte reads, read 2 bytes, ignore the second.
    pub fn i2c_controller(&mut self, addr: u8, txbuf: Option<&[u8]>, rxbuf: Option<&mut [u8]>, timeout_ms: u32) -> u32 {
        let mut ret: u32 = 0;

        // hoist this up to optimize performance a bit
        let do_rx: bool = rxbuf.is_some();

        // write half
        if txbuf.is_some() {
            let txbuf_checked : &[u8] = txbuf.unwrap();

            unsafe{ (*self.txd).write((addr << 1 | 0) as u32); }
            // trrdy should drop when data is accepted
            ret += self.i2c_wait_n(Status::TRRDY.bits(), timeout_ms);
            // issue write+start
            unsafe{ (*self.command).write((Command::STA | Command::WR | Command::CKSDIS).bits()); }

            for i in 0..txbuf_checked.len() {
                // when trrdy goes high again, it's ready to accept the next datum
                ret += self.i2c_wait((Status::TRRDY).bits(), timeout_ms);
                ret += self.i2c_wait_n(Status::TIP.bits(), timeout_ms); // wait until the transaction in progress is done

                // write data
                unsafe{ (*self.txd).write(txbuf_checked[i] as u32); }

                // now issue the write command
                unsafe{ (*self.command).write((Command::WR | Command::CKSDIS).bits()); }

                if i == (txbuf_checked.len() - 1) { // && !do_rx // repeated-start does not work with this IP block; always stop
                    // trrdy going high indicates command was accepted
                    ret += self.i2c_wait((Status::TRRDY).bits(), timeout_ms);
                    // now issue 'stop' command
                    unsafe{ (*self.command).write((Command::STO | Command::CKSDIS).bits()); }
                    // wait until busy drops, indicates we are done with write-phase
                    unsafe{ I2C_DBGSTR[0] = (*self.status).read(); }
                    ret += self.i2c_wait_n(Status::BUSY.bits(), timeout_ms);
                }
            }
        }
        // let the write "stop" condition complete
        if self.i2c_wait_n(Status::BUSY.bits(), timeout_ms) != 0 {
            unsafe{ I2C_DBGSTR[1] += 1; }  ret += 1;
        }

        // read half
        if do_rx {
            let rxbuf_checked : &mut [u8] = rxbuf.unwrap();

            unsafe{ (*self.txd).write((addr << 1 | 1) as u32); } // set "read" for address mode
            // ensure the address write was committed
            if self.i2c_wait_n(Status::TRRDY.bits(), timeout_ms) != 0 {
                unsafe{ I2C_DBGSTR[2] += 1; }  ret += 1;
            }
            // issue bus write + start
            unsafe{ (*self.command).write((Command::STA | Command::WR | Command::CKSDIS).bits()); }

            // SRW goes high once the address is sent and we're in read mode
            if self.i2c_wait(Status::SRW.bits(), timeout_ms) != 0 {
                unsafe{ I2C_DBGSTR[3] += 1; }  ret += 1;
            }
            // issue the "read" command
            unsafe{ (*self.command).write((Command::RD).bits()); }

            for i in 0..rxbuf_checked.len() {
                if i == (rxbuf_checked.len() - 1) {
                    if rxbuf_checked.len() == 1 {
                        // HACK ALERT -- fail if we try to read just one byte
                        // still do two reads, but ignore the second byte

                        // time delay requirement inserted here if only one byte read:
                        // 2 * tSCL min, 7 * tSCL max: 20-70 microseconds
                        //
                        // in practice, even with hardware timer support I was unable
                        // to get this path to work

                        // wait for trrdy to indicate data is available
                        if self.i2c_wait(Status::TRRDY.bits(), timeout_ms) != 0 {
                            unsafe{ I2C_DBGSTR[5] += 1; }  ret += 1;
                        }
                        // read the data
                        rxbuf_checked[0] = unsafe{ (*self.rxd).read() } as u8;

                        // initiate the "read stop" command
                        unsafe{ (*self.command).write((Command::RD | Command::STO | Command::ACK | Command::CKSDIS).bits()); }
                        // wait for trrdy to indicate data is available to be read
                        if self.i2c_wait(Status::TRRDY.bits(), timeout_ms) != 0 {
                            unsafe{ I2C_DBGSTR[4] += 1; }  ret += 1;
                        }
                        // rxbuf_checked[i] = unsafe{ (*self.rxd).read() } as u8; // ignored
                    }
                    // initiate the "read stop" command
                    unsafe{ (*self.command).write((Command::RD | Command::STO | Command::ACK | Command::CKSDIS).bits()); }
                    // wait for trrdy to indicate data is available to be read
                    if self.i2c_wait(Status::TRRDY.bits(), timeout_ms) != 0 {
                        unsafe{ I2C_DBGSTR[4] += 1; }  ret += 1;
                    }
                    rxbuf_checked[i] = unsafe{ (*self.rxd).read() } as u8;
                } else {
                    // wait for trrdy to indicate data is available
                    if self.i2c_wait(Status::TRRDY.bits(), timeout_ms) != 0 {
                        unsafe{ I2C_DBGSTR[5] += 1; }  ret += 1;
                    }
                    // read the data
                    rxbuf_checked[i] = unsafe{ (*self.rxd).read() } as u8;

                    // RD command implicitly repeats -- no need to re-issue the command
                }
            }
        }
        ret
    }


    /// A special version for C-FFI access functions that assume a separate "register" and "data"
    /// fields.
    pub fn i2c_controller_write_ffi(&mut self, addr: u8, reg: u8, data: &[u8],  timeout_ms: u32) -> u32 {
        let mut ret: u32 = 0;

        unsafe{ (*self.txd).write((addr << 1 | 0) as u32); }
        // trrdy should drop when data is accepted
        ret += self.i2c_wait_n(Status::TRRDY.bits(), timeout_ms);
        // issue write+start
        unsafe{ (*self.command).write((Command::STA | Command::WR | Command::CKSDIS).bits()); }

        // write the register destination field
        ret += self.i2c_wait((Status::TRRDY).bits(), timeout_ms);
        ret += self.i2c_wait_n(Status::TIP.bits(), timeout_ms); // wait until the transaction in progress is done
        unsafe{ (*self.txd).write(reg as u32); }
        unsafe{ (*self.command).write((Command::WR | Command::CKSDIS).bits()); }

        // now write the data block
        for i in 0..data.len() {
            // when trrdy goes high again, it's ready to accept the next datum
            ret += self.i2c_wait((Status::TRRDY).bits(), timeout_ms);
            ret += self.i2c_wait_n(Status::TIP.bits(), timeout_ms); // wait until the transaction in progress is done

            // write data
            unsafe{ (*self.txd).write(data[i] as u32); }

            // now issue the write command
            unsafe{ (*self.command).write((Command::WR | Command::CKSDIS).bits()); }

            if i == (data.len() - 1) { // && !do_rx // repeated-start does not work with this IP block; always stop
                // trrdy going high indicates command was accepted
                ret += self.i2c_wait((Status::TRRDY).bits(), timeout_ms);
                // now issue 'stop' command
                unsafe{ (*self.command).write((Command::STO | Command::CKSDIS).bits()); }
                // wait until busy drops, indicates we are done with write-phase
                unsafe{ I2C_DBGSTR[0] = (*self.status).read(); }
                ret += self.i2c_wait_n(Status::BUSY.bits(), timeout_ms);
            }
        }
        // let the write "stop" condition complete
        if self.i2c_wait_n(Status::BUSY.bits(), timeout_ms) != 0 {
            unsafe{ I2C_DBGSTR[1] += 1; }  ret += 1;
        }
        ret
    }

    /// A special version for C-FFI access functions that assume a separate "register" and "data"
    /// fields.
    pub fn i2c_controller_read_ffi(&mut self, addr: u8, reg: u8, rxbuf_checked: &mut [u8], timeout_ms: u32) -> u32 {
        let mut ret: u32 = 0;

        // write half
        unsafe{ (*self.txd).write((addr << 1 | 0) as u32); }
        // trrdy should drop when data is accepted
        ret += self.i2c_wait_n(Status::TRRDY.bits(), timeout_ms);
        // issue write+start
        unsafe{ (*self.command).write((Command::STA | Command::WR | Command::CKSDIS).bits()); }

        // when trrdy goes high again, it's ready to accept the next datum
        ret += self.i2c_wait((Status::TRRDY).bits(), timeout_ms);
        ret += self.i2c_wait_n(Status::TIP.bits(), timeout_ms); // wait until the transaction in progress is done

        // write data
        unsafe{ (*self.txd).write(reg as u32); }

        // now issue the write command
        unsafe{ (*self.command).write((Command::WR | Command::CKSDIS).bits()); }

        // trrdy going high indicates command was accepted
        ret += self.i2c_wait((Status::TRRDY).bits(), timeout_ms);
        ret += self.i2c_wait_n(Status::TIP.bits(), timeout_ms); // wait until the transaction in progress is done

        /*
        // now issue 'stop' command
        unsafe{ (*self.command).write((Command::STO | Command::CKSDIS).bits()); }
        // wait until busy drops, indicates we are done with write-phase
        unsafe{ I2C_DBGSTR[0] = (*self.status).read(); }
        ret += self.i2c_wait_n(Status::BUSY.bits(), timeout_ms);

        // let the write "stop" condition complete
        if self.i2c_wait_n(Status::BUSY.bits(), timeout_ms) != 0 {
            unsafe{ I2C_DBGSTR[1] += 1; }  ret += 1;
        }
        */

        // read half
        unsafe{ (*self.txd).write((addr << 1 | 1) as u32); } // set "read" for address mode
        // ensure the address write was committed
        if self.i2c_wait_n(Status::TRRDY.bits(), timeout_ms) != 0 {
            unsafe{ I2C_DBGSTR[2] += 1; }  ret += 1;
        }
        // issue bus write + repeated start
        unsafe{ (*self.command).write((Command::STA | Command::WR | Command::CKSDIS).bits()); }

        // SRW goes high once the address is sent and we're in read mode
        if self.i2c_wait(Status::SRW.bits(), timeout_ms) != 0 {
            unsafe{ I2C_DBGSTR[3] += 1; }  ret += 1;
        }
        // issue the "read" command
        unsafe{ (*self.command).write((Command::RD).bits()); }

        for i in 0..rxbuf_checked.len() {
            if i == (rxbuf_checked.len() - 1) {
                if rxbuf_checked.len() == 1 {
                    // wait for trrdy to indicate data is available
                    if self.i2c_wait(Status::TRRDY.bits(), timeout_ms) != 0 {
                        unsafe{ I2C_DBGSTR[5] += 1; }  ret += 1;
                    }
                    // read the data
                    rxbuf_checked[0] = unsafe{ (*self.rxd).read() } as u8;

                    // initiate the "read stop" command
                    unsafe{ (*self.command).write((Command::RD | Command::STO | Command::ACK | Command::CKSDIS).bits()); }
                    // wait for trrdy to indicate data is available to be read
                    if self.i2c_wait(Status::TRRDY.bits(), timeout_ms) != 0 {
                        unsafe{ I2C_DBGSTR[4] += 1; }  ret += 1;
                    }
                    // rxbuf_checked[i] = unsafe{ (*self.rxd).read() } as u8; // this is a dummy, don't actually read
                } else {
                    // initiate the "read stop" command
                    unsafe{ (*self.command).write((Command::RD | Command::STO | Command::ACK | Command::CKSDIS).bits()); }
                    // wait for trrdy to indicate data is available to be read
                    if self.i2c_wait(Status::TRRDY.bits(), timeout_ms) != 0 {
                        unsafe{ I2C_DBGSTR[4] += 1; }  ret += 1;
                    }
                    rxbuf_checked[i] = unsafe{ (*self.rxd).read() } as u8;
                }
            } else {
                // wait for trrdy to indicate data is available
                if self.i2c_wait(Status::TRRDY.bits(), timeout_ms) != 0 {
                    unsafe{ I2C_DBGSTR[5] += 1; }  ret += 1;
                }
                // read the data
                rxbuf_checked[i] = unsafe{ (*self.rxd).read() } as u8;

                // RD command implicitly repeats -- no need to re-issue the command
            }
        }
        ret
    }

}
