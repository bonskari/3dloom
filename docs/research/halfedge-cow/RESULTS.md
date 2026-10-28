# Half-edge-only copy-on-write Mesh storage

Only the half-edge vector changes to `Arc<Vec<HalfEdge>>`. Vertices, faces, maps and invisible-edge flags remain owned as before. The public half-edge slice API and JSON array representation are unchanged. Builders acquire `Arc::make_mut` once before their construction loops; no per-edge atomic operation is added. Private tests cover sharing, position/face/UV/visibility edit independence, actual half-edge detachment, undo-style restore and JSON roundtrip. A separate original/Cow package test proves bidirectional old/new JSON compatibility.

The isolated workspace links genuine unchanged original Mesh/Shape/Scene packages and separate Cow package copies into one probe, alternating their complete pipeline order each pose. Original/Cow source is not selected by a fake deep-clone switch. The saved immutable17capture input passed317poses/five skins:1585 complete StackValue/meta/vertex-bit/topology/map/visibility comparisons, world outputs, unchanged cages/every weight, and actual JSON crossloads. Candidate half-edge pointers stayed shared with the immutable base through Skin and display clones.

| Complete five-skin pose | Original ms | Cow ms | Saved ms |
|---|---:|---:|---:|
|Skin batch|4.047414|3.736697|0.310717|
|Display clone + checked world transform on bounded workers|1.185305|1.111985|0.073320|
|Combined pipeline|5.235855|4.851894|0.383962|

The application was paused and other agents held jobs for the quiet9.95s measurement. Setup and reference verification are outside timings. This is a modest measured CPU pipeline gain, not a native FPS result or evidence that60fps has been achieved.

The isolated correctness run passed265 tests, with2 opt-in tests ignored. After promotion, the real production Mesh/Scene suites passed264 tests, with1 ignored; no Scene/Shape/runtime source was changed. Reproducible source setup and payload inventory follow in a documentation-only update. The unchanged-source baseline is Git commit `dda8487d5468672fde9df7a28834f2210c456d3a` before this promotion.
