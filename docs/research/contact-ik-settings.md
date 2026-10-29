# Effective contact IK settings

Regressions reproduced `SetLoomRigLimb` changing only the legacy base weight while an active contact continued using its own unchanged weight (`/tmp/loom-contact-limb-settings-red.log`). The separate Animation Editor also displayed the inactive base value (`/tmp/loom-contact-weight-ui-red.log`).

Active contacts now route the limb setting to the actual contact weight. With Auto Key off, an exact-key edit changes that key; an edit between keys changes both positive weight contributors to the requested value. Constant fallback edits retain the fallback and do not change future keys. Shared target references and source bone/helper tracks remain intact. The legacy branch remains in use when no contact is active.

Auto Key inserts/updates the current contact key while retaining existing neighboring keys. Joined targets reuse their reference. Interpolated targets get a captured reference in their common Body/World space; mixed spaces capture the current World target. Constant fallbacks seed an earlier frame0 key when authoring the first positive-time weight key, including before an existing future key.

Releasing to zero weight captures affine FK matching in the same Undo transaction. Constant contact/matching data stay constant; animated contacts retain matching keys. Positive weight edits preserve the anchors and change the actual blend rather than silently resetting the targets to the hand/foot position. The editor slider reads and edits the effective contact weight; pointer click and Undo are covered by the real egui event harness.

A twisted-limb solver-switch regression found matching bone directions but different Y/Z axes when switching to HI. The existing two-bone contact roll reconstruction now also transports the reference twist for HI. It retains HI's solved joint positions; legacy `solve` and longer HI chains are unchanged. The regression passes at full matrix tolerance0.003.

Evidence before the final broad run:

- Focused commands/Scene run:116 passed, 3 ignored, including the solver switch (`/tmp/loom-contact-settings-solver-focused.log`). Earlier keys/rig/contact/Scene run:154 passed, 3 ignored. The final broad run also covers the later constant-fallback/future-key case.
- Separate editor/rig UI tests:8 passed (`/tmp/loom-contact-weight-ui-green.log`), including displayed contact weight, actual slider click and metadata Undo.
- Updated production Ray opt-in:1 passed in54.12s (`/tmp/loom-contact-settings-ray.log`), including full-matrix zero-release through limb settings for all four limbs, retained node/material fingerprints and prior capture/Free/freeze checks. This is not native performance evidence.
- Read-only review found no concrete issue in the weight/Auto Key path or two-bone HI roll expansion. General affine-parent direct calls to the solver helper and native visual guarantees were not certified.

Still incomplete: ordinary rig pose keys, contact routing for the separate legacy Key IK/FK command/timeline, contact editor rows, Free FK gizmo behavior, native interaction/performance and the remaining full goal. No new app installation or public push is part of this source checkpoint.

Final broad run exited0:1568 passed, 0 failed, 49 ignored across147 binaries (`/tmp/loom-contact-settings-full.log`, Scene/Commands/UI/Bevy with lib+integration tests). The UI suite passed449 tests with2 ignored in127.57s. This includes the later constant-fallback/future-key regression. Original Ray SHA-256 remains `84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935`.
