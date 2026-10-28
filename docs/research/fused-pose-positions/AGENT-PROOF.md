# Candidate implementation and correctness evidence

Local candidate commit: `747cb9d`. **Default remains off.** Only `LOOM_EXPERIMENTAL_FUSED_POSE_POSITIONS=1` selects the fused CPU preparation. There is no UI preference or shader/math change, and no performance benefit is claimed yet.

## Ownership and publication

`PreparedPositions` owns reusable Vec4 staging, ordered min/max and a private finite-validity proof. Runtime corner gathering fills it directly from the same current CPU world-baked mesh and the existing immutable corner-source mapping. The order of min/max operations and each `[x,y,z,1]` record matches the original commit loop. Empty gathering has vacuous finite validity, matching the original raw predicate; ordinary target-count validation still applies.

The original raw path remains available, including its finite scan within the original profiling scope. The experimental prepared path keeps exact target count/index/source-range/usage/alignment/alias validation; its finite check is performed while gathering. All candidate layouts and all GPU guards are accepted before any target/history publication. Copy commands are recorded before the corresponding history swap, preserving the original raw operation order.

Successful prepared commit swaps staging into `last_positions` and returns the target's old allocation to runtime staging. It invalidates the consumed proof. A second commit without a fresh gather therefore rejects rather than publishing swapped-out old positions. Production renderer updates/gathers before every commit attempt; guard-recovery retries follow rejected commits, which did not consume proofs. Repeated same-pose renderer calls are explicitly tested. `advance_history` and its queue-write timing are unchanged.

The old raw `candidates()` accessor is now cfg(test) only. Production uses mutable `commit_candidates()` to carry an exclusive prepared proof across the all-guard boundary. Relevant entries in both object/runtime maps are still associated by exact `(hidden,id)`; there is no pointer/hash-only cache substitution.

## Explicitly run gates

Exact commands, environment prefixes, log hashes, input hash and final source hashes are in `agent-command-manifest.json`.

- API RED: missing `PreparedPositions` type before implementation. This is an API-missing TDD result, not a reproduced numeric failure.
- Final atomic/history regressions: **2 passed, 1 ignored**. Covers ±0/order/bounds, nonfinite rejection and recovery, empty validity, invalid later layout, failed later guard, no partial target history publication, consumed-proof rejection, repeated current pose, actual previous-position GPU-buffer readback after history advance, and ordinary CPU-update fallback.
- CPU-helper actual317: **4121 parts / 105,868,173 vertices**, exact Vec4-history and ordered min/max bits. This helper uses converted current part positions; it alone is not runtime-integration proof.
- Final flag-on real renderer actual317: **passed**, with explicit runtime-mode assertion. Times are integers 0..157, 39.25, and half-integers 0.5..157.5. Each time syncs twice, exercising current runtime gather/commit and repeated same-pose publication. All 13 target histories and entity Aabb center/extents match current CPU reference bits; all 13 current GPU-position first words are read back, resource handles stay stable and no final renderer error remains.
- Final flag-on GPU parity: **passed** at `0,0,39.25,39.25,95.71835,149`. Checks complete 64-byte GPU math records against the existing serial-dispatch math path at the same current inputs, alongside current target history/bounds and resource checks. This is not a new independent F64 tangent sweep; production shader sources are unchanged.
- Final full serial Bevy library suites: **88 passed / 38 ignored**, both default and flag-on.
- Natural guard recovery and faceted-descriptor rebuild: flag-on regression passed, including prepared current-position bits. Its old test-only raw-Vec3 length assertion initially failed because the fused representation intentionally uses Vec4 staging; the corrected test checks the actual prepared representation. This was a test-contract adaptation, not a GPU output failure.
- Bake + render integration: **1 + 7 passed** before the final copy-before-swap ordering correction. The final atomic/history, actual317, GPU parity and both library suites were rerun after that correction. Integration log is labeled accordingly, not represented as a final-source rerun.
- `git diff --check` passed.

No full independent F64 rerun or native FPS result is claimed for this host-only candidate. None of its code changes weights, shader dispatch/math, GPU position source packing, normal/tangent/UV records or source topology guards. Correctness tests ran while the parent's separate native reliability test was active, so printed `sync_ms` and test durations are **not quiet performance measurements**.

The immutable input path/hash is recorded in the manifest. That hash applies to these experiments, not retrospectively to historical captures without retained inputs. Root-owned `REVIEW-GATES.md` and `benchmark.py` were left unchanged.
