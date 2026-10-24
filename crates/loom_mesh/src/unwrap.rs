//! Unwrap UVW: pure texture-coordinate editing over one map channel of a
//! mesh — break / weld / stitch, transforms, alignment, Relax, Pack,
//! Flatten / Normal / Unfold / Pelt mapping, seams and overlap checks.
//!
//! Texture vertices (tvs) are indices into `MapChannel::verts`; a corner is
//! a face half-edge. Functions take and return the channel's data through
//! the mesh so the topology is always at hand.

use crate::half_edge::MapChannel;
use crate::uvmap::{project_faces, MapParams, Projection};
use crate::{FaceId, HalfEdgeId, Mesh, VertexId};
use glam::{Mat4, Vec2, Vec3};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

fn uv2(v: Vec3) -> Vec2 {
    Vec2::new(v.x, v.y)
}

/// The tv at a corner.
pub fn corner_tv(mesh: &Mesh, ch: u32, he: HalfEdgeId) -> Option<u32> {
    mesh.map(ch)?.corners.get(he.0 as usize).copied()
}

/// A face's tvs in loop order.
pub fn face_tvs(mesh: &Mesh, ch: u32, f: FaceId) -> Vec<u32> {
    mesh.face_corners(f).unwrap_or_default().into_iter().filter_map(|he| corner_tv(mesh, ch, he)).collect()
}

/// Every face's tvs (index = face id).
pub fn tv_faces(mesh: &Mesh, ch: u32) -> Vec<Vec<u32>> {
    (0..mesh.face_count()).map(|f| face_tvs(mesh, ch, FaceId(f as u32))).collect()
}

/// Unique undirected tv edges (min, max).
pub fn tv_edges(mesh: &Mesh, ch: u32) -> Vec<(u32, u32)> {
    let mut set = BTreeSet::new();
    for tvs in tv_faces(mesh, ch) {
        for i in 0..tvs.len() {
            let (a, b) = (tvs[i], tvs[(i + 1) % tvs.len()]);
            if a != b {
                set.insert((a.min(b), a.max(b)));
            }
        }
    }
    set.into_iter().collect()
}

/// The tvs used by the corners of `faces`.
pub fn tvs_of_faces(mesh: &Mesh, ch: u32, faces: &[FaceId]) -> BTreeSet<u32> {
    faces.iter().flat_map(|f| face_tvs(mesh, ch, *f)).collect()
}

/// Faces whose every tv is in `tvs` (vertex → polygon selection).
pub fn faces_of_tvs(mesh: &Mesh, ch: u32, tvs: &BTreeSet<u32>) -> Vec<FaceId> {
    (0..mesh.face_count() as u32).map(FaceId).filter(|f| { let t = face_tvs(mesh, ch, *f); !t.is_empty() && t.iter().all(|v| tvs.contains(v)) }).collect()
}

/// Tv edges whose both tvs are in `tvs`.
pub fn edges_of_tvs(mesh: &Mesh, ch: u32, tvs: &BTreeSet<u32>) -> Vec<(u32, u32)> {
    tv_edges(mesh, ch).into_iter().filter(|(a, b)| tvs.contains(a) && tvs.contains(b)).collect()
}

/// Corners (face half-edges) using tv `tv`.
fn corners_of_tv(mesh: &Mesh, ch: u32, tv: u32) -> Vec<HalfEdgeId> {
    let Some(m) = mesh.map(ch) else { return Vec::new() };
    (0..m.corners.len()).filter(|i| m.corners[*i] == tv && mesh.half_edges()[*i].face.is_some()).map(|i| HalfEdgeId(i as u32)).collect()
}

/// Drops texture vertices no corner uses, renumbering the rest.
pub fn compact(ch: &mut MapChannel) {
    ch.compact();
}

/// Gives `faces` fresh texture vertices from `uv_of` (one per vertex and
/// distinct value, so equal values at a vertex stay welded and a seam's two
/// values split); the faces outside keep theirs.
pub fn assign_corner_uvs(mesh: &mut Mesh, ch: u32, faces: &[FaceId], uv_of: impl Fn(HalfEdgeId) -> Vec3) {
    let mut writes: Vec<(HalfEdgeId, VertexId, Vec3)> = Vec::new();
    for f in faces {
        for he in mesh.face_corners(*f).unwrap_or_default() {
            let v = mesh.half_edges()[he.0 as usize].origin;
            writes.push((he, v, uv_of(he)));
        }
    }
    let mut map = mesh.ensure_map(ch).clone();
    let mut by_key: HashMap<(u32, [u32; 3]), u32> = HashMap::new();
    for (he, v, uv) in writes {
        let key = (v.0, [uv.x.to_bits(), uv.y.to_bits(), uv.z.to_bits()]);
        let tv = *by_key.entry(key).or_insert_with(|| {
            map.verts.push(uv);
            (map.verts.len() - 1) as u32
        });
        map.corners[he.0 as usize] = tv;
    }
    mesh.set_map(ch, map);
}

/// Gives every corner of a tv that lies on a different side of a cut its own
/// tv: corners at one tv stay joined only while they are connected through
/// face edges that are not in `cuts` (geometry edges as canonical
/// half-edge ids). The core of Break, Flip and Detach.
pub fn split_at(mesh: &mut Mesh, ch: u32, tvs: &BTreeSet<u32>, cuts: &HashSet<HalfEdgeId>) {
    let n_he = mesh.half_edge_count();
    let Some(map) = mesh.map(ch).cloned() else { return };
    let mut out = map.clone();
    for tv in tvs {
        let corners = corners_of_tv(mesh, ch, *tv);
        if corners.len() < 2 {
            continue;
        }
        // Union corners joined across an uncut edge: corner `he` (origin v)
        // joins the corner of the twin's next (same vertex v in the other face)
        // via edge he; and via its prev edge likewise.
        let idx: HashMap<HalfEdgeId, usize> = corners.iter().enumerate().map(|(i, h)| (*h, i)).collect();
        let mut parent: Vec<usize> = (0..corners.len()).collect();
        fn find(p: &mut [usize], i: usize) -> usize {
            let mut r = i;
            while p[r] != r {
                r = p[r];
            }
            let mut c = i;
            while p[c] != r {
                let n = p[c];
                p[c] = r;
                c = n;
            }
            r
        }
        for (i, he) in corners.iter().enumerate() {
            let h = &mesh.half_edges()[he.0 as usize];
            // Edge leaving v along `he`: the other face's corner at v is twin.next.
            let twin = h.twin;
            let other = mesh.half_edges()[twin.0 as usize].next;
            let canon = mesh.canonical_edge_id(*he).unwrap_or(*he);
            if mesh.half_edges()[twin.0 as usize].face.is_some() && !cuts.contains(&canon) {
                if let Some(j) = idx.get(&other) {
                    let (a, b) = (find(&mut parent, i), find(&mut parent, *j));
                    parent[a] = b;
                }
            }
            // Edge arriving at v along prev: the other face's corner is prev.twin.
            let prev = h.prev;
            let ptwin = mesh.half_edges()[prev.0 as usize].twin;
            let canon_p = mesh.canonical_edge_id(prev).unwrap_or(prev);
            if mesh.half_edges()[ptwin.0 as usize].face.is_some() && !cuts.contains(&canon_p) {
                if let Some(j) = idx.get(&ptwin) {
                    let (a, b) = (find(&mut parent, i), find(&mut parent, *j));
                    parent[a] = b;
                }
            }
        }
        let mut group_tv: HashMap<usize, u32> = HashMap::new();
        for (i, he) in corners.iter().enumerate() {
            let r = find(&mut parent, i);
            let new = match group_tv.get(&r) {
                Some(n) => *n,
                None => {
                    let n = if group_tv.is_empty() {
                        *tv
                    } else {
                        out.verts.push(map.verts[*tv as usize]);
                        (out.verts.len() - 1) as u32
                    };
                    group_tv.insert(r, n);
                    n
                }
            };
            out.corners[he.0 as usize] = new;
        }
    }
    debug_assert_eq!(out.corners.len(), n_he);
    mesh.set_map(ch, out);
}

/// Break: the selected tvs split so every corner (face) gets its own.
pub fn break_tvs(mesh: &mut Mesh, ch: u32, tvs: &BTreeSet<u32>) {
    let cuts: HashSet<HalfEdgeId> = (0..mesh.half_edge_count() as u32).map(HalfEdgeId).filter_map(|h| mesh.canonical_edge_id(h)).collect();
    split_at(mesh, ch, tvs, &cuts);
}

/// Break at the edge level: the faces on either side of the selected tv
/// edges separate.
pub fn break_edges(mesh: &mut Mesh, ch: u32, edges: &[(u32, u32)]) {
    let sel: HashSet<(u32, u32)> = edges.iter().map(|(a, b)| (*a.min(b), *a.max(b))).collect();
    let mut cuts = HashSet::new();
    let mut tvs = BTreeSet::new();
    for f in 0..mesh.face_count() {
        let Some(corners) = mesh.face_corners(FaceId(f as u32)) else { continue };
        for (i, he) in corners.iter().enumerate() {
            let a = corner_tv(mesh, ch, *he).unwrap_or(0);
            let b = corner_tv(mesh, ch, corners[(i + 1) % corners.len()]).unwrap_or(0);
            if sel.contains(&(a.min(b), a.max(b))) {
                cuts.insert(mesh.canonical_edge_id(*he).unwrap_or(*he));
                tvs.insert(a);
                tvs.insert(b);
            }
        }
    }
    split_at(mesh, ch, &tvs, &cuts);
}

/// Break at the face level (Detach Edge Verts): the faces separate from
/// their neighbours along their boundary.
pub fn detach_faces(mesh: &mut Mesh, ch: u32, faces: &[FaceId]) {
    let set: HashSet<FaceId> = faces.iter().copied().collect();
    let mut cuts = HashSet::new();
    let mut tvs = BTreeSet::new();
    for f in faces {
        let Some(corners) = mesh.face_corners(*f) else { continue };
        for he in corners {
            let twin = mesh.half_edges()[he.0 as usize].twin;
            let other = mesh.half_edges()[twin.0 as usize].face;
            if other.is_none_or(|o| !set.contains(&o)) {
                cuts.insert(mesh.canonical_edge_id(he).unwrap_or(he));
                tvs.insert(corner_tv(mesh, ch, he).unwrap_or(0));
                let next = mesh.half_edges()[he.0 as usize].next;
                tvs.insert(corner_tv(mesh, ch, next).unwrap_or(0));
            }
        }
    }
    split_at(mesh, ch, &tvs, &cuts);
}

/// Weld Selected: selected tvs within `threshold` (UV distance) of each
/// other merge at their average, whichever vertices they sit on (any selected
/// texture vertices within the threshold weld).
pub fn weld_tvs(mesh: &mut Mesh, ch: u32, tvs: &BTreeSet<u32>, threshold: f32) -> usize {
    let Some(map) = mesh.map(ch).cloned() else { return 0 };
    let mut out = map.clone();
    let mut welded = 0;
    let mut remaining: Vec<u32> = tvs.iter().copied().filter(|t| (*t as usize) < map.verts.len()).collect();
    while let Some(seed) = remaining.pop() {
        let mut cluster = vec![seed];
        let mut i = 0;
        while i < remaining.len() {
            if cluster.iter().any(|c| uv2(map.verts[*c as usize]).distance(uv2(map.verts[remaining[i] as usize])) <= threshold) {
                cluster.push(remaining.remove(i));
                i = 0;
            } else {
                i += 1;
            }
        }
        if cluster.len() > 1 {
            let avg = cluster.iter().map(|c| map.verts[*c as usize]).sum::<Vec3>() / cluster.len() as f32;
            let target = cluster[0];
            out.verts[target as usize] = avg;
            for c in out.corners.iter_mut() {
                if cluster.contains(c) {
                    *c = target;
                }
            }
            welded += cluster.len() - 1;
        }
    }
    compact(&mut out);
    mesh.set_map(ch, out);
    welded
}

/// Target Weld: tv `from` merges into `to` (at `to`'s position).
pub fn target_weld(mesh: &mut Mesh, ch: u32, from: u32, to: u32) -> Result<(), String> {
    let Some(map) = mesh.map(ch).cloned() else { return Err("no map channel".into()) };
    if from as usize >= map.verts.len() || to as usize >= map.verts.len() {
        return Err("no such texture vertex".into());
    }
    let mut out = map;
    for c in out.corners.iter_mut() {
        if *c == from {
            *c = to;
        }
    }
    compact(&mut out);
    mesh.set_map(ch, out);
    Ok(())
}

/// Stitch Selected: for every geometry edge that is a seam in UV (its two
/// sides use different tvs) and touches the selected tvs, the far side moves
/// to the selected side (`bias` 0) … to its own place (`bias` 1), or both
/// meet half way with `average`; then the pairs weld.
pub fn stitch(mesh: &mut Mesh, ch: u32, tvs: &BTreeSet<u32>, bias: f32, average: bool) -> usize {
    let Some(map) = mesh.map(ch).cloned() else { return 0 };
    let mut out = map.clone();
    let mut merged: Vec<(u32, u32)> = Vec::new(); // (keep, drop)
    for i in 0..mesh.half_edge_count() {
        let he = &mesh.half_edges()[i];
        let Some(_) = he.face else { continue };
        let twin = &mesh.half_edges()[he.twin.0 as usize];
        if twin.face.is_none() || he.twin.0 < i as u32 {
            continue;
        }
        let a0 = map.corners[i];
        let a1 = map.corners[he.next.0 as usize];
        let b1 = map.corners[he.twin.0 as usize]; // twin origin = he's destination
        let b0 = map.corners[twin.next.0 as usize]; // twin's next origin = he's origin
        if a0 == b0 && a1 == b1 {
            continue;
        }
        let a_sel = tvs.contains(&a0) || tvs.contains(&a1);
        let b_sel = tvs.contains(&b0) || tvs.contains(&b1);
        if !a_sel && !b_sel {
            continue;
        }
        // The selected side is the source; the other side stitches onto it.
        let (src, dst) = if a_sel { ([a0, a1], [b0, b1]) } else { ([b0, b1], [a0, a1]) };
        for k in 0..2 {
            if src[k] == dst[k] {
                continue;
            }
            let (ps, pd) = (map.verts[src[k] as usize], map.verts[dst[k] as usize]);
            let target = if average { (ps + pd) * 0.5 } else { ps.lerp(pd, bias.clamp(0.0, 1.0)) };
            out.verts[src[k] as usize] = target;
            out.verts[dst[k] as usize] = target;
            merged.push((src[k], dst[k]));
        }
    }
    let n = merged.len();
    for (keep, drop) in merged {
        for c in out.corners.iter_mut() {
            if *c == drop {
                *c = keep;
            }
        }
    }
    compact(&mut out);
    mesh.set_map(ch, out);
    n
}

/// Selection centre in UV.
pub fn centre(mesh: &Mesh, ch: u32, tvs: &BTreeSet<u32>) -> Vec2 {
    let Some(m) = mesh.map(ch) else { return Vec2::ZERO };
    if tvs.is_empty() {
        return Vec2::ZERO;
    }
    let (mut lo, mut hi) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
    for t in tvs {
        if let Some(v) = m.verts.get(*t as usize) {
            lo = lo.min(uv2(*v));
            hi = hi.max(uv2(*v));
        }
    }
    (lo + hi) * 0.5
}

/// UV bounds of tvs (or of the whole channel).
pub fn bounds(mesh: &Mesh, ch: u32, tvs: Option<&BTreeSet<u32>>) -> (Vec2, Vec2) {
    let Some(m) = mesh.map(ch) else { return (Vec2::ZERO, Vec2::ONE) };
    let (mut lo, mut hi) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
    let mut any = false;
    for (i, v) in m.verts.iter().enumerate() {
        if tvs.is_some_and(|s| !s.contains(&(i as u32))) {
            continue;
        }
        any = true;
        lo = lo.min(uv2(*v));
        hi = hi.max(uv2(*v));
    }
    if any { (lo, hi) } else { (Vec2::ZERO, Vec2::ONE) }
}

/// Applies `f` to every selected tv (with soft weights when given: 1 = full).
pub fn transform_tvs(mesh: &mut Mesh, ch: u32, tvs: &BTreeSet<u32>, weights: Option<&HashMap<u32, f32>>, f: impl Fn(Vec2) -> Vec2) {
    let Some(m) = mesh.map_mut(ch) else { return };
    let apply = |m: &mut MapChannel, t: u32, w: f32| {
        if let Some(v) = m.verts.get_mut(t as usize) {
            let p = uv2(*v);
            let q = f(p);
            let r = p.lerp(q, w);
            *v = Vec3::new(r.x, r.y, v.z);
        }
    };
    for t in tvs {
        apply(m, *t, 1.0);
    }
    if let Some(w) = weights {
        for (t, k) in w {
            if !tvs.contains(t) {
                apply(m, *t, k.clamp(0.0, 1.0));
            }
        }
    }
}

pub fn move_tvs(mesh: &mut Mesh, ch: u32, tvs: &BTreeSet<u32>, weights: Option<&HashMap<u32, f32>>, delta: Vec2) {
    transform_tvs(mesh, ch, tvs, weights, |p| p + delta);
}

pub fn rotate_tvs(mesh: &mut Mesh, ch: u32, tvs: &BTreeSet<u32>, weights: Option<&HashMap<u32, f32>>, degrees: f32, pivot: Vec2) {
    let (s, c) = degrees.to_radians().sin_cos();
    transform_tvs(mesh, ch, tvs, weights, |p| {
        let d = p - pivot;
        pivot + Vec2::new(d.x * c - d.y * s, d.x * s + d.y * c)
    });
}

pub fn scale_tvs(mesh: &mut Mesh, ch: u32, tvs: &BTreeSet<u32>, weights: Option<&HashMap<u32, f32>>, factor: Vec2, pivot: Vec2) {
    transform_tvs(mesh, ch, tvs, weights, |p| pivot + (p - pivot) * factor);
}

/// Mirror in place about the selection centre (horizontal = flip u).
pub fn mirror_tvs(mesh: &mut Mesh, ch: u32, tvs: &BTreeSet<u32>, horizontal: bool) {
    let c = centre(mesh, ch, tvs);
    transform_tvs(mesh, ch, tvs, None, |p| if horizontal { Vec2::new(2.0 * c.x - p.x, p.y) } else { Vec2::new(p.x, 2.0 * c.y - p.y) });
}

/// Flip: the selected faces detach from their neighbours, then mirror.
pub fn flip_faces(mesh: &mut Mesh, ch: u32, faces: &[FaceId], horizontal: bool) {
    detach_faces(mesh, ch, faces);
    let tvs = tvs_of_faces(mesh, ch, faces);
    mirror_tvs(mesh, ch, &tvs, horizontal);
}

/// Align Horizontally / Vertically: all selected tvs onto the average v / u.
pub fn align(mesh: &mut Mesh, ch: u32, tvs: &BTreeSet<u32>, horizontal: bool, at: Option<f32>) {
    let Some(m) = mesh.map(ch) else { return };
    if tvs.is_empty() {
        return;
    }
    let avg = at.unwrap_or_else(|| tvs.iter().map(|t| if horizontal { m.verts[*t as usize].y } else { m.verts[*t as usize].x }).sum::<f32>() / tvs.len() as f32);
    transform_tvs(mesh, ch, tvs, None, |p| if horizontal { Vec2::new(p.x, avg) } else { Vec2::new(avg, p.y) });
}

/// Linear Align: the selection projects onto its best-fit line.
pub fn linear_align(mesh: &mut Mesh, ch: u32, tvs: &BTreeSet<u32>) {
    let Some(m) = mesh.map(ch) else { return };
    if tvs.len() < 2 {
        return;
    }
    let pts: Vec<Vec2> = tvs.iter().map(|t| uv2(m.verts[*t as usize])).collect();
    let mean = pts.iter().sum::<Vec2>() / pts.len() as f32;
    let (mut sxx, mut sxy, mut syy) = (0.0f32, 0.0f32, 0.0f32);
    for p in &pts {
        let d = *p - mean;
        sxx += d.x * d.x;
        sxy += d.x * d.y;
        syy += d.y * d.y;
    }
    // Principal direction of the 2x2 covariance.
    let theta = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    let dir = Vec2::new(theta.cos(), theta.sin());
    transform_tvs(mesh, ch, tvs, None, |p| mean + dir * (p - mean).dot(dir));
}

/// Space Horizontally / Vertically: the selected tvs spread evenly between
/// the two extremes, keeping their order.
pub fn space(mesh: &mut Mesh, ch: u32, tvs: &BTreeSet<u32>, horizontal: bool) {
    let Some(m) = mesh.map(ch).cloned() else { return };
    if tvs.len() < 3 {
        return;
    }
    let mut order: Vec<u32> = tvs.iter().copied().collect();
    let key = |t: &u32| if horizontal { m.verts[*t as usize].x } else { m.verts[*t as usize].y };
    order.sort_by(|a, b| key(a).total_cmp(&key(b)));
    let (lo, hi) = (key(&order[0]), key(order.last().unwrap()));
    let n = order.len();
    let out = mesh.map_mut(ch).unwrap();
    for (i, t) in order.iter().enumerate() {
        let v = lo + (hi - lo) * i as f32 / (n - 1) as f32;
        let p = &mut out.verts[*t as usize];
        if horizontal {
            p.x = v;
        } else {
            p.y = v;
        }
    }
}

/// Straighten Edges: each selected tv edge snaps to horizontal or vertical,
/// whichever it is closer to (the second tv moves).
pub fn straighten_edges(mesh: &mut Mesh, ch: u32, edges: &[(u32, u32)]) {
    let Some(m) = mesh.map_mut(ch) else { return };
    for (a, b) in edges {
        let (pa, pb) = (uv2(m.verts[*a as usize]), uv2(m.verts[*b as usize]));
        let d = pb - pa;
        let q = &mut m.verts[*b as usize];
        if d.x.abs() >= d.y.abs() {
            q.y = pa.y;
        } else {
            q.x = pa.x;
        }
    }
}

/// Relax methods (Relax dialog > Relax By).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelaxBy {
    FaceAngles,
    EdgeAngles,
    Centers,
}

/// Relax: iterations of a weighted Laplacian pull toward the neighbours.
/// `stretch` (0–1) blends the 3D-length-aware weights with uniform ones;
/// `keep_boundary` pins the tvs on open UV edges. `tvs` None = all.
#[allow(clippy::too_many_arguments)]
pub fn relax(mesh: &mut Mesh, ch: u32, tvs: Option<&BTreeSet<u32>>, iterations: u32, amount: f32, stretch: f32, by: RelaxBy, keep_boundary: bool) {
    let Some(m) = mesh.map(ch).cloned() else { return };
    let faces = tv_faces(mesh, ch);
    // Neighbour weights per tv: from the face corners (3D edge lengths / angles).
    let mut nbrs: Vec<HashMap<u32, f32>> = vec![HashMap::new(); m.verts.len()];
    let mut edge_count: HashMap<(u32, u32), u32> = HashMap::new();
    for (fi, tvs_f) in faces.iter().enumerate() {
        let n = tvs_f.len();
        if n < 3 {
            continue;
        }
        let pts = mesh.face_positions(FaceId(fi as u32)).unwrap_or_default();
        for i in 0..n {
            let (a, b) = (tvs_f[i], tvs_f[(i + 1) % n]);
            if a == b {
                continue;
            }
            *edge_count.entry((a.min(b), a.max(b))).or_default() += 1;
            let len = pts[i].distance(pts[(i + 1) % n]).max(1e-6);
            let w = match by {
                RelaxBy::Centers => 1.0,
                RelaxBy::EdgeAngles => 1.0 / len,
                RelaxBy::FaceAngles => {
                    // Cotangent of the angle opposite the edge in the face's triangle fan.
                    let c = pts[(i + 2) % n];
                    let (u, v) = (pts[i] - c, pts[(i + 1) % n] - c);
                    let cot = u.dot(v) / u.cross(v).length().max(1e-6);
                    cot.clamp(0.05, 20.0)
                }
            };
            let w = w * (1.0 - stretch) + stretch;
            *nbrs[a as usize].entry(b).or_default() += w;
            *nbrs[b as usize].entry(a).or_default() += w;
        }
    }
    let boundary: HashSet<u32> = edge_count.iter().filter(|(_, c)| **c == 1).flat_map(|((a, b), _)| [*a, *b]).collect();
    let mut verts = m.verts.clone();
    for _ in 0..iterations {
        let prev = verts.clone();
        for (t, nb) in nbrs.iter().enumerate() {
            let t32 = t as u32;
            if nb.is_empty() || tvs.is_some_and(|s| !s.contains(&t32)) || (keep_boundary && boundary.contains(&t32)) {
                continue;
            }
            let total: f32 = nb.values().sum();
            if total <= 0.0 {
                continue;
            }
            let target = nb.iter().map(|(o, w)| uv2(prev[*o as usize]) * *w).sum::<Vec2>() / total;
            let p = uv2(prev[t]);
            let q = p.lerp(target, amount.clamp(0.0, 1.0));
            verts[t] = Vec3::new(q.x, q.y, prev[t].z);
        }
    }
    let out = mesh.map_mut(ch).unwrap();
    out.verts = verts;
}

/// Connected groups of faces (joined through shared tv edges).
pub fn clusters(mesh: &Mesh, ch: u32, faces: &[FaceId]) -> Vec<Vec<FaceId>> {
    let set: HashSet<FaceId> = faces.iter().copied().collect();
    let tvf = tv_faces(mesh, ch);
    let mut by_edge: HashMap<(u32, u32), Vec<FaceId>> = HashMap::new();
    for f in faces {
        let t = &tvf[f.0 as usize];
        for i in 0..t.len() {
            let (a, b) = (t[i], t[(i + 1) % t.len()]);
            by_edge.entry((a.min(b), a.max(b))).or_default().push(*f);
        }
    }
    let mut seen: HashSet<FaceId> = HashSet::new();
    let mut out = Vec::new();
    for f in faces {
        if !seen.insert(*f) {
            continue;
        }
        let mut comp = vec![*f];
        let mut queue = VecDeque::from([*f]);
        while let Some(cur) = queue.pop_front() {
            let t = &tvf[cur.0 as usize];
            for i in 0..t.len() {
                let (a, b) = (t[i], t[(i + 1) % t.len()]);
                for o in by_edge.get(&(a.min(b), a.max(b))).into_iter().flatten() {
                    if set.contains(o) && seen.insert(*o) {
                        comp.push(*o);
                        queue.push_back(*o);
                    }
                }
            }
        }
        out.push(comp);
    }
    out
}

/// Pack methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackMethod {
    Linear,
    Recursive,
}

/// Pack UVs: the clusters of `faces` are laid out without overlap. Linear
/// packs rows (shelves); Recursive keeps a list of free rectangles and
/// fills holes. `spacing` is the gap (0–1 of the square), `normalize`
/// scales the result into 0–1, `rotate` turns tall clusters on their side.
#[allow(clippy::too_many_arguments)]
pub fn pack(mesh: &mut Mesh, ch: u32, faces: &[FaceId], method: PackMethod, spacing: f32, normalize: bool, rotate: bool, fill_holes: bool) {
    let groups = clusters(mesh, ch, faces);
    if groups.is_empty() {
        return;
    }
    // Per cluster: tvs, bounds.
    struct Item {
        tvs: BTreeSet<u32>,
        size: Vec2,
        lo: Vec2,
        rotated: bool,
    }
    let mut items: Vec<Item> = groups
        .iter()
        .map(|g| {
            let tvs = tvs_of_faces(mesh, ch, g);
            let (lo, hi) = bounds(mesh, ch, Some(&tvs));
            let mut size = (hi - lo).max(Vec2::splat(1e-6));
            let mut rotated = false;
            if rotate && size.y > size.x * 1.25 {
                size = Vec2::new(size.y, size.x);
                rotated = true;
            }
            Item { tvs, size, lo, rotated }
        })
        .collect();
    // Scale so the total area fits a unit square with room.
    let area: f32 = items.iter().map(|i| (i.size.x + spacing) * (i.size.y + spacing)).sum();
    let scale = if normalize { (0.9 / area.sqrt().max(1e-6)).min(1e6) } else { 1.0 };
    for it in &mut items {
        it.size *= scale;
    }
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|a, b| items[*b].size.y.total_cmp(&items[*a].size.y));
    let mut placed: Vec<Vec2> = vec![Vec2::ZERO; items.len()];
    let width = 1.0f32;
    match (method, fill_holes) {
        (PackMethod::Linear, _) | (PackMethod::Recursive, false) => {
            // Shelves.
            let (mut x, mut y, mut shelf_h) = (0.0f32, 0.0f32, 0.0f32);
            for i in order {
                let s = items[i].size + Vec2::splat(spacing);
                if x + s.x > width && x > 0.0 {
                    x = 0.0;
                    y += shelf_h;
                    shelf_h = 0.0;
                }
                placed[i] = Vec2::new(x, y);
                x += s.x;
                shelf_h = shelf_h.max(s.y);
            }
        }
        (PackMethod::Recursive, true) => {
            // Guillotine free rectangles: best short-side fit, split, fill holes.
            let mut free: Vec<(Vec2, Vec2)> = vec![(Vec2::ZERO, Vec2::new(width, 1e9))];
            for i in order {
                let s = items[i].size + Vec2::splat(spacing);
                let mut best: Option<(usize, f32)> = None;
                for (k, (_, fs)) in free.iter().enumerate() {
                    if fs.x >= s.x && fs.y >= s.y {
                        let score = (fs.x - s.x).min(fs.y - s.y);
                        if best.is_none_or(|(_, b)| score < b) {
                            best = Some((k, score));
                        }
                    }
                }
                let Some((k, _)) = best else { continue };
                let (fo, fs) = free.remove(k);
                placed[i] = fo;
                // Split the rest into two rectangles (shorter leftover first).
                let right = (Vec2::new(fo.x + s.x, fo.y), Vec2::new(fs.x - s.x, s.y));
                let below = (Vec2::new(fo.x, fo.y + s.y), Vec2::new(fs.x, fs.y - s.y));
                if right.1.x > 1e-6 && right.1.y > 1e-6 {
                    free.push(right);
                }
                if below.1.x > 1e-6 && below.1.y > 1e-6 {
                    free.push(below);
                }
                free.sort_by(|a, b| (a.0.y, a.0.x).partial_cmp(&(b.0.y, b.0.x)).unwrap_or(std::cmp::Ordering::Equal));
            }
        }
    }
    // Apply: move each cluster so its (rotated) bounds start at `placed`.
    let (mut lo_all, mut hi_all) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
    for (i, it) in items.iter().enumerate() {
        let p = placed[i] + Vec2::splat(spacing * 0.5);
        let lo = it.lo;
        let rot = it.rotated;
        let rotated_width = it.size.x;
        transform_tvs(mesh, ch, &it.tvs, None, |q| {
            let d = (q - lo) * scale;
            // 90° counter-clockwise about the corner, shifted back into the box.
            let d = if rot { Vec2::new(rotated_width - d.y, d.x) } else { d };
            p + d
        });
        lo_all = lo_all.min(p);
        hi_all = hi_all.max(p + it.size);
    }
    if normalize {
        let extent = (hi_all - lo_all).max_element().max(1e-6);
        let all: BTreeSet<u32> = items.iter().flat_map(|i| i.tvs.iter().copied()).collect();
        transform_tvs(mesh, ch, &all, None, |q| (q - lo_all) / extent);
    }
}

/// Flatten Mapping grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlattenBy {
    Angle,
    SmoothingGroup,
    MaterialId,
}

/// A planar basis (u, v) for a cluster from its average normal.
fn plane_basis(normal: Vec3) -> (Vec3, Vec3) {
    let n = normal.normalize_or_zero();
    let helper = if n.z.abs() < 0.9 { Vec3::Z } else { Vec3::X };
    let u = helper.cross(n).normalize_or_zero();
    let v = n.cross(u).normalize_or_zero();
    (u, v)
}

/// Projects `faces` onto the plane of `normal` (world-size uvs), as one
/// cluster of its own.
fn planar_cluster(mesh: &mut Mesh, ch: u32, faces: &[FaceId], normal: Vec3) {
    let (u, v) = plane_basis(normal);
    let positions: Vec<Vec3> = mesh.vertices().iter().map(|x| x.position).collect();
    let origins: Vec<u32> = mesh.half_edges().iter().map(|h| h.origin.0).collect();
    assign_corner_uvs(mesh, ch, faces, |he| {
        let p = positions[origins[he.0 as usize] as usize];
        Vec3::new(p.dot(u), p.dot(v), 0.0)
    });
}

/// Flatten Mapping: faces group into near-planar clusters (by normal angle,
/// smoothing group or material id), each projected flat, then packed.
#[allow(clippy::too_many_arguments)]
pub fn flatten(mesh: &mut Mesh, ch: u32, faces: &[FaceId], angle_deg: f32, spacing: f32, normalize: bool, rotate: bool, fill_holes: bool, by: FlattenBy) {
    mesh.ensure_map(ch);
    let set: HashSet<FaceId> = faces.iter().copied().collect();
    let mut groups: Vec<Vec<FaceId>> = Vec::new();
    match by {
        FlattenBy::SmoothingGroup | FlattenBy::MaterialId => {
            let mut by_key: BTreeMap<u32, Vec<FaceId>> = BTreeMap::new();
            for f in faces {
                let face = &mesh.faces()[f.0 as usize];
                let key = if by == FlattenBy::SmoothingGroup { face.smoothing_groups } else { face.material_id as u32 };
                by_key.entry(key).or_default().push(*f);
            }
            // Within a key, keep only connected faces together.
            for (_, g) in by_key {
                groups.extend(geometry_components(mesh, &g));
            }
        }
        FlattenBy::Angle => {
            let cos_t = angle_deg.to_radians().cos();
            let mut unassigned: BTreeSet<FaceId> = set.iter().copied().collect();
            // Largest faces first as seeds.
            let mut seeds: Vec<FaceId> = faces.to_vec();
            seeds.sort_by(|a, b| face_area(mesh, *b).total_cmp(&face_area(mesh, *a)));
            for seed in seeds {
                if !unassigned.remove(&seed) {
                    continue;
                }
                let n0 = mesh.face_normal(seed);
                let mut comp = vec![seed];
                let mut queue = VecDeque::from([seed]);
                while let Some(cur) = queue.pop_front() {
                    for nb in face_neighbours(mesh, cur) {
                        if unassigned.contains(&nb) && mesh.face_normal(nb).dot(n0) >= cos_t {
                            unassigned.remove(&nb);
                            comp.push(nb);
                            queue.push_back(nb);
                        }
                    }
                }
                groups.push(comp);
            }
        }
    }
    for g in &groups {
        let normal: Vec3 = g.iter().map(|f| mesh.face_normal(*f) * face_area(mesh, *f)).sum::<Vec3>().normalize_or_zero();
        let normal = if normal.length_squared() < 0.5 { mesh.face_normal(g[0]) } else { normal };
        planar_cluster(mesh, ch, g, normal);
    }
    pack(mesh, ch, faces, PackMethod::Recursive, spacing, normalize, rotate, fill_holes);
}

fn face_area(mesh: &Mesh, f: FaceId) -> f32 {
    let pts = mesh.face_positions(f).unwrap_or_default();
    let mut n = Vec3::ZERO;
    for i in 0..pts.len() {
        n += pts[i].cross(pts[(i + 1) % pts.len()]);
    }
    n.length() * 0.5
}

fn face_neighbours(mesh: &Mesh, f: FaceId) -> Vec<FaceId> {
    mesh.face_corners(f)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|he| mesh.half_edges()[mesh.half_edges()[he.0 as usize].twin.0 as usize].face)
        .collect()
}

/// Connected components of `faces` through shared geometry edges.
pub fn geometry_components(mesh: &Mesh, faces: &[FaceId]) -> Vec<Vec<FaceId>> {
    let set: HashSet<FaceId> = faces.iter().copied().collect();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for f in faces {
        if !seen.insert(*f) {
            continue;
        }
        let mut comp = vec![*f];
        let mut queue = VecDeque::from([*f]);
        while let Some(cur) = queue.pop_front() {
            for nb in face_neighbours(mesh, cur) {
                if set.contains(&nb) && seen.insert(nb) {
                    comp.push(nb);
                    queue.push_back(nb);
                }
            }
        }
        out.push(comp);
    }
    out
}

/// Normal Mapping kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalKind {
    BackFront,
    LeftRight,
    TopBottom,
    Box,
    Diamond,
}

/// Normal Mapping: each face joins the projection direction its normal is
/// closest to; every direction's faces become planar clusters, then packed.
pub fn normal_map(mesh: &mut Mesh, ch: u32, faces: &[FaceId], kind: NormalKind, spacing: f32, normalize: bool, rotate: bool, align_by_width: bool) {
    mesh.ensure_map(ch);
    let dirs: Vec<Vec3> = match kind {
        NormalKind::BackFront => vec![Vec3::Y, Vec3::NEG_Y],
        NormalKind::LeftRight => vec![Vec3::X, Vec3::NEG_X],
        NormalKind::TopBottom => vec![Vec3::Z, Vec3::NEG_Z],
        NormalKind::Box => vec![Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z],
        NormalKind::Diamond => {
            let d = std::f32::consts::FRAC_1_SQRT_2;
            vec![Vec3::new(d, d, 0.0), Vec3::new(-d, d, 0.0), Vec3::new(d, -d, 0.0), Vec3::new(-d, -d, 0.0), Vec3::Z, Vec3::NEG_Z]
        }
    };
    let mut by_dir: Vec<Vec<FaceId>> = vec![Vec::new(); dirs.len()];
    for f in faces {
        let n = mesh.face_normal(*f);
        let best = (0..dirs.len()).max_by(|a, b| n.dot(dirs[*a]).total_cmp(&n.dot(dirs[*b]))).unwrap_or(0);
        by_dir[best].push(*f);
    }
    for (k, g) in by_dir.iter().enumerate() {
        for comp in geometry_components(mesh, g) {
            planar_cluster(mesh, ch, &comp, dirs[k]);
            if align_by_width {
                // Align By Width: a cluster taller than wide turns on its side.
                let tvs = tvs_of_faces(mesh, ch, &comp);
                let (lo, hi) = bounds(mesh, ch, Some(&tvs));
                if hi.y - lo.y > hi.x - lo.x {
                    rotate_tvs(mesh, ch, &tvs, None, 90.0, (lo + hi) * 0.5);
                }
            }
        }
    }
    pack(mesh, ch, faces, PackMethod::Recursive, spacing, normalize, rotate && !align_by_width, true);
}

/// Unfold Mapping walk order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnfoldWalk {
    Farthest,
    Closest,
}

/// Unfold Mapping: faces are laid flat one after another, each attached to
/// an already-placed neighbour along their shared edge with true 3D edge
/// lengths (no distortion for triangles, a planar fit for n-gons).
pub fn unfold(mesh: &mut Mesh, ch: u32, faces: &[FaceId], walk: UnfoldWalk, normalize: bool) {
    mesh.ensure_map(ch);
    for comp in geometry_components(mesh, faces) {
        detach_faces(mesh, ch, &comp);
        unfold_component(mesh, ch, &comp, walk);
    }
    if normalize {
        let tvs = tvs_of_faces(mesh, ch, faces);
        let (lo, hi) = bounds(mesh, ch, Some(&tvs));
        let extent = (hi - lo).max_element().max(1e-6);
        transform_tvs(mesh, ch, &tvs, None, |q| (q - lo) / extent);
    }
}

fn unfold_component(mesh: &mut Mesh, ch: u32, comp: &[FaceId], walk: UnfoldWalk) {
    let set: HashSet<FaceId> = comp.iter().copied().collect();
    // Start at the largest face.
    let start = *comp.iter().max_by(|a, b| face_area(mesh, **a).total_cmp(&face_area(mesh, **b))).unwrap();
    let mut placed: HashMap<FaceId, Vec<Vec2>> = HashMap::new(); // uv per corner (loop order)
    // The first face: its own planar projection.
    let pts0 = mesh.face_positions(start).unwrap_or_default();
    let (u, v) = plane_basis(mesh.face_normal(start));
    placed.insert(start, pts0.iter().map(|p| Vec2::new(p.dot(u), p.dot(v))).collect());
    let mut frontier: VecDeque<FaceId> = VecDeque::from([start]);
    let mut seen: HashSet<FaceId> = HashSet::from([start]);
    while let Some(cur) = match walk {
        UnfoldWalk::Closest => frontier.pop_front(),
        UnfoldWalk::Farthest => frontier.pop_back(),
    } {
        let corners = mesh.face_corners(cur).unwrap_or_default();
        let cur_uv = placed[&cur].clone();
        for (i, he) in corners.iter().enumerate() {
            let twin = mesh.half_edges()[he.0 as usize].twin;
            let Some(nb) = mesh.half_edges()[twin.0 as usize].face else { continue };
            if !set.contains(&nb) || seen.contains(&nb) {
                continue;
            }
            // Shared edge: cur corners i → i+1 is nb's twin edge (twin origin = cur's i+1).
            let a_uv = cur_uv[i];
            let b_uv = cur_uv[(i + 1) % corners.len()];
            let nb_corners = mesh.face_corners(nb).unwrap_or_default();
            let nb_pts: Vec<Vec3> = nb_corners.iter().map(|h| mesh.vertices()[mesh.half_edges()[h.0 as usize].origin.0 as usize].position).collect();
            let k = nb_corners.iter().position(|h| *h == twin).unwrap_or(0);
            // In nb, corner k is at cur's vertex i+1 (B) and k+1 at vertex i (A).
            let (pb, pa) = (nb_pts[k], nb_pts[(k + 1) % nb_pts.len()]);
            // Local planar frame of nb with A at origin and the AB edge along +x,
            // then map to the placed edge (b_uv → a_uv keeps the edge length ratio).
            let n = mesh.face_normal(nb);
            let ex = (pb - pa).normalize_or_zero();
            let ey = n.cross(ex).normalize_or_zero();
            let len3 = pa.distance(pb).max(1e-6);
            let dir = (b_uv - a_uv).normalize_or_zero();
            let scale = a_uv.distance(b_uv) / len3;
            // Place nb on the far side of the edge from cur: cur's interior is on
            // the left of a→b (CCW), so nb goes to the right.
            let perp = Vec2::new(dir.y, -dir.x);
            let uvs: Vec<Vec2> = nb_pts
                .iter()
                .map(|p| {
                    let d = *p - pa;
                    let (x, y) = (d.dot(ex), d.dot(ey));
                    // nb's interior lies on +ey? Its corners after A,B go around CCW seen along n; map +y → perp side.
                    a_uv + dir * (x * scale) + perp * (y * scale).abs() * if y >= 0.0 { 1.0 } else { -1.0 }
                })
                .collect();
            // Ensure nb's interior is on the far side: if its centroid is on cur's side, flip across the edge.
            let c = uvs.iter().sum::<Vec2>() / uvs.len() as f32;
            let cur_c = cur_uv.iter().sum::<Vec2>() / cur_uv.len() as f32;
            let side = |p: Vec2| (p - a_uv).dot(perp);
            let uvs = if side(c).signum() == side(cur_c).signum() { uvs.into_iter().map(|p| p - perp * 2.0 * side(p)).collect() } else { uvs };
            placed.insert(nb, uvs);
            seen.insert(nb);
            frontier.push_back(nb);
        }
    }
    // Write the corner uvs; a tv shared by several placed corners keeps the
    // first value (the shared edge of neighbours agrees by construction).
    let writes: Vec<(HalfEdgeId, Vec2)> = placed
        .iter()
        .flat_map(|(f, uvs)| mesh.face_corners(*f).unwrap_or_default().into_iter().zip(uvs.iter().copied()).collect::<Vec<_>>())
        .collect();
    let m = mesh.map_mut(ch).expect("channel");
    let mut done: HashSet<u32> = HashSet::new();
    for (he, uv) in writes {
        let tv = m.corners[he.0 as usize];
        if done.insert(tv) {
            m.verts[tv as usize] = Vec3::new(uv.x, uv.y, 0.0);
        }
    }
}

/// Pelt Mapping: the faces cut at `seams` (geometry vertex pairs) spread
/// their boundary on a circle and the inside relaxes harmonically.
pub fn pelt(mesh: &mut Mesh, ch: u32, faces: &[FaceId], seams: &BTreeSet<(u32, u32)>, iterations: u32, pins: &BTreeSet<u32>) {
    mesh.ensure_map(ch);
    // Cut: every seam edge and the faces' outer boundary.
    let set: HashSet<FaceId> = faces.iter().copied().collect();
    let mut cuts: HashSet<HalfEdgeId> = HashSet::new();
    let mut tvs = BTreeSet::new();
    for f in faces {
        let Some(corners) = mesh.face_corners(*f) else { continue };
        for he in corners {
            let h = &mesh.half_edges()[he.0 as usize];
            let next = &mesh.half_edges()[h.next.0 as usize];
            let (a, b) = (h.origin.0, next.origin.0);
            let twin_face = mesh.half_edges()[h.twin.0 as usize].face;
            let is_seam = seams.contains(&(a.min(b), a.max(b)));
            if is_seam || twin_face.is_none_or(|o| !set.contains(&o)) {
                cuts.insert(mesh.canonical_edge_id(he).unwrap_or(he));
                tvs.insert(corner_tv(mesh, ch, he).unwrap_or(0));
                tvs.insert(corner_tv(mesh, ch, h.next).unwrap_or(0));
            }
        }
    }
    let all_tvs = tvs_of_faces(mesh, ch, faces);
    split_at(mesh, ch, &all_tvs, &cuts);
    for comp in clusters(mesh, ch, faces) {
        pelt_component(mesh, ch, &comp, iterations, pins);
    }
}

fn pelt_component(mesh: &mut Mesh, ch: u32, comp: &[FaceId], iterations: u32, pins: &BTreeSet<u32>) {
    let tvf = tv_faces(mesh, ch);
    // Boundary tv edges of the component (used by one face) → loops.
    let mut count: HashMap<(u32, u32), u32> = HashMap::new();
    let mut directed: Vec<(u32, u32)> = Vec::new();
    for f in comp {
        let t = &tvf[f.0 as usize];
        for i in 0..t.len() {
            let (a, b) = (t[i], t[(i + 1) % t.len()]);
            *count.entry((a.min(b), a.max(b))).or_default() += 1;
            directed.push((a, b));
        }
    }
    let boundary: Vec<(u32, u32)> = directed.into_iter().filter(|(a, b)| count.get(&(*a.min(b), *a.max(b))) == Some(&1)).collect();
    if boundary.is_empty() {
        return;
    }
    let mut next: HashMap<u32, u32> = boundary.iter().map(|(a, b)| (*a, *b)).collect();
    // Longest loop by 3D length (approximated by count) becomes the circle.
    let mut loops: Vec<Vec<u32>> = Vec::new();
    while let Some((&start, _)) = next.iter().next() {
        let mut lp = vec![start];
        let mut cur = start;
        while let Some(n) = next.remove(&cur) {
            if n == start {
                break;
            }
            lp.push(n);
            cur = n;
        }
        loops.push(lp);
    }
    loops.sort_by_key(|l| std::cmp::Reverse(l.len()));
    let outer = &loops[0];
    let tvs: BTreeSet<u32> = comp.iter().flat_map(|f| tvf[f.0 as usize].iter().copied()).collect();
    let (lo, hi) = bounds(mesh, ch, Some(&tvs));
    let r = (hi - lo).max_element().max(1e-6) * 0.5;
    let c = (lo + hi) * 0.5;
    let m = mesh.map_mut(ch).expect("channel");
    let n = outer.len();
    for (i, t) in outer.iter().enumerate() {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        m.verts[*t as usize] = Vec3::new(c.x + r * a.cos(), c.y + r * a.sin(), m.verts[*t as usize].z);
    }
    // Interior: harmonic relax with the circle and the pinned tvs fixed.
    let interior: BTreeSet<u32> = tvs.iter().copied().filter(|t| !outer.contains(t) && !pins.contains(t)).collect();
    if !interior.is_empty() {
        relax(mesh, ch, Some(&interior), iterations.max(1), 1.0, 0.0, RelaxBy::FaceAngles, false);
    }
}

/// Quick Planar Map: `faces` project along `normal` (their average normal
/// when None) and fit the unit square.
pub fn quick_planar(mesh: &mut Mesh, ch: u32, faces: &[FaceId], normal: Option<Vec3>) {
    mesh.ensure_map(ch);
    let n = normal.unwrap_or_else(|| faces.iter().map(|f| mesh.face_normal(*f) * face_area(mesh, *f)).sum::<Vec3>().normalize_or_zero());
    let n = if n.length_squared() < 0.5 { Vec3::Z } else { n };
    planar_cluster(mesh, ch, faces, n);
    let tvs = tvs_of_faces(mesh, ch, faces);
    let (lo, hi) = bounds(mesh, ch, Some(&tvs));
    let extent = (hi - lo).max_element().max(1e-6);
    transform_tvs(mesh, ch, &tvs, None, |q| (q - lo) / extent);
}

/// Planar / Cylindrical / Spherical / Box map of `faces` with a gizmo.
pub fn map_with_gizmo(mesh: &mut Mesh, ch: u32, faces: &[FaceId], projection: Projection, gizmo: Mat4, size: Vec3) {
    mesh.ensure_map(ch);
    let params = MapParams {
        projection,
        gizmo: gizmo.inverse(),
        length: size.y.max(1e-6),
        width: size.x.max(1e-6),
        height: size.z.max(1e-6),
        u_tile: 1.0,
        v_tile: 1.0,
        w_tile: 1.0,
        u_flip: false,
        v_flip: false,
        w_flip: false,
    };
    let mapped = project_faces(mesh, &params, ch, faces);
    let src = mapped.map(ch).expect("channel").clone();
    assign_corner_uvs(mesh, ch, faces, |he| src.verts[src.corners[he.0 as usize] as usize]);
}

/// Shortest edge path between two vertices (Point To Point seams), as
/// vertex pairs.
pub fn seam_path(mesh: &Mesh, from: VertexId, to: VertexId) -> Vec<(u32, u32)> {
    let n = mesh.vertex_count();
    if from.0 as usize >= n || to.0 as usize >= n {
        return Vec::new();
    }
    let mut dist = vec![f32::INFINITY; n];
    let mut prev: Vec<Option<u32>> = vec![None; n];
    let mut heap = std::collections::BinaryHeap::new();
    dist[from.0 as usize] = 0.0;
    heap.push(std::cmp::Reverse((ordered(0.0), from.0)));
    while let Some(std::cmp::Reverse((d, v))) = heap.pop() {
        let d = d.0;
        if d > dist[v as usize] {
            continue;
        }
        if v == to.0 {
            break;
        }
        let p = mesh.vertices()[v as usize].position;
        for he in mesh.outgoing_half_edges(VertexId(v)) {
            let h = &mesh.half_edges()[he.0 as usize];
            let o = mesh.half_edges()[h.twin.0 as usize].origin;
            let nd = d + p.distance(mesh.vertices()[o.0 as usize].position);
            if nd < dist[o.0 as usize] {
                dist[o.0 as usize] = nd;
                prev[o.0 as usize] = Some(v);
                heap.push(std::cmp::Reverse((ordered(nd), o.0)));
            }
        }
    }
    let mut path = Vec::new();
    let mut cur = to.0;
    while let Some(p) = prev[cur as usize] {
        path.push((p.min(cur), p.max(cur)));
        cur = p;
    }
    path.reverse();
    path
}

#[derive(PartialEq)]
struct Ordered(f32);
impl Eq for Ordered {}
impl PartialOrd for Ordered {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Ordered {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&o.0)
    }
}
fn ordered(x: f32) -> Ordered {
    Ordered(x)
}

/// Signed UV area of a face (negative = inverted / flipped).
pub fn uv_area(mesh: &Mesh, ch: u32, f: FaceId) -> f32 {
    let Some(m) = mesh.map(ch) else { return 0.0 };
    let t = face_tvs(mesh, ch, f);
    let mut a = 0.0;
    for i in 0..t.len() {
        let (p, q) = (uv2(m.verts[t[i] as usize]), uv2(m.verts[t[(i + 1) % t.len()] as usize]));
        a += p.x * q.y - q.x * p.y;
    }
    a * 0.5
}

/// Faces whose UV winding is inverted.
pub fn inverted_faces(mesh: &Mesh, ch: u32) -> Vec<FaceId> {
    (0..mesh.face_count() as u32).map(FaceId).filter(|f| uv_area(mesh, ch, *f) < -1e-12).collect()
}

/// Faces whose UV polygons overlap another face's (triangle fans, bbox pre-check).
pub fn overlapped_faces(mesh: &Mesh, ch: u32) -> Vec<FaceId> {
    let Some(m) = mesh.map(ch) else { return Vec::new() };
    let polys: Vec<Vec<Vec2>> = (0..mesh.face_count()).map(|f| face_tvs(mesh, ch, FaceId(f as u32)).iter().map(|t| uv2(m.verts[*t as usize])).collect()).collect();
    let boxes: Vec<(Vec2, Vec2)> = polys.iter().map(|p| p.iter().fold((Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)), |(lo, hi), q| (lo.min(*q), hi.max(*q)))).collect();
    let mut out = BTreeSet::new();
    let tri = |p: &[Vec2]| -> Vec<[Vec2; 3]> { (1..p.len().saturating_sub(1)).map(|i| [p[0], p[i], p[i + 1]]).collect() };
    for i in 0..polys.len() {
        for j in i + 1..polys.len() {
            let (a, b) = (&boxes[i], &boxes[j]);
            if a.1.x <= b.0.x || b.1.x <= a.0.x || a.1.y <= b.0.y || b.1.y <= a.0.y {
                continue;
            }
            // Faces sharing a tv edge are neighbours, not overlaps.
            let ti: HashSet<u32> = face_tvs(mesh, ch, FaceId(i as u32)).into_iter().collect();
            let shared = face_tvs(mesh, ch, FaceId(j as u32)).iter().filter(|t| ti.contains(t)).count();
            if shared >= 2 {
                continue;
            }
            let hit = tri(&polys[i]).iter().any(|t1| tri(&polys[j]).iter().any(|t2| tris_overlap(t1, t2)));
            if hit {
                out.insert(FaceId(i as u32));
                out.insert(FaceId(j as u32));
            }
        }
    }
    out.into_iter().collect()
}

fn tris_overlap(a: &[Vec2; 3], b: &[Vec2; 3]) -> bool {
    // Separating axis test on the 6 edge normals.
    let mut axes: Vec<Vec2> = Vec::with_capacity(6);
    for t in [a, b] {
        for i in 0..3 {
            let e = t[(i + 1) % 3] - t[i];
            axes.push(Vec2::new(-e.y, e.x));
        }
    }
    for ax in axes {
        if ax.length_squared() < 1e-18 {
            continue;
        }
        let pr = |t: &[Vec2; 3]| t.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), p| (lo.min(p.dot(ax)), hi.max(p.dot(ax))));
        let (a0, a1) = pr(a);
        let (b0, b1) = pr(b);
        if a1 <= b0 + 1e-9 || b1 <= a0 + 1e-9 {
            return false;
        }
    }
    true
}

/// Expand (grow) a tv selection by one ring of tv edges.
pub fn expand(mesh: &Mesh, ch: u32, tvs: &BTreeSet<u32>) -> BTreeSet<u32> {
    let mut out = tvs.clone();
    for (a, b) in tv_edges(mesh, ch) {
        if tvs.contains(&a) || tvs.contains(&b) {
            out.insert(a);
            out.insert(b);
        }
    }
    out
}

/// Contract (shrink): tvs with an unselected neighbour drop out.
pub fn contract(mesh: &Mesh, ch: u32, tvs: &BTreeSet<u32>) -> BTreeSet<u32> {
    let mut drop = BTreeSet::new();
    for (a, b) in tv_edges(mesh, ch) {
        if tvs.contains(&a) != tvs.contains(&b) {
            drop.insert(a);
            drop.insert(b);
        }
    }
    tvs.difference(&drop).copied().collect()
}

/// Select Element: whole UV clusters of the faces touching `tvs`.
pub fn element(mesh: &Mesh, ch: u32, tvs: &BTreeSet<u32>) -> BTreeSet<u32> {
    let all: Vec<FaceId> = (0..mesh.face_count() as u32).map(FaceId).collect();
    let mut out = BTreeSet::new();
    for comp in clusters(mesh, ch, &all) {
        let ctvs = tvs_of_faces(mesh, ch, &comp);
        if ctvs.iter().any(|t| tvs.contains(t)) {
            out.extend(ctvs);
        }
    }
    out
}

/// Soft-selection weights: tvs within `falloff` (UV distance) of the
/// selection get 1 → 0 by a smooth curve.
pub fn soft_weights(mesh: &Mesh, ch: u32, tvs: &BTreeSet<u32>, falloff: f32) -> HashMap<u32, f32> {
    let Some(m) = mesh.map(ch) else { return HashMap::new() };
    let sel: Vec<Vec2> = tvs.iter().map(|t| uv2(m.verts[*t as usize])).collect();
    let mut out = HashMap::new();
    if falloff <= 0.0 || sel.is_empty() {
        return out;
    }
    for (i, v) in m.verts.iter().enumerate() {
        if tvs.contains(&(i as u32)) {
            continue;
        }
        let d = sel.iter().map(|s| s.distance(uv2(*v))).fold(f32::INFINITY, f32::min);
        if d < falloff {
            let t = 1.0 - d / falloff;
            out.insert(i as u32, t * t * (3.0 - 2.0 * t));
        }
    }
    out
}

/// Soft selection by edge distance: tvs within `range` tv edges of the
/// selection get 1 → 0 by hop count (Soft Selection > Edge Distance).
pub fn soft_weights_edges(mesh: &Mesh, ch: u32, tvs: &BTreeSet<u32>, range: u32) -> HashMap<u32, f32> {
    let mut out = HashMap::new();
    if range == 0 || tvs.is_empty() {
        return out;
    }
    let mut adj: HashMap<u32, Vec<u32>> = HashMap::new();
    for (a, b) in tv_edges(mesh, ch) {
        adj.entry(a).or_default().push(b);
        adj.entry(b).or_default().push(a);
    }
    let mut dist: HashMap<u32, u32> = tvs.iter().map(|t| (*t, 0)).collect();
    let mut frontier: Vec<u32> = tvs.iter().copied().collect();
    for d in 1..=range {
        let mut next = Vec::new();
        for t in &frontier {
            for n in adj.get(t).into_iter().flatten() {
                if !dist.contains_key(n) {
                    dist.insert(*n, d);
                    next.push(*n);
                }
            }
        }
        frontier = next;
    }
    for (t, d) in dist {
        if d > 0 {
            let x = 1.0 - d as f32 / (range as f32 + 1.0);
            out.insert(t, x * x * (3.0 - 2.0 * x));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::standard_primitives::{box_segmented, plane_segmented, sphere_segmented};
    use crate::uvmap::{auto_map, AutoMap};

    fn boxed() -> Mesh {
        let mut m = box_segmented(2.0, 2.0, 2.0, [1, 1, 1]);
        auto_map(&mut m, AutoMap::Box, 1);
        m
    }

    #[test]
    fn break_then_weld_round_trip() {
        let mut m = plane_segmented(2.0, 2.0, 2, 2);
        auto_map(&mut m, AutoMap::Planar, 1);
        let before = m.map(1).unwrap().verts.len();
        let all: BTreeSet<u32> = (0..before as u32).collect();
        break_tvs(&mut m, 1, &all);
        m.check_invariants().unwrap();
        assert_eq!(m.map(1).unwrap().verts.len(), 16, "every corner its own tv");
        let all: BTreeSet<u32> = (0..16).collect();
        let n = weld_tvs(&mut m, 1, &all, 1e-4);
        assert!(n > 0);
        m.check_invariants().unwrap();
        assert_eq!(m.map(1).unwrap().verts.len(), before, "welded back to the same tvs");
    }

    #[test]
    fn stitch_closes_a_seam() {
        let mut m = plane_segmented(2.0, 2.0, 2, 1);
        auto_map(&mut m, AutoMap::Planar, 1);
        // Split the middle edge and move the right face away.
        let right = FaceId(1);
        detach_faces(&mut m, 1, &[right]);
        let rt = tvs_of_faces(&m, 1, &[right]);
        move_tvs(&mut m, 1, &rt, None, Vec2::new(0.3, 0.0));
        assert_eq!(tv_edges(&m, 1).len(), 8, "detached: the seam edge is doubled");
        let lt = tvs_of_faces(&m, 1, &[FaceId(0)]);
        let n = stitch(&mut m, 1, &lt, 0.0, false);
        assert_eq!(n, 2);
        assert_eq!(tv_edges(&m, 1).len(), 7, "the seam's two edges became one");
        m.check_invariants().unwrap();
    }

    #[test]
    fn relax_reduces_edge_length_variance() {
        let mut m = sphere_segmented(5.0, 12, 0.0);
        auto_map(&mut m, AutoMap::Spherical, 1);
        let var = |m: &Mesh| {
            let ch = m.map(1).unwrap();
            let l: Vec<f32> = tv_edges(m, 1).iter().map(|(a, b)| uv2(ch.verts[*a as usize]).distance(uv2(ch.verts[*b as usize]))).collect();
            let mean = l.iter().sum::<f32>() / l.len() as f32;
            l.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / l.len() as f32
        };
        let before = var(&m);
        relax(&mut m, 1, None, 20, 0.5, 0.0, RelaxBy::Centers, true);
        assert!(var(&m) < before, "{} < {before}", var(&m));
    }

    #[test]
    fn pack_keeps_clusters_inside_and_apart() {
        let mut m = boxed();
        let faces: Vec<FaceId> = (0..6).map(FaceId).collect();
        let all: BTreeSet<u32> = (0..m.map(1).unwrap().verts.len() as u32).collect();
        break_tvs(&mut m, 1, &all);
        pack(&mut m, 1, &faces, PackMethod::Recursive, 0.02, true, true, true);
        let (lo, hi) = bounds(&m, 1, None);
        assert!(lo.x >= -1e-4 && lo.y >= -1e-4 && hi.x <= 1.0 + 1e-4 && hi.y <= 1.0 + 1e-4, "{lo:?} {hi:?}");
        assert!(overlapped_faces(&m, 1).is_empty());
        assert_eq!(clusters(&m, 1, &faces).len(), 6);
    }

    #[test]
    fn flatten_box_gives_six_clusters_and_normal_map_too() {
        let mut m = boxed();
        let faces: Vec<FaceId> = (0..6).map(FaceId).collect();
        flatten(&mut m, 1, &faces, 45.0, 0.02, true, false, true, FlattenBy::Angle);
        assert_eq!(clusters(&m, 1, &faces).len(), 6);
        assert!(overlapped_faces(&m, 1).is_empty());
        m.check_invariants().unwrap();
        // Square faces stay square: packing and normalizing scale uniformly.
        for f in &faces {
            let tvs = tvs_of_faces(&m, 1, &[*f]);
            let (lo, hi) = bounds(&m, 1, Some(&tvs));
            let s = hi - lo;
            assert!((s.x - s.y).abs() < 1e-3, "face {f:?} is {s:?}");
            assert!(s.x > 0.2, "{s:?}");
        }
        let mut m = boxed();
        normal_map(&mut m, 1, &faces, NormalKind::Box, 0.02, true, false, false);
        assert_eq!(clusters(&m, 1, &faces).len(), 6);
        let mut m = boxed();
        normal_map(&mut m, 1, &faces, NormalKind::TopBottom, 0.02, true, false, false);
        let cl = clusters(&m, 1, &faces);
        assert_eq!(cl.len(), 2, "{:?}", cl.iter().map(|c| c.len()).collect::<Vec<_>>());
    }

    #[test]
    fn unfold_strip_is_a_strip() {
        let mut m = plane_segmented(1.0, 4.0, 4, 1);
        m.ensure_map(1);
        let faces: Vec<FaceId> = (0..4).map(FaceId).collect();
        unfold(&mut m, 1, &faces, UnfoldWalk::Closest, true);
        m.check_invariants().unwrap();
        assert_eq!(clusters(&m, 1, &faces).len(), 1, "unfolded faces stay joined");
        let (lo, hi) = bounds(&m, 1, None);
        let size = hi - lo;
        assert!((size.max_element() - 1.0).abs() < 1e-4 && (size.min_element() - 0.25).abs() < 0.02, "{size:?}");
        assert!(overlapped_faces(&m, 1).is_empty());
    }

    #[test]
    fn pelt_of_a_hemisphere_is_a_disc() {
        let mut m = sphere_segmented(5.0, 12, 0.5);
        m.ensure_map(1);
        let faces: Vec<FaceId> = (0..m.face_count() as u32).map(FaceId).collect();
        pelt(&mut m, 1, &faces, &BTreeSet::new(), 50, &BTreeSet::new());
        m.check_invariants().unwrap();
        assert!(inverted_faces(&m, 1).len() < faces.len() / 4, "{}", inverted_faces(&m, 1).len());
        assert!(overlapped_faces(&m, 1).len() < faces.len() / 4);
    }

    #[test]
    fn rotate_clusters_rotates_instead_of_mirroring() {
        let mut m = box_segmented(1.0, 1.0, 4.0, [1, 1, 1]);
        auto_map(&mut m, AutoMap::Box, 1);
        let faces: Vec<FaceId> = (0..6).map(FaceId).collect();
        flatten(&mut m, 1, &faces, 45.0, 0.02, true, true, true, FlattenBy::Angle);
        assert!(inverted_faces(&m, 1).is_empty(), "rotated clusters must keep their winding");
        assert!(overlapped_faces(&m, 1).is_empty());
    }

    #[test]
    fn weld_selected_joins_tvs_of_different_vertices_within_threshold() {
        let mut m = plane_segmented(2.0, 2.0, 2, 2);
        auto_map(&mut m, AutoMap::Planar, 1);
        let n = m.map(1).unwrap().verts.len() as u32;
        let all: BTreeSet<u32> = (0..n).collect();
        let welded = weld_tvs(&mut m, 1, &all, 10.0);
        assert!(welded > 0);
        assert_eq!(m.map(1).unwrap().verts.len(), 1, "everything within 10 welds to one tv");
        m.check_invariants().unwrap();
        // Target Weld joins any two tvs.
        let mut m = plane_segmented(2.0, 2.0, 2, 2);
        auto_map(&mut m, AutoMap::Planar, 1);
        target_weld(&mut m, 1, 0, 1).unwrap();
        assert_eq!(m.map(1).unwrap().verts.len(), 8);
    }

    #[test]
    fn normal_map_align_by_width_lays_clusters_wide() {
        let mut m = box_segmented(1.0, 1.0, 4.0, [1, 1, 1]);
        auto_map(&mut m, AutoMap::Box, 1);
        let faces: Vec<FaceId> = (0..6).map(FaceId).collect();
        normal_map(&mut m, 1, &faces, NormalKind::Box, 0.02, true, false, true);
        for g in clusters(&m, 1, &faces) {
            let tvs = tvs_of_faces(&m, 1, &g);
            let (lo, hi) = bounds(&m, 1, Some(&tvs));
            assert!(hi.x - lo.x >= hi.y - lo.y - 1e-4, "cluster {g:?} is taller than wide: {:?}", hi - lo);
        }
        assert!(inverted_faces(&m, 1).is_empty());
    }

    #[test]
    fn soft_weights_by_edge_distance() {
        let mut m = plane_segmented(4.0, 4.0, 4, 4);
        auto_map(&mut m, AutoMap::Planar, 1);
        let centre = (0..m.map(1).unwrap().verts.len() as u32).find(|t| uv2(m.map(1).unwrap().verts[*t as usize]).distance(Vec2::splat(0.5)) < 1e-4).unwrap();
        let sel: BTreeSet<u32> = [centre].into_iter().collect();
        let w1 = soft_weights_edges(&m, 1, &sel, 1);
        let w2 = soft_weights_edges(&m, 1, &sel, 2);
        assert_eq!(w1.len(), 4, "one ring: the four neighbours");
        assert!(w2.len() > w1.len());
        assert!(w1.values().all(|w| *w > 0.0 && *w < 1.0));
    }

    #[test]
    fn pelt_keeps_pinned_tvs() {
        let mut m = sphere_segmented(5.0, 12, 0.5);
        m.ensure_map(1);
        let faces: Vec<FaceId> = (0..m.face_count() as u32).map(FaceId).collect();
        pelt(&mut m, 1, &faces, &BTreeSet::new(), 20, &BTreeSet::new());
        // Pin the tv nearest the disc centre and pelt again: it stays put.
        let ch = m.map(1).unwrap().clone();
        let c = {
            let tvs: BTreeSet<u32> = (0..ch.verts.len() as u32).collect();
            centre(&m, 1, &tvs)
        };
        let pin = (0..ch.verts.len() as u32).min_by(|a, b| uv2(ch.verts[*a as usize]).distance(c).total_cmp(&uv2(ch.verts[*b as usize]).distance(c))).unwrap();
        let before = ch.verts[pin as usize];
        let pins: BTreeSet<u32> = [pin].into_iter().collect();
        pelt(&mut m, 1, &faces, &BTreeSet::new(), 20, &pins);
        let after = m.map(1).unwrap().verts[pin as usize];
        assert!((after - before).length() < 1e-6, "{before:?} → {after:?}");
    }

    #[test]
    fn seam_path_on_a_grid_is_the_short_route() {
        let m = plane_segmented(4.0, 4.0, 4, 4);
        let a = (0..m.vertex_count()).map(|v| VertexId(v as u32)).find(|v| m.vertex_position(*v).unwrap() == Vec3::new(-2.0, -2.0, 0.0)).unwrap();
        let b = (0..m.vertex_count()).map(|v| VertexId(v as u32)).find(|v| m.vertex_position(*v).unwrap() == Vec3::new(2.0, -2.0, 0.0)).unwrap();
        let path = seam_path(&m, a, b);
        assert_eq!(path.len(), 4, "{path:?}");
    }

    #[test]
    fn quick_planar_fits_the_square_and_overlaps_are_found() {
        let mut m = boxed();
        let faces: Vec<FaceId> = (0..6).map(FaceId).collect();
        quick_planar(&mut m, 1, &faces, Some(Vec3::Z));
        let (lo, hi) = bounds(&m, 1, None);
        assert!(lo.x.abs() < 1e-4 && (hi.x - 1.0).abs() < 1e-4, "{lo:?} {hi:?}");
        assert!(!overlapped_faces(&m, 1).is_empty(), "top and bottom overlap");
        assert!(!inverted_faces(&m, 1).is_empty(), "the bottom face is inverted");
    }
}

/// Edge Snap: the nearest point on a texture edge within `threshold` of
/// `p`, skipping edges that touch `exclude` (the tvs being dragged).
pub fn snap_to_edge(mesh: &Mesh, ch: u32, exclude: &BTreeSet<u32>, p: Vec2, threshold: f32) -> Option<Vec2> {
    let map = mesh.map(ch)?;
    let mut best: Option<(f32, Vec2)> = None;
    for (a, b) in tv_edges(mesh, ch) {
        if exclude.contains(&a) || exclude.contains(&b) {
            continue;
        }
        let (pa, pb) = (map.verts[a as usize], map.verts[b as usize]);
        let (pa, pb) = (Vec2::new(pa.x, pa.y), Vec2::new(pb.x, pb.y));
        let ab = pb - pa;
        let t = if ab.length_squared() < 1e-12 { 0.0 } else { ((p - pa).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) };
        let q = pa + ab * t;
        let d = q.distance(p);
        if d <= threshold && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, q));
        }
    }
    best.map(|(_, q)| q)
}

/// Sketch: `n` evenly spaced positions along the drawn shape. `align`:
/// 0 Free Form (the drawn points as a polyline), 1 Line (first to last
/// point, ends included), 2 Box (the rectangle with corners points[0] and
/// points[1], closed), 3 Circle (centre points[0] through points[1], closed).
pub fn sketch_points(align: u32, points: &[Vec2], n: usize) -> Vec<Vec2> {
    if n == 0 || points.is_empty() {
        return Vec::new();
    }
    let (path, closed): (Vec<Vec2>, bool) = match align {
        1 => (vec![points[0], *points.last().unwrap()], false),
        2 => {
            let (a, b) = (points[0], *points.last().unwrap());
            (vec![a, Vec2::new(b.x, a.y), b, Vec2::new(a.x, b.y)], true)
        }
        3 => {
            let c = points[0];
            let r = points.last().unwrap().distance(c);
            let a0 = (*points.last().unwrap() - c).to_angle();
            return (0..n).map(|i| c + Vec2::from_angle(a0 + std::f32::consts::TAU * i as f32 / n as f32) * r).collect();
        }
        _ => (points.to_vec(), false),
    };
    let mut pts = path;
    if closed {
        pts.push(pts[0]);
    }
    let mut acc = vec![0.0f32];
    for w in pts.windows(2) {
        acc.push(acc.last().unwrap() + w[0].distance(w[1]));
    }
    let total = *acc.last().unwrap();
    if total < 1e-9 {
        return vec![pts[0]; n];
    }
    // Open shapes include both ends; closed ones do not repeat the start.
    let steps = if closed { n } else { n.saturating_sub(1).max(1) };
    (0..n)
        .map(|i| {
            let s = total * i as f32 / steps as f32;
            let k = acc.partition_point(|a| *a <= s).clamp(1, pts.len() - 1);
            let seg = (acc[k] - acc[k - 1]).max(1e-9);
            pts[k - 1].lerp(pts[k], ((s - acc[k - 1]) / seg).clamp(0.0, 1.0))
        })
        .collect()
}
