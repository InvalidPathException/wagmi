#![deny(unsafe_code)]
pub mod module;
pub mod spec;
mod leb128;
mod byte_iter;
mod error;
mod validator;

pub use spec::{ValType, Signature};
pub use module::Module;

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
