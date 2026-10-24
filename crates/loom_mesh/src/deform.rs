//! Deformation modifiers as pure functions over positions in gizmo space
//! (the gizmo's local frame: the deformation axis is Z unless stated).
//! Each takes the point, the extent of the object along the axis and the
//! modifier parameters, and returns the deformed point.

use crate::indexed::{IndexedFace, IndexedMesh};
use crate::{FaceId, HalfEdgeId, Mesh, VertexId};
use glam::{Mat3, Vec3};
use std::collections::BTreeMap;

/// Limit Effect: `(lower, upper)` along the axis when on.
pub type Limits = Option<(f32, f32)>;

/// Maps the deformation axis to Z: returns (to_z, from_z) rotations.
pub fn axis_frame(axis: u8) -> (Mat3, Mat3) {
    let to_z = match axis {
        0 => Mat3::from_cols(Vec3::Z, Vec3::Y, -Vec3::X).transpose(), // X → Z
        1 => Mat3::from_cols(Vec3::X, Vec3::Z, -Vec3::Y).transpose(), // Y → Z
        _ => Mat3::IDENTITY,
    };
    (to_z, to_z.transpose())
}

/// The part of `z` inside the limits, and the overshoot beyond them.
fn limited(z: f32, limits: Limits) -> (f32, f32) {
    match limits {
        Some((lo, _)) if z < lo => (lo, z - lo),
        Some((_, hi)) if z > hi => (hi, z - hi),
        _ => (z, 0.0),
    }
}

/// Bend: `angle` degrees over the object's `len` along Z, bending toward
/// +X rotated by `direction` degrees about Z. Points beyond the limits
/// continue straight along the bent end.
pub fn bend(p: Vec3, angle_deg: f32, direction_deg: f32, len: f32, limits: Limits) -> Vec3 {
    if angle_deg.abs() < 1e-6 || len <= 1e-9 {
        return p;
    }
    let rot = Mat3::from_rotation_z(-direction_deg.to_radians());
    let q = rot * p;
    let (z, over) = limited(q.z, limits);
    let span = limits.map(|(lo, hi)| (hi - lo).max(1e-6)).unwrap_or(len);
    let k = angle_deg.to_radians() / span; // curvature
    let r = 1.0 / k;
    let c = z * k;
    let (sn, cs) = c.sin_cos();
    let x = r - (r - q.x) * cs + over * sn;
    let zz = (r - q.x) * sn + over * cs;
    rot.transpose() * Vec3::new(x, q.y, zz)
}

/// Taper: scale the X/Y (per `effect`: 0 = XY, 1 = X, 2 = Y) by
/// `1 + amount·t + curve·t·(1−t)` with t the position along Z over `len`.
pub fn taper(p: Vec3, amount: f32, curve: f32, effect: u8, symmetry: bool, len: f32, limits: Limits) -> Vec3 {
    if len <= 1e-9 {
        return p;
    }
    let (z, _) = limited(p.z, limits);
    let (base, span) = limits.map(|(lo, hi)| (lo, (hi - lo).max(1e-6))).unwrap_or((0.0, len));
    let mut t = (z - base) / span;
    if symmetry {
        t = t.abs();
    }
    let f = 1.0 + amount * t + curve * t * (1.0 - t);
    let (fx, fy) = match effect {
        1 => (f, 1.0),
        2 => (1.0, f),
        _ => (f, f),
    };
    Vec3::new(p.x * fx, p.y * fy, p.z)
}

/// Twist: rotate about Z by `angle·t^e` where `e = 2^(bias/50)`.
pub fn twist(p: Vec3, angle_deg: f32, bias: f32, len: f32, limits: Limits) -> Vec3 {
    if len <= 1e-9 {
        return p;
    }
    let (z, _) = limited(p.z, limits);
    let (base, span) = limits.map(|(lo, hi)| (lo, (hi - lo).max(1e-6))).unwrap_or((0.0, len));
    let t = ((z - base) / span).clamp(-1.0, 1.0);
    let e = 2f32.powf(bias / 50.0);
    let a = angle_deg.to_radians() * t.abs().powf(e) * t.signum();
    let (sn, cs) = a.sin_cos();
    Vec3::new(p.x * cs - p.y * sn, p.x * sn + p.y * cs, p.z)
}

/// Stretch: scale Z by `1 + stretch`, the cross section by `1/sqrt(1+stretch)`
/// bulged in the middle by `amplify`.
pub fn stretch(p: Vec3, stretch: f32, amplify: f32, len: f32, limits: Limits) -> Vec3 {
    let s = (1.0 + stretch).max(0.01);
    let (z, _) = limited(p.z, limits);
    let (base, span) = limits.map(|(lo, hi)| (lo, (hi - lo).max(1e-6))).unwrap_or((0.0, len.max(1e-6)));
    let t = ((z - base) / span).clamp(0.0, 1.0);
    let bulge = 4.0 * t * (1.0 - t);
    let r = 1.0 / s.sqrt();
    let f = 1.0 + (r - 1.0) * (1.0 + amplify * bulge);
    Vec3::new(p.x * f, p.y * f, base + (p.z - base) * s)
}

/// Squeeze: Axial Bulge pushes points along Z by their radius, Radial
/// Squeeze pinches the radius by the height.
pub fn squeeze(p: Vec3, axial_amount: f32, axial_curve: f32, radial_amount: f32, radial_curve: f32, len: f32, radius: f32, limits: Limits) -> Vec3 {
    let (z, _) = limited(p.z, limits);
    let rho = (p.x * p.x + p.y * p.y).sqrt();
    let rr = if radius > 1e-9 { (rho / radius).clamp(0.0, 1.0) } else { 0.0 };
    let tz = if len > 1e-9 { (z / len).clamp(-1.0, 1.0) } else { 0.0 };
    let dz = axial_amount * len * (1.0 - rr).powf(axial_curve.max(0.01)) * 0.5;
    let rf = 1.0 - radial_amount * (1.0 - tz.abs()).powf(radial_curve.max(0.01));
    Vec3::new(p.x * rf, p.y * rf, p.z + dz)
}

/// Spherify: blend toward the sphere of `radius` around the centre by `percent`.
pub fn spherify(p: Vec3, radius: f32, percent: f32) -> Vec3 {
    let d = p.normalize_or_zero() * radius;
    p.lerp(d, (percent / 100.0).clamp(0.0, 1.0))
}

/// Wave: Z += A(y)·sin(2π x/λ + φ)·e^(−decay·|x|), A from amp1 to amp2 across Y.
pub fn wave(p: Vec3, amp1: f32, amp2: f32, wavelength: f32, phase: f32, decay: f32, y_extent: f32) -> Vec3 {
    if wavelength.abs() < 1e-9 {
        return p;
    }
    let ty = if y_extent > 1e-9 { ((p.y / y_extent) + 0.5).clamp(0.0, 1.0) } else { 0.5 };
    let a = amp1 + (amp2 - amp1) * ty;
    let s = (std::f32::consts::TAU * p.x / wavelength + phase * std::f32::consts::TAU).sin();
    Vec3::new(p.x, p.y, p.z + a * s * (-decay * p.x.abs()).exp())
}

/// Ripple: Z += A(θ)·sin(2π r/λ + φ)·e^(−decay·r), A = amp1 along X, amp2 along Y.
pub fn ripple(p: Vec3, amp1: f32, amp2: f32, wavelength: f32, phase: f32, decay: f32) -> Vec3 {
    if wavelength.abs() < 1e-9 {
        return p;
    }
    let r = (p.x * p.x + p.y * p.y).sqrt();
    let (c2, s2) = if r > 1e-9 { ((p.x / r).powi(2), (p.y / r).powi(2)) } else { (1.0, 0.0) };
    let a = amp1 * c2 + amp2 * s2;
    let s = (std::f32::consts::TAU * r / wavelength + phase * std::f32::consts::TAU).sin();
    Vec3::new(p.x, p.y, p.z + a * s * (-decay * r).exp())
}

/// Melt: sag along −Z by `amount` and spread outward; `solidity` (0.1 soft
/// … 1 firm) keeps the top, `flip` melts the other way.
pub fn melt(p: Vec3, amount: f32, spread: f32, solidity: f32, flip: bool, zmin: f32, zmax: f32) -> Vec3 {
    let len = (zmax - zmin).max(1e-6);
    let (bottom, z) = if flip { (zmax, zmax - (p.z - zmin) + zmin) } else { (zmin, p.z) };
    let t = ((z - zmin) / len).clamp(0.0, 1.0);
    let sag = (amount / 100.0).clamp(0.0, 1.0) * (1.0 - solidity.clamp(0.0, 1.0) * t);
    let nz = zmin + (z - zmin) * (1.0 - sag).max(0.02);
    let sp = 1.0 + spread / 100.0 * sag * (1.0 - t);
    let out = Vec3::new(p.x * sp, p.y * sp, nz);
    if flip { Vec3::new(out.x, out.y, bottom - (out.z - zmin)) } else { out }
}

/// Skew: shear X (rotated by `direction`) by `amount·t` along Z.
pub fn skew(p: Vec3, amount: f32, direction_deg: f32, len: f32, limits: Limits) -> Vec3 {
    if len <= 1e-9 {
        return p;
    }
    let (z, over) = limited(p.z, limits);
    let (base, span) = limits.map(|(lo, hi)| (lo, (hi - lo).max(1e-6))).unwrap_or((0.0, len));
    let t = (z - base) / span;
    let _ = over;
    let d = direction_deg.to_radians();
    let shift = amount * t;
    Vec3::new(p.x + shift * d.cos(), p.y + shift * d.sin(), p.z)
}

/// Deterministic 3D gradient noise in [-1, 1].
pub fn perlin(p: Vec3, seed: u32) -> f32 {
    fn hash(x: i32, y: i32, z: i32, seed: u32) -> u32 {
        let mut h = (x as u32).wrapping_mul(0x8da6b343) ^ (y as u32).wrapping_mul(0xd8163841) ^ (z as u32).wrapping_mul(0xcb1ab31f) ^ seed.wrapping_mul(0x9e3779b9);
        h ^= h >> 13;
        h = h.wrapping_mul(0x5bd1e995);
        h ^ (h >> 15)
    }
    fn grad(h: u32, d: Vec3) -> f32 {
        const G: [Vec3; 12] = [
            Vec3::new(1.0, 1.0, 0.0), Vec3::new(-1.0, 1.0, 0.0), Vec3::new(1.0, -1.0, 0.0), Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, 0.0, 1.0), Vec3::new(-1.0, 0.0, 1.0), Vec3::new(1.0, 0.0, -1.0), Vec3::new(-1.0, 0.0, -1.0),
            Vec3::new(0.0, 1.0, 1.0), Vec3::new(0.0, -1.0, 1.0), Vec3::new(0.0, 1.0, -1.0), Vec3::new(0.0, -1.0, -1.0),
        ];
        G[(h % 12) as usize].dot(d)
    }
    let f = Vec3::new(p.x.floor(), p.y.floor(), p.z.floor());
    let d = p - f;
    let (x, y, z) = (f.x as i32, f.y as i32, f.z as i32);
    let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let (u, v, w) = (fade(d.x), fade(d.y), fade(d.z));
    let mut acc = 0.0;
    for (dx, dy, dz) in [(0, 0, 0), (1, 0, 0), (0, 1, 0), (1, 1, 0), (0, 0, 1), (1, 0, 1), (0, 1, 1), (1, 1, 1)] {
        let g = grad(hash(x + dx, y + dy, z + dz, seed), d - Vec3::new(dx as f32, dy as f32, dz as f32));
        let wx = if dx == 1 { u } else { 1.0 - u };
        let wy = if dy == 1 { v } else { 1.0 - v };
        let wz = if dz == 1 { w } else { 1.0 - w };
        acc += g * wx * wy * wz;
    }
    acc.clamp(-1.0, 1.0)
}

/// Noise: displace by per-axis `strength` with (fractal) noise of `scale`.
pub fn noise(p: Vec3, seed: u32, scale: f32, fractal: bool, roughness: f32, iterations: u32, strength: Vec3) -> Vec3 {
    let s = scale.max(1e-3);
    let sample = |q: Vec3, off: f32| -> f32 {
        if !fractal {
            return perlin(q / s + Vec3::splat(off), seed);
        }
        let mut amp = 1.0;
        let mut freq = 1.0;
        let mut sum = 0.0;
        let mut norm = 0.0;
        for _ in 0..iterations.clamp(1, 10) {
            sum += perlin(q / s * freq + Vec3::splat(off), seed) * amp;
            norm += amp;
            amp *= roughness.clamp(0.0, 1.0);
            freq *= 2.0;
        }
        sum / norm.max(1e-6)
    };
    p + Vec3::new(sample(p, 0.0) * strength.x, sample(p, 31.7) * strength.y, sample(p, 73.1) * strength.z)
}

/// Mirror: reflect across the plane through the origin (gizmo) with
/// `axis` 0..5 = X, Y, Z, XY, YZ, ZX, offset along the normal; `copy`
/// keeps the original and appends the mirrored half with flipped winding.
pub fn mirror(mesh: &mut Mesh, axis: u8, offset: f32, copy: bool) -> Result<(), String> {
    let m = match axis {
        0 => Vec3::new(-1.0, 1.0, 1.0),
        1 => Vec3::new(1.0, -1.0, 1.0),
        2 => Vec3::new(1.0, 1.0, -1.0),
        3 => Vec3::new(-1.0, -1.0, 1.0),
        4 => Vec3::new(1.0, -1.0, -1.0),
        _ => Vec3::new(-1.0, 1.0, -1.0),
    };
    let shift = match axis {
        0 | 3 | 5 => Vec3::X * offset,
        1 | 4 => Vec3::Y * offset,
        _ => Vec3::Z * offset,
    };
    let flips = (m.x < 0.0) as u8 + (m.y < 0.0) as u8 + (m.z < 0.0) as u8;
    let mut im = IndexedMesh::from_mesh(mesh);
    let base = im.positions.len() as u32;
    if copy {
        for i in 0..base as usize {
            let p = (im.positions[i] - shift) * m + shift;
            let attrs = im.vattrs[i];
            let id = im.add_vertex(p, None);
            im.vattrs[id as usize] = attrs;
        }
        let n = im.faces.len();
        for i in 0..n {
            let f = im.faces[i].clone();
            let ids: Vec<u32> = f.ids.iter().map(|v| v + base).collect();
            if flips % 2 == 1 {
                im.faces.push(IndexedFace::reversed(&f, ids.into_iter().rev().collect()));
            } else {
                im.faces.push(IndexedFace::same_order(&f, ids));
            }
        }
    } else {
        for p in &mut im.positions {
            *p = (*p - shift) * m + shift;
        }
        if flips % 2 == 1 {
            for f in &mut im.faces {
                let rev: Vec<u32> = f.ids.iter().rev().copied().collect();
                f.set_ids(rev);
            }
        }
    }
    *mesh = im.build()?;
    Ok(())
}

/// Symmetry: keep the side of the plane the normal points away from
/// (`flip` swaps), mirror it, and weld the seam within `threshold`.
pub fn symmetry(mesh: &mut Mesh, axis: u8, flip: bool, slice: bool, weld: bool, threshold: f32) -> Result<(), String> {
    let n = match axis { 0 => Vec3::X, 1 => Vec3::Y, _ => Vec3::Z } * if flip { -1.0 } else { 1.0 };
    if slice {
        // Cut at the plane and drop the positive side.
        let _ = crate::geom_ops::slice(mesh, Vec3::ZERO, n, None, false)?;
        let drop: Vec<FaceId> = (0..mesh.face_count() as u32)
            .map(FaceId)
            .filter(|f| {
                let pts = mesh.face_positions(*f).unwrap_or_default();
                let c = pts.iter().sum::<Vec3>() / pts.len().max(1) as f32;
                c.dot(n) > 1e-5
            })
            .collect();
        if !drop.is_empty() {
            crate::edit_ops::delete_faces(mesh, &drop)?;
        }
    }
    let mirror_axis = match axis { 0 => 0, 1 => 1, _ => 2 };
    mirror(mesh, mirror_axis, 0.0, true)?;
    if weld {
        let seam: Vec<VertexId> = (0..mesh.vertex_count() as u32).map(VertexId).filter(|v| mesh.vertex_position(*v).is_some_and(|p| p.dot(n).abs() <= threshold)).collect();
        if seam.len() >= 2 {
            crate::topo_ops::weld_vertices(mesh, &seam, threshold)?;
        }
    }
    Ok(())
}

/// Slice modifier: `mode` 0 = Refine Mesh, 1 = Split Mesh, 2 = Remove Top, 3 = Remove Bottom.
pub fn slice_modifier(mesh: &mut Mesh, origin: Vec3, normal: Vec3, mode: u8) -> Result<(), String> {
    let n = normal.normalize_or_zero();
    crate::geom_ops::slice(mesh, origin, n, None, mode == 1)?;
    if mode >= 2 {
        let drop: Vec<FaceId> = (0..mesh.face_count() as u32)
            .map(FaceId)
            .filter(|f| {
                let pts = mesh.face_positions(*f).unwrap_or_default();
                let c = pts.iter().sum::<Vec3>() / pts.len().max(1) as f32;
                let d = (c - origin).dot(n);
                if mode == 2 { d > 1e-5 } else { d < -1e-5 }
            })
            .collect();
        if !drop.is_empty() {
            crate::edit_ops::delete_faces(mesh, &drop)?;
        }
    }
    Ok(())
}

/// Lattice: struts (cylinders along the edges) and joints (octahedral
/// spheres at the vertices). `struts` / `joints` select what to build.
pub fn lattice(mesh: &Mesh, struts: bool, joints: bool, strut_radius: f32, strut_sides: u32, joint_radius: f32, joint_segments: u32) -> Result<Mesh, String> {
    let mut out = IndexedMesh::default();
    let sides = strut_sides.clamp(3, 32) as usize;
    if struts {
        let hes = mesh.half_edges();
        for (i, _) in hes.iter().enumerate() {
            let e = HalfEdgeId(i as u32);
            if mesh.canonical_edge_id(e) != Some(e) {
                continue;
            }
            let Some((a, b)) = mesh.edge_vertices(e) else { continue };
            let (pa, pb) = (mesh.vertex_position(a).unwrap(), mesh.vertex_position(b).unwrap());
            let axis = (pb - pa).normalize_or_zero();
            if axis.length_squared() < 0.5 {
                continue;
            }
            let helper = if axis.z.abs() < 0.9 { Vec3::Z } else { Vec3::X };
            let u = helper.cross(axis).normalize();
            let v = axis.cross(u);
            let base = out.positions.len() as u32;
            for end in [pa, pb] {
                for k in 0..sides {
                    let t = k as f32 / sides as f32 * std::f32::consts::TAU;
                    out.add_vertex(end + (u * t.cos() + v * t.sin()) * strut_radius, None);
                }
            }
            for k in 0..sides as u32 {
                let k1 = (k + 1) % sides as u32;
                out.faces.push(IndexedFace { ids: vec![base + k, base + k1, base + sides as u32 + k1, base + sides as u32 + k], smoothing_groups: 1, material_id: 0, hidden: false, uv: Default::default() });
            }
            // Caps.
            out.faces.push(IndexedFace { ids: (0..sides as u32).rev().map(|k| base + k).collect(), smoothing_groups: 2, material_id: 0, hidden: false, uv: Default::default() });
            out.faces.push(IndexedFace { ids: (0..sides as u32).map(|k| base + sides as u32 + k).collect(), smoothing_groups: 2, material_id: 0, hidden: false, uv: Default::default() });
        }
    }
    if joints {
        let segs = joint_segments.clamp(1, 4);
        for v in mesh.vertices() {
            let sphere = crate::standard_primitives::geosphere_primitive(joint_radius, segs, crate::standard_primitives::GeoBase::Octa);
            let sim = IndexedMesh::from_mesh(&sphere);
            let base = out.positions.len() as u32;
            for p in &sim.positions {
                out.add_vertex(*p + v.position, None);
            }
            for f in &sim.faces {
                out.faces.push(IndexedFace { ids: f.ids.iter().map(|i| i + base).collect(), smoothing_groups: 1, material_id: 1, hidden: false, uv: Default::default() });
            }
        }
    }
    if out.faces.is_empty() {
        return Ok(Mesh::default());
    }
    out.build()
}

/// Preserve: relax toward the edge lengths of `original` (same topology),
/// `iterations` steps of strength `weight`.
pub fn preserve(mesh: &mut Mesh, original: &Mesh, iterations: u32, weight: f32) -> Result<(), String> {
    if original.vertex_count() != mesh.vertex_count() {
        return Err("Preserve: the original has a different topology".into());
    }
    let mut im = IndexedMesh::from_mesh(mesh);
    let edges: Vec<(usize, usize, f32)> = original
        .half_edges()
        .iter()
        .enumerate()
        .filter_map(|(i, _)| {
            let e = HalfEdgeId(i as u32);
            if original.canonical_edge_id(e) != Some(e) {
                return None;
            }
            let (a, b) = original.edge_vertices(e)?;
            Some((a.0 as usize, b.0 as usize, original.vertex_position(a)?.distance(original.vertex_position(b)?)))
        })
        .collect();
    for _ in 0..iterations.clamp(1, 100) {
        let mut delta: BTreeMap<usize, (Vec3, f32)> = BTreeMap::new();
        for (a, b, len) in &edges {
            let (pa, pb) = (im.positions[*a], im.positions[*b]);
            let d = pb - pa;
            let cur = d.length().max(1e-6);
            let corr = d / cur * (cur - len) * 0.5;
            let ea = delta.entry(*a).or_insert((Vec3::ZERO, 0.0));
            ea.0 += corr;
            ea.1 += 1.0;
            let eb = delta.entry(*b).or_insert((Vec3::ZERO, 0.0));
            eb.0 -= corr;
            eb.1 += 1.0;
        }
        for (i, (d, n)) in delta {
            im.positions[i] += d / n * weight.clamp(0.0, 1.0);
        }
    }
    *mesh = im.build()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::standard_primitives::box_segmented;

    #[test]
    fn bend_90_turns_the_top_sideways() {
        // A 10-tall line bent 90° over its length ends up level, radius 10/(π/2).
        let top = bend(Vec3::new(0.0, 0.0, 10.0), 90.0, 0.0, 10.0, None);
        let r = 10.0 / std::f32::consts::FRAC_PI_2;
        assert!((top.x - r).abs() < 1e-3 && (top.z - r).abs() < 1e-3, "{top:?}");
        assert_eq!(bend(Vec3::new(1.0, 2.0, 0.0), 90.0, 0.0, 10.0, None), Vec3::new(1.0, 2.0, 0.0), "the base stays");
        // Beyond an upper limit the geometry continues straight.
        let beyond = bend(Vec3::new(0.0, 0.0, 15.0), 90.0, 0.0, 10.0, Some((0.0, 10.0)));
        assert!((beyond.x - (r + 5.0)).abs() < 1e-3 && (beyond.z - r).abs() < 1e-3, "{beyond:?}");
    }

    #[test]
    fn taper_and_twist_scale_by_height() {
        let p = taper(Vec3::new(2.0, 2.0, 10.0), 0.5, 0.0, 0, false, 10.0, None);
        assert!((p.x - 3.0).abs() < 1e-5 && (p.y - 3.0).abs() < 1e-5);
        let q = taper(Vec3::new(2.0, 2.0, 10.0), 0.5, 0.0, 1, false, 10.0, None);
        assert!((q.x - 3.0).abs() < 1e-5 && (q.y - 2.0).abs() < 1e-5, "Effect X only");
        let t = twist(Vec3::new(1.0, 0.0, 10.0), 90.0, 0.0, 10.0, None);
        assert!(t.x.abs() < 1e-5 && (t.y - 1.0).abs() < 1e-5, "{t:?}");
        let half = twist(Vec3::new(1.0, 0.0, 5.0), 90.0, 0.0, 10.0, None);
        assert!((half.x - half.y).abs() < 1e-5, "45° at half height");
    }

    #[test]
    fn symmetry_welds_the_seam_and_mirror_copies() {
        let mut m = box_segmented(10.0, 10.0, 10.0, [2, 1, 1]);
        symmetry(&mut m, 0, false, true, true, 0.01).unwrap();
        m.check_invariants().unwrap();
        // The slice cuts 6 faces (bottom 2, top 2, ±Y) → 16; the kept half has 8, mirrored 16.
        assert_eq!(m.face_count(), 16, "{}", m.face_count());
        let (lo, hi) = m.aabb();
        assert!((lo.x + 5.0).abs() < 1e-4 && (hi.x - 5.0).abs() < 1e-4);
        assert_eq!(m.vertex_count(), 18, "12 + 12 with the 6 seam vertices welded");
        let mut c = box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
        mirror(&mut c, 0, 20.0, true).unwrap();
        assert_eq!(c.face_count(), 12);
        c.check_invariants().unwrap();
        assert!(c.face_normal(FaceId(6)).length() > 0.9);
    }

    #[test]
    fn slice_modes_and_spherify_and_noise() {
        let mut m = box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
        slice_modifier(&mut m, Vec3::new(0.0, 0.0, 5.0), Vec3::Z, 0).unwrap();
        assert_eq!(m.face_count(), 10);
        let mut t = box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
        slice_modifier(&mut t, Vec3::new(0.0, 0.0, 5.0), Vec3::Z, 2).unwrap();
        assert_eq!(t.face_count(), 5, "Remove Top leaves the open bottom half");
        let (_, hi) = t.aabb();
        assert!((hi.z - 5.0).abs() < 1e-4);
        let s = spherify(Vec3::new(5.0, 5.0, 5.0), 5.0, 100.0);
        assert!((s.length() - 5.0).abs() < 1e-4);
        let a = noise(Vec3::new(1.0, 2.0, 3.0), 7, 10.0, true, 0.5, 4, Vec3::splat(2.0));
        let b = noise(Vec3::new(1.0, 2.0, 3.0), 7, 10.0, true, 0.5, 4, Vec3::splat(2.0));
        assert_eq!(a, b, "deterministic");
        assert!((a - Vec3::new(1.0, 2.0, 3.0)).length() > 1e-4 || perlin(Vec3::new(0.1, 0.2, 0.3), 7).abs() > 0.0);
    }

    #[test]
    fn lattice_and_preserve() {
        let m = box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
        let l = lattice(&m, true, true, 0.5, 4, 1.0, 1).unwrap();
        l.check_invariants().unwrap();
        assert!(l.face_count() >= 12 * 6 + 8 * 8);
        let mut stretched = box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
        let mut im = IndexedMesh::from_mesh(&stretched);
        for p in &mut im.positions {
            p.z *= 2.0;
        }
        stretched = im.build().unwrap();
        preserve(&mut stretched, &m, 50, 0.5).unwrap();
        let (lo, hi) = stretched.aabb();
        assert!((hi.z - lo.z) < 18.0, "edge lengths pulled back toward 10: {}", hi.z - lo.z);
    }
}
