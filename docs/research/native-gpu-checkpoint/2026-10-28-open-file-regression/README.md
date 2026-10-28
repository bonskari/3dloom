# Same-binary Open File regression

Installed commit 0d8a4c38baebb5b36d923d654361c8dc3a925968, normal default precision, no experimental/profile flags. Fresh PID3223827 loaded the unchanged original Ray scene. The runner measured active animation windows in Realistic and Wireframe, then restored frame39 and verified complete node records/source-file SHA unchanged.

Fresh: Realistic61.4402423 poses/s (minimum one-second window60.5882803), Wireframe93.6848076 (minimum89.5542617).

Same process, only Open File of the same source, without selection edits: Realistic54.5915927 (minimum49.1761397), Wireframe90.5968262 (minimum88.2538406).

This reproduces a reload-associated slowdown without selection changes. It does not yet prove which renderer cache or operation causes it. The earlier retained-half-edge CPU probe establishes an expensive possible path, not this actor's runtime identity. Next step is profiling the exact half-edge predicates in both caches before/after reload.

Counts are distinct ready submitted pose times, not independent per-frame vertex fingerprints; separate geometry correctness gates remain necessary. Full scene geometry/materials/skin influences were retained. No claim of stable60FPS acceptance. No source scene writes.
