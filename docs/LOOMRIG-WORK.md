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
