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
    pub(crate) half_edges: Vec<HalfEdge>,
    pub(crate) faces: Vec<Face>,
    /// Map channels by number (1 = the default texture channel). Channel 0,
    /// the vertex colour, lives on `Vertex.color`.
    #[serde(default)]
    pub(crate) maps: std::collections::BTreeMap<u32, MapChannel>,
}

impl Mesh {
    pub fn new() -> Self {
        Self::default()
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
        let Some(pts) = self.face_positions(id) else { return glam::Vec3::Z };
        let mut n = glam::Vec3::ZERO;
        for i in 0..pts.len() {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            n += glam::Vec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y));
        }
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
        for he in &self.half_edges {
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

    /// Returns each unique edge as a pair of vertex positions.
    /// Each undirected edge is emitted exactly once. Useful for wireframe overlays.
    pub fn unique_edges(&self) -> Vec<(glam::Vec3, glam::Vec3)> {
        let mut edges = Vec::new();
        for (i, he) in self.half_edges.iter().enumerate() {
            // Emit only the half-edge with the lower id of the twin pair.
            if (i as u32) < he.twin.0 {
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
        for (i, he) in self.half_edges.iter().enumerate() {
            let id = HalfEdgeId(i as u32);

            // Twin of twin = self
            let twin = &self.half_edges[he.twin.0 as usize];
            if twin.twin != id {
                return Err(format!("HE {}: twin's twin is not self", i));
            }

            // Next/prev consistency
            let next = &self.half_edges[he.next.0 as usize];
            if next.prev != id {
                return Err(format!("HE {}: next's prev is not self", i));
            }
            let prev = &self.half_edges[he.prev.0 as usize];
            if prev.next != id {
                return Err(format!("HE {}: prev's next is not self", i));
            }

            // Face consistency: all half-edges around a face share the face
            if he.face != next.face {
                return Err(format!("HE {}: face differs from next's face", i));
            }
        }
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
