# Half-edge-only copy-on-write Mesh storage

Only the half-edge vector changes to `Arc<Vec<HalfEdge>>`. Vertices, faces, maps and invisible-edge flags remain owned as before. The public half-edge slice API and JSON array representation are unchanged. Builders acquire `Arc::make_mut` once before their construction loops; no per-edge atomic operation is added. Private tests cover sharing, position/face/UV/visibility edit independence, actual half-edge detachment, undo-style restore and JSON roundtrip. A separate original/Cow package test proves bidirectional old/new JSON compatibility.

The isolated workspace links genuine unchanged original Mesh/Shape/Scene packages and separate Cow package copies into one probe, alternating their complete pipeline order each pose. Original/Cow source is not selected by a fake deep-clone switch. The saved immutable17capture input passed317poses/five skins:1585 complete StackValue/meta/vertex-bit/topology/map/visibility comparisons, world outputs, unchanged cages/every weight, and actual JSON crossloads. Candidate half-edge pointers stayed shared with the immutable base through Skin and display clones.

| Complete five-skin pose | Original ms | Cow ms | Saved ms |
|---|---:|---:|---:|
|Skin batch|4.047414|3.736697|0.310717|
|Display clone + checked world transform on bounded workers|1.185305|1.111985|0.073320|
|Combined pipeline|5.235855|4.851894|0.383962|

The application was paused and other agents held jobs for the quiet9.95s measurement. Setup and reference verification are outside timings. This is a modest measured CPU pipeline gain, not a native FPS result or evidence that60fps has been achieved.

The isolated correctness run passed265 tests, with2 opt-in tests ignored. After promotion, the real production Mesh/Scene suites passed264 tests, with1 ignored; no Scene/Shape/runtime source was changed. The unchanged-source baseline is Git commit `dda8487d5468672fde9df7a28834f2210c456d3a` before this promotion.


## Actual clone payload and mutation scope

`payload.json` records the immutable capture SHA and exact element counts. Existing debug DWARF (`llvm-dwarfdump` on `target/debug/deps/libloom_mesh-ff9cbe32d97f34a4.rlib`) reported Vertex52B, HalfEdge24B, Face12B and Vec3 12B. These are live-element payload sizes on this x86_64 build, not measured physical bus traffic; Vec/BTreeMap headers, allocator overhead and spare capacity are excluded.

| Field, all five meshes | Elements | Payload bytes |
|---|---:|---:|
|Owned vertices (unchanged)|63340|3293680|
|Half-edges (now shared on Clone)|348474|8363376|
|Owned faces (unchanged)|111323|1335876|
|Owned UV1 values (unchanged)|63340|760080|
|Owned UV1 corner indices (unchanged)|348474|1393896|
|Invisible flags (empty)|0|0|

Total15146908bytes per complete five-Mesh clone; half-edges account for55.215%. Sharing every originally suggested field would cover78.255%, but faces/maps/visibility were deliberately not changed. Avoided payload is not used to predict an FPS gain.

The half-edge write audit found18 direct writes (8 pushes,10 field assignments), all in five builders across indexed.rs, primitives.rs and standard_primitives.rs. There are no other whole-field replacement sites or Mesh literal initializers for this field. A single `for ... in &self.half_edges` read changes to `.iter()`. Mesh::default/Clone/PartialEq retain value semantics; field-only serde preserves the sequence format without enabling global serde Rc support. One Arc::make_mut binding per builder avoids a new atomic check for every constructed edge.

An opaque retained strong-Arc handle could later prove just this table unchanged, preventing address-reuse ambiguity. It cannot prove vertex.he, Face fields, map data, visibility or part descriptors: those remain separate checks. No validation/descriptor shortcut is part of this change.

## Reproduce the paired source test

The setup script archives the recorded pre-promotion Git baseline and creates original/Cow package copies only under ignored target/. It applies the retained four-file patch; original sources remain byte-identical. Cargo dependencies and output artifacts are isolated from the application's target. The original measurement seeded its isolated release cache with Btrfs reflinks; that is optional for reproducibility and does not change the source experiment.

```sh
python3 docs/research/halfedge-cow/setup_probe.py
CARGO_TARGET_DIR="$PWD/target/mesh-halfedge-cow-repro/build" cargo test --manifest-path target/mesh-halfedge-cow-repro/Cargo.toml --release -j4 -p loom_mesh_cow -p loom_scene_cow -p cow_probe
LOOM_PERF_SCENE='/tmp/loom-conditioning-capture/input.loom' CARGO_TARGET_DIR="$PWD/target/mesh-halfedge-cow-repro/build" cargo test --manifest-path target/mesh-halfedge-cow-repro/Cargo.toml --release -j4 -p cow_probe actual_halfedge_cow_317 -- --ignored --nocapture
python3 docs/research/halfedge-cow/count_payload.py /tmp/loom-conditioning-capture/input.loom
```
