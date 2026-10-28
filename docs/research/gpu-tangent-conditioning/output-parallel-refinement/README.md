# Isolated output-parallel whole-group Float64 refinement

Not a production change. The input is the same pre-experiment frozen `/tmp/loom-conditioning-capture/input.loom`, SHA25684fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935. The earlier historical raw102 provenance limitation is unchanged. Actual producer here is current CPU final world-baked positions through GpuPoseRuntime, all5Skin meshes/all13materialparts. CPU final tangent answers are never supplied to the algorithm.

## Minimal dispatch/data change

The immutable u32 structural recipe appends a count-header and (groupID,preparedFace) rows, one row for each existing ordered group member. Previously unused recipe1.w points to the header. No new storage binding or uniform field is added. Refine dispatch is ceil(unique_output_rows/64), rather than ceil(groups/64). A row looks up the same risk flag, group normal/representative and own face, then executes exactly one former outer-k-loop body. The full inner-j loop, every current derivative/projection/membership/angle/weighted accumulation and original summation order remain intact. Risk and threshold, normal/UV/position math, handedness ownership, epilogue and validation are unchanged.

`source-parity.json` verifies exact production baseline/risk/serial-refinement bytes; all shader text before refine_group, the complete single-output F64 body, and validate tail are byte-identical between serial and parallel versions. Parallel source SHA256bbfe876303abcf9b0ebcc3305489b97152c35248f05b54f7037b0f4d87e7ebcc. Removing the outer loop can still change compiler behavior, so actual output-byte/F64 oracles remain mandatory.

## Explicit write ownership and limits

Source contract: mikktspace/generated.rs Build4 seed adds a triangle only for an unassigned corner; AssignRecur1018–1059 returns before AddTriToGroup if the matching representative corner is already assigned to the same or a different group. It selects the first matching representative corner. The prototype does not merely assume uniqueness: it computes that same first canonical corner for each row and uses a CPU owner bitmap indexed by originalFace*3+corner. Any duplicate writer (within one group or across groups), missing representative corner or out-of-range original face is rejected before new per-part GPU resources are created. No membership list is deduplicated, truncated or changed. Valid duplicated input triangles have distinct original-face destinations and remain supported. Shared final indexed mesh vertices retain separate original-corner spaces; the existing final last-owner packing is unchanged. Active Any triangles retain adopted orientation; inactive position-degenerate tails still use the original epilogue.

Metadata count/offset/end indexing is checked against u32 and storage size. Explicit unique_writers<=corners keeps refinement dispatch inside the existing checked corner dispatch bound. Appended metadata is included in the ordinary recipe storage-size preflight before allocation/bind creation. Empty groups produce zero work. No new capability or production environment controls are added.

## Bounded correctness

Ten synthetic tests GREEN3.12s, including all8 prior finite/angle/Any/degen gates plus malformed duplicate-writer rejection and22 serial-byte/risk/F64 cases: duplicate triangles, shared indexed vertices, Any/default, all-position-degenerate and interspersed tails, nonmanifold edges,6scales and9near-antiparallel UV skews. Invalid repeated group members/cross-group destinations are explicitly rejected, never silently skipped.

Four actual CPU-position poses0/39.25/95.71835/149:52parts /1,335,876 PackedVertex records. Every64B output and every risk flag equals the original serial production refinement; independent F64 max1.437707381266227e-6, P/N/UV bytes, finite output, CPU history positions and handedness all pass. Full317 and4post149 endpoints are recorded separately after terminal completion, not inferred from this bounded set.

Independent read-only performance_review confirmed no important correctness findings in mapping/ownership/limits and inner-j order, without running jobs. No production promotion follows from that review alone.

## Quiet paired cost (session40841)

Root confirmed no CPU/GPU jobs, native80eb app paused atframe39 in fast1 comparator with ordinary idle redraw. The cost run completedGREEN4.24s. Four current-CPU-position poses/all13parts,64AB/BA samples per mode/pose after4warmups. Both modes use the same global8stage schedule, risk shader and validated current inputs. Pipeline compilation, current geometry/preparation, structure compilation, CPU F64 oracle and output readback checks lie outside timed spans. Whole GPU and per-pass query timestamps are reported; CPU encoding includes common timestamp encoding/resolve, CPU submit-to-completion includes common144B queryresolve and device wait. Timestamp readback mapping and geometry readback occur outside those clocks. All output bytes and risk flags matched before/after timed batches.

| Mean across4poses (ms) | Serial | Output parallel |
|---|---:|---:|
| Whole GPU |1.524650|0.535169|
| F64 refinement |1.137617|0.148397|
| Risk |0.143834|0.145262|
| CPU encoding |0.116315|0.123136|
| CPU submit→completion |1.613820|0.623443|

Whole GPU saved0.989481ms; refinement saved0.989221ms (~87.0%). The risk source/stage is unchanged; its small measured difference has no isolated causal attribution or statistical interval. This is a bounded quiet stage comparison, not native FPS, end-to-end rendering, UI cost or a guarantee of60FPS. Old fast5 numbers from other runs are not a paired baseline in this experiment.

## Reproduction

`quality-source.tar` and `cost-source.tar` contain full isolated source trees, exact direct-rustc build.py and args.json. Extract under `/tmp/loom-refine-output-parallel` or `/tmp/loom-refine-output-parallel-cost`; each build.py needs the matching local release dependency fingerprints/rlibs it names. Source manifests and from-production patches record exact content. No production paths are patched when using the scratch trees.

```
python3 /tmp/loom-refine-output-parallel/build.py
/tmp/loom-refine-output-parallel/tests precision_tests --nocapture --test-threads=1
LOOM_PERF_SCENE=/tmp/loom-conditioning-capture/input.loom /tmp/loom-refine-output-parallel/tests actual_ray_precise_current_cpu_positions_f64_quality --ignored --nocapture --test-threads=1
# Only in an agreed quiet cost window:
python3 /tmp/loom-refine-output-parallel-cost/build.py
LOOM_PERF_SCENE=/tmp/loom-conditioning-capture/input.loom /tmp/loom-refine-output-parallel-cost/tests actual_ray_precise_current_cpu_positions_f64_quality --ignored --nocapture --test-threads=1
```

## Final scratch quality terminal results

Same compiled binary/module combination, one GPU run at a time: full317 session98536 GREEN75.35s,4121parts /105,868,173vertices, maxF64 3.338108500276807e-6; endpoint session74485 GREEN3.01s,52parts /1,335,876vertices, maxF64 1.4115617414233096e-6. Every complete64B output record and every risk flag equals the serial production refinement in every part. All P/N/UV bytes, CPU history positions, handedness and finite outputs passed. Endpoints are149.00001525878906/150/153.5/157; they do not imply continuous post149 coverage. No production edits were needed for these scratch sweeps.

The final quality harness also includes a separate endpoint-only wrapper added after the cost harness snapshot; the cost source therefore differs in that test-wrapper shape as well as timestamps. Shader/module/entrypoint math is the same. Source tar/manifests are the exact final compiled snapshots; no unsupported portability claims are made for the local dependency cache.
