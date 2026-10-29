# Contact controller source checkpoint

Effective contact goals and poles now expose their evaluated world poses to viewport transforms without replacing retained source helper position/rotation tracks. Contact-only SetTransform, selection transforms, locks, type-in and gimbal orientation route edits to active references. Pending master/parent transforms are evaluated before grouped target edits; dependent edits must converge before committing. Ordinary unparented controls in one rig do not clone the scene for this evaluation.

Set Key mode previously discarded contact target edits while converting a buffered pose into keys. The posed rig metadata is now restored with node keys in the same Undo group when the IK parameter filter is enabled. Tests cover retention, Undo/Redo, filtering and explicit discard. This preserves explicit contact authoring; it does not implement ordinary rig pose keys or Free matching.

Review identified a cross-rig contact dependency cycle outside the node hierarchy. An isolated two-rig regression reproduced stack overflow before the fix. Iterative dependency validation now rejects these cycles before world evaluation, records the error and permits evaluation after the dependency is removed. The review accepted the correction without further findings.

Verification:

- Before the final cycle guard, the broad scene/commands run passed 1010 tests, with 9 ignored, across 143 test binaries (`/tmp/loom-controller-checkpoint-tests.log`). This is not evidence for the later guard.
- Final focused scene/commands/keys/contact run passed 128 tests, with 3 ignored (`/tmp/loom-contact-cycle-final.log`). It includes cross-rig rejection, finite fallback, diagnostics and recovery.
- Final Ray capture/freeze preservation passed separately (1 test, 31.15s, `/tmp/loom-contact-controller-ray-final.log`) with unchanged original scene SHA-256 `84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935`.
- Read-only review covered the full source diff and the subsequent cycle fix. Declined scopes remain open: shared reference editing semantics across other times, advanced pivot-only/child-compensation modes (explicitly rejected), Free matching, normal pose keys, editor rows, native rendering/input and performance acceptance.

Task14 and the full goal remain incomplete. No installed application replacement, source scene save, public push or native-performance claim is made by this checkpoint. Separate Animation Editor tools and actual Ray interaction/performance verification are still required.

## Follow-up interaction evidence

The real egui pointer-event harness now drags the selected contact goal and pole through the Move gizmo. It verifies one Undo per gesture, unchanged source/helper transform tracks, hand-to-goal agreement, and pole-driven elbow movement without moving the hand target. This is UI input integration evidence, not a physical-device or installed-native-app check.

The full UI library suite passed 448 tests with 2 ignored and no failures in 132.09s (`/tmp/loom-contact-ui-suite.log`).

The opt-in production Ray test also moves each limb's goal and pole together, checks evaluated target/end positions and exact retained source-node/material fingerprints, then undoes the edit before the existing capture/freeze checks. It passed in 42.83s (`/tmp/loom-contact-controller-ray-gesture.log`). Debug command timings were 16.08ms left leg, 20.34ms right leg, 20.05ms left arm and 14.69ms right arm while the UI suite was running concurrently. These are command timings, not FPS or a native interaction-latency acceptance result.
