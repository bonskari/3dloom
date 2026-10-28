# Parallel GPU-pose input preparation — default off, native acceptance pending

Current rendering skins on the CPU, then uploads current world positions and computes normals/tangents on the GPU. This experiment parallelizes existing independent cached-runtime input preparation, not skinning/shader arithmetic. No geometry, material part or Skin weight is removed.

`LOOM_EXPERIMENTAL_PARALLEL_POSE_PREPARATION=1` defers warm-cache updates to the existing Bevy ComputeTaskPool. Cache construction stays in the existing serial cold path. Every job borrows an exclusive runtime plus its immutable current Mesh and descriptors. All jobs finish and errors are ordered by original input order before the unchanged global encoding/guard/commit path. Cache-only preparation may change on a failed pose, but committed render geometry/history is still gated by the original all-parts validation. No render target is published from a worker.

Unset/other values retain serial preparation. This is an internal experiment, not a completed performance fix. Do not enable by default or claim a native gain before same-binary unprofiled off/on measurements and complete-scene verification. The first scheduling experiment improved an isolated Skin benchmark without proving a native gain; it is not evidence for this different experiment.

The API RED is a missing-helper compile failure, not a reproduced rendering bug. The final real-device test covers ordinary and supported precise preparation, a later nonfinite source failure, both retained cache entries, successful retry, changed faceted descriptors and complete GPU-record byte equality against serial preparation. Source meshes remain unchanged. The current release parallel suite passes90tests/38ignored; the serial debug suite passes90/38 (before the final extra full-record assertion). The final focused real-device test and final release suite include that assertion.

Actual Ray with the experimental flag passed317integer/fractional poses, all5objects/13material parts, exact current CPU-history positions and AABB bits, resource retention and no final pose error. A separate six-sample precise renderer test checks batched-vs-serial GPU records. Printed sync_ms values are NOT quiet performance evidence: the older native app reliability run and other correctness work were active. Shaders, weights and tangent arithmetic are unchanged.

The earlier b4aca16 production-default15minute reliability/backup run completed separately; it does not certify this experiment. Native visual/input verification, production FPS headroom and experiment-specific long stability remain pending.

The shared benchmark now accepts an explicit fourth positional `1` for this experiment and asserts the actual environment flag. It records experimental_overrides_unset=false for that run. Omit that argument for the genuine unset default; a stray parallel flag then rejects. API addresses/preferences/credentials are never archived.

## Same-binary native comparison

Both ordered pairs used installedd8d3264, full original source, four viewports, fresh processes, identical native click/rotate/Undo/reload preparation and the Rotate tool. No compiler/probe jobs ran during these FPS measurements. Experimental/profile overrides were absent except the explicitly recorded parallel flag in on runs. All four viewport pixel crops were identical to the preceding b4 version in every verification, and all nodes/selection/frame/source data were restored.

| Order/config | Realistic average | Realistic minimum active window | Wireframe average |
|---|---:|---:|---:|
| off1 |61.3276|53.5260|89.7311|
| on1 |63.8768|60.8008|91.5623|
| on2 |63.8804|59.6128|91.5941|
| off2 |62.1434|55.7583|92.4024|

Realistic average improved in both comparisons. Wireframe improvement did not repeat. The second on run missed the retained all-active-window60 gate; do not enable by default or claim completed performance acceptance. No repeated testing until a lucky pass.

The counter already counts distinct ready submitted pose times, but the verifier divides deltas by client IPC round-trip wall time. It does not expose an independent timestamp at the event where a ready distinct submission is recorded. Next add/test that diagnostic timestamp and retain both clocks side by side, to quantify timing uncertainty rather than assume IPC explains the shortfall. Never change the pose-count/geometry-ready gates or silently replace archived results to manufacture a pass. No specific cause for the remaining lowest window is established.
