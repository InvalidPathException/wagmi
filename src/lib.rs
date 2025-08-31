
#![allow(unsafe_code)]
pub mod wasm_memory;

#[deny(unsafe_code)]
pub mod module;
pub mod signature;
mod leb128;
mod byte_iter;
mod error;
mod validator;
mod instance;

pub use signature::{ValType, Signature};
pub use error::Error;
pub use module::Module;
pub use validator::Validator;
pub use wasm_memory::WasmMemory;
pub use instance::{Instance, WasmValue, WasmGlobal, WasmTable, RuntimeFunction, RuntimeType, ExportValue};
pub type Imports = std::collections::HashMap<String, std::collections::HashMap<String, ExportValue>>;

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
