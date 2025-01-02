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
    
    #[derive(Debug)]
    pub struct WasmMemoryError(String);

    impl fmt::Display for WasmMemoryError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl Error for WasmMemoryError {}

    #[macro_export]
    macro_rules! memory_load {
        ($stack:expr, $memory:expr, $wasm_ty:ident, $load_fn:expr, $offset:expr) => {{
            let base_addr = $stack
                .pop()
                .expect("Stack underflow when popping load address")
                .to_i32()
                .expect("Expected i32 memory address") as usize;

            let effective_addr = base_addr + $offset as usize;

            let value = $load_fn(&$memory, effective_addr)
                .expect("Failed to load from memory");

            $stack.push(WasmValue::$wasm_ty(value));
        }};
    }

    #[macro_export]
    macro_rules! memory_store {
        ($stack:expr, $memory:expr, $wasm_ty:ident, $store_fn:expr, $offset:expr) => {{
            let raw_value = $stack
                .pop()
                .expect("Stack underflow when popping store value");

            let base_addr = $stack
                .pop()
                .expect("Stack underflow when popping store address")
                .to_i32()
                .expect("Expected i32 memory address") as usize;

            let effective_addr = base_addr + $offset as usize;

            let typed_value = match raw_value {
                WasmValue::$wasm_ty(v) => v,
                _ => panic!(
                    "Expected a {} for store, got {:?}",
                    stringify!($wasm_ty),
                    raw_value
                ),
            };

            $store_fn(&mut $memory, effective_addr, typed_value)
                .expect("Failed to store in memory");
        }};
    }


    macro_rules! define_load_fn {
        ($name:ident, $type:ty, $num_bytes:expr) => {
            paste! {
                #[inline(always)]
                pub fn [<$name _load>](
                    mem: &[u8],
                    addr: usize
                ) -> Result<$type, WasmMemoryError> {
                    if addr + $num_bytes > mem.len() {
                        return Err(WasmMemoryError(
                            concat!(stringify!($name), ".load out of bounds").to_string()
                        ));
                    }
                    let bytes = &mem[addr..addr + $num_bytes];
                    Ok(<$type>::from_le_bytes(bytes.try_into().unwrap()))
                }
            }
        };
    }

    macro_rules! define_partial_load_fn {
        ($fn_name:ident, $ret_type:ty, $inner_type:ty, $num_bytes:expr, $extend:expr) => {
            paste! {
                #[inline(always)]
                pub fn [<$fn_name>](
                    mem: &[u8],
                    addr: usize
                ) -> Result<$ret_type, WasmMemoryError>
                {
                    if addr + $num_bytes > mem.len() {
                        return Err(WasmMemoryError(
                            concat!(stringify!($fn_name), " out of bounds").to_string()
                        ));
                    }
                    let mut buf = [0u8; $num_bytes];
                    buf.copy_from_slice(&mem[addr..addr + $num_bytes]);

                    Ok(($extend)(<$inner_type>::from_le_bytes(buf)) as $ret_type)
                }
            }
        };
    }

    macro_rules! define_store_fn {
        ($name:ident, $type:ty, $num_bytes:expr) => {
            paste! {
                #[inline(always)]
                pub fn [<$name _store>](
                    mem: &mut [u8],
                    addr: usize,
                    value: $type
                ) -> Result<(), WasmMemoryError> {
                    if addr + $num_bytes > mem.len() {
                        return Err(WasmMemoryError(
                            concat!(stringify!($name), ".store out of bounds").to_string()
                        ));
                    }
                    let bytes = value.to_le_bytes();
                    mem[addr..addr + $num_bytes].copy_from_slice(&bytes);
                    Ok(())
                }
            }
        };
    }

    macro_rules! define_partial_store_fn {
        ($fn_name:ident, $type:ty, $num_bytes:expr) => {
            paste! {
                #[inline(always)]
                pub fn [<$fn_name>](
                    mem: &mut [u8],
                    addr: usize,
                    value: $type
                ) -> Result<(), WasmMemoryError> {
                    if addr + $num_bytes > mem.len() {
                        return Err(WasmMemoryError(
                            concat!(stringify!($fn_name), " out of bounds").to_string()
                        ));
                    }
                    let full_bytes = value.to_le_bytes();
                    mem[addr..addr + $num_bytes]
                        .copy_from_slice(&full_bytes[..$num_bytes]);
                    Ok(())
                }
            }
        };
    }

    define_load_fn!(i32, i32, 4);
    define_load_fn!(i64, i64, 8);
    define_load_fn!(f32, f32, 4);
    define_load_fn!(f64, f64, 8);
    define_store_fn!(i32, i32, 4);
    define_store_fn!(i64, i64, 8);
    define_store_fn!(f32, f32, 4);
    define_store_fn!(f64, f64, 8);
    define_partial_load_fn!(i32_load8_s, i32, i8, 1, |v: i8| v as i32);
    define_partial_load_fn!(i32_load8_u, i32, u8, 1, |v: u8| v as i32);
    define_partial_load_fn!(i32_load16_s, i32, i16, 2, |v: i16| v as i32);
    define_partial_load_fn!(i32_load16_u, i32, u16, 2, |v: u16| v as i32);
    define_partial_load_fn!(i64_load8_s, i64, i8, 1, |v: i8| v as i64);
    define_partial_load_fn!(i64_load8_u, i64, u8, 1, |v: u8| v as i64);
    define_partial_load_fn!(i64_load16_s, i64, i16, 2, |v: i16| v as i64);
    define_partial_load_fn!(i64_load16_u, i64, u16, 2, |v: u16| v as i64);
    define_partial_load_fn!(i64_load32_s, i64, i32, 4, |v: i32| v as i64);
    define_partial_load_fn!(i64_load32_u, i64, u32, 4, |v: u32| v as i64);
    define_partial_store_fn!(i32_store8, i32, 1);
    define_partial_store_fn!(i32_store16, i32, 2);
    define_partial_store_fn!(i64_store8, i64, 1);
    define_partial_store_fn!(i64_store16, i64, 2);
    define_partial_store_fn!(i64_store32, i64, 4);

    #[inline(always)]
    pub fn read_memarg(iter: &mut &[u8]) -> (u32, u32) {
        let align = read_leb128_u(iter).expect("Failed to read align as LEB128");
        let offset = read_leb128_u(iter).expect("Failed to read offset as LEB128");
        (align as u32, offset as u32)
    }
}
