# Provisional seven-pass precision cost: promotion blocked

The unchanged numerical prototype passed full 317 but did NOT meet the cost gate.
It is not promoted. The separate finite-intermediate correction was not part of
this measurement; its cost is unknown. The normal application was unchanged.

## Actual paired test

The immutable input was `/tmp/loom-conditioning-capture/input.loom`, SHA256
`84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935`.
Actual current Ray attributes/recipes for ALL 13 material parts were prepared at
0/39.25/95.71835/149. Pipeline creation, source/preparation/recipe construction,
initial independent-F64 validation and output comparisons were outside timing.
The Device/Queue/programs/buffers remained allocated throughout repeats.

Each pose used four paired warmup iterations, then 64 paired samples per mode,
alternating AB/BA order. Original mode is the ACTUAL production
`GpuRecipeMath::encode_batch`, copied as a separate module: FIVE global passes
across 13 part bindings, not thirteen per-part submissions or a seven-no-op mode.
Only its shader include path changes to the exact original WGSL baseline file.
Candidate mode is the seven-pass risk/refinement implementation from the archived
full 317 prototype; its fast passes use that same original shader module.

CPU encoding includes encoder construction, command recording and finish.
Submit timing is queue.submit only. Submit-to-completion includes a wait for that
submission, including common query-resolution/16-byte timestamp-copy bookkeeping.
Timestamp CPU mapping and all geometry output readbacks are outside those timers.
GPU span starts before math encoding's commands and ends before timestamp resolve;
it excludes query resolution/copy and every CPU/readback validation step.
These scopes are not a full render frame, guard-readback integration or FPS test.

## Results

Four-pose means, milliseconds per thirteen-part batch:

| Scope | Original five passes | Provisional seven passes |
| --- | ---: | ---: |
| CPU encoding | 0.097982 | 0.137268 |
| CPU submit | 0.016336 | 0.023679 |
| Submit to completion | 0.353835 | 3.966230 |
| GPU span | 0.206681 | 3.826022 |

Per-pose GPU spans were 0.205973–0.207867 ms original and 3.797137–3.878717 ms
candidate. All post-run original/candidate output bytes exactly matched their
separately validated pre-timing outputs. The test completed once in 7.29 seconds;
that wall duration includes unmeasured setup and quality work.

The roughly 3.62 ms extra GPU math is a performance blocker. This measurement
does not identify whether the risk pass or f64 refinement dominates. Per-pass
GPU timestamps are a separate next diagnostic; no dominant cause is invented.
No gate is lowered and no CPU/captured tangent injection substitutes for math.

## Artifacts

`cost.log` and `summary.json` preserve all measurements. Apply `harness.patch`
AFTER the archived full 317 isolated prototype patch to reproduce this specific
cost harness; `build.py` retains the host-specific matching-dependency rustc
recipe. Commands:

```bash
python3 /tmp/loom-f64-cost/build.py > /tmp/loom-f64-cost/build.log 2>&1
LOOM_PERF_SCENE=/tmp/loom-conditioning-capture/input.loom /tmp/loom-f64-cost/tests actual_ray_hybrid_current_gpu_tangents_f64_diagnostics --ignored --nocapture --test-threads=1 > /tmp/loom-f64-cost/cost.log 2>&1
```

This was the quiet window supplied after root's integration tests, before its
next release build. The application was paused. No native-FPS extrapolation is
claimed. The later finite/boundary revision must retain its own correctness and
cost evidence rather than inheriting this old prototype's numbers.
