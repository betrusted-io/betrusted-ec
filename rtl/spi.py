from litex.soc.integration.doc import AutoDoc, ModuleDoc
from litex.soc.interconnect.csr_eventmanager import *

from litex.soc.interconnect import wishbone
from migen.genlib.fifo import SyncFIFOBuffered

from migen.genlib.cdc import MultiReg
from migen.genlib.cdc import PulseSynchronizer

class SpiMaster(Module, AutoCSR, AutoDoc):
    def __init__(self, pads):
        self.intro = ModuleDoc("""Simple soft SPI master module optimized for Betrusted applications

        Requires a clock domain 'spi', which runs at the speed of the SPI bus. 
        
        Simulation benchmarks 16.5us to transfer 16x16 bit words including setup overhead (sysclk=100MHz, spiclk=25MHz)
        which is about 15Mbps system-level performance, assuming the receiver can keep up.
        """)

        self.miso = pads.miso
        self.mosi = pads.mosi
        self.sclk = pads.sclk
        self.csn = pads.csn

        self.comb += self.sclk.eq(~ClockSignal("spi"))  # TODO: add clock gating to save power; note receiver reqs for CS pre-clocks

        self.tx = CSRStorage(16, name="tx", description="""Tx data, for MOSI""")
        self.rx = CSRStatus(16, name="rx", description="""Rx data, from MISO""")
        self.control = CSRStorage(fields=[
            CSRField("go", description="Initiate a SPI cycle by writing a `1`. Does not automatically clear."),
            CSRField("intena", description="Enable interrupt on transaction finished"),
        ])
        self.status = CSRStatus(fields=[
            CSRField("tip", description="Set when transaction is in progress"),
            CSRField("txfull", description="Set when Tx register is full"),
        ])
        self.wifi = CSRStorage(fields=[
            CSRField("reset", description="Write `1` to this register to reset the wifi chip; write `0` to enable normal operation", reset=1),
            CSRField("pa_ena", description="Mapped to PA_ENABLE for wifi (only useful if configured in wifi firmware)"),
            CSRField("wakeup", description="Wakeup the wifi chip"),
        ])
        self.comb += [
            pads.pa_enable.eq(self.wifi.fields.pa_ena),
            pads.wakeup.eq(self.wifi.fields.wakeup),
            pads.res_n.eq(~self.wifi.fields.reset),
        ]

        self.submodules.ev = EventManager()
        self.ev.spi_int = EventSourceProcess(description="Triggered on conclusion of each transaction")  # falling edge triggered
        self.ev.wirq = EventSourceProcess(description="Interrupt request from wifi chip") # falling edge triggered
        self.ev.finalize()
        self.comb += self.ev.spi_int.trigger.eq(self.control.fields.intena & self.status.fields.tip)
        self.specials += MultiReg(pads.wirq, self.ev.wirq.trigger)

        # Replica CSR into "spi" clock domain
        self.tx_r = Signal(16)
        self.rx_r = Signal(16)
        self.tip_r = Signal()
        self.txfull_r = Signal()
        self.go_r = Signal()
        self.tx_written = Signal()

        self.specials += MultiReg(self.tip_r, self.status.fields.tip)
        self.specials += MultiReg(self.txfull_r, self.status.fields.txfull)
        self.specials += MultiReg(self.control.fields.go, self.go_r, "spi")
        self.specials += MultiReg(self.tx.re, self.tx_written, "spi")
        # extract rising edge of go -- necessary in case of huge disparity in sysclk-to-spi clock domain
        self.go_d = Signal()
        self.go_edge = Signal()
        self.sync.spi += self.go_d.eq(self.go_r)
        self.comb += self.go_edge.eq(self.go_r & ~self.go_d)

        self.csn_r = Signal(reset=1)
        self.comb += self.csn.eq(self.csn_r)
        self.comb += self.rx.status.eq(self.rx_r) ## invalid while transaction is in progress
        fsm = FSM(reset_state="IDLE")
        fsm = ClockDomainsRenamer("spi")(fsm)
        self.submodules += fsm
        spicount = Signal(4)
        fsm.act("IDLE",
                If(self.go_edge,
                   NextState("RUN"),
                   NextValue(self.tx_r, Cat(0, self.tx.storage[:15])),
                   # stability guaranteed so no synchronizer necessary
                   NextValue(spicount, 15),
                   NextValue(self.txfull_r, 0),
                   NextValue(self.tip_r, 1),
                   NextValue(self.csn_r, 0),
                   NextValue(self.mosi, self.tx.storage[15]),
                   NextValue(self.rx_r, Cat(self.miso, self.rx_r[:15])),
                ).Else(
                    NextValue(self.tip_r, 0),
                    NextValue(self.csn_r, 1),
                    If(self.tx_written,
                       NextValue(self.txfull_r, 1),
                    ),
                )
        )
        fsm.act("RUN",
                If(spicount > 0,
                   NextValue(self.mosi, self.tx_r[15]),
                   NextValue(self.tx_r, Cat(0, self.tx_r[:15])),
                   NextValue(spicount, spicount - 1),
                   NextValue(self.rx_r, Cat(self.miso, self.rx_r[:15])),
                ).Else(
                    NextValue(self.csn_r, 1),
                    NextValue(self.tip_r, 0),
                    NextState("IDLE"),
                ),
        )

class StickyBit(Module):
    def __init__(self):
        self.flag = Signal()
        self.bit = Signal()
        self.clear = Signal()

        self.sync += [
            If(self.clear,
                self.bit.eq(0)
            ).Else(
                If(self.flag,
                    self.bit.eq(1)
                ).Else(
                    self.bit.eq(self.bit)
                )
            )
        ]

class SpiFifoSlave(Module, AutoCSR, AutoDoc):
    def __init__(self, pads):
        self.intro = ModuleDoc("""Simple soft SPI slave module optimized for Betrusted-EC (UP5K arch) use

        Assumes a free-running sclk and csn performs the function of framing bits
        Thus csn must go high between each frame, you cannot hold csn low for burst transfers.
        """)

        self.miso = pads.miso
        self.mosi = pads.mosi
        self.csn = pads.csn

        ### clock is not wired up in this module, it's moved up to CRG for implementation-dependent buffering

        self.control = CSRStorage(fields=[
            CSRField("clrerr", description="Clear FIFO error flags", pulse=True),
            CSRField("host_int", description="0->1 raises an interrupt to the COM host"), # rising edge triggered on other side
        ])
        self.comb += pads.irq.eq(self.control.fields.host_int)
        self.status = CSRStatus(fields=[
            CSRField("tip", description="Set when transaction is in progress"),
            CSRField("rx_avail", description="Set when Rx FIFO has new, valid contents to read"),
            CSRField("rx_over", description="Set if Rx FIFO has overflowed"),
            CSRField("rx_under", description="Set if Rx FIFO underflows"),
            CSRField("rx_level", size=12, description="Level of Rx FIFO"),
            CSRField("tx_avail", description="Set when Tx FIFO has space for new content"),
            CSRField("tx_empty", description="Set when Tx FIFO is empty"),
            CSRField("tx_level", size=12, description="Level of Tx FIFO"),
            CSRField("tx_over", description="Set when Tx FIFO overflows"),
            CSRField("tx_under", description = "Set when Tx FIFO underflows"),
        ])
        self.submodules.ev = EventManager()
        self.ev.spi_avail = EventSourcePulse(description="Triggered when Rx FIFO leaves empty state")  # rising edge triggered
        self.ev.spi_event = EventSourceProcess(description="Triggered every time a packet completes")  # falling edge triggered
        self.ev.finalize()
        self.comb += self.ev.spi_avail.trigger.eq(self.status.fields.rx_avail)
        self.comb += self.ev.spi_event.trigger.eq(self.status.fields.tip)

        self.bus = bus = wishbone.Interface()
        rd_ack = Signal()
        wr_ack = Signal()
        self.comb +=[
            If(bus.we,
               bus.ack.eq(wr_ack),
            ).Else(
                bus.ack.eq(rd_ack),
            )
        ]

        # read/rx subsystemx
        self.submodules.rx_fifo = rx_fifo = SyncFIFOBuffered(16, 1280) # should infer SB_RAM256x16's. 2560 depth > 2312 bytes = wifi MTU
        self.submodules.rx_under = StickyBit()
        self.comb += [
            self.status.fields.rx_level.eq(rx_fifo.level),
            self.status.fields.rx_avail.eq(rx_fifo.readable),
            self.rx_under.clear.eq(self.control.fields.clrerr),
            self.status.fields.rx_under.eq(self.rx_under.bit),
        ]


        bus_read = Signal()
        bus_read_d = Signal()
        rd_ack_pipe = Signal()
        self.comb += bus_read.eq(bus.cyc & bus.stb & ~bus.we & (bus.cti == 0))
        self.sync += [  # This is the bus responder -- only works for uncached memory regions
            bus_read_d.eq(bus_read),
            If(bus_read & ~bus_read_d,  # One response, one cycle
                rd_ack_pipe.eq(1),
                If(rx_fifo.readable,
                    bus.dat_r.eq(rx_fifo.dout),
                    rx_fifo.re.eq(1),
                ).Else(
                    # Don't stall the bus indefinitely if we try to read from an empty fifo...just
                    # return garbage
                    bus.dat_r.eq(0xdeadbeef),
                    rx_fifo.re.eq(0),
                    self.rx_under.flag.eq(1),
                )
               ).Else(
                rx_fifo.re.eq(0),
                rd_ack_pipe.eq(0),
            ),
            rd_ack.eq(rd_ack_pipe),
        ]

        # tx/write spislave
        self.submodules.tx_fifo = tx_fifo = SyncFIFOBuffered(16, 1280)
        self.submodules.tx_over = StickyBit()
        self.comb += [
           self.tx_over.clear.eq(self.control.fields.clrerr),
           self.status.fields.tx_over.eq(self.tx_over.bit)
        ]

        self.sync += [
            If(bus.cyc & bus.stb & bus.we & ~bus.ack,
                If(tx_fifo.writable,
                    tx_fifo.din.eq(bus.dat_w),
                    tx_fifo.we.eq(1),
                    wr_ack.eq(1),
                ).Else(
                    self.tx_over.flag.eq(1),
                    tx_fifo.we.eq(0),
                    wr_ack.eq(0),
                )
               ).Else(
                tx_fifo.we.eq(0),
                wr_ack.eq(0),
            )
        ]

        # Replica CSR into "spi" clock domain
        self.txrx = Signal(16)
        self.tip_r = Signal()
        self.rxfull_r = Signal()
        self.rxover_r = Signal()
        self.csn_r = Signal()

        self.specials += MultiReg(~self.csn, self.tip_r)
        self.comb += self.status.fields.tip.eq(self.tip_r)
        tip_d = Signal()
        donepulse = Signal()
        self.sync += tip_d.eq(self.tip_r)
        self.comb += donepulse.eq(~self.tip_r & tip_d)  # done pulse goes high when tip drops

        self.submodules.rx_over = StickyBit()
        self.comb += [
            self.status.fields.rx_over.eq(self.rx_over.bit),
            self.rx_over.clear.eq(self.control.fields.clrerr),
            self.rx_over.flag.eq(~self.rx_fifo.writable & donepulse),
        ]
        self.submodules.tx_under = StickyBit()
        self.comb += [
            self.status.fields.tx_under.eq(self.tx_under.bit),
            self.tx_under.clear.eq(self.control.fields.clrerr),
            self.tx_under.flag.eq(~self.tx_fifo.readable & donepulse),
        ]

        self.comb += self.miso.eq(self.txrx[15])
        self.comb += [
            self.rx_fifo.din.eq(self.txrx),
            self.rx_fifo.we.eq(donepulse),
            self.tx_fifo.re.eq(donepulse),
        ]

        csn_d = Signal()
        self.sync.spislave += [
            csn_d.eq(self.csn),
            # "Sloppy" clock boundary crossing allowed because "rxfull" is synchronized and CPU should grab data based on that
            If(self.csn == 0,
               self.txrx.eq(Cat(self.mosi, self.txrx[0:15])),
            ).Else(
               self.txrx.eq(self.tx_fifo.dout)
            )
        ]



class SpiSlave(Module, AutoCSR, AutoDoc):
    def __init__(self, pads):
        self.intro = ModuleDoc("""Simple soft SPI slave module optimized for Betrusted-EC (UP5K arch) use

        Assumes a free-running sclk and csn performs the function of framing bits
        Thus csn must go high between each frame, you cannot hold csn low for burst transfers
        """)

        self.miso = pads.miso
        self.mosi = pads.mosi
        self.csn = pads.csn

        ### clock is not wired up in this module, it's moved up to CRG for implementation-dependent buffering

        self.tx = CSRStorage(16, name="tx", description="""Tx data, to MISO""")
        self.rx = CSRStatus(16, name="rx", description="""Rx data, from MOSI""")
        self.control = CSRStorage(fields=[
            CSRField("intena", description="Enable interrupt on transaction finished"),
            CSRField("clrerr", description="Clear Rx overrun error", pulse=True),
        ])
        self.status = CSRStatus(fields=[
            CSRField("tip", description="Set when transaction is in progress"),
            CSRField("rxfull", description="Set when Rx register has new, valid contents to read"),
            CSRField("rxover", description="Set if Rx register was not read before another transaction was started")
        ])

        self.submodules.ev = EventManager()
        self.ev.spi_int = EventSourceProcess()  # falling edge triggered
        self.ev.finalize()
        self.comb += self.ev.spi_int.trigger.eq(self.control.fields.intena & self.status.fields.tip)

        # Replica CSR into "spi" clock domain
        self.txrx = Signal(16)
        self.tip_r = Signal()
        self.rxfull_r = Signal()
        self.rxover_r = Signal()
        self.csn_r = Signal()

        self.specials += MultiReg(self.tip_r, self.status.fields.tip)
        self.comb += self.tip_r.eq(~self.csn)
        tip_d = Signal()
        donepulse = Signal()
        self.sync += tip_d.eq(self.tip_r)
        self.comb += donepulse.eq(~self.tip_r & tip_d)  # done pulse goes high when tip drops

        self.comb += self.status.fields.rxfull.eq(self.rxfull_r)
        self.comb += self.status.fields.rxover.eq(self.rxover_r)

        self.sync += [
            If(self.rx.we,
               self.rxfull_r.eq(0),
            ).Else(
                If(donepulse,
                   self.rxfull_r.eq(1)
                ).Else(
                    self.rxfull_r.eq(self.rxfull_r),
                ),

                If(self.tip_r & self.rxfull_r,
                   self.rxover_r.eq(1)
                ).Elif(self.control.fields.clrerr,
                   self.rxover_r.eq(0)
                ).Else(
                    self.rxover_r.eq(self.rxover_r)
                ),
            )
        ]

        self.comb += self.miso.eq(self.txrx[15])
        csn_d = Signal()
        self.sync.spislave += [
            csn_d.eq(self.csn),
            # "Sloppy" clock boundary crossing allowed because "rxfull" is synchronized and CPU should grab data based on that
            If(self.csn == 0,
               self.txrx.eq(Cat(self.mosi, self.txrx[0:15])),
               self.rx.status.eq(self.rx.status),
            ).Else(
               If(self.csn & ~csn_d,
                 self.rx.status.eq(self.txrx),
               ).Else(
                   self.rx.status.eq(self.rx.status)
               ),
               self.txrx.eq(self.tx.storage)
            )
        ]

