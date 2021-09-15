//! Pseudorandom generator for network protocol fields needing random values.
//!
//! This random value generator is based around a Rust port of the original C
//! xoshiro128++ PRNG implementation by David Blackman and Sebastiano Vigna at
//! https://prng.di.unimi.it/xoshiro128plusplus.c
//!
//! CREDITS (copyright notice from xoshiro128plusplus.c):
//!
//!  > Written in 2019 by David Blackman and Sebastiano Vigna (vigna@acm.org)
//!  >
//!  > To the extent possible under law, the author has dedicated all copyright
//!  > and related and neighboring rights to this software to the public domain
//!  > worldwide. This software is distributed without any warranty.
//!  >
//!  > See <http://creativecommons.org/publicdomain/zero/1.0/>.
//!

/// Generate randomized network protocol values based on xoshiro128++ PRNG
#[derive(Copy, Clone)]
pub struct NetPrng {
    s: [u32; 4],
}
impl NetPrng {
    /// Initialize from a random seed
    pub const fn new_from(seed: &[u16; 8]) -> Self {
        Self {
            s: [
                ((seed[0] as u32) << 16) | seed[1] as u32,
                ((seed[2] as u32) << 16) | seed[3] as u32,
                ((seed[4] as u32) << 16) | seed[5] as u32,
                ((seed[6] as u32) << 16) | seed[7] as u32,
            ],
        }
    }

    /// Reseed the PRNG
    pub fn reseed(&mut self, seed: &[u16; 8]) {
        *self = Self::new_from(seed);
    }

    /// Encode new pseudorandom hostname string into str_buf and return as &str.
    /// See RFC 952, RFC 1123 § 2.1, and RFC 2181 § 11
    pub fn hostname<'a>(&mut self, str_buf: &'a mut [u8; 8]) -> Result<&'a str, u8> {
        // Select hostname length between 5 and 8 characters
        let len = str_buf.len() - ((self.next() & 0b011) as usize);
        // Gather enough random bytes for up to 8 characters
        let rbytes0: [u8; 4] = self.next().to_le_bytes();
        let rbytes4: [u8; 4] = self.next().to_le_bytes();
        let skip = str_buf.len() - len;
        let rbytes = rbytes0.iter().chain(rbytes4.iter()).skip(skip);
        // Set first len bytes of str_buf to independent random symbols picked from charset
        for (i, (dst, src)) in str_buf.iter_mut().zip(rbytes).enumerate() {
            let mut masked_src = src & 0b0001_1111;
            if (i == 0) && masked_src < 10 {
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
        match core::str::from_utf8(&str_buf[..len]) {
            Ok(hostname) => Ok(hostname),
            Err(_) => Err(0x01),
        }
    }

    /// Advance xoshiro128++ PRNG.
    /// Credits: This function was ported by Sam Blenny in 2021 from the 2019
    /// public domain xoshiro128plusplus.c implementation by David Blackman and
    /// Sebastiano Vigna
    pub fn next(&mut self) -> u32 {
        let result: u32 = (self.s[0] + self.s[3]).rotate_left(7) + self.s[0];
        let t: u32 = self.s[1] << 9;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(11);
        result
    }
}
