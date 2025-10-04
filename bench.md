This file documents the coremark bench results to document performance improvements over time.
- ac6b813: avg ≈ 500 (introduced benchmarking without criterion)
- 5f4af0c: avg = 529.773949, n = 20 (improved leb128 handling with unsafe)
    - we try to avoid unsafe code in module/validator/instance directly
- current: avg = 810.367084, n = 20 (use a dense offset table instead of hashmap, better leb128, and cold path hinting)
    - rust hashmaps are apparently very bad, this might be because of our memory access pattern, when we interpret the control op codes (corresponding to side table entries) they are kind of accessed sequentially, so even btreemap yields better results
    - current design: two-level indirection (one array covering all possible code indices and directing them to a densely packed side table)
    - we also try to not use nightly features... making error creation cold path is a really elegant solution in this regard
    - repr(C) for the SideTableEntry struct caused mysterious improvements, not sure if it is a fluke


Hardware Overview:
- Model Name: MacBook Pro
- Model Identifier: Mac16,8
- Model Number: MX2H3LL/A
- Chip: Apple M4 Pro
- Total Number of Cores: 12 (8 performance and 4 efficiency)
- Memory: 24 GB