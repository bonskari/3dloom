# Free contact FK matching

The command regression first reproduced Free activation discarding the visible solved pose (`/tmp/loom-free-matching-red.log`). The authoring command now captures a rig-owned affine FK correction for each of the three limb joints at the requested time. It compares the visible local matrices against a real-scene probe with that limb's contacts and previous matches disabled, retaining source controllers and existing artist pose offsets.

`RigPoseTrack` stores validated correction keys. Proper polar rotations interpolate with slerp; full stretch and translation interpolate independently. Corrections must be finite, invertible affine transforms with positive determinant. Empty tracks remain omitted from serialization and retain legacy evaluation. Animated tracks report time dependency even before the first key; frozen corrections have no keys or animated reads.

Free capture inserts the correction and an earlier baseline where needed in the same metadata transaction as the contact key. Moving/deleting a contact also moves/deletes its same-time correction. At zero blend, matched FK matrices bypass the subsequent IK/TRS reconstruction, preserving end-frame shear. Source bone/helper controllers, skin weights and materials are not baked or replaced.

Authoring a new key changes the interpolation interval by design. This does not promise that all prior interpolated poses remain identical after adding a Free key. The no-pop check compares the requested-time pose immediately before and after capture. Ordinary rig pose-key authoring, active-contact IK settings and Free-mode controller interaction remain unfinished.

Verification before the final broad run:

- 150 focused Scene/Commands/rig/key/contact tests passed, 3 ignored (`/tmp/loom-free-matching-final-focused.log`). A later additional assertion evaluates the moved Free key at its destination; the final broad run covers it.
- Plain, nonuniform and mirrored hierarchies retain all three solved world matrices; tests cover source transforms, Undo/Redo, move/delete, actual copy-save/open and constant freezing without time dependency.
- The production Ray test passed all four limbs at frame40 (1 test, 46.82s, `/tmp/loom-free-matching-ray.log`), retaining complete source-node/material fingerprints and existing capture/freeze checks. It is command/evaluation evidence, not native rendering or FPS acceptance.
- Read-only review found no concrete issue. Its suggested destination-pose assertion was added. Declined scopes remain open: ordinary pose-key commands, Free FK gizmo behavior, contact-aware SetLimb settings, UI/native/performance and preservation of the entire prior interpolation interval.

No installed app replacement or public push is part of this checkpoint. The full goal and integration tasks remain active and incomplete.

Final broad verification: `cargo test -p loom_scene -p loom_commands -p loom_ui -p loom_bevy --lib --tests -- --test-threads=1` exited0, with 1561 passed, 0 failed and 49 ignored across 147 test binaries (`/tmp/loom-free-matching-full.log`). The UI suite accounts for 448 passed/2 ignored. This includes the destination-pose assertion added after review. The separately executed production Ray opt-in test is not counted in that total. Original Ray file SHA-256 remains `84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935`.
