const TICKS_PER_MS: u64 = 100;

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

pub fn delay_us(p: &betrusted_pac::Peripherals, us: u64) {
    let starttime: u64 = (p.TICKTIMER.time0.read().bits() as u64) | ((p.TICKTIMER.time1.read().bits() as u64) << 32);
    let tick_increment: u64 = us / 10; // each tick increment is 10us with TICKS_PER_MS = 100

    while ((p.TICKTIMER.time0.read().bits() as u64) | ((p.TICKTIMER.time1.read().bits() as u64) << 32)) > (starttime + tick_increment) {

    }
}
