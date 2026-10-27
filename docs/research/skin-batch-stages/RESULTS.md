# Actual Core Skin batch stages, source snapshot only

No production source was modified. An optimized Scene source snapshot adds timers around unchanged expressions and appends a probe. It links the exact current Cargo release dependencies plus the existing bounded four-worker task pool. The application was paused; no other compiler/test jobs were visible during the two measured runs. Full logs and the narrow instrumentation patch are retained here.

Both317-pose runs compared all1585 full StackValues against a separate serial Scene evaluator: complete Mesh/meta/warnings/selections/shape/patch state, vertex float bits and every map value bit. Editor cages remained equal and serialized full modifier data (including all weights/influences) matched before/after. No weighting, arithmetic or metadata operation was skipped or replaced.

## Normal batch run

| Per complete five-skin pose | ms |
|---|---:|
| Main-thread preparation |1.881210|
| Included base display_output mesh copies |1.005432|
| Four-worker executor wall |2.040730|
| Main-thread publication |0.008040|
| Worker apply CPU sum |4.035577|
| Largest individual apply job |1.987710|
| Worker finish metadata CPU sum |0.000256|

Worker apply detail (CPU sums across five jobs, not wall-time components): weighted-point copy0.154246ms, bone/matrix setup0.188263ms, output-point copy0.048711ms, all-influence loop3.532025ms, gizmos0.000250ms, position write0.104520ms. Largest weight-loop job1.778535ms. Total test5.33s includes parsing/reference verification outside measured stages. Timer/recording overhead exists; recording uses one bounded counter push per worker stage and is not part of the inner measured scope.

`finish_modifier_value` performs warning/selection/shape/patch metadata bookkeeping. It does **not** call `Mesh::check_invariants`; neither does Skin `apply_owned`. The UI's later `transform_mesh` does validate topology, measured separately in the clone/world-stage probe. There is no invariant-check saving to remove from this finish stage.

## Actual settings

Every skin: fast_updates=false, always_deform=true, back_transform=true, zero gizmos, no patch, bone_affect_limit20, all vertices explicit, rigid_all=false and zero individually rigid vertices. No fast/strongest-only jobs ran.

| Node | Vertices/weights | Influence min–max | Bone palette |
|---|---:|---:|---:|
|Body001|12710|1–12|342|
|Face001|34271|1–12|875|
|SkeletalMesh001|3490|1–5|342|
|SkeletalMesh002|9860|3–8|342|
|SkeletalMesh003|3009|2–8|342|

## Separate effective-weight replay

The second run precomputes identical current point queries, then times `point_weights_into` as a whole job loop (without per-vertex timers), retaining every influence. Its CPU sum was2.361554ms/pose, largest job1.111771ms. Query construction is outside that replay interval. This replay warms caches: the subsequent real all-influence loop fell to1.936912ms CPU sum. These timings therefore must not be subtracted as additive components of the first run.

The source's explicit-weight path copies, filters, limits, normalizes and sorts on each call. On these actual data the limit20 exceeds the observed maximum12 and no rigid path applies. A future exact effective-weight cache would need proof that all relevant input content remains unchanged; a guessed version key is insufficient.

GPU skinning plus CPU readback could avoid approximately the measured2.04ms executor wall before paying GPU dispatch/readback costs. The1.88ms main preparation (including1.01ms base mesh copies) remains under the current API. This is a potentially meaningful but bounded gain, insufficient alone to turn the measured41native poses/sec into60. This probe did not measure GPU skin/readback latency and makes no net FPS claim.

Reproduction:

```sh
python3 docs/research/skin-batch-stages/compile_probe.py
LOOM_PERF_SCENE='/mnt/2tb/hollowreach-ue/Hollowreach/SourceArt/RayLoom20261025/RayAnimationTest_LoomRig.loom' target/skin-batch-stage-probe/loom_scene_tests actual_skin_batch_stages_317 --nocapture
LOOM_WEIGHT_REPLAY=1 LOOM_PERF_SCENE='/mnt/2tb/hollowreach-ue/Hollowreach/SourceArt/RayLoom20261025/RayAnimationTest_LoomRig.loom' target/skin-batch-stage-probe/loom_scene_tests actual_skin_batch_stages_317 --nocapture
```

The helper copies only into ignored `target/`, applies the retained narrow patch and compiles it. It uses the explicit Bevy release-test fingerprint `loom_bevy-d37bfa3ba707912c` to find exact Scene/task-pool dependencies; build the release test dependencies or update this explicit fingerprint if the graph changes. A source mismatch makes the patch fail instead of silently changing the experiment.
