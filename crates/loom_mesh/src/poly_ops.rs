//! Editable Poly > Edit Polygons operations: Extrude (Group / Local Normal /
//! By Polygon), Inset, Outline, Bevel, Hinge From Edge, Flip, Make Planar and
//! plane alignment. Every op rebuilds through `IndexedMesh`.

use crate::indexed::{IndexedFace, IndexedMesh};
use crate::{FaceId, HalfEdgeId, Mesh, VertexId};
use glam::{Mat4, Vec3};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtrudeKind {
    Group,
    LocalNormal,
    ByPolygon,
}

/// Connected groups of the selected faces (faces sharing an edge).
pub fn face_groups(mesh: &Mesh, faces: &[FaceId]) -> Vec<Vec<FaceId>> {
    let sel: BTreeSet<FaceId> = faces.iter().copied().filter(|f| (f.0 as usize) < mesh.face_count()).collect();
    let hes = mesh.half_edges();
    let mut seen: BTreeSet<FaceId> = BTreeSet::new();
    let mut groups = Vec::new();
    for f in &sel {
        if !seen.insert(*f) {
            continue;
        }
        let mut group = vec![*f];
        let mut stack = vec![*f];
        while let Some(x) = stack.pop() {
            let start = mesh.faces()[x.0 as usize].he;
            let mut cur = start;
            loop {
                let twin = hes[hes[cur.0 as usize].twin.0 as usize].face;
                if let Some(nb) = twin {
                    if sel.contains(&nb) && seen.insert(nb) {
                        group.push(nb);
                        stack.push(nb);
                    }
                }
                cur = hes[cur.0 as usize].next;
                if cur == start {
                    break;
                }
            }
        }
        groups.push(group);
    }
    groups
}

/// Boundary half-edges of a face group, in the group's faces (one per edge).
fn group_boundary(mesh: &Mesh, group: &BTreeSet<FaceId>) -> Vec<HalfEdgeId> {
    let hes = mesh.half_edges();
    let mut out = Vec::new();
    for f in group {
        let start = mesh.faces()[f.0 as usize].he;
        let mut cur = start;
        loop {
            let other = hes[hes[cur.0 as usize].twin.0 as usize].face;
            if !other.is_some_and(|o| group.contains(&o)) {
                out.push(cur);
            }
            cur = hes[cur.0 as usize].next;
            if cur == start {
                break;
            }
        }
    }
    out
}

fn face_normal_area(mesh: &Mesh, f: FaceId) -> Vec3 {
    let Some(pts) = mesh.face_positions(f) else { return Vec3::ZERO };
    let mut n = Vec3::ZERO;
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        n += a.cross(b);
    }
    n * 0.5
}

/// Outward in-plane direction of boundary half-edge `he` (in face `f`), unit.
fn edge_outward(mesh: &Mesh, he: HalfEdgeId, f: FaceId) -> Vec3 {
    let hes = mesh.half_edges();
    let a = mesh.vertex_position(hes[he.0 as usize].origin).unwrap_or_default();
    let b = mesh.vertex_position(hes[hes[he.0 as usize].next.0 as usize].origin).unwrap_or_default();
    let n = face_normal_area(mesh, f).normalize_or_zero();
    (b - a).cross(n).normalize_or_zero()
}

/// Per boundary vertex of a group: the unit direction that moves it outward
/// along the group's surface, mitred so an offset of `d` keeps both of its
/// boundary edges `d` away (the inset / outline direction).
fn boundary_offsets(mesh: &Mesh, group: &BTreeSet<FaceId>) -> BTreeMap<VertexId, Vec3> {
    let hes = mesh.half_edges();
    let mut per_vertex: BTreeMap<VertexId, Vec<Vec3>> = BTreeMap::new();
    for he in group_boundary(mesh, group) {
        let f = hes[he.0 as usize].face.unwrap();
        let out = edge_outward(mesh, he, f);
        let a = hes[he.0 as usize].origin;
        let b = hes[hes[he.0 as usize].next.0 as usize].origin;
        per_vertex.entry(a).or_default().push(out);
        per_vertex.entry(b).or_default().push(out);
    }
    per_vertex
        .into_iter()
        .map(|(v, dirs)| {
            let sum: Vec3 = dirs.iter().sum();
            let avg = sum / dirs.len() as f32;
            let len2 = avg.length_squared().max(0.05);
            // Miter: avg has length cos(half-angle); divide by it (clamped) so the
            // edges stay `d` away.
            (v, avg / len2)
        })
        .collect()
}

/// Vertices used only by faces of the group (they move with it, no copies).
fn interior_vertices(mesh: &Mesh, group: &BTreeSet<FaceId>, boundary: &BTreeSet<VertexId>) -> BTreeSet<VertexId> {
    let mut out = BTreeSet::new();
    for f in group {
        for v in mesh.face_vertex_ids(*f).unwrap_or_default() {
            if !boundary.contains(&v) {
                out.insert(v);
            }
        }
    }
    out
}

/// Vertex normal from the group's faces only.
fn group_vertex_normal(mesh: &Mesh, group: &BTreeSet<FaceId>, v: VertexId) -> Vec3 {
    let mut n = Vec3::ZERO;
    for f in mesh.faces_around_vertex(v) {
        if group.contains(&f) {
            n += face_normal_area(mesh, f);
        }
    }
    n.normalize_or_zero()
}

/// Generic "lift" of face groups: every group's faces are replaced by copies
/// whose vertices are moved by `displace(group, v, position)`; the boundary
/// gets `rings` strips of side quads between the originals and the copies
/// (`ring_pos(group, v, p, t)` gives the position at fraction `t`). Boundary
/// vertices `displace` leaves in place (hinge) stay shared.
#[allow(clippy::type_complexity, clippy::needless_range_loop)]
fn lift(
    mesh: &mut Mesh,
    faces: &[FaceId],
    kind: ExtrudeKind,
    rings: u32,
    ring_pos: &dyn Fn(&BTreeSet<FaceId>, VertexId, Vec3, f32) -> Vec3,
) -> Result<Vec<FaceId>, String> {
    if faces.is_empty() {
        return Err("select polygons first".into());
    }
    let groups: Vec<BTreeSet<FaceId>> = match kind {
        ExtrudeKind::ByPolygon => faces.iter().map(|f| [*f].into_iter().collect()).collect(),
        _ => face_groups(mesh, faces).into_iter().map(|g| g.into_iter().collect()).collect(),
    };
    let mut im = IndexedMesh::from_mesh(mesh);
    let rings = rings.max(1) as usize;
    let mut caps = Vec::new();
    let mut side_faces: Vec<IndexedFace> = Vec::new();
    for group in &groups {
        let boundary = group_boundary(mesh, group);
        let bverts: BTreeSet<VertexId> = boundary.iter().flat_map(|he| {
            let hes = mesh.half_edges();
            [hes[he.0 as usize].origin, hes[hes[he.0 as usize].next.0 as usize].origin]
        }).collect();
        // Rings of copies per boundary vertex: ring[k][v] for k = 1..=rings.
        let mut ring_ids: Vec<BTreeMap<VertexId, u32>> = vec![BTreeMap::new(); rings + 1];
        for v in &bverts {
            let p = mesh.vertex_position(*v).unwrap_or_default();
            ring_ids[0].insert(*v, v.0);
            for k in 1..=rings {
                let q = ring_pos(group, *v, p, k as f32 / rings as f32);
                let id = if q.distance(p) < 1e-6 { v.0 } else { im.add_vertex(q, Some(v.0)) };
                ring_ids[k].insert(*v, id);
            }
        }
        // Interior vertices move with the cap.
        for v in interior_vertices(mesh, group, &bverts) {
            let p = mesh.vertex_position(v).unwrap_or_default();
            im.positions[v.0 as usize] = ring_pos(group, v, p, 1.0);
        }
        // Side strips.
        let hes = mesh.half_edges();
        for he in &boundary {
            let f = hes[he.0 as usize].face.unwrap();
            let a = hes[he.0 as usize].origin;
            let b = hes[hes[he.0 as usize].next.0 as usize].origin;
            for k in 0..rings {
                let ids = vec![ring_ids[k][&a], ring_ids[k][&b], ring_ids[k + 1][&b], ring_ids[k + 1][&a]];
                let mut dedup: Vec<u32> = Vec::new();
                for id in ids {
                    if dedup.last() != Some(&id) {
                        dedup.push(id);
                    }
                }
                while dedup.len() > 1 && dedup.first() == dedup.last() {
                    dedup.pop();
                }
                if dedup.len() >= 3 {
                    side_faces.push(IndexedFace::like(&im.faces[f.0 as usize], dedup));
                }
            }
        }
        // Caps: the group's faces with boundary vertices replaced by the top ring.
        for f in group {
            let top = &ring_ids[rings];
            let face = &mut im.faces[f.0 as usize];
            for id in &mut face.ids {
                if let Some(t) = top.get(&VertexId(*id)) {
                    *id = *t;
                }
            }
            caps.push(*f);
        }
    }
    im.faces.extend(side_faces);
    *mesh = im.build()?;
    Ok(caps)
}

/// Extrude polygons by `height` along the group normal (Group), each vertex's
/// normal (Local Normal) or each polygon's own normal (By Polygon). Returns the
/// cap polygons (same ids as the originals).
pub fn extrude_faces(mesh: &mut Mesh, faces: &[FaceId], height: f32, kind: ExtrudeKind) -> Result<Vec<FaceId>, String> {
    let snapshot = mesh.clone();
    let group_normal = |g: &BTreeSet<FaceId>| -> Vec3 { g.iter().map(|f| face_normal_area(&snapshot, *f)).sum::<Vec3>().normalize_or_zero() };
    let f = |g: &BTreeSet<FaceId>, v: VertexId, p: Vec3, t: f32| -> Vec3 {
        let n = match kind {
            ExtrudeKind::LocalNormal => group_vertex_normal(&snapshot, g, v),
            _ => group_normal(g),
        };
        p + n * height * t
    };
    lift(mesh, faces, kind, 1, &f)
}

/// Inset: the polygons shrink by `amount` inside a ring of new polygons.
pub fn inset(mesh: &mut Mesh, faces: &[FaceId], amount: f32, by_polygon: bool) -> Result<Vec<FaceId>, String> {
    let snapshot = mesh.clone();
    let kind = if by_polygon { ExtrudeKind::ByPolygon } else { ExtrudeKind::Group };
    let offsets: BTreeMap<Vec<u32>, BTreeMap<VertexId, Vec3>> = BTreeMap::new();
    let cell = std::cell::RefCell::new(offsets);
    let f = move |g: &BTreeSet<FaceId>, v: VertexId, p: Vec3, t: f32| -> Vec3 {
        let key: Vec<u32> = g.iter().map(|f| f.0).collect();
        let mut map = cell.borrow_mut();
        let offs = map.entry(key).or_insert_with(|| boundary_offsets(&snapshot, g));
        offs.get(&v).map(|d| p - *d * amount * t).unwrap_or(p)
    };
    lift(mesh, faces, kind, 1, &f)
}

/// Outline: moves the selection's boundary vertices outward by `amount`
/// (inward when negative); no new polygons.
pub fn outline(mesh: &mut Mesh, faces: &[FaceId], amount: f32) -> Result<(), String> {
    let groups: Vec<BTreeSet<FaceId>> = face_groups(mesh, faces).into_iter().map(|g| g.into_iter().collect()).collect();
    let mut im = IndexedMesh::from_mesh(mesh);
    for g in &groups {
        for (v, d) in boundary_offsets(mesh, g) {
            im.positions[v.0 as usize] += d * amount;
        }
    }
    *mesh = im.build()?;
    Ok(())
}

/// Bevel: extrude by `height`, then outline the caps by `outline_amount`.
pub fn bevel(mesh: &mut Mesh, faces: &[FaceId], height: f32, outline_amount: f32, kind: ExtrudeKind) -> Result<Vec<FaceId>, String> {
    let caps = extrude_faces(mesh, faces, height, kind)?;
    if outline_amount != 0.0 {
        match kind {
            ExtrudeKind::ByPolygon => {
                for c in &caps {
                    outline(mesh, &[*c], outline_amount)?;
                }
            }
            _ => outline(mesh, &caps, outline_amount)?,
        }
    }
    Ok(caps)
}

/// Hinge From Edge: the polygons swing `angle_deg` about `hinge` (one of
/// their edges, or any edge) in `segments` steps, with side polygons.
pub fn hinge_from_edge(mesh: &mut Mesh, faces: &[FaceId], hinge: HalfEdgeId, angle_deg: f32, segments: u32) -> Result<Vec<FaceId>, String> {
    let (a, b) = mesh.edge_vertices(hinge).ok_or("invalid hinge edge")?;
    let (pa, pb) = (mesh.vertex_position(a).unwrap(), mesh.vertex_position(b).unwrap());
    let axis = (pb - pa).normalize_or_zero();
    if axis.length_squared() < 1e-12 {
        return Err("the hinge edge has no length".into());
    }
    let f = move |_: &BTreeSet<FaceId>, _: VertexId, p: Vec3, t: f32| -> Vec3 {
        let m = Mat4::from_translation(pa) * Mat4::from_axis_angle(axis, (angle_deg * t).to_radians()) * Mat4::from_translation(-pa);
        m.transform_point3(p)
    };
    lift(mesh, faces, ExtrudeKind::Group, segments.max(1), &f)
}

/// Flip Normals: reverses the winding of whole elements. A half-edge mesh
/// needs consistent winding, so a selection that is only part of an element
/// is refused.
pub fn flip_faces(mesh: &mut Mesh, faces: &[FaceId]) -> Result<(), String> {
    let sel: BTreeSet<FaceId> = faces.iter().copied().collect();
    for f in &sel {
        for e in crate::select_ops::element_of(mesh, *f) {
            if !sel.contains(&e) {
                return Err("Flip Normals needs whole elements selected (part of one cannot be flipped on a manifold mesh)".into());
            }
        }
    }
    let mut im = IndexedMesh::from_mesh(mesh);
    for f in &sel {
        let rev: Vec<u32> = im.faces[f.0 as usize].ids.iter().rev().copied().collect();
        im.faces[f.0 as usize].set_ids(rev);
    }
    *mesh = im.build()?;
    Ok(())
}

/// Make Planar: the vertices move onto their average plane; with an axis the
/// coordinate along that axis is averaged instead (the X / Y / Z buttons).
pub fn make_planar(mesh: &mut Mesh, verts: &[VertexId], faces: &[FaceId], axis: Option<usize>) -> Result<(), String> {
    let vs: BTreeSet<VertexId> = verts.iter().copied().collect();
    if vs.is_empty() {
        return Err("nothing selected".into());
    }
    let pts: Vec<Vec3> = vs.iter().filter_map(|v| mesh.vertex_position(*v)).collect();
    let centre = pts.iter().sum::<Vec3>() / pts.len() as f32;
    let mut im = IndexedMesh::from_mesh(mesh);
    match axis {
        Some(ax) => {
            for v in &vs {
                im.positions[v.0 as usize][ax] = centre[ax];
            }
        }
        None => {
            let mut n: Vec3 = faces.iter().map(|f| face_normal_area(mesh, *f)).sum();
            if n.length_squared() < 1e-12 {
                // No faces: fit the vertices (Newell over the vertex fan order is
                // meaningless, so use the faces around them).
                for v in &vs {
                    for f in mesh.faces_around_vertex(*v) {
                        n += face_normal_area(mesh, f);
                    }
                }
            }
            let n = n.normalize_or_zero();
            if n.length_squared() < 0.5 {
                return Err("the selection has no plane".into());
            }
            for v in &vs {
                let p = im.positions[v.0 as usize];
                im.positions[v.0 as usize] = p - n * (p - centre).dot(n);
            }
        }
    }
    *mesh = im.build()?;
    Ok(())
}

/// Projects the vertices onto the plane through `origin` with `normal`
/// (View Align / Grid Align).
pub fn align_to_plane(mesh: &mut Mesh, verts: &[VertexId], origin: Vec3, normal: Vec3) -> Result<(), String> {
    let n = normal.normalize_or_zero();
    if n.length_squared() < 0.5 {
        return Err("bad plane".into());
    }
    let mut im = IndexedMesh::from_mesh(mesh);
    for v in verts {
        if let Some(p) = im.positions.get_mut(v.0 as usize) {
            *p -= n * (*p - origin).dot(n);
        }
    }
    *mesh = im.build()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::standard_primitives::{box_segmented, plane_segmented};

    fn top_faces(m: &Mesh) -> Vec<FaceId> {
        (0..m.face_count() as u32).map(FaceId).filter(|f| face_normal_area(m, *f).normalize_or_zero().z > 0.9).collect()
    }

    #[test]
    fn extrude_group_vs_by_polygon_face_counts() {
        let base = box_segmented(10.0, 10.0, 10.0, [2, 2, 1]);
        let top = top_faces(&base);
        assert_eq!(top.len(), 4);
        let mut g = base.clone();
        let caps = extrude_faces(&mut g, &top, 2.0, ExtrudeKind::Group).unwrap();
        assert_eq!(caps.len(), 4);
        // Group: 8 boundary edges → 8 sides; the centre vertex moves, no copies.
        assert_eq!(g.face_count(), base.face_count() + 8);
        assert_eq!(g.vertex_count(), base.vertex_count() + 8);
        g.check_invariants().unwrap();
        let z: f32 = caps.iter().flat_map(|c| g.face_positions(*c).unwrap()).map(|p| p.z).fold(0.0, f32::max);
        assert!((z - 12.0).abs() < 1e-4);
        let mut p = base.clone();
        extrude_faces(&mut p, &top, 2.0, ExtrudeKind::ByPolygon).unwrap();
        // By Polygon: every polygon gets 4 sides of its own.
        assert_eq!(p.face_count(), base.face_count() + 16);
        p.check_invariants().unwrap();
        let mut l = base.clone();
        extrude_faces(&mut l, &top, 2.0, ExtrudeKind::LocalNormal).unwrap();
        assert_eq!(l.face_count(), base.face_count() + 8);
        l.check_invariants().unwrap();
    }

    #[test]
    fn inset_keeps_outer_ring() {
        let mut m = box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
        let top = top_faces(&m);
        let inner = inset(&mut m, &top, 1.0, false).unwrap();
        assert_eq!(m.face_count(), 6 + 4);
        m.check_invariants().unwrap();
        let pts = m.face_positions(inner[0]).unwrap();
        for p in &pts {
            assert!((p.x.abs() - 4.0).abs() < 1e-3 && (p.y.abs() - 4.0).abs() < 1e-3, "{p:?}");
            assert!((p.z - 10.0).abs() < 1e-4);
        }
    }

    #[test]
    fn bevel_height_and_outline() {
        let mut m = box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
        let top = top_faces(&m);
        let caps = bevel(&mut m, &top, 3.0, -1.0, ExtrudeKind::Group).unwrap();
        assert_eq!(m.face_count(), 10);
        let pts = m.face_positions(caps[0]).unwrap();
        for p in &pts {
            assert!((p.z - 13.0).abs() < 1e-4);
            assert!((p.x.abs() - 4.0).abs() < 1e-3, "{p:?}");
        }
        m.check_invariants().unwrap();
    }

    #[test]
    fn hinge_rotates_about_edge() {
        let mut m = plane_segmented(10.0, 10.0, 1, 1);
        let f = FaceId(0);
        let edge = m.faces()[0].he;
        let (a, b) = m.edge_vertices(edge).unwrap();
        let (pa, pb) = (m.vertex_position(a).unwrap(), m.vertex_position(b).unwrap());
        hinge_from_edge(&mut m, &[f], edge, 90.0, 2).unwrap();
        m.check_invariants().unwrap();
        // The hinge vertices stay put and shared; the far vertices rose by 10.
        assert_eq!(m.vertex_position(a).unwrap(), pa);
        assert_eq!(m.vertex_position(b).unwrap(), pb);
        let z = m.face_positions(f).unwrap().iter().map(|p| p.z.abs()).fold(0.0, f32::max);
        assert!((z - 10.0).abs() < 1e-3, "{z}");
        // 2 segments: two strips on the three non-hinge edges, nothing on the hinge.
        assert_eq!(m.face_count(), 1 + 3 * 2);
    }

    #[test]
    fn flip_reverses_winding_and_normal() {
        let mut m = box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
        let all: Vec<FaceId> = (0..6).map(FaceId).collect();
        let n0 = face_normal_area(&m, FaceId(0));
        flip_faces(&mut m, &all).unwrap();
        assert!((face_normal_area(&m, FaceId(0)) + n0).length() < 1e-4);
        m.check_invariants().unwrap();
        assert!(flip_faces(&mut m, &[FaceId(0)]).is_err(), "part of an element cannot flip");
    }

    #[test]
    fn make_planar_z_flattens() {
        let mut m = plane_segmented(10.0, 10.0, 2, 2);
        let im = {
            let mut im = IndexedMesh::from_mesh(&m);
            im.positions[4].z = 3.0;
            im
        };
        m = im.build().unwrap();
        let all: Vec<VertexId> = (0..9).map(VertexId).collect();
        make_planar(&mut m, &all, &[], Some(2)).unwrap();
        let zs: Vec<f32> = (0..9).map(|v| m.vertex_position(VertexId(v)).unwrap().z).collect();
        assert!(zs.iter().all(|z| (z - 3.0 / 9.0).abs() < 1e-4), "{zs:?}");
        // Free plane: the fitted plane through the average.
        let faces: Vec<FaceId> = (0..4).map(FaceId).collect();
        make_planar(&mut m, &all, &faces, None).unwrap();
        let zs: Vec<f32> = (0..9).map(|v| m.vertex_position(VertexId(v)).unwrap().z).collect();
        assert!(zs.iter().all(|z| (z - zs[0]).abs() < 1e-4), "{zs:?}");
    }
}

/// Sweeps polygon groups through `rings` steps: `ring_pos(group, vertex,
/// position, t)` gives every boundary / interior vertex its position at
/// fraction `t` (Extrude Along Spline builds its frames on this).
pub fn sweep_faces(mesh: &mut Mesh, faces: &[FaceId], rings: u32, ring_pos: &dyn Fn(&BTreeSet<FaceId>, VertexId, Vec3, f32) -> Vec3) -> Result<Vec<FaceId>, String> {
    lift(mesh, faces, ExtrudeKind::Group, rings, ring_pos)
}

/// Area-weighted normal and centre of a face group.
pub fn group_frame(mesh: &Mesh, group: &BTreeSet<FaceId>) -> (Vec3, Vec3) {
    let n = group.iter().map(|f| face_normal_area(mesh, *f)).sum::<Vec3>().normalize_or_zero();
    let mut c = Vec3::ZERO;
    let mut count = 0;
    for f in group {
        for p in mesh.face_positions(*f).unwrap_or_default() {
            c += p;
            count += 1;
        }
    }
    (n, if count > 0 { c / count as f32 } else { Vec3::ZERO })
}
