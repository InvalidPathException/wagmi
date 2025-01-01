use std::fmt::Debug;

macro_rules! define_name_map {
    ($( $name:ident = $value:expr => $string:expr ),* $(,)?) => {
        pub mod constants {
            $(pub const $name: u8 = $value;)*
        }

        #[derive(Debug, Copy, Clone, PartialEq, Eq)]
        pub enum Opcode {
            $($name = $value),*
        }

        impl Opcode {
            #[inline]
            pub fn from_byte(byte: u8) -> Option<Self> {
                match byte {
                    $($value => Some(Self::$name),)*
                    _ => None,
                }
            }

            #[inline]
            pub fn as_str(&self) -> &'static str {
                match self {
                    $(Self::$name => $string,)*
                }
            }
        }
    };
}

pub mod opcodes {
    define_name_map!(
        UNREACHABLE = 0x00 => "unreachable",
        NOP = 0x01 => "nop",
        BLOCK = 0x02 => "block",
        LOOP = 0x03 => "loop",
        IF = 0x04 => "if",
        ELSE = 0x05 => "else",
        END = 0x0B => "end",
        BR = 0x0C => "br",
        BR_IF = 0x0D => "br_if",
        BR_TABLE = 0x0E => "br_table",
        RETURN = 0x0F => "return",
        CALL = 0x10 => "call",
        CALL_INDIRECT = 0x11 => "call_indirect",
        DROP = 0x1A => "drop",
        SELECT = 0x1B => "select",
        LOCAL_GET = 0x20 => "local.get",
        LOCAL_SET = 0x21 => "local.set",
        LOCAL_TEE = 0x22 => "local.tee",
        GLOBAL_GET = 0x23 => "global.get",
        GLOBAL_SET = 0x24 => "global.set",
        I32_LOAD = 0x28 => "i32.load",
        I64_LOAD = 0x29 => "i64.load",
        F32_LOAD = 0x2A => "f32.load",
        F64_LOAD = 0x2B => "f64.load",
        I32_LOAD8_S = 0x2C => "i32.load8_s",
        I32_LOAD8_U = 0x2D => "i32.load8_u",
        I32_LOAD16_S = 0x2E => "i32.load16_s",
        I32_LOAD16_U = 0x2F => "i32.load16_u",
        I64_LOAD8_S = 0x30 => "i64.load8_s",
        I64_LOAD8_U = 0x31 => "i64.load8_u",
        I64_LOAD16_S = 0x32 => "i64.load16_s",
        I64_LOAD16_U = 0x33 => "i64.load16_u",
        I64_LOAD32_S = 0x34 => "i64.load32_s",
        I64_LOAD32_U = 0x35 => "i64.load32_u",
        I32_STORE = 0x36 => "i32.store",
        I64_STORE = 0x37 => "i64.store",
        F32_STORE = 0x38 => "f32.store",
        F64_STORE = 0x39 => "f64.store",
        I32_STORE8 = 0x3A => "i32.store8",
        I32_STORE16 = 0x3B => "i32.store16",
        I64_STORE8 = 0x3C => "i64.store8",
        I64_STORE16 = 0x3D => "i64.store16",
        I64_STORE32 = 0x3E => "i64.store32",
        MEMORY_SIZE = 0x3F => "memory.size",
        MEMORY_GROW = 0x40 => "memory.grow",
        I32_CONST = 0x41 => "i32.const",
        I64_CONST = 0x42 => "i64.const",
        F32_CONST = 0x43 => "f32.const",
        F64_CONST = 0x44 => "f64.const",
        I32_EQZ = 0x45 => "i32.eqz",
        I32_EQ = 0x46 => "i32.eq",
        I32_NE = 0x47 => "i32.ne",
        I32_LT_S = 0x48 => "i32.lt_s",
        I32_LT_U = 0x49 => "i32.lt_u",
        I32_GT_S = 0x4A => "i32.gt_s",
        I32_GT_U = 0x4B => "i32.gt_u",
        I32_LE_S = 0x4C => "i32.le_s",
        I32_LE_U = 0x4D => "i32.le_u",
        I32_GE_S = 0x4E => "i32.ge_s",
        I32_GE_U = 0x4F => "i32.ge_u",
        I64_EQZ = 0x50 => "i64.eqz",
        I64_EQ = 0x51 => "i64.eq",
        I64_NE = 0x52 => "i64.ne",
        I64_LT_S = 0x53 => "i64.lt_s",
        I64_LT_U = 0x54 => "i64.lt_u",
        I64_GT_S = 0x55 => "i64.gt_s",
        I64_GT_U = 0x56 => "i64.gt_u",
        I64_LE_S = 0x57 => "i64.le_s",
        I64_LE_U = 0x58 => "i64.le_u",
        I64_GE_S = 0x59 => "i64.ge_s",
        I64_GE_U = 0x5A => "i64.ge_u",
        F32_EQ = 0x5B => "f32.eq",
        F32_NE = 0x5C => "f32.ne",
        F32_LT = 0x5D => "f32.lt",
        F32_GT = 0x5E => "f32.gt",
        F32_LE = 0x5F => "f32.le",
        F32_GE = 0x60 => "f32.ge",
        F64_EQ = 0x61 => "f64.eq",
        F64_NE = 0x62 => "f64.ne",
        F64_LT = 0x63 => "f64.lt",
        F64_GT = 0x64 => "f64.gt",
        F64_LE = 0x65 => "f64.le",
        F64_GE = 0x66 => "f64.ge",
        I32_CLZ = 0x67 => "i32.clz",
        I32_CTZ = 0x68 => "i32.ctz",
        I32_POPCNT = 0x69 => "i32.popcnt",
        I32_ADD = 0x6A => "i32.add",
        I32_SUB = 0x6B => "i32.sub",
        I32_MUL = 0x6C => "i32.mul",
        I32_DIV_S = 0x6D => "i32.div_s",
        I32_DIV_U = 0x6E => "i32.div_u",
        I32_REM_S = 0x6F => "i32.rem_s",
        I32_REM_U = 0x70 => "i32.rem_u",
        I32_AND = 0x71 => "i32.and",
        I32_OR = 0x72 => "i32.or",
        I32_XOR = 0x73 => "i32.xor",
        I32_SHL = 0x74 => "i32.shl",
        I32_SHR_S = 0x75 => "i32.shr_s",
        I32_SHR_U = 0x76 => "i32.shr_u",
        I32_ROTL = 0x77 => "i32.rotl",
        I32_ROTR = 0x78 => "i32.rotr",
        I64_CLZ = 0x79 => "i64.clz",
        I64_CTZ = 0x7A => "i64.ctz",
        I64_POPCNT = 0x7B => "i64.popcnt",
        I64_ADD = 0x7C => "i64.add",
        I64_SUB = 0x7D => "i64.sub",
        I64_MUL = 0x7E => "i64.mul",
        I64_DIV_S = 0x7F => "i64.div_s",
        I64_DIV_U = 0x80 => "i64.div_u",
        I64_REM_S = 0x81 => "i64.rem_s",
        I64_REM_U = 0x82 => "i64.rem_u",
        I64_AND = 0x83 => "i64.and",
        I64_OR = 0x84 => "i64.or",
        I64_XOR = 0x85 => "i64.xor",
        I64_SHL = 0x86 => "i64.shl",
        I64_SHR_S = 0x87 => "i64.shr_s",
        I64_SHR_U = 0x88 => "i64.shr_u",
        I64_ROTL = 0x89 => "i64.rotl",
        I64_ROTR = 0x8A => "i64.rotr",
        F32_ABS = 0x8B => "f32.abs",
        F32_NEG = 0x8C => "f32.neg",
        F32_CEIL = 0x8D => "f32.ceil",
        F32_FLOOR = 0x8E => "f32.floor",
        F32_TRUNC = 0x8F => "f32.trunc",
        F32_NEAREST = 0x90 => "f32.nearest",
        F32_SQRT = 0x91 => "f32.sqrt",
        F32_ADD = 0x92 => "f32.add",
        F32_SUB = 0x93 => "f32.sub",
        F32_MUL = 0x94 => "f32.mul",
        F32_DIV = 0x95 => "f32.div",
        F32_MIN = 0x96 => "f32.min",
        F32_MAX = 0x97 => "f32.max",
        F32_COPYSIGN = 0x98 => "f32.copysign",
        F64_ABS = 0x99 => "f64.abs",
        F64_NEG = 0x9A => "f64.neg",
        F64_CEIL = 0x9B => "f64.ceil",
        F64_FLOOR = 0x9C => "f64.floor",
        F64_TRUNC = 0x9D => "f64.trunc",
        F64_NEAREST = 0x9E => "f64.nearest",
        F64_SQRT = 0x9F => "f64.sqrt",
        F64_ADD = 0xA0 => "f64.add",
        F64_SUB = 0xA1 => "f64.sub",
        F64_MUL = 0xA2 => "f64.mul",
        F64_DIV = 0xA3 => "f64.div",
        F64_MIN = 0xA4 => "f64.min",
        F64_MAX = 0xA5 => "f64.max",
        F64_COPYSIGN = 0xA6 => "f64.copysign",
        I32_WRAP_I64 = 0xA7 => "i32.wrap_i64",
        I32_TRUNC_F32_S = 0xA8 => "i32.trunc_f32_s",
        I32_TRUNC_F32_U = 0xA9 => "i32.trunc_f32_u",
        I32_TRUNC_F64_S = 0xAA => "i32.trunc_f64_s",
        I32_TRUNC_F64_U = 0xAB => "i32.trunc_f64_u",
        I64_EXTEND_I32_S = 0xAC => "i64.extend_i32_s",
        I64_EXTEND_I32_U = 0xAD => "i64.extend_i32_u",
        I64_TRUNC_F32_S = 0xAE => "i64.trunc_f32_s",
        I64_TRUNC_F32_U = 0xAF => "i64.trunc_f32_u",
        I64_TRUNC_F64_S = 0xB0 => "i64.trunc_f64_s",
        I64_TRUNC_F64_U = 0xB1 => "i64.trunc_f64_u",
        F32_CONVERT_I32_S = 0xB2 => "f32.convert_i32_s",
        F32_CONVERT_I32_U = 0xB3 => "f32.convert_i32_u",
        F32_CONVERT_I64_S = 0xB4 => "f32.convert_i64_s",
        F32_CONVERT_I64_U = 0xB5 => "f32.convert_i64_u",
        F32_DEMOTE_F64 = 0xB6 => "f32.demote_f64",
        F64_CONVERT_I32_S = 0xB7 => "f64.convert_i32_s",
        F64_CONVERT_I32_U = 0xB8 => "f64.convert_i32_u",
        F64_CONVERT_I64_S = 0xB9 => "f64.convert_i64_s",
        F64_CONVERT_I64_U = 0xBA => "f64.convert_i64_u",
        F64_PROMOTE_F32 = 0xBB => "f64.promote_f32",
        I32_REINTERPRET_F32 = 0xBC => "i32.reinterpret_f32",
        I64_REINTERPRET_F64 = 0xBD => "i64.reinterpret_f64",
        F32_REINTERPRET_I32 = 0xBE => "f32.reinterpret_i32",
        F64_REINTERPRET_I64 = 0xBF => "f64.reinterpret_i64"
    );
}

#[derive(Debug, Clone, Copy)]
pub enum WasmValue {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
}

impl WasmValue {
    pub fn from_i32(value: i32) -> Self {
        WasmValue::I32(value)
    }
    pub fn from_i64(value: i64) -> Self {
        WasmValue::I64(value)
    }
    pub fn from_f32(value: f32) -> Self {
        WasmValue::F32(value)
    }
    pub fn from_f64(value: f64) -> Self {
        WasmValue::F64(value)
    }
    pub fn to_i32(&self) -> Option<i32> {
        if let WasmValue::I32(v) = *self {
            Some(v)
        } else {
            None
        }
    }
    pub fn to_i64(&self) -> Option<i64> {
        if let WasmValue::I64(v) = *self {
            Some(v)
        } else {
            None
        }
    }
    pub fn to_f32(&self) -> Option<f32> {
        if let WasmValue::F32(v) = *self {
            Some(v)
        } else {
            None
        }
    }
    pub fn to_f64(&self) -> Option<f64> {
        if let WasmValue::F64(v) = *self {
            Some(v)
        } else {
            None
        }
    }
    pub fn f32_to_i32_trunc(value: f32) -> Option<i32> {
        (value.is_finite() && value >= i32::MIN as f32 && value <= i32::MAX as f32)
            .then(|| value as i32)
    }
    pub fn f32_to_i64_trunc(value: f32) -> Option<i64> {
        (value.is_finite() && value >= i64::MIN as f32 && value <= i64::MAX as f32)
            .then(|| value as i64)
    }
    pub fn f64_to_i32_trunc(value: f64) -> Option<i32> {
        (value.is_finite() && value >= i32::MIN as f64 && value <= i32::MAX as f64)
            .then(|| value as i32)
    }
    pub fn f64_to_i64_trunc(value: f64) -> Option<i64> {
        (value.is_finite() && value >= i64::MIN as f64 && value <= i64::MAX as f64)
            .then(|| value as i64)
    }
}

pub mod op_impl {
    use paste::paste;

    #[macro_export]
    macro_rules! binary_fn {
        ($stack:expr, $in_type:ident, $out_type:ident, $func:expr) => {{
            paste::paste! {
                let val1 = $stack.pop().expect("Stack underflow").[<to_ $in_type>]()
                    .expect(concat!("Wrong type (expected ", stringify!($in_type), ")"));
                let top = $stack.last_mut().expect("Stack underflow");
                let val2 = top.[<to_ $in_type>]()
                    .expect(concat!("Wrong type (expected ", stringify!($in_type), ")"));
                *top = WasmValue::[<$out_type:upper>]($func(val2, val1));
            }
        }};
    }

    #[macro_export]
    macro_rules! div_s {
    ($stack:expr, $type:ident) => {{
            paste::paste! {
                let val1 = $stack.pop().expect("Stack underflow").[<to_ $type>]()
                    .expect(concat!("Wrong type (expected ", stringify!($type), ")"));
                let top = $stack.last_mut().expect("Stack underflow");
                let val2 = top.[<to_ $type>]()
                    .expect(concat!("Wrong type (expected ", stringify!($type), ")"));
    
                if val1 == 0 {
                    panic!("Division by zero");
                }
    
                *top = WasmValue::[<$type:upper>](val2.wrapping_div(val1));
            }
        }};
    }

    #[macro_export]
    macro_rules! div_u {
    ($stack:expr, $type:ident) => {{
            paste::paste! {
                let val1 = $stack.pop().expect("Stack underflow").[<to_ $type>]()
                    .expect(concat!("Wrong type (expected ", stringify!($type), ")"));
                let top = $stack.last_mut().expect("Stack underflow");
                let val2 = top.[<to_ $type>]()
                    .expect(concat!("Wrong type (expected ", stringify!($type), ")"));
    
                if val1 == 0 {
                    panic!("Division by zero");
                }
                
                *top = WasmValue::[<$type:upper>]((val2 as u64).wrapping_div(val1 as u64) as $type);
            }
        }};
    }

    #[macro_export]
    macro_rules! div_f {
        ($stack:expr, $type:ident) => {{
            paste::paste! {
                let val1 = $stack.pop().expect("Stack underflow").[<to_ $type>]()
                    .expect(concat!("Wrong type (expected ", stringify!($type), ")"));
                let top = $stack.last_mut().expect("Stack underflow");
                let val2 = top.[<to_ $type>]()
                    .expect(concat!("Wrong type (expected ", stringify!($type), ")"));
    
                // Perform floating-point division
                let result = val2 / val1;
    
                *top = WasmValue::[<$type:upper>](result);
            }
        }};
    }
    
    #[macro_export]
    macro_rules! unary_fn {
        ($stack:expr, $in_type:ident, $out_type:ident, $func:expr) => {{
            paste::paste! {
                let top = $stack.last_mut().expect("Stack underflow");
                let val = top.[<to_ $in_type>]()
                    .expect(concat!("Wrong type (expected ", stringify!($in_type), ")"));
                *top = WasmValue::[<$out_type:upper>]($func(val));
            }
        }};
    }

    macro_rules! define_irelop {
        ($name:literal, $signed_type:ty, $unsigned_type:ty) => {
            paste! {
                #[inline(always)]
                pub fn [<$name _eq>](a: $signed_type, b: $signed_type) -> i32 {
                    if a ^ b == 0 { 1 } else { 0 }
                }
                #[inline(always)]
                pub fn [<$name _ne>](a: $signed_type, b: $signed_type) -> i32 {
                    if a ^ b != 0 { 1 } else { 0 }
                }
                #[inline(always)]
                pub fn [<$name _lt_u>](a: $signed_type, b: $signed_type) -> i32 {
                    if (a as $unsigned_type) < (b as $unsigned_type) { 1 } else { 0 }
                }
                #[inline(always)]
                pub fn [<$name _gt_u>](a: $signed_type, b: $signed_type) -> i32 {
                    if (a as $unsigned_type) > (b as $unsigned_type) { 1 } else { 0 }
                }
                #[inline(always)]
                pub fn [<$name _le_u>](a: $signed_type, b: $signed_type) -> i32 {
                    if (a as $unsigned_type) <= (b as $unsigned_type) { 1 } else { 0 }
                }
                #[inline(always)]
                pub fn [<$name _ge_u>](a: $signed_type, b: $signed_type) -> i32 {
                    if (a as $unsigned_type) >= (b as $unsigned_type) { 1 } else { 0 }
                }
                #[inline(always)]
                pub fn [<$name _lt_s>](a: $signed_type, b: $signed_type) -> i32 {
                    if a < b { 1 } else { 0 }
                }
                #[inline(always)]
                pub fn [<$name _gt_s>](a: $signed_type, b: $signed_type) -> i32 {
                    if a > b { 1 } else { 0 }
                }
                #[inline(always)]
                pub fn [<$name _le_s>](a: $signed_type, b: $signed_type) -> i32 {
                    if a <= b { 1 } else { 0 }
                }
                #[inline(always)]
                pub fn [<$name _ge_s>](a: $signed_type, b: $signed_type) -> i32 {
                    if a >= b { 1 } else { 0 }
                }
            }
        };
    }

    macro_rules! define_frelop {
        ($name:literal, $float_type:ty) => {
            paste! {
                #[inline(always)]
                pub fn [<$name _eq>](a: $float_type, b: $float_type) -> i32 {
                    if a == b { 1 } else { 0 }
                }
                #[inline(always)]
                pub fn [<$name _ne>](a: $float_type, b: $float_type) -> i32 {
                    if a != b { 1 } else { 0 }
                }
                #[inline(always)]
                pub fn [<$name _lt>](a: $float_type, b: $float_type) -> i32 {
                    if a < b { 1 } else { 0 }
                }
                #[inline(always)]
                pub fn [<$name _gt>](a: $float_type, b: $float_type) -> i32 {
                    if a > b { 1 } else { 0 }
                }
                #[inline(always)]
                pub fn [<$name _le>](a: $float_type, b: $float_type) -> i32 {
                    if a <= b { 1 } else { 0 }
                }
                #[inline(always)]
                pub fn [<$name _ge>](a: $float_type, b: $float_type) -> i32 {
                    if a >= b { 1 } else { 0 }
                }
            }
        };
    }

    macro_rules! define_ibinop {
        ($name:literal, $type:ty) => {
            paste! {
                #[inline(always)]
                pub fn [<$name _add>](a: $type, b: $type) -> $type {
                    a.wrapping_add(b)
                }
                #[inline(always)]
                pub fn [<$name _sub>](a: $type, b: $type) -> $type {
                    a.wrapping_sub(b)
                }
                #[inline(always)]
                pub fn [<$name _mul>](a: $type, b: $type) -> $type {
                    a.wrapping_mul(b)
                }
                #[inline(always)]
                pub fn [<$name _and>](a: $type, b: $type) -> $type {
                    a & b
                }
                #[inline(always)]
                pub fn [<$name _or>](a: $type, b: $type) -> $type {
                    a | b
                }
                #[inline(always)]
                pub fn [<$name _xor>](a: $type, b: $type) -> $type {
                    a ^ b
                }
                #[inline(always)]
                pub fn [<$name _shl>](a: $type, b: $type) -> $type {
                    a.wrapping_shl(b as u32)
                }
                #[inline(always)]
                pub fn [<$name _shr_s>](a: $type, b: $type) -> $type {
                    a >> (b as u32)
                }
                #[inline(always)]
                pub fn [<$name _shr_u>](a: $type, b: $type) -> $type {
                    a.wrapping_shr(b as u32)
                }
                #[inline(always)]
                pub fn [<$name _rotl>](a: $type, b: $type) -> $type {
                    a.rotate_left(b as u32)
                }
                #[inline(always)]
                pub fn [<$name _rotr>](a: $type, b: $type) -> $type {
                    a.rotate_right(b as u32)
                }
            }
        };
    }

    macro_rules! define_fbinop {
        ($name:literal, $type:ty) => {
            paste! {
                #[inline(always)]
                pub fn [<$name _add>](a: $type, b: $type) -> $type {
                    a + b
                }
                #[inline(always)]
                pub fn [<$name _sub>](a: $type, b: $type) -> $type {
                    a - b
                }
                #[inline(always)]
                pub fn [<$name _mul>](a: $type, b: $type) -> $type {
                    a * b
                }
                #[inline(always)]
                pub fn [<$name _min>](a: $type, b: $type) -> $type {
                    a.min(b)
                }
                #[inline(always)]
                pub fn [<$name _max>](a: $type, b: $type) -> $type {
                    a.max(b)
                }
                #[inline(always)]
                pub fn [<$name _copysign>](a: $type, b: $type) -> $type {
                    a.copysign(b)
                }
            }
        };
    }

    define_irelop!("i32", i32, u32);
    define_irelop!("i64", i64, u64);
    define_frelop!("f32", f32);
    define_frelop!("f64", f64);
    define_ibinop!("i32", i32);
    define_ibinop!("i64", i64);
    define_fbinop!("f32", f32);
    define_fbinop!("f64", f64);
}
