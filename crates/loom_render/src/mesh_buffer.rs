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

/// How one mesh is drawn (Display panel properties).
#[derive(Debug, Clone, Copy, PartialEq)]
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
}

impl Default for MeshStyle {
    fn default() -> Self {
        Self { color: [0.7, 0.7, 0.7], alpha: 1.0, backface_cull: false, vertex_colors: false, texture: TextureSel::None, specular: [0.0; 4], wire_color: None, diagonals: false, unlit: false }
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
    pub vertex_buffer: Buffer,
    pub index_buffer: Buffer,
    pub index_count: u32,
    /// Pre-built line buffer of every unique edge, colored white — used to draw
    /// the selection wireframe overlay when this mesh is selected.
    pub edge_vertex_buffer: Buffer,
    pub selected_edge_vertex_buffer: Buffer,
    pub edge_vertex_count: u32,
    /// Cached AABB so picking doesn't have to recompute it every frame.
    pub aabb_min: Vec3,
    pub aabb_max: Vec3,
    pub style: MeshStyle,
}

impl MeshBuffer {
    pub fn from_mesh(device: &Device, mesh: &Mesh, style: MeshStyle) -> Self {
        let (mut verts, indices) = triangulate_with_face_normals(mesh, [style.color[0], style.color[1], style.color[2], style.alpha], style.vertex_colors);
        for v in &mut verts {
            // A negative exponent tells the shader to leave the colour unlit.
            v.spec = if style.unlit { [0.0, 0.0, 0.0, -1.0] } else { style.specular };
        }
        let vertex_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("loom_mesh_vertex_buffer"),
            contents: bytemuck::cast_slice(&verts),
            usage: BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("loom_mesh_index_buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: BufferUsages::INDEX,
        });

        let mut edges = mesh.unique_edges();
        if style.diagonals {
            edges.extend(dashed(&fan_diagonals(mesh), DIAGONAL_DASHES));
        }
        let edge_vertex_count = (edges.len() * 2) as u32;
        let edge_vertex_buffer = edge_buffer(device, &edges, style.wire_color.unwrap_or(style.color), "loom_mesh_edge_buffer");
        let selected_edge_vertex_buffer = edge_buffer(
            device,
            &edges,
            [1.0, 1.0, 1.0],
            "loom_mesh_selected_edge_buffer",
        );

        let (aabb_min, aabb_max) = mesh.aabb();

        Self {
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
            edge_vertex_buffer,
            selected_edge_vertex_buffer,
            edge_vertex_count,
            aabb_min,
            aabb_max,
            style,
        }
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

fn edge_buffer(device: &Device, edges: &[(Vec3, Vec3)], color: [f32; 3], label: &str) -> Buffer {
    use crate::grid::LineVertex;
    let mut edge_verts: Vec<LineVertex> = Vec::with_capacity(edges.len() * 2);
    for (a, b) in edges {
        edge_verts.push(LineVertex {
            position: (*a).into(),
            color,
        });
        edge_verts.push(LineVertex {
            position: (*b).into(),
            color,
        });
    }
    device.create_buffer_init(&BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(&edge_verts),
        usage: BufferUsages::VERTEX,
    })
}

/// Triangulate every face fan-style. Each vertex is duplicated per face;
/// the corner normal follows the face's smoothing groups (flat where a
/// face has none). Hidden faces are skipped.
pub fn triangulate_with_face_normals(mesh: &Mesh, color: [f32; 4], vertex_colors: bool) -> (Vec<GpuVertex>, Vec<u32>) {
    let mut verts = Vec::new();
    let mut indices = Vec::new();

    for (face_i, face) in mesh.faces().iter().enumerate() {
        if face.hidden {
            continue;
        }
        let face_id = loom_mesh::FaceId(face_i as u32);
        let mut loop_verts: Vec<Vec3> = Vec::new();
        let mut loop_ids: Vec<loom_mesh::VertexId> = Vec::new();
        let mut loop_hes: Vec<loom_mesh::HalfEdgeId> = Vec::new();
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

        let flat = face.smoothing_groups == 0;
        let n = mesh.face_normal(face_id);

        let base = verts.len() as u32;
        for (i, p) in loop_verts.iter().enumerate() {
            // An explicit normal (Edit Normals) wins over the smoothing groups.
            let normal = match mesh.explicit_normal(loop_hes[i]) {
                Some(e) => e,
                None if flat => n,
                None => mesh.vertex_normal(loop_ids[i], face_id),
            };
            let vc = mesh.vertices()[loop_ids[i].0 as usize].color;
            let color = if vertex_colors { [color[0] * vc[0], color[1] * vc[1], color[2] * vc[2], color[3]] } else { color };
            // Map channel 1 at this corner; (0, 0) when the mesh has none.
            let uv = mesh.corner_uv(1, loop_hes[i]).map(|t| [t.x, t.y]).unwrap_or([0.0, 0.0]);
            verts.push(GpuVertex {
                position: (*p).into(),
                normal: normal.into(),
                uv,
                color,
                spec: [0.0; 4],
            });
        }
        for i in 1..(loop_verts.len() as u32 - 1) {
            indices.push(base);
            indices.push(base + i);
            indices.push(base + i + 1);
        }
    }

    (verts, indices)
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
    fn sphere_triangulates_within_index_range() {
        let m = loom_mesh::sphere_segmented(1.25, 32, 0.0);
        let (verts, indices) = triangulate_with_face_normals(&m, [1.0, 1.0, 1.0, 1.0], false);
        assert!(!verts.is_empty() && !indices.is_empty());
        assert!(indices.iter().all(|&i| (i as usize) < verts.len()));
        assert!(verts.iter().all(|v| v.position.iter().all(|x| x.is_finite()) && v.normal.iter().all(|x| x.is_finite())));
        assert_eq!(std::mem::size_of::<GpuVertex>(), 64);
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
}
