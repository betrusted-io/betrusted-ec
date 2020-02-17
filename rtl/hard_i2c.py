import os

from migen import *

from litex.soc.interconnect.csr import *
from migen.genlib.cdc import MultiReg
from litex.soc.interconnect.csr_eventmanager import *
from litex.soc.integration.doc import AutoDoc, ModuleDoc
from litex.soc.interconnect import wishbone


class HardI2C(Module, AutoCSR, AutoDoc):
    """Verilog RTL-based Portable I2C Core"""
    def __init__(self, platform, pads):
        self.intro = ModuleDoc("""HardI2C: ICE40-specific hardened I2C adapter
        This takes the SB_I2C block and adapts it to the wishbone environment
        within Litex. 
        
        All functions of this IP block are programmed by wishbone cycles, by
        writing to and reading from registers scattered across a 16-word region.
        
        See "Advanced iCE40 I2C and SPI Hardened IP Usage Guide (TN1276) 
        (http://www.latticesemi.com/view_document?document_id=50117) for 
        more details."
        """)

        self.sda = TSTriple(1)
        self.scl = TSTriple(1)
        self.specials += [
            self.scl.get_tristate(pads.scl),
            self.sda.get_tristate(pads.sda),
        ]

        self.submodules.ev = EventManager()
        self.ev.i2c_int = EventSourcePulse(description="I2C cycle completed")  # rising edge triggered
        self.ev.gg_int = EventSourceProcess(description="Gas gauge interrupt") # falling edge
        self.ev.gyro_int = EventSourceProcess(description="Gyro interrupt") # falling edge
        self.ev.finalize()
        self.specials += MultiReg(pads.gg_int_n, self.ev.gg_int.trigger)
        self.specials += MultiReg(pads.gyro_int_n, self.ev.gyro_int.trigger)

        self.bus = bus = wishbone.Interface()
        platform.toolchain.attr_translate['I2C_LOCK'] = ("BEL", "X0/Y31/i2c_0") # lock position, because IP software address depend on placer decision
        self.specials += [
            Instance("SB_I2C",
                i_SBCLKI=ClockSignal(),
                i_SBRWI=bus.we,
                i_SBSTBI=bus.cyc,  # doc inconsistency: there claims to be both STB and CYC inputs, but they aren't here on this block? use cyc, I guess.
                i_SBADRI7=0,
                i_SBADRI6=0,
                i_SBADRI5=0, # upper right = 0011, upper left = 0001. Apparently X0/Y31/i2c_0 is the upper left block.
                i_SBADRI4=1,
                i_SBADRI3=bus.adr[3],
                i_SBADRI2=bus.adr[2],
                i_SBADRI1=bus.adr[1],
                i_SBADRI0=bus.adr[0],
                i_SBDATI7=bus.dat_w[7],
                i_SBDATI6=bus.dat_w[6],
                i_SBDATI5=bus.dat_w[5],
                i_SBDATI4=bus.dat_w[4],
                i_SBDATI3=bus.dat_w[3],
                i_SBDATI2=bus.dat_w[2],
                i_SBDATI1=bus.dat_w[1],
                i_SBDATI0=bus.dat_w[0],
                i_SCLI=self.scl.i,
                i_SDAI=self.sda.i,
                o_SBDATO7=bus.dat_r[7],
                o_SBDATO6=bus.dat_r[6],
                o_SBDATO5=bus.dat_r[5],
                o_SBDATO4=bus.dat_r[4],
                o_SBDATO3=bus.dat_r[3],
                o_SBDATO2=bus.dat_r[2],
                o_SBDATO1=bus.dat_r[1],
                o_SBDATO0=bus.dat_r[0],
                o_SBACKO=bus.ack,
                o_I2CIRQ=self.ev.i2c_int.trigger,
                # o_I2CWKUP=  # not used
                o_SCLO=self.scl.o,
                o_SCLOE=self.scl.oe,
                o_SDAO=self.sda.o,
                o_SDAOE=self.sda.oe,
                # not sure these are needed...just copied from the sim template
                p_I2C_SLAVE_INIT_ADDR = "0b1111100001",
                p_BUS_ADDR74 = "0b0001",
                attr=('keep', 'I2C_LOCK'),
            )
        ]
        self.comb += self.bus.dat_r[8:].eq(0)  # this actually costs quite a few gates...

