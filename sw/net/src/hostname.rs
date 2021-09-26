/// Hold a host name string
/// 63 byte max length comes from syntax spec for "label" in RFC 1035 § 2.3.1 Preferred name syntax
pub struct Hostname {
    pub length: usize,
    pub buffer: [u8; 63],
}
impl Hostname {
    pub const fn new_blank() -> Self {
        Hostname {
            length: 1,
            buffer: [0; 63],
        }
    }

    /// Generate a new pseudorandom alphanumeric hostname of length 5 to 8 characters
    /// See RFC 952, RFC 1123 § 2.1, and RFC 2181 § 11
    pub fn randomize(&mut self, entropy0: u32, entropy1: u32) {
        // Select hostname length between 5 and 8 characters
        self.length = 8 - ((entropy0 & 0b011) as usize);
        // Prepare enough random bytes for up to 8 characters
        let rbytes0: [u8; 4] = entropy0.to_le_bytes();
        let rbytes4: [u8; 4] = entropy1.to_le_bytes();
        let rbytes = rbytes0.iter().chain(rbytes4.iter()).take(self.length);
        // Set first .length bytes of .buffer to independent random symbols picked from charset
        for (i, (dst, src)) in self.buffer.iter_mut().zip(rbytes).enumerate() {
            let mut masked_src = src & 0b0001_1111;
            if (i == 0) && masked_src <= 10 {
                masked_src += 11; // Avoid starting with a number or 'A'
            }
            // Translation table for charset "0123456789ABCDFGHJKLMNPQRSTVWXYZ" (32 symbols)
            // 0123456789 ABCD E FGH I JKLMN O PQRST U VWXYZ
            // 0000000000 1111 1 111 1 12222 2 22222 3 33333
            // 0123456789 0123 4 567 8 90123 4 56789 0 12345
            // 0000000000 1111   111   11122   22222   22233
            // 0123456789 0123   456   78901   23456   78901
            *dst = match masked_src {
                x @ 0..=9 => ('0' as u8) + x,
                x @ 10..=13 => ('A' as u8) + x - 10,
                x @ 14..=16 => ('F' as u8) + x - 14,
                x @ 17..=21 => ('J' as u8) + x - 17,
                x @ 22..=26 => ('P' as u8) + x - 22,
                x @ 27..=31 => ('V' as u8) + x - 27,
                _ => '0' as u8,
            };
        }
    }

    /// Return hostname as &str
    pub fn as_str(&self) -> &str {
        match core::str::from_utf8(self.as_bytes()) {
            Ok(s) => s,
            _ => &"",
        }
    }

    /// Return a byte slice for this hostname
    pub fn as_bytes(&self) -> &[u8] {
        &self.buffer[..self.length]
    }

    /// Return length of hostname
    pub fn len(&self) -> usize {
        self.length
    }
}
