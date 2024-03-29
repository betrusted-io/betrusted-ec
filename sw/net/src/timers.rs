use betrusted_hal::hal_time::{TimeMs, TimeMsErr};
use debug::{log, loghex, loghexln, logln, LL};

// This is used by logging macros
const LOG_LEVEL: LL = LL::Debug;

/// Countdown tracks a one-shot countdown timer.
#[derive(Copy, Clone)]
pub struct Countdown {
    done_time: Option<TimeMs>,
}
#[derive(Copy, Clone, PartialEq)]
pub enum CountdownStatus {
    NotStarted,
    NotDone,
    Done,
}
impl Countdown {
    /// Initialize a new countdown timer in halted state
    pub const fn new() -> Self {
        Self { done_time: None }
    }

    /// Start countdown timer with interval in ms
    pub fn start(&mut self, interval_ms: u32) {
        self.done_time = Some(TimeMs::now().add_ms(interval_ms));
    }

    /// Start countdown timer with interval in seconds (backed by 40-bit hardware ms timer)
    pub fn start_s(&mut self, interval_s: u32) {
        self.done_time = Some(TimeMs::now().add_s(interval_s));
    }

    /// Clear the timer
    pub fn clear(&mut self) {
        self.done_time = None;
    }

    /// Return countdown timer status
    pub fn status(&self) -> CountdownStatus {
        match self.done_time {
            Some(done_time) => match TimeMs::now() >= done_time {
                true => CountdownStatus::Done,
                _ => CountdownStatus::NotDone,
            },
            None => CountdownStatus::NotStarted,
        }
    }

    /// Debug log the timer's internal state
    pub fn debug_log(&self, tag: &str) {
        log!(LL::Debug, "{} ", tag);
        match self.status() {
            CountdownStatus::Done => log!(LL::Debug, "Done "),
            CountdownStatus::NotDone => log!(LL::Debug, "NotDone "),
            CountdownStatus::NotStarted => log!(LL::Debug, "NotStarted "),
        };
        match self.done_time {
            Some(dt) => {
                let now = TimeMs::now();
                loghex!(LL::Debug, " now ", now.time0);
                loghex!(LL::Debug, " ", now.time1);
                loghex!(LL::Debug, " exp ", dt.time0);
                loghexln!(LL::Debug, " ", dt.time1);
            }
            _ => logln!(LL::Debug, "--"),
        };
    }
}

/// Stopwatch tracks elapsed time relative to a starting timestamp.
#[derive(Copy, Clone)]
pub struct Stopwatch {
    start_time: Option<TimeMs>,
}
#[derive(Copy, Clone)]
pub enum StopwatchErr {
    Overflow,
    Underflow,
    NotStarted,
}
impl Stopwatch {
    /// Initialize a new stopwatch timer in halted state
    pub const fn new() -> Self {
        Self { start_time: None }
    }

    /// Start the timer by recording a reference timestamp.
    /// You can reset the timer by calling this again to start over at 0s.
    pub fn start(&mut self) {
        self.start_time = Some(TimeMs::now());
    }

    /// Reset timer to its newly initialized state (halted)
    pub fn reset(&mut self) {
        self.start_time = None;
    }

    /// Return elapsed ms since stopwatch was started
    pub fn elapsed_ms(&self) -> Result<u32, StopwatchErr> {
        match self.start_time {
            Some(t) => match TimeMs::now().sub_u32(&t) {
                Ok(ms) => Ok(ms),
                Err(TimeMsErr::Underflow) => Err(StopwatchErr::Underflow),
                Err(TimeMsErr::Overflow) => Err(StopwatchErr::Overflow),
            },
            None => Err(StopwatchErr::NotStarted),
        }
    }

    /// Return elapsed seconds since stopwatch was started
    pub fn elapsed_s(&self) -> Result<u32, StopwatchErr> {
        let ms = self.elapsed_ms()?;
        // Adding 500ms before the integer division is meant to act like a floating point
        // round(). Without the +500, the integer division would act like a floor().
        let seconds = ms.saturating_add(500) / 1000;
        Ok(seconds)
    }
}

/// RetryTimer helps track retry sequences with scheduler timestamps.
///
/// RFC 2131 says DHCP clients should use randomized (+/-1s) exponential backoff for
/// retries. The RFC says the timing should be chosen based on the type of network, then
/// goes on to give a bunch of "SHOULD" stuff for 10Mb/s Ethernet. But, not sure where
/// that leaves us for wifi PHY. For now, this uses a base of 2 seconds with the last
/// attempt at 16 seconds. Two seconds comes from measuring timing of some other devices I
/// use for network testing, which happens to largely agree with the RFC. The 16 second
/// upper limit is shorter than 64 from the RFC. I picked 16 to fit within my target
/// window of 30 seconds for factory pass/fail test of wifi connectivity.
///
/// For a good network with non-congested RF conditions, DHCP binding handshakes seem to
/// finish in a few seconds with one, or maybe two, retries. Not sure if it makes sense to
/// extend the exponential backoff to try harder under marginal network and RF conditions?
/// If DHCP handshake won't work, odds of other stuff working are not great. Perhaps it
/// would be better to just fail quickly and report an error so people can relocate, reset
/// the router, or whatever, rather than leading them on to keep waiting for a connection
/// in marginal conditions?
///
#[derive(Copy, Clone)]
pub struct RetryTimer {
    retry: Retry,
    time: Option<TimeMs>,
}
#[derive(Copy, Clone, PartialEq)]
enum Retry {
    R2s,
    R4s,
    R8s,
    R16s,
    RRenew,
    Halted,
}
#[derive(Copy, Clone, PartialEq)]
pub enum RetryStatus {
    Halted,
    TimerRunning,
    TimerExpired,
}
impl RetryTimer {
    /// Return a new halted timer
    pub const fn new_halted() -> Self {
        Self {
            retry: Retry::Halted,
            time: None,
        }
    }

    /// Return timer with randomized 0..2048ms offset added to the specified delay.
    fn new_random(retry: Retry, ms: u32, entropy: u32) -> Self {
        let offset = match retry {
            Retry::R2s => entropy & (2048 - 1),
            Retry::R4s => (entropy >> 7) & (2048 - 1),
            Retry::R8s => (entropy >> 14) & (2048 - 1),
            Retry::R16s => (entropy >> 21) & (2048 - 1),
            Retry::RRenew => entropy & (2048 - 1),
            Retry::Halted => 0,
        };
        Self {
            retry: retry,
            time: match retry {
                Retry::Halted => None,
                _ => Some(TimeMs::now().add_ms(ms + offset)),
            },
        }
    }

    /// Schedule and return the first randomized retry
    pub fn new_first_random(entropy: u32) -> Self {
        Self::new_random(Retry::R2s, 1000, entropy)
    }

    /// Schedule and return the first >60s retry for renew, per RFC 2131
    pub fn new_first_random_renew(entropy: u32) -> Self {
        Self::new_random(Retry::RRenew, 60000, entropy)
    }

    /// Schedule the next randomized retry timer, following the retry sequence
    pub fn schedule_next(&mut self, entropy: u32) {
        let new_retry = match self.retry {
            Retry::R2s => Self::new_random(Retry::R4s, 3000, entropy),
            Retry::R4s => Self::new_random(Retry::R8s, 7000, entropy),
            Retry::R8s => Self::new_random(Retry::R16s, 15000, entropy),
            _ => Self {
                retry: Retry::Halted,
                time: None,
            },
        };
        *self = new_retry;
    }

    /// Schedule the next DHCP Renewing retry timer for >60 seconds, per RFC 2131
    pub fn schedule_next_renew(&mut self, entropy: u32) {
        *self = Self::new_random(Retry::RRenew, 60000, entropy);
    }

    /// Return timer status
    pub fn status(&self) -> RetryStatus {
        match self.retry {
            Retry::Halted => RetryStatus::Halted,
            _ => {
                if let Some(time) = self.time {
                    match TimeMs::now() > time {
                        true => RetryStatus::TimerExpired,
                        false => RetryStatus::TimerRunning,
                    }
                } else {
                    // This is bad... something got out of sync between .retry and .time
                    RetryStatus::Halted
                }
            }
        }
    }
}
