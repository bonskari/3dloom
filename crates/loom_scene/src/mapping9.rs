//! Phase 9 mapping helpers: Camera Map and Camera Map (WSM), MapScaler and
//! MapScaler (WSM), UVW Mapping Add and UVW Mapping Clear.

use crate::modifier::{EvalCtx, StackValue};
use crate::node::NodeId;
use crate::param::Param;
use glam::{Mat4, Vec3};
use loom_mesh::FaceId;
use serde::{Deserialize, Serialize};

/// The render aspect Camera Map projects with (the default 640 × 480 output).
pub const RENDER_ASPECT: f32 = 4.0 / 3.0;

/// Camera Map: planar mapping from a camera's view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraMapData {
    pub camera: Option<NodeId>,
    pub channel: u32,
    /// Vertex Color Channel instead of a map channel.
    pub vertex_color: bool,
    /// The WSM: the camera's current placement every evaluation.
    pub world: bool,
    /// Object-space version: the camera relative to the object when it was picked.
    pub camera_in_object: Option<[[f32; 4]; 4]>,
    /// The camera's horizontal field of view (degrees) at pick time (object space).
    pub fov: f32,
}

impl Default for CameraMapData {
    fn default() -> Self {
        Self { camera: None, channel: 1, vertex_color: false, world: false, camera_in_object: None, fov: 45.0 }
    }
}

fn target_channel(vertex_color: bool, channel: u32) -> u32 {
    if vertex_color {
        0
    } else {
        channel.max(1)
    }
}

/// Every vertex projected through a camera (`view` = object space → camera
/// space, camera looking down its -Z); u, v in 0..1 across the frame.
fn project_all(v: &mut StackValue, view: Mat4, fov_deg: f32, ch: u32) {
    let half_w = (fov_deg.to_radians() * 0.5).tan().max(1e-6);
    let half_h = half_w / RENDER_ASPECT;
    let faces: Vec<FaceId> = (0..v.mesh.face_count() as u32).map(FaceId).collect();
    let pos: Vec<Vec3> = (0..v.mesh.vertex_count() as u32).map(|i| v.mesh.vertex_position(loom_mesh::VertexId(i)).unwrap_or_default()).collect();
    let mesh = &mut v.mesh;
    let origin_of: Vec<u32> = mesh.half_edges().iter().map(|h| h.origin.0).collect();
    loom_mesh::unwrap::assign_corner_uvs(mesh, ch, &faces, |he| {
        let p = view.transform_point3(pos[origin_of[he.0 as usize] as usize]);
        let depth = (-p.z).max(1e-6);
        Vec3::new(0.5 + 0.5 * p.x / depth / half_w, 0.5 + 0.5 * p.y / depth / half_h, 0.0)
    });
}

pub fn camera_map(d: &CameraMapData, mut v: StackValue, ctx: &EvalCtx<'_>) -> Result<StackValue, String> {
    let Some(cam) = d.camera else { return Ok(v) };
    let ch = target_channel(d.vertex_color, d.channel);
    let (view, fov) = if d.world {
        let world = ctx.node_world_matrix(cam).ok_or("Camera Map: the camera is gone")?;
        let fov = ctx.camera_fov.and_then(|f| f(cam)).unwrap_or(d.fov);
        (world.inverse() * ctx.object_world, fov)
    } else {
        let Some(m) = d.camera_in_object else { return Ok(v) };
        (Mat4::from_cols_array_2d(&m).inverse(), d.fov)
    };
    project_all(&mut v, view, fov, ch);
    Ok(v)
}

/// MapScaler: box mapping whose repeat size is Scale scene units, measured in
/// the object's space (object-space version) or the world (WSM).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MapScalerData {
    pub scale: Param<f32>,
    pub u_offset: Param<f32>,
    pub v_offset: Param<f32>,
    /// Wrap Texture: the map runs on around the sides instead of starting over on each.
    pub wrap: bool,
    pub channel: u32,
    pub world: bool,
}

impl Default for MapScalerData {
    fn default() -> Self {
        Self { scale: Param::new(1.0), u_offset: Param::new(0.0), v_offset: Param::new(0.0), wrap: true, channel: 1, world: false }
    }
}

pub fn map_scaler(d: &MapScalerData, mut v: StackValue, ctx: &EvalCtx<'_>) -> Result<StackValue, String> {
    let s = d.scale.get().max(1e-6);
    let (uo, vo) = (d.u_offset.get(), d.v_offset.get());
    let m = if d.world { ctx.object_world } else { Mat4::IDENTITY };
    let n = Mat4::from_mat3(glam::Mat3::from_mat4(m).inverse().transpose());
    let faces: Vec<FaceId> = (0..v.mesh.face_count() as u32).map(FaceId).collect();
    let pos: Vec<Vec3> = (0..v.mesh.vertex_count() as u32).map(|i| m.transform_point3(v.mesh.vertex_position(loom_mesh::VertexId(i)).unwrap_or_default())).collect();
    let (lo, hi) = pos.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(a, b), p| (a.min(*p), b.max(*p)));
    let (w, l) = (hi.x - lo.x, hi.y - lo.y);
    let normals: Vec<Vec3> = faces.iter().map(|f| n.transform_vector3(v.mesh.face_normal(*f)).normalize_or_zero()).collect();
    let face_of: Vec<Option<u32>> = v.mesh.half_edges().iter().map(|h| h.face.map(|f| f.0)).collect();
    let origin_of: Vec<u32> = v.mesh.half_edges().iter().map(|h| h.origin.0).collect();
    let wrap = d.wrap;
    loom_mesh::unwrap::assign_corner_uvs(&mut v.mesh, d.channel.max(1), &faces, |he| {
        let p = pos[origin_of[he.0 as usize] as usize];
        let nrm = face_of[he.0 as usize].map(|f| normals[f as usize]).unwrap_or(Vec3::Z);
        let a = nrm.abs();
        // Box projection by the face's dominant axis. Wrap Texture: the sides
        // share one U running counter-clockwise round the bounds from the
        // lower-left corner (-Y side, +X, +Y, -X), so the map does not restart.
        let (u, t) = if a.z >= a.x && a.z >= a.y {
            (p.x, p.y)
        } else if !wrap {
            if a.x >= a.y { (p.y * nrm.x.signum(), p.z) } else { (-p.x * nrm.y.signum(), p.z) }
        } else if a.x >= a.y {
            if nrm.x > 0.0 { (w + (p.y - lo.y), p.z) } else { (2.0 * w + l + (hi.y - p.y), p.z) }
        } else if nrm.y < 0.0 {
            (p.x - lo.x, p.z)
        } else {
            (w + l + (hi.x - p.x), p.z)
        };
        Vec3::new(u / s + uo, t / s + vo, 0.0)
    });
    Ok(v)
}

/// UVW Mapping Add: a new map channel after the highest one, every corner at (0, 0, 0).
pub fn mapping_add(mut v: StackValue) -> Result<StackValue, String> {
    let next = v.mesh.texture_maps().map(|(c, _)| *c).max().unwrap_or(0) + 1;
    let faces: Vec<FaceId> = (0..v.mesh.face_count() as u32).map(FaceId).collect();
    loom_mesh::unwrap::assign_corner_uvs(&mut v.mesh, next, &faces, |_| Vec3::ZERO);
    Ok(v)
}

/// UVW Mapping Clear: map channel `channel` removed.
pub fn mapping_clear(channel: u32, mut v: StackValue) -> Result<StackValue, String> {
    v.mesh.remove_map(channel);
    Ok(v)
}
