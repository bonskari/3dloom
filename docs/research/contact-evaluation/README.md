# Contact evaluator — development checkpoint, not native delivery

The Scene LoomRig solver now consumes typed contact samples. World-space references remain world-space; body-space references use the characterized root's current full world frame (including master offset). Independent references interpolate world position, pole and proper rotation; shared references retain a single target. Key blends interpolate and override the legacy blend only within the authored contact domain. Before the first key and with an empty track, existing goal/pole/blend evaluation is retained. Static scenes register contact time dependence before their first key.

References now also carry a validated pole_position in their chosen space. No original Ray contact data exists: the prior metadata checkpoint was unreleased and uninstalled. This field is required rather than inventing a captured pole on load.

Contact end orientation uses a proper polar frame. Mirrored frames use a fixed X-reflection convention. Replacing orientation left-rotates the full solved world linear transform and converts it back through the actual parent inverse; it retains signed stretch/shear and the solved pivot instead of decomposing a sheared parent to a quaternion. This policy is tested on a bent three-joint chain with mirrored/nonuniform rotated ancestors, including no-pop capture and an orientation-only edit. This is bounded evidence, not certification of every possible skeleton/controller configuration.

Nonfinite, nonaffine, singular or nonconvergent reference frames return errors. Scene retains runtime per-rig/limb errors and exposes them in application diagnostics at playback.rig_contact_errors. A failed target is not silently reported as solved. Wide finite key intervals interpolate in f64 to avoid f32 time-subtraction overflow. These cases have RED/GREEN evidence.

The real-command fixtures test joined hold, moving targets, root and master movement, body reference, free release, fractional/reverse/random lookup, blend interpolation, no source-controller mutation, capture continuity, affine stretch retention and invalid-frame recovery. No native app replacement or original Ray file edit occurs here.

Still required before native contact authoring delivery:

- Atomic preset/capture/edit/move/delete commands and Undo/Redo; ordinary rig pose keys.
- Shared effective target visualization and direct goal/pole/body editing; the old helper display is not yet contact-aware.
- Separate Animation Editor contact rows and editable properties; real Ray inspection and post-change performance/stability.
- Contact-aware New Scene Keep/animation-deletion behavior. Source audit shows the existing metadata-freezing loop only handles blend and pose_offsets; author and verify the contact case before enabling native authoring.
- Broader native skeleton/affine and Kimodo/Mac acceptance required by the full goal.

Task11 remains in progress. This checkpoint is not a claim that planted/sliding/free authoring is finished or available in the installed application.

Verification: `cargo test -p loom_scene -p loom_commands -p loom_ui -j4 -- --test-threads=1` exited0: 1432 passed, 0 failed, 10 ignored across146 test/doc-test binaries; UI447 passed/2ignored. The focused sampler run passed7 tests. Retained regression output includes existing compiler unused/dead-code warnings in unchanged code; no unrelated cleanup was performed. Original Ray source SHA256 remains84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935. The installed application still reports442101869f6cfaaa19d0335f6c002878da707810; these new evaluator tests do not certify native Ray contact behavior or performance.
