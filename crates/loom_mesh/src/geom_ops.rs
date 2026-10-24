//! Editable Poly > Edit Geometry: Attach, Detach, Slice / QuickSlice, Cut,
//! MSmooth, Tessellate, Relax and the transform constraints.

use crate::indexed::{IndexedFace, IndexedMesh};
use crate::select_ops::{is_border_edge, vertex_edges, vertex_neighbours};
use crate::{FaceId, HalfEdgeId, Mesh, VertexId};
use glam::{Mat4, Vec3};
use std::collections::{BTreeMap, BTreeSet};

/// Attach: `other` joins `mesh`, its vertices transformed by `other_to_this`.
pub fn attach(mesh: &mut Mesh, other: &Mesh, other_to_this: Mat4) -> Result<usize, String> {
    let mut im = IndexedMesh::from_mesh(mesh);
    let base = im.positions.len() as u32;
    let oim = IndexedMesh::from_mesh(other);
    for (i, p) in oim.positions.iter().enumerate() {
        let attrs = oim.vattrs[i];
        let id = im.add_vertex(other_to_this.transform_point3(*p), None);
        im.vattrs[id as usize] = attrs;
        if oim.keep.contains(&(i as u32)) {
            im.keep.insert(id);
        }
    }
    let added = oim.faces.len();
    for f in &oim.faces {
        im.faces.push(IndexedFace::same_order(f, f.ids.iter().map(|i| i + base).collect()));
    }
    *mesh = im.build()?;
    Ok(added)
}

/// Detach: the polygons become a new mesh (vertex ids compacted). Unless
/// `keep_source`, they leave `mesh`; isolated vertices they leave behind go too.
pub fn detach(mesh: &mut Mesh, faces: &[FaceId], keep_source: bool) -> Result<Mesh, String> {
    let sel: BTreeSet<FaceId> = faces.iter().copied().collect();
    if sel.is_empty() {
        return Err("select polygons to detach".into());
    }
    let im = IndexedMesh::from_mesh(mesh);
    let mut part = IndexedMesh { positions: im.positions.clone(), vattrs: im.vattrs.clone(), faces: Vec::new(), keep: BTreeSet::new() };
    for f in &sel {
        part.faces.push(im.faces[f.0 as usize].clone());
    }
    let detached = part.build()?;
    if !keep_source {
        let mut rest = im;
        rest.faces = rest.faces.iter().enumerate().filter(|(i, _)| !sel.contains(&FaceId(*i as u32))).map(|(_, f)| f.clone()).collect();
        rest.keep.clear();
        *mesh = rest.build()?;
    }
    Ok(detached)
}

/// Detach To Element: the polygons stay in place but no longer share
/// vertices with the rest of the mesh.
pub fn detach_to_element(mesh: &mut Mesh, faces: &[FaceId]) -> Result<(), String> {
    let sel: BTreeSet<FaceId> = faces.iter().copied().collect();
    if sel.is_empty() {
        return Err("select polygons to detach".into());
    }
    let mut im = IndexedMesh::from_mesh(mesh);
    let mut used_outside: BTreeSet<u32> = BTreeSet::new();
    for (i, f) in im.faces.iter().enumerate() {
        if !sel.contains(&FaceId(i as u32)) {
            used_outside.extend(f.ids.iter().copied());
        }
    }
    let mut copies: BTreeMap<u32, u32> = BTreeMap::new();
    for f in &sel {
        let ids = im.faces[f.0 as usize].ids.clone();
        let new_ids: Vec<u32> = ids
            .iter()
            .map(|v| {
                if used_outside.contains(v) {
                    if let Some(c) = copies.get(v) {
                        *c
                    } else {
                        let c = im.add_vertex(im.positions[*v as usize], Some(*v));
                        copies.insert(*v, c);
                        c
                    }
                } else {
                    *v
                }
            })
            .collect();
        im.faces[f.0 as usize].set_ids(new_ids);
    }
    *mesh = im.build()?;
    Ok(())
}

fn sign(d: f32) -> i8 {
    if d > 1e-5 { 1 } else if d < -1e-5 { -1 } else { 0 }
}

/// Splits polygon `ids` between the positions `i` and `j` of two cut vertices.
fn split_polygon(ids: &[u32], i: usize, j: usize) -> (Vec<u32>, Vec<u32>) {
    let (i, j) = if i < j { (i, j) } else { (j, i) };
    let a: Vec<u32> = ids[i..=j].to_vec();
    let mut b: Vec<u32> = ids[j..].to_vec();
    b.extend_from_slice(&ids[..=i]);
    (a, b)
}

/// Slice: cuts the polygons (all when `faces` is None) with the plane through
/// `origin` with `normal`; returns the new edges. `split` tears the mesh
/// along them (Slice Plane > Split).
#[allow(clippy::map_entry)]
pub fn slice(mesh: &mut Mesh, origin: Vec3, normal: Vec3, faces: Option<&[FaceId]>, split: bool) -> Result<Vec<HalfEdgeId>, String> {
    let n = normal.normalize_or_zero();
    if n.length_squared() < 0.5 {
        return Err("the slice plane has no normal".into());
    }
    let targets: BTreeSet<FaceId> = match faces {
        Some(f) => f.iter().copied().collect(),
        None => (0..mesh.face_count() as u32).map(FaceId).collect(),
    };
    let mut im = IndexedMesh::from_mesh(mesh);
    let dist: Vec<f32> = im.positions.iter().map(|p| (*p - origin).dot(n)).collect();
    // Crossing vertices per edge, shared by both faces of the edge.
    let mut crossing: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    let mut on_plane: BTreeSet<u32> = dist.iter().enumerate().filter(|(_, d)| sign(**d) == 0).map(|(i, _)| i as u32).collect();
    for f in &targets {
        let ids = im.faces[f.0 as usize].ids.clone();
        for k in 0..ids.len() {
            let (a, b) = (ids[k], ids[(k + 1) % ids.len()]);
            let (sa, sb) = (sign(dist[a as usize]), sign(dist[b as usize]));
            if sa * sb < 0 {
                let key = (a.min(b), a.max(b));
                if !crossing.contains_key(&key) {
                    let t = dist[a as usize] / (dist[a as usize] - dist[b as usize]);
                    let p = im.positions[a as usize].lerp(im.positions[b as usize], t);
                    let v = im.add_vertex(p, Some(a));
                    crossing.insert(key, v);
                    on_plane.insert(v);
                }
            }
        }
    }
    // Insert the crossing vertices into every face that has the edge.
    for face in &mut im.faces {
        let ids = face.ids.clone();
        let mut out = Vec::with_capacity(ids.len() * 2);
        for k in 0..ids.len() {
            let (a, b) = (ids[k], ids[(k + 1) % ids.len()]);
            out.push(a);
            if let Some(v) = crossing.get(&(a.min(b), a.max(b))) {
                out.push(*v);
            }
        }
        face.set_ids(out);
    }
    // Split the target polygons between their on-plane vertices.
    let mut new_edges: Vec<(u32, u32)> = Vec::new();
    let mut pieces: Vec<IndexedFace> = Vec::new();
    let mut removed: Vec<usize> = Vec::new();
    for f in &targets {
        let face = &im.faces[f.0 as usize];
        let cuts: Vec<usize> = face.ids.iter().enumerate().filter(|(_, v)| on_plane.contains(v)).map(|(i, _)| i).collect();
        if cuts.len() < 2 {
            continue;
        }
        // Order the cut vertices along the plane line so pairs are consecutive.
        let fnorm = {
            let pts: Vec<Vec3> = face.ids.iter().map(|i| im.positions[*i as usize]).collect();
            let mut nn = Vec3::ZERO;
            for i in 0..pts.len() {
                nn += pts[i].cross(pts[(i + 1) % pts.len()]);
            }
            nn.normalize_or_zero()
        };
        let dir = n.cross(fnorm).normalize_or_zero();
        let mut ordered = cuts.clone();
        ordered.sort_by(|a, b| {
            let pa = im.positions[face.ids[*a] as usize].dot(dir);
            let pb = im.positions[face.ids[*b] as usize].dot(dir);
            pa.partial_cmp(&pb).unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut polys: Vec<Vec<u32>> = vec![face.ids.clone()];
        let mut any = false;
        for pair in ordered.chunks(2) {
            if pair.len() < 2 {
                break;
            }
            let (va, vb) = (face.ids[pair[0]], face.ids[pair[1]]);
            // Both cut vertices adjacent in the polygon = already an edge.
            let Some(pi) = polys.iter().position(|p| p.contains(&va) && p.contains(&vb)) else { continue };
            let poly = polys.remove(pi);
            let i = poly.iter().position(|x| *x == va).unwrap();
            let j = poly.iter().position(|x| *x == vb).unwrap();
            if (i + 1) % poly.len() == j || (j + 1) % poly.len() == i {
                polys.push(poly);
                continue;
            }
            let (p1, p2) = split_polygon(&poly, i, j);
            polys.push(p1);
            polys.push(p2);
            new_edges.push((va, vb));
            any = true;
        }
        if any {
            removed.push(f.0 as usize);
            for p in polys {
                pieces.push(IndexedFace::like(face, p));
            }
        }
    }
    removed.sort_unstable_by(|a, b| b.cmp(a));
    for r in removed {
        im.faces.remove(r);
    }
    im.faces.extend(pieces);
    let built = im.build().map_err(|e| format!("Slice: {e}"))?;
    *mesh = built;
    let mut out: Vec<HalfEdgeId> = Vec::new();
    for (a, b) in &new_edges {
        if let Some(e) = crate::topo_ops::edge_between(mesh, VertexId(*a), VertexId(*b)) {
            if let Some(c) = mesh.canonical_edge_id(e) {
                out.push(c);
            }
        }
    }
    if split && !out.is_empty() {
        crate::topo_ops::split_edges(mesh, &out)?;
        // Ids survive a split (copies are appended), so the edges still exist.
    }
    Ok(out)
}

/// A point on a polygon's surface.
#[derive(Debug, Clone, Copy)]
pub struct SurfacePoint {
    pub face: FaceId,
    pub point: Vec3,
}

/// Snaps a surface point to the polygon's boundary: an existing vertex when
/// within 2 % of an edge's end, else a new vertex on the nearest edge.
fn boundary_vertex(mesh: &mut Mesh, sp: SurfacePoint) -> Result<VertexId, String> {
    let ids = mesh.face_vertex_ids(sp.face).ok_or("invalid face")?;
    let n = ids.len();
    let mut best: Option<(f32, usize, f32)> = None;
    for k in 0..n {
        let (a, b) = (mesh.vertex_position(ids[k]).unwrap(), mesh.vertex_position(ids[(k + 1) % n]).unwrap());
        let ab = b - a;
        let t = if ab.length_squared() < 1e-12 { 0.0 } else { ((sp.point - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) };
        let d = (a + ab * t).distance(sp.point);
        if best.is_none_or(|(bd, _, _)| d < bd) {
            best = Some((d, k, t));
        }
    }
    let (_, k, t) = best.ok_or("empty polygon")?;
    let (va, vb) = (ids[k], ids[(k + 1) % n]);
    if t < 0.02 {
        return Ok(va);
    }
    if t > 0.98 {
        return Ok(vb);
    }
    let e = crate::topo_ops::edge_between(mesh, va, vb).ok_or("edge not found")?;
    let c = mesh.canonical_edge_id(e).ok_or("edge not found")?;
    let (ca, _) = mesh.edge_vertices(c).unwrap();
    let tc = if ca == va { t } else { 1.0 - t };
    crate::topo_ops::insert_vertex_on_edge(mesh, c, tc)
}

/// Cut: a straight cut over the surface from one point to another, in the
/// plane that contains both points and the view direction. The end points
/// snap to the polygons' edges / vertices; the polygons in between are
/// sliced. Returns the new edges.
pub fn cut(mesh: &mut Mesh, from: SurfacePoint, to: SurfacePoint, view_dir: Vec3) -> Result<Vec<HalfEdgeId>, String> {
    let va = boundary_vertex(mesh, from)?;
    let vb = boundary_vertex(mesh, to)?;
    if va == vb {
        return Err("Cut needs two different points".into());
    }
    let (pa, pb) = (mesh.vertex_position(va).unwrap(), mesh.vertex_position(vb).unwrap());
    let dir = pb - pa;
    let mut n = dir.cross(view_dir).normalize_or_zero();
    if n.length_squared() < 0.5 {
        n = dir.cross(mesh.face_normal(from.face)).normalize_or_zero();
    }
    if n.length_squared() < 0.5 {
        return Err("Cut: the two points are too close".into());
    }
    // Walk the polygons the segment crosses, from va towards vb.
    let plane_d = |p: Vec3| (p - pa).dot(n);
    let along = |p: Vec3| (p - pa).dot(dir) / dir.length_squared();
    let mut chain: Vec<FaceId> = Vec::new();
    let mut visited: BTreeSet<FaceId> = BTreeSet::new();
    let mut frontier: Vec<FaceId> = mesh.faces_around_vertex(va);
    let mut found = false;
    'walk: while let Some(f) = frontier.pop() {
        if !visited.insert(f) {
            continue;
        }
        let ids = mesh.face_vertex_ids(f).unwrap_or_default();
        if ids.contains(&vb) && chain.len() + 1 >= 1 && (ids.contains(&va) || !chain.is_empty()) {
            chain.push(f);
            found = true;
            break;
        }
        // The exit of this polygon: a crossing edge or an on-plane vertex ahead of the entry.
        let k = ids.len();
        let mut best: Option<(f32, Option<HalfEdgeId>, Option<VertexId>)> = None;
        for i in 0..k {
            let (a, b) = (ids[i], ids[(i + 1) % k]);
            let (qa, qb) = (mesh.vertex_position(a).unwrap(), mesh.vertex_position(b).unwrap());
            let (da, db) = (plane_d(qa), plane_d(qb));
            if sign(da) == 0 && a != va {
                let t = along(qa);
                if t > 1e-4 && best.is_none_or(|(bt, _, _)| t > bt) {
                    best = Some((t, None, Some(a)));
                }
            }
            if sign(da) * sign(db) < 0 {
                let t = da / (da - db);
                let q = qa.lerp(qb, t);
                let s = along(q);
                if s > 1e-4 && best.is_none_or(|(bt, _, _)| s > bt) {
                    let e = crate::topo_ops::edge_between(mesh, a, b);
                    best = Some((s, e, None));
                }
            }
        }
        let Some((t, edge, vertex)) = best else { continue };
        if !ids.contains(&va) && chain.is_empty() {
            continue;
        }
        if t > 1.0 + 1e-3 {
            continue;
        }
        chain.push(f);
        if let Some(e) = edge {
            let hes = mesh.half_edges();
            if let Some(nf) = hes[hes[e.0 as usize].twin.0 as usize].face {
                frontier.push(nf);
            }
        } else if let Some(v) = vertex {
            if v == vb {
                found = true;
                break 'walk;
            }
            frontier.extend(mesh.faces_around_vertex(v));
        }
    }
    if !found {
        return Err("Cut: no path over the surface between the two points".into());
    }
    slice(mesh, pa, n, Some(&chain), false)
}

/// MSmooth: one Catmull-Clark pass. `smoothness` 0..1 blends from the plain
/// subdivision to the smoothed one; separating by smoothing groups or
/// material ids treats the edges between them as creases.
pub fn msmooth(mesh: &Mesh, smoothness: f32, separate_by_smoothing: bool, separate_by_materials: bool) -> Result<Mesh, String> {
    msmooth_faces(mesh, None, smoothness, separate_by_smoothing, separate_by_materials)
}

/// MSmooth of a polygon subset (at a sub-object level): only the subset
/// subdivides; its boundary with the rest is a crease, and the untouched
/// neighbours pick up the new edge vertices so the mesh stays manifold.
#[allow(clippy::needless_range_loop)]
pub fn msmooth_faces(mesh: &Mesh, subset: Option<&[FaceId]>, smoothness: f32, separate_by_smoothing: bool, separate_by_materials: bool) -> Result<Mesh, String> {
    let s = smoothness.clamp(0.0, 1.0);
    let im = IndexedMesh::from_mesh(mesh);
    let hes = mesh.half_edges();
    let nv = im.positions.len();
    let inside: Option<BTreeSet<FaceId>> = subset.map(|f| f.iter().copied().collect());
    let in_subset = |f: FaceId| inside.as_ref().is_none_or(|set| set.contains(&f));
    let is_crease = |e: HalfEdgeId| -> bool {
        if is_border_edge(mesh, e) {
            return true;
        }
        let (Some(fa), Some(fb)) = (hes[e.0 as usize].face, hes[hes[e.0 as usize].twin.0 as usize].face) else { return true };
        if !in_subset(fa) || !in_subset(fb) {
            return true;
        }
        let (a, b) = (&mesh.faces()[fa.0 as usize], &mesh.faces()[fb.0 as usize]);
        (separate_by_smoothing && (a.smoothing_groups & b.smoothing_groups) == 0) || (separate_by_materials && a.material_id != b.material_id)
    };
    // Edges touching the subset (they get an edge point); vertices used by it.
    let subset_edge = |e: HalfEdgeId| -> bool {
        let (fa, fb) = (hes[e.0 as usize].face, hes[hes[e.0 as usize].twin.0 as usize].face);
        fa.is_some_and(in_subset) || fb.is_some_and(in_subset)
    };
    let mut out = IndexedMesh { positions: Vec::new(), vattrs: Vec::new(), faces: Vec::new(), keep: BTreeSet::new() };
    // Face points.
    let face_pt: Vec<Vec3> = im.faces.iter().map(|f| f.ids.iter().map(|i| im.positions[*i as usize]).sum::<Vec3>() / f.ids.len() as f32).collect();
    // Edge points keyed by canonical edge.
    let mut edge_pt: BTreeMap<(u32, u32), (Vec3, Vec3)> = BTreeMap::new(); // (linear, smooth)
    for (i, he) in hes.iter().enumerate() {
        let e = HalfEdgeId(i as u32);
        if mesh.canonical_edge_id(e) != Some(e) || !subset_edge(e) {
            continue;
        }
        let (a, b) = mesh.edge_vertices(e).unwrap();
        let (pa, pb) = (im.positions[a.0 as usize], im.positions[b.0 as usize]);
        let mid = (pa + pb) * 0.5;
        let smooth = if is_crease(e) {
            mid
        } else {
            let fa = he.face.unwrap();
            let fb = hes[he.twin.0 as usize].face.unwrap();
            (pa + pb + face_pt[fa.0 as usize] + face_pt[fb.0 as usize]) * 0.25
        };
        edge_pt.insert((a.0.min(b.0), a.0.max(b.0)), (mid, smooth));
    }
    // Vertex points.
    let mut vert_pt: Vec<Vec3> = Vec::with_capacity(nv);
    for v in 0..nv {
        let vid = VertexId(v as u32);
        let p = im.positions[v];
        let edges = vertex_edges(mesh, vid);
        let touches = mesh.faces_around_vertex(vid).iter().any(|f| in_subset(*f));
        let creases: Vec<HalfEdgeId> = edges.iter().copied().filter(|e| is_crease(*e)).collect();
        let smooth = if edges.is_empty() || creases.len() >= 3 || !touches {
            p
        } else if creases.len() == 2 {
            let other = |e: HalfEdgeId| {
                let (a, b) = mesh.edge_vertices(e).unwrap();
                im.positions[if a == vid { b.0 } else { a.0 } as usize]
            };
            (p * 6.0 + other(creases[0]) + other(creases[1])) / 8.0
        } else {
            let faces = mesh.faces_around_vertex(vid);
            let n = edges.len() as f32;
            let q: Vec3 = faces.iter().map(|f| face_pt[f.0 as usize]).sum::<Vec3>() / faces.len().max(1) as f32;
            let r: Vec3 = edges
                .iter()
                .map(|e| {
                    let (a, b) = mesh.edge_vertices(*e).unwrap();
                    (im.positions[a.0 as usize] + im.positions[b.0 as usize]) * 0.5
                })
                .sum::<Vec3>()
                / n;
            (q + r * 2.0 + p * (n - 3.0)) / n
        };
        vert_pt.push(p.lerp(smooth, s));
    }
    for v in 0..nv {
        let id = out.add_vertex(vert_pt[v], None);
        out.vattrs[id as usize] = im.vattrs[v];
    }
    let mut edge_id: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for (k, (lin, sm)) in &edge_pt {
        let id = out.add_vertex(lin.lerp(*sm, s), None);
        out.vattrs[id as usize] = im.vattrs[k.0 as usize];
        edge_id.insert(*k, id);
    }
    for (fi, f) in im.faces.iter().enumerate() {
        let k = f.ids.len();
        if !in_subset(FaceId(fi as u32)) {
            // Outside the subset: keep the polygon, with the new edge vertices inserted.
            let mut ids = Vec::with_capacity(k * 2);
            for i in 0..k {
                let v = f.ids[i];
                let next = f.ids[(i + 1) % k];
                ids.push(v);
                if let Some(e) = edge_id.get(&(v.min(next), v.max(next))) {
                    ids.push(*e);
                }
            }
            out.faces.push(IndexedFace::like(f, ids));
            continue;
        }
        let c = out.add_vertex(face_pt[fi], None);
        out.vattrs[c as usize] = im.vattrs[f.ids[0] as usize];
        for i in 0..k {
            let v = f.ids[i];
            let next = f.ids[(i + 1) % k];
            let prev = f.ids[(i + k - 1) % k];
            let e_next = edge_id[&(v.min(next), v.max(next))];
            let e_prev = edge_id[&(v.min(prev), v.max(prev))];
            out.faces.push(IndexedFace::like(f, vec![v, e_next, c, e_prev]));
        }
    }
    out.keep = im.keep.clone();
    out.build()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TessKind {
    Edge,
    Face,
}

/// Tessellate: Edge splits every edge of the polygons and connects the
/// midpoints; Face fans each polygon from its centre. `tension` (percent)
/// pushes the new vertices out along the normal.
#[allow(clippy::map_entry)]
pub fn tessellate(mesh: &mut Mesh, faces: &[FaceId], kind: TessKind, tension: f32) -> Result<(), String> {
    let sel: BTreeSet<FaceId> = faces.iter().copied().collect();
    if sel.is_empty() {
        return Err("select polygons to tessellate".into());
    }
    let mut im = IndexedMesh::from_mesh(mesh);
    let t = tension * 0.01;
    match kind {
        TessKind::Face => {
            let mut pieces = Vec::new();
            let mut removed: Vec<usize> = Vec::new();
            for f in &sel {
                let face = im.faces[f.0 as usize].clone();
                let pts: Vec<Vec3> = face.ids.iter().map(|i| im.positions[*i as usize]).collect();
                let centre = pts.iter().sum::<Vec3>() / pts.len() as f32;
                let radius = pts.iter().map(|p| p.distance(centre)).sum::<f32>() / pts.len() as f32;
                let c = im.add_vertex(centre + mesh.face_normal(*f) * radius * t, Some(face.ids[0]));
                let k = face.ids.len();
                for i in 0..k {
                    pieces.push(IndexedFace::like_at(&face, vec![face.ids[i], face.ids[(i + 1) % k], c], &im.positions));
                }
                removed.push(f.0 as usize);
            }
            removed.sort_unstable_by(|a, b| b.cmp(a));
            for r in removed {
                im.faces.remove(r);
            }
            im.faces.extend(pieces);
        }
        TessKind::Edge => {
            let mut mids: BTreeMap<(u32, u32), u32> = BTreeMap::new();
            for f in &sel {
                let ids = im.faces[f.0 as usize].ids.clone();
                let k = ids.len();
                for i in 0..k {
                    let (a, b) = (ids[i], ids[(i + 1) % k]);
                    let key = (a.min(b), a.max(b));
                    if !mids.contains_key(&key) {
                        let (pa, pb) = (im.positions[a as usize], im.positions[b as usize]);
                        let e = crate::topo_ops::edge_between(mesh, VertexId(a), VertexId(b));
                        let n = e.map(|e| {
                            let hes = mesh.half_edges();
                            let fa = hes[e.0 as usize].face.map(|f| mesh.face_normal(f)).unwrap_or(Vec3::ZERO);
                            let fb = hes[hes[e.0 as usize].twin.0 as usize].face.map(|f| mesh.face_normal(f)).unwrap_or(Vec3::ZERO);
                            (fa + fb).normalize_or_zero()
                        }).unwrap_or(Vec3::ZERO);
                        let m = im.add_vertex((pa + pb) * 0.5 + n * pa.distance(pb) * 0.5 * t, Some(a));
                        mids.insert(key, m);
                    }
                }
            }
            // Every face with a split edge gets the midpoint; selected faces are rebuilt.
            let mut pieces = Vec::new();
            let mut removed: Vec<usize> = Vec::new();
            let positions = im.positions.clone();
            for (fi, face) in im.faces.iter_mut().enumerate() {
                let ids = face.ids.clone();
                let k = ids.len();
                let mut with_mids: Vec<u32> = Vec::new();
                for i in 0..k {
                    let (a, b) = (ids[i], ids[(i + 1) % k]);
                    with_mids.push(a);
                    if let Some(m) = mids.get(&(a.min(b), a.max(b))) {
                        with_mids.push(*m);
                    }
                }
                if sel.contains(&FaceId(fi as u32)) {
                    // Corner triangles and the central polygon of midpoints.
                    let mut centre = Vec::new();
                    for i in 0..k {
                        let a = ids[i];
                        let m_next = mids[&(a.min(ids[(i + 1) % k]), a.max(ids[(i + 1) % k]))];
                        let m_prev = mids[&(a.min(ids[(i + k - 1) % k]), a.max(ids[(i + k - 1) % k]))];
                        pieces.push(IndexedFace::like_at(face, vec![a, m_next, m_prev], &positions));
                        centre.push(m_next);
                    }
                    pieces.push(IndexedFace::like_at(face, centre, &positions));
                    removed.push(fi);
                } else {
                    face.set_ids(with_mids);
                }
            }
            removed.sort_unstable_by(|a, b| b.cmp(a));
            for r in removed {
                im.faces.remove(r);
            }
            im.faces.extend(pieces);
        }
    }
    *mesh = im.build()?;
    Ok(())
}

/// Relax: every vertex moves `amount` of the way to the average of its
/// neighbours, `iterations` times. Hold Boundary Points keeps border vertices;
/// Hold Outer Points keeps selected vertices that touch unselected ones.
pub fn relax(mesh: &mut Mesh, verts: &[VertexId], amount: f32, iterations: u32, hold_boundary: bool, hold_outer: bool) -> Result<usize, String> {
    let sel: BTreeSet<VertexId> = verts.iter().copied().collect();
    if sel.is_empty() {
        return Err("select vertices to relax".into());
    }
    let movable: Vec<VertexId> = sel
        .iter()
        .copied()
        .filter(|v| {
            let nb = vertex_neighbours(mesh, *v);
            if nb.is_empty() {
                return false;
            }
            if hold_boundary && crate::select_ops::is_border_vertex(mesh, *v) {
                return false;
            }
            if hold_outer && nb.iter().any(|n| !sel.contains(n)) {
                return false;
            }
            true
        })
        .collect();
    let neighbours: Vec<Vec<usize>> = movable.iter().map(|v| vertex_neighbours(mesh, *v).into_iter().map(|n| n.0 as usize).collect()).collect();
    let mut im = IndexedMesh::from_mesh(mesh);
    for _ in 0..iterations.max(1) {
        let snapshot = im.positions.clone();
        for (i, v) in movable.iter().enumerate() {
            let avg = neighbours[i].iter().map(|n| snapshot[*n]).sum::<Vec3>() / neighbours[i].len() as f32;
            let p = snapshot[v.0 as usize];
            im.positions[v.0 as usize] = p + (avg - p) * amount;
        }
    }
    let moved = movable.len();
    *mesh = im.build()?;
    Ok(moved)
}

/// Edit Geometry > Constraints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Constraint {
    None,
    Edges,
    Faces,
    Normals,
}

/// Restricts a vertex displacement: Edges keeps it along the best-aligned
/// adjacent edge, Faces keeps it in the vertex's surface plane, Normals keeps
/// only the component along the vertex normal.
pub fn constrain_delta(mesh: &Mesh, v: VertexId, delta: Vec3, constraint: Constraint) -> Vec3 {
    if delta.length_squared() < 1e-14 {
        return delta;
    }
    let p = mesh.vertex_position(v).unwrap_or_default();
    match constraint {
        Constraint::None => delta,
        Constraint::Edges => {
            let mut best = (0.0f32, Vec3::ZERO);
            for n in vertex_neighbours(mesh, v) {
                let e = (mesh.vertex_position(n).unwrap_or(p) - p).normalize_or_zero();
                let d = delta.dot(e);
                if d > best.0 {
                    best = (d, e);
                }
            }
            if best.0 > 0.0 { best.1 * best.0 } else { Vec3::ZERO }
        }
        Constraint::Faces => {
            let n = vertex_normal(mesh, v);
            delta - n * delta.dot(n)
        }
        Constraint::Normals => {
            let n = vertex_normal(mesh, v);
            n * delta.dot(n)
        }
    }
}

fn vertex_normal(mesh: &Mesh, v: VertexId) -> Vec3 {
    let mut n = Vec3::ZERO;
    for f in mesh.faces_around_vertex(v) {
        n += mesh.face_normal(f);
    }
    n.normalize_or_zero()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::standard_primitives::{box_segmented, plane_segmented};

    #[test]
    fn attach_keeps_world_positions() {
        let mut a = box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
        let b = box_segmented(4.0, 4.0, 4.0, [1, 1, 1]);
        attach(&mut a, &b, Mat4::from_translation(Vec3::new(30.0, 0.0, 0.0))).unwrap();
        assert_eq!(a.face_count(), 12);
        assert_eq!(a.vertex_count(), 16);
        let xs: Vec<f32> = (8..16).map(|v| a.vertex_position(VertexId(v)).unwrap().x).collect();
        assert!(xs.iter().all(|x| (x - 30.0).abs() <= 2.0 + 1e-4), "{xs:?}");
        a.check_invariants().unwrap();
        assert_eq!(crate::select_ops::element_of(&a, FaceId(6)).len(), 6);
    }

    #[test]
    fn detach_as_clone_keeps_source() {
        let mut m = box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
        let top: Vec<FaceId> = (0..6).map(FaceId).filter(|f| m.face_normal(*f).z > 0.9).collect();
        let part = detach(&mut m, &top, true).unwrap();
        assert_eq!(part.face_count(), 1);
        assert_eq!(part.vertex_count(), 4);
        assert_eq!(m.face_count(), 6, "clone keeps the source");
        let part2 = detach(&mut m, &top, false).unwrap();
        assert_eq!(part2.face_count(), 1);
        assert_eq!(m.face_count(), 5);
        m.check_invariants().unwrap();
        let mut e = box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
        detach_to_element(&mut e, &top).unwrap();
        assert_eq!(e.face_count(), 6);
        assert_eq!(e.vertex_count(), 12, "the top gets its own four vertices");
        assert_eq!(crate::select_ops::element_of(&e, top[0]).len(), 1);
    }

    #[test]
    fn slice_box_adds_loop() {
        let mut m = box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
        let edges = slice(&mut m, Vec3::new(0.0, 0.0, 4.0), Vec3::Z, None, false).unwrap();
        assert_eq!(edges.len(), 4);
        assert_eq!(m.face_count(), 10);
        assert_eq!(m.vertex_count(), 12);
        m.check_invariants().unwrap();
        for e in &edges {
            let (a, b) = m.edge_vertices(*e).unwrap();
            assert!((m.vertex_position(a).unwrap().z - 4.0).abs() < 1e-4);
            assert!((m.vertex_position(b).unwrap().z - 4.0).abs() < 1e-4);
        }
        // Split tears the box into two closed shells (the new edges open).
        let mut s = box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
        slice(&mut s, Vec3::new(0.0, 0.0, 4.0), Vec3::Z, None, true).unwrap();
        assert_eq!(s.vertex_count(), 16);
        assert_eq!(crate::select_ops::element_of(&s, FaceId(0)).len(), 5);
    }

    #[test]
    fn quickslice_through_plane() {
        // A tilted plane through a 2x2 plane cuts only the polygons it crosses.
        let mut m = plane_segmented(20.0, 20.0, 2, 2);
        let faces: Vec<FaceId> = (0..4).map(FaceId).collect();
        let edges = slice(&mut m, Vec3::new(3.0, 0.0, 0.0), Vec3::X, Some(&faces), false).unwrap();
        assert_eq!(edges.len(), 2);
        assert_eq!(m.face_count(), 6);
        m.check_invariants().unwrap();
    }

    #[test]
    fn cut_between_two_face_points() {
        let mut m = plane_segmented(20.0, 20.0, 2, 2);
        // From the left edge of the bottom-left polygon to the right edge of the bottom-right one.
        let from = SurfacePoint { face: FaceId(0), point: Vec3::new(-10.0, -7.0, 0.0) };
        let bl = (0..4u32).map(FaceId).find(|f| m.face_positions(*f).unwrap().iter().all(|p| p.x <= 1e-4 && p.y <= 1e-4)).unwrap();
        let br = (0..4u32).map(FaceId).find(|f| m.face_positions(*f).unwrap().iter().all(|p| p.x >= -1e-4 && p.y <= 1e-4)).unwrap();
        let from = SurfacePoint { face: bl, ..from };
        let to = SurfacePoint { face: br, point: Vec3::new(10.0, -3.0, 0.0) };
        let edges = cut(&mut m, from, to, Vec3::Z).unwrap();
        assert_eq!(edges.len(), 2, "one new edge per crossed polygon");
        assert_eq!(m.face_count(), 6);
        m.check_invariants().unwrap();
    }

    #[test]
    fn msmooth_keeps_open_border_and_attributes() {
        let mut m = plane_segmented(10.0, 10.0, 1, 1);
        m.faces_mut()[0].material_id = 3;
        let s = msmooth(&m, 1.0, false, false).unwrap();
        assert_eq!(s.face_count(), 4);
        assert_eq!(s.vertex_count(), 9);
        s.check_invariants().unwrap();
        assert!(s.faces().iter().all(|f| f.material_id == 3));
        // Corners of an open quad stay (two crease edges each → 6/8 + 1/8 + 1/8 on a straight corner keeps the corner on its edges).
        let corners: Vec<Vec3> = (0..4).map(|v| s.vertex_position(VertexId(v)).unwrap()).collect();
        assert!(corners.iter().all(|c| c.x.abs() > 3.7 && c.y.abs() > 3.7), "{corners:?}");
        let b = box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
        let sb = msmooth(&b, 1.0, false, false).unwrap();
        assert_eq!(sb.face_count(), 24);
        let r = sb.vertex_position(VertexId(0)).unwrap();
        assert!(r.length() < Vec3::new(5.0, 5.0, 0.0).length(), "corners pull in");
        let s0 = msmooth(&b, 0.0, false, false).unwrap();
        assert_eq!(s0.vertex_position(VertexId(0)).unwrap(), b.vertex_position(VertexId(0)).unwrap(), "smoothness 0 keeps positions");
    }

    #[test]
    fn tessellate_face_kind_quadruples() {
        let mut m = box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
        tessellate(&mut m, &[FaceId(0)], TessKind::Face, 0.0).unwrap();
        assert_eq!(m.face_count(), 9);
        m.check_invariants().unwrap();
        let mut e = box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
        tessellate(&mut e, &[FaceId(0)], TessKind::Edge, 0.0).unwrap();
        // 4 corner triangles + the diamond; the four neighbours got the midpoints.
        assert_eq!(e.face_count(), 10);
        assert_eq!(e.vertex_count(), 12);
        e.check_invariants().unwrap();
    }

    #[test]
    fn relax_moves_toward_neighbours_holds_boundary() {
        let mut m = plane_segmented(20.0, 20.0, 2, 2);
        let mut im = IndexedMesh::from_mesh(&m);
        let centre = (0..9).find(|i| im.positions[*i].length() < 1e-3).unwrap();
        im.positions[centre].z = 4.0;
        m = im.build().unwrap();
        let all: Vec<VertexId> = (0..9).map(VertexId).collect();
        relax(&mut m, &all, 1.0, 1, true, false).unwrap();
        assert!((m.vertex_position(VertexId(centre as u32)).unwrap().z).abs() < 1e-4, "the centre drops to its neighbours");
        assert!((0..9).filter(|i| *i != centre).all(|i| m.vertex_position(VertexId(i as u32)).unwrap().z.abs() < 1e-4), "borders held");
    }

    #[test]
    fn constrain_to_edges_keeps_vertex_on_edge() {
        let m = plane_segmented(20.0, 20.0, 2, 2);
        let centre = (0..9).map(VertexId).find(|v| m.vertex_position(*v).unwrap().length() < 1e-3).unwrap();
        let d = constrain_delta(&m, centre, Vec3::new(3.0, 1.0, 2.0), Constraint::Edges);
        assert!((d - Vec3::new(3.0, 0.0, 0.0)).length() < 1e-4, "{d:?}");
        let f = constrain_delta(&m, centre, Vec3::new(3.0, 1.0, 2.0), Constraint::Faces);
        assert!((f - Vec3::new(3.0, 1.0, 0.0)).length() < 1e-4, "{f:?}");
        let n = constrain_delta(&m, centre, Vec3::new(3.0, 1.0, 2.0), Constraint::Normals);
        assert!((n - Vec3::new(0.0, 0.0, 2.0)).length() < 1e-4, "{n:?}");
    }
}
