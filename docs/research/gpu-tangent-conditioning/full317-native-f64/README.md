# Full 317 native Float64 current-group numerical gate

**Numerical quality gate passed. No production promotion or FPS claim.**
This is the isolated condition-selected prototype, with an unchanged fast shader
and separate current-attribute risk / native Float64 refinement passes. It does
not consume CPU final tangents, an independent reference, or captured contribution
values as GPU inputs. See `integration-design.md` for the proposed production
boundary and remaining gates/cost risks.

## Provenance and command

The input is the immutable read-only snapshot created BEFORE the bounded capture:

```
/tmp/loom-conditioning-capture/input.loom
SHA256 84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935
```

This is new-run provenance, not a substitute for the earlier audit's missing
historical input hash. The 259 MB local snapshot is not committed. Source was
copied and modified solely under `/tmp/loom-f64-full 317/src`; production was not
edited. The archived patch and source-base hashes identify those isolated changes.

```bash
python3 /tmp/loom-f64-full 317/build.py > /tmp/loom-f64-full 317/build.log 2>&1
LOOM_PERF_SCENE=/tmp/loom-conditioning-capture/input.loom /tmp/loom-f64-full 317/tests actual_ray_hybrid_current_gpu_tangents_f64_diagnostics --ignored --nocapture --test-threads=1 > /tmp/loom-f64-full 317/trial.log 2>&1
```

Direct-rustc compilation completed successfully. The same run session completed
once without restart in 352.37 seconds: one ignored diagnostic test passed, zero
failed. This execution duration is not a runtime performance benchmark. Other
CPU correctness work was permitted concurrently; no timing conclusions follow.

All 317 f32 times use `149.0 * frame / 316.0`, frames 0 through 316. All five Skin
objects and all 13 material parts were tested. The existing audit producer uses
all-influence GPU skinning and current GPU SG-normal packing on the real scene,
then tests tangent math on those SAME represented current P/N/UV attributes.
This is not a live renderer.sync / CPU-position-producer integration test.

## Results

| Check | Observed result |
| --- | --- |
| Part-poses | 4121 |
| Expanded vertices tested | 105,868,173 |
| Unchanged original-path records | 105,850,547 |
| Whole-group precision outputs | 17,626 |
| Maximum independent-F64 tangent vector error | 3.3343802863e-6 |
| Required vector gate | <=2e-5 |
| Flagged group instances | 2841 / 20,077,195 |
| Structural first/previous guard hits | 4108 / 4108 each |

Every candidate tangent was finite and passed independent-F64 handedness and
vector checks. The worst remaining error was unflagged Face001/frame 53/part 0;
it is below the unchanged gate. All 102 historically failing samples are included
in this sweep and now pass. There is no blanket assertion that every precision
output improved against every other reference.

Every unflagged output was compared across ALL 64 bytes of PackedVertex against
the paired original fast path. Precision outputs additionally passed BYTE-level
position/normal/UV comparisons and BIT-level handedness checks against that path,
including signed-zero representations. Output ownership was resolved from current
indices, structural assignment and the actual degenerate epilogue source mapping.
No shorter-array zip or sampled-vertex acceptance replaced full comparisons.

`part-evidence.jsonl.gz` preserves one actual GPU-risk readback record per part-pose:
node/frame/part, counts, actual flagged group indices, maximum F64 error and passed
equality checks. Group indices are observations, not a static dispatch list in
the algorithm. `summary.json` preserves totals and the worst part-pose.
The full raw output buffers were not archived; exact comparison assertions and
per-part completion records are retained instead of nonexistent buffer hashes.

The baseline WGSL file in this archive is byte-identical to the original
production `gpu_mikk_recipe.wgsl` at capture time; source-manifest records its hash.
Its fast pipelines are compiled from that separate, untouched module. Baseline
comparison's two no-op scheduling entries live in a separate TEST-only module.
The risk/refinement module cannot change ordinary fast-pipeline compiler rounding.
Nonflagged byte comparisons confirm the effect on this actual Device/driver.

## Scope and next gates

The native path uses an actual SHADER_F64-capable RTX 5080/Vulkan Device. It uses
only basic f64 operations and the separately tested degree 41 range-reduced angle
polynomial, never the crashing native f64 atan2 path. Unsupported devices must
retain the ordinary full-quality backend. No production selection was changed.

This closes the full-Ray numerical gate on this hashed input and Device. It does
not establish arbitrary-mesh/material/driver correctness or a steady-state cost.
Before promotion: review the separate feature-gated Program design, synthetic
Any/zero/degenerate/seam/nonmanifold cases, live renderer failure/recovery/source
epoch tests, actual mapped-material images with positive controls, and measured
end-to-end risk/refinement/commit/render cost and distinct-pose FPS.

The prototype's separate risk pass duplicates weighted math and serializes each
group. Sparse Float64 selection alone does not prove speed. Preparation-only
timestamps in the archived diagnostic are not risk/refinement timings. No
full 317 quality threshold was relaxed and no quality-disabled backend was used.
