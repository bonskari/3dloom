//! Map channels: per-corner texture coordinates that survive edits.

use glam::Vec3;
use loom_mesh::poly_ops::ExtrudeKind;
use loom_mesh::uvmap::{auto_map, project, AutoMap, MapParams, Projection};
use loom_mesh::{box_segmented, FaceId, Mesh};

fn boxed() -> Mesh {
    let mut m = box_segmented(10.0, 20.0, 30.0, [1, 1, 1]);
    auto_map(&mut m, AutoMap::Box, 1);
    m
}

/// The corner uvs of a face, in loop order.
fn face_uvs(m: &Mesh, f: FaceId, ch: u32) -> Vec<Vec3> {
    m.face_corners(f).unwrap().into_iter().map(|he| m.corner_uv(ch, he).unwrap()).collect()
}

#[test]
fn box_primitive_has_box_mapping_on_channel_1() {
    let m = boxed();
    m.check_invariants().unwrap();
    let ch = m.map(1).expect("channel 1");
    assert_eq!(ch.corners.len(), m.half_edge_count());
    assert_eq!(m.face_count(), 6);
    for f in 0..6 {
        for uv in face_uvs(&m, FaceId(f as u32), 1) {
            assert!((-1e-5..=1.0 + 1e-5).contains(&uv.x) && (-1e-5..=1.0 + 1e-5).contains(&uv.y), "{uv:?}");
        }
    }
    // The +Z face spans the unit square.
    let top = (0..6).map(|f| FaceId(f as u32)).find(|f| m.face_normal(*f).z > 0.9).unwrap();
    let uvs = face_uvs(&m, top, 1);
    let (lo, hi) = uvs.iter().fold((Vec3::splat(9.0), Vec3::splat(-9.0)), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
    assert!(lo.x.abs() < 1e-5 && lo.y.abs() < 1e-5 && (hi.x - 1.0).abs() < 1e-5 && (hi.y - 1.0).abs() < 1e-5, "{uvs:?}");
}

#[test]
fn channel_survives_extrude_and_delete() {
    let mut m = boxed();
    let top = (0..6).map(|f| FaceId(f as u32)).find(|f| m.face_normal(*f).z > 0.9).unwrap();
    let side = (0..6).map(|f| FaceId(f as u32)).find(|f| m.face_normal(*f).x > 0.9).unwrap();
    let side_uvs = face_uvs(&m, side, 1);
    let side_pos = m.face_positions(side).unwrap();
    let new_faces = loom_mesh::poly_ops::extrude_faces(&mut m, &[top], 5.0, ExtrudeKind::Group).unwrap();
    m.check_invariants().unwrap();
    assert!(!new_faces.is_empty());
    // The untouched side keeps its uvs bit for bit (found again by position).
    let side2 = (0..m.face_count()).map(|f| FaceId(f as u32)).find(|f| m.face_positions(*f).unwrap() == side_pos).unwrap();
    assert_eq!(face_uvs(&m, side2, 1), side_uvs);
    // Every corner has a tv, including the new side faces.
    for f in 0..m.face_count() {
        assert_eq!(face_uvs(&m, FaceId(f as u32), 1).len(), m.face_vertex_ids(FaceId(f as u32)).unwrap().len());
    }
    loom_mesh::edit_ops::delete_faces(&mut m, &[FaceId(0)]).unwrap();
    m.check_invariants().unwrap();
    let side3 = (0..m.face_count()).map(|f| FaceId(f as u32)).find(|f| m.face_positions(*f).unwrap() == side_pos).unwrap();
    assert_eq!(face_uvs(&m, side3, 1), side_uvs);
}

#[test]
fn missing_channel_is_created_on_demand() {
    let mut m = box_segmented(1.0, 1.0, 1.0, [1, 1, 1]);
    assert!(m.map(1).is_none());
    assert_eq!(m.corner_uv(1, loom_mesh::HalfEdgeId(0)), None);
    m.ensure_map(1);
    let ch = m.map(1).unwrap();
    assert_eq!(ch.verts, vec![Vec3::ZERO]);
    assert!(ch.corners.iter().all(|c| *c == 0));
    m.check_invariants().unwrap();
    m.set_corner_uv(1, loom_mesh::HalfEdgeId(0), Vec3::new(0.25, 0.5, 0.0));
    assert_eq!(m.corner_uv(1, loom_mesh::HalfEdgeId(0)), Some(Vec3::new(0.25, 0.5, 0.0)));
    assert_eq!(m.corner_uv(1, loom_mesh::HalfEdgeId(1)), Some(Vec3::ZERO), "other corners keep tv 0");
}

#[test]
fn planar_projection_maps_bounds_to_unit_square() {
    let m = box_segmented(10.0, 20.0, 30.0, [1, 1, 1]);
    let params = MapParams { projection: Projection::Planar, ..MapParams::fit(&m) };
    let out = project(&m, &params, 1);
    out.check_invariants().unwrap();
    let mut lo = Vec3::splat(9.0);
    let mut hi = Vec3::splat(-9.0);
    for f in 0..out.face_count() {
        for uv in face_uvs(&out, FaceId(f as u32), 1) {
            lo = lo.min(uv);
            hi = hi.max(uv);
        }
    }
    assert!((lo.x).abs() < 1e-4 && (lo.y).abs() < 1e-4 && (hi.x - 1.0).abs() < 1e-4 && (hi.y - 1.0).abs() < 1e-4, "{lo:?} {hi:?}");
    // Tiling and flip.
    let params = MapParams { u_tile: 2.0, v_flip: true, ..params };
    let out = project(&m, &params, 1);
    let (mut lo, mut hi) = (Vec3::splat(9.0), Vec3::splat(-9.0));
    for f in 0..out.face_count() {
        for uv in face_uvs(&out, FaceId(f as u32), 1) {
            lo = lo.min(uv);
            hi = hi.max(uv);
        }
    }
    assert!((hi.x - 2.0).abs() < 1e-4 && lo.y.abs() < 1e-4 && (hi.y - 1.0).abs() < 1e-4, "{lo:?} {hi:?}");
}

#[test]
fn cylindrical_seam_corners_differ() {
    let m = loom_mesh::cylinder_segmented(5.0, 10.0, 1, 1, 8, None);
    let params = MapParams { projection: Projection::Cylindrical { cap: true }, ..MapParams::fit(&m) };
    let out = project(&m, &params, 1);
    out.check_invariants().unwrap();
    let mut max_u = 0.0f32;
    let mut min_u = 1.0f32;
    let mut seam_ok = false;
    for f in 0..out.face_count() {
        let id = FaceId(f as u32);
        let n = out.face_normal(id);
        let uvs = face_uvs(&out, id, 1);
        if n.z.abs() > 0.9 {
            // Caps are planar: inside the unit square.
            assert!(uvs.iter().all(|uv| (-1e-4..=1.0 + 1e-4).contains(&uv.x) && (-1e-4..=1.0 + 1e-4).contains(&uv.y)), "{uvs:?}");
            continue;
        }
        let (fl, fh) = uvs.iter().fold((9.0f32, -9.0f32), |(a, b), uv| (a.min(uv.x), b.max(uv.x)));
        assert!(fh - fl < 0.5, "a side face must not wrap around the seam: {uvs:?}");
        if fh > 1.0 - 1e-3 {
            seam_ok = true;
        }
        max_u = max_u.max(fh);
        min_u = min_u.min(fl);
        assert!(uvs.iter().all(|uv| (-1e-4..=1.0 + 1e-4).contains(&uv.y)));
    }
    assert!(seam_ok && min_u.abs() < 1e-4 && (max_u - 1.0).abs() < 1e-4, "{min_u} {max_u}");
}

#[test]
fn obj_round_trips_uvs() {
    let m = boxed();
    let text = loom_mesh::obj::write(&[("Box".into(), m.clone(), glam::Mat4::IDENTITY)]);
    assert!(text.contains("vt "), "{text}");
    let back = &loom_mesh::obj::parse(&text, "x").unwrap()[0].mesh;
    assert_eq!(back.face_count(), m.face_count());
    for f in 0..m.face_count() {
        let pos = m.face_positions(FaceId(f as u32)).unwrap();
        let g = (0..back.face_count()).map(|g| FaceId(g as u32)).find(|g| back.face_positions(*g).unwrap() == pos).unwrap();
        for (a, b) in face_uvs(&m, FaceId(f as u32), 1).iter().zip(face_uvs(back, g, 1)) {
            assert!((*a - b).length() < 1e-5, "{a:?} {b:?}");
        }
    }
}

#[test]
fn loom_round_trips_maps() {
    let m = boxed();
    let json = serde_json::to_string(&m).unwrap();
    let back: Mesh = serde_json::from_str(&json).unwrap();
    assert_eq!(back, m);
    // An old mesh without `maps` still loads.
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let mut o = v.as_object().unwrap().clone();
    o.remove("maps");
    let old: Mesh = serde_json::from_value(serde_json::Value::Object(o)).unwrap();
    assert!(old.map(1).is_none());
    old.check_invariants().unwrap();
}

/// Planar-mapped plane: every corner's uv must equal the projection of its
/// position, whatever edit ran (flip, insert, connect, triangulate,
/// tessellate, turn, remove, shell, attach, mirror).
fn planar_plane() -> Mesh {
    let mut m = loom_mesh::plane_segmented(4.0, 4.0, 2, 2);
    auto_map(&mut m, AutoMap::Planar, 1);
    m
}

fn check_planar_uvs(m: &Mesh, what: &str) {
    m.check_invariants().unwrap();
    assert!(m.map(1).is_some(), "{what}: channel 1 vanished");
    for f in 0..m.face_count() {
        for he in m.face_corners(FaceId(f as u32)).unwrap() {
            let p = m.vertices()[m.half_edges()[he.0 as usize].origin.0 as usize].position;
            let uv = m.corner_uv(1, he).unwrap_or_else(|| panic!("{what}: corner {} has no uv", he.0));
            let expect = Vec3::new(p.x / 4.0 + 0.5, p.y / 4.0 + 0.5, 0.5);
            assert!((uv - expect).length() < 1e-4, "{what}: corner {} at {p:?} has uv {uv:?}, expected {expect:?}", he.0);
        }
    }
}

#[test]
fn uvs_follow_common_edits() {
    use loom_mesh::{HalfEdgeId, VertexId};
    let mut m = planar_plane();
    let all: Vec<FaceId> = (0..m.face_count()).map(|i| FaceId(i as u32)).collect();
    loom_mesh::poly_ops::flip_faces(&mut m, &all).unwrap();
    check_planar_uvs(&m, "flip");
    let mut m = planar_plane();
    let e = (0..m.half_edge_count() as u32).map(HalfEdgeId).find(|h| m.half_edges()[h.0 as usize].face.is_some()).unwrap();
    loom_mesh::topo_ops::insert_vertex_on_edge(&mut m, e, 0.5).unwrap();
    check_planar_uvs(&m, "insert vertex on edge");
    let mut m = planar_plane();
    let edges: Vec<HalfEdgeId> = (0..m.half_edge_count() as u32).map(HalfEdgeId).filter(|h| m.canonical_edge_id(*h) == Some(*h)).take(2).collect();
    loom_mesh::topo_ops::connect_edges(&mut m, &edges, 1, 0.0, 0.0).unwrap();
    check_planar_uvs(&m, "connect");
    let m = loom_mesh::mesh_mods::triangulate(&planar_plane()).unwrap();
    check_planar_uvs(&m, "triangulate");
    let mut m = planar_plane();
    loom_mesh::geom_ops::tessellate(&mut m, &[FaceId(0)], loom_mesh::geom_ops::TessKind::Face, 0.0).unwrap();
    check_planar_uvs(&m, "tessellate face");
    let mut m = planar_plane();
    loom_mesh::geom_ops::tessellate(&mut m, &[FaceId(0), FaceId(1)], loom_mesh::geom_ops::TessKind::Edge, 0.0).unwrap();
    check_planar_uvs(&m, "tessellate edge");
    let mut m = loom_mesh::mesh_mods::triangulate(&planar_plane()).unwrap();
    let inner = (0..m.half_edge_count() as u32).map(HalfEdgeId).find(|h| { let hh = &m.half_edges()[h.0 as usize]; hh.face.is_some() && m.half_edges()[hh.twin.0 as usize].face.is_some() }).unwrap();
    loom_mesh::topo_ops::turn_edge(&mut m, inner).unwrap();
    check_planar_uvs(&m, "turn edge");
    let mut m = planar_plane();
    let centre = (0..m.vertex_count() as u32).map(VertexId).find(|v| m.vertex_position(*v).unwrap().length() < 1e-5).unwrap();
    loom_mesh::topo_ops::remove_vertices(&mut m, &[centre]).unwrap();
    check_planar_uvs(&m, "remove vertex");
    let (m, _, _) = loom_mesh::mesh_mods::shell(&planar_plane(), 0.0, 0.0, 1).unwrap();
    m.check_invariants().unwrap();
    for f in 0..m.face_count() {
        for he in m.face_corners(FaceId(f as u32)).unwrap() {
            let p = m.vertices()[m.half_edges()[he.0 as usize].origin.0 as usize].position;
            let uv = m.corner_uv(1, he).unwrap();
            if p.z.abs() < 1e-4 && m.face_normal(FaceId(f as u32)).z.abs() > 0.9 {
                assert!((uv.x - (p.x / 4.0 + 0.5)).abs() < 1e-4, "shell: {uv:?} at {p:?}");
            }
        }
    }
    let mut m = planar_plane();
    let other = planar_plane();
    loom_mesh::geom_ops::attach(&mut m, &other, glam::Mat4::from_translation(Vec3::new(0.0, 0.0, 5.0))).unwrap();
    m.check_invariants().unwrap();
    for f in 4..8 {
        for he in m.face_corners(FaceId(f)).unwrap() {
            let p = m.vertices()[m.half_edges()[he.0 as usize].origin.0 as usize].position;
            let uv = m.corner_uv(1, he).unwrap();
            assert!((uv.x - (p.x / 4.0 + 0.5)).abs() < 1e-4, "attach: {uv:?} at {p:?}");
        }
    }
    let mut m = planar_plane();
    loom_mesh::deform::mirror(&mut m, 2, 0.0, true).unwrap();
    m.check_invariants().unwrap();
    assert!(m.map(1).is_some() && (0..m.half_edge_count()).all(|i| m.half_edges()[i].face.is_none() || m.corner_uv(1, HalfEdgeId(i as u32)).is_some()));
}

#[test]
fn projections_weld_only_within_a_vertex() {
    // Face mapping gives every face the same four values; they must stay six separate charts.
    let m = box_segmented(2.0, 2.0, 2.0, [1, 1, 1]);
    let params = MapParams { projection: Projection::Face, ..MapParams::fit(&m) };
    let out = project(&m, &params, 1);
    assert_eq!(out.map(1).unwrap().verts.len(), 24, "no welding across faces that share no vertex");
    // A vertex shared by three faces with equal values welds (box mapping of the +Z corner: no, planar: yes).
    let out = project(&m, &MapParams::fit(&m), 1);
    assert_eq!(out.map(1).unwrap().verts.len(), 8, "planar: one tv per vertex");
}

#[test]
fn cylindrical_odd_sides_keep_each_face_narrow() {
    let m = loom_mesh::cylinder_segmented(5.0, 10.0, 1, 1, 5, None);
    let params = MapParams { projection: Projection::Cylindrical { cap: true }, ..MapParams::fit(&m) };
    let out = project(&m, &params, 1);
    for f in 0..out.face_count() {
        let id = FaceId(f as u32);
        if out.face_normal(id).z.abs() > 0.9 {
            continue;
        }
        let us: Vec<f32> = face_uvs(&out, id, 1).iter().map(|uv| uv.x).collect();
        let (lo, hi) = us.iter().fold((9.0f32, -9.0f32), |(a, b), u| (a.min(*u), b.max(*u)));
        assert!(hi - lo < 0.3, "face {f} spans u {lo}..{hi}");
    }
}
