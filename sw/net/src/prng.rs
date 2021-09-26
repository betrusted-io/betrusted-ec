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
    count: usize,
    s: [u32; 4],
}
impl NetPrng {
    /// Initialize from a random seed
    pub const fn new_from(seed: &[u16; 8]) -> Self {
        Self {
            count: 0,
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

    /// Advance xoshiro128++ PRNG using a special-case wrapper to mix bits after reseed.
    ///
    /// This gives the appearance of better randomness if initialized with a low entropy
    /// seed. Low entropy seed is what happens after an EC firmware re-flash without a SoC
    /// reset, because PRNG currently only gets reseeded from TRNG when Xous boots. Not
    /// resetting the SoC is useful as a means to avoid typing wifi SSID and PSK so much
    /// during periods of frequently recompiling and flashing the EC firmware.
    ///
    pub fn next(&mut self) -> u32 {
        const MIX_MIN: usize = 5;
        if self.count < MIX_MIN {
            for _ in 0..MIX_MIN {
                let _ = self.next_inner();
            }
        }
        self.next_inner()
    }

    /// Advance xoshiro128++ PRNG.
    /// Credits: This function was ported by Sam Blenny in 2021 from the 2019
    /// public domain xoshiro128plusplus.c implementation by David Blackman and
    /// Sebastiano Vigna
    fn next_inner(&mut self) -> u32 {
        self.count = self.count.saturating_add(1);
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
