# Four-viewport bone-overlay read cost

Read-only isolated release probe of the original `helper_overlay::lines` implementation after bone axes were added. No production edit, GPU work, scene mesh rebuild, or live-app modification was performed. The source is retained as `helper_overlay_snapshot.rs`: its `lines`/`bone_axis` implementation is unchanged; only the subsequent painter `draw` function/tests were omitted for standalone compilation. The unused `helper_labels` import warning reflects that omission.

Frozen real Ray input: `/tmp/loom-conditioning-capture/input.loom`, SHA256 `84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935`, 461 nodes. Source full-file SHA256 `36b657fbd1733f579c292160f4e255b5caf1d6241cf9663fde7297ea89db9735` (prefix hash in source.json). Cameras come from this document's SceneSettings via real LoomApp::camera_for; viewport rectangles were read from the paused native app's diagnostics. The perspective camera distance/yaw/pitch were bit-checked against those live diagnostics. Matrices/rectangles are recorded in result.json. The first three viewports use the frozen axis-view cameras, fourth is Realistic perspective.

Frame 39, warm 30 four-view calls per filter state, then four ABBA cycles of 200 four-view calls per block. The only paired state edit was display.hide_bones; all other helper code, scene data, and matrices remained identical. Scene/IK caches were warm. This does not measure changing-pose or geometry-refresh cost.

| hide_bones | Mean ms / four-view call | Block range ms | animated_reads / call | Depth / top lines |
|---|---:|---:|---:|---:|
| false | 2.335728 | 2.322681–2.343873 | 21412 | 504 / 1848 |
| true | 0.085304 | 0.084806–0.085953 | 496 | 504 / 168 |

Paired difference: 2.250424 ms / four-view call and 20916 animated reads. Per-ABBA-cycle differences: 2.255806, 2.245563, 2.244744, 2.255585 ms. The top-line difference is 420 bone axes × four views. Total timed visible/hidden work: 3737.165 / 136.486 ms across 1600 calls each.

This isolates overlay line calculation. It is not native FPS, GPU/painter cost, or proof of the cause of every reported performance regression. Root had completed native benchmarks and the app was paused; no active rustc/Cargo jobs were observed at measurement start. Direct compile handle39663 and run41814 both exited0.

Reproduction: copy these source/probe/compiler files into `target/helper-overlay-probe/`; capture read-only native diagnostics there as diagnostics.json; run compile.py after a coherent release build and only run the produced probe after compile succeeds. It resolves exact release dependency fingerprints. Preserve the frozen input checksum. No LOOM_HOME or user backup paths are written by the probe.

The archived diagnostics include only viewport evidence; unrelated local preferences are omitted.
