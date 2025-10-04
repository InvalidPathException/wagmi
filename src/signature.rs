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
        if *idx >= bytes.len() { return Err(Error::malformed(UNEXPECTED_END)); }
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
                return Err(Error::malformed(INVALID_VALUE_TYPE));
            }
            Ok(types[n as usize].clone())
        }
    }
}

#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Default)]
pub struct RuntimeSignature(u32);

impl RuntimeSignature {
    const HAS_RESULT: u32 = 1 << 16;
    const HAS_I32: u32 = 1 << 17; const HAS_I64: u32 = 1 << 18;
    const HAS_F32: u32 = 1 << 19; const HAS_F64: u32 = 1 << 20;

    #[inline(always)] pub fn n_params(&self) -> u32 { self.0 & 0xFFFF }
    #[inline(always)] pub fn has_result(&self) -> bool { (self.0 & Self::HAS_RESULT) != 0 }
    #[inline(always)] pub fn has_i32(&self) -> bool { (self.0 & Self::HAS_I32) != 0 }
    #[inline(always)] pub fn has_i64(&self) -> bool { (self.0 & Self::HAS_I64) != 0 }
    #[inline(always)] pub fn has_f32(&self) -> bool { (self.0 & Self::HAS_F32) != 0 }
    #[inline(always)] pub fn has_f64(&self) -> bool { (self.0 & Self::HAS_F64) != 0 }

    #[inline(always)]
    pub fn from_signature(sig: &Signature) -> Self {
        let mut bits: u32 = (sig.params.len() as u32) & 0xFFFF;
        if sig.result.is_some() { bits |= Self::HAS_RESULT; }
        for &param in &sig.params { set_type_bit32(&mut bits, param); }
        if let Some(res) = sig.result { set_type_bit32(&mut bits, res); }
        RuntimeSignature(bits)
    }

    #[inline(always)]
    pub fn from_counts(n_params: u32, has_result: bool) -> Self {
        let mut bits: u32 = n_params & 0xFFFF;
        if has_result { bits |= Self::HAS_RESULT; }
        RuntimeSignature(bits)
    }

    #[inline(always)]
    pub fn control_from_counts(n_params: u32, has_result: bool) -> Self {
        let base = Self::from_counts(n_params, has_result);
        base.with_presence()
    }
    
    #[inline(always)] pub fn with_presence(self) -> Self { RuntimeSignature(self.0 | (1 << 31)) }
    #[inline(always)] pub fn is_present(&self) -> bool { (self.0 & (1 << 31)) != 0 }
}

#[inline(always)]
fn set_type_bit32(bits: &mut u32, t: ValType) {
    match t {
        ValType::I32 => *bits |= RuntimeSignature::HAS_I32,
        ValType::I64 => *bits |= RuntimeSignature::HAS_I64,
        ValType::F32 => *bits |= RuntimeSignature::HAS_F32,
        ValType::F64 => *bits |= RuntimeSignature::HAS_F64,
        _ => {}
    }
}