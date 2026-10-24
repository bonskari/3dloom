//! 2D spline Boolean (Geometry rollout > Boolean: Union, Subtraction,
//! Intersection) on the interpolated outlines in the XY plane
//! (Greiner–Hormann clipping). The results are corner-knot splines.

use crate::spline::{Knot, Spline};
use glam::{Vec2, Vec3};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolOp {
    Union,
    Subtraction,
    Intersection,
}

fn signed_area(p: &[Vec2]) -> f32 {
    let n = p.len();
    (0..n).map(|i| p[i].perp_dot(p[(i + 1) % n])).sum::<f32>() * 0.5
}

fn point_in_polygon(pt: Vec2, poly: &[Vec2]) -> bool {
    let n = poly.len();
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > pt.y) != (b.y > pt.y) && pt.x < (b.x - a.x) * (pt.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[derive(Clone)]
struct V {
    p: Vec2,
    intersect: bool,
    entry: bool,
    /// Index of the matching intersection vertex in the other list.
    link: usize,
    visited: bool,
}

fn segment_intersection(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> Option<(f32, f32)> {
    let r = b - a;
    let s = d - c;
    let denom = r.perp_dot(s);
    if denom.abs() < 1e-9 {
        return None;
    }
    let t = (c - a).perp_dot(s) / denom;
    let u = (c - a).perp_dot(r) / denom;
    ((1e-6..1.0 - 1e-6).contains(&t) && (1e-6..1.0 - 1e-6).contains(&u)).then_some((t, u))
}

/// Builds the two linked vertex lists with intersections inserted.
#[allow(clippy::needless_range_loop)]
fn build(a: &[Vec2], b: &[Vec2]) -> (Vec<V>, Vec<V>, usize) {
    let mut la: Vec<Vec<(f32, Vec2)>> = vec![Vec::new(); a.len()];
    let mut lb: Vec<Vec<(f32, Vec2)>> = vec![Vec::new(); b.len()];
    let mut count = 0;
    for i in 0..a.len() {
        for j in 0..b.len() {
            let (p0, p1) = (a[i], a[(i + 1) % a.len()]);
            let (q0, q1) = (b[j], b[(j + 1) % b.len()]);
            if let Some((t, u)) = segment_intersection(p0, p1, q0, q1) {
                let x = p0 + (p1 - p0) * t;
                la[i].push((t, x));
                lb[j].push((u, x));
                count += 1;
            }
        }
    }
    let make = |poly: &[Vec2], ins: &[Vec<(f32, Vec2)>]| -> Vec<V> {
        let mut out = Vec::new();
        for i in 0..poly.len() {
            out.push(V { p: poly[i], intersect: false, entry: false, link: usize::MAX, visited: false });
            let mut xs = ins[i].clone();
            xs.sort_by(|x, y| x.0.total_cmp(&y.0));
            for (_, x) in xs {
                out.push(V { p: x, intersect: true, entry: false, link: usize::MAX, visited: false });
            }
        }
        out
    };
    let mut va = make(a, &la);
    let mut vb = make(b, &lb);
    // Link intersections by position.
    for i in 0..va.len() {
        if !va[i].intersect {
            continue;
        }
        let at = va[i].p;
        if let Some(j) = (0..vb.len()).find(|j| vb[*j].intersect && vb[*j].link == usize::MAX && vb[*j].p.distance(at) < 1e-4) {
            va[i].link = j;
            vb[j].link = i;
        }
    }
    (va, vb, count)
}

fn mark_entries(list: &mut [V], other: &[Vec2], start_inside_flip: bool) {
    let mut inside = point_in_polygon(list[0].p, other);
    if start_inside_flip {
        inside = !inside;
    }
    for v in list.iter_mut() {
        if v.intersect {
            v.entry = !inside;
            inside = !inside;
        }
    }
}

/// Clips polygon `a` against `b`. Both are simple, non-self-intersecting.
pub fn clip(a: &[Vec2], b: &[Vec2], op: BoolOp) -> Vec<Vec<Vec2>> {
    if a.len() < 3 || b.len() < 3 {
        return Vec::new();
    }
    let mut a: Vec<Vec2> = a.to_vec();
    let mut b: Vec<Vec2> = b.to_vec();
    if signed_area(&a) < 0.0 {
        a.reverse();
    }
    if signed_area(&b) < 0.0 {
        b.reverse();
    }
    let (mut va, mut vb, count) = build(&a, &b);
    if count == 0 {
        let a_in_b = point_in_polygon(a[0], &b);
        let b_in_a = point_in_polygon(b[0], &a);
        return match op {
            BoolOp::Union => {
                if a_in_b { vec![b] } else if b_in_a { vec![a] } else { vec![a, b] }
            }
            BoolOp::Intersection => {
                if a_in_b { vec![a] } else if b_in_a { vec![b] } else { Vec::new() }
            }
            BoolOp::Subtraction => {
                if a_in_b {
                    Vec::new()
                } else if b_in_a {
                    let mut hole = b;
                    hole.reverse();
                    vec![a, hole]
                } else {
                    vec![a]
                }
            }
        };
    }
    // Entry/exit marking per Greiner–Hormann; subtraction reverses b's sense.
    let (flip_a, flip_b) = match op {
        BoolOp::Intersection => (false, false),
        BoolOp::Union => (true, true),
        BoolOp::Subtraction => (true, false),
    };
    mark_entries(&mut va, &b, flip_a);
    mark_entries(&mut vb, &a, flip_b);
    let mut results = Vec::new();
    while let Some(start) = (0..va.len()).find(|i| va[*i].intersect && !va[*i].visited) {
        let mut poly = Vec::new();
        let mut in_a = true;
        let mut idx = start;
        let mut guard = 0;
        loop {
            guard += 1;
            if guard > va.len() + vb.len() + 4 {
                break;
            }
            let list: &mut Vec<V> = if in_a { &mut va } else { &mut vb };
            list[idx].visited = true;
            poly.push(list[idx].p);
            let forward = list[idx].entry;
            // Walk to the next intersection in this list.
            let n = list.len();
            loop {
                idx = if forward { (idx + 1) % n } else { (idx + n - 1) % n };
                if list[idx].intersect {
                    break;
                }
                poly.push(list[idx].p);
            }
            list[idx].visited = true;
            let link = list[idx].link;
            if link == usize::MAX {
                break;
            }
            in_a = !in_a;
            idx = link;
            let list2: &mut Vec<V> = if in_a { &mut va } else { &mut vb };
            if list2[idx].visited || (in_a && idx == start) {
                list2[idx].visited = true;
                break;
            }
        }
        if poly.len() >= 3 {
            results.push(poly);
        }
    }
    results
}

/// Boolean of two closed splines of a shape: `a` keeps the result, `b` goes.
pub fn boolean(a: &Spline, b: &Spline, op: BoolOp, steps: u32) -> Vec<Spline> {
    let pa: Vec<Vec2> = a.polyline(steps, true, false).iter().map(|p| p.truncate()).collect();
    let pb: Vec<Vec2> = b.polyline(steps, true, false).iter().map(|p| p.truncate()).collect();
    clip(&pa, &pb, op)
        .into_iter()
        .map(|poly| Spline::new(poly.into_iter().map(|p| Knot::corner(Vec3::new(p.x, p.y, 0.0))).collect(), true))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<Vec2> {
        vec![Vec2::new(x0, y0), Vec2::new(x1, y0), Vec2::new(x1, y1), Vec2::new(x0, y1)]
    }

    fn area(p: &[Vec2]) -> f32 {
        signed_area(p).abs()
    }

    #[test]
    fn overlapping_rectangles_union_subtract_intersect() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(5.0, 5.0, 15.0, 15.0);
        let u = clip(&a, &b, BoolOp::Union);
        assert_eq!(u.len(), 1);
        assert!((area(&u[0]) - 175.0).abs() < 1e-3, "{}", area(&u[0]));
        let i = clip(&a, &b, BoolOp::Intersection);
        assert_eq!(i.len(), 1);
        assert!((area(&i[0]) - 25.0).abs() < 1e-3, "{}", area(&i[0]));
        let s = clip(&a, &b, BoolOp::Subtraction);
        assert_eq!(s.len(), 1);
        assert!((area(&s[0]) - 75.0).abs() < 1e-3, "{}", area(&s[0]));
        // Disjoint and nested cases.
        let c = rect(20.0, 20.0, 30.0, 30.0);
        assert_eq!(clip(&a, &c, BoolOp::Union).len(), 2);
        assert!(clip(&a, &c, BoolOp::Intersection).is_empty());
        let inner = rect(2.0, 2.0, 4.0, 4.0);
        let hole = clip(&a, &inner, BoolOp::Subtraction);
        assert_eq!(hole.len(), 2);
        assert!(signed_area(&hole[1]) < 0.0, "the hole runs clockwise");
        let sq = Spline::new(vec![Knot::corner(Vec3::ZERO), Knot::corner(Vec3::X * 10.0), Knot::corner(Vec3::new(10.0, 10.0, 0.0)), Knot::corner(Vec3::Y * 10.0)], true);
        let mut sq2 = sq.clone();
        sq2.translate(Vec3::new(5.0, 5.0, 0.0));
        let out = boolean(&sq, &sq2, BoolOp::Union, 6);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].knots.len(), 8);
    }
}
