# Owned material geometry preparation measurement

This is a test-only candidate; production `material_meshes_cached` remains unchanged.

The actual five Ray skins and13material parts were prepared at159poses. Paired constructor timing alternated clone/move order each pose. Producing the owned input copy for the test was outside **both** timed intervals; the candidate moves already-owned position, normal, UV and index vectors into the same Bevy attributes instead of cloning them.

- Existing `MeshData::to_bevy`:0.347ms per whole pose.
- Moving all four vectors:0.007ms per whole pose.
- Saving:0.340ms per pose, substantially less than a1ms estimate.

Every geometry/UV attribute bit and index matches. Running the test-only owned13-material job path across317integer/half-frame poses also matched all2853tangent outputs against the fresh serial corrected reference, with2844neighbor-cache hits and9initial misses. The combined job timing from that correctness run is not a fair native-app performance result.

The microbenchmark is `convert::tests::actual_ray_mesh_data_clone_versus_move`. The existing full-material test selects the owned candidate with `LOOM_OWNED_PREPARATION=1`; `LOOM_SUBFRAME_PARITY=1` extends it to317poses. Both require `LOOM_PERF_SCENE` pointing to the actual saved Ray scene. No public or renderer API was changed, and a consuming production API would additionally need a deliberate bounds path for initial entity creation. This small saving alone cannot establish60FPS.
