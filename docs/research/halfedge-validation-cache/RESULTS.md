# Isolated success-only shared half-edge validation prototype

Production is unchanged. This prototype uses post-COW baseline `aebf742495c006735eb12ae2538c62ccac4dfbda` (including the immutable snapshot API from3af118d), rather than the earlier owned-Vec implementation.

The private storage contains an owned HalfEdge vector and `OnceLock<()>`. Its read-only Deref exposes a slice; it has no DerefMut. The only mutable-vector accessor clears the success proof **before** returning the mutable borrow. All five constructor bindings and two existing test mutation sites pass through that accessor. A child-module privacy boundary prevents other Mesh modules from writing the vector/cache fields directly.

The shared storage caches only successful completion of the unchanged half-edge loop. Invalid graphs still return the original first error or panic on the original out-of-bounds indexing; failures are not cached. Concurrent initial readers may validate twice, then share the successful proof. Maps remain owned and every map count/index check runs on **every** Mesh validation call.

Mesh Clone shares the storage and proof. Storage Clone (used by Arc::make_mut) copies edges into a fresh empty proof. Value equality and Debug print only edges; serialization remains the original sequence and loading starts with no proof. The opaque snapshot keeps strong storage ownership and compares only connectivity. No epoch, naked allocation address, global registry, unsafe Send or skipped map validation is involved.

Correctness is green:270 unit/integration tests,0 failures,2 opt-in tests ignored,0 warnings. Explicit cases cover validation success followed by shared/unique mutation, valid repair, undo/snapshot behavior, cache-independent equality/Debug/JSON, map1 and map7 changes after a cached HE success, baseline error equality and repeated malformed-index panics. The original and candidate Mesh/Shape/Scene packages are separate real source builds linked into one probe.

The actual317-pose paired timing has **not run yet**: it is held until the other agent's full GPU quality run finishes. No performance or native FPS claim is made, and production promotion is not authorized by this document.

Reproduction uses the fixed baseline archive and changes only ignored target/:

```sh
python3 docs/research/halfedge-validation-cache/setup_probe.py
CARGO_TARGET_DIR="$PWD/target/halfedge-validation-repro/build" cargo test --manifest-path target/halfedge-validation-repro/Cargo.toml --release -j4 -p loom_mesh_cow -p loom_scene_cow -p cow_probe
LOOM_PERF_SCENE='/tmp/loom-conditioning-capture/input.loom' CARGO_TARGET_DIR="$PWD/target/halfedge-validation-repro/build" cargo test --manifest-path target/halfedge-validation-repro/Cargo.toml --release -j4 -p cow_probe actual_halfedge_validation_317 -- --ignored --nocapture
```
