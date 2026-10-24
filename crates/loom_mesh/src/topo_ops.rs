//! Editable Poly > Edit Vertices / Edges / Borders operations on the
//! half-edge mesh, built through `IndexedMesh` so vertices can be shared or
//! separated on purpose.

use crate::indexed::{fan_around, fan_is_closed, IndexedFace, IndexedMesh};
use crate::select_ops::{is_border_edge, vertex_edges};
use crate::{FaceId, HalfEdgeId, Mesh, VertexId};
use glam::Vec3;
use std::collections::{BTreeMap, BTreeSet};

fn canonical(mesh: &Mesh, e: HalfEdgeId) -> Option<HalfEdgeId> {
    mesh.canonical_edge_id(e)
}

/// Vertex loop of a face starting right after `v`, excluding `v`.
fn chain_after(mesh: &Mesh, out: HalfEdgeId) -> Vec<u32> {
    let hes = mesh.half_edges();
    let v = hes[out.0 as usize].origin;
    let mut ids = Vec::new();
    let mut cur = hes[out.0 as usize].next;
    for _ in 0..hes.len() {
        let o = hes[cur.0 as usize].origin;
        if o == v {
            break;
        }
        ids.push(o.0);
        cur = hes[cur.0 as usize].next;
    }
    ids
}

/// Remove (Editable Poly > Edit Vertices): merge the polygons around each
/// vertex into one. Border vertices with a single face just leave it.
pub fn remove_vertices(mesh: &mut Mesh, verts: &[VertexId]) -> Result<usize, String> {
    let mut removed = 0;
    // Work by position so ids stay valid across rebuilds.
    let targets: Vec<Vec3> = verts.iter().filter_map(|v| mesh.vertex_position(*v)).collect();
    for target in targets {
        let Some(v) = (0..mesh.vertex_count()).map(|i| VertexId(i as u32)).find(|v| mesh.vertex_position(*v) == Some(target)) else { continue };
        let fan = fan_around(mesh, v);
        let mut im = IndexedMesh::from_mesh(mesh);
        if fan.is_empty() {
            continue;
        }
        if vertex_edges(mesh, v).len() == 2 {
            // Only two edges meet: the vertex just drops out of its polygons (the
            // Remove after Insert Vertex / Connect).
            for face in &mut im.faces {
                let kept: Vec<u32> = face.ids.iter().copied().filter(|x| *x != v.0).collect();
                face.set_ids(kept);
            }
            im.faces.retain(|f| f.ids.len() >= 3);
            *mesh = im.build().map_err(|e| format!("Remove: {e}"))?;
            removed += 1;
            continue;
        }
        let faces: Vec<usize> = fan.iter().map(|(f, _)| f.0 as usize).collect();
        // Chains in reverse fan order join end to start.
        let mut merged: Vec<u32> = Vec::new();
        for (_, out) in fan.iter().rev() {
            for id in chain_after(mesh, *out) {
                if merged.last() != Some(&id) {
                    merged.push(id);
                }
            }
        }
        if !fan_is_closed(mesh, v) {
            // Open fan: the border edges at v disappear; v itself goes.
        }
        while merged.len() > 1 && merged.first() == merged.last() {
            merged.pop();
        }
        // The merged polygon takes each corner's uv from the fan face that had it.
        let src = faces[1..].iter().fold(im.faces[faces[0]].clone(), |acc, f| merged_uv_source(&acc, &im.faces[*f]));
        let mut keep: Vec<IndexedFace> = im.faces.iter().enumerate().filter(|(i, _)| !faces.contains(i)).map(|(_, f)| f.clone()).collect();
        if merged.len() >= 3 {
            keep.push(IndexedFace::like(&src, merged));
        }
        im.faces = keep;
        *mesh = im.build().map_err(|e| format!("Remove: {e}"))?;
        removed += 1;
    }
    Ok(removed)
}

/// Break: every polygon using the vertex gets its own copy of it.
pub fn break_vertices(mesh: &mut Mesh, verts: &[VertexId]) -> Result<usize, String> {
    let mut im = IndexedMesh::from_mesh(mesh);
    let mut made = 0;
    for v in verts {
        let faces = im.faces_with(v.0);
        for fi in faces.into_iter().skip(1) {
            let copy = im.add_vertex(im.positions[v.0 as usize], Some(v.0));
            for id in &mut im.faces[fi].ids {
                if *id == v.0 {
                    *id = copy;
                    made += 1;
                }
            }
        }
    }
    *mesh = im.build()?;
    Ok(made)
}

/// Groups of faces around `v` separated by the selected edges, in fan order.
fn vertex_groups(mesh: &Mesh, v: VertexId, selected: &BTreeSet<HalfEdgeId>) -> Vec<Vec<usize>> {
    let fan = fan_around(mesh, v);
    if fan.is_empty() {
        return Vec::new();
    }
    let mut groups: Vec<Vec<usize>> = vec![vec![]];
    for (i, (f, out)) in fan.iter().enumerate() {
        groups.last_mut().unwrap().push(f.0 as usize);
        let edge = canonical(mesh, *out).unwrap_or(*out);
        let last = i + 1 == fan.len();
        if selected.contains(&edge) && !last {
            groups.push(vec![]);
        }
    }
    // A closed fan whose closing edge is unselected joins the first and last groups.
    if fan_is_closed(mesh, v) && groups.len() > 1 {
        let (_, last_out) = fan[fan.len() - 1];
        let closing = canonical(mesh, last_out).unwrap_or(last_out);
        if !selected.contains(&closing) {
            let tail = groups.pop().unwrap();
            groups[0].extend(tail);
        }
    }
    groups
}

/// Split Edges: the selected edges become open borders (the mesh tears
/// along them). Needs the selected edges to reach a border or close a loop.
pub fn split_edges(mesh: &mut Mesh, edges: &[HalfEdgeId]) -> Result<usize, String> {
    let selected: BTreeSet<HalfEdgeId> = edges.iter().filter_map(|e| canonical(mesh, *e)).collect();
    if selected.is_empty() {
        return Ok(0);
    }
    let ends: Vec<(VertexId, VertexId)> = selected.iter().filter_map(|e| mesh.edge_vertices(*e)).collect();
    let (im, _) = split_copies(mesh, &selected)?;
    let built = im.build().map_err(|_| "Split needs the selected edges to reach a border or form a loop".to_string())?;
    // An edge whose end vertices were not copied stays shared: nothing opened.
    let opened = ends.iter().filter(|(a, b)| edge_between(&built, *a, *b).or_else(|| edge_between(&built, *b, *a)).is_none_or(|e| crate::select_ops::is_border_edge(&built, e))).count();
    if opened == 0 {
        return Err("Split needs the selected edges to reach a border or form a loop".into());
    }
    *mesh = built;
    Ok(opened)
}

/// Copies every vertex touched by `selected` once per face group; returns
/// the indexed mesh and, per vertex, its groups with their vertex id.
#[allow(clippy::type_complexity)]
fn split_copies(mesh: &Mesh, selected: &BTreeSet<HalfEdgeId>) -> Result<(IndexedMesh, BTreeMap<u32, Vec<(Vec<usize>, u32)>>), String> {
    let mut im = IndexedMesh::from_mesh(mesh);
    let mut touched: BTreeSet<VertexId> = BTreeSet::new();
    for e in selected {
        if let Some((a, b)) = mesh.edge_vertices(*e) {
            touched.insert(a);
            touched.insert(b);
        }
    }
    let mut info = BTreeMap::new();
    for v in touched {
        let groups = vertex_groups(mesh, v, selected);
        let mut entries = Vec::new();
        for (gi, g) in groups.iter().enumerate() {
            let id = if gi == 0 { v.0 } else { im.add_vertex(im.positions[v.0 as usize], Some(v.0)) };
            for fi in g {
                for x in &mut im.faces[*fi].ids {
                    if *x == v.0 {
                        *x = id;
                    }
                }
            }
            entries.push((g.clone(), id));
        }
        info.insert(v.0, entries);
    }
    Ok((im, info))
}

/// Cut-vertex plan for Chamfer / Extrude (edges). At every vertex a cut
/// edge touches, the fan is split into sectors by the cut edges and each
/// sector gets one copy of the vertex, slid `amount` along the sector's
/// own edges. Where only one cut meets a closed fan the
/// vertex itself stays and the two cut-adjacent faces get copies that the
/// neighbouring faces also pick up (the chamfer polygon ends in a point).
struct CutPlan {
    im: IndexedMesh,
    /// (vertex, face) → the id that face uses at the vertex.
    own: BTreeMap<(u32, u32), u32>,
    /// Kept vertices (single cut, closed fan) → (copy of the face whose
    /// out edge is the cut, copy of the next face).
    kept: BTreeMap<u32, (u32, u32)>,
    /// Closed fans with ≥ 3 cuts: the cut edges in fan order and the
    /// sector copies (sector j lies after cut j).
    corners: BTreeMap<u32, (Vec<HalfEdgeId>, Vec<u32>)>,
}

fn cut_plan(mesh: &Mesh, selected: &BTreeSet<HalfEdgeId>, amount: f32) -> CutPlan {
    let hes = mesh.half_edges();
    let mut im = IndexedMesh::from_mesh(mesh);
    let mut own: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    let mut kept: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
    let mut corners = BTreeMap::new();
    let mut touched: BTreeSet<VertexId> = BTreeSet::new();
    for e in selected {
        if let Some((a, b)) = mesh.edge_vertices(*e) {
            touched.insert(a);
            touched.insert(b);
        }
    }
    for v in touched {
        let fan = fan_around(mesh, v);
        let k = fan.len();
        if k == 0 {
            continue;
        }
        let p = mesh.vertex_position(v).unwrap_or_default();
        let closed = fan_is_closed(mesh, v);
        let is_cut = |out: HalfEdgeId| selected.contains(&canonical(mesh, out).unwrap_or(out));
        let cut: Vec<bool> = fan.iter().map(|(_, out)| is_cut(*out)).collect();
        let edge_dir = |out: HalfEdgeId| -> Vec3 {
            let dest = hes[hes[out.0 as usize].twin.0 as usize].origin;
            (mesh.vertex_position(dest).unwrap_or(p) - p).normalize_or_zero()
        };
        // Direction of face i's other (incoming) edge at v.
        let in_dir = |i: usize| -> Vec3 {
            let (f, out) = fan[i];
            let _ = f;
            let prev = hes[out.0 as usize].prev;
            let x = hes[prev.0 as usize].origin;
            (mesh.vertex_position(x).unwrap_or(p) - p).normalize_or_zero()
        };
        let cuts: Vec<usize> = (0..k).filter(|i| cut[*i]).collect();
        let m = cuts.len();
        if m == 0 {
            continue;
        }
        if closed && m == 1 && k >= 3 {
            let i = cuts[0];
            let next = (i + 1) % k;
            let c_l = im.add_vertex(p + in_dir(i) * amount, Some(v.0));
            let c_r = im.add_vertex(p + edge_dir(fan[next].1) * amount, Some(v.0));
            for (j, (f, _)) in fan.iter().enumerate() {
                own.insert((v.0, f.0), if j == i { c_l } else if j == next { c_r } else { v.0 });
            }
            // The neighbours of the cut-adjacent faces pick up the copies next to v.
            let before = (i + k - 1) % k;
            let after = (next + 1) % k;
            let splice = |im: &mut IndexedMesh, f: FaceId, repl: Vec<u32>| {
                let face = &mut im.faces[f.0 as usize];
                if let Some(pos) = face.ids.iter().position(|x| *x == v.0) {
                    face.ids.splice(pos..=pos, repl);
                }
            };
            if before == after {
                splice(&mut im, fan[before].0, vec![c_r, v.0, c_l]);
            } else {
                splice(&mut im, fan[before].0, vec![v.0, c_l]);
                splice(&mut im, fan[after].0, vec![c_r, v.0]);
            }
            splice(&mut im, fan[i].0, vec![c_l]);
            splice(&mut im, fan[next].0, vec![c_r]);
            kept.insert(v.0, (c_l, c_r));
            continue;
        }
        // Sectors: runs of faces between consecutive cuts (and the borders of an open fan).
        let mut sector_faces: Vec<Vec<usize>> = Vec::new();
        let mut sector_dirs: Vec<Vec3> = Vec::new();
        let sector_dir = |faces: &[usize], bound_cuts: &[HalfEdgeId]| -> Vec3 {
            // Inner edges of the sector (not the bounding cuts), else the cuts' bisector.
            let mut inner: Vec<HalfEdgeId> = Vec::new();
            for &fi in faces {
                let (_, out) = fan[fi];
                let prev = hes[out.0 as usize].prev;
                let twin_prev = hes[prev.0 as usize].twin; // v → x
                for e in [out, twin_prev] {
                    let c = canonical(mesh, e).unwrap_or(e);
                    if !bound_cuts.iter().any(|b| canonical(mesh, *b).unwrap_or(*b) == c) && !inner.contains(&c) {
                        inner.push(c);
                    }
                }
            }
            let sum: Vec3 = inner.iter().map(|e| {
                let (a, b) = mesh.edge_vertices(*e).unwrap();
                let other = if a == v { b } else { a };
                (mesh.vertex_position(other).unwrap_or(p) - p).normalize_or_zero()
            }).sum();
            if sum.length_squared() > 1e-8 {
                return sum.normalize();
            }
            let bis: Vec3 = bound_cuts.iter().map(|e| {
                let (a, b) = mesh.edge_vertices(*e).unwrap();
                let other = if a == v { b } else { a };
                (mesh.vertex_position(other).unwrap_or(p) - p).normalize_or_zero()
            }).sum();
            bis.normalize_or_zero()
        };
        if closed {
            for j in 0..m {
                let start = cuts[j];
                let end = cuts[(j + 1) % m];
                let mut faces = Vec::new();
                let mut i = (start + 1) % k;
                loop {
                    faces.push(i);
                    if i == end {
                        break;
                    }
                    i = (i + 1) % k;
                }
                let bounds = [fan[start].1, fan[end].1];
                sector_dirs.push(sector_dir(&faces, &bounds));
                sector_faces.push(faces);
            }
        } else {
            let mut start = 0usize;
            for (j, &c) in cuts.iter().enumerate() {
                let faces: Vec<usize> = (start..=c).collect();
                let bounds: Vec<HalfEdgeId> = if j == 0 { vec![fan[c].1] } else { vec![fan[cuts[j - 1]].1, fan[c].1] };
                let dir = if j == 0 { in_dir(0) } else { sector_dir(&faces, &bounds) };
                sector_dirs.push(dir);
                sector_faces.push(faces);
                start = c + 1;
            }
            let faces: Vec<usize> = (start..k).collect();
            if !faces.is_empty() {
                sector_dirs.push(edge_dir(fan[k - 1].1));
                sector_faces.push(faces);
            }
        }
        let mut copies = Vec::new();
        for (faces, dir) in sector_faces.iter().zip(&sector_dirs) {
            let id = im.add_vertex(p + *dir * amount, Some(v.0));
            for &fi in faces {
                own.insert((v.0, fan[fi].0 .0), id);
                let face = &mut im.faces[fan[fi].0 .0 as usize];
                if let Some(pos) = face.ids.iter().position(|x| *x == v.0) {
                    face.ids[pos] = id;
                }
            }
            copies.push(id);
        }
        if closed && m >= 3 {
            corners.insert(v.0, (cuts.iter().map(|i| fan[*i].1).collect(), copies.clone()));
        }
    }
    CutPlan { im, own, kept, corners }
}

/// The interior of a chamfer strip's end ring at `v`, oriented from the
/// sector after the cut (in fan order) to the sector before it.
fn end_chain(mesh: &Mesh, v: VertexId, cut: HalfEdgeId, ring: &[u32]) -> Vec<u32> {
    let interior: Vec<u32> = ring[1..ring.len() - 1].to_vec();
    if mesh.half_edges()[cut.0 as usize].origin == v {
        interior.into_iter().rev().collect()
    } else {
        interior
    }
}

/// Chamfer (edges): the selected edges become strips `segments` wide; the
/// strip is `amount` away from the old edge on both sides. `open` leaves the
/// gap as a hole.
pub fn chamfer_edges(mesh: &mut Mesh, edges: &[HalfEdgeId], amount: f32, segments: u32, open: bool) -> Result<Vec<FaceId>, String> {
    let selected: BTreeSet<HalfEdgeId> = edges.iter().filter_map(|e| canonical(mesh, *e)).filter(|e| !is_border_edge(mesh, *e)).collect();
    if selected.is_empty() {
        return Err("select edges with a face on both sides".into());
    }
    let CutPlan { mut im, own, kept, corners } = cut_plan(mesh, &selected, amount);
    let first_new = im.faces.len();
    let segs = segments.max(1) as usize;
    let mut chains: BTreeMap<(u32, HalfEdgeId), Vec<u32>> = BTreeMap::new();
    for e in &selected {
        let Some((a, b)) = mesh.edge_vertices(*e) else { continue };
        let he = &mesh.half_edges()[e.0 as usize];
        let (Some(f1), Some(f2)) = (he.face, mesh.half_edges()[he.twin.0 as usize].face) else { continue };
        let (Some(a1), Some(b1), Some(a2), Some(b2)) = (own.get(&(a.0, f1.0)).copied(), own.get(&(b.0, f1.0)).copied(), own.get(&(a.0, f2.0)).copied(), own.get(&(b.0, f2.0)).copied()) else { continue };
        let ring = |im: &mut IndexedMesh, v: VertexId, c1: u32, c2: u32| -> Vec<u32> {
            let mut r = vec![c1];
            if kept.contains_key(&v.0) {
                r.extend(std::iter::repeat(v.0).take(segs - 1));
            } else {
                for s in 1..segs {
                    let t = s as f32 / segs as f32;
                    r.push(im.add_vertex(im.positions[c1 as usize].lerp(im.positions[c2 as usize], t), Some(v.0)));
                }
            }
            r.push(c2);
            r
        };
        let ring_a = ring(&mut im, a, a1, a2);
        let ring_b = ring(&mut im, b, b1, b2);
        chains.insert((a.0, *e), end_chain(mesh, a, *e, &ring_a));
        chains.insert((b.0, *e), end_chain(mesh, b, *e, &ring_b));
        if open {
            continue;
        }
        let src = IndexedFace::like(&im.faces[f1.0 as usize], vec![]);
        if segs == 1 {
            // One polygon; a kept end vertex sits in the middle of its end.
            let mut poly: Vec<u32> = vec![b2];
            if kept.contains_key(&b.0) {
                poly.push(b.0);
            }
            poly.extend([b1, a1]);
            if kept.contains_key(&a.0) {
                poly.push(a.0);
            }
            poly.push(a2);
            if let Some(poly) = dedup_loop(poly) {
                im.faces.push(IndexedFace::like(&src, poly));
            }
        } else {
            for s in 0..segs {
                let ids = vec![ring_b[s], ring_a[s], ring_a[s + 1], ring_b[s + 1]];
                if let Some(poly) = dedup_loop(ids) {
                    im.faces.push(IndexedFace::like(&src, poly));
                }
            }
        }
    }
    if !open {
        for (v, (cuts, copies)) in &corners {
            let m = cuts.len();
            let mut poly: Vec<u32> = Vec::new();
            for j in (0..m).rev() {
                poly.push(copies[j]);
                if let Some(chain) = chains.get(&(*v, canonical(mesh, cuts[j]).unwrap_or(cuts[j]))) {
                    poly.extend(chain.iter().copied());
                }
            }
            if let Some(poly) = dedup_loop(poly) {
                let src = IndexedFace::like(&im.faces[0], vec![]);
                im.faces.push(IndexedFace::like(&src, poly));
            }
        }
    }
    let new_faces: Vec<FaceId> = (first_new..im.faces.len()).map(|i| FaceId(i as u32)).collect();
    *mesh = im.build().map_err(|e| format!("Chamfer: {e}"))?;
    Ok(new_faces)
}

/// Drops consecutive duplicates (and a closing duplicate); None when fewer than three remain.
fn dedup_loop(ids: Vec<u32>) -> Option<Vec<u32>> {
    let mut out: Vec<u32> = Vec::with_capacity(ids.len());
    for id in ids {
        if out.last() != Some(&id) {
            out.push(id);
        }
    }
    while out.len() > 1 && out.first() == out.last() {
        out.pop();
    }
    (out.len() >= 3).then_some(out)
}

/// The half-edge from `a` to `b`, if the edge exists.
pub fn edge_between(mesh: &Mesh, a: VertexId, b: VertexId) -> Option<HalfEdgeId> {
    mesh.half_edges().iter().enumerate().find_map(|(i, he)| {
        let twin = &mesh.half_edges()[he.twin.0 as usize];
        (he.origin == a && twin.origin == b).then_some(HalfEdgeId(i as u32))
    })
}

/// Chamfer (vertices): each vertex becomes a polygon whose corners sit
/// `amount` along its edges (`open` leaves the hole).
pub fn chamfer_vertices(mesh: &mut Mesh, verts: &[VertexId], amount: f32, open: bool) -> Result<Vec<FaceId>, String> {
    let mut im = IndexedMesh::from_mesh(mesh);
    let mut new_faces = Vec::new();
    for v in verts {
        let fan = fan_around(mesh, *v);
        if fan.is_empty() {
            continue;
        }
        let p = mesh.vertex_position(*v).unwrap_or_default();
        let closed = fan_is_closed(mesh, *v);
        let point_along = |im: &mut IndexedMesh, dest: VertexId| -> u32 {
            let q = mesh.vertex_position(dest).unwrap_or(p);
            let d = (q - p).normalize_or_zero();
            let len = (q - p).length();
            im.add_vertex(p + d * amount.min(len * 0.49), Some(v.0))
        };
        // One ring point per fan edge (the outgoing edge of each fan entry);
        // an open fan also gets one on its first border edge.
        let mut ring: Vec<u32> = Vec::new();
        if !closed {
            let hes = mesh.half_edges();
            let prev = hes[fan[0].1 .0 as usize].prev;
            ring.push(point_along(&mut im, hes[prev.0 as usize].origin));
        }
        for (_, out) in &fan {
            let dest = mesh.half_edges()[mesh.half_edges()[out.0 as usize].twin.0 as usize].origin;
            ring.push(point_along(&mut im, dest));
        }
        let k = fan.len();
        for (i, (f, _)) in fan.iter().enumerate() {
            let face = &mut im.faces[f.0 as usize];
            let pos = face.ids.iter().position(|x| *x == v.0).unwrap();
            // Face i sits between ring point i (its incoming edge) and i+1 (its out edge);
            // closed fans wrap.
            let (before, after) = if closed { (ring[(i + k - 1) % k], ring[i]) } else { (ring[i], ring[i + 1]) };
            face.ids.splice(pos..=pos, [before, after]);
        }
        if closed && !open && ring.len() >= 3 {
            let src = IndexedFace::like(&im.faces[fan[0].0 .0 as usize], vec![]);
            new_faces.push(FaceId(im.faces.len() as u32));
            im.faces.push(IndexedFace::like(&src, ring.iter().rev().copied().collect()));
        }
    }
    *mesh = im.build().map_err(|e| format!("Chamfer: {e}"))?;
    Ok(new_faces)
}

/// Extrude (vertices): a pyramid at each vertex, `width` wide at its base
/// and `height` along the vertex normal.
pub fn extrude_vertices(mesh: &mut Mesh, verts: &[VertexId], height: f32, width: f32) -> Result<Vec<FaceId>, String> {
    let mut im = IndexedMesh::from_mesh(mesh);
    let mut new_faces = Vec::new();
    for v in verts {
        let fan = fan_around(mesh, *v);
        if fan.len() < 2 || !fan_is_closed(mesh, *v) {
            continue;
        }
        let p = mesh.vertex_position(*v).unwrap_or_default();
        let normal = fan.iter().map(|(f, _)| mesh.face_normal(*f)).sum::<Vec3>().normalize_or_zero();
        let ring: Vec<u32> = fan
            .iter()
            .map(|(_, out)| {
                let dest = mesh.half_edges()[mesh.half_edges()[out.0 as usize].twin.0 as usize].origin;
                let q = mesh.vertex_position(dest).unwrap_or(p);
                let len = (q - p).length();
                im.add_vertex(p + (q - p).normalize_or_zero() * width.min(len * 0.49), Some(v.0))
            })
            .collect();
        let k = fan.len();
        for (i, (f, _)) in fan.iter().enumerate() {
            let face = &mut im.faces[f.0 as usize];
            let pos = face.ids.iter().position(|x| *x == v.0).unwrap();
            face.ids.splice(pos..=pos, [ring[(i + k - 1) % k], ring[i]]);
        }
        let tip = im.add_vertex(p + normal * height, Some(v.0));
        let src = IndexedFace::like(&im.faces[fan[0].0 .0 as usize], vec![]);
        for i in 0..k {
            new_faces.push(FaceId(im.faces.len() as u32));
            im.faces.push(IndexedFace::like(&src, vec![ring[i], ring[(i + k - 1) % k], tip]));
        }
    }
    *mesh = im.build().map_err(|e| format!("Extrude: {e}"))?;
    Ok(new_faces)
}

/// Extrude (edges): the selected edges rise by `height` along the surface
/// normal with a base `width` wide.
pub fn extrude_edges(mesh: &mut Mesh, edges: &[HalfEdgeId], height: f32, width: f32) -> Result<Vec<FaceId>, String> {
    let selected: BTreeSet<HalfEdgeId> = edges.iter().filter_map(|e| canonical(mesh, *e)).filter(|e| !is_border_edge(mesh, *e)).collect();
    if selected.is_empty() {
        return Err("select edges with a face on both sides".into());
    }
    let CutPlan { mut im, own, kept, corners } = cut_plan(mesh, &selected, width.max(1e-4));
    let first_new = im.faces.len();
    // Ridge vertex per touched vertex: lifted along the mean normal of the cut faces there.
    let mut ridge: BTreeMap<u32, u32> = BTreeMap::new();
    let mut normals: BTreeMap<u32, Vec3> = BTreeMap::new();
    for e in &selected {
        let Some((a, b)) = mesh.edge_vertices(*e) else { continue };
        let he = &mesh.half_edges()[e.0 as usize];
        let (Some(f1), Some(f2)) = (he.face, mesh.half_edges()[he.twin.0 as usize].face) else { continue };
        let n = mesh.face_normal(f1) + mesh.face_normal(f2);
        *normals.entry(a.0).or_insert(Vec3::ZERO) += n;
        *normals.entry(b.0).or_insert(Vec3::ZERO) += n;
    }
    for (v, n) in &normals {
        let p = mesh.vertex_position(VertexId(*v)).unwrap_or_default();
        ridge.insert(*v, im.add_vertex(p + n.normalize_or_zero() * height, Some(*v)));
    }
    for e in &selected {
        let Some((a, b)) = mesh.edge_vertices(*e) else { continue };
        let he = &mesh.half_edges()[e.0 as usize];
        let (Some(f1), Some(f2)) = (he.face, mesh.half_edges()[he.twin.0 as usize].face) else { continue };
        let (Some(a1), Some(b1), Some(a2), Some(b2)) = (own.get(&(a.0, f1.0)).copied(), own.get(&(b.0, f1.0)).copied(), own.get(&(a.0, f2.0)).copied(), own.get(&(b.0, f2.0)).copied()) else { continue };
        let (ra, rb) = (ridge[&a.0], ridge[&b.0]);
        let src = IndexedFace::like(&im.faces[f1.0 as usize], vec![]);
        im.faces.push(IndexedFace::like(&src, vec![b1, a1, ra, rb]));
        im.faces.push(IndexedFace::like(&src, vec![a2, b2, rb, ra]));
    }
    // Pointed ends: two triangles close the ridge onto the kept vertex.
    for (v, (c_l, c_r)) in &kept {
        let r = ridge[v];
        let src = IndexedFace::like(&im.faces[0], vec![]);
        im.faces.push(IndexedFace::like(&src, vec![*c_l, *v, r]));
        im.faces.push(IndexedFace::like(&src, vec![*v, *c_r, r]));
    }
    // Corners with three or more ridges: a fan of triangles around the ridge vertex.
    for (v, (_, copies)) in &corners {
        let r = ridge[v];
        let src = IndexedFace::like(&im.faces[0], vec![]);
        let ids: Vec<u32> = copies.iter().rev().copied().collect();
        for i in 0..ids.len() {
            im.faces.push(IndexedFace::like(&src, vec![ids[i], ids[(i + 1) % ids.len()], r]));
        }
    }
    let new_faces: Vec<FaceId> = (first_new..im.faces.len()).map(|i| FaceId(i as u32)).collect();
    *mesh = im.build().map_err(|e| format!("Extrude: {e}"))?;
    Ok(new_faces)
}

/// Weld: selected vertices closer than `threshold` merge at their average.
/// Refuses a result that would not be manifold.
pub fn weld_vertices(mesh: &mut Mesh, verts: &[VertexId], threshold: f32) -> Result<usize, String> {
    let mut im = IndexedMesh::from_mesh(mesh);
    let mut parent: Vec<u32> = (0..im.positions.len() as u32).collect();
    fn find(p: &mut [u32], i: u32) -> u32 {
        let mut r = i;
        while p[r as usize] != r {
            r = p[r as usize];
        }
        let mut c = i;
        while p[c as usize] != r {
            let n = p[c as usize];
            p[c as usize] = r;
            c = n;
        }
        r
    }
    let sel: Vec<u32> = verts.iter().map(|v| v.0).filter(|v| (*v as usize) < im.positions.len()).collect();
    for i in 0..sel.len() {
        for j in i + 1..sel.len() {
            if im.positions[sel[i] as usize].distance(im.positions[sel[j] as usize]) <= threshold {
                let (a, b) = (find(&mut parent, sel[i]), find(&mut parent, sel[j]));
                if a != b {
                    parent[a as usize] = b;
                }
            }
        }
    }
    let mut groups: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for v in &sel {
        let r = find(&mut parent, *v);
        groups.entry(r).or_default().push(*v);
    }
    let mut welded = 0;
    for (root, members) in groups {
        if members.len() < 2 {
            continue;
        }
        let centre = members.iter().map(|m| im.positions[*m as usize]).sum::<Vec3>() / members.len() as f32;
        im.positions[root as usize] = centre;
        for m in members {
            if m != root {
                im.remap(m, root);
                welded += 1;
            }
        }
    }
    im.drop_degenerate();
    let built = im.build().map_err(|_| "Weld would make the mesh non-manifold".to_string())?;
    *mesh = built;
    Ok(welded)
}

/// Target Weld: `from` moves onto `to` and they merge.
pub fn weld_pair(mesh: &mut Mesh, from: VertexId, to: VertexId) -> Result<(), String> {
    let mut im = IndexedMesh::from_mesh(mesh);
    im.remap(from.0, to.0);
    im.drop_degenerate();
    *mesh = im.build().map_err(|_| "Target Weld would make the mesh non-manifold".to_string())?;
    Ok(())
}

/// Collapse: every connected cluster of selected vertices becomes one vertex
/// at the cluster's centre.
pub fn collapse_vertices(mesh: &mut Mesh, verts: &[VertexId]) -> Result<usize, String> {
    let sel: BTreeSet<VertexId> = verts.iter().copied().collect();
    let mut im = IndexedMesh::from_mesh(mesh);
    let mut seen: BTreeSet<VertexId> = BTreeSet::new();
    let mut collapsed = 0;
    for v in &sel {
        if seen.contains(v) {
            continue;
        }
        // Flood the cluster through edges between selected vertices.
        let mut cluster = vec![*v];
        let mut stack = vec![*v];
        seen.insert(*v);
        while let Some(x) = stack.pop() {
            for nb in crate::select_ops::vertex_neighbours(mesh, x) {
                if sel.contains(&nb) && seen.insert(nb) {
                    cluster.push(nb);
                    stack.push(nb);
                }
            }
        }
        let centre = cluster.iter().map(|c| im.positions[c.0 as usize]).sum::<Vec3>() / cluster.len() as f32;
        im.positions[cluster[0].0 as usize] = centre;
        for c in cluster.iter().skip(1) {
            im.remap(c.0, cluster[0].0);
            collapsed += 1;
        }
    }
    im.drop_degenerate();
    *mesh = im.build().map_err(|_| "Collapse would make the mesh non-manifold".to_string())?;
    Ok(collapsed)
}

/// Connect (vertices): new edges between selected vertices that share a
/// polygon without already sharing an edge.
pub fn connect_vertices(mesh: &mut Mesh, verts: &[VertexId]) -> Result<usize, String> {
    let sel: BTreeSet<u32> = verts.iter().map(|v| v.0).collect();
    let mut im = IndexedMesh::from_mesh(mesh);
    let mut made = 0;
    let mut out = Vec::new();
    for face in &im.faces {
        let mut pieces = vec![face.ids.clone()];
        let mut progressed = true;
        while progressed {
            progressed = false;
            let mut next = Vec::new();
            for ids in pieces.drain(..) {
                let n = ids.len();
                let picks: Vec<usize> = (0..n).filter(|i| sel.contains(&ids[*i])).collect();
                let mut split = None;
                'outer: for a in 0..picks.len() {
                    for b in a + 1..picks.len() {
                        let (i, j) = (picks[a], picks[b]);
                        if (j + n - i) % n > 1 && (i + n - j) % n > 1 {
                            split = Some((i, j));
                            break 'outer;
                        }
                    }
                }
                match split {
                    Some((i, j)) => {
                        let first: Vec<u32> = (i..=j).map(|k| ids[k]).collect();
                        let second: Vec<u32> = (j..=i + n).map(|k| ids[k % n]).collect();
                        next.push(first);
                        next.push(second);
                        made += 1;
                        progressed = true;
                    }
                    None => next.push(ids),
                }
            }
            pieces = next;
        }
        for ids in pieces {
            out.push(IndexedFace::like_at(face, ids, &im.positions));
        }
    }
    im.faces = out;
    *mesh = im.build()?;
    Ok(made)
}

/// Where along an edge the connect points sit (Pinch and Slide as percents).
fn connect_params(segments: u32, pinch: f32, slide: f32) -> Vec<f32> {
    let n = segments.max(1);
    (1..=n)
        .map(|i| {
            let t = i as f32 / (n + 1) as f32;
            let t = 0.5 + (t - 0.5) * (1.0 - pinch / 100.0).clamp(0.0, 2.0) + slide / 100.0 * 0.5;
            t.clamp(0.02, 0.98)
        })
        .collect()
}

/// Connect (edges): `segments` new edges across every polygon whose two
/// selected edges face each other.
pub fn connect_edges(mesh: &mut Mesh, edges: &[HalfEdgeId], segments: u32, pinch: f32, slide: f32) -> Result<Vec<(Vec3, Vec3)>, String> {
    let selected: BTreeSet<HalfEdgeId> = edges.iter().filter_map(|e| canonical(mesh, *e)).collect();
    if selected.is_empty() {
        return Err("select edges to connect".into());
    }
    let params = connect_params(segments, pinch, slide);
    let mut im = IndexedMesh::from_mesh(mesh);
    // New vertices per selected edge, keyed by its (lower, higher) vertex ids, in that direction.
    let mut points: BTreeMap<(u32, u32), Vec<u32>> = BTreeMap::new();
    for e in &selected {
        let Some((a, b)) = mesh.edge_vertices(*e) else { continue };
        let (lo, hi) = if a.0 < b.0 { (a, b) } else { (b, a) };
        let (pa, pb) = (mesh.vertex_position(lo).unwrap(), mesh.vertex_position(hi).unwrap());
        let ids: Vec<u32> = params.iter().map(|t| im.add_vertex(pa.lerp(pb, *t), Some(lo.0))).collect();
        points.insert((lo.0, hi.0), ids);
    }
    let new_segments = split_at_points(&mut im, &points);
    *mesh = im.build().map_err(|e| format!("Connect: {e}"))?;
    if new_segments.is_empty() {
        return Err("Connect needs two or more selected edges in a polygon".into());
    }
    Ok(new_segments)
}

/// Puts the new edge points (keyed by the edge's (lower, higher) vertex ids,
/// listed in that direction) into every polygon's loop and splits the
/// polygons with two or more of them. Returns the new edges' end positions.
fn split_at_points(im: &mut IndexedMesh, points: &BTreeMap<(u32, u32), Vec<u32>>) -> Vec<(Vec3, Vec3)> {
    let mut new_segments = Vec::new();
    let mut out = Vec::new();
    for face in &im.faces {
        // Insert the points into this face's loop, then connect if exactly two edges are selected.
        let n = face.ids.len();
        let mut loop_ids: Vec<u32> = Vec::new();
        let mut inserted: Vec<(usize, Vec<u32>)> = Vec::new(); // (position of first inserted id in loop_ids, ids)
        for i in 0..n {
            let (a, b) = (face.ids[i], face.ids[(i + 1) % n]);
            loop_ids.push(a);
            let key = if a < b { (a, b) } else { (b, a) };
            if let Some(ids) = points.get(&key) {
                let mut ids = ids.clone();
                if a > b {
                    ids.reverse();
                }
                inserted.push((loop_ids.len(), ids.clone()));
                loop_ids.extend(ids);
            }
        }
        if inserted.len() < 2 {
            // A polygon with one selected edge gets nothing (its midpoints stay only
            // if a neighbour connects them).
            out.push(IndexedFace::like_at(face, loop_ids, &im.positions));
            continue;
        }
        if inserted.len() > 2 {
            // Three or more selected edges: the first points of every inserted run
            // form a centre polygon; the rest of the loop becomes corner pieces.
            let m = loop_ids.len();
            let firsts: Vec<usize> = inserted.iter().map(|(p, _)| *p).collect();
            let centre: Vec<u32> = firsts.iter().map(|p| loop_ids[*p]).collect();
            for (j, &start) in firsts.iter().enumerate() {
                let end = firsts[(j + 1) % firsts.len()];
                let mut piece = Vec::new();
                let mut i = start;
                loop {
                    piece.push(loop_ids[i % m]);
                    if i % m == end % m {
                        break;
                    }
                    i += 1;
                }
                out.push(IndexedFace::like_at(face, piece, &im.positions));
            }
            for j in 0..centre.len() {
                new_segments.push((im.positions[centre[j] as usize], im.positions[centre[(j + 1) % centre.len()] as usize]));
            }
            // The pieces close with m_{j+1} → m_j, so the centre runs m_j → m_{j+1}.
            out.push(IndexedFace::like_at(face, centre, &im.positions));
            continue;
        }
        let (p0, ids0) = &inserted[0];
        let (p1, _) = &inserted[1];
        let m = loop_ids.len();
        let k = ids0.len();
        // Walk the loop: pieces between successive pairs (ids0[i], ids1[k-1-i]).
        let pair = |i: usize| -> (usize, usize) { (p0 + i, p1 + (k - 1 - i)) };
        let mut cuts: Vec<(usize, usize)> = (0..k).map(pair).collect();
        cuts.sort();
        // Pieces: from cut i to cut i+1 along the loop, both directions.
        let mut pieces: Vec<Vec<u32>> = Vec::new();
        let piece = |from: usize, to: usize| -> Vec<u32> {
            let mut v = Vec::new();
            let mut i = from;
            loop {
                v.push(loop_ids[i % m]);
                if i % m == to % m {
                    break;
                }
                i += 1;
            }
            v
        };
        // Middle strips between consecutive cuts, and the two end caps.
        let first = cuts[0];
        let last = cuts[k - 1];
        pieces.push(piece(first.1, first.0)); // cap on the side after ids1's end... loop from second cut index to first
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            // Quad-ish piece: a.0..b.0 along the loop, then b.1..a.1 along the loop.
            let mut v = piece(a.0, b.0);
            v.extend(piece(b.1, a.1));
            pieces.push(v);
        }
        pieces.push(piece(last.0, last.1));
        for ids in pieces {
            let mut ids = ids;
            ids.dedup();
            while ids.len() > 1 && ids.first() == ids.last() {
                ids.pop();
            }
            if ids.len() >= 3 {
                out.push(IndexedFace::like_at(face, ids, &im.positions));
            }
        }
        for i in 0..k {
            let (x, y) = pair(i);
            new_segments.push((im.positions[loop_ids[x % m] as usize], im.positions[loop_ids[y % m] as usize]));
        }
    }
    im.faces = out;
    new_segments
}

/// Insert Vertex on an edge at parameter `t` from its canonical origin.
pub fn insert_vertex_on_edge(mesh: &mut Mesh, edge: HalfEdgeId, t: f32) -> Result<VertexId, String> {
    let e = canonical(mesh, edge).ok_or("invalid edge")?;
    let (a, b) = mesh.edge_vertices(e).ok_or("invalid edge")?;
    let (pa, pb) = (mesh.vertex_position(a).unwrap(), mesh.vertex_position(b).unwrap());
    let mut im = IndexedMesh::from_mesh(mesh);
    let nv = im.add_vertex(pa.lerp(pb, t.clamp(0.0, 1.0)), Some(a.0));
    for face in &mut im.faces {
        let n = face.ids.len();
        if let Some(i) = (0..n).find(|i| (face.ids[*i] == a.0 && face.ids[(i + 1) % n] == b.0) || (face.ids[*i] == b.0 && face.ids[(i + 1) % n] == a.0)) {
            let mut ids = face.ids.clone();
            ids.insert(i + 1, nv);
            face.set_ids(ids);
        }
    }
    *mesh = im.build()?;
    // The new vertex is the last one used by a face.
    Ok(VertexId((mesh.vertex_count() - 1) as u32))
}

/// Insert Vertex inside a polygon: the polygon becomes a fan around it.
pub fn insert_vertex_in_face(mesh: &mut Mesh, face: FaceId, point: Vec3) -> Result<VertexId, String> {
    let mut im = IndexedMesh::from_mesh(mesh);
    let src = im.faces.get(face.0 as usize).cloned().ok_or("invalid face")?;
    let nv = im.add_vertex(point, Some(src.ids[0]));
    let n = src.ids.len();
    im.faces.remove(face.0 as usize);
    for i in 0..n {
        im.faces.push(IndexedFace::like(&src, vec![src.ids[i], src.ids[(i + 1) % n], nv]));
    }
    *mesh = im.build()?;
    Ok(VertexId((mesh.vertex_count() - 1) as u32))
}

/// Remove Isolated Vertices: drops vertices no polygon uses.
pub fn remove_isolated_vertices(mesh: &mut Mesh) -> Result<usize, String> {
    let before = mesh.vertex_count();
    let mut im = IndexedMesh::from_mesh(mesh);
    im.keep.clear();
    *mesh = im.build()?;
    Ok(before - mesh.vertex_count())
}

/// The vertex loop of the border containing `edge`, in boundary order.
pub fn border_loop(mesh: &Mesh, edge: HalfEdgeId) -> Vec<VertexId> {
    let hes = mesh.half_edges();
    let Some(e) = canonical(mesh, edge) else { return Vec::new() };
    let start = if hes[e.0 as usize].face.is_none() { e } else { hes[e.0 as usize].twin };
    if hes[start.0 as usize].face.is_some() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut cur = start;
    for _ in 0..hes.len() {
        out.push(hes[cur.0 as usize].origin);
        cur = hes[cur.0 as usize].next;
        if cur == start {
            break;
        }
    }
    out
}

/// Cap: one polygon closing each selected border loop.
pub fn cap_borders(mesh: &mut Mesh, edges: &[HalfEdgeId]) -> Result<Vec<FaceId>, String> {
    let mut loops: Vec<Vec<VertexId>> = Vec::new();
    for e in edges {
        let l = border_loop(mesh, *e);
        if l.len() >= 3 && !loops.iter().any(|x| x.contains(&l[0]) && x.len() == l.len()) {
            loops.push(l);
        }
    }
    if loops.is_empty() {
        return Err("select a border to cap".into());
    }
    let mut im = IndexedMesh::from_mesh(mesh);
    let mut new_faces = Vec::new();
    for l in loops {
        let src = im.faces.first().cloned().map(|f| IndexedFace::like(&f, vec![])).unwrap_or(IndexedFace { ids: vec![], smoothing_groups: 0, material_id: 1, hidden: false, uv: Default::default() });
        new_faces.push(FaceId(im.faces.len() as u32));
        im.faces.push(IndexedFace::like(&src, l.iter().map(|v| v.0).collect()));
    }
    *mesh = im.build().map_err(|e| format!("Cap: {e}"))?;
    Ok(new_faces)
}

/// Bridge two border loops (or two polygons, which are removed first) with
/// `segments` rings of quads.
pub fn bridge(mesh: &mut Mesh, a: &[HalfEdgeId], b: &[HalfEdgeId], faces: Option<(FaceId, FaceId)>, segments: u32, twist: i32) -> Result<Vec<FaceId>, String> {
    let (loop_a, loop_b): (Vec<u32>, Vec<u32>) = match faces {
        Some((fa, fb)) => {
            let la = mesh.face_vertex_ids(fa).ok_or("invalid face")?.into_iter().map(|v| v.0).collect();
            let lb = mesh.face_vertex_ids(fb).ok_or("invalid face")?.into_iter().map(|v| v.0).collect();
            (la, lb)
        }
        None => {
            let la = a.first().map(|e| border_loop(mesh, *e)).unwrap_or_default();
            let lb = b.first().map(|e| border_loop(mesh, *e)).unwrap_or_default();
            (la.into_iter().map(|v| v.0).collect(), lb.into_iter().map(|v| v.0).collect())
        }
    };
    if loop_a.len() < 3 || loop_b.len() < 3 {
        return Err("Bridge needs two borders or two polygons".into());
    }
    if loop_a.len() != loop_b.len() {
        return Err("Bridge needs loops with the same number of vertices".into());
    }
    let mut im = IndexedMesh::from_mesh(mesh);
    let src = faces.map(|(fa, _)| IndexedFace::like(&im.faces[fa.0 as usize], vec![])).unwrap_or_else(|| IndexedFace::like(&im.faces[0], vec![]));
    if let Some((fa, fb)) = faces {
        let mut rm = [fa.0 as usize, fb.0 as usize];
        rm.sort_unstable_by(|x, y| y.cmp(x));
        for i in rm {
            im.faces.remove(i);
        }
    }
    let n = loop_a.len();
    // Pair each a-vertex with the nearest b-vertex of a consistent winding:
    // walk b backwards so the quads face outward.
    let pa: Vec<Vec3> = loop_a.iter().map(|i| im.positions[*i as usize]).collect();
    let pb: Vec<Vec3> = loop_b.iter().map(|i| im.positions[*i as usize]).collect();
    let mut best = (f32::INFINITY, 0usize, false);
    for off in 0..n {
        for rev in [false, true] {
            let d: f32 = (0..n).map(|i| pa[i].distance(pb[if rev { (off + n - i) % n } else { (off + i) % n }])).sum();
            if d < best.0 {
                best = (d, off, rev);
            }
        }
    }
    let (_, off, rev) = best;
    // Twist shifts the pairing by whole vertices (Twist 1 / Twist 2 net effect).
    let off = (off as i64 + twist as i64).rem_euclid(n as i64) as usize;
    let bmap = |i: usize| -> usize { if rev { (off + n - i) % n } else { (off + i) % n } };
    let segs = segments.max(1) as usize;
    let mut rings: Vec<Vec<u32>> = vec![loop_a.clone()];
    for s in 1..segs {
        let t = s as f32 / segs as f32;
        rings.push((0..n).map(|i| im.add_vertex(pa[i].lerp(pb[bmap(i)], t), Some(loop_a[i]))).collect());
    }
    rings.push((0..n).map(|i| loop_b[bmap(i)]).collect());
    let first_new = im.faces.len();
    for s in 0..segs {
        for i in 0..n {
            let j = (i + 1) % n;
            im.faces.push(IndexedFace::like(&src, vec![rings[s][j], rings[s][i], rings[s + 1][i], rings[s + 1][j]]));
        }
    }
    let new_faces: Vec<FaceId> = (first_new..im.faces.len()).map(|i| FaceId(i as u32)).collect();
    match im.build() {
        Ok(m) => {
            *mesh = m;
            Ok(new_faces)
        }
        Err(_) => {
            // Other winding.
            for f in &mut im.faces[first_new..] {
                let rev: Vec<u32> = f.ids.iter().rev().copied().collect();
                f.set_ids(rev);
            }
            *mesh = im.build().map_err(|e| format!("Bridge: {e}"))?;
            Ok(new_faces)
        }
    }
}

/// Turn: flips the diagonal shared by two triangles.
pub fn turn_edge(mesh: &mut Mesh, edge: HalfEdgeId) -> Result<(), String> {
    let e = canonical(mesh, edge).ok_or("invalid edge")?;
    let he = &mesh.half_edges()[e.0 as usize];
    let (Some(f1), Some(f2)) = (he.face, mesh.half_edges()[he.twin.0 as usize].face) else { return Err("Turn needs an edge between two triangles".into()) };
    let ids1 = mesh.face_vertex_ids(f1).unwrap();
    let ids2 = mesh.face_vertex_ids(f2).unwrap();
    if ids1.len() != 3 || ids2.len() != 3 {
        return Err("Turn needs an edge between two triangles (Retriangulate first)".into());
    }
    let (a, b) = mesh.edge_vertices(e).unwrap();
    let c = *ids1.iter().find(|v| **v != a && **v != b).unwrap();
    let d = *ids2.iter().find(|v| **v != a && **v != b).unwrap();
    let mut im = IndexedMesh::from_mesh(mesh);
    // f1 has a→b; new triangles share c→d / d→c.
    let a_then_b = (0..3).any(|i| ids1[i] == a && ids1[(i + 1) % 3] == b);
    let (x, y) = if a_then_b { (a, b) } else { (b, a) };
    // The turned triangles take their corner uvs from both source triangles.
    let (src1, src2) = (im.faces[f1.0 as usize].clone(), im.faces[f2.0 as usize].clone());
    let both = merged_uv_source(&src1, &src2);
    im.faces[f1.0 as usize] = IndexedFace::like(&both, vec![x.0, d.0, c.0]);
    im.faces[f2.0 as usize] = IndexedFace::like(&both, vec![y.0, c.0, d.0]);
    *mesh = im.build().map_err(|e| format!("Turn: {e}"))?;
    Ok(())
}

/// Create: a new polygon from vertices (the winding that keeps the mesh
/// manifold wins; a detached polygon takes the given order).
pub fn create_polygon(mesh: &mut Mesh, verts: &[VertexId]) -> Result<FaceId, String> {
    if verts.len() < 3 {
        return Err("a polygon needs at least three vertices".into());
    }
    let mut im = IndexedMesh::from_mesh(mesh);
    let src = im.faces.first().map(|f| IndexedFace::like(f, vec![])).unwrap_or(IndexedFace { ids: vec![], smoothing_groups: 0, material_id: 1, hidden: false, uv: Default::default() });
    let fid = FaceId(im.faces.len() as u32);
    im.faces.push(IndexedFace::like(&src, verts.iter().map(|v| v.0).collect()));
    if let Ok(m) = im.build() {
        *mesh = m;
        return Ok(fid);
    }
    let last = im.faces.last_mut().unwrap();
    let rev: Vec<u32> = last.ids.iter().rev().copied().collect();
    last.set_ids(rev);
    *mesh = im.build().map_err(|e| format!("Create: {e}"))?;
    Ok(fid)
}

/// Vertices that touch no polygon.
pub fn isolated_vertices(mesh: &Mesh) -> Vec<VertexId> {
    (0..mesh.vertex_count() as u32).map(VertexId).filter(|v| vertex_edges(mesh, *v).is_empty()).collect()
}

/// The selected border edges as runs: each run is the faceless half-edges of
/// one connected stretch of a border, in border order, and whether it is the
/// whole border loop. Edges with polygons on both sides are left out. Runs
/// come in increasing order of their lowest edge id.
pub fn border_runs(mesh: &Mesh, edges: &[HalfEdgeId]) -> Vec<(Vec<HalfEdgeId>, bool)> {
    let hes = mesh.half_edges();
    let mut set: BTreeSet<HalfEdgeId> = BTreeSet::new();
    for e in edges {
        let Some(c) = canonical(mesh, *e) else { continue };
        let h = if hes[c.0 as usize].face.is_none() { c } else { hes[c.0 as usize].twin };
        if hes[h.0 as usize].face.is_none() && hes[hes[h.0 as usize].twin.0 as usize].face.is_some() {
            set.insert(h);
        }
    }
    let prev: BTreeMap<HalfEdgeId, HalfEdgeId> = set.iter().map(|h| (hes[h.0 as usize].next, *h)).collect();
    let mut seen: BTreeSet<HalfEdgeId> = BTreeSet::new();
    let mut runs: Vec<(HalfEdgeId, Vec<HalfEdgeId>, bool)> = Vec::new();
    for &h in &set {
        if seen.contains(&h) {
            continue;
        }
        let mut start = h;
        let mut closed = false;
        for _ in 0..set.len() {
            match prev.get(&start) {
                Some(p) if *p == h => {
                    closed = true;
                    break;
                }
                Some(p) => start = *p,
                None => break,
            }
        }
        let mut run = vec![start];
        let mut cur = hes[start.0 as usize].next;
        while set.contains(&cur) && cur != start {
            run.push(cur);
            cur = hes[cur.0 as usize].next;
        }
        seen.extend(run.iter().copied());
        let low = run.iter().filter_map(|x| canonical(mesh, *x)).min().unwrap_or(start);
        runs.push((low, run, closed));
    }
    runs.sort_by_key(|r| r.0);
    runs.into_iter().map(|(_, r, c)| (r, c)).collect()
}

/// Bridge (edges): joins each pair of open border runs (from
/// [`border_runs`]) with new polygons. Runs with the same number of edges get
/// `segments` rows of quads; runs of different lengths one row of triangles.
/// Runs that share an end vertex close with a triangle there.
pub fn bridge_edge_runs(mesh: &mut Mesh, pairs: &[(Vec<HalfEdgeId>, Vec<HalfEdgeId>)], segments: u32) -> Result<Vec<FaceId>, String> {
    let hes = mesh.half_edges();
    let polyline = |run: &[HalfEdgeId]| -> Vec<u32> {
        let mut v: Vec<u32> = run.iter().map(|h| hes[h.0 as usize].origin.0).collect();
        if let Some(l) = run.last() {
            v.push(hes[hes[l.0 as usize].twin.0 as usize].origin.0);
        }
        v
    };
    let mut im = IndexedMesh::from_mesh(mesh);
    let mut new_faces = Vec::new();
    let mut push = |im: &mut IndexedMesh, src: &IndexedFace, mut ids: Vec<u32>| {
        ids.dedup();
        while ids.len() > 1 && ids.first() == ids.last() {
            ids.pop();
        }
        if ids.len() >= 3 {
            new_faces.push(FaceId(im.faces.len() as u32));
            im.faces.push(IndexedFace::like(src, ids));
        }
    };
    for (ra, rb) in pairs {
        let (Some(a0), false) = (ra.first(), rb.is_empty()) else { return Err("Bridge needs two edge selections".into()) };
        // Both runs go in border order, so the second is walked backwards to face the first.
        let a = polyline(ra);
        let c: Vec<u32> = polyline(rb).into_iter().rev().collect();
        let f = hes[hes[a0.0 as usize].twin.0 as usize].face.ok_or("not a border edge")?;
        let src = IndexedFace::like(&im.faces[f.0 as usize], vec![]);
        let (m, n) = (a.len() - 1, c.len() - 1);
        if m == n {
            let segs = segments.max(1) as usize;
            let mut rings: Vec<Vec<u32>> = vec![a.clone()];
            for s in 1..segs {
                let t = s as f32 / segs as f32;
                let ring = (0..=m)
                    .map(|k| if a[k] == c[k] { a[k] } else { im.add_vertex(im.positions[a[k] as usize].lerp(im.positions[c[k] as usize], t), Some(a[k])) })
                    .collect();
                rings.push(ring);
            }
            rings.push(c.clone());
            for s in 0..segs {
                for i in 0..m {
                    push(&mut im, &src, vec![rings[s][i], rings[s][i + 1], rings[s + 1][i + 1], rings[s + 1][i]]);
                }
            }
        } else {
            if segments > 1 {
                return Err("Bridge: with more than one segment both edge selections need the same number of edges".into());
            }
            // Zip the two runs by length along them.
            let along = |ids: &[u32]| -> Vec<f32> {
                let mut acc = vec![0.0];
                for w in ids.windows(2) {
                    let d = im.positions[w[0] as usize].distance(im.positions[w[1] as usize]);
                    acc.push(acc.last().unwrap() + d);
                }
                let total = acc.last().copied().unwrap_or(1.0).max(1e-9);
                acc.into_iter().map(|x| x / total).collect()
            };
            let (ta, tc) = (along(&a), along(&c));
            let (mut i, mut j) = (0, 0);
            while i < m || j < n {
                if j == n || (i < m && ta[i + 1] <= tc[j + 1]) {
                    push(&mut im, &src, vec![a[i], a[i + 1], c[j]]);
                    i += 1;
                } else {
                    push(&mut im, &src, vec![a[i], c[j + 1], c[j]]);
                    j += 1;
                }
            }
        }
    }
    *mesh = im.build().map_err(|e| format!("Bridge: {e}"))?;
    Ok(new_faces)
}

/// The edges Swift Loop splits for a click on `edge` at `t` (from its
/// canonical origin): the edge's ring across quads both ways, each as a
/// half-edge and the split parameter from that half-edge's origin, in order
/// along the loop; and whether the loop closes.
pub fn swift_loop_ring(mesh: &Mesh, edge: HalfEdgeId, t: f32) -> (Vec<(HalfEdgeId, f32)>, bool) {
    let hes = mesh.half_edges();
    let Some(h0) = canonical(mesh, edge) else { return (Vec::new(), false) };
    let mut seen: BTreeSet<HalfEdgeId> = [h0].into_iter().collect();
    // Across a quad a→b→c→d the point t from a on a→b continues t from d on d→c.
    let mut walk = |start: HalfEdgeId, t: f32| -> (Vec<(HalfEdgeId, f32)>, bool) {
        let mut out = Vec::new();
        let mut h = start;
        loop {
            if hes[h.0 as usize].face.is_none() {
                return (out, false);
            }
            let n1 = hes[h.0 as usize].next;
            let n2 = hes[n1.0 as usize].next;
            let n3 = hes[n2.0 as usize].next;
            if hes[n3.0 as usize].next != h {
                return (out, false);
            }
            let nh = hes[n2.0 as usize].twin;
            let Some(c) = canonical(mesh, nh) else { return (out, false) };
            if !seen.insert(c) {
                return (out, c == h0);
            }
            out.push((nh, t));
            h = nh;
        }
    };
    let (fwd, closed) = walk(h0, t);
    let mut ring: Vec<(HalfEdgeId, f32)> = Vec::new();
    if !closed {
        let (bwd, _) = walk(hes[h0.0 as usize].twin, 1.0 - t);
        ring.extend(bwd.into_iter().rev());
    }
    ring.push((h0, t));
    ring.extend(fwd);
    (ring, closed)
}

/// Where Swift Loop's new loop runs (the preview): the split points in
/// order, and whether the loop closes.
pub fn swift_loop_points(mesh: &Mesh, edge: HalfEdgeId, t: f32) -> (Vec<Vec3>, bool) {
    let (ring, closed) = swift_loop_ring(mesh, edge, t);
    let pts = ring
        .iter()
        .filter_map(|(h, t)| {
            let (a, b) = mesh.edge_vertices(*h)?;
            Some(mesh.vertex_position(a)?.lerp(mesh.vertex_position(b)?, *t))
        })
        .collect();
    (pts, closed)
}

/// Swift Loop: inserts the edge loop through the click on `edge` at `t`
/// (from its canonical origin), across the edge's whole ring. Returns the
/// new edges' end positions.
pub fn swift_loop(mesh: &mut Mesh, edge: HalfEdgeId, t: f32) -> Result<Vec<(Vec3, Vec3)>, String> {
    let (ring, _) = swift_loop_ring(mesh, edge, t.clamp(0.02, 0.98));
    if ring.is_empty() {
        return Err("Swift Loop: no edge there".into());
    }
    let mut im = IndexedMesh::from_mesh(mesh);
    let mut points: BTreeMap<(u32, u32), Vec<u32>> = BTreeMap::new();
    for (h, t) in ring {
        let (a, b) = mesh.edge_vertices(h).ok_or("invalid edge")?;
        let p = mesh.vertex_position(a).unwrap().lerp(mesh.vertex_position(b).unwrap(), t);
        let lo = a.0.min(b.0);
        let id = im.add_vertex(p, Some(lo));
        points.insert((lo, a.0.max(b.0)), vec![id]);
    }
    let segs = split_at_points(&mut im, &points);
    *mesh = im.build().map_err(|e| format!("Swift Loop: {e}"))?;
    if segs.is_empty() {
        return Err("Swift Loop needs a quad next to the edge".into());
    }
    Ok(segs)
}

/// Clean Remove (Ctrl+Remove on edges): removes the edges, then the vertices
/// they leave behind with only two edges.
pub fn clean_remove_edges(mesh: &mut Mesh, edges: &[HalfEdgeId]) -> Result<usize, String> {
    let ends: Vec<Vec3> = edges
        .iter()
        .filter_map(|e| mesh.edge_vertices(*e))
        .flat_map(|(a, b)| [a, b])
        .filter_map(|v| mesh.vertex_position(v))
        .collect();
    crate::edit_ops::dissolve_edges(mesh, edges)?;
    let mut leftovers: Vec<VertexId> = Vec::new();
    for p in ends {
        if let Some(v) = (0..mesh.vertex_count() as u32).map(VertexId).find(|v| mesh.vertex_position(*v) == Some(p)) {
            if vertex_edges(mesh, v).len() == 2 && !leftovers.contains(&v) {
                leftovers.push(v);
            }
        }
    }
    remove_vertices(mesh, &leftovers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{box_primitive, plane_segmented};

    fn interior_vertex(m: &Mesh) -> VertexId {
        (0..m.vertex_count() as u32).map(VertexId).find(|v| fan_is_closed(m, *v)).expect("interior vertex")
    }

    #[test]
    fn remove_vertex_merges_faces() {
        let mut m = plane_segmented(20.0, 20.0, 2, 2);
        let v = interior_vertex(&m);
        assert_eq!(remove_vertices(&mut m, &[v]).unwrap(), 1);
        assert_eq!(m.face_count(), 1);
        assert_eq!(m.vertex_count(), 8);
        m.check_invariants().unwrap();
    }

    #[test]
    fn break_vertex_splits_corners() {
        let mut m = plane_segmented(20.0, 20.0, 2, 2);
        let v = interior_vertex(&m);
        assert_eq!(break_vertices(&mut m, &[v]).unwrap(), 3);
        assert_eq!(m.vertex_count(), 12);
        assert_eq!(m.face_count(), 4);
    }

    #[test]
    fn weld_within_threshold_merges() {
        let mut m = plane_segmented(20.0, 20.0, 2, 2);
        let v = interior_vertex(&m);
        break_vertices(&mut m, &[v]).unwrap();
        let all: Vec<VertexId> = (0..m.vertex_count() as u32).map(VertexId).collect();
        assert_eq!(weld_vertices(&mut m, &all, 0.1).unwrap(), 3);
        assert_eq!(m.vertex_count(), 9);
        m.check_invariants().unwrap();
    }

    #[test]
    fn weld_refuses_to_make_non_manifold() {
        let mut m = plane_segmented(20.0, 20.0, 2, 2);
        // Welding two opposite neighbours of the centre vertex gives two faces the same directed edge
        // (centre -> welded vertex) without any face becoming degenerate.
        let v = interior_vertex(&m);
        let c = m.vertex_position(v).unwrap();
        let nb = crate::select_ops::vertex_neighbours(&m, v);
        let a = nb[0];
        let pa = m.vertex_position(a).unwrap();
        let b = *nb.iter().find(|n| (m.vertex_position(**n).unwrap() + pa - c * 2.0).length() < 1e-3).unwrap();
        let before = m.clone();
        let r = weld_vertices(&mut m, &[a, b], 100.0);
        assert!(r.is_err());
        assert_eq!(m.vertex_count(), before.vertex_count(), "a refused weld leaves the mesh alone");
    }

    #[test]
    fn insert_vertex_on_box_edge_keeps_manifold() {
        let mut m = box_primitive(Vec3::splat(0.5));
        let v = insert_vertex_on_edge(&mut m, HalfEdgeId(0), 0.5).unwrap();
        assert_eq!(m.vertex_count(), 9);
        let with_v: Vec<_> = (0..m.face_count() as u32).filter(|f| m.face_vertex_ids(FaceId(*f)).unwrap().contains(&v)).map(|f| m.face_vertex_ids(FaceId(f)).unwrap()).collect();
        assert_eq!(with_v.len(), 2, "{with_v:?}");
        assert_eq!(m.faces_around_vertex(v).len(), 2);
        m.check_invariants().unwrap();
    }

    #[test]
    fn collapse_whole_face_removes_it() {
        let mut m = box_primitive(Vec3::splat(0.5));
        let ids = m.face_vertex_ids(FaceId(0)).unwrap();
        collapse_vertices(&mut m, &ids).unwrap();
        assert_eq!(m.face_count(), 5, "the top collapses to a point: four triangles and the bottom");
        m.check_invariants().unwrap();
    }

    /// Number of distinct border loops (a chamfer ending at a border may add
    /// edges to a loop, never a loop).
    fn border_count(m: &Mesh) -> usize {
        let mut seen: Vec<BTreeSet<VertexId>> = Vec::new();
        for e in (0..m.half_edges().len() as u32).map(HalfEdgeId).filter(|e| m.canonical_edge_id(*e) == Some(*e) && is_border_edge(m, *e)) {
            let l: BTreeSet<VertexId> = border_loop(m, e).into_iter().collect();
            if !seen.contains(&l) {
                seen.push(l);
            }
        }
        seen.len()
    }

    fn coincident_pairs(m: &Mesh) -> usize {
        let n = m.vertex_count();
        let mut c = 0;
        for i in 0..n {
            for j in i + 1..n {
                if m.vertex_position(VertexId(i as u32)).unwrap().distance(m.vertex_position(VertexId(j as u32)).unwrap()) < 1e-5 {
                    c += 1;
                }
            }
        }
        c
    }

    fn assert_clean(before: &Mesh, after: &Mesh, what: &str) {
        after.check_invariants().unwrap();
        assert_eq!(border_count(after), border_count(before), "{what}: border count changed");
        assert_eq!(coincident_pairs(after), 0, "{what}: coincident vertices");
    }

    #[test]
    fn chamfer_single_interior_edge_of_a_grid_keeps_the_end_vertices() {
        let base = plane_segmented(30.0, 30.0, 3, 3);
        let v = interior_vertex(&base);
        let nb = crate::select_ops::vertex_neighbours(&base, v);
        let other = *nb.iter().find(|n| !crate::select_ops::is_border_vertex(&base, **n)).unwrap();
        let e = edge_between(&base, v, other).unwrap();
        let mut m = base.clone();
        let new = chamfer_edges(&mut m, &[e], 1.0, 1, false).unwrap();
        assert_clean(&base, &m, "single interior edge");
        assert_eq!(new.len(), 1);
        assert_eq!(m.face_vertex_ids(new[0]).unwrap().len(), 6, "the chamfer polygon is a hexagon ending at the kept vertices");
        assert_eq!(m.vertex_count(), base.vertex_count() + 4);
        // A line of three edges through two inner vertices.
        let line: Vec<HalfEdgeId> = {
            let hes = base.half_edges();
            (0..hes.len() as u32).map(HalfEdgeId).filter(|h| base.canonical_edge_id(*h) == Some(*h)).filter(|h| {
                let (a, b) = base.edge_vertices(*h).unwrap();
                let (pa, pb) = (base.vertex_position(a).unwrap(), base.vertex_position(b).unwrap());
                (pa.y - 5.0).abs() < 1e-4 && (pb.y - 5.0).abs() < 1e-4
            }).collect()
        };
        assert_eq!(line.len(), 3);
        let mut l = base.clone();
        chamfer_edges(&mut l, &line, 1.0, 1, false).unwrap();
        assert_clean(&base, &l, "line of three edges");
    }

    #[test]
    fn chamfer_box_loops_and_segments_stay_closed() {
        let base = crate::standard_primitives::box_segmented(40.0, 40.0, 40.0, [1, 1, 1]);
        let top: Vec<HalfEdgeId> = (0..base.half_edges().len() as u32).map(HalfEdgeId).filter(|h| base.canonical_edge_id(*h) == Some(*h)).filter(|h| {
            let (a, b) = base.edge_vertices(*h).unwrap();
            base.vertex_position(a).unwrap().z > 39.0 && base.vertex_position(b).unwrap().z > 39.0
        }).collect();
        assert_eq!(top.len(), 4);
        let mut m = base.clone();
        let new = chamfer_edges(&mut m, &top, 2.0, 1, false).unwrap();
        assert_clean(&base, &m, "top loop");
        assert_eq!(new.len(), 4);
        assert_eq!(m.face_count(), 10);
        let all: Vec<HalfEdgeId> = (0..base.half_edges().len() as u32).map(HalfEdgeId).filter(|h| base.canonical_edge_id(*h) == Some(*h)).collect();
        let mut s3 = base.clone();
        chamfer_edges(&mut s3, &all, 2.0, 3, false).unwrap();
        assert_clean(&base, &s3, "all edges, 3 segments");
        assert_eq!(s3.face_count(), 6 + 12 * 3 + 8);
        let mut x = base.clone();
        extrude_edges(&mut x, &[all[0]], 3.0, 1.0).unwrap();
        assert_clean(&base, &x, "extrude one edge");
        let mut xl = base.clone();
        extrude_edges(&mut xl, &top, 3.0, 1.0).unwrap();
        assert_clean(&base, &xl, "extrude top loop");
    }

    #[test]
    fn chamfer_any_single_edge_of_a_standard_box() {
        let base = crate::standard_primitives::box_segmented(40.0, 40.0, 40.0, [1, 1, 1]);
        let edges: Vec<HalfEdgeId> = (0..base.half_edges().len() as u32).map(HalfEdgeId).filter(|h| base.canonical_edge_id(*h) == Some(*h)).collect();
        for e in edges {
            let mut m = base.clone();
            let new = chamfer_edges(&mut m, &[e], 1.0, 1, false).unwrap_or_else(|err| panic!("edge {e:?}: {err}"));
            assert_eq!(new.len(), 1, "edge {e:?}");
            assert_eq!(m.face_count(), 7, "edge {e:?}");
            m.check_invariants().unwrap();
        }
    }

    #[test]
    fn chamfer_edge_makes_segments_faces() {
        let mut m = box_primitive(Vec3::splat(0.5));
        let e = HalfEdgeId(0);
        let new = chamfer_edges(&mut m, &[e], 0.1, 2, false).unwrap();
        assert_eq!(new.len(), 2);
        assert_eq!(m.face_count(), 8);
        m.check_invariants().unwrap();
        let mut m2 = box_primitive(Vec3::splat(0.5));
        let all: Vec<HalfEdgeId> = (0..m2.half_edges().len() as u32).map(HalfEdgeId).filter(|h| m2.canonical_edge_id(*h) == Some(*h)).collect();
        chamfer_edges(&mut m2, &all, 0.1, 1, false).unwrap();
        assert_eq!(m2.face_count(), 6 + 12 + 8, "every edge a strip, every corner a triangle");
        m2.check_invariants().unwrap();
    }

    #[test]
    fn chamfer_vertex_makes_a_polygon() {
        let mut m = box_primitive(Vec3::splat(0.5));
        let new = chamfer_vertices(&mut m, &[VertexId(0)], 0.2, false).unwrap();
        assert_eq!(new.len(), 1);
        assert_eq!(m.face_count(), 7);
        assert_eq!(m.face_vertex_ids(new[0]).unwrap().len(), 3);
        m.check_invariants().unwrap();
    }

    #[test]
    fn connect_edges_three_segments() {
        let mut m = box_primitive(Vec3::splat(0.5));
        let ring = crate::select_ops::edge_ring(&m, &[HalfEdgeId(0)]);
        assert_eq!(ring.len(), 4);
        let segs = connect_edges(&mut m, &ring, 3, 0.0, 0.0).unwrap();
        assert_eq!(segs.len(), 12);
        assert_eq!(m.face_count(), 6 - 4 + 16);
        m.check_invariants().unwrap();
    }

    #[test]
    fn connect_vertices_adds_a_diagonal() {
        let mut m = plane_segmented(10.0, 10.0, 1, 1);
        assert_eq!(connect_vertices(&mut m, &[VertexId(0), VertexId(2)]).unwrap(), 1);
        assert_eq!(m.face_count(), 2);
        m.check_invariants().unwrap();
    }

    #[test]
    fn cap_open_border_closes_mesh() {
        let mut m = box_primitive(Vec3::splat(0.5));
        crate::edit_ops::delete_faces(&mut m, &[FaceId(0)]).unwrap();
        let border = crate::select_ops::border_edges(&m);
        assert_eq!(border.len(), 4);
        cap_borders(&mut m, &[border[0]]).unwrap();
        assert_eq!(m.face_count(), 6);
        assert!(crate::select_ops::border_edges(&m).is_empty());
    }

    #[test]
    fn bridge_two_borders() {
        let mut m = box_primitive(Vec3::splat(0.5));
        // Remove the top and bottom: two borders, bridge them inside out.
        let top = (0..6u32).map(FaceId).find(|f| m.face_normal(*f).z > 0.9).unwrap();
        let bottom = (0..6u32).map(FaceId).find(|f| m.face_normal(*f).z < -0.9).unwrap();
        crate::edit_ops::delete_faces(&mut m, &[top, bottom]).unwrap();
        let border = crate::select_ops::border_edges(&m);
        assert_eq!(border.len(), 8);
        let loop_a = border_loop(&m, border[0]);
        let other = border.iter().find(|e| !loop_a.contains(&m.edge_vertices(**e).unwrap().0)).unwrap();
        let new = bridge(&mut m, &[border[0]], &[*other], None, 2, 0).unwrap();
        assert_eq!(new.len(), 8);
        assert!(crate::select_ops::border_edges(&m).is_empty());
        m.check_invariants().unwrap();
    }

    #[test]
    fn extrude_edges_height_width() {
        let mut m = box_primitive(Vec3::splat(0.5));
        let new = extrude_edges(&mut m, &[HalfEdgeId(0)], 0.3, 0.1).unwrap();
        // Two sides plus the four end triangles that close the ridge onto the kept vertices.
        assert_eq!(new.len(), 6);
        assert_eq!(m.face_count(), 12);
        m.check_invariants().unwrap();
        let (min, max) = m.aabb();
        assert!(max.max_element() > 0.5 + 0.2 || min.min_element() < -0.5 - 0.2, "the ridge sticks out");
    }

    #[test]
    fn extrude_vertex_makes_a_pyramid() {
        let mut m = box_primitive(Vec3::splat(0.5));
        let new = extrude_vertices(&mut m, &[VertexId(0)], 0.5, 0.2).unwrap();
        assert_eq!(new.len(), 3);
        assert_eq!(m.face_count(), 9);
        m.check_invariants().unwrap();
    }

    #[test]
    fn turn_edge_flips_diagonal() {
        let mut m = plane_segmented(10.0, 10.0, 1, 1);
        connect_vertices(&mut m, &[VertexId(0), VertexId(2)]).unwrap();
        let diag = edge_between(&m, VertexId(0), VertexId(2)).or_else(|| edge_between(&m, VertexId(2), VertexId(0))).unwrap();
        turn_edge(&mut m, diag).unwrap();
        assert!(edge_between(&m, VertexId(1), VertexId(3)).is_some() || edge_between(&m, VertexId(3), VertexId(1)).is_some());
        m.check_invariants().unwrap();
    }

    #[test]
    fn split_edges_tears_the_mesh() {
        let mut m = plane_segmented(20.0, 20.0, 2, 2);
        let v = interior_vertex(&m);
        // Two collinear interior edges through the centre reach the border on both ends.
        let es: Vec<HalfEdgeId> = vertex_edges(&m, v).into_iter().filter(|e| { let (a, b) = m.edge_vertices(*e).unwrap(); (m.vertex_position(a).unwrap().y - m.vertex_position(b).unwrap().y).abs() < 1e-4 }).collect();
        assert_eq!(es.len(), 2);
        split_edges(&mut m, &es).unwrap();
        assert_eq!(m.vertex_count(), 12);
        assert_eq!(crate::select_ops::border_edges(&m).len(), 12);
        m.check_invariants().unwrap();
    }

    #[test]
    fn insert_vertex_on_edge_and_in_face() {
        let mut m = box_primitive(Vec3::splat(0.5));
        insert_vertex_on_edge(&mut m, HalfEdgeId(0), 0.5).unwrap();
        assert_eq!(m.vertex_count(), 9);
        m.check_invariants().unwrap();
        insert_vertex_in_face(&mut m, FaceId(0), Vec3::new(0.0, 0.0, 0.5)).unwrap();
        m.check_invariants().unwrap();
    }
}


/// A face template holding the corners of two faces (for Turn, where the
/// new triangles draw their uvs from either source triangle).
fn merged_uv_source(a: &IndexedFace, b: &IndexedFace) -> IndexedFace {
    let mut ids = a.ids.clone();
    let mut uv = a.uv.clone();
    for (k, id) in b.ids.iter().enumerate() {
        if ids.contains(id) {
            continue;
        }
        ids.push(*id);
        for (ch, row) in &mut uv {
            if let Some(v) = b.uv.get(ch).and_then(|r| r.get(k)) {
                row.push(*v);
            }
        }
    }
    for (ch, row) in &mut uv {
        // A channel b lacks: keep the rows the same length.
        if row.len() != ids.len() {
            let avg = if row.is_empty() { glam::Vec3::ZERO } else { row.iter().sum::<glam::Vec3>() / row.len() as f32 };
            row.resize(ids.len(), avg);
            let _ = ch;
        }
    }
    IndexedFace { ids, smoothing_groups: a.smoothing_groups, material_id: a.material_id, hidden: a.hidden, uv }
}
