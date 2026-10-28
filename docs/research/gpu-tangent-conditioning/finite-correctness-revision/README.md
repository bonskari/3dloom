# Fail-closed finite-intermediate revision (isolated)

The review finding was first a code-level concern, then reproduced on the actual
GPU. All input P/N/UV values were finite: a triangle with position scale 1e30 and
UV scale 1e10. CPU/GPU preparation flags matched [8], structural guard 0, risk flags
[0,0,0], but the old prototype published [NaN,NaN,NaN,-1] instead of the independent
F64 result [1,0,0,-1]. `red-original.log` preserves that failing execution.

Only isolated `/tmp/loom-f64-finite/src` changed. The revision uses integer
exponent-bit checks rather than NaN-sensitive float threshold comparisons:

- Invalid current input attributes reject through the existing guard path.
- Nonfinite/underflowed derivative, projection, corner-edge, angle, accumulation
  or condition intermediates select full current-group Float64 refinement.
- An eighth GPU pass validates the final tangent and consumed P/N/UV fields;
  nonfinite output sets guard bit 32. The existing whole-pose consumer rejects
  guard failures; it must never treat the output as a successful candidate.

The reproduced finite-overflow case now refines successfully: risk [1,1,1], guard 0,
all three tangents exactly [1,0,0,-1]. It is not hidden by rejecting recoverable
geometry. Tests additionally cover:

- Six position scales from 1e-16 to 1e30.
- Nine near-antiparallel UV skews 1 through 1e20, including the f64 pi-rounding
  subgroup boundary; all finite outputs match independent F64 within 2e-5.
- A deliberately corrupted final NaN tangent as a negative control: actual GPU
  final validation sets guard 32 and readback rejects it.
- Invalid source normals: existing preparation rejects them BEFORE recipe
  creation with a nonfinite-attribute error, correctly earlier than refinement.

The initial test incorrectly expected invalid input to reach the later math
stage; the final test asserts that earlier rejection. The finite-overflow
reproduction itself failed before the fix and passes afterward.

`green3.log`: all three new GPU regressions passed. `all-recipe-tests.log`: all
seven existing+new recipe tests passed, including Any orientation, last owner,
degenerate epilogue/defaults and angular boundary cases. No production source,
feature selection or test-environment controls were promoted.

Apply `revision.patch` after the archived full 317 prototype to recreate the
isolated revision. This eight-pass revision has NOT inherited the old full 317
quality or cost result: changes to risk selection and final validation require
repeat quality/cost gates before promotion. Its cost was not benchmarked.
The separate old seven-pass cost remains a blocker, not a performance claim for
this revision. Numeric guard 32 should be distinguished from structural misses
in a future production recovery path; no production recovery code changed here.
