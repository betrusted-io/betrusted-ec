import os

from migen import *

from litex.soc.interconnect.csr import *
from migen.genlib.cdc import MultiReg
from litex.soc.interconnect.csr_eventmanager import *
from litex.soc.integration.doc import AutoDoc, ModuleDoc


class RtlI2C(Module, AutoCSR, AutoDoc):
    """Verilog RTL-based Portable I2C Core"""
    def __init__(self, platform, pads):
        self.intro = ModuleDoc("""RtlI2C: A verilog RTL-based I2C core
        RtlI2C is an RTL-based I2C core derived from the OpenCores I2C master IP. 
        """)
        self.sda = TSTriple(1)
        self.scl = TSTriple(1)
        self.specials += [
            self.scl.get_tristate(pads.scl),
            self.sda.get_tristate(pads.sda),
        ]

        platform.add_source(os.path.join("rtl", "timescale.v"))
        platform.add_source(os.path.join("rtl", "i2c_master_defines.v"))
        platform.add_source(os.path.join("rtl", "i2c_master_bit_ctrl.v"))
        platform.add_source(os.path.join("rtl", "i2c_master_byte_ctrl.v"))

        self.prescale = CSRStorage(16, reset=0xFFFF, name="prescale", description="""
        Prescaler value. Set to (module clock / (5 * I2C freq) - 1). Example: if module clock
        is equal to sysclk; syclk is 100MHz; and I2C freq is 100kHz, then prescaler 
        is (100MHz / (5 * 100kHz) - 1) = 199. Reset value: 0xFFFF""")
        self.control = CSRStorage(fields=[
            CSRField("Resvd", size=6, description="Reserved (for cross-compatibility with OpenCores drivers)"),
            CSRField("IEN", description="When set to `1`, interrupts are enabled."),
            CSRField("EN", description="When set to `1`, the core is enabled."),
        ])
        self.txr = CSRStorage(8, name="txr", description="""
        Next byte to transmit to slave devices. LSB indicates R/W during address phases, 
        `1` for reading from slaves, `0` for writing to slaves""")
        self.rxr = CSRStatus(8, name="rxr", description="""
        Data being read from slaved devices""")
        self.command = CSRStorage(write_from_dev=True, fields=[
            CSRField("IACK", description="Interrupt acknowledge; when set, clears a pending interrupt"),
            CSRField("Resvd", size=2, description="reserved for cross-compatibility with OpenCores drivers"),
            CSRField("ACK", description="when a receiver, sent ack (`ACK=0`) or nack (`ACK=1`)"),
            CSRField("WR", description="write to slave"),
            CSRField("RD", description="read from slave"),
            CSRField("STO", description="generate stop condition"),
            CSRField("STA", description="generate (repeated) start condition"),
        ])
        self.status = CSRStatus(8, fields=[
            CSRField("IF", description="Interrupt flag, This bit is set when an interrupt is pending, which will cause a processor interrupt request if the IEN bit is set. The Interrupt Flag is set upon the completion of one byte of data transfer."),
            CSRField("TIP", description="transfer in progress"),
            CSRField("Resvd", size=3, description="reserved for cross-compatibility with OpenCores drivers"),
            CSRField("ArbLost", description="Set when arbitration for the bus is lost"),
            CSRField("Busy", description="I2C block is busy processing the latest command"),
            CSRField("RxACK", description="Received acknowledge from slave. 1 = no ack received, 0 = ack received"),
        ])
        self.bitbang_mode = CSRStorage(1, description="When set, I2C is driven by bitbang GPIO operations")
        self.bb = CSRStorage(fields=[
            CSRField("scl", description="Directly controls the SCL pin", reset=1),
            CSRField("sda_val", description="Applies its value to SDA when `sda_dir` is 1", reset=1),
            CSRField("sda_dir", description="When `1`, drives SDA; `0` tristates", reset=0),
        ])
        self.bb_r = CSRStatus(1, description="The value currently sensed on the SDA pin")

        self.submodules.ev = EventManager()
        self.ev.i2c_int = EventSourcePulse(description="I2C cycle completed")
        self.ev.gg_int = EventSourcePulse(description="Gas gauge interrupt")
        self.ev.gyro_int = EventSourcePulse(description="Gyro interrupt")
        self.ev.usbcc_int = EventSourcePulse(description="USB CC register changed")
        self.ev.finalize()
        usb_cc_int = Signal()
        usb_cc_int_r = Signal()
        gg_int = Signal()
        gg_int_r = Signal()
        gyro_int = Signal()
        gyro_int_r = Signal()
        self.specials += MultiReg(~pads.gg_int_n, gg_int)
        self.specials += MultiReg(~pads.gyro_int_n, gyro_int)
        self.specials += MultiReg(~pads.usbcc_int_n, usb_cc_int)
        self.sync += [
            usb_cc_int_r.eq(usb_cc_int),
            self.ev.usbcc_int.trigger.eq(usb_cc_int & ~usb_cc_int_r),
            gg_int_r.eq(gg_int),
            self.ev.gg_int.trigger.eq(gg_int & ~gg_int_r),
            gyro_int_r.eq(gyro_int),
            self.ev.gyro_int.trigger.eq(gyro_int & ~gyro_int_r),
        ]

        # control register
        ena = Signal()
        int_ena = Signal()
        self.comb += [
            ena.eq(self.control.fields.EN),
            int_ena.eq(self.control.fields.IEN),
        ]

        # command register
        start = Signal()
        stop = Signal()
        ack = Signal()
        iack = Signal()
        read = Signal()
        write = Signal()
        self.comb += [
            start.eq(self.command.fields.STA),
            stop.eq(self.command.fields.STO),
            read.eq(self.command.fields.RD),
            write.eq(self.command.fields.WR),
            ack.eq(self.command.fields.ACK),
            iack.eq(self.command.fields.IACK),
        ],

        # status register
        rxack = Signal()
        busy = Signal()
        arb_lost = Signal()
        tip = Signal()
        intflag = Signal()
        self.comb += [
            self.status.fields.RxACK.eq(rxack),
            self.status.fields.Busy.eq(busy),
            self.status.fields.ArbLost.eq(arb_lost),
            self.status.fields.TIP.eq(tip),
            self.status.fields.IF.eq(intflag)
        ]


        done = Signal()
        i2c_al = Signal()
        scl_i = Signal()
        scl_o = Signal()
        scl_oen = Signal()
        sda_i = Signal()
        sda_o = Signal()
        sda_oen = Signal()
        self.specials += [
            Instance("i2c_master_byte_ctrl",
                     i_clk=ClockSignal(),
                     i_rst=ResetSignal(),
                     i_nReset=1,
                     i_ena=ena,
                     i_clk_cnt=self.prescale.storage,
                     i_start=start,
                     i_stop=stop & ~done,
                     i_read=read & ~done,
                     i_write=write & ~done,
                     i_ack_in=ack,
                     i_din=self.txr.storage,
                     o_cmd_ack=done,  # this is a one-cycle wide pulse
                     o_ack_out=rxack,
                     o_dout=self.rxr.status,
                     o_i2c_busy=busy,
                     o_i2c_al=i2c_al,
                     i_scl_i=scl_i,
                     o_scl_o=scl_o,
                     o_scl_oen=scl_oen,
                     i_sda_i=sda_i,
                     o_sda_o=sda_o,
                     o_sda_oen=sda_oen,
                     )
        ]
        self.comb += [
            If(self.bitbang_mode.storage,
                self.sda.o.eq(self.bb.fields.sda_val),
                self.sda.oe.eq(self.bb.fields.sda_dir),
                self.scl.o.eq(self.bb.fields.scl),
                self.scl.oe.eq(1),
            ).Else(
                self.sda.o.eq(sda_o),
                self.sda.oe.eq(~sda_oen),
                self.scl.o.eq(scl_o),
                self.scl.oe.eq(~scl_oen),
            ),
            sda_i.eq(self.sda.i),
            scl_i.eq(self.scl.i),

            self.bb_r.status.eq(self.sda.i),
        ]

        self.comb += [
            If(done | i2c_al,
               self.command.we.eq(1),
               self.command.dat_w.eq(0),
               ).Else(
                self.command.we.eq(0)
            ),
        ]
        self.sync += [
            tip.eq(read | write),
            intflag.eq( (done | i2c_al | intflag) & ~iack),
            arb_lost.eq(i2c_al | (arb_lost & ~start)),
        ]

        self.comb += self.ev.i2c_int.trigger.eq(intflag & int_ena)

