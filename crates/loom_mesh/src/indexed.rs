//! An index-based polygon soup for topology edits: positions plus faces as
//! vertex-index loops with attributes. Converting from and to `Mesh` keeps
//! every attribute; vertices with the same position stay distinct unless an
//! edit merges them.

use crate::half_edge::{Face, HalfEdge, HalfEdgeId, MapChannel, Vertex, VertexId};
use crate::{FaceId, Mesh};
use glam::Vec3;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct IndexedFace {
    pub ids: Vec<u32>,
    pub smoothing_groups: u32,
    pub material_id: u16,
    pub hidden: bool,
    /// Map channel → one texture coordinate per corner (same length as
    /// `ids`); a channel missing here gets texture vertex 0 on `build`.
    pub uv: BTreeMap<u32, Vec<Vec3>>,
}

impl Default for IndexedFace {
    fn default() -> Self {
        Self { ids: Vec::new(), smoothing_groups: 0, material_id: 1, hidden: false, uv: BTreeMap::new() }
    }
}

impl IndexedFace {
    /// Copies the source face's attributes. Each corner takes the uv of the
    /// source corner with the same vertex id; a corner whose vertex is new
    /// (a split edge, a face centre) interpolates between the nearest
    /// matched corners on either side of it in the new loop, so an inserted
    /// vertex lands on the edge's uv and a centre at the face's average.
    pub fn like(src: &IndexedFace, ids: Vec<u32>) -> Self {
        let uv = remap_uv(&src.uv, &src.ids, &ids, None);
        Self { ids, smoothing_groups: src.smoothing_groups, material_id: src.material_id, hidden: src.hidden, uv }
    }

    /// Like [`Self::like`], but a new vertex that lies on one of the source
    /// loop's edges takes the uv interpolated along that edge.
    pub fn like_at(src: &IndexedFace, ids: Vec<u32>, positions: &[Vec3]) -> Self {
        let uv = remap_uv(&src.uv, &src.ids, &ids, Some(positions));
        Self { ids, smoothing_groups: src.smoothing_groups, material_id: src.material_id, hidden: src.hidden, uv }
    }

    /// Copies attributes and uvs corner for corner (the new loop is the
    /// old one renumbered, in the same order).
    pub fn same_order(src: &IndexedFace, ids: Vec<u32>) -> Self {
        let uv = if ids.len() == src.ids.len() { src.uv.clone() } else { remap_uv(&src.uv, &src.ids, &ids, None) };
        Self { ids, smoothing_groups: src.smoothing_groups, material_id: src.material_id, hidden: src.hidden, uv }
    }

    /// Copies attributes with the loop (and its uvs) reversed, renumbered.
    pub fn reversed(src: &IndexedFace, ids: Vec<u32>) -> Self {
        let mut uv = BTreeMap::new();
        if ids.len() == src.ids.len() {
            for (ch, vals) in &src.uv {
                uv.insert(*ch, vals.iter().rev().copied().collect());
            }
        } else {
            uv = remap_uv(&src.uv, &src.ids, &ids, None);
        }
        Self { ids, smoothing_groups: src.smoothing_groups, material_id: src.material_id, hidden: src.hidden, uv }
    }

    /// Replaces the loop, carrying the uvs by vertex id as `like` does.
    pub fn set_ids(&mut self, ids: Vec<u32>) {
        self.uv = remap_uv(&self.uv, &self.ids, &ids, None);
        self.ids = ids;
    }
}

/// Per-channel uv rows for `new_ids`, matched to `old_ids` by vertex id;
/// unmatched corners interpolate between their nearest matched neighbours
/// along the new loop (both sides), else take the old row's average.
fn remap_uv(uv: &BTreeMap<u32, Vec<Vec3>>, old_ids: &[u32], new_ids: &[u32], pos: Option<&[Vec3]>) -> BTreeMap<u32, Vec<Vec3>> {
    let mut out = BTreeMap::new();
    let n = new_ids.len();
    // A new vertex lying on an old edge takes the uv interpolated along it.
    let on_edge = |vals: &[Vec3], i: usize| -> Option<Vec3> {
        let pos = pos?;
        let p = *pos.get(new_ids[i] as usize)?;
        let m = old_ids.len();
        (0..m).find_map(|k| {
            let (a, b) = (*pos.get(old_ids[k] as usize)?, *pos.get(old_ids[(k + 1) % m] as usize)?);
            let ab = b - a;
            let len2 = ab.length_squared();
            if len2 < 1e-12 {
                return None;
            }
            let t = (p - a).dot(ab) / len2;
            let d = (a + ab * t - p).length_squared();
            (t > -1e-4 && t < 1.0 + 1e-4 && d < 1e-6 * len2).then(|| vals[k].lerp(vals[(k + 1) % m], t.clamp(0.0, 1.0)))
        })
    };
    // A new vertex inside the old polygon takes mean value coordinates over
    // its loop (exact for an affine map, as every projection on a planar face).
    let inside = |vals: &[Vec3], i: usize| -> Option<Vec3> {
        let pos = pos?;
        let p = *pos.get(new_ids[i] as usize)?;
        let m = old_ids.len();
        let vs: Vec<Vec3> = old_ids.iter().map(|id| pos.get(*id as usize).copied()).collect::<Option<_>>()?;
        let d: Vec<f32> = vs.iter().map(|v| (*v - p).length()).collect();
        if d.iter().any(|x| *x < 1e-9) {
            return None;
        }
        let half_tan = |a: usize, b: usize| {
            let (ua, ub) = ((vs[a] - p) / d[a], (vs[b] - p) / d[b]);
            let ang = ua.dot(ub).clamp(-1.0, 1.0).acos();
            (ang * 0.5).tan()
        };
        let w: Vec<f32> = (0..m).map(|k| (half_tan((k + m - 1) % m, k) + half_tan(k, (k + 1) % m)) / d[k]).collect();
        let sum: f32 = w.iter().sum();
        (sum.is_finite() && sum > 0.0).then(|| (0..m).map(|k| vals[k] * (w[k] / sum)).sum::<Vec3>())
    };
    for (ch, vals) in uv {
        if vals.len() != old_ids.len() {
            continue;
        }
        let matched: Vec<Option<Vec3>> =
            new_ids.iter().enumerate().map(|(i, v)| old_ids.iter().position(|o| o == v).map(|k| vals[k]).or_else(|| on_edge(vals, i)).or_else(|| inside(vals, i))).collect();
        let avg = if vals.is_empty() { Vec3::ZERO } else { vals.iter().sum::<Vec3>() / vals.len() as f32 };
        let row: Vec<Vec3> = (0..n)
            .map(|i| {
                if let Some(v) = matched[i] {
                    return v;
                }
                // Nearest matched corner before and after along the loop.
                let before = (1..n).map(|d| (i + n - d) % n).find(|j| matched[*j].is_some()).and_then(|j| matched[j]);
                let after = (1..n).map(|d| (i + d) % n).find(|j| matched[*j].is_some()).and_then(|j| matched[j]);
                match (before, after) {
                    (Some(a), Some(b)) => (a + b) * 0.5,
                    (Some(a), None) | (None, Some(a)) => a,
                    (None, None) => avg,
                }
            })
            .collect();
        out.insert(*ch, row);
    }
    out
}

#[derive(Debug, Clone, Default)]
pub struct IndexedMesh {
    pub positions: Vec<Vec3>,
    /// (colour, illumination, alpha, hidden) per vertex.
    pub vattrs: Vec<([f32; 3], [f32; 3], f32, bool)>,
    pub faces: Vec<IndexedFace>,
    /// Vertices kept even when no face uses them (isolated vertices: the
    /// source mesh's and ones Create Vertex adds).
    pub keep: BTreeSet<u32>,
}

impl IndexedMesh {
    pub fn from_mesh(mesh: &Mesh) -> Self {
        let positions = mesh.vertices().iter().map(|v| v.position).collect();
        let vattrs = mesh.vertices().iter().map(|v| (v.color, v.illum, v.alpha, v.hidden)).collect();
        let faces = (0..mesh.face_count())
            .filter_map(|i| {
                let f = &mesh.faces()[i];
                let corners = mesh.face_corners(FaceId(i as u32))?;
                let ids = corners.iter().map(|he| mesh.half_edges()[he.0 as usize].origin.0).collect();
                let mut uv = BTreeMap::new();
                for (ch, m) in mesh.maps() {
                    uv.insert(*ch, corners.iter().map(|he| m.verts.get(m.corners[he.0 as usize] as usize).copied().unwrap_or(Vec3::ZERO)).collect());
                }
                Some(IndexedFace { ids, smoothing_groups: f.smoothing_groups, material_id: f.material_id, hidden: f.hidden, uv })
            })
            .collect();
        let keep = mesh.vertices().iter().enumerate().filter(|(_, v)| v.he.is_none()).map(|(i, _)| i as u32).collect();
        Self { positions, vattrs, faces, keep }
    }

    /// Adds a vertex, copying attributes from `like` when given.
    pub fn add_vertex(&mut self, p: Vec3, like: Option<u32>) -> u32 {
        let attrs = like.and_then(|i| self.vattrs.get(i as usize).copied()).unwrap_or(([1.0; 3], [1.0; 3], 1.0, false));
        self.positions.push(p);
        self.vattrs.push(attrs);
        (self.positions.len() - 1) as u32
    }

    /// Builds the half-edge mesh. Fails on duplicate directed edges (a
    /// non-manifold result) or a face with fewer than three distinct vertices.
    /// Vertices keep their relative order: the ones a face uses plus `keep`,
    /// ascending by index, so ids of untouched vertices only shift down past
    /// removed ones (renumbered after a delete).
    pub fn build(&self) -> Result<Mesh, String> {
        let mut mesh = Mesh::default();
        let mut used: BTreeSet<u32> = self.keep.iter().copied().filter(|i| (*i as usize) < self.positions.len()).collect();
        for face in &self.faces {
            used.extend(face.ids.iter().copied());
        }
        let mut remap: BTreeMap<u32, VertexId> = BTreeMap::new();
        for id in used {
            let p = self.positions[id as usize];
            let (c, il, a, h) = self.vattrs.get(id as usize).copied().unwrap_or(([1.0; 3], [1.0; 3], 1.0, false));
            remap.insert(id, VertexId(mesh.vertices.len() as u32));
            mesh.vertices.push(Vertex { position: p, he: None, color: c, illum: il, alpha: a, hidden: h });
        }
        let mut directed: BTreeMap<(VertexId, VertexId), HalfEdgeId> = BTreeMap::new();
        // Channels present on any face; per channel, per half-edge, the corner uv.
        let channels: BTreeSet<u32> = self.faces.iter().flat_map(|f| f.uv.keys().copied()).collect();
        let mut corner_uvs: BTreeMap<u32, Vec<Vec3>> = channels.iter().map(|c| (*c, Vec::new())).collect();
        for face in &self.faces {
            let mut ids: Vec<u32> = Vec::with_capacity(face.ids.len());
            let mut kept: Vec<usize> = Vec::with_capacity(face.ids.len());
            for (k, id) in face.ids.iter().enumerate() {
                if ids.last() != Some(id) {
                    ids.push(*id);
                    kept.push(k);
                }
            }
            while ids.len() > 1 && ids.first() == ids.last() {
                ids.pop();
                kept.pop();
            }
            if ids.len() < 3 {
                return Err("a face needs at least three distinct vertices".into());
            }
            let vids: Vec<VertexId> = ids.iter().map(|id| remap[id]).collect();
            let face_id = FaceId(mesh.faces.len() as u32);
            let start = mesh.half_edges.len() as u32;
            let n = vids.len();
            for (ch, out) in corner_uvs.iter_mut() {
                match face.uv.get(ch) {
                    Some(vals) if vals.len() == face.ids.len() => out.extend(kept.iter().map(|k| vals[*k])),
                    _ => out.extend(std::iter::repeat_n(Vec3::ZERO, n)),
                }
            }
            for i in 0..n {
                let he_id = HalfEdgeId(start + i as u32);
                let (o, d) = (vids[i], vids[(i + 1) % n]);
                if directed.insert((o, d), he_id).is_some() {
                    return Err("the result would not be manifold (an edge used twice in the same direction)".into());
                }
                if mesh.vertices[o.0 as usize].he.is_none() {
                    mesh.vertices[o.0 as usize].he = Some(he_id);
                }
                mesh.half_edges.push(HalfEdge { origin: o, twin: he_id, next: HalfEdgeId(start + ((i + 1) % n) as u32), prev: HalfEdgeId(start + ((i + n - 1) % n) as u32), face: Some(face_id) });
            }
            mesh.faces.push(Face { he: HalfEdgeId(start), smoothing_groups: face.smoothing_groups, material_id: face.material_id, hidden: face.hidden });
        }
        // Twins, then boundary half-edges for unmatched face edges.
        let pairs: Vec<((VertexId, VertexId), HalfEdgeId)> = directed.iter().map(|(k, v)| (*k, *v)).collect();
        let mut boundary: Vec<HalfEdgeId> = Vec::new();
        for ((o, d), he) in pairs {
            if let Some(t) = directed.get(&(d, o)) {
                mesh.half_edges[he.0 as usize].twin = *t;
            } else {
                let b = HalfEdgeId(mesh.half_edges.len() as u32);
                mesh.half_edges.push(HalfEdge { origin: d, twin: he, next: b, prev: b, face: None });
                mesh.half_edges[he.0 as usize].twin = b;
                for out in corner_uvs.values_mut() {
                    out.push(Vec3::ZERO);
                }
                boundary.push(b);
            }
        }
        // Link each boundary half-edge (d→o) to the boundary half-edge that
        // leaves o on the same side of the fan: rotate around o away from the
        // face edge until the next gap. Vertices on two borders stay valid.
        for b in boundary {
            let h = mesh.half_edges[b.0 as usize].twin; // o→d in a face
            let mut cur = h;
            let mut next = None;
            for _ in 0..mesh.half_edges.len() {
                let prev = mesh.half_edges[cur.0 as usize].prev; // x→o in cur's face
                let t = mesh.half_edges[prev.0 as usize].twin; // o→x
                if mesh.half_edges[t.0 as usize].face.is_none() {
                    next = Some(t);
                    break;
                }
                cur = t;
                if cur == h {
                    break;
                }
            }
            if let Some(n) = next {
                mesh.half_edges[b.0 as usize].next = n;
                mesh.half_edges[n.0 as usize].prev = b;
            }
        }
        mesh.check_invariants()?;
        let origins: Vec<u32> = mesh.half_edges.iter().map(|h| h.origin.0).collect();
        for (ch, vals) in corner_uvs {
            let welded = weld_channel(&vals, &origins);
            mesh.set_map(ch, welded);
        }
        Ok(mesh)
    }

    /// Faces using vertex `v`, in index order.
    pub fn faces_with(&self, v: u32) -> Vec<usize> {
        self.faces.iter().enumerate().filter(|(_, f)| f.ids.contains(&v)).map(|(i, _)| i).collect()
    }

    /// Replaces every use of `from` by `to` in the faces.
    pub fn remap(&mut self, from: u32, to: u32) {
        for f in &mut self.faces {
            for id in &mut f.ids {
                if *id == from {
                    *id = to;
                }
            }
        }
    }

    /// Faces whose ids repeat or collapse below three vertices are dropped.
    pub fn drop_degenerate(&mut self) {
        self.faces.retain(|f| {
            let mut ids = f.ids.clone();
            ids.dedup();
            while ids.len() > 1 && ids.first() == ids.last() {
                ids.pop();
            }
            let mut sorted = ids.clone();
            sorted.sort();
            sorted.dedup();
            sorted.len() >= 3 && sorted.len() == ids.len()
        });
    }
}

/// Turns per-corner coordinates into a channel: corners of the same
/// geometry vertex with bit-identical values share one texture vertex (so
/// a corner welded in the editor stays welded, and two charts that happen
/// to coincide stay apart).
pub fn weld_channel(vals: &[Vec3], origins: &[u32]) -> MapChannel {
    weld_channel_charts(vals, origins, &[])
}

/// `weld_channel` with a chart id per corner: corners of different charts
/// (the six sides of a box map, every face of a face map) never share a
/// texture vertex even when their values coincide.
pub fn weld_channel_charts(vals: &[Vec3], origins: &[u32], charts: &[u32]) -> MapChannel {
    let mut index: std::collections::HashMap<(u32, u32, [u32; 3]), u32> = std::collections::HashMap::new();
    let mut verts = Vec::new();
    let corners = vals
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let key = (origins.get(i).copied().unwrap_or(u32::MAX), charts.get(i).copied().unwrap_or(0), [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]);
            *index.entry(key).or_insert_with(|| {
                verts.push(*v);
                (verts.len() - 1) as u32
            })
        })
        .collect();
    MapChannel { verts, corners }
}

/// Faces around vertex `v` of a half-edge mesh in rotation order, each with
/// the outgoing half-edge of `v` in that face. Consecutive entries share
/// the edge of the first one's outgoing half-edge; an open fan starts at
/// the border so it is contiguous.
pub fn fan_around(mesh: &Mesh, v: VertexId) -> Vec<(FaceId, HalfEdgeId)> {
    let hes = mesh.half_edges();
    let Some(mut start) = mesh.vertices().get(v.0 as usize).and_then(|x| x.he) else { return Vec::new() };
    if hes[start.0 as usize].face.is_none() {
        // v.he points along the border: rotate once to land in a face.
        let t = hes[start.0 as usize].twin;
        let n = hes[t.0 as usize].next;
        if hes[n.0 as usize].origin != v || hes[n.0 as usize].face.is_none() {
            return Vec::new();
        }
        start = n;
    }
    // Backwards (prev.twin) to the border, if any.
    let mut first = start;
    for _ in 0..hes.len() {
        let prev = hes[first.0 as usize].prev;
        let back = hes[prev.0 as usize].twin;
        if hes[back.0 as usize].face.is_none() || back == start {
            break;
        }
        first = back;
    }
    // Forwards (twin.next) collecting faces.
    let mut out = Vec::new();
    let mut cur = first;
    for _ in 0..hes.len() {
        let Some(f) = hes[cur.0 as usize].face else { break };
        out.push((f, cur));
        let twin = hes[cur.0 as usize].twin;
        let fwd = hes[twin.0 as usize].next;
        if fwd == first || hes[fwd.0 as usize].face.is_none() || hes[fwd.0 as usize].origin != v {
            break;
        }
        cur = fwd;
    }
    out
}

/// True when the fan around `v` is closed (no border at `v`).
pub fn fan_is_closed(mesh: &Mesh, v: VertexId) -> bool {
    let fan = fan_around(mesh, v);
    let Some((_, last)) = fan.last() else { return false };
    let hes = mesh.half_edges();
    let twin = hes[last.0 as usize].twin;
    let fwd = hes[twin.0 as usize].next;
    fan.first().is_some_and(|(_, f)| *f == fwd)
}
