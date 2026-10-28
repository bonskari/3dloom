# Normal-startup precise geometry and source-buffer preflight

This checkpoint changes only gpu_pose_renderer.rs selection/tests and gpu_pose_runtime.rs source-buffer preflight/test. No shader, GPU math/dispatch, UI, source-position packing, Skin weights, material, queue upload or environment API changes are made. unchanged-shader-sha256.json machine-checks every Bevy WGSL file against the preceding commit.

## Selection policy

Missing LOOM_EXPERIMENTAL_GPU_POSES now selects the existing precise8 current-geometry program when the actual shared Device meets its existing SHADER_F64/layout/compute limits. The existing literal precise8 remains equivalent. Missing feature/limits returns the same full-quality CPU backend before precision shader creation. Literal1 preserves fast5 comparison; literal0, empty and all other values keep CPU selection. OsStr comparison distinguishes a missing variable from an unrecognized/non-UTF8 value.

The default-supported Device test first failed (62102): selection returnedNone. It now requires Some with precisionSome, explicit1 with precisionNone and0/other with no GPU backend. Unsupported Device tests assert missing-variable CPU fallback inside validation-error scopes, both without requested SHADER_F64 and on4actual low-limit Devices (bindings9, workgroup_x63, invocations63, storage8). These return without GPU validation errors. Existing no-F64 renderer CPU-fallback output and lifecycle tests also pass.

This establishes selection and regression behavior, not the subsequent normal-startup nativeFPS or long stability result; those remain root's next release validation. The precise shader/module combination itself is unchanged from the prior accepted F64/byte audits.

## Actual source allocation gap and minimum fix

Runtime::with_precision previously called create_buffer_init for all current CPU source positions before the downstream geometry storage checks. An actual Device with max_buffer_size256 and max_storage_buffer_binding_size256, plus17valid source vertices (272bytes), reproduced a panic in Buffer::get_mapped_range because the labeled source buffer was invalid (source-limit-red.log). This is a reproduced validation ordering failure, not a hypothetical diagnosis.

The runtime now computes checked u64 source byte count after the same source_positions validation, compares it to actual Device max_buffer_size AND max_storage_buffer_binding_size, and returns an error before create_buffer_init. The existing empty/nonfinite ordering is retained. No generic abstraction, extra state or source queue changes are introduced. The test separately exercises a max-buffer256 Device and a storage-binding256 Device, requiring the precise source-limit error and an empty GPU validation scope. No huge allocation is needed. The existing topology/part/whole-pose checks still run after this preflight on eligible inputs.

## Verification

Whole Bevy package session18539 GREEN:85lib +1bake +7render,0fail. The source-limit and new default/fallback cases are included. Commands:

```
cargo test -p loom_bevy --lib default_selection_on_supported_device_uses_precise_geometry -j4 -- --nocapture --test-threads=1
cargo test -p loom_bevy --lib source_gpu_buffer_limits_reject_before_resource_creation -j4 -- --nocapture --test-threads=1
cargo test -p loom_bevy -j4 -- --test-threads=1
```

No native timing, new numerical approximation, quality reduction or fallback-to-fast behavior is inferred by this checkpoint. Unsupported precise geometry continues through ordinary CPU geometry.
