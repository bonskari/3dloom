# Contact-key implementation contract — not implemented yet

Implementation starts after Task9's production-unset performance delivery. This design refines the pending proposal; it does not certify contact functionality.

## Source-backed interaction

Autodesk's [Using IK Keyframe Parameters](https://help.autodesk.com/cloudhelp/2020/ENU/3DSMax-Character-Animation/files/GUID-F85AA7B6-5462-47AF-80B0-1C4F2D5A47BB.htm) describes three key properties: IK blend, Body/Object reference space and joining the previous IK reference. The [Key Info rollout](https://help.autodesk.com/cloudhelp/2024/ENU/3DSMax-Character-Animation/files/GUID-24693B38-20BD-435D-9816-BE5BDA528506.htm) exposes presets and editable key parameters. Plant defaults to full IK with a joined object/world reference; Slide uses full IK with a new reference; Free defaults to FK/body space. A sliding target can move; automatic projection onto a floor plane is not part of these preset definitions.

Our implementation must keep an empty contact track exactly compatible with the current rig. Once contact keys are authored, Free must genuinely release to FK by default. This supersedes the earlier unimplemented Free=restore-old-blend proposal. A plane projection would be an explicit optional future constraint, not the definition of Slide. These are implementation decisions based on the user's Biped workflow request, not additional user-approved features.

## Required behavior and tests

1. Persist typed per-limb keys and stable contact references, with empty/default serialization omitted. Validate finite times/poses, unique key times, normalized usable rotations and valid references. No synthetic source data or saved-document mutation during loading.
2. Preserve a planted reference across joined keys and across root/master motion. A new contact after a Free span captures a new stance. Editing a shared reference must affect its joined keys coherently. Deleting/moving a key must not leave dangling references or silently overwrite another key.
3. Sliding creates an independently editable moving IK target; Free defaults to blend0/body space. Blend, solver and key settings must remain editable; presets must not make the existing settings silently ineffective. Preserve all imported bone controllers and Skin data.
4. Capture the currently solved pose when creating a key. Verify no activation pop at root/middle/end, including pole/bend orientation and end orientation. Existing solver-switch matching is relevant code, not proof of contact continuity.
5. Goal/pole visualization, viewport dragging, inspector/MCP transforms and pose-key capture must resolve the same effective target. A locked foot whose visible controller moves without changing the intended editable contact is not acceptable. All changes need atomic Undo/Redo.
6. Define and test affine behavior under mirrored/nonuniform parents and shear; do not promise exact orientation by naively decomposing a skewed world matrix. Include singular-input validation. Test both source-preserving empty tracks and active contacts at fractional/backward/random-access times.
7. Time-dependent contact lookup must notify anim::note_animated_read even before its first key; a static-scene IK cache must not remain valid when a future contact key becomes active. No per-draw evaluation or topology generation.
8. Add atomic rig-pose keys covering control transforms, IK/FK values and pose_offsets, plus contact state/targets when applicable. Generic Set Keys currently covers node tracks, not all rig metadata. Source clips and their bone animation remain intact.
9. Use the separate Animation Editor for contact rows, pose keys, moving clips and editable settings. Preserve main UI layout. Add direct usable end/master/pelvis/body controls, not only list selection. Verify imported Ray materials plus exact desk/chair placement.
10. Verify actual Ray interaction, contact edits, source-motion playback, save/reopen, native rendering and performance after implementation. No claim of full Biped equivalence or completion from a data-model-only increment.

## Existing-state checks

Current RigLimb has blend and pose_offsets, no contact track. Current generated goal/pole helpers are world-space (no parent); FK display controls parent their mapped bones. SetLoomRigMapping already carries prior limb metadata by role name and reuses controls. A new real-command regression test confirmed solver, keyed blend, pose_offsets, goal/pole IDs and original bone records survive an identical mapping apply and an unrelated head-role change. It passed before any production change; no mapping-loss bug was reproduced or fixed.

Suggested next tasks: typed key/reference sampling and validation; evaluator with no-pop matching/cache/affine tests; atomic commands and pose-key routing; shared viewport goal/pole editing; separate timeline/editor controls; real Ray acceptance. Treat each as incomplete until its actual behavior is verified.
