use crate::com_bus::com_rx;
use com_rs::serdes::{
    SerdesError, StringDes, STR_32_U8_SIZE, STR_32_WORDS, STR_64_U8_SIZE, STR_64_WORDS,
};
use com_rs::ComState;

/// Error codes related to COM bus protocol
pub enum WlanError {
    Assert = 1,
    Timeout = 2,
    StrLen = 3,
    Utf8 = 4,
}

pub struct WlanState {
    pass_: StringDes<STR_64_WORDS, STR_64_U8_SIZE>,
    ssid_: StringDes<STR_32_WORDS, STR_32_U8_SIZE>,
}

impl WlanState {
    pub fn new() -> Self {
        Self {
            pass_: StringDes::<STR_64_WORDS, STR_64_U8_SIZE>::new(),
            ssid_: StringDes::<STR_32_WORDS, STR_32_U8_SIZE>::new(),
        }
    }

    /// Make a string slice for the SSID
    pub fn ssid(&self) -> Result<&str, WlanError> {
        match self.ssid_.as_str() {
            Ok(ssid) => Ok(ssid),
            Err(SerdesError::StrLenTooBig) => Err(WlanError::StrLen),
            Err(SerdesError::Utf8Decode) => Err(WlanError::Utf8),
        }
    }

    /// Make a string slice for the password
    pub fn pass(&self) -> Result<&str, WlanError> {
        match self.pass_.as_str() {
            Ok(pass) => Ok(pass),
            Err(SerdesError::StrLenTooBig) => Err(WlanError::StrLen),
            Err(SerdesError::Utf8Decode) => Err(WlanError::Utf8),
        }
    }
}

/// Implement the ComState::WLAN_SET_SSID verb to set the SSID for use by ComState::WLAN_JOIN.
pub fn set_ssid(ws: &mut WlanState) -> Result<&str, WlanError> {
    if ComState::WLAN_SET_SSID.w_words != STR_32_WORDS as u16 {
        return Err(WlanError::Assert); // This should never happen
    }
    let mut rx_words = [0u16; STR_32_WORDS];
    for w in rx_words.iter_mut() {
        match com_rx(500) {
            Ok(rx) => *w = rx,
            Err(_) => return Err(WlanError::Timeout), // This means COM bus out of sync. VERY BAD.
        }
    }
    match ws.ssid_.decode_u16(&rx_words) {
        Ok(ssid) => Ok(ssid),
        Err(SerdesError::StrLenTooBig) => Err(WlanError::StrLen),
        Err(SerdesError::Utf8Decode) => Err(WlanError::Utf8),
    }
}

/// Implement the ComState::WLAN_SET_PASS verb to set the password for use by ComState::WLAN_JOIN.
pub fn set_pass(ws: &mut WlanState) -> Result<&str, WlanError> {
    if ComState::WLAN_SET_PASS.w_words != STR_64_WORDS as u16 {
        return Err(WlanError::Assert); // This should never happen
    }
    let mut rx_words = [0u16; STR_64_WORDS];
    for w in rx_words.iter_mut() {
        match com_rx(500) {
            Ok(rx) => *w = rx,
            Err(_) => return Err(WlanError::Timeout), // This means COM bus out of sync. VERY BAD.
        }
    }
    match ws.pass_.decode_u16(&rx_words) {
        Ok(pass) => Ok(pass),
        Err(SerdesError::StrLenTooBig) => Err(WlanError::StrLen),
        Err(SerdesError::Utf8Decode) => Err(WlanError::Utf8),
    }
}
