//! Owned numeric boundary for the upstream group's pointer-based evaluator.
//! No raw pointer is captured by, sent to, or shared between worker tasks.
use super::*;
use std::ops::Range;

struct Triangle {
    neighbours: [i32; 3],
    groups: [Option<usize>; 3],
    os: Vec3,
    ot: Vec3,
    mag_s: f32,
    mag_t: f32,
    original_face: i32,
    flags: i32,
    original_offset: i32,
    corners: [u8; 4],
    indices: [i32; 3],
}

struct Group {
    faces: Range<usize>,
    representative: i32,
    orientation: bool,
}

pub(super) fn generate<I: Geometry + Sync>(
    output: &mut [STSpace],
    triangles: &[STriInfo],
    groups: &[SGroup],
    indices: &[i32],
    threshold: f32,
    geometry: &I,
    pool: &bevy_tasks::TaskPool,
) -> Option<bool> {
    let pack_scope = loom_render::profile::Scope::new("mikk.pack");
    let mut face_indices = Vec::with_capacity(triangles.len() * 3);
    let mut owned_groups = Vec::with_capacity(groups.len());
    for group in groups {
        let start = face_indices.len();
        // Build4RuleGroups allocated each face range inside its own stable
        // group buffer. Copy the indices before creating any worker tasks.
        face_indices.extend_from_slice(unsafe { std::slice::from_raw_parts(group.pFaceIndices, group.iNrFaces as usize) });
        owned_groups.push(Group { faces: start..face_indices.len(), representative: group.iVertexRepresentative, orientation: group.bOrientPreserving });
    }
    let owned_triangles: Vec<_> = triangles.iter().zip(indices.chunks_exact(3)).map(|(tri, ids)| {
        let assigned = tri.AssignedGroup.map(|group| {
            if group.is_null() { None } else {
                // AssignedGroup was populated from this exact groups allocation.
                let index = unsafe { group.cast_const().offset_from(groups.as_ptr()) } as usize;
                assert!(index < groups.len());
                Some(index)
            }
        });
        Triangle { neighbours: tri.FaceNeighbors, groups: assigned, os: tri.vOs, ot: tri.vOt, mag_s: tri.fMagS, mag_t: tri.fMagT, original_face: tri.iOrgFaceNumber, flags: tri.iFlag, original_offset: tri.iTSpacesOffs, corners: tri.vert_num, indices: [ids[0], ids[1], ids[2]] }
    }).collect();
    // Each output slot must have one group owner. Quads use averaging across
    // groups and are kept on the unchanged serial path by the caller.
    let mut owners = vec![usize::MAX; output.len()];
    for (group_id, group) in owned_groups.iter().enumerate() {
        for face in &face_indices[group.faces.clone()] {
            let tri = &owned_triangles[*face as usize];
            let corner = tri.groups.iter().position(|id| *id == Some(group_id))?;
            let slot = tri.original_offset as usize + tri.corners[corner] as usize;
            if owners[slot] != usize::MAX && owners[slot] != group_id { return None; }
            owners[slot] = group_id;
        }
    }
    drop(pack_scope);
    let workers = pool.thread_num().min(8).min(groups.len());
    let results = pool.scope(|scope| {
        for worker in 0..workers {
            let range = groups.len() * worker / workers..groups.len() * (worker + 1) / workers;
            let owned_groups = &owned_groups;
            let owned_triangles = &owned_triangles;
            let face_indices = &face_indices;
            let owners = &owners;
            scope.spawn(async move {
                let unpack_scope = loom_render::profile::Scope::new("mikk.worker_pack");
                let mut included = vec![false; owned_triangles.len()];
                for group in &owned_groups[range.clone()] { for face in &face_indices[group.faces.clone()] { included[*face as usize] = true; } }
                // The compact triangle order is monotonic in the original
                // prepared order, preserving every sorted subgroup sum order.
                let selected: Vec<_> = included.iter().enumerate().filter_map(|(i, yes)| yes.then_some(i)).collect();
                let mut local_index = vec![usize::MAX; owned_triangles.len()];
                for (local, global) in selected.iter().enumerate() { local_index[*global] = local; }
                let mut local_faces: Vec<Vec<i32>> = owned_groups[range.clone()].iter().map(|group| face_indices[group.faces.clone()].iter().map(|f| local_index[*f as usize] as i32).collect()).collect();
                let mut local_groups: Vec<_> = owned_groups[range.clone()].iter().zip(&mut local_faces).map(|(group, faces)| SGroup { iNrFaces: faces.len() as i32, pFaceIndices: faces.as_mut_ptr(), iVertexRepresentative: group.representative, bOrientPreserving: group.orientation }).collect();
                let group_base = local_groups.as_mut_ptr();
                let mut local_triangles = Vec::with_capacity(selected.len());
                let mut local_indices = Vec::with_capacity(selected.len() * 3);
                for (local, global) in selected.iter().enumerate() {
                    let tri = &owned_triangles[*global];
                    let assigned = tri.groups.map(|group| match group {
                        Some(id) if range.contains(&id) => unsafe { group_base.add(id - range.start) },
                        _ => std::ptr::null_mut(),
                    });
                    local_triangles.push(STriInfo { FaceNeighbors: tri.neighbours, AssignedGroup: assigned, vOs: tri.os, vOt: tri.ot, fMagS: tri.mag_s, fMagT: tri.mag_t, iOrgFaceNumber: tri.original_face, iFlag: tri.flags, iTSpacesOffs: (local * 3) as i32, vert_num: tri.corners });
                    // Encoded face/corner values remain global: Geometry reads
                    // exactly the original source positions, normals and UVs.
                    local_indices.extend_from_slice(&tri.indices);
                }
                let mut spaces = vec![STSpace::zero(); selected.len() * 3];
                let mut view = super::super::ReadGeometry(geometry);
                drop(unpack_scope);
                let math_scope = loom_render::profile::Scope::new("mikk.group_math");
                // All pointers below were reconstructed inside this worker and
                // remain valid until the original evaluator returns.
                let success = unsafe { GenerateTSpaces(&mut spaces, local_triangles.as_ptr(), local_groups.as_ptr(), local_groups.len() as i32, local_indices.as_ptr(), threshold, &mut view) };
                drop(math_scope);
                let mut result = Vec::new();
                for (local, global) in selected.iter().enumerate() {
                    let tri = &owned_triangles[*global];
                    for corner in 0..3 {
                        let slot = tri.original_offset as usize + tri.corners[corner] as usize;
                        if range.contains(&owners[slot]) { result.push((slot, spaces[local * 3 + tri.corners[corner] as usize])); }
                    }
                }
                (success, result)
            });
        }
    });
    let _merge_scope = loom_render::profile::Scope::new("mikk.merge");
    for (success, spaces) in results {
        if !success { return Some(false); }
        for (slot, space) in spaces { output[slot] = space; }
    }
    Some(true)
}

/// MergeVertsFast only reads/writes entries of its current grid cell. Keep
/// the original global grid and each cell's insertion order, but evaluate
/// disjoint cells with worker-local indices and scratch storage.
pub(super) fn weld_cells<I: Geometry + Sync>(
    indices: &mut [i32], table: &[i32], offsets: &[i32], counts: &[i32],
    geometry: &I, pool: &bevy_tasks::TaskPool,
) {
    let pack_scope = loom_render::profile::Scope::new("mikk.weld_pack");
    let workers = pool.thread_num().min(8);
    let mut jobs = vec![Vec::new(); workers];
    let mut sizes = vec![0usize; workers];
    for (cell, count) in counts.iter().enumerate().filter(|(_, count)| **count >= 2) {
        let worker = sizes.iter().enumerate().min_by_key(|(_, size)| **size).unwrap().0;
        jobs[worker].push(cell);
        sizes[worker] += *count as usize;
    }
    let original: &[i32] = indices;
    drop(pack_scope);
    let results = pool.scope(|scope| {
        for (cells, capacity) in jobs.into_iter().zip(sizes) {
            scope.spawn(async move {
                let _scope = loom_render::profile::Scope::new("mikk.weld_jobs");
                let mut view = super::super::ReadGeometry(geometry);
                let mut local_indices = Vec::new();
                let mut scratch = Vec::new();
                let mut result = Vec::with_capacity(capacity);
                for cell in cells {
                    let count = counts[cell] as usize;
                    let entries = &table[offsets[cell] as usize..offsets[cell] as usize + count];
                    local_indices.clear();
                    scratch.clear();
                    for (local, global) in entries.iter().enumerate() {
                        let encoded = original[*global as usize];
                        let p = get_position(&mut view, encoded as usize);
                        local_indices.push(encoded);
                        scratch.push(STmpVert { vert: [p.x, p.y, p.z], index: local as i32 });
                    }
                    // All pointers are local to this task. The encoded Geometry
                    // indices remain global; only array offsets are compacted.
                    unsafe { MergeVertsFast(local_indices.as_mut_ptr(), scratch.as_mut_ptr(), &mut view, 0, count as i32 - 1); }
                    result.extend(entries.iter().zip(&local_indices).map(|(global, encoded)| (*global as usize, *encoded)));
                }
                result
            });
        }
    });
    let _merge_scope = loom_render::profile::Scope::new("mikk.weld_merge");
    for result in results { for (global, encoded) in result { indices[global] = encoded; } }
}
