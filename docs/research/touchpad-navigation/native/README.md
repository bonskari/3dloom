# Installed Linux native verification

Installed5f91f1e passed six actual application IPC/input checks: pixel pan, native Zoom event, Command zoom once, Alt orbit, explicit X11 Line-pan mode, precision-mouse Point zoom. All original node records, selection, source file bytes (SHA25684fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935) and original device preference were restored. Four viewport crops exactly match the prior version. These are application input-integration checks; they do not certify physical Mac hardware.

No profile/experimental flags in ready-pose benchmarks. Both versions include the prior topology-snapshot adoption fix; old backup reports8b5e2fd but contains its documented two-file adoption patch (source944e16c). New executable is5f91f1e. Both use the same full source scene, four viewports, restored frame39 and PointerGone condition.

| Version/state | Realistic ready poses/s | Wireframe ready poses/s |
|---|---:|---:|
| New fresh |59.8276|90.1410|
| New same-process reload |57.4202|87.5009|
| Old fresh |58.7607|89.0240|
| Old same-process reload |57.5378|86.9176|

These measurements do NOT pass the60poses/s Realistic goal. They do not identify the cause of current lower throughput or show a slowdown unique to touchpad changes. No unchanged UI redraws are counted as new poses; moving-clip windows use the earlier retained benchmark policy.

A separate diagnostic5f91process verified retained half-edge predicates remain cheap after reload (around0.156/0.069ms TOTAL per~1400checks). Recent steady timings: ui.frame14.62–14.71ms, skin_batch2.92–2.97ms, bevy.sync~5.25ms, GPU pose renderer~3.55ms, guard_wait~0.585ms, geometry_data~0.91ms. Profiling scope counts on render.upload.queue are invocations, not proven GPU write counts. Actual nonempty write/byte counters are the next measurement; no batching benefit is claimed.
