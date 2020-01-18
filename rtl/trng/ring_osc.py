from migen import *
from migen.genlib.cdc import MultiReg

from litex.soc.interconnect.csr import *
from litex.soc.integration.doc import AutoDoc, ModuleDoc

"""
TrngRingOsc builds a pair of ring oscillators. One is the "slow" oscillator, which circumscribes
the die, and attempts to hit the target_freq supplied as a parameter. The other is a "fast" oscillator,
which is typically targeted to run in the 50-100MHz range (primarily for power reasons). The idea
is to have the "fast" oscillator sample at a period that is faster than the average jitter picked
up by the slow oscillator as it circumscribes the die. Thus, if the quality of entropy is not good
enough, the fix is to slow down the target_freq parameter.

* self.trng_raw is the unsynchronized output TRNG stream
* self.trng_out_sync is the TRNG stream, but jammed through a sysclk synchronizer
* self.trng_slow and self.trng_fast are debug hooks for checking the ring oscillators  
"""
class TrngRingOsc(Module, AutoCSR, AutoDoc):
    def __init__(self, platform, device_root='xc7s50', target_freq=1e6):
        # device_root is passed because the package doesn't matter for the on-chip ring oscillator,
        # just the size of the silicon die.
        self.trng_raw = Signal()  # raw TRNG output bitstream
        self.trng_out_sync = Signal()  # single-bit output, synchronized to sysclk
        self.ctl = CSRStorage(fields=[
            CSRField("ena", size=1, description="Enable the TRNG; 0 puts the TRNG into full powerdown", reset=1)
        ])
        self.rand = CSRStatus(fields=[
            CSRField("rand", size=32, description="Random data shifted into a 32-bit register for easier collection")
        ])
        self.status = CSRStatus(fields=[
            CSRField("fresh", size=1, description="When set, the rand register contains a fresh set of bits to be read; cleaned by reading the `rand` register")
        ])

        rand_strobe = Signal()
        rand_strobe_r = Signal()
        rand_cnt = Signal(max=self.rand.size)
        self.sync += [
            rand_strobe_r.eq(rand_strobe),

            If(self.rand.we,
               rand_cnt.eq(0),
               self.status.fields.fresh.eq(0)
            ).Else(
                If(rand_strobe & ~rand_strobe_r,
                    self.rand.fields.rand.eq(Cat(self.trng_out_sync,self.rand.fields.rand[:-1])),
                    If(rand_cnt < self.rand.size - 1,
                       rand_cnt.eq(rand_cnt + 1),
                       self.status.fields.fresh.eq(0)
                    ).Else(
                       self.status.fields.fresh.eq(1)
                    )
                )
            )
        ]

        target_period = (1/target_freq)*1e9  # period is in ns

        if device_root == 'xc7s50':
            stage_delay = 3  # rough delay of each ring oscllator stage (incl routing) in ns

            stages = int((target_period // stage_delay) + 1)
            if stages % 2 == 0:
                stages = stages + 1

            ring_cw = Signal(stages+1) # ring oscillator clockwise
            ring_ccw = Signal(stages+1) # ring oscillator counter-clockwise
            x_min = 0
            x_max = 65
            y_min = 0
            y_max = 99  # 149 if you want to deal with the special case notch in the upper right
            x_mid = (x_max - x_min) // 2
            y_mid = (y_max - y_min) // 2
            y_span = y_max - y_min
            x_span = x_max - x_min
            x = 0
            y = 0
            for stage in range(stages):
                stagename = 'RINGOSC_CW' + str(stage)
                platform.toolchain.attr_translate[stagename + 'LOCK'] = ("LOC", "SLICE_X" + str(x) + 'Y' + str(y))
                self.specials += [
                    Instance("LUT1",
                             name=stagename,
                             p_INIT=1,
                             i_I0=ring_cw[stage+1],
                             o_O=ring_cw[stage],
                             attr=("KEEP", "DONT_TOUCH", stagename + 'LOCK')
                             )
                ]

                stagename = 'RINGOSC_CCW' + str(stage)
                # initially, share the CLB -- but see if performance is better if the LUTs are spread farther apart
                platform.toolchain.attr_translate[stagename + 'LOCK'] = ("LOC", "SLICE_" + str(x) + 'Y' + str(y))
                self.specials += [
                    Instance("LUT1",
                             name=stagename,
                             p_INIT=1,
                             i_I0=ring_ccw[stage],
                             o_O=ring_ccw[stage+1],
                             attr=("KEEP", "DONT_TOUCH", stagename + 'LOCK')
                             )
                ]
                # spiral the pattern of LUTs counter-clockwise, starting at the lower left:
                #  (0,ymax)   (xmax, ymax)
                #  (0,0)      (xmax, 0)
                # we stride in on the Y-axis, and once we hit the middle, we stride in on the X-axis
                if x <= x_mid and y <= y_mid: # lower left, go right
                    x = x + x_span
                elif x > x_mid and y <= y_mid: # lower right, go up
                    y = y + y_span
                    if y <= y_mid:  # we hit the middle
                        x = x - 1
                        y = 0
                        y_span = y_max - y_min
                    else:
                        y_span = y_span - 1

                elif x > x_mid and y > y_mid: # upper right, go left
                    x = x - x_span
                else: # upper left, go down to origin + lap
                    y = y - y_span
                    if y > y_mid:  # we hit the middle
                        x = x + 1
                        y = y_max
                        y_span = y_max - y_min
                    else:
                        y_span = y_span - 1


            # close the rings with a power gate
            self.comb += ring_cw[stages].eq(ring_cw[0] & self.ctl.fields.ena)
            self.comb += ring_ccw[0].eq(ring_ccw[stages] & self.ctl.fields.ena)

            # instantitae the noise slicing flip flop explicitly
            self.specials += [
                Instance("FDCE",
                         i_C=ring_cw[0],
                         i_D=ring_ccw[0],
                         i_CE=self.ctl.fields.ena,
                         i_CLR=0,
                         o_Q=self.trng_raw,
                         )
            ]

        elif device_root == 'ice40-up5k':
            stage_delay = 11  # rough delay of each ring oscillator stage (incl routing) in ns
            fast_stages = 1

            stages = int((target_period // stage_delay) + 1)
            if stages % 2 == 0:
                stages = stages + 1

            ring_cw = Signal(stages+1) # ring oscillator clockwise
            ring_ccw = Signal(fast_stages+1) # ring oscillator counter-clockwise (fast)
            x_min = 1
            x_max = 22
            y_min = 1
            y_max = 30
            y_span = y_max - y_min
            x_span = x_max - x_min
            x_mid = (x_max - x_min) // 2
            y_mid = (y_max - y_min) // 2
            x = 1
            y = 1
            for stage in range(stages):
                stagename = 'RINGOSC_CW' + str(stage)
                platform.toolchain.attr_translate[stagename + 'LOCK'] = ("BEL", "X" + str(x) + '/Y' + str(y) + '/lc0')
                self.specials += Instance("SB_LUT4",
                                          p_LUT_INIT=1,
                                          o_O=ring_cw[stage],
                                          i_I0=ring_cw[stage+1],
                                          i_I1=0,
                                          i_I2=0,
                                          i_I3=0,
                                          attr=("KEEP", "DONT_TOUCH", stagename + 'LOCK')
                                          )

                if stage < fast_stages:
                    stagename = 'RINGOSC_CCW' + str(stage)
                    # initially, share the CLB -- but see if performance is better if the LUTs are spread farther apart
                    platform.toolchain.attr_translate[stagename + 'LOCK'] = ("BEL", "X" + str(x) + '/Y' + str(y) + '/lc1')
                    self.specials += Instance("SB_LUT4",
                                              p_LUT_INIT=1,
                                              o_O=ring_ccw[stage+1],
                                              i_I0=ring_ccw[stage],
                                              i_I1=0,
                                              i_I2=0,
                                              i_I3=0,
                                              attr=("KEEP", "DONT_TOUCH", stagename + 'LOCK')
                                              )

                # spiral the pattern of LUTs counter-clockwise, starting at the lower left:
                #  (0,ymax)   (xmax, ymax)
                #  (0,0)      (xmax, 0)
                # we stride in on the Y-axis, and once we hit the middle, we stride in on the X-axis
                if x <= x_mid and y <= y_mid: # lower left, go right
                    x = x + x_span
                elif x > x_mid and y <= y_mid: # lower right, go up
                    y = y + y_span
                    if y <= y_mid:  # we hit the middle
                        x = x - 1
                        y = 0
                        y_span = y_max - y_min
                    else:
                        y_span = y_span - 1

                elif x > x_mid and y > y_mid: # upper right, go left
                    x = x - x_span
                else: # upper left, go down to origin + lap
                    y = y - y_span
                    if y > y_mid:  # we hit the middle
                        x = x + 1
                        y = y_max
                        y_span = y_max - y_min
                    else:
                        y_span = y_span - 1


            # close the rings with a power gate
            self.comb += ring_cw[stages].eq(ring_cw[0] & self.ctl.fields.ena)
            self.comb += ring_ccw[0].eq(ring_ccw[fast_stages] & self.ctl.fields.ena)

            # instantiate the noise slicing flip flop explicitly
            self.specials += [
                Instance("SB_DFFE",
                         i_C=ring_cw[int(stages//2)],
                         i_D=ring_ccw[0], # ccw is fast, ideally, [period of fast osc] < [jitter of slow osc]
                         i_E=self.ctl.fields.ena,
                         o_Q=self.trng_raw,
                         )
            ]

        else:
            print("RingOsc: unsupported part " + device_root)

        self.specials += MultiReg(ring_cw[int(stages//2)], rand_strobe)
        self.specials += MultiReg(self.trng_raw, self.trng_out_sync)

        # make fast osc available for debug
        self.trng_fast = Signal()
        self.comb += self.trng_fast.eq(ring_ccw[0])
        # make slow osc available for debug
        self.trng_slow = Signal()
        self.comb += self.trng_slow.eq(ring_cw[0])
