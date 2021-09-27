#![allow(dead_code)] // so we don't get tons of warnings on unused but defined SPI commands

use debug::{sprintln};
use utralib::generated::*;

pub struct SpiCmd {
    command: u8,
    is_quad: bool,
    is_read: bool,
    use_addr: bool,
    has_data: bool,
    dummy: Option<u8>,
    return_count: Option<u8>, // if a fixed number of read bytes are stipulated by command
}

pub const CMD_RDID:   SpiCmd = SpiCmd {command: 0x9f, is_quad: false, is_read: true,  use_addr:false,  has_data:false, dummy:None,  return_count:Some(3)};
pub const CMD_WRSR:   SpiCmd = SpiCmd {command: 0x01, is_quad: false, is_read: false, use_addr:true,   has_data:false, dummy:None,  return_count:None}; // abuses "addr" as SR data
pub const CMD_NOP:    SpiCmd = SpiCmd {command: 0x00, is_quad: false, is_read: false, use_addr:false,  has_data:false, dummy:None,  return_count:None};
pub const CMD_WREN:   SpiCmd = SpiCmd {command: 0x06, is_quad: false, is_read: false, use_addr:false,  has_data:false, dummy:None,  return_count:None};
pub const CMD_RDSR:   SpiCmd = SpiCmd {command: 0x05, is_quad: false, is_read: true,  use_addr:false,  has_data:false, dummy:None,  return_count:Some(1)};
pub const CMD_RDSCUR: SpiCmd = SpiCmd {command: 0x2B, is_quad: false, is_read: true,  use_addr:false,  has_data:false, dummy:None,  return_count:Some(1)};
pub const CMD_RDCR:   SpiCmd = SpiCmd {command: 0x15, is_quad: false, is_read: true,  use_addr:false,  has_data:false, dummy:None,  return_count:Some(2)};
pub const CMD_SE:     SpiCmd = SpiCmd {command: 0x20, is_quad: false, is_read: false, use_addr:true,   has_data:false, dummy:None,  return_count:None};
pub const CMD_BE32K:  SpiCmd = SpiCmd {command: 0x52, is_quad: false, is_read: false, use_addr:true,   has_data:false, dummy:None,  return_count:None};
pub const CMD_BE64K:  SpiCmd = SpiCmd {command: 0xD8, is_quad: false, is_read: false, use_addr:true,   has_data:false, dummy:None,  return_count:None};
pub const CMD_4PP:    SpiCmd = SpiCmd {command: 0x38, is_quad: true,  is_read: false, use_addr:true,   has_data:true,  dummy:None,  return_count:None};
pub const CMD_4READ:  SpiCmd = SpiCmd {command: 0xEB, is_quad: true,  is_read: true,  use_addr:true,   has_data:true,  dummy:Some(6), return_count:None};
pub const CMD_WRDI:   SpiCmd = SpiCmd {command: 0x04, is_quad: false, is_read: false, use_addr:false,  has_data:false, dummy:None,  return_count:None};

pub const SPI_SR_WEL_MASK: u8 = 0x2;
pub const SPI_SR_WIP_MASK: u8 = 0x1;
pub const SPI_RDSCUR_E_FAIL_MASK: u8 = 0x40;
pub const SPI_RDSCUR_P_FAIL_MASK: u8 = 0x20;

const OE_MASK_1BIT: u32 = 0x1;
const OE_MASK_4BIT: u32 = 0xF;

fn spi_1bit_write(byte: u8) {
    let mut spicsr = CSR::new(HW_PICORVSPI_BASE as *mut u32);
    let spi_ms = CSR::new(HW_PICORVSPI_BASE as *mut u32);
    let mut sr = byte;
    for _ in 0..8 {
        if sr & 0x80 != 0 {
            spicsr.wo(utra::picorvspi::WDATA,
                spi_ms.ms(utra::picorvspi::WDATA_OE, OE_MASK_1BIT)
                    | spi_ms.ms(utra::picorvspi::WDATA_DATA, 1)
            );
        } else {
            spicsr.wo(utra::picorvspi::WDATA,
                spi_ms.ms(utra::picorvspi::WDATA_OE, OE_MASK_1BIT)
                    | spi_ms.ms(utra::picorvspi::WDATA_DATA, 0)
            );
        }
        sr <<= 1;
    }
}

#[inline]
fn spi_quad_write(byte: u8) {
    let mut spicsr = CSR::new(HW_PICORVSPI_BASE as *mut u32);
    let spi_ms = CSR::new(HW_PICORVSPI_BASE as *mut u32);
    spicsr.wo(utra::picorvspi::WDATA,
        spi_ms.ms(utra::picorvspi::WDATA_OE, OE_MASK_4BIT)
            | spi_ms.ms(utra::picorvspi::WDATA_DATA, (byte as u32 >> 4) & 0xF)
    );
    spicsr.wo(utra::picorvspi::WDATA,
        spi_ms.ms(utra::picorvspi::WDATA_OE, OE_MASK_4BIT)
            | spi_ms.ms(utra::picorvspi::WDATA_DATA, byte as u32 & 0xF)
    );
}

fn spi_1bit_read() -> u8 {
    let mut spicsr = CSR::new(HW_PICORVSPI_BASE as *mut u32);
    let mut byte: u8 = 0;
    // data is already present entering the first iteration
    for _ in 0..8 {
        byte <<= 1;
        byte |= ((spicsr.rf(utra::picorvspi::RDATA_DATA) >> 1) & 0x1) as u8; // CIPO is bit rdata[1]
        spicsr.wfo(utra::picorvspi::WDATA_OE, 0x0);
    }

    byte
}

fn spi_quad_read() -> u8 {
    let mut spicsr = CSR::new(HW_PICORVSPI_BASE as *mut u32);
    let mut byte: u8 = (spicsr.rf(utra::picorvspi::RDATA_DATA) << 4) as u8;
    // data is already present entering the first iteration
    spicsr.wfo(utra::picorvspi::WDATA_OE, 0x0);
    byte |= spicsr.rf(utra::picorvspi::RDATA_DATA) as u8;
    spicsr.wfo(utra::picorvspi::WDATA_OE, 0x0);

    byte
}

pub fn spi_standby() {
    let mut spicsr = CSR::new(HW_PICORVSPI_BASE as *mut u32);
    // ensure drivers are off to save power
    spicsr.wfo(utra::picorvspi::WDATA_OE, 0);
}

/// called before any exit, successful or failed
fn exit_bb() {
    let mut spicsr = CSR::new(HW_PICORVSPI_BASE as *mut u32);
    let spi_ms = CSR::new(HW_PICORVSPI_BASE as *mut u32);
    // raise CS
    spicsr.wo(utra::picorvspi::MODE,
        spi_ms.ms(utra::picorvspi::MODE_BITBANG, 1)
            | spi_ms.ms(utra::picorvspi::MODE_CSN, 1)
    );
    // ensure drivers are off to save power
    spicsr.wfo(utra::picorvspi::WDATA_OE, 0);

    // exit bitbang mode
    spicsr.wo(utra::picorvspi::MODE,
        spi_ms.ms(utra::picorvspi::MODE_BITBANG, 0)
            | spi_ms.ms(utra::picorvspi::MODE_CSN, 1)
    );
}

pub fn spi_cmd(cmd: SpiCmd, address: Option<u32>, data: Option<&mut [u8]>) -> bool {
    let mut spicsr = CSR::new(HW_PICORVSPI_BASE as *mut u32);
    let spi_ms = CSR::new(HW_PICORVSPI_BASE as *mut u32);

    // turn on bitbang mode, pre-set CS so it doesn't glitch going into bitbang mode
    spicsr.wo(utra::picorvspi::MODE,
        spi_ms.ms(utra::picorvspi::MODE_BITBANG, 0)
            | spi_ms.ms(utra::picorvspi::MODE_CSN, 1)
    );
    // turn on bitbang mode
    spicsr.wo(utra::picorvspi::MODE,
        spi_ms.ms(utra::picorvspi::MODE_BITBANG, 1)
            | spi_ms.ms(utra::picorvspi::MODE_CSN, 1)
    );


    // turn on bitbang mode, lower CS
    spicsr.wo(utra::picorvspi::MODE,
        spi_ms.ms(utra::picorvspi::MODE_BITBANG, 1)
            | spi_ms.ms(utra::picorvspi::MODE_CSN, 0)
    );

    // shift out the command
    spi_1bit_write(cmd.command);

    if !cmd.is_quad {
        // single-bit path
        if cmd.use_addr {
            if address.is_some() {
                let addr = address.unwrap();
                spi_1bit_write((addr >> 16) as u8);
                spi_1bit_write((addr >> 8) as u8);
                spi_1bit_write((addr >> 0) as u8);
            } else {
                // we expected an address, but none was given: this is a user error
                exit_bb();
                return false
            }
        }

        if cmd.is_read {
            if cmd.dummy.is_some() {
                let numdum = cmd.dummy.unwrap();
                for _ in 0..numdum {
                    spi_1bit_write(0x00);
                }
            }
            if cmd.return_count.is_some() {
                if data.is_some() {
                    let count = cmd.return_count.unwrap();
                    let data_checked: &mut [u8] = data.unwrap();
                    if data_checked.len() < count as usize {
                        // not enough data to return the read result
                        exit_bb();
                        return false
                    }
                    for i in 0..count {
                        data_checked[i as usize] = spi_1bit_read();
                    }
                } else {
                    // we expected a return data vector, but wasn't given one
                    exit_bb();
                    return false
                }
            }
        } else {
            if cmd.has_data {
                // TODO: implement 1-bit PP opcode. Currently not defined
                exit_bb();
                return false
            } else {
                // nothing to do here, move along
            }
        }
    } else {
        // quad path
        if cmd.use_addr {
            if address.is_some() {
                let addr = address.unwrap();
                spi_quad_write((addr >> 16) as u8);
                spi_quad_write((addr >> 8) as u8);
                spi_quad_write((addr >> 0) as u8);
            } else {
                // we expected an address, but none was given: this is a user error
                exit_bb();
                return false
            }
        }
        if cmd.is_read {
            if cmd.dummy.is_some() {
                let numdum = cmd.dummy.unwrap();
                for _ in 0..numdum {
                    spi_quad_write(0x00);
                }
            }
            // there are no 1-bit quad return counts, so we only check for return data
            if data.is_none() {
                exit_bb();
                return false
            } else {
                let data_checked = data.unwrap();
                for i in 0..data_checked.len() {
                    data_checked[i] = spi_quad_read();
                }
            }
        } else {
            // write command -- they never have dummy bytes, so don't even bother checking it
            if data.is_none() {
                // you asked me to write, but gave me no data!
                exit_bb();
                return false
            } else {
                let data_checked = data.unwrap();
                for i in 0..data_checked.len() {
                    spi_quad_write(data_checked[i]);
                }
            }
        }
    }

    exit_bb();
    true
}

pub fn spi_erase_region(addr: u32, len: u32) {
    let mut sr: [u8; 1] = [0; 1];

    let mut erased: u32 = 0;
    while erased < len {
        loop {
            spi_cmd(CMD_WREN, None, None);
            spi_cmd(CMD_RDSR, None, Some(&mut sr));
            sprintln!("SR: {:02x}", sr[0]);
            if sr[0] & SPI_SR_WEL_MASK != 0 {
                break;
            }
        }
        if (len - erased >= 0x1_0000) && (((addr + erased) & 0xFFFF) == 0) {
            spi_cmd(CMD_BE64K, Some(addr + erased), None);
            erased += 65536;
        } else if (len - erased >= 0x8000) && (((addr + erased) & 0x7FFF) == 0) {
            spi_cmd(CMD_BE32K, Some(addr + erased), None);
            erased += 32768;
        } else {
            spi_cmd(CMD_SE, Some(addr + erased), None);
            erased += 4096;
        }
        loop {
            spi_cmd(CMD_RDSR, None, Some(&mut sr));
            sprintln!("erase wait: {:02x}", sr[0]);
            if sr[0] & SPI_SR_WIP_MASK == 0 {
                break;
            }
        }
        spi_cmd(CMD_RDSCUR, None, Some(&mut sr));
        if sr[0] & (SPI_RDSCUR_E_FAIL_MASK | SPI_RDSCUR_P_FAIL_MASK) != 0 {
            sprintln!("erase fail!");
        } else {
            sprintln!("erase success!");
        }
        spi_cmd(CMD_WRDI, None, None);
    }
}

pub fn spi_program_page(addr: u32, data: &mut [u8]) {
    let mut sr: [u8; 1] = [0; 1];
    let fast_and_furious = false;

    if fast_and_furious {
        // skip most the checks, in favor of speed.
        spi_cmd(CMD_WREN, None, None);
        spi_cmd(CMD_4PP, Some(addr), Some(data));
        loop {
            spi_cmd(CMD_RDSR, None, Some(&mut sr));
            if sr[0] & SPI_SR_WIP_MASK == 0 {
                break;
            }
        }
    } else {
        loop {
            spi_cmd(CMD_WREN, None, None);
            spi_cmd(CMD_RDSR, None, Some(&mut sr));
            //sprintln!("SR: {:02x}", sr[0]);
            if sr[0] & SPI_SR_WEL_MASK != 0 {
                break;
            }
        }
        spi_cmd(CMD_4PP, Some(addr), Some(data));
        loop {
            spi_cmd(CMD_RDSR, None, Some(&mut sr));
            //sprintln!("program wait: {:02x}", sr[0]);
            if sr[0] & SPI_SR_WIP_MASK == 0 {
                break;
            }
        }
        spi_cmd(CMD_RDSCUR, None, Some(&mut sr));
        if sr[0] & (SPI_RDSCUR_E_FAIL_MASK | SPI_RDSCUR_P_FAIL_MASK) != 0 {
            sprintln!("program fail!");
        } else {
            sprintln!("program success!");
        }
        spi_cmd(CMD_WRDI, None, None);
    }
}
