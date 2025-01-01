pub mod leb128 {
    use std::error::Error;
    use std::fmt;
    use paste::paste;

    #[derive(Debug)]
    pub struct LEB128Error(String);

    impl fmt::Display for LEB128Error {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl Error for LEB128Error {}

    #[inline(always)]
    pub(crate) fn read_leb128_s<I>(iter: &mut I) -> Result<i64, LEB128Error>
    where
        I: Iterator<Item = u8>,
    {
        let mut result: i64 = 0;
        let mut shift = 0;

        for (bytes_read, byte) in iter.enumerate() {
            result |= ((byte & 0x7f) as i64) << shift;
            shift += 7;

            if bytes_read >= 10 {
                return Err(LEB128Error("Too many bytes for LEB128".to_string()));
            }

            if (byte & 0x80) == 0 {
                if shift < 64 && (result & (1 << (shift - 1))) != 0 {
                    result |= -1i64 << shift;
                }
                return Ok(result);
            }
        }

        Err(LEB128Error("Premature end of input".to_string()))
    }

    #[inline(always)]
    pub(crate) fn read_leb128_u<I>(iter: &mut I) -> Result<u64, LEB128Error>
    where
        I: Iterator<Item = u8>,
    {
        let mut result: u64 = 0;
        let mut shift = 0;

        for (bytes_read, byte) in iter.enumerate() {
            result |= ((byte & 0x7f) as u64) << shift;
            shift += 7;

            if bytes_read >= 10 {
                return Err(LEB128Error("Too many bytes for LEB128".to_string()));
            }

            if (byte & 0x80) == 0 {
                return Ok(result);
            }
        }

        Err(LEB128Error("Premature end of input".to_string()))
    }

    macro_rules! define_load {
        ($name:ident, $wasm_type:ty, $leb_fn:ident) => {
            paste! {
                #[inline(always)]
                pub fn [<$name _load>](
                    stack: &mut Vec<WasmValue>,
                    memory: &[u8],
                ) -> Result<(), LEB128Error> {
                    let addr = stack.pop()
                        .ok_or_else(|| LEB128Error("Stack underflow".to_string()))?
                        .to_i32()
                        .map_err(|_| LEB128Error("Invalid stack value for address".to_string()))? as usize;
    
                    let mut memory_slice = memory.get(addr..)
                        .ok_or_else(|| LEB128Error("Memory access out of bounds".to_string()))?;
    
                    let value = $leb_fn(&mut memory_slice)?;
                    stack.push(WasmValue::$name(value));
                    
                    Ok(())
                }
            }
        };
    }

    macro_rules! define_load_int {
        ($name:literal, $type:ty, $leb_fn:ident) => {
            paste! {
                #[inline(always)]
                pub fn [<$name _load>](
                    iter: &mut impl Iterator<Item = u8>,
                ) -> Result<$type, LEB128Error> {
                    $leb_fn(iter).and_then(|result| {
                        <$type>::try_from(result).map_err(|_| {
                            LEB128Error("Result is not target type".to_string())
                        })
                    })
                }
                #[inline(always)]
                pub fn [<$name _load8_s>](
                    iter: &mut impl Iterator<Item = u8>,
                ) -> Result<$type, LEB128Error> {
                    let value: i8 = $leb_fn(iter)? as i8;
                    Ok(value as $type)
                }
                #[inline(always)]
                pub fn [<$name _load8_u>](
                    iter: &mut impl Iterator<Item = u8>,
                ) -> Result<$type, LEB128Error> {
                    let value: u8 = $leb_fn(iter)? as u8;
                    Ok(value as $type)
                }
                #[inline(always)]
                pub fn [<$name _load16_s>](
                    iter: &mut impl Iterator<Item = u8>,
                ) -> Result<$type, LEB128Error> {
                    let value: i16 = $leb_fn(iter)? as i16;
                    Ok(value as $type)
                }
                #[inline(always)]
                pub fn [<$name _load16_u>](
                    iter: &mut impl Iterator<Item = u8>,
                ) -> Result<$type, LEB128Error> {
                    let value: u16 = $leb_fn(iter)? as u16;
                    Ok(value as $type)
                }
                #[inline(always)]
                pub fn [<$name _load32_s>](
                    iter: &mut impl Iterator<Item = u8>,
                ) -> Result<$type, LEB128Error> {
                    let value: i32 = $leb_fn(iter)? as i32;
                    Ok(value as $type)
                }
                #[inline(always)]
                pub fn [<$name _load32_u>](
                    iter: &mut impl Iterator<Item = u8>,
                ) -> Result<$type, LEB128Error> {
                    let value: u32 = $leb_fn(iter)? as u32;
                    Ok(value as $type)
                }
            }
        };
    }

    macro_rules! define_load_float {
        ($name:literal, $type:ty) => {
            paste! {
                #[inline(always)]
                pub fn [<$name _load>](
                    iter: &mut impl Iterator<Item = u8>,
                ) -> Result<$type, LEB128Error> {
                    read_leb128_u(iter).map(|result| result as $type)
                }
            }
        };
    }

    define_load_int!("i32", i32, read_leb128_s);
    define_load_int!("i64", i64, read_leb128_s);
    define_load_float!("f32", f32);
    define_load_float!("f64", f64);
}
