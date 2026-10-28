# Rejected deferred base-Mesh clone prototype

Only a copied Core under ignored `target/` changed. The real job stores `Cow<Mesh>` plus selection instead of an already owned StackValue. Main-thread `p.display_mesh()`/`object.mesh()` resolution is retained, including owned NURMS generation; `Cow::into_owned` and construction of the complete real StackValue move inside the existing worker animation scope. No empty/default mesh substitutes for the input, and only immutable Mesh data crosses the worker boundary. Weighting, matrices, gizmos, metadata and cache publication remain unchanged.

A benchmark-only toggle converts the same Cow to Owned on main for the baseline; the candidate defers ownership to the existing four-worker scope. Both run in one optimized binary, alternating order each pose. Setup and reference verification are outside timed batches. The app was paused and no other compiler/test process was visible at measurement start.

## Quiet paired317-pose result

| Per complete five-skin pose | Baseline ms | Deferred ms |
|---|---:|---:|
| Whole batch |3.119847|3.750280|
| Main preparation |1.834082|0.838190|
| Executor wall |1.277019|2.894902|

The candidate improves main preparation by0.995892ms but makes the complete batch0.630433ms slower. The executor increase is measured; its cause beyond the moved work has not been isolated. No scheduling/cache explanation is asserted as fact. The complete test took7.90s. These standalone times are not native FPS measurements.

## Correctness

All1585 full StackValues/meta/warnings/selections/shape/patch state, complete Mesh contents, vertex float bits and map value bits matched both the original uncached batch and ordinary serial evaluator across317actual poses. Editor cages, serialized modifiers/every influence/weight and Scene serialization stayed unchanged.

All48 ordinary Scene tests passed (two opt-in tests ignored), including viewport/render Fast Updates behavior, worker TLS/time changes, cache publication, geometry-attachment constraint fallback, instance/general-stack fallback and edits. An added explicit test verifies plain inputs remain Cow::Borrowed until the worker, NURMS inputs are already Cow::Owned on main, and both match the serial full result. No production change was made because the measured net result is negative.

## Reproduction

```sh
python3 docs/research/deferred-skin-clone/compile_probe.py
target/deferred-skin-clone-probe/loom_scene_tests
LOOM_PERF_SCENE='/mnt/2tb/hollowreach-ue/Hollowreach/SourceArt/RayLoom20261025/RayAnimationTest_LoomRig.loom' target/deferred-skin-clone-probe/loom_scene_tests actual_deferred_clone_317 --ignored --nocapture
```

The helper copies only into ignored `target/`, applies the narrow retained patch, adds the probe module and links exact Cargo release dependencies using fingerprint `loom_bevy-d37bfa3ba707912c`. Build those release dependencies or update the explicit fingerprint if its graph changes. A Core source mismatch causes patch failure rather than silently altering the experiment. Prototype, tests and result logs are preserved here.
