# WAGMI (WebAssembly General & Minimal Interpreter)
WAGMI is a minimalistic in-place WebAssembly interpreter written in Rust (WIP).

Original idea: https://www.cs.tufts.edu/comp/150FP/archive/ben-titzer/wasm-interp.pdf

The spec test suite in this repo is from https://github.com/WebAssembly/spec/releases/tag/list

The associated `wast2json` binary is of version `1.0.13 (1.0.14)`, you can find the Windows version here: https://github.com/WebAssembly/wabt/releases/tag/1.0.13

Newer tests/wast converter may not work since this project specifically targets the 1.0 core WebAssembly standard. There will not be active development beyond full 1.0 support.