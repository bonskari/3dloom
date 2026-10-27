# Prepared MikkTSpace group evaluation

`generated.rs` is derived from `bevy_mikktspace` 0.16.1's generated implementation. Its original Morten S. Mikkelsen license is retained at the top of that file. The host changes are identified in the header: Rust module/std integration, explicit raw-pointer vector references for the current compiler, phase measurements, and optional parallel-cell/group hooks. The original tangent arithmetic and complete-mesh preparation order are retained.

`parallel.rs` copies preparation into owned numeric indices/ranges before creating tasks. Each worker reconstructs only its group-member triangles and local pointers inside that worker. No raw pointer crosses the task boundary and there is no `unsafe impl Send`/`Sync`. Compact triangle order is monotonic in the original prepared order; encoded source face/corner IDs, original face identity, floating-point operations and accumulation order are retained. Group output ownership is checked before parallel evaluation. Quad geometry keeps the original serial evaluator and its averaging behavior.

After group evaluation, the original serial degenerate epilogue and ordered setters run over the complete mesh. This differs from the rejected halo experiment, which repeated preparation on fragments and changed actual Face tangents.

The parity test compares all tangent component bits with Bevy's original `Mesh::generate_tangents()`. The actual Ray source test covers integer poses 0–157 and the halo-failure pose 39.25 for Body and Face material 0. Measurements and deployment status belong in the project performance notes; passing this isolated comparison is not an application-FPS claim.

Welding retains the original complete-mesh bounding box, grid cell assignment, cell insertion order and `MergeVertsFast` algorithm. Independent cells use worker-local scratch/index arrays while keeping the original globally encoded representative values; only disjoint cell results are merged afterward. This is not fragment-local welding. The integrated cache/tangent parity test also passed all 159 Ray poses after this change in the debug build; release phase measurements must still be taken separately.

The first global edge quicksort partitions with the original pivot seed and swap order, then evaluates disjoint recursive ranges on the existing task pool. Serial leaves retain upstream tie ordering. Whole-array tests include duplicate keys and all three channels. After this hook, all 159 actual Ray poses retained exact cached geometry and tangent bits; the full Bevy unit, GPU rendering and bake tests passed.

The test-only preparation guard probe measured only 7 exact representative-map matches over 158 consecutive pose transitions for both Body and Face, although canonical equality classes and pre-group flags matched all transitions. No connectivity cache is enabled on the basis of canonical equivalence alone.
