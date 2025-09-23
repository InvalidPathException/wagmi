This file documents the coremark bench results to document performance improvements over time.
- ac6b813: avg ≈ 500 (introduced benchmarking without criterion)
- current: avg = 529.773949, n = 20 (improved leb128 handling with unsafe)
    - we try to avoid unsafe code in module/validator/instance directly


Hardware Overview:
- Model Name: MacBook Pro
- Model Identifier: Mac16,8
- Model Number: MX2H3LL/A
- Chip: Apple M4 Pro
- Total Number of Cores: 12 (8 performance and 4 efficiency)
- Memory: 24 GB