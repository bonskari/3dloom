# Prepared MikkTSpace group evaluation

`generated.rs` is derived from `bevy_mikktspace` 0.16.1's generated implementation. Its original Morten S. Mikkelsen license is retained at the top of that file. The host changes are identified in the header: Rust module/std integration, explicit raw-pointer vector references for the current compiler, phase measurements, and optional parallel-cell/group hooks. The original tangent arithmetic and complete-mesh preparation order are retained.

`parallel.rs` copies preparation into owned numeric indices/ranges before creating tasks. Each worker reconstructs only its group-member triangles and local pointers inside that worker. No raw pointer crosses the task boundary and there is no `unsafe impl Send`/`Sync`. Compact triangle order is monotonic in the original prepared order; encoded source face/corner IDs, original face identity, floating-point operations and accumulation order are retained. Group output ownership is checked before parallel evaluation. Quad geometry keeps the original serial evaluator and its averaging behavior.

After group evaluation, the original serial degenerate epilogue and ordered setters run over the complete mesh. This differs from the rejected halo experiment, which repeated preparation on fragments and changed actual Face tangents.

The parity test compares all tangent component bits with Bevy's original `Mesh::generate_tangents()`. The actual Ray source test covers integer poses 0–157 and the halo-failure pose 39.25 for Body and Face material 0. Measurements and deployment status belong in the project performance notes; passing this isolated comparison is not an application-FPS claim.

Welding retains the original complete-mesh bounding box, grid cell assignment, cell insertion order and `MergeVertsFast` algorithm. Independent cells use worker-local scratch/index arrays while keeping the original globally encoded representative values; only disjoint cell results are merged afterward. This is not fragment-local welding. The integrated cache/tangent parity test also passed all 159 Ray poses after this change in the debug build; release phase measurements must still be taken separately.

The first global edge quicksort partitions with the original pivot seed and swap order, then evaluates disjoint recursive ranges on the existing task pool. Serial leaves retain upstream tie ordering. Whole-array tests include duplicate keys and all three channels. After this hook, all 159 actual Ray poses retained exact cached geometry and tangent bits; the full Bevy unit, GPU rendering and bake tests passed.

The test-only preparation guard probe measured only 7 exact representative-map matches over 158 consecutive pose transitions for both Body and Face, although canonical equality classes and pre-group flags matched all transitions. No connectivity cache is enabled on the basis of canonical equivalence alone.

## Corrected manifold adjacency cache

The neighbor builder now flushes both final sort buckets. The original omission is covered by an explicit adjacency oracle and preserved legacy test switch in `neighbor_probe.rs`; tangent arithmetic is unchanged. This deliberate correction differs from the earlier exact-upstream implementation. See `docs/research/tangent-neighbors/RESULTS.md` for the actual tangent and pixel comparisons and positive controls.

`NeighborCache` stores only numeric face neighbors. Each pose still runs complete-mesh welding and computes fresh triangle tangent derivatives. The cache checks canonical current welded equality classes, derivative flags and original face order. On a miss, corrected complete preparation runs; adjacency is stored only if every welded edge has incidence at most two. Ambiguous nonmanifold geometry always uses fresh corrected preparation. With two or fewer incident faces, opposite-edge pairing is unique and the corrected sort makes it independent of representative labels. Group membership, subgroup angles, tangent accumulation and the degenerate epilogue still run on the current pose.

Tests invalidate adjacency on changed topology, position/normal/UV equality classes, orientation flags and geometric degeneracy. A deformation that preserves dependencies hits the cache while producing fresh changed tangents. The production cache passed every tangent bit against fresh corrected preparation for all159actual Ray poses (316hits,2initial misses); isolated combined Body/Face time was25.864ms/pose fresh versus18.923ms cached. No FPS claim follows from this isolated result.

## Subgroup scratch ownership

Parallel group workers move the temporary member buffer into and out of the subgroup view instead of cloning it for every face. Unique subgroup vectors use `clone_from` to retain allocation capacity. Full vector contents, lengths and the original math/accumulation order remain unchanged. The serial const-mode reference retains the original clone operations for independent bit comparison.

A paired, alternating-order release comparison over all159Ray poses with the application paused measured Body5.796→5.558ms and Face12.916→12.603ms with exact tangent bits in both modes. The full13-part batch also matched all1431tangent results against the fresh serial corrected reference, plus geometry/UV bits and indices. The measured gain is modest (~0.55ms combined in the paired comparison), not an application FPS result.
