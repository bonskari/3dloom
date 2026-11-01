//! Adobe Illustrator 88 (.ai) import: the path operators of the file's
//! PostScript body read as splines on the XY plane, one point per unit.
//! moveto `m`, lineto `l L`, curveto `c C`, `v V` (the first control at
//! the current point) and `y Y` (the second control at the end); a path
//! ends at a painting operator: `s f F b B` (and closepath `h H`, `n`)
//! close it, `S N` leave it open. Comments, text and other operators are
//! skipped.

use crate::{Knot, KnotKind, Spline};
use glam::Vec3;

struct Path {
    knots: Vec<Knot>,
}

impl Path {
    fn current(&self) -> Option<Vec3> {
        self.knots.last().map(|k| k.pos)
    }

    fn curve(&mut self, c1: Vec3, c2: Vec3, end: Vec3) {
        if let Some(k) = self.knots.last_mut() {
            k.out_vec = c1 - k.pos;
        }
        self.knots.push(Knot { pos: end, in_vec: c2 - end, out_vec: Vec3::ZERO, kind: KnotKind::Corner });
    }

    fn finish(mut self, closed: bool) -> Option<Spline> {
        // A closed path often repeats its first point at the end.
        if closed && self.knots.len() > 2 && self.knots.first().map(|k| k.pos) == self.knots.last().map(|k| k.pos) {
            let last = self.knots.pop().unwrap();
            self.knots[0].in_vec = last.in_vec;
        }
        if self.knots.len() < 2 {
            return None;
        }
        for k in &mut self.knots {
            if k.in_vec != Vec3::ZERO || k.out_vec != Vec3::ZERO {
                k.kind = KnotKind::BezierCorner;
            }
        }
        Some(Spline::new(self.knots, closed))
    }
}

/// The splines of an Illustrator 88 file's text.
pub fn read(text: &str) -> Result<Vec<Spline>, String> {
    if !text.starts_with("%!PS-Adobe") {
        return Err("not an Adobe Illustrator file (no %!PS-Adobe header)".into());
    }
    let mut out = Vec::new();
    let mut path: Option<Path> = None;
    let mut nums: Vec<f32> = Vec::new();
    for line in text.lines() {
        if line.starts_with('%') {
            continue;
        }
        for tok in line.split_whitespace() {
            if let Ok(v) = tok.parse::<f32>() {
                nums.push(v);
                continue;
            }
            let p = |i: usize| Vec3::new(nums[nums.len() - i * 2], nums[nums.len() - i * 2 + 1], 0.0);
            match tok {
                "m" if nums.len() >= 2 => {
                    if let Some(prev) = path.take().and_then(|p| p.finish(false)) {
                        out.push(prev);
                    }
                    path = Some(Path { knots: vec![Knot::corner(p(1))] });
                }
                "l" | "L" if nums.len() >= 2 => {
                    if let Some(path) = path.as_mut() {
                        path.knots.push(Knot::corner(p(1)));
                    }
                }
                "c" | "C" if nums.len() >= 6 => {
                    if let Some(path) = path.as_mut() {
                        path.curve(p(3), p(2), p(1));
                    }
                }
                "v" | "V" if nums.len() >= 4 => {
                    if let Some(path) = path.as_mut() {
                        let cur = path.current().unwrap_or_default();
                        path.curve(cur, p(2), p(1));
                    }
                }
                "y" | "Y" if nums.len() >= 4 => {
                    if let Some(path) = path.as_mut() {
                        path.curve(p(2), p(1), p(1));
                    }
                }
                "s" | "f" | "F" | "b" | "B" | "h" | "H" | "n" => {
                    if let Some(s) = path.take().and_then(|p| p.finish(true)) {
                        out.push(s);
                    }
                }
                "S" | "N" => {
                    if let Some(s) = path.take().and_then(|p| p.finish(false)) {
                        out.push(s);
                    }
                }
                _ => {}
            }
            nums.clear();
        }
    }
    if let Some(s) = path.and_then(|p| p.finish(false)) {
        out.push(s);
    }
    if out.is_empty() {
        return Err("no paths".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQUARE_AND_ARC: &str = "%!PS-Adobe-2.0 EPSF-1.2\n%%Creator:Adobe Illustrator(TM) 1.0\n%%BoundingBox:0 0 200 100\n%%EndProlog\n%%BeginSetup\n%%EndSetup\n0 0 m\n100 0 l\n100 100 l\n0 100 l\n0 0 l\nf\n120 0 m\n120 55 165 100 200 100 c\nS\n%%Trailer\n";

    /// A filled square closes (its repeated first point merged); a stroked
    /// curve stays open with Bezier handles from its control points.
    #[test]
    fn a_closed_square_and_an_open_curve() {
        let s = read(SQUARE_AND_ARC).unwrap();
        assert_eq!(s.len(), 2);
        assert!(s[0].closed && s[0].knots.len() == 4, "{:?}", s[0]);
        assert!(s[0].knots.iter().all(|k| k.kind == KnotKind::Corner));
        assert_eq!(s[0].knots[2].pos, Vec3::new(100.0, 100.0, 0.0));
        assert!(!s[1].closed && s[1].knots.len() == 2);
        assert_eq!(s[1].knots[0].out_vec, Vec3::new(0.0, 55.0, 0.0));
        assert_eq!(s[1].knots[1].in_vec, Vec3::new(-35.0, 0.0, 0.0));
        assert_eq!(s[1].knots[1].kind, KnotKind::BezierCorner);
        assert!(read("hello").is_err());
    }
}
