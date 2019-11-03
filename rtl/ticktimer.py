from migen import *

from litex.soc.interconnect.csr import *
from migen.genlib.cdc import MultiReg
from litex.soc.interconnect.csr_eventmanager import *
from litex.soc.integration.doc import AutoDoc, ModuleDoc

class TickTimer(Module, AutoCSR, AutoDoc):
    """Millisecond timer"""
    def __init__(self, clkspertick):
        self.clkspertick = int(clkspertick)

        self.intro = ModuleDoc("""TickTimer: A practical systick timer.
        
        TIMER0 in the system gives a high-resolution, sysclk-speed timer which overflows
        very quickly and requires OS overhead to convert it into a practically usable time source
        which counts off in systicks, instead of sysclks.
        
        The hardware parameter to the block is the number of sysclocks per tick.
        """)

        prescaler = Signal(max=self.clkspertick, reset=self.clkspertick)
        timer = Signal(48)  # offer up to 48 bits of system time

        self.control = CSRStorage(2, fields=[
            CSRField("reset", description="Write a `1` to this bit to reset the count to 0", pulse=True),
            CSRField("pause", description="Write a `1` to this field to pause counting, 0 for free-run")
        ])
        self.time = CSRStatus(48, name="time", description="""Elapsed time in systicks""")

        self.sync += [
            If(self.control.fields.reset,
               timer.eq(0),
               prescaler.eq(self.clkspertick),
            ).Else(
                If(prescaler == 0,
                   prescaler.eq(self.clkspertick),
                   If(self.control.fields.pause == 0,
                      timer.eq(timer + 1),
                    )
                ).Else(
                   prescaler.eq(prescaler - 1),
                )
            )
        ]

        self.comb += self.time.status.eq(timer)


