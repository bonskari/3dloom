# Read-only source_positions capacity CPU probe

No production code or GPU commands changed. The exact current source_positions function was extracted from loom_bevy/src/gpu_pose_runtime.rs and only renamed in the standalone probe. The alternative checks the same empty case and finite predicate/error string, preallocates Vec::with_capacity(mesh.vertex_count()), and pushes the same [x,y,z,1.0] array in vertex order. It introduces no scratch reuse/cache abstraction.

Actual frozen Ray input `/tmp/loom-conditioning-capture/input.loom`, SHA25684fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935. This is the previously frozen experiment input, not a newly assigned hash for the historical raw102 run. Scene time39;5actual Skin meshes evaluated using Scene::eval_mesh and transformed to current world space, matching the runtime producer. Total63,340unique source vertices.

The other agent's materialID CPU job5731 was terminal before compilation. Standalone direct-rustc compile66168 passed, pureCPU job43716 passed, and quietclear was sent immediately afterward. No queue.write_buffer, Bevy rendering, device creation or GPU timing is present. Native app remains paused; this does not establish a nativeFPS improvement or attribute the complete native source_upload scope to this helper.

| Node | Vertices | Current capacity | Reserved capacity |
|---|---:|---:|---:|
|4543|12,710|16,384|12,710|
|4544|34,271|65,536|34,271|
|4545|3,490|4,096|3,490|
|4546|9,860|16,384|9,860|
|4547|3,009|4,096|3,009|

All output components compare by exact f32 bits, including a signed-zero/f32MAX control. Empty meshes return exactly the same nonempty-source error.9nonfinite cases (NaN,+Inf,-Inf at first/middle/last vertex) return the same nonfinite error. The serialized Scene at frame39 hashes4cf225a47205a3a4c1b1b16e99115d4a416fad4311a413750df4101f0acddd90 both before and after. Serialized Scene SHA is distinct from the frozen .loom file SHA.

64ABBA rounds,16whole-five-mesh calls per timed batch. Timing includes helper allocate/fill/drop; scene parsing/evaluation/world transformation, parity/error checks and SHA serialization are outside timing. black_box prevents eliding input/output work. Mean current0.580339046ms versus reserved0.078915627ms, delta0.501423419ms per five-mesh pose. Capacities are directly observed; allocation counts and the separate contribution of allocator growth versus loop/collect code generation are not instrumented. No claim isolates those causes from this two-function comparison.

main.rs is the complete executable source, original-function.rs the exact extracted function, source-manifest.json records source hashes, args.json/build.py record matched local release dependencies. Run in the same local dependency environment:

```
python3 /tmp/loom-source-position-capacity/build.py
LOOM_PERF_SCENE=/tmp/loom-conditioning-capture/input.loom /tmp/loom-source-position-capacity/probe
```

No production optimization is delivered by this research artifact alone.

## Minimal production-function checkpoint

The production helper now uses the exact benchmarked reserved function body (machine-asserted byte equality after renaming only the function identifier). No scratch state, reuse API, queue changes, source epoch changes or GPU math changes are made. production-function.rs and production-proof.json record it.

Capacity-budget RED37154 reproduced current capacity4 for3 vertices. GREEN78637 verified exact3/17 capacity plus all float bits (signed zero, subnormal, f32MAX), empty message and27nonfinite cases (NaN/+Inf/-Inf ×first/middle/last ×XYZ). Actual CPU-only ignored test50125 passed on5real world-baked Skin meshes atframe39:63,340vertices compare bitwise to the legacy function, and serialized Scene bytes remain exactly unchanged. No GPU job was used. This establishes output/error preservation and the intended capacity budget; it does not separately count allocations or prove native FPS gain.
