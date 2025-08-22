#![deny(unsafe_code)]

mod spec;
mod leb128;
mod byte_iter;
mod error_msg;

// Debug macro that only prints when wasm_debug feature is enabled
#[cfg(feature = "wasm_debug")]
macro_rules! debug_println {
    ($($arg:tt)*) => {
        eprintln!($($arg)*);
    };
}

#[cfg(not(feature = "wasm_debug"))]
macro_rules! debug_println {
    ($($arg:tt)*) => {};
}

pub(crate) use debug_println;
