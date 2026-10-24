//! Parametric shapes as splines in the local XY plane (Helix in 3D).

use crate::outline::{fillet_or_chamfer, KAPPA};
use crate::spline::{Knot, Spline};
use glam::Vec3;
use std::f32::consts::TAU;

fn v(x: f32, y: f32) -> Vec3 {
    Vec3::new(x, y, 0.0)
}

/// Rectangle: `length` along Y, `width` along X, optional rounded corners.
pub fn rectangle(length: f32, width: f32, corner_radius: f32) -> Spline {
    let (hw, hl) = (width * 0.5, length * 0.5);
    let r = corner_radius.max(0.0).min(hw).min(hl);
    let mut s = Spline::new(vec![Knot::corner(v(-hw, -hl)), Knot::corner(v(hw, -hl)), Knot::corner(v(hw, hl)), Knot::corner(v(-hw, hl))], true);
    if r > 1e-6 {
        // Fillet each corner; indices shift by one per fillet.
        let mut i = 0;
        for _ in 0..4 {
            fillet_or_chamfer(&mut s, i, r, true);
            i += 2;
        }
    }
    s
}

/// Circle as four Bezier arcs.
pub fn circle(radius: f32) -> Spline {
    ellipse(radius * 2.0, radius * 2.0)
}

/// Ellipse: `length` along Y, `width` along X.
pub fn ellipse(length: f32, width: f32) -> Spline {
    let (a, b) = (width * 0.5, length * 0.5);
    let (ka, kb) = (a * KAPPA, b * KAPPA);
    Spline::new(
        vec![
            Knot::bezier(v(a, 0.0), v(0.0, -kb), v(0.0, kb)),
            Knot::bezier(v(0.0, b), v(ka, 0.0), v(-ka, 0.0)),
            Knot::bezier(v(-a, 0.0), v(0.0, kb), v(0.0, -kb)),
            Knot::bezier(v(0.0, -b), v(-ka, 0.0), v(ka, 0.0)),
        ],
        true,
    )
}

/// Bezier arc knots from angle `from` to `to` (degrees, counter-clockwise).
fn arc_knots(radius: f32, from: f32, to: f32) -> Vec<Knot> {
    let mut to = to;
    while to <= from {
        to += 360.0;
    }
    let sweep = to - from;
    let pieces = ((sweep / 90.0).ceil() as usize).max(1);
    let step = sweep / pieces as f32;
    let k = 4.0 / 3.0 * (step.to_radians() / 4.0).tan() * radius;
    let mut knots = Vec::new();
    for i in 0..=pieces {
        let a = (from + step * i as f32).to_radians();
        let pos = v(a.cos(), a.sin()) * radius;
        let tangent = v(-a.sin(), a.cos());
        let in_vec = if i == 0 { Vec3::ZERO } else { -tangent * k };
        let out_vec = if i == pieces { Vec3::ZERO } else { tangent * k };
        knots.push(Knot::bezier(pos, in_vec, out_vec));
    }
    knots
}

/// Arc: open from `from` to `to` degrees; `pie_slice` closes it through the centre.
pub fn arc(radius: f32, from: f32, to: f32, pie_slice: bool, reverse: bool) -> Spline {
    let mut knots = arc_knots(radius, from, to);
    let closed = pie_slice;
    if pie_slice {
        knots.push(Knot::corner(Vec3::ZERO));
    }
    let mut s = Spline::new(knots, closed);
    if reverse {
        s.reverse();
    }
    s
}

/// Donut: two concentric circles.
pub fn donut(radius1: f32, radius2: f32) -> Vec<Spline> {
    vec![circle(radius1.max(radius2)), circle(radius1.min(radius2))]
}

/// NGon: `inscribed` measures the radius to the edge midpoints, else to the
/// vertices; `circular` makes a Bezier circle through the vertices.
pub fn ngon(radius: f32, inscribed: bool, sides: u32, corner_radius: f32, circular: bool) -> Spline {
    let n = sides.max(3) as usize;
    let r = if inscribed { radius / (std::f32::consts::PI / n as f32).cos() } else { radius };
    let step = TAU / n as f32;
    if circular {
        let k = 4.0 / 3.0 * (step / 4.0).tan() * r;
        let knots = (0..n).map(|i| {
            let a = step * i as f32;
            let t = v(-a.sin(), a.cos());
            Knot::bezier(v(a.cos(), a.sin()) * r, -t * k, t * k)
        }).collect();
        return Spline::new(knots, true);
    }
    let mut s = Spline::new((0..n).map(|i| { let a = step * i as f32; Knot::corner(v(a.cos(), a.sin()) * r) }).collect(), true);
    if corner_radius > 1e-6 {
        let mut i = 0;
        for _ in 0..n {
            fillet_or_chamfer(&mut s, i, corner_radius, true);
            i += 2;
        }
    }
    s
}

/// Star: `points` outer vertices at `radius1`, inner at `radius2`,
/// `distortion` rotates the inner ring (degrees), fillets round the corners.
pub fn star(radius1: f32, radius2: f32, points: u32, distortion: f32, fillet1: f32, fillet2: f32) -> Spline {
    let n = points.max(3) as usize;
    let step = TAU / n as f32;
    let mut knots = Vec::with_capacity(2 * n);
    for i in 0..n {
        let a = step * i as f32;
        knots.push(Knot::corner(v(a.cos(), a.sin()) * radius1));
        let b = a + step * 0.5 + distortion.to_radians();
        knots.push(Knot::corner(v(b.cos(), b.sin()) * radius2));
    }
    let mut s = Spline::new(knots, true);
    if fillet1 > 1e-6 || fillet2 > 1e-6 {
        let mut i = 0;
        for k in 0..2 * n {
            let amount = if k % 2 == 0 { fillet1 } else { fillet2 };
            if amount > 1e-6 && fillet_or_chamfer(&mut s, i, amount, true).is_some() {
                i += 2;
            } else {
                i += 1;
            }
        }
    }
    s
}

/// Helix: smooth knots, 16 per turn, from `radius1` at the bottom to
/// `radius2` at the top; `bias` (-1..1) packs the turns toward one end.
pub fn helix(radius1: f32, radius2: f32, height: f32, turns: f32, bias: f32, cw: bool) -> Spline {
    let per_turn = 16;
    let count = ((turns.max(0.01) * per_turn as f32).ceil() as usize).max(1);
    let e = 3f32.powf(-bias.clamp(-1.0, 1.0));
    let knots = (0..=count).map(|i| {
        let t = (i as f32 / count as f32).min(1.0);
        let a = t * turns * TAU * if cw { -1.0 } else { 1.0 };
        let r = radius1 + (radius2 - radius1) * t;
        Knot::smooth(Vec3::new(a.cos() * r, a.sin() * r, height * t.powf(e)))
    }).collect();
    Spline::new(knots, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Shape;

    #[test]
    fn parametric_shapes_have_the_expected_knots_and_extents() {
        let r = rectangle(10.0, 20.0, 0.0);
        assert_eq!(r.knots.len(), 4);
        let rr = rectangle(10.0, 20.0, 2.0);
        assert_eq!(rr.knots.len(), 8);
        let c = circle(5.0);
        let pts = c.polyline(6, true, false);
        assert_eq!(pts.len(), 28);
        assert!(pts.iter().all(|p| (p.length() - 5.0).abs() < 0.02), "circle radius");
        let a = arc(5.0, 0.0, 90.0, false, false);
        assert_eq!(a.knots.len(), 2);
        assert!(!a.closed);
        assert!(arc(5.0, 0.0, 270.0, true, false).closed);
        assert_eq!(donut(5.0, 3.0).len(), 2);
        assert_eq!(ngon(5.0, false, 6, 0.0, false).knots.len(), 6);
        let in_r = ngon(5.0, true, 6, 0.0, false);
        assert!(in_r.knots[0].pos.length() > 5.0, "inscribed radius reaches the edge midpoints");
        assert_eq!(star(10.0, 5.0, 5, 0.0, 0.0, 0.0).knots.len(), 10);
        assert_eq!(star(10.0, 5.0, 5, 0.0, 1.0, 0.0).knots.len(), 15);
        let h = helix(5.0, 5.0, 20.0, 2.0, 0.0, false);
        assert_eq!(h.knots.len(), 33);
        assert!((h.knots.last().unwrap().pos.z - 20.0).abs() < 1e-5);
        let shape = Shape::from_splines(vec![c]);
        let (lo, hi) = shape.aabb();
        assert!((hi.x - 5.0).abs() < 0.02 && (lo.y + 5.0).abs() < 0.02);
    }
}
