use crate::com_bus::com_rx;

// These constants help with reading the right number of u16 words from the COM
// bus for COM verbs that take string arguments. Strings here are assumed to be
// variable length, in a fixed size buffer (size varies by verb), and the first
// word holds the number of bytes in the string (can be null terminated or not).
const SSID_COM_RX_WORDS: usize = 17;
const PASS_COM_RX_WORDS: usize = 33;
const SSID_BUF_SIZE: usize = SSID_COM_RX_WORDS * 2;
const PASS_BUF_SIZE: usize = PASS_COM_RX_WORDS * 2;

pub type PassBuf = [u8; PASS_BUF_SIZE];
pub type SsidBuf = [u8; SSID_BUF_SIZE];

pub fn new_blank_passbuf() -> PassBuf {
    [0; PASS_BUF_SIZE]
}

pub fn new_blank_ssidbuf() -> SsidBuf {
    [0; SSID_BUF_SIZE]
}

/// Implement the ComState::SET_SSID verb to set the SSID for use by ComState::WLAN_JOIN.
pub fn set_ssid(sbuf: &mut SsidBuf) -> Result<&str, u8> {
    if SSID_COM_RX_WORDS * 2 != sbuf.len() {
        return Err(1);
    }
    let mut err_code = 0;
    for i in 0..SSID_COM_RX_WORDS {
        match com_rx(500) {
            Ok(result) => {
                let b = result.to_le_bytes();
                let n = i * 2;
                sbuf[n] = b[0];
                sbuf[n + 1] = b[1];
            }
            Err(_) => err_code = 2,
        }
    }
    if err_code > 0 {
        // Problem with COM bus RX timeout
        return Err(err_code);
    }
    if (sbuf[0] + 2) as usize >= SSID_BUF_SIZE || sbuf[1] != 0 {
        // String length that was encoded in first rx word had an impossible value
        return Err(3);
    }
    // Use the string length argument (not C-style null termination) to make a string slice
    let end_of_str = 2 + sbuf[0] as usize;
    match core::str::from_utf8(&sbuf[2..end_of_str]) {
        Ok(ssid) => Ok(ssid),
        _ => Err(4),
    }
}

/// Implement the ComState::SET_PASS verb to set the password for use by ComState::WLAN_JOIN.
pub fn set_pass(pbuf: &mut PassBuf) -> Result<&str, u8> {
    if PASS_COM_RX_WORDS * 2 != pbuf.len() {
        return Err(1);
    }
    let mut err_code = 0;
    for i in 0..PASS_COM_RX_WORDS {
        match com_rx(500) {
            Ok(result) => {
                let b = result.to_le_bytes();
                let n = i * 2;
                pbuf[n] = b[0];
                pbuf[n + 1] = b[1];
            }
            Err(_) => err_code = 2,
        }
    }
    if err_code > 0 {
        // Problem with COM bus RX timeout
        return Err(err_code);
    }
    if (pbuf[0] + 2) as usize >= PASS_BUF_SIZE || pbuf[1] != 0 {
        // String length that was encoded in first rx word had an impossible value
        return Err(3);
    }
    // Use the string length argument (not C-style null termination) to make a string slice
    let end_of_str = 2 + pbuf[0] as usize;
    match core::str::from_utf8(&pbuf[2..end_of_str]) {
        Ok(pass) => Ok(pass),
        _ => Err(4),
    }
}
