# First native precise8 comparison

Installed binary/source commit `80eb3f9`. Both runs used the same binary, four viewports, full Ray geometry/material parts and the original frame-39 camera. Explicit `LOOM_EXPERIMENTAL_GPU_POSES=precise8` was compared with `=1` (the unchanged fast5 comparator). Profiling, builds and other agent GPU jobs were stopped. Ordinary default launches still use the CPU backend.

| Mode | Realistic ready pose times/s | Wireframe ready pose times/s |
|---|---:|---:|
| precise8 | 49.6991 | 76.9220 |
| fast5 | 53.1380 | 77.7250 |

The 60 fps requirement is **not met** in Realistic. These results do not certify the goal complete. Subsequent work investigates the bone overlay and precision refinement costs independently.

Each mode warmed for eight seconds and collected 24 approximately one-second windows. Summaries retain only windows with increasing submitted times inside the moving clip range below frame 149, excluding loop transitions and the clip tail. They measure new ready geometry pose times submitted by the application, not an independent fingerprint of every vertex in each frame. Geometry correctness has separate actual-asset gates. Raw logs include every window and the minimum retained-window rate.

Five skinned objects/thirteen parts and no GPU pose errors or recovered recipes were required throughout. Both runs restored frame39, original shading/selection and all node records; the scene SHA remained `84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935`. No scene save occurred. Between modes, the app was closed through its own File > Exit before relaunch so the automatic-backup lifecycle could finish normally.

`benchmark.py` is the exact runner. Supply a new log path and expected experimental mode; it refuses to overwrite an earlier log. It requires an already-open paused clean Ray scene and an otherwise quiet GPU. Local preferences are not archived.
