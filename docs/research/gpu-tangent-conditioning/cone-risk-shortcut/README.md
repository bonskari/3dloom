# Isolated cone8 risk shortcut (not production)

The immutable input is `/tmp/loom-conditioning-capture/input.loom`, SHA256 `84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935`. It was copied before this experiment from `/mnt/2tb/hollowreach-ue/Hollowreach/SourceArt/RayLoom20261025/RayAnimationTest_LoomRig.loom`. This hash does not identify the older historical raw102 input.

`quality-source.tar` and `cost-source.tar` contain complete copied Bevy test sources and their direct-rustc build scripts. These require the matching local release dependency fingerprints/rlibs referenced by build.py. Extract into `/tmp/loom-f64-cone` and `/tmp/loom-f64-cone-cost`, respectively; run `python3 build.py`. No Cargo production sources are patched. `cone.patch` records changes from the finite-corrected prototype; `cost-harness.patch` records the subsequent harness changes. Test-only environment selectors are not production APIs.

## Bound and conservative eligibility

For non-normalized finite axis h, if every current projected direction s_i satisfies h·s_i >= m > 0, then for any nonnegative angle weights a_i, ||sum(a_i s_i)|| >= m sum(a_i)/||h||. Thus the existing cancellation indicator K is bounded by ||h||/m. The shader subtracts a conservative rounding allowance from m and inflates the numerator, then skips quadratic risk work only when its upper indicator is below 1e-5 (half the unchanged 2e-5 risk threshold). This bounds the existing heuristic, not all final tangent error or arbitrary WGSL implementations.

The margin is 256*eps32*max(1,max_abs_dot_terms)*(1+projection_condition*(1+normal_squared)); bounded accumulation gamma adds a further denominator subtraction. Groups above 256 members, unbounded gamma, nonfinite/zero/degenerate or poorly conditioned own-angle cases retain full old risk calculation or fail closed. No member is dropped. Collinear zero-angle and near-antiparallel cases are explicit regressions. The exact details remain in the archived shader.

The original fast module is unchanged. F64 refinement and final validation are compiled from the exact same finite-reference shader in both variants; refinement scheduling and arithmetic are unchanged. Only risk selection differs.

## Correctness before cost

Eight synthetic GPU tests passed (synthetic.log): scale/overflow recovery, nonfinite final rejection, near-antiparallel boundaries, Any/default/degenerate and zero-angle cases. Bounded 17 part-poses passed independent F64 <=2e-5 and finite8 equality: 1,414,482 packed records byte-identical, identical flags with no dropped or extra flags (bounded17.log). These are bounded results, not full317 acceptance. Full317 verification is recorded separately after completion.

Reproduce bounded gate:

```
/tmp/loom-f64-cone/tests gpu_pose_geometry::mikk_recipe::tests --nocapture --test-threads=1
LOOM_PERF_SCENE=/tmp/loom-conditioning-capture/input.loom /tmp/loom-f64-cone/tests actual_ray_hybrid_current_gpu_tangents_f64_diagnostics --ignored --nocapture --test-threads=1
```

## Quiet paired cost

App paused at frame39 after native stability completed; no other GPU jobs. Session6422 completed in12.05s. Four poses (0,39.25,95.71835,149), all13 parts, 64 AB/BA GPU timestamp samples per mode after4 warmups. Pipeline compilation, preparation, CPU oracle work and output checks are outside timing. No CPU encode/completion timing or FPS claim. Run `LOOM_PERF_SCENE=/tmp/loom-conditioning-capture/input.loom /tmp/loom-f64-cone-cost/tests actual_ray_hybrid_current_gpu_tangents_f64_diagnostics --ignored --nocapture --test-threads=1`.

Mean GPU milliseconds across four poses:

| Pass | finite8 | cone8 |
|---|---:|---:|
| Whole | 4.040870 | 1.614262 |
| Risk | 2.703538 | 0.142342 |
| F64 refine | 1.014504 | 1.211552 |
| Validate | 0.046461 | 0.012283 |
| Pack | 0.069720 | 0.041105 |

Risk decreased94.7%; whole GPU span decreased60.1%. Refinement source/scheduling is unchanged, but its measured time varied; the cause is not isolated. Do not attribute it to clocks/cache or claim refinement improved. Flags and complete output bytes matched outside timed spans for every tested part. This is still slower than the original fast-only pipeline; no production promotion is approved.

## Exact full317 source separation

`cone.patch` is a bounded diagnostic patch: it includes 17-part-pose filters and part0 selection, and is neither a production patch nor the full317 source. `full317-source.tar` contains the actual full317 source and build.py, with those filters removed and times `(0..317)*149/316`. `full317-source-sha256.txt` hashes every source file actually compiled. `bounded-to-full317.patch` records exactly the harness change; no production source is activated. Expected completion is4121 parts; only terminal evidence may establish that count. Environment/reference selectors remain isolated test controls.
