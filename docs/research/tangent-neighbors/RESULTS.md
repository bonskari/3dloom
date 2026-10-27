# Original final-bucket neighbor defect and guarded-cache experiment

The original vendored neighbor builder does not flush its final i0 bucket for the secondary i1 sort, or its final i0/i1 bucket for the tertiary face sort. A four-triangle manifold fixture enumerates all 24 face orders and checks against explicit opposite-edge adjacency: original fails 15 orders; flushing both final buckets passes all 24. This is separate from the earlier rejected halo experiment.

Actual source: RayAnimationTest_LoomRig.loom, Body001 and Face001 material 0, integer poses 0–157 plus 39.25. Both meshes have zero welded edges with incidence greater than two in every tested pose. Fixing the final buckets changes no Body tangent bits. Face changes 14 corners at frame17, 7 at121, 14 at146, 14 at39.25. Largest tangent component difference is 0.43010038; this is not an ULP-only difference.

Real GPU renders use the actual Face StandardMaterial, diffuse texture, and normal texture (1024×1024, 9915 distinct RGB values). Front, left, right and back 768×768 renders at all four affected poses have zero changed pixels. A deliberate tangent-rotation control changes 160822, 160625, 160864 and160456 front pixels respectively, confirming the shader consumes the replacement tangents. Fresh mesh asset handles are required in this test; the earlier same-handle experiment failed the positive control and is not evidence of visual equality. The saved frame39.25 images are byte-identical and were visually inspected.

The test-only guarded neighbor cache checks the current complete-mesh welding equivalence classes, current derivative flags and original face order. It only stores meshes whose welded edge incidence is at most two. It recomputes welding and pose derivatives, and retains fresh subgroup and tangent arithmetic. Across all159poses it has316hits/2first misses; every output tangent bit matches fresh corrected preparation. Combined Body/Face tangent time in this isolated release run was26.613ms/pose fresh versus19.580ms cached. This is not an application FPS measurement.

The implementation and opt-in reproductions live in `crates/loom_bevy/src/mikktspace/neighbor_probe.rs`. At this checkpoint the corrected path and cache are test-only; production remains unchanged until the guarded production implementation and regressions are verified.

## Complete material and framed GPU audit

The full five-skin/13-material batch passed1431corrected-reference tangent comparisons across159poses. Geometry position/normal/UV bits and indices also match. Persistent caches had1422hits/9initial misses; isolated batched material preparation averaged19.287ms/pose in that run.

An additional comparison against the unchanged Bevy tangent generator found these affected parts:

| Part | Node/material | Changed poses | Changed corners | Maximum component difference |
|---|---|---:|---:|---:|
|1|Face001/0|4|49|0.43010038|
|2|Face001/1|11|66|0.010680703|
|4|Face001/3|3|15|0.0067144632|
|5|Face001/4|6|30|0.008490868|
|10|SkeletalMesh001/0 (shoes/socks)|30|630|0.104471855|
|12|SkeletalMesh003/0 (pants)|2|40|0.120447636|

The remaining parts were unchanged. All affected node-pose combinations (54) were rendered with **all** actual normal-mapped material parts replaced together, in four directions, at768×768. Every source vertex is asserted to have positive clip w and |NDC x/y|<0.98; the earlier closer camera clipped geometry and is not used for this complete audit. All54front-view mutation controls changed33905–89625pixels, confirming the replacement tangent path remains active. Actual textures are loaded and their dimensions/unique RGB values checked.

Of216fully framed view comparisons,214are pixel-identical. Pants/right at frame30 changes two pixels, and at80 changes one pixel. Each change is one8-bit channel step:

- Frame30: (362,625) RGBA(12,11,12,255)→(12,11,13,255); (367,628) (13,14,14,255)→(14,14,14,255).
- Frame80: (354,623) (5,5,5,255)→(5,5,6,255).

The corrected adjacency fix was accepted with these quantified shading changes. Representative fully framed head, shoe and pants original/corrected images are preserved in `images/framed-*`. This is a tested adjacency correction, not a claim of pixel identity at every possible camera or pose. The live application's FPS still needs measurement separately.
