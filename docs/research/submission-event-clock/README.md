# Ready distinct-submission event clock — native comparison pending

The existing counter is unchanged: only the final non-discarded UI submission with current ready geometry is recorded; duplicate UI input times and held pose samples do not become new sampled animation times. Add a monotonic Instant timestamp at that same record point, updated only when the distinct counter changes. Its epoch is private to the app instance. No source geometry, weights, material, playback timing or rendering gate changes.

The private record_at helper allows deterministic clock testing. Tests cover uninitialized state, first event, duplicate UI pass, held pose redraws, new samples, scene replacement, and independence from artificial input-frame times. API RED is missing field/helper compilation, not a rendering bug. Focused tests2passed; full UI447passed/2ignored.

The diagnostic field is playback.submitted_distinct_seconds. The verifier retains the original seconds/fps/active_ready_pose_fps/min_active_window_fps client-wall fields, adding event_seconds/event_fps/active_event_ready_pose_fps/min_active_event_window_fps with the identical pose counter delta and active-clip filter. It rejects a missing/nonadvancing event clock instead of silently substituting one. Start records declare both rate clocks. Archived old comparisons are retained unchanged.

This is measurement clarification, not a performance improvement. No claim that IPC caused the prior lowest window has been established. Native side-by-side clock results remain pending. Do not enable the parallel preparation experiment by default or declare performance acceptance from this code change alone. Keep all geometry-ready and distinct-pose gates unchanged.

## Native dual-clock result

Installed95eac04, exact original source, identical fresh-process native viewport/rotate/Undo/reload verification, no profile or unrelated experimental flags; parallel flag explicitly recorded. All four viewport pixels, all nodes, source bytes, frame and selection were preserved.

| Configuration | Realistic client / event mean | Realistic client / event minimum | Wireframe event mean |
|---|---:|---:|---:|
| parallel on |64.2758 /64.2773|60.7550 /60.7639|91.5013|
| parallel off |62.8018 /62.9481|59.7092 /59.7109|91.6856|

The clocks agreed closely on the Realistic minima. This result does NOT establish IPC timing as the cause of the earlier below60window. The on run passed the accepted moving-window60gate in both clocks; off did not. The same numerator/readiness/filter was used, and old client-clock fields and old comparisons remain available.

The experiment's own15minute runtime-preparation reliability/real-backup test is now live on95eac04 with explicit flag1. No completed result is claimed yet. Do not transfer the earlier b4 serial-preparation stability result to it. Parallel preparation is still defaultoff in source. Native production-default verification is required before delivery of any later default change.
