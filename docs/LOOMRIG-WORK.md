# LoomRig core

LoomRig is a non-destructive evaluation layer over existing bone NodeIds. It never replaces the hierarchy, Skin modifier, inverse bind matrices, or imported transform tracks. A neutral rig leaves source animation playing (limb IK weights start at zero). Metadata and ordinary Point helper nodes are serialized with the scene; command changes form undo groups.

## API

- `CreateLoomRig { root, mapping: Option<HumanoidMapping> }`: `None` characterizes MetaHuman names inside the selected root; explicit mapping supports arbitrary names. Missing, ambiguous, unrelated and noncontiguous bones are errors.
- `SetLoomRigMapping { rig, mapping }`: validates first, reuses controls by bone/role and limb name while preserving their native animation keys. Edits that would discard authored controls or change the rig root are rejected before mutation.
- `SetLoomRigLimb { rig, limb, solver, blend }`: accepts HI or Limb and finite 0–1 weights. Captures evaluated joint orientations into native keyed FK input offsets and matches goal/pole controls to the visible pose so current pose survives switching. Auto Key creates blend and match-offset keys at the current frame. Original bone keys are untouched.
- `RemoveLoomRig { rig }`: removes controls and metadata, preserves skeleton and skin.

Public serde types live in `loom_scene::loomrig`. `LoomRig.mapping.root`, `.master`, `.bindings` and `.limbs` supply UI access. Each limb exposes mapping, goal, pole, solver and blend. Each explicit limb is three contiguous joints (start/middle/end); extra arbitrary-length chains remain supported by the existing IK chain system.

Master uses a world transform delta converted through the skeleton's actual parent frame. Pelvis/spine/head and limb FK controls carry local additive offsets and display at the driven bone pivot. Limb goals are world-space position/orientation targets, enabling stationary feet in full IK; poles drive bend planes for both solvers. Goal helpers remain independent of skeleton parenting. FK offset controls follow native animated bones.

Pose matching stores native Euler rotation tracks as additive FK inputs before IK. The selected native blend track scales IK goal influence; moved goals still obey the IK constraint after solver switches. Skin uses the same evaluated world matrices as the viewport.

## Verification

Tests in `loom_scene/tests/loomrig.rs` and `loom_commands/tests/loomrig.rs` exercise actual evaluation, persistence, undo, validation, original transform preservation, switched-pose continuity, transformed parents, FK helper display, goal orientation and pole movement. Focused verification: scene 3/3 and command 11/11 pass, including a planted world goal while original keyed root motion continues, Auto Key blend interpolation, HI-to-Limb match followed by a moved constrained goal, scaled-parent IK, and preservation of authored control animation. Full suite was deferred at parent request until all shared changes stabilize.

## Integration still owned by parent

Lifecycle agent implemented reference remapping/pruning and deletion undo; reported persistence 6/6 plus adjacent file/merge 35/35 tests passing. Parent still owns integrated UI, real MetaHuman Body342 import/skin verification and broad suite.

## Independent review follow-up

Verified and fixed deserialized-scene IK cache initialization: both cache and edit revision previously defaulted to zero, allowing first pose evaluation after loading to skip the rig. Deserialized edit revision now starts at one. `Scene::revision()` exposes edit generation for UI caches and stays unchanged during playback.

A 3D HI pole regression reached the correct bend plane but stalled 0.006497 units from its target despite a 0.0001 threshold. The general glam arc constructor snaps sufficiently small angles to identity. HI CCD now retains those small arc corrections, with the existing opposite-vector fallback. Fresh command IK4/4, scene IK7/7, command LoomRig13/13 and scene LoomRig4/4 pass.

Opt-in `loomrig_real` test uses the real Ray MetaHuman FBX and Kimodo seated BVH through `LOOM_RETARGET_FBX` / `LOOM_RETARGET_BVH`. Verified342 original bones, automatic4limb characterization, unchanged neutral native animation, retargeted motion, unchanged native transforms after control edits, unchanged skin bindings/weights/hierarchy, and byte-identical original FBX. Both full-IK world feet remain planted across sampled source-motion frames (measured drift0.000074 and0.000018 scene units); left goal error0.000034. Moving the leg goal deformed1745 of3490 vertices in the actual leg-weighted `SkeletalMesh001`. Test selects positive weights on the controlled leg; choosing the largest mesh alone had originally measured zero changed vertices and was not a valid leg-deformation assertion. Successful real test runtime9.67s in debug.

Source clip moves/trims operate on clip-member node controllers; rig blend and FK input-offset controllers live separately in `Scene.loom_rigs` and do not implicitly shift with source clips.

## FK interaction audit

Actual command regressions reproduced self-frame feedback: the second world-space Move(+10) and Rotate(+15°) stayed at the first offset, and repeated absolute world type-in missed its target. The displayed FK helper follows the already corrected bone, so using that display parent as the stored offset's edit frame removed the existing correction during each edit.

`Scene::edit_parent_frame` removes the stored helper offset from the evaluated display frame for world-to-stored conversions. Native world move/rotation, absolute type-in and Parent/Gimbal reference axes use that edit frame; display parenting and native skeletal animation remain unchanged. Five interaction tests pass, covering rotated/scaled parent, animated source, Auto Key, original keys unchanged, direct local offsets, absolute world movement/rotation and undo/redo. Existing LoomRig13/13 and transform/frame29/29 pass. The actual gizmo pointer-step path passes accumulating three move steps and three rotation steps. UI test build retains existing compiler warnings; no new interaction-test warning was emitted.

## Playback priority and next interaction work (26 October)

User priority: at least 60 fps during actual Play on the current Ray scene,
preferably higher. Measure presented UI updates over wall time, not idle view
speed or a CPU-only microbenchmark. Preserve character geometry, materials and
all skin influences.

All added animation extras now open separately through Animation > Animation
Editor (Clips & IK/FK, Kimodo, LoomRig). The original Motion panel and main time
slider remain. Explicit Display Bones modes clear the conflicting Hide Bones
filter; a regression reproduces the formerly invisible skeleton.

After performance: selectable limb-shaped controls with Biped-style direct
manipulation and pose keys; planted, sliding and free contact keys. Planted keys
were explicitly confirmed as important. This interaction work is deferred while
the playback target remains unmet.

First measured intermediate build (UI/commands opt0, optimized scene/render):
Wireframe 180 updates / 12.018994 s = 14.976 fps; Realistic 57 / 12.150285 s =
4.691 fps. Both measured during actual Play using the diagnostics frame counter.
These are not final release numbers or an apples-to-apples comparison with the
user's earlier 1 fps report. Further corrections/full optimized build pending.

Latest actual Play measurement after lazy face buffers, persistent GPU streams,
frame-wide tangent workers and static editing-cage reuse: Wireframe 1441 updates
/12.008134 s = 120.002 fps; Realistic 119/12.031813 s = 9.890 fps. Camera and
geometry were unchanged between these two samples. No decimation or influence
truncation. Realistic still misses the 60 fps target. User confirmed both modes must meet
the target before Biped interaction work.

Broad Bevy multi_threaded caused a reproduced bake-shadow regression (5/6
failures). Narrow direct bevy_tasks/multi_threaded preserves workers without
changing ECS/render scheduling; bake passed six consecutive process runs with
that configuration. Do not re-enable broad Bevy parallel scheduling casually.

Final verification of the committed production path: full workspace 1664 passed,
zero failed, eight ignored. Geometry, skin data, skeleton, materials, rig mappings
and clips compare unchanged against SVN r1319. One pole controller now has an
additional constant-equivalent pose key at frame 133; that key is preserved.
Rejected halo tangent research is retained in docs/research/tangent-halo and is
not compiled into the application. Wireframe Play is left active for review.

2026-10-27 follow-up actual Play, installed build stamped 8e51788 (full opt3):
Realistic → Wireframe → Realistic → Wireframe, same camera and four-view layout.
Measured 196/12.002119 = 16.330 fps, 1431/12.006973 = 119.181 fps,
189/12.011321 = 15.735 fps, 1441/12.012696 = 119.956 fps. The unused-PBR
gate keeps Wireframe fast after switching from Realistic. Realistic remains
below target; no Biped work started. Geometry, materials and skin influences
remain intact. Current representative Realistic scopes: sync 32.4 ms, renderer
update 9.2 ms, UI world-mesh rebuild 10.5 ms. Exact storage-buffer PBR geometry
update is under GPU parity testing and is not yet installed or claimed faster.
