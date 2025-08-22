#![deny(unsafe_code)]

pub mod spec;
mod leb128;

pub use leb128::*;

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
