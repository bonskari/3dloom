# Native capacity and material-ID checkpoint

Installed source/binary `327f1bd`, explicit precise8, profiling disabled. Same full Ray scene, four viewports and frame-39 camera as preceding runs. No other agent build or GPU jobs were running.

| Shading | Active ready pose times/s | Minimum retained one-second window |
|---|---:|---:|
| Realistic | 62.2215 | 61.3354 |
| Wireframe | 95.6055 | 91.1590 |

Both exceed 60 in this bounded benchmark. This is not completion of the expanded animation-tool goal; default-startup and longer reliability checks follow. No 120 fps claim is made.

The exact runner and its active-clip-window policy are archived with the first precise8 native comparison. All five skinned objects/thirteen parts remained committed without errors or recoveries. No geometry, materials or influences were reduced. Frame 39, original selection/shading and every node record were restored; the source scene file stayed unchanged. All four viewport crops were pixel-identical to accurate `09a93cd`.

Metrics count distinct submitted ready pose times, not an independent hash of every vertex on every frame. Separate source, geometry, Float64, byte-parity and image checks substantiate fidelity. The normal-launch default was not yet changed in this measured binary.
