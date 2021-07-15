//! This file is for dead code that was hanging around in main.rs. I'm putting
//! it here so it's out of the way. To see how it used to be, look for commits
//! from before July 2021.
use utralib::generated::HW_SPIFLASH_MEM;

#[allow(dead_code)] // used for debugging
pub fn dump_rom_addr(addr: u32) {
    let rom_ptr: *mut u32 = (addr + HW_SPIFLASH_MEM as u32) as *mut u32;
    let rom = rom_ptr as *mut Volatile<u32>;
    for i in 0..64 {
        if i % 8 == 0 {
            sprint!("\n\r0x{:06x}: ", addr + i * 4);
        }
        let data: u32 = unsafe { (*rom.add(i as usize)).read() };
        sprint!(
            "{:02x} {:02x} {:02x} {:02x} ",
            data & 0xFF,
            (data >> 8) & 0xff,
            (data >> 16) & 0xff,
            (data >> 24) & 0xff
        );
    }
    sprintln!("");
}

pub fn cut_from_main() {
    /*
    // check that the gas gauge capacity is correct; if not, reset it
    if gg_set_design_capacity(&mut i2c, None) != 1100 {
        gg_set_design_capacity(&mut i2c, Some(1100));
    } */
    // seems to work better with the default 1340mAh capacity even though that's not our actual capacity

    /*  // kept around as a quick test routine for SPI flashing
        let mut idcode: [u8; 3] = [0; 3];
        spi_cmd(CMD_RDID, None, Some(&mut idcode));
        sprintln!("SPI ID code: {:02x} {:02x} {:02x}", idcode[0], idcode[1], idcode[2]);
        let test_addr = 0x8_0000;
        dump_rom_addr(test_addr);
        spi_erase_region(test_addr, 4096);

        dump_rom_addr(test_addr);

        let mut test_data: [u8; 256] = [0; 256];
        for i in 0..256 {
            test_data[i] = (255 - i) as u8;
        }
        spi_program_page(test_addr, &mut test_data);

        dump_rom_addr(test_addr);
    */
}
