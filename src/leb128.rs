use crate::spec::{malformed, Error};
use crate::error_msg;

#[inline(always)]
fn read_leb128_u64(bytes: &[u8], mut pos: usize) -> Result<(u64, usize), Error> {
    let mut result = 0u64;
    let mut shift = 0;
    loop {
        let byte = *bytes.get(pos).ok_or_else(|| Error::Malformed(error_msg::UNEXPECTED_END))?;
        pos += 1;
        result |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 { return Ok((result, pos)); }
        shift += 7;
    }
}

#[inline(always)]
fn read_leb128_i64(bytes: &[u8], mut pos: usize) -> Result<(i64, usize), Error> {
    let mut result = 0i64;
    let mut shift = 0;
    let mut byte;
    loop {
        byte = *bytes.get(pos).ok_or_else(|| Error::Malformed(error_msg::UNEXPECTED_END.into()))?;
        pos += 1;
        if shift < 63 {
            result |= ((byte & 0x7f) as i64) << shift;
        }
        shift = (shift + 7).min(63);
        if byte & 0x80 == 0 { break; }
    }
    if shift < 64 && (byte & 0x40) != 0 {
        result |= (!0i64).checked_shl(shift).unwrap_or(!0i64);
    }
    Ok((result, pos))
}

#[inline]
pub fn safe_read_leb128<T>(bytes: &[u8], pc: &mut usize, bits: u8) -> Result<T, Error>
where T: TryFrom<u64> {
    let (result, end) = read_leb128_u64(bytes, *pc)?;
    let consumed = end - *pc;
    if consumed > (bits as usize + 6) / 7 { return malformed(error_msg::INTEGER_TOO_LONG); }

    // Only bits=1 and bits=32 are used
    if (bits == 1 && result > 1 ) || (bits == 32 && result > 0xFFFFFFFF) { return malformed(error_msg::INTEGER_TOO_LARGE); }

    if consumed > 1 {
        let used = (consumed - 1) * 7;
        if used < bits as usize {
            let rem = bits as usize - used;
            if rem < 32 && (bytes[end - 1] as u32) >> rem != 0 { return malformed(error_msg::INTEGER_TOO_LARGE); }
        }
    }
    *pc = end;
    Ok(T::try_from(result).ok().unwrap())
}

#[inline]
pub fn safe_read_sleb128<T>(bytes: &[u8], pc: &mut usize, bits: u8) -> Result<T, Error>
where T: TryFrom<i64> {
    let (result, end) = read_leb128_i64(bytes, *pc)?;
    let consumed = end - *pc;

    match bits { // Only bits=32, 33, 64 are used
        32 | 33 => {
            const MIN_I32: i128 = -(1i128 << 31);
            const MAX_I32: i128 = (1i128 << 31) - 1;
            if (result as i128) < MIN_I32 || (result as i128) > MAX_I32 { return malformed(error_msg::INTEGER_TOO_LARGE); }
        }
        64 => {} // Already i64
        _ => unreachable!()
    }

    if consumed > (bits as usize + 6) / 7 { return malformed(error_msg::INTEGER_TOO_LONG); }
    if consumed >= 1 {
        let last = bytes[end - 1];
        if ((last != 0 && last != 127) as usize + (consumed - 1) * 7) >= bits as usize {
            return malformed(error_msg::INTEGER_TOO_LARGE);
        }
    }
    *pc = end;
    Ok(T::try_from(result).ok().unwrap())
}

#[inline]
pub fn read_leb128<T>(bytes: &[u8], pc: &mut usize) -> Result<T, Error>
where T: TryFrom<u64> {
    let (val, end) = read_leb128_u64(bytes, *pc)?;
    *pc = end;
    Ok(T::try_from(val).ok().unwrap())
}

#[inline]
pub fn read_sleb128<T>(bytes: &[u8], pc: &mut usize) -> Result<T, Error>
where T: TryFrom<i64> {
    let (val, end) = read_leb128_i64(bytes, *pc)?;
    *pc = end;
    Ok(T::try_from(val).ok().unwrap())
}