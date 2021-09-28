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
struct TimeMs {
    time0: u32, // Low 32-bits from hardware timer
    time1: u32, // High 8-bits from hardware timer
}
impl TimeMs {
    /// Return timestamp for current timer value
    pub fn now() -> Self {
        let ticktimer_csr = CSR::new(HW_TICKTIMER_BASE as *mut u32);
        Self {
            time0: ticktimer_csr.r(utra::ticktimer::TIME0),
            time1: ticktimer_csr.r(utra::ticktimer::TIME1),
        }
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
    // main event loop. In the pursuit of that ideal, surprising things may happen.
    //
    // The delay time is capped at 238 ms, which is (2^32)/18MHz. There might be good
    // reasons for calling code to want longer delays. But, code which blocks the event
    // loop for more than perhaps 10-20ms will mess up network latency. The right solution
    // is to use a state machine to track long intervals.
    //
    // Logging a warning here about requests for long delays is impractical, because the
    // logging code calls delay_ms(). So, my awkward compromise is to silently limit the
    // requested delay inteval. This is a big dangerous footgun. Consider yourself warned.
    //
    const MAX_MS: usize = 238;
    const CLOCK_HZ: usize = 18_000_000;
    const MAX_LOOP_ITERATIONS: usize = MAX_MS * CLOCK_HZ;
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
