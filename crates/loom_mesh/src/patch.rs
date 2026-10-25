//! Editable Patch: bicubic Bezier patches over a lattice of vertices and
//! handles (edge vectors). Quad patches are tensor-product cubics, tri
//! patches cubic triangles; the interior control points are automatic
//! (a quad's are the bicubic Coons interior of its four edges, so a patch
//! with straight handles is flat). A patch mesh is tessellated by Steps
//! for display and rendering.

use crate::indexed::{IndexedFace, IndexedMesh};
use crate::{FaceId, Mesh};
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// An edge between two vertices with its two handles: `vec[0]` next to
/// `v[0]`, `vec[1]` next to `v[1]` (indices into `PatchMesh::vecs`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatchEdge {
    pub v: [u32; 2],
    pub vec: [u32; 2],
}

/// A tri (3 corners) or quad (4 corners) patch, corners counter-clockwise
/// seen from the side its normal points to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Patch {
    pub v: Vec<u32>,
    #[serde(default = "one_u32")]
    pub smoothing_groups: u32,
    #[serde(default = "one_u16")]
    pub material_id: u16,
}

fn one_u32() -> u32 {
    1
}
fn one_u16() -> u16 {
    1
}

impl Patch {
    pub fn new(v: Vec<u32>) -> Self {
        Self { v, smoothing_groups: 1, material_id: 1 }
    }
    pub fn is_quad(&self) -> bool {
        self.v.len() == 4
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PatchMesh {
    pub verts: Vec<Vec3>,
    pub vecs: Vec<Vec3>,
    pub edges: Vec<PatchEdge>,
    pub patches: Vec<Patch>,
}

/// Cubic Bezier point.
fn bez(p: [Vec3; 4], t: f32) -> Vec3 {
    let s = 1.0 - t;
    p[0] * (s * s * s) + p[1] * (3.0 * s * s * t) + p[2] * (3.0 * s * t * t) + p[3] * (t * t * t)
}

impl PatchMesh {
    /// The edge joining `a` and `b`: (index, true when stored as b→a).
    pub fn edge_index(&self, a: u32, b: u32) -> Option<(usize, bool)> {
        self.edges.iter().position(|e| e.v == [a, b]).map(|i| (i, false)).or_else(|| self.edges.iter().position(|e| e.v == [b, a]).map(|i| (i, true)))
    }

    /// The edge a–b, made with straight handles (at a third and two thirds)
    /// when it does not exist yet.
    pub fn ensure_edge(&mut self, a: u32, b: u32) -> usize {
        if let Some((i, _)) = self.edge_index(a, b) {
            return i;
        }
        let (pa, pb) = (self.verts[a as usize], self.verts[b as usize]);
        let h0 = self.vecs.len() as u32;
        self.vecs.push(pa.lerp(pb, 1.0 / 3.0));
        self.vecs.push(pa.lerp(pb, 2.0 / 3.0));
        self.edges.push(PatchEdge { v: [a, b], vec: [h0, h0 + 1] });
        self.edges.len() - 1
    }

    /// Adds a patch on existing vertices (edges made as needed).
    pub fn add_patch(&mut self, v: Vec<u32>) -> usize {
        for k in 0..v.len() {
            self.ensure_edge(v[k], v[(k + 1) % v.len()]);
        }
        self.patches.push(Patch::new(v));
        self.patches.len() - 1
    }

    /// The control points of the directed edge a→b: [a, handle at a, handle at b, b].
    pub fn edge_curve(&self, a: u32, b: u32) -> [Vec3; 4] {
        let (pa, pb) = (self.verts[a as usize], self.verts[b as usize]);
        match self.edge_index(a, b) {
            Some((i, rev)) => {
                let e = &self.edges[i];
                let (h0, h1) = (self.vecs[e.vec[0] as usize], self.vecs[e.vec[1] as usize]);
                if rev { [pa, h1, h0, pb] } else { [pa, h0, h1, pb] }
            }
            None => [pa, pa.lerp(pb, 1.0 / 3.0), pa.lerp(pb, 2.0 / 3.0), pb],
        }
    }

    /// The 4 × 4 control net of a quad patch, `b[i][j]` with i along
    /// corner 0 → 1 and j along corner 0 → 3.
    pub fn quad_net(&self, p: &Patch) -> [[Vec3; 4]; 4] {
        let v = &p.v;
        let e0 = self.edge_curve(v[0], v[1]);
        let e1 = self.edge_curve(v[1], v[2]);
        let e2 = self.edge_curve(v[3], v[2]);
        let e3 = self.edge_curve(v[0], v[3]);
        let mut b = [[Vec3::ZERO; 4]; 4];
        for k in 0..4 {
            b[k][0] = e0[k];
            b[3][k] = e1[k];
            b[k][3] = e2[k];
            b[0][k] = e3[k];
        }
        // Coons interior (the bilinearly blended patch of the four edges).
        let c = |b: &[[Vec3; 4]; 4], i: usize, j: usize| -> Vec3 {
            let (fi, fj) = (|x: usize| if i == 1 { x } else { 3 - x }, |x: usize| if j == 1 { x } else { 3 - x });
            let g = |x: usize, y: usize| b[fi(x)][fj(y)];
            (g(0, 0) * -4.0 + (g(1, 0) + g(0, 1)) * 6.0 - (g(3, 0) + g(0, 3)) * 2.0 + (g(3, 1) + g(1, 3)) * 3.0 - g(3, 3)) / 9.0
        };
        let (b11, b21, b12, b22) = (c(&b, 1, 1), c(&b, 2, 1), c(&b, 1, 2), c(&b, 2, 2));
        b[1][1] = b11;
        b[2][1] = b21;
        b[1][2] = b12;
        b[2][2] = b22;
        b
    }

    /// The ten control points of a tri patch: corners, the six edge
    /// handles and the automatic centre.
    fn tri_net(&self, p: &Patch) -> TriNet {
        let v = &p.v;
        let e0 = self.edge_curve(v[0], v[1]);
        let e1 = self.edge_curve(v[1], v[2]);
        let e2 = self.edge_curve(v[2], v[0]);
        let corners = (e0[0] + e1[0] + e2[0]) / 3.0;
        let edge_avg = (e0[1] + e0[2] + e1[1] + e1[2] + e2[1] + e2[2]) / 6.0;
        TriNet { e0, e1, e2, centre: edge_avg + (edge_avg - corners) * 0.5 }
    }

    /// A point of a quad patch at (u, v).
    pub fn quad_point(&self, p: &Patch, u: f32, v: f32) -> Vec3 {
        let b = self.quad_net(p);
        let rows: [Vec3; 4] = std::array::from_fn(|j| bez([b[0][j], b[1][j], b[2][j], b[3][j]], u));
        bez(rows, v)
    }

    /// A point of patch `p` at (u, v): a quad's tensor-product point, a tri's
    /// point with weights (u, v, 1 − u − v) towards its corners 0, 1, 2.
    pub fn surface_point(&self, p: usize, u: f32, v: f32) -> Option<Vec3> {
        let pa = self.patches.get(p)?;
        if pa.v.iter().any(|i| *i as usize >= self.verts.len()) {
            return None;
        }
        match pa.v.len() {
            4 => Some(self.quad_point(pa, u, v)),
            3 => Some(self.tri_net(pa).point(u, v, (1.0 - u - v).max(0.0))),
            _ => None,
        }
    }

    /// The normal at a patch's first corner direction (flat estimate from the corners).
    pub fn patch_normal(&self, p: usize) -> Vec3 {
        let Some(pa) = self.patches.get(p) else { return Vec3::Z };
        let pts: Vec<Vec3> = pa.v.iter().map(|i| self.verts[*i as usize]).collect();
        let mut n = Vec3::ZERO;
        for i in 0..pts.len() {
            n += pts[i].cross(pts[(i + 1) % pts.len()]);
        }
        n.normalize_or_zero()
    }

    /// Patch Grids > Quad Patch: `width` along X, `length` along Y, centred
    /// on the origin, `lsegs` × `wsegs` quad patches facing +Z.
    pub fn quad_grid(length: f32, width: f32, lsegs: u32, wsegs: u32) -> Self {
        let (ls, ws) = (lsegs.max(1), wsegs.max(1));
        let mut pm = PatchMesh::default();
        for j in 0..=ls {
            for i in 0..=ws {
                pm.verts.push(Vec3::new(-width * 0.5 + width * i as f32 / ws as f32, -length * 0.5 + length * j as f32 / ls as f32, 0.0));
            }
        }
        let id = |i: u32, j: u32| j * (ws + 1) + i;
        for j in 0..ls {
            for i in 0..ws {
                pm.add_patch(vec![id(i, j), id(i + 1, j), id(i + 1, j + 1), id(i, j + 1)]);
            }
        }
        pm
    }

    /// Patch Grids > Tri Patch: the rectangle cut by one diagonal into two
    /// tri patches facing +Z.
    pub fn tri_grid(length: f32, width: f32) -> Self {
        let (w, l) = (width * 0.5, length * 0.5);
        let mut pm = PatchMesh { verts: vec![Vec3::new(-w, -l, 0.0), Vec3::new(w, -l, 0.0), Vec3::new(w, l, 0.0), Vec3::new(-w, l, 0.0)], ..Default::default() };
        pm.add_patch(vec![0, 1, 2]);
        pm.add_patch(vec![0, 2, 3]);
        pm
    }

    /// Convert to Editable Patch from a mesh: triangles become tri patches,
    /// quads quad patches, larger polygons a fan of tri patches; the handles
    /// lie on the straight edges, so the surface keeps its shape.
    pub fn from_mesh(mesh: &Mesh) -> Self {
        let mut pm = PatchMesh { verts: mesh.vertices().iter().map(|v| v.position).collect(), ..Default::default() };
        for f in 0..mesh.face_count() {
            let Some(ids) = mesh.face_vertex_ids(FaceId(f as u32)) else { continue };
            let ids: Vec<u32> = ids.iter().map(|v| v.0).collect();
            let face = &mesh.faces()[f];
            let push = |pm: &mut PatchMesh, v: Vec<u32>| {
                let i = pm.add_patch(v);
                pm.patches[i].smoothing_groups = face.smoothing_groups;
                pm.patches[i].material_id = face.material_id;
            };
            match ids.len() {
                3 | 4 => push(&mut pm, ids),
                n if n > 4 => {
                    for k in 1..n - 1 {
                        push(&mut pm, vec![ids[0], ids[k], ids[k + 1]]);
                    }
                }
                _ => {}
            }
        }
        pm
    }

    /// The tessellated surface: each patch edge cut into `steps + 1`
    /// segments. Edges inside a patch are invisible unless `interior`
    /// (Show Interior Edges).
    pub fn tessellate(&self, steps: u32, interior: bool) -> Mesh {
        self.tessellate_with(steps, interior, true).or_else(|_| self.tessellate_with(steps, interior, false)).unwrap_or_default()
    }

    fn tessellate_with(&self, steps: u32, interior: bool, shared: bool) -> Result<Mesh, String> {
        let n = (steps.min(100) + 1) as usize;
        let mut im = IndexedMesh::default();
        let mut vis: Vec<Vec<bool>> = Vec::new();
        let mut corner_ids: HashMap<(usize, u32), u32> = HashMap::new();
        let mut edge_ids: HashMap<(usize, usize, usize), u32> = HashMap::new();
        for (pi, p) in self.patches.iter().enumerate() {
            let scope = if shared { usize::MAX } else { pi };
            if p.v.iter().any(|v| *v as usize >= self.verts.len()) || !(p.v.len() == 3 || p.v.len() == 4) {
                continue;
            }
            // The id of point k (0..=n from a) on the edge a→b.
            let mut edge_point = |im: &mut IndexedMesh, a: u32, b: u32, k: usize| -> u32 {
                if k == 0 || k == n {
                    let v = if k == 0 { a } else { b };
                    return *corner_ids.entry((scope, v)).or_insert_with(|| im.add_vertex(self.verts[v as usize], None));
                }
                let (ei, rev) = self.edge_index(a, b).unwrap_or((usize::MAX, false));
                let kk = if rev { n - k } else { k };
                let key = (scope, ei, kk);
                if let Some(id) = edge_ids.get(&key) {
                    return *id;
                }
                let curve = if ei == usize::MAX { self.edge_curve(a, b) } else { let e = &self.edges[ei]; self.edge_curve(e.v[0], e.v[1]) };
                let t = if ei == usize::MAX { k as f32 / n as f32 } else { kk as f32 / n as f32 };
                let id = im.add_vertex(bez(curve, t), None);
                if ei != usize::MAX {
                    edge_ids.insert(key, id);
                }
                id
            };
            let face = |ids: Vec<u32>| IndexedFace { ids, smoothing_groups: p.smoothing_groups, material_id: p.material_id, ..Default::default() };
            if p.is_quad() {
                let v = &p.v;
                let net = self.quad_net(p);
                let mut g = vec![vec![0u32; n + 1]; n + 1];
                for (i, row) in g.iter_mut().enumerate() {
                    for (j, cell) in row.iter_mut().enumerate() {
                        *cell = if j == 0 {
                            edge_point(&mut im, v[0], v[1], i)
                        } else if j == n {
                            edge_point(&mut im, v[3], v[2], i)
                        } else if i == 0 {
                            edge_point(&mut im, v[0], v[3], j)
                        } else if i == n {
                            edge_point(&mut im, v[1], v[2], j)
                        } else {
                            let (u, w) = (i as f32 / n as f32, j as f32 / n as f32);
                            let rows: [Vec3; 4] = std::array::from_fn(|jj| bez([net[0][jj], net[1][jj], net[2][jj], net[3][jj]], u));
                            im.add_vertex(bez(rows, w), None)
                        };
                    }
                }
                for j in 0..n {
                    for i in 0..n {
                        im.faces.push(face(vec![g[i][j], g[i + 1][j], g[i + 1][j + 1], g[i][j + 1]]));
                        vis.push(vec![interior || j == 0, interior || i + 1 == n, interior || j + 1 == n, interior || i == 0]);
                    }
                }
            } else {
                let v = &p.v;
                let net = self.tri_net(p);
                // Point (a, b, c), a + b + c = n: weights towards corners 0, 1, 2.
                let mut pts: HashMap<(usize, usize), u32> = HashMap::new();
                for b in 0..=n {
                    for c in 0..=(n - b) {
                        let a = n - b - c;
                        let id = if c == 0 {
                            edge_point(&mut im, v[0], v[1], b)
                        } else if a == 0 {
                            edge_point(&mut im, v[1], v[2], c)
                        } else if b == 0 {
                            edge_point(&mut im, v[2], v[0], a)
                        } else {
                            im.add_vertex(net.point(a as f32 / n as f32, b as f32 / n as f32, c as f32 / n as f32), None)
                        };
                        pts.insert((b, c), id);
                    }
                }
                let on_border = |x: (usize, usize), y: (usize, usize)| {
                    let a = |q: (usize, usize)| n - q.0 - q.1;
                    (x.1 == 0 && y.1 == 0) || (a(x) == 0 && a(y) == 0) || (x.0 == 0 && y.0 == 0)
                };
                for b in 0..n {
                    for c in 0..(n - b) {
                        let tri_cells = [[(b, c), (b + 1, c), (b, c + 1)]];
                        let down = if n - b - c >= 2 { Some([(b + 1, c), (b + 1, c + 1), (b, c + 1)]) } else { None };
                        for cell in tri_cells.iter().chain(down.iter()) {
                            im.faces.push(face(cell.iter().map(|q| pts[q]).collect()));
                            vis.push((0..3).map(|k| interior || on_border(cell[k], cell[(k + 1) % 3])).collect());
                        }
                    }
                }
            }
        }
        crate::tri_mesh::build(&im, &vis)
    }

    /// The lattice the sub-object levels pick on: vertices `0..verts`, then
    /// the handles as loose vertices, and one polygon per patch.
    pub fn cage(&self) -> Mesh {
        let mut im = IndexedMesh::default();
        for p in self.verts.iter().chain(self.vecs.iter()) {
            let id = im.add_vertex(*p, None);
            im.keep.insert(id);
        }
        let loose = im.clone();
        for p in &self.patches {
            im.faces.push(IndexedFace { ids: p.v.clone(), smoothing_groups: p.smoothing_groups, material_id: p.material_id, ..Default::default() });
        }
        im.build().or_else(|_| loose.build()).unwrap_or_default()
    }

    /// The lattice moved as the cage was: a vertex carries its handles
    /// along unless those were moved themselves.
    pub fn with_cage(&self, cage: &Mesh) -> PatchMesh {
        let nv = self.verts.len();
        let mut out = self.clone();
        if cage.vertex_count() != nv + self.vecs.len() {
            return out;
        }
        let pos = |i: usize| cage.vertices()[i].position;
        let moved_vec: Vec<bool> = (0..self.vecs.len()).map(|h| pos(nv + h) != self.vecs[h]).collect();
        for (h, m) in moved_vec.iter().enumerate() {
            if *m {
                out.vecs[h] = pos(nv + h);
            }
        }
        for v in 0..nv {
            let delta = pos(v) - self.verts[v];
            if delta == Vec3::ZERO {
                continue;
            }
            out.verts[v] = pos(v);
            for e in &self.edges {
                for s in 0..2 {
                    if e.v[s] as usize == v && !moved_vec[e.vec[s] as usize] {
                        out.vecs[e.vec[s] as usize] += delta;
                    }
                }
            }
        }
        out
    }

    /// Patches using each edge.
    fn edge_uses(&self) -> Vec<usize> {
        let mut uses = vec![0usize; self.edges.len()];
        for p in &self.patches {
            for k in 0..p.v.len() {
                if let Some((i, _)) = self.edge_index(p.v[k], p.v[(k + 1) % p.v.len()]) {
                    uses[i] += 1;
                }
            }
        }
        uses
    }

    /// The edge is used by one patch only (Add Tri / Add Quad need that).
    pub fn is_open_edge(&self, e: usize) -> bool {
        self.edge_uses().get(e).is_some_and(|u| *u == 1)
    }

    /// Drops vertices, handles and edges nothing uses, renumbering the rest.
    pub fn compact(&mut self) {
        let used_edges: BTreeSet<usize> = self
            .patches
            .iter()
            .flat_map(|p| (0..p.v.len()).filter_map(|k| self.edge_index(p.v[k], p.v[(k + 1) % p.v.len()]).map(|x| x.0)).collect::<Vec<_>>())
            .collect();
        let edges: Vec<PatchEdge> = used_edges.iter().map(|i| self.edges[*i].clone()).collect();
        let used_v: BTreeSet<u32> = self.patches.iter().flat_map(|p| p.v.iter().copied()).chain(edges.iter().flat_map(|e| e.v)).collect();
        let used_h: BTreeSet<u32> = edges.iter().flat_map(|e| e.vec).collect();
        let vmap: BTreeMap<u32, u32> = used_v.iter().enumerate().map(|(i, v)| (*v, i as u32)).collect();
        let hmap: BTreeMap<u32, u32> = used_h.iter().enumerate().map(|(i, h)| (*h, i as u32)).collect();
        self.verts = used_v.iter().map(|v| self.verts[*v as usize]).collect();
        self.vecs = used_h.iter().map(|h| self.vecs[*h as usize]).collect();
        self.edges = edges.into_iter().map(|e| PatchEdge { v: [vmap[&e.v[0]], vmap[&e.v[1]]], vec: [hmap[&e.vec[0]], hmap[&e.vec[1]]] }).collect();
        for p in &mut self.patches {
            for v in &mut p.v {
                *v = vmap[v];
            }
        }
    }

    /// Delete (patch level): the patches go, with what only they used.
    pub fn delete_patches(&mut self, ids: &[usize]) {
        let del: BTreeSet<usize> = ids.iter().copied().collect();
        let mut i = 0;
        self.patches.retain(|_| {
            let keep = !del.contains(&i);
            i += 1;
            keep
        });
        self.compact();
    }

    /// Delete (vertex level): the patches using the vertices go.
    pub fn delete_vertices(&mut self, verts: &[u32]) {
        let del: BTreeSet<u32> = verts.iter().copied().collect();
        let ids: Vec<usize> = self.patches.iter().enumerate().filter(|(_, p)| p.v.iter().any(|v| del.contains(v))).map(|(i, _)| i).collect();
        self.delete_patches(&ids);
    }

    /// Add Tri / Add Quad: a new patch on an open edge, continuing the
    /// surface outward in the plane of the patch it borders. Returns the
    /// new patch.
    pub fn add_on_edge(&mut self, e: usize, quad: bool) -> Result<usize, String> {
        if !self.is_open_edge(e) {
            return Err("Add Tri / Add Quad need an open edge (used by one patch)".into());
        }
        let [x, y] = self.edges[e].v;
        // The direction the bordering patch runs along the edge.
        let (pi, a, b) = self
            .patches
            .iter()
            .enumerate()
            .find_map(|(pi, p)| {
                (0..p.v.len()).find_map(|k| {
                    let (a, b) = (p.v[k], p.v[(k + 1) % p.v.len()]);
                    ((a, b) == (x, y) || (a, b) == (y, x)).then_some((pi, a, b))
                })
            })
            .ok_or("the edge borders no patch")?;
        let (pa, pb) = (self.verts[a as usize], self.verts[b as usize]);
        let n = self.patch_normal(pi);
        let out = (pb - pa).cross(n).normalize_or_zero() * pa.distance(pb);
        let id = if quad {
            let c = self.verts.len() as u32;
            self.verts.push(pa + out);
            self.verts.push(pb + out);
            self.add_patch(vec![b, a, c, c + 1])
        } else {
            let c = self.verts.len() as u32;
            self.verts.push((pa + pb) * 0.5 + out);
            self.add_patch(vec![b, a, c])
        };
        Ok(id)
    }

    /// Weld: selected vertices closer than `threshold` merge at their
    /// average. Returns how many vertices went.
    pub fn weld(&mut self, verts: &[u32], threshold: f32) -> usize {
        let sel: Vec<u32> = verts.iter().copied().filter(|v| (*v as usize) < self.verts.len()).collect();
        let mut target: BTreeMap<u32, u32> = BTreeMap::new();
        for (i, a) in sel.iter().enumerate() {
            if target.contains_key(a) {
                continue;
            }
            let mut cluster = vec![*a];
            for b in &sel[i + 1..] {
                if !target.contains_key(b) && self.verts[*a as usize].distance(self.verts[*b as usize]) <= threshold {
                    cluster.push(*b);
                }
            }
            if cluster.len() > 1 {
                let avg = cluster.iter().map(|v| self.verts[*v as usize]).sum::<Vec3>() / cluster.len() as f32;
                let old = self.verts[*a as usize];
                self.verts[*a as usize] = avg;
                for v in &cluster {
                    target.insert(*v, *a);
                }
                // Handles at the welded vertex follow it.
                let d = avg - old;
                for e in &self.edges {
                    for s in 0..2 {
                        if cluster.contains(&e.v[s]) {
                            self.vecs[e.vec[s] as usize] += if e.v[s] == *a { d } else { avg - self.verts[e.v[s] as usize] };
                        }
                    }
                }
            }
        }
        let gone = target.iter().filter(|(k, v)| k != v).count();
        if gone == 0 {
            return 0;
        }
        let map = |v: u32| target.get(&v).copied().unwrap_or(v);
        for e in &mut self.edges {
            e.v = [map(e.v[0]), map(e.v[1])];
        }
        // Edges collapsed to a point go; doubled edges keep the first.
        let mut seen: BTreeSet<(u32, u32)> = BTreeSet::new();
        self.edges.retain(|e| e.v[0] != e.v[1] && seen.insert((e.v[0].min(e.v[1]), e.v[0].max(e.v[1]))));
        for p in &mut self.patches {
            for v in &mut p.v {
                *v = map(*v);
            }
            p.v.dedup();
            while p.v.len() > 1 && p.v.first() == p.v.last() {
                p.v.pop();
            }
        }
        self.patches.retain(|p| p.v.len() >= 3);
        self.compact();
        gone
    }

    /// Extrude (patches): the selected patches move `amount` along their
    /// normals with side patches joining them to the rest; `outline` then
    /// grows (or shrinks) their outline in their plane (Bevel). The
    /// patches keep their ids.
    pub fn extrude(&mut self, ids: &[usize], amount: f32, outline: f32) -> Result<(), String> {
        let sel: BTreeSet<usize> = ids.iter().copied().filter(|p| *p < self.patches.len()).collect();
        if sel.is_empty() {
            return Err("select patches first".into());
        }
        let mut normals: BTreeMap<u32, Vec3> = BTreeMap::new();
        for p in &sel {
            let n = self.patch_normal(*p);
            for v in &self.patches[*p].v {
                *normals.entry(*v).or_default() += n;
            }
        }
        let mut directed: BTreeSet<(u32, u32)> = BTreeSet::new();
        for p in &sel {
            let v = &self.patches[*p].v;
            for k in 0..v.len() {
                directed.insert((v[k], v[(k + 1) % v.len()]));
            }
        }
        let outline_edges: Vec<(u32, u32)> = directed.iter().copied().filter(|(a, b)| !directed.contains(&(*b, *a))).collect();
        // New vertices.
        let mut map: BTreeMap<u32, u32> = BTreeMap::new();
        let mut delta: BTreeMap<u32, Vec3> = BTreeMap::new();
        for (v, n) in &normals {
            let d = n.normalize_or_zero() * amount;
            map.insert(*v, self.verts.len() as u32);
            delta.insert(*v, d);
            self.verts.push(self.verts[*v as usize] + d);
        }
        // Outline: the outline vertices move in the plane.
        if outline != 0.0 {
            let mut dirs: BTreeMap<u32, Vec<Vec3>> = BTreeMap::new();
            for (a, b) in &outline_edges {
                let n = (normals[a] + normals[b]).normalize_or_zero();
                let d = (self.verts[*b as usize] - self.verts[*a as usize]).cross(n).normalize_or_zero();
                dirs.entry(*a).or_default().push(d);
                dirs.entry(*b).or_default().push(d);
            }
            for (v, ds) in dirs {
                let d = ds.iter().copied().sum::<Vec3>().normalize_or_zero();
                let cos = ds.iter().map(|x| x.dot(d)).fold(1.0f32, f32::min).max(0.2);
                let off = d * (outline / cos);
                self.verts[map[&v] as usize] += off;
                *delta.get_mut(&v).unwrap() += off;
            }
        }
        // The cap's edges: copies of the old ones, handles moved with their vertices.
        let cap_edges: BTreeSet<(u32, u32)> = directed.iter().map(|(a, b)| (*a.min(b), *a.max(b))).collect();
        for (a, b) in cap_edges {
            let Some((i, _)) = self.edge_index(a, b) else { continue };
            let e = self.edges[i].clone();
            let h0 = self.vecs.len() as u32;
            self.vecs.push(self.vecs[e.vec[0] as usize] + delta[&e.v[0]]);
            self.vecs.push(self.vecs[e.vec[1] as usize] + delta[&e.v[1]]);
            self.edges.push(PatchEdge { v: [map[&e.v[0]], map[&e.v[1]]], vec: [h0, h0 + 1] });
        }
        for p in &sel {
            let v: Vec<u32> = self.patches[*p].v.iter().map(|v| map[v]).collect();
            self.patches[*p].v = v;
        }
        for (a, b) in outline_edges {
            let src = self.patches[*sel.iter().next().unwrap()].clone();
            let i = self.add_patch(vec![a, b, map[&b], map[&a]]);
            self.patches[i].smoothing_groups = src.smoothing_groups;
            self.patches[i].material_id = src.material_id;
        }
        self.compact();
        Ok(())
    }

    /// Relax: each vertex moves `amount` of the way to the average of its
    /// edge neighbours, `iterations` times; boundary vertices stay when
    /// `keep_boundary`. Handles follow their vertices.
    pub fn relax(&mut self, verts: &[u32], amount: f32, iterations: u32, keep_boundary: bool) {
        let uses = self.edge_uses();
        let boundary: BTreeSet<u32> = self.edges.iter().zip(uses.iter()).filter(|(_, u)| **u < 2).flat_map(|(e, _)| e.v).collect();
        let sel: BTreeSet<u32> = verts.iter().copied().filter(|v| !(keep_boundary && boundary.contains(v))).collect();
        for _ in 0..iterations {
            let mut moved = Vec::new();
            for v in &sel {
                let nb: Vec<Vec3> = self.edges.iter().filter_map(|e| if e.v[0] == *v { Some(e.v[1]) } else if e.v[1] == *v { Some(e.v[0]) } else { None }).map(|u| self.verts[u as usize]).collect();
                if nb.is_empty() {
                    continue;
                }
                let avg = nb.iter().copied().sum::<Vec3>() / nb.len() as f32;
                let p = self.verts[*v as usize];
                moved.push((*v, (avg - p) * amount));
            }
            for (v, d) in moved {
                self.verts[v as usize] += d;
                for e in &self.edges {
                    for s in 0..2 {
                        if e.v[s] == v {
                            self.vecs[e.vec[s] as usize] += d;
                        }
                    }
                }
            }
        }
    }

    /// Flip (normals) of the patches.
    pub fn flip(&mut self, ids: &[usize]) {
        for p in ids {
            if let Some(pa) = self.patches.get_mut(*p) {
                pa.v.reverse();
            }
        }
    }

    /// The patches joined to `seed` through shared edges (Element).
    pub fn element_of(&self, seed: usize) -> Vec<usize> {
        let mut seen = BTreeSet::from([seed]);
        let mut stack = vec![seed];
        while let Some(p) = stack.pop() {
            let vs: BTreeSet<u32> = self.patches[p].v.iter().copied().collect();
            for (q, other) in self.patches.iter().enumerate() {
                if seen.contains(&q) {
                    continue;
                }
                if other.v.iter().filter(|v| vs.contains(v)).count() >= 2 {
                    seen.insert(q);
                    stack.push(q);
                }
            }
        }
        seen.into_iter().collect()
    }
}

/// The control net of a cubic triangle.
struct TriNet {
    e0: [Vec3; 4],
    e1: [Vec3; 4],
    e2: [Vec3; 4],
    centre: Vec3,
}

impl TriNet {
    /// The point with weights (u, v, w) towards corners 0, 1, 2.
    fn point(&self, u: f32, v: f32, w: f32) -> Vec3 {
        let (e0, e1, e2) = (&self.e0, &self.e1, &self.e2);
        // b300 b030 b003, b210 b120 (edge 0), b021 b012 (edge 1), b102 b201 (edge 2).
        e0[0] * (u * u * u)
            + e0[3] * (v * v * v)
            + e1[3] * (w * w * w)
            + e0[1] * (3.0 * u * u * v)
            + e0[2] * (3.0 * u * v * v)
            + e1[1] * (3.0 * v * v * w)
            + e1[2] * (3.0 * v * w * w)
            + e2[1] * (3.0 * w * w * u)
            + e2[2] * (3.0 * w * u * u)
            + self.centre * (6.0 * u * v * w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_quad_grid_tessellates_flat_with_hidden_interior() {
        let pm = PatchMesh::quad_grid(20.0, 30.0, 1, 1);
        assert_eq!(pm.patches.len(), 1);
        assert_eq!(pm.vecs.len(), 8);
        let m = pm.tessellate(5, false);
        assert_eq!(m.face_count(), 36);
        assert_eq!(m.vertex_count(), 49);
        assert!(m.vertices().iter().all(|v| v.position.z.abs() < 1e-5));
        let (lo, hi) = m.aabb();
        assert!((hi.x - lo.x - 30.0).abs() < 1e-4 && (hi.y - lo.y - 20.0).abs() < 1e-4);
        // Only the patch outline shows: 4 sides × 6 segments.
        assert_eq!(m.unique_edges().len(), 24);
        assert_eq!(pm.tessellate(5, true).unique_edges().len(), 84, "Show Interior Edges");
        assert!(m.face_normal(FaceId(0)).z > 0.99);
    }

    #[test]
    fn neighbouring_patches_share_tessellated_edges() {
        let pm = PatchMesh::quad_grid(10.0, 20.0, 1, 2);
        let m = pm.tessellate(3, false);
        assert_eq!(m.face_count(), 2 * 16);
        assert_eq!(m.vertex_count(), 5 * 9, "the shared edge's points are welded");
        assert!(m.check_invariants().is_ok());
    }

    #[test]
    fn tri_patch_tessellates_into_steps_squared_triangles() {
        let pm = PatchMesh::tri_grid(10.0, 10.0);
        let m = pm.tessellate(2, true);
        assert_eq!(m.face_count(), 2 * 9);
        assert!(m.vertices().iter().all(|v| v.position.z.abs() < 1e-5));
        assert!(m.check_invariants().is_ok());
    }

    #[test]
    fn moving_a_handle_bends_the_surface_and_steps_refine_it() {
        let mut pm = PatchMesh::quad_grid(10.0, 10.0, 1, 1);
        pm.vecs[0].z += 3.0;
        let m = pm.tessellate(5, false);
        let top = m.vertices().iter().map(|v| v.position.z).fold(f32::MIN, f32::max);
        assert!(top > 0.5, "the edge bulges: {top}");
        assert!(pm.tessellate(1, false).vertex_count() < m.vertex_count());
    }

    #[test]
    fn cage_moves_carry_handles() {
        let pm = PatchMesh::quad_grid(10.0, 10.0, 1, 1);
        let mut cage = pm.cage();
        assert_eq!(cage.vertex_count(), 4 + 8);
        assert_eq!(cage.face_count(), 1);
        cage.vertices_mut()[0].position += Vec3::Z;
        let moved = pm.with_cage(&cage);
        assert_eq!(moved.verts[0].z, 1.0);
        let e = &moved.edges[0];
        assert_eq!(moved.vecs[e.vec[0] as usize].z, 1.0, "the handle at the vertex follows it");
        assert_eq!(moved.vecs[e.vec[1] as usize].z, 0.0);
    }

    #[test]
    fn add_quad_weld_and_extrude() {
        let mut pm = PatchMesh::quad_grid(10.0, 10.0, 1, 1);
        let e = (0..pm.edges.len()).find(|e| pm.is_open_edge(*e)).unwrap();
        let p = pm.add_on_edge(e, true).unwrap();
        assert_eq!(pm.patches.len(), 2);
        assert_eq!(pm.verts.len(), 6);
        assert!(!pm.is_open_edge(e));
        assert!(pm.patch_normal(p).z > 0.99, "the new patch faces the same way");
        assert!(pm.tessellate(2, false).check_invariants().is_ok());
        pm.add_on_edge((0..pm.edges.len()).find(|e| pm.is_open_edge(*e)).unwrap(), false).unwrap();
        assert_eq!(pm.patches.len(), 3);

        let mut two = PatchMesh::quad_grid(10.0, 10.0, 1, 1);
        let other = PatchMesh::quad_grid(10.0, 10.0, 1, 1);
        let off = two.verts.len() as u32;
        for v in &other.verts {
            two.verts.push(*v + Vec3::X * 10.0);
        }
        two.add_patch(other.patches[0].v.iter().map(|v| v + off).collect());
        assert_eq!(two.verts.len(), 8);
        let all: Vec<u32> = (0..8).collect();
        assert_eq!(two.weld(&all, 0.1), 2);
        assert_eq!(two.verts.len(), 6);
        assert!(two.tessellate(2, false).check_invariants().is_ok());

        let mut ex = PatchMesh::quad_grid(10.0, 10.0, 1, 1);
        ex.extrude(&[0], 5.0, 0.0).unwrap();
        assert_eq!(ex.patches.len(), 5);
        assert!(ex.verts.iter().filter(|v| (v.z - 5.0).abs() < 1e-5).count() == 4);
        let m = ex.tessellate(2, false);
        assert!(m.check_invariants().is_ok());
        let mut bev = PatchMesh::quad_grid(10.0, 10.0, 1, 1);
        bev.extrude(&[0], 5.0, -2.0).unwrap();
        let (lo, hi) = bev.tessellate(1, false).aabb();
        assert!((hi.x - lo.x - 10.0).abs() < 1e-4);
        let top: Vec<f32> = bev.verts.iter().filter(|v| (v.z - 5.0).abs() < 1e-5).map(|v| v.x).collect();
        assert!(top.iter().all(|x| x.abs() < 3.0 + 1e-4), "{top:?}");
    }

    #[test]
    fn mesh_converts_to_patches_with_the_same_shape() {
        let b = crate::box_primitive(Vec3::splat(5.0));
        let pm = PatchMesh::from_mesh(&b);
        assert_eq!(pm.patches.len(), 6);
        assert_eq!(pm.edges.len(), 12);
        let m = pm.tessellate(2, false);
        let (lo, hi) = m.aabb();
        assert!((hi - lo - Vec3::splat(10.0)).length() < 1e-3);
        assert!(m.check_invariants().is_ok());
        assert_eq!(m.unique_edges().len(), 12 * 3);
    }

    #[test]
    fn relax_smooths_and_delete_compacts() {
        let mut pm = PatchMesh::quad_grid(10.0, 10.0, 2, 2);
        pm.verts[4].z = 4.0;
        pm.relax(&[4], 0.5, 1, true);
        assert!((pm.verts[4].z - 2.0).abs() < 1e-4);
        pm.delete_patches(&[0, 1]);
        assert_eq!(pm.patches.len(), 2);
        assert_eq!(pm.verts.len(), 6);
    }
}
