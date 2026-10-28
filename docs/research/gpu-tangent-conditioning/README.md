# Remaining current GPU tangent precision failures

Read-only follow-up to the full 317-pose diagnostic, not a production correction
or a passing strict gate. Native FPS sampling was left undisturbed: this follow-up
ran only bounded log parsing and six-triangle arithmetic, no builds/GPU jobs.

## Preserved observations

`raw102.log` contains every `GPU_F64_GATE_FAILURE` record extracted from
`/tmp/loom-gpu-recipe-f64-diagnostics317.log`. The complete diagnostic examined
105,868,173 expanded vertices across 4121 parts. All handedness/finite checks
passed; 102 vertex samples exceeded the unchanged 2e-5 vector gate.

The 102 failures reduce to 17 part-poses and 12 distinct expanded vertex indices:

- Body001: 84 samples, 14 poses, vertices 51156,51161,51170,51176,51182,51183.
- Face001: 18 samples, 3 poses, vertices 86468,86471,86475,87422,87434,87435.

Each six-vertex set has identical logged errors at each affected pose. This is
consistent with shared tangent outputs, but the log alone does not prove its
structural group mapping. No clothing samples exceeded this particular F64 gate.

| Node | Time | Part | Samples | Maximum vector error |
| --- | --- | --- | --- | --- |
| Body001 | 4.243671 | 0 | 6 | 3.35820933e-05 |
| Body001 | 8.95886 | 0 | 6 | 2.7874904e-05 |
| Face001 | 15.560126 | 0 | 6 | 2.16443958e-05 |
| Body001 | 27.81962 | 0 | 6 | 2.20569218e-05 |
| Body001 | 29.234177 | 0 | 6 | 2.42270265e-05 |
| Body001 | 49.037975 | 0 | 6 | 0.000113468798 |
| Face001 | 51.86709 | 0 | 6 | 7.86794536e-05 |
| Body001 | 65.54114 | 0 | 6 | 4.4052378e-05 |
| Body001 | 69.31329 | 0 | 6 | 9.0639308e-05 |
| Body001 | 75.91456 | 0 | 6 | 2.53323365e-05 |
| Body001 | 95.71835 | 0 | 6 | 0.000171042257 |
| Body001 | 107.50633 | 0 | 6 | 2.10133231e-05 |
| Body001 | 116.93671 | 0 | 6 | 6.10342141e-05 |
| Body001 | 122.12342 | 0 | 6 | 9.31584338e-05 |
| Body001 | 126.83861 | 0 | 6 | 0.000140557563 |
| Face001 | 131.5538 | 0 | 6 | 2.18296655e-05 |
| Body001 | 146.17088 | 0 | 6 | 4.28236721e-05 |

Worst: Body001/95.71835/part0, GPU–F64 error 0.0001710422573644296.
Its simultaneous CPU–F64 error was 0.0002473316348026018. The GPU candidate is
closer for that sample; both exceed the conservative numeric gate.

## What the retained data proves and omits

The F64 failure branch records node/time/part/vertex/error only:
`crates/loom_bevy/src/gpu_pose_geometry.rs:1428`. Independent F64 recomputation
starts at `gpu_mikk_f64.rs:32` on the same represented positions/normals/UVs.

The JSON dump at `gpu_pose_geometry.rs:1342` is conditioned on a new global
**GPU–CPU** maximum, not a new GPU–F64 maximum. Consequently the available JSONs
do not contain the current 95.71835 worst-F64 group's inputs, GPU derivatives,
subgroup membership, weights, or unnormalized accumulator. The exact cause of
that worst error is **not yet established**. Analytic detection of all 102 failures
cannot be verified from the retained log alone.

`body-time 0-group.json` preserves an existing six-triangle group for Body001,
time 0, target vertex 51156, one of the repeatedly affected indices. It is NOT the
worst 95.71835 input andtime 0 is not among the 102 failing samples. On those exact
represented inputs, independent f64 arithmetic gives:

- `sum(angle)/length(sum(angle*projected_tangent))` =78.48787528150415.
- Unnormalized tangent sum length 0.07918239391002442; angle sum 6.21485785770093.
- Per-triangle derivative subtraction condition
  `(length(y.y*d1)+length(x.y*d2))/length(y.y*d1-x.y*d2)` ranges 2.17855–15.26250.
- Projection condition `length(s)/length(s-dot(n,s)*n)` ranges 1.03295–1.30027.

These measurements establish cancellation sensitivity for this retained group
at time 0. They do not isolate the worst-pose error stage or prove that compensated
summation alone fixes it. Source stages are `gpu_mikk_recipe.wgsl:57` (derivative),
`:21` (projection),`:31` (atan2 corner weight),`:106` (ordered weighted sum),
and`:108` (normalization). Final normalization can amplify prior term errors when
the weighted sum is small.

## Bounded next capture (design; not implemented)

After native sampling is cleared, replay only the 17part-poses above. For each
of the 12expanded targets, resolve its actual output owner/group from the current
recipe; do not assume static vertex IDs in production. Capture:

1. Current represented positions/normals/UV, prepared/original indices, flags,
   ordered structural members and output ownership.
2. GPU raw derivative numerator and normalized derivative; projected derivatives;
   exact dynamic inclusion bitset; corner weight; every weighted contribution;
   unnormalized sum and final tangent. The independent F64 reference must retain
   its corresponding values, recomputed from those same represented inputs.
3. Sum-stage variants evaluated on the SAME captured GPU contributions: naive f32,
   compensated f32, and f64. Separately evaluate F64 contributions with the same
   membership, then independently recomputed membership. This distinguishes
   accumulation error from derivative/projection/angle error and boundary changes.
4. Record derivative, projection and weighted-sum conditions for all members,
   including bounded nearby passing groups. A predictor tested only on known
   failures cannot establish useful precision-dispatch eligibility.

One bounded correction candidate is compensated accumulation of CURRENT weighted
contributions, selected using a measured cancellation/error estimate from current
terms. No static vertex/time lists, saved tangents, truncated groups, rest basis,
or test oracle would enter rendering. It should be implemented only if the
stage-isolation capture shows accumulation dominates and compensation closes the
gap; otherwise this candidate is rejected rather than claimed as a correction.
The trigger threshold must come from the measured error budget and must retain
all existing whole-pose structural/layout guards. Derivative-dominated cases
would require a separately justified higher-precision arithmetic stage.

Acceptance remains: unchanged strict gates reported honestly; all 17captured
cases plus synthetic cancellation/degeneracy/boundary cases; full 317sweep with
all vertices and handedness/finite checks; actual-material image controls; and
end-to-end runtime cost including trigger overhead. No production edits or
activation are included here.

Full source log SHA256: `d1ea22ed07929ad33fa0dc72d4b2d0280eb97d7ea469929c414cf5d43025c992`.
