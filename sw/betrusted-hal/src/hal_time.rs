use core::cmp::Ordering;
use utralib::generated::*;

pub fn time_init() {
    let mut ticktimer_csr = CSR::new(HW_TICKTIMER_BASE as *mut u32);

    ticktimer_csr.wfo(utra::ticktimer::CONTROL_RESET, 1);
}

/// Struct to work with 40-bit ms resolution hardware timestamps.
/// 40-bit overflow would take 34 years of uptime, so no need to worry about it.
/// 32-bit overflow would take 49.7 days of uptime, so need to consider it.
#[derive(Copy, Clone, PartialEq)]
pub struct TimeMs {
    pub time0: u32, // Low 32-bits from hardware timer
    pub time1: u32, // High 8-bits from hardware timer
}
#[derive(Copy, Clone)]
pub enum TimeMsErr {
    Overflow,
    Underflow,
}
impl TimeMs {
    /// Return timestamp for current timer value
    pub fn now() -> Self {
        let ticktimer_csr = CSR::new(HW_TICKTIMER_BASE as *mut u32);
        let now = Self {
            time0: ticktimer_csr.r(utra::ticktimer::TIME0),
            time1: ticktimer_csr.r(utra::ticktimer::TIME1),
        };

        // ============================================================
        // DANGER! This is for testing overflow logic by by forcing a
        // u32 overflow at 20 seconds after boot... leave it turned off
        // ============================================================
        if false {
            let twenty_seconds_before_u32_overflow: u32 = 0xffff_b1df;
            return now.add_ms(twenty_seconds_before_u32_overflow);
        }
        // ============================================================

        // This is the right one to use normally
        return now;
    }

    /// Calculate a timestamp for interval ms after &self.
    /// This can overflow at 34 years of continuous uptime, but we will ignore that.
    pub fn add_ms(&self, interval_ms: u32) -> Self {
        match self.time0.overflowing_add(interval_ms) {
            (t0, false) => Self {
                time0: t0,
                time1: self.time1,
            },
            (t0, true) => Self {
                time0: t0,
                // Enforce 40-bit overflow if crossing the 34 year boundary
                time1: 0x0000_00ff & self.time1.wrapping_add(1),
            },
        }
    }

    /// Calculate a timestamp for interval seconds after &self.
    ///
    /// This is based on a 40-bit ms timer that will overflow at 34 years of continuous
    /// uptime. To simplify time handling code, we will ignore that and just saturate the
    /// timer at END_OF_TIME_MS. That means the timer intervals will clip at the rollover
    /// point in the unlikely event that you manage to achieve 34 years of uptime.
    ///
    pub fn add_s(&self, interval_s: u32) -> Self {
        const MS_TIMER_BITS: u32 = 40;
        const END_OF_TIME_MS: u64 = 2u64.pow(MS_TIMER_BITS) - 1;
        const MS_PER_SECOND: u64 = 1000;
        let time: u64 = match (interval_s as u64).overflowing_mul(MS_PER_SECOND) {
            (t, overflow) if (overflow == false) && (t < END_OF_TIME_MS) => t,
            _ => END_OF_TIME_MS,
        };
        let start_time = ((self.time1 as u64) << 32) | (self.time0 as u64);
        let mut target = start_time.saturating_add(time);
        if target > END_OF_TIME_MS {
            target = END_OF_TIME_MS;
        }
        let high_bits = (target >> 32) as u32;
        let low_bits = (target & 0xffff_ffff) as u32;
        Self {
            time0: low_bits,
            time1: high_bits,
        }
    }

    /// Return the milliseconds elapsed from earlier to self.
    ///
    /// CAUTION: I think this math is right, but maybe I'm wrong? Don't blindly trust this.
    ///
    pub fn sub_u32(&self, earlier: &Self) -> Result<u32, TimeMsErr> {
        if self < earlier {
            return Err(TimeMsErr::Underflow);
        }
        match self.time1.wrapping_sub(earlier.time1) {
            // Subtle math things happening here... in the 1 case, this relies on wrapping
            // being equivalent to borrowing a bit from the high word
            0 | 1 => Ok(self.time0.wrapping_sub(earlier.time0)),
            // If high words differ by more than 1 LSB, time diff is greater than 2^32 ms
            _ => Err(TimeMsErr::Overflow),
        }
    }

    /// Return high word of timestamp
    pub fn ms_high_word(&self) -> u32 {
        self.time1
    }

    /// Return low word of timestamp
    pub fn ms_low_word(&self) -> u32 {
        self.time0
    }
}
impl PartialOrd for TimeMs {
    /// This allows for `if TimeMs::now() >= stop {...}`
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match self.time1.partial_cmp(&other.time1) {
            Some(Ordering::Equal) => self.time0.partial_cmp(&other.time0),
            x => x, // Some(Less) | Some(Greater) | None
        }
    }
}

pub fn delay_ms(ms: u32) {
    // DANGER! DANGER! DANGER!
    //
    // This code is designed with the intent to never, no matter what, panic nor block the
    // main event loop. In the pursuit of that ideal, surprising things may happen. In
    // particular, there is a cap on the maximum delay time that is silently enforced.
    //
    // Logging a warning here about requests for long delays is impractical, because the
    // logging code calls delay_ms(). So, my awkward compromise is to silently limit the
    // requested delay inteval. This is a big dangerous footgun. Consider yourself warned.
    //
    // Not blocking the main event loop means this function is careful to impose upper
    // bounds on how long it can take to return. Those limits are:
    //
    // 1. The delay is capped at a max of 500 ms, which is chosen to be an order of
    //    magnitude larger than reasonable maximum delays of about 10-20ms of per
    //    iteration of the main event loop. Long intervals should be managed with a state
    //    machine to avoid negative effects on network responsiveness.
    //
    // 2. The for loop is limited by a counter to prevent runaway code in the event of an
    //    IO problem with the timer or an error in the delay calculations. The loop
    //    counter estimates 1 clock cycle per iteration because it makes the math easy.
    //    The actual iterations will be slower, but estimating how much slower is
    //    difficult. It doesn't matter. The point is that the counter is large enough not
    //    to truncate the delay and small enough to force the loop to end within seconds
    //    rather than minutes, weeks, or not at all.
    //
    // Loop counter math:
    // 1. Each iteration of the for loop is definitely going to take at least 1 cycle of
    //    the 18MHz CPU clock to finish
    // 2. The hardware timer resolution is 1ms
    // 3. There are 0.001(s/ms) * 18e+6(Hz) = 18000 CPU clock cycles per ms
    // 4. A 500ms delay should finish within 500 * 18000 = 9e+6 clock cycles
    // 5. Maximum value for u32 loop counter is 4e+9, so 9e+6 will fit fine
    //
    const MAX_MS: usize = 500;
    const MAX_LOOP_ITERATIONS: usize = MAX_MS * 18_000;
    let capped_ms = match ms < MAX_MS as u32 {
        true => ms,
        false => MAX_MS as u32,
    };
    let stop_time = TimeMs::now().add_ms(capped_ms);
    for _ in 0..MAX_LOOP_ITERATIONS {
        if TimeMs::now() >= stop_time {
            break;
        }
    }
}

/// Return the low word from the 40-bit hardware millisecond timer.
pub fn get_time_ms() -> u32 {
    let ticktimer_csr = CSR::new(HW_TICKTIMER_BASE as *mut u32);
    ticktimer_csr.r(utra::ticktimer::TIME0)
}

pub fn get_time_ticks() -> u64 {
    let ticktimer_csr = CSR::new(HW_TICKTIMER_BASE as *mut u32);

    let mut time: u64;

    time = ticktimer_csr.r(utra::ticktimer::TIME0) as u64;
    time |= (ticktimer_csr.r(utra::ticktimer::TIME1) as u64) << 32;

    time
}

pub fn set_msleep_target_ticks(delta_ticks: u32) {
    let mut ticktimer_csr = CSR::new(HW_TICKTIMER_BASE as *mut u32);

    let mut time: u64;

    time = ticktimer_csr.r(utra::ticktimer::TIME0) as u64;
    time |= (ticktimer_csr.r(utra::ticktimer::TIME1) as u64) << 32;

    time += delta_ticks as u64;

    ticktimer_csr.wo(
        utra::ticktimer::MSLEEP_TARGET1,
        ((time >> 32) & 0xFFFF_FFFF) as u32,
    );
    ticktimer_csr.wo(
        utra::ticktimer::MSLEEP_TARGET0,
        (time & 0xFFFF_FFFFF) as u32,
    );
}

/// callers must deal with overflow, but the function is fast
pub fn get_time_ticks_trunc() -> u32 {
    let ticktimer_csr = CSR::new(HW_TICKTIMER_BASE as *mut u32);

    ticktimer_csr.r(utra::ticktimer::TIME0)
}

pub fn delay_ticks(ticks: u32) {
    let ticktimer_csr = CSR::new(HW_TICKTIMER_BASE as *mut u32);

    let start: u32 = ticktimer_csr.r(utra::ticktimer::TIME0);

    loop {
        let cur: u32 = ticktimer_csr.r(utra::ticktimer::TIME0);
        if cur > start {
            if (cur - start) > ticks {
                break;
            }
        } else {
            // handle overflow
            if (cur + (0xffff_ffff - start)) > ticks {
                break;
            }
        }
    }
}
