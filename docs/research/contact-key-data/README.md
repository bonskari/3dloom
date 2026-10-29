# Contact metadata increment

Typed per-limb contact keys hold finite times, editable IK blends and stable shared reference IDs. References contain a body/world pose with validated finite position and unit rotation. Validated deserialization rejects malformed tracks; keys must be strictly ordered with no duplicate times or dangling references. Insert/update/move operations validate before mutation, and removing a key retains a reference still used by another key. Empty tracks are omitted from RigLimb serialization and legacy limbs default to empty.

The held metadata lookup registers an animated read for every nonempty track, including queries before the first key. This establishes cache dependency information for the later evaluator; it does not itself drive skeleton evaluation. The existing installed application is unchanged by this increment.

RED evidence records missing model/module and missing RigLimb field before their implementation. Tests cover fractional, reverse and random lookup order; invalid input; atomic collisions; reference edits; compatible serialization; authored-data save/read, merge, mapping reapply and Undo/Redo.

Not delivered here: visible planted/sliding/free behavior, capture continuity, pole/end affine handling, target editing, contact commands or timeline UI. These remain required by the active animation goal.

Verification: `cargo test -p loom_scene -p loom_commands -j4` exited 0: 977 passed, 0 failed, 8 ignored across 143 test/doc-test binaries. The final added reference-edit test then passed in `cargo test -p loom_scene --lib loomrig_contact -j4` (5 passed). The broad suite emitted one pre-existing unused_mut warning in review7_fixes.rs:175; it was not modified. Ray source SHA256 remains 84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935; installed app ping remains commit442101869f6cfaaa19d0335f6c002878da707810.
