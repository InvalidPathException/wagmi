# WAGMI (WebAssembly General & Minimal Interpreter)

WAGMI is a minimalistic in-place WebAssembly interpreter written in Rust, with full support for the [WebAssembly 1.0 core standard](https://www.w3.org/TR/wasm-core-1/).

Based on the [original paper by Ben L. Titzer](https://www.cs.tufts.edu/comp/150FP/archive/ben-titzer/wasm-interp.pdf).

## Project Structure

- `/src` - Core interpreter implementation
  - `module.rs` - WebAssembly module parsing and structure
  - `validator.rs` - Module validation logic
  - `instance.rs` - Runtime instance and execution engine
  - `wasm_memory.rs` - Linear memory management
  - `signature.rs` - Function signature handling
  - `leb128.rs` - LEB128 encoding/decoding utilities
  - `byte_iter.rs` - Byte stream iteration helpers
  - `error.rs` - Error types and handling
  - `lib.rs` - Library entry point
  - `/bin` - Example usage demonstrations
- `/tests` - Tests
  - `/spec_tests.rs` - Test runner
  - `/core` - WebAssembly spec test suite
- `/tools` - WebAssembly text format to bytecode translation tools 
- `/docs` - More detailed documentation

## Testing

The spec test suite is from the [WebAssembly specification repository](https://github.com/WebAssembly/spec/releases/tag/list). The `wast2json` and `wat2wasm` binaries used are version 1.0.13 (1.0.14) [Windows version available here](https://github.com/WebAssembly/wabt/releases/tag/1.0.15).

**Note:** This project specifically targets the WebAssembly 1.0 core standard. Newer test converters may not be compatible. There is no plan for development beyond 1.0 support.