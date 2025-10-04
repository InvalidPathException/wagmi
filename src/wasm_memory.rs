use crate::error::OOB_MEMORY_ACCESS;

macro_rules! impl_unsigned {
    ($type:ty, $size:literal, $load_name:ident, $store_name:ident) => {
        #[inline(always)]
        pub fn $load_name(&self, ptr: u32, offset: u32) -> Result<$type, &'static str> {
            let addr = (ptr as usize).checked_add(offset as usize).ok_or(OOB_MEMORY_ACCESS)?;
            if addr.saturating_add($size) > self.data.len() { return Err(OOB_MEMORY_ACCESS); }
            unsafe { Ok((self.data.as_ptr().add(addr) as *const $type).read_unaligned()) }
        }
        #[inline(always)]
        pub fn $store_name(&mut self, ptr: u32, offset: u32, v: $type) -> Result<(), &'static str> {
            let addr = (ptr as usize).checked_add(offset as usize).ok_or(OOB_MEMORY_ACCESS)?;
            if addr.saturating_add($size) > self.data.len() { return Err(OOB_MEMORY_ACCESS); }
            unsafe { (self.data.as_mut_ptr().add(addr) as *mut $type).write_unaligned(v); }
            Ok(())
        }
    };
}

macro_rules! impl_signed_load {
    ($name:ident, $target:ty, $source:ident) => {
        #[inline(always)]
        pub fn $name(&self, ptr: u32, offset: u32) -> Result<$target, &'static str> {
            Ok(self.$source(ptr, offset)? as $target)
        }
    };
}

pub struct WasmMemory {
    data: Vec<u8>,
    current: u32,
    maximum: u32,
}

impl WasmMemory {
    pub const MAX_PAGES: u32 = 65536;
    pub const PAGE_SIZE: u32 = 65536;

    pub fn new(initial: u32, maximum: u32) -> Self {
        let maximum = maximum.min(Self::MAX_PAGES);
        let data = vec![0; (initial as usize) * (Self::PAGE_SIZE as usize)];
        Self { data, current: initial, maximum }
    }

    pub fn size(&self) -> u32 { self.current }
    pub fn max(&self) -> u32 { self.maximum }

    pub fn grow(&mut self, delta: u32) -> u32 {
        if delta == 0 { return self.current; }
        if delta > self.maximum.saturating_sub(self.current) { return u32::MAX; }
        let old = self.current;
        self.current += delta;
        self.data.resize((self.current as usize) * (Self::PAGE_SIZE as usize), 0);
        old
    }

    impl_unsigned!(u8,  1, load_u8, store_u8);    impl_unsigned!(u16, 2, load_u16, store_u16);
    impl_unsigned!(u32, 4, load_u32, store_u32);  impl_unsigned!(u64, 8, load_u64, store_u64);
    impl_signed_load!(load_i8,  i8,  load_u8);    impl_signed_load!(load_i16, i16, load_u16);
    impl_signed_load!(load_i32, i32, load_u32);   impl_signed_load!(load_i64, i64, load_u64);

    #[inline(always)]
    pub fn load_f32(&self, ptr: u32, offset: u32) -> Result<f32, &'static str> {
        Ok(f32::from_bits(self.load_u32(ptr, offset)?))
    }
    #[inline(always)]
    pub fn store_f32(&mut self, ptr: u32, offset: u32, v: f32) -> Result<(), &'static str> {
        self.store_u32(ptr, offset, v.to_bits())
    }
    #[inline(always)]
    pub fn load_f64(&self, ptr: u32, offset: u32) -> Result<f64, &'static str> {
        Ok(f64::from_bits(self.load_u64(ptr, offset)?))
    }
    #[inline(always)]
    pub fn store_f64(&mut self, ptr: u32, offset: u32, v: f64) -> Result<(), &'static str> {
        self.store_u64(ptr, offset, v.to_bits())
    }
    #[inline(always)]
    pub fn write_bytes(&mut self, offset: u32, bytes: &[u8]) -> Result<(), &'static str> {
        let start = offset as usize;
        let end = start.checked_add(bytes.len()).ok_or(OOB_MEMORY_ACCESS)?;
        if end > self.data.len() { return Err(OOB_MEMORY_ACCESS); }
        self.data[start..end].copy_from_slice(bytes);
        Ok(())
    }
}