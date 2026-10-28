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

1. Desktop minimum performance/stability gate delivered locally in f8bd88d. Installed default-unset release measured63.6553Realistic and93.4861Wireframe ready submitted pose times/s, minimum accepted active windows61.7019/89.0491. Original source hash, all node/selection/frame records and four-view frame39 pixels were preserved;5GPUobjects/13parts and all geometry/materials/influences remain. Separate15-minute edge and combined-fused reliability runs completed with no pose errors/recoveries and verified real scene-equal auto-backups.434UI/89Bevy tests plus actual317-pose runtime history/bounds and full GPU-byte gates support correctness. Preferred120fps and nativeMac certification remain incomplete. Evidence: docs/research/cpu-edge-batch/ and docs/research/fused-pose-positions/production-defaults/. Preserve these properties and rerun native throughput after animation changes; this is not completion of the broader goal.
2. Restore reliable skeleton display and selection. Bone/geometry category independence, bulk buttons, on-top generic axes and matching through-skin selection have regression coverage. Generic axes are not limb-shaped Biped controls; those and final interactive usability checks remain incomplete.
3. Complete Biped-like LoomRig controls using the retained imported skeleton, editable chains/IK/FK, and pose plus planted/sliding/free keys. Existing Point controls and IK/FK tracks do not satisfy these requirements.
4. Keep animation authoring in separately opened tools. Verify actual Kimodo generation, local API preferences, movable clips, materials, and exact desk/chair preservation in the final installed application.
5. Commit locally to main, never push. Keep the goal active until the full user requirements are verified.

## Next animation phase: source audit, not completed functionality

The desktop minimum native performance/reliability gate has passed; continue with the remaining animation tasks below. Mac native verification remains pending.

- The generic Set Keys pose buffer (`anim_cmds.rs:553–600`) records node tracks. It does not capture LoomRig `blend` or `pose_offsets`; those require an atomic rig-pose-key operation alongside control transforms. Preserve source bone controllers rather than baking corrections into them.
- Existing limb blend-key commands, undo, key movement and the separate Animation Editor remain the base. Current clip movement moves member-node keys; rig metadata tracks are independent and must not accidentally move with a source clip.
- Proposed persistent contact data: a default-empty, typed per-limb contact track with discrete HOLD semantics. Before its first key the state is Free. Plant/Slide use effective IK=1; Free restores the authored blend curve. This is a pending design/implementation, not present behavior.
- Plant captures the currently solved end-pivot world position and orientation. Slide constrains the authored goal to an explicit plane while retaining tangential movement. The existing world-goal/end-orientation path requires real contact tests before promising this behavior. An end pivot is not an automatically detected foot sole or terrain contact.
- A typed contact lookup must notify `anim::note_animated_read()` when time-dependent, or the IK cache may reuse a static-scene result at another time.
- Required tests include planted position/orientation under root motion, tilted-plane sliding, exact Free/empty-track compatibility, fractional/backward/random-access time evaluation without a scene-version edit, undo/reload, atomic key collision rejection, clip movement, and explicit nonuniform-scale/shear orientation behavior.
- Limb-shaped controls still need matching draw/pick geometry mapped onto the existing control IDs. The repaired generic bone-axis overlay is skeleton visibility, not completion of Biped-style LoomRig controls.

## Added user requirements — macOS and touchpads

User explicitly requested macOS support and confirmed touchpad navigation for both macOS and Linux. Keep these within the active work scope after the performance/stability gate. Audit native build/package/runtime paths and Metal capability fallback without reducing geometry or weights. Add usable two-finger pan, pinch zoom and modifier-assisted orbit while preserving mouse controls and ensuring navigation affects only the hovered viewport. Verify event routing through the real input integration, not just a mathematical helper. Actual macOS build/runtime evidence is required before claiming Mac support complete; Linux-only tests cannot certify it.

## Task 9: limb-shaped FK display and direct selection

**Files:** Create `crates/loom_ui/src/loomrig_overlay.rs`; modify `lib.rs`, `helper_overlay.rs`, `viewport_widget.rs`.
**Interfaces:** `ControlProxy { control:NodeId, vertices:[Vec3;8] }`; `ControlProxy::segments(&self)` iterates fixed shared box-edge indices; `prepare(scene:&Scene, out:&mut Vec<ControlProxy>)` fills pose-dependent world vertices for each existing upper/lower FK binding. `LoomApp.rig_controls` retains the buffer and rebuilds with world meshes, never in draw/pick calls.

- [ ] RED: tests cover two FK spans per mapped limb, current solved endpoint alignment under authored offsets/nonuniform and mirrored transforms, zero-length finite handling, no serialized-node/rig changes, retained topology/storage across poses, no new animated reads in draw/pick, direct through-skin limb-outline clicks selecting the existing control ID, hidden/frozen/category and material-drop behavior.
- [ ] GREEN: use a shared canonical tapered eight-vertex/twelve-edge limb cage and a pose frame derived from actual mapped endpoints plus current control orientation. Do not change source control transforms to align visuals. Keep all existing Point mesh/helper geometry; add meaningful limb outlines rather than removing geometry to increase FPS. World-vertex work occurs once per pose, not once per viewport.
- [ ] Integrate on-top drawing in both shading modes and corresponding control-hit priority before generic bone axes in object selection. Keep `node_at` on surface picking. Existing control IDs route to current gizmo/FK commands; goals/poles/master and IK/FK authoring remain available in separate Animation Editor.
- [ ] Verify `cargo test -p loom_ui --lib loomrig_overlay -j4 -- --test-threads=1` and focused viewport/helper tests; full UI suite; actual Ray source/skin records and native direct click/edit/Undo; default native throughput remains≥60 in both views. No claim of full Biped/contacts completion from this first visual/selection increment.
- [ ] Local main commit with evidence; no push. Continue remaining end/body/goal interactions and contact/pose/timeline requirements afterward.
