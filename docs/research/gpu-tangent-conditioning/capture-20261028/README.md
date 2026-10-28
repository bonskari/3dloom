# Bounded current-tangent conditioning capture

This is a new, isolated experiment, not retroactive provenance for the earlier
317-pose audit. No production files were changed. Two isolated GPU runs completed
in 7.65 and 7.76 seconds; the second adds untouched-baseline derivative readback.
No full 317-pose rerun or performance benchmark was undertaken.

## Immutable input and reproducibility

Before compiling/running, the current saved scene was copied to
`/tmp/loom-conditioning-capture/input.loom`, hashed, and made read-only. Its SHA-256
was verified again when archiving:

```
84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935
```

Source path:
`/mnt/2tb/hollowreach-ue/Hollowreach/SourceArt/RayLoom20261025/RayAnimationTest_LoomRig.loom`.
The 259 MB snapshot remains local and is not committed. This hash describes the
new capture input, not the unrecorded historical input. `source-manifest.json`
records source-base and executable hashes. `SHA256SUMS` identifies archived files.

`capture.patch` instruments an isolated copy of Bevy source, leaving production
untouched. Apply it with `patch -p1` inside the copied `src/`, after verifying
its three source-base hashes. It also preserves an unmodified baseline WGSL.
`build.py` is the host-specific direct-rustc script used, resolving matching
cached release dependency fingerprints without Cargo/artifact mutation.

Commands used after source-copy instrumentation:

```bash
python3 /tmp/loom-conditioning-capture/build.py > /tmp/loom-conditioning-capture/build2.log 2>&1
LOOM_PERF_SCENE=/tmp/loom-conditioning-capture/input.loom /tmp/loom-conditioning-capture/tests actual_ray_hybrid_current_gpu_tangents_f64_diagnostics --ignored --nocapture --test-threads=1 > /tmp/loom-conditioning-capture/capture2.log 2>&1
python3 /tmp/loom-conditioning-capture/analyze.py
python3 /tmp/loom-conditioning-capture/summarize.py
```

There is one test filter/one test thread. The source copy selects only the 17
historically affected Body/Face part-poses listed in `frames.json`, using the
original f32 time formula `149.0 * frame / 316.0`. It reuses the original audit's
all-influence GPU skin, current SG-normal and tangent-preparation path, so this
isolates the historical SAME-GPU-ATTRIBUTE failures. It is not a separate capture
of the native backend's CPU-position producer.

## Captured data and validity checks

For each part-pose: six historical expanded targets plus three nearby passing
controls, totaling 153 outputs / 68 unique structural group instances. Every
member carries current represented positions/UVs, group normal, flags/order,
actual baseline derivatives, traced projected derivatives/weights/contributions
and running sums. `captures.json.gz` is a gzip-compressed JSON object keyed by
capture filename; two worst cases are also stored as readable JSON. To unpack,
load that object with Python `gzip.open`/`json.load` and write its values using
the filename keys into `/tmp/loom-conditioning-capture/` before analysis.

The baseline uses the ORIGINAL unmodified WGSL; the traced evaluator uses a
separate program. Shader instrumentation DOES change some evaluator results:
maximum target delta 0.0001275752. Those traced results are not silently substituted
for production results. The 102 baseline failure locations match the historical
node/frame/vertex set exactly. The isolated audit's ordinary `GPU_F64_*` log lines
come from its instrumented candidate; use `analysis.json`/`summary.json` for the
explicitly separate untouched-baseline error comparisons.

Readback of the baseline derivative buffer was added without modifying that
shader. All eight f32 words of its derivative records match traced derivatives
bit-for-bit across 904 captured member rows. Traced target output with debugging
enabled matches the same instrumented program with debugging disabled exactly.
Independent Python reconstruction matches the existing Rust F64 tangent direction
within 2.36e-13. All 15 subsequent part preparation guards hit.

The instrumented subgroup inclusion bitsets match independently recomputed F64
membership for all 153 captures. Untouched-baseline membership is not directly
traced, so this is not claimed as direct observation of its branch decisions.

## Supported numerical findings

| Current sample | Untouched GPU–F64 error | Tangent-sum condition | Projection condition | Baseline derivative + otherwise F64 error |
| --- | ---: | ---: | ---: | ---: |
| Body001/95.71835/51156 | 1.71042e-4 | 2326.90 | 1.50634 | 9.90082e-5 |
| Face001/51.86709/86468 | 7.86794e-5 | 42.1826 | 132.317 | 1.68817e-5 |

At Body's worst sample the real weighted sum has length 0.0026657926, while the
sum of term norms is 6.20302775. Individual baseline derivative-direction errors
are only around 3.26e-7, but the nearly cancelling weighted sum is highly sensitive
to such upstream errors. Retaining the actual untouched-baseline derivatives and
performing projection/weights/sum in F64 still exceeds 2e-5 for 30 of the 102 outputs.
Thus higher-precision accumulation alone cannot generally remove the observed
upstream derivative error. This conclusion does not depend on traced evaluator
rounding matching production.

For the Face worst case, the derivative subtraction is well-conditioned
(condition 1.21634), but projecting nearly normal-aligned directions amplifies
small errors (condition 132.317). The traced projection-only error reaches 6.74e-6.
That projection measurement belongs to the instrumented evaluator; its target
baseline delta is 4.54e-7, and is retained as a limitation on exact stage attribution.
The baseline-derivative/F64-rest calculation demonstrates how much the captured
baseline derivative error contributes without relying on traced projection math.

Naive/Neumaier-compensated/F64 accumulation of the SAME traced GPU contribution
words was separately evaluated. Worst Body errors are 1.64857e-4 /1.52910e-4 /
1.52910e-4; Face remains about 7.86e-5. These numbers reject compensated summation
as a sufficient standalone correction for this capture. They do not establish
exact production per-term rounding: the trace can alter FMA contraction and the
separately stored product need not equal a fused multiply-add's intermediate.

## Analytic predictor: bounded evidence, not a formal GPU error bound

The diagnostic indicator uses current attributes and actual baseline derivatives:

- D: maximum per-member derivative numerator subtraction condition
  `(length(y.y*d1)+length(x.y*d2))/length(y.y*d1-x.y*d2)`.
- P: maximum `length(s)/length(s-dot(n,s)*n)` for baseline derivative s.
- K: `sum(length(weight*projected_s))/length(sum(weight*projected_s))`, using
  baseline derivatives and independently computed current corner weights.
- Indicator: `2^-24 * K * (D + P + contributing_member_count)`.

On these 153 outputs, comparison with the unchanged 2e-5 threshold detects all 102
baseline failures and flags none of the 51 nearby controls. Minimum failing
indicator 1.00917e-4; maximum control 1.03942e-6. Controls' maximum baseline error
is 1.08751e-7. The equivalent approximate condition from traced GPU terms is close
at the worst Body sample (2326.56 versus2326.90).

This is evidence for current-attribute conditioning as a dispatch signal. The
sample was selected using known failures and nearby controls. It does not prove
all 317 recall/false-positive rates, an implementable GPU predictor's exact values,
a formal WGSL transcendental error budget, or general-material correctness.
No static vertex/frame IDs belong in a production precision-selection rule.

## Bounded correction to test next

A supported candidate is adaptive higher precision for the WHOLE CURRENT risky
group, including derivative numerators, projection, membership/angles and weighted
sum. Final-sum-only compensation is insufficient. A production implementation
would consume real current represented attributes plus validated structural
metadata and recompute all group members/output owners, not saved tangents,
truncated influences or a test-reference result. The test-only F64 module must
not be wired into the renderer as an oracle.

One possible production route is an owned sparse CPU high-precision math stage
for condition-flagged groups, batching any required current GPU-normal readback
with pose validation. Corrected current spaces would enter BEFORE the existing
degenerate epilogue/packing so all output owners inherit them. An alternative
GPU high-precision representation needs comparable evidence across all stages;
retaining native atan2 while upgrading only derivatives is not already proven
sufficient. Neither route is implemented or benchmarked here.

Before activating a correction: verify an actual current-attribute trigger,
all captured cases and synthetic cancellation/projection/boundary cases, then
full 317/all-vertex numerical acceptance and material-image positive controls.
Measure trigger/readback/math/commit overhead end-to-end. Whole-pose descriptor,
recipe and commit guards remain mandatory. This experiment supports a precision
investigation; it is not a completed quality fix or a 60-FPS claim.
