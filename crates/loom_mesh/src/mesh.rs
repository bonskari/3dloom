use crate::half_edge::*;

/// A polygon with the face attributes a rebuild must carry.
#[derive(Debug, Clone, PartialEq)]
pub struct Polygon {
    pub points: Vec<glam::Vec3>,
    pub smoothing_groups: u32,
    pub material_id: u16,
    pub hidden: bool,
    /// Map channel → one coordinate per point.
    pub uv: std::collections::BTreeMap<u32, Vec<glam::Vec3>>,
}

impl Polygon {
    pub fn new(points: Vec<glam::Vec3>) -> Self {
        Self { points, smoothing_groups: 0, material_id: 1, hidden: false, uv: Default::default() }
    }

    /// Same attributes as `src`, new points; each point's uv comes from the
    /// source point at the same position, else the source's first uv.
    pub fn like(src: &Polygon, points: Vec<glam::Vec3>) -> Self {
        let mut uv = std::collections::BTreeMap::new();
        for (ch, vals) in &src.uv {
            let row = points
                .iter()
                .map(|p| src.points.iter().position(|q| q == p).and_then(|k| vals.get(k)).copied().unwrap_or_else(|| vals.first().copied().unwrap_or_default()))
                .collect();
            uv.insert(*ch, row);
        }
        Self { points, smoothing_groups: src.smoothing_groups, material_id: src.material_id, hidden: src.hidden, uv }
    }
}

#[derive(Debug, Default, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Mesh {
    pub(crate) vertices: Vec<Vertex>,
    #[serde(with="half_edges_serde")]
    pub(crate) half_edges: std::sync::Arc<half_edge_storage::HalfEdgeData>,
    pub(crate) faces: Vec<Face>,
    /// Map channels by number (1 = the default texture channel). Channel 0,
    /// the vertex colour, lives on `Vertex.color`.
    #[serde(default)]
    pub(crate) maps: std::collections::BTreeMap<u32, MapChannel>,
    /// Editable Mesh: per half-edge, the edge is invisible (both halves set).
    /// Empty, or of another length than `half_edges` after an edit that did
    /// not carry it, means every edge is visible.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) invisible: Vec<bool>,
}

/// An immutable connectivity snapshot. It excludes vertices, faces, maps and
/// edge visibility. Keeping this handle alive forces edits to detach storage.
#[derive(Clone)]
pub struct HalfEdgeSnapshot(std::sync::Arc<half_edge_storage::HalfEdgeData>);
impl HalfEdgeSnapshot {
    pub fn matches(&self, mesh: &Mesh) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &mesh.half_edges)
            || self.0.as_slice() == mesh.half_edges()
    }
}

impl Mesh {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn half_edge_snapshot(&self) -> HalfEdgeSnapshot {
        HalfEdgeSnapshot(self.half_edges.clone())
    }

    pub fn half_edge_count(&self) -> usize {
        self.half_edges.len()
    }

    /// The half-edges of a face's loop, in order (one per corner).
    pub fn face_corners(&self, id: FaceId) -> Option<Vec<HalfEdgeId>> {
        let face = self.faces.get(id.0 as usize)?;
        let mut out = Vec::new();
        let start = face.he;
        let mut he_id = start;
        loop {
            out.push(he_id);
            he_id = self.half_edges.get(he_id.0 as usize)?.next;
            if he_id == start || out.len() > 1024 {
                break;
            }
        }
        Some(out)
    }

    pub fn map(&self, channel: u32) -> Option<&MapChannel> {
        self.maps.get(&channel)
    }

    pub fn map_mut(&mut self, channel: u32) -> Option<&mut MapChannel> {
        self.maps.get_mut(&channel)
    }

    pub fn maps(&self) -> &std::collections::BTreeMap<u32, MapChannel> {
        &self.maps
    }

    /// Installs a channel (its `corners` must match the half-edge count).
    /// Boundary half-edges take the texture vertex of the face corner at
    /// their origin, and unused texture vertices are dropped, so a channel
    /// never carries stray tvs.
    pub fn set_map(&mut self, channel: u32, mut map: MapChannel) {
        debug_assert_eq!(map.corners.len(), self.half_edges.len());
        self.fix_boundary_corners(&mut map);
        map.compact();
        self.maps.insert(channel, map);
    }

    pub(crate) fn fix_boundary_corners(&self, map: &mut MapChannel) {
        for (i, he) in self.half_edges.iter().enumerate() {
            if he.face.is_none() {
                let twin_next = self.half_edges[he.twin.0 as usize].next;
                if let Some(c) = map.corners.get(twin_next.0 as usize).copied() {
                    if i < map.corners.len() {
                        map.corners[i] = c;
                    }
                }
            }
        }
    }

    pub fn remove_map(&mut self, channel: u32) -> Option<MapChannel> {
        self.maps.remove(&channel)
    }

    /// The channel, created with every corner on one texture vertex at the
    /// origin when it does not exist yet (a fresh channel is all zero).
    pub fn ensure_map(&mut self, channel: u32) -> &mut MapChannel {
        let n = self.half_edges.len();
        self.maps.entry(channel).or_insert_with(|| MapChannel { verts: vec![glam::Vec3::ZERO], corners: vec![0; n] })
    }

    /// The texture coordinate at a face corner.
    /// The explicit normal at a corner (Edit Normals), if it has one.
    pub fn explicit_normal(&self, he: HalfEdgeId) -> Option<glam::Vec3> {
        let n = self.corner_uv(crate::NORMALS_CHANNEL, he)?;
        (n.length_squared() > 1e-12).then(|| n.normalize())
    }

    /// The texture map channels (every channel but the explicit-normals one).
    pub fn texture_maps(&self) -> impl Iterator<Item = (&u32, &MapChannel)> {
        self.maps.iter().filter(|(c, _)| **c != crate::NORMALS_CHANNEL)
    }

    pub fn corner_uv(&self, channel: u32, he: HalfEdgeId) -> Option<glam::Vec3> {
        let m = self.maps.get(&channel)?;
        m.verts.get(*m.corners.get(he.0 as usize)? as usize).copied()
    }

    /// Sets a corner's coordinate on its own texture vertex (a new one when
    /// the corner shared its tv with others).
    pub fn set_corner_uv(&mut self, channel: u32, he: HalfEdgeId, uv: glam::Vec3) {
        let m = self.ensure_map(channel);
        let i = he.0 as usize;
        if i >= m.corners.len() {
            return;
        }
        let tv = m.corners[i];
        let shared = m.corners.iter().filter(|c| **c == tv).count() > 1;
        if shared {
            m.verts.push(uv);
            m.corners[i] = (m.verts.len() - 1) as u32;
        } else {
            m.verts[tv as usize] = uv;
        }
    }

    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }
    pub fn edge_count(&self) -> usize {
        self.half_edges.len() / 2
    }
    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    pub fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }
    pub fn half_edges(&self) -> &[HalfEdge] {
        &self.half_edges
    }
    pub fn faces(&self) -> &[Face] {
        &self.faces
    }
    pub fn faces_mut(&mut self) -> &mut [Face] {
        &mut self.faces
    }
    pub fn vertices_mut(&mut self) -> &mut [Vertex] {
        &mut self.vertices
    }

    /// Unit normal of a face (Newell's method, so n-gons and slightly
    /// non-planar faces work).
    pub fn face_normal(&self, id: FaceId) -> glam::Vec3 {
        let Some(face) = self.faces.get(id.0 as usize) else { return glam::Vec3::Z };
        let term = |a: glam::Vec3, b: glam::Vec3| glam::Vec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y));
        let mut n = glam::Vec3::ZERO;
        let mut first = None;
        let mut previous = None;
        let mut current = face.he;
        let mut count = 0;
        loop {
            let Some(he) = self.half_edges.get(current.0 as usize) else { return glam::Vec3::Z };
            if let Some(p) = self.vertex_position(he.origin) {
                if let Some(prev) = previous { n += term(prev, p); }
                first.get_or_insert(p);
                previous = Some(p);
            }
            current = he.next;
            count += 1;
            if current == face.he { break; }
            if count > 1024 { return glam::Vec3::Z; }
        }
        if let (Some(last), Some(first)) = (previous, first) { n += term(last, first); }
        n.normalize_or_zero()
    }

    /// Every half-edge leaving `v`, by rotating around the vertex (O(valence);
    /// boundary half-edges included). A bow-tie vertex yields only the fan
    /// its stored half-edge belongs to.
    pub fn outgoing_half_edges(&self, v: VertexId) -> Vec<HalfEdgeId> {
        let Some(start) = self.vertices.get(v.0 as usize).and_then(|x| x.he) else { return Vec::new() };
        let mut out = vec![start];
        let mut cur = start;
        for _ in 0..self.half_edges.len() {
            let prev = self.half_edges[cur.0 as usize].prev;
            let next = self.half_edges[prev.0 as usize].twin;
            if next == start || self.half_edges[next.0 as usize].origin != v {
                break;
            }
            out.push(next);
            cur = next;
        }
        out
    }

    /// Faces around a vertex (both sides of every outgoing half-edge).
    pub fn faces_around_vertex(&self, v: VertexId) -> Vec<FaceId> {
        let mut out = Vec::new();
        for he_id in self.outgoing_half_edges(v) {
            let he = &self.half_edges[he_id.0 as usize];
            if let Some(f) = he.face {
                if !out.contains(&f) {
                    out.push(f);
                }
            }
            let twin = &self.half_edges[he.twin.0 as usize];
            if let Some(f) = twin.face {
                if !out.contains(&f) {
                    out.push(f);
                }
            }
        }
        out
    }

    /// Normal at a corner of `face`: the average of the normals of the faces
    /// around `v` that share a smoothing group with `face` (just the face's
    /// own normal when it has no groups).
    pub fn vertex_normal(&self, v: VertexId, face: FaceId) -> glam::Vec3 {
        let fn0 = self.face_normal(face);
        let groups = self.faces.get(face.0 as usize).map(|f| f.smoothing_groups).unwrap_or(0);
        if groups == 0 {
            return fn0;
        }
        let mut n = glam::Vec3::ZERO;
        for f in self.faces_around_vertex(v) {
            if self.faces[f.0 as usize].smoothing_groups & groups != 0 {
                n += self.face_normal(f);
            }
        }
        if n.length_squared() < 1e-12 { fn0 } else { n.normalize() }
    }

    /// Editable Poly > Auto Smooth: faces joined by an edge whose dihedral
    /// angle is at most `threshold_deg` share a smoothing group; adjacent
    /// smooth regions get different bits (greedy colouring of 32 bits).
    pub fn auto_smooth(&mut self, threshold_deg: f32) {
        let all: Vec<FaceId> = (0..self.faces.len() as u32).map(FaceId).collect();
        self.auto_smooth_faces(&all, threshold_deg);
    }

    /// Auto Smooth on a subset: only edges between two faces of the subset
    /// join regions; faces outside keep their groups.
    pub fn auto_smooth_faces(&mut self, faces: &[FaceId], threshold_deg: f32) {
        self.auto_smooth_faces_with(faces, threshold_deg, false);
    }

    /// Auto Smooth with Prevent Indirect Smoothing: regions that meet only
    /// at a vertex get different bits too, so no vertex normal mixes them.
    pub fn auto_smooth_faces_with(&mut self, faces: &[FaceId], threshold_deg: f32, prevent_indirect: bool) {
        let n = self.faces.len();
        if n == 0 || faces.is_empty() {
            return;
        }
        let subset: std::collections::HashSet<usize> = faces.iter().map(|f| f.0 as usize).filter(|f| *f < n).collect();
        let normals: Vec<glam::Vec3> = (0..n).map(|i| self.face_normal(FaceId(i as u32))).collect();
        let cos_t = threshold_deg.to_radians().cos();
        // Union-find over faces joined by smooth edges.
        let mut parent: Vec<usize> = (0..n).collect();
        fn find(p: &mut Vec<usize>, i: usize) -> usize {
            let mut r = i;
            while p[r] != r {
                r = p[r];
            }
            let mut c = i;
            while p[c] != r {
                let nx = p[c];
                p[c] = r;
                c = nx;
            }
            r
        }
        let mut adjacent: Vec<(usize, usize)> = Vec::new();
        for he in self.half_edges.iter() {
            let twin = &self.half_edges[he.twin.0 as usize];
            if let (Some(a), Some(b)) = (he.face, twin.face) {
                let (a, b) = (a.0 as usize, b.0 as usize);
                if a < b && subset.contains(&a) && subset.contains(&b) {
                    adjacent.push((a, b));
                    if normals[a].dot(normals[b]) >= cos_t - 1e-6 {
                        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                        if ra != rb {
                            parent[ra] = rb;
                        }
                    }
                }
            }
        }
        let roots: Vec<usize> = (0..n).map(|i| find(&mut parent, i)).collect();
        // Colour the regions: neighbouring regions take different bits.
        let mut region_bits: std::collections::HashMap<usize, u32> = std::collections::HashMap::new();
        let mut neighbours: std::collections::HashMap<usize, std::collections::HashSet<usize>> = std::collections::HashMap::new();
        if prevent_indirect {
            // Every two subset faces around a vertex count as neighbours.
            for v in 0..self.vertices.len() as u32 {
                let around: Vec<usize> = self.faces_around_vertex(VertexId(v)).into_iter().map(|f| f.0 as usize).filter(|f| subset.contains(f)).collect();
                for (i, a) in around.iter().enumerate() {
                    for b in &around[i + 1..] {
                        adjacent.push((*a, *b));
                    }
                }
            }
        }
        for (a, b) in &adjacent {
            let (ra, rb) = (roots[*a], roots[*b]);
            if ra != rb {
                neighbours.entry(ra).or_default().insert(rb);
                neighbours.entry(rb).or_default().insert(ra);
            }
        }
        let mut order: Vec<usize> = roots.clone();
        order.sort();
        order.dedup();
        for r in order {
            let used: u32 = neighbours.get(&r).map(|ns| ns.iter().filter_map(|n| region_bits.get(n)).fold(0, |acc, b| acc | b)).unwrap_or(0);
            let bit = (0..32).map(|i| 1u32 << i).find(|b| used & b == 0).unwrap_or(1);
            region_bits.insert(r, bit);
        }
        for (i, f) in self.faces.iter_mut().enumerate() {
            if subset.contains(&i) {
                f.smoothing_groups = region_bits[&roots[i]];
            }
        }
    }

    /// A face as positions plus its attributes, for rebuilding.
    pub fn polygon(&self, id: FaceId) -> Option<Polygon> {
        let f = self.faces.get(id.0 as usize)?;
        let corners = self.face_corners(id)?;
        let mut uv = std::collections::BTreeMap::new();
        for (ch, m) in &self.maps {
            uv.insert(*ch, corners.iter().map(|he| m.verts.get(m.corners[he.0 as usize] as usize).copied().unwrap_or_default()).collect());
        }
        Some(Polygon { points: self.face_positions(id)?, smoothing_groups: f.smoothing_groups, material_id: f.material_id, hidden: f.hidden, uv })
    }

    /// Vertex attributes keyed by position, for carrying them across a rebuild.
    pub fn vertex_attrs_by_position(&self) -> std::collections::HashMap<(u32, u32, u32), ([f32; 3], [f32; 3], f32, bool)> {
        self.vertices
            .iter()
            .map(|v| ((v.position.x.to_bits(), v.position.y.to_bits(), v.position.z.to_bits()), (v.color, v.illum, v.alpha, v.hidden)))
            .collect()
    }

    pub fn vertex_position(&self, id: VertexId) -> Option<glam::Vec3> {
        self.vertices.get(id.0 as usize).map(|v| v.position)
    }

    pub fn edge_vertices(&self, id: HalfEdgeId) -> Option<(VertexId, VertexId)> {
        let he = self.half_edges.get(id.0 as usize)?;
        let twin = self.half_edges.get(he.twin.0 as usize)?;
        Some((he.origin, twin.origin))
    }

    pub fn canonical_edge_id(&self, id: HalfEdgeId) -> Option<HalfEdgeId> {
        let he = self.half_edges.get(id.0 as usize)?;
        Some(if id.0 <= he.twin.0 { id } else { he.twin })
    }

    /// The 1-based edge number the reference shows ("Edge 12"): edges counted
    /// in the order the polygons first use them.
    pub fn edge_number(&self, id: HalfEdgeId) -> Option<u32> {
        let c = self.canonical_edge_id(id)?;
        let before = (0..c.0).filter(|i| self.canonical_edge_id(HalfEdgeId(*i)) == Some(HalfEdgeId(*i))).count();
        Some(before as u32 + 1)
    }

    pub fn face_vertex_ids(&self, id: FaceId) -> Option<Vec<VertexId>> {
        let face = self.faces.get(id.0 as usize)?;
        let mut ids = Vec::new();
        let start = face.he;
        let mut he_id = start;
        loop {
            let he = self.half_edges.get(he_id.0 as usize)?;
            ids.push(he.origin);
            he_id = he.next;
            if he_id == start {
                break;
            }
            if ids.len() > 1024 {
                return None;
            }
        }
        Some(ids)
    }

    pub fn face_positions(&self, id: FaceId) -> Option<Vec<glam::Vec3>> {
        self.face_vertex_ids(id).map(|ids| {
            ids.into_iter()
                .filter_map(|id| self.vertex_position(id))
                .collect()
        })
    }

    /// Returns the axis-aligned bounding box of the mesh as (min, max).
    /// For an empty mesh, returns (ZERO, ZERO).
    pub fn aabb(&self) -> (glam::Vec3, glam::Vec3) {
        if self.vertices.is_empty() {
            return (glam::Vec3::ZERO, glam::Vec3::ZERO);
        }
        let mut min = self.vertices[0].position;
        let mut max = min;
        for v in &self.vertices[1..] {
            min = min.min(v.position);
            max = max.max(v.position);
        }
        (min, max)
    }

    /// The edge is drawn (Editable Mesh edges can be invisible).
    pub fn edge_visible(&self, he: HalfEdgeId) -> bool {
        self.invisible.len() != self.half_edges.len() || !self.invisible.get(he.0 as usize).copied().unwrap_or(false)
    }

    /// Sets an edge's visibility (both half-edges).
    pub fn set_edge_visible(&mut self, he: HalfEdgeId, visible: bool) {
        let n = self.half_edges.len();
        let Some(twin) = self.half_edges.get(he.0 as usize).map(|h| h.twin) else { return };
        if self.invisible.len() != n {
            if visible {
                return;
            }
            self.invisible = vec![false; n];
        }
        self.invisible[he.0 as usize] = !visible;
        self.invisible[twin.0 as usize] = !visible;
    }

    /// Some edge is invisible (an Editable Mesh with hidden polygon diagonals).
    pub fn has_invisible_edges(&self) -> bool {
        self.invisible.len() == self.half_edges.len() && self.invisible.iter().any(|i| *i)
    }

    /// The invisible edges as position pairs, each once.
    pub fn invisible_edges(&self) -> Vec<(glam::Vec3, glam::Vec3)> {
        if !self.has_invisible_edges() {
            return Vec::new();
        }
        let mut edges = Vec::new();
        for (i, he) in self.half_edges.iter().enumerate() {
            if (i as u32) < he.twin.0 && self.invisible[i] {
                let twin = &self.half_edges[he.twin.0 as usize];
                edges.push((self.vertices[he.origin.0 as usize].position, self.vertices[twin.origin.0 as usize].position));
            }
        }
        edges
    }

    /// Returns each unique visible edge as a pair of vertex positions.
    /// Each undirected edge is emitted exactly once. Useful for wireframe overlays.
    pub fn unique_edges(&self) -> Vec<(glam::Vec3, glam::Vec3)> {
        let mut edges = Vec::new();
        let flags = self.invisible.len() == self.half_edges.len();
        for (i, he) in self.half_edges.iter().enumerate() {
            // Emit only the half-edge with the lower id of the twin pair.
            if (i as u32) < he.twin.0 && !(flags && self.invisible[i]) {
                let a = self.vertices[he.origin.0 as usize].position;
                let twin = &self.half_edges[he.twin.0 as usize];
                let b = self.vertices[twin.origin.0 as usize].position;
                edges.push((a, b));
            }
        }
        edges
    }

    /// Verifies half-edge invariants. Returns Err with the first violation found.
    pub fn check_invariants(&self) -> Result<(), String> {
        self.half_edges.check()?;
        for (ch, m) in &self.maps {
            if m.corners.len() != self.half_edges.len() {
                return Err(format!("map channel {ch}: {} corners for {} half-edges", m.corners.len(), self.half_edges.len()));
            }
            if let Some(bad) = m.corners.iter().find(|c| **c as usize >= m.verts.len()) {
                return Err(format!("map channel {ch}: corner points at texture vertex {bad} of {}", m.verts.len()));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn streamed_face_normals_match_newell_positions() {
        let meshes = [crate::sphere_segmented(1.25, 24, 0.0), crate::box_primitive(Vec3::ONE)];
        for mesh in meshes {
            for i in 0..mesh.face_count() {
                let face = FaceId(i as u32);
                let pts = mesh.face_positions(face).unwrap();
                let mut n = Vec3::ZERO;
                for i in 0..pts.len() {
                    let a = pts[i];
                    let b = pts[(i + 1) % pts.len()];
                    n += Vec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y));
                }
                assert_eq!(mesh.face_normal(face), n.normalize_or_zero());
            }
        }
    }

    #[test]
    fn empty_mesh_has_no_elements() {
        let m = Mesh::new();
        assert_eq!(m.vertex_count(), 0);
        assert_eq!(m.edge_count(), 0);
        assert_eq!(m.face_count(), 0);
    }

    #[test]
    fn empty_mesh_invariants_hold() {
        assert!(Mesh::new().check_invariants().is_ok());
    }

    #[test]
    fn auto_smooth_box_gives_six_groups_sphere_one() {
        let mut b = crate::box_primitive(Vec3::splat(0.5));
        b.auto_smooth(45.0);
        let groups: Vec<u32> = b.faces().iter().map(|f| f.smoothing_groups).collect();
        assert!(groups.iter().all(|g| *g != 0));
        // Adjacent box faces never share a group bit.
        for he in b.half_edges() {
            if let (Some(f), Some(t)) = (he.face, b.half_edges()[he.twin.0 as usize].face) {
                assert_eq!(b.faces()[f.0 as usize].smoothing_groups & b.faces()[t.0 as usize].smoothing_groups, 0);
            }
        }
        let mut s = crate::sphere_segmented(1.0, 16, 0.0);
        s.auto_smooth(45.0);
        let first = s.faces()[0].smoothing_groups;
        assert!(first != 0 && s.faces().iter().all(|f| f.smoothing_groups == first));
    }

    #[test]
    fn vertex_normal_follows_smoothing_groups() {
        let mut b = crate::box_primitive(Vec3::splat(0.5));
        b.auto_smooth(45.0);
        // Faceted: the corner normal of each face equals that face's normal.
        let f = FaceId(0);
        let v = b.face_vertex_ids(f).unwrap()[0];
        assert!(b.vertex_normal(v, f).abs_diff_eq(b.face_normal(f), 1e-5));
        // One group everywhere: the corner normal averages the faces around the vertex.
        for face in b.faces_mut() {
            face.smoothing_groups = 1;
        }
        let n = b.vertex_normal(v, f);
        assert!((n.length() - 1.0).abs() < 1e-5);
        assert!(!n.abs_diff_eq(b.face_normal(f), 1e-3), "smoothed corner differs from the flat normal");
    }

    #[test]
    fn extruded_cap_inherits_sg_and_material() {
        let mut b = crate::box_primitive(Vec3::splat(0.5));
        b.faces_mut()[0].smoothing_groups = 1 << 4;
        b.faces_mut()[0].material_id = 3;
        let cap = crate::edit_ops::extrude_face(&mut b, FaceId(0), 0.5).unwrap();
        assert_eq!(b.faces()[cap.0 as usize].smoothing_groups, 1 << 4);
        assert_eq!(b.faces()[cap.0 as usize].material_id, 3);
    }
}

mod half_edges_serde {
    use super::HalfEdge;
    use serde::{Serialize,Deserialize};
    pub(super) fn serialize<S:serde::Serializer>(value:&std::sync::Arc<super::half_edge_storage::HalfEdgeData>,serializer:S)->Result<S::Ok,S::Error>{value.as_ref().as_slice().serialize(serializer)}
    pub(super) fn deserialize<'de,D:serde::Deserializer<'de>>(deserializer:D)->Result<std::sync::Arc<super::half_edge_storage::HalfEdgeData>,D::Error>{Vec::<HalfEdge>::deserialize(deserializer).map(|edges|std::sync::Arc::new(super::half_edge_storage::HalfEdgeData::new(edges)))}
}

#[cfg(test)]
mod half_edge_cow_tests {
    use super::*;
    #[test]
    fn topology_snapshot_stays_immutable_and_checks_independent_equal_storage() {
        let mut mesh = crate::box_primitive(glam::Vec3::ONE);
        let snapshot = mesh.half_edge_snapshot();
        let loaded: Mesh = serde_json::from_value(serde_json::to_value(&mesh).unwrap()).unwrap();
        assert!(snapshot.matches(&loaded));
        mesh.vertices[0].position += glam::Vec3::X;
        mesh.faces[0].hidden = true;
        mesh.set_edge_visible(HalfEdgeId(0), false);
        assert!(snapshot.matches(&mesh), "non-connectivity fields need their own guards");
        let count = mesh.vertex_count() as u32;
        let edges = std::sync::Arc::make_mut(&mut mesh.half_edges).edges_mut();
        edges[0].origin.0 = (edges[0].origin.0 + 1) % count;
        assert!(!snapshot.matches(&mesh));
        assert!(snapshot.matches(&loaded), "a retained proof cannot mutate with its source");
    }

    #[test]
    fn clone_edit_and_undo_snapshots_detach_and_keep_json_arrays(){
        let mut source=crate::box_primitive(glam::Vec3::ONE);
        crate::uvmap::auto_map(&mut source,crate::uvmap::AutoMap::Spherical,1);
        source.set_edge_visible(HalfEdgeId(0),false);
        let undo=source.clone();let before=serde_json::to_value(&source).unwrap();
        assert!(before["half_edges"].is_array());
        let mut edited=source.clone();
        assert!(std::sync::Arc::ptr_eq(&source.half_edges,&edited.half_edges));
        assert_ne!(source.vertices.as_ptr(),edited.vertices.as_ptr());
        edited.vertices[0].position+=glam::Vec3::X;edited.faces[0].hidden=true;
        edited.ensure_map(1).verts[0]=glam::Vec3::Y;edited.set_edge_visible(HalfEdgeId(0),true);
        assert_eq!(source,undo);assert_eq!(serde_json::to_value(&source).unwrap(),before);
        let hes=std::sync::Arc::make_mut(&mut edited.half_edges).edges_mut();
        for he in hes {he.origin.0=match he.origin.0{0=>1,1=>0,v=>v};}
        edited.vertices.swap(0,1);edited.check_invariants().unwrap();
        assert!(!std::sync::Arc::ptr_eq(&source.half_edges,&edited.half_edges));
        assert_eq!(serde_json::to_value(&source).unwrap(),before);
        edited=undo.clone();assert_eq!(serde_json::to_value(&edited).unwrap(),before);
        let loaded:Mesh=serde_json::from_value(before.clone()).unwrap();assert_eq!(serde_json::to_value(&loaded).unwrap(),before);loaded.check_invariants().unwrap();
    }
}

mod half_edge_storage {
    use super::{HalfEdge,HalfEdgeId};
    #[derive(Default)]
    pub(crate) struct HalfEdgeData { edges:Vec<HalfEdge>, valid:std::sync::OnceLock<()> }
    impl HalfEdgeData {
        pub(crate) fn new(edges:Vec<HalfEdge>)->Self{Self{edges,valid:std::sync::OnceLock::new()}}
        pub(crate) fn as_slice(&self)->&[HalfEdge]{&self.edges}
        pub(crate) fn edges_mut(&mut self)->&mut Vec<HalfEdge>{let _=self.valid.take();&mut self.edges}
        pub(crate) fn check(&self)->Result<(),String>{
            if self.valid.get().is_none(){self.check_uncached()?;let _=self.valid.set(());}
            Ok(())
        }
        #[cfg(test)]
        pub(super) fn is_cached(&self)->bool{self.valid.get().is_some()}
        fn check_uncached(&self)->Result<(),String>{
        for (i, he) in self.edges.iter().enumerate() {
            let id = HalfEdgeId(i as u32);

            // Twin of twin = self
            let twin = &self.edges[he.twin.0 as usize];
            if twin.twin != id {
                return Err(format!("HE {}: twin's twin is not self", i));
            }

            // Next/prev consistency
            let next = &self.edges[he.next.0 as usize];
            if next.prev != id {
                return Err(format!("HE {}: next's prev is not self", i));
            }
            let prev = &self.edges[he.prev.0 as usize];
            if prev.next != id {
                return Err(format!("HE {}: prev's next is not self", i));
            }

            // Face consistency: all half-edges around a face share the face
            if he.face != next.face {
                return Err(format!("HE {}: face differs from next's face", i));
            }
        }
            Ok(())
        }
    }
    impl std::ops::Deref for HalfEdgeData{type Target=[HalfEdge];fn deref(&self)->&[HalfEdge]{&self.edges}}
    impl Clone for HalfEdgeData{fn clone(&self)->Self{Self::new(self.edges.clone())}}
    impl PartialEq for HalfEdgeData{fn eq(&self,other:&Self)->bool{self.edges==other.edges}}
    impl std::fmt::Debug for HalfEdgeData{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{std::fmt::Debug::fmt(&self.edges,f)}}
}

#[cfg(test)]
mod validation_cache_tests {
    use super::*;
    #[test]
    fn successful_proof_is_shared_but_not_part_of_value_debug_or_json(){
        let mesh=crate::box_primitive(glam::Vec3::ONE);let original=serde_json::to_value(&mesh).unwrap();
        let loaded:Mesh=serde_json::from_value(original.clone()).unwrap();assert!(!mesh.half_edges.is_cached());assert!(!loaded.half_edges.is_cached());
        let clone=mesh.clone();mesh.check_invariants().unwrap();assert!(clone.half_edges.is_cached());
        assert_eq!(mesh,loaded);assert_eq!(format!("{mesh:?}"),format!("{loaded:?}"));assert_eq!(serde_json::to_value(&mesh).unwrap(),original);
        let snapshot=mesh.half_edge_snapshot();assert!(snapshot.matches(&loaded));
        let mut edited=mesh.clone();let old=edited.half_edges[0].next;
        std::sync::Arc::make_mut(&mut edited.half_edges).edges_mut()[0].next=HalfEdgeId(0);
        assert!(!edited.half_edges.is_cached());assert!(edited.check_invariants().is_err());assert!(mesh.half_edges.is_cached());assert!(!snapshot.matches(&edited));
        std::sync::Arc::make_mut(&mut edited.half_edges).edges_mut()[0].next=old;edited.check_invariants().unwrap();assert!(snapshot.matches(&edited));
        edited=clone;assert_eq!(serde_json::to_value(&edited).unwrap(),original);
    }
    #[test]
    fn unique_storage_mutation_clears_success_and_panics_are_not_cached(){
        let mut mesh=crate::box_primitive(glam::Vec3::ONE);mesh.check_invariants().unwrap();assert!(mesh.half_edges.is_cached());
        let old=mesh.half_edges[0].next;std::sync::Arc::make_mut(&mut mesh.half_edges).edges_mut()[0].next=HalfEdgeId(0);
        assert!(!mesh.half_edges.is_cached());assert_eq!(mesh.check_invariants().unwrap_err(),"HE 0: next's prev is not self");
        std::sync::Arc::make_mut(&mut mesh.half_edges).edges_mut()[0].next=old;mesh.check_invariants().unwrap();
        std::sync::Arc::make_mut(&mut mesh.half_edges).edges_mut()[0].twin=HalfEdgeId(u32::MAX);
        for _ in 0..2{assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(||mesh.check_invariants())).is_err());assert!(!mesh.half_edges.is_cached());}
    }
    #[test]
    fn every_map_is_checked_after_cached_halfedge_success(){
        let mut mesh=crate::box_primitive(glam::Vec3::ONE);mesh.check_invariants().unwrap();assert!(mesh.half_edges.is_cached());
        mesh.ensure_map(1).corners[0]=999;assert!(mesh.check_invariants().unwrap_err().starts_with("map channel 1: corner"));assert!(mesh.half_edges.is_cached());
        mesh.ensure_map(1).corners[0]=0;mesh.check_invariants().unwrap();
        mesh.ensure_map(7).corners.pop();assert!(mesh.check_invariants().unwrap_err().starts_with("map channel 7:"));assert!(mesh.half_edges.is_cached());
        mesh.remove_map(7);mesh.check_invariants().unwrap();
    }
}
