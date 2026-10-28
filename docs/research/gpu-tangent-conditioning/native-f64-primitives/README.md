# Native Float64 primitive gate

Isolated correctness probes only. No production feature selection or math was
changed. The current native application's Device was not queried directly:
the headless probe used Vulkan on NVIDIA RTX 5080, vendor 4318/device 11266,
driver 610.57.04. The adapter advertised SHADER_F64; the probe requested ONLY that
feature and verified it on its own created Device. Integration must separately
check its actual shared Device, preserving the ordinary backend if unsupported.

## Basic operations and storage

The first shader validated, executed and read back native f64 add/multiply/divide/
sqrt results exactly matching the CPU f64 expectations in this small test.
Wide addition retained `1+approximately1e-8`; multiplication retained the
`1-2^-46` residual, and an array of vec4<f64> had the expected 32-byte stride.
See `crash.log` for the completed BASIC_PASS before the subsequent failure.
This small test establishes local capability, not every IEEE operation/range.

## Native atan2 failure: do not rerun

The same process then attempted a separate native f64 atan2 shader and terminated
with SIGSEGV (exit 139). `crash-stack.txt` records the fault in
libnvidia-gpucomp.so.610.57.04, followed by NVIDIA SPIR-V/compiler frames. The
specific invalid instruction or underlying defect has not been established.
This was the isolated probe process, not the running editor.

`crash-reproducer.rs` preserves the actual source for evidence. It contains the
crashing case and must NOT be run as the normal capability test. No native f64
atan2 test was repeated. The design instead uses explicit current-angle math
built from successfully tested basic operations, with its own precision gate.

## Range-reduced angle polynomial

`angle.wgsl` implements the geometric upper-half-plane angle with
`y=abs(cross_length)` and `x=dot`, so result lies in[0,pi]. It handles signed
zeros and returns pi/2 when both operands are zero, matching the tangent
reference's zero-projected-edge convention. Native atan2 is not called.

First form `r=min(abs(x),y)/max(abs(x),y)` using branches to avoid evaluating an
unused zero denominator. For r>tan(pi/8), reduce with
`z=(r-1)/(r+1)` and add pi/4. Otherwise z=r. Restore complementary angle/sign
quadrants afterward. In real arithmetic, abs(z)<=tan(pi/8).

The degree 41 odd alternating atan series has real-arithmetic truncation bound
`tan(pi/8)^43/43 =8.075642313e-19`. Positive coefficients and Horner subtraction
avoid Naga 24's unsupported constant unary-negation case for f64 literals.
Explicit `lf` literals prevent accidental f32 rounding of constants.
Floating-point coefficient/range-reduction errors are additional to truncation;
the measured native gate includes them. No error-free two-sum/FMA assumption is
used. WGSL does not guarantee the rounding events required by portable
double-double transforms: its [floating-point rules](https://www.w3.org/TR/WGSL/#floating-point-evaluation)
permit reassociation, and fma accuracy is inherited from multiply/add.

Actual GPU angle gate: **41,219 cases**, dense grids, deterministic random inputs,
zero/signed-zero, near-collinear/endpoints and reduction boundaries. Maximum
absolute error against CPU f64 atan2: **4.4408920985e-16**; required <=4e-15.
See `angle.rs`, `angle.log`. Input/reference angles are test data; a future
production group solver must consume real current mesh attributes instead.

The Rust test was compiled with optimized rustc against the existing cached
wgpu 24/pollster/bytemuck dependencies using the same host-specific script as
other standalone probes. It did not build or modify workspace artifacts.
The host paths in source are archival, not a portable Cargo test harness.

A condition-selected group kernel may be prepared next, but passing these
primitive tests is not a whole-tangent accuracy gate or an end-to-end cost proof.
