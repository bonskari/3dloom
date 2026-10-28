use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use loom_mesh::Mesh;
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    Buffer, BufferUsages, Device,
};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GpuVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    /// Object colour (Name and Color rollout), linear RGB + alpha.
    pub color: [f32; 4],
    /// Blinn highlight: specular colour × level (rgb) and the exponent (a); 0 = none.
    pub spec: [f32; 4],
    /// x = Self-Illumination 0–1, y = map Amount 0–1 (-1: composed geometry alpha), z / w = decal
    /// (Tile off) on U / V: outside 0–1 the map leaves the colour.
    pub extra: [f32; 4],
}

/// What the fragment shader samples with map channel 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextureSel {
    #[default]
    None,
    Checker,
    /// Index into the bitmaps uploaded with the meshes.
    Bitmap(u32),
}

/// How one material ID's faces draw (a Multi/Sub-Object sub-material).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceStyle {
    pub color: [f32; 3],
    pub alpha: f32,
    pub specular: [f32; 4],
    pub self_illum: f32,
    pub texture: TextureSel,
    /// The map's Offset / Tiling / Angle as a 2×3 UV matrix (see `GpuVertex::uv`).
    pub uv: [f32; 6],
    pub decal: [bool; 2],
    /// The map's Amount, 0–1.
    pub texture_amount: f32,
    /// The composed bitmap alpha controls geometry opacity, rather than RGB decal coverage.
    pub texture_alpha: bool,
    /// Faceted: flat normals whatever the smoothing groups.
    pub faceted: bool,
    /// 2-Sided: never culled.
    pub two_sided: bool,
    /// Wire: the faces draw as their edges, in the material colour.
    pub wire: bool,
}

/// Per face material ID styles; IDs without one draw `missing`.
#[derive(Debug, Clone, PartialEq)]
pub struct SubStyles {
    pub by_id: std::collections::BTreeMap<u16, SurfaceStyle>,
    pub missing: SurfaceStyle,
}

pub const UV_IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];

/// One draw call's slice of a mesh's index buffer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawRange {
    pub start: u32,
    pub count: u32,
    pub texture: TextureSel,
    pub transparent: bool,
    pub cull: bool,
}

/// How one mesh is drawn (Display panel properties).
#[derive(Debug, Clone, PartialEq)]
pub struct MeshStyle {
    pub color: [f32; 3],
    /// 1.0 = opaque; See-Through draws at 0.35.
    pub alpha: f32,
    pub backface_cull: bool,
    /// Vertex Channel Display: tint by the vertex colours.
    pub vertex_colors: bool,
    /// Show Map in Viewport.
    pub texture: TextureSel,
    /// Blinn highlight of the object's material (see `GpuVertex::spec`).
    pub specular: [f32; 4],
    /// Edges and wireframe: the object (wire) colour even when a material colours the faces.
    pub wire_color: Option<[f32; 3]>,
    /// Edges Only off: the triangulation diagonals show as dashed edges.
    pub diagonals: bool,
    /// The faces show their colour unlit (vertex colours without Shaded).
    pub unlit: bool,
    /// The material's Self-Illumination, 0–1.
    pub self_illum: f32,
    /// The shown map's UV transform, decal axes and Amount (see `SurfaceStyle`).
    pub uv: [f32; 6],
    pub decal: [bool; 2],
    pub texture_amount: f32,
    /// The composed bitmap alpha controls geometry opacity, rather than RGB decal coverage.
    pub texture_alpha: bool,
    pub faceted: bool,
    pub two_sided: bool,
    pub wire: bool,
    /// A Multi/Sub-Object material: each face draws with its material ID's style.
    pub sub: Option<std::sync::Arc<SubStyles>>,
}

impl Default for MeshStyle {
    fn default() -> Self {
        Self {
            color: [0.7, 0.7, 0.7],
            alpha: 1.0,
            backface_cull: false,
            vertex_colors: false,
            texture: TextureSel::None,
            specular: [0.0; 4],
            wire_color: None,
            self_illum: 0.0,
            uv: UV_IDENTITY,
            decal: [false; 2],
            texture_amount: 1.0,
            texture_alpha: false,
            faceted: false,
            two_sided: false,
            wire: false,
            sub: None,
            diagonals: false,
            unlit: false,
        }
    }
}

impl MeshStyle {
    /// The style of the whole mesh as one surface.
    pub fn surface(&self) -> SurfaceStyle {
        SurfaceStyle {
            color: self.color,
            alpha: self.alpha,
            specular: self.specular,
            self_illum: self.self_illum,
            texture: self.texture,
            uv: self.uv,
            decal: self.decal,
            texture_amount: self.texture_amount,
            texture_alpha: self.texture_alpha,
            faceted: self.faceted,
            two_sided: self.two_sided,
            wire: self.wire,
        }
    }

    /// The style a face with material ID `id` (0-based) draws with.
    pub fn for_face(&self, id: u16) -> SurfaceStyle {
        match &self.sub {
            Some(sub) => sub.by_id.get(&id).copied().unwrap_or(sub.missing),
            None => self.surface(),
        }
    }
}

/// A decoded RGBA8 image for the viewport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Bitmap {
    /// The checker map: an 8 × 8 black / white grid (256 px, repeat-sampled).
    pub fn checker() -> Self {
        let n = 256u32;
        let mut rgba = Vec::with_capacity((n * n * 4) as usize);
        for y in 0..n {
            for x in 0..n {
                let on = ((x / 32) + (y / 32)) % 2 == 0;
                let v = if on { 235 } else { 25 };
                rgba.extend_from_slice(&[v, v, v, 255]);
            }
        }
        Self { width: n, height: n, rgba }
    }
}

pub struct MeshBuffer {
    pub faces_ready: bool,
    pub vertex_buffer: Buffer,
    pub index_buffer: Buffer,
    pub index_count: u32,
    /// Every unique edge, using the object wire colour. Selection reuses its
    /// positions through the constant-white line pipeline.
    pub edge_vertex_buffer: Buffer,
    pub edge_vertex_count: u32,
    /// Cached AABB so picking doesn't have to recompute it every frame.
    pub aabb_min: Vec3,
    pub aabb_max: Vec3,
    pub style: MeshStyle,
    /// Draw calls: faces grouped by texture, transparency and culling.
    pub ranges: Vec<DrawRange>,
    /// Faces of a Wire material: their edges, always drawn in shaded views.
    pub wire_vertex_buffer: Buffer,
    pub wire_vertex_count: u32,
    edge_staging: Vec<crate::grid::LineVertex>,
}

impl MeshBuffer {
    pub fn from_mesh(device:&Device,mesh:&Mesh,style:MeshStyle)->Self{Self::from_mesh_with_faces(device,mesh,style,true)}
    pub fn update_from_mesh(&mut self,device:&Device,queue:&wgpu::Queue,mesh:&Mesh,style:MeshStyle){self.update_from_mesh_with_faces(device,queue,mesh,style,true);}
    pub fn from_mesh_with_faces(device: &Device, mesh: &Mesh, style: MeshStyle, faces: bool) -> Self {
        let _scope = crate::profile::Scope::new("render.upload");
        let (verts, indices, ranges) = if faces {triangulate_styled(mesh, &style)} else {(Vec::new(),Vec::new(),Vec::new())};
        let wire = wire_lines(mesh, &style);
        let wire_vertex_count = wire.len() as u32;
        let wire_vertex_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("loom_mesh_wire_buffer"),
            // wgpu rejects an empty vertex buffer: one unused vertex.
            contents: bytemuck::cast_slice(if wire.is_empty() { &[crate::grid::LineVertex { position: [0.0; 3], color: [0.0; 3] }][..] } else { &wire[..] }),
            usage: mesh_usage(BufferUsages::VERTEX),
        });
        let vertex_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("loom_mesh_vertex_buffer"),
            contents: bytemuck::cast_slice(&verts),
            usage: mesh_usage(BufferUsages::VERTEX),
        });
        let index_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("loom_mesh_index_buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: mesh_usage(BufferUsages::INDEX),
        });

        let mut edge_staging = Vec::new();
        stage_edges(mesh, &style, &mut edge_staging);
        let edge_vertex_count = edge_staging.len() as u32;
        let edge_vertex_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("loom_mesh_edge_buffer"),
            contents: bytemuck::cast_slice(&edge_staging),
            usage: mesh_usage(BufferUsages::VERTEX),
        });

        let (aabb_min, aabb_max) = mesh.aabb();

        Self {
            faces_ready: faces,
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
            edge_vertex_buffer,
            edge_vertex_count,
            aabb_min,
            aabb_max,
            style,
            ranges,
            wire_vertex_buffer,
            wire_vertex_count,
            edge_staging,
        }
    }

    /// Updates every geometry stream while keeping GPU allocations that fit.
    pub fn update_from_mesh_with_faces(&mut self, device: &Device, queue: &wgpu::Queue, mesh: &Mesh, style: MeshStyle, faces: bool) {
        let _scope = crate::profile::Scope::new("render.upload");
        let (verts, indices, ranges) = if faces {triangulate_styled(mesh, &style)} else {(Vec::new(),Vec::new(),Vec::new())};
        let wire = { let _scope = crate::profile::Scope::new("render.upload.wire"); wire_lines(mesh, &style) };
        { let _scope = crate::profile::Scope::new("render.upload.edges"); stage_edges(mesh, &style, &mut self.edge_staging); }
        let queue_scope = crate::profile::Scope::new("render.upload.queue");
        update_buffer(device, queue, &mut self.vertex_buffer, bytemuck::cast_slice(&verts), BufferUsages::VERTEX, "loom_mesh_vertex_buffer");
        update_buffer(device, queue, &mut self.index_buffer, bytemuck::cast_slice(&indices), BufferUsages::INDEX, "loom_mesh_index_buffer");
        update_buffer(device, queue, &mut self.edge_vertex_buffer, bytemuck::cast_slice(&self.edge_staging), BufferUsages::VERTEX, "loom_mesh_edge_buffer");
        update_buffer(device, queue, &mut self.wire_vertex_buffer, bytemuck::cast_slice(&wire), BufferUsages::VERTEX, "loom_mesh_wire_buffer");
        drop(queue_scope);
        self.faces_ready = faces;
        self.index_count = indices.len() as u32;
        self.edge_vertex_count = self.edge_staging.len() as u32;
        self.wire_vertex_count = wire.len() as u32;
        { let _scope = crate::profile::Scope::new("render.upload.bounds"); (self.aabb_min, self.aabb_max) = mesh.aabb(); }
        self.style = style;
        self.ranges = ranges;
    }

}

/// Dashes per diagonal when Edges Only is off.
pub const DIAGONAL_DASHES: usize = 6;

/// The diagonals the fan triangulation adds inside each visible polygon
/// (corner 0 to corners 2 .. n-2), as `triangulate_with_face_normals` cuts it;
/// an Editable Mesh shows its invisible edges instead.
pub fn fan_diagonals(mesh: &Mesh) -> Vec<(Vec3, Vec3)> {
    if mesh.has_invisible_edges() {
        return mesh.invisible_edges();
    }
    let mut out = Vec::new();
    for (i, face) in mesh.faces().iter().enumerate() {
        if face.hidden {
            continue;
        }
        let Some(ids) = mesh.face_vertex_ids(loom_mesh::FaceId(i as u32)) else { continue };
        let pts: Vec<Vec3> = ids.iter().filter_map(|v| mesh.vertex_position(*v)).collect();
        for p in pts.iter().take(pts.len().saturating_sub(1)).skip(2) {
            out.push((pts[0], *p));
        }
    }
    out
}

/// Each segment cut into `dashes` dashes with gaps of the same length.
pub fn dashed(segments: &[(Vec3, Vec3)], dashes: usize) -> Vec<(Vec3, Vec3)> {
    let parts = (dashes * 2).saturating_sub(1).max(1) as f32;
    let mut out = Vec::with_capacity(segments.len() * dashes);
    for (a, b) in segments {
        for k in 0..dashes {
            let t0 = (2 * k) as f32 / parts;
            let t1 = (2 * k + 1) as f32 / parts;
            out.push((a.lerp(*b, t0), a.lerp(*b, t1)));
        }
    }
    out
}

/// World line segments with their own colours (helper line art: tapes,
/// protractors, grid objects, point tripods), depth-tested like edges.
pub struct LineBuffer {
    pub buffer: Buffer,
    pub count: u32,
}

impl LineBuffer {
    /// None when there are no lines.
    pub fn new(device: &Device, lines: &[(Vec3, Vec3, [f32; 3])]) -> Option<Self> {
        use crate::grid::LineVertex;
        if lines.is_empty() {
            return None;
        }
        let verts: Vec<LineVertex> = lines.iter().flat_map(|(a, b, color)| [LineVertex { position: (*a).into(), color: *color }, LineVertex { position: (*b).into(), color: *color }]).collect();
        let buffer = device.create_buffer_init(&BufferInitDescriptor { label: Some("loom_helper_lines"), contents: bytemuck::cast_slice(&verts), usage: BufferUsages::VERTEX });
        Some(Self { buffer, count: verts.len() as u32 })
    }
}

fn mesh_usage(usage: BufferUsages) -> BufferUsages {
    let usage = usage | BufferUsages::COPY_DST;
    // GPU byte-for-byte regression readback; production buffers only need writes.
    #[cfg(test)]
    let usage = usage | BufferUsages::COPY_SRC;
    usage
}

fn update_buffer(device: &Device, queue: &wgpu::Queue, buffer: &mut Buffer, contents: &[u8], usage: BufferUsages, label: &str) {
    if contents.len() as u64 > buffer.size() {
        *buffer = device.create_buffer_init(&BufferInitDescriptor { label: Some(label), contents, usage: mesh_usage(usage) });
    } else if !contents.is_empty() {
        queue.write_buffer(buffer, 0, contents);
    }
}

#[cfg(test)]
fn edge_vertices(edges: &[(Vec3, Vec3)], color: [f32; 3]) -> Vec<crate::grid::LineVertex> {
    edges.iter().flat_map(|(a, b)| [crate::grid::LineVertex { position: (*a).into(), color }, crate::grid::LineVertex { position: (*b).into(), color }]).collect()
}

// Match Mesh::unique_edges order and visibility exactly, writing current pose
// positions directly instead of allocating an intermediate segment vector.
fn stage_edges(mesh: &Mesh, style: &MeshStyle, vertices: &mut Vec<crate::grid::LineVertex>) {
    vertices.clear();
    vertices.reserve(mesh.half_edge_count());
    let color = style.wire_color.unwrap_or(style.color);
    let half_edges = mesh.half_edges();
    let positions = mesh.vertices();
    let invisible = mesh.has_invisible_edges();
    for (i, he) in half_edges.iter().enumerate() {
        if (i as u32) < he.twin.0 && (!invisible || mesh.edge_visible(loom_mesh::HalfEdgeId(i as u32))) {
            let twin = &half_edges[he.twin.0 as usize];
            vertices.push(crate::grid::LineVertex { position: positions[he.origin.0 as usize].position.into(), color });
            vertices.push(crate::grid::LineVertex { position: positions[twin.origin.0 as usize].position.into(), color });
        }
    }
    if style.diagonals {
        for (a, b) in dashed(&fan_diagonals(mesh), DIAGONAL_DASHES) {
            vertices.push(crate::grid::LineVertex { position: a.into(), color });
            vertices.push(crate::grid::LineVertex { position: b.into(), color });
        }
    }
}

/// Triangulate every face fan-style. Each vertex is duplicated per face;
/// the corner normal follows the face's smoothing groups (flat where a
/// face has none). Hidden faces are skipped.
pub fn triangulate_with_face_normals(mesh: &Mesh, color: [f32; 4], vertex_colors: bool) -> (Vec<GpuVertex>, Vec<u32>) {
    let style = MeshStyle { color: [color[0], color[1], color[2]], alpha: color[3], vertex_colors, ..Default::default() };
    let (verts, indices, _) = triangulate_styled(mesh, &style);
    (verts, indices)
}

/// The edges of the faces whose style is Wire, as line-list vertices in the
/// material colour.
pub fn wire_lines(mesh: &Mesh, style: &MeshStyle) -> Vec<crate::grid::LineVertex> {
    let any_wire = match &style.sub {
        Some(sub) => sub.missing.wire || sub.by_id.values().any(|s| s.wire),
        None => style.wire,
    };
    if !any_wire { return Vec::new(); }
    let mut out = Vec::new();
    for (i, face) in mesh.faces().iter().enumerate() {
        let look = style.for_face(face.material_id);
        if face.hidden || !look.wire {
            continue;
        }
        let Some(pts) = mesh.face_positions(loom_mesh::FaceId(i as u32)) else { continue };
        for k in 0..pts.len() {
            for p in [pts[k], pts[(k + 1) % pts.len()]] {
                out.push(crate::grid::LineVertex { position: p.into(), color: look.color });
            }
        }
    }
    out
}

/// As `triangulate_with_face_normals`, each face coloured by its material
/// ID's style (`MeshStyle::for_face`), its UVs through that style's map
/// transform; the indices come grouped into one draw range per texture,
/// transparency and culling.
pub fn triangulate_styled(mesh: &Mesh, style: &MeshStyle) -> (Vec<GpuVertex>, Vec<u32>, Vec<DrawRange>) {
    let _scope = crate::profile::Scope::new("render.triangulate");
    let vertex_colors = style.vertex_colors;
    // Smoothing visits the same neighbouring faces at every corner. Compute
    // their normals once per triangulation, including hidden neighbours.
    let face_normals: Vec<Vec3> = (0..mesh.face_count()).map(|i| mesh.face_normal(loom_mesh::FaceId(i as u32))).collect();
    let mut vertex_faces: Vec<Option<Vec<loom_mesh::FaceId>>> = vec![None; mesh.vertex_count()];
    let mut vertex_normals = vec![(0, Vec3::ZERO); mesh.vertex_count()];
    let mut verts = Vec::new();
    // Triangles per draw group, in first-seen order.
    let mut groups: Vec<(DrawRange, Vec<u32>)> = Vec::new();
    let mut loop_verts: Vec<Vec3> = Vec::new();
    let mut loop_ids: Vec<loom_mesh::VertexId> = Vec::new();
    let mut loop_hes: Vec<loom_mesh::HalfEdgeId> = Vec::new();

    for (face_i, face) in mesh.faces().iter().enumerate() {
        if face.hidden {
            continue;
        }
        loop_verts.clear();
        loop_ids.clear();
        loop_hes.clear();
        let start = face.he;
        let mut he_id = start;
        loop {
            let he = &mesh.half_edges()[he_id.0 as usize];
            let v = &mesh.vertices()[he.origin.0 as usize];
            loop_verts.push(v.position);
            loop_ids.push(he.origin);
            loop_hes.push(he_id);
            he_id = he.next;
            if he_id == start {
                break;
            }
            if loop_verts.len() > 1024 {
                panic!("face loop did not close — mesh invariants violated");
            }
        }
        if loop_verts.len() < 3 {
            continue;
        }

        let look = style.for_face(face.material_id);
        if look.wire {
            continue;
        }
        let color = [look.color[0], look.color[1], look.color[2], look.alpha];
        let m = look.uv;
        let extra = [look.self_illum, if look.texture_alpha { -1.0 } else { look.texture_amount }, if look.decal[0] { 1.0 } else { 0.0 }, if look.decal[1] { 1.0 } else { 0.0 }];
        let flat = face.smoothing_groups == 0 || look.faceted;
        let n = face_normals[face_i];

        let base = verts.len() as u32;
        for (i, p) in loop_verts.iter().enumerate() {
            // An explicit normal (Edit Normals) wins over the smoothing groups.
            let normal = match mesh.explicit_normal(loop_hes[i]) {
                Some(e) => e,
                None if flat => n,
                None => {
                    let cached = &mut vertex_normals[loop_ids[i].0 as usize];
                    if cached.0 == face.smoothing_groups {
                        cached.1
                    } else {
                        let around = vertex_faces[loop_ids[i].0 as usize].get_or_insert_with(|| mesh.faces_around_vertex(loop_ids[i]));
                        let mut sum = Vec3::ZERO;
                        for f in around {
                            if mesh.faces()[f.0 as usize].smoothing_groups & face.smoothing_groups != 0 {
                                sum += face_normals[f.0 as usize];
                            }
                        }
                        let normal = if sum.length_squared() < 1e-12 { n } else { sum.normalize() };
                        // Degenerate sums fall back to this face's normal, so they
                        // cannot be shared with a different face at the vertex.
                        if sum.length_squared() >= 1e-12 { *cached = (face.smoothing_groups, normal); }
                        normal
                    }
                },
            };
            let vc = mesh.vertices()[loop_ids[i].0 as usize].color;
            let color = if vertex_colors { [color[0] * vc[0], color[1] * vc[1], color[2] * vc[2], color[3]] } else { color };
            // Map channel 1 at this corner; (0, 0) when the mesh has none.
            let uv = mesh.corner_uv(1, loop_hes[i]).map(|t| [t.x, t.y]).unwrap_or([0.0, 0.0]);
            let uv = [m[0] * uv[0] + m[1] * uv[1] + m[2], m[3] * uv[0] + m[4] * uv[1] + m[5]];
            verts.push(GpuVertex {
                position: (*p).into(),
                normal: normal.into(),
                uv,
                color,
                // A negative exponent tells the shader to leave the colour unlit.
                spec: if style.unlit { [0.0, 0.0, 0.0, -1.0] } else { look.specular },
                extra,
            });
        }
        let key = DrawRange { start: 0, count: 0, texture: look.texture, transparent: look.alpha < 1.0 || look.texture_alpha, cull: style.backface_cull && !look.two_sided };
        let g = match groups.iter().position(|(r, _)| (r.texture, r.transparent, r.cull) == (key.texture, key.transparent, key.cull)) {
            Some(g) => g,
            None => {
                groups.push((key, Vec::new()));
                groups.len() - 1
            }
        };
        for i in 1..(loop_verts.len() as u32 - 1) {
            groups[g].1.extend([base, base + i, base + i + 1]);
        }
    }

    let mut indices = Vec::new();
    let mut ranges = Vec::new();
    for (mut r, idx) in groups {
        r.start = indices.len() as u32;
        r.count = idx.len() as u32;
        indices.extend(idx);
        ranges.push(r);
    }
    (verts, indices, ranges)
}

#[cfg(test)]
mod tests {
    #[test]
    fn checker_is_an_eight_by_eight_grid_and_uvs_come_from_channel_1() {
        let c = Bitmap::checker();
        assert_eq!((c.width, c.height, c.rgba.len()), (256, 256, 256 * 256 * 4));
        let px = |x: u32, y: u32| c.rgba[((y * 256 + x) * 4) as usize];
        assert_ne!(px(0, 0), px(40, 0), "cells are 32 px");
        assert_eq!(px(0, 0), px(64, 0));
        assert_eq!(px(0, 0), px(32, 32));
        let mut m = loom_mesh::box_segmented(2.0, 2.0, 2.0, [1, 1, 1]);
        loom_mesh::uvmap::auto_map(&mut m, loom_mesh::uvmap::AutoMap::Box, 1);
        let (verts, _) = triangulate_with_face_normals(&m, [1.0; 4], false);
        assert!(verts.iter().any(|v| v.uv == [1.0, 1.0]) && verts.iter().any(|v| v.uv == [0.0, 0.0]));
        let plain = loom_mesh::box_primitive(glam::Vec3::ONE);
        let (verts, _) = triangulate_with_face_normals(&plain, [1.0; 4], false);
        assert!(verts.iter().all(|v| v.uv == [0.0, 0.0]), "no channel: uv (0, 0)");
    }

    use super::*;
    use loom_mesh::box_primitive;

    #[test]
    fn persistent_edge_staging_matches_reference_after_connectivity_visibility_and_style_edits() {
        let mut mesh = box_primitive(Vec3::ONE);
        let mut staging = Vec::new();
        let mut style = MeshStyle { color: [0.2, 0.4, 0.6], ..Default::default() };
        let check = |mesh: &Mesh, style: &MeshStyle, staging: &mut Vec<crate::grid::LineVertex>| {
            let mut edges = mesh.unique_edges();
            if style.diagonals { edges.extend(dashed(&fan_diagonals(mesh), DIAGONAL_DASHES)); }
            let expected = edge_vertices(&edges, style.wire_color.unwrap_or(style.color));
            stage_edges(mesh, style, staging);
            assert_eq!(bytemuck::cast_slice::<_, u8>(staging), bytemuck::cast_slice::<_, u8>(&expected));
        };
        check(&mesh, &style, &mut staging);
        mesh.faces_mut()[0].hidden = true;
        check(&mesh, &style, &mut staging); // Unique edges include hidden faces.
        style.diagonals = true;
        check(&mesh, &style, &mut staging);
        let he = mesh.faces()[1].he;
        mesh.set_edge_visible(he, false);
        check(&mesh, &style, &mut staging); // Invisible edges become dashed diagonals.
        style.wire_color = Some([-0.0, 0.7, 1.0]);
        mesh.vertices_mut()[0].position += Vec3::new(0.1, -0.2, 0.3);
        mesh.vertices_mut()[0].hidden = true;
        check(&mesh, &style, &mut staging);
        let counts = (mesh.vertex_count(), mesh.half_edge_count(), mesh.face_count());
        let faces: Vec<_> = (0..mesh.face_count()).map(|i| loom_mesh::FaceId(i as u32)).collect();
        loom_mesh::poly_ops::flip_faces(&mut mesh, &faces).unwrap();
        assert_eq!(counts, (mesh.vertex_count(), mesh.half_edge_count(), mesh.face_count()));
        check(&mesh, &style, &mut staging);
        let large = loom_mesh::sphere_segmented(1.0, 32, 0.0);
        check(&large, &style, &mut staging);
        let allocation = staging.as_ptr();
        check(&mesh, &style, &mut staging);
        assert_eq!(allocation, staging.as_ptr(), "smaller updates reuse CPU line storage");
        check(&Mesh::new(), &style, &mut staging);
    }

    #[test]
    fn wire_material_fast_path_respects_sub_material_overrides_and_missing_ids() {
        let mut mesh = box_primitive(Vec3::ONE);
        for (i, f) in mesh.faces_mut().iter_mut().enumerate() { f.material_id = (i % 2) as u16; }
        let off = MeshStyle::default().surface();
        let on = SurfaceStyle { wire: true, color: [0.1, 0.4, 0.7], ..off };
        let mut style = MeshStyle { wire: true, sub: Some(std::sync::Arc::new(SubStyles { by_id: [(0, off)].into_iter().collect(), missing: off })), ..Default::default() };
        assert!(wire_lines(&mesh, &style).is_empty(), "sub-materials override the base wire flag");
        style.sub = Some(std::sync::Arc::new(SubStyles { by_id: [(0, off)].into_iter().collect(), missing: on }));
        let expected: Vec<_> = mesh.faces().iter().enumerate().filter(|(_, f)| f.material_id == 1).flat_map(|(i, _)| {
            let points = mesh.face_positions(loom_mesh::FaceId(i as u32)).unwrap();
            (0..points.len()).flat_map(move |k| [points[k], points[(k + 1) % points.len()]])
        }).map(|p| crate::grid::LineVertex { position: p.to_array(), color: on.color }).collect();
        assert_eq!(bytemuck::cast_slice::<_, u8>(&wire_lines(&mesh, &style)), bytemuck::cast_slice::<_, u8>(&expected));
    }

    #[test]
    fn triangulated_smoothing_matches_corner_normals() {
        let mut mesh = loom_mesh::sphere_segmented(1.25, 24, 0.0);
        for (i, face) in mesh.faces_mut().iter_mut().enumerate() {
            face.smoothing_groups = [0, 1, 3, 2][i % 4];
            face.hidden = i % 7 == 0;
        }
        let (verts, _) = triangulate_with_face_normals(&mesh, [1.0; 4], false);
        let mut corner = 0;
        for (i, face) in mesh.faces().iter().enumerate().filter(|(_, f)| !f.hidden) {
            let start = face.he;
            let mut he_id = start;
            loop {
                let he = &mesh.half_edges()[he_id.0 as usize];
                let expected = mesh.explicit_normal(he_id).unwrap_or_else(|| mesh.vertex_normal(he.origin, loom_mesh::FaceId(i as u32)));
                assert_eq!(verts[corner].normal, expected.to_array());
                corner += 1;
                he_id = he.next;
                if he_id == start { break; }
            }
        }
        assert_eq!(corner, verts.len());
    }

    #[test]
    fn reused_gpu_buffers_match_fresh_geometry_after_growth_shrink_and_style_changes() {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let Some(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else { return };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None)).unwrap();
        let read = |buffer: &wgpu::Buffer, size: u64| {
            if size == 0 { return Vec::new(); }
            let output = device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ, mapped_at_creation: false });
            let mut encoder = device.create_command_encoder(&Default::default());
            encoder.copy_buffer_to_buffer(buffer, 0, &output, 0, size);
            queue.submit([encoder.finish()]);
            let slice = output.slice(..);
            let (tx, rx) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |result| { tx.send(result).unwrap(); });
            device.poll(wgpu::Maintain::Wait);
            rx.recv().unwrap().unwrap();
            let bytes = slice.get_mapped_range().to_vec();
            output.unmap();
            bytes
        };
        let small = box_primitive(Vec3::ONE);
        let mut moved = box_primitive(Vec3::splat(0.5));
        loom_mesh::edit_ops::translate_mesh(&mut moved, Vec3::new(3.0, -2.0, 1.0)).unwrap();
        let large = loom_mesh::sphere_segmented(1.25, 32, 0.0);
        let empty = Mesh::new();
        let mut edited = small.clone();
        edited.faces_mut()[0].hidden = true;
        edited.set_edge_visible(edited.faces()[1].he, false);
        edited.vertices_mut()[0].position += Vec3::new(0.2, -0.1, 0.3);
        let mut flipped = edited.clone();
        let faces: Vec<_> = (0..flipped.face_count()).map(|i| loom_mesh::FaceId(i as u32)).collect();
        loom_mesh::poly_ops::flip_faces(&mut flipped, &faces).unwrap();
        assert_eq!((edited.vertex_count(), edited.half_edge_count(), edited.face_count()), (flipped.vertex_count(), flipped.half_edge_count(), flipped.face_count()));
        let plain = MeshStyle::default();
        let colored = MeshStyle { color: [0.2, 0.4, 0.8], wire_color: Some([0.8, 0.1, 0.3]), alpha: 0.4, diagonals: true, ..Default::default() };
        let wire = MeshStyle { color: [0.0, 0.8, 0.2], wire: true, ..Default::default() };
        let mut lazy=MeshBuffer::from_mesh_with_faces(&device,&small,plain.clone(),false);
        assert_eq!(lazy.index_count,0,"wire-only views must not build unused shaded faces");
        let full=MeshBuffer::from_mesh(&device,&small,plain.clone());
        assert_eq!(read(&lazy.edge_vertex_buffer,full.edge_vertex_buffer.size()),read(&full.edge_vertex_buffer,full.edge_vertex_buffer.size()));
        lazy.update_from_mesh_with_faces(&device,&queue,&small,plain.clone(),true);
        assert_eq!(lazy.index_count,full.index_count);
        assert_eq!(read(&lazy.vertex_buffer,full.vertex_buffer.size()),read(&full.vertex_buffer,full.vertex_buffer.size()));
        assert_eq!(read(&lazy.index_buffer,full.index_buffer.size()),read(&full.index_buffer,full.index_buffer.size()));
        let mut reused = MeshBuffer::from_mesh(&device, &small, plain.clone());
        for (mesh, style) in [(&large, &colored), (&moved, &plain), (&small, &wire), (&edited, &colored), (&flipped, &colored), (&empty, &plain), (&small, &colored), (&small, &wire)] {
            let old = [reused.vertex_buffer.clone(), reused.index_buffer.clone(), reused.edge_vertex_buffer.clone(), reused.wire_vertex_buffer.clone()];
            reused.update_from_mesh(&device, &queue, mesh, style.clone());
            let fresh = MeshBuffer::from_mesh(&device, mesh, style.clone());
            assert_eq!((reused.index_count, reused.edge_vertex_count, reused.wire_vertex_count), (fresh.index_count, fresh.edge_vertex_count, fresh.wire_vertex_count));
            assert_eq!(reused.style, fresh.style);
            assert_eq!(reused.ranges, fresh.ranges);
            assert_eq!((reused.aabb_min, reused.aabb_max), (fresh.aabb_min, fresh.aabb_max));
            let current = [&reused.vertex_buffer, &reused.index_buffer, &reused.edge_vertex_buffer, &reused.wire_vertex_buffer];
            let expected = [&fresh.vertex_buffer, &fresh.index_buffer, &fresh.edge_vertex_buffer, &fresh.wire_vertex_buffer];
            for (stream, ((current, expected), old)) in current.into_iter().zip(expected).zip(old).enumerate() {
                // Inactive wire bytes are never drawn; transitions back to wire
                // must still replace all active vertices exactly.
                if stream != 3 || fresh.wire_vertex_count > 0 {
                    assert_eq!(read(current, expected.size()), read(expected, expected.size()));
                }
                if expected.size() <= old.size() { assert_eq!(current, &old, "an allocation that fits must be reused"); }
                else { assert_ne!(current, &old, "larger geometry needs a new allocation"); }
            }
        }
    }

    #[test]
    fn sphere_triangulates_within_index_range() {
        let m = loom_mesh::sphere_segmented(1.25, 32, 0.0);
        let (verts, indices) = triangulate_with_face_normals(&m, [1.0, 1.0, 1.0, 1.0], false);
        assert!(!verts.is_empty() && !indices.is_empty());
        assert!(indices.iter().all(|&i| (i as usize) < verts.len()));
        assert!(verts.iter().all(|v| v.position.iter().all(|x| x.is_finite()) && v.normal.iter().all(|x| x.is_finite())));
        assert_eq!(std::mem::size_of::<GpuVertex>(), 80);
    }

    #[test]
    fn box_triangulates_to_12_triangles() {
        let m = box_primitive(Vec3::splat(0.5));
        let (verts, indices) = triangulate_with_face_normals(&m, [1.0, 1.0, 1.0, 1.0], false);
        // 6 quads × 4 dup verts = 24 verts, 6 × 2 triangles × 3 idx = 36 idx
        assert_eq!(verts.len(), 24);
        assert_eq!(indices.len(), 36);
    }

    #[test]
    fn edges_only_off_adds_one_dashed_diagonal_per_quad() {
        let mut m = box_primitive(Vec3::splat(0.5));
        let d = fan_diagonals(&m);
        assert_eq!(d.len(), 6, "one diagonal per quad");
        let (_, indices) = triangulate_with_face_normals(&m, [1.0; 4], false);
        // Each diagonal is an edge of the fan triangles drawn for its face.
        let (verts, _) = triangulate_with_face_normals(&m, [1.0; 4], false);
        for (a, b) in &d {
            let shared = indices.chunks(3).any(|t| {
                let p: Vec<Vec3> = t.iter().map(|i| Vec3::from(verts[*i as usize].position)).collect();
                p.iter().any(|q| q.distance(*a) < 1e-6) && p.iter().any(|q| q.distance(*b) < 1e-6)
            });
            assert!(shared, "diagonal {a:?}-{b:?} is a triangle edge");
            assert!((*a - *b).length() > 1.0, "a diagonal crosses the face");
        }
        let dashes = dashed(&d, DIAGONAL_DASHES);
        assert_eq!(dashes.len(), 6 * DIAGONAL_DASHES);
        let (a, b) = d[0];
        assert!(dashes[0].0.distance(a) < 1e-6 && dashes[DIAGONAL_DASHES - 1].1.distance(b) < 1e-6, "the dashes start and end at the corners");
        let drawn: f32 = dashes[..DIAGONAL_DASHES].iter().map(|(p, q)| p.distance(*q)).sum();
        assert!((drawn / a.distance(b) - DIAGONAL_DASHES as f32 / (2 * DIAGONAL_DASHES - 1) as f32).abs() < 1e-4, "gaps as long as the dashes");
        m.faces_mut()[0].hidden = true;
        assert_eq!(fan_diagonals(&m).len(), 5, "hidden faces add none");
    }

    #[test]
    fn hidden_faces_are_not_drawn() {
        let mut m = box_primitive(Vec3::splat(0.5));
        m.faces_mut()[0].hidden = true;
        let (verts, indices) = triangulate_with_face_normals(&m, [1.0, 1.0, 1.0, 1.0], false);
        assert_eq!(verts.len(), 20);
        assert_eq!(indices.len(), 30);
    }

    #[test]
    fn smoothed_box_corners_share_a_normal() {
        let mut m = box_primitive(Vec3::splat(0.5));
        for f in m.faces_mut() {
            f.smoothing_groups = 1;
        }
        let (verts, _) = triangulate_with_face_normals(&m, [1.0, 1.0, 1.0, 1.0], false);
        // Every corner at the same position gets the same averaged normal.
        let p0 = verts[0].position;
        let normals: Vec<[f32; 3]> = verts.iter().filter(|v| v.position == p0).map(|v| v.normal).collect();
        assert!(normals.len() >= 3);
        assert!(normals.iter().all(|n| (0..3).all(|i| (n[i] - normals[0][i]).abs() < 1e-5)));
    }

    #[test]
    fn vertex_colours_tint_when_shown() {
        let mut m = box_primitive(Vec3::splat(0.5));
        for v in m.vertices_mut() {
            v.color = [1.0, 0.0, 0.0];
        }
        let (plain, _) = triangulate_with_face_normals(&m, [1.0, 1.0, 1.0, 1.0], false);
        assert_eq!(plain[0].color, [1.0, 1.0, 1.0, 1.0]);
        let (tinted, _) = triangulate_with_face_normals(&m, [1.0, 1.0, 1.0, 1.0], true);
        assert_eq!(tinted[0].color, [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn multi_sub_faces_take_their_material_id_colour_and_group_by_texture() {
        let mut m = box_primitive(Vec3::splat(0.5));
        for (i, f) in m.faces_mut().iter_mut().enumerate() {
            f.material_id = if i < 2 { 0 } else if i < 4 { 1 } else { 7 };
        }
        let red = SurfaceStyle { color: [1.0, 0.0, 0.0], ..MeshStyle::default().surface() };
        let green = SurfaceStyle { color: [0.0, 1.0, 0.0], texture: TextureSel::Bitmap(0), self_illum: 0.5, ..MeshStyle::default().surface() };
        let black = SurfaceStyle { color: [0.0; 3], ..MeshStyle::default().surface() };
        let style = MeshStyle { sub: Some(std::sync::Arc::new(SubStyles { by_id: [(0, red), (1, green)].into_iter().collect(), missing: black })), ..Default::default() };
        let (verts, indices, ranges) = triangulate_styled(&m, &style);
        assert_eq!(verts.len(), 24);
        let colour_of_face = |f: usize| verts[f * 4].color;
        assert_eq!(colour_of_face(0), [1.0, 0.0, 0.0, 1.0], "ID 1 faces are red");
        assert_eq!(colour_of_face(2), [0.0, 1.0, 0.0, 1.0], "ID 2 faces are green");
        assert_eq!(verts[8].extra[0], 0.5, "with their Self-Illumination");
        assert_eq!(colour_of_face(5), [0.0, 0.0, 0.0, 1.0], "an ID without a sub-material is black");
        // Two draws: the untextured faces (red and black) and the green bitmap ones.
        assert_eq!(ranges.len(), 2);
        let bitmap = ranges.iter().find(|r| r.texture == TextureSel::Bitmap(0)).unwrap();
        assert_eq!(bitmap.count, 12);
        assert_eq!(ranges.iter().map(|r| r.count).sum::<u32>() as usize, indices.len());
        // Every index of the bitmap range belongs to a green face.
        for i in &indices[bitmap.start as usize..(bitmap.start + bitmap.count) as usize] {
            assert_eq!(verts[*i as usize].color[1], 1.0);
        }
    }

    #[test]
    fn map_uv_transform_and_faceted() {
        let mut m = loom_mesh::sphere_segmented(1.0, 8, 0.0);
        loom_mesh::uvmap::auto_map(&mut m, loom_mesh::uvmap::AutoMap::Box, 1);
        for f in m.faces_mut() {
            f.smoothing_groups = 1;
        }
        let plain = triangulate_styled(&m, &MeshStyle::default()).0;
        let tiled = triangulate_styled(&m, &MeshStyle { uv: [2.0, 0.0, 0.5, 0.0, 3.0, 0.0], faceted: true, ..Default::default() }).0;
        for (a, b) in plain.iter().zip(&tiled) {
            assert!((b.uv[0] - (2.0 * a.uv[0] + 0.5)).abs() < 1e-5 && (b.uv[1] - 3.0 * a.uv[1]).abs() < 1e-5);
        }
        // Faceted: the corners of a face share the face normal.
        assert_eq!(tiled[0].normal, tiled[1].normal);
        assert_ne!(plain[0].normal, plain[1].normal);
    }

    #[test]
    fn wire_faces_draw_as_edges_in_the_material_colour() {
        let mut m = box_primitive(Vec3::splat(0.5));
        for f in m.faces_mut() {
            f.material_id = 0;
        }
        m.faces_mut()[0].material_id = 1;
        let wire = SurfaceStyle { color: [0.0, 1.0, 0.0], wire: true, ..MeshStyle::default().surface() };
        let style = MeshStyle { sub: Some(std::sync::Arc::new(SubStyles { by_id: [(1, wire)].into_iter().collect(), missing: MeshStyle::default().surface() })), ..Default::default() };
        let (verts, _, _) = triangulate_styled(&m, &style);
        assert_eq!(verts.len(), 20, "the wire face is not filled");
        let lines = wire_lines(&m, &style);
        assert_eq!(lines.len(), 8, "its four edges");
        assert!(lines.iter().all(|v| v.color == [0.0, 1.0, 0.0]));
        assert!(wire_lines(&m, &MeshStyle::default()).is_empty());
    }
    #[test]
    fn opacity_bitmap_routes_full_material_opacity_to_blend_without_changing_vertex_layout() {
        let mesh = box_primitive(Vec3::ONE);
        let (vertices, _, ranges) = triangulate_styled(&mesh, &MeshStyle { texture: TextureSel::Bitmap(0), texture_alpha: true, ..Default::default() });
        assert!(ranges.iter().all(|range| range.transparent));
        assert!(vertices.iter().all(|vertex| vertex.extra[1] == -1.0 && vertex.color[3] == 1.0));
        let (_, _, opaque) = triangulate_styled(&mesh, &MeshStyle::default());
        assert!(opaque.iter().all(|range| !range.transparent));
    }

}
