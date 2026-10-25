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
8. [ ] Review integration, build/install locally, reload Ray test scene and verify real generated motion + controls. Save source and scene in their respective Git/SVN repositories; no public push.

## Current ledger
- Baseline local main 1440c88. Running app first release with clips, saved Ray scene loaded; body-only core exists but not integrated.
- Independent workers: body-import owns FBX exchange/import options; rig-core owns scene rig/evaluation/commands; retarget owns new retarget module/tests. Parent owns UI, timeline and live integration. Shared registration edits coordinated.
- Ruling: continue implementation without another approval round, as explicitly requested by user.
- Review focus: existing animation with neutral controls; world placement/unit conversion; deletion/undo stale references; source/target node ownership; live solver changes on nonzero frames.

- Resumed after explicit stop: source modifications and local commits preserved; native bake and final integration resumed. Material Editor/Crop background loading is now implemented with focused tests; final workspace run and visible scene verification remain outstanding.
