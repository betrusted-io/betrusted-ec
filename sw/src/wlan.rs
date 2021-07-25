use crate::com_bus::com_rx;

// These constants help with reading the right number of u16 words from the COM
// bus for COM verbs that take string arguments. Strings here are assumed to be
// variable length, in a fixed size buffer (size varies by verb), and the first
// word holds the number of bytes in the string (can be null terminated or not).
//
const SSID_COM_RX_WORDS: usize = 17; // 1 length + 16 data (max lentth 32 bytes)
const PASS_COM_RX_WORDS: usize = 33; // 1 length + 32 data (max length 64 bytes)
pub const SSID_BUF_SIZE: usize = (SSID_COM_RX_WORDS - 1) * 2;
pub const PASS_BUF_SIZE: usize = (PASS_COM_RX_WORDS - 1) * 2;

/// Error codes related to COM bus protocol
pub enum WlanError {
    Assert = 1,
    Timeout = 2,
    StrLen = 3,
    Utf8 = 4,
}

pub struct WlanState {
    pass_len: u16,
    ssid_len: u16,
    pass_buf: [u8; PASS_BUF_SIZE],
    ssid_buf: [u8; SSID_BUF_SIZE],
}

impl WlanState {
    pub fn new() -> Self {
        Self {
            pass_len: 0,
            ssid_len: 0,
            pass_buf: [0; PASS_BUF_SIZE],
            ssid_buf: [0; SSID_BUF_SIZE],
        }
    }

    /// Make a string slice for the SSID (doing it this way helps pacify borrow checker).
    pub fn ssid(&self) -> Result<&str, WlanError> {
        if self.ssid_len as usize > SSID_BUF_SIZE {
            // Looks like an out of range string length argument came over the COM bus
            return Err(WlanError::StrLen);
        }
        let end = self.ssid_len as usize;
        match core::str::from_utf8(&self.ssid_buf[..end]) {
            Ok(ssid) => Ok(ssid),
            _ => Err(WlanError::Utf8),
        }
    }

    /// Make a string slice for the password (doing it this way helps pacify borrow checker).
    pub fn pass(&self) -> Result<&str, WlanError> {
        if self.pass_len as usize > PASS_BUF_SIZE {
            // Looks like an out of range string length argument came over the COM bus
            return Err(WlanError::StrLen);
        }
        let end = self.pass_len as usize;
        match core::str::from_utf8(&self.pass_buf[..end]) {
            Ok(pass) => Ok(pass),
            _ => Err(WlanError::Utf8),
        }
    }
}

/// Implement the ComState::WLAN_SET_SSID verb to set the SSID for use by ComState::WLAN_JOIN.
pub fn set_ssid(ws: &mut WlanState) -> Result<&str, WlanError> {
    if SSID_COM_RX_WORDS * 2 != ws.ssid_buf.len() + 2 {
        return Err(WlanError::Assert); // This should never happen
    }
    for i in 0..SSID_COM_RX_WORDS {
        match com_rx(500) {
            Ok(rx_word) => match i {
                0 => ws.ssid_len = rx_word,
                _ => {
                    let b = rx_word.to_le_bytes();
                    let n = (i - 1) * 2;
                    ws.ssid_buf[n] = b[0];
                    ws.ssid_buf[n + 1] = b[1];
                }
            },
            Err(_) => return Err(WlanError::Timeout), // This means COM bus out of sync. VERY BAD.
        }
    }
    ws.ssid()
}

/// Implement the ComState::WLAN_SET_PASS verb to set the password for use by ComState::WLAN_JOIN.
pub fn set_pass(ws: &mut WlanState) -> Result<&str, WlanError> {
    if PASS_COM_RX_WORDS * 2 != ws.pass_buf.len() + 2 {
        return Err(WlanError::Assert); // This should never happen
    }
    for i in 0..PASS_COM_RX_WORDS {
        match com_rx(500) {
            Ok(rx_word) => match i {
                0 => ws.pass_len = rx_word,
                _ => {
                    let b = rx_word.to_le_bytes();
                    let n = (i - 1) * 2;
                    ws.pass_buf[n] = b[0];
                    ws.pass_buf[n + 1] = b[1];
                }
            },
            Err(_) => return Err(WlanError::Timeout), // This means COM bus out of sync. VERY BAD.
        }
    }
    ws.pass()
}
