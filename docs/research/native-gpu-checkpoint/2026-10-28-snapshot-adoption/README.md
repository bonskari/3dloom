# Retain equal reloaded topology in GPU caches

Binary reports8b5e2fd, plus the two production-file edits frozen in source-sha.json. No geometry/material/skin-influence reduction. Normal default GPU precision, no profile/experimental environment flags during throughput runs.

Fresh: Realistic62.1591892 ready poses/s (minimum one-second window60.6084606), Wireframe95.6283553 (minimum85.7584505). Same process after only Open File: Realistic60.0582819 (minimum56.3383545), Wireframe95.1803153 (minimum91.8578301). Every node record and source-file SHA restored unchanged.

The persistent deep-check cost is removed in the actual profiled actor: after reload,1490checks total0.154ms descriptor-half-edge /0.059ms edge-half-edge, versus prior unfixed1400checks146.063/145.061ms. Whole descriptor average0.062ms after fix. Profile is diagnostic, separate from throughput acceptance.

The bounded average exceeds60 in both modes after reload, but Realistic has little margin and one-second windows below60. Do not claim stable60FPS or goal completion. Longer post-fix stability/editing verification remains. The named functional tests verify guard semantics, not Arc identity by themselves; the runtime predicate profile independently verifies the removed repeated cost.

## Final correctness verification

Serial lib regressions:86Bevy passed/36ignored,35Render passed/0ignored. The separately enabled actual-Ray descriptor oracle passed317poses/1585checks, including fractional frames. All four frame39viewport crops are pixel-identical to the accurate pre-fix default version (image-parity.json).

The first parallel full-suite invocation was interrupted after thread stacks showed many concurrent Vulkan device/instance calls waiting inside Vulkan/NVIDIA driver locks; it is not recorded as passing, nor does this prove why the driver stalled. The same full lib suites completed when invoked with --test-threads=1. No production rendering backend was altered for these tests.
