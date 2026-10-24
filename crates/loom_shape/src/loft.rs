//! Loft: cross-section shapes placed along a path at percents, skinned.

use crate::spline::Shape;
use glam::{Mat3, Quat, Vec3};
use loom_mesh::indexed::{IndexedFace, IndexedMesh};
use loom_mesh::Mesh;

/// Skin Parameters rollout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoftParams {
    pub cap_start: bool,
    pub cap_end: bool,
    pub shape_steps: u32,
    pub path_steps: u32,
    pub contour: bool,
    pub flip_normals: bool,
}

impl Default for LoftParams {
    fn default() -> Self {
        Self { cap_start: true, cap_end: true, shape_steps: 5, path_steps: 5, contour: true, flip_normals: false }
    }
}

/// Resamples a polyline to `n` points by arc length (closed: no repeat).
fn resample(pl: &[Vec3], closed: bool, n: usize) -> Vec<Vec3> {
    if pl.len() < 2 || n < 2 {
        return pl.to_vec();
    }
    let mut pts = pl.to_vec();
    if closed {
        pts.push(pl[0]);
    }
    let mut cum = vec![0.0f32];
    for w in pts.windows(2) {
        cum.push(cum.last().unwrap() + w[0].distance(w[1]));
    }
    let total = *cum.last().unwrap();
    (0..n)
        .map(|i| {
            let target = if closed { total * i as f32 / n as f32 } else { total * i as f32 / (n - 1) as f32 };
            let k = cum.iter().position(|c| *c >= target).unwrap_or(cum.len() - 1).max(1);
            let (a, b) = (cum[k - 1], cum[k]);
            let t = if b - a > 1e-9 { (target - a) / (b - a) } else { 0.0 };
            pts[k - 1].lerp(pts[k], t.clamp(0.0, 1.0))
        })
        .collect()
}

/// Lofts `shapes` (each with its path percent 0..100) along the first
/// spline of `path`. Sections use their first spline, in its local XY,
/// placed on the path with the section's X across and Y up (Contour), or
/// kept in world orientation (Contour off).
pub fn loft(path: &Shape, shapes: &[(f32, Shape)], params: LoftParams) -> Result<Mesh, String> {
    let spline = path.splines.first().ok_or("the path has no spline")?;
    let mut sections: Vec<(f32, Vec<Vec3>, bool)> = shapes
        .iter()
        .filter_map(|(pct, s)| {
            let sp = s.splines.first()?;
            let mut shape = s.clone();
            shape.steps = params.shape_steps;
            let pl = shape.polylines().into_iter().next()?;
            (pl.len() >= 2).then_some((pct.clamp(0.0, 100.0), pl, sp.closed))
        })
        .collect();
    if sections.is_empty() {
        return Err("Get Shape: the loft needs a cross-section shape".into());
    }
    sections.sort_by(|a, b| a.0.total_cmp(&b.0));
    let closed_section = sections[0].2;
    // Every section resampled to the same point count.
    let n = sections.iter().map(|s| s.1.len()).max().unwrap().max(3);
    let sections: Vec<(f32, Vec<Vec3>)> = sections.into_iter().map(|(p, pl, c)| (p, resample(&pl, c, n))).collect();
    // Path samples.
    let mut path_shape = path.clone();
    path_shape.steps = params.path_steps;
    let pts = path_shape.polylines().into_iter().next().ok_or("the path has no points")?;
    let m = pts.len();
    if m < 2 {
        return Err("the path needs two points".into());
    }
    let mut cum = vec![0.0f32];
    for w in pts.windows(2) {
        cum.push(cum.last().unwrap() + w[0].distance(w[1]));
    }
    if spline.closed {
        cum.push(cum.last().unwrap() + pts[m - 1].distance(pts[0]));
    }
    let total = cum[m - if spline.closed { 0 } else { 1 }].max(1e-6);
    let tangent = |i: usize| -> Vec3 {
        let prev = if i == 0 { if spline.closed { pts[m - 1] } else { pts[0] } } else { pts[i - 1] };
        let next = if i + 1 == m { if spline.closed { pts[0] } else { pts[m - 1] } } else { pts[i + 1] };
        (next - prev).normalize_or_zero()
    };
    let mut frames: Vec<Quat> = Vec::with_capacity(m);
    let t0 = tangent(0);
    let up = if t0.z.abs() < 0.9 { Vec3::Z } else { Vec3::X };
    let side = up.cross(t0).normalize_or_zero();
    let up2 = t0.cross(side).normalize_or_zero();
    frames.push(Quat::from_mat3(&Mat3::from_cols(side, up2, t0)));
    let mut prev_t = t0;
    for i in 1..m {
        let t = tangent(i);
        let step = if params.contour && t.length_squared() > 0.5 { Quat::from_rotation_arc(prev_t, t) } else { Quat::IDENTITY };
        frames.push((step * frames[i - 1]).normalize());
        if t.length_squared() > 0.5 {
            prev_t = t;
        }
    }
    let section_at = |pct: f32| -> Vec<Vec3> {
        let k = sections.iter().position(|s| s.0 >= pct).unwrap_or(sections.len() - 1);
        if k == 0 || sections[k].0 <= pct {
            return sections[k].1.clone();
        }
        let (p0, s0) = (&sections[k - 1].0, &sections[k - 1].1);
        let (p1, s1) = (&sections[k].0, &sections[k].1);
        let t = ((pct - p0) / (p1 - p0).max(1e-6)).clamp(0.0, 1.0);
        s0.iter().zip(s1).map(|(a, b)| a.lerp(*b, t)).collect()
    };
    let mut im = IndexedMesh::default();
    let base = 0u32;
    for i in 0..m {
        let pct = cum[i] / total * 100.0;
        let sec = section_at(pct);
        for p in &sec {
            im.add_vertex(pts[i] + frames[i] * *p, None);
        }
    }
    let rings = if spline.closed { m } else { m - 1 };
    let edges = if closed_section { n } else { n - 1 };
    for i in 0..rings {
        let (a, b) = (base + (i * n) as u32, base + (((i + 1) % m) * n) as u32);
        for k in 0..edges {
            let k2 = (k + 1) % n;
            im.faces.push(IndexedFace { ids: vec![a + k as u32, b + k as u32, b + k2 as u32, a + k2 as u32], smoothing_groups: 1, material_id: 1, hidden: false, uv: Default::default() });
        }
    }
    if closed_section && !spline.closed {
        // Caps: fans over the resampled ring (sections are simple outlines).
        if params.cap_start {
            let c = im.add_vertex((0..n).map(|k| im.positions[k]).sum::<Vec3>() / n as f32, None);
            for k in 0..n {
                let k2 = (k + 1) % n;
                im.faces.push(IndexedFace { ids: vec![c, k as u32, k2 as u32], smoothing_groups: 2, material_id: 2, hidden: false, uv: Default::default() });
            }
        }
        if params.cap_end {
            let last = ((m - 1) * n) as u32;
            let c = im.add_vertex((0..n).map(|k| im.positions[last as usize + k]).sum::<Vec3>() / n as f32, None);
            for k in 0..n as u32 {
                let k2 = (k + 1) % n as u32;
                im.faces.push(IndexedFace { ids: vec![c, last + k2, last + k], smoothing_groups: 2, material_id: 3, hidden: false, uv: Default::default() });
            }
        }
    }
    // Orient outward (the section's winding is arbitrary), then Flip Normals.
    let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for p in &im.positions {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let c = (lo + hi) * 0.5;
    let mut score = 0.0;
    for f in &im.faces {
        let p: Vec<Vec3> = f.ids.iter().map(|i| im.positions[*i as usize]).collect();
        let mut nrm = Vec3::ZERO;
        for i in 0..p.len() {
            nrm += p[i].cross(p[(i + 1) % p.len()]);
        }
        score += nrm.dot(p.iter().sum::<Vec3>() / p.len() as f32 - c);
    }
    if (score < 0.0) != params.flip_normals {
        for f in &mut im.faces {
            f.ids.reverse();
        }
    }
    im.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parametric::{circle, rectangle};
    use crate::spline::{Knot, Spline};

    #[test]
    fn loft_a_circle_into_a_square_along_a_line() {
        let path = Shape::from_splines(vec![Spline::new(vec![Knot::corner(Vec3::ZERO), Knot::corner(Vec3::Z * 20.0)], false)]);
        let shapes = vec![(0.0, Shape::from_splines(vec![circle(5.0)])), (100.0, Shape::from_splines(vec![rectangle(10.0, 10.0, 0.0)]))];
        let mut params = LoftParams::default();
        params.path_steps = 3;
        let m = loft(&path, &shapes, params).unwrap();
        m.check_invariants().unwrap();
        assert!(loom_mesh::select_ops::border_edges(&m).is_empty(), "capped loft is closed");
        let (lo, hi) = m.aabb();
        assert!((hi.z - 20.0).abs() < 1e-4 && lo.z.abs() < 1e-4);
        assert!((hi.x - 5.0).abs() < 0.3 && (lo.y + 5.0).abs() < 0.3, "{lo:?} {hi:?}");
        // The circle's 24 points set the ring size; a straight path is one ring; two fan caps.
        assert_eq!(m.face_count(), 24 + 2 * 24);
        assert!(loft(&path, &[], params).is_err());
    }
}
