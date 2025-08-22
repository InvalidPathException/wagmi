use crate::spec::Error;
use crate::error_msg::*;

#[derive(Clone, Copy)]
pub struct ByteIter<'a> {
    pub bytes: &'a [u8],
    pub idx: usize,
}

impl<'a> ByteIter<'a> {
    #[inline]
    pub fn new(bytes: &'a [u8], idx: usize) -> Self { Self { bytes, idx } }
    #[inline]
    pub fn empty(&self) -> bool { self.idx >= self.bytes.len() }
    #[inline]
    pub fn has_n_left(&self, n: usize) -> bool { self.idx + n <= self.bytes.len() }
    #[inline]
    pub fn get_with_at_least(&self, n: usize) -> Result<usize, Error> {
        if !self.has_n_left(n) { return Err(Error::Malformed(UNEXPECTED_END)); }
        Ok(self.idx)
    }
    #[inline]
    pub fn cur(&self) -> usize { self.idx }
    #[inline]
    pub fn advance(&mut self, n: usize) { self.idx += n; }
    #[inline]
    pub fn read_u8(&mut self) -> Result<u8, Error> {
        if self.idx >= self.bytes.len() { return Err(Error::Malformed(UNEXPECTED_END)); }
        let b = self.bytes[self.idx];
        self.idx += 1;
        Ok(b)
    }
    #[inline]
    pub fn peek_u8(&self) -> Result<u8, Error> {
        if self.idx >= self.bytes.len() { return Err(Error::Malformed(UNEXPECTED_END)); }
        Ok(self.bytes[self.idx])
    }
    #[inline]
    pub fn slice_from(&self, start: usize, len: usize) -> Result<&'a [u8], Error> {
        let end = start.checked_add(len).ok_or_else(|| Error::Malformed(UNEXPECTED_END_SHORT))?;
        if end > self.bytes.len() { return Err(Error::Malformed(UNEXPECTED_END_SHORT)); }
        Ok(&self.bytes[start..end])
    }
}