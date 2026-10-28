# Current-position GPU edge gather: bounded design

Design only; no renderer implementation or GPU experiment is included. Native
profiling reports about 1.8 ms in CPU edge staging and 1.3 ms in queue uploads
per frame across the viewport meshes. Those costs are not the proven savings
of this proposal. The existing runtime covers eligible deforming objects, not
all roughly 461 visible meshes; unchanged rigid/helper paths retain their cost.

## Reuse the actual source, with a current-pose proof

`GpuPoseRuntime.source` (`gpu_pose_runtime.rs:24,42,248`) contains **every** current
CPU world-baked mesh vertex as a vec4, including vertices unused by visible faces.
It has STORAGE usage and is already updated with the real CPU editor geometry.
Expose an owned cloned Buffer handle plus vertex count/object identity/current
successful-sync epoch. Do not use the expanded corner output, which lacks some
source vertices. Do not transform source positions: direct bit gathering avoids
additional arithmetic, including changes to signed zero.

The renderer cache is private (`gpu_pose_renderer.rs:18`) and may contain entries
after an update/guard/commit failure. Cache presence, matching vertex count, or a
previous successful frame is not sufficient evidence that a buffer is current.
Publish packets only for objects in the latest successful whole-pose result,
after all descriptor/layout/recipe guards and any recovery have succeeded.
Invalidate availability at sync start and on errors, CPU fallback, object removal,
cache replacement, geometry edits and backend disable. Failed sync must reject
GPU edge reuse for that epoch, while the ordinary current CPU upload remains valid.

`Gpu::Running(BevyView)` (`realistic.rs:15,29`) can forward packets from the renderer.
The UI attaches its exact `world_version` and NodeIds. `PendingUpload`
(`loom_ui/src/lib.rs:77,962`) currently carries mesh/style arrays without either;
its payload must associate each display-mesh index with its NodeId and epoch.
A callback uses a packet only when all three match its retained CPU display mesh.
The packet must come from the SAME displayed mesh, not a cage or a rigid bone's
local mesh. CPU meshes, picking, bounds and subobject tools remain current.

Cloning a Buffer keeps storage alive; it does not freeze contents. The frame
contract must ensure no next-epoch source update is submitted before the current
callback's gather/render commands. Preserve the existing same-device/queue,
serial frame ordering; do not add an asynchronous reader of mutable source state.

## Exact edge plan and output

Build an immutable ordered endpoint-index array using precisely the current
`stage_edges` rule (`mesh_buffer.rs`): iterate halfedges in order, select
`i < twin`, apply effective edge visibility, emit `origin` then `twin.origin`.
Validate indices/capacity/storage limits before replacing a current draw buffer.

A cached plan needs exact origin/twin and effective visibility comparison,
excluding positions. `HalfEdge` has no visibility field (`half_edge.rs:44`);
`Mesh::edge_visible` and `has_invisible_edges` use a separate private vector
(`mesh.rs:443,462`). Existing geometry `DescriptorSnapshot` compares halfedges,
faces and maps but does not prove visibility. Snapshot the effective visibility
of relevant halfedges and compare it exactly each pose, without hashing as proof.
Measure that guard's cost; it can consume the CPU saving being sought.

The gather kernel reads source positions and writes the existing edge buffer.
Give that target VERTEX|STORAGE usage, and bind it again after target growth.
`LineVertex` is 24 bytes: three position f32 followed by three color f32.
WGSL must use an `array<u32>` and six scalar stores per vertex. A struct with two
vec3 fields would have a different storage stride. Read source position bits as
u32 and copy them unchanged; write the exact current wire-color bits from a
persistent per-object parameter block. No interpolation or transform is needed.
Do not queue-write one shared color parameter repeatedly before multiple dispatches;
each draw must retain its own parameters or use distinct batch parameter offsets.

Selected edges already use the constant-white shader on this same VBO. Ordinary
edges retain their color. Material-wire lines and face/index buffers remain on
the existing path. Initially `style.diagonals == true` retains its full CPU edge
path: fan/invisible diagonal selection and dashed interpolation are unchanged.
Other ineligible/empty/unsupported meshes likewise keep the existing full-fidelity
path. No styles, geometry or subobject lines are disabled to obtain eligibility.

## Smallest integration boundary

Put the gather kernel/edge plan beside `MeshBuffer` in loom_render. It accepts a
buffer descriptor packet rather than depending on loom_bevy; Bevy already depends
on loom_render. Add a MeshBuffer update entry that can skip ONLY ordinary edge
staging/upload when a validated gather will replace that stream. Bounds, faces,
material-wire output and style/count metadata retain their current updates.

`ViewportCallback::prepare` (`viewport_widget.rs:176`) already receives an encoder.
Encode gather before the viewport's draw, after same-epoch source uploads; normal
wgpu buffer hazards apply. Reuse pipeline/bindings/endpoint storage across poses.
No GPU readback or additional CPU position upload is required on this path.

Estimated change scope, not measured effort: one render compute/plan module and
MeshBuffer entry; runtime/renderer source-packet access; Gpu forwarding; typed
pending-upload identity/epoch payload; callback dispatch and regression tests.
This crosses Render, Bevy and UI ownership and should be implemented in stages.

## Gates before activation

- Byte-exact CPU/GPU position/color streams, including signed zero, unused vertices,
  border edges, invisible-edge edits, topology reorder/growth/shrink and color edits.
- Exact selected/default/subobject pixels with positive controls in both formats.
- Diagonal/material-wire/shaded CPU paths unchanged; mixed eligible/ineligible
  scenes and multiple viewports preserve ordering and source identity.
- Rejected/missing/stale epoch, failed sync/guard recovery, removed/recreated object,
  reordered display array and resized target cannot consume a stale source buffer.
- Actual Ray all 317 poses: full output-stream parity, whole-pose guards retained,
  then quiet end-to-end callback upload/guard/dispatch timing and true native FPS.
  Kernel timestamps alone cannot establish a gain.
