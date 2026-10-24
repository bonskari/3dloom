//! Whole-mesh operations behind the mesh modifiers: Shell, Subdivide, Cap
//! Holes, STL Check, Turn To Mesh (triangulate), Face Extrude and Material
//! By Element.

use crate::indexed::{IndexedFace, IndexedMesh};
use crate::poly_ops::{self, ExtrudeKind};
use crate::select_ops::{border_edges, element_of, is_border_edge};
use crate::{FaceId, HalfEdgeId, Mesh, VertexId};
use glam::Vec3;
use std::collections::{BTreeMap, BTreeSet};

/// Area-weighted vertex normals.
pub fn vertex_normals(mesh: &Mesh) -> Vec<Vec3> {
    let mut n = vec![Vec3::ZERO; mesh.vertex_count()];
    for f in 0..mesh.face_count() {
        let id = FaceId(f as u32);
        let Some(pts) = mesh.face_positions(id) else { continue };
        let Some(ids) = mesh.face_vertex_ids(id) else { continue };
        let mut nn = Vec3::ZERO;
        for i in 0..pts.len() {
            nn += pts[i].cross(pts[(i + 1) % pts.len()]);
        }
        for v in ids {
            n[v.0 as usize] += nn;
        }
    }
    n.into_iter().map(|v| v.normalize_or_zero()).collect()
}

/// One unit normal per half-edge (boundary half-edges get zero): the
/// area-weighted average of the faces around the corner's vertex that share
/// a smoothing-group bit with the corner's face; group 0 is flat.
pub fn corner_normals(mesh: &Mesh) -> Vec<Vec3> {
    let nf = mesh.face_count();
    let face_n: Vec<Vec3> = (0..nf)
        .map(|i| {
            let pts = mesh.face_positions(FaceId(i as u32)).unwrap_or_default();
            (0..pts.len()).map(|k| pts[k].cross(pts[(k + 1) % pts.len()])).sum()
        })
        .collect();
    let mut around: Vec<Vec<usize>> = vec![Vec::new(); mesh.vertex_count()];
    for f in 0..nf {
        for v in mesh.face_vertex_ids(FaceId(f as u32)).unwrap_or_default() {
            around[v.0 as usize].push(f);
        }
    }
    let mut out = vec![Vec3::ZERO; mesh.half_edge_count()];
    for (i, he) in mesh.half_edges().iter().enumerate() {
        let Some(f) = he.face else { continue };
        if let Some(n) = mesh.explicit_normal(crate::HalfEdgeId(i as u32)) {
            out[i] = n;
            continue;
        }
        let sg = mesh.faces()[f.0 as usize].smoothing_groups;
        let n: Vec3 = if sg == 0 {
            face_n[f.0 as usize]
        } else {
            around[he.origin.0 as usize].iter().filter(|g| mesh.faces()[**g].smoothing_groups & sg != 0).map(|g| face_n[*g]).sum()
        };
        out[i] = n.normalize_or_zero();
    }
    out
}

/// Smoothing groups from a set of hard edges `(min vertex, max vertex)`:
/// faces joined across a soft edge form a region, every region gets one
/// bit, and regions touching across a hard edge or at a vertex never share it (greedy
/// 32-colouring). A mesh with no hard edges is all group 1.
pub fn smoothing_groups_from_hard_edges(mesh: &mut Mesh, hard: &std::collections::HashSet<(u32, u32)>) {
    let n = mesh.face_count();
    if n == 0 {
        return;
    }
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], i: usize) -> usize {
        let mut r = i;
        while p[r] != r {
            r = p[r];
        }
        let mut c = i;
        while p[c] != r {
            let next = p[c];
            p[c] = r;
            c = next;
        }
        r
    }
    let mut hard_pairs: Vec<(usize, usize)> = Vec::new();
    for he in mesh.half_edges() {
        let (Some(fa), Some(fb)) = (he.face, mesh.half_edges()[he.twin.0 as usize].face) else { continue };
        let (a, b) = (fa.0 as usize, fb.0 as usize);
        if a >= b {
            continue;
        }
        let va = he.origin.0;
        let vb = mesh.half_edges()[he.twin.0 as usize].origin.0;
        if hard.contains(&(va.min(vb), va.max(vb))) {
            hard_pairs.push((a, b));
        } else {
            let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
            if ra != rb {
                parent[ra] = rb;
            }
        }
    }
    let roots: Vec<usize> = (0..n).map(|i| find(&mut parent, i)).collect();
    let mut region_of: BTreeMap<usize, usize> = BTreeMap::new();
    for r in &roots {
        let k = region_of.len();
        region_of.entry(*r).or_insert(k);
    }
    let regions = region_of.len();
    let mut adjacent: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); regions];
    for (a, b) in hard_pairs {
        let (ra, rb) = (region_of[&roots[a]], region_of[&roots[b]]);
        if ra != rb {
            adjacent[ra].insert(rb);
            adjacent[rb].insert(ra);
        }
    }
    // Regions meeting only at a vertex (a pyramid's opposite sides at the
    // apex) must not share a bit either, or that vertex's normal blends them.
    for v in 0..mesh.vertex_count() {
        let around: BTreeSet<usize> = mesh.faces_around_vertex(VertexId(v as u32)).iter().map(|f| region_of[&roots[f.0 as usize]]).collect();
        for a in &around {
            for b in &around {
                if a != b {
                    adjacent[*a].insert(*b);
                }
            }
        }
    }
    let mut bit: Vec<u32> = vec![0; regions];
    for r in 0..regions {
        let used: u32 = adjacent[r].iter().map(|o| bit[*o]).fold(0, |acc, b| acc | b);
        let free = (0..32).find(|k| used & (1 << k) == 0).unwrap_or(r % 32);
        bit[r] = 1 << free;
    }
    for (f, face) in mesh.faces_mut().iter_mut().enumerate() {
        face.smoothing_groups = bit[region_of[&roots[f]]];
    }
}

/// Shell: the surface gets thickness. Outer faces move out by `outer`,
/// a flipped inner copy moves in by `inner`, and every border edge gets
/// `segments` rim quads. Returns the mesh and the face ranges
/// `(inner_faces, rim_faces)` so the caller can override material IDs.
#[allow(clippy::needless_range_loop)]
pub fn shell(mesh: &Mesh, inner: f32, outer: f32, segments: u32) -> Result<(Mesh, Vec<FaceId>, Vec<FaceId>), String> {
    let segments = segments.max(1) as usize;
    let im = IndexedMesh::from_mesh(mesh);
    let n = im.positions.len();
    let normals = vertex_normals(mesh);
    let mut out = IndexedMesh { positions: Vec::with_capacity(2 * n), vattrs: Vec::new(), faces: Vec::new(), keep: BTreeSet::new() };
    for i in 0..n {
        out.add_vertex(im.positions[i] + normals[i] * outer, Some(0));
        out.vattrs[i] = im.vattrs[i];
    }
    for i in 0..n {
        let id = out.add_vertex(im.positions[i] - normals[i] * inner, None);
        out.vattrs[id as usize] = im.vattrs[i];
    }
    for f in &im.faces {
        out.faces.push(f.clone());
    }
    let mut inner_faces = Vec::new();
    for f in &im.faces {
        let ids: Vec<u32> = f.ids.iter().rev().map(|i| i + n as u32).collect();
        inner_faces.push(FaceId(out.faces.len() as u32));
        out.faces.push(IndexedFace::reversed(f, ids));
    }
    // Rim rings between the outer (ring 0) and inner (ring `segments`) vertices.
    let mut ring: BTreeMap<(u32, usize), u32> = BTreeMap::new();
    let mut ring_id = |out: &mut IndexedMesh, v: u32, k: usize| -> u32 {
        if k == 0 {
            return v;
        }
        if k == segments {
            return v + n as u32;
        }
        *ring.entry((v, k)).or_insert_with(|| {
            let t = k as f32 / segments as f32;
            let p = out.positions[v as usize].lerp(out.positions[v as usize + n], t);
            out.add_vertex(p, Some(v))
        })
    };
    let mut rim_faces = Vec::new();
    for e in border_edges(mesh) {
        // Whichever half-edge of the pair has the face runs a→b along it.
        let hes = mesh.half_edges();
        let side = if hes[e.0 as usize].face.is_some() { e } else { hes[e.0 as usize].twin };
        let Some((a, b)) = mesh.edge_vertices(side) else { continue };
        let Some(face) = hes[side.0 as usize].face else { continue };
        let like = &im.faces[face.0 as usize];
        for k in 0..segments {
            let (a0, b0, a1, b1) = (ring_id(&mut out, a.0, k), ring_id(&mut out, b.0, k), ring_id(&mut out, a.0, k + 1), ring_id(&mut out, b.0, k + 1));
            rim_faces.push(FaceId(out.faces.len() as u32));
            out.faces.push(IndexedFace::like(like, vec![b0, a0, a1, b1]));
        }
    }
    Ok((out.build()?, inner_faces, rim_faces))
}

/// Ear-clipping triangulation of one polygon (indices into `pts`).
fn clip_ears(pts: &[Vec3], ids: &[u32]) -> Vec<[u32; 3]> {
    if ids.len() == 3 {
        return vec![[ids[0], ids[1], ids[2]]];
    }
    let mut normal = Vec3::ZERO;
    for i in 0..ids.len() {
        normal += pts[ids[i] as usize].cross(pts[ids[(i + 1) % ids.len()] as usize]);
    }
    let normal = normal.normalize_or_zero();
    let mut ring: Vec<u32> = ids.to_vec();
    let mut tris = Vec::new();
    let mut guard = 0;
    while ring.len() > 3 && guard < 10_000 {
        guard += 1;
        let m = ring.len();
        let mut clipped = false;
        for i in 0..m {
            let (a, b, c) = (ring[(i + m - 1) % m], ring[i], ring[(i + 1) % m]);
            let (pa, pb, pc) = (pts[a as usize], pts[b as usize], pts[c as usize]);
            if (pb - pa).cross(pc - pb).dot(normal) <= 1e-12 {
                continue; // reflex or degenerate corner
            }
            let inside = ring.iter().any(|v| {
                if *v == a || *v == b || *v == c {
                    return false;
                }
                let p = pts[*v as usize];
                let s0 = (pb - pa).cross(p - pa).dot(normal);
                let s1 = (pc - pb).cross(p - pb).dot(normal);
                let s2 = (pa - pc).cross(p - pc).dot(normal);
                s0 >= 0.0 && s1 >= 0.0 && s2 >= 0.0
            });
            if inside {
                continue;
            }
            tris.push([a, b, c]);
            ring.remove(i);
            clipped = true;
            break;
        }
        if !clipped {
            // Degenerate polygon: fan the rest.
            for i in 1..ring.len() - 1 {
                tris.push([ring[0], ring[i], ring[i + 1]]);
            }
            return tris;
        }
    }
    tris.push([ring[0], ring[1], ring[2]]);
    tris
}

/// Turn To Mesh: every polygon becomes triangles (ear clipping).
pub fn triangulate(mesh: &Mesh) -> Result<Mesh, String> {
    let im = IndexedMesh::from_mesh(mesh);
    let mut out = IndexedMesh { positions: im.positions.clone(), vattrs: im.vattrs.clone(), faces: Vec::new(), keep: im.keep.clone() };
    for f in &im.faces {
        for t in clip_ears(&im.positions, &f.ids) {
            out.faces.push(IndexedFace::like(f, t.to_vec()));
        }
    }
    out.build()
}

/// Subdivide: triangulates, then splits every edge longer than `size`
/// (at its midpoint, both triangles) until none is left.
pub fn subdivide(mesh: &Mesh, size: f32) -> Result<Mesh, String> {
    let size = size.max(1e-4);
    let mut im = IndexedMesh::from_mesh(&triangulate(mesh)?);
    for _ in 0..24 {
        let mut split: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        for f in &im.faces {
            for i in 0..3 {
                let (a, b) = (f.ids[i], f.ids[(i + 1) % 3]);
                let key = (a.min(b), a.max(b));
                if im.positions[a as usize].distance(im.positions[b as usize]) > size {
                    split.entry(key).or_insert(u32::MAX);
                }
            }
        }
        if split.is_empty() {
            break;
        }
        for (key, id) in split.iter_mut() {
            let p = (im.positions[key.0 as usize] + im.positions[key.1 as usize]) * 0.5;
            *id = im.add_vertex(p, Some(key.0));
        }
        let old = std::mem::take(&mut im.faces);
        for f in old {
            let mids: Vec<Option<u32>> = (0..3).map(|i| { let (a, b) = (f.ids[i], f.ids[(i + 1) % 3]); split.get(&(a.min(b), a.max(b))).copied() }).collect();
            let [a, b, c] = [f.ids[0], f.ids[1], f.ids[2]];
            let mut push = |ids: Vec<u32>| im.faces.push(IndexedFace::like_at(&f, ids, &im.positions));
            match (mids[0], mids[1], mids[2]) {
                (None, None, None) => push(vec![a, b, c]),
                (Some(m), None, None) => { push(vec![a, m, c]); push(vec![m, b, c]); }
                (None, Some(m), None) => { push(vec![a, b, m]); push(vec![a, m, c]); }
                (None, None, Some(m)) => { push(vec![a, b, m]); push(vec![m, b, c]); }
                (Some(m0), Some(m1), None) => { push(vec![a, m0, m1]); push(vec![m0, b, m1]); push(vec![a, m1, c]); }
                (None, Some(m1), Some(m2)) => { push(vec![a, b, m1]); push(vec![m1, c, m2]); push(vec![a, m1, m2]); }
                (Some(m0), None, Some(m2)) => { push(vec![a, m0, m2]); push(vec![m0, b, c]); push(vec![m0, c, m2]); }
                (Some(m0), Some(m1), Some(m2)) => { push(vec![a, m0, m2]); push(vec![m0, b, m1]); push(vec![m2, m1, c]); push(vec![m0, m1, m2]); }
            }
        }
    }
    im.build()
}

/// Cap Holes: one polygon over every border loop. Returns the cap count.
pub fn cap_holes(mesh: &mut Mesh) -> Result<usize, String> {
    let edges = border_edges(mesh);
    if edges.is_empty() {
        return Ok(0);
    }
    Ok(crate::topo_ops::cap_borders(mesh, &edges)?.len())
}

/// STL Check report: four tests. The half-edge mesh cannot hold an
/// edge with more than two faces, so Multiple Edges is always zero.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StlReport {
    pub open_edges: usize,
    pub double_faces: usize,
    pub spikes: usize,
    pub multiple_edges: usize,
    /// Faces each test flags, in test order (open edges, double faces, spikes, multiple edges).
    pub faces: [Vec<FaceId>; 4],
}

pub fn stl_check(mesh: &Mesh) -> StlReport {
    let mut r = StlReport::default();
    let hes = mesh.half_edges();
    let mut open_faces = BTreeSet::new();
    let mut border_per_face: BTreeMap<FaceId, usize> = BTreeMap::new();
    for (i, he) in hes.iter().enumerate() {
        let e = HalfEdgeId(i as u32);
        if he.face.is_none() && is_border_edge(mesh, e) {
            r.open_edges += 1;
            if let Some(f) = hes[he.twin.0 as usize].face {
                open_faces.insert(f);
                *border_per_face.entry(f).or_default() += 1;
            }
        }
    }
    r.faces[0] = open_faces.into_iter().collect();
    let mut seen: BTreeMap<Vec<u32>, FaceId> = BTreeMap::new();
    for f in 0..mesh.face_count() {
        let id = FaceId(f as u32);
        let Some(ids) = mesh.face_vertex_ids(id) else { continue };
        let mut key: Vec<u32> = ids.iter().map(|v| v.0).collect();
        key.sort();
        if let Some(first) = seen.get(&key) {
            if !r.faces[1].contains(first) {
                r.faces[1].push(*first);
            }
            r.faces[1].push(id);
        } else {
            seen.insert(key, id);
        }
    }
    r.double_faces = r.faces[1].len();
    // A spike shares only one edge with the rest of the mesh.
    for (f, borders) in border_per_face {
        let edges = mesh.face_vertex_ids(f).map(|v| v.len()).unwrap_or(0);
        if edges >= 3 && edges - borders <= 1 {
            r.faces[2].push(f);
        }
    }
    r.spikes = r.faces[2].len();
    r
}

/// Face Extrude: the faces move along their normals by `amount` and scale
/// about their centres by `scale` percent.
pub fn face_extrude(mesh: &mut Mesh, faces: &[FaceId], amount: f32, scale: f32) -> Result<Vec<FaceId>, String> {
    let tops = poly_ops::extrude_faces(mesh, faces, amount, ExtrudeKind::ByPolygon)?;
    let s = scale / 100.0;
    if (s - 1.0).abs() > 1e-6 {
        for f in &tops {
            let Some(ids) = mesh.face_vertex_ids(*f) else { continue };
            let c = ids.iter().map(|v| mesh.vertex_position(*v).unwrap_or_default()).sum::<Vec3>() / ids.len() as f32;
            for v in ids {
                let p = mesh.vertex_position(v).unwrap_or_default();
                mesh.vertices_mut()[v.0 as usize].position = c + (p - c) * s;
            }
        }
    }
    Ok(tops)
}

/// Material By Element: every element gets one material ID, cycling
/// through `ids` in element order, or random from `ids` with `seed`.
pub fn material_by_element(mesh: &mut Mesh, ids: &[u16], random: bool, seed: u32) {
    if ids.is_empty() {
        return;
    }
    let mut done: BTreeSet<FaceId> = BTreeSet::new();
    let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    let mut element = 0usize;
    for f in 0..mesh.face_count() {
        let id = FaceId(f as u32);
        if done.contains(&id) {
            continue;
        }
        let pick = if random {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            ids[(state as usize) % ids.len()]
        } else {
            ids[element % ids.len()]
        };
        element += 1;
        for e in element_of(mesh, id) {
            done.insert(e);
            mesh.faces_mut()[e.0 as usize].material_id = pick;
        }
    }
}

/// Delete Mesh: the given faces (or everything when `faces` is None).
pub fn delete_mesh(mesh: &mut Mesh, faces: Option<&[FaceId]>) -> Result<(), String> {
    match faces {
        Some(f) => crate::edit_ops::delete_faces(mesh, f),
        None => {
            *mesh = Mesh::default();
            Ok(())
        }
    }
}

/// Faces of a sub-object selection, for the modifiers that act on the
/// selection passed up the stack.
pub fn faces_of_selection(mesh: &Mesh, level: crate::select_ops::SubLevel, ids: &[u32]) -> Vec<FaceId> {
    use crate::select_ops::SubLevel;
    match level {
        SubLevel::Face | SubLevel::Element => ids.iter().map(|i| FaceId(*i)).filter(|f| (f.0 as usize) < mesh.face_count()).collect(),
        SubLevel::Vertex => {
            let mut out = BTreeSet::new();
            for v in ids {
                out.extend(mesh.faces_around_vertex(VertexId(*v)));
            }
            out.into_iter().collect()
        }
        SubLevel::Edge | SubLevel::Border => {
            let mut out = BTreeSet::new();
            for e in ids {
                let Some(he) = mesh.half_edges().get(*e as usize) else { continue };
                out.extend(he.face);
                out.extend(mesh.half_edges()[he.twin.0 as usize].face);
            }
            out.into_iter().collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{box_primitive, plane_segmented};

    #[test]
    fn shell_thickens_an_open_plane() {
        let plane = plane_segmented(10.0, 10.0, 2, 2);
        let (m, inner, rim) = shell(&plane, 1.0, 0.5, 2).unwrap();
        m.check_invariants().unwrap();
        assert_eq!(inner.len(), 4);
        assert_eq!(rim.len(), 8 * 2, "8 border edges x 2 segments");
        assert_eq!(m.face_count(), 4 + 4 + 16);
        assert!(border_edges(&m).is_empty(), "the shell is closed");
        let (lo, hi) = m.aabb();
        assert!((hi.z - 0.5).abs() < 1e-5 && (lo.z + 1.0).abs() < 1e-5, "{lo:?} {hi:?}");
        // A closed box gets a hollow interior and no rim.
        let (b, _, rim) = shell(&box_primitive(Vec3::splat(5.0)), 1.0, 0.0, 1).unwrap();
        b.check_invariants().unwrap();
        assert!(rim.is_empty());
        assert_eq!(b.face_count(), 12);
    }

    #[test]
    fn triangulate_and_subdivide_keep_the_surface() {
        let b = box_primitive(Vec3::splat(5.0));
        let t = triangulate(&b).unwrap();
        t.check_invariants().unwrap();
        assert_eq!(t.face_count(), 12);
        let s = subdivide(&b, 4.0).unwrap();
        s.check_invariants().unwrap();
        assert!(s.face_count() > 12);
        assert_eq!(s.aabb(), b.aabb());
        for i in 0..s.face_count() {
            let pts = s.face_positions(FaceId(i as u32)).unwrap();
            for j in 0..3 {
                assert!(pts[j].distance(pts[(j + 1) % 3]) <= 4.0 + 1e-4);
            }
        }
    }

    #[test]
    fn cap_holes_and_stl_check_report_open_edges() {
        let mut b = box_primitive(Vec3::splat(5.0));
        crate::edit_ops::delete_faces(&mut b, &[FaceId(0)]).unwrap();
        let r = stl_check(&b);
        assert_eq!(r.open_edges, 4);
        assert_eq!(r.faces[0].len(), 4);
        assert_eq!((r.double_faces, r.spikes), (0, 0));
        assert_eq!(cap_holes(&mut b).unwrap(), 1);
        assert_eq!(stl_check(&b).open_edges, 0);
        assert_eq!(b.face_count(), 6);
        // A lone triangle hanging off one edge is a spike.
        let mut p = plane_segmented(10.0, 10.0, 2, 2);
        let ids = p.face_vertex_ids(FaceId(0)).unwrap();
        let (a, c) = (p.vertex_position(ids[0]).unwrap(), p.vertex_position(ids[1]).unwrap());
        let mut im = IndexedMesh::from_mesh(&p);
        let tip = im.add_vertex((a + c) * 0.5 + Vec3::new(0.0, -20.0, 0.0), None);
        im.faces.push(IndexedFace::like(&im.faces[0].clone(), vec![ids[1].0, ids[0].0, tip]));
        p = im.build().unwrap();
        assert_eq!(stl_check(&p).spikes, 1);
    }

    #[test]
    fn face_extrude_scales_tops_and_material_by_element_cycles() {
        let mut b = box_primitive(Vec3::splat(5.0));
        let tops = face_extrude(&mut b, &[FaceId(0), FaceId(1)], 2.0, 50.0).unwrap();
        b.check_invariants().unwrap();
        assert_eq!(tops.len(), 2);
        let pts = b.face_positions(tops[0]).unwrap();
        let c = pts.iter().sum::<Vec3>() / pts.len() as f32;
        assert!((pts[0].distance(c) - 5.0 * 2f32.sqrt() * 0.5).abs() < 1e-3, "{pts:?}");
        let mut two = box_primitive(Vec3::splat(1.0));
        crate::geom_ops::attach(&mut two, &box_primitive(Vec3::splat(1.0)), glam::Mat4::from_translation(Vec3::X * 10.0)).unwrap();
        material_by_element(&mut two, &[3, 7], false, 0);
        let ids: BTreeSet<u16> = two.faces().iter().map(|f| f.material_id).collect();
        assert_eq!(ids.into_iter().collect::<Vec<_>>(), vec![3, 7]);
        material_by_element(&mut two, &[5], true, 1);
        assert!(two.faces().iter().all(|f| f.material_id == 5));
        let mut d = box_primitive(Vec3::splat(1.0));
        delete_mesh(&mut d, Some(&[FaceId(2)])).unwrap();
        assert_eq!(d.face_count(), 5);
        delete_mesh(&mut d, None).unwrap();
        assert_eq!(d.face_count(), 0);
    }
}
