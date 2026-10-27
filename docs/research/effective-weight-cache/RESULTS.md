# Rejected exact effective-weight cache prototype

This experiment changed only a copied Core under ignored `target/`; no production source was modified. The cache is **not shipped** because the complete measured batch became slower.

The Scene-owned serde-skipped map retains per-node raw weight indices/f32 bits/length/order, bone count, influence limit, rigid_all/rigid flags and explicit flags. It is used only by eligible single-Skin EditablePoly/Mesh jobs with matching counts, all explicit vertices and no patch. Entries retain an Arc of the original `point_weights_into` results, preserving influence ordering and f32 normalization semantics. The blend still uses current matrices, positions and gizmos; metadata behavior is unchanged. The ordinary serial evaluator remains uncached. Nonfinite input weights fall back; signed-zero changes conservatively miss.

## Correctness

Synthetic checks passed same-count weight value/order edits, individual/all rigid flags, influence limit, bone-count changes, explicit/count fallbacks, NaN/Infinity fallbacks and ±0 changes. Changed relevant content could not reuse the original Arc. Every cached application matched the ordinary uncached application.

The saved Ray scene passed all317 integer/half-frame poses/five skins (1585 outputs). Complete StackValues/meta/warnings/selections/shape/patch state, complete Mesh contents, vertex float bits and every map value bit matched both uncached batch and uncached serial results. Editor cages and serialized modifier data/every weight remained unchanged. Scene serialization matched the uncached scene, confirming that the cache does not enter saved data.

## Quiet paired result

The app was paused; other agents held jobs. The same bounded four-worker pool ran both batches, alternating their order each pose; setup/verification were outside the timed batch calls. The candidate timing includes cache lookup, eligibility checking, exact raw-content validation, cold construction, preparation, executor and publication.

| Per complete five-skin pose | ms |
|---|---:|
| Uncached batch |3.109006|
| Cached batch |4.123250|
| Candidate regression |+1.014244|
| Included serial main-thread guard |1.276544|
| First cold candidate pose |7.776024|

There were1580 hits and5 cold misses. The entire317-pose test took8.11s. These standalone absolute times are not directly interchangeable with native UI timing or the earlier stage harness. The measured raw-content proof costs enough to reject this implementation despite correct results. No revision-only key or omitted guard was substituted to manufacture a gain.

## Reproduction

```sh
python3 docs/research/effective-weight-cache/compile_probe.py
target/effective-weights-probe/loom_scene_tests exact_effective_cache_edits --nocapture
LOOM_PERF_SCENE='/mnt/2tb/hollowreach-ue/Hollowreach/SourceArt/RayLoom20261025/RayAnimationTest_LoomRig.loom' target/effective-weights-probe/loom_scene_tests actual_effective_weight_cache_317 --nocapture
```

The helper copies current Scene source into ignored `target/`, applies the retained narrow patch, adds only the cache/probe modules and links the exact Cargo release dependencies. Its explicit Bevy-test fingerprint is `loom_bevy-d37bfa3ba707912c`; build those release dependencies or update that explicit path if the graph changes. A source mismatch makes patch application fail. The original prototype source, patch and complete short result logs are retained here rather than discarded.
