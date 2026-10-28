# Integration design after the full numeric gate

Design only. No production promotion is included. The isolated full 317 trial
passed; shader scheduling, native material images and steady-state cost still
need review/measurement before enabling a production path.

## Program ownership and feature eligibility

Retain the original FastProgram shader module and binding layout 0..8 unchanged.
Create a separate shared PrecisionProgram for the f32 risk and f64 refinement
pipelines, using layout 0..9. The latter module can concatenate original helpers
and new precision functions; it must NOT supply the ordinary fast pipelines.
This prevents unrelated precision/tracing code from changing baseline compiler
rounding for unflagged outputs.

PrecisionProgram construction requires the ACTUAL shared Device to contain
SHADER_F64, at least nine storage buffers per compute stage, and all existing
storage/range/dispatch limits. Do not infer the active Device from an adapter
name, request new features blindly, or create Float64 shaders before eligibility
is established. Reuse the renderer's Device/Queue; no second production Device.

Unsupported hardware retains the existing full-quality CPU geometry/Mikk path
with all influences/materials/shadows. Do not present a fast-only GPU diagnostic
as the newly validated precision backend. Backend selection should happen before
cache/asset mutation, avoiding repeated failed construction each frame. Any
initialization failure withdraws current GPU source availability for that epoch.

Share immutable programs per Device/renderer. Per-part state owns two bind groups
referencing the SAME current source/preparation/recipe/spaces/output buffers and
a persistent 4-byte-per-group risk buffer. Replace per-part bindings/risk capacity
on validated recipe/topology changes. They must not be recreated every pose.
Risk flags are cleared per current pose, never reused as persistent eligibility.

## Submission and guards

Keep the existing order:

1. Current source positions / SG-normal packing / current preparation.
2. Existing recipe guard initialization and original derive/evaluate passes.
3. Independent current-condition risk pass, then conditional whole-group f64 pass.
4. Existing degenerate epilogue and final output-owner packing.
5. Existing batch guard readback, all-target layout/alias preflight and atomic
   whole-pose commit into persistent dynamic material buffers.

Every math pass honors the existing scratch guard word. Precision flags are
numeric dispatch data, NOT structural guard failures or a request to rebuild
from stale/rest metadata. Structural misses retain the real current-attribute
compiler/recovery path; no outputs are committed until the entire current pose
is valid. Clear risks again after recovery before rerunning.

Refinement recomputes all current member contributions/output corners in a
flagged group before epilogue/pack. The validated group's handedness remains
unchanged. Positions/normals/UV bytes and current CPU positions for history,
bounds/picking remain under the existing contracts. Candidate/output/guard APIs
need not change. GPU-edge source packets still publish only after successful
same-world-version whole-pose commit; failed sync cannot expose an old buffer.

No CPU final tangent, independent F64 reference, saved capture term, static node
or vertex list enters production. CPU structural compilation is still genuine
current topology metadata; the independent F64 implementation remains test-only.

## Trigger and unresolved cost

The trial's current-attribute indicator uses derivative subtraction, projection,
weighted-sum cancellation and contributing count; both S/T directions enter its
conservative checks. It selected 2841/20077195 group instances over full 317 and
refined 17626/105868173 output vertices. This is measured selection on Ray, not a
formal floating-point error bound or proof of arbitrary meshes/hardware.

The correctness prototype deliberately repeats weighted math in a separate
per-group risk pass, preserving original fast rounding. This adds work and may
serialize large groups; the current group refinement also uses one invocation
per group. Do not truncate groups to reduce cost. A per-corner parallel form or
metadata reuse could reduce cost, but requires the same byte/quality regression
checks and separate profiling. Native Float64 throughput/register pressure and
extra pass barriers remain unmeasured in the live renderer.

Measure original-fast vs precision on identical current sources/recipes/all 13
parts with shared programs and persistent buffers: CPU encode/submission, GPU
risk/refine/whole math span, existing guard-readback wait, and final copy/Bevy
render. Then measure actual distinct-pose Play FPS and actual-material images.
Preparation-only timestamps in the old audit log do not measure precision cost.

Review/gates before activation: synthetic Any/zero/degenerate/seam/nonmanifold
cases; missing-feature/capacity fallback; exact nonflagged and P/N/UV/handedness
checks through renderer.sync; actual mapped materials with mutation controls;
whole-pose failure/recovery/source-withdrawal tests; shared-queue lifetime proof;
and measured end-to-end budget. Full 317 numerical success alone proves no FPS
improvement and does not finish general-material acceptance.
