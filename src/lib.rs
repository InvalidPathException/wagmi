#![allow(unsafe_code)]
pub mod wasm_memory;

pub mod instance;
#[deny(unsafe_code)]
pub mod module;
pub mod signature;
pub mod validator;

// Internal modules
mod error;
mod leb128;
mod opcodes;

// Core types
pub use signature::{Signature, ValType};

// Runtime types
pub use instance::{
    ExportValue, Imports, Instance, RuntimeFunction, WasmGlobal, WasmTable, WasmValue,
};
pub use signature::RuntimeSignature;

// Main API types
pub use module::Module;
pub use validator::Validator;
pub use wasm_memory::WasmMemory;

// Utility types
pub use error::Error;
