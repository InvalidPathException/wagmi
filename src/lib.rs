#![allow(unsafe_code)]
pub mod wasm_memory;

#[deny(unsafe_code)]
pub mod module;
pub mod signature;
pub mod validator;
pub mod instance;

// Internal modules
mod leb128;
mod byte_iter;
mod error;

// Core types
pub use signature::{Signature, ValType};

// Runtime types
pub use instance::{ExportValue, Imports, Instance, RuntimeFunction, RuntimeType, WasmGlobal, WasmTable, WasmValue};

// Main API types
pub use module::Module;
pub use validator::Validator;
pub use wasm_memory::WasmMemory;

// Utility types
pub use error::Error;

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
