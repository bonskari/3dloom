# Read-only retained half-edge snapshot identity probe

No production, installed application or scene change was made. The standalone probe is CPU-only: no Device/Queue creation or GPU work. Compile93592 and process82842 both ended successfully. Root explicitly permitted CPU work during the15minute reliability run; the app was animating during this probe, so these are paired predicate costs under concurrent native use, not quiet FPS or a causal actor diagnosis. Root retained the first17~55FPS stability samples before our probe. All jobs were terminal before the subsequent quiet95539 native window.

## Proven source behavior

- loom_mesh/src/mesh.rs:56: HalfEdgeSnapshot::matches performs Arc::ptr_eq OR complete HalfEdge slice value equality. Its current implementation only reads the retained Arc; a value-equal different allocation is not adopted.
- loom_bevy/src/gpu_pose_geometry.rs:157: DescriptorSnapshot adds vertex HE IDs, faces, UV/explicit-normal bitwise comparisons. matches_descriptors returns a hit without refreshing this snapshot. Runtime::update only rebuilds on a miss.
- loom_render/src/gpu_edges.rs:246: GpuEdges::matches_geometry retains its snapshot on a value-equal hit and independently checks effective visibility. UI viewport_widget.rs:199 retains cached edges for IDs still present; reuse additionally requires the same current source-buffer/range identity.
- loom_bevy/src/gpu_pose_renderer.rs:154/197 retains/reuses runtime caches by(hidden,nodeID) when IDs remain present. This provides a possible same-ID reload persistence route; the probe does not introspect the installed actor to establish that it took it.
- loom_bevy/src/prepared_geometry.rs:18 already uses Vec<HalfEdge> equality every time. It has no Arc identity shortcut whose loss could explain a new reload-dependent path.

## Selection and reload inspection

OpenFile execute.rs:168 reads a new document; load_document:3676 assigns the new Scene. Deserializing half-edge storage creates new Arc/Vec storage even when values match. The command-side loader resets selection/history and marks mesh_dirty; it does not itself own/reset the external UI renderer caches.

SelectNodes execute.rs:3091 updates node-ID BTreeSets, retains SubSelection entries and refreshes center mode. SubSelection::clear/retain_nodes state.rs:1207/1238 modify only selection BTreeMap/BTreeSets; they do not mutate Mesh or HE storage directly. Selection undo snapshots contain selection IDs, not meshes. The execute wrapper still calls stack::sync_selection:780: conditional editable-poly/selection-modifier passes_up/stored-selection changes can call Scene::node_mut and invalidate evaluated geometry. No direct HE write occurs in the examined selection routines, but no actual SelectNodes command/COW pointer trace was executed here, so indirect reevaluation is not ruled out. transform_mesh edit_ops.rs:45 changes vertex positions and validates topology; it does not mutate HE data. Skin source inspection found no HE write.

## Actual data and contracts

The known frozen input path is `/tmp/loom-conditioning-capture/input.loom`, previously verified SHA25684fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935; it was not rehashed during this run. The probe evaluates5actual Skin meshes atframe39 in world space, then serializes/deserializes each Mesh to fresh equal storage. This models fresh HE allocation; it is not an actual installed OpenFile command or actor-memory trace.

63,340vertices /111,323faces /348,474halfedges. Complete Mesh value equality passed and every old/fresh HE-slice pointer differed. HE equality is equality of all stored origin/twin/next/prev/face fields, not an unsafe comparison of Rust padding bytes. The exact GPU DescriptorSnapshot source and exact GpuEdges geometry-predicate body are copied as CPU-only guard structs in main.rs; the actual public HalfEdgeSnapshot::matches is called directly from loom_mesh. No fake numeric geometry is substituted for the actual meshes. The geometry-cache comparison copies only its real topology predicate, excluding face-normal/fan preparation. All5edge guards had all_visible=true in this input.

Controls: changing only positions remains accepted by these intentionally position-independent topology/descriptor guards; changing one stored HE origin without changing counts is rejected by all three. That negative control is a stored-field mutation, not a claim that the altered topology is a valid editable mesh. Full source-epoch/freshness and other current descriptor guards remain outside the HE snapshot and must not be weakened.

## Paired CPU costs

32ABBA rounds ×16repeats per timed batch across all5meshes. A=old snapshot/old shared mesh, B=old snapshot/fresh value-equal mesh. A separately constructed fresh snapshot/fresh mesh is also measured; this is a baseline identity observation, not an implemented adoption fix.

| Predicate (ms /5meshes) | Shared | Retained old→fresh equal | Fresh snapshot→fresh |
|---|---:|---:|---:|
| HalfEdgeSnapshot |0.000009|0.362507|0.000019|
| GPU descriptor |0.177981|0.645474|0.174660|
| GPU edge geometry |0.000020|0.362556|0.000032|
| Bevy topology Vec guard |0.479358|0.467861|0.506259|

The two GPU-guard models add0.467493+0.362536=0.830029ms per five-mesh round in this probe. The Vec guard shows no new Arc-dependent cost. The source and repeated timings establish the persistent deep-comparison condition for equal fresh allocations. They do NOT establish installed actor cache identity, actual per-frame multiplicity/cache-temperature costs, or that this accounts for the entire observed~55FPS state. Controlled same-binary restart/OpenFile/selection A/B remains necessary. No fix or proposed lock/alias cache was implemented or benchmarked.

## Reproduction

main.rs, build.py, args.json and source-manifest.json record the runnable local standalone source/dependency setup. Build.py requires the matching local release fingerprints/rlibs. Source predicates are retained exactly in the artifact; no production file is patched.

```
python3 /tmp/loom-half-edge-identity/build.py
LOOM_PERF_SCENE=/tmp/loom-conditioning-capture/input.loom /tmp/loom-half-edge-identity/probe
```

Do not run this CPU cost probe during a quiet native FPS window.
