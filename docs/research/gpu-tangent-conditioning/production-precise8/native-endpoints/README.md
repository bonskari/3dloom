# Bounded native timeline endpoint coverage

The native timeline extends to157 whereas the earlier317 quality audit ended at149.0. This standalone scratch probe closes the identified sampled endpoint coverage gap; it does not claim every intermediate timestamp or all animated editor nodes are numerically audited.

Scratch copied the exact production Bevy sources at80eb3f96fb39d0e53a34b21e2e70e63adcfb59d7. Only the ignored test name and sample-time list changed; scratch-test.patch is the complete difference. No production source was edited. All3 production shader bytes, module/entrypoint combinations, actual CPU final posed-position producer, current structural guards, precision arithmetic and atomic runtime behavior remain unchanged.

Input is the pre-experiment immutable `/tmp/loom-conditioning-capture/input.loom`, SHA25684fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935. Nominal samples149.000015/150/153.5/157; actual first f32 is149.00001525878906 (logged149.00002). source-manifest.json records every actual source hash and time bits. This hash does not relabel the old historical raw102 input.

Build session75930 terminated successfully. One GPU correctness job session70797 terminated GREEN:4 poses ×13 parts =52 parts /1,335,876 expanded vertices; maximum tangent-vector error1.4115617414233096e-6 against independently recomputed current-attribute F64. Every tangent is finite, handedness matches exactly, P/N/UV bytes equal pre-math current GPU attributes and positions equal current CPU history. No assumption is made that post149 animation is constant. This specifically covers the5 Skin-producing mesh objects and their13 material parts; it does not separately validate the animated pole4988's editor representation or all scene nodes.

The root app release build was active on CPU. Elapsed3.42s is merely terminal runtime, not a performance/FPS result. No other GPU probe ran concurrently from this agent.

Reproduction (requires matching local release fingerprint dependencies referenced by build.py): copy the frozen production Bevy src tree to `/tmp/loom-precision-native-endpoints/src`, apply scratch-test.patch there, run `python3 /tmp/loom-precision-native-endpoints/build.py`, then:

```
LOOM_PERF_SCENE=/tmp/loom-conditioning-capture/input.loom /tmp/loom-precision-native-endpoints/tests actual_ray_precise_native_endpoint_cpu_positions_f64_quality --ignored --nocapture --test-threads=1
```

No full317 sweep or production-default change was performed for this bounded task.
