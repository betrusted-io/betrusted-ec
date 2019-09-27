#!/usr/bin/env python3
# This variable defines all the external programs that this module
# relies on.  lxbuildenv reads this variable in order to ensure
# the build will finish without exiting due to missing third-party
# programs.
LX_DEPENDENCIES = ["riscv", "icestorm", "nextpnr-ice40", "yosys"]

# Import lxbuildenv to integrate the deps/ directory
import lxbuildenv

import argparse
import os

# Disable pylint's E1101, which breaks completely on migen
#pylint:disable=E1101

from migen import *

from litex_boards.partner.targets.fomu import BaseSoC
from litex.soc.integration import SoCCore
from litex.soc.integration.soc_core import csr_map_update
from litex.soc.integration.builder import Builder
from litex.soc.interconnect import wishbone
from litex.soc.interconnect.csr import AutoCSR, CSRStatus, CSRStorage, CSRField

import spibone
import lxsocdoc
from rtl.rtl_i2c import RtlI2C
from rtl.sbled import SBLED
from rtl.messible import Messible
from rtl.warmboot import SBWarmBoot

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

class XoulonSoC(BaseSoC):
    csr_peripherals = [
        "messible",
        "warmboot",
        "rgb",
    ]
    csr_map_update(BaseSoC.csr_map, csr_peripherals)

    def __init__(self, board, boot_source="rand", **kwargs):
        BaseSoC.__init__(self, board, **kwargs)
        if boot_source == "rand":
            kwargs['cpu_reset_address']=0
            bios_size = 0x2000
            self.submodules.random_rom = RandomFirmwareROM(bios_size)
            self.add_constant("ROM_DISABLE", 1)
            self.register_rom(self.random_rom.bus, bios_size)

        # Add debug variant of the CPU
        self.cpu.use_external_variant("rtl/VexRiscv_Fomu_Debug.v")
        os.path.join("build", "gateware")
        self.register_mem("vexriscv_debug", 0xf00f0000, self.cpu.debug_bus, 0x100)

        # add I2C interface
        self.submodules.i2c = RtlI2C(self.platform, self.platform.request("i2c", 0))
        self.add_csr("i2c")
        self.add_interrupt("i2c")

        # # Add SPI Wishbone bridge
        # spi_pads = self.platform.request("spiflash")
        # self.submodules.spibone = ClockDomainsRenamer("usb_12")(spibone.SpiWishboneBridge(spi_pads, wires=4))
        # self.add_wb_master(self.spibone.wishbone)

        # Add a Messible for device->host communications
        self.submodules.messible = Messible()

        # Add the RGB LED for giving feedback
        self.submodules.rgb = SBLED(board, self.platform.request("rgb_led"))

def main():
    parser = argparse.ArgumentParser(
        description="Build Xoulon Development SoC Gateware")
    parser.add_argument(
        "--seed", default=0, help="seed to use in nextpnr"
    )
    parser.add_argument(
        "--document-only", default=False, action="store_true",
        help="Don't build gateware or software, only build documentation"
    )
    args = parser.parse_args()

    soc = XoulonSoC("evt", 
        cpu_type="vexriscv", cpu_variant="min+debug",
        usb_bridge=True, pnr_seed=args.seed)
    builder = Builder(soc, output_dir="build", csr_csv="build/csr.csv",
        compile_software=False, compile_gateware=not args.document_only)
    vns = builder.build()
    soc.do_exit(vns)
    lxsocdoc.generate_docs(soc, "build/documentation/", project_name="Xoulon Test MCU", author="Sean \"xobs\" Cross")
    lxsocdoc.generate_svd(soc, "build/software", vendor="Foosn", name="Xoulon")

if __name__ == "__main__":
    main()
