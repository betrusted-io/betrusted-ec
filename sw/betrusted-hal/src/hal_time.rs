const TICKS_PER_MS: u64 = 1;

pub fn time_init(p: &betrusted_pac::Peripherals) {
    p.TICKTIMER.control.write( |w| {w.reset().bit(true)});
}

// time APIs needed (ideally)
// get current time - in milliseconds, as u32
// delay for milliseconds
pub fn get_time_ms(p: &betrusted_pac::Peripherals) -> u32 {
    let mut time: u64;
    
    time = p.TICKTIMER.time0.read().bits() as u64;
    time |= (p.TICKTIMER.time1.read().bits() as u64) << 32;

    (time / TICKS_PER_MS) as u32
}

pub fn delay_ms(p: &betrusted_pac::Peripherals, ms: u32) {
    let starttime: u32 = get_time_ms(p);

    loop {
        if get_time_ms(p) > (starttime + ms) {
            break;
        }
    }
}

/// callers must deal with overflow, but the function is fast
pub fn get_time_ticks_trunc(p: &betrusted_pac::Peripherals) -> u32 {
    p.TICKTIMER.time0.read().bits()
}

pub fn delay_ticks(p: &betrusted_pac::Peripherals, ticks: u32) {
    let start: u32 = p.TICKTIMER.time0.read().bits();

    loop {
        let cur: u32 = p.TICKTIMER.time0.read().bits();
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
