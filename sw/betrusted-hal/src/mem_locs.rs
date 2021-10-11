
// locate firmware at SPI top minus 400kiB. Datasheet says reserve at least 350kiB for firmwares.
pub const WFX_FIRMWARE_OFFSET: usize = 0x2000_0000 + 1024 * 1024 - 400 * 1024; // 0x2009_C000

//pub const WFX_FIRMWARE_SIZE: usize = 290896; // version C0, as burned to ROM v3.3.2
pub const WFX_FIRMWARE_SIZE: usize = 305232; // version C0, as burned to ROM v3.12.1. Also applicable for v3.12.3.

// RAM alloc areas:
// 0x1000_0000: base of RAM
// 0x1001_3000: top of code + data region (76k)
//            : stack grows down (20k)
// 0x1001_8000: base of wfx alloc area
//       +2000: space for 4 x 2k buffers
// 0x1001_A000: base of packet buffer
//       +6000: space for 12 x 2k inbound packet buffers
// 0x1002_0000: first out of bounds address

// make a very shitty, temporary malloc that can hold up to 16 entries in the 32k space
// this is all to avoid including the "alloc" crate, which is "nightly" and not "stable"
// reserve top 32kiB for WFX FFI RAM buffers
pub const WFX_RAM_LENGTH: usize = 0x2000;
pub const WFX_RAM_OFFSET: usize = 0x1001_8000;

pub const STACK_END:   usize = 0x1001_3000; // stack grows down
pub const STACK_START: usize = 0x1001_8000;
pub const STACK_LEN: usize = STACK_START - STACK_END;
pub const STACK_CANARY: u32 = 0xACE0BACE;

pub const PKT_BUF_BASE: usize = 0x1001_A000;
pub const PKT_BUF_LEN: usize = 0x6000;
