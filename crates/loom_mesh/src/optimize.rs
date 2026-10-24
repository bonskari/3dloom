//! Polygon reduction: Turn To Poly (merge coplanar faces), Optimize
//! (planar merge + collinear vertex removal) and quadric edge collapse for
//! ProOptimizer / MultiRes.

use crate::indexed::{IndexedFace, IndexedMesh};
use crate::mesh_mods::triangulate;
use crate::{FaceId, HalfEdgeId, Mesh};
use glam::{Mat4, Vec3, Vec4};
use std::collections::{BTreeMap, BTreeSet};

fn face_normal(im: &IndexedMesh, f: &IndexedFace) -> Vec3 {
    let mut n = Vec3::ZERO;
    for i in 0..f.ids.len() {
        n += im.positions[f.ids[i] as usize].cross(im.positions[f.ids[(i + 1) % f.ids.len()] as usize]);
    }
    n.normalize_or_zero()
}

/// The outer loop of a set of faces: one vertex ring, or None when the
/// region has holes / is not simply connected.
fn region_loop(im: &IndexedMesh, faces: &[usize]) -> Option<Vec<u32>> {
    let set: BTreeSet<usize> = faces.iter().copied().collect();
    let mut dir: BTreeSet<(u32, u32)> = BTreeSet::new();
    for f in faces {
        let ids = &im.faces[*f].ids;
        for i in 0..ids.len() {
            dir.insert((ids[i], ids[(i + 1) % ids.len()]));
        }
    }
    let _ = set;
    // Boundary edges: directed edges whose reverse is not in the region.
    let mut next: BTreeMap<u32, u32> = BTreeMap::new();
    let mut count = 0;
    for (a, b) in &dir {
        if !dir.contains(&(*b, *a)) {
            if next.insert(*a, *b).is_some() {
                return None; // a vertex with two outgoing boundary edges: not a simple ring
            }
            count += 1;
        }
    }
    if count < 3 {
        return None;
    }
    let start = *next.keys().next()?;
    let mut ring = vec![start];
    let mut cur = start;
    loop {
        cur = *next.get(&cur)?;
        if cur == start {
            break;
        }
        ring.push(cur);
        if ring.len() > count {
            return None;
        }
    }
    (ring.len() == count).then_some(ring)
}

fn is_convex(im: &IndexedMesh, ring: &[u32], normal: Vec3) -> bool {
    let m = ring.len();
    (0..m).all(|i| {
        let (a, b, c) = (im.positions[ring[(i + m - 1) % m] as usize], im.positions[ring[i] as usize], im.positions[ring[(i + 1) % m] as usize]);
        (b - a).cross(c - b).dot(normal) >= -1e-6
    })
}

/// Turn To Poly: adjacent faces whose normals differ by at most
/// `threshold_deg` merge into one polygon (never more than `max_size`
/// vertices when given; only convex results when `convex_only`).
pub fn turn_to_poly(mesh: &Mesh, threshold_deg: f32, max_size: Option<usize>, convex_only: bool) -> Result<Mesh, String> {
    let im = IndexedMesh::from_mesh(mesh);
    let normals: Vec<Vec3> = im.faces.iter().map(|f| face_normal(&im, f)).collect();
    let cos = threshold_deg.to_radians().cos();
    // Union-find over faces, merging greedily across shared edges.
    let mut region: Vec<usize> = (0..im.faces.len()).collect();
    let mut members: Vec<Vec<usize>> = (0..im.faces.len()).map(|i| vec![i]).collect();
    let mut by_edge: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for (fi, f) in im.faces.iter().enumerate() {
        for i in 0..f.ids.len() {
            let (a, b) = (f.ids[i], f.ids[(i + 1) % f.ids.len()]);
            by_edge.entry((a.min(b), a.max(b))).or_default().push(fi);
        }
    }
    for fs in by_edge.values() {
        if fs.len() != 2 {
            continue;
        }
        let (ra, rb) = (region[fs[0]], region[fs[1]]);
        if ra == rb || normals[fs[0]].dot(normals[fs[1]]) < cos {
            continue;
        }
        let merged: Vec<usize> = members[ra].iter().chain(members[rb].iter()).copied().collect();
        let Some(ring) = region_loop(&im, &merged) else { continue };
        if max_size.is_some_and(|m| ring.len() > m) {
            continue;
        }
        if convex_only && !is_convex(&im, &ring, normals[fs[0]]) {
            continue;
        }
        for f in &members[rb] {
            region[*f] = ra;
        }
        let moved = std::mem::take(&mut members[rb]);
        members[ra].extend(moved);
    }
    let mut out = IndexedMesh { positions: im.positions.clone(), vattrs: im.vattrs.clone(), faces: Vec::new(), keep: im.keep.clone() };
    for (r, fs) in members.iter().enumerate() {
        if fs.is_empty() {
            continue;
        }
        if fs.len() == 1 {
            out.faces.push(im.faces[fs[0]].clone());
            continue;
        }
        let ring = region_loop(&im, fs).ok_or("region lost its ring")?;
        out.faces.push(IndexedFace::like(&im.faces[r], ring));
    }
    out.build()
}

/// Optimize: Turn To Poly at `face_threshold_deg`, then every polygon corner
/// straighter than `edge_threshold_deg` goes (unless the straightened edge
/// would be longer than `max_edge_length`), and the result is triangles.
pub fn optimize(mesh: &Mesh, face_threshold_deg: f32, edge_threshold_deg: f32, max_edge_length: f32) -> Result<Mesh, String> {
    let merged = turn_to_poly(mesh, face_threshold_deg, None, false)?;
    let im = IndexedMesh::from_mesh(&merged);
    // A corner may go only if it is a corner of every face using it.
    let mut keep_vertex: BTreeSet<u32> = BTreeSet::new();
    let cos = edge_threshold_deg.to_radians().cos();
    for f in &im.faces {
        let m = f.ids.len();
        for i in 0..m {
            let (a, b, c) = (im.positions[f.ids[(i + m - 1) % m] as usize], im.positions[f.ids[i] as usize], im.positions[f.ids[(i + 1) % m] as usize]);
            let straight = (b - a).normalize_or_zero().dot((c - b).normalize_or_zero()) >= cos;
            let fits = max_edge_length <= 0.0 || a.distance(c) <= max_edge_length;
            if !(straight && fits) {
                keep_vertex.insert(f.ids[i]);
            }
        }
    }
    let mut out = IndexedMesh { positions: im.positions.clone(), vattrs: im.vattrs.clone(), faces: Vec::new(), keep: im.keep.clone() };
    for f in &im.faces {
        let ids: Vec<u32> = f.ids.iter().copied().filter(|v| keep_vertex.contains(v)).collect();
        if ids.len() >= 3 {
            out.faces.push(IndexedFace::like(f, ids));
        } else {
            out.faces.push(f.clone());
        }
    }
    triangulate(&out.build()?)
}

fn plane_quadric(a: Vec3, b: Vec3, c: Vec3) -> Mat4 {
    let n = (b - a).cross(c - a);
    let area = n.length();
    if area < 1e-12 {
        return Mat4::ZERO;
    }
    let n = n / area;
    let p = Vec4::new(n.x, n.y, n.z, -n.dot(a));
    Mat4::from_cols(p * p.x, p * p.y, p * p.z, p * p.w) * (area * 0.5)
}

fn quadric_error(q: &Mat4, p: Vec3) -> f32 {
    let v = p.extend(1.0);
    v.dot(*q * v)
}

/// Quadric error metric edge collapse (Garland–Heckbert) down to
/// `target_vertices` vertices; triangles only, no flips, borders kept.
pub fn decimate(mesh: &Mesh, target_vertices: usize) -> Result<Mesh, String> {
    let mut im = IndexedMesh::from_mesh(&triangulate(mesh)?);
    im.keep.clear();
    let n = im.positions.len();
    let mut quadrics = vec![Mat4::ZERO; n];
    for f in &im.faces {
        let q = plane_quadric(im.positions[f.ids[0] as usize], im.positions[f.ids[1] as usize], im.positions[f.ids[2] as usize]);
        for v in &f.ids {
            quadrics[*v as usize] += q;
        }
    }
    let mut border: BTreeSet<u32> = BTreeSet::new();
    let mut edge_faces: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for f in &im.faces {
        for i in 0..3 {
            let (a, b) = (f.ids[i], f.ids[(i + 1) % 3]);
            *edge_faces.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    for ((a, b), c) in &edge_faces {
        if *c == 1 {
            border.insert(*a);
            border.insert(*b);
        }
    }
    let mut alive: Vec<bool> = vec![false; n];
    for f in &im.faces {
        for v in &f.ids {
            alive[*v as usize] = true;
        }
    }
    let mut count = alive.iter().filter(|a| **a).count();
    let target = target_vertices.max(3);
    let mut banned: BTreeSet<(u32, u32)> = BTreeSet::new();
    while count > target {
        // Cheapest edge whose collapse keeps every triangle facing the same way.
        let mut best: Option<(f32, u32, u32, Vec3)> = None;
        for f in &im.faces {
            for i in 0..3 {
                let (a, b) = (f.ids[i], f.ids[(i + 1) % 3]);
                let key = (a.min(b), a.max(b));
                if banned.contains(&key) || (border.contains(&a) && border.contains(&b) && edge_faces.get(&key).copied().unwrap_or(0) != 1) {
                    continue;
                }
                let q = quadrics[a as usize] + quadrics[b as usize];
                let (pa, pb) = (im.positions[a as usize], im.positions[b as usize]);
                let candidates = if border.contains(&a) { vec![pa] } else if border.contains(&b) { vec![pb] } else { vec![(pa + pb) * 0.5, pa, pb] };
                let (p, err) = candidates.into_iter().map(|p| (p, quadric_error(&q, p))).min_by(|x, y| x.1.total_cmp(&y.1)).unwrap();
                if best.as_ref().is_none_or(|(e, ..)| err < *e) {
                    best = Some((err, a, b, p));
                }
            }
        }
        let Some((_, a, b, p)) = best else { break };
        // Flip test: faces around a or b (not shared) must keep their normal.
        let flips = im.faces.iter().any(|f| {
            let has_a = f.ids.contains(&a);
            let has_b = f.ids.contains(&b);
            if has_a == has_b {
                return false;
            }
            let before = face_normal(&im, f);
            let moved: Vec<u32> = f.ids.iter().map(|v| if *v == a || *v == b { u32::MAX } else { *v }).collect();
            let pts: Vec<Vec3> = moved.iter().map(|v| if *v == u32::MAX { p } else { im.positions[*v as usize] }).collect();
            let after = (pts[1] - pts[0]).cross(pts[2] - pts[0]);
            after.length() < 1e-12 || after.normalize().dot(before) < 0.1
        });
        if flips {
            banned.insert((a.min(b), a.max(b)));
            continue;
        }
        im.positions[a as usize] = p;
        let qb = quadrics[b as usize];
        quadrics[a as usize] += qb;
        im.remap(b, a);
        im.drop_degenerate();
        if border.contains(&b) {
            border.insert(a);
        }
        alive[b as usize] = false;
        count -= 1;
        edge_faces.clear();
        for f in &im.faces {
            for i in 0..3 {
                let (x, y) = (f.ids[i], f.ids[(i + 1) % 3]);
                *edge_faces.entry((x.min(y), x.max(y))).or_default() += 1;
            }
        }
    }
    im.build()
}

/// Vertex count of the triangulated mesh (what ProOptimizer counts).
pub fn triangle_vertex_count(mesh: &Mesh) -> usize {
    let mut used = BTreeSet::new();
    for f in 0..mesh.face_count() {
        if let Some(ids) = mesh.face_vertex_ids(FaceId(f as u32)) {
            used.extend(ids);
        }
    }
    used.len()
}

/// Edge ids of a mesh's faces, for callers that key by half-edge.
pub fn face_edges(mesh: &Mesh, f: FaceId) -> Vec<HalfEdgeId> {
    let Some(face) = mesh.faces().get(f.0 as usize) else { return Vec::new() };
    let mut out = vec![face.he];
    let mut cur = mesh.half_edges()[face.he.0 as usize].next;
    while cur != face.he {
        out.push(cur);
        cur = mesh.half_edges()[cur.0 as usize].next;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{box_segmented, sphere_segmented};

    #[test]
    fn turn_to_poly_merges_coplanar_faces_and_optimize_triangulates_them() {
        let b = box_segmented(10.0, 10.0, 10.0, [2, 2, 2]);
        assert_eq!(b.face_count(), 24);
        let p = turn_to_poly(&b, 1.0, None, false).unwrap();
        p.check_invariants().unwrap();
        assert_eq!(p.face_count(), 6, "each side becomes one polygon");
        assert_eq!(p.aabb(), b.aabb());
        let limited = turn_to_poly(&b, 1.0, Some(6), false).unwrap();
        assert!(limited.face_count() > 6);
        let o = optimize(&b, 1.0, 1.0, 0.0).unwrap();
        o.check_invariants().unwrap();
        assert_eq!(o.face_count(), 12, "a cube in triangles: {}", o.face_count());
        assert_eq!(o.aabb(), b.aabb());
    }

    #[test]
    fn decimate_reaches_the_target_without_flipping() {
        let s = sphere_segmented(5.0, 16, 0.0);
        let before = triangle_vertex_count(&s);
        let d = decimate(&s, before / 2).unwrap();
        d.check_invariants().unwrap();
        let after = triangle_vertex_count(&d);
        assert!(after <= before / 2 + 2 && after < before, "{before} → {after}");
        let (lo, hi) = d.aabb();
        assert!(hi.z <= 5.0 + 1e-3 && lo.z >= -5.0 - 1e-3);
        // Every triangle still points outward.
        for i in 0..d.face_count() {
            let f = FaceId(i as u32);
            let c = d.face_positions(f).unwrap().iter().sum::<Vec3>() / 3.0;
            assert!(d.face_normal(f).dot(c) > 0.0, "flipped face {i}");
        }
    }
}
