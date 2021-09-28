use utralib::generated::*;

pub fn time_init() {
    let mut ticktimer_csr = CSR::new(HW_TICKTIMER_BASE as *mut u32);

    ticktimer_csr.wfo(utra::ticktimer::CONTROL_RESET, 1);
}

/// Return the low word from the 64-bit hardware millisecond timer.
///
/// Note: High word is available in ticktimer_csr.r(utra::ticktimer::TIME1)
/// TODO: Consider how to deal with rollover at 49.7 days from power up
///
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

    ticktimer_csr.wo(utra::ticktimer::MSLEEP_TARGET1, ((time >> 32) & 0xFFFF_FFFF) as u32);
    ticktimer_csr.wo(utra::ticktimer::MSLEEP_TARGET0, (time & 0xFFFF_FFFFF) as u32);
}

pub fn delay_ms(ms: u32) {
    let starttime: u32 = get_time_ms();

    loop {
        if get_time_ms() > (starttime + ms) {
            break;
        }
    }
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
        } else { // handle overflow
            if (cur + (0xffff_ffff - start)) > ticks {
                break;
            }
        }
    }
}
