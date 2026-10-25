//! Editable Mesh: a mesh of triangles whose edges can be invisible. A
//! polygon is the set of triangles joined by invisible edges; the
//! wireframe shows only the visible edges.
//!
//! The operations here read a mesh as triangles plus per-corner edge flags
//! (corner k flags the edge from corner k to corner k + 1), rebuild it, and
//! write the flags back, so every edit keeps the triangle invariant.

use crate::half_edge::{FaceId, HalfEdgeId, VertexId};
use crate::indexed::{IndexedFace, IndexedMesh};
use crate::Mesh;
use glam::Vec3;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

type PosKey = ([u32; 3], [u32; 3]);

fn bits(p: Vec3) -> [u32; 3] {
    [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]
}

/// An undirected edge keyed by its end positions.
fn pkey(a: Vec3, b: Vec3) -> PosKey {
    let (ka, kb) = (bits(a), bits(b));
    if ka <= kb { (ka, kb) } else { (kb, ka) }
}

/// The mesh as indexed faces plus, per face, the visibility of the edge
/// leaving each corner.
pub fn read(mesh: &Mesh) -> (IndexedMesh, Vec<Vec<bool>>) {
    let im = IndexedMesh::from_mesh(mesh);
    let vis = (0..mesh.face_count())
        .filter_map(|i| mesh.face_corners(FaceId(i as u32)))
        .map(|corners| corners.iter().map(|he| mesh.edge_visible(*he)).collect())
        .collect();
    (im, vis)
}

/// Builds the mesh and sets the edge flags: an edge is invisible when every
/// face using it marks it invisible.
pub fn build(im: &IndexedMesh, vis: &[Vec<bool>]) -> Result<Mesh, String> {
    let mut m = im.build()?;
    set_flags(&mut m, vis);
    Ok(m)
}

fn set_flags(m: &mut Mesh, vis: &[Vec<bool>]) {
    let n = m.half_edges.len();
    let mut seen_visible = vec![false; n];
    let mut seen_hidden = vec![false; n];
    for (fi, row) in vis.iter().enumerate() {
        let Some(corners) = m.face_corners(FaceId(fi as u32)) else { continue };
        if corners.len() != row.len() {
            continue;
        }
        for (k, he) in corners.iter().enumerate() {
            let t = m.half_edges[he.0 as usize].twin;
            let mark = if row[k] { &mut seen_visible } else { &mut seen_hidden };
            mark[he.0 as usize] = true;
            mark[t.0 as usize] = true;
        }
    }
    let inv: Vec<bool> = (0..n).map(|i| seen_hidden[i] && !seen_visible[i]).collect();
    // Full length even when all are visible: the mesh's flags are exact.
    m.invisible = inv;
}

/// Every face is a triangle.
pub fn is_triangles(mesh: &Mesh) -> bool {
    (0..mesh.face_count()).all(|i| mesh.face_corners(FaceId(i as u32)).is_some_and(|c| c.len() == 3))
}

/// Triangles of one polygon: a fan from the first corner when the polygon
/// is convex (the cut the viewport draws), else ear clipping.
fn triangulate_loop(pts: &[Vec3], ids: &[u32]) -> Vec<[u32; 3]> {
    let n = ids.len();
    if n == 3 {
        return vec![[ids[0], ids[1], ids[2]]];
    }
    let mut normal = Vec3::ZERO;
    for i in 0..n {
        normal += pts[ids[i] as usize].cross(pts[ids[(i + 1) % n] as usize]);
    }
    let convex = (0..n).all(|i| {
        let (a, b, c) = (pts[ids[(i + n - 1) % n] as usize], pts[ids[i] as usize], pts[ids[(i + 1) % n] as usize]);
        (b - a).cross(c - b).dot(normal) > -1e-9
    });
    if convex {
        return (1..n - 1).map(|i| [ids[0], ids[i], ids[i + 1]]).collect();
    }
    crate::mesh_mods::clip_ears(pts, ids)
}

/// Splits `face` into triangles; `edge_vis(a, b)` gives a polygon edge's
/// visibility, the cuts are invisible.
fn split_face(im: &IndexedMesh, face: &IndexedFace, edge_vis: &dyn Fn(u32, u32) -> bool, out: &mut Vec<IndexedFace>, vis: &mut Vec<Vec<bool>>) {
    let ids = &face.ids;
    let n = ids.len();
    let on_loop = |a: u32, b: u32| (0..n).any(|i| ids[i] == a && ids[(i + 1) % n] == b);
    for t in triangulate_loop(&im.positions, ids) {
        out.push(IndexedFace::same_order(face, t.to_vec()).with_uv_of(face));
        vis.push((0..3).map(|k| {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            on_loop(a, b) && edge_vis(a, b)
        }).collect());
    }
}

impl IndexedFace {
    /// The uvs of `src` for this face's corners, taken by vertex id.
    fn with_uv_of(mut self, src: &IndexedFace) -> Self {
        self.uv = src
            .uv
            .iter()
            .map(|(ch, vals)| {
                let row = self.ids.iter().map(|id| src.ids.iter().position(|s| s == id).and_then(|k| vals.get(k)).copied().unwrap_or_default()).collect();
                (*ch, row)
            })
            .collect();
        self
    }
}

/// Convert to Editable Mesh: every polygon becomes triangles; the polygon's
/// own edges stay visible, the cuts are invisible. Triangles keep the flags
/// they already carry.
pub fn from_poly(mesh: &Mesh) -> Result<Mesh, String> {
    let (im, old_vis) = read(mesh);
    let mut out = IndexedMesh { positions: im.positions.clone(), vattrs: im.vattrs.clone(), faces: Vec::new(), keep: im.keep.clone() };
    let mut vis = Vec::new();
    for (fi, f) in im.faces.iter().enumerate() {
        if f.ids.len() == 3 {
            out.faces.push(f.clone());
            vis.push(old_vis.get(fi).cloned().unwrap_or_else(|| vec![true; 3]));
            continue;
        }
        split_face(&im, f, &|_, _| true, &mut out.faces, &mut vis);
    }
    build(&out, &vis)
}

/// The result of an edit made with polygon tools, made triangles again:
/// faces that are no longer triangles are cut (cuts invisible), and an edge
/// takes its visibility from the edge at the same positions in `old`
/// (invisible there → invisible), else it is visible.
pub fn retriangulate_like(old: &Mesh, new: Mesh) -> Result<Mesh, String> {
    if new.invisible.len() == new.half_edges.len() && is_triangles(&new) {
        return Ok(new);
    }
    let hidden: HashSet<PosKey> = old.invisible_edges().into_iter().map(|(a, b)| pkey(a, b)).collect();
    let im = IndexedMesh::from_mesh(&new);
    let mut out = IndexedMesh { positions: im.positions.clone(), vattrs: im.vattrs.clone(), faces: Vec::new(), keep: im.keep.clone() };
    let mut vis = Vec::new();
    let pos = im.positions.clone();
    let edge_vis = |a: u32, b: u32| !hidden.contains(&pkey(pos[a as usize], pos[b as usize]));
    for f in &im.faces {
        split_face(&im, f, &edge_vis, &mut out.faces, &mut vis);
    }
    build(&out, &vis)
}

/// The faces adjacent across each corner edge of `f`, with that half-edge.
fn neighbours(mesh: &Mesh, f: FaceId) -> Vec<(HalfEdgeId, Option<FaceId>)> {
    mesh.face_corners(f)
        .unwrap_or_default()
        .into_iter()
        .map(|he| {
            let t = mesh.half_edges[he.0 as usize].twin;
            (he, mesh.half_edges[t.0 as usize].face)
        })
        .collect()
}

fn angle_deg(a: Vec3, b: Vec3) -> f32 {
    a.dot(b).clamp(-1.0, 1.0).acos().to_degrees()
}

/// The polygon a triangle belongs to: the faces joined to it by invisible
/// edges. With `planar` (Ignore Visible Edges) the faces joined by any edge
/// whose two faces meet at no more than that many degrees instead.
pub fn polygon_of(mesh: &Mesh, seed: FaceId, planar: Option<f32>) -> Vec<FaceId> {
    if seed.0 as usize >= mesh.face_count() {
        return Vec::new();
    }
    let mut seen = BTreeSet::from([seed]);
    let mut stack = vec![seed];
    while let Some(f) = stack.pop() {
        let nf = mesh.face_normal(f);
        for (he, g) in neighbours(mesh, f) {
            let Some(g) = g else { continue };
            if seen.contains(&g) {
                continue;
            }
            let joins = match planar {
                None => !mesh.edge_visible(he),
                Some(t) => angle_deg(nf, mesh.face_normal(g)) <= t + 1e-4,
            };
            if joins {
                seen.insert(g);
                stack.push(g);
            }
        }
    }
    seen.into_iter().collect()
}

/// Convert to Editable Poly: triangles joined by invisible edges merge into
/// one polygon. A group whose outline is not one simple loop stays as
/// triangles.
pub fn to_poly(mesh: &Mesh) -> Result<Mesh, String> {
    if !mesh.has_invisible_edges() {
        let mut m = mesh.clone();
        m.invisible.clear();
        return Ok(m);
    }
    let nf = mesh.face_count();
    let mut group: Vec<Option<usize>> = vec![None; nf];
    let mut groups: Vec<Vec<FaceId>> = Vec::new();
    for f in 0..nf {
        if group[f].is_some() {
            continue;
        }
        let g = polygon_of(mesh, FaceId(f as u32), None);
        for x in &g {
            group[x.0 as usize] = Some(groups.len());
        }
        groups.push(g);
    }
    let im = IndexedMesh::from_mesh(mesh);
    let mut out = IndexedMesh { positions: im.positions.clone(), vattrs: im.vattrs.clone(), faces: Vec::new(), keep: im.keep.clone() };
    for (gi, faces) in groups.iter().enumerate() {
        if faces.len() == 1 {
            out.faces.push(im.faces[faces[0].0 as usize].clone());
            continue;
        }
        // Outline half-edges: the face side of edges leading out of the group.
        let mut outline: BTreeMap<u32, HalfEdgeId> = BTreeMap::new();
        let mut simple = true;
        for f in faces {
            for (he, g) in neighbours(mesh, *f) {
                if g.and_then(|g| group[g.0 as usize]) == Some(gi) {
                    continue;
                }
                let o = mesh.half_edges[he.0 as usize].origin.0;
                if outline.insert(o, he).is_some() {
                    simple = false;
                }
            }
        }
        let mut loop_ids: Vec<HalfEdgeId> = Vec::new();
        if simple {
            if let Some((_, start)) = outline.iter().next() {
                let mut he = *start;
                for _ in 0..=outline.len() {
                    loop_ids.push(he);
                    let t = mesh.half_edges[he.0 as usize].twin;
                    let dest = mesh.half_edges[t.0 as usize].origin.0;
                    match outline.get(&dest) {
                        Some(n) if *n == *start => break,
                        Some(n) => he = *n,
                        None => {
                            simple = false;
                            break;
                        }
                    }
                }
            }
            simple = simple && loop_ids.len() == outline.len() && loop_ids.len() >= 3;
        }
        if !simple {
            for f in faces {
                out.faces.push(im.faces[f.0 as usize].clone());
            }
            continue;
        }
        let src = &im.faces[faces[0].0 as usize];
        let ids: Vec<u32> = loop_ids.iter().map(|he| mesh.half_edges[he.0 as usize].origin.0).collect();
        let mut uv = BTreeMap::new();
        for ch in mesh.maps().keys() {
            uv.insert(*ch, loop_ids.iter().map(|he| mesh.corner_uv(*ch, *he).unwrap_or_default()).collect());
        }
        out.faces.push(IndexedFace { ids, smoothing_groups: src.smoothing_groups, material_id: src.material_id, hidden: src.hidden, uv });
    }
    out.build()
}

/// Uv rows for `ids` from the corners of `faces` (first face holding the vertex).
fn uv_from(faces: &[&IndexedFace], ids: &[u32], extra: &HashMap<u32, BTreeMap<u32, Vec3>>) -> BTreeMap<u32, Vec<Vec3>> {
    let mut out = BTreeMap::new();
    let chans: BTreeSet<u32> = faces.iter().flat_map(|f| f.uv.keys().copied()).collect();
    for ch in chans {
        let row = ids
            .iter()
            .map(|id| {
                if let Some(v) = extra.get(id).and_then(|m| m.get(&ch)) {
                    return *v;
                }
                faces.iter().find_map(|f| f.ids.iter().position(|x| x == id).and_then(|k| f.uv.get(&ch)?.get(k).copied())).unwrap_or_default()
            })
            .collect();
        out.insert(ch, row);
    }
    out
}

fn tri(src: &IndexedFace, ids: [u32; 3], uv: BTreeMap<u32, Vec<Vec3>>) -> IndexedFace {
    IndexedFace { ids: ids.to_vec(), smoothing_groups: src.smoothing_groups, material_id: src.material_id, hidden: src.hidden, uv }
}

/// Rotates a triangle's ids (and flags) so it starts at `first`.
fn rotate_to(ids: &[u32], vis: &[bool], first: u32) -> Option<([u32; 3], [bool; 3])> {
    let k = ids.iter().position(|x| *x == first)?;
    Some(([ids[k], ids[(k + 1) % 3], ids[(k + 2) % 3]], [vis[k], vis[(k + 1) % 3], vis[(k + 2) % 3]]))
}

/// The two triangles on an edge: (face, [a, b, c]) with the edge a→b, and
/// (face, [b, a, d]) for the other side when there is one.
#[allow(clippy::type_complexity)]
fn edge_tris(mesh: &Mesh, im: &IndexedMesh, vis: &[Vec<bool>], he: HalfEdgeId) -> Result<((usize, [u32; 3], [bool; 3]), Option<(usize, [u32; 3], [bool; 3])>), String> {
    let h = mesh.half_edges.get(he.0 as usize).ok_or("no such edge")?;
    let t = &mesh.half_edges[h.twin.0 as usize];
    let (a, b) = (h.origin.0, t.origin.0);
    let side = |face: Option<FaceId>, first: u32| -> Option<(usize, [u32; 3], [bool; 3])> {
        let f = face?.0 as usize;
        let ids = &im.faces.get(f)?.ids;
        if ids.len() != 3 {
            return None;
        }
        let (r, v) = rotate_to(ids, &vis[f], first)?;
        Some((f, r, v))
    };
    let (one, two) = match (side(h.face, a), side(t.face, b)) {
        (Some(x), y) => (x, y),
        (None, Some(y)) => {
            // Only the twin side has a face: use it as the first side.
            return Ok((y, None));
        }
        (None, None) => return Err("the edge has no triangle".into()),
    };
    Ok((one, two))
}

/// Turn: the edge between two triangles becomes the other diagonal of
/// their quad; it keeps its visibility.
pub fn turn_edge(mesh: &Mesh, he: HalfEdgeId) -> Result<Mesh, String> {
    let (mut im, mut vis) = read(mesh);
    let ((f1, [a, b, c], v1), two) = edge_tris(mesh, &im, &vis, he)?;
    let (f2, [_, _, d], v2) = two.ok_or("Turn needs an edge between two faces")?;
    let (src1, src2) = (im.faces[f1].clone(), im.faces[f2].clone());
    let none = HashMap::new();
    let t = v1[0];
    im.faces[f1] = tri(&src1, [a, d, c], uv_from(&[&src1, &src2], &[a, d, c], &none));
    vis[f1] = vec![v2[1], t, v1[2]];
    im.faces[f2] = tri(&src2, [d, b, c], uv_from(&[&src2, &src1], &[d, b, c], &none));
    vis[f2] = vec![v2[2], v1[1], t];
    build(&im, &vis)
}

/// Divide (edge): a vertex at `t` along the edge splits the faces on both
/// sides; the new edges to the opposite corners are invisible. Returns the
/// mesh and the new vertex.
pub fn divide_edge(mesh: &Mesh, he: HalfEdgeId, t: f32) -> Result<(Mesh, VertexId), String> {
    let (mut im, mut vis) = read(mesh);
    let ((f1, [a, b, c], v1), two) = edge_tris(mesh, &im, &vis, he)?;
    let p = im.positions[a as usize].lerp(im.positions[b as usize], t);
    let m = im.add_vertex(p, Some(a));
    let src1 = im.faces[f1].clone();
    let mut extra: HashMap<u32, BTreeMap<u32, Vec3>> = HashMap::new();
    let mid_uv = |f: &IndexedFace, x: u32, y: u32, t: f32| -> BTreeMap<u32, Vec3> {
        f.uv.iter()
            .filter_map(|(ch, vals)| {
                let ix = f.ids.iter().position(|i| *i == x)?;
                let iy = f.ids.iter().position(|i| *i == y)?;
                Some((*ch, vals.get(ix)?.lerp(*vals.get(iy)?, t)))
            })
            .collect()
    };
    extra.insert(m, mid_uv(&src1, a, b, t));
    im.faces[f1] = tri(&src1, [a, m, c], uv_from(&[&src1], &[a, m, c], &extra));
    vis[f1] = vec![v1[0], false, v1[2]];
    im.faces.push(tri(&src1, [m, b, c], uv_from(&[&src1], &[m, b, c], &extra)));
    vis.push(vec![v1[0], v1[1], false]);
    if let Some((f2, [_, _, d], v2)) = two {
        let src2 = im.faces[f2].clone();
        let mut extra2 = HashMap::new();
        extra2.insert(m, mid_uv(&src2, a, b, t));
        im.faces[f2] = tri(&src2, [b, m, d], uv_from(&[&src2], &[b, m, d], &extra2));
        vis[f2] = vec![v2[0], false, v2[2]];
        im.faces.push(tri(&src2, [m, a, d], uv_from(&[&src2], &[m, a, d], &extra2)));
        vis.push(vec![v2[0], v2[1], false]);
    }
    let out = build(&im, &vis)?;
    Ok((out, VertexId(m)))
}

/// Divide (face): a vertex at the centre splits the triangle in three; the
/// new edges are invisible.
pub fn divide_face(mesh: &Mesh, f: FaceId) -> Result<Mesh, String> {
    let (mut im, mut vis) = read(mesh);
    let fi = f.0 as usize;
    let src = im.faces.get(fi).cloned().ok_or("no such face")?;
    if src.ids.len() != 3 {
        return Err("not a triangle".into());
    }
    let [a, b, c] = [src.ids[0], src.ids[1], src.ids[2]];
    let p = (im.positions[a as usize] + im.positions[b as usize] + im.positions[c as usize]) / 3.0;
    let m = im.add_vertex(p, Some(a));
    let mut extra: HashMap<u32, BTreeMap<u32, Vec3>> = HashMap::new();
    extra.insert(m, src.uv.iter().map(|(ch, v)| (*ch, v.iter().copied().sum::<Vec3>() / v.len().max(1) as f32)).collect());
    let v = vis[fi].clone();
    im.faces[fi] = tri(&src, [a, b, m], uv_from(&[&src], &[a, b, m], &extra));
    vis[fi] = vec![v[0], false, false];
    im.faces.push(tri(&src, [b, c, m], uv_from(&[&src], &[b, c, m], &extra)));
    vis.push(vec![v[1], false, false]);
    im.faces.push(tri(&src, [c, a, m], uv_from(&[&src], &[c, a, m], &extra)));
    vis.push(vec![v[2], false, false]);
    build(&im, &vis)
}

/// The mesh with a flag per half-edge (all visible when it had none).
fn with_flags(mesh: &Mesh) -> Mesh {
    let mut m = mesh.clone();
    if m.invisible.len() != m.half_edges.len() {
        m.invisible = vec![false; m.half_edges.len()];
    }
    m
}

/// Visible / Invisible on the given edges.
pub fn set_visibility(mesh: &Mesh, edges: &[HalfEdgeId], visible: bool) -> Mesh {
    let mut m = with_flags(mesh);
    for e in edges {
        m.set_edge_visible(*e, visible);
    }
    m
}

/// Auto Edge: which visibility changes are made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum AutoEdgeMode {
    /// Set and Clear Edge Visibility.
    #[default]
    SetAndClear,
    /// Set: only makes edges over the threshold visible.
    Set,
    /// Clear: only makes edges under the threshold invisible.
    Clear,
}

/// Auto Edge: an edge whose two faces meet at more than `threshold`
/// degrees is visible, one at or under it invisible (by `mode`). Open edges
/// stay visible.
pub fn auto_edge(mesh: &Mesh, edges: &[HalfEdgeId], threshold: f32, mode: AutoEdgeMode) -> Mesh {
    let mut m = with_flags(mesh);
    for e in edges {
        let Some(h) = mesh.half_edges.get(e.0 as usize) else { continue };
        let t = &mesh.half_edges[h.twin.0 as usize];
        let (Some(f), Some(g)) = (h.face, t.face) else {
            m.set_edge_visible(*e, true);
            continue;
        };
        let ang = angle_deg(mesh.face_normal(f), mesh.face_normal(g));
        if ang > threshold {
            if mode != AutoEdgeMode::Clear {
                m.set_edge_visible(*e, true);
            }
        } else if mode != AutoEdgeMode::Set {
            m.set_edge_visible(*e, false);
        }
    }
    m
}

/// Explode: the selected faces in groups, faces joined when they meet at
/// no more than `angle` degrees.
pub fn explode_groups(mesh: &Mesh, faces: &[FaceId], angle: f32) -> Vec<Vec<FaceId>> {
    let sel: BTreeSet<FaceId> = faces.iter().copied().filter(|f| (f.0 as usize) < mesh.face_count()).collect();
    let mut done: BTreeSet<FaceId> = BTreeSet::new();
    let mut out = Vec::new();
    for s in &sel {
        if done.contains(s) {
            continue;
        }
        let mut group = vec![*s];
        done.insert(*s);
        let mut i = 0;
        while i < group.len() {
            let f = group[i];
            i += 1;
            let nf = mesh.face_normal(f);
            for (_, g) in neighbours(mesh, f) {
                let Some(g) = g else { continue };
                if sel.contains(&g) && !done.contains(&g) && angle_deg(nf, mesh.face_normal(g)) <= angle + 1e-4 {
                    done.insert(g);
                    group.push(g);
                }
            }
        }
        group.sort();
        out.push(group);
    }
    out
}

/// Explode to Elements: each group gets its own vertices.
pub fn explode_to_elements(mesh: &Mesh, faces: &[FaceId], angle: f32) -> Result<Mesh, String> {
    let groups = explode_groups(mesh, faces, angle);
    let (mut im, vis) = read(mesh);
    for g in groups {
        let mut map: HashMap<u32, u32> = HashMap::new();
        for f in &g {
            let ids = im.faces[f.0 as usize].ids.clone();
            let new: Vec<u32> = ids
                .iter()
                .map(|id| *map.entry(*id).or_insert_with(|| {
                    let p = im.positions[*id as usize];
                    im.add_vertex(p, Some(*id))
                }))
                .collect();
            im.faces[f.0 as usize].ids = new;
        }
    }
    build(&im, &vis)
}

/// The mesh split in two: (the faces not listed, the listed faces). Both
/// keep their edge flags; vertices go with the faces that use them.
pub fn split(mesh: &Mesh, faces: &[FaceId]) -> Result<(Mesh, Mesh), String> {
    let sel: BTreeSet<usize> = faces.iter().map(|f| f.0 as usize).collect();
    let (im, vis) = read(mesh);
    let mut keep = IndexedMesh { positions: im.positions.clone(), vattrs: im.vattrs.clone(), faces: Vec::new(), keep: im.keep.clone() };
    let mut part = IndexedMesh { positions: im.positions.clone(), vattrs: im.vattrs.clone(), faces: Vec::new(), keep: BTreeSet::new() };
    let (mut kv, mut pv) = (Vec::new(), Vec::new());
    for (i, f) in im.faces.iter().enumerate() {
        if sel.contains(&i) {
            part.faces.push(f.clone());
            pv.push(vis[i].clone());
        } else {
            keep.faces.push(f.clone());
            kv.push(vis[i].clone());
        }
    }
    Ok((build(&keep, &kv)?, build(&part, &pv)?))
}

/// Attach: `other` joins `mesh` as triangles (its own edge flags kept, its
/// polygons cut), moved by `other_to_this`.
pub fn attach(mesh: &Mesh, other: &Mesh, other_to_this: glam::Mat4) -> Result<Mesh, String> {
    let other = from_poly(other)?;
    let (mut im, mut vis) = read(mesh);
    let (oim, ovis) = read(&other);
    let base = im.positions.len() as u32;
    for (i, p) in oim.positions.iter().enumerate() {
        let id = im.add_vertex(other_to_this.transform_point3(*p), None);
        im.vattrs[id as usize] = oim.vattrs[i];
        if oim.keep.contains(&(i as u32)) {
            im.keep.insert(id);
        }
    }
    for (f, v) in oim.faces.iter().zip(ovis) {
        let mut nf = f.clone();
        nf.ids = f.ids.iter().map(|i| i + base).collect();
        im.faces.push(nf);
        vis.push(v);
    }
    build(&im, &vis)
}

/// The faces' normals at a vertex, summed (unit length).
fn region_normals(mesh: &Mesh, im: &IndexedMesh, sel: &BTreeSet<usize>) -> BTreeMap<u32, Vec3> {
    let mut n: BTreeMap<u32, Vec3> = BTreeMap::new();
    for f in sel {
        let fnorm = mesh.face_normal(FaceId(*f as u32));
        for id in &im.faces[*f].ids {
            *n.entry(*id).or_default() += fnorm;
        }
    }
    for v in n.values_mut() {
        *v = v.normalize_or_zero();
    }
    n
}

/// Directed outline edges (a, b) of a face region: edges of selected faces
/// whose other side is not selected, with the face and its flag.
fn outline_edges(im: &IndexedMesh, vis: &[Vec<bool>], sel: &BTreeSet<usize>) -> Vec<(u32, u32, usize, bool)> {
    let mut directed: HashSet<(u32, u32)> = HashSet::new();
    for f in sel {
        let ids = &im.faces[*f].ids;
        for k in 0..ids.len() {
            directed.insert((ids[k], ids[(k + 1) % ids.len()]));
        }
    }
    let mut out = Vec::new();
    for f in sel {
        let ids = &im.faces[*f].ids;
        for k in 0..ids.len() {
            let (a, b) = (ids[k], ids[(k + 1) % ids.len()]);
            if !directed.contains(&(b, a)) {
                out.push((a, b, *f, vis[*f][k]));
            }
        }
    }
    out
}

/// Extrude (faces): the selected faces move `amount` along their normals as
/// one region and side faces join them to the rest (two triangles per
/// outline edge, the diagonal invisible). The faces keep their ids.
pub fn extrude(mesh: &Mesh, faces: &[FaceId], amount: f32) -> Result<Mesh, String> {
    let (mut im, mut vis) = read(mesh);
    let sel: BTreeSet<usize> = faces.iter().map(|f| f.0 as usize).filter(|f| *f < im.faces.len()).collect();
    if sel.is_empty() {
        return Err("select faces first".into());
    }
    let normals = region_normals(mesh, &im, &sel);
    let outline = outline_edges(&im, &vis, &sel);
    let mut map: BTreeMap<u32, u32> = BTreeMap::new();
    for (v, n) in &normals {
        let p = im.positions[*v as usize] + *n * amount;
        map.insert(*v, im.add_vertex(p, Some(*v)));
    }
    let cap_uv: Vec<IndexedFace> = sel.iter().map(|f| im.faces[*f].clone()).collect();
    for f in &sel {
        let ids: Vec<u32> = im.faces[*f].ids.iter().map(|id| map[id]).collect();
        im.faces[*f].ids = ids;
    }
    for (a, b, f, flag) in outline {
        let src = cap_uv.iter().find(|c| c.ids.contains(&a) && c.ids.contains(&b)).cloned().unwrap_or_else(|| im.faces[f].clone());
        let (a2, b2) = (map[&a], map[&b]);
        let mut extra: HashMap<u32, BTreeMap<u32, Vec3>> = HashMap::new();
        for (x, y) in [(a, a2), (b, b2)] {
            let row: BTreeMap<u32, Vec3> = src.uv.iter().filter_map(|(ch, vals)| Some((*ch, *vals.get(src.ids.iter().position(|i| *i == x)?)?))).collect();
            extra.insert(y, row);
        }
        im.faces.push(tri(&src, [a, b, b2], uv_from(&[&src], &[a, b, b2], &extra)));
        vis.push(vec![flag, true, false]);
        im.faces.push(tri(&src, [a, b2, a2], uv_from(&[&src], &[a, b2, a2], &extra)));
        vis.push(vec![false, flag, true]);
    }
    build(&im, &vis)
}

/// Bevel's outline step: the outline vertices of the selected faces move in
/// the faces' plane, outward for a positive `amount`.
pub fn outline(mesh: &Mesh, faces: &[FaceId], amount: f32) -> Result<Mesh, String> {
    let (im, vis) = read(mesh);
    let sel: BTreeSet<usize> = faces.iter().map(|f| f.0 as usize).filter(|f| *f < im.faces.len()).collect();
    if sel.is_empty() {
        return Err("select faces first".into());
    }
    let normals = region_normals(mesh, &im, &sel);
    let edges = outline_edges(&im, &vis, &sel);
    let mut out_dirs: HashMap<u32, Vec<Vec3>> = HashMap::new();
    for (a, b, _, _) in &edges {
        let (pa, pb) = (im.positions[*a as usize], im.positions[*b as usize]);
        let n = (normals[a] + normals[b]).normalize_or_zero();
        let d = (pb - pa).cross(n).normalize_or_zero();
        out_dirs.entry(*a).or_default().push(d);
        out_dirs.entry(*b).or_default().push(d);
    }
    let mut m = mesh.clone();
    for (v, dirs) in out_dirs {
        let sum: Vec3 = dirs.iter().copied().sum();
        let d = sum.normalize_or_zero();
        let cos = dirs.iter().map(|x| x.dot(d)).fold(1.0f32, f32::min).max(0.2);
        if let Some(vx) = m.vertices.get_mut(v as usize) {
            vx.position += d * (amount / cos);
        }
    }
    Ok(m)
}

/// Tessellate: By Edge splits each selected triangle into four at its edge
/// midpoints (neighbouring faces are cut at the shared midpoints, the cuts
/// invisible); Face-Center splits it into three at its centre. `tension`
/// (percent) lifts the new vertices along the surface normal.
pub fn tessellate(mesh: &Mesh, faces: &[FaceId], tension: f32, face_center: bool) -> Result<(Mesh, Vec<FaceId>), String> {
    let (mut im, mut vis) = read(mesh);
    let sel: BTreeSet<usize> = faces.iter().map(|f| f.0 as usize).filter(|f| *f < im.faces.len()).collect();
    if sel.is_empty() {
        return Err("select faces first".into());
    }
    let k = tension / 100.0;
    let mut new_sel: Vec<usize> = Vec::new();
    if face_center {
        for f in &sel {
            let src = im.faces[*f].clone();
            let ids = src.ids.clone();
            if ids.len() != 3 {
                continue;
            }
            let pts: Vec<Vec3> = ids.iter().map(|i| im.positions[*i as usize]).collect();
            let c = (pts[0] + pts[1] + pts[2]) / 3.0;
            let size = (pts[0].distance(pts[1]) + pts[1].distance(pts[2]) + pts[2].distance(pts[0])) / 3.0;
            let m = im.add_vertex(c + mesh.face_normal(FaceId(*f as u32)) * (k * size * 0.25), Some(ids[0]));
            let mut extra: HashMap<u32, BTreeMap<u32, Vec3>> = HashMap::new();
            extra.insert(m, src.uv.iter().map(|(ch, v)| (*ch, v.iter().copied().sum::<Vec3>() / v.len().max(1) as f32)).collect());
            let v = vis[*f].clone();
            im.faces[*f] = tri(&src, [ids[0], ids[1], m], uv_from(&[&src], &[ids[0], ids[1], m], &extra));
            vis[*f] = vec![v[0], true, true];
            new_sel.push(*f);
            for (x, y, fl) in [(ids[1], ids[2], v[1]), (ids[2], ids[0], v[2])] {
                im.faces.push(tri(&src, [x, y, m], uv_from(&[&src], &[x, y, m], &extra)));
                vis.push(vec![fl, true, true]);
                new_sel.push(im.faces.len() - 1);
            }
        }
        let out = build(&im, &vis)?;
        return Ok((out, new_sel.into_iter().map(|f| FaceId(f as u32)).collect()));
    }
    // By Edge: one midpoint per edge of a selected face.
    let key = |a: u32, b: u32| (a.min(b), a.max(b));
    let mut mids: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for f in &sel {
        let ids = im.faces[*f].ids.clone();
        for i in 0..ids.len() {
            let (a, b) = (ids[i], ids[(i + 1) % ids.len()]);
            if mids.contains_key(&key(a, b)) {
                continue;
            }
            let (pa, pb) = (im.positions[a as usize], im.positions[b as usize]);
            // Normal: the faces on the edge.
            let mut n = Vec3::ZERO;
            for (fi, face) in im.faces.iter().enumerate() {
                let fid = &face.ids;
                if (0..fid.len()).any(|j| key(fid[j], fid[(j + 1) % fid.len()]) == key(a, b)) {
                    n += mesh.face_normal(FaceId(fi as u32));
                }
            }
            let p = (pa + pb) * 0.5 + n.normalize_or_zero() * (k * pa.distance(pb) * 0.25);
            let m = im.add_vertex(p, Some(a));
            mids.insert(key(a, b), m);
        }
    }
    let old = std::mem::take(&mut im.faces);
    let old_vis = std::mem::take(&mut vis);
    for (fi, src) in old.iter().enumerate() {
        let ids = &src.ids;
        let n = ids.len();
        let mut extra: HashMap<u32, BTreeMap<u32, Vec3>> = HashMap::new();
        for i in 0..n {
            let (a, b) = (ids[i], ids[(i + 1) % n]);
            if let Some(m) = mids.get(&key(a, b)) {
                let row = src.uv.iter().map(|(ch, v)| (*ch, v[i].lerp(v[(i + 1) % n], 0.5))).collect();
                extra.insert(*m, row);
            }
        }
        let fv = &old_vis[fi];
        if sel.contains(&fi) && n == 3 {
            let (a, b, c) = (ids[0], ids[1], ids[2]);
            let (mab, mbc, mca) = (mids[&key(a, b)], mids[&key(b, c)], mids[&key(c, a)]);
            let parts: [([u32; 3], [bool; 3]); 4] = [
                ([a, mab, mca], [fv[0], true, fv[2]]),
                ([mab, b, mbc], [fv[0], fv[1], true]),
                ([mca, mbc, c], [true, fv[1], fv[2]]),
                ([mab, mbc, mca], [true, true, true]),
            ];
            for (t, v) in parts {
                im.faces.push(tri(src, t, uv_from(&[src], &t, &extra)));
                vis.push(v.to_vec());
                new_sel.push(im.faces.len() - 1);
            }
            continue;
        }
        // A face touching split edges: its loop with the midpoints, cut.
        let mut loop_ids = Vec::new();
        let mut loop_vis = Vec::new();
        for i in 0..n {
            let (a, b) = (ids[i], ids[(i + 1) % n]);
            loop_ids.push(a);
            loop_vis.push(fv[i]);
            if let Some(m) = mids.get(&key(a, b)) {
                loop_ids.push(*m);
                loop_vis.push(fv[i]);
            }
        }
        if loop_ids.len() == n {
            im.faces.push(src.clone());
            vis.push(fv.clone());
            continue;
        }
        let lf = IndexedFace { ids: loop_ids.clone(), uv: uv_from(&[src], &loop_ids, &extra), ..src.clone() };
        let ln = loop_ids.len();
        let edge_vis = |a: u32, b: u32| (0..ln).find(|j| loop_ids[*j] == a && loop_ids[(j + 1) % ln] == b).map(|j| loop_vis[j]).unwrap_or(true);
        let mut faces_out = Vec::new();
        let mut vis_out = Vec::new();
        split_face(&im, &lf, &edge_vis, &mut faces_out, &mut vis_out);
        im.faces.extend(faces_out);
        vis.extend(vis_out);
    }
    let out = build(&im, &vis)?;
    Ok((out, new_sel.into_iter().map(|f| FaceId(f as u32)).collect()))
}

/// Unify (normals): each element holding a selected face turns its faces to
/// point outward (the element's signed volume positive).
pub fn unify(mesh: &Mesh, faces: &[FaceId]) -> Result<Mesh, String> {
    let (mut im, mut vis) = read(mesh);
    let mut done: BTreeSet<FaceId> = BTreeSet::new();
    let mut changed = false;
    for f in faces {
        if done.contains(f) || f.0 as usize >= mesh.face_count() {
            continue;
        }
        let element = crate::select_ops::element_of(mesh, *f);
        done.extend(element.iter().copied());
        let mut volume = 0.0;
        for e in &element {
            let pts: Vec<Vec3> = im.faces[e.0 as usize].ids.iter().map(|i| im.positions[*i as usize]).collect();
            for k in 1..pts.len().saturating_sub(1) {
                volume += pts[0].dot(pts[k].cross(pts[k + 1]));
            }
        }
        if volume < 0.0 {
            changed = true;
            for e in &element {
                flip_one(&mut im, &mut vis, e.0 as usize);
            }
        }
    }
    if !changed {
        return Ok(mesh.clone());
    }
    build(&im, &vis)
}

fn flip_one(im: &mut IndexedMesh, vis: &mut [Vec<bool>], f: usize) {
    let face = &mut im.faces[f];
    let n = face.ids.len();
    // Reversed loop: corner k of the new loop is old corner n-1-k; the edge
    // leaving it is the old edge entering that corner.
    let old_vis = vis[f].clone();
    let rev: Vec<u32> = face.ids.iter().rev().copied().collect();
    for row in face.uv.values_mut() {
        row.reverse();
    }
    face.ids = rev;
    vis[f] = (0..n).map(|k| old_vis[(2 * n - 2 - k) % n]).collect();
}

/// Flip (normals) of whole elements.
pub fn flip(mesh: &Mesh, faces: &[FaceId]) -> Result<Mesh, String> {
    let sel: BTreeSet<FaceId> = faces.iter().copied().collect();
    for f in &sel {
        for e in crate::select_ops::element_of(mesh, *f) {
            if !sel.contains(&e) {
                return Err("Flip needs whole elements selected (part of one cannot be flipped on a manifold mesh)".into());
            }
        }
    }
    let (mut im, mut vis) = read(mesh);
    for f in &sel {
        flip_one(&mut im, &mut vis, f.0 as usize);
    }
    build(&im, &vis)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad() -> Mesh {
        crate::quad_primitive(Vec3::ZERO, Vec3::X, Vec3::Y)
    }

    #[test]
    fn convert_cuts_polygons_with_invisible_diagonals() {
        let b = crate::box_primitive(Vec3::splat(0.5));
        let m = from_poly(&b).unwrap();
        assert_eq!(m.face_count(), 12);
        assert!(is_triangles(&m));
        assert_eq!(m.invisible_edges().len(), 6, "one hidden diagonal per side");
        assert_eq!(m.unique_edges().len(), 12, "the wireframe shows the box's 12 edges");
        // Each triangle's polygon is its side's two triangles.
        for f in 0..12 {
            assert_eq!(polygon_of(&m, FaceId(f), None).len(), 2);
        }
        // Back to a poly: six quads.
        let p = to_poly(&m).unwrap();
        assert_eq!(p.face_count(), 6);
        assert!(!p.has_invisible_edges());
    }

    #[test]
    fn ignore_visible_edges_selects_by_planar_threshold() {
        let m = from_poly(&crate::box_primitive(Vec3::splat(0.5))).unwrap();
        // All edges visible: by visibility only the face itself.
        let all: Vec<HalfEdgeId> = (0..m.half_edges.len() as u32).map(HalfEdgeId).collect();
        let vis = set_visibility(&m, &all, true);
        assert_eq!(polygon_of(&vis, FaceId(0), None).len(), 1);
        // Ignore Visible Edges at 45 degrees: the coplanar pair.
        assert_eq!(polygon_of(&vis, FaceId(0), Some(45.0)).len(), 2);
        // At 90 degrees the whole box.
        assert_eq!(polygon_of(&vis, FaceId(0), Some(90.0)).len(), 12);
    }

    #[test]
    fn turn_swaps_the_diagonal_and_keeps_it_invisible() {
        let m = from_poly(&quad()).unwrap();
        let diag = (0..m.half_edges.len() as u32).map(HalfEdgeId).find(|h| !m.edge_visible(*h)).unwrap();
        let (a, b) = m.edge_vertices(diag).unwrap();
        let t = turn_edge(&m, diag).unwrap();
        assert_eq!(t.face_count(), 2);
        let hidden: Vec<HalfEdgeId> = (0..t.half_edges.len() as u32).map(HalfEdgeId).filter(|h| !t.edge_visible(*h)).collect();
        assert_eq!(hidden.len(), 2, "one invisible edge (two halves)");
        let (c, d) = t.edge_vertices(hidden[0]).unwrap();
        assert!(![a, b].contains(&c) && ![a, b].contains(&d), "the other diagonal");
        assert_eq!(t.unique_edges().len(), 4);
    }

    #[test]
    fn divide_edge_splits_both_sides_with_invisible_spokes() {
        let m = from_poly(&quad()).unwrap();
        let border = (0..m.half_edges.len() as u32).map(HalfEdgeId).find(|h| m.half_edges[h.0 as usize].face.is_some() && m.edge_visible(*h)).unwrap();
        let (d, v) = divide_edge(&m, border, 0.5).unwrap();
        assert_eq!(d.face_count(), 3);
        assert_eq!(d.vertex_count(), 5);
        assert!(v.0 == 4);
        // Visible: 4 quad sides, one of them now in two pieces.
        assert_eq!(d.unique_edges().len(), 5);
        let inner = divide_edge(&m, (0..m.half_edges.len() as u32).map(HalfEdgeId).find(|h| !m.edge_visible(*h)).unwrap(), 0.5).unwrap().0;
        assert_eq!(inner.face_count(), 4);
        assert_eq!(inner.unique_edges().len(), 4, "the quad still looks like one quad");
    }

    #[test]
    fn auto_edge_hides_coplanar_edges() {
        let m = from_poly(&crate::box_primitive(Vec3::splat(0.5))).unwrap();
        let all: Vec<HalfEdgeId> = (0..m.half_edges.len() as u32).map(HalfEdgeId).collect();
        let shown = set_visibility(&m, &all, true);
        assert_eq!(shown.unique_edges().len(), 18);
        let auto = auto_edge(&shown, &all, 24.0, AutoEdgeMode::SetAndClear);
        assert_eq!(auto.unique_edges().len(), 12);
        let set_only = auto_edge(&shown, &all, 24.0, AutoEdgeMode::Set);
        assert_eq!(set_only.unique_edges().len(), 18, "Set never hides");
    }

    #[test]
    fn extrude_and_outline_keep_triangles() {
        let m = from_poly(&crate::box_primitive(Vec3::splat(0.5))).unwrap();
        let top = polygon_of(&m, (0..12).map(FaceId).find(|f| m.face_normal(*f).z > 0.9).unwrap(), None);
        let e = extrude(&m, &top, 0.5).unwrap();
        assert!(is_triangles(&e));
        assert_eq!(e.face_count(), 12 + 8);
        let (lo, hi) = e.aabb();
        assert!((hi.z - lo.z - 1.5).abs() < 1e-4, "{lo} {hi}");
        // Side walls show as quads: 12 + 4 risers + 4 top edges.
        assert_eq!(e.unique_edges().len(), 20);
        let o = outline(&e, &top, 0.25).unwrap();
        let (lo2, hi2) = o.aabb();
        assert!((hi2.x - lo2.x - 1.5).abs() < 1e-4, "{lo2} {hi2}");
    }

    #[test]
    fn tessellate_by_edge_and_face_center() {
        let m = from_poly(&quad()).unwrap();
        let (t, sel) = tessellate(&m, &[FaceId(0)], 0.0, false).unwrap();
        assert!(is_triangles(&t));
        assert_eq!(sel.len(), 4);
        // Face 0 → 4, face 1 split at the shared midpoint → 2.
        assert_eq!(t.face_count(), 6);
        assert!(t.check_invariants().is_ok());
        let (c, sel) = tessellate(&m, &[FaceId(0), FaceId(1)], 0.0, true).unwrap();
        assert_eq!(c.face_count(), 6);
        assert_eq!(sel.len(), 6);
    }

    #[test]
    fn explode_splits_by_angle() {
        let m = from_poly(&crate::box_primitive(Vec3::splat(0.5))).unwrap();
        let all: Vec<FaceId> = (0..12).map(FaceId).collect();
        assert_eq!(explode_groups(&m, &all, 24.0).len(), 6);
        assert_eq!(explode_groups(&m, &all, 95.0).len(), 1);
        let e = explode_to_elements(&m, &all, 24.0).unwrap();
        assert_eq!(e.vertex_count(), 24);
        assert_eq!(e.face_count(), 12);
        assert_eq!(e.invisible_edges().len(), 6);
    }

    #[test]
    fn unify_turns_inside_out_elements_outward() {
        let m = from_poly(&crate::box_primitive(Vec3::splat(0.5))).unwrap();
        let all: Vec<FaceId> = (0..12).map(FaceId).collect();
        let flipped = flip(&m, &all).unwrap();
        assert_eq!(flipped.invisible_edges().len(), 6, "flags follow the flip");
        let top = (0..12).map(FaceId).find(|f| m.face_normal(*f).z > 0.9).unwrap();
        assert!(flipped.face_normal(top).z < -0.9);
        let u = unify(&flipped, &[FaceId(0)]).unwrap();
        assert!(u.face_normal(top).z > 0.9);
        assert_eq!(u.invisible_edges().len(), 6);
    }

    #[test]
    fn polygon_edits_are_made_triangles_again() {
        let m = from_poly(&crate::box_primitive(Vec3::splat(0.5))).unwrap();
        // A polygon tool's result: the box as quads again (as Delete would rebuild).
        let mut quads = to_poly(&m).unwrap();
        crate::edit_ops::delete_faces(&mut quads, &[FaceId(0)]).unwrap();
        let r = retriangulate_like(&m, quads).unwrap();
        assert!(is_triangles(&r));
        assert_eq!(r.face_count(), 10);
        assert_eq!(r.invisible_edges().len(), 5);
    }
}
