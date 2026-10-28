# Minimal production integration checkpoint (experimental only)

The same tested output-parallel shader/module/entrypoint combination is integrated into precise8. CPU-default and `LOOM_EXPERIMENTAL_GPU_POSES=1` fast5 comparison remain unchanged; no UI/app/lib/renderer/env selection changes are made. Only6 Bevy source files change: gpu_mikk_recipe.rs, gpu_mikk_precision.rs, gpu_mikk_refinement.wgsl, new cfg(test)-only gpu_mikk_refinement_serial_test.wgsl, gpu_mikk_precision_tests.rs, gpu_pose_runtime.rs (test-only diff).

Metadata/owner bitmap is built only for parallel precise8. Fast5 and the cfg(test) serial reference do not allocate writer bitmaps or append any rows/header; recipe1.w remains0. Across22 synthetic cases the fast recipe words equal exactly the pre-table prefix and Params bytes equal the same prefix configuration with recipe1.w zeroed. The original5pass source and dispatch remain untouched.

The serial source/factory exists only under cfg(test); no capture/reference mode is shipped. Existing SHADER_F64/nine-storage/ten-binding/64wide feature checks, per-part storage and corner dispatch caps and whole-pose guard/atomic commit remain intact. Appended rows have checked offset/count/u32-end and unique original-corner writers; all inner members and per-output F64 operation order remain unchanged.

## Exact source proof

Baseline WGSL786edcbc6beaaf020df20840eaf24f29decbb9f69e7ac24fbabc46db6b602129 and risk WGSLdaa19bffd2a34be32e2cba02dee47cb62c44c4656082862e7f23c1285ba4cbe7 remain byte-identical. Parallel refinement bbfe876303abcf9b0ebcc3305489b97152c35248f05b54f7037b0f4d87e7ebcc matches the approved scratch exactly. Serial test source7c259f2d87145766ff736219ede938832dade3095f9dfb576691b740251128d8 is the old production source. Exact pre-refine functions, single-output F64 body and post-refine validation tail equality are machine-asserted in source-byteproof.json.

## Final quality and regressions

The real current CPU final world-position producer feeds every test. Immutable source input remains SHA25684fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935; no historical input provenance is relabeled.

- Four critical poses0/39.25/95.71835/149:52parts /1,335,876vertices, maxF64 error1.437707381266227e-6.
- Final production-config317 session38408 GREEN75.43s:4121parts /105,868,173vertices, max3.338108500276807e-6.
- Same final binary endpoint session51934 GREEN3.08s:149.00001525878906/150/153.5/157,52parts /1,335,876vertices, max1.4115617414233096e-6.
- Every complete64B output record and every risk flag equals serial refinement for every part. P/N/UV bytes, CPU history positions, finite output and handedness are verified against the same current attributes and independent recomputed F64.
- Final binary10synthetic/ownership/fast-no-metadata tests session7715 GREEN2.96s. Valid duplicate triangles/Any/mixed degenerate/shared-vertex ownership and malformed duplicate destinations remain covered.
- Full Bevy package79lib +1bake +7render passed, including existing no-F64/low-limit Device fallback and renderer lifecycle tests.

One cfg(test)-only regression was concretely reproduced while running the old fast5 positive control: the new comparison harness read a missing fast5 risk buffer and failed with the wrong panic. The test helper now returns an empty risk vector for fast5, which has no precision-risk buffer. The positive control then passed for its intended numeric failure (Body39.25 vertex51156 error3.296773465e-5). This is not a production math or shader change. Package tests preceded that narrow helper fix; the final10synthetic tests, fast5 positive control and full317/endpoints followed it.

The private PrecisionProgram factory takes the baked module source and dispatch kind only to share construction with cfg(test) serial reference; the only production constructor selects the parallel shader. Module layout and entrypoint routing remain baseline guard/derive/evaluate/epilogue/pack, cone risk, parallel-refinement refine_group+validate_output. No production oracle or captured numeric terms are supplied.

`source.tar` and source-sha256.json retain the exact final compiled snapshot; all6 changed production files were byte-compared to this snapshot before archiving. Local direct build depends on the matching release fingerprints/rlibs in args.json/build.py. The quiet cost result in the parent folder is from the isolated prototype; its serial variant still carried unused row metadata. Production deliberately omits that metadata on serial/fast paths. Final native FPS is not inferred from that cost and still requires the combined release/native benchmark.
