//! Editable spline geometry (SplineShape, and the Line object, which keeps
//! the same chunks one container deeper under 0x2000): a 0x1010 container
//! with one 0x1010 per spline { 0x2900 knot count, 0x290A knots, 0x290D
//! closed } beside 0x1060, the interpolation steps. A knot is 52 bytes:
//! u32 type (0 smooth, 1 corner, 2 Bezier, 3 Bezier corner), u32, f32, then
//! the knot, its in handle and its out handle (absolute, three f32 each), u32.

use crate::chunk::Chunk;
use glam::Vec3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KnotType {
    Smooth,
    Corner,
    Bezier,
    BezierCorner,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MaxKnot {
    pub kind: KnotType,
    pub pos: Vec3,
    /// Handles, absolute (object space).
    pub in_handle: Vec3,
    pub out_handle: Vec3,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MaxSpline {
    pub knots: Vec<MaxKnot>,
    pub closed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MaxShape {
    pub splines: Vec<MaxSpline>,
    pub steps: u32,
}

const KNOT: usize = 52;

/// The shape in `obj` (its own 0x1010, or the one under 0x2000).
pub fn read(obj: &Chunk) -> Result<MaxShape, String> {
    let holder = if obj.child(0x1010).is_some() { obj } else { obj.child(0x2000).filter(|c| c.child(0x1010).is_some()).ok_or("no spline data")? };
    let list = holder.child(0x1010).ok_or("no spline data")?;
    let steps = holder.child(0x1060).and_then(|c| c.u32s().first().copied()).unwrap_or(6).min(100);
    let mut splines = Vec::new();
    for sp in list.kids().iter().filter(|c| c.id == 0x1010) {
        let n = sp.child(0x2900).and_then(|c| c.u32s().first().copied()).ok_or("a spline without a knot count")? as usize;
        let data = &sp.child(0x290a).ok_or("a spline without knots")?.data;
        if data.len() != n.checked_mul(KNOT).ok_or("knot count too large")? {
            return Err(format!("a spline's knot data is {} bytes for {n} knots", data.len()));
        }
        let closed = sp.child(0x290d).and_then(|c| c.u32s().first().copied()).unwrap_or(0) != 0;
        let mut knots = Vec::with_capacity(n);
        for k in data.chunks_exact(KNOT) {
            let u = |o: usize| u32::from_le_bytes(k[o..o + 4].try_into().unwrap());
            let f = |o: usize| f32::from_le_bytes(k[o..o + 4].try_into().unwrap());
            let v = |o: usize| Vec3::new(f(o), f(o + 4), f(o + 8));
            let kind = match u(0) {
                0 => KnotType::Smooth,
                1 => KnotType::Corner,
                2 => KnotType::Bezier,
                3 => KnotType::BezierCorner,
                t => return Err(format!("unknown knot type {t}")),
            };
            let (pos, in_handle, out_handle) = (v(12), v(24), v(36));
            if ![pos, in_handle, out_handle].iter().all(|p| p.is_finite()) {
                return Err("a knot is not a number".into());
            }
            knots.push(MaxKnot { kind, pos, in_handle, out_handle });
        }
        splines.push(MaxSpline { knots, closed });
    }
    Ok(MaxShape { splines, steps })
}
