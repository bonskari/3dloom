# Prepared bone-axis line calculation: exact paired probe

This standalone release-prefix comparison uses the frozen real Ray document and one shared real LoomApp/AppState, both variants with bones visible. The wrapper only adds the new bone_axes Vec required by the new prefix; Deref exposes the unchanged actual app state. No production source edit, GPU work, mesh rebuild, or live-app mutation was performed.

The cache is built once from the same visible, non-link-replacing bones using scene.world_matrix(node.id) as pivot_world and object.bone_tip_local, then finite world endpoints. Its setup is outside the warm line-calculation timing. This is an isolated materialization of those axes, **not the whole rebuild or its additional production cost**: production already obtains pivot_world during its geometry rebuild. One cold standalone setup took0.986770ms/5258 animated_reads for420 axes.

Input SHA256:84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935. Full serialized Scene SHA256 before and after parity/timing:4cf225a47205a3a4c1b1b16e99115d4a416fad4311a413750df4101f0acddd90. No source node/weight/prop mutation occurred.

Old prefix SHA256:de71e17598689609661c7b177137d377f8ce8ef66badc9f922e0d0eaa8f6f287.
New prefix SHA256:13fa8216138cf31d4eb6d12bd7297e7c990ed5293c250ec38e99cefa677ffbcc.
Full new helper_overlay.rs SHA256:6738b48cae1fbc520c8d1a36223f6f21ea022ebf44541513c97b516daf51bd8b.

Before timing, all four viewport outputs passed **ordered exact f32-bit positions/linear colors and byte sRGB colors**. No sorting/tolerance was used. Axis-view counts were168depth/420top each; Realistic perspective0depth/588top. Matrices come from the frozen document's actual SceneSettings via LoomApp.camera_for; actual native diagnostic rectangles and perspective orbit bit-checks are preserved from the baseline probe. See result.json for matrices/rectangles.

Warm30 calls/variant, then4ABBAcycles×200four-view calls/block:

| Implementation | Mean ms / four-view call | Block range ms | animated_reads / call |
|---|---:|---:|---:|
| Archived old prefix |2.327134|2.302358–2.359733|21412|
| New prepared axes prefix |0.205691|0.201892–0.210340|496|

Difference2.121444ms/four-view call. Per-cycle differences2.113578,2.116272,2.128121,2.127803ms. Total timed old/new work3723.415/329.105ms across1600calls each. The gain is only the warm helper_overlay::lines calculation; no native FPS or complete rebuild claim.

Source preparations were allowed during root regression22363; direct rustc compile66633 began only after that job terminalGREEN13/13 and quiet confirmation. GPU agent explicitly held all jobs; probe39286 exited0 and released the quiet window. Compiler args identify exact completed release dependency artifacts; no Cargo build was run for this probe. Copy source files into target/helper-overlay-cache-probe and the retained diagnostics into target/helper-overlay-probe/diagnostics.json to reproduce with a coherent release build; compile.py must succeed before running. The probe reads source.json from target/helper-overlay-cache-probe. It never writes user documents/backups.

The archived diagnostics retain only viewport evidence; local preferences are omitted.
