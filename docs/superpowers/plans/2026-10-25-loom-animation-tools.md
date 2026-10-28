# Loom animation tools implementation plan

**Goal:** Author Ray's body animation in local 3D Loom using Kimodo source motions, an editable control rig and movable clips.
**Architecture:** Retain imported skeleton, skin bindings and native controllers. Characterization maps semantic body joints onto existing bones; independent controls add reversible pose corrections. Kimodo uses the configured local API and durable GPU queue.
**Spec:** User instructions in this task and docs/KIMODO.md.

## Constraints
- Work directly in local main, no push. Preserve all existing character/scene work.
- No face control rig or facial animation; preserve head geometry and neck/head body motion.
- API URL is a local preference only. Real generation and assets only.
- Direct app commands; no desktop keyboard/mouse interception.
- Every operation undoable and serialized, clear errors before partial mutation.

## Deliverables and verification
1. [x] Real Kimodo generation, API settings and native BVH import. Real queued service test passed.
2. [x] FPS-safe BVH/FBX source clips and timeline dragging, import/undo/project identity tests.
3. [x] Finish native MetaHuman body-only import option. Test original Ray FBX, reference surface and neck deformation; update saved test scene with desk/chair transforms preserved.
4. [x] LoomRig scene model and commands: characterization, master/pelvis/spine/head, limb goal and pole controls, editable mappings, live IK/FK/solver matching. Verify original controllers/bindings survive, controls affect evaluated skin, undo and serialization.
5. [x] Motion-panel LoomRig UI using actual commands: creation, control selection, chain/mapping edits, pose match and solver settings. Headless egui interaction tests.
6. [x] Real Kimodo/MetaHuman retarget: reference-frame/proportion correction, original placement and skin preserved. Verify actual source samples and target geometry; expose in Motion UI.
7. [ ] Character timeline: trim source range, move clips and animate control corrections/contact changes; preserve timing and undo. Clip sequencing and animated IK/FK exist; typed planted/sliding/free contacts and their editing/verification remain incomplete.
8. [x] Review integration, build/install locally, reload Ray test scene and verify real generated motion + controls. Save source and scene in their respective Git/SVN repositories; no public push.

## Current ledger
- Local main includes the real Kimodo API workflow, body-only profile, editable LoomRig, retargeting, movable/trimmed native clips, animated IK/FK tracks and evaluated FBX bake.
- Desktop Loom has been rebuilt and installed. `RayAnimationTest_LoomRig.loom` is open with actual Ray materials, 342 retained body bones, 61 mapped retarget joints, one rig and two clips. Desk/chair transforms were checked during preparation and saved-scene readback.
- Native playback changed 131 node transforms between observed frames; live IK/FK change and Undo succeeded. Synthetic/native-gizmo tests also cover repeated FK edits and skin deformation.
- Final full workspace: 1653 passed, zero failed, seven ignored. Actual-asset FBX regressions additionally cover material slot binding, source preservation and roundtrip export.
- Source and material-export checkpoint is SVN r1317; final corrected scene and preparation script are SVN r1318. Git changes remain local main, no push.
- Preview limits: Unreal screen-space eye-occlusion composition and hair grooms are not reproduced; original source shader data and full Unreal character are preserved. The timeline supports non-overlapping native clip sequencing, not an overlapping NLA crossfade mixer.

## Revalidated priorities — 2026-10-28

Earlier checked items record the initial implementation, not completion of the expanded current user goal.

1. Keep full geometry/materials/influences and verify distinct ready-pose throughput in both shading modes. Snapshot adoption removed the measured repeated half-edge comparison cost; the15minute post-fix reliability run completed without pose errors and verified a real complete auto-backup. Current controlled installed tests still miss the performance gate: new5f91fresh59.83/reload57.42Realistic, versus old fixed binary58.76/57.54 under the same policy. Wireframe86.9–90.1. Do not attribute this to touchpad changes from these results. Retained topology profiles stay cheap after reload. A single-weight CPU specialization was rejected after exact parity passed but wholebatch slowed2.643→3.059ms. Actual native counters established 450 nonempty CPU edge writes and 1,069,632 bytes per pose. Experimental exact CPU-edge batching is committed default-off in 0b4adc1: 36 render tests plus the UI GPU callback test passed. First same-binary native off/on pair measured Realistic59.07→62.00 and Wireframe91.29→91.68; four viewport crops at frame39 were pixel-exact and all node/source data were preserved. Realistic minimum active window58.71 still misses sustained60; the reversed second pair also improved Realistic60.25→61.53, but its minimum58.53 still misses sustained60. Profiled native counters confirmed450→1 actual CPU edge writes per pose with the same1,069,632bytes. Reliability/default promotion remains pending. A separate fused-current-position candidate is committed default-off in747cb9d: real317-pose runtime history/bounds bits, repeated same poses, six-sample full GPU batch/serial records, atomic invalid-later-part rejection, and88-test Bevy suites in both modes passed. Its flagged native on-runs measured62.34/63.36Realistic (minimum60.84/61.01) and90.65/91.83Wireframe, all accepted active windows≥60; reversed controls showed the same direction of Realistic improvement. Four-view frame39 pixels were exact. Edge batching separately passed15minutes plus a real scene-equal auto-backup. The combined fused15minute run remains active. Commit f8bd88d promotes both defaults after434UI/89Bevy and real-Ray GPU gates without overrides; its release build and default-unset native acceptance remain pending. Do not claim the whole performance/stability phase delivered before those gates finish. See docs/research/fused-pose-positions/. See docs/research/cpu-edge-batch/. Do not promote this first ordered pair to full performance acceptance. No geometry/weights/material reduction. Evidence: `docs/research/touchpad-navigation/native/`, `docs/research/single-explicit-weight/`.
2. Restore reliable skeleton display and selection. Bone/geometry category independence, bulk buttons, on-top generic axes and matching through-skin selection have regression coverage. Generic axes are not limb-shaped Biped controls; those and final interactive usability checks remain incomplete.
3. Complete Biped-like LoomRig controls using the retained imported skeleton, editable chains/IK/FK, and pose plus planted/sliding/free keys. Existing Point controls and IK/FK tracks do not satisfy these requirements.
4. Keep animation authoring in separately opened tools. Verify actual Kimodo generation, local API preferences, movable clips, materials, and exact desk/chair preservation in the final installed application.
5. Commit locally to main, never push. Keep the goal active until the full user requirements are verified.

## Next animation phase: source audit, not completed functionality

Implementation remains behind the native performance/reliability gate.

- The generic Set Keys pose buffer (`anim_cmds.rs:553–600`) records node tracks. It does not capture LoomRig `blend` or `pose_offsets`; those require an atomic rig-pose-key operation alongside control transforms. Preserve source bone controllers rather than baking corrections into them.
- Existing limb blend-key commands, undo, key movement and the separate Animation Editor remain the base. Current clip movement moves member-node keys; rig metadata tracks are independent and must not accidentally move with a source clip.
- Proposed persistent contact data: a default-empty, typed per-limb contact track with discrete HOLD semantics. Before its first key the state is Free. Plant/Slide use effective IK=1; Free restores the authored blend curve. This is a pending design/implementation, not present behavior.
- Plant captures the currently solved end-pivot world position and orientation. Slide constrains the authored goal to an explicit plane while retaining tangential movement. The existing world-goal/end-orientation path requires real contact tests before promising this behavior. An end pivot is not an automatically detected foot sole or terrain contact.
- A typed contact lookup must notify `anim::note_animated_read()` when time-dependent, or the IK cache may reuse a static-scene result at another time.
- Required tests include planted position/orientation under root motion, tilted-plane sliding, exact Free/empty-track compatibility, fractional/backward/random-access time evaluation without a scene-version edit, undo/reload, atomic key collision rejection, clip movement, and explicit nonuniform-scale/shear orientation behavior.
- Limb-shaped controls still need matching draw/pick geometry mapped onto the existing control IDs. The repaired generic bone-axis overlay is skeleton visibility, not completion of Biped-style LoomRig controls.

## Added user requirements — macOS and touchpads

User explicitly requested macOS support and confirmed touchpad navigation for both macOS and Linux. Keep these within the active work scope after the performance/stability gate. Audit native build/package/runtime paths and Metal capability fallback without reducing geometry or weights. Add usable two-finger pan, pinch zoom and modifier-assisted orbit while preserving mouse controls and ensuring navigation affects only the hovered viewport. Verify event routing through the real input integration, not just a mathematical helper. Actual macOS build/runtime evidence is required before claiming Mac support complete; Linux-only tests cannot certify it.
