pub mod hal_time {
    const TICKS_PER_MS: u32 = 1;

    pub fn time_init(peripherals: &betrusted_pac::Peripherals) {
        peripherals.TICKTIMER.control.write( |w| {w.reset().bit(true)});
    }

    // time APIs needed (ideally)
    // get current time - in milliseconds, as u32
    // delay for milliseconds
    pub fn get_time_ms(peripherals: &betrusted_pac::Peripherals) -> u32 {
        let mut time: u32;
        
        time = peripherals.TICKTIMER.time3.read().bits();
        time = (time << 8) | peripherals.TICKTIMER.time2.read().bits();
        time = (time << 8) | peripherals.TICKTIMER.time1.read().bits();
        time = (time << 8) | peripherals.TICKTIMER.time0.read().bits();

        time / TICKS_PER_MS
    }

    pub fn delay_ms(peripherals: &betrusted_pac::Peripherals, ms: u32) {
        let starttime: u32 = get_time_ms(peripherals);

        loop {
        if get_time_ms(peripherals) > (starttime + ms * TICKS_PER_MS) {
            break;
        }
        }
    }


}