//! Outline, Fillet, Chamfer, Weld and Connect on splines (Geometry rollout).

use crate::interp;
use crate::spline::{Knot, KnotKind, Shape, Spline};
use glam::Vec3;

/// Kappa for a quarter-circle Bezier arc.
pub const KAPPA: f32 = 0.552_284_8;

/// Direction a spline leaves knot `i` (first-segment tangent) and arrives (last-segment tangent).
fn tangents_at(s: &Spline, i: usize) -> (Option<Vec3>, Option<Vec3>) {
    let n = s.knots.len();
    let incoming = if i == 0 && !s.closed { None } else { s.segment((i + n - 1) % n).map(|b| interp::tangent_at(&b, 1.0)) };
    let outgoing = if i + 1 == n && !s.closed { None } else { s.segment(i).map(|b| interp::tangent_at(&b, 0.0)) };
    (incoming, outgoing)
}

/// Offset every knot of a planar (XY) spline by `amount` to the right of
/// travel (outward for a counter-clockwise loop; negative = left), mitring
/// at the corners.
pub fn offset_knots(s: &Spline, amount: f32) -> Spline {
    let mut out = s.clone();
    for i in 0..s.knots.len() {
        let (tin, tout) = tangents_at(s, i);
        let right = |t: Vec3| Vec3::new(t.y, -t.x, 0.0);
        let d = match (tin, tout) {
            (Some(a), Some(b)) => {
                let (na, nb) = (right(a), right(b));
                let m = na + nb;
                let denom = 1.0 + na.dot(nb);
                if denom.abs() < 1e-4 { na } else { m / denom }
            }
            (Some(a), None) => right(a),
            (None, Some(b)) => right(b),
            (None, None) => Vec3::ZERO,
        };
        out.knots[i].pos += d * amount;
    }
    out
}

/// Outline: a closed spline gains an offset copy (as a second spline); an
/// open spline becomes one closed spline through both sides. `center`
/// splits the offset to both sides of the original.
pub fn outline(shape: &mut Shape, spline: usize, amount: f32, center: bool) {
    let Some(s) = shape.splines.get(spline).cloned() else { return };
    if center {
        let a = offset_knots(&s, amount * 0.5);
        let b = offset_knots(&s, -amount * 0.5);
        if s.closed {
            shape.splines[spline] = a;
            shape.splines.push(b);
        } else {
            shape.splines[spline] = join_sides(a, b);
        }
        return;
    }
    let off = offset_knots(&s, amount);
    if s.closed {
        shape.splines.push(off);
    } else {
        shape.splines[spline] = join_sides(s, off);
    }
}

fn join_sides(a: Spline, mut b: Spline) -> Spline {
    b.reverse();
    let mut knots = a.knots;
    for k in &mut knots {
        if k.kind == KnotKind::Smooth {
            k.kind = KnotKind::Corner;
        }
    }
    let n = knots.len();
    if n > 0 {
        knots[n - 1].kind = KnotKind::Corner;
        knots[n - 1].out_vec = Vec3::ZERO;
    }
    for mut k in b.knots {
        if k.kind == KnotKind::Smooth {
            k.kind = KnotKind::Corner;
        }
        knots.push(k);
    }
    if let Some(k) = knots.get_mut(n) {
        k.kind = KnotKind::Corner;
        k.in_vec = Vec3::ZERO;
    }
    if let Some(last) = knots.last_mut() {
        last.kind = KnotKind::Corner;
        last.out_vec = Vec3::ZERO;
    }
    if let Some(first) = knots.first_mut() {
        first.kind = KnotKind::Corner;
        first.in_vec = Vec3::ZERO;
    }
    Spline::new(knots, true)
}

/// Fillet (round) or chamfer (cut) corner `i` by `amount` along both
/// segments. Returns the two knots that replace it.
pub fn fillet_or_chamfer(s: &mut Spline, i: usize, amount: f32, fillet: bool) -> Option<(usize, usize)> {
    let n = s.knots.len();
    if i >= n || amount <= 0.0 {
        return None;
    }
    let prev_i = if s.closed { (i + n - 1) % n } else { i.checked_sub(1)? };
    let next_i = if s.closed { (i + 1) % n } else { (i + 1 < n).then_some(i + 1)? };
    let prev_seg = s.segment(prev_i)?;
    let next_seg = s.segment(i)?;
    let corner = s.knots[i].pos;
    // Points `amount` back along the incoming segment and forward along the outgoing one.
    let a_len = interp::length(&prev_seg, 32);
    let c_len = interp::length(&next_seg, 32);
    let amount = amount.min(a_len * 0.5).min(c_len * 0.5);
    let ta = 1.0 - interp::t_at_length(&{ let mut r = prev_seg; r.reverse(); r }, amount);
    let tc = interp::t_at_length(&next_seg, amount);
    let (pa_l, _pa_r) = interp::split_at(&prev_seg, ta);
    let (_pc_l, pc_r) = interp::split_at(&next_seg, tc);
    let a = pa_l[3];
    let c = pc_r[0];
    let (ka, kc) = if fillet {
        let f = KAPPA;
        (Knot::bezier(a, pa_l[2] - a, (corner - a) * f), Knot::bezier(c, (corner - c) * f, pc_r[1] - c))
    } else {
        (Knot::bezier(a, pa_l[2] - a, Vec3::ZERO), Knot::bezier(c, Vec3::ZERO, pc_r[1] - c))
    };
    // Neighbours keep their curvature with the split control points.
    s.knots[prev_i].out_vec = pa_l[1] - pa_l[0];
    if s.knots[prev_i].kind != KnotKind::Corner || !interp::is_straight(&prev_seg) {
        s.knots[prev_i].kind = KnotKind::Bezier;
    }
    s.knots[next_i].in_vec = pc_r[2] - pc_r[3];
    if s.knots[next_i].kind != KnotKind::Corner || !interp::is_straight(&next_seg) {
        s.knots[next_i].kind = KnotKind::Bezier;
    }
    let straight = interp::is_straight(&prev_seg) && interp::is_straight(&next_seg);
    let (ka, kc) = if straight && !fillet { (Knot::corner(a), Knot::corner(c)) } else { (ka, kc) };
    s.knots[i] = ka;
    s.knots.insert(i + 1, kc);
    Some((i, i + 1))
}

/// Weld: the two end knots of open splines within `threshold` join. Ends of
/// the same spline close it; ends of two splines merge them into one.
pub fn weld(shape: &mut Shape, threshold: f32) -> usize {
    let mut welded = 0;
    // Same spline: first and last.
    for s in &mut shape.splines {
        if !s.closed && s.knots.len() >= 3 {
            let (f, l) = (s.knots[0].pos, s.knots[s.knots.len() - 1].pos);
            if f.distance(l) <= threshold {
                let mid = (f + l) * 0.5;
                s.knots[0].pos = mid;
                s.knots.pop();
                s.closed = true;
                welded += 1;
            }
        }
    }
    // Different splines.
    let mut i = 0;
    while i < shape.splines.len() {
        let mut merged = false;
        if !shape.splines[i].closed {
            for j in (i + 1)..shape.splines.len() {
                if shape.splines[j].closed {
                    continue;
                }
                let a = shape.splines[i].clone();
                let b = shape.splines[j].clone();
                let ends = [(a.knots[a.knots.len() - 1].pos, b.knots[0].pos, false, false), (a.knots[a.knots.len() - 1].pos, b.knots[b.knots.len() - 1].pos, false, true), (a.knots[0].pos, b.knots[0].pos, true, false), (a.knots[0].pos, b.knots[b.knots.len() - 1].pos, true, true)];
                if let Some((_, _, rev_a, rev_b)) = ends.iter().find(|(p, q, _, _)| p.distance(*q) <= threshold) {
                    let mut a = a;
                    let mut b = b;
                    if *rev_a {
                        a.reverse();
                    }
                    if *rev_b {
                        b.reverse();
                    }
                    let joint = (a.knots[a.knots.len() - 1].pos + b.knots[0].pos) * 0.5;
                    a.knots.pop();
                    let mut first = b.knots.remove(0);
                    first.pos = joint;
                    a.knots.push(first);
                    a.knots.extend(b.knots);
                    shape.splines[i] = a;
                    shape.splines.remove(j);
                    welded += 1;
                    merged = true;
                    break;
                }
            }
        }
        if !merged {
            i += 1;
        }
    }
    welded
}

/// Connect: a straight segment from an end knot of one spline to an end knot
/// of another (or the two ends of the same spline, closing it).
pub fn connect(shape: &mut Shape, a: (usize, usize), b: (usize, usize)) -> bool {
    if a.0 == b.0 {
        if let Some(s) = shape.splines.get_mut(a.0) {
            let n = s.knots.len();
            if !s.closed && n >= 2 && ((a.1 == 0 && b.1 == n - 1) || (a.1 == n - 1 && b.1 == 0)) {
                s.closed = true;
                return true;
            }
        }
        return false;
    }
    let (ia, ib) = (a.0.min(b.0), a.0.max(b.0));
    let (ka, kb) = if a.0 < b.0 { (a.1, b.1) } else { (b.1, a.1) };
    let mut sa = shape.splines[ia].clone();
    let mut sb = shape.splines[ib].clone();
    if sa.closed || sb.closed {
        return false;
    }
    let end = |s: &Spline, k: usize| k == 0 || k + 1 == s.knots.len();
    if !end(&sa, ka) || !end(&sb, kb) {
        return false;
    }
    if ka == 0 {
        sa.reverse();
    }
    if kb != 0 {
        sb.reverse();
    }
    sa.knots.extend(sb.knots);
    shape.splines[ia] = sa;
    shape.splines.remove(ib);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Spline {
        Spline::new(vec![Knot::corner(Vec3::ZERO), Knot::corner(Vec3::X * 10.0), Knot::corner(Vec3::new(10.0, 10.0, 0.0)), Knot::corner(Vec3::Y * 10.0)], true)
    }

    #[test]
    fn outline_of_a_square_adds_an_offset_square() {
        let mut shape = Shape::from_splines(vec![square()]);
        outline(&mut shape, 0, 2.0, false);
        assert_eq!(shape.splines.len(), 2);
        let off = &shape.splines[1];
        assert_eq!(off.knots.len(), 4);
        // Counter-clockwise square: a positive offset (to the right of travel) grows it.
        assert!((off.knots[0].pos - Vec3::new(-2.0, -2.0, 0.0)).length() < 1e-4, "{:?}", off.knots[0].pos);
        assert!((off.knots[2].pos - Vec3::new(12.0, 12.0, 0.0)).length() < 1e-4);
        let open = Spline::new(vec![Knot::corner(Vec3::ZERO), Knot::corner(Vec3::X * 10.0)], false);
        let mut s2 = Shape::from_splines(vec![open]);
        outline(&mut s2, 0, 1.0, false);
        assert_eq!(s2.splines.len(), 1);
        assert!(s2.splines[0].closed && s2.splines[0].knots.len() == 4);
    }

    #[test]
    fn fillet_and_chamfer_replace_a_corner_with_two_knots() {
        let mut s = square();
        fillet_or_chamfer(&mut s, 1, 2.0, false).unwrap();
        assert_eq!(s.knots.len(), 5);
        assert!((s.knots[1].pos - Vec3::new(8.0, 0.0, 0.0)).length() < 1e-3, "{:?}", s.knots[1].pos);
        assert!((s.knots[2].pos - Vec3::new(10.0, 2.0, 0.0)).length() < 1e-3, "{:?}", s.knots[2].pos);
        assert!(s.is_straight(1), "a chamfer is a straight cut");
        let mut f = square();
        fillet_or_chamfer(&mut f, 1, 2.0, true).unwrap();
        assert_eq!(f.knots.len(), 5);
        assert!(!f.is_straight(1), "a fillet is an arc");
        let mid = interp::point_at(&f.segment(1).unwrap(), 0.5);
        assert!(mid.x > 8.5 && mid.y < 1.5 && mid.x < 10.0, "{mid:?}");
    }

    #[test]
    fn weld_closes_and_connect_joins() {
        let mut shape = Shape::from_splines(vec![Spline::new(vec![Knot::corner(Vec3::ZERO), Knot::corner(Vec3::X * 10.0), Knot::corner(Vec3::Y * 10.0), Knot::corner(Vec3::new(0.0, 0.05, 0.0))], false)]);
        assert_eq!(weld(&mut shape, 0.1), 1);
        assert!(shape.splines[0].closed && shape.splines[0].knots.len() == 3);
        let mut two = Shape::from_splines(vec![
            Spline::new(vec![Knot::corner(Vec3::ZERO), Knot::corner(Vec3::X * 10.0)], false),
            Spline::new(vec![Knot::corner(Vec3::new(20.0, 5.0, 0.0)), Knot::corner(Vec3::new(10.0, 0.0, 0.0))], false),
        ]);
        assert!(connect(&mut two, (0, 1), (1, 1)));
        assert_eq!(two.splines.len(), 1);
        assert_eq!(two.splines[0].knots.len(), 4);
    }
}
