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


Batched GPU guards staged in gpu_pose_commit.rs: one persistent52-byte
MAP_READ buffer for13 parts, one queue submission and map/poll synchronization.
Readback follows prior compute submissions on the same queue. Mutable receiver
prevents overlapping maps. Counts/source usages/ranges are checked first;
raw current flags are returned, and producer must reject the whole pose on
any nonzero value before the first render-buffer commit. Missing-type RED
then actual13-buffer current-change/recovery GPU regression GREEN. Independent
read-only review found no important issue. Not yet connected to native playback.


Complete-pose commit staging now owns submission: preflight ALL layouts,
read ALL current guards, reject the entire pose on any nonzero word, copy ALL
parts and submit before returning bounds. Second-part layout/guard failures
leave both target buffers unchanged; successful batch updates both. Review
found source/target aliasing could bypass preflight and mutate partial history;
regression reproduced an incorrectly accepted cross-target copy, then passed
with self/cross-target identity rejection before mutation. Direct self-copy
rejection/history GPU test also passed; independent re-review confirmed fix.
Dynamic8test suite passed before final alias guard; changed batch2tests and
final direct transport regression passed after alias fix. No native activation.


Internal renderer integration now uses the real CPU world-baked source positions,
GPU current SG normals/preparation/tangents, all-part guarded atomic commit,
current AABB/layer updates and persistent mesh/material/entity handles. Only
successfully committed objects skip CPU geometry work. CPU/GPU provenance
prevents an old CPU hash from accepting newer GPU storage on fallback.
Two-object actual GPU readback/resource test and natural geometric degeneracy
miss/recompile passed. Actual saved Ray5skins/all13parts passed at0,39.25,
95.71835,149 with stable entity/buffer handles and current positions. Warm
renderer sync5.707/5.435ms in that integration test (not native UI FPS).
Default Bevy lib63passed30ignored; UI400passed1ignored; production lib/UI
checks passed. Read-only review found no important host-integration issue.
The renderer remains opt-in via LOOM_EXPERIMENTAL_GPU_POSES=1 for internal
native comparison only. Default is CPU. Native diagnostics now report real
committed object/part counts, recipe recoveries and last error, alongside
actual distinct submitted pose counts. Strict2e-5 characterization still fails
102/105868173samples (max.0001710423), so no general quality or60fps claim.


Native internal GPU comparison installed0b3d92b, actual appPID2617654,
LOOM_EXPERIMENTAL_GPU_POSES=1, same full461-node Ray scene/fourviews/camera.
Four alternating warmup4s+sample12s wall-clock measurements:
Realistic41.432/41.034 distinct ready submitted pose times/sec;
Wireframe70.648/71.598. Prior CPU checkpoint24.908/24.884 Realistic.
Real diagnostic status5committed objects/13parts, last_error=None,
recipe recoveries0. Frame39 viewport750x480 differs in6pixels,max1RGB
from prior exactCPU checkpoint; geometry/materials/camera not reduced.
Evidence persisted docs/research/native-gpu-checkpoint/2026-10-28*.
Representative native UIworld7.8ms (Skinbatch3.7), GPUrenderer5.6,
fullBevysync7.3, Bevyupdate1.8, UIframe18.7–19.1ms. Wire UIframe8.55ms;
actualwall intervals include additional work/wait beyond these inclusive scopes.
Target60Realistic still fails. App paused and restoredframe39 after measurement.
No general quality acceptance claimed: strictnumeric102 failures remain.


Presentation hypothesis to test: native UI frame18.7–19.1ms differs from
~24ms submitted-pose intervals. Configuration uses AutoVsync; current desktop
mode is3840x2160@119.88Hz, scale1.5, VRR automatic. This does not establish
vsync as the cause. Internal LOOM_EXPERIMENTAL_NO_VSYNC=1 requests AutoNoVsync
for a controlled same-scene comparison; ordinary launches retain AutoVsync.
Added inclusive guard-readback/wait and GPU-copy profile scopes to distinguish
queue wait from CPU work. UI production check passed; measurements pending.


2026-10-28 continuation: the controlled presentation experiment showed no
measured improvement: AutoNoVsync Realistic40.626/40.415, Wire70.169/69.864;
AutoVsync Realistic40.511/40.416, Wire69.441/70.227 distinct ready poses/s.
Raw measurements are in research/native-gpu-checkpoint/*vsync*.jsonl.
Removed the experimental presentation override. This does not establish the
source of every presentation wait. The native render.upload callback is outside
the UIframe scope and accounts for roughly4.2ms of otherwise uncounted work.

Fixed dynamic geometry history at the scene render boundary: intermediate CPU
or GPU updates no longer overwrite the last rendered positions. Removed the
redundant per-pose previous-position upload; idle buffers also avoid rewriting
unchanged history. The real-GPU regression failed before the fix and passes
afterward for both CPU and GPU producers. Bevy library64passed/30ignored,
including11dynamic geometry tests covering PBR/prepass/shadows/resource reuse.
Skin weight visualization now computes face colors lazily once per rebuild,
rather than eagerly for each visible object. UI400passed/1ignored before the
whitespace-only cleanup. These changes have not yet been measured natively.

Native305855b measurement with history fix and d65988d selected-edge sharing:
Realistic43.088/42.305 and Wireframe76.086/75.974 distinct ready poses/s.
Same scene and camera, full geometry/materials, GPUstatus5objects/13parts,
last_error=None/recoveries0. Compared with8884ba8 control40.511/40.416 and
69.441/70.227, combined changes improve both modes; this measurement does not
isolate individual contributions. Realistic60 still fails. App paused/frame39
restored. Raw four-window results persisted2026-10-28-history-edge.jsonl.


Next measured boundary56e6570: native encode+submit1.232–1.239ms,
guardwait1.067–1.086ms, layoutvalidation~0.438ms/pose (26calls),
scenebounds0.194–0.196ms. This identifies command encoding as a larger target
than the additional bounds scan; no cause inferred beyond these timings.

c4fbe10 shares GPU tangent passes by stage across independent material parts
(5passes instead of5perpart). Each part retains its own pipeline/bindgroup,
all guard initialization and dependencies preserved between stages. Ordinary
Bevy65passed/30ignored. ActualRay4pose integration passed, with full output
byte equality vs prior serial math for all13parts at39.25/95.71835/149;
immutable render resources and current position readbacks also passed.
Native performance measurement is in progress; no improvement claimed yet.

Native c4fbe10 result: Realistic46.392/46.059; Wireframe76.483/78.191
actual distinct ready poses/s. Same camera/geometry/materials as prior checkpoint.
Native encode+submit~0.793–0.799ms, guardwait~0.173–0.177ms,
GPUrenderer4.332–4.378ms. All5objects/13parts committed, no errors/recoveries.
Frame39 Realistic viewport byte-identical to305855b screenshot (0pixels differ).
Raw results persisted2026-10-28-stage-batch.jsonl. Independent read-only review
found no important issues in pass isolation/stage ordering or history boundary.
App paused/frame39 restored. Realistic60 and broader numeric quality gate remain
unfinished; no completion or general default-enable claim.


bce2500 moves post-evaluation Skin display cloning/world transforms onto the
existing bounded4worker pool. Scene evaluation/cache publication and object
matrix reads remain on host. Main loop takes owned current display meshes and
skips the already-applied world transform; editing cages/styling remain separate.
The operations themselves are unchanged, including transform_mesh invariants.
UI401passed/1ignored; explicit translated-object integration passes exact serial
world-display/position-bits/cage comparison. Independent read-only review found
no important TLS/scene/endresult/displaybox issue. Native benefit unmeasured.

Native bce2500 ready-pose rates: [('Realistic', 47.261), ('Wireframe', 80.676), ('Realistic', 47.104), ('Wireframe', 80.279)].
Same scene/camera/full materials/geometry,5objects/13parts committed with no
error or recovery. Frame39 Realistic viewport byte-identical to c4fbe10.
Rebuild world7.261–7.324ms, skin display worker wall1.199–1.216ms.
Raw rates2026-10-28-skin-display.jsonl. App paused/frame39 restored.
This measured incremental gain is retained. Realistic60 remains unfinished.


7a7b1e7 native upload breakdown: over a steady Realistic report interval,
234UIframes/107874meshupdates: edges413.396ms(~1.77ms/frame), queue300.263ms
(~1.28ms/frame), bounds15.279ms(~0.065ms/frame), wire construction2.358ms.
These are inclusive reported scopes; totals are not additional frame costs.
Wire-material preparation was already skipped when no wire material exists,
but each zero-count wire stream still wrote one unused line vertex every pose.
d8a0bd4 removes only that inactive upload, retaining prior allocation and zero
draw count. Nonempty wire reactivation writes current vertices normally.
Full Render31unit+1integrationpassed, including realGPU reuse/empty/wire/empty/
wire transitions and exact active buffer/raster comparisons. Native gain pending.

Native d8a0bd4 rates [('Realistic', 46.96), ('Wireframe', 82.442), ('Realistic', 46.649), ('Wireframe', 82.14)]. Realistic has no established
improvement versus bce2500 (also adds per-object profiling); Wireframe increased
in this run. Do not attribute a Realistic gain to this cleanup. All5objects and
13parts committed without reported error/recovery. Frame39 Realistic viewport
byte-identical to bce2500. App paused/frame39 restored. Full60gate unfinished.


35e58f0/e617f00 exact GPU edge integration: source handles published only for
successfullycommitted current visible poses. Typed GeometryUpload associates
IDs/world_version; onlysamependingversion receives sourcehandles afterBevy sync.
Callback validatesactualsourcebuffer/count and exactorigin/twin/effectivevisibility,
updatescolor, records gather beforepaint, selects persistent GPUedge output.
CPU editing/picking mesh remains unchanged. Absent/failed/replaced/mismatched
sources and diagonalstyles keep current ordinaryedge rendering. No sourcevertex
or influence reduction. Default nonexperimental backend has no GPU sources.
66Bevy/34Render/402UI librarytests passed (30/0/1opt-in ignored). Added realGPU
callback coverage for pose/color/sourceID/sourcebuffer/visibility/diagonal/second
viewport/shading transitions, and failed currentreadback sourcewithdrawal/recovery.
Independent read-only review found no important sourceepoch/resourcebarrier issue.
Standalone317audit and its historicalinputhash limitation archived separately in
research/gpu-edge-gather; do not claim an exact historical scene SHA.

Native e617f00 ready-pose FPS: Realistic51.272/51.390, Wireframe84.464/84.557.
Prior d8a0bd4 Realistic46.960/46.649, Wire82.442/82.140. Same savedscene path
and camera. All5objects/13parts current, no error/recovery. Profile258poses
shows1290GPUedge dispatches (5/pose),117648CPUedge stages (456/pose),118938total
meshupdates (461/pose), so current gather is actuallyused rather than merely
compiled. CPUedge stage72.882ms/258 (~0.282ms/frame); queue124.662ms/258
(~0.483ms/frame). Timings inclusive; exactgeometry guard time is outside these.
Allfour frame39 viewports match prior d8a0bd4 screenshots pixel-for-pixel
(Top/Front/Left/Realistic:0changedpixels). Apppaused/frame39 restored.
Rawfour-windowmeasurements2026-10-28-gpu-edges.jsonl. Realistic60 and full
numeric GPUquality/stability/rigtool requirements remainunfinished; goalactive.


bb30aa1 removes duplicate layout validation through private-field
ValidatedGpuCopy tokens. Targets exclusively borrowed and source positions
immutablyborrowed until commit/drop; crossalias preflight remains first,
alltokensvalidated beforeguardreadback, allguardszero beforeanyrender/history
mutation.66Bevytests passed, independent read-onlyreview foundnoimportantissue.
Native Realistic51.068/51.003, Wire82.315/82.932: no establishedwholeframeFPS
benefit vs e617f00. Do not labelthis anFPSgain. Validationprofiles13calls/pose
ratherthan26; privatecheckedtoken contract retained. Allfour frame39viewports
remainpixelidentical to e617f00; apppaused/frame39 restored. Rawfourwindows in
2026-10-28-validated-copy.jsonl. Realistic60 remainsunfinished.
Bounded17part-pose tangent-conditioningcapture nowruns against animmutable
prehashed input snapshot, separatelyfromFPSmeasurement; no productionmathchange.


8907fec caches whether the GPUedge recipe has every effectiveedgevisible.
For that case it uses exact !Mesh.has_invisible_edges() instead of scanning
edge_visible for everyhalfedge eachpose. Origin/twin/count/sourceproofs remain;
nonallvisible recipes retain exact peredgevisibility comparisons. FullRender
34unit+1integrationpassed. Not installed/measurednatively yet; no FPSclaim.

9c4182f boundedconditioning evidence (separatehashedimmutableinput): all102
baselinefailures reproduced,51controls, no membermismatch; baseline derivatives
bitidenticalto instrumentedderivatives904memberrecords despite evaluatortrace
perturbation. IndependentPython/RustF64≤2.36e-13. BaselineGPUderivatives with
F64remainingmathstillfail30/102; finalcompensatedsumalone is insufficient.
Higherprecisionwholecurrentgroup prototype is researchonly; no mathchange shipped.

Read-only COW audit: fiveSkin meshClone heap payload15.147MB excludingallocator
metadata: halfedges8.363MB(55.2%), vertices3.294MB, faces1.336MB, UV2.154MB.
Isolatedhalfedges-only Arc+COW prototype now runs correctness/paired probes;
productionMesh schema/ownership remains unchanged pendingpositiveverification.
