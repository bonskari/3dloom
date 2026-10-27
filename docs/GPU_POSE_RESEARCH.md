# Current-pose GPU geometry research (2026-10-27)

Status: experimental hybrid current-geometry path; native activation is gated
separately. Realistic playback has not reached the 60 distinct poses/second
target. The core now computes complete current GPU tangents using a genuine
CPU structural compiler; general production quality is not established.

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

## Initial captured-group tangent evaluator

`gpu_mikk_group.wgsl` evaluates actual ordered EvalTspace subgroup fixtures
captured from CPU corrected Mikk at Face001/39.25. All 256 groups differ in
bits; maximum tangent-vector error was 1.91e-7 and relative magnitude error
2.48e-7. Native GPU acos differed in 500 input angles by at most 1.19e-7.
This implements subgroup arithmetic, not weld/group selection or the complete
Mikk algorithm.

At that initial checkpoint, missing GPU stages were current attribute welding, corrected deterministic edge
pairing, orientation/connected groups and subgroup selection, deterministic
last-corner ownership, and degenerate epilogue. These must be checked against a
CPU structural oracle on the SAME GPU attributes. No assumption that bind-pose
welds/groups remain valid is authorized. Nonmanifold edges, UV/normal seams,
material isolation, signed zero equality and degenerate triangles need explicit
coverage. Native acos's numerical difference needs coupled image acceptance.

Production transport, presented-pose synchronization, bounds, unsupported
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

### Stage timestamps and structural recipe reuse

GPU timestamp queries exposed a prototype bottleneck: serial DegenPrologue took
58.6–59.2 ms across thirteen Ray parts at the three sampled poses, even though
all parts had zero position-degenerate triangles. Parallel per-triangle flags
and workgroup-reduced active counts now establish identity order, and the
original serial swap procedure runs only when degenerates exist. Synthetic
interspersed/all-degenerate ordering is still checked against the CPU path.

All thirteen parts now dispatch in one encoder with one pass per stage and one
submit. Across 317 poses, GPU preparation span averaged 1.026 ms (p50 0.811,
p95 2.609, maximum 9.744); CPU encoding averaged 0.758 ms and submission 0.155
ms in the optimized isolated probe. The three-pose sample alone was much faster
(0.139–0.157 ms GPU); it should not substitute for the longer result. These
measurements exclude upload, normal generation, final tangents and rendering.
Timestamp queries and oracle readbacks are also not a production frame loop.

Canonical current weld classes, exact active/tail face order and initial flags
were compared against both the first and previous pose. Both guards hit all
4108 non-initial part checks (317 poses, thirteen initial parts). All 4121 CPU
structural parity checks passed. This supports a hybrid current structural
compiler plus GPU dynamic tangent math, rather than requiring GPU group
construction for this animation. Cached recipes must still validate these
current guards and recompile on misses before presenting candidate output.
Sixteen current UV/derivative-degenerate Any triangles are present on Face
parts; their globally ordered orientation decisions cannot be dropped.

A separate full-width avalanche hash candidate was tested on another 317-pose
sweep and removed: measured GPU span averaged 1.667 ms, worse than the original
hash run. Both retained exact structure/guard results. No hash-collision cause
for the timing variability is established. A sampled isolated run had GPU P5
and 405 MHz memory clocks; sustained production/render timing remains required.

## Complete hybrid tangent candidate (2026-10-28)

`gpu_mikk_recipe.rs` / `.wgsl` consumes `mikktspace::compile_structure` output.
That production compiler stops after actual current welding, corrected edge
pairing and Build4RuleGroups; it emits no tangent values and performs no tangent
setters. The cached recipe contains indices, initial/post-Any flags, assigned
groups, ordered group members, representatives and orientation. Every pose still
validates current canonical classes, flags and active/tail ordering. A rejected
guard cannot overwrite prior candidate output; the renderer must reject the
whole pose before copying any candidate into persistent render buffers.

GPU work recomputes current UV derivatives, dynamic subgroup membership,
projected corner-angle weights, tangent accumulation, the original last-corner
output owner and triangle-degenerate epilogue. Synthetic tests cover all-Any,
all-position-degenerate and interspersed-degenerate inputs, Any orientation,
persistent pose updates and real guard misses. Input CPU tangent fields are
poisoned with NaN in a test; candidate output remains finite and correct.

A native float dot predicate was proven wrong on Face001/39.25: actual GPU dotT
was -1.0000001192092896, CPU -0.9999998807907104; the GPU omitted one member even
when fed the exact current CPU derivatives. The subgroup mask was 125 versus
127. A geometric 180-degree antiparallel predicate now handles exact opposite,
near opposite and zero directions without adding an epsilon. Corner weighting
uses stable atan2(cross length, dot), preserving the original pi/2 rule for a
zero projected edge. The old strict CPU comparison remains an ignored failing
characterization, not a relabeled passing test.

This exposes CPU float boundary artifacts in the opposite direction too:
Face001/0 vertex87923 candidate-vs-f64 error was 6.86e-8 while CPU error was
0.00341184. SkeletalMesh002/149 vertex14387 candidate error was 1.61e-7 while
CPU error was 1.52455. These are independently checked against the same current
represented attributes; they are not evidence that every CPU delta improves.
The historical ad hoc "pants" artifact name identifies SkeletalMesh002; its
isolated closeup shows collared clothing, so the node name is authoritative.

### Full independent numeric characterization

`gpu_mikk_f64.rs` independently recomputes f64 UV derivatives, projections,
dynamic angular subgroup tests and atan2 weights, then preserves original
output ownership and inactive-triangle copies. Its output is the rendered
vertex tangent and handedness; unused bitangent/magnitude values are not the
acceptance target. Current represented P/N/UV are identical inputs to both
paths, and structural metadata has separate exact checks.

The full 317-pose, thirteen-part diagnostic examined **105,868,173 vertices**.
All handedness and finite-value checks passed; all 4108 non-initial structural
guards hit. There were **102 vertex samples above the conservative 2e-5 f64
vector gate**, with maximum **0.0001710423** at Body001/95.71835. The strict f64
acceptance test therefore still fails and remains visible. The diagnostic
completing is not a strict-gate pass. It also recorded 1527 CPU-delta offenders;
no blanket claim that every offender improves is made.

### Actual-material images

Robust GPU, current CPU tangent and independent f64 tangent images use identical
current represented positions/normals and actual diffuse/normal textures. The
original legacy CPU geometry/TBN is saved as an additional reference. Camera
framing and varying normal-map data are checked, with tangent mutation controls.

- Nine lit 512-square head views at 0/39.25/149: seven GPU/f64 images identical;
  two have one or two pixels differ by one RGB unit. Mutation: 102775 pixels.
- Twelve component-only Body views include the worst 95.71835 sample; its three
  GPU/f64 views are identical. Mutation: 73749 pixels.
- Nine isolated SkeletalMesh002 clothing views frame the offender using its
  actual triangle normal and a facing light. GPU/f64 differences stay within
  one RGB unit. At149/front their PNG hashes are identical, while the CPU-current
  image differs in 23 pixels by up to 68 RGB units. Mutation: 159754 pixels.

The earlier full-outfit clothing closeup was dark/occluded and is not used as
strong visual evidence. Isolated material views retain actual geometry,
textures and standard PBR/shadows; isolation is solely a test camera setup, not
a production performance shortcut. These thirty sampled views do not establish
unbounded general-material correctness. The candidate remains an explicitly
experimental native performance path until the remaining quality decision and
end-to-end render budget are verified.

Artifacts: `/tmp/loom-gpu-hybrid-tbn-lit-audit`,
`/tmp/loom-gpu-hybrid-body-lit-audit`, and
`/tmp/loom-gpu-hybrid-pants-component-lit-audit`. Boundary JSON and independent
Python verification are preserved separately under
`docs/research/gpu-tangent-boundary`.

### Production-facing resource contract (not activation)

`GpuPoseGeometry::from_current_positions` takes actual CPU final posed vertices;
it allocates no CPU tangent-oracle resource. This preserves existing CPU Skin,
Morph/gizmo/editor/picking results, including all twelve influences. It outputs
unrendered current position/normal/UV intermediates and immutable part ranges
and expanded-corner source indices. `GpuPreparation` owns current validated
canonical/flag/order resources. `GpuRecipeMath` owns complete candidate outputs
and a guard word; `output()` and `guard_buffer()` expose buffers to the renderer.

The persistent dynamic material buffer may receive candidate bytes only after
all pose guards and layouts validate. `gpu_pose_commit` owns that transaction;
CPU current source positions continue to provide AABB/history. Renderer/resource
transport and runtime orchestration are separate commits, not proof of native
activation or 60 FPS. Hardware-ineligible geometry must retain the full-quality
existing CPU/StandardMaterial route.
