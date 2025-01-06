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
    ($stack:expr, $memory:expr, $type:ty, $num_bytes:expr, $extend:expr, $offset:expr) => {
        let base_addr = <i32>::try_from($stack.pop().expect("Stack underflow when popping load address"))
            .unwrap_or_else(|err| panic!("Expected i32 memory address: {}", err)) as usize;
        let effective_addr = base_addr + $offset as usize;
        if effective_addr + $num_bytes > $memory.len() {
            panic!("Memory load out of bounds");
        }

        let mut buf = [0u8; $num_bytes];
        buf.copy_from_slice(&$memory[effective_addr..effective_addr + $num_bytes]);

        let value = $extend(<$type>::from_le_bytes(buf));
        $stack.push(WasmValue::from(value));
    };
}

#[macro_export]
macro_rules! memory_store {
    ($stack:expr, $memory:expr, $type:ty, $mask:expr, $num_bytes:expr, $offset:expr) => {
        let val = <$type>::try_from($stack.pop().expect("Stack underflow when popping store value"))
            .unwrap_or_else(|err| panic!("Conversion error: {}", err));

        let base_addr = <i32>::try_from($stack.pop().expect("Stack underflow when popping store address"))
            .unwrap_or_else(|err| panic!("Expected i32 memory address: {}", err)) as usize;
        let effective_addr = base_addr + $offset as usize;
        if effective_addr + $num_bytes > $memory.len() {
            panic!("Memory store out of bounds");
        }

        let masked_val = (val as u64) & $mask;
        let bytes = masked_val.to_le_bytes();
        $memory[effective_addr..effective_addr + $num_bytes].copy_from_slice(&bytes[..$num_bytes]);
    };

    ($stack:expr, $memory:expr, $type:ty, $num_bytes:expr, $offset:expr) => {
        let val = <$type>::try_from($stack.pop().expect("Stack underflow when popping store value"))
            .unwrap_or_else(|err| panic!("Conversion error: {}", err));

        let base_addr = <i32>::try_from($stack.pop().expect("Stack underflow when popping store address"))
            .unwrap_or_else(|err| panic!("Expected i32 memory address: {}", err)) as usize;
        let effective_addr = base_addr + $offset as usize;
        if effective_addr + $num_bytes > $memory.len() {
            panic!("Memory store out of bounds");
        }

        let bytes = val.to_le_bytes();
        $memory[effective_addr..effective_addr + $num_bytes].copy_from_slice(&bytes[..$num_bytes]);
    };
}

#[inline(always)]
pub fn read_offset(iter: &mut &[u8]) -> u32 {
    read_leb128_u(iter).expect("Failed to read align as LEB128");
    read_leb128_u(iter).expect("Failed to read offset as LEB128") as u32
}
