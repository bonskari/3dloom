# Fused current-position preparation — pending verification

This experiment must preserve the current production result, not substitute reduced geometry or weaker guards. Default remains off until actual native comparison.

Required evidence before delivery:

- Same exact corner-source order and all thirteen material parts on the real Ray scene, with all original geometry and skin influences.
- Prepared positions/history and min/max bounds have identical f32 bits, including signed zero, on317 integer/fractional poses. A helper-only test over already-expanded MeshData is useful but does not prove runtime corner-source gathering or commit integration.
- All layout, source usage/range/alignment, cross-target alias and whole-pose GPU guards remain. A bad later part must leave all target history and geometry uncommitted; valid retry must work.
- Invalid/nonfinite preparation cannot reuse a previous proof. Empty-input semantics must match the raw validation path where applicable. Consumed staging cannot silently publish swapped-out old positions on a repeated commit.
- Actual renderer update/commit lifecycle handles repeated same-time poses; CPU fallback, rebuilt topology/material parts and frame-history advancement remain correct.
- Full existing GPU output parity must execute the experimental path explicitly, not only the default-off path. The317-pose helper result alone cannot support this claim.
- Compare unprofiled release builds in the same complete native scene; profile-scope reductions alone do not prove a frame-rate gain. Preserve exact four-view frame39 pixel parity, original node records and source SHA.

The separate CPU-edge batching reliability run may overlap development correctness builds, so its observed frame-rate fields are not quiet performance acceptance evidence.
