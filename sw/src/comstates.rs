/// COM link states. These constants encode the commands sent from the SoC to the EC.

#[non_exhaustive]
pub struct ComState;

impl ComState {
    // direct coding states
    pub const SSID_CHECK: u16   = 0x2000;
    pub const SSID_FETCH: u16   = 0x2100;

    pub const FLASH_WAITACK:u16 = 0x3000;
    pub const FLASH_ACK: u16    = 0x3CC3;
    pub const FLASH_ERASE: u16  = 0x3200;
    pub const FLASH_PP: u16     = 0x3300;
    pub const FLASH_LOCK: u16   = 0x3400; // lock activity for updates
    pub const FLASH_UNLOCK: u16 = 0x3434; // unlock activity for updates

    pub const LOOP_TEST: u16    = 0x4000;

    pub const CHG_START: u16    = 0x5A00;
    pub const CHG_BOOST_ON: u16 = 0x5ABB;
    pub const CHG_BOOST_OFF: u16= 0x5AFE;

    pub const BL_START: u16     = 0x6800; // back light range encoded in state arg
    pub const BL_END: u16       = 0x6BFF;

    pub const GAS_GAUGE: u16    = 0x7000;

    pub const STAT: u16         = 0x8000;

    pub const POWER_OFF: u16    = 0x9000;
    pub const READ_CHARGE_STATE: u16 = 0x9100;
    pub const POWER_SHIPMODE:u16= 0x9200;

    pub const GYRO_UPDATE: u16  = 0xA000;
    pub const GYRO_READ: u16    = 0xA100;

    pub const POLL_USB_CC: u16  = 0xB000;

    pub const LINK_READ: u16    = 0xF0F0;
    pub const LINK_SYNC: u16    = 0xFFFF;

    // response codes (not used as commands)
//    pub const IDLE: u16         = 0x07E4; // 2020 decimal
    pub const ERROR: u16        = 0xDEAD;
//    pub const CMD_ACK: u16      = 0xCAFE;
}
