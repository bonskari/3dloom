# Production default selection — native acceptance pending

Both existing internal experimental flags now select the proven path when unset; explicit1 still enables and0 retains the previous comparison path. There is no new user preference or model change. The running long reliability test still uses the previous747cb9d binary with both flags1; it is not interrupted by this source change.

Dedicated unset-flag tests first failed on the existing default-off policy, then passed after the selection change. Full UI434passed/2ignored; full Bevy89passed/38ignored. The actual six-sample Ray renderer/math-byte/history/bounds/resource test passed with both overrides unset, and asserts the runtime uses fused preparation. Test-only mode expectation was updated to the default-on contract. Source/log data are retained here; no native application default-unset FPS or delivery claim is made yet.

Commands: unset both LOOM_EXPERIMENTAL_CPU_EDGE_BATCH and LOOM_EXPERIMENTAL_FUSED_POSE_POSITIONS; cargo test -p loom_ui --lib -j4 -- --test-threads=1; cargo test --release -p loom_bevy --lib -j4 -- --test-threads=1. The real-Ray test uses LOOM_PERF_SCENE=/tmp/loom-conditioning-capture/input.loom and cargo test --release -p loom_bevy --lib actual_ray_precise_all_thirteen_parts_keep_current_pose_and_resources -j4 -- --ignored --nocapture --test-threads=1.
