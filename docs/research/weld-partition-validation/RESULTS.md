# Test-only exact previous-weld partition validation

Production welding remains unchanged. The experiment is behind `cfg(test)` and explicitly enabled only on test cache objects (`LOOM_VALIDATE_WELD=1` for the actual-material probe).

A previous partition is reused only when indexed topology and all attribute dimensions match, UV bytes are unchanged, and the mesh is nondegenerate and manifold in the welded graph. Every current position/normal/UV tuple must match its cached-class representative **bit for bit**. All representatives must be finite. Distinct classes must remain distinct under float equality, with positive/negative zero canonicalized; static UV buckets restrict those comparisons without a global per-pose hash. Any failed proof runs the complete current upstream weld. Saved indices are only encoded representative references, not saved pose attributes or tangents. Triangle derivatives, current projected vectors, group construction, subgroup classification and angle-weighted tangent math still run on the current pose.

The callback's owned state uses a thread-local **stack**, including disabled sentinels, because nested existing-task-pool scopes can poll another material job on the same thread. No raw pointer crosses a worker boundary. Scope cleanup drops abandoned state during unwinding.

Synthetic tests cover changed normal equality (split), newly equal classes (including ±0 across classes), mixed member zero bits, changed UVs/topology, nonfinite normals, current deformed values, and nonmanifold fresh fallback.

The actual five skins/13parts across317integer/half-frame poses passed all2853tangent bit comparisons against fresh serial corrected preparation, plus position/normal/UV bits and indices. Result:2844validated weld hits,9initial misses,0member-bit/merge/nonfinite rejections on this Ray dataset (99.68% including cold starts).

A separate quiet, alternating-order159-pose pair compared the current full-weld path against validation:

| Part | Full weld | Validated partition |
|---|---:|---:|
|Body001/0|5.216ms|4.198ms|
|Face001/0|11.815ms|9.805ms|
|Combined|17.031ms|14.003ms|

Net saving is about3.03ms/17.8% in this isolated four-worker comparison; it includes the validation cost. It is not a native application FPS claim. Post-group metadata caching has **not** been added to this experiment yet. Production adoption requires review and the remaining requested checks; no environment variable enables this path in a normal build.

## Production promotion (27 October 2026)

After independent guard/TLS review, commit `51799c6` enables the same validated-weld path for production renderer caches. The opening test-only status above describes the original experiment. Production has no runtime enable switch: failed proofs always use fresh current-attribute welding. Group preparation and all current tangent math remain unchanged. The normal library and test library passed 51 unit tests, one GPU bake test and seven GPU render tests (22 opt-in tests ignored). Native playback performance for this combined build has not yet been measured.

The production-enabled release test binary also passed the full317-pose/all13-material sweep against fresh serial corrected preparation: all2853 tangent comparisons and all position/normal/UV bits and indices matched. There were2844 validated hits,9 cold misses and0 member-bit/merge/nonfinite rejections. This correctness run overlapped application compilation, so its elapsed time is not a performance measurement.
