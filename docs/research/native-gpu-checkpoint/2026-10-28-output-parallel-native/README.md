# Native output-parallel precision and cached axes

Binary `09a93cd`, explicit precise8, profiling disabled during the ready-pose benchmark: Realistic **59.4108** ready pose times/s, Wireframe **95.3611**. Minimum retained one-second windows were 56.5135 and 90.4803 respectively. Realistic still misses the 60 fps requirement; no acceptance claim is made.

The exact benchmark runner and active-clip-window policy are in the adjacent `2026-10-28-precise8-first-native` directory. All five objects and thirteen parts remained committed without GPU errors or recovery. Frame 39, selection, all 461 node records and source scene SHA were restored unchanged. Four viewport crops were pixel-identical to the previous accurate `80eb3f9` binary.

The separate Realistic profiles include profiling overhead and are diagnostic only. `realistic-profile.log` used `09a93cd`; `material-ids-profile.log` used `d86fac2`, which only added a scope around existing ID collection. Startup and idle report blocks remain in these filtered logs and must be excluded from steady-play comparisons. Typical active GPU-pose renderer scope was about 4.05 ms/frame. Material-ID collection totaled about 0.136 ms/frame across five branches; this is not the entire renderer cost.

Raw local preferences and unrelated journal messages are deliberately omitted. All changes and evidence remain local, without a push.
