# Native direct end/body controls — c4708d0

Code commit `c4708d049792d8322d211b0d59c58626df7cd8cc` is installed locally; binary SHA-256 `1262d68ce836c74e9348700812f42b850d497cbd25c08217c78492cb542270b4`. The release build completed in 20.32s. Prior 71883fd binary and four autobacks (814,052,858 bytes) were retained and hash-verified before replacement. There was no original scene write or public push.

Native app-owned pointer input selected the actual FK foot, pelvis, master and IK hand cages and dragged their Move gizmos. Each gesture made one Undo entry. FK/body operations changed only the intended existing input node; the IK operation changed its contact target without changing raw nodes. Complete source nodes/Skin/modifier/material data were retained. One Undo restored the complete pre-edit scene. All five skinned objects/thirteen parts remained committed with no reported GPU-pose error or recovered recipe. The four posed screenshots were inspected; desk/chair remained stationary during character/body moves.

After cleanup and reloading the verified unchanged source, nodes, frame 39, selection 4988, empty history and modes were restored. All four viewport interiors matched this new version's baseline pixels exactly. The original scene SHA-256 remains `84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935`. Full legitimate scene copies remain under `/home/b1s/dev/scratch-scripts/loom-native-c4708d0-shapes-v2`; no State/preferences dump was saved.

The initial pointer verifier clicked the previously probed foot location and selected the desk. Inspection of `cycle_object_pick` confirmed that repeated clicks within 2 px advance through overlapping objects, with the same hit list retained across reopening this file. The verifier now starts a first-selection test with a genuine empty-background click. This is explicit test setup; no product code was changed to bypass picking. The first run's pending copy is retained locally.

Three native ready-pose runs are retained. They count distinct submitted pose times with complete five-object/thirteen-part geometry ready, exclude clip-tail/wrap intervals, and do not independently fingerprint each submitted vertex buffer.

| Run | Realistic average / minimum window | Wireframe average / minimum window |
| --- | ---: | ---: |
| First new-build desktop run | 62.2712 / 57.2139 | 89.6939 / 83.1662 |
| Matched prior 71883fd | 66.5204 / 64.4026 | 94.3544 / 89.3211 |
| Matched current c4708d0 | 67.0263 / 64.3038 | 94.6825 / 89.8023 |

The matched pair used the same original scene, camera/tool/mode settings, fresh processes and benchmark, with no screenshot viewing during measurement. It restored the current binary and clean source runtime afterward (PID 1028721). The first run occurred while native screenshots were being viewed and other desktop GPU activity had been observed. These observations do not establish why its Realistic windows dipped below 60. No run is removed, the dip is not called fixed, and 120 is not achieved. A single passing pair does not prove stability across all desktop activity.

An additional outstanding observation: undoing only native selection markers back to empty history left the dirty flag true even when the entire scene matched the source and selection was restored. Reopening the verified unchanged source reset it. A focused saved-marker Undo/Redo regression and root correction are still required; no saved-state fix is claimed here.

`authoring-proof.jsonl`, `proof.json`, three complete throughput logs, screenshots and `verify.py` retain the scoped evidence. Functional native control checks passed; intermittent Realistic frame-time dips, saved-state bookkeeping, remaining ordinary-key/time-rescale/Kimodo/Mac work and final integrated acceptance keep the full goal active.
