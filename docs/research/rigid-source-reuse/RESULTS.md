# Read-only immutable Arc bone reuse probe

No production changes were made. `probe.rs` was appended as a test module to a snapshot of the real `loom_bevy/src` under `target/rigid-source-probe/src`; the snapshot was compiled with optimized direct rustc and the exact Cargo release dependency fingerprints. The real scene was read through `LOOM_PERF_SCENE`. No geometry/math was substituted.

The saved scene supplied420 eligible bone cage meshes after excluding hidden nodes/layers, modifiers, instances and box-display geometry:2520 faces/3360 vertices. This probe does not reproduce parent-group or application category visibility;420 is an upper bound rather than a claim about the342 live-visible objects previously discussed.

After warming the actual GeometryCache,159 actual pose matrices gave:

| Work per complete bone set | Milliseconds |
|---|---:|
| Existing geometry preparation + content/material hashes + exact transformed-position bounds |0.239067|
| Retained strong Arc identity + same exact transformed-position bounds |0.021748|
| Saving |0.217319|

Separate phase instrumentation measured preparation0.161322ms, hashing0.066113ms and bounds0.032786ms. Those individual measurements include inner timer overhead; the total baseline above was measured separately without inner timers. Scene setup/evaluation and matrix construction were outside both measured intervals. The application was paused and other agents' jobs were stopped for the measurement. This is not an application FPS result.

A safe future interface would pass an optional borrowed `Arc<Mesh>` with a verified matching mesh payload, retain a strong Arc on the renderer object (preventing address-reuse ambiguity), and use `Arc::ptr_eq`. Copy-on-write edits then produce a different allocation. It must preserve the existing material/wire/mirrored-sign/deforming/visibility rebuild conditions and all transform/layer/shadow updates. Exact bounds require retained prepared positions and the existing transformed-position min/max; merely transforming local Aabb corners would enlarge bounds under rotations/shears.

The measured maximum gain is small relative to the remaining Realistic frame budget. Production API expansion is deferred; the probe is preserved for later use.

Reproduction from the repository root (requires the existing release test dependency fingerprints; first build `cargo test --release -p loom_bevy --lib --no-run` if needed):

```sh
python3 docs/research/rigid-source-reuse/compile_probe.py
LOOM_PERF_SCENE='/mnt/2tb/hollowreach-ue/Hollowreach/SourceArt/RayLoom20261025/RayAnimationTest_LoomRig.loom' target/rigid-source-probe/loom_bevy_tests actual_rigid_arc_probe --nocapture
```

The compiler helper copies source into ignored `target/`, appends only this probe module, and links the exact release dependency hashes. It does not modify production source. Its fingerprint path records the dependency graph used for this experiment (`loom_bevy-d37bfa3ba707912c`); update that explicit path if the graph changes.
