//! Vol. Select, Delete Spline and Normalize Spline.

use crate::modifier::{EvalCtx, StackValue, VolSelectData};
use glam::Vec3;
use loom_mesh::select_ops::SubLevel;
use loom_mesh::{FaceId, VertexId};
use loom_shape::{Knot, Shape, Spline};

/// Whether `p` (object space) is inside the volume.
fn inside(d: &VolSelectData, p: Vec3, lo: Vec3, hi: Vec3, target: Option<&loom_mesh::Mesh>) -> bool {
    let q = d.gizmo.matrix().inverse().transform_point3(p);
    let c = (lo + hi) * 0.5;
    let half = ((hi - lo) * 0.5).max(Vec3::splat(1e-6));
    match d.volume {
        1 => (q - c).length() <= half.max_element() + 1e-5,
        2 => {
            let r = half.x.max(half.y);
            let xy = Vec3::new(q.x - c.x, q.y - c.y, 0.0).length();
            xy <= r + 1e-5 && (q.z - c.z).abs() <= half.z + 1e-5
        }
        3 => target.is_some_and(|m| point_in_mesh(m, p)),
        _ => (q - c).abs().cmple(half + Vec3::splat(1e-5)).all(),
    }
}

/// Parity of crossings along +X against the mesh's triangulated faces.
fn point_in_mesh(m: &loom_mesh::Mesh, p: Vec3) -> bool {
    let mut hits = 0;
    for f in 0..m.face_count() {
        let Some(pts) = m.face_positions(FaceId(f as u32)) else { continue };
        for k in 1..pts.len().saturating_sub(1) {
            let (a, b, c) = (pts[0], pts[k], pts[k + 1]);
            // Ray p + t·X: solve against the triangle (Möller–Trumbore).
            let dir = Vec3::X;
            let e1 = b - a;
            let e2 = c - a;
            let h = dir.cross(e2);
            let det = e1.dot(h);
            if det.abs() < 1e-9 {
                continue;
            }
            let inv = 1.0 / det;
            let s = p - a;
            let u = inv * s.dot(h);
            if !(0.0..=1.0).contains(&u) {
                continue;
            }
            let q = s.cross(e1);
            let v = inv * dir.dot(q);
            if v < 0.0 || u + v > 1.0 {
                continue;
            }
            if inv * e2.dot(q) > 1e-6 {
                hits += 1;
            }
        }
    }
    hits % 2 == 1
}

pub fn vol_select(d: &VolSelectData, mut v: StackValue, ctx: &EvalCtx<'_>) -> Result<StackValue, String> {
    if d.level == 0 {
        v.selection = None;
        return Ok(v);
    }
    let m = &v.mesh;
    let (lo, hi) = m.aabb();
    // A Mesh Object volume, brought into this object's space.
    let target = if d.volume == 3 {
        let node = d.node.ok_or("Vol. Select > Mesh Object: pick an object")?;
        let (world, mesh) = ctx.lookup.and_then(|l| l(node)).ok_or("Vol. Select: the picked object is gone")?;
        let mut mesh = mesh;
        let to_local = ctx.object_inverse * world;
        let _ = loom_mesh::edit_ops::transform_mesh(&mut mesh, to_local);
        Some(mesh)
    } else {
        None
    };
    let face_hit = |f: usize| -> bool {
        let face = &m.faces()[f];
        match d.select_by {
            1 => face.material_id as u32 == d.material_id,
            2 => face.smoothing_groups & (1u32 << (d.smoothing_group.clamp(1, 32) - 1)) != 0,
            _ => {
                let vs = m.face_vertex_ids(FaceId(f as u32)).unwrap_or_default();
                let ins = |vv: &VertexId| inside(d, m.vertex_position(*vv).unwrap_or_default(), lo, hi, target.as_ref());
                if d.crossing { vs.iter().any(ins) } else { !vs.is_empty() && vs.iter().all(ins) }
            }
        }
    };
    let (level, picked): (SubLevel, Vec<u32>) = if d.level == 1 {
        let ids: Vec<u32> = if d.select_by == 0 {
            (0..m.vertex_count() as u32).filter(|i| inside(d, m.vertex_position(VertexId(*i)).unwrap_or_default(), lo, hi, target.as_ref())).collect()
        } else {
            let mut s: std::collections::BTreeSet<u32> = Default::default();
            for f in (0..m.face_count()).filter(|f| face_hit(*f)) {
                s.extend(m.face_vertex_ids(FaceId(f as u32)).unwrap_or_default().iter().map(|v| v.0));
            }
            s.into_iter().collect()
        };
        (SubLevel::Vertex, ids)
    } else {
        (SubLevel::Face, (0..m.face_count()).filter(|f| face_hit(*f)).map(|f| f as u32).collect())
    };
    let count = if level == SubLevel::Vertex { m.vertex_count() } else { m.face_count() };
    let mut set: std::collections::BTreeSet<u32> = picked.into_iter().collect();
    if d.invert {
        set = (0..count as u32).filter(|i| !set.contains(i)).collect();
    }
    let incoming: std::collections::BTreeSet<u32> = match &v.selection {
        Some((l, ids)) if *l == level => ids.iter().copied().collect(),
        _ => Default::default(),
    };
    let out: Vec<u32> = match d.method {
        1 => incoming.union(&set).copied().collect(),
        2 => incoming.difference(&set).copied().collect(),
        _ => set.into_iter().collect(),
    };
    v.selection = Some((level, out));
    Ok(v)
}

/// Delete Spline: the incoming spline selection, or the whole shape without one.
pub fn delete_spline(mut v: StackValue) -> Result<StackValue, String> {
    let sh = v.shape.take().ok_or("Delete Spline needs a shape below it")?;
    let out = match v.shape_selection.take() {
        None => Shape { splines: Vec::new(), ..sh.clone() },
        Some((level, ids)) => {
            let ids: std::collections::BTreeSet<u32> = ids.into_iter().collect();
            let mut splines = Vec::new();
            let mut flat_knot = 0u32;
            let mut flat_seg = 0u32;
            for (si, sp) in sh.splines.iter().enumerate() {
                let nk = sp.knots.len() as u32;
                let ns = sp.segment_count() as u32;
                match level {
                    3 => {
                        if !ids.contains(&(si as u32)) {
                            splines.push(sp.clone());
                        }
                    }
                    1 => {
                        let keep: Vec<Knot> = sp.knots.iter().enumerate().filter(|(k, _)| !ids.contains(&(flat_knot + *k as u32))).map(|(_, k)| k.clone()).collect();
                        if keep.len() >= 2 {
                            splines.push(Spline::new(keep, sp.closed));
                        }
                    }
                    _ => {
                        // Segments: what stays of the spline splits into open runs.
                        let gone: Vec<bool> = (0..ns).map(|s| ids.contains(&(flat_seg + s))).collect();
                        if !gone.iter().any(|g| *g) {
                            splines.push(sp.clone());
                        } else {
                            let n = sp.knots.len();
                            // Start a run after a deleted segment so a closed spline unrolls once.
                            let start = gone.iter().position(|g| *g).map(|g| (g + 1) % ns as usize).unwrap_or(0);
                            let mut run: Vec<Knot> = Vec::new();
                            for k in 0..ns as usize {
                                let s = (start + k) % ns as usize;
                                if gone[s] {
                                    if run.len() >= 2 {
                                        splines.push(Spline::new(std::mem::take(&mut run), false));
                                    }
                                    run.clear();
                                    continue;
                                }
                                if run.is_empty() {
                                    run.push(sp.knots[s % n].clone());
                                }
                                run.push(sp.knots[(s + 1) % n].clone());
                            }
                            if run.len() >= 2 {
                                splines.push(Spline::new(run, false));
                            }
                        }
                    }
                }
                flat_knot += nk;
                flat_seg += ns;
            }
            Shape { splines, ..sh.clone() }
        }
    };
    v.mesh = crate::node::shape_cage_mesh(&out, false);
    v.shape = Some(out);
    Ok(v)
}

/// Normalize Spline: knots every `seg` along each spline (smooth knots).
pub fn normalize_spline(mut v: StackValue, seg: f32) -> Result<StackValue, String> {
    let sh = v.shape.take().ok_or("Normalize Spline needs a shape below it")?;
    let mut splines = Vec::new();
    for sp in &sh.splines {
        let ns = sp.segment_count();
        if ns == 0 {
            splines.push(sp.clone());
            continue;
        }
        let lens: Vec<f32> = (0..ns).map(|i| loom_shape::interp::length(&sp.segment(i).unwrap(), 64)).collect();
        let total: f32 = lens.iter().sum();
        let n = ((total / seg.max(1e-4)).round() as usize).max(if sp.closed { 3 } else { 1 });
        let step = total / n as f32;
        let at = |d: f32| -> Vec3 {
            let mut d = d;
            for (i, l) in lens.iter().enumerate() {
                if d <= *l || i == ns - 1 {
                    let b = sp.segment(i).unwrap();
                    return loom_shape::interp::point_at(&b, loom_shape::interp::t_at_length(&b, d.min(*l)));
                }
                d -= l;
            }
            sp.knots.last().map(|k| k.pos).unwrap_or_default()
        };
        let count = if sp.closed { n } else { n + 1 };
        let knots: Vec<Knot> = (0..count).map(|k| Knot::smooth(at(k as f32 * step))).collect();
        splines.push(Spline::new(knots, sp.closed));
    }
    let out = Shape { splines, ..sh.clone() };
    v.mesh = crate::node::shape_cage_mesh(&out, false);
    v.shape = Some(out);
    v.shape_selection = None;
    Ok(v)
}

/// Fillet/Chamfer: replays its operations on the incoming shape. Every
/// operation names input vertices (what the Vertex level shows); a vertex
/// keeps the last operation applied to it.
pub fn fillet_chamfer(d: &crate::modifier::FilletChamferData, mut v: StackValue) -> Result<StackValue, String> {
    let mut sh = v.shape.take().ok_or("Fillet/Chamfer needs a shape below it")?;
    if !d.ops.is_empty() && d.knots != 0 && sh.knot_count() != d.knots {
        v.warnings.push("Fillet/Chamfer: the spline below changed its knots; the operations were dropped".into());
    } else {
        let mut last: std::collections::BTreeMap<(usize, usize), (f32, bool)> = Default::default();
        for op in &d.ops {
            for k in &op.knots {
                if let Some(at) = sh.knot_at(*k as usize) {
                    last.insert(at, (op.amount, op.fillet));
                }
            }
        }
        // Highest first: inserting knots after a vertex keeps the lower indices valid.
        for ((si, k), (amount, fillet)) in last.into_iter().rev() {
            let _ = loom_shape::outline::fillet_or_chamfer(&mut sh.splines[si], k, amount, fillet);
        }
    }
    v.mesh = crate::node::shape_cage_mesh(&sh, false);
    v.shape = Some(sh);
    v.shape_selection = None;
    Ok(v)
}
