This file documents the coremark bench results to keep track of performance improvements over time.
- ac6b813: avg ≈ 500 (introduced benchmarking without criterion)
- 5f4af0c: avg = 529.773949, n = 20 (improved leb128 handling with unsafe)
    - we try to avoid unsafe code in module/validator/instance directly
- 899f989: avg = 810.367084, n = 20 (improve leb128 handling, move sidetable to using arrays, label error creation as cold paths)
    - rust hashmaps are apparently very bad, this might be because of our memory access pattern, when we interpret the control op codes (corresponding to side table entries) they are kind of accessed sequentially, so even btreemap yields better results
    - current design: two-level indirection (one array covering all possible code indices and directing them to a densely packed side table)
    - we also try to not use nightly features... making error creation cold path is a really elegant solution in this regard
    - repr(C) for the SideTableEntry struct caused mysterious improvements, not sure if it is a fluke
- cc02503: avg = 855.71814, n = 20 (remove defensive malformed check in main loop)
    - since the module is already validated at run time, there is no reason for the check to exist, it was a remnant of early development phase that lacked proper handling for some malformed modules
- current: no significant difference

On nightly, the performance is slightly better (sometimes reaching 900)

Next step: use direct threading to improve branch prediction 

Hardware Overview:
- Model Name: MacBook Pro
- Model Identifier: Mac16,8
- Model Number: MX2H3LL/A
- Chip: Apple M4 Pro
- Total Number of Cores: 12 (8 performance and 4 efficiency)
- Memory: 24 GB

Performance of other Rust-based interpreters:
wasmi: ~1700
tinywasm: ~630

Goal:
We expect/hope to reach ~1200 after threaded dispatch implementation. It seems like Ben Titzer only reached performance comparable to production-ready, optimizing interpreters through manually crafted assembly code for hot paths. 

Higher performance may not be pursued after the point and instead I might focus on adding more instructions to achieve Wasm 2.0 spec parity (should be easy with AI).