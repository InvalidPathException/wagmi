#![allow(clippy::needless_return)]
use std::fmt::{Display, Formatter};
use crate::leb128::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Malformed(String),
    Validation(String),
    Trap(String),
    Link(String),
    Uninstantiable(String),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Malformed(s)
            | Error::Validation(s)
            | Error::Trap(s)
            | Error::Link(s)
            | Error::Uninstantiable(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for Error {}

#[inline(always)]
pub fn malformed<T>(msg: &str) -> Result<T, Error> { Err(Error::Malformed(msg.to_string())) }
#[inline(always)]
pub fn validation<T>(msg: &str) -> Result<T, Error> { Err(Error::Validation(msg.to_string())) }
#[inline(always)]
pub fn trap<T>(msg: &str) -> Result<T, Error> { Err(Error::Trap(msg.to_string())) }
#[inline(always)]
pub fn link<T>(msg: &str) -> Result<T, Error> { Err(Error::Link(msg.to_string())) }
#[inline(always)]
pub fn uninstantiable<T>(msg: &str) -> Result<T, Error> { Err(Error::Uninstantiable(msg.to_string())) }

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ValType {
    Null = 0x00,
    Any = 0xff,
    I32 = 0x7f,
    I64 = 0x7e,
    F32 = 0x7d,
    F64 = 0x7c,
}

#[inline(always)]
pub fn is_val_type(byte: u8) -> bool { matches!(byte, 0x7f | 0x7e | 0x7d | 0x7c) }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub params: Vec<ValType>,
    pub result: ValType,
    pub result_count: u8,
}

impl Default for Signature {
    fn default() -> Self {
        Self { params: Vec::new(), result: ValType::Null, result_count: 0 }
    }
}

impl Signature {
    pub fn results_view(&self) -> &[ValType] {
        if self.result_count == 0 { &[] } else { std::slice::from_ref(&self.result) }
    }

    pub fn read_blocktype(types: &[Signature], bytes: &[u8], idx: &mut usize) -> Result<Signature, Error> {
        const EMPTY_TYPE: u8 = 0x40;
        fn singles(byte: u8) -> Option<Signature> {
            match byte {
                0x40 => Some(Signature::default()),
                0x7f => Some(Signature { params: vec![], result: ValType::I32, result_count: 1 }),
                0x7e => Some(Signature { params: vec![], result: ValType::I64, result_count: 1 }),
                0x7d => Some(Signature { params: vec![], result: ValType::F32, result_count: 1 }),
                0x7c => Some(Signature { params: vec![], result: ValType::F64, result_count: 1 }),
                _ => None,
            }
        }
        if *idx >= bytes.len() { return malformed("unexpected end of section or function"); }
        let byte = bytes[*idx];
        return if byte == EMPTY_TYPE || is_val_type(byte) {
            *idx += 1;
            Ok(singles(byte).unwrap())
        } else {
            let n: i64 = safe_read_sleb128(bytes, idx, 33)?;
            if n < 0 || (n as usize) >= types.len() {
                return malformed("invalid value type");
            }
            Ok(types[n as usize].clone())
        }
    }
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Mut {
    Const = 0x0,
    Var = 0x1,
}

#[inline(always)]
pub fn is_mut_byte(byte: u8) -> bool { matches!(byte, 0x0 | 0x1) }
#[inline(always)]
pub fn is_valid_utf8(bytes: &[u8]) -> bool { std::str::from_utf8(bytes).is_ok() }