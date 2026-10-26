# Read-only investigation: an exact prepared MikkTSpace split

This describes a possible next implementation. It has not been implemented or performance-validated. The rejected halo prototype is not sufficient evidence that this approach will match the original algorithm.

## Preserve the complete preparation phase

The inspected dependency is `bevy_mikktspace` 0.16.1. Its public API only exposes `generate_tangents`; the generated implementation is private. An implementation exposing intermediate preparation would therefore need a narrowly maintained fork/vendor copy, with the upstream license retained and alterations identified.

A conservative split would retain these original whole-mesh phases and their order:

1. `GenerateInitialVerticesIndexList`.
2. `GenerateSharedVerticesIndexList`.
3. Geometric-degenerate detection and `DegenPrologue`.
4. `InitTriInfo`, including the original `BuildNeighborsFast` edge ordering.
5. `Build4RuleGroups`.
6. Parallel evaluation of the groups currently processed by `GenerateTSpaces`.
7. Original serial `DegenEpilogue` and face/corner tangent setters.

This retains global weld representatives, neighbor choices, group/orientation assignments, degenerate inheritance, and final indexed-vertex overwrite order. In particular, do not replace preparation with a locally reindexed fragment and assume a complete vertex fan is sufficient: the stored prototype already disproves that assumption for Ray.

`BuildNeighborsFast` has prefix-run sorting loops without a final explicit sort call for the last `i0`/`i1` runs. That is an observation about the inspected source, not a verified cause of the halo failure. An exact compatibility implementation must preserve the existing behavior until a separately justified algorithm change is requested.

## Which data depend on the pose?

| Data | Reuse conditions / required work |
| --- | --- |
| Initial triangle-to-source-corner mapping | Invariant while the Bevy triangle-list indices and face inclusion remain identical. Bevy's adapter presents triangles, not quads. |
| Shared vertex representatives, `piTriListIn` after welding | Pose-dependent: upstream compares current position, normal and UV floats exactly. Counts and source vertex IDs are insufficient. Canonical weld equivalence alone does not preserve the upstream representative's encoded face/corner ID. |
| Geometric-degenerate mask and reordered triangle list | Recompute or validate against current positions. A skin pose can collapse an edge. |
| `iOrgFaceNumber`, `iTSpacesOffs`, `vert_num` | Derived from source corner layout, but their array ordering follows degenerate reordering. |
| `FaceNeighbors` | Derived from the complete welded index list and global edge ordering; reuse requires a sufficiently strong exact preparation guard, not just mesh counts. |
| `vOs`, `vOt`, `fMagS`, `fMagT` | Always update from current positions and UVs using the original arithmetic order. |
| UV orientation and `GROUP_WITH_ANY` flags | Validate every pose. Zero UV area or degenerate tangent vectors can change orientation inheritance. |
| Group membership, representative and orientation | Potentially reusable only when the complete relevant welding, degeneracy, neighbor and orientation preparation remains identical. |
| Tangent subgroups inside `GenerateTSpaces` | Still pose-dependent: membership tests project current tangent vectors against current normals and compare their directions. Do not cache merely because the UVs are unchanged. |
| Corner angle weights and accumulated tangent spaces | Always calculate from current geometry/normals, preserving accumulation order. |
| Degenerate tangent inheritance and final indexed setters | Run after group results are merged, preserving original global ordering. |

An initial exact parallel implementation should perform fresh whole-mesh preparation every pose. Persistent preparation caching should be a separate optimization, measured for actual hit rates and validated against complete serial preparation.

## Safe worker boundary

The generated structures contain raw pointers: `STriInfo.AssignedGroup`, `SGroup.pFaceIndices`, and subgroup pointers. Do not add an unchecked `Send`/`Sync` implementation to move them across worker threads.

A safer boundary would copy the prepared read-only information into owned vectors of indices/ranges, retain current per-triangle tangent data, and evaluate groups against immutable geometry views. Keep a defined mapping from each group to its destination corner tangent-space slots.

For Bevy's triangle-only adapter, a nondegenerate face corner is assigned to at most one group. That makes independent group results a plausible parallel boundary, but this must be asserted and tested before relying on it. Merge into the original per-face-corner tangent-space array, then execute the original degenerate epilogue and ordered setters. Merging directly into indexed vertex tangents risks changing the original last-writer behavior when several triangle corners reference one vertex.

## Required validation before production

- Compare all four tangent components by bits with the unchanged Bevy implementation across the actual Ray animation range, including the failing Face frame 39.25.
- Include UV seams, overlapping/coincident geometry, explicit/split normals, mirrored UV orientation, geometric degenerates, zero-UV triangles, hidden/material changes, and repeated indexed vertices.
- Prove that every parallel output slot has one owner or select the unchanged serial calculation for unsupported cases.
- Verify cache invalidation against current preparation, rather than counting vertices/faces or reusing rest-pose tangents.
- Measure release-build preparation, group evaluation and completion separately before claiming a frame-rate benefit. The largest current Face material took about 36 ms in the application's earlier profiling; the parallelizable fraction of that time has not yet been isolated.
- Preserve all skin influences. Ray's imported data includes up to 12 influences per vertex and a Face palette with 875 bone entries; a four-influence replacement is not equivalent.
