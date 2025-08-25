use crate::error::*;
use crate::leb128::*;

pub const MAGIC_HEADER: &[u8; 4] = b"\0asm";

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
pub fn is_val_type(byte: u8) -> bool { matches!(byte, 0x7c..=0x7f) }

#[inline]
pub fn valtype_from_byte(byte: u8) -> Option<ValType> {
    match byte {
        0x7f => Some(ValType::I32),
        0x7e => Some(ValType::I64),
        0x7d => Some(ValType::F32),
        0x7c => Some(ValType::F64),
        0x00 => Some(ValType::Null),
        _ => None,
    }
}

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
        if *idx >= bytes.len() { return Err(Error::Malformed(UNEXPECTED_END)); }
        let byte = bytes[*idx];
        if byte == EMPTY_TYPE || is_val_type(byte) {
            *idx += 1;
            Ok(singles(byte).unwrap())
        } else {
            let n: i64 = safe_read_sleb128(bytes, idx, 33)?;
            if n < 0 || (n as usize) >= types.len() {
                return Err(Error::Malformed(INVALID_VALUE_TYPE));
            }
            Ok(types[n as usize].clone())
        }
    }
}

#[inline(always)]
pub fn mutability_from_byte(byte: u8) -> Option<bool> {
    match byte {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

#[inline(always)] pub fn is_utf8(bytes: &[u8]) -> bool { std::str::from_utf8(bytes).is_ok() }