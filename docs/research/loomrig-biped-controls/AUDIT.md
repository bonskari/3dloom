# LoomRig limb-shaped control source audit

Read-only source audit, 2026-10-28. No controller implementation, build, scene evaluation or native input was performed. This is the next animation phase after the performance/stability gate, not completed Biped functionality. Scope follows [the existing animation plan](../../superpowers/plans/2026-10-25-loom-animation-tools.md), particularly its remaining limb-shaped draw/pick requirement.

## Actual creation and transform contracts

- [`loom_commands/src/loomrig.rs::build`](../../../crates/loom_commands/src/loomrig.rs) creates master, pelvis/spine/head/FK, goals and poles through the same `helper` closure using `ObjectKind::point(4.)` (lines25–33). FK helpers are parented to the mapped bone with identity raw offset; `ControlBinding` retains bone/control IDs and `rest` (lines66–71). Goals/poles keep separate IDs and world-space rest positions (95–106). Goal rotation is initialized from the end bone (109–117).
- [`loom_scene/src/loomrig.rs`](../../../crates/loom_scene/src/loomrig.rs) owns `ControlBinding { bone, control, rest }`, `RigLimb` goal/pole/rest/solver/blend/pose_offsets and LoomRig mapping. It currently contains no persistent visual-shape records.
- [`Scene::local_matrix`](../../../crates/loom_scene/src/scene.rs) explicitly returns identity for non-master FK controls (around695): they display at the corrected bone pivot. Their stored transform remains the animatable input offset.
- [`Scene::ensure_ik`](../../../crates/loom_scene/src/scene.rs), around907–917, composes that stored offset with the original bone transform and binding rest. Master composition is separate. Reorienting a control's transform merely to align a shape would therefore change the rig, rather than just its appearance.
- [`Scene::edit_parent_frame`](../../../crates/loom_scene/src/scene.rs), around1075, removes the stored offset from the solved display frame before converting world edits to stored coordinates. New shape picking must still select the original control ID and use this existing transform/edit route.
- [`SetLoomRigMapping`](../../../crates/loom_commands/src/loomrig.rs) reuses matching existing controls/bindings/limbs and rejects edits that discard authored controls. Improving only new-rig creation would leave the saved Ray's existing Point controls unchanged; existing-rig visual upgrade must be explicit and undoable while retaining their IDs and tracks.

Do not change source skeleton controllers, skin binds/weights, control IDs/parents/raw transform controllers, binding rest, IK chain mapping, goal/pole rests, solver, blend or pose offsets to implement visual controls. Alignment/length belongs in local shape geometry, not in an extra control transform.

## Draw and pick paths

- [`helper_overlay::lines`](../../../crates/loom_ui/src/helper_overlay.rs), around78–105, currently draws Point/helper segments via [`helpers::helper_lines`](../../../crates/loom_commands/src/helpers.rs), around185. It uses the control node's current object/world transform. Actual helper geometry is not inherently empty: [`ObjectKind::cage_mesh`](../../../crates/loom_scene/src/node.rs), around1679, gives Point a small box cage.
- [`object_selection_hits`](../../../crates/loom_ui/src/viewport_widget.rs), around1990, collects selectable generic bone-axis hits **before** ordinary shape/surface hits. Visible limb-shaped controls therefore need a deliberate object-selection priority over those skeleton axes when their displayed shape is hit; generic axes are not equivalent controls.
- [`object_hits` / `pick_shape`](../../../crates/loom_ui/src/viewport_widget.rs), around2023/2450, already allow directly picking spline outlines. However, shape hits currently enter the general surface/material-drop route too, and `pick_shape` itself has no explicit `is_selectable` check. Existing outer routing/visibility must be audited before reusing it for controls; hidden/frozen/category behavior cannot be assumed from the outline helper alone.
- [`node_at`](../../../crates/loom_ui/src/viewport_widget.rs), around2010, deliberately uses ordinary object hits for material drops. A control-pick priority belongs in object selection, preserving that separate surface contract.

## Smallest bounded implementation seam

Use matching display and hit geometry attached to **existing LoomRig control IDs**. Keep source transform/IK evaluation intact. Persistent local limb-shape points/segments must be created on rig creation or explicit visual/mapping edits and retained across poses; each pose transforms those same points using the existing corrected control display frame. Drawing and picking consume the same records and visibility rules. No per-frame primitive construction or topology/normal/tangent rebuild is justified for fixed control geometry.

The existing `EditableSpline` route is a useful interaction reference, not an already-proven zero-generation solution: [`ObjectKind::shape`](../../../crates/loom_scene/src/node.rs), around1081, clones the Shape, and [`draw_shapes` / `pick_shape`](../../../crates/loom_ui/src/viewport_widget.rs), around2400/2450, call `polylines` and project generated paths. Simply replacing Point with parametric spline primitives does not establish geometry reuse. Conversely, retaining Points as transform/identity carriers with dedicated persistent local visuals need not rewrite FK/IK semantics. The implementation choice remains pending; this audit does not introduce a cache key, global setting or new UI design.

Shape dimensions must be derived from real mapped limb geometry in a defined local frame. Static bind-local shapes follow the corrected bone matrix; they must not be claimed to exactly track endpoints when animated child translations change limb length unless that behavior is explicitly handled and tested.

## Required focused gates

1. Creation and explicit upgrade preserve all pre-existing node IDs, raw control tracks, bone tracks, bindings, chain metadata and serialized skin data; undo/redo/save/reopen preserve both visuals and rig behavior.
2. Click the displayed upper/lower/end limb shape away from its pivot in Realistic and Wire: select the original control ID through the skin. Check skeleton-axis overlap priority, hidden/frozen/category rules, perspective/orthographic views and near-plane clipping. Material drops keep their existing surface target.
3. Actual FK rotation/translation/scale offsets still deform the mapped limb and current skin once, without doubled visual offsets. Test already-authored nonidentity offsets, master transform, inherited/nonuniform/mirrored scale and source animation at multiple times.
4. Existing IK goal/pole movement, solver/chain edits, IK/FK matching and animated blend/pose-offset tracks give identical evaluated skeleton/skin outputs before and after visual-only changes. Visuals follow the solved pose, not an independently evaluated rest pose.
5. Instrument geometry creation/retention: repeated Play poses and selection changes do not rebuild control local topology. Explicit shape/mapping edits rebuild only affected visuals. Draw and pick use the identical retained shape definition.

Existing starting tests are [`loom_ui/src/ui_tests/loomrig.rs`](../../../crates/loom_ui/src/ui_tests/loomrig.rs) and the LoomRig command/scene suites. A motion-panel control-selection test alone does not verify direct limb-shaped viewport manipulation or geometry reuse.
