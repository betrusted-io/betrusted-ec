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
    pub fn hostname<'a>(&mut self, str_buf: &'a mut [u8; 15]) -> Result<&'a str, u8> {
        let rbytes0: [u8; 4] = self.next().to_le_bytes();
        let rbytes4: [u8; 4] = self.next().to_le_bytes();
        let rbytes8: [u8; 4] = self.next().to_le_bytes();
        let rbytes12: [u8; 4] = self.next().to_le_bytes();
        let mut rbytes = rbytes0
            .iter()
            .chain(rbytes4.iter())
            .chain(rbytes8.iter())
            .chain(rbytes12.iter());
        // Use bottom 3 bits of first random byte to select hostname length between 8 and 15
        let len = str_buf.len() - ((rbytes.next().unwrap_or(&7) & 0b0111) as usize);
        let sb_it = str_buf.iter_mut();
        // Set first len bytes of str_buf to random selection from [0-9A-Z] and remaining bytes to null
        for (i, c) in sb_it.enumerate() {
            *c = 0;
            if i >= len {
                continue;
            }
            if let Some(rb) = rbytes.next() {
                // Pick 1 from {10 digits, 26 letters}
                *c = match rb % (10 + 26) {
                    x @ 0..=9 => ('0' as u8) + x,
                    x @ 10..=35 => ('A' as u8) + x - 10,
                    _ => '0' as u8,
                };
            }
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
