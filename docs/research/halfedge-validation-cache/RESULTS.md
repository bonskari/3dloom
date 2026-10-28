# Isolated success-only shared half-edge validation prototype

The isolated experiment uses post-COW baseline `aebf742495c006735eb12ae2538c62ccac4dfbda` (including the immutable snapshot API from3af118d), rather than the earlier owned-Vec implementation.

The private storage contains an owned HalfEdge vector and `OnceLock<()>`. Its read-only Deref exposes a slice; it has no DerefMut. The only mutable-vector accessor clears the success proof **before** returning the mutable borrow. All five constructor bindings and two existing test mutation sites pass through that accessor. A child-module privacy boundary prevents other Mesh modules from writing the vector/cache fields directly.

The shared storage caches only successful completion of the unchanged half-edge loop. Invalid graphs still return the original first error or panic on the original out-of-bounds indexing; failures are not cached. Concurrent initial readers may validate twice, then share the successful proof. Maps remain owned and every map count/index check runs on **every** Mesh validation call.

Mesh Clone shares the storage and proof. Storage Clone (used by Arc::make_mut) copies edges into a fresh empty proof. Value equality and Debug print only edges; serialization remains the original sequence and loading starts with no proof. The opaque snapshot keeps strong storage ownership and compares only connectivity. No epoch, naked allocation address, global registry, unsafe Send or skipped map validation is involved.

Correctness is green:270 unit/integration tests,0 failures,2 opt-in tests ignored,0 warnings. Explicit cases cover validation success followed by shared/unique mutation, valid repair, undo/snapshot behavior, cache-independent equality/Debug/JSON, map1 and map7 changes after a cached HE success, baseline error equality and repeated malformed-index panics. The original and candidate Mesh/Shape/Scene packages are separate real source builds linked into one probe.

After the full GPU quality run finished, two quiet317-pose paired runs passed all1585 full StackValue/Mesh/map/vertex-bit/world-output comparisons and unchanged source/cage/weight checks. The same unchanged post-COW original and candidate packages alternate their complete pipeline each pose. The second run reverses the initial order. Both retain the independent ordinary serial reference and JSON compatibility checks.

| Stage | First original ms | First candidate ms | Reversed original ms | Reversed candidate ms |
|---|---:|---:|---:|---:|
|Skin batch|3.397596|3.632386|3.423698|3.618138|
|Display clone/checked world transform|1.062832|0.394405|1.065975|0.392784|
|Combined|4.463438|4.029788|4.493002|4.013984|

Combined savings were0.433650ms and0.479017ms; display-stage savings were0.668428ms and0.673191ms. Skin batch was slower by0.234790ms and0.194440ms despite no Skin arithmetic changes; that regression's cause was not isolated and is included in the net result. No cache/scheduling explanation is asserted as fact. Total test times were9.46s and9.17s. The application was paused, other agents held jobs, and no other compiler/test processes were visible. These are standalone paired CPU pipeline results, not native FPS measurements. The parent approved production promotion after both positive pairs and read-only review.

Reproduction uses the fixed baseline archive and changes only ignored target/:

```sh
python3 docs/research/halfedge-validation-cache/setup_probe.py
CARGO_TARGET_DIR="$PWD/target/halfedge-validation-repro/build" cargo test --manifest-path target/halfedge-validation-repro/Cargo.toml --release -j4 -p loom_mesh_cow -p loom_scene_cow -p cow_probe
# Add LOOM_REVERSE_ORDER=1 to reverse the initial pipeline order.
LOOM_PERF_SCENE='/tmp/loom-conditioning-capture/input.loom' CARGO_TARGET_DIR="$PWD/target/halfedge-validation-repro/build" cargo test --manifest-path target/halfedge-validation-repro/Cargo.toml --release -j4 -p cow_probe actual_halfedge_validation_317 -- --ignored --nocapture
```


## Production promotion

The exact tested four Mesh source files were promoted after verifying that production matched the recorded baseline. Production Mesh/Scene correctness then passed268 tests,0 failures,1 opt-in test ignored and0 warnings. Scene/Shape/runtime logic was not changed. All map checks remain unconditional. Native application FPS and pixel comparison for this promotion have not yet been measured.
