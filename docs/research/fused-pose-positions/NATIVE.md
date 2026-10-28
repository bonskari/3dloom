# First native fused-position comparison

Same release binary from747cb9d, verified by actual app ping, source build SHAffbcdd60f5c83f7afea6ab60ec1d753860d7a3466b87490c3c0cdb12564a8e99. Full original Ray scene with four viewports, all5GPU objects/13material parts and all influences retained. CPU edge batching stayed explicitly on in both runs; only LOOM_EXPERIMENTAL_FUSED_POSE_POSITIONS changed. No profiling or concurrent own correctness/build jobs.

| Fused preparation | Realistic ready pose times/s | Minimum active window | Wireframe ready pose times/s | Minimum active window |
| --- | ---: | ---: | ---: | ---: |
| Off |60.8936|56.5733|90.8027|83.3283|
| On |62.3369|60.8396|90.6467|88.2687|

These count distinct ready submitted pose times in moving-clip windows; complete geometry correctness comes from the separate full history/bounds and GPU-byte gates. The first on run exceeds60 in every accepted active one-second window in both modes. One ordered pair does not establish repeatability or long-run stability of this candidate. Default promotion and final performance acceptance remain pending.

All four frame39 viewport crops were exact in RGB off/on. Both runs restored all original node records/frame/selection and unchanged source bytes. See native-pixel-parity.json, native-*-1.jsonl and native-*-frame39.png. The earlier15-minute reliability proof applies to CPU-edge batching on the prior binary, not retrospectively to this new fused preparation.

The running app currently uses both explicit experimental flags and is paused at frame39; this is not a default-on production claim. No public push was performed.

## Reversed repeated pair

Second fresh-process order was on then off, same binary and edge batching on. Exact results:

```json
{
  "off": [
    {
      "kind": "summary",
      "shading": "Realistic",
      "active_windows": 17,
      "active_ready_pose_fps": 62.38153039895674,
      "min_active_window_fps": 59.22422560036437
    },
    {
      "kind": "summary",
      "shading": "Wireframe",
      "active_windows": 18,
      "active_ready_pose_fps": 91.94210586750562,
      "min_active_window_fps": 87.73945776464909
    }
  ],
  "on": [
    {
      "kind": "summary",
      "shading": "Realistic",
      "active_windows": 17,
      "active_ready_pose_fps": 63.361048535737915,
      "min_active_window_fps": 61.01382758661878
    },
    {
      "kind": "summary",
      "shading": "Wireframe",
      "active_windows": 18,
      "active_ready_pose_fps": 91.83412618959554,
      "min_active_window_fps": 83.08009136764969
    }
  ]
}
```

Both experimental-on runs exceeded60 in all accepted active windows in both modes. Both preserved node/selection/frame/source data. The repeated pair and first pair show the Realistic gain in the same direction; long-run fused stability and default-unset acceptance are still pending.

## Fused fifteen-minute reliability gate

The747cb9d binary with both flags1 completed30×30-second samples without pose errors/recoveries, all5GPU objects/13material parts present. All node records, undo history and source bytes were restored. A real271346304-byte auto-backup completed during playback and its scene JSON exactly matched the source. Concurrent default-promotion correctness builds/tests were allowed: observed rates are not a quiet performance acceptance result. This validates the same fused/batched algorithms whose initialization default is changed in f8bd88d; native default-unset acceptance follows separately.
