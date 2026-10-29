# Native Ray authoring checkpoint — eb7b2f0

The installed local application now runs code commit `eb7b2f096b213cacaa96dd1ee9f5b525096376db`, binary SHA-256 `e676e84b887a51bf0f639f37482facb23b151000fd49a71f71bd1c34a4c865b2`. Release compilation exited0 in12m55s. The previous4421018 binary is retained as `~/.local/share/3dloom/loom.4421018.before-rig-pose-tools-20261029` with SHA-256 `8cade7e12cc4f8bbcf83339e0665f1d5fcb432ff3d39a6bfb364dc83082df1b7`.

The existing app exited through its own File→Exit UI, with the original Ray scene clean at frame39, selection4988 and empty Undo. Installation was atomic and the same user service was relaunched. No operating-system mouse/keyboard input, Unreal edit or public push was used.

## Actual application checks

The initial pixel comparison failed because the previous app had Rotate active while the new startup defaulted to Select. Selecting Rotate in the new app made all four viewport interior crops pixel-identical; the mismatch was experimentally isolated to runtime tool state. No product code or image baseline was changed to pass the comparison.

The aligned native pointer test passed: select FK control4975 directly in the viewport, rotate its gizmo, verify only that control's authored node record changed, Undo and reload the original scene. All node records, selection, frame, history and original source-file hash were restored. The renderer reported all5Skin objects/13parts committed without a geometry error or recipe recovery. See `pointer-verification.jsonl`, `default.png` and `fk-rotated.png`.

The separately opened Animation Editor was then exercised through actual native input:

- Key Rig Pose created frame39 keys for28owned controls in one Undo step. Saved real-scene copies verified all unowned source nodes (including skin data), source animation and materials unchanged.
- Dragging the Pose row moved39→49 across the actual FK PRS channels. Context-menu deletion removed actual keys. Undo restored the entire serialized scene, not only visible node read-outs.
- Planted, Sliding and Free buttons produced World/1, World/1 and Body/0 contact states at frame39. All raw source nodes/materials remained unchanged. Free also had its affine matching key. Full-rig capture with an active Free contact preserved the redundant raw goal position/rotation and pole position tracks.
- Four Undo operations restored the complete scene exactly to the baseline. The editor closed, the original scene reloaded clean at39/4988 with empty history, and all four viewport interior crops again matched the original image exactly.

`proof.json` records filtered evidence only. Native saved scene copies are retained locally in `/tmp/loom-native-pose-tools-edits-eb7b2f0/`; full State/preferences and the Kimodo endpoint are not archived here. `pose-timeline.png` and `planted-editor.png` show the actual extra editor.

## Ready geometry throughput

The unchanged native benchmark checks all5objects/13parts committed and counts submitted distinct pose times; it does not count unchanged UI redraws. Moving-clip windows exclude clip-tail/wrap windows. Full geometry correctness is established by the separate source/reference and native image/input checks, not an independent vertex fingerprint in every benchmark frame. No experimental or profiling environment flags were enabled. This is the established CPU skinning/GPU normal-position pipeline, not a new GPU bone-skinning claim.

| Run | Realistic average / minimum active1s window | Wireframe average / minimum active1s window |
|---|---:|---:|
| First candidate |63.55 /59.24|89.83 /82.72|
| Matched previous4421018 |66.63 /64.22|94.50 /89.08|
| Matched candidateeb7b2f0 |65.92 /63.00|94.47 /88.95|

The matched comparison restored the new binary afterward and both benchmark runs restored the original scene, frame, selection and shading. Full sample logs are retained. The latest run exceeds60in every included active window, but the first candidate run's single59.24window remains unexplained. No code changed between those two candidate measurements; this is not evidence that a performance bug was fixed. A single pair is insufficient for causal attribution of the small old/new difference.120was not achieved.

The15-minute native playback/real-auto-backup stability run exited0 after900.69s and30samples (`stability.jsonl`). All samples retained5objects/13parts with no geometry error or recovery;59418new submitted pose times were recorded. Observed30s rates were64.30–66.70. Resident-plus-swap memory during playback stayed between1324200and1407596KiB; the separate post-reload allocation state is retained in the raw evidence rather than labelled a leak-free guarantee. A real271346303-byte automatic backup completed after playback began and its entire scene equalled the original file. All node records, selection, frame, source hash and original empty Undo history were restored. This was a reliability run with concurrent correctness work, not a quiet FPS benchmark; its30s samples do not erase the earlier single1s59.24result.

Buffered Set Key native FK authoring also passed through the real gizmo and extra editor (`buffered-verification.jsonl`, `buffered-accepted.png`). A zero key at0survived a posed correction at39; accepting the key preserved all four viewport crops exactly; the accepted correction survived frame40; one Undo removed the accepted pose and complete cleanup restored the entire original scene/runtime. No source bone/controller/material data was rewritten. A focused rotated-rest-frame regression independently verifies analytic15°/30°/45°world rotations in both ordinary and buffered modes; all23gizmo-filter tests passed. Native stored local correction31.68°must not be confused with the gizmo reference-frame Euler read-out. No product fix was made or claimed from that check.

Remaining direct IK/Free interactions, target Auto Key, Biped end/body controllers, ordinary-key routing, full rig-channel timeline rescale, final Kimodo prompt→animation and physical Mac verification remain required. The full goal is active and incomplete.
