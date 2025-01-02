pub mod leb128 {
    use paste::paste;
    use std::error::Error;
    use std::fmt;

    #[derive(Debug)]
    pub struct LEB128Error(String);

    impl fmt::Display for LEB128Error {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl Error for LEB128Error {}

    #[inline(always)]
    pub(crate) fn read_leb128_s(slice: &mut &[u8]) -> Result<i64, LEB128Error> {
        let mut result: i64 = 0;
        let mut shift = 0;

        for (bytes_read, &byte) in slice.iter().enumerate() {
            result |= ((byte & 0x7f) as i64) << shift;
            shift += 7;

            if bytes_read >= 10 {
                return Err(LEB128Error("Too many bytes for LEB128".to_string()));
            }

            if (byte & 0x80) == 0 {
                *slice = &slice[bytes_read + 1..];
                if shift < 64 && (result & (1 << (shift - 1))) != 0 {
                    result |= -1i64 << shift;
                }
                return Ok(result);
            }
        }

        Err(LEB128Error("Premature end of input".to_string()))
    }

    #[inline(always)]
    pub(crate) fn read_leb128_u(slice: &mut &[u8]) -> Result<u64, LEB128Error> {
        let mut result: u64 = 0;
        let mut shift = 0;

        for (bytes_read, &byte) in slice.iter().enumerate() {
            result |= ((byte & 0x7f) as u64) << shift;
            shift += 7;

            if bytes_read >= 10 {
                return Err(LEB128Error("Too many bytes for LEB128".to_string()));
            }

            if (byte & 0x80) == 0 {
                *slice = &slice[bytes_read + 1..];
                return Ok(result);
            }
        }

        Err(LEB128Error("Premature end of input".to_string()))
    }

    #[macro_export]
    macro_rules! memory_load {
        ($stack:expr, $memory:expr, $type:ident, $leb_fn:expr) => {{
            let addr = $stack
                .pop()
                .expect("Stack underflow")
                .to_i32()
                .expect("Expected i32 memory index") as usize;

            let mut memory_slice = $memory.get(addr..).expect("Memory access out of bounds");

            let value = $leb_fn(&mut memory_slice).expect("Failed to decode value");

            $stack.push(WasmValue::$type(value));
        }};
    }

    macro_rules! define_load_int {
        ($name:literal, $type:ty, $leb_fn:ident) => {
            paste::paste! {
                #[inline(always)]
                pub fn [<$name _load>](
                    slice: &mut &[u8],
                ) -> Result<$type, LEB128Error> {
                    $leb_fn(slice).and_then(|result| {
                        <$type>::try_from(result).map_err(|_| {
                            LEB128Error("Result is not target type".to_string())
                        })
                    })
                }
                #[inline(always)]
                pub fn [<$name _load8_s>](
                    slice: &mut &[u8],
                ) -> Result<$type, LEB128Error> {
                    let value: i8 = $leb_fn(slice)? as i8;
                    Ok(value as $type)
                }
                #[inline(always)]
                pub fn [<$name _load8_u>](
                    slice: &mut &[u8],
                ) -> Result<$type, LEB128Error> {
                    let value: u8 = $leb_fn(slice)? as u8;
                    Ok(value as $type)
                }
                #[inline(always)]
                pub fn [<$name _load16_s>](
                    slice: &mut &[u8],
                ) -> Result<$type, LEB128Error> {
                    let value: i16 = $leb_fn(slice)? as i16;
                    Ok(value as $type)
                }
                #[inline(always)]
                pub fn [<$name _load16_u>](
                    slice: &mut &[u8],
                ) -> Result<$type, LEB128Error> {
                    let value: u16 = $leb_fn(slice)? as u16;
                    Ok(value as $type)
                }
                #[inline(always)]
                pub fn [<$name _load32_s>](
                    slice: &mut &[u8],
                ) -> Result<$type, LEB128Error> {
                    let value: i32 = $leb_fn(slice)? as i32;
                    Ok(value as $type)
                }
                #[inline(always)]
                pub fn [<$name _load32_u>](
                    slice: &mut &[u8],
                ) -> Result<$type, LEB128Error> {
                    let value: u32 = $leb_fn(slice)? as u32;
                    Ok(value as $type)
                }
            }
        };
    }

    macro_rules! define_load_float {
        ($name:literal, $type:ty) => {
            paste::paste! {
                #[inline(always)]
                pub fn [<$name _load>](
                    slice: &mut &[u8],
                ) -> Result<$type, LEB128Error> {
                    read_leb128_u(slice).map(|result| result as $type)
                }
            }
        };
    }

    define_load_int!("i32", i32, read_leb128_s);
    define_load_int!("i64", i64, read_leb128_s);
    define_load_float!("f32", f32);
    define_load_float!("f64", f64);
}
