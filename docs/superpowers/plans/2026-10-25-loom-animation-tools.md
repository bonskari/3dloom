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

The initial f8 desktop gate is historical evidence. Current Task9 default promotion and production-unset verification still gate implementation of the remaining animation tasks below. Mac native verification remains pending.

- The generic Set Keys pose buffer (`anim_cmds.rs:553–600`) records node tracks. It does not capture LoomRig `blend` or `pose_offsets`; those require an atomic rig-pose-key operation alongside control transforms. Preserve source bone controllers rather than baking corrections into them.
- Existing limb blend-key commands, undo, key movement and the separate Animation Editor remain the base. Current clip movement moves member-node keys; rig metadata tracks are independent and must not accidentally move with a source clip.
- Pending contact data and behavior follow docs/research/biped-contact-semantics/DESIGN.md. An empty track preserves existing behavior; authored Free keys default to FK/body space. This supersedes the earlier unimplemented restore-old-blend proposal. Preset properties remain editable.
- Joined planted references and independently moving sliding IK references need shared draw/edit/evaluation semantics. No automatic plane projection is implied. Test capture continuity and orientation before promising contact behavior; an end pivot is not automatically a foot sole or terrain contact.
- A typed contact lookup must notify `anim::note_animated_read()` when time-dependent, or the IK cache may reuse a static-scene result at another time.
- Required tests include planted position/orientation under root motion, moving sliding targets and reference-space changes, exact Free/empty-track compatibility, fractional/backward/random-access time evaluation without a scene-version edit, undo/reload, atomic key collision rejection, clip movement, and explicit nonuniform-scale/shear orientation behavior.
- Limb-shaped controls still need matching draw/pick geometry mapped onto the existing control IDs. The repaired generic bone-axis overlay is skeleton visibility, not completion of Biped-style LoomRig controls.

## Added user requirements — macOS and touchpads

User explicitly requested macOS support and confirmed touchpad navigation for both macOS and Linux. Keep these within the active work scope after the performance/stability gate. Audit native build/package/runtime paths and Metal capability fallback without reducing geometry or weights. Add usable two-finger pan, pinch zoom and modifier-assisted orbit while preserving mouse controls and ensuring navigation affects only the hovered viewport. Verify event routing through the real input integration, not just a mathematical helper. Actual macOS build/runtime evidence is required before claiming Mac support complete; Linux-only tests cannot certify it.

## Task 9: limb-shaped FK display and direct selection

**Files:** Create `crates/loom_ui/src/loomrig_overlay.rs`; modify `lib.rs`, `helper_overlay.rs`, `viewport_widget.rs`.
**Interfaces:** `ControlProxy { control:NodeId, vertices:[Vec3;8] }`; `ControlProxy::segments(&self)` iterates fixed shared box-edge indices; `prepare(scene:&Scene, out:&mut Vec<ControlProxy>)` fills pose-dependent world vertices for each existing upper/lower FK binding. `LoomApp.rig_controls` retains the buffer and rebuilds with world meshes, never in draw/pick calls.

- [x] RED: tests cover two FK spans per mapped limb, current solved endpoint alignment under authored offsets/nonuniform and mirrored transforms, zero-length finite handling, no serialized-node/rig changes, retained topology/storage across poses, no new animated reads in draw/pick, direct through-skin limb-outline clicks selecting the existing control ID, hidden/frozen/category and material-drop behavior.
- [x] GREEN: use a shared canonical tapered eight-vertex/twelve-edge limb cage and a pose frame derived from actual mapped endpoints plus current control orientation. Do not change source control transforms to align visuals. Keep all existing Point mesh/helper geometry; add meaningful limb outlines rather than removing geometry to increase FPS. World-vertex work occurs once per pose, not once per viewport.
- [x] Integrate on-top drawing in both shading modes and corresponding control-hit priority before generic bone axes in object selection. Keep `node_at` on surface picking. Existing control IDs route to current gizmo/FK commands; goals/poles/master and IK/FK authoring remain available in separate Animation Editor.
- [x] Verify `cargo test -p loom_ui --lib loomrig_overlay -j4 -- --test-threads=1` and focused viewport/helper tests; full UI suite; actual Ray source/skin records and native direct click/edit/Undo; default native throughput remains≥60 in both views. No claim of full Biped/contacts completion from this first visual/selection increment.
- [x] Local main commit with evidence; no push. Continue remaining end/body/goal interactions and contact/pose/timeline requirements afterward.

## Next contact phase: source-backed interaction contract

Before implementing contacts, follow docs/research/biped-contact-semantics/DESIGN.md. It supersedes the earlier pending Free=restore-blend and Slide=automatic-plane assumptions: empty tracks remain compatible; Free defaults to FK/body space, and Planted/Sliding presets distinguish joined versus new IK references. Blend/settings remain editable. Contact behavior is not implemented yet; performance delivery still gates this phase.

## Task9 delivered checkpoint — 4421018

Desktop production-unset64.8022Realistic/92.7629Wireframe ready distinctpose times/s, minimum event windows62.0464/88.6476; bothclient/eventclockgatespassed. Native4viewpixels and FKpointer/rotation/Undo preservedoriginalnodes/selection/frame/sourceSHA. Runtime branch15m/real-backup validation complete. Evidence docs/research/parallel-pose-preparation/production-defaults/VERIFIED.md. First8FKcages completed; remaining fullcontrols/contacts/pose/timeline/Kimodo/Mac requirements stillopen.

## Task 10: typed contact-key data and validated sampling

**Prerequisite:** Task9 production-unset minimum/native delivery (4421018) verified. This task is data/sampling only, not completion of contact authoring or Biped tools.
**Files:** create `crates/loom_scene/src/loomrig_contact.rs`; expose from `lib.rs`; add default-empty omitted-on-save contact data to `RigLimb` in `loomrig.rs`; update construction in `loom_commands/src/loomrig.rs` only as required by the new field; tests beside the new module plus existing rig compatibility tests.

- [x] Define typed body/world reference poses, stable shared-reference identities and ordered per-limb contact keys with editable IK blend. Follow `docs/research/biped-contact-semantics/DESIGN.md`; no implicit floor projection. Joined references must survive key movement/deletion without dangling links or silent overwrites.
- [x] RED: old empty/default rigs serialize compatibly; new data roundtrips; reject nonfinite/duplicate times, invalid rotations/blends and dangling references; before-first/held/fractional/backward/random-access key lookup; shared-reference edits and atomic key-collision rejection.
- [x] GREEN: smallest validated data operations needed by those tests. A nonempty time lookup must register an animated read even before its first key; an empty track must not create time dependence. Do not yet change skeleton evaluation or claim visible planted/sliding behavior.
- [x] Verify core Scene tests plus rig-command serialization/undo/mapping preservation tests. Keep original Ray source untouched; local main commit, no push.

Following tasks still required: contact evaluator and no-pop/affine/pole tests; transform/pose-key command routing; direct body/end/goal/pole controls; separate Animation Editor contact rows and clip/key editing; real Ray acceptance, final Kimodo workflow and native Mac verification. Preserve the full objective.

## Task 11: contact target evaluation and skeleton integration

**Files:** contact model/sampler and Scene IK evaluator; focused real-command fixtures.

- [ ] RED: world contacts hold through root/master motion; body contacts follow the characterized root; free blend0 releases to unchanged FK; before-first/empty tracks preserve prior evaluation; fractional/reverse/random-access time changes invalidate static IK caches. Test moving sliding references, editable blends, joined references and source-controller preservation.
- [ ] GREEN: resolve reference poses in world or characterized-root coordinates, interpolate independently authored targets/blends between keys, retain joined target identity, feed the same resolved goal/pole data into solving and later visualization/editing. Contact reference poses need captured pole/bend information; extend the unreleased metadata with validated pole coordinates rather than deriving a new bend at each playback time.
- [ ] Verify no-pop root/middle/end and orientation, mirrored/nonuniform/sheared and singular frames explicitly; do not claim affine support from naive quaternion decomposition. Preserve legacy empty-track behavior. Complete target editing and authoring commands in subsequent tasks before claiming usable planted/sliding/free tools.
- [ ] Core Scene and command regressions, local main commit, no push; no native replacement until an actual useful authoring increment is ready and verified.

## Task 12: atomic contact authoring commands

- [ ] Capture the current solved end/pole pose at the requested finite time; presets create World/full-IK Planted and Sliding references or Body/FK Free references. Reuse a compatible prior world reference for continuous planting; never silently snap a newly captured stance to a different old target. Expose explicit editable key/reference data as well as preset creation.
- [ ] Add typed command routing for creation, key/reference edits, collision-safe move and deletion; one exact Undo/Redo entry, invalid edits leave all scene/rig/source records untouched. Capture must restore scene evaluation time, source node tracks and existing contact IDs. Persist/save/reopen existing authored references.
- [ ] Test real bent/twisted chains, root/master and affine frames; confirm solved-pose capture continuity for full-IK activation, and track remaining Free-to-FK pose matching until normal rig-pose keys are implemented.
- [ ] Do not enable native contact authoring until direct target display/editing and contact-aware scene-freeze/delete-animation behaviors are implemented and tested. Task11 and full goal remain incomplete until these integration gates pass.

## Next integration gate: static contacts and effective controller edits

- New Scene Keep must preserve the current solved pose while removing contact time dependence. Implement and test an explicit constant contact state, consistent with constant native controllers; do not fake a static track with sentinel key times or silently discard a live constraint. Keep empty legacy serialization compatible. Cover contacts before/inside/after keys, body/world/mixed references and affine parents; preserve each source scene on disk.
- Generic animation deletion and normal rig-pose capture must account for rig-owned channels. Verify selected controller/limb ownership and existing key filters, not just node transforms.
- Before enabling native authoring, goal/pole rendering, hit testing, inspector values, SetTransform and actual gizmo edits must share the effective contact target. An interpolated target must follow the pointer exactly; do not merely move a raw helper while the constrained foot remains elsewhere. Combine multi-controller edits into one correct Undo/Redo transaction without overwriting sibling rig edits.
- Add the controls/contact rows to the separate Animation Editor; full native Ray pose/skin/appearance, contact-edit, clip/Kimodo and performance acceptance remains required.

## Task 13: constant contact controllers and scene-freeze preservation

- [x] RED: a sampled contact freezes to a validated constant state with no key times or animated reads; before-first/held/interpolated body/world targets, legacy serialization, invalid states and shared reference deletion are covered. New Scene Keep at these times preserves the full solved pose and removes contact key animation.
- [x] GREEN: explicit optional constant reference/blend state in ContactTrack; no sentinel times, synthetic keys or discarded live constraint. Sample it before the first animated key; validate saved and edited references. Freeze the current effective target and blend before changing source controllers; retain an unambiguous Body/World reference space, and capture a mixed-space interval in world coordinates.
- [x] Cover undoable selected-controller animation deletion, unaffected limbs, body/world/mixed and affine frames, save/reopen and source preservation. Do not claim affine flattening to TRS is exact.
- [x] Full Scene/Commands/UI regressions; actual Ray constant-freeze test; local main commit, no push. Continue effective viewport/controller edits and separate editor contacts, Free pose matching, normal rig pose keys and native acceptance afterward.

## Task 14: shared effective contact controllers

- [ ] RED: active goal/pole world poses agree with contact samples, including constants and interpolated Body/World references; inactive/before-first controllers retain raw fallback. Direct SetTransform and actual gizmo-driven Move/Rotate edits affect the intended contact, not only a raw marker. Source controller tracks remain intact; joined references, collision-free Undo/Redo and multi-controller edits are preserved.
- [ ] GREEN: one effective controller read path for world geometry, selection/gizmo and transform values; edit the active shared references with exact sampled position/orientation deltas, using correct world↔Body conversion. Between independent references, edit both positively weighted references so the current handle follows the pointer exactly. Do not rewrite the legacy source helper's position/rotation as a hidden workaround.

### Free-to-FK matching increment (not Task14 completion)

- Reproduce Free activation changing the visible three-joint pose. Capture per-joint affine FK corrections at the requested time against the same rig with the selected limb's contacts/matches disabled, retaining imported controllers and artist FK offsets.
- Store validated rig-owned pose correction keys, with polar-rotation interpolation and full stretch/translation retained; skip the subsequent IK/TRS reconstruction at zero blend. This is the affine matching foundation, not completed ordinary rig-pose-key authoring.
- Keep matching keys with moved/deleted contact keys, in one metadata Undo transaction. Preserve matching during save/reopen, characterization metadata reuse and selected-animation freezing without residual time dependence.
- Verify plain/mirrored/nonuniform hierarchies, validation, rotation interpolation, actual Ray four-limb Free activation and source fingerprints. Separate editor authoring, Free FK interaction, effective IK settings, normal pose keys and native performance remain required.

### Effective contact IK settings increment

- SetLimb must change the evaluated contact weight and keep shared anchors/source tracks. Without Auto Key, edit the exact key or both positive interval contributors; with Auto Key, key the sampled target/weight at the current frame and retain neighboring keys. Preserve constant fallback and seed its frame0 baseline when needed.
- Zero release must preserve the solved affine pose in one Undo transaction, retaining constant data without synthetic times. The separate editor slider must show and edit the contact's effective value.
- Test solver switching on twisted limbs: preserve solved joint positions and transport axial roll for the two-bone contact HI path, leaving legacy solving unchanged. Cover real Ray settings release and full source fingerprints.
- The existing legacy Key IK/FK command/timeline still needs contact/ordinary-pose routing. This increment does not satisfy the full pose-key, contact editor or native acceptance gate.

## Task 15: separate contact editor and timeline

- Keep contact presets, key/reference properties and timeline markers inside the separately opened Animation Editor. Preserve the main UI and existing source/base IK tracks.
- Add explicit Set Contact Preset (create or replace this key) while retaining Create Contact Key's collision protection. Key Contact captures the effective target and weight at the requested time; it is not a full-rig pose-key command.
- Provide Planted/Sliding/Free authoring, key weights/shared-anchor selection, reference position/rotation/pole coordinates and Body/World conversion that preserves the current world pose. The anchor inspector defaults to the active reference at the current frame.
- Draw actual contact markers beside the base IK/FK track, with drag/release move, collision rejection, explicit contact deletion and Undo including associated FK corrections. Test real pointer/menu events and exclusion from the closed editor/main UI.
- Verify command atomicity, source preservation, requested-time restoration, affine reference conversion, actual Ray preset replacement/key capture and UI regressions. Full-rig pose keys, buffered-pose integration, Free FK/gizmo and Auto Key target editing, native rendering/performance and the full goal remain required.

## Task 16: full rig pose keys and buffered-pose safety

- First preserve the complete Set Key rollback path beyond the configured Undo limit, including limit changes, selection steps, scene replacement with equal history positions, accepted key Undo/Redo and interactive transactions. Same-frame seeking/fractional playback must not accept an unkeyed preview.
- Distinguish transient pose entries from accepted history; preserve this classification through Undo/Redo and gesture grouping. New baselines accept prior history. Finish gestures at pose boundaries and never discard accepted structural/key work as a preview.
- Then add full-rig pose capture for owned control transforms and rig-owned blend/offset/matching/contact channels, with atomic buffered-pose integration and source animation retained. Provide separately opened editor authoring and grouped timeline operations, validation, locks/collisions and save/reopen tests.
- Full-rig capture command and separately opened editor button now have focused command and egui coverage (see `docs/research/full-rig-pose-keys.md`). Grouped timeline move/delete authoring now has command and egui coverage (see `docs/research/grouped-rig-pose-timeline.md`); contextual ordinary-key routing, complete rig-channel time rescaling and final native/visual/performance acceptance remain incomplete. Source checkpoints must not be presented as completion of rig pose keys or the full goal.
- [ ] Preserve target/reference identity, independent goal/pole fields, visual helper scale, locks and legacy empty tracks; test affine skeleton frames and parented controller conversions explicitly. Record remaining Free/FK pose-matching and auto-key/pose-key routing until implemented; do not claim them complete from IK target edits.
- [ ] Add separate Animation Editor contact authoring/properties, native Ray pointer/Undo and visual evidence, actual default ≥60 ready distinct poses/s in both modes, save/reopen and source/material/desk/chair preservation before native delivery. Full goal remains active afterward for remaining rig/pose/timeline/Kimodo/Mac requirements.

Native checkpointeb7b2f0: actual viewport FK/gizmo/Undo, pose capture/move/delete, three contact presets and buffered FK keying passed with complete source-scene restoration. Current-code15m real-backup reliability passed; see `docs/research/rig-pose-native-eb7b2f0/README.md`. Quiet matched candidate65.92R/94.47W, minimum63.00R/88.95W, but initial59.24Rwindow remains recorded/unexplained;120not achieved. These checks do not complete remaining Free FK/contact target Auto Key, Biped end/body controls, ordinary-key routing, global rig time rescale, final Kimodo and physical Mac gates. Continue those real authoring features, not repeated already-passing prerequisites.
