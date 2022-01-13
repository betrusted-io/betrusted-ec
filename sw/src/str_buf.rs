#![allow(dead_code)]
use core::fmt::{self, Write};
use core::str::Utf8Error;

/// StrBuf holds a string buffer of N bytes and implments Write for use with write!()
pub struct StrBuf<const N: usize> {
    buf: [u8; N],
    length: usize,
}
impl<const N: usize> StrBuf<N> {
    /// Return a new buffer full of nulls
    pub const fn new() -> Self {
        Self {
            buf: [0; N],
            length: 0,
        }
    }

    /// Return length of the buffered string as bytes
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.length
    }

    /// Return the buffered string as bytes
    pub fn bytes(&self) -> &[u8] {
        &self.buf[..self.length]
    }

    /// Return the buffered string as a string slice
    pub fn as_str(&self) -> Result<&str, Utf8Error> {
        core::str::from_utf8(&self.bytes())
    }
}
impl<const N: usize> Write for StrBuf<N> {
    /// Write a string slice to the buffer
    fn write_str(&mut self, s: &str) -> Result<(), fmt::Error> {
        let src_len = s.len();
        let dst_start = self.length;
        if dst_start + src_len > self.buf.len() {
            return Err(fmt::Error);
        }
        for (dst, src) in self.buf[dst_start..].iter_mut().zip(s.as_bytes().iter()) {
            *dst = *src;
        }
        self.length = dst_start + src_len;
        Ok(())
    }
}
