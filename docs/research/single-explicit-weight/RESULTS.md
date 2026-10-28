# Rejected single explicit-influence preparation shortcut

Isolated CPU-only experiment; production source and installed app were not modified. No GPU commands were issued. This exact candidate is rejected because the complete measured batch regressed. No further optimization was attempted.

The immutable input was `/tmp/loom-conditioning-capture/input.loom`, SHA-256 `84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935` (verified after execution). Original historical audits are not assigned this hash by this experiment.

## Candidate and existing contract

Production already reuses one weight scratch Vec per Skin application. It has no single-row special case. The candidate changes only scratch preparation for an explicit raw row of length one: reject invalid bone/weight exactly as before, compute the same f32 sum and weight/sum division, preserve rigid selection, and return. It does **not** replace the weight with 1, bypass any blend operations, alter matrices, or cache weights. The original blend loop, missing-world-matrix handling, relative residual, fast mode, patch-owner routing and gizmos remain unchanged. An isolated atomic selector chooses original/candidate preparation on each single-row query; both measured routes pay that selector branch. This is an experiment, not a proposed production API.

Actual input row prevalence:

| Skin | Single rows | Total vertices |
|---|---:|---:|
| Body001 | 1136 | 12710 |
| Face001 | 7353 | 34271 |
| SkeletalMesh001 | 316 | 3490 |
| SkeletalMesh002 | 0 | 9860 |
| SkeletalMesh003 | 0 | 3009 |
| Total | 8805 | 63340 |

## Correctness

`single_exact_flags` passed: original/candidate weight bits across explicit/envelope, rigid/all, rigid handles/all patch handles, fast strongest selection, relative envelopes, limits 0/1/20, valid/invalid bone indices, ±0, negative/threshold-adjacent weights, f32::MAX, Infinity and NaN. Patch owners query either owner vertex. This synthetic checks prepared weights; it does not independently exercise every complete patch/missing-world output. Those downstream operations are unchanged in the source patch.

Actual 317 times: integers 0..157, 39.25, and half-integers 0.5..157.5. All five Skin outputs matched original batched and serial evaluation: complete Mesh/StackValue metadata, vertex position/color/illumination/alpha bits, map-value bits, warnings and selection. Editor cages, serialized modifier weights and Scene serialization remained unchanged. This includes actual current pose matrices, with all existing influences retained.

## Quiet complete-batch result

The parent kept the native app paused and held heavy jobs. A bounded four-worker pool ran four independently cached Scenes in ABBA / BAAB order per pose: two original and two candidate complete five-Skin batches. Setup, comparison and serialization are outside the measured batch calls. 634 measurements per route were averaged; cold work is included equally.

| Route | ms per complete five-Skin batch |
|---|---:|
| Original + isolated selector | 2.642854 |
| Single-row candidate + isolated selector | 3.059294 |
| Candidate regression | +0.416440 |

Whole test finished in 9.91 seconds. The cause of the regression is not established; this does not prove every possible single-row implementation is slower. It is sufficient to reject this candidate. These helper/batch timings are not native FPS measurements.

Live handles: final compile 52868 terminal success (two unused scratch timing-variable warnings); synthetic unit terminal success; complete ABBA/parity 27966 terminal success. Earlier harness import failures were corrected only in scratch and are not correctness results.

## Artifacts and reproduction

`prototype.patch` preserves exact candidate changes plus synthetic test and scene test-module registration; `probe.rs` preserves the final ABBA harness. `manifest.json` records source/input/artifact hashes, and `args.json` records the executed direct-rustc arguments. `result.log` and `synthetic.log` are terminal outputs. The build helper requires the existing exact release dependency fingerprints and fails on source/dependency mismatches rather than substituting libraries.

```sh
python3 docs/research/single-explicit-weight/compile_probe.py
target/single-explicit-weight/tests single_exact_flags --nocapture --test-threads=1
LOOM_PERF_SCENE=/tmp/loom-conditioning-capture/input.loom target/single-explicit-weight/tests actual_single_weight_317 --nocapture --test-threads=1
```

No production change, commit or push was made by this experiment.
