# Separate contact editor

The separately opened Animation Editor now offers Planted, Sliding and Free presets for the selected limb. `SetLoomRigContactPreset` explicitly creates or replaces the current key with a captured solved pose, in one metadata Undo transaction. The original `CreateLoomRigContactKey` retains its collision protection. Existing anchors used by other keys and source controllers are preserved.

The active-contact button is `Key Contact`, routed to the new `KeyLoomRigContact` command. It keys the effective target and weight at the requested time, retaining neighboring keys and restoring evaluation time. The legacy `Key IK / FK` button remains for the base channel only when no contact is active. This is not a full-rig pose-key command.

The editor exposes individual contact key weights and shared-anchor selection, plus anchor position, quaternion-backed Euler rotation, pole position and Body/World space. Space conversion preserves the selected anchor's current world pose. The inspector follows the active anchor when the frame or active reference changes, while retaining explicit alternate-anchor selection within that context. A regression reproduced the initial incorrect default-to-first-anchor behavior before the correction.

Contact markers appear on distinct timeline rows beside the existing base IK/FK tracks. Pointer drag moves the contact and its associated FK matching key on release; collisions are rejected. The context menu explicitly deletes a Contact Key, retaining the base curve. Undo restores the full rig metadata.

Evidence before the final broad run:

- Initial real egui event tests failed for missing preset buttons and contact markers (`/tmp/loom-contact-editor-red.log`). Five focused contact UI tests then passed (`/tmp/loom-contact-editor-final-focused.log`), including closed-editor exclusion from the main UI, preset replacement/Undo, key creation, marker move/delete/collision and affine reference-space conversion. The final broad run adds explicit pole and joint-matrix assertions to the conversion test.
- The active-anchor regression failed before its selection fix (`/tmp/loom-contact-anchor-selection-red.log`).
- Direct command test passed (`/tmp/loom-contact-editor-commands.log`): bent-pose Free→Planted replacement, original Create collision protection, invalid-preset atomicity, effective key capture, source retention, requested-time restoration and inactive-contact rejection.
- Production Ray opt-in passed all four limbs (1 test,70.00s, `/tmp/loom-contact-editor-ray.log`), including current-key preset replacement and requested-time key creation, full joint world matrices, retained source-node/material fingerprints and prior capture/matching/freeze checks.
- Read-only review found no concrete issue in this source increment. It did not certify full-rig pose keys, complete buffered-pose integration, Free FK gizmos, target Auto Key editing or native performance. Manual key-weight editing intentionally edits the animation curve; the separate Free preset performs pose matching.

No installed application replacement or public push is part of this checkpoint. Task15 native/visual acceptance, ordinary rig pose keys, the remaining rig interactions and the full goal remain incomplete.

Final broad run exited0:1572 passed, 0 failed,49 ignored across147 binaries (`/tmp/loom-contact-editor-full.log`, Scene/Commands/UI/Bevy lib+integration tests). UI accounts for452 passed/2 ignored in128.95s. This includes the final pole and joint-matrix conversion assertions. Original Ray file SHA-256 is unchanged: `84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935`. Source call-site audit confirms the contact panel and timeline are reachable only from the guarded, separately opened Animation Editor.
