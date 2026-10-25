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
7. [x] Character timeline: trim source range, move clips and animate control corrections/contact changes; preserve timing and undo. Test sample equivalence and save/reload.
8. [x] Review integration, build/install locally, reload Ray test scene and verify real generated motion + controls. Save source and scene in their respective Git/SVN repositories; no public push.

## Current ledger
- Local main includes the real Kimodo API workflow, body-only profile, editable LoomRig, retargeting, movable/trimmed native clips, animated IK/FK tracks and evaluated FBX bake.
- Desktop Loom has been rebuilt and installed. `RayAnimationTest_LoomRig.loom` is open with actual Ray materials, 342 retained body bones, 61 mapped retarget joints, one rig and two clips. Desk/chair transforms were checked during preparation and saved-scene readback.
- Native playback changed 131 node transforms between observed frames; live IK/FK change and Undo succeeded. Synthetic/native-gizmo tests also cover repeated FK edits and skin deformation.
- Final full workspace: 1653 passed, zero failed, seven ignored. Actual-asset FBX regressions additionally cover material slot binding, source preservation and roundtrip export.
- Source and material-export checkpoint is SVN r1317; final corrected scene and preparation script are SVN r1318. Git changes remain local main, no push.
- Preview limits: Unreal screen-space eye-occlusion composition and hair grooms are not reproduced; original source shader data and full Unreal character are preserved. The timeline supports non-overlapping native clip sequencing, not an overlapping NLA crossfade mixer.
