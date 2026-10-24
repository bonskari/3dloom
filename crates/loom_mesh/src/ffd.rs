//! Free-Form Deformation (FFD 2x2x2 / 3x3x3 / 4x4x4 / Box / Cyl): a lattice
//! of control points fitted to the object. Every vertex moves by the
//! weighted sum of its control points' displacements from their rest
//! positions; box lattices weight with Bernstein polynomials (a Bézier
//! volume, identity at rest), cylindrical ones go round the axis
//! periodically. Only In Volume / All Vertices and Falloff apply.

use glam::Vec3;

/// Lattice dimensions: points along X, Y, Z (Cyl: around, radial, height).
pub type Dims = [u32; 3];

pub fn index(dims: Dims, i: usize, j: usize, k: usize) -> usize {
    let [nx, ny, _] = dims.map(|d| d.max(2) as usize);
    i + j * nx + k * nx * ny
}

/// Rest lattice in the unit cube, index = x + y·nx + z·nx·ny. A
/// cylindrical lattice puts its `nx` "around" points on rings about the
/// cube's Z axis (the innermost ring collapses onto the axis).
pub fn rest_points(dims: Dims, cyl: bool) -> Vec<Vec3> {
    let [nx, ny, nz] = dims.map(|d| d.max(2) as usize);
    let mut out = Vec::with_capacity(nx * ny * nz);
    for k in 0..nz {
        for j in 0..ny {
            for i in 0..nx {
                let u = if cyl {
                    unit_to_cyl(Vec3::new(i as f32 / nx as f32, j as f32 / (ny - 1) as f32, k as f32 / (nz - 1) as f32))
                } else {
                    Vec3::new(i as f32 / (nx - 1) as f32, j as f32 / (ny - 1) as f32, k as f32 / (nz - 1) as f32)
                };
                out.push(u);
            }
        }
    }
    out
}

fn binomial(n: usize, k: usize) -> f32 {
    let mut r = 1.0f32;
    for t in 0..k {
        r *= (n - t) as f32 / (t + 1) as f32;
    }
    r
}

/// Bernstein weights of degree `n - 1` at `t` (extrapolates outside 0..1).
fn bernstein(n: usize, t: f32) -> Vec<f32> {
    let d = n - 1;
    (0..n).map(|i| binomial(d, i) * t.powi(i as i32) * (1.0 - t).powi((d - i) as i32)).collect()
}

/// Periodic hat weights around a ring of `n` points at angles i/n.
fn periodic(n: usize, theta: f32) -> Vec<f32> {
    let t = theta.rem_euclid(1.0) * n as f32;
    let i0 = (t.floor() as usize) % n;
    let f = t - t.floor();
    let mut w = vec![0.0; n];
    w[i0] += 1.0 - f;
    w[(i0 + 1) % n] += f;
    w
}

/// Unit-cube point → (angle 0..1, radius 0..1 at the cube wall, height).
pub fn cyl_to_unit(p: Vec3) -> Vec3 {
    let (dx, dy) = (p.x - 0.5, p.y - 0.5);
    let r = (dx * dx + dy * dy).sqrt() * 2.0;
    let theta = (dy.atan2(dx) / std::f32::consts::TAU).rem_euclid(1.0);
    Vec3::new(theta, r, p.z)
}

pub fn unit_to_cyl(q: Vec3) -> Vec3 {
    let theta = q.x * std::f32::consts::TAU;
    let r = q.y * 0.5;
    Vec3::new(0.5 + r * theta.cos(), 0.5 + r * theta.sin(), q.z)
}

/// Weight of every control point for a unit-space point.
pub fn weights(dims: Dims, cyl: bool, u: Vec3) -> Vec<f32> {
    let [nx, ny, nz] = dims.map(|d| d.max(2) as usize);
    let q = if cyl { cyl_to_unit(u) } else { u };
    let bx = if cyl { periodic(nx, q.x) } else { bernstein(nx, q.x) };
    let (by, bz) = (bernstein(ny, q.y), bernstein(nz, q.z));
    let mut out = vec![0.0; nx * ny * nz];
    for k in 0..nz {
        for j in 0..ny {
            for i in 0..nx {
                out[index(dims, i, j, k)] = bx[i] * by[j] * bz[k];
            }
        }
    }
    out
}

/// Whether a unit-space point lies inside the lattice volume.
pub fn inside(cyl: bool, u: Vec3) -> bool {
    let z_in = (0.0..=1.0).contains(&u.z);
    if cyl {
        z_in && cyl_to_unit(u).y <= 1.0
    } else {
        z_in && (0.0..=1.0).contains(&u.x) && (0.0..=1.0).contains(&u.y)
    }
}

/// Distance (in cube units) of a unit-space point from the lattice volume.
fn distance_out(cyl: bool, u: Vec3) -> f32 {
    let out = |t: f32| if t < 0.0 { -t } else if t > 1.0 { t - 1.0 } else { 0.0 };
    if cyl {
        let q = cyl_to_unit(u);
        Vec3::new((q.y - 1.0).max(0.0) * 0.5, 0.0, out(u.z)).length()
    } else {
        Vec3::new(out(u.x), out(u.y), out(u.z)).length()
    }
}

/// Deforms a unit-space point: `points` and `rest` are the control points
/// (unit space, same indexing). Outside the volume: `all` (All Vertices)
/// extrapolates, else the point is left alone (Only In Volume); with
/// `falloff` > 0 the effect fades to nothing `falloff` cube units away.
pub fn deform(u: Vec3, dims: Dims, cyl: bool, points: &[Vec3], rest: &[Vec3], all: bool, falloff: f32) -> Vec3 {
    let n = dims.iter().map(|d| d.max(&2).to_owned() as usize).product::<usize>();
    if points.len() != n || rest.len() != n {
        return u;
    }
    let is_in = inside(cyl, u);
    if !is_in && !all {
        return u;
    }
    let w = weights(dims, cyl, u);
    let mut d = Vec3::ZERO;
    for i in 0..n {
        if w[i] != 0.0 {
            d += (points[i] - rest[i]) * w[i];
        }
    }
    if !is_in && falloff > 0.0 {
        d *= (1.0 - distance_out(cyl, u) / falloff).clamp(0.0, 1.0);
    }
    u + d
}

/// Lattice edges (pairs of point indices) for drawing: neighbours along
/// each dimension; a cylindrical ring closes on itself.
pub fn lattice_edges(dims: Dims, cyl: bool) -> Vec<(usize, usize)> {
    let [nx, ny, nz] = dims.map(|d| d.max(2) as usize);
    let mut out = Vec::new();
    for k in 0..nz {
        for j in 0..ny {
            for i in 0..nx {
                let a = index(dims, i, j, k);
                if i + 1 < nx {
                    out.push((a, index(dims, i + 1, j, k)));
                } else if cyl {
                    out.push((a, index(dims, 0, j, k)));
                }
                if j + 1 < ny {
                    out.push((a, index(dims, i, j + 1, k)));
                }
                if k + 1 < nz {
                    out.push((a, index(dims, i, j, k + 1)));
                }
            }
        }
    }
    out
}

/// Conform To Shape: each control point moves toward the lattice centre
/// until it meets the surface (the nearest mesh vertex within `tolerance`
/// of that ray), then back out by `offset`. Points that hit nothing stay.
pub fn conform_to_shape(points: &mut [Vec3], mesh_unit: &[Vec3], offset: f32, tolerance: f32) {
    let centre = Vec3::splat(0.5);
    for p in points.iter_mut() {
        let dir = (centre - *p).normalize_or_zero();
        if dir.length_squared() < 0.5 {
            continue;
        }
        let mut best: Option<(f32, Vec3)> = None;
        for v in mesh_unit {
            let t = (*v - *p).dot(dir);
            if t < 0.0 {
                continue;
            }
            let off = (*v - *p - dir * t).length();
            if off <= tolerance && best.is_none_or(|(bt, _)| t < bt) {
                best = Some((t, *v));
            }
        }
        if let Some((_, v)) = best {
            *p = v - dir * offset;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rest_lattice_is_identity() {
        for dims in [[2, 2, 2], [3, 3, 3], [4, 4, 4], [5, 3, 2]] {
            for cyl in [false, true] {
                let pts = rest_points(dims, cyl);
                for u in [Vec3::new(0.2, 0.7, 0.4), Vec3::ZERO, Vec3::ONE, Vec3::new(0.5, 0.5, 0.5)] {
                    let d = deform(u, dims, cyl, &pts, &pts, true, 0.0);
                    assert!((d - u).length() < 1e-5, "{dims:?} cyl={cyl} {u:?} → {d:?}");
                }
            }
        }
    }

    #[test]
    fn moving_a_corner_moves_near_points_more() {
        let dims = [3, 3, 3];
        let rest = rest_points(dims, false);
        let mut pts = rest.clone();
        pts[index(dims, 2, 2, 2)] += Vec3::new(0.0, 0.0, 0.5);
        let near = deform(Vec3::new(0.9, 0.9, 0.9), dims, false, &pts, &rest, false, 0.0);
        let far = deform(Vec3::new(0.1, 0.1, 0.1), dims, false, &pts, &rest, false, 0.0);
        assert!(near.z - 0.9 > 0.2, "{near:?}");
        assert!((far.z - 0.1).abs() < 1e-3, "{far:?}");
        // Only In Volume leaves outside points alone; All Vertices moves them.
        let out = Vec3::new(1.2, 1.2, 1.2);
        assert_eq!(deform(out, dims, false, &pts, &rest, false, 0.0), out);
        assert!(deform(out, dims, false, &pts, &rest, true, 0.0) != out);
        // Falloff fades the outside effect to nothing one cube unit away.
        let faded = deform(Vec3::new(1.0, 1.0, 2.5), dims, false, &pts, &rest, true, 1.0);
        assert!((faded - Vec3::new(1.0, 1.0, 2.5)).length() < 1e-5);
    }

    #[test]
    fn cylinder_rings_close_and_weight_their_neighbours() {
        let dims = [8, 2, 2];
        let rest = rest_points(dims, true);
        // The innermost ring sits on the axis, the outer on the cube wall.
        assert!((rest[index(dims, 0, 0, 0)] - Vec3::new(0.5, 0.5, 0.0)).length() < 1e-6);
        assert!((rest[index(dims, 0, 1, 0)] - Vec3::new(1.0, 0.5, 0.0)).length() < 1e-6);
        let mut pts = rest.clone();
        pts[index(dims, 0, 1, 0)] += Vec3::new(0.3, 0.0, 0.0);
        // A point just past the seam (angle 0.95) still feels ring point 0.
        let u = unit_to_cyl(Vec3::new(0.95, 1.0, 0.0));
        let d = deform(u, dims, true, &pts, &rest, false, 0.0);
        assert!(d.x - u.x > 0.1, "{d:?} vs {u:?}");
        let p = Vec3::new(0.8, 0.5, 0.3);
        assert!((unit_to_cyl(cyl_to_unit(p)) - p).length() < 1e-5);
        assert_eq!(lattice_edges([2, 2, 2], false).len(), 12);
        assert_eq!(lattice_edges([4, 4, 4], false).len(), 3 * 16 * 3);
        assert_eq!(lattice_edges([8, 2, 2], true).len(), 8 * 4 + 8 * 2 + 8 * 2);
    }

    #[test]
    fn conform_moves_points_onto_the_shape() {
        let dims = [2, 2, 2];
        let mut pts = rest_points(dims, false);
        let mut surf = Vec::new();
        for i in 0..200 {
            let t = i as f32 / 200.0 * std::f32::consts::TAU;
            for z in [-0.3f32, 0.0, 0.3] {
                let r = (0.09 - z * z).max(0.0).sqrt();
                surf.push(Vec3::new(0.5 + r * t.cos(), 0.5 + r * t.sin(), 0.5 + z));
            }
        }
        conform_to_shape(&mut pts, &surf, 0.0, 0.2);
        assert!(pts.iter().all(|p| (*p - Vec3::splat(0.5)).length() < 0.45), "{pts:?}");
    }
}
