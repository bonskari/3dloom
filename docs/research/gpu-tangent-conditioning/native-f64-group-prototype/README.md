# Condition-selected native Float64 tangent-group prototype

**Isolated research, not production promotion.** The prototype consumed current
GPU mesh attributes, not captured terms or CPU-reference tangents. The bounded
17-part-pose trial passed; general quality and runtime cost remain unverified.

## Input and run

The immutable snapshot is the same read-only
`/tmp/loom-conditioning-capture/input.loom` used by the prior stage isolation:

```
SHA256 84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935
```

It was copied/hashed BEFORE either experiment; this does not replace the
historical audit's missing input hash. Source was copied/instrumented only under
`/tmp/loom-f64-groups/src`. No production files were edited.

```bash
python3 /tmp/loom-f64-groups/build.py > /tmp/loom-f64-groups/build2.log 2>&1
LOOM_PERF_SCENE=/tmp/loom-conditioning-capture/input.loom /tmp/loom-f64-groups/tests actual_ray_hybrid_current_gpu_tangents_f64_diagnostics --ignored --nocapture --test-threads=1 > /tmp/loom-f64-groups/trial2.log 2>&1
```

Compile and trial both completed. Trial duration 5.32 seconds is an execution
observation, NOT a performance benchmark. Root's separate release build was
running; no native-FPS conclusion follows.

The script is host/cache-specific, matching the prior direct-rustc recipe.
`prototype.patch` recreates the isolated changes against source-base hashes in
`source-manifest.json`. The shaders are also archived directly for review.
`SHA256SUMS` identifies archived artifacts.

## Actual algorithm

The original f32 derive/evaluate pipelines remain on a separate baseline shader
module. Its active functions are unchanged; it has only two extra unused no-op
entry points to keep the same host dispatch schedule for the comparison.
A separate risk/refinement module contains the candidate arithmetic.

The risk pass reads CURRENT positions/UVs, baseline current derivatives and normal.
It recomputes derivative-subtraction condition D, projection condition P, and
weighted-sum cancellation K. It evaluates
`2^-24 * K * (D + P + contributing_count) >= 2e-5` and flags the entire group.
Unlike the earlier diagnostic, D/P conservatively include BOTH S/T directions.
Zero/underflowed denominators flag the group instead of suppressing geometry.
The existing Any flags and dynamic subgroup membership remain in the calculation.
This indicator is a tested heuristic, not a proved general WGSL error bound.

Only flagged groups enter native Float64 refinement. The kernel reloads actual
current represented P/N/UV, recomputes both derivatives, projection, dynamic
membership, current corner weights, ordered weighted sums and normalization.
It uses the separately gated range-reduced degree 41 angle polynomial; native
f64 atan2 is NOT used. There are no static node/frame/vertex IDs or saved tangent
values in the shader. Historical times select test cases only, never production
precision eligibility.

Current spaces are written before the existing degenerate epilogue and pack.
Handedness remains the validated structural group's orientation. No positions,
normals or UVs are replaced. Guard failures still reject candidate output.
No CPU final-tangent reference is an input; independent F64 math is a test oracle
used solely AFTER GPU readback to evaluate the result.

The isolated constructor requires the actual Device's SHADER_F64 feature and
nine storage bindings. It returns an error when unsupported; no ordinary backend
or application feature request was changed. A future integration must retain
full-quality ordinary rendering when precision hardware is unavailable.

## Bounded results and exact-byte assertions

`summary.json` lists every part-pose. Across 17 part-poses:

| Check | Observed result |
| --- | --- |
| All tested expanded vertices | 1,414,482 |
| Whole-group precision outputs | 102 |
| Original-path outputs | 1,414,380 |
| Maximum candidate–independent-F64 vector error | 4.6495482e-7 |
| Flagged group instances | 17 / 250,021 |
| Flags per part-pose | Exactly 1 |
| Subsequent preparation guard hits | 15 /15 |

All tested vertices passed the unchanged 2e-5 independent-F64 gate, with finite
vectors and exact handedness. This includes all 102 historical failing output
locations. All 1,414,380 nonflagged output records were compared byte-for-byte
across their entire 64-byte PackedVertex layout against the paired original path.
Flagged outputs additionally passed value-equality checks for positions/normals/UV/handedness. The subsequent full-trial source strengthens these to byte/bit checks, including signed zeros.
These are runtime assertions in the archived source, not approximate image checks.

The paired runtime assertions and counts are retained. The full raw baseline/
candidate output buffers and raw risk-index lists were not separately archived;
the test read the actual risk buffer to derive output ownership before asserting
byte equality. Do not present nonexistent stored buffer hashes as evidence. The
next full trial should additionally persist each actual flagged group-index list.

## Remaining gates and cost risks

- Full 317 poses/all 13 parts: independent F64, finite/handedness, unchanged P/N/UV,
  exact nonflagged bytes, whole-pose guards and retained actual risk lists.
- Synthetic zero/Any/degenerate, UV seam, boundary and nonmanifold cases.
- Actual-material image tests with positive controls after the numerical gate.
- End-to-end measured cost of risk pass, extra barriers/dispatch, sparse f64 work
  and integration, including unsupported-hardware behavior.

The risk pass currently recomputes weighted sums in a separate per-group pass to
avoid changing original f32 rounding. It duplicates work and serializes large
groups; the refinement pass also processes each flagged group in one invocation.
This is a correctness prototype, not a demonstrated production scheduling design.
No large-group truncation is allowed. Parallel per-corner scheduling would need
its own exact-output validation. Native f64 throughput and register pressure are
hardware-dependent; sparse selection does not establish a net gain by itself.

There was no full 317 run, no renderer promotion and no 60-FPS claim in this step.
