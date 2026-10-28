# Parallel Skin input preparation — native verification pending

The existing batch evaluator cloned/evaluated the five base objects on the caller before dispatching Skin jobs. This change borrows only their immutable PolyObject data and performs the identical Editable Poly display_mesh or Editable Mesh plain-mesh clone inside each existing worker job. The bounded pool size and Skin arithmetic/weights are unchanged. Workers retain captured animation time/rendering state; base-object animated reads join the existing result/cache dependency tracking. No unsafe code, stale mesh cache, reduced influence count or geometry omission.

The real failing scheduling test used an animated NURMS base with a static bone. Old code read the animated base before entering the executor. The candidate defers it and preserves serial outputs across forward/fractional/backward times plus same-time cache reuse. A separate test retains Editable Mesh semantics even when its PolyObject subdivision settings are enabled. Existing worker TLS, render/time changes, cycle-guard fallback, disabled/instanced stack and full metadata tests remain. The first broad ObjectKind borrow was rejected by Rust because an unrelated CompoundCache contains RefCell; the final borrow is narrowed to the already eligible, thread-safe PolyObject variants. That compile error is not the behavioral RED.

## Quiet actual-scene pair

Actual saved Ray source, all five full Skin outputs,317integer/fractional poses, alternating which evaluator runs first at every pose, one existing-style four-worker pool. Application paused; no compiler or test jobs were visible when this pair ran. Reference comparisons/parsing/serialization occur outside timing. Baseline and candidate use separate scenes; an additional independent serial scene checks every result.

| Complete five-skin batch | Baseline ms | Candidate ms |
|---|---:|---:|
| Caller preparation + worker execution + publication |3.321400|2.377052|
| Included caller preparation |1.308302|0.851801|

All1585complete StackValues/Meshes/meta/warnings/selections/shape/patch values matched baseline and serial reference. Position/color/illumination/alpha/map values matched by float bits. Source cages and serialized full modifiers, including every weight/influence, remained unchanged. This isolated CPU result does not establish native FPS or a high-frequency Mac result.

Scene full suite115passed/1ignored (real-scene test not implicitly run by that suite). The standalone real-scene pair above independently exercised317poses. Full UI suite446passed/2ignored and Bevy suite89passed/38ignored. Native default FPS remains pending. Do not markTask9or the whole goal complete yet.

Reproduce after building release dependencies: `python3 docs/research/parallel-skin-input/compile_probe.py`, then `LOOM_PERF_SCENE=/path/to/real.loom target/parallel-skin-input-probe/tests actual_parallel_skin_input_317 --nocapture`. The builder copies sources only into ignored target/, injects the retained original baseline there, and links exact recorded release dependency fingerprints. Source-proof.json records source hashes; no full State/preferences or local Kimodo address is archived.

## First native result: no proven end-to-end gain

Installedb2e2d82 preserved all four viewport pixel crops exactly and passed real FK pointer/gizmo/skin/Undo, all original node/selection/frame/source checks. The unprofiled production candidate measured60.183R/86.987W (min57.028R/81.872W). Interleaved previous171722a with the same Rotate tool measured the values retained in native/baseline-now.jsonl; its Realistic average60.729/min55.586 also missed stable60. These data do not establish a causal slowdown or a net speedup. The candidate native ui.skin_batch remained around3.09–3.17ms in steady profile windows despite the isolated benchmark gain. Additional UI executor-vs-empty-cache instrumentation is needed before attributing the difference. Native performance acceptance and long stability remain incomplete; do not ship a claim of improved FPS.
