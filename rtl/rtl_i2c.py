import os

from migen import *

from litex.soc.interconnect.csr import *
from migen.genlib.cdc import MultiReg
from litex.soc.interconnect.csr_eventmanager import *


class RtlI2C(Module, AutoCSR):
    def __init__(self, platform, pads):
        self.sda = TSTriple(1)
        self.scl = TSTriple(1)
        self.specials += [
            self.scl.get_tristate(pads.scl),
            self.sda.get_tristate(pads.sda),
        ]

        self.submodules.ev = EventManager()
        self.ev.i2c_int = EventSourcePulse()  # rising edge triggered
        self.ev.finalize()

        platform.add_source(os.path.join("rtl", "timescale.v"))
        platform.add_source(os.path.join("rtl", "i2c_master_defines.v"))
        platform.add_source(os.path.join("rtl", "i2c_master_bit_ctrl.v"))
        platform.add_source(os.path.join("rtl", "i2c_master_byte_ctrl.v"))

        self.prescale = CSRStorage(16, reset=0xFFFF)
        self.control = CSRStorage(8)
        self.txr = CSRStorage(8)  # output to devices
        self.rxr = CSRStatus(8)   # input from devices
        self.command = CSRStorage(8, write_from_dev=True)
        self.status = CSRStatus(8)

        # control register
        ena = Signal()
        int_ena = Signal()
        self.comb += [
            ena.eq(self.control.storage[7]),
            int_ena.eq(self.control.storage[6]),
        ]

        # command register
        start = Signal()
        stop = Signal()
        ack = Signal()
        iack = Signal()
        read = Signal()
        write = Signal()
        self.comb += [
            start.eq(self.command.storage[7]),
            stop.eq(self.command.storage[6]),
            read.eq(self.command.storage[5]),
            write.eq(self.command.storage[4]),
            ack.eq(self.command.storage[3]),
            iack.eq(self.command.storage[0]),
        ],

        # status register
        rxack = Signal()
        busy = Signal()
        arb_lost = Signal()
        tip = Signal()
        intflag = Signal()
        self.comb += [
            self.status.status[7].eq(rxack),
            self.status.status[6].eq(busy),
            self.status.status[5].eq(arb_lost),
            self.status.status[1].eq(tip),
            self.status.status[0].eq(intflag)
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
            sda_i.eq(self.sda.i),
            self.sda.o.eq(sda_o),
            self.sda.oe.eq(~sda_oen),
            scl_i.eq(self.scl.i),
            self.scl.o.eq(scl_o),
            self.scl.oe.eq(~scl_oen),
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

