# Timing investigation after native acceptance gap

Instrumented171722a, real Ray scene, four viewports, Perspective Realistic. All source nodes/selection/frame restored after22seconds of playback. `LOOM_PROFILE_FRAMES=1` only for diagnosis; no other experimental overrides. These inclusive profile timings are not production FPS acceptance.

After startup and partial windows, steady reports show ui.loomrig_prepare approximately0.011ms per ready pose; helper_lines0.068–0.069ms per viewport and helper_draw0.021ms per viewport, approximately0.36ms combined over all four views. These helper scopes include the original helpers/bone lines, not just the new FK outlines. Do not remove original geometry or controls to gain FPS.

The same steady reports show ui.frame14.169–14.402ms, ui.rebuild_world_meshes5.630–5.714ms, ui.skin_batch3.167–3.207ms, bevy.sync4.711–4.792ms. gpu_pose.renderer3.233ms is nested within Bevy synchronization, not an independent cost to add. Inclusive/parallel timings must not be summed as independent frame costs.

The new preparation's measured cost is small; no claim that it explains the roughly1fps variation or the lowest production windows. The production acceptance gap remains open. Next investigate measured shared rebuild/skin/sync boundaries, preserving exact outputs and all geometry/weights/materials. Do not markTask9or the full goal complete from average FPS alone.

Raw filtered journal reports retained in timing-boundaries.log. The application log did not contain stderr profile reports; the initial empty temporary capture was not used as evidence. The journal was filtered to timing lines only; no full State/preferences or local API address is archived.
