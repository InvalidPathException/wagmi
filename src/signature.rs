use crate::error::*;
use crate::leb128::*;

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ValType {
    I32 = 0x7f,
    I64 = 0x7e,
    F32 = 0x7d,
    F64 = 0x7c,
    Any = 0xff,
}

#[inline(always)]
pub fn is_val_type(byte: u8) -> bool { matches!(byte, 0x7c..=0x7f) }

#[inline]
pub fn val_type_from_byte(byte: u8) -> Option<ValType> {
    match byte {
        0x7f => Some(ValType::I32),
        0x7e => Some(ValType::I64),
        0x7d => Some(ValType::F32),
        0x7c => Some(ValType::F64),
        0xff => Some(ValType::Any),
        _ => None,
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Signature {
    pub params: Vec<ValType>,
    pub result: Option<ValType>,
}

impl Signature {
    pub fn read(types: &[Signature], bytes: &[u8], idx: &mut usize) -> Result<Signature, Error> {
        const VOID: u8 = 0x40;
        if *idx >= bytes.len() { return Err(Error::Malformed(UNEXPECTED_END)); }
        let byte = bytes[*idx];
        if byte == VOID {
            *idx += 1;
            Ok(Signature::default())
        } else if let Some(vt) = val_type_from_byte(byte) {
            *idx += 1;
            Ok(Signature { params: vec![], result: Some(vt) })
        } else {
            let n: i64 = safe_read_sleb128(bytes, idx, 33)?;
            if n < 0 || (n as usize) >= types.len() {
                return Err(Error::Malformed(INVALID_VALUE_TYPE));
            }
            Ok(types[n as usize].clone())
        }
    }
}