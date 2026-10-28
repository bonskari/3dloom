# First native comparison — not an acceptance result

Same release binary from commit `0b4adc1`, unprofiled, full original Ray scene, four viewports. Fresh app processes; CPU edge batch off then on. Original source SHA256 `84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935` remained unchanged. Five committed GPU objects/thirteen parts; no reported GPU error or recovered recipes.

| Mode | Realistic ready pose times/s | Wireframe ready pose times/s |
| --- | ---: | ---: |
| Batch off | 59.0726 | 91.2872 |
| Batch on | 61.9951 | 91.6790 |

These count distinct ready submitted pose times in moving-clip windows, not independent per-frame vertex fingerprints. Realistic minimum active one-second window was 55.9523 off and 58.7088 on, so this does **not** establish sustained ≥60 fps. One ordered pair also does not establish repeatability. Default promotion remains pending repeated native comparison and reliability checks.

All four frame39 viewport crops are byte-exact in RGB between modes. Scene node records, selection, frame and source bytes were preserved. See native-pixel-parity.json, native-*-1.jsonl and native-*-frame39.png. The top/side viewports and perspective material output are all included; only surrounding UI is excluded from the pixel comparison.

The running test app may carry the explicit experimental flag; this is not a default-on production change. Native queue counters are recorded below. No public push was performed.

## Reversed second pair

The same unprofiled release binary was relaunched fresh with batch on, then fresh with batch off. Realistic: on61.5275 / off60.2533 ready pose times/s. Wireframe: on92.6462 / off90.2848. Minimum Realistic active windows: on58.5305 / off58.1655. All per-run node/source preservation checks passed. The improvement has the same direction in both pairs, but sustained60 remains unproven and no default promotion has been made. Raw second-pair files: native-on-2.jsonl and native-off-2.jsonl.

## Actual queue calls

A separate profiled native run measured302 world-geometry updates (139222 per-mesh scopes /461 nodes). Its nonempty existing-buffer counter reported302 shared CPU-edge writes and323028864bytes, exactly **one write and1069632bytes per pose**. The baseline measured450writes and the same1069632bytes per pose. Allocation growth is outside this counter. Per-mesh render.upload.queue timing excludes the new shared upload and is not a total-upload comparison. Profile instrumentation was disabled again afterward; the app was restored to default-off batch mode at frame39. See native-on-profile.log and native-on-counter-summary.json.

## Fifteen-minute reliability gate

The explicit batch-on, unprofiled old candidate binary completed30×30-second samples with5committed objects/13parts, no reported pose errors or recipe recoveries. All original node records, undo history and source bytes were restored. A real271346304-byte automatic backup completed after playback began and its scene JSON matched the source scene. See native-stability-15m.jsonl and stability.py. Concurrent development correctness builds/tests were allowed; observed rates include clip tails and wrap, so they are reliability fields rather than quiet ≥60fps acceptance. This verifies batching reliability, not yet the separate fused-position candidate.
