#!/usr/bin/env python3
# This variable defines all the external programs that this module
# relies on.  lxbuildenv reads this variable in order to ensure
# the build will finish without exiting due to missing third-party
# programs.
LX_DEPENDENCIES = ["riscv", "icestorm", "yosys"]

# Import lxbuildenv to integrate the deps/ directory
import lxbuildenv
import argparse
import os
import subprocess

from migen import *
from migen import Module, Signal, Instance, ClockDomain, If
from migen.genlib.resetsync import AsyncResetSynchronizer
from migen.fhdl.specials import TSTriple
from migen.fhdl.bitcontainer import bits_for
from migen.fhdl.structure import ClockSignal, ResetSignal, Replicate, Cat

from litex.build.lattice.platform import LatticePlatform
from litex.build.sim.platform import SimPlatform
from litex.build.generic_platform import Pins, IOStandard, Misc, Subsignal
from litex.soc.cores import up5kspram
from litex.soc.integration.soc_core import SoCCore
from litex.soc.integration.builder import Builder
from litex.soc.interconnect import wishbone
from litex.soc.interconnect.csr import *
from litex.soc.cores.uart import UARTWishboneBridge
from litex.soc.cores import uart

from gateware.ice40_hard_i2c import HardI2C
from gateware.ticktimer import TickTimer
from gateware.spi_ice40 import *

import litex.soc.doc as lxsocdoc

# Ish. It's actually slightly smaller, but this is divisible by 4096 (erase sector size).
GATEWARE_SIZE = 0x1a000

# 1 MB (8 Mb)
SPI_FLASH_SIZE = 1 * 1024 * 1024

io_pvt = [
    ("serial", 0,
     Subsignal("rx", Pins("48")),  # MON0
     Subsignal("tx", Pins("3")),  # MON1
     IOStandard("LVCMOS33")
     ),
    # serial is muxed in with these key monitor pins
#    ("up5k_keycol0", 0, Pins("48"), IOStandard("LVCMOS33")),
#    ("up5k_keycol1", 0, Pins("3"), IOStandard("LVCMOS33")),  # this is marking "high"
    ("up5k_keyrow0", 0, Pins("4"), IOStandard("LVCMOS33")),
    ("up5k_keyrow1", 0, Pins("2"), IOStandard("LVCMOS33")),

    ("spiflash", 0,
     Subsignal("cs_n", Pins("16"), IOStandard("LVCMOS18")),
     Subsignal("clk", Pins("15"), IOStandard("LVCMOS18")),
     Subsignal("cipo", Pins("17"), IOStandard("LVCMOS18")),
     Subsignal("copi", Pins("14"), IOStandard("LVCMOS18")),
     Subsignal("wp", Pins("18"), IOStandard("LVCMOS18")),
     Subsignal("hold", Pins("13"), IOStandard("LVCMOS18")),
     ),
    ("spiflash4x", 0,
     Subsignal("cs_n", Pins("16"), IOStandard("LVCMOS18")),
     Subsignal("clk", Pins("15"), IOStandard("LVCMOS18")),
     Subsignal("dq", Pins("14 17 18 13"), IOStandard("LVCMOS18")),
     ),

    # I2C
    ("i2c", 0,
     Subsignal("scl", Pins("23"), IOStandard("LVCMOS18")),
     Subsignal("sda", Pins("25"), IOStandard("LVCMOS18")),
     Subsignal("gg_int_n", Pins("37"), IOStandard("LVCMOS18")),     # DVT / mux to LPCLK if we want to go very low power
     Subsignal("gyro_int_n", Pins("43"), IOStandard("LVCMOS18")),
     Subsignal("usbcc_int_n", Pins("42"), IOStandard("LVCMOS18")),  # DVT
    ),

    ("clk12", 0, Pins("35"), IOStandard("LVCMOS18")),

    ("com", 0,
        Subsignal("csn", Pins("11"), IOStandard("LVCMOS18")),
        Subsignal("cipo", Pins("10"), IOStandard("LVCMOS18")),
        Subsignal("copi", Pins("9"), IOStandard("LVCMOS18")),
        Subsignal("irq", Pins("6"), IOStandard("LVCMOS18")), # ACTIVE HIGH
        Subsignal("hold", Pins("12"), IOStandard("LVCMOS18")),
     ),
    ("com_sclk", 0, Pins("20"), IOStandard("LVCMOS18")),

    ("extcommin", 0, Pins("45"), IOStandard("LVCMOS33")),
    ("lcd_disp", 0, Pins("44"), IOStandard("LVCMOS33")),

    ("power", 0,
        Subsignal("s0", Pins("19"), IOStandard("LVCMOS18")),
        # Subsignal("s1", Pins("12"), IOStandard("LVCMOS18")),  # PVT changed to hold
        Subsignal("sys_on", Pins("47"), IOStandard("LVCMOS33")), # sys_on
        Subsignal("u_to_t_on", Pins("46"), IOStandard("LVCMOS33")), # u_to_t_on
        Subsignal("fpga_dis", Pins("21"), IOStandard("LVCMOS18")), # fpga_dis
     ),

    ("led", 0,
         Subsignal("rgb0", Pins("39"), IOStandard("LVCMOS33")),
         Subsignal("rgb1", Pins("40"), IOStandard("LVCMOS33")),
         Subsignal("rgb2", Pins("41"), IOStandard("LVCMOS33")),
     ),

    ("wifi", 0,
         Subsignal("cipo", Pins("27"), IOStandard("LVCMOS18")),
         Subsignal("copi", Pins("28"), IOStandard("LVCMOS18")),
         Subsignal("csn", Pins("32"), IOStandard("LVCMOS18")),
         Subsignal("sclk", Pins("26"), IOStandard("LVCMOS18")),
         Subsignal("pa_enable", Pins("34"), IOStandard("LVCMOS18")),
         Subsignal("res_n", Pins("36"), IOStandard("LVCMOS18")),
         Subsignal("wirq", Pins("31"), IOStandard("LVCMOS18")),
         Subsignal("wakeup", Pins("38"), IOStandard("LVCMOS18")),
     ),
    # ("lpclk", 0, Pins("37"), IOStandard("LVCMOS18")),  # conflicts with SB_PLL40_2_PAD...what weirdness
]

sysclkfreq=18e6

class BetrustedPlatform(LatticePlatform):
    def __init__(self, io, toolchain="icestorm", revision="pvt"):
        self.revision = revision
        LatticePlatform.__init__(self, "ice40-up5k-sg48", io, toolchain="icestorm")

    def create_programmer(self):
        raise ValueError("programming is not supported")

    class CRG(Module, AutoCSR, AutoDoc):
        def __init__(self, platform):
            clk12_raw = platform.request("clk12")
            clk_sys = Signal()

            self.clock_domains.cd_sys = ClockDomain()
            self.comb += self.cd_sys.clk.eq(clk_sys)

            platform.add_period_constraint(clk12_raw, 1e9/12e6)  # this is fixed and comes from external crystal

            # POR reset logic- POR generated from sys clk, POR logic feeds sys clk
            # reset. Just need a pulse one cycle wide to get things working right.
            # ^^^ this line is a lie. I have found devices that do not reliably reset with
            # one pulse. Extending the pulse to 2 wide seems to fix the issue.
            self.clock_domains.cd_por = ClockDomain()
            reset_cascade = Signal(reset=1)
            reset_cascade2 = Signal(reset=1)
            reset_initiator = Signal()
            self.sync.por += [
                reset_cascade.eq(reset_initiator),
                reset_cascade2.eq(reset_cascade),
            ]
            self.comb += [
                self.cd_por.clk.eq(self.cd_sys.clk),
                self.cd_sys.rst.eq(reset_cascade2),
            ]

            # generate a >1us-wide pulse at ~1Hz based on sysclk for display extcomm signal
            # count down from 12e6 to 0 so that first extcomm pulse comes after lcd_disp is high
            # note: going to a 25-bit counter makes this the critical path at speeds > 12 MHz, so stay with
            # 24 bits but a faster extcomm clock.
            extcomm = platform.request("extcommin", 0)
            extcomm_div = Signal(24, reset=int(12e6)) # datasheet range is 0.5Hz - 5Hz, so actual speed is 1.5Hz
            self.sync += [
                If(extcomm_div == 0,
                   extcomm_div.eq(int(12e6))
                ).Else(
                   extcomm_div.eq(extcomm_div - 1)
                ),

                If(extcomm_div < 13,
                   extcomm.eq(1)
                ).Else(
                   extcomm.eq(0)
                )
            ]
            self.comb += platform.request("lcd_disp", 0).eq(1)  # force display on for now

            ### WATCHDOG RESET, uses the extcomm_div divider to save on gates
            self.watchdog = CSRStorage(17, fields=[
                CSRField("reset_code", size=16, description="Write `600d` then `c0de` in sequence to this register to reset the watchdog timer"),
                CSRField("enable", description="Enable the watchdog timer. Cannot be disabled once enabled, except with a reset. Notably, a watchdog reset will disable the watchdog.", reset=0),
            ])
            wdog_enabled=Signal(reset=0)
            self.sync += [
                If(self.watchdog.fields.enable,
                    wdog_enabled.eq(1)
                ).Else(
                    wdog_enabled.eq(wdog_enabled)
                )
            ]
            wdog_cycle_r = Signal()
            wdog_cycle = Signal()
            self.sync += wdog_cycle_r.eq(extcomm)
            self.comb += wdog_cycle.eq(extcomm & ~wdog_cycle_r)
            wdog = FSM(reset_state="IDLE")
            self.submodules += wdog
            wdog.act("IDLE",
                If(wdog_enabled,
                    NextState("WAIT_ARM")
                )
            )
            wdog.act("WAIT_ARM",
                # sync up to the watchdog cycle so we give ourselves a full cycle to disarm the watchdog
                If(wdog_cycle,
                    NextState("ARMED")
                )
            )
            wdog.act("ARMED",
                If(wdog_cycle,
                    self.cd_sys.rst.eq(1),
                ),
                If(self.watchdog.re,
                    If(self.watchdog.fields.reset_code == 0x600d,
                        NextState("DISARM1")
                    )
                )
            )
            wdog.act("DISARM1",
                If(wdog_cycle,
                    self.cd_sys.rst.eq(1),
                ),
                If(self.watchdog.re,
                    If(self.watchdog.fields.reset_code == 0xc0de,
                       NextState("DISARMED")
                    ).Else(
                       NextState("ARMED")
                    )
                )
            )
            wdog.act("DISARMED",
                If(wdog_cycle,
                    NextState("ARMED")
                )
            )


            self.specials += Instance(
                "SB_PLL40_PAD",
                # Parameters
                p_DIVR = 0,
                p_DIVF = 47, # 18 MHz
                p_DIVQ = 5,
                p_FILTER_RANGE = 1,
                p_FEEDBACK_PATH = "SIMPLE",
                p_DELAY_ADJUSTMENT_MODE_FEEDBACK = "FIXED",
                p_FDA_FEEDBACK = 0,
                p_DELAY_ADJUSTMENT_MODE_RELATIVE = "FIXED",
                p_FDA_RELATIVE = 0,
                p_SHIFTREG_DIV_MODE = 1,
                p_PLLOUT_SELECT = "GENCLK",
                p_ENABLE_ICEGATE = 0,
                # IO
                i_PACKAGEPIN = clk12_raw,
                o_PLLOUTGLOBAL = clk_sys,   # from PLL
                i_BYPASS = 0,
                i_RESETB = 1,
            )
            # global buffer for input SPI clock
            self.clock_domains.cd_sclk = ClockDomain()
            clk_sclk = Signal()
            self.comb += self.cd_sclk.clk.eq(clk_sclk)

            self.specials += Instance(
                "SB_GB",
                i_USER_SIGNAL_TO_GLOBAL_BUFFER=platform.request("com_sclk"),
                o_GLOBAL_BUFFER_OUTPUT=clk_sclk,
            )

            # Add a period constraint for each clock wire.
            # NextPNR picks the clock domain's name randomly from one of the wires
            # that it finds in the domain.  Migen passes the information on timing
            # to NextPNR in a file called `top_pre_pack.py`.  In order to ensure
            # it chooses the timing for this net, annotate period constraints for
            # all wires.
            platform.add_period_constraint(clk_sclk, 1e9/20e6)
            platform.add_period_constraint(clk_sys, 1e9/sysclkfreq)
            platform.add_period_constraint(self.cd_por.clk, 1e9/sysclkfreq)


class PicoRVSpi(Module, AutoCSR, AutoDoc):
    def __init__(self, platform, pads, size=2*1024*1024):
        self.intro = ModuleDoc("See https://github.com/cliffordwolf/picorv32/tree/master/picosoc#spi-flash-controller-config-register; used with modifications")
        self.size = size

        self.bus = bus = wishbone.Interface()

        self.reset = Signal()

        cfg = Signal(32)
        cfg_we = Signal(4)
        cfg_out = Signal(32)

        # Add pulse the cfg_we line after reset
        reset_counter = Signal(2, reset=3)
        ic_reset = Signal(reset=1)
        self.sync += \
            If(reset_counter != 0,
                reset_counter.eq(reset_counter - 1)
            ).Else(
                ic_reset.eq(0)
            )

        self.rdata = CSRStatus(fields=[
            CSRField("data", size=4, description="Data bits from SPI [3:hold, 2:wp, 1:cipo, 0:copi]"),
        ])
        self.mode = CSRStorage(fields=[
            CSRField("bitbang", size=1, description="Turn on bitbang mode", reset = 0),
            CSRField("csn", size=1, description="Chip select (set to `0` to select the chip)"),
        ])
        self.wdata = CSRStorage(description="Writes to this field automatically pulse CLK", fields=[
            CSRField("data", size=4, description="Data bits to SPI [3:hold, 2:wp, 1:cipo, 0:copi]"),
            CSRField("oe", size=4, description="Output enable for data pins"),
        ])
        bb_clk = Signal()
        self.sync += [
            bb_clk.eq(self.wdata.re), # auto-clock the SPI chip whenever wdata is written. Delay a cycle for setup/hold.
        ]

        copi_pad = TSTriple()
        cipo_pad = TSTriple()
        cs_n_pad = TSTriple()
        clk_pad  = TSTriple()
        wp_pad   = TSTriple()
        hold_pad = TSTriple()
        self.specials += copi_pad.get_tristate(pads.copi)
        self.specials += cipo_pad.get_tristate(pads.cipo)
        self.specials += cs_n_pad.get_tristate(pads.cs_n)
        self.specials += clk_pad.get_tristate(pads.clk)
        self.specials += wp_pad.get_tristate(pads.wp)
        self.specials += hold_pad.get_tristate(pads.hold)

        reset = Signal()
        self.comb += [
            reset.eq(ResetSignal() | self.reset),
            cs_n_pad.oe.eq(~reset),
            clk_pad.oe.eq(~reset),
        ]

        flash_addr = Signal(24)
        # size/4 because data bus is 32 bits wide, -1 for base 0
        mem_bits = bits_for(int(size/4)-1)
        pad = Signal(2)
        self.comb += flash_addr.eq(Cat(pad, bus.adr[0:mem_bits-1]))

        read_active = Signal()
        spi_ready = Signal()
        self.sync += [
            If(bus.stb & bus.cyc & ~read_active,
                read_active.eq(1),
                bus.ack.eq(0),
            )
            .Elif(read_active & spi_ready,
                read_active.eq(0),
                bus.ack.eq(1),
            )
            .Else(
                bus.ack.eq(0),
                read_active.eq(0),
            )
        ]

        o_rdata = Signal(32)
        self.comb += bus.dat_r.eq(o_rdata)

        self.specials += Instance("spimemio",
            o_flash_io0_oe = copi_pad.oe,
            o_flash_io1_oe = cipo_pad.oe,
            o_flash_io2_oe = wp_pad.oe,
            o_flash_io3_oe = hold_pad.oe,

            o_flash_io0_do = copi_pad.o,
            o_flash_io1_do = cipo_pad.o,
            o_flash_io2_do = wp_pad.o,
            o_flash_io3_do = hold_pad.o,
            o_flash_csb    = cs_n_pad.o,
            o_flash_clk    = clk_pad.o,

            i_flash_io0_di = copi_pad.i,
            i_flash_io1_di = cipo_pad.i,
            i_flash_io2_di = wp_pad.i,
            i_flash_io3_di = hold_pad.i,

            i_resetn = ~reset,
            i_clk = ClockSignal(),

            i_valid = bus.stb & bus.cyc,
            o_ready = spi_ready,
            i_addr  = flash_addr,
            o_rdata = o_rdata,

            i_bb_oe = self.wdata.fields.oe,
            i_bb_wd = self.wdata.fields.data,
            i_bb_clk = bb_clk,
            i_bb_csb = self.mode.fields.csn,
            o_bb_rd = self.rdata.fields.data,
            i_config_update = self.mode.re | ic_reset,
            i_memio_enable = ~self.mode.fields.bitbang,
        )
        platform.add_source("rtl/spimemio.v")

class BtPower(Module, AutoCSR, AutoDoc):
    def __init__(self, pads):
        self.intro = ModuleDoc("""BtPower - power control pins (EC)""")

        self.power = CSRStorage(8, fields =[
            CSRField("self", description="Writing `1` to this keeps the EC powered on", reset=1),
            CSRField("soc_on", description="Writing `1` to this powers on the SoC", reset=1),
            CSRField("discharge", description="Writing `1` to this connects a low-value resistor across FPGA domain supplies to force a full discharge"),
            CSRField("kbddrive", description="Writing `1` to this drives the scan column to 1. Do this prior to reading to mitigate noise")
        ])

        self.stats = CSRStatus(8, fields=[
            CSRField("state", size=1, description="Current power state of the SOC"),
            CSRField("monkey", size=2, description="Power-on key monitor input"),
        ])
        self.mon0 = Signal()
        self.mon1 = Signal()
        self.soc_on = Signal()
        self.comb += [
            pads.sys_on.eq(self.power.fields.self),
            pads.u_to_t_on.eq(self.power.fields.soc_on),
            pads.fpga_dis.eq(self.power.fields.discharge & ~pads.s0),
            self.stats.fields.state.eq(pads.s0),
            self.stats.fields.monkey.eq(Cat(self.mon0, self.mon1)),

            self.soc_on.eq(self.power.fields.soc_on),
        ]

# a pared-down version of GitInfo to use less gates
class GitInfo(Module, AutoCSR, AutoDoc):
    def __init__(self):
        self.intro = ModuleDoc("""SoC Version Information

            This block contains various information about the state of the source code
            repository when this SoC was built.
            """)

        def makeint(i, base=10):
            try:
                return int(i, base=base)
            except:
                return 0
        def get_gitver():
            major = 0
            minor = 0
            rev = 0
            gitrev = 0
            gitextra = 0
            dirty = 0

            def decode_version(v):
                version = v.split(".")
                major = 0
                minor = 0
                rev = 0
                if len(version) >= 3:
                    rev = makeint(version[2])
                if len(version) >= 2:
                    minor = makeint(version[1])
                if len(version) >= 1:
                    major = makeint(version[0])
                return (major, minor, rev)
            git_rev_cmd = subprocess.Popen(["git", "describe", "--tags", "--long", "--dirty=+", "--abbrev=8"],
                                stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE)
            (git_stdout, _) = git_rev_cmd.communicate()
            if git_rev_cmd.wait() != 0:
                print('unable to get git version')
                return (major, minor, rev, gitrev, gitextra, dirty)
            raw_git_rev = git_stdout.decode().strip()

            if raw_git_rev[-1] == "+":
                raw_git_rev = raw_git_rev[:-1]
                dirty = 1

            parts = raw_git_rev.split("-")

            if len(parts) >= 3:
                if parts[0].startswith("v"):
                    version = parts[0]
                    if version.startswith("v"):
                        version = parts[0][1:]
                    (major, minor, rev) = decode_version(version)
                gitextra = makeint(parts[1])
                if parts[2].startswith("g"):
                    gitrev = makeint(parts[2][1:], base=16)
            elif len(parts) >= 2:
                if parts[1].startswith("g"):
                    gitrev = makeint(parts[1][1:], base=16)
                version = parts[0]
                if version.startswith("v"):
                    version = parts[0][1:]
                (major, minor, rev) = decode_version(version)
            elif len(parts) >= 1:
                version = parts[0]
                if version.startswith("v"):
                    version = parts[0][1:]
                (major, minor, rev) = decode_version(version)

            return (major, minor, rev, gitrev, gitextra, dirty)

        (major, minor, rev, gitrev, gitextra, dirty) = get_gitver()

        self.gitrev = CSRStatus(32, reset=gitrev, description="First 32-bits of the git revision.  This documentation was built from git rev ``{:08x}``, so this value is {}, which should be enough to check out the exact git version used to build this firmware.".format(gitrev, gitrev))
        self.dirty = CSRStatus(fields=[
            CSRField("dirty", reset=dirty, description="Set to ``1`` if this device was built from a git repo with uncommitted modifications.")
        ])

        self.comb += [
            self.gitrev.status.eq(gitrev),
            self.dirty.fields.dirty.eq(dirty),
        ]

class BaseSoC(SoCCore):
    SoCCore.mem_map = {
        "rom":      0x00000000,  # (default shadow @0x80000000)
        "sram":     0x10000000,  # (default shadow @0xa0000000)
        "spiflash": 0x20000000,  # (default shadow @0xa0000000)
        "i2c":      0xb0000000,
        "csr":      0xe0000000,  # (default shadow @0xe0000000)
        "com":      0xd0000000,
    }

    def __init__(self, platform,
                 use_dsp=False, placer="heap", output_dir="build",
                 pnr_seed=0, sim=False,
                 **kwargs):

        self.output_dir = output_dir

        clk_freq = int(sysclkfreq)

        # Core -------------------------------------------------------------------------------------------
        SoCCore.__init__(self, platform, clk_freq,
            integrated_sram_size=0,
            with_uart=False,
            cpu_reset_address=self.mem_map["spiflash"]+GATEWARE_SIZE,
            csr_data_width=32, **kwargs)

        self.submodules.crg = platform.CRG(platform)
        self.add_csr("crg")

        # Version ----------------------------------------------------------------------------------------
        self.submodules.git = GitInfo()
        self.add_csr("git")

        # Power management -------------------------------------------------------------------------------
        # Betrusted Power management interface
        self.submodules.power = BtPower(platform.request("power"))
        self.add_csr("power")

        # Keyboard power-on + debug mux ------------------------------------------------------------------

        # Use to debug bootstrapping issues, avoid internal state on SoC messing with access to UART pads.
        # Prevents sleep/wake from working due to loss of modal pinmux.
        debugonly = False

        if debugonly:
            self.submodules.uart_phy = uart.UARTPHY(
                pads=platform.request("serial"),
                clk_freq=clk_freq,
                baudrate=115200)
            self.submodules.uart = ResetInserter()(uart.UART(self.uart_phy,
            tx_fifo_depth=16,
            rx_fifo_depth=16))
            self.add_csr("uart_phy")
            self.add_csr("uart")
            self.add_interrupt("uart")
        else:
            serialpads = platform.request("serial")
            dbguart_tx = Signal()
            dbguart_rx = Signal()
            dbgpads = Record([('rx', 1), ('tx', 1)], name="serial")
            dbgpads.rx = dbguart_rx
            dbgpads.tx = dbguart_tx

            # serialpad RX needs to drive a scan signal so we can detect it on the other side of the matrix
            keycol0_ts = TSTriple(1)
            self.specials += keycol0_ts.get_tristate(serialpads.rx)
            #self.comb += serialpads.rx.eq(1)

            self.comb += dbgpads.rx.eq(keycol0_ts.i)

            self.comb += self.power.mon1.eq(platform.request("up5k_keyrow1"))
            self.comb += self.power.mon0.eq(platform.request("up5k_keyrow0"))

            # serialpad TX is what we use to test for keyboard hit to power on the SOC
            # only allow test keyboard hit patterns when the SOC is powered off
            self.comb += [
                If(self.power.stats.fields.state,
                    serialpads.tx.eq(dbgpads.tx),
                    keycol0_ts.oe.eq(0)
                ).Else(
                    serialpads.tx.eq(self.power.power.fields.kbddrive),
                    keycol0_ts.oe.eq(1)
                )
            ]
            self.comb += keycol0_ts.o.eq(1) # drive a '1' for scan

            # Uart block ------------------------------------------------------------------------------------
            self.submodules.uart_phy = uart.UARTPHY(
                pads=dbgpads,
                clk_freq=clk_freq,
                baudrate=115200)
            self.submodules.uart = ResetInserter()(uart.UART(self.uart_phy,
                tx_fifo_depth=16,
                rx_fifo_depth=16))
            self.add_csr("uart_phy")
            self.add_csr("uart")
            self.add_interrupt("uart")

        # RAM/ROM/reset cluster --------------------------------------------------------------------------
        spram_size = 128*1024
        self.submodules.spram = up5kspram.Up5kSPRAM(size=spram_size)
        self.register_mem("sram", self.mem_map["sram"], self.spram.bus, spram_size)

        # Add a simple bit-banged SPI Flash module
        spi_pads = platform.request("spiflash")
        self.submodules.picorvspi = PicoRVSpi(platform, spi_pads)
        self.register_mem("spiflash", self.mem_map["spiflash"],
            self.picorvspi.bus, size=SPI_FLASH_SIZE)
        self.add_csr("picorvspi")

        # I2C --------------------------------------------------------------------------------------------
        self.submodules.i2c = HardI2C(platform, platform.request("i2c", 0))
        self.add_wb_slave(self.mem_map["i2c"], self.i2c.bus, 16*4)
        self.add_memory_region("i2c", self.mem_map["i2c"], 16*4, type='io')
        self.add_csr("i2c")
        self.add_interrupt("i2c")

        # High-resolution tick timer ---------------------------------------------------------------------
        self.submodules.ticktimer = TickTimer(1000, clk_freq, bits=40)
        self.add_csr("ticktimer")
        self.add_interrupt("ticktimer")

        # COM port (spi peripheral to Artix) ------------------------------------------------------------------
        self.submodules.com = SpiFifoPeripheral(platform.request("com"), pipeline_cipo=True)
        self.add_wb_slave(self.mem_map["com"], self.com.bus, 4)
        self.add_memory_region("com", self.mem_map["com"], 4, type='io')
        self.add_csr("com")
        self.comb += self.com.oe.eq(self.power.stats.fields.state)  # only drive to FPGA when it's powered up

        # SPI port to wifi (controller) ------------------------------------------------------------------
        self.submodules.wifi = ClockDomainsRenamer({'spi':'sys'})(SpiController(platform.request("wifi"), gpio_cs=True))  # control CS with GPIO per wf200 API spec
        self.add_csr("wifi")
        self.add_interrupt("wifi")


        #### Platform config & build below ---------------------------------------------------------------
        # Override default LiteX's yosys/build templates
        assert hasattr(platform.toolchain, "yosys_template")
        assert hasattr(platform.toolchain, "build_template")
        platform.toolchain.yosys_template = [
            "{read_files}",
            "attrmap -tocase keep -imap keep=\"true\" keep=1 -imap keep=\"false\" keep=0 -remove keep=0",
            "synth_ice40 -json {build_name}.json -top {build_name}",
        ]
        platform.toolchain.build_template = [
            "yosys -q -l {build_name}.rpt {build_name}.ys",
            "nextpnr-ice40 --json {build_name}.json --pcf {build_name}.pcf --asc {build_name}.txt \
            --pre-pack {build_name}_pre_pack.py --{architecture} --package {package}",
            "icepack {build_name}.txt {build_name}.bin"
        ]

        # Add "-relut -dffe_min_ce_use 4" to the synth_ice40 command.
        # The "-reult" adds an additional LUT pass to pack more stuff in,
        # and the "-dffe_min_ce_use 4" flag prevents Yosys from generating a
        # Clock Enable signal for a LUT that has fewer than 4 flip-flops.
        # This increases density, and lets us use the FPGA more efficiently.
        platform.toolchain.yosys_template[2] += " -relut -abc2 -dffe_min_ce_use 4 -relut"
        if use_dsp:
            platform.toolchain.yosys_template[2] += " -dsp"

        # Disable final deep-sleep power down so firmware words are loaded
        # onto softcore's address bus.
        platform.toolchain.build_template[2] = "icepack -s {build_name}.txt {build_name}.bin"

        # Allow us to set the nextpnr seed
        platform.toolchain.build_template[1] += " --seed " + str(pnr_seed)

        if placer is not None:
            platform.toolchain.build_template[1] += " --placer {}".format(placer)

        # Allow loops for RNG placement
        platform.toolchain.build_template[1] += " --ignore-loops"

        if sim:
            class _WishboneBridge(Module):
                def __init__(self, interface):
                    self.wishbone = interface
            self.add_cpu(_WishboneBridge(self.platform.request("wishbone")))
            self.add_wb_master(self.cpu.wishbone)


    def copy_memory_file(self, src):
        import os
        from shutil import copyfile
        if not os.path.exists(self.output_dir):
            os.mkdir(self.output_dir)
        if not os.path.exists(os.path.join(self.output_dir, "gateware")):
            os.mkdir(os.path.join(self.output_dir, "gateware"))
        copyfile(os.path.join("rtl", src), os.path.join(self.output_dir, "gateware", src))


def make_multiboot_header(filename, boot_offsets=[160]):
    """
    ICE40 allows you to program the SB_WARMBOOT state machine by adding the following
    values to the bitstream, before any given image:

    [7e aa 99 7e]       Sync Header
    [92 00 k0]          Boot mode (k = 1 for cold boot, 0 for warmboot)
    [44 03 o1 o2 o3]    Boot address
    [82 00 00]          Bank offset
    [01 08]             Reboot
    [...]               Padding (up to 32 bytes)

    Note that in ICE40, the second nybble indicates the number of remaining bytes
    (with the exception of the sync header).

    The above construct is repeated five times:

    INITIAL_BOOT        The image loaded at first boot
    BOOT_S00            The first image for SB_WARMBOOT
    BOOT_S01            The second image for SB_WARMBOOT
    BOOT_S10            The third image for SB_WARMBOOT
    BOOT_S11            The fourth image for SB_WARMBOOT
    """
    while len(boot_offsets) < 5:
        boot_offsets.append(boot_offsets[0])

    with open(filename, 'wb') as output:
        for offset in boot_offsets:
            # Sync Header
            output.write(bytes([0x7e, 0xaa, 0x99, 0x7e]))

            # Boot mode
            output.write(bytes([0x92, 0x00, 0x00]))

            # Boot address
            output.write(bytes([0x44, 0x03,
                    (offset >> 16) & 0xff,
                    (offset >> 8)  & 0xff,
                    (offset >> 0)  & 0xff]))

            # Bank offset
            output.write(bytes([0x82, 0x00, 0x00]))

            # Reboot command
            output.write(bytes([0x01, 0x08]))

            for x in range(17, 32):
                output.write(bytes([0]))

def pad_file(pad_src, pad_dest, length):
    with open(pad_dest, "wb") as output:
        with open(pad_src, "rb") as b:
            output.write(b.read())
        output.truncate(length)

def merge_file(bios, gateware, dest):
    with open(dest, "wb") as output:
        count = 0
        with open(gateware, "rb") as gw:
            count = count + output.write(gw.read())
        with open(bios, "rb") as b:
            b.seek(count)
            output.write(b.read())



def main():
    if os.environ['PYTHONHASHSEED'] != "1":
        print( "PYTHONHASHEED must be set to 1 for consistent validation results. Failing to set this results in non-deterministic compilation results")
        exit()

    parser = argparse.ArgumentParser(description="Build the Betrusted Embedded Controller")
    parser.add_argument(
        "--revision", choices=["dvt", "pvt"], default="pvt",
        help="build EC for a particular hardware revision"
    )
    parser.add_argument(
        "-D", "--document-only", default=False, action="store_true", help="Build docs only"
    )
    parser.add_argument(
        "--with-dsp", help="use dsp inference in yosys (not all yosys builds have -dsp)", action="store_true"
    )
    parser.add_argument(
        "--no-cpu", help="disable cpu generation for debugging purposes", action="store_true"
    )
    parser.add_argument(
        "--placer", choices=["sa", "heap"], help="which placer to use in nextpnr", default="heap",
    )
    parser.add_argument(
        "--seed", default=0, help="seed to use in nextpnr"
    )
    args = parser.parse_args()

    output_dir = 'build'

    compile_gateware = True
    compile_software = False # this is now done with Rust

    if args.document_only:
        compile_gateware = False
        compile_software = False

    cpu_type = "vexriscv"
    cpu_variant = "minimal"
    #cpu_variant = cpu_variant + "+debug"

    if args.no_cpu:
        cpu_type = None
        cpu_variant = None

    if args.revision == 'dvt' or args.revision == 'pvt':
        io = io_pvt
    else:
        print("Invalid hardware revision")
        exit(1)

    platform = BetrustedPlatform(io, revision=args.revision)

    soc = BaseSoC(platform, cpu_type=cpu_type, cpu_variant=cpu_variant,
                            use_dsp=args.with_dsp, placer=args.placer,
                            pnr_seed=args.seed,
                            output_dir=output_dir)
    builder = Builder(soc, output_dir=output_dir, csr_csv="build/csr.csv", compile_software=compile_software, compile_gateware=compile_gateware)
    # If we compile software, pull the code from somewhere other than
    # the built-in litex "bios" binary, which makes assumptions about
    # what peripherals are available.
    if compile_software:
        builder.software_packages = [
            ("bios", os.path.abspath(os.path.join(os.path.dirname(__file__), "bios")))
        ]

    try:
        vns = builder.build()
    except OSError:
        exit(1)

    soc.do_exit(vns)

    if not args.document_only:
        make_multiboot_header(os.path.join(output_dir, "gateware", "multiboot-header.bin"), [
            160,
            160,
            157696,
            262144,
            262144 + 32768,
        ])

        with open(os.path.join(output_dir, 'gateware', 'multiboot-header.bin'), 'rb') as multiboot_header_file:
            multiboot_header = multiboot_header_file.read()
            with open(os.path.join(output_dir, 'gateware', 'betrusted_ec.bin'), 'rb') as top_file:
                top = top_file.read()
                with open(os.path.join(output_dir, 'gateware', 'betrusted_ec_multiboot.bin'), 'wb') as top_multiboot_file:
                    top_multiboot_file.write(multiboot_header)
                    top_multiboot_file.write(top)
        pad_file(os.path.join(output_dir, 'gateware', 'betrusted_ec.bin'), os.path.join(output_dir, 'gateware', 'betrusted_ec_pad.bin'), 0x1a000)
        pad_file(os.path.join(output_dir, 'gateware', 'betrusted_ec_multiboot.bin'), os.path.join(output_dir, 'gateware', 'betrusted_ec_multiboot_pad.bin'), 0x1a000)

    lxsocdoc.generate_docs(soc, "build/documentation", note_pulses=True)
    lxsocdoc.generate_svd(soc, "build/software")

if __name__ == "__main__":
    from datetime import datetime
    start = datetime.now()
    main()
    print("Run completed in {}".format(datetime.now()-start))
