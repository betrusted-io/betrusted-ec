/// COM link states. These constants encode the commands sent from the SoC to the EC.

pub struct ComSpec {
    /// the "verb" specifying the command
    pub verb: u16,
    /// number of payload words expected -- not counting the verb or dummies for read
    pub w_words: u16,
    /// number of words to be returned to host; host must generate dummy exchanges equal to this to empty the FIFO
    pub r_words: u16,
    /// specifies if this "verb" is a response code, or a verb
    pub response: bool,
}

#[non_exhaustive]
pub struct ComState;

impl ComState {
    pub const SSID_CHECK: ComSpec            = ComSpec{verb: 0x2000, w_words: 0,     r_words: 1     ,response: false};
    pub const SSID_FETCH: ComSpec            = ComSpec{verb: 0x2100, w_words: 0,     r_words: 16*6  ,response: false};

    pub const FLASH_WAITACK: ComSpec         = ComSpec{verb: 0x3000, w_words: 0,     r_words: 1     ,response: false};
    pub const FLASH_ACK: ComSpec             = ComSpec{verb: 0x3CC3, w_words: 0,     r_words: 0     ,response: true};
    pub const FLASH_ERASE: ComSpec           = ComSpec{verb: 0x3200, w_words: 4,     r_words: 0     ,response: false};
    pub const FLASH_PP: ComSpec              = ComSpec{verb: 0x3300, w_words: 130,   r_words: 0     ,response: false};
    pub const FLASH_LOCK: ComSpec            = ComSpec{verb: 0x3400, w_words: 0,     r_words: 0     ,response: false}; // lock activity for updates
    pub const FLASH_UNLOCK: ComSpec          = ComSpec{verb: 0x3434, w_words: 0,     r_words: 0     ,response: false}; // unlock activity for updates

    pub const LOOP_TEST: ComSpec             = ComSpec{verb: 0x4000, w_words: 0,     r_words: 1     ,response: false};

    pub const CHG_START: ComSpec             = ComSpec{verb: 0x5A00, w_words: 0,     r_words: 0     ,response: false};
    pub const CHG_BOOST_ON: ComSpec          = ComSpec{verb: 0x5ABB, w_words: 0,     r_words: 0     ,response: false};
    pub const CHG_BOOST_OFF: ComSpec         = ComSpec{verb: 0x5AFE, w_words: 0,     r_words: 0     ,response: false};

    // this is an odd bird: back light is set by directly using the lower 10 bits to code the backlight level
    pub const BL_START: ComSpec              = ComSpec{verb: 0x6800, w_words: 0,     r_words: 0     ,response: false};
    pub const BL_END: ComSpec                = ComSpec{verb: 0x6BFF, w_words: 0,     r_words: 0     ,response: false};

    pub const GAS_GAUGE: ComSpec             = ComSpec{verb: 0x7000, w_words: 0,     r_words: 4     ,response: false};

    pub const STAT: ComSpec                  = ComSpec{verb: 0x8000, w_words: 0,     r_words: 16    ,response: false};

    pub const POWER_OFF: ComSpec             = ComSpec{verb: 0x9000, w_words: 0,     r_words: 1     ,response: false};
    pub const READ_CHARGE_STATE: ComSpec     = ComSpec{verb: 0x9100, w_words: 0,     r_words: 1     ,response: false};
    pub const POWER_SHIPMODE: ComSpec        = ComSpec{verb: 0x9200, w_words: 0,     r_words: 0     ,response: false};

    pub const GYRO_UPDATE: ComSpec           = ComSpec{verb: 0xA000, w_words: 0,     r_words: 0     ,response: false};
    pub const GYRO_READ: ComSpec             = ComSpec{verb: 0xA100, w_words: 0,     r_words: 4     ,response: false};

    pub const POLL_USB_CC: ComSpec           = ComSpec{verb: 0xB000, w_words: 0,     r_words: 3     ,response: false};

    pub const LINK_READ: ComSpec             = ComSpec{verb: 0xF0F0, w_words: 0,     r_words: 0     ,response: false}; // dummy command to "pump" the bus to read data
    pub const LINK_SYNC: ComSpec             = ComSpec{verb: 0xFFFF, w_words: 0,     r_words: 0     ,response: false};

    pub const ERROR: ComSpec                 = ComSpec{verb: 0xDEAD, w_words: 0,     r_words: 0     ,response: true};
}
