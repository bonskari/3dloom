# First native fused-position comparison

Same release binary from747cb9d, verified by actual app ping, source build SHAffbcdd60f5c83f7afea6ab60ec1d753860d7a3466b87490c3c0cdb12564a8e99. Full original Ray scene with four viewports, all5GPU objects/13material parts and all influences retained. CPU edge batching stayed explicitly on in both runs; only LOOM_EXPERIMENTAL_FUSED_POSE_POSITIONS changed. No profiling or concurrent own correctness/build jobs.

| Fused preparation | Realistic ready pose times/s | Minimum active window | Wireframe ready pose times/s | Minimum active window |
| --- | ---: | ---: | ---: | ---: |
| Off |60.8936|56.5733|90.8027|83.3283|
| On |62.3369|60.8396|90.6467|88.2687|

These count distinct ready submitted pose times in moving-clip windows; complete geometry correctness comes from the separate full history/bounds and GPU-byte gates. The first on run exceeds60 in every accepted active one-second window in both modes. One ordered pair does not establish repeatability or long-run stability of this candidate. Default promotion and final performance acceptance remain pending.

All four frame39 viewport crops were exact in RGB off/on. Both runs restored all original node records/frame/selection and unchanged source bytes. See native-pixel-parity.json, native-*-1.jsonl and native-*-frame39.png. The earlier15-minute reliability proof applies to CPU-edge batching on the prior binary, not retrospectively to this new fused preparation.

The running app currently uses both explicit experimental flags and is paused at frame39; this is not a default-on production claim. No public push was performed.
