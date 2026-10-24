use glam::Vec3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub struct VertexId(pub u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub struct HalfEdgeId(pub u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub struct FaceId(pub u32);

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Vertex {
    pub position: Vec3,
    /// One outgoing half-edge. None only for isolated vertices.
    pub he: Option<HalfEdgeId>,
    /// Vertex colour (Editable Poly > Vertex Properties), linear RGB.
    #[serde(default = "white")]
    pub color: [f32; 3],
    /// Vertex illumination.
    #[serde(default = "white")]
    pub illum: [f32; 3],
    /// Vertex alpha.
    #[serde(default = "one")]
    pub alpha: f32,
    /// Hidden at the Vertex sub-object level.
    #[serde(default)]
    pub hidden: bool,
}

impl Vertex {
    pub fn new(position: Vec3, he: Option<HalfEdgeId>) -> Self {
        Self { position, he, color: [1.0; 3], illum: [1.0; 3], alpha: 1.0, hidden: false }
    }
}

fn white() -> [f32; 3] {
    [1.0; 3]
}

fn one() -> f32 {
    1.0
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HalfEdge {
    pub origin: VertexId,
    pub twin: HalfEdgeId,
    pub next: HalfEdgeId,
    pub prev: HalfEdgeId,
    /// None for boundary half-edges (no incident face).
    pub face: Option<FaceId>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Face {
    /// One half-edge of this face's boundary loop.
    pub he: HalfEdgeId,
    /// Smoothing group bits (Polygon: Smoothing Groups rollout); 0 = none.
    #[serde(default)]
    pub smoothing_groups: u32,
    /// Material ID (Polygon: Material IDs rollout), 1-based.
    #[serde(default = "mat_one")]
    pub material_id: u16,
    /// Hidden at the Polygon sub-object level.
    #[serde(default)]
    pub hidden: bool,
}

impl Face {
    pub fn new(he: HalfEdgeId) -> Self {
        Self { he, smoothing_groups: 0, material_id: 1, hidden: false }
    }

    /// A new face derived from `src` (extrude, connect, chamfer): same
    /// smoothing groups and material ID.
    pub fn derived(he: HalfEdgeId, src: &Face) -> Self {
        Self { he, smoothing_groups: src.smoothing_groups, material_id: src.material_id, hidden: false }
    }
}

fn mat_one() -> u16 {
    1
}

/// A map channel (channels 1–99): texture vertices and, per half-edge
/// (a face corner), the index of the texture vertex used there. Boundary
/// half-edges carry an entry too (unused), so `corners.len()` always equals
/// the mesh's half-edge count.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MapChannel {
    pub verts: Vec<Vec3>,
    pub corners: Vec<u32>,
}

impl MapChannel {
    /// Drops texture vertices no corner uses, renumbering the rest.
    pub fn compact(&mut self) {
        let mut used = vec![false; self.verts.len()];
        for c in &self.corners {
            if let Some(u) = used.get_mut(*c as usize) {
                *u = true;
            }
        }
        if used.iter().all(|u| *u) && !self.verts.is_empty() {
            return;
        }
        let mut remap = vec![0u32; self.verts.len()];
        let mut verts = Vec::with_capacity(self.verts.len());
        for (i, u) in used.iter().enumerate() {
            if *u {
                remap[i] = verts.len() as u32;
                verts.push(self.verts[i]);
            }
        }
        if verts.is_empty() {
            verts.push(Vec3::ZERO);
        }
        for c in &mut self.corners {
            *c = remap.get(*c as usize).copied().unwrap_or(0);
        }
        self.verts = verts;
    }
}
