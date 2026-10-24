//! Spline crossings and the tools built on them: Trim, Extend, Cross
//! Insert, Cross Section and Surface (spline cage → patches). Crossings
//! are found in the shape's local XY plane (Construction Plane
//! projection), on the interpolated polylines.

use crate::interp::{self, Bezier};
use crate::spline::{Knot, KnotKind, Shape, Spline};
use glam::{Vec2, Vec3};
use loom_mesh::indexed::{IndexedFace, IndexedMesh};
use loom_mesh::Mesh;

const SAMPLES: usize = 24;
const EPS: f32 = 1e-4;

/// Polyline samples of a spline: (param = segment + t, point).
fn samples(sp: &Spline) -> Vec<(f32, Vec3)> {
    let mut out = Vec::new();
    for i in 0..sp.segment_count() {
        let b = sp.segment(i).unwrap();
        let n = if interp::is_straight(&b) { 1 } else { SAMPLES };
        for k in 0..n {
            let t = k as f32 / n as f32;
            out.push((i as f32 + t, interp::point_at(&b, t)));
        }
    }
    if let Some(last) = sp.knots.last() {
        if sp.closed {
            if let Some(first) = sp.knots.first() {
                out.push((sp.segment_count() as f32, first.pos));
            }
        } else {
            out.push((sp.segment_count() as f32, last.pos));
        }
    }
    out
}

fn xy(p: Vec3) -> Vec2 {
    Vec2::new(p.x, p.y)
}

/// Intersection of 2D segments a0-a1 and b0-b1: (ta, tb).
fn seg_hit(a0: Vec2, a1: Vec2, b0: Vec2, b1: Vec2) -> Option<(f32, f32)> {
    let r = a1 - a0;
    let s = b1 - b0;
    let den = r.perp_dot(s);
    if den.abs() < 1e-12 {
        return None;
    }
    let d = b0 - a0;
    let ta = d.perp_dot(s) / den;
    let tb = d.perp_dot(r) / den;
    ((-EPS..=1.0 + EPS).contains(&ta) && (-EPS..=1.0 + EPS).contains(&tb)).then_some((ta.clamp(0.0, 1.0), tb.clamp(0.0, 1.0)))
}

/// A crossing: (spline a, param on a, spline b, param on b, point on a).
pub type Crossing = (usize, f32, usize, f32, Vec3);

fn at_open_end(sp: &Spline, param: f32) -> bool {
    !sp.closed && (param < EPS || param > sp.segment_count() as f32 - EPS)
}

/// Every place two different pieces of the shape cross. Contact where both
/// sides end (two open splines meeting end to end) is not a crossing.
pub fn crossings(shape: &Shape) -> Vec<Crossing> {
    let all: Vec<Vec<(f32, Vec3)>> = shape.splines.iter().map(samples).collect();
    let mut out: Vec<Crossing> = Vec::new();
    for a in 0..all.len() {
        for b in a..all.len() {
            for i in 0..all[a].len().saturating_sub(1) {
                for j in 0..all[b].len().saturating_sub(1) {
                    // Neighbouring edges of one spline always touch.
                    if a == b && (j <= i + 1 || (shape.splines[a].closed && i == 0 && j == all[a].len() - 2)) {
                        continue;
                    }
                    let (pa0, pa1) = (all[a][i], all[a][i + 1]);
                    let (pb0, pb1) = (all[b][j], all[b][j + 1]);
                    let Some((ta, tb)) = seg_hit(xy(pa0.1), xy(pa1.1), xy(pb0.1), xy(pb1.1)) else { continue };
                    let qa = pa0.0 + (pa1.0 - pa0.0) * ta;
                    let qb = pb0.0 + (pb1.0 - pb0.0) * tb;
                    if at_open_end(&shape.splines[a], qa) && at_open_end(&shape.splines[b], qb) {
                        continue;
                    }
                    let p = pa0.1.lerp(pa1.1, ta);
                    if out.iter().any(|c| c.0 == a && c.2 == b && (c.4 - p).length() < 1e-3) {
                        continue;
                    }
                    out.push((a, qa, b, qb, p));
                }
            }
        }
    }
    out
}

/// The spline and param nearest to `p` (in XY).
pub fn nearest(shape: &Shape, p: Vec3) -> Option<(usize, f32, f32)> {
    let mut best: Option<(usize, f32, f32)> = None;
    for (si, sp) in shape.splines.iter().enumerate() {
        let s = samples(sp);
        for w in s.windows(2) {
            let (a, b) = (xy(w[0].1), xy(w[1].1));
            let ab = b - a;
            let t = if ab.length_squared() < 1e-12 { 0.0 } else { ((xy(p) - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) };
            let d = (a + ab * t).distance(xy(p));
            if best.is_none_or(|bb| d < bb.2) {
                best = Some((si, w[0].0 + (w[1].0 - w[0].0) * t, d));
            }
        }
    }
    best
}

/// The Bezier of segment `i` cut to its part between t0 and t1.
fn piece(b: &Bezier, t0: f32, t1: f32) -> Bezier {
    if t1 <= t0 + 1e-6 {
        let p = interp::point_at(b, t0);
        return [p; 4];
    }
    let (_, right) = interp::split_at(b, t0);
    let local = (t1 - t0) / (1.0 - t0).max(1e-6);
    interp::split_at(&right, local.clamp(0.0, 1.0)).0
}

/// The part of a spline between params `a` and `b` (a < b; on a closed
/// spline `b` may run past the end and wrap).
fn sub_spline(sp: &Spline, a: f32, b: f32) -> Spline {
    let n = sp.segment_count();
    let mut beziers: Vec<Bezier> = Vec::new();
    let mut t = a;
    while t < b - 1e-5 {
        let seg = (t.floor() as usize).min(n.saturating_sub(1).max(0));
        let seg_end = (seg + 1) as f32;
        let end = b.min(seg_end);
        let bz = sp.segment(seg % n.max(1)).unwrap();
        beziers.push(piece(&bz, t - seg as f32, end - seg as f32));
        t = end;
        if (t - seg_end).abs() < 1e-6 && t >= n as f32 && b > t {
            // Wrapping on a closed spline: continue from the start.
            return {
                let mut first = sub_spline(sp, a, n as f32);
                let rest = sub_spline(sp, 0.0, b - n as f32);
                first.knots.pop();
                first.knots.extend(rest.knots);
                first
            };
        }
    }
    let mut knots: Vec<Knot> = Vec::new();
    for (i, bz) in beziers.iter().enumerate() {
        let in_vec = if i == 0 { Vec3::ZERO } else { beziers[i - 1][2] - beziers[i - 1][3] };
        knots.push(Knot { pos: bz[0], in_vec, out_vec: bz[1] - bz[0], kind: KnotKind::BezierCorner });
    }
    if let Some(last) = beziers.last() {
        knots.push(Knot { pos: last[3], in_vec: last[2] - last[3], out_vec: Vec3::ZERO, kind: KnotKind::BezierCorner });
    }
    Spline::new(knots, false)
}

/// Trim: the piece of the nearest spline around `pick`, between the
/// crossings on each side, is removed. False when that spline crosses
/// nothing (nothing to trim to).
pub fn trim(shape: &mut Shape, pick: Vec3) -> bool {
    let Some((si, pp, _)) = nearest(shape, pick) else { return false };
    let mut hits: Vec<f32> = crossings(shape).into_iter().flat_map(|(a, qa, b, qb, _)| {
        let mut v = Vec::new();
        if a == si {
            v.push(qa);
        }
        if b == si {
            v.push(qb);
        }
        v
    }).collect();
    let sp = shape.splines[si].clone();
    let n = sp.segment_count() as f32;
    hits.retain(|h| !at_open_end(&sp, *h));
    hits.sort_by(f32::total_cmp);
    hits.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
    if hits.is_empty() || (sp.closed && hits.len() < 2) {
        return false;
    }
    let lower = hits.iter().copied().filter(|h| *h < pp).fold(None, |m: Option<f32>, h| Some(m.map_or(h, |m| m.max(h))));
    let upper = hits.iter().copied().filter(|h| *h > pp).fold(None, |m: Option<f32>, h| Some(m.map_or(h, |m| m.min(h))));
    let mut pieces = Vec::new();
    if sp.closed {
        let lo = lower.unwrap_or(*hits.last().unwrap());
        let hi = upper.unwrap_or(hits[0]);
        let end = if lo <= hi { lo + n } else { lo };
        pieces.push(sub_spline(&sp, hi, end));
    } else {
        if let Some(lo) = lower {
            pieces.push(sub_spline(&sp, 0.0, lo));
        }
        if let Some(hi) = upper {
            pieces.push(sub_spline(&sp, hi, n));
        }
    }
    pieces.retain(|p| p.knots.len() >= 2);
    shape.splines.remove(si);
    for (k, p) in pieces.into_iter().enumerate() {
        shape.splines.insert(si + k, p);
    }
    shape.binds.clear();
    true
}

/// Extend: the open end of the nearest spline nearest to `pick` grows along
/// its end tangent to the first piece of the shape it meets (a new corner
/// knot there). False when nothing lies ahead.
pub fn extend(shape: &mut Shape, pick: Vec3) -> bool {
    let Some((si, pp, _)) = nearest(shape, pick) else { return false };
    let sp = shape.splines[si].clone();
    if sp.closed || sp.knots.len() < 2 {
        return false;
    }
    let n = sp.segment_count();
    let at_end = pp > n as f32 / 2.0;
    let (origin, dir) = if at_end {
        let b = sp.segment(n - 1).unwrap();
        (b[3], interp::tangent_at(&b, 1.0))
    } else {
        let b = sp.segment(0).unwrap();
        (b[0], -interp::tangent_at(&b, 0.0))
    };
    let (o, d) = (xy(origin), xy(dir).normalize_or_zero());
    if d == Vec2::ZERO {
        return false;
    }
    let far = o + d * 1.0e6;
    let mut best: Option<(f32, Vec3)> = None;
    for (sj, other) in shape.splines.iter().enumerate() {
        let s = samples(other);
        for w in s.windows(2) {
            if sj == si {
                // Not back onto the end segment itself.
                let end_param = if at_end { n as f32 } else { 0.0 };
                if (w[0].0 - end_param).abs() <= 1.0 + 1e-4 && (w[1].0 - end_param).abs() <= 1.0 + 1e-4 {
                    continue;
                }
            }
            if let Some((ta, tb)) = seg_hit(o, far, xy(w[0].1), xy(w[1].1)) {
                let dist = ta * 1.0e6;
                if dist > 1e-3 && best.is_none_or(|b| dist < b.0) {
                    best = Some((dist, w[0].1.lerp(w[1].1, tb)));
                }
            }
        }
    }
    let Some((_, hit)) = best else { return false };
    let z = origin.z + dir.z * 0.0;
    let k = Knot::corner(Vec3::new(hit.x, hit.y, z));
    if at_end {
        shape.splines[si].knots.push(k);
    } else {
        shape.splines[si].knots.insert(0, k);
    }
    shape.binds.clear();
    true
}

/// Cross Insert: a knot on both splines wherever two splines cross within
/// `threshold` in height (only the one nearest `near`, when given).
/// Returns how many crossings got knots.
pub fn cross_insert(shape: &mut Shape, threshold: f32, near: Option<Vec3>) -> usize {
    let mut hits: Vec<Crossing> = crossings(shape).into_iter().filter(|(a, qa, b, qb, _)| {
        let pa = point_at_param(&shape.splines[*a], *qa);
        let pb = point_at_param(&shape.splines[*b], *qb);
        (pa.z - pb.z).abs() <= threshold.max(0.0) + 1e-5
    }).collect();
    if let Some(p) = near {
        // The clicked crossing only.
        hits.sort_by(|x, y| xy(x.4).distance(xy(p)).total_cmp(&xy(y.4).distance(xy(p))));
        hits.truncate(1);
    }
    // Per spline, params descending so earlier inserts do not shift later ones.
    let mut by_spline: std::collections::BTreeMap<usize, Vec<f32>> = Default::default();
    for (a, qa, b, qb, _) in &hits {
        by_spline.entry(*a).or_default().push(*qa);
        by_spline.entry(*b).or_default().push(*qb);
    }
    for (si, mut params) in by_spline {
        params.sort_by(|x, y| y.total_cmp(x));
        params.dedup_by(|x, y| (*x - *y).abs() < 1e-4);
        for q in params {
            let seg = q.floor() as usize;
            let t = q - seg as f32;
            if t < 1e-3 || t > 1.0 - 1e-3 {
                continue;
            }
            shape.splines[si].refine(seg, t);
        }
    }
    shape.binds.clear();
    hits.len()
}

fn point_at_param(sp: &Spline, q: f32) -> Vec3 {
    let n = sp.segment_count();
    if n == 0 {
        return sp.knots.first().map(|k| k.pos).unwrap_or_default();
    }
    let seg = (q.floor() as usize).min(n - 1);
    interp::point_at(&sp.segment(seg).unwrap(), (q - seg as f32).clamp(0.0, 1.0))
}

/// Cross Section: splines through knot i of each listed spline, in order.
pub fn cross_section(shape: &Shape, splines: &[usize], kind: KnotKind) -> Vec<Spline> {
    let list: Vec<&Spline> = splines.iter().filter_map(|i| shape.splines.get(*i)).collect();
    if list.len() < 2 {
        return Vec::new();
    }
    let count = list.iter().map(|s| s.knots.len()).min().unwrap_or(0);
    (0..count)
        .map(|i| {
            let knots = list.iter().map(|s| Knot { pos: s.knots[i].pos, in_vec: Vec3::ZERO, out_vec: Vec3::ZERO, kind: if kind == KnotKind::Bezier || kind == KnotKind::BezierCorner { KnotKind::Smooth } else { kind } }).collect();
            Spline::new(knots, false)
        })
        .collect()
}

/// Surface: every closed region of three or four segments in the spline
/// cage (knots joined within `threshold`) becomes a Coons patch of
/// `steps + 1` divisions a side. Patches face away from the cage centre
/// (`flip` turns them); `remove_interior` drops patches all of whose edges
/// three or more patches share.
pub fn surface(shape: &Shape, threshold: f32, steps: u32, flip: bool, remove_interior: bool) -> Mesh {
    // Nodes: welded knot positions.
    let mut nodes: Vec<Vec3> = Vec::new();
    let node_of = |p: Vec3, nodes: &mut Vec<Vec3>| -> usize {
        if let Some(i) = nodes.iter().position(|q| q.distance(p) <= threshold.max(1e-5)) {
            return i;
        }
        nodes.push(p);
        nodes.len() - 1
    };
    // Edges: (node a, node b, bezier from a to b).
    let mut edges: Vec<(usize, usize, Bezier)> = Vec::new();
    for sp in &shape.splines {
        for i in 0..sp.segment_count() {
            let b = sp.segment(i).unwrap();
            let (a, c) = (node_of(b[0], &mut nodes), node_of(b[3], &mut nodes));
            if a != c && !edges.iter().any(|e| (e.0 == a && e.1 == c) || (e.0 == c && e.1 == a)) {
                edges.push((a, c, b));
            }
        }
    }
    let edge = |u: usize, v: usize| -> Option<Bezier> {
        edges.iter().find_map(|e| {
            if e.0 == u && e.1 == v {
                Some(e.2)
            } else if e.0 == v && e.1 == u {
                Some([e.2[3], e.2[2], e.2[1], e.2[0]])
            } else {
                None
            }
        })
    };
    let nbrs: Vec<Vec<usize>> = (0..nodes.len()).map(|u| edges.iter().filter_map(|e| if e.0 == u { Some(e.1) } else if e.1 == u { Some(e.0) } else { None }).collect()).collect();
    let mut cycles: Vec<Vec<usize>> = Vec::new();
    let mut seen: std::collections::BTreeSet<Vec<usize>> = Default::default();
    for u in 0..nodes.len() {
        for &v in &nbrs[u] {
            for &w in &nbrs[v] {
                if w == u {
                    continue;
                }
                if nbrs[w].contains(&u) {
                    let mut key = vec![u, v, w];
                    key.sort();
                    if seen.insert(key) {
                        cycles.push(vec![u, v, w]);
                    }
                    continue;
                }
                for &x in &nbrs[w] {
                    if x == u || x == v || !nbrs[x].contains(&u) {
                        continue;
                    }
                    // A chord splits the quad into triangles: those win.
                    if nbrs[u].contains(&w) || nbrs[v].contains(&x) {
                        continue;
                    }
                    let mut key = vec![u, v, w, x];
                    key.sort();
                    if seen.insert(key) {
                        cycles.push(vec![u, v, w, x]);
                    }
                }
            }
        }
    }
    if remove_interior {
        let edge_key = |a: usize, b: usize| (a.min(b), a.max(b));
        let mut uses: std::collections::HashMap<(usize, usize), usize> = Default::default();
        for c in &cycles {
            for k in 0..c.len() {
                *uses.entry(edge_key(c[k], c[(k + 1) % c.len()])).or_default() += 1;
            }
        }
        cycles.retain(|c| (0..c.len()).any(|k| uses[&edge_key(c[k], c[(k + 1) % c.len()])] < 3));
    }
    let centre = nodes.iter().copied().sum::<Vec3>() / nodes.len().max(1) as f32;
    let div = (steps + 1) as usize;
    let mut im = IndexedMesh::default();
    let vid = |p: Vec3, im: &mut IndexedMesh| -> u32 {
        if let Some(i) = im.positions.iter().position(|q| q.distance(p) < 1e-4) {
            return i as u32;
        }
        im.add_vertex(p, None)
    };
    for c in cycles {
        let curves: Vec<Bezier> = (0..c.len()).map(|k| edge(c[k], c[(k + 1) % c.len()]).unwrap()).collect();
        // Coons patch: bottom c0(u), right c1(v), top c2 reversed, left c3 reversed (a triangle's left is a point).
        let bottom = curves[0];
        let right = curves[1];
        let top = curves[2];
        let left = if c.len() == 4 { Some(curves[3]) } else { None };
        let p00 = bottom[0];
        let p10 = bottom[3];
        let p11 = right[3];
        let p01 = left.map(|l| l[0]).unwrap_or(top[3]);
        let s = |u: f32, v: f32| -> Vec3 {
            let b = interp::point_at(&bottom, u);
            let t = interp::point_at(&top, 1.0 - u);
            let r = interp::point_at(&right, v);
            let l = left.map(|l| interp::point_at(&l, 1.0 - v)).unwrap_or(p01);
            (1.0 - v) * b + v * t + (1.0 - u) * l + u * r - ((1.0 - u) * (1.0 - v) * p00 + u * (1.0 - v) * p10 + (1.0 - u) * v * p01 + u * v * p11)
        };
        let grid: Vec<Vec<Vec3>> = (0..=div).map(|j| (0..=div).map(|i| s(i as f32 / div as f32, j as f32 / div as f32)).collect()).collect();
        let n0 = (grid[0][1] - grid[0][0]).cross(grid[1][0] - grid[0][0]);
        let mid = grid[div / 2][div / 2];
        let outward = n0.dot(mid - centre) >= 0.0;
        let reverse = outward == flip;
        for j in 0..div {
            for i in 0..div {
                let mut q = vec![grid[j][i], grid[j][i + 1], grid[j + 1][i + 1], grid[j + 1][i]];
                if reverse {
                    q.reverse();
                }
                let mut ids: Vec<u32> = q.iter().map(|p| vid(*p, &mut im)).collect();
                ids.dedup();
                if ids.first() == ids.last() && ids.len() > 1 {
                    ids.pop();
                }
                if ids.len() >= 3 {
                    im.faces.push(IndexedFace { ids, smoothing_groups: 1, ..Default::default() });
                }
            }
        }
    }
    im.build().unwrap_or_default()
}
