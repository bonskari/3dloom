//! Smoothing groups from hard edges and shared corner normals (FBX import/export).

use loom_mesh::{mesh_mods, FaceId};
use std::collections::HashSet;

fn box10() -> loom_mesh::Mesh {
    loom_mesh::box_segmented(10.0, 10.0, 10.0, [1, 1, 1])
}

#[test]
fn smoothing_groups_from_hard_edges_colour_adjacent_regions() {
    // A box: every edge hard → six regions, neighbours get different bits.
    let mut m = box10();
    let mut hard = HashSet::new();
    for i in 0..m.half_edge_count() {
        let he = &m.half_edges()[i];
        let a = he.origin.0;
        let b = m.half_edges()[he.twin.0 as usize].origin.0;
        hard.insert((a.min(b), a.max(b)));
    }
    mesh_mods::smoothing_groups_from_hard_edges(&mut m, &hard);
    for f in 0..6 {
        let g = m.faces()[f].smoothing_groups;
        assert_ne!(g, 0, "face {f} must get a group");
        assert_eq!(g.count_ones(), 1);
    }
    for i in 0..m.half_edge_count() {
        let he = &m.half_edges()[i];
        let (Some(fa), Some(fb)) = (he.face, m.half_edges()[he.twin.0 as usize].face) else { continue };
        assert_eq!(m.faces()[fa.0 as usize].smoothing_groups & m.faces()[fb.0 as usize].smoothing_groups, 0, "hard edge {i} shares a bit");
    }
    // No hard edges: one group everywhere.
    mesh_mods::smoothing_groups_from_hard_edges(&mut m, &HashSet::new());
    assert!(m.faces().iter().all(|f| f.smoothing_groups == 1));
}

#[test]
fn corner_normals_follow_smoothing_groups() {
    let mut m = box10();
    for f in m.faces_mut() {
        f.smoothing_groups = 0;
    }
    let flat = mesh_mods::corner_normals(&m);
    assert_eq!(flat.len(), m.half_edge_count());
    for f in 0..6 {
        for he in m.face_corners(FaceId(f as u32)).unwrap() {
            assert!((flat[he.0 as usize] - m.face_normal(FaceId(f as u32))).length() < 1e-5);
        }
    }
    for f in m.faces_mut() {
        f.smoothing_groups = 1;
    }
    let smooth = mesh_mods::corner_normals(&m);
    // Every corner of a fully smooth box points along its vertex diagonal (the box sits on z = 0).
    let (lo, hi) = m.aabb();
    let centre = (lo + hi) * 0.5;
    for i in 0..m.half_edge_count() {
        let he = &m.half_edges()[i];
        if he.face.is_none() {
            continue;
        }
        let v = m.vertices()[he.origin.0 as usize].position - centre;
        assert!((smooth[i] - v.normalize()).length() < 1e-5, "{:?} vs {:?}", smooth[i], v.normalize());
    }
}
