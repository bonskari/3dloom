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

Measurement correction (27 October, fractional-playback audit): previous FPS
figures below count UI/render updates, not distinct animation samples. Playback
floored time to integer source frames (30 fps in this scene), and repeated egui
layout passes could tick more than once. Therefore the earlier Wireframe120fps
result does NOT establish60 distinct interpolated poses/s. Fractional playback,
once-per-input-frame ticking and submission pose-time telemetry are being fixed;
both modes must be remeasured before declaring the performance goal met.

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

Persistent PBR geometry integration (75a8413, UI hook 750c399): the actual
Ray frame39 Realistic screenshot matches all360000 viewport pixels exactly.
Bevy update dropped to~2.3ms, but actual Play regressed to~14fps because
posed storage transport increased sync to~50ms. Ray release probe isolated
13parts/333969 expanded vertices: pack17.377ms, full buffer update22.175ms.
Typed bulk packing is being tested; this intermediate is not a completed
performance fix. Wireframe repeat-switch samples remain~120fps.

Typed packing e5ff492 + bulk exact hash154eb6e + parallel edge sort6dbc0cf:
actual Play four-mode repeat test yielded20.44/20.88fps Realistic and
119.76/119.85fps Wireframe (wall-clock rates), full geometry unchanged.
Actual frame39 screenshot still matches all360000 compared pixels.
Long-running process1626762 was inspected at20:51 after~10h uptime:
Bevyupdate had grown to~260ms while sync remained~32ms. CPU perf sampling
shows~74% in MainEntity→Tick EntityHash-map insertion, plus~6% removal,
under Bevy material specialization. Cause and long-run fix under investigation;
the earlier20fps result is not a claim of stable long-session performance.

99876d0 fixes the long-session lifecycle root cause: upstream light
specialization unconditionally re-adds removed NotShadowCaster entities to
every material type's request list after material cleanup removed them.
PostUpdate now rejects requests without the matching live Mesh3d/material
before extraction. Accelerated shadow-disabled rebuild test failed atframe1
with2entries before fix;128frames remain1entry/capacity3 after fix.
Live shadow-caster changes still update their specialization tick.
No periodic cache purge or shadow disabling is used.

Installed eb65d1e checkpoint (old serial Skin evaluator, guarded tangent cache
and lifecycle fix): actual repeat-switch wall rates23.082/22.864fps Realistic,
119.796/119.832fps Wireframe. Representative material work11.6ms, sync25.1ms,
renderer update2.27ms, UI world rebuild10.5ms. Target60fps is not met.

Next pending installed checkpoint: Core Skin batch4a92ef9 (48 Core tests
including159actualfull StackValue/metadata poses passed); UI166c8f4 (398
passed,1ignored), exact subgroup allocation reuse97248ba (~.55ms/fullpose
isolated), storage-cap/empty fixa9f4c05 (6targetedGPU tests). Independent
review found the storage-cap issue and confirmed its fix; no important
verified issue in Core/UI batch. Other-workspace verification running before
release build. These are local commits only, no push.

Fractional playback69728b5 +SetKeybuffer guard049e5a6: source30fps can
produce60 distinct evaluated transforms/60ticks. Stop/scrub/mode boundaries
snap tointeger editing time; readonly queries preserve visible fractional
pose; AutoKey writes integer source keys. Nonfinite/overflow advances reject
before mutation. Commands851tests passed before final narrow SetKey fix;
final21key regressions passed. UI914930c gates tick once perinputtime and
exposes ready-geometry submitted pose-time counts, distinct from UI updates.
Repeated-layout regression reproduceddoubleadvance thenpassed; held-pose
counter test passed. FullcurrentUI400passed,1ignored. Opt-in317half-frame
material audit pending before next installed true-pose FPS measurement.


Installed 51799c6 production checkpoint (2026-10-27): exact guarded weld
reuse, projected derivative reuse and edge staging now in native app.
400 UI tests passed, 1 ignored; release build passed. Frame39 viewport
750x480 comparison against prior fractional/Skinbatch build: 0 changed pixels.
Four alternating 12-second wall-clock samples with 4-second warmup each:
Realistic 24.908 / 24.884 distinct submitted pose times/sec;
Wireframe 76.083 / 75.502. Source30fps, fractional interpolation, same four-view
scene/camera/materials/full geometry. Raw evidence:
/tmp/loom-edge-weld-true-fps.jsonl. Previous same protocol:
Realistic23.320/22.885, Wireframe68.465/67.580.
Wireframe exceeds60; Realistic still fails goal. Native Realistic profile
representative sync22.290ms, UI rebuild7.525ms, Bevy update2.399ms.
Do not claim overall completion or begin controls ahead of Realistic target.
App left paused after benchmark to free resources for GPU correctness work.

GPU tangent isolation checkpoint: diagnostic actual3poses/all13parts completed
(/tmp/loom-gpu-recipe-isolation.log), not the strict acceptance test.
Face001 time39.25 target prepared29180 CPU subgroup mask127 vs GPU125,
also125 with exact CPU current derivatives injected only in test. Thus the
large .00395884 vector difference remains in subgroup/evaluator math, not
solely in GPU derivative generation. Independent explicit-f32 projection/dot
calculation on /tmp/loom-gpu-hybrid-worst.json: excluded member28821 has
CPU dotT=-0.9999998807907104, only1.1920929e-7 above strict -1 threshold.
GPU exact intermediate dot still needs instrumentation; do not assert its
value or claim the numerical gate passed. Body subgroup masks match while
~2.7e-5 residual remains. Strict2e-5 gate is unchanged; no production GPU
route enabled. Test fixture export f34022a passed its targeted test.


GPU transport staging (not enabled in native renderer): DynamicGeometryBuffer
commit_gpu validates immutable indices/count, finite current source positions,
64-byte aligned/range-checked COPY_SRC candidate, before history mutation.
It copies validated packed records into the existing persistent buffer and
retains exact CPU-position bounds/history; no CPU normal/tangent repack.
Producer must validate ALL part guards/layouts before the first commit and
submit the encoder before Bevy update; never discard after successful commit.
RED missing-method compile observed; exact GPU buffer readback/history test
passed, including bad usage/range/overflow/count/index/nonfinite rejection.
Forward/deferred PBR/prepass/shadow/allocator-offset render parity passed.
All8 dynamic-geometry tests passed (1 opt-in ignored); final expanded rejection
regression passed. Independent review found no important issue under the
explicit producer submission contract. Full compute quality/native FPS remain
unverified; this API alone is not production completion.
