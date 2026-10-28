# Same-binary Open File regression

Installed commit 0d8a4c38baebb5b36d923d654361c8dc3a925968, normal default precision, no experimental/profile flags. Fresh PID3223827 loaded the unchanged original Ray scene. The runner measured active animation windows in Realistic and Wireframe, then restored frame39 and verified complete node records/source-file SHA unchanged.

Fresh: Realistic61.4402423 poses/s (minimum one-second window60.5882803), Wireframe93.6848076 (minimum89.5542617).

Same process, only Open File of the same source, without selection edits: Realistic54.5915927 (minimum49.1761397), Wireframe90.5968262 (minimum88.2538406).

This reproduces a reload-associated slowdown without selection changes. It does not yet prove which renderer cache or operation causes it. The earlier retained-half-edge CPU probe establishes an expensive possible path, not this actor's runtime identity. Next step is profiling the exact half-edge predicates in both caches before/after reload.

Counts are distinct ready submitted pose times, not independent per-frame vertex fingerprints; separate geometry correctness gates remain necessary. Full scene geometry/materials/skin influences were retained. No claim of stable60FPS acceptance. No source scene writes.

## Runtime predicate evidence

A diagnostic binary built from0d8a4c3 plus exactly the two Scope wrappers committed in56e5c93 was launched with LOOM_PROFILE_FRAMES=1 (PID3231136). This is a diagnostic run, not FPS acceptance. The same actor was played fresh, stopped/frame39, then Open File of the unchanged source and played again.

Fresh steady block:1510descriptor checks, half-edge total0.147ms, edge half-edge total0.072ms; whole descriptor average0.060ms, ui.frame14.129ms. After reload steady block:1400checks, descriptor HE total146.063ms/average0.104ms and edge HE total145.061ms/average0.104ms; whole descriptor average0.166ms, ui.frame14.857ms. Five checks of each type per ready pose add about1.04ms of repeated HE checking. The archived profile measures complete predicates, not a separately instrumented Arc fallback counter. Together with the exact source/CPU allocation probe, this identifies persistent old-snapshot comparison work after reload. It need not explain every throughput difference.

## Prior15minute reliability run

pre-fix-stability.jsonl preserves30×30second samples and actual auto-backup scene-equality proof. All samples retained5objects/13parts with no pose error or recovery; all source scene bytes and node records were restored. It does NOT pass the performance gate: throughput stayed54.4–56.7poses/s after reload. The first17samples preceded the separate CPU snapshot probe. Later samples are not quiet acceptance measurements.
