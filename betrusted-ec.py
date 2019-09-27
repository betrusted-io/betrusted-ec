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

from migen import *
from migen import Module, Signal, Instance, ClockDomain, If
from migen.genlib.resetsync import AsyncResetSynchronizer
from migen.fhdl.specials import TSTriple
from migen.fhdl.bitcontainer import bits_for
from migen.fhdl.structure import ClockSignal, ResetSignal, Replicate, Cat

from litex.build.lattice.platform import LatticePlatform
from litex.build.generic_platform import Pins, IOStandard, Misc, Subsignal
from litex.soc.cores import up5kspram
from litex.soc.integration import SoCCore
from litex.soc.integration.builder import Builder
from litex.soc.integration.soc_core import csr_map_update
from litex.soc.interconnect import wishbone
from litex.soc.interconnect.csr import AutoCSR, CSRStatus, CSRStorage

from rtl.rtl_i2c import RtlI2C

import lxsocdoc

# Ish. It's actually slightly smaller, but this is divisible by 4.
GATEWARE_SIZE = 0x1a000

# 1 MB (8 Mb)
SPI_FLASH_SIZE = 1 * 1024 * 1024

_io = [
    ("serial", 0,
     Subsignal("rx", Pins("48")),
     Subsignal("tx", Pins("3"), Misc("PULLUP")),
     IOStandard("LVCMOS33")
     ),
    # serial is muxed in with these key monitor pins -- TODO
#    ("up5k_keycol0", 0, Pins("48"), IOStandard("LVCMOS33")),
#    ("up5k_keycol1", 0, Pins("3"), IOStandard("LVCMOS33")),
    ("up5k_keyrow0", 0, Pins("4"), IOStandard("LVCMOS33")),
    ("up5k_keyrow1", 0, Pins("2"), IOStandard("LVCMOS33")),

    ("spiflash", 0,
     Subsignal("cs_n", Pins("16"), IOStandard("LVCMOS18")),
     Subsignal("clk", Pins("15"), IOStandard("LVCMOS18")),
     Subsignal("miso", Pins("17"), IOStandard("LVCMOS18")),
     Subsignal("mosi", Pins("14"), IOStandard("LVCMOS18")),
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
     ),
    ("gg_int", 0, Pins("42"), IOStandard("LVCMOS18")),
    ("gyro_int0", 0, Pins("43"), IOStandard("LVCMOS18")),

    ("fpga_dis", 0, Pins("21"), IOStandard("LVCMOS18")),

    ("clk12", 0, Pins("35"), IOStandard("LVCMOS18")),

    ("com_cs", 0, Pins("11"), IOStandard("LVCMOS18")),
    ("com_irq", 0, Pins("6"), IOStandard("LVCMOS18")),
    ("com_miso", 0, Pins("10"), IOStandard("LVCMOS18")),
    ("com_mosi", 0, Pins("9"), IOStandard("LVCMOS18")),
    ("com_sclk", 0, Pins("20"), IOStandard("LVCMOS18")),

    ("extcommin", 0, Pins("45"), IOStandard("LVCMOS33")),
    ("lcd_disp", 0, Pins("44"), IOStandard("LVCMOS33")),

    ("pwr_s0", 0, Pins("19"), IOStandard("LVCMOS18")),
    ("pwr_s1", 0, Pins("12"), IOStandard("LVCMOS18")),
    ("sys_on", 0, Pins("47"), IOStandard("LVCMOS33")),
    ("u_to_t_on", 0, Pins("46"), IOStandard("LVCMOS33")),

    ("led", 0,
         Subsignal("rgb0", Pins("39"), IOStandard("LVCMOS33")),
         Subsignal("rgb1", Pins("40"), IOStandard("LVCMOS33")),
         Subsignal("rgb2", Pins("41"), IOStandard("LVCMOS33")),
     ),

    ("wifi_cs", 0, Pins("28"), IOStandard("LVCMOS18")),
    ("wifi_lpclk", 0, Pins("37"), IOStandard("LVCMOS18")),
    ("wifi_miso", 0, Pins("32"), IOStandard("LVCMOS18")),
    ("wifi_mosi", 0, Pins("34"), IOStandard("LVCMOS18")),
    ("wifi_pa_enable", 0, Pins("27"), IOStandard("LVCMOS18")),
    ("wifi_res_n", 0, Pins("26"), IOStandard("LVCMOS18")),
    ("wifi_sclk", 0, Pins("36"), IOStandard("LVCMOS18")),
    ("wifi_wirq", 0, Pins("31"), IOStandard("LVCMOS18")),
    ("wifi_wup", 0, Pins("38"), IOStandard("LVCMOS18")),
]

_connectors = []

class Platform(LatticePlatform):
    def __init__(self, toolchain="icestorm", revision="evt"):
        LatticePlatform.__init__(self, "ice40-up5k-sg48", _io, toolchain="icestorm")

    def create_programmer(self):
        raise ValueError("programming is not supported")


class _CRG(Module):
    def __init__(self, platform):
        clk12_raw = platform.request("clk12")
        clk12 = Signal()

        reset_delay = Signal(12, reset=4095)
        self.clock_domains.cd_por = ClockDomain()
        self.reset = Signal()

        self.clock_domains.cd_sys = ClockDomain()

        platform.add_period_constraint(self.cd_sys.clk, 1e9/12e6)

        # POR reset logic- POR generated from sys clk, POR logic feeds sys clk
        # reset.
        self.comb += [
            self.cd_por.clk.eq(self.cd_sys.clk),
            self.cd_sys.rst.eq(reset_delay != 0),
        ]

        self.specials += Instance(
            "SB_GB",
            i_USER_SIGNAL_TO_GLOBAL_BUFFER=clk12_raw,
            o_GLOBAL_BUFFER_OUTPUT=clk12,
        )

        self.comb += self.cd_sys.clk.eq(clk12)

        self.sync.por += \
            If(reset_delay != 0,
                reset_delay.eq(reset_delay - 1)
            )
        self.specials += AsyncResetSynchronizer(self.cd_por, self.reset)

boot_offset = 0x1000000
bios_size = 0x8000


class RandomFirmwareROM(wishbone.SRAM):
    """
    Seed the random data with a fixed number, so different bitstreams
    can all share firmware.
    """
    def __init__(self, size, seed=2373):
        def xorshift32(x):
            x = x ^ (x << 13) & 0xffffffff
            x = x ^ (x >> 17) & 0xffffffff
            x = x ^ (x << 5)  & 0xffffffff
            return x & 0xffffffff

        def get_rand(x):
            out = 0
            for i in range(32):
                x = xorshift32(x)
                if (x & 1) == 1:
                    out = out | (1 << i)
            return out & 0xffffffff
        data = []
        seed = 1
        for d in range(int(size / 4)):
            seed = get_rand(seed)
            data.append(seed)
        wishbone.SRAM.__init__(self, size, read_only=True, init=data)

class FirmwareROM(wishbone.SRAM):
    def __init__(self, size, filename):
        data = []
        with open(filename, 'rb') as inp:
            data = inp.read()
        wishbone.SRAM.__init__(self, size, read_only=True, init=data)

class SBLED(Module, AutoCSR):
    def __init__(self, revision, pads):
        rgba_pwm = Signal(3)

        self.dat = CSRStorage(8)
        self.addr = CSRStorage(4)
        self.ctrl = CSRStorage(6)
        self.raw = CSRStorage(3)

        ledd_value = Signal(3)
        self.comb += [
            If(self.ctrl.storage[3], rgba_pwm[0].eq(self.raw.storage[0])).Else(rgba_pwm[0].eq(ledd_value[0])),
            If(self.ctrl.storage[4], rgba_pwm[1].eq(self.raw.storage[1])).Else(rgba_pwm[1].eq(ledd_value[1])),
            If(self.ctrl.storage[5], rgba_pwm[2].eq(self.raw.storage[2])).Else(rgba_pwm[2].eq(ledd_value[2])),
        ]

        self.specials += Instance("SB_RGBA_DRV",
            i_CURREN = self.ctrl.storage[1],
            i_RGBLEDEN = self.ctrl.storage[2],
            i_RGB0PWM = rgba_pwm[0],
            i_RGB1PWM = rgba_pwm[1],
            i_RGB2PWM = rgba_pwm[2],
            o_RGB0 = pads.rgb0,
            o_RGB1 = pads.rgb1,
            o_RGB2 = pads.rgb2,
            p_CURRENT_MODE = "0b1",
            p_RGB0_CURRENT = "0b000011",
            p_RGB1_CURRENT = "0b000011",
            p_RGB2_CURRENT = "0b000011",
        )

        self.specials += Instance("SB_LEDDA_IP",
            i_LEDDCS = self.dat.re,
            i_LEDDCLK = ClockSignal(),
            i_LEDDDAT7 = self.dat.storage[7],
            i_LEDDDAT6 = self.dat.storage[6],
            i_LEDDDAT5 = self.dat.storage[5],
            i_LEDDDAT4 = self.dat.storage[4],
            i_LEDDDAT3 = self.dat.storage[3],
            i_LEDDDAT2 = self.dat.storage[2],
            i_LEDDDAT1 = self.dat.storage[1],
            i_LEDDDAT0 = self.dat.storage[0],
            i_LEDDADDR3 = self.addr.storage[3],
            i_LEDDADDR2 = self.addr.storage[2],
            i_LEDDADDR1 = self.addr.storage[1],
            i_LEDDADDR0 = self.addr.storage[0],
            i_LEDDDEN = self.dat.re,
            i_LEDDEXE = self.ctrl.storage[0],
            # o_LEDDON = led_is_on, # Indicates whether LED is on or not
            # i_LEDDRST = ResetSignal(), # This port doesn't actually exist
            o_PWMOUT0 = ledd_value[0],
            o_PWMOUT1 = ledd_value[1],
            o_PWMOUT2 = ledd_value[2],
            o_LEDDON = Signal(),
        )


class SBWarmBoot(Module, AutoCSR):
    def __init__(self, parent):
        self.ctrl = CSRStorage(size=8)
        self.addr = CSRStorage(size=32)
        do_reset = Signal()
        self.comb += [
            # "Reset Key" is 0xac (0b101011xx)
            do_reset.eq(self.ctrl.storage[2] & self.ctrl.storage[3] & ~self.ctrl.storage[4]
                      & self.ctrl.storage[5] & ~self.ctrl.storage[6] & self.ctrl.storage[7])
        ]
        self.specials += Instance("SB_WARMBOOT",
            i_S0   = self.ctrl.storage[0],
            i_S1   = self.ctrl.storage[1],
            i_BOOT = do_reset,
        )
        parent.config["BITSTREAM_SYNC_HEADER1"] = 0x7e99aa7e
        parent.config["BITSTREAM_SYNC_HEADER2"] = 0x7eaa997e


class PicoRVSpi(Module, AutoCSR):
    def __init__(self, platform, pads, size=2*1024*1024):
        self.size = size

        self.bus = bus = wishbone.Interface()

        self.reset = Signal()

        self.cfg1 = CSRStorage(size=8)
        self.cfg2 = CSRStorage(size=8)
        self.cfg3 = CSRStorage(size=8)
        self.cfg4 = CSRStorage(size=8)

        self.stat1 = CSRStatus(size=8)
        self.stat2 = CSRStatus(size=8)
        self.stat3 = CSRStatus(size=8)
        self.stat4 = CSRStatus(size=8)

        cfg = Signal(32)
        cfg_we = Signal(4)
        cfg_out = Signal(32)
        self.comb += [
            cfg.eq(Cat(self.cfg1.storage, self.cfg2.storage, self.cfg3.storage, self.cfg4.storage)),
            cfg_we.eq(Cat(self.cfg1.re, self.cfg2.re, self.cfg3.re, self.cfg4.re)),
            self.stat1.status.eq(cfg_out[0:8]),
            self.stat2.status.eq(cfg_out[8:16]),
            self.stat3.status.eq(cfg_out[16:24]),
            self.stat4.status.eq(cfg_out[24:32]),
        ]

        mosi_pad = TSTriple()
        miso_pad = TSTriple()
        cs_n_pad = TSTriple()
        clk_pad  = TSTriple()
        wp_pad   = TSTriple()
        hold_pad = TSTriple()
        self.specials += mosi_pad.get_tristate(pads.mosi)
        self.specials += miso_pad.get_tristate(pads.miso)
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
            o_flash_io0_oe = mosi_pad.oe,
            o_flash_io1_oe = miso_pad.oe,
            o_flash_io2_oe = wp_pad.oe,
            o_flash_io3_oe = hold_pad.oe,

            o_flash_io0_do = mosi_pad.o,
            o_flash_io1_do = miso_pad.o,
            o_flash_io2_do = wp_pad.o,
            o_flash_io3_do = hold_pad.o,
            o_flash_csb    = cs_n_pad.o,
            o_flash_clk    = clk_pad.o,

            i_flash_io0_di = mosi_pad.i,
            i_flash_io1_di = miso_pad.i,
            i_flash_io2_di = wp_pad.i,
            i_flash_io3_di = hold_pad.i,

            i_resetn = ~reset,
            i_clk = ClockSignal(),

            i_valid = bus.stb & bus.cyc,
            o_ready = spi_ready,
            i_addr  = flash_addr,
            o_rdata = o_rdata,

            i_cfgreg_we = cfg_we,
            i_cfgreg_di = cfg,
            o_cfgreg_do = cfg_out,
        )
        platform.add_source("rtl/spimemio.v")

class Version(Module, AutoCSR):
    def __init__(self, model):
        def makeint(i, base=10):
            try:
                return int(i, base=base)
            except:
                return 0
        def get_gitver():
            import subprocess
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
            git_rev_cmd = subprocess.Popen(["git", "describe", "--tags", "--dirty=+"],
                                stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE)
            (git_stdout, _) = git_rev_cmd.communicate()
            if git_rev_cmd.wait() != 0:
                print('unable to get git version')
                return
            raw_git_rev = git_stdout.decode().strip()

            dirty = False
            if raw_git_rev[-1] == "+":
                raw_git_rev = raw_git_rev[:-1]
                dirty = True

            parts = raw_git_rev.split("-")
            major = 0
            minor = 0
            rev = 0
            gitrev = 0
            gitextra = 0

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

        self.major = CSRStatus(8)
        self.minor = CSRStatus(8)
        self.revision = CSRStatus(8)
        self.gitrev = CSRStatus(32)
        self.gitextra = CSRStatus(10)
        self.dirty = CSRStatus(1)
        self.model = CSRStatus(8)

        (major, minor, rev, gitrev, gitextra, dirty) = get_gitver()
        self.comb += [
            self.major.status.eq(major),
            self.minor.status.eq(minor),
            self.revision.status.eq(rev),
            self.gitrev.status.eq(gitrev),
            self.gitextra.status.eq(gitextra),
            self.dirty.status.eq(dirty),
        ]
        if model == "evt":
            self.comb += self.model.status.eq(0x45) # 'E'
        elif model == "dvt":
            self.comb += self.model.status.eq(0x44) # 'D'
        elif model == "pvt":
            self.comb += self.model.status.eq(0x50) # 'P'
        elif model == "hacker":
            self.comb += self.model.status.eq(0x48) # 'H'
        else:
            self.comb += self.model.status.eq(0x3f) # '?'

class BaseSoC(SoCCore):
    SoCCore.csr_map = {
        "ctrl":           0,  # provided by default (optional)
        "crg":            1,  # user
        "uart_phy":       2,  # provided by default (optional)
        "uart":           3,  # provided by default (optional)
        "identifier_mem": 4,  # provided by default (optional)
        "timer0":         5,  # provided by default (optional)
        "cpu_or_bridge":  8,
        "picorvspi":      10,
        "reboot":         12,
        "version":        14,
    }

    SoCCore.mem_map = {
#        "rom":      0x00000000,  # (default shadow @0x80000000)
        "sram":     0x10000000,  # (default shadow @0xa0000000)
        "spiflash": 0x20000000,  # (default shadow @0xa0000000)
        "main_ram": 0x40000000,  # (default shadow @0xc0000000)
        "csr":      0x60000000,  # (default shadow @0xe0000000)
    }

    def __init__(self, platform, boot_source="spi",
                 debug=None, bios_file=None,
                 use_dsp=False, placer="heap", output_dir="build",
                 pnr_seed=0,
                 **kwargs):
        # Disable integrated RAM as we'll add it later
        self.integrated_sram_size = 0

        self.output_dir = output_dir

        clk_freq = int(12e6)
        self.submodules.crg = _CRG(platform)

        SoCCore.__init__(self, platform, clk_freq, integrated_sram_size=0, with_uart=True, **kwargs)

        if debug is not None:
            if debug == "uart":
                from litex.soc.cores.uart import UARTWishboneBridge
                self.submodules.uart_bridge = UARTWishboneBridge(platform.request("serial"), clk_freq, baudrate=115200)
                self.add_wb_master(self.uart_bridge.wishbone)
            if hasattr(self, "cpu"):
                self.cpu.use_external_variant("rtl/VexRiscv_Fomu_Debug.v")
                os.path.join(output_dir, "gateware")
                self.register_mem("vexriscv_debug", 0xf00f0000, self.cpu.debug_bus, 0x100)
        else:
            if hasattr(self, "cpu"):
                self.cpu.use_external_variant("rtl/VexRiscv_Fomu.v")

        # SPRAM- UP5K has single port RAM, might as well use it as SRAM to
        # free up scarce block RAM.
        spram_size = 128*1024
        self.submodules.spram = up5kspram.Up5kSPRAM(size=spram_size)
        self.register_mem("sram", self.mem_map["sram"], self.spram.bus, spram_size)

        if boot_source == "rand":
            kwargs['cpu_reset_address']=0
            bios_size = 0x2000
            self.submodules.random_rom = RandomFirmwareROM(bios_size)
            self.add_constant("ROM_DISABLE", 1)
            self.register_rom(self.random_rom.bus, bios_size)
        elif boot_source == "bios":
            kwargs['cpu_reset_address'] = 0
            if bios_file is None:
                self.integrated_rom_size = bios_size = 0x2000
                self.submodules.rom = wishbone.SRAM(bios_size, read_only=True, init=[])
                self.register_rom(self.rom.bus, bios_size)
            else:
                bios_size = 0x2000
                self.submodules.firmware_rom = FirmwareROM(bios_size, bios_file)
                self.add_constant("ROM_DISABLE", 1)
                self.register_rom(self.firmware_rom.bus, bios_size)

        elif boot_source == "spi":
            bios_size = 0x8000
            kwargs['cpu_reset_address']=self.mem_map["spiflash"]+GATEWARE_SIZE
            self.flash_boot_address = self.mem_map["spiflash"]+GATEWARE_SIZE+bios_size
            self.add_memory_region("rom", 0, 0) # Required to keep litex happy
        else:
            raise ValueError("unrecognized boot_source: {}".format(boot_source))

        # Add a simple bit-banged SPI Flash module
        spi_pads = platform.request("spiflash")
        self.submodules.picorvspi = PicoRVSpi(platform, spi_pads)
        self.register_mem("spiflash", self.mem_map["spiflash"],
            self.picorvspi.bus, size=SPI_FLASH_SIZE)

        self.submodules.reboot = SBWarmBoot(self)
        if hasattr(self, "cpu"):
            self.cpu.cpu_params.update(
                i_externalResetVector=self.reboot.addr.storage,
            )

        # self.submodules.version = Version(platform.revision)

        # add I2C interface
        self.submodules.i2c = RtlI2C(platform, platform.request("i2c", 0))
        self.add_csr("i2c")
        self.add_interrupt("i2c")

        self.comb += platform.request("fpga_dis", 0).eq(0)

        # Add "-relut -dffe_min_ce_use 4" to the synth_ice40 command.
        # The "-reult" adds an additional LUT pass to pack more stuff in,
        # and the "-dffe_min_ce_use 4" flag prevents Yosys from generating a
        # Clock Enable signal for a LUT that has fewer than 4 flip-flops.
        # This increases density, and lets us use the FPGA more efficiently.
        platform.toolchain.nextpnr_yosys_template[2] += " -relut -dffe_min_ce_use 4"
        if use_dsp:
            platform.toolchain.nextpnr_yosys_template[2] += " -dsp"

        # Disable final deep-sleep power down so firmware words are loaded
        # onto softcore's address bus.
        platform.toolchain.build_template[3] = "icepack -s {build_name}.txt {build_name}.bin"
        platform.toolchain.nextpnr_build_template[2] = "icepack -s {build_name}.txt {build_name}.bin"

        # Allow us to set the nextpnr seed
        platform.toolchain.nextpnr_build_template[1] += " --seed " + str(pnr_seed)

        if placer is not None:
            platform.toolchain.nextpnr_build_template[1] += " --placer {}".format(placer)

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


def main():
    if os.environ['PYTHONHASHSEED'] != "1":
        print( "PYTHONHASHEED must be set to 1 for consistent validation results. Failing to set this results in non-deterministic compilation results")
        exit()

    parser = argparse.ArgumentParser(description="Build the Betrusted Embedded Controller")
    parser.add_argument(
        "--boot-source", choices=["spi", "rand", "bios"], default="spi",
        help="where to have the CPU obtain its executable code from"
    )
    parser.add_argument(
        "--with-debug", help="enable debug support", choices=["usb", "uart", None]
    )
    parser.add_argument(
        "--revision", choices=["evt"], default="evt",
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
    parser.add_argument(
        "--export-random-rom-file", help="Generate a random ROM file and save it to a file"
    )
    args = parser.parse_args()

    output_dir = 'build'

    if args.export_random_rom_file is not None:
        size = 0x2000
        def xorshift32(x):
            x = x ^ (x << 13) & 0xffffffff
            x = x ^ (x >> 17) & 0xffffffff
            x = x ^ (x << 5)  & 0xffffffff
            return x & 0xffffffff

        def get_rand(x):
            out = 0
            for i in range(32):
                x = xorshift32(x)
                if (x & 1) == 1:
                    out = out | (1 << i)
            return out & 0xffffffff
        seed = 1
        with open(args.export_random_rom_file, "w", newline="\n") as output:
            for d in range(int(size / 4)):
                seed = get_rand(seed)
                print("{:08x}".format(seed), file=output)
        return 0

    compile_gateware = True
    compile_software = True

    if args.document_only:
        compile_gateware = False
        compile_software = False

    cpu_type = "vexriscv"
    cpu_variant = "min"
    if args.with_debug:
        cpu_variant = cpu_variant + "+debug"

    if args.no_cpu:
        cpu_type = None
        cpu_variant = None

    platform = Platform(revision=args.revision)

    soc = BaseSoC(platform, cpu_type=cpu_type, cpu_variant=cpu_variant,
                            debug=args.with_debug, boot_source=args.boot_source,
                            use_dsp=args.with_dsp, placer=args.placer,
                            pnr_seed=args.seed,
                            output_dir=output_dir)
    builder = Builder(soc, output_dir=output_dir, csr_csv="build/csr.csv", compile_software=compile_software, compile_gateware=compile_gateware)
    if compile_software:
        builder.software_packages = [
            ("bios", os.path.abspath(os.path.join(os.path.dirname(__file__), "bios")))
        ]
    vns = builder.build()
    soc.do_exit(vns)

    make_multiboot_header(os.path.join(output_dir, "gateware", "multiboot-header.bin"), [
        160,
        160,
        157696,
        262144,
        262144 + 32768,
    ])

    with open(os.path.join(output_dir, 'gateware', 'multiboot-header.bin'), 'rb') as multiboot_header_file:
        multiboot_header = multiboot_header_file.read()
        with open(os.path.join(output_dir, 'gateware', 'top.bin'), 'rb') as top_file:
            top = top_file.read()
            with open(os.path.join(output_dir, 'gateware', 'top-multiboot.bin'), 'wb') as top_multiboot_file:
                top_multiboot_file.write(multiboot_header)
                top_multiboot_file.write(top)

    lxsocdoc.generate_docs(soc, "build/documentation", note_pulses=True)
    lxsocdoc.generate_svd(soc, "build/software")

if __name__ == "__main__":
    main()
