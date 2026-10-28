# Experimental precise8 native integration evidence

Not enabled by default. `LOOM_EXPERIMENTAL_GPU_POSES=1` remains the existing fast5 comparison. `LOOM_EXPERIMENTAL_GPU_POSES=precise8` selects the condition-selected F64 pipeline only when the actual shared Device capabilities support it; unsupported devices keep ordinary CPU geometry. No capture/reference switches or diagnostic node/part filters are included in production selection. All such probes are cfg(test).

The source position producer is now the real runtime path: CPU Scene::eval_mesh final output, transformed into world space, uploaded through GpuPoseRuntime. All Skin influences, modifiers and CPU editor/picking geometry remain current. GPU Newell/SG normals and current Mikk arithmetic feed existing atomic whole-pose commit and StandardMaterial storage geometry. Structure compilation contains no CPU tangent values. The default CPU path and fast5 shader source remain unchanged.

Input was the pre-experiment immutable `/tmp/loom-conditioning-capture/input.loom`, SHA256 `84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935`. Historical raw102 input provenance limitations remain unchanged.

## Final module combinations

- Baseline `gpu_mikk_recipe.wgsl` SHA256 `786edcbc6beaaf020df20840eaf24f29decbb9f69e7ac24fbabc46db6b602129`: guard_init, derive, evaluate, epilogue, pack.
- `gpu_mikk_precision.wgsl` SHA256 `daa19bffd2a34be32e2cba02dee47cb62c44c4656082862e7f23c1285ba4cbe7`: risk only.
- `gpu_mikk_refinement.wgsl` SHA256 `7c259f2d87145766ff736219ede938832dade3095f9dfb576691b740251128d8`: refine_group, validate_output.

Those sources are byte-identical to the earlier isolated cone317-tested sources. A shared PrecisionProgram owns the ten-binding layout and eight pipelines once per renderer; each current recipe owns its risk scratch. Precise8 uses eight global stage barriers in the same order. Risk scratch is cleared per frame. Descriptor rebuild and current-structure recovery retain the precision mode. Final nonfinite validation sets guard32 before the unchanged all-or-nothing commit; errors return to full CPU geometry.

## Results

- No-F64 actual Device selection: RED missing implementation, then GREEN; no shader/layout validation error, precise8 absent, fast1 and default selection unchanged.
- Eight production-program synthetic tests GREEN3.72s: overflow recovery, final nonfinite rejection, scales, near-antiparallel boundaries, zero-angle, Any/original owner and degenerate/default behavior.
- Four poses0/39.25/95.71835/149:52 parts /1,335,876 vertices, maxF64 error1.437707381e-6, P/N/UV bytes and CPU history positions preserved, finite tangents and exact handedness.
- Fast5 positive control on same CPU producer correctly trips the unchanged numeric gate: Body39.25 vertex51156 error3.296773465e-5. Expected-panic characterization is separate from precise8 acceptance.
- Renderer4-pose/all13 resource/current-pose integration GREEN2.91s.
- Full317 precise8 independent F64: session2421 GREEN70.76s,4121 parts /105,868,173 vertices, max3.338108500e-6. All output tangents finite, exact handedness, every P/N/UV byte unchanged from current pre-math attributes, positions equal CPU history values. This is correctness, not timing/FPS evidence.
- Whole default Bevy lib suite76 passed/33 ignored before the later low-device-limit regression addition.
- Review found missing10-binding/64-wide Device guards. Actual max_bindings=9 Device reproduced RED. Guards were added before any layout/shader creation; four actual Device low-limit cases (bindings9, size_x63, invocations63, storage8) then passed without validation errors. This host capability preflight change does not change shader source/module/entrypoint math.

`quality-source-sha256.json` hashes the exact source snapshot compiled for full317 (before the later host capability guard extension). Logs are retained separately. No native FPS or final visual acceptance has yet been established for precise8; earlier quiet cone8 cost adds roughly1.4ms GPU span versus fast5, so the native60FPS margin remains a material risk.

Reproduce against current reviewed sources:

```
cargo test -p loom_bevy --lib precision_tests -j4 -- --test-threads=1
cargo test -p loom_bevy --lib precise_selection -j4 -- --test-threads=1
LOOM_PERF_SCENE=/tmp/loom-conditioning-capture/input.loom cargo test -p loom_bevy --release --lib actual_ray_precise_current_cpu_positions_f64_quality -j4 -- --ignored --nocapture --test-threads=1
LOOM_PERF_SCENE=/tmp/loom-conditioning-capture/input.loom LOOM_PRECISE_FULL317=1 cargo test -p loom_bevy --release --lib actual_ray_precise_current_cpu_positions_f64_quality -j4 -- --ignored --nocapture --test-threads=1
```

Final post-review-capability package suite:77 lib +1 bake +7 render tests passed (final-bevy-package.log). The strengthened no-F64 test constructs an actual renderer on a Device without SHADER_F64 and syncs deforming geometry twice: ordinary CPU material buffers remain present, GPU-current false, GPU status absent (no-f64-renderer.log).
