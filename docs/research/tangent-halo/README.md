# Rejected MikkTSpace halo prototype

This is a research snapshot, not compiled or called by the renderer. The production path continues to calculate the original MikkTSpace tangents. Do not wire this prototype into production without resolving its actual posed-scene parity failure.

The experiment partitions triangle outputs across the existing bounded task pool. Each partition includes the complete current position/normal/UV weld fans of its target corners. Zero-UV and geometric-degenerate triangles extend the halo through all three vertex fans. Included faces retain their original global order. Weld keys canonicalize signed zero and reject nonfinite attributes.

The initial conservative version rejected Ray's Face skin because it contains seven zero-UV-area triangles. Extending the dependency closure made frame 0 byte-identical, but frame 39.25 still differs at 14 tangent vertices. The differences change tangent directions materially; they are not just floating-point rounding. The exact remaining dependency or ordering cause has not been verified.

## Evidence

The actual saved scene was `/mnt/2tb/hollowreach-ue/Hollowreach/SourceArt/RayLoom20261025/RayAnimationTest_LoomRig.loom`, with frame range 0–157. Tests evaluated the scene's skin deformation and object matrices, and compared every tangent component's `f32::to_bits()` result with Bevy's unchanged complete-mesh `generate_tangents()`.

| Mesh | Pose | Triangles | Different tangent vertices |
| --- | ---: | ---: | ---: |
| Body001, material 0 | 0 | 23,560 | 0 |
| Face001, material 0 | 0 | 47,218 | 0 |
| Body001, material 0 | 39.25 | 23,560 | 0 |
| Face001, material 0 | 39.25 | 47,218 | 14 |

The test stops at the first failed pose, so later planned poses were not validated. `RESULTS.txt` preserves diagnostic timings from the unoptimized debug build; these are not release-performance or application-FPS measurements.

## Reproduction

Copy `tangent_halo.rs` to `crates/loom_bevy/src/convert/tangent_halo.rs`, add a test-only `mod tangent_halo;` declaration under `#[cfg(test)]` in `convert.rs`, and add the workspace `serde_json` dev dependency to `loom_bevy`.

Run the synthetic parity/degenerate test with:

```sh
cargo test -p loom_bevy --lib complete_weld_halos -- --nocapture
```

Run the actual posed-scene comparison with:

```sh
LOOM_PERF_SCENE=/mnt/2tb/hollowreach-ue/Hollowreach/SourceArt/RayLoom20261025/RayAnimationTest_LoomRig.loom \
  cargo test -p loom_bevy --lib actual_ray_pose_tangents -- --ignored --nocapture
```

The latter must currently fail at Face001 frame 39.25. Do not relax the equality check or substitute cached rest-pose tangents to make it pass.
