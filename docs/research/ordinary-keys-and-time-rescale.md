# Contextual ordinary keys and full rig-channel time rescaling

## Summary

Ordinary key commands (`SetKeys`, `KeySelection`, `selection_key_times`, `GoToNextKey`/`GoToPreviousKey`, `MoveKeysInTime`, `CopyKeys`, `DeleteKeys`) and global timeline rescaling (`RescaleTime`) now account for `LoomRig` contact, pose-matching, blend, pose-offset, and `pose_frames` channels alongside node tracks.

## Contextual ordinary key routing

- Active contact targets (`limb.goal` position/rotation and `limb.pole` position) skip raw node transform track creation in both `SetKeys` and `KeySelection`, routing target key creation through `key_contact_target` so the prior frame anchor and shared reference identities are preserved while visual helper scale tracks remain editable.
- In `SetKeyMode`, `SetKeys` builds edits against `state.undo.pose_baseline` before rolling back the preview buffer (`discard_pose`). If validation fails (for example, a non-keyable/Noise blend controller, locked track, or `XRef Scene` guard), the live Set Key preview buffer remains intact.
- When `key_filters.ik_parameters` is enabled and a limb is explicitly selected or posed (without an active contact), `SetKeys` also keys `limb.blend` and the three `limb.pose_offsets` channels. When all owned controls of a rig are selected, `SetKeys` records the frame in `rig.pose_frames`.
- Full-rig pose capture (`KeyLoomRigPose`) snapshots moved contact targets in `SetKeyMode` via `key_contact_target` against the baseline contact track so prior contact anchors are never overwritten.
- `selection_key_times`, `GoToNextKey`/`GoToPreviousKey`, `MoveKeysInTime`, `CopyKeys`, and `DeleteKeys` include selected rig contact keys, `pose_matches`, `blend`, `pose_offsets`, and `pose_frames`.

## Full rig-channel time rescaling (`RescaleTime`)

- `RigPoseTrack::rescaled` ([crates/loom_scene/src/loomrig_pose.rs](file:///home/b1s/dev/github/3dloom/crates/loom_scene/src/loomrig_pose.rs)) and `ContactTrack::rescaled` ([crates/loom_scene/src/loomrig_contact.rs](file:///home/b1s/dev/github/3dloom/crates/loom_scene/src/loomrig_contact.rs)) validate finite, strictly ordered rescaled key times separated by at least `SAME_TIME` (`1e-4`), preserving constant states and contact reference tables.
- `RescaleTime` ([crates/loom_commands/src/anim_cmds.rs](file:///home/b1s/dev/github/3dloom/crates/loom_commands/src/anim_cmds.rs)) rescales recursive `Controller::List` tracks, `object_offset` tracks, `visibility`, all `LoomRig` channels (`pose_frames`, `blend`, `pose_offsets`, `pose_matches`, `contacts`), and `animation_clips`.
- Validation rejects invalid ranges (`end <= start`), nonfinite or colliding rescaled keys, locked rig control tracks, and `XRef Scene` guards atomically before mutating `scene`, `frame_range`, `current_frame`, or `undo`.
- `timing_undo` is recorded around the `Re-scale Time` commit so `UndoSceneOperation` and `RedoSceneOperation` restore `frame_range`, `current_frame`, `loom_rigs`, and `animation_clips` in one step.
