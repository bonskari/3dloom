//! Standard Primitives (Create > Geometry > Standard Primitives):
//! Box, Cone, Sphere, GeoSphere, Cylinder, Tube, Torus, Pyramid, Teapot,
//! Plane. Conventions: Length runs along Y, Width along X, objects
//! that have a base sit on z = 0 (box, cone, cylinder, tube, pyramid,
//! teapot); sphere, geosphere, torus and plane are centred on the pivot.

use crate::half_edge::{Face, FaceId, HalfEdge, HalfEdgeId, Vertex, VertexId};
use crate::teapot_data;
use crate::Mesh;
use glam::Vec3;
use std::collections::HashMap;
use std::f32::consts::TAU;

/// Slice range in degrees (Slice From, Slice To), as the rollouts.
pub type Slice = Option<(f32, f32)>;

/// Builds a mesh from polygons given as world positions. Vertices within
/// 1e-5 merge, repeated vertices in a polygon collapse, and every edge
/// without a neighbour gets a boundary half-edge with `face == None`.
pub fn build_merged(polygons: &[Vec<Vec3>]) -> Mesh {
    let mut mesh = Mesh::default();
    let mut vertex_map: HashMap<(i64, i64, i64), VertexId> = HashMap::new();
    let mut edge_map: HashMap<(VertexId, VertexId), HalfEdgeId> = HashMap::new();
    let key = |p: Vec3| ((p.x * 1e5).round() as i64, (p.y * 1e5).round() as i64, (p.z * 1e5).round() as i64);
    for poly in polygons {
        let mut ids: Vec<VertexId> = Vec::with_capacity(poly.len());
        for pos in poly {
            let id = *vertex_map.entry(key(*pos)).or_insert_with(|| {
                let id = VertexId(mesh.vertices.len() as u32);
                mesh.vertices.push(Vertex::new(*pos, None));
                id
            });
            if ids.last() != Some(&id) {
                ids.push(id);
            }
        }
        while ids.len() > 1 && ids.first() == ids.last() {
            ids.pop();
        }
        if ids.len() < 3 {
            continue;
        }
        // Skip a polygon that would duplicate an existing directed edge
        // (degenerate input); the mesh would not be manifold.
        if (0..ids.len()).any(|i| edge_map.contains_key(&(ids[i], ids[(i + 1) % ids.len()]))) {
            continue;
        }
        let face = FaceId(mesh.faces.len() as u32);
        let base = mesh.half_edges.len() as u32;
        let n = ids.len();
        for i in 0..n {
            let he = HalfEdgeId(base + i as u32);
            edge_map.insert((ids[i], ids[(i + 1) % n]), he);
            if mesh.vertices[ids[i].0 as usize].he.is_none() {
                mesh.vertices[ids[i].0 as usize].he = Some(he);
            }
            mesh.half_edges.push(HalfEdge {
                origin: ids[i],
                twin: he,
                next: HalfEdgeId(base + ((i + 1) % n) as u32),
                prev: HalfEdgeId(base + ((i + n - 1) % n) as u32),
                face: Some(face),
            });
        }
        mesh.faces.push(Face::new(HalfEdgeId(base)));
    }
    // Twins, and boundary half-edges for unmatched ones.
    let mut boundary_by_origin: HashMap<VertexId, HalfEdgeId> = HashMap::new();
    // In face order: boundary half-edges get the same ids on every build
    // (corner data downstream is keyed by half-edge id).
    let mut pairs: Vec<((VertexId, VertexId), HalfEdgeId)> = edge_map.iter().map(|(k, v)| (*k, *v)).collect();
    pairs.sort_by_key(|(_, he)| he.0);
    for ((from, to), he) in pairs {
        if let Some(twin) = edge_map.get(&(to, from)) {
            mesh.half_edges[he.0 as usize].twin = *twin;
        } else {
            let b = HalfEdgeId(mesh.half_edges.len() as u32);
            mesh.half_edges.push(HalfEdge { origin: to, twin: he, next: b, prev: b, face: None });
            mesh.half_edges[he.0 as usize].twin = b;
            boundary_by_origin.insert(to, b);
        }
    }
    // Link boundary loops: a boundary half-edge's next starts where it ends.
    let boundary: Vec<HalfEdgeId> = boundary_by_origin.values().copied().collect();
    for b in boundary {
        let end = {
            let he = &mesh.half_edges[b.0 as usize];
            mesh.half_edges[he.twin.0 as usize].origin
        };
        if let Some(next) = boundary_by_origin.get(&end).copied() {
            mesh.half_edges[b.0 as usize].next = next;
            mesh.half_edges[next.0 as usize].prev = b;
        }
    }
    // Primitives come with smoothing: faceted across hard edges, smooth
    // across shallow ones (Auto Smooth at 45°).
    mesh.auto_smooth(45.0);
    mesh
}

/// Flips every polygon if the enclosed signed volume is negative, so closed
/// solids come out with outward normals regardless of how the profile ran.
fn oriented(mut polys: Vec<Vec<Vec3>>) -> Vec<Vec<Vec3>> {
    let mut volume = 0.0f32;
    for p in &polys {
        for i in 1..p.len().saturating_sub(1) {
            volume += p[0].dot(p[i].cross(p[i + 1]));
        }
    }
    if volume < 0.0 {
        for p in &mut polys {
            p.reverse();
        }
    }
    polys
}

fn grid(corner: Vec3, du: Vec3, dv: Vec3, nu: u32, nv: u32) -> Vec<Vec<Vec3>> {
    let (nu, nv) = (nu.max(1), nv.max(1));
    let mut polys = Vec::with_capacity((nu * nv) as usize);
    for j in 0..nv {
        for i in 0..nu {
            let (u0, u1) = (i as f32 / nu as f32, (i + 1) as f32 / nu as f32);
            let (v0, v1) = (j as f32 / nv as f32, (j + 1) as f32 / nv as f32);
            polys.push(vec![
                corner + du * u0 + dv * v0,
                corner + du * u1 + dv * v0,
                corner + du * u1 + dv * v1,
                corner + du * u0 + dv * v1,
            ]);
        }
    }
    polys
}

/// Box: Length (Y), Width (X), Height (Z); segs = [length, width, height].
pub fn box_segmented(length: f32, width: f32, height: f32, segs: [u32; 3]) -> Mesh {
    let (l, w, h) = (length.max(0.0), width.max(0.0), height.max(0.0));
    let (ls, ws, hs) = (segs[0].max(1), segs[1].max(1), segs[2].max(1));
    let x0 = -w * 0.5;
    let y0 = -l * 0.5;
    let (dx, dy, dz) = (Vec3::X * w, Vec3::Y * l, Vec3::Z * h);
    let o = Vec3::new(x0, y0, 0.0);
    let mut polys = Vec::new();
    polys.extend(grid(o, dy, dx, ls, ws)); // bottom (faces -Z)
    polys.extend(grid(o + dz, dx, dy, ws, ls)); // top
    polys.extend(grid(o, dx, dz, ws, hs)); // -Y side
    polys.extend(grid(o + dy, dz, dx, hs, ws)); // +Y side
    polys.extend(grid(o, dz, dy, hs, ls)); // -X side
    polys.extend(grid(o + dx, dy, dz, ls, hs)); // +X side
    build_merged(&oriented(polys))
}

/// Plane: Length (Y), Width (X), centred on the pivot at z = 0, facing +Z.
pub fn plane_segmented(length: f32, width: f32, length_segs: u32, width_segs: u32) -> Mesh {
    let (l, w) = (length.max(0.0), width.max(0.0));
    let o = Vec3::new(-w * 0.5, -l * 0.5, 0.0);
    build_merged(&grid(o, Vec3::X * w, Vec3::Y * l, width_segs, length_segs))
}

/// Revolves a profile of (radius, z) points around Z. `columns` columns over
/// the full turn, or over the slice. A closed profile (torus, tube) joins its
/// last point to its first. Slice cuts are capped with the profile polygon.
fn revolve(profile: &[(f32, f32)], columns: u32, slice: Slice, closed_profile: bool, twist_deg: f32) -> Vec<Vec<Vec3>> {
    let full_columns = columns.max(3);
    let (a0, a1, wrap) = match slice {
        None => (0.0, TAU, true),
        Some((from, to)) => {
            let (f, mut t) = (from.to_radians(), to.to_radians());
            if t < f {
                t += TAU;
            }
            // Slice From == Slice To (e.g. the default 0/0) keeps the whole object.
            if (t - f).abs() < 1e-6 || (t - f) >= TAU - 1e-6 {
                (0.0, TAU, true)
            } else {
                (f, t, false)
            }
        }
    };
    // Sides are distributed over the full turn; a slice keeps the same
    // angular density, so a half slice shows half the sides.
    let columns = if wrap { full_columns } else { ((full_columns as f32 * (a1 - a0) / TAU).round() as u32).max(1) };
    let point = |c: u32, (r, z): (f32, f32), row_frac: f32| -> Vec3 {
        let a = a0 + (a1 - a0) * (c % columns.max(1)) as f32 / columns as f32 + twist_deg.to_radians() * row_frac;
        let a = if !wrap && c == columns { a1 + twist_deg.to_radians() * row_frac } else { a };
        Vec3::new(a.cos() * r, a.sin() * r, z)
    };
    let n = profile.len();
    let rows = if closed_profile { n } else { n - 1 };
    let mut polys = Vec::new();
    for i in 0..rows {
        let j = (i + 1) % n;
        let (fi, fj) = (i as f32 / n as f32, j as f32 / n as f32);
        for c in 0..columns {
            let c2 = if wrap { (c + 1) % columns } else { c + 1 };
            polys.push(vec![
                point(c, profile[i], fi),
                point(c2, profile[i], fi),
                point(c2, profile[j], fj),
                point(c, profile[j], fj),
            ]);
        }
    }
    if !wrap {
        let cap_start: Vec<Vec3> = profile.iter().enumerate().map(|(i, p)| point(0, *p, i as f32 / n as f32)).collect();
        let cap_end: Vec<Vec3> = profile.iter().enumerate().rev().map(|(i, p)| point(columns, *p, i as f32 / n as f32)).collect();
        polys.push(cap_start);
        polys.push(cap_end);
    }
    polys
}

/// Cylinder: Radius, Height, Height Segs, Cap Segs, Sides, Slice.
pub fn cylinder_segmented(radius: f32, height: f32, height_segs: u32, cap_segs: u32, sides: u32, slice: Slice) -> Mesh {
    cone_primitive(radius, radius, height, height_segs, cap_segs, sides, slice)
}

/// Cone: Radius 1 (base), Radius 2 (top), Height, Height Segs, Cap Segs, Sides, Slice.
pub fn cone_primitive(radius1: f32, radius2: f32, height: f32, height_segs: u32, cap_segs: u32, sides: u32, slice: Slice) -> Mesh {
    let (r1, r2, h) = (radius1.max(0.0), radius2.max(0.0), height.max(0.0));
    let (hs, cs) = (height_segs.max(1), cap_segs.max(1));
    let mut profile = vec![(0.0, 0.0)];
    for k in 1..=cs {
        profile.push((r1 * k as f32 / cs as f32, 0.0));
    }
    for k in 1..=hs {
        let t = k as f32 / hs as f32;
        profile.push((r1 + (r2 - r1) * t, h * t));
    }
    for k in (0..cs).rev() {
        profile.push((r2 * k as f32 / cs as f32, h));
    }
    build_merged(&oriented(revolve(&profile, sides, slice, false, 0.0)))
}

/// Tube: Radius 1, Radius 2 (either order), Height, Height Segs, Cap Segs, Sides, Slice.
pub fn tube_primitive(radius1: f32, radius2: f32, height: f32, height_segs: u32, cap_segs: u32, sides: u32, slice: Slice) -> Mesh {
    let outer = radius1.max(radius2).max(0.0);
    let inner = radius1.min(radius2).max(0.0);
    let h = height.max(0.0);
    let (hs, cs) = (height_segs.max(1), cap_segs.max(1));
    let mut profile = Vec::new();
    for k in 0..=hs {
        profile.push((outer, h * k as f32 / hs as f32));
    }
    for k in 1..=cs {
        profile.push((outer + (inner - outer) * k as f32 / cs as f32, h));
    }
    for k in (0..hs).rev() {
        profile.push((inner, h * k as f32 / hs as f32));
    }
    for k in (1..cs).rev() {
        profile.push((outer + (inner - outer) * k as f32 / cs as f32, 0.0));
    }
    build_merged(&oriented(revolve(&profile, sides, slice, true, 0.0)))
}

/// Sphere: Radius, Segments, Hemisphere (0 = full, 0.5 = half; the bottom
/// is chopped and capped, as in Chop mode).
pub fn sphere_segmented(radius: f32, segments: u32, hemisphere: f32) -> Mesh {
    let r = radius.max(0.0);
    let seg = segments.max(4);
    let rings = (seg / 2).max(2);
    let h = hemisphere.clamp(0.0, 1.0);
    let z_cut = -r + 2.0 * r * h;
    let theta_cut = if r > 0.0 { (z_cut / r).clamp(-1.0, 1.0).acos() } else { std::f32::consts::PI };
    let mut profile = Vec::new();
    if h > 0.0 {
        profile.push((0.0, z_cut));
    }
    let steps = ((theta_cut / std::f32::consts::PI) * rings as f32).ceil().max(1.0) as u32;
    for k in (0..=steps).rev() {
        let theta = theta_cut * k as f32 / steps as f32;
        profile.push((theta.sin() * r, theta.cos() * r));
    }
    build_merged(&oriented(revolve(&profile, seg, None, false, 0.0)))
}

/// Torus: Radius 1 (ring), Radius 2 (cross-section), Segments, Sides, Slice,
/// with Rotation/Twist via `torus_full`.
pub fn torus_primitive(radius1: f32, radius2: f32, segments: u32, sides: u32, slice: Slice) -> Mesh {
    torus_full(radius1, radius2, 0.0, 0.0, segments, sides, slice)
}

pub fn torus_full(radius1: f32, radius2: f32, rotation_deg: f32, twist_deg: f32, segments: u32, sides: u32, slice: Slice) -> Mesh {
    let (r1, r2) = (radius1.max(0.0), radius2.max(0.0));
    let (segments, sides) = (segments.max(3), sides.max(3));
    // Ring angle range (Slice) and the number of ring columns over it.
    let (a0, a1, wrap) = match slice {
        None => (0.0, TAU, true),
        Some((from, to)) => {
            let (f, mut t) = (from.to_radians(), to.to_radians());
            if t < f {
                t += TAU;
            }
            if (t - f).abs() < 1e-6 || (t - f) >= TAU - 1e-6 {
                (0.0, TAU, true)
            } else {
                (f, t, false)
            }
        }
    };
    let columns = if wrap { segments } else { ((segments as f32 * (a1 - a0) / TAU).round() as u32).max(1) };
    let cols = if wrap { columns } else { columns + 1 };
    // Rotation turns every cross-section about the ring's centre
    // circle; Twist adds a progressive turn per segment around the ring.
    let point = |c: u32, k: u32| -> Vec3 {
        let frac = c as f32 / columns as f32;
        let a = a0 + (a1 - a0) * frac;
        let t = rotation_deg.to_radians() + twist_deg.to_radians() * frac + TAU * k as f32 / sides as f32;
        let r = r1 + r2 * t.cos();
        Vec3::new(a.cos() * r, a.sin() * r, r2 * t.sin())
    };
    let mut polys = Vec::new();
    for c in 0..columns {
        let c2 = if wrap { (c + 1) % columns } else { c + 1 };
        for k in 0..sides {
            let k2 = (k + 1) % sides;
            polys.push(vec![point(c, k), point(c2, k), point(c2, k2), point(c, k2)]);
        }
    }
    if !wrap {
        polys.push((0..sides).map(|k| point(0, k)).collect());
        polys.push((0..sides).rev().map(|k| point(cols - 1, k)).collect());
    }
    build_merged(&oriented(polys))
}

/// Pyramid: Width (X), Depth (Y), Height; segs = [width, depth, height].
pub fn pyramid_primitive(width: f32, depth: f32, height: f32, segs: [u32; 3]) -> Mesh {
    let (w, d, h) = (width.max(0.0), depth.max(0.0), height.max(0.0));
    let (ws, ds, hs) = (segs[0].max(1), segs[1].max(1), segs[2].max(1));
    let apex = Vec3::new(0.0, 0.0, h);
    let corners = [
        Vec3::new(-w * 0.5, -d * 0.5, 0.0),
        Vec3::new(w * 0.5, -d * 0.5, 0.0),
        Vec3::new(w * 0.5, d * 0.5, 0.0),
        Vec3::new(-w * 0.5, d * 0.5, 0.0),
    ];
    let mut polys = grid(corners[0], Vec3::Y * d, Vec3::X * w, ds, ws);
    for side in 0..4 {
        let (a, b) = (corners[side], corners[(side + 1) % 4]);
        let cols = if side % 2 == 0 { ws } else { ds };
        for row in 0..hs {
            let (t0, t1) = (row as f32 / hs as f32, (row + 1) as f32 / hs as f32);
            let (a0, b0) = (a.lerp(apex, t0), b.lerp(apex, t0));
            let (a1, b1) = (a.lerp(apex, t1), b.lerp(apex, t1));
            for c in 0..cols {
                let (u0, u1) = (c as f32 / cols as f32, (c + 1) as f32 / cols as f32);
                polys.push(vec![a0.lerp(b0, u0), a0.lerp(b0, u1), a1.lerp(b1, u1), a1.lerp(b1, u0)]);
            }
        }
    }
    build_merged(&oriented(polys))
}

/// GeoSphere base solid (Parameters rollout > Geodesic Base Type).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeoBase {
    Tetra,
    Octa,
    Icosa,
}

/// GeoSphere: Radius, Segments (subdivision frequency), base type.
pub fn geosphere_primitive(radius: f32, segments: u32, base: GeoBase) -> Mesh {
    let r = radius.max(0.0);
    let f = segments.max(1);
    let (verts, tris): (Vec<Vec3>, Vec<[usize; 3]>) = match base {
        GeoBase::Tetra => (
            vec![Vec3::new(1.0, 1.0, 1.0), Vec3::new(-1.0, -1.0, 1.0), Vec3::new(-1.0, 1.0, -1.0), Vec3::new(1.0, -1.0, -1.0)],
            vec![[0, 1, 2], [0, 3, 1], [0, 2, 3], [1, 3, 2]],
        ),
        GeoBase::Octa => (
            vec![Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z],
            vec![[0, 2, 4], [2, 1, 4], [1, 3, 4], [3, 0, 4], [2, 0, 5], [1, 2, 5], [3, 1, 5], [0, 3, 5]],
        ),
        GeoBase::Icosa => {
            let t = (1.0 + 5f32.sqrt()) * 0.5;
            let v = vec![
                Vec3::new(-1.0, t, 0.0), Vec3::new(1.0, t, 0.0), Vec3::new(-1.0, -t, 0.0), Vec3::new(1.0, -t, 0.0),
                Vec3::new(0.0, -1.0, t), Vec3::new(0.0, 1.0, t), Vec3::new(0.0, -1.0, -t), Vec3::new(0.0, 1.0, -t),
                Vec3::new(t, 0.0, -1.0), Vec3::new(t, 0.0, 1.0), Vec3::new(-t, 0.0, -1.0), Vec3::new(-t, 0.0, 1.0),
            ];
            let tris = vec![
                [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11], [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
                [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9], [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
            ];
            (v, tris)
        }
    };
    let project = |p: Vec3| p.normalize_or_zero() * r;
    let mut polys = Vec::new();
    for [ia, ib, ic] in tris {
        let (a, b, c) = (verts[ia], verts[ib], verts[ic]);
        // Barycentric grid of frequency f.
        let at = |i: u32, j: u32| -> Vec3 {
            let (u, v) = (i as f32 / f as f32, j as f32 / f as f32);
            project(a * (1.0 - u - v) + b * u + c * v)
        };
        for i in 0..f {
            for j in 0..(f - i) {
                polys.push(vec![at(i, j), at(i + 1, j), at(i, j + 1)]);
                if j + 1 < f - i {
                    polys.push(vec![at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)]);
                }
            }
        }
    }
    build_merged(&oriented(polys))
}

/// Teapot Parts group of the Teapot rollout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeapotParts {
    pub body: bool,
    pub handle: bool,
    pub spout: bool,
    pub lid: bool,
}

impl Default for TeapotParts {
    fn default() -> Self {
        Self { body: true, handle: true, spout: true, lid: true }
    }
}

/// Teapot: Radius (body radius), Segments (tessellation per patch edge).
pub fn teapot_primitive(radius: f32, segments: u32, parts: TeapotParts) -> Mesh {
    let scale = radius.max(0.0) / 2.0;
    let n = segments.max(1);
    let mut polys = Vec::new();
    for (pi, patch) in teapot_data::PATCHES.iter().enumerate() {
        let keep = match pi {
            0..=11 | 28..=31 => parts.body,
            12..=15 => parts.handle,
            16..=19 => parts.spout,
            _ => parts.lid,
        };
        if !keep {
            continue;
        }
        let cp: Vec<Vec3> = patch.iter().map(|&i| Vec3::from(teapot_data::VERTICES[i as usize]) * scale).collect();
        let eval = |u: f32, v: f32| -> Vec3 {
            let b = |t: f32| [(1.0 - t).powi(3), 3.0 * t * (1.0 - t).powi(2), 3.0 * t * t * (1.0 - t), t.powi(3)];
            let (bu, bv) = (b(u), b(v));
            let mut p = Vec3::ZERO;
            for (i, bui) in bu.iter().enumerate() {
                for (j, bvj) in bv.iter().enumerate() {
                    p += cp[i * 4 + j] * (bui * bvj);
                }
            }
            p
        };
        for i in 0..n {
            for j in 0..n {
                let (u0, u1) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
                let (v0, v1) = (j as f32 / n as f32, (j + 1) as f32 / n as f32);
                polys.push(vec![eval(u0, v0), eval(u1, v0), eval(u1, v1), eval(u0, v1)]);
            }
        }
    }
    build_merged(&oriented(polys))
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn no_nan(m: &Mesh) -> bool {
        m.vertices().iter().all(|v| v.position.is_finite())
    }

    fn all_kinds(size: f32) -> Vec<(&'static str, Mesh)> {
        vec![
            ("box", box_segmented(size, size, size, [1, 1, 1])),
            ("cone", cone_primitive(size, 0.0, size, 1, 1, 3, None)),
            ("sphere", sphere_segmented(size, 4, 0.0)),
            ("geosphere", geosphere_primitive(size, 1, GeoBase::Icosa)),
            ("cylinder", cylinder_segmented(size, size, 1, 1, 3, None)),
            ("tube", tube_primitive(size, size * 0.5, size, 1, 1, 3, None)),
            ("torus", torus_primitive(size, size * 0.25, 3, 3, None)),
            ("pyramid", pyramid_primitive(size, size, size, [1, 1, 1])),
            ("teapot", teapot_primitive(size, 1, TeapotParts::default())),
            ("plane", plane_segmented(size, size, 1, 1)),
        ]
    }

    #[test]
    fn every_primitive_passes_invariants_at_min_params() {
        // Minimum segment counts with a real size: valid and non-empty.
        for (name, m) in all_kinds(1.0) {
            assert!(no_nan(&m), "{name} has NaN");
            m.check_invariants().unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(m.face_count() > 0, "{name} has no faces");
        }
        // Zero size (0 is allowed): still valid and finite, possibly empty.
        for (name, m) in all_kinds(0.0) {
            assert!(no_nan(&m), "{name} has NaN at size 0");
            m.check_invariants().unwrap_or_else(|e| panic!("{name} at 0: {e}"));
        }
    }

    #[test]
    fn twins_are_matched_for_closed_solids() {
        for (name, m) in all_kinds(1.0) {
            // The plane is open; the teapot's rim, lid, handle and spout are
            // separate surfaces in the Newell data).
            if name == "plane" || name == "teapot" {
                continue;
            }
            let unmatched = m.half_edges().iter().filter(|he| he.face.is_none()).count();
            assert_eq!(unmatched, 0, "{name} has {unmatched} boundary half-edges (inconsistent winding)");
            let self_twins = m.half_edges().iter().enumerate().filter(|(i, he)| he.twin.0 as usize == *i).count();
            assert_eq!(self_twins, 0, "{name} has {self_twins} self-twins");
        }
    }

    #[test]
    fn box_segments_multiply_faces() {
        let m = box_segmented(2.0, 3.0, 4.0, [2, 3, 4]);
        // 2 faces of 2x3, 2 of 2x4, 2 of 3x4
        assert_eq!(m.face_count(), 2 * (2 * 3 + 2 * 4 + 3 * 4));
        let (min, max) = m.aabb();
        assert!(min.abs_diff_eq(Vec3::new(-1.5, -1.0, 0.0), 1e-5) && max.abs_diff_eq(Vec3::new(1.5, 1.0, 4.0), 1e-5), "{min} {max}");
        m.check_invariants().unwrap();
    }

    #[test]
    fn torus_vertex_count_is_segments_times_sides() {
        let m = torus_primitive(2.0, 0.5, 24, 12, None);
        assert_eq!(m.vertex_count(), 24 * 12);
        assert_eq!(m.face_count(), 24 * 12);
        m.check_invariants().unwrap();
    }

    #[test]
    fn cone_with_zero_radius2_has_apex_triangles() {
        let m = cone_primitive(1.0, 0.0, 2.0, 1, 1, 8, None);
        m.check_invariants().unwrap();
        let (min, max) = m.aabb();
        assert!((max.z - 2.0).abs() < 1e-5 && min.z.abs() < 1e-5);
    }

    #[test]
    fn sliced_cylinder_has_cut_faces_and_is_closed() {
        let full = cylinder_segmented(1.0, 2.0, 1, 1, 8, None);
        let half = cylinder_segmented(1.0, 2.0, 1, 1, 8, Some((0.0, 180.0)));
        assert!(half.face_count() < full.face_count());
        half.check_invariants().unwrap();
        assert!(half.half_edges().iter().all(|he| he.face.is_some()), "sliced cylinder must be closed");
    }

    #[test]
    fn teapot_has_closed_manifold_body() {
        let m = teapot_primitive(1.0, 4, TeapotParts::default());
        m.check_invariants().unwrap();
        assert!(m.face_count() > 500);
        let (min, max) = m.aabb();
        assert!(min.z.abs() < 1e-4, "teapot sits on z=0: {min}");
        assert!(max.z > 0.5 && max.x.abs() <= 1.01 * 1.0 * 3.5 / 2.0, "{max}");
        let body_only = teapot_primitive(1.0, 4, TeapotParts { body: true, handle: false, spout: false, lid: false });
        assert!(body_only.face_count() < m.face_count());
    }

    #[test]
    fn geosphere_triangle_count_follows_subdivision() {
        let m = geosphere_primitive(1.0, 2, GeoBase::Icosa);
        assert_eq!(m.face_count(), 20 * 4);
        m.check_invariants().unwrap();
        let o = geosphere_primitive(1.0, 1, GeoBase::Octa);
        assert_eq!(o.face_count(), 8);
    }

    #[test]
    fn hemisphere_half_keeps_upper_half() {
        let m = sphere_segmented(1.0, 16, 0.5);
        let (min, max) = m.aabb();
        assert!(min.z.abs() < 1e-4 && (max.z - 1.0).abs() < 1e-4, "{min} {max}");
        m.check_invariants().unwrap();
        assert!(m.half_edges().iter().all(|he| he.face.is_some()), "chopped sphere is capped");
    }
}

/// Fins of a bone: thin triangular blades on the sides (±Y), front
/// (+Z) and back (−Z), `size` tall, running from `start_taper` to
/// `1 - end_taper` of the bone length.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoneFinParams {
    pub side: bool,
    pub front: bool,
    pub back: bool,
    pub size: f32,
    pub start_taper: f32,
    pub end_taper: f32,
}

/// A bone: a box from x = 0 to x = `length` whose far end is scaled by
/// `1 - taper`, `width` across Y and `height` across Z, centred on the X axis.
/// Smoothing group 0, material id 1, map channel 1 planar.
pub fn bone_mesh(length: f32, width: f32, height: f32, taper: f32, fins: &BoneFinParams) -> Mesh {
    let l = length.max(1e-3);
    let (w, h) = (width.max(1e-3) * 0.5, height.max(1e-3) * 0.5);
    let k = (1.0 - taper).clamp(0.0, 1.0);
    let base = [Vec3::new(0.0, -w, -h), Vec3::new(0.0, w, -h), Vec3::new(0.0, w, h), Vec3::new(0.0, -w, h)];
    let tip = [Vec3::new(l, -w * k, -h * k), Vec3::new(l, w * k, -h * k), Vec3::new(l, w * k, h * k), Vec3::new(l, -w * k, h * k)];
    let mut polys: Vec<Vec<Vec3>> = vec![
        vec![base[0], base[3], base[2], base[1]], // base cap (faces -X)
        vec![tip[0], tip[1], tip[2], tip[3]],     // tip cap (faces +X)
        vec![base[0], base[1], tip[1], tip[0]],   // -Z
        vec![base[1], base[2], tip[2], tip[1]],   // +Y
        vec![base[2], base[3], tip[3], tip[2]],   // +Z
        vec![base[3], base[0], tip[0], tip[3]],   // -Y
    ];
    let (x0, x1) = (l * fins.start_taper.clamp(0.0, 1.0), l * (1.0 - fins.end_taper.clamp(0.0, 1.0)));
    let fin = |dir: Vec3, half: f32| -> Vec<Vec<Vec3>> {
        // A blade in the plane of `dir` and X: a triangle each side of the bone surface.
        let a = dir * half;
        let peak = dir * (half + fins.size.max(0.0));
        let mid = (x0 + x1) * 0.5;
        let p0 = Vec3::X * x0 + a;
        let p1 = Vec3::X * x1 + a;
        let pk = Vec3::X * mid + peak;
        vec![vec![p0, p1, pk], vec![p1, p0, pk]]
    };
    if x1 > x0 {
        if fins.side {
            polys.extend(fin(Vec3::Y, w));
            polys.extend(fin(Vec3::NEG_Y, w));
        }
        if fins.front {
            polys.extend(fin(Vec3::Z, h));
        }
        if fins.back {
            polys.extend(fin(Vec3::NEG_Z, h));
        }
    }
    let mut mesh = build_merged(&polys);
    crate::uvmap::auto_map(&mut mesh, crate::uvmap::AutoMap::Planar, 1);
    mesh
}

/// A Dummy helper: a cube of `size` centred on the origin.
pub fn dummy_mesh(size: f32) -> Mesh {
    let s = size.max(1e-3);
    let mut m = box_segmented(s, s, s, [1, 1, 1]);
    for v in m.vertices_mut() {
        v.position.z -= s * 0.5;
    }
    m
}

#[cfg(test)]
mod determinism_tests {
    /// Corner data (Edit Normals groups, Unwrap edits) is keyed by half-edge
    /// id: rebuilding a primitive must give the same ids every time.
    #[test]
    fn open_primitives_build_the_same_half_edges_every_time() {
        let a = super::plane_segmented(100.0, 100.0, 30, 30);
        for _ in 0..5 {
            assert_eq!(super::plane_segmented(100.0, 100.0, 30, 30).half_edges(), a.half_edges());
        }
    }
}
