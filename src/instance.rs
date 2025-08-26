use crate::signature::*;
use crate::wasm_memory::WasmMemory;

#[derive(Copy, Clone, Default)]
pub struct WasmValue(pub u64);

impl WasmValue {
    #[inline] pub fn from_i32(v: i32) -> Self { Self(v as u32 as u64) }
    #[inline] pub fn as_i32(self) -> i32 { self.0 as u32 as i32 }
    #[inline] pub fn from_u32(v: u32) -> Self { Self(v as u64) }
    #[inline] pub fn as_u32(self) -> u32 { self.0 as u32 }
    #[inline] pub fn from_i64(v: i64) -> Self { Self(v as u64) }
    #[inline] pub fn as_i64(self) -> i64 { self.0 as i64 }
    #[inline] pub fn from_u64(v: u64) -> Self { Self(v) }
    #[inline] pub fn as_u64(self) -> u64 { self.0 }
    #[inline] pub fn from_f32_bits(bits: u32) -> Self { Self(bits as u64) }
    #[inline] pub fn as_f32_bits(self) -> u32 { self.0 as u32 }
    #[inline] pub fn from_f64_bits(bits: u64) -> Self { Self(bits) }
    #[inline] pub fn as_f64_bits(self) -> u64 { self.0 }
    #[inline] pub fn from_f32(v: f32) -> Self { Self::from_f32_bits(v.to_bits()) }
    #[inline] pub fn as_f32(self) -> f32 { f32::from_bits(self.as_f32_bits()) }
    #[inline] pub fn from_f64(v: f64) -> Self { Self::from_f64_bits(v.to_bits()) }
    #[inline] pub fn as_f64(self) -> f64 { f64::from_bits(self.as_f64_bits()) }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct RuntimeType(u64);

macro_rules! set_type_bit {
    ($bits:expr, $t:expr) => {
        match $t {
            ValType::I32 => $bits |= RuntimeType::HAS_I32,
            ValType::I64 => $bits |= RuntimeType::HAS_I64,
            ValType::F32 => $bits |= RuntimeType::HAS_F32,
            ValType::F64 => $bits |= RuntimeType::HAS_F64,
            _ => unreachable!()
        }
    };
}

impl RuntimeType {
    const HAS_RESULT: u64 = 1 << 32;
    const HAS_I32: u64 = 1 << 33; const HAS_I64: u64 = 1 << 34;
    const HAS_F32: u64 = 1 << 35; const HAS_F64: u64 = 1 << 36;
    #[inline] pub fn n_params(&self) -> u32 { self.0 as u32 }
    #[inline] pub fn has_result(&self) -> bool { (self.0 & Self::HAS_RESULT) != 0 }
    #[inline] pub fn has_i32(&self) -> bool { (self.0 & Self::HAS_I32) != 0 }
    #[inline] pub fn has_i64(&self) -> bool { (self.0 & Self::HAS_I64) != 0 }
    #[inline] pub fn has_f32(&self) -> bool { (self.0 & Self::HAS_F32) != 0 }
    #[inline] pub fn has_f64(&self) -> bool { (self.0 & Self::HAS_F64) != 0 }

    pub fn from_signature(sig: &Signature) -> Self {
        let mut bits: u64 = sig.params.len() as u64;
        if sig.result.is_some() { bits |= Self::HAS_RESULT; }
        for &param in &sig.params { set_type_bit!(bits, param); }
        if let Some(res) = sig.result { set_type_bit!(bits, res); }
        RuntimeType(bits)
    }
}
