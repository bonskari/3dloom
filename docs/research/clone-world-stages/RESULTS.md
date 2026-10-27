# Read-only Skin display clone/world-transform stages

No production source changed. The optimized probe ran on the actual saved Ray scene, all five Skin outputs at317integer/half-frame poses. Scene evaluation/matrix lookup, reference comparisons and serialization were outside the timed stages. A bounded four-worker pool was constructed before measurement. The application was paused and no other test/compiler jobs were visible during the7.02s run.

| Work per complete five-skin pose | Milliseconds |
|---|---:|
| Full Mesh clone on caller |0.924099|
| Existing transform_mesh (positions + invariant validation) |0.949502|
| Combined caller work |1.873601|
| Same full clone + same checked transform on four workers, including scope/sort |1.262063|
| Potential wall-clock saving |0.611538|
| Independently measured position transform only |0.085840|
| Independently measured invariant validation only |1.053093|

The independent position/check measurements use different cache conditions and must not be added to reconstruct the combined measurement. The clone/parallel stages have fixed ordering in this probe. This isolates CPU display preparation, not a native UI/FPS result. Inputs came from the serial evaluator outside measurement; production uses the already independently verified batch evaluator.

All1585 result meshes matched the existing checked-transform reference in complete Mesh equality (including topology, maps, edge metadata and vertex attributes); transformed position bits matched exactly. Input evaluated meshes and editor cages remained unchanged. Serialized full modifier data, including every Skin influence/weight, matched before/after the complete sweep.

Moving this unchanged output clone/checked-transform work to existing workers could save about0.6ms in this probe. It is insufficient by itself to turn the measured41native poses/sec into60. No validation was removed, no geometry/weights were reduced, and no production API or scheduling change is proposed as complete.

Reproduction from the repository root:

```sh
cargo test --release -p loom_bevy --lib --no-run
python3 docs/research/clone-world-stages/compile_probe.py
LOOM_PERF_SCENE='/mnt/2tb/hollowreach-ue/Hollowreach/SourceArt/RayLoom20261025/RayAnimationTest_LoomRig.loom' target/clone-world-probe/loom_bevy_tests actual_clone_world_317 --nocapture
```

The helper copies current Bevy source into ignored `target/`, appends only the source probe and links exact Cargo release dependency fingerprints. The explicit fingerprint path `loom_bevy-d37bfa3ba707912c` records this experiment's graph; update it if the graph changes.
