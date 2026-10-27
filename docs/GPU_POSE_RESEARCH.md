# Current-pose GPU geometry research (2026-10-27)

Status: test-only, unwired. Realistic playback has not reached the 60 distinct
poses/second target. None of these experiments supplies production tangents.

`gpu_skin.rs` computes all supplied influences (including Ray's twelve and joint
874), missing-bone handling and relative residual weights. Its affine point
arithmetic discards intermediate w, matching glam's transform_point3 semantics;
valid inverse-affine bottom-row roundoff is accepted. `gpu_pose_geometry.rs`
reconstructs CURRENT face Newell and smoothing-group fan normals, including
explicit/faceted normals and material packing. These are not rest normals.

The original strict actual-Ray guard remains an ignored diagnostic and fails:
world position errors reached 3.71e-4 cm; world-space legacy Newell SG normal
vector differences reached about 0.11. Feeding identical CPU positions to the
GPU still produced a 9.70e-3 normal difference. Exact-represented centering
reduced this to 9.52e-4. This establishes translation-sensitive cancellation;
it does not establish bit parity.

An alternative first-current-vertex-relative cross-sum reconstructs unit face
normals before the original ordered SG sum. Across all five Ray skins at
0, 39.25 and 149, its maximum normal-vector error against f64 on the SAME
GPU-represented positions was 2.79e-6. Legacy CPU normals against f64 on CPU
positions reached 0.156. Distinct GPU/CPU represented positions still affect
thin triangles: corresponding f64 normals differed up to 0.0941. No production
CoreMesh normal math changed.

## Coupled normal-mapped images

The image experiment explicitly recomputes corrected CPU Mikk tangents on each
candidate's CURRENT positions/normals. This is a test oracle, not an eventual
CPU/GPU rendering pipeline. Comparing new normals with old tangents would not
be a valid TBN quality comparison.

Initial 18 views at 256 square were too dark for useful face inspection. A
separate nine-view audit uses camera-facing 10000-lux directional light, 512
square front and two side-front views at all three poses, complete head NDC
framing, actual diffuse and varying normal textures. All scene material parts
are preserved. A flipped-tangent control changed 102775 pixels.

Stable GPU normals with coupled current Mikk versus f64 normals on the same
GPU positions with coupled current Mikk: four images identical; five images
had one or two pixels differ by one RGB unit, with no coverage differences.
Legacy versus stable changed 2350–3261 pixels per view, maximum 42 RGB units,
mean absolute RGB difference 0.003858–0.005094 on a 0–255 scale; one view had
two coverage pixels differ. Inspection of the lit 39.25 side-front pair showed
visible eyes/nose/skin detail and no obvious artifact at this resolution. This
is bounded quality evidence, not comprehensive visual acceptance.

Artifacts from the local opt-in tests:
`/tmp/loom-gpu-pose-lit-audit/{time}-{view}-{legacy,gpu-stable,f64-oracle}.png`.

## Current tangent evaluator

`gpu_mikk_group.wgsl` evaluates actual ordered EvalTspace subgroup fixtures
captured from CPU corrected Mikk at Face001/39.25. All 256 groups differ in
bits; maximum tangent-vector error was 1.91e-7 and relative magnitude error
2.48e-7. Native GPU acos differed in 500 input angles by at most 1.19e-7.
This implements subgroup arithmetic, not weld/group selection or the complete
Mikk algorithm.

Missing GPU stages are current attribute welding, corrected deterministic edge
pairing, orientation/connected groups and subgroup selection, deterministic
last-corner ownership, and degenerate epilogue. These must be checked against a
CPU structural oracle on the SAME GPU attributes. No assumption that bind-pose
welds/groups remain valid is authorized. Nonmanifold edges, UV/normal seams,
material isolation, signed zero equality and degenerate triangles need explicit
coverage. Native acos's numerical difference needs coupled image acceptance.

CPU editor/picking geometry, presented-pose synchronization, bounds, unsupported
modifier stacks/hardware and complete StandardMaterial passes remain integration
work. No shader quality feature is disabled, and no influence is truncated.

## Opt-in tests

Set LOOM_PERF_SCENE to the actual RayAnimationTest_LoomRig.loom document and run
these ignored Bevy lib tests individually:

- `gpu_pose_geometry::tests::actual_ray_gpu_skin_normals_and_material_packing`
  (strict legacy guard, intentionally still fails).
- `gpu_pose_geometry::tests::actual_ray_origin_relative_sg_accuracy_against_same_position_f64`.
- `gpu_pose_geometry::mikk_probe::actual_current_mikk_groups_measure_gpu_math_and_native_acos`.
- `gpu_pose_geometry::render_audit::actual_ray_stable_gpu_sg_lit_front_image_audit`.

GPU synthetic all-influence/current SG packing and affine-roundoff regression
tests run normally. The actual scene and GPU tests are genuine work, not mocks.

## Current weld and corrected edge preparation

`gpu_mikk_prepare.rs` / `.wgsl` is a further test-only stage. It consumes the
current packed GPU buffer and immutable triangle indices directly. A separate
invocation per material part preserves material isolation. Vertex classes use
current position, normal and UV equality, with signed zero canonicalized for
hashing. Hash collisions compare the complete key. Atomic minimum
representatives make labels deterministic. Edge incidence uses a separate
collision-checked undirected-edge hash and unbounded-by-valence linked lists;
no incidence is truncated. Each edge class pairs opposing edges in corrected
prepared-face order, including nonmanifold buckets.

The triangle-only DegenPrologue is deliberately serial in this prototype to
preserve the existing exact swap order; inactive degenerate triangles remain in
the tail. Current orientation/derivative-degeneracy flags are computed on GPU.
MeshData already presents triangulated faces, so source quad triangulation is
outside this stage. Complete groups/subgroups and degenerate tangent epilogue
are still absent. This stage is not a complete tangent implementation or a
production performance result.

Synthetic GPU checks cover signed zero, UV and current-normal seams,
interspersed/all-degenerate triangles, four-way nonmanifold opposing pairs,
persistent buffer updates, forced collisions for every hash key and reported
nonfinite inputs. Actual Ray three-pose checks at 0/39.25/149 passed across all
five skins/all thirteen parts against the actual corrected CPU preparation
fixture on the SAME GPU-readback attributes. Classes are compared after
canonicalizing representative labels, while original face order, current flags
and active face-neighbor indices are compared exactly. The complete 317-pose
sweep from 0 through 149 passed all 4121 comparisons (317 times thirteen parts).
Its 239.88-second test duration includes CPU oracles and GPU readbacks and is
not a rendering throughput measurement. The current full Bevy lib test binary
passed 47 tests with 18 opt-in tests ignored; all three preparation synthetic
tests passed.
