# Measured current-attribute boundary cases

These are actual posed Ray triangle groups from the full material diagnostic, not generated example data. Files preserve the represented positions, UVs, normal, ordered group members and CPU/GPU output for independent review. Run `python3 verify_group_f64.py <case.json>` from this folder. The script recomputes the selected group in f64; it is diagnostic evidence, not a complete renderer or full-vertex acceptance test.

The original CPU comparison gate remains failed. Some robust GPU results are substantially closer to the independent reference than the CPU result: exact current group selection at the fixed 180-degree boundary is sensitive to f32 normalization rounding. Full-vertex reference tests, material image comparisons and native throughput still determine production readiness. No GPU production route is enabled by these fixtures.
