# Per-pass GPU diagnostic: old seven-pass cost attribution

This diagnostic uses the UNCHANGED old seven-pass prototype measured in
`../provisional-cost/`; it does not measure the finite-intermediate eight-pass
revision. Actual four poses/all 13 current Ray parts, same immutable input 84fe...
f935. No CPU build or other root job was active. No CPU encoding/completion
clocks were collected in this diagnostic.

Each global pass across all 13 part bindings has encoder GPU timestamps immediately
before and after it. Four warmup iterations were excluded, followed by 32 samples
per mode/pose, alternating AB/BA. Query resolution, mapping and exact output
checks are outside timestamped intervals. Source/preparation/recipe/pipeline
construction and F64 acceptance remain outside timing.

Means across the four poses, GPU milliseconds per thirteen-part batch:

| Pass | Actual original five-pass batch | Old provisional seven-pass batch |
| --- | ---: | ---: |
| Guard | 0.024634 | 0.023624 |
| Derive | 0.011755 | 0.011483 |
| Evaluate | 0.129047 | 0.130345 |
| Risk | absent | 2.689294 |
| Float64 refinement | absent | 0.988350 |
| Epilogue | 0.007871 | 0.007839 |
| Pack | 0.033205 | 0.061718 |

**Risk is the measured dominant added pass**, about 73% of the combined risk+
Float64 span. Refinement is independently nearly 1 ms, so optimizing only risk
is not already enough to establish an acceptable end-to-end budget. The reason
for the exact scheduling/latency cost has not been isolated further; no hardware
or cache cause is invented. The old original evaluation remains about 0.13 ms.

All post-run output bytes matched their separately validated values; the test
completed once in 8.54 seconds. Per-pass query instrumentation differs from the
two-timestamp whole-span benchmark, so the sum of these intervals need not equal
that earlier span exactly. This is cost attribution, not a native FPS result.

`diagnostic.log` and `samples.json` retain all samples' aggregated pass values.
Apply `diagnostic.patch` after the cost harness patch to reproduce the host
instrumentation. No production math or backend selection was changed. A faster
algorithm must preserve current attributes/groups and pass independent-quality,
unchanged-byte and new cost gates; captured/CPU answers are not a substitute.
