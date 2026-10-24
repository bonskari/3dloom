//! Mapping projections (UVW Map, primitive "Generate Mapping Coords"):
//! pure per-corner functions over a mesh in a gizmo frame.

use crate::half_edge::MapChannel;
use crate::indexed::{weld_channel, weld_channel_charts};
use crate::{FaceId, Mesh};
use glam::{Mat4, Vec3};
use std::f32::consts::{PI, TAU};

/// UVW Map > Mapping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Projection {
    Planar,
    Cylindrical { cap: bool },
    Spherical,
    ShrinkWrap,
    Box,
    Face,
    XyzToUvw,
}

/// The UVW Map parameters that shape a projection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapParams {
    pub projection: Projection,
    /// Object space → gizmo space (the gizmo's inverse transform).
    pub gizmo: Mat4,
    /// Gizmo extent along its X (Width), Y (Length), Z (Height).
    pub length: f32,
    pub width: f32,
    pub height: f32,
    pub u_tile: f32,
    pub v_tile: f32,
    pub w_tile: f32,
    pub u_flip: bool,
    pub v_flip: bool,
    pub w_flip: bool,
}

impl MapParams {
    /// A planar map fitted to the mesh bounds, gizmo at the bounds centre.
    pub fn fit(mesh: &Mesh) -> Self {
        let (lo, hi) = fit_gizmo(mesh);
        let size = hi - lo;
        let centre = (lo + hi) * 0.5;
        Self {
            projection: Projection::Planar,
            gizmo: Mat4::from_translation(centre).inverse(),
            length: size.y.max(1e-6),
            width: size.x.max(1e-6),
            height: size.z.max(1e-6),
            u_tile: 1.0,
            v_tile: 1.0,
            w_tile: 1.0,
            u_flip: false,
            v_flip: false,
            w_flip: false,
        }
    }
}

/// The mesh bounds (what Fit sizes the gizmo to).
pub fn fit_gizmo(mesh: &Mesh) -> (Vec3, Vec3) {
    if mesh.vertex_count() == 0 {
        return (Vec3::ZERO, Vec3::ONE);
    }
    mesh.aabb()
}

/// Projections the primitives generate their channel 1 with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AutoMap {
    Box,
    Planar,
    Spherical,
    Cylindrical { cap: bool },
    /// u around the ring (XY angle), v around the tube (angle in the
    /// ring's radial plane).
    Torus,
}

/// Generates `channel` on the whole mesh from its own bounds.
pub fn auto_map(mesh: &mut Mesh, kind: AutoMap, channel: u32) {
    let fitted = MapParams::fit(mesh);
    let params = match kind {
        AutoMap::Box => MapParams { projection: Projection::Box, ..fitted },
        AutoMap::Planar => fitted,
        AutoMap::Spherical => MapParams { projection: Projection::Spherical, ..fitted },
        AutoMap::Cylindrical { cap } => MapParams { projection: Projection::Cylindrical { cap }, ..fitted },
        AutoMap::Torus => {
            let ch = torus_channel(mesh);
            mesh.set_map(channel, ch);
            return;
        }
    };
    let ch = channel_for(mesh, &params, channel, None);
    mesh.set_map(channel, ch);
}

/// A copy of `mesh` with `channel` replaced by the projection (every face,
/// or only `faces` when given — UVW Map applies to the passed-up
/// selection).
pub fn project(mesh: &Mesh, params: &MapParams, channel: u32) -> Mesh {
    let mut out = mesh.clone();
    let ch = channel_for(mesh, params, channel, None);
    out.set_map(channel, ch);
    out
}

/// Projects only `faces`; other corners keep their coordinates.
pub fn project_faces(mesh: &Mesh, params: &MapParams, channel: u32, faces: &[FaceId]) -> Mesh {
    let mut out = mesh.clone();
    let ch = channel_for(mesh, params, channel, Some(faces));
    out.set_map(channel, ch);
    out
}

fn channel_for(mesh: &Mesh, params: &MapParams, channel: u32, faces: Option<&[FaceId]>) -> MapChannel {
    let n = mesh.half_edge_count();
    let mut vals: Vec<Vec3> = (0..n)
        .map(|i| mesh.corner_uv(channel, crate::HalfEdgeId(i as u32)).unwrap_or(Vec3::ZERO))
        .collect();
    // Chart per corner: untouched corners keep their old tv as a chart of its
    // own, mapped corners join the projection's chart (per face for Face,
    // per side for Box, caps apart for a capped cylinder).
    let old = mesh.map(channel);
    let mut charts: Vec<u32> = (0..n).map(|i| old.map(|m| m.corners[i]).unwrap_or(0).wrapping_add(1_000_000)).collect();
    let only: Option<std::collections::HashSet<FaceId>> = faces.map(|f| f.iter().copied().collect());
    for fi in 0..mesh.face_count() {
        let id = FaceId(fi as u32);
        if only.as_ref().is_some_and(|s| !s.contains(&id)) {
            continue;
        }
        let Some(corners) = mesh.face_corners(id) else { continue };
        let pts: Vec<Vec3> = corners.iter().map(|he| params.gizmo.transform_point3(mesh.vertices()[mesh.half_edges()[he.0 as usize].origin.0 as usize].position)).collect();
        let normal = params.gizmo.transform_vector3(mesh.face_normal(id)).normalize_or_zero();
        let uvs = face_uvs(params, &pts, normal);
        let chart = match params.projection {
            Projection::Face => 10 + fi as u32,
            Projection::Box => {
                let a = normal.abs();
                if a.z >= a.x && a.z >= a.y { if normal.z < 0.0 { 1 } else { 2 } } else if a.x >= a.y { if normal.x < 0.0 { 3 } else { 4 } } else if normal.y < 0.0 { 5 } else { 6 }
            }
            Projection::Cylindrical { cap: true } if normal.z.abs() > 0.99 => if normal.z < 0.0 { 1 } else { 2 },
            _ => 0,
        };
        for (he, uv) in corners.iter().zip(uvs) {
            vals[he.0 as usize] = tile(params, uv);
            charts[he.0 as usize] = chart;
        }
    }
    let origins: Vec<u32> = mesh.half_edges().iter().map(|h| h.origin.0).collect();
    weld_channel_charts(&vals, &origins, &charts)
}

fn tile(p: &MapParams, uv: Vec3) -> Vec3 {
    let mut u = uv.x * p.u_tile;
    let mut v = uv.y * p.v_tile;
    let mut w = uv.z * p.w_tile;
    if p.u_flip {
        u = p.u_tile - u;
    }
    if p.v_flip {
        v = p.v_tile - v;
    }
    if p.w_flip {
        w = p.w_tile - w;
    }
    Vec3::new(u, v, w)
}

/// Planar in the gizmo's XY: width along X, length along Y.
fn planar(p: &MapParams, pt: Vec3) -> Vec3 {
    Vec3::new(pt.x / p.width + 0.5, pt.y / p.length + 0.5, pt.z / p.height + 0.5)
}

/// Angle around Z as 0..1, continuous within a face: each corner's angle is
/// taken on the side of the face centre's angle (no wrap across the seam),
/// and the whole face shifts by one integer so its centre lies in 0..1 —
/// a face straddling the seam reaches past 1 rather than tearing.
fn around(pts: &[Vec3]) -> Vec<f32> {
    let centre = pts.iter().sum::<Vec3>() / pts.len() as f32;
    let ac = centre.y.atan2(centre.x);
    let uc = ac / TAU + 0.5;
    let shift = -(uc + 1e-6).floor();
    pts.iter()
        .map(|p| {
            let mut a = p.y.atan2(p.x);
            if p.x.abs() < 1e-7 && p.y.abs() < 1e-7 {
                a = ac;
            }
            while a - ac > PI {
                a -= TAU;
            }
            while ac - a > PI {
                a += TAU;
            }
            a / TAU + 0.5 + shift
        })
        .collect()
}

fn face_uvs(p: &MapParams, pts: &[Vec3], normal: Vec3) -> Vec<Vec3> {
    match p.projection {
        Projection::Planar => pts.iter().map(|q| planar(p, *q)).collect(),
        Projection::XyzToUvw => pts.iter().map(|q| Vec3::new(q.x / p.width, q.y / p.length, q.z / p.height)).collect(),
        Projection::Face => {
            let n = pts.len();
            (0..n)
                .map(|i| match (n, i) {
                    (3, 0) | (4, 0) => Vec3::new(0.0, 0.0, 0.0),
                    (3, 1) | (4, 1) => Vec3::new(1.0, 0.0, 0.0),
                    (3, _) | (4, 2) => Vec3::new(1.0, 1.0, 0.0),
                    (4, _) => Vec3::new(0.0, 1.0, 0.0),
                    _ => {
                        let a = i as f32 / n as f32 * TAU;
                        Vec3::new(0.5 + 0.5 * a.cos(), 0.5 + 0.5 * a.sin(), 0.0)
                    }
                })
                .collect()
        }
        Projection::Box => {
            // The dominant axis of the normal picks one of six planar maps.
            let a = normal.abs();
            pts.iter()
                .map(|q| {
                    if a.z >= a.x && a.z >= a.y {
                        let u = q.x / p.width + 0.5;
                        let u = if normal.z < 0.0 { 1.0 - u } else { u };
                        Vec3::new(u, q.y / p.length + 0.5, q.z / p.height + 0.5)
                    } else if a.x >= a.y {
                        let u = q.y / p.length + 0.5;
                        let u = if normal.x < 0.0 { 1.0 - u } else { u };
                        Vec3::new(u, q.z / p.height + 0.5, q.x / p.width + 0.5)
                    } else {
                        let u = q.x / p.width + 0.5;
                        let u = if normal.y > 0.0 { 1.0 - u } else { u };
                        Vec3::new(u, q.z / p.height + 0.5, q.y / p.length + 0.5)
                    }
                })
                .collect()
        }
        Projection::Cylindrical { cap } => {
            if cap && normal.z.abs() > 0.99 {
                return pts.iter().map(|q| planar(p, *q)).collect();
            }
            let us = around(pts);
            pts.iter().zip(us).map(|(q, u)| Vec3::new(u, q.z / p.height + 0.5, (q.x * q.x + q.y * q.y).sqrt() / p.width.max(p.length) * 2.0)).collect()
        }
        Projection::Spherical => {
            let us = around(pts);
            let r = 0.5 * p.width.max(p.length).max(p.height).max(1e-6);
            pts.iter()
                .zip(us)
                .map(|(q, u)| {
                    let len = q.length();
                    let v = if len < 1e-7 { 0.5 } else { (q.z / len).clamp(-1.0, 1.0).asin() / PI + 0.5 };
                    Vec3::new(u, v, len / r)
                })
                .collect()
        }
        Projection::ShrinkWrap => {
            // One pole: the direction's XY, scaled by how far from the top it is.
            pts.iter()
                .map(|q| {
                    let d = q.normalize_or_zero();
                    let k = ((1.0 - d.z) * 0.5).sqrt();
                    let xy = glam::Vec2::new(d.x, d.y).normalize_or_zero() * k;
                    Vec3::new(xy.x * 0.5 + 0.5, xy.y * 0.5 + 0.5, q.length() / p.height.max(1e-6))
                })
                .collect()
        }
    }
}

/// Torus mapping: u = angle around Z, v = angle around the tube.
fn torus_channel(mesh: &Mesh) -> MapChannel {
    let n = mesh.half_edge_count();
    let mut vals = vec![Vec3::ZERO; n];
    // Ring radius: the mean distance of the vertices from Z.
    let r1 = mesh.vertices().iter().map(|v| (v.position.x * v.position.x + v.position.y * v.position.y).sqrt()).sum::<f32>() / mesh.vertex_count().max(1) as f32;
    for fi in 0..mesh.face_count() {
        let id = FaceId(fi as u32);
        let Some(corners) = mesh.face_corners(id) else { continue };
        let pts: Vec<Vec3> = corners.iter().map(|he| mesh.vertices()[mesh.half_edges()[he.0 as usize].origin.0 as usize].position).collect();
        let us = around(&pts);
        // Tube angle: in the radial plane, (radial distance - r1, z).
        let local: Vec<Vec3> = pts.iter().map(|q| Vec3::new((q.x * q.x + q.y * q.y).sqrt() - r1, q.z, 0.0)).collect();
        let vs = around(&local);
        for ((he, u), v) in corners.iter().zip(us).zip(vs) {
            vals[he.0 as usize] = Vec3::new(u, v, 0.0);
        }
    }
    let origins: Vec<u32> = mesh.half_edges().iter().map(|h| h.origin.0).collect();
    weld_channel(&vals, &origins)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::standard_primitives::{box_segmented, sphere_segmented, torus_primitive};

    #[test]
    fn box_mapping_puts_every_face_in_the_unit_square() {
        let mut m = box_segmented(2.0, 4.0, 6.0, [2, 2, 2]);
        auto_map(&mut m, AutoMap::Box, 1);
        m.check_invariants().unwrap();
        for i in 0..m.half_edge_count() {
            let uv = m.corner_uv(1, crate::HalfEdgeId(i as u32)).unwrap();
            assert!(uv.x > -1e-5 && uv.x < 1.0 + 1e-5 && uv.y > -1e-5 && uv.y < 1.0 + 1e-5, "{uv:?}");
        }
    }

    #[test]
    fn spherical_v_runs_pole_to_pole() {
        let mut m = sphere_segmented(5.0, 12, 0.0);
        auto_map(&mut m, AutoMap::Spherical, 1);
        let (mut lo, mut hi) = (1.0f32, 0.0f32);
        for i in 0..m.half_edge_count() {
            let uv = m.corner_uv(1, crate::HalfEdgeId(i as u32)).unwrap();
            lo = lo.min(uv.y);
            hi = hi.max(uv.y);
        }
        assert!(lo < 0.05 && hi > 0.95, "{lo} {hi}");
    }

    #[test]
    fn torus_covers_both_angles() {
        let mut m = torus_primitive(10.0, 2.0, 12, 8, None);
        auto_map(&mut m, AutoMap::Torus, 1);
        m.check_invariants().unwrap();
        let (mut ulo, mut uhi, mut vlo, mut vhi) = (1.0f32, 0.0f32, 1.0f32, 0.0f32);
        for i in 0..m.half_edge_count() {
            let uv = m.corner_uv(1, crate::HalfEdgeId(i as u32)).unwrap();
            ulo = ulo.min(uv.x);
            uhi = uhi.max(uv.x);
            vlo = vlo.min(uv.y);
            vhi = vhi.max(uv.y);
        }
        assert!(ulo < 0.01 && uhi > 0.99 && vlo < 0.01 && vhi > 0.99, "{ulo} {uhi} {vlo} {vhi}");
    }
}
