//! Camera Match: the camera (position, orientation, horizontal FOV) that
//! sees known scene points at given pixels of an image, found by
//! Levenberg-Marquardt from several starting views around the points.

use glam::{Mat4, Quat, Vec2, Vec3};

/// The fewest points a match needs.
pub const MIN_POINTS: usize = 5;

/// A solved camera.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Match {
    /// The camera's world matrix (looking down local -Z, local +Y up).
    pub world: Mat4,
    /// Horizontal field of view, degrees.
    pub fov: f32,
    /// Current Camera Error: the root mean square distance, in pixels,
    /// between the assigned pixels and where the camera sees the points.
    pub error: f32,
}

/// Where a camera with world matrix `world` and horizontal FOV `fov`
/// (degrees) sees `p` in a `size` image (pixels, y down); none behind it.
pub fn project(world: Mat4, fov: f32, size: Vec2, p: Vec3) -> Option<Vec2> {
    let q = world.inverse().transform_point3(p);
    if q.z >= -1e-6 {
        return None;
    }
    let half_w = (fov.to_radians() * 0.5).tan();
    let half_h = half_w * size.y / size.x;
    let (x, y) = (q.x / -q.z / half_w, q.y / -q.z / half_h);
    Some(Vec2::new((x + 1.0) * 0.5 * size.x, (1.0 - y) * 0.5 * size.y))
}

/// Unknowns: position, rotation vector, and ln(tan(fov / 2)).
type Params = [f64; 7];

fn camera(p: &Params) -> (Mat4, f32) {
    let r = Vec3::new(p[3] as f32, p[4] as f32, p[5] as f32);
    let rot = if r.length() > 1e-9 { Quat::from_axis_angle(r.normalize(), r.length()) } else { Quat::IDENTITY };
    let fov = 2.0 * (p[6].exp().atan() as f32).to_degrees();
    (Mat4::from_rotation_translation(rot, Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32)), fov)
}

fn residuals(p: &Params, points: &[(Vec3, Vec2)], size: Vec2) -> Vec<f64> {
    let (world, fov) = camera(p);
    let mut r = Vec::with_capacity(points.len() * 2);
    for (w, px) in points {
        match project(world, fov, size, *w) {
            Some(s) => r.extend([(s.x - px.x) as f64, (s.y - px.y) as f64]),
            // Behind the camera: far off, so the solver turns away from it.
            None => r.extend([1e4, 1e4]),
        }
    }
    r
}

fn cost(r: &[f64]) -> f64 {
    r.iter().map(|x| x * x).sum()
}

/// Solves the 6 or 7 unknowns (the FOV stays with `freeze_fov`) by
/// Levenberg-Marquardt from `p`, at most `iterations` steps.
fn refine(mut p: Params, points: &[(Vec3, Vec2)], size: Vec2, iterations: u32, freeze_fov: bool) -> (Params, f64) {
    let n = if freeze_fov { 6 } else { 7 };
    let mut r = residuals(&p, points, size);
    let mut c = cost(&r);
    let mut lambda = 1e-3;
    for _ in 0..iterations {
        // Numerical Jacobian.
        let mut jac = vec![vec![0.0; r.len()]; n];
        for (k, col) in jac.iter_mut().enumerate() {
            let h = 1e-4 * p[k].abs().max(1.0) * if k < 3 { 1.0 } else { 0.1 };
            let mut q = p;
            q[k] += h;
            let rq = residuals(&q, points, size);
            for (i, v) in col.iter_mut().enumerate() {
                *v = (rq[i] - r[i]) / h;
            }
        }
        // Normal equations (J^T J + lambda diag) d = -J^T r.
        let mut a = vec![vec![0.0; n]; n];
        let mut g = vec![0.0; n];
        for (i, (row, gi)) in a.iter_mut().zip(g.iter_mut()).enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = jac[i].iter().zip(&jac[j]).map(|(x, y)| x * y).sum();
            }
            *gi = -jac[i].iter().zip(&r).map(|(x, y)| x * y).sum::<f64>();
        }
        let mut improved = false;
        for _ in 0..10 {
            let mut m = a.clone();
            for (i, row) in m.iter_mut().enumerate() {
                row[i] += lambda * a[i][i].max(1e-9);
            }
            let Some(d) = solve(m, g.clone()) else {
                lambda *= 10.0;
                continue;
            };
            let mut q = p;
            for (v, dv) in q.iter_mut().zip(&d) {
                *v += dv;
            }
            let rq = residuals(&q, points, size);
            let cq = cost(&rq);
            if cq < c {
                let done = (c - cq) < 1e-12 * c.max(1e-12);
                (p, r, c) = (q, rq, cq);
                lambda = (lambda * 0.3).max(1e-12);
                improved = !done;
                break;
            }
            lambda *= 10.0;
        }
        if !improved {
            break;
        }
    }
    (p, c)
}

/// Gaussian elimination with partial pivoting.
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let piv = (col..n).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[piv][col].abs() < 1e-14 {
            return None;
        }
        a.swap(col, piv);
        b.swap(col, piv);
        let pivot = a[col].clone();
        for row in col + 1..n {
            let f = a[row][col] / pivot[col];
            for (v, pv) in a[row].iter_mut().zip(&pivot).skip(col) {
                *v -= f * pv;
            }
            b[row] -= f * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let s: f64 = (i + 1..n).map(|k| a[i][k] * x[k]).sum();
        x[i] = (b[i] - s) / a[i][i];
    }
    Some(x)
}

fn params_of(world: Mat4, fov: f32) -> Params {
    let (_, rot, pos) = world.to_scale_rotation_translation();
    let (axis, angle) = rot.to_axis_angle();
    let r = axis * angle;
    let t = ((fov.to_radians() * 0.5).tan() as f64).ln();
    [pos.x as f64, pos.y as f64, pos.z as f64, r.x as f64, r.y as f64, r.z as f64, t]
}

/// The camera that sees each scene point at its pixel of a `size` image.
/// `start` (an existing camera's world matrix and FOV) is tried first;
/// `freeze_fov` keeps its FOV (or 45 without one). Needs `MIN_POINTS`.
pub fn solve_camera(points: &[(Vec3, Vec2)], size: Vec2, start: Option<(Mat4, f32)>, iterations: u32, freeze_fov: bool) -> Result<Match, String> {
    if points.len() < MIN_POINTS {
        return Err(format!("Camera Match needs at least {MIN_POINTS} CamPoints with screen coordinates ({} assigned)", points.len()));
    }
    let fov0 = start.map_or(45.0, |s| s.1);
    let centre = points.iter().map(|p| p.0).sum::<Vec3>() / points.len() as f32;
    let extent = points.iter().map(|p| p.0.distance(centre)).fold(0.0f32, f32::max).max(1e-3);
    let mut starts: Vec<Params> = start.iter().map(|(w, f)| params_of(*w, *f)).collect();
    // Views from all around the points, looking at their centre.
    for i in 0..27 {
        let d = Vec3::new((i % 3) as f32 - 1.0, ((i / 3) % 3) as f32 - 1.0, (i / 9) as f32 - 1.0).normalize_or_zero();
        if d == Vec3::ZERO {
            continue;
        }
        for k in [2.0, 5.0] {
            let eye = centre + d * extent * k;
            let up = if d.z.abs() > 0.99 { Vec3::Y } else { Vec3::Z };
            let world = Mat4::look_at_rh(eye, centre, up).inverse();
            starts.push(params_of(world, fov0));
        }
    }
    let quick = iterations.min(40);
    let best = starts
        .into_iter()
        .map(|s| refine(s, points, size, quick, freeze_fov))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("starting views");
    let (p, c) = refine(best.0, points, size, iterations, freeze_fov);
    let (world, fov) = camera(&p);
    Ok(Match { world, fov, error: (c / points.len() as f64).sqrt() as f32 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene_points() -> Vec<Vec3> {
        vec![Vec3::ZERO, Vec3::new(100.0, 0.0, 0.0), Vec3::new(0.0, 80.0, 0.0), Vec3::new(100.0, 80.0, 0.0), Vec3::new(0.0, 0.0, 60.0), Vec3::new(100.0, 0.0, 60.0), Vec3::new(50.0, 80.0, 40.0)]
    }

    /// The pixels a known camera sees give that camera back, error ~0.
    #[test]
    fn finds_the_camera_that_took_the_picture() {
        let size = Vec2::new(640.0, 480.0);
        let truth = Mat4::look_at_rh(Vec3::new(-150.0, -220.0, 130.0), Vec3::new(40.0, 30.0, 20.0), Vec3::Z).inverse();
        let fov = 50.0;
        let points: Vec<(Vec3, Vec2)> = scene_points().into_iter().map(|p| (p, project(truth, fov, size, p).unwrap())).collect();
        let m = solve_camera(&points, size, None, 500, false).unwrap();
        assert!(m.error < 0.05, "error {}", m.error);
        let eye = m.world.w_axis.truncate();
        assert!(eye.distance(truth.w_axis.truncate()) < 0.5, "eye {eye}");
        assert!((m.fov - fov).abs() < 0.1, "fov {}", m.fov);
        // Freeze FOV keeps a given FOV.
        let start = Mat4::look_at_rh(Vec3::new(0.0, -300.0, 100.0), Vec3::ZERO, Vec3::Z).inverse();
        let m = solve_camera(&points, size, Some((start, 50.0)), 500, true).unwrap();
        assert_eq!(m.fov, 50.0);
        assert!(m.error < 0.05, "frozen error {}", m.error);
        // Too few points.
        assert!(solve_camera(&points[..4], size, None, 500, false).is_err());
    }
}
