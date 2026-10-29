# Set Key pose history prerequisites

Before adding the full-rig pose key, command regressions reproduced several existing history failures:

- Exceeding the normal Undo limit made Clear Set Key Buffer restore only the last few edits (`/tmp/loom-pose-buffer-limit-red.log`).
- An accepted key restored by Undo → frame change → Redo was discarded on the next frame change (`/tmp/loom-pose-buffer-redo-red.log`).
- Same-frame seeking accepted an unkeyed preview, undoing selection below the baseline left a future start position, and previously accepted edits retained their old preview classification (`/tmp/loom-pose-buffer-boundaries-red.log`).
- An open drag could absorb an accepted key; an accepted mixed transaction could later be rolled back as a preview (`/tmp/loom-pose-buffer-open-drag-red.log`).

Undo history now retains every inverse after the live pose's rollback floor. Releasing/rebasing the pose restores ordinary trimming. Shared trimming also keeps absolute positions and saved indices consistent when reducing the Undo limit or redoing after a limit change.

Entries carry a runtime-only pose-preview flag. Transient transform/parameter edits set it; explicit key and structural commits remain accepted. Transactions preserve preview status only when all their steps are previews. Undo/Redo retain the flag. Selection-only markers remain harmless preview steps. Establishing a new baseline accepts all preceding history, and history navigation behind the old baseline rebases it.

Pose boundaries finish open gestures. Discard rolls back only preview entries, stopping at accepted work. This preserves a mixed transaction's accepted structural edits and prevents a later pointer release from absorbing an already recorded key. Same-integer-frame seeking/playback keeps the existing baseline. Scene replacement rebuilds it even when the new history position equals the old one.

Focused Scene/Commands/key run passed118 tests with2 ignored (`/tmp/loom-pose-buffer-final-focused.log`). Real egui goal/pole drag coverage was expanded to Set Key mode, including preview Redo and discard (`/tmp/loom-pose-buffer-gizmo.log`); the final broad run covers the later gesture-boundary changes. The updated actual-Ray test also exercises rollback beyond a small Undo limit against full source data.

The full-rig pose-key command, editor/timeline integration, remaining rig interactions and native/performance acceptance are still incomplete. This source increment does not install an app, alter the original Ray file or publish a repository.

Final broad verification exited0:1583 passed,0 failed,49 ignored across147 binaries (`/tmp/loom-pose-buffer-full.log`, Scene/Commands/UI/Bevy lib+integration tests). UI452/2 ignored includes real goal/pole pointer drags in both ordinary and Set Key modes, with preview Redo/discard after frame changes. Updated real-Ray opt-in passed separately in71.25s (`/tmp/loom-pose-buffer-ray.log`): six goal/pole moves against a history limit2 fully restore rig metadata, source-node/material fingerprints, solved joint matrices and the saved-state flag. All prior Ray preservation checks also pass.

Read-only review found no concrete issue in this history prerequisite. FullRigPoseKey, its buffer/UI integration, future command classification and native performance were explicitly not certified. The temporary history extension retains actual rollback inverses, so memory use may exceed the configured history limit while a pose remains pending; normal trimming resumes on acceptance/discard.
