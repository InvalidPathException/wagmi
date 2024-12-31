pub mod leb128 {
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

    // Pedantic LEB128 reader function, only accepts canonical LEB128
    fn read_leb128<I, R, F>(
        iter: &mut I,
        is_signed: bool,
        convert: F,
    ) -> Result<R, LEB128Error>
    where
        I: Iterator<Item = u8>,
        R: Copy,
        F: FnOnce(i64) -> Result<R, LEB128Error>,
    {
        let mut result: i64 = 0;
        let mut shift: u64 = 0;
        let mut bytes_read = 0;

        loop {
            let byte = iter.next().ok_or_else(|| LEB128Error("Premature end of input".to_string()))?;
            result |= ((byte & 0x7f) as i64) << shift;
            shift += 7;
            bytes_read += 1;

            // Only accepts canonical LEB128
            if bytes_read > 1 + (size_of::<R>() * 8 / 7) {
                return Err(LEB128Error("Too many bytes for given type".to_string()));
            }

            // Then check if we are done
            if (byte & 0x80) == 0 {
                break;
            }
        }

        if is_signed && shift < 64 && (result & (1 << (shift - 1))) != 0 {
            result |= -1i64 << shift;
        }

        convert(result)
    }

    pub fn read_signed<I, T>(iter: &mut I) -> Result<T, LEB128Error>
    where
        I: Iterator<Item = u8>,
        T: TryFrom<i64> + Copy,
    {
        read_leb128(iter, true, |result| {
            T::try_from(result).map_err(|_| LEB128Error("Result is not target type".to_string()))
        })
    }

    pub fn read_unsigned<I, T>(iter: &mut I) -> Result<T, LEB128Error>
    where
        I: Iterator<Item = u8>,
        T: TryFrom<u64> + Copy,
    {
        read_leb128(iter, false, |result| {
            let unsigned_result = result as u64;
            T::try_from(unsigned_result).map_err(|_| LEB128Error("Result is not target type".to_string()))
        })
    }
}
