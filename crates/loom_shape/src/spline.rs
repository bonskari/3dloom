//! Knots, splines and shapes with interpolation and rendering rollouts.

use crate::interp::{self, Bezier};
use glam::{Quat, Vec3};
use loom_mesh::indexed::{IndexedFace, IndexedMesh};
use loom_mesh::Mesh;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Vertex types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KnotKind {
    Corner,
    Smooth,
    Bezier,
    BezierCorner,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Knot {
    pub pos: Vec3,
    /// Incoming handle relative to `pos` (used by Bezier / Bezier Corner).
    pub in_vec: Vec3,
    pub out_vec: Vec3,
    pub kind: KnotKind,
}

impl Knot {
    pub fn corner(pos: Vec3) -> Self {
        Self { pos, in_vec: Vec3::ZERO, out_vec: Vec3::ZERO, kind: KnotKind::Corner }
    }
    pub fn smooth(pos: Vec3) -> Self {
        Self { pos, in_vec: Vec3::ZERO, out_vec: Vec3::ZERO, kind: KnotKind::Smooth }
    }
    pub fn bezier(pos: Vec3, in_vec: Vec3, out_vec: Vec3) -> Self {
        Self { pos, in_vec, out_vec, kind: KnotKind::Bezier }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Spline {
    pub knots: Vec<Knot>,
    pub closed: bool,
}

impl Spline {
    pub fn new(knots: Vec<Knot>, closed: bool) -> Self {
        Self { knots, closed }
    }

    pub fn segment_count(&self) -> usize {
        if self.closed {
            self.knots.len()
        } else {
            self.knots.len().saturating_sub(1)
        }
    }

    fn knot_at(&self, i: isize) -> Option<&Knot> {
        let n = self.knots.len() as isize;
        if n == 0 {
            return None;
        }
        if self.closed {
            self.knots.get(i.rem_euclid(n) as usize)
        } else {
            (0..n).contains(&i).then(|| &self.knots[i as usize])
        }
    }

    /// Outgoing handle of knot `i` (Smooth knots get an automatic tangent).
    pub fn out_handle(&self, i: usize) -> Vec3 {
        let k = &self.knots[i];
        match k.kind {
            KnotKind::Corner => Vec3::ZERO,
            KnotKind::Bezier | KnotKind::BezierCorner => k.out_vec,
            KnotKind::Smooth => {
                let next = self.knot_at(i as isize + 1).map(|n| n.pos);
                let prev = self.knot_at(i as isize - 1).map(|p| p.pos);
                let Some(next) = next else { return Vec3::ZERO };
                let dir = match prev {
                    Some(p) => (next - p).normalize_or_zero(),
                    None => (next - k.pos).normalize_or_zero(),
                };
                dir * (next.distance(k.pos) / 3.0)
            }
        }
    }

    /// Incoming handle of knot `i`.
    pub fn in_handle(&self, i: usize) -> Vec3 {
        let k = &self.knots[i];
        match k.kind {
            KnotKind::Corner => Vec3::ZERO,
            KnotKind::Bezier | KnotKind::BezierCorner => k.in_vec,
            KnotKind::Smooth => {
                let next = self.knot_at(i as isize + 1).map(|n| n.pos);
                let prev = self.knot_at(i as isize - 1).map(|p| p.pos);
                let Some(prev) = prev else { return Vec3::ZERO };
                let dir = match next {
                    Some(n) => (prev - n).normalize_or_zero(),
                    None => (prev - k.pos).normalize_or_zero(),
                };
                dir * (prev.distance(k.pos) / 3.0)
            }
        }
    }

    /// Segment `i` as a cubic Bezier (from knot i to knot i+1).
    pub fn segment(&self, i: usize) -> Option<Bezier> {
        let n = self.knots.len();
        if i >= self.segment_count() {
            return None;
        }
        let j = (i + 1) % n;
        let (a, b) = (&self.knots[i], &self.knots[j]);
        Some([a.pos, a.pos + self.out_handle(i), b.pos + self.in_handle(j), b.pos])
    }

    pub fn is_straight(&self, i: usize) -> bool {
        self.segment(i).is_some_and(|b| interp::is_straight(&b))
    }

    /// Interpolated points; a closed spline does not repeat its first point.
    pub fn polyline(&self, steps: u32, optimize: bool, adaptive: bool) -> Vec<Vec3> {
        let mut out = Vec::new();
        if self.knots.is_empty() {
            return out;
        }
        out.push(self.knots[0].pos);
        for i in 0..self.segment_count() {
            let b = self.segment(i).unwrap();
            if adaptive {
                adaptive_points(&b, 0.02 * b[0].distance(b[3]).max(1e-3), 0, &mut out);
            } else {
                let n = if optimize && interp::is_straight(&b) { 1 } else { steps + 1 };
                for s in 1..=n {
                    out.push(interp::point_at(&b, s as f32 / n as f32));
                }
            }
        }
        if self.closed {
            out.pop();
        }
        out
    }

    pub fn length(&self) -> f32 {
        (0..self.segment_count()).map(|i| interp::length(&self.segment(i).unwrap(), 32)).sum()
    }

    pub fn reverse(&mut self) {
        self.knots.reverse();
        for k in &mut self.knots {
            std::mem::swap(&mut k.in_vec, &mut k.out_vec);
        }
    }

    /// Make First: knot `i` becomes the first (closed: rotates, open: reverses when it is the last).
    pub fn make_first(&mut self, i: usize) {
        if i >= self.knots.len() {
            return;
        }
        if self.closed {
            self.knots.rotate_left(i);
        } else if i == self.knots.len() - 1 {
            self.reverse();
        }
    }

    /// Refine: a knot at parameter `t` of segment `i` (Bezier, keeping the curve).
    pub fn refine(&mut self, i: usize, t: f32) -> Option<usize> {
        let b = self.segment(i)?;
        let (l, r) = interp::split_at(&b, t);
        let n = self.knots.len();
        let j = (i + 1) % n;
        let kind = |k: &Knot| if k.kind == KnotKind::Corner && interp::is_straight(&b) { KnotKind::Corner } else { KnotKind::Bezier };
        // The neighbours keep their curve with the split's control points.
        let (ka, kb) = (self.knots[i].clone(), self.knots[j].clone());
        self.knots[i].kind = kind(&ka);
        self.knots[i].out_vec = l[1] - l[0];
        self.knots[i].in_vec = if ka.kind == KnotKind::Corner { Vec3::ZERO } else { self.in_handle(i) };
        self.knots[j].kind = kind(&kb);
        self.knots[j].in_vec = r[2] - r[3];
        self.knots[j].out_vec = if kb.kind == KnotKind::Corner { Vec3::ZERO } else { self.out_handle(j) };
        let new = if interp::is_straight(&b) { Knot::corner(l[3]) } else { Knot::bezier(l[3], l[2] - l[3], r[1] - r[0]) };
        self.knots.insert(i + 1, new);
        Some(i + 1)
    }

    /// Divide: `n` evenly spaced knots along segment `i`.
    pub fn divide(&mut self, i: usize, n: u32) {
        let n = n.max(1);
        // Each refine shortens segment `i` to its first part, so the next cut
        // sits at k/(k+1) of what remains, measured by arc length (a straight
        // Bezier is not linear in t).
        for k in (1..=n).rev() {
            let Some(b) = self.segment(i) else { return };
            let len = interp::length(&b, 64);
            let t = interp::t_at_length(&b, len * k as f32 / (k + 1) as f32);
            self.refine(i, t);
        }
    }

    /// Break at knot `i`: an open spline splits in two, a closed one opens there.
    pub fn break_at(&mut self, i: usize) -> Option<Spline> {
        if i >= self.knots.len() {
            return None;
        }
        if self.closed {
            self.knots.rotate_left(i);
            let first = self.knots[0].clone();
            self.knots.push(first);
            self.closed = false;
            return None;
        }
        if i == 0 || i == self.knots.len() - 1 {
            return None;
        }
        let tail: Vec<Knot> = self.knots[i..].to_vec();
        self.knots.truncate(i + 1);
        Some(Spline::new(tail, false))
    }

    pub fn close(&mut self) {
        if self.knots.len() >= 3 {
            self.closed = true;
        }
    }

    /// Mirror about an axis through the origin (0 = X: flip X, 1 = Y, 2 = both).
    pub fn mirror(&mut self, axis: u8) {
        let m = match axis { 0 => Vec3::new(-1.0, 1.0, 1.0), 1 => Vec3::new(1.0, -1.0, 1.0), _ => Vec3::new(-1.0, -1.0, 1.0) };
        for k in &mut self.knots {
            k.pos *= m;
            k.in_vec *= m;
            k.out_vec *= m;
        }
        // A mirrored closed loop runs the other way; keep the winding.
        if axis != 2 {
            self.reverse();
        }
    }

    pub fn translate(&mut self, d: Vec3) {
        for k in &mut self.knots {
            k.pos += d;
        }
    }
}

fn adaptive_points(b: &Bezier, tol: f32, depth: u32, out: &mut Vec<Vec3>) {
    if depth >= 8 || interp::flat_enough(b, tol) {
        out.push(b[3]);
        return;
    }
    let (l, r) = interp::split_at(b, 0.5);
    adaptive_points(&l, tol, depth + 1, out);
    adaptive_points(&r, tol, depth + 1, out);
}

/// Rendering rollout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderParams {
    pub enable_in_renderer: bool,
    pub enable_in_viewport: bool,
    pub use_viewport_settings: bool,
    pub generate_mapping: bool,
    pub rectangular: bool,
    pub thickness: f32,
    pub sides: u32,
    pub angle: f32,
    pub length: f32,
    pub width: f32,
    pub auto_smooth: bool,
    pub auto_smooth_threshold: f32,
}

impl Default for RenderParams {
    fn default() -> Self {
        Self {
            enable_in_renderer: false,
            enable_in_viewport: false,
            use_viewport_settings: false,
            generate_mapping: true,
            rectangular: false,
            thickness: 1.0,
            sides: 12,
            angle: 0.0,
            length: 6.0,
            width: 2.0,
            auto_smooth: true,
            auto_smooth_threshold: 40.0,
        }
    }
}

/// A shape: its splines plus the Interpolation and Rendering rollouts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub splines: Vec<Spline>,
    pub steps: u32,
    pub optimize: bool,
    pub adaptive: bool,
    pub render: RenderParams,
    /// Editable Spline > Bind: (flat knot, flat segment) — the end knot sits
    /// on the segment's middle and follows it.
    #[serde(default)]
    pub binds: Vec<(usize, usize)>,
}

impl Default for Shape {
    fn default() -> Self {
        Self { splines: Vec::new(), steps: 6, optimize: true, adaptive: false, render: RenderParams::default(), binds: Vec::new() }
    }
}

impl Shape {
    pub fn from_splines(splines: Vec<Spline>) -> Self {
        Self { splines, ..Default::default() }
    }

    /// An open corner polyline through `points`.
    pub fn polyline_shape(points: &[Vec3]) -> Self {
        Self::from_splines(vec![Spline::new(points.iter().map(|p| Knot::corner(*p)).collect(), false)])
    }

    /// The same shape with every knot and handle transformed by `m`.
    pub fn transformed(&self, m: glam::Mat4) -> Self {
        let mut out = self.clone();
        for s in &mut out.splines {
            for k in &mut s.knots {
                k.pos = m.transform_point3(k.pos);
                k.in_vec = m.transform_vector3(k.in_vec);
                k.out_vec = m.transform_vector3(k.out_vec);
            }
        }
        out
    }

    /// One polyline per spline (closed ones do not repeat the first point).
    pub fn polylines(&self) -> Vec<Vec<Vec3>> {
        self.splines.iter().map(|s| s.polyline(self.steps, self.optimize, self.adaptive)).collect()
    }

    pub fn knot_count(&self) -> usize {
        self.splines.iter().map(|s| s.knots.len()).sum()
    }

    /// Shape Check: where the shape's interpolated segments cross each other
    /// (within one spline or between two of its splines), in the shape's own
    /// space. Segments cross where they meet in the local XY plane at one
    /// height. Curves that only touch do not count: the end point of an open
    /// spline lying on another curve, or two curves meeting at a shared vertex
    /// without passing through each other. Straight pieces lying on top of each
    /// other (collinear overlaps) count, marked at both ends of the overlap.
    pub fn self_intersections(&self) -> Vec<Vec3> {
        let lines: Vec<(Vec<Vec3>, bool)> = self
            .splines
            .iter()
            .map(|s| {
                // Repeated points would make segments two apart touch.
                let mut pts = s.polyline(self.steps, self.optimize, self.adaptive);
                pts.dedup_by(|a, b| a.distance(*b) < 1e-6);
                if s.closed && pts.len() > 1 && pts[0].distance(pts[pts.len() - 1]) < 1e-6 {
                    pts.pop();
                }
                let closed = s.closed && pts.len() > 2;
                (pts, closed)
            })
            .collect();
        // (spline, index in spline, a, b) per segment.
        let mut segs: Vec<(usize, usize, Vec3, Vec3)> = Vec::new();
        let mut counts = Vec::new();
        for (si, (pts, closed)) in lines.iter().enumerate() {
            let n = pts.len();
            let m = if *closed { n } else { n.saturating_sub(1) };
            for i in 0..m {
                segs.push((si, i, pts[i], pts[(i + 1) % n]));
            }
            counts.push(m);
        }
        let extent = segs.iter().fold(0.0_f32, |e, s| e.max(s.2.abs().max_element()).max(s.3.abs().max_element())).max(1.0);
        let tol = extent * 1e-4;
        // The XY directions leaving the curve at parameter `u` of segment
        // `seg`: both ways along it inside the segment, to the neighbouring
        // vertices at a vertex; None at an open spline's end (a touch only).
        let dirs = |seg: &(usize, usize, Vec3, Vec3), u: f32, len: f32| -> Option<[glam::Vec2; 2]> {
            let (pts, closed) = &lines[seg.0];
            let n = pts.len();
            let eps = tol / len.max(1e-12);
            let vertex = if u <= eps {
                seg.1
            } else if u >= 1.0 - eps {
                (seg.1 + 1) % n
            } else {
                let d = (seg.3 - seg.2).truncate();
                return Some([-d, d]);
            };
            if !closed && (vertex == 0 || vertex == n - 1) {
                return None;
            }
            let p = pts[vertex];
            Some([(pts[(vertex + n - 1) % n] - p).truncate(), (pts[(vertex + 1) % n] - p).truncate()])
        };
        let mut out: Vec<Vec3> = Vec::new();
        // Collinear overlaps (a, b ends); their outer ends are marked at the end.
        let mut overlaps: Vec<(Vec3, Vec3)> = Vec::new();
        let push = |pa: Vec3, out: &mut Vec<Vec3>| {
            if !out.iter().any(|o| o.distance(pa) < tol) {
                out.push(pa);
            }
        };
        for (x, a) in segs.iter().enumerate() {
            for b in &segs[x + 1..] {
                let mut neighbours = false;
                if a.0 == b.0 {
                    let m = counts[a.0];
                    let d = a.1.abs_diff(b.1);
                    // Neighbours share an end (the last and first segment of a closed spline too).
                    neighbours = d <= 1 || (lines[a.0].1 && d == m - 1);
                }
                let (p, r) = (a.2, a.3 - a.2);
                let (q, t) = (b.2, b.3 - b.2);
                let (rl, tl) = (r.truncate().length(), t.truncate().length());
                if rl < 1e-9 || tl < 1e-9 {
                    continue;
                }
                let den = r.x * t.y - r.y * t.x;
                let w = q - p;
                if den.abs() <= 1e-6 * rl * tl {
                    // Parallel: an overlap when collinear, at one height, and
                    // sharing more than a point.
                    if (w.x * r.y - w.y * r.x).abs() > tol * rl {
                        continue;
                    }
                    let rr = r.truncate().dot(r.truncate());
                    let t0 = w.truncate().dot(r.truncate()) / rr;
                    let t1 = (b.3 - p).truncate().dot(r.truncate()) / rr;
                    let (lo, hi) = (t0.min(t1).max(0.0), t0.max(t1).min(1.0));
                    if (hi - lo) * rl <= tol {
                        continue;
                    }
                    let (pa, pb) = (p + r * lo, p + r * hi);
                    let height = |pt: Vec3| q.z + t.z * ((pt - q).truncate().dot(t.truncate()) / (tl * tl));
                    if (pa.z - height(pa)).abs() > tol || (pb.z - height(pb)).abs() > tol {
                        continue;
                    }
                    overlaps.push((pa, pb));
                    continue;
                }
                if neighbours {
                    continue;
                }
                let u = (w.x * t.y - w.y * t.x) / den;
                let v = (w.x * r.y - w.y * r.x) / den;
                let (eu, ev) = (tol / rl, tol / tl);
                if !(-eu..=1.0 + eu).contains(&u) || !(-ev..=1.0 + ev).contains(&v) {
                    continue;
                }
                let (u, v) = (u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
                let (pa, pb) = (p + r * u, q + t * v);
                if (pa.z - pb.z).abs() > tol {
                    continue;
                }
                let (Some(da), Some(db)) = (dirs(a, u, rl), dirs(b, v, tl)) else { continue };
                if passes_through(da, db) {
                    push(pa, &mut out);
                }
            }
        }
        // An overlap running over several segments is one overlap: an end
        // that lies inside another overlap, or where two overlaps continue
        // each other along the line, is not marked.
        for (i, &(a, b)) in overlaps.iter().enumerate() {
            for (e, other) in [(a, b), (b, a)] {
                let dir = (other - e).normalize_or_zero();
                let inner = overlaps.iter().enumerate().any(|(j, &(c, d))| {
                    let cd = d - c;
                    let t = (e - c).dot(cd) / cd.length_squared().max(1e-12);
                    let on_line = (c + cd * t).distance(e) < tol;
                    if !on_line {
                        return false;
                    }
                    let len = cd.length();
                    if t * len > tol && (1.0 - t) * len > tol {
                        return true;
                    }
                    // Touching end to end, running on the other way.
                    let far = if e.distance(c) < tol { d } else if e.distance(d) < tol { c } else { return false };
                    j != i && (far - e).normalize_or_zero().dot(dir) < -0.99
                });
                if !inner {
                    push(e, &mut out);
                }
            }
        }
        out
    }

    /// Attach: the other shape's splines join this one.
    pub fn attach(&mut self, other: &Shape) {
        self.splines.extend(other.splines.iter().cloned());
    }

    /// Explode: every segment becomes its own spline.
    pub fn explode(&mut self) {
        let mut out = Vec::new();
        for s in &self.splines {
            for i in 0..s.segment_count() {
                let b = s.segment(i).unwrap();
                let (a, c) = (&s.knots[i], &s.knots[(i + 1) % s.knots.len()]);
                let straight = interp::is_straight(&b);
                let ka = if straight { Knot::corner(b[0]) } else { Knot::bezier(b[0], a.in_vec, b[1] - b[0]) };
                let kc = if straight { Knot::corner(b[3]) } else { Knot::bezier(b[3], b[2] - b[3], c.out_vec) };
                out.push(Spline::new(vec![ka, kc], false));
            }
        }
        self.splines = out;
    }

    /// The viewport / render tube: a `sides`-gon (or Length x Width
    /// rectangle) swept along every polyline, ends capped.
    pub fn tube_mesh(&self) -> Mesh {
        let r = &self.render;
        let mut im = IndexedMesh { positions: Vec::new(), vattrs: Vec::new(), faces: Vec::new(), keep: BTreeSet::new() };
        let profile: Vec<Vec3> = if r.rectangular {
            let (l, w) = (r.length * 0.5, r.width * 0.5);
            vec![Vec3::new(-w, -l, 0.0), Vec3::new(w, -l, 0.0), Vec3::new(w, l, 0.0), Vec3::new(-w, l, 0.0)]
        } else {
            let n = r.sides.max(3);
            (0..n).map(|i| {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                Vec3::new(a.cos(), a.sin(), 0.0) * (r.thickness * 0.5)
            }).collect()
        };
        let spin = Quat::from_rotation_z(r.angle.to_radians());
        let profile: Vec<Vec3> = profile.iter().map(|p| spin * *p).collect();
        for (si, spline) in self.splines.iter().enumerate() {
            let pts = spline.polyline(self.steps, self.optimize, self.adaptive);
            let n = pts.len();
            if n < 2 {
                continue;
            }
            // Tangents and parallel-transported frames.
            let tangent = |i: usize| -> Vec3 {
                let prev = if i == 0 { if spline.closed { pts[n - 1] } else { pts[0] } } else { pts[i - 1] };
                let next = if i + 1 == n { if spline.closed { pts[0] } else { pts[n - 1] } } else { pts[i + 1] };
                (next - prev).normalize_or_zero()
            };
            let mut frames: Vec<Quat> = Vec::with_capacity(n);
            let mut prev_t = tangent(0);
            frames.push(Quat::from_rotation_arc(Vec3::Z, if prev_t.length_squared() < 0.5 { Vec3::Z } else { prev_t }));
            for i in 1..n {
                let t = tangent(i);
                let step = if t.length_squared() < 0.5 || prev_t.length_squared() < 0.5 { Quat::IDENTITY } else { Quat::from_rotation_arc(prev_t, t) };
                frames.push((step * frames[i - 1]).normalize());
                if t.length_squared() > 0.5 {
                    prev_t = t;
                }
            }
            let base = im.positions.len() as u32;
            let m = profile.len() as u32;
            for i in 0..n {
                for p in &profile {
                    im.add_vertex(pts[i] + frames[i] * *p, None);
                }
            }
            let rings = if spline.closed { n } else { n - 1 };
            let mat = si as u16 + 1;
            for i in 0..rings {
                let (a, b) = (base + (i as u32) * m, base + (((i + 1) % n) as u32) * m);
                for k in 0..m {
                    let k2 = (k + 1) % m;
                    im.faces.push(IndexedFace { ids: vec![a + k, b + k, b + k2, a + k2], smoothing_groups: 1, material_id: mat, hidden: false, uv: Default::default() });
                }
            }
            if !spline.closed {
                im.faces.push(IndexedFace { ids: (0..m).map(|k| base + k).collect(), smoothing_groups: 2, material_id: mat, hidden: false, uv: Default::default() });
                let last = base + ((n - 1) as u32) * m;
                im.faces.push(IndexedFace { ids: (0..m).rev().map(|k| last + k).collect(), smoothing_groups: 2, material_id: mat, hidden: false, uv: Default::default() });
            }
        }
        let mut mesh = im.build().unwrap_or_default();
        if r.auto_smooth {
            mesh.auto_smooth(r.auto_smooth_threshold);
        }
        mesh
    }

    /// Bounding box of the interpolated shape.
    pub fn aabb(&self) -> (Vec3, Vec3) {
        let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        for pl in self.polylines() {
            for p in pl {
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
        if lo.x > hi.x {
            (Vec3::ZERO, Vec3::ZERO)
        } else {
            (lo, hi)
        }
    }
}

/// Flat sub-object indexing over a shape's splines (vertex = knot, segment,
/// spline), as the Editable Spline levels address them.
impl Shape {
    /// (spline, knot) of flat knot index `i`.
    pub fn knot_at(&self, i: usize) -> Option<(usize, usize)> {
        let mut base = 0;
        for (si, s) in self.splines.iter().enumerate() {
            if i < base + s.knots.len() {
                return Some((si, i - base));
            }
            base += s.knots.len();
        }
        None
    }

    pub fn knot_index(&self, spline: usize, knot: usize) -> usize {
        self.splines.iter().take(spline).map(|s| s.knots.len()).sum::<usize>() + knot
    }

    /// (spline, segment) of flat segment index `i`.
    pub fn segment_at(&self, i: usize) -> Option<(usize, usize)> {
        let mut base = 0;
        for (si, s) in self.splines.iter().enumerate() {
            if i < base + s.segment_count() {
                return Some((si, i - base));
            }
            base += s.segment_count();
        }
        None
    }

    pub fn segment_index(&self, spline: usize, segment: usize) -> usize {
        self.splines.iter().take(spline).map(|s| s.segment_count()).sum::<usize>() + segment
    }

    pub fn segment_count(&self) -> usize {
        self.splines.iter().map(|s| s.segment_count()).sum()
    }

    /// Moves every bound knot onto the middle of its segment; binds whose
    /// knot or segment is gone are dropped.
    pub fn apply_binds(&mut self) {
        let binds = std::mem::take(&mut self.binds);
        let mut keep = Vec::new();
        for (k, seg) in binds {
            let (Some((ks, ki)), Some((ss, si))) = (self.knot_at(k), self.segment_at(seg)) else { continue };
            if ks == ss {
                continue;
            }
            let Some(b) = self.splines[ss].segment(si) else { continue };
            self.splines[ks].knots[ki].pos = interp::point_at(&b, 0.5);
            keep.push((k, seg));
        }
        self.binds = keep;
    }

    /// Every knot position in flat order.
    pub fn knot_positions(&self) -> Vec<Vec3> {
        self.splines.iter().flat_map(|s| s.knots.iter().map(|k| k.pos)).collect()
    }

    /// Sets every knot position from flat order (handles ride along).
    pub fn set_knot_positions(&mut self, positions: &[Vec3]) {
        let mut i = 0;
        for s in &mut self.splines {
            for k in &mut s.knots {
                if let Some(p) = positions.get(i) {
                    k.pos = *p;
                }
                i += 1;
            }
        }
    }

    /// The two flat knot indices of a flat segment.
    pub fn segment_knots(&self, i: usize) -> Option<(usize, usize)> {
        let (si, seg) = self.segment_at(i)?;
        let s = &self.splines[si];
        let n = s.knots.len();
        Some((self.knot_index(si, seg), self.knot_index(si, (seg + 1) % n)))
    }

    /// Delete knots (flat indices): their segments go with them; a closed
    /// spline opens, an open one splits in two (the vertex is deleted and
    /// the neighbours joined when the spline stays one piece).
    pub fn delete_knots(&mut self, flat: &[usize]) {
        let mut per: Vec<Vec<usize>> = vec![Vec::new(); self.splines.len()];
        for f in flat {
            if let Some((si, k)) = self.knot_at(*f) {
                per[si].push(k);
            }
        }
        let mut out = Vec::new();
        for (si, s) in self.splines.iter().enumerate() {
            let mut gone = per[si].clone();
            gone.sort();
            gone.dedup();
            if gone.is_empty() {
                out.push(s.clone());
                continue;
            }
            let keep: Vec<Knot> = s.knots.iter().enumerate().filter(|(i, _)| !gone.contains(i)).map(|(_, k)| k.clone()).collect();
            if keep.len() >= 2 {
                let closed = s.closed && keep.len() >= 3;
                out.push(Spline::new(keep, closed));
            }
        }
        self.splines = out;
    }

    /// Delete segments (flat indices): each removal opens or splits its spline.
    pub fn delete_segments(&mut self, flat: &[usize]) {
        let mut per: Vec<Vec<usize>> = vec![Vec::new(); self.splines.len()];
        for f in flat {
            if let Some((si, seg)) = self.segment_at(*f) {
                per[si].push(seg);
            }
        }
        let mut out = Vec::new();
        for (si, s) in self.splines.iter().enumerate() {
            let mut gone = per[si].clone();
            gone.sort();
            gone.dedup();
            if gone.is_empty() {
                out.push(s.clone());
                continue;
            }
            // Walk the knots as a chain; a removed segment ends the current piece.
            let n = s.knots.len();
            let order: Vec<usize> = if s.closed {
                // Start just after a removed segment so the ring opens there.
                let start = (gone[0] + 1) % n;
                (0..n).map(|i| (start + i) % n).collect()
            } else {
                (0..n).collect()
            };
            let mut piece: Vec<Knot> = Vec::new();
            for (idx, ki) in order.iter().enumerate() {
                piece.push(s.knots[*ki].clone());
                let seg_after = *ki; // segment from knot ki to the next
                let last = idx + 1 == order.len();
                let cut = if s.closed { gone.contains(&seg_after) } else { seg_after < s.segment_count() && gone.contains(&seg_after) };
                if cut || last {
                    if piece.len() >= 2 {
                        out.push(Spline::new(std::mem::take(&mut piece), false));
                    } else {
                        piece.clear();
                    }
                }
            }
        }
        self.splines = out;
    }

    /// Fuse: the knots move to their average position.
    pub fn fuse(&mut self, flat: &[usize]) {
        let pts: Vec<Vec3> = flat.iter().filter_map(|f| self.knot_at(*f)).map(|(si, k)| self.splines[si].knots[k].pos).collect();
        if pts.is_empty() {
            return;
        }
        let c = pts.iter().sum::<Vec3>() / pts.len() as f32;
        for f in flat {
            if let Some((si, k)) = self.knot_at(*f) {
                self.splines[si].knots[k].pos = c;
            }
        }
    }
}

/// Whether a curve leaving a point along `b` passes through one leaving it
/// along `a` (one of `b`'s directions on each side), rather than touching it.
/// A direction along one of `a`'s is a tangent touch or an overlap, not a pass.
fn passes_through(a: [glam::Vec2; 2], b: [glam::Vec2; 2]) -> bool {
    use std::f32::consts::TAU;
    let ang = |d: glam::Vec2| {
        let x = d.y.atan2(d.x) - a[0].y.atan2(a[0].x);
        x.rem_euclid(TAU)
    };
    // A direction with no length in XY (a vertical piece) gives no side.
    if a.iter().chain(b.iter()).any(|d| d.length_squared() < 1e-12) {
        return false;
    }
    let end = ang(a[1]);
    let eps = 1e-4;
    let mut inside = 0;
    for d in b {
        let x = ang(d);
        if x < eps || x > TAU - eps || (x - end).abs() < eps {
            return false;
        }
        if x < end {
            inside += 1;
        }
    }
    inside == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Spline {
        Spline::new(vec![Knot::corner(Vec3::ZERO), Knot::corner(Vec3::X * 10.0), Knot::corner(Vec3::new(10.0, 10.0, 0.0)), Knot::corner(Vec3::Y * 10.0)], true)
    }

    /// Shape Check: a square has no crossings, a bow tie one in the middle,
    /// a circle and a line through it two, a circle (smooth, many steps) none.
    #[test]
    fn self_intersections_find_crossing_segments_only() {
        assert!(Shape::from_splines(vec![square()]).self_intersections().is_empty());
        let bow = Spline::new(vec![Knot::corner(Vec3::ZERO), Knot::corner(Vec3::new(10.0, 10.0, 0.0)), Knot::corner(Vec3::new(10.0, 0.0, 0.0)), Knot::corner(Vec3::new(0.0, 10.0, 0.0))], true);
        let hits = Shape::from_splines(vec![bow]).self_intersections();
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert!(hits[0].distance(Vec3::new(5.0, 5.0, 0.0)) < 1e-4);
        let circle = crate::parametric::circle(10.0);
        assert!(Shape::from_splines(vec![circle.clone()]).self_intersections().is_empty());
        let line = Spline::new(vec![Knot::corner(Vec3::new(-20.0, 0.0, 0.0)), Knot::corner(Vec3::new(20.0, 0.0, 0.0))], false);
        assert_eq!(Shape::from_splines(vec![circle, line.clone()]).self_intersections().len(), 2);
        // A line passing above the circle (another height) does not cross it.
        let mut high = line;
        for k in &mut high.knots {
            k.pos.z = 5.0;
        }
        assert!(Shape::from_splines(vec![crate::parametric::circle(10.0), high]).self_intersections().is_empty());
    }

    fn open(points: &[Vec3]) -> Spline {
        Spline::new(points.iter().map(|p| Knot::corner(*p)).collect(), false)
    }

    /// Shape Check: curves that only touch are not crossings: two open
    /// splines sharing an end point, an open spline ending on another curve,
    /// two closed squares meeting at a corner; a line through a square's
    /// corner (in one side, out the other) still is.
    #[test]
    fn self_intersections_ignore_touching_at_an_end_or_shared_vertex() {
        let a = open(&[Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0)]);
        let b = open(&[Vec3::new(10.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 0.0)]);
        assert!(Shape::from_splines(vec![a.clone(), b]).self_intersections().is_empty());
        let t = open(&[Vec3::new(5.0, 0.0, 0.0), Vec3::new(5.0, 8.0, 0.0)]);
        assert!(Shape::from_splines(vec![a, t]).self_intersections().is_empty());
        let mut other = square();
        for k in &mut other.knots {
            k.pos += Vec3::new(10.0, 10.0, 0.0);
        }
        assert!(Shape::from_splines(vec![square(), other]).self_intersections().is_empty(), "corner contact");
        let diag = open(&[Vec3::new(-5.0, 15.0, 0.0), Vec3::new(15.0, -5.0, 0.0)]);
        let hits = Shape::from_splines(vec![square(), diag]).self_intersections();
        assert_eq!(hits.len(), 2, "{hits:?}");
        // A line grazing the corner from outside only touches it.
        let graze = open(&[Vec3::new(5.0, 15.0, 0.0), Vec3::new(15.0, 5.0, 0.0)]);
        assert!(Shape::from_splines(vec![square(), graze]).self_intersections().is_empty());
        // A closed spline that runs through one of its own vertices again
        // without crossing (two loops touching at a point) is not a crossing.
        let closed = |pts: &[Vec3]| Spline::new(pts.iter().map(|p| Knot::corner(*p)).collect(), true);
        let kiss = closed(&[Vec3::ZERO, Vec3::new(5.0, 5.0, 0.0), Vec3::new(-5.0, 5.0, 0.0), Vec3::ZERO, Vec3::new(-5.0, -5.0, 0.0), Vec3::new(5.0, -5.0, 0.0)]);
        assert!(Shape::from_splines(vec![kiss]).self_intersections().is_empty());
        // A figure eight through the same vertex does cross there.
        let eight = closed(&[Vec3::ZERO, Vec3::new(5.0, 5.0, 0.0), Vec3::new(-5.0, 5.0, 0.0), Vec3::ZERO, Vec3::new(5.0, -5.0, 0.0), Vec3::new(-5.0, -5.0, 0.0)]);
        let hits = Shape::from_splines(vec![eight]).self_intersections();
        assert_eq!(hits.len(), 1, "{hits:?}");
    }

    /// Shape Check: straight segments lying on top of each other are
    /// reported at both ends of the overlap, between splines and within one
    /// (a spline that doubles back on itself); end-to-end ones are not.
    #[test]
    fn self_intersections_report_collinear_overlaps() {
        let a = open(&[Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0)]);
        let b = open(&[Vec3::new(4.0, 0.0, 0.0), Vec3::new(20.0, 0.0, 0.0)]);
        let hits = Shape::from_splines(vec![a.clone(), b]).self_intersections();
        assert_eq!(hits.len(), 2, "{hits:?}");
        assert!(hits.iter().any(|p| p.distance(Vec3::new(4.0, 0.0, 0.0)) < 1e-4));
        assert!(hits.iter().any(|p| p.distance(Vec3::new(10.0, 0.0, 0.0)) < 1e-4));
        let end_to_end = open(&[Vec3::new(10.0, 0.0, 0.0), Vec3::new(20.0, 0.0, 0.0)]);
        assert!(Shape::from_splines(vec![a.clone(), end_to_end]).self_intersections().is_empty());
        let lifted = open(&[Vec3::new(4.0, 0.0, 3.0), Vec3::new(20.0, 0.0, 3.0)]);
        assert!(Shape::from_splines(vec![a, lifted]).self_intersections().is_empty(), "another height");
        // A two-segment line over a one-segment one: one overlap, two marks.
        let one = open(&[Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0)]);
        let two = open(&[Vec3::new(2.0, 0.0, 0.0), Vec3::new(5.0, 0.0, 0.0), Vec3::new(8.0, 0.0, 0.0)]);
        let hits = Shape::from_splines(vec![one, two]).self_intersections();
        assert_eq!(hits.len(), 2, "{hits:?}");
        assert!(hits.iter().any(|p| p.distance(Vec3::new(2.0, 0.0, 0.0)) < 1e-4) && hits.iter().any(|p| p.distance(Vec3::new(8.0, 0.0, 0.0)) < 1e-4));
        let back = open(&[Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0), Vec3::new(6.0, 0.0, 0.0)]);
        let hits = Shape::from_splines(vec![back]).self_intersections();
        assert_eq!(hits.len(), 2, "{hits:?}");
        assert!(hits.iter().any(|p| p.distance(Vec3::new(6.0, 0.0, 0.0)) < 1e-4));
    }

    #[test]
    fn corner_square_interpolates_to_four_points_with_optimize() {
        let s = square();
        assert_eq!(s.polyline(6, true, false).len(), 4);
        assert_eq!(s.polyline(6, false, false).len(), 4 * 7);
        assert!((s.length() - 40.0).abs() < 1e-4);
        let mut r = s.clone();
        r.refine(0, 0.5);
        assert_eq!(r.knots.len(), 5);
        assert_eq!(r.knots[1].pos, Vec3::X * 5.0);
        assert_eq!(r.knots[1].kind, KnotKind::Corner);
        let mut d = s.clone();
        d.divide(1, 3);
        assert_eq!(d.knots.len(), 7);
        let mut b = s.clone();
        assert!(b.break_at(2).is_none());
        assert!(!b.closed && b.knots.len() == 5);
        let tail = b.break_at(2).unwrap();
        assert_eq!((b.knots.len(), tail.knots.len()), (3, 3));
    }

    #[test]
    fn smooth_knots_curve_and_the_tube_closes() {
        let s = Spline::new(vec![Knot::smooth(Vec3::ZERO), Knot::smooth(Vec3::new(10.0, 10.0, 0.0)), Knot::smooth(Vec3::new(20.0, 0.0, 0.0))], false);
        let pts = s.polyline(6, true, false);
        assert_eq!(pts.len(), 1 + 2 * 7);
        assert!(pts[3].y > 3.0, "{:?}", pts[3]);
        let mut shape = Shape::from_splines(vec![square()]);
        shape.render.enable_in_viewport = true;
        let m = shape.tube_mesh();
        m.check_invariants().unwrap();
        assert_eq!(m.face_count(), 4 * 12);
        let mut open = Shape::from_splines(vec![s]);
        open.render.sides = 6;
        let t = open.tube_mesh();
        t.check_invariants().unwrap();
        assert_eq!(t.face_count(), 14 * 6 + 2);
        let mut e = shape.clone();
        e.explode();
        assert_eq!(e.splines.len(), 4);
    }
}
