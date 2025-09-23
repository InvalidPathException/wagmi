use crate::error::Error::*;
use crate::error::*;

#[inline]
pub fn safe_read_leb128<T>(bytes: &[u8], pc: &mut usize, bits: u8) -> Result<T, Error>
where T: TryFrom<u64> {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    let mut end = *pc;
    let mut byte: u8;
    unsafe {
        loop {
            if end >= bytes.len() { return Err(Malformed(UNEXPECTED_END)); }
            byte = *bytes.get_unchecked(end);
            end += 1;
            result |= ((byte & 0x7f) as u64) << shift;
            if byte & 0x80 == 0 { break; }
            shift += 7;
        }
    }
    let consumed = end - *pc;
    if consumed > (bits as usize).div_ceil(7) { return Err(Malformed(INT_TOO_LONG)); }

    // Only bits=1 and bits=32 are used
    if (bits == 1 && result > 1) || (bits == 32 && result > 0xFFFFFFFF) { return Err(Malformed(INT_TOO_LARGE)); }

    if consumed > 1 {
        let used = (consumed - 1) * 7;
        if used < bits as usize {
            let rem = bits as usize - used;
            if rem < 32 && (bytes[end - 1] as u32) >> rem != 0 { return Err(Malformed(INT_TOO_LARGE)); }
        }
    }
    *pc = end;
    Ok(T::try_from(result).ok().unwrap())
}

#[inline]
pub fn safe_read_sleb128<T>(bytes: &[u8], pc: &mut usize, bits: u8) -> Result<T, Error>
where T: TryFrom<i64> {
    let mut result: i64 = 0;
    let mut shift: u32 = 0;
    let mut end = *pc;
    let mut byte: u8;
    unsafe {
        loop {
            if end >= bytes.len() { return Err(Malformed(UNEXPECTED_END)); }
            byte = *bytes.get_unchecked(end);
            end += 1;
            if shift < 63 {
                result |= ((byte & 0x7f) as i64) << shift;
            }
            shift = (shift + 7).min(63);
            if byte & 0x80 == 0 { break; }
        }
    }
    if shift < 64 && (byte & 0x40) != 0 {
        result |= (!0i64).checked_shl(shift).unwrap_or(!0i64);
    }
    let consumed = end - *pc;

    match bits { // Only bits=32, 33, 64 are used
        32 | 33 => {
            const MIN_I32: i128 = -(1i128 << 31);
            const MAX_I32: i128 = (1i128 << 31) - 1;
            if (result as i128) < MIN_I32 || (result as i128) > MAX_I32 { return Err(Malformed(INT_TOO_LARGE)); }
        }
        64 => {} // Already i64
        _ => unreachable!()
    }

    if consumed > (bits as usize).div_ceil(7) { return Err(Malformed(INT_TOO_LONG)); }
    if consumed >= 1 {
        let last = bytes[end - 1];
        if ((last != 0 && last != 127) as usize + (consumed - 1) * 7) >= bits as usize {
            return Err(Malformed(INT_TOO_LARGE));
        }
    }
    *pc = end;
    Ok(T::try_from(result).ok().unwrap())
}

#[inline(always)]
pub fn read_leb128<T>(bytes: &[u8], pc: &mut usize) -> Result<T, Error>
where T: TryFrom<u64> {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    let mut byte: u8;
    unsafe {
        loop {
            byte = *bytes.get_unchecked(*pc);
            *pc += 1;
            result |= ((byte & 0x7f) as u64) << shift;
            if byte & 0x80 == 0 {
                return Ok(T::try_from(result).ok().unwrap_unchecked());
            }
            shift += 7;
        }
    }
}

#[inline(always)]
pub fn read_sleb128<T>(bytes: &[u8], pc: &mut usize) -> Result<T, Error>
where T: TryFrom<i64> {
    let mut result: i64 = 0;
    let mut shift: u32 = 0;
    let mut byte: u8;
    unsafe {
        loop {
            byte = *bytes.get_unchecked(*pc);
            *pc += 1;
            if shift < 63 {
                result |= ((byte & 0x7f) as i64) << shift;
            }
            shift = (shift + 7).min(63);
            if byte & 0x80 == 0 { break; }
        }
        if shift < 64 && (byte & 0x40) != 0 {
            result |= (!0i64) << shift;
        }
        Ok(T::try_from(result).ok().unwrap_unchecked())
    }
}
