# First native comparison — not an acceptance result

Same release binary from commit `0b4adc1`, unprofiled, full original Ray scene, four viewports. Fresh app processes; CPU edge batch off then on. Original source SHA256 `84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935` remained unchanged. Five committed GPU objects/thirteen parts; no reported GPU error or recovered recipes.

| Mode | Realistic ready pose times/s | Wireframe ready pose times/s |
| --- | ---: | ---: |
| Batch off | 59.0726 | 91.2872 |
| Batch on | 61.9951 | 91.6790 |

These count distinct ready submitted pose times in moving-clip windows, not independent per-frame vertex fingerprints. Realistic minimum active one-second window was 55.9523 off and 58.7088 on, so this does **not** establish sustained ≥60 fps. One ordered pair also does not establish repeatability. Default promotion remains pending repeated native comparison and reliability checks.

All four frame39 viewport crops are byte-exact in RGB between modes. Scene node records, selection, frame and source bytes were preserved. See native-pixel-parity.json, native-*-1.jsonl and native-*-frame39.png. The top/side viewports and perspective material output are all included; only surrounding UI is excluded from the pixel comparison.

The running test app may carry the explicit experimental flag; this is not a default-on production change. Native queue counters for batch-on still need measurement. No public push was performed.
