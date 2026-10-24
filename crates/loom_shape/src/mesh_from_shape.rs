//! Meshes from shapes: Extrude, Lathe, Bevel and Sweep (the modifiers),
//! with capping of polygons with holes (nested closed splines).

use crate::spline::Shape;
use glam::{Quat, Vec2, Vec3};
use loom_mesh::indexed::{IndexedFace, IndexedMesh};
use loom_mesh::Mesh;
use std::collections::BTreeSet;

fn signed_area(p: &[Vec2]) -> f32 {
    let n = p.len();
    (0..n).map(|i| p[i].perp_dot(p[(i + 1) % n])).sum::<f32>() * 0.5
}

fn point_in_polygon(pt: Vec2, poly: &[Vec2]) -> bool {
    let n = poly.len();
    let mut inside = false;
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > pt.y) != (b.y > pt.y) && pt.x < (b.x - a.x) * (pt.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Ear clipping of a simple 2D polygon (counter-clockwise), as index triples.
fn ear_clip(pts: &[Vec2]) -> Vec<[usize; 3]> {
    let mut ring: Vec<usize> = (0..pts.len()).collect();
    let mut tris = Vec::new();
    let mut guard = 0;
    while ring.len() > 3 && guard < 20_000 {
        guard += 1;
        let m = ring.len();
        let mut clipped = false;
        for i in 0..m {
            let (a, b, c) = (ring[(i + m - 1) % m], ring[i], ring[(i + 1) % m]);
            let (pa, pb, pc) = (pts[a], pts[b], pts[c]);
            if (pb - pa).perp_dot(pc - pb) <= 1e-9 {
                continue;
            }
            let inside = ring.iter().any(|v| {
                if *v == a || *v == b || *v == c {
                    return false;
                }
                let p = pts[*v];
                // Bridge duplicates sit exactly on a corner: not inside.
                if p.distance_squared(pa) < 1e-10 || p.distance_squared(pb) < 1e-10 || p.distance_squared(pc) < 1e-10 {
                    return false;
                }
                (pb - pa).perp_dot(p - pa) > 1e-9 && (pc - pb).perp_dot(p - pb) > 1e-9 && (pa - pc).perp_dot(p - pc) > 1e-9
            });
            if inside {
                continue;
            }
            tris.push([a, b, c]);
            ring.remove(i);
            clipped = true;
            break;
        }
        if !clipped {
            for i in 1..ring.len() - 1 {
                tris.push([ring[0], ring[i], ring[i + 1]]);
            }
            return tris;
        }
    }
    if ring.len() == 3 {
        tris.push([ring[0], ring[1], ring[2]]);
    }
    tris
}

/// Triangulates an outer loop with holes (2D): holes are bridged into the
/// outer ring, then ear-clipped. Returns index triples into the
/// concatenation `outer ++ holes[0] ++ holes[1] …`.
pub fn cap_polygon(outer: &[Vec2], holes: &[Vec<Vec2>]) -> Vec<[usize; 3]> {
    if outer.len() < 3 {
        return Vec::new();
    }
    // Work on a ring of global indices so bridges keep the originals.
    let mut all: Vec<Vec2> = outer.to_vec();
    let mut ring: Vec<usize> = (0..outer.len()).collect();
    let mut hole_ranges = Vec::new();
    for h in holes {
        let start = all.len();
        all.extend(h.iter().copied());
        hole_ranges.push((start, h.len()));
    }
    // Bridge holes from the right-most one inward.
    let mut order: Vec<usize> = (0..holes.len()).collect();
    order.sort_by(|a, b| {
        let ma = holes[*a].iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
        let mb = holes[*b].iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
        mb.total_cmp(&ma)
    });
    for hi in order {
        let (start, len) = hole_ranges[hi];
        if len < 3 {
            continue;
        }
        // Hole vertex with the largest x, and the nearest ring vertex to its right.
        let hv = (0..len).max_by(|a, b| all[start + a].x.total_cmp(&all[start + b].x)).unwrap();
        let hp = all[start + hv];
        let rpos = (0..ring.len()).min_by(|a, b| all[ring[*a]].distance_squared(hp).total_cmp(&all[ring[*b]].distance_squared(hp))).unwrap();
        // Splice: ring[..=rpos], hole from hv around (clockwise for a hole), back to hv, ring[rpos..].
        let mut hole_seq: Vec<usize> = (0..len).map(|k| start + (hv + k) % len).collect();
        // The hole loop must run opposite to the outer loop.
        let hole_pts: Vec<Vec2> = hole_seq.iter().map(|i| all[*i]).collect();
        if signed_area(&hole_pts) > 0.0 {
            hole_seq[1..].reverse();
        }
        hole_seq.push(start + hv);
        let mut new_ring = ring[..=rpos].to_vec();
        new_ring.extend(hole_seq);
        new_ring.extend(ring[rpos..].iter().copied());
        ring = new_ring;
    }
    let pts: Vec<Vec2> = ring.iter().map(|i| all[*i]).collect();
    ear_clip(&pts).into_iter().map(|[a, b, c]| [ring[a], ring[b], ring[c]]).collect()
}

/// Polylines with zero-length edges removed (duplicate consecutive knots
/// would make degenerate faces).
fn clean_polylines(shape: &Shape) -> Vec<Vec<Vec3>> {
    shape
        .polylines()
        .into_iter()
        .map(|pl| {
            let mut out: Vec<Vec3> = Vec::with_capacity(pl.len());
            for p in pl {
                if out.last().is_none_or(|l| l.distance(p) > 1e-6) {
                    out.push(p);
                }
            }
            while out.len() > 1 && out.first().unwrap().distance(*out.last().unwrap()) <= 1e-6 {
                out.pop();
            }
            out
        })
        .collect()
}

/// Closed polylines of a shape grouped as (outer, holes) by nesting depth.
fn nested_loops(shape: &Shape) -> Vec<(Vec<Vec3>, Vec<Vec<Vec3>>)> {
    let loops: Vec<Vec<Vec3>> = shape.splines.iter().zip(clean_polylines(shape)).filter(|(s, pl)| s.closed && pl.len() >= 3).map(|(_, pl)| pl).collect();
    let flat: Vec<Vec<Vec2>> = loops.iter().map(|l| l.iter().map(|p| p.truncate()).collect()).collect();
    let depth: Vec<usize> = (0..loops.len()).map(|i| (0..loops.len()).filter(|j| *j != i && point_in_polygon(flat[i][0], &flat[*j])).count()).collect();
    let mut out: Vec<(Vec<Vec3>, Vec<Vec<Vec3>>)> = Vec::new();
    let outers: Vec<usize> = (0..loops.len()).filter(|i| depth[*i].is_multiple_of(2)).collect();
    for o in &outers {
        out.push((loops[*o].clone(), Vec::new()));
    }
    for i in 0..loops.len() {
        if depth[i] % 2 == 1 {
            // The smallest outer containing it (deepest even ancestor).
            if let Some((k, _)) = outers.iter().enumerate().filter(|(_, o)| point_in_polygon(flat[i][0], &flat[**o])).max_by_key(|(_, o)| depth[**o]) {
                out[k].1.push(loops[i].clone());
            }
        }
    }
    out
}

/// Adds the cap triangles of (outer, holes) at `z` (in the XY plane of the
/// shape) to `im`; `flip` turns them to face -Z.
fn add_cap(im: &mut IndexedMesh, outer: &[Vec3], holes: &[Vec<Vec3>], offset: Vec3, flip: bool, mat: u16, sg: u32) {
    let mut o2: Vec<Vec2> = outer.iter().map(|p| p.truncate()).collect();
    let mut outer3: Vec<Vec3> = outer.to_vec();
    if signed_area(&o2) < 0.0 {
        o2.reverse();
        outer3.reverse();
    }
    let holes2: Vec<Vec<Vec2>> = holes.iter().map(|h| h.iter().map(|p| p.truncate()).collect()).collect();
    let tris = cap_polygon(&o2, &holes2);
    let mut all3: Vec<Vec3> = outer3;
    for h in holes {
        all3.extend(h.iter().copied());
    }
    let base: Vec<u32> = all3.iter().map(|p| im.add_vertex(*p + offset, None)).collect();
    for [a, b, c] in tris {
        let ids = if flip { vec![base[c], base[b], base[a]] } else { vec![base[a], base[b], base[c]] };
        im.faces.push(IndexedFace { ids, smoothing_groups: sg, material_id: mat, hidden: false, uv: Default::default() });
    }
}

fn is_ccw(pl: &[Vec3]) -> bool {
    signed_area(&pl.iter().map(|p| p.truncate()).collect::<Vec<_>>()) > 0.0
}

/// Extrude: every polyline sweeps along local Z by `amount` in `segments`
/// rings; closed outlines get caps (holes from nested splines).
pub fn extrude(shape: &Shape, amount: f32, segments: u32, cap_start: bool, cap_end: bool) -> Result<Mesh, String> {
    let segs = segments.max(1) as usize;
    let mut im = IndexedMesh::default();
    // Nesting: outer outlines run counter-clockwise (sides face out), holes clockwise (sides face in).
    let polylines = clean_polylines(shape);
    let flat: Vec<Vec<Vec2>> = polylines.iter().map(|l| l.iter().map(|p| p.truncate()).collect()).collect();
    let is_hole: Vec<bool> = (0..polylines.len())
        .map(|i| {
            if !shape.splines[i].closed || flat[i].len() < 3 {
                return false;
            }
            (0..polylines.len()).filter(|j| *j != i && shape.splines[*j].closed && flat[*j].len() >= 3 && point_in_polygon(flat[i][0], &flat[*j])).count() % 2 == 1
        })
        .collect();
    for (si, (s, pl)) in shape.splines.iter().zip(polylines).enumerate() {
        if pl.len() < 2 {
            continue;
        }
        let n = pl.len();
        let want_ccw = !is_hole[si];
        let _ = si;
        let pl: Vec<Vec3> = if s.closed && is_ccw(&pl) != want_ccw { pl.iter().rev().copied().collect() } else { pl };
        let base = im.positions.len() as u32;
        for k in 0..=segs {
            let z = amount * k as f32 / segs as f32;
            for p in &pl {
                im.add_vertex(*p + Vec3::Z * z, None);
            }
        }
        let rings = segs;
        let edges = if s.closed { n } else { n - 1 };
        for k in 0..rings {
            let (a, b) = (base + (k * n) as u32, base + ((k + 1) * n) as u32);
            for i in 0..edges {
                let i2 = (i + 1) % n;
                let ids = vec![a + i as u32, a + i2 as u32, b + i2 as u32, b + i as u32];
                im.faces.push(IndexedFace { ids, smoothing_groups: 2, material_id: 3, hidden: false, uv: Default::default() });
            }
        }
    }
    let sides_only = im.clone();
    for (outer, holes) in nested_loops(shape) {
        if cap_start {
            add_cap(&mut im, &outer, &holes, Vec3::ZERO, true, 1, 1);
        }
        if cap_end {
            add_cap(&mut im, &outer, &holes, Vec3::Z * amount, false, 2, 1);
        }
    }
    if amount < 0.0 {
        reverse_all(&mut im);
    }
    build_or_uncapped(im, sides_only, amount < 0.0)
}

/// Builds the capped mesh; a self-intersecting outline whose caps would
/// not be manifold builds without caps instead.
fn build_or_uncapped(capped: IndexedMesh, mut sides: IndexedMesh, reversed: bool) -> Result<Mesh, String> {
    match merge_vertices(&capped) {
        Ok(m) => Ok(m),
        Err(_) => {
            if reversed {
                reverse_all(&mut sides);
            }
            merge_vertices(&sides)
        }
    }
}

/// Reverses every face of an indexed mesh.
fn reverse_all(im: &mut IndexedMesh) {
    for f in &mut im.faces {
        f.ids.reverse();
    }
}

/// Whether the faces mostly point away from the mesh centre.
fn faces_outward(im: &IndexedMesh) -> bool {
    let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for p in &im.positions {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let c = (lo + hi) * 0.5;
    let mut score = 0.0;
    for f in &im.faces {
        let pts: Vec<Vec3> = f.ids.iter().map(|i| im.positions[*i as usize]).collect();
        if pts.len() < 3 {
            continue;
        }
        let mut n = Vec3::ZERO;
        for i in 0..pts.len() {
            n += pts[i].cross(pts[(i + 1) % pts.len()]);
        }
        let fc = pts.iter().sum::<Vec3>() / pts.len() as f32;
        score += n.dot(fc - c);
    }
    score >= 0.0
}

/// Welds coincident vertices (caps share the outline) and builds the mesh.
fn merge_vertices(im: &IndexedMesh) -> Result<Mesh, String> {
    let mut out = IndexedMesh::default();
    let mut map: Vec<u32> = Vec::with_capacity(im.positions.len());
    let mut by_key: std::collections::HashMap<(i64, i64, i64), u32> = std::collections::HashMap::new();
    for p in &im.positions {
        let key = ((p.x * 1e4).round() as i64, (p.y * 1e4).round() as i64, (p.z * 1e4).round() as i64);
        let id = *by_key.entry(key).or_insert_with(|| out.add_vertex(*p, None));
        map.push(id);
    }
    for f in &im.faces {
        let mut ids: Vec<u32> = f.ids.iter().map(|i| map[*i as usize]).collect();
        ids.dedup();
        while ids.len() > 1 && ids.first() == ids.last() {
            ids.pop();
        }
        if ids.len() >= 3 {
            out.faces.push(IndexedFace::like(f, ids));
        }
    }
    out.build()
}

/// Lathe: the polylines revolve `degrees` about an axis (0 X, 1 Y, 2 Z)
/// placed at the shape's Min / Center / Max (`align` 0 / 1 / 2) across that
/// axis, in `segments` steps. Points on the axis weld into a core.
#[allow(clippy::too_many_arguments)]
pub fn lathe(shape: &Shape, degrees: f32, segments: u32, cap_start: bool, cap_end: bool, weld_core: bool, flip_normals: bool, axis: u32, align: u32) -> Result<Mesh, String> {
    let segs = segments.max(3) as usize;
    let full = (degrees - 360.0).abs() < 1e-3 || degrees >= 360.0;
    let axis_dir = match axis { 0 => Vec3::X, 1 => Vec3::Y, _ => Vec3::Z };
    // Alignment: the axis passes through the min / centre / max of the shape
    // across it, or (3, before an Align button is pressed) the pivot.
    let (lo, hi) = shape.aabb();
    let across = if axis == 0 { Vec3::Y } else { Vec3::X };
    let pick = |v: Vec3| v.dot(across);
    let a0 = match align { 0 => pick(lo), 2 => pick(hi), 3 => 0.0, _ => (pick(lo) + pick(hi)) * 0.5 };
    let origin = across * a0;
    let mut im = IndexedMesh::default();
    let steps = segs;
    let ring_count = if full { segs } else { segs + 1 };
    for (s, pl) in shape.splines.iter().zip(shape.polylines()) {
        if pl.len() < 2 {
            continue;
        }
        let n = pl.len();
        let base = im.positions.len() as u32;
        let mut on_axis: Vec<bool> = Vec::with_capacity(n);
        for k in 0..ring_count {
            let ang = degrees.to_radians() * k as f32 / steps as f32;
            let rot = Quat::from_axis_angle(axis_dir, ang);
            for p in &pl {
                let rel = *p - origin;
                let radial = rel - axis_dir * rel.dot(axis_dir);
                let core = weld_core && radial.length() < 1e-4;
                if k == 0 {
                    on_axis.push(core);
                }
                im.add_vertex(origin + rot * rel, None);
            }
        }
        let edges = if s.closed { n } else { n - 1 };
        for k in 0..steps {
            let (a, b) = (base + (k * n) as u32, base + (((k + 1) % ring_count) * n) as u32);
            for i in 0..edges {
                let i2 = (i + 1) % n;
                let ids = vec![a + i as u32, b + i as u32, b + i2 as u32, a + i2 as u32];
                im.faces.push(IndexedFace { ids, smoothing_groups: 1, material_id: 1, hidden: false, uv: Default::default() });
            }
        }
        // Core weld: every ring's copy of an axis point is the same vertex.
        for (i, core) in on_axis.iter().enumerate() {
            if *core {
                for k in 1..ring_count {
                    im.remap(base + (k * n + i) as u32, base + i as u32);
                }
            }
        }
    }
    im.drop_degenerate();
    let sides_only = im.clone();
    if !full {
        // Caps: the closed profiles at the start and end angles.
        for (outer, holes) in nested_loops(shape) {
            let place = |pts: &[Vec3], ang: f32| -> Vec<Vec3> {
                let rot = Quat::from_axis_angle(axis_dir, ang);
                pts.iter().map(|p| origin + rot * (*p - origin)).collect()
            };
            if cap_start {
                add_cap_3d(&mut im, &place(&outer, 0.0), &holes.iter().map(|h| place(h, 0.0)).collect::<Vec<_>>(), -axis_dir.cross(Vec3::ONE).normalize_or_zero(), true, 2);
            }
            if cap_end {
                let ang = degrees.to_radians();
                add_cap_3d(&mut im, &place(&outer, ang), &holes.iter().map(|h| place(h, ang)).collect::<Vec<_>>(), Vec3::ZERO, false, 3);
            }
        }
    }
    // Lathes of a profile drawn on the far side face inward: orient outward, then honour Flip Normals.
    let flip = faces_outward(&im) == flip_normals;
    if flip {
        reverse_all(&mut im);
    }
    if weld_core {
        build_or_uncapped(im, sides_only, flip)
    } else {
        // Weld Core off: the axis copies stay separate vertices (no welding by position).
        im.build().or_else(|_| {
            let mut s = sides_only;
            if flip {
                reverse_all(&mut s);
            }
            s.build()
        })
    }
}

/// Cap of a planar loop that is not in the XY plane: projects onto its
/// own plane for triangulation.
fn add_cap_3d(im: &mut IndexedMesh, outer: &[Vec3], holes: &[Vec<Vec3>], _hint: Vec3, flip: bool, mat: u16) {
    if outer.len() < 3 {
        return;
    }
    let mut normal = Vec3::ZERO;
    for i in 0..outer.len() {
        normal += outer[i].cross(outer[(i + 1) % outer.len()]);
    }
    let normal = normal.normalize_or_zero();
    if normal.length_squared() < 0.5 {
        return;
    }
    let u = if normal.x.abs() < 0.9 { Vec3::X } else { Vec3::Y }.cross(normal).normalize();
    let v = normal.cross(u);
    let to2 = |p: Vec3| Vec2::new(p.dot(u), p.dot(v));
    let mut o2: Vec<Vec2> = outer.iter().map(|p| to2(*p)).collect();
    let mut h2: Vec<Vec<Vec2>> = holes.iter().map(|h| h.iter().map(|p| to2(*p)).collect()).collect();
    // The triangulation follows the loop's own order: mirror the projection
    // so the outer reads counter-clockwise for the ear clipper.
    if signed_area(&o2) < 0.0 {
        for p in o2.iter_mut().chain(h2.iter_mut().flatten()) {
            p.y = -p.y;
        }
    }
    let tris = cap_polygon(&o2, &h2);
    let mut all: Vec<Vec3> = outer.to_vec();
    for h in holes {
        all.extend(h.iter().copied());
    }
    let base: Vec<u32> = all.iter().map(|p| im.add_vertex(*p, None)).collect();
    for [a, b, c] in tris {
        let ids = if flip { vec![base[c], base[b], base[a]] } else { vec![base[a], base[b], base[c]] };
        im.faces.push(IndexedFace { ids, smoothing_groups: 4, material_id: mat, hidden: false, uv: Default::default() });
    }
}

/// Offsets a closed 2D outline by `amount` (positive = outward for a
/// counter-clockwise loop), mitred.
fn offset_loop(pl: &[Vec3], amount: f32) -> Vec<Vec3> {
    let n = pl.len();
    let ccw = is_ccw(pl);
    (0..n)
        .map(|i| {
            let (prev, cur, next) = (pl[(i + n - 1) % n], pl[i], pl[(i + 1) % n]);
            let d0 = (cur - prev).truncate().normalize_or_zero();
            let d1 = (next - cur).truncate().normalize_or_zero();
            let n0 = Vec2::new(d0.y, -d0.x);
            let n1 = Vec2::new(d1.y, -d1.x);
            let m = n0 + n1;
            let denom = 1.0 + n0.dot(n1);
            let off = if denom.abs() < 1e-4 { n0 } else { m / denom };
            let sign = if ccw { 1.0 } else { -1.0 };
            cur + Vec3::new(off.x, off.y, 0.0) * amount * sign
        })
        .collect()
}

/// Bevel: the closed outlines rise through up to three levels, each with its
/// own height and outline offset (Level 1 / 2 / 3).
pub fn bevel(shape: &Shape, start_outline: f32, levels: &[(f32, f32)], cap_start: bool, cap_end: bool) -> Result<Mesh, String> {
    let levels: Vec<(f32, f32)> = levels.to_vec();
    if levels.is_empty() {
        return Err("Bevel needs at least one level".into());
    }
    let mut im = IndexedMesh::default();
    let mut top_loops: Vec<(Vec<Vec3>, Vec<Vec<Vec3>>)> = Vec::new();
    let mut top_z = 0.0;
    for (outer, holes) in nested_loops(shape) {
        let mut loops: Vec<(Vec<Vec3>, bool)> = vec![(outer.clone(), true)];
        loops.extend(holes.iter().map(|h| (h.clone(), false)));
        let mut rings_out: Vec<Vec<Vec3>> = Vec::new();
        for (pl, is_outer) in &loops {
            let pl: Vec<Vec3> = if *is_outer == is_ccw(pl) { pl.clone() } else { pl.iter().rev().copied().collect() };
            // Rings: start outline, then each level.
            let mut rings: Vec<Vec<Vec3>> = vec![offset_loop(&pl, start_outline)];
            let mut z = 0.0;
            let mut cur = rings[0].clone();
            for (h, o) in &levels {
                z += h;
                cur = offset_loop(&cur, *o).into_iter().map(|p| Vec3::new(p.x, p.y, z)).collect();
                rings.push(cur.clone());
            }
            top_z = z;
            let n = pl.len();
            let base = im.positions.len() as u32;
            for r in &rings {
                for p in r {
                    im.add_vertex(*p, None);
                }
            }
            for k in 0..rings.len() - 1 {
                let (a, b) = (base + (k * n) as u32, base + ((k + 1) * n) as u32);
                for i in 0..n {
                    let i2 = (i + 1) % n;
                    im.faces.push(IndexedFace { ids: vec![a + i as u32, a + i2 as u32, b + i2 as u32, b + i as u32], smoothing_groups: 2, material_id: 3, hidden: false, uv: Default::default() });
                }
            }
            rings_out.push(rings.last().unwrap().clone());
            if *is_outer {
                top_loops.push((rings.last().unwrap().clone(), Vec::new()));
            } else if let Some(last) = top_loops.last_mut() {
                last.1.push(rings.last().unwrap().clone());
            }
        }
        if cap_start {
            let o = offset_loop(&outer, start_outline);
            let hs: Vec<Vec<Vec3>> = holes.iter().map(|h| offset_loop(h, start_outline)).collect();
            add_cap(&mut im, &o, &hs, Vec3::ZERO, true, 1, 1);
        }
    }
    if cap_end {
        for (outer, holes) in &top_loops {
            add_cap_3d(&mut im, outer, holes, Vec3::Z, false, 2);
        }
    }
    let _ = top_z;
    merge_vertices(&im)
}

/// Built-in Sweep sections and a custom profile.
#[derive(Debug, Clone, PartialEq)]
pub enum Section {
    Angle { length: f32, width: f32, thickness: f32 },
    Bar { length: f32, width: f32 },
    Channel { length: f32, width: f32, thickness: f32 },
    Cylinder { radius: f32 },
    HalfRound { radius: f32 },
    Pipe { radius: f32, thickness: f32 },
    QuarterRound { radius: f32 },
    Tee { length: f32, width: f32, thickness: f32 },
    Tube { length: f32, width: f32, thickness: f32 },
    WideFlange { length: f32, width: f32, thickness: f32 },
    /// Loops of a custom section shape (first outer, then holes), in its XY plane.
    Custom(Vec<Vec<Vec3>>),
}

impl Section {
    pub const NAMES: [&'static str; 10] = ["Angle", "Bar", "Channel", "Cylinder", "Half Round", "Pipe", "Quarter Round", "Tee", "Tube", "Wide Flange"];

    fn arc(cx: f32, cy: f32, r: f32, from: f32, to: f32, n: usize) -> Vec<Vec2> {
        (0..=n).map(|i| {
            let a = (from + (to - from) * i as f32 / n as f32).to_radians();
            Vec2::new(cx + r * a.cos(), cy + r * a.sin())
        }).collect()
    }

    /// The section's loops (outer first, holes after), counter-clockwise outer.
    pub fn loops(&self) -> Vec<Vec<Vec2>> {
        let v = |x: f32, y: f32| Vec2::new(x, y);
        match *self {
            Section::Angle { length: l, width: w, thickness: t } => vec![vec![v(0.0, 0.0), v(w, 0.0), v(w, t), v(t, t), v(t, l), v(0.0, l)]],
            Section::Bar { length: l, width: w } => vec![vec![v(-w / 2.0, -l / 2.0), v(w / 2.0, -l / 2.0), v(w / 2.0, l / 2.0), v(-w / 2.0, l / 2.0)]],
            Section::Channel { length: l, width: w, thickness: t } => vec![vec![v(0.0, 0.0), v(w, 0.0), v(w, t), v(t, t), v(t, l - t), v(w, l - t), v(w, l), v(0.0, l)]],
            Section::Cylinder { radius: r } => vec![Self::arc(0.0, 0.0, r, 0.0, 360.0, 24).into_iter().take(24).collect()],
            Section::HalfRound { radius: r } => vec![Self::arc(0.0, 0.0, r, 0.0, 180.0, 12)],
            Section::Pipe { radius: r, thickness: t } => vec![
                Self::arc(0.0, 0.0, r, 0.0, 360.0, 24).into_iter().take(24).collect(),
                Self::arc(0.0, 0.0, (r - t).max(0.01), 0.0, 360.0, 24).into_iter().take(24).collect(),
            ],
            Section::QuarterRound { radius: r } => {
                let mut pts = vec![v(0.0, 0.0)];
                pts.extend(Self::arc(0.0, 0.0, r, 0.0, 90.0, 8));
                vec![pts]
            }
            Section::Tee { length: l, width: w, thickness: t } => vec![vec![v(-t / 2.0, 0.0), v(t / 2.0, 0.0), v(t / 2.0, l - t), v(w / 2.0, l - t), v(w / 2.0, l), v(-w / 2.0, l), v(-w / 2.0, l - t), v(-t / 2.0, l - t)]],
            Section::Tube { length: l, width: w, thickness: t } => vec![
                vec![v(-w / 2.0, -l / 2.0), v(w / 2.0, -l / 2.0), v(w / 2.0, l / 2.0), v(-w / 2.0, l / 2.0)],
                vec![v(-w / 2.0 + t, -l / 2.0 + t), v(w / 2.0 - t, -l / 2.0 + t), v(w / 2.0 - t, l / 2.0 - t), v(-w / 2.0 + t, l / 2.0 - t)],
            ],
            Section::WideFlange { length: l, width: w, thickness: t } => vec![vec![
                v(-w / 2.0, 0.0), v(w / 2.0, 0.0), v(w / 2.0, t), v(t / 2.0, t), v(t / 2.0, l - t), v(w / 2.0, l - t), v(w / 2.0, l), v(-w / 2.0, l), v(-w / 2.0, l - t), v(-t / 2.0, l - t), v(-t / 2.0, t), v(-w / 2.0, t),
            ]],
            Section::Custom(ref loops) => loops.iter().map(|l| l.iter().map(|p| p.truncate()).collect()).collect(),
        }
    }
}

/// Sweep parameters beyond the section.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SweepParams {
    pub mirror: bool,
    pub offset: Vec2,
    pub angle_deg: f32,
    pub banking: bool,
    pub smooth_section: bool,
    pub smooth_path: bool,
}

impl Default for SweepParams {
    fn default() -> Self {
        Self { mirror: false, offset: Vec2::ZERO, angle_deg: 0.0, banking: true, smooth_section: true, smooth_path: true }
    }
}

/// Sweep: the section travels along every polyline of the shape with
/// parallel-transported frames (open paths get end caps).
pub fn sweep(shape: &Shape, section: &Section, params: SweepParams) -> Result<Mesh, String> {
    let mut loops = section.loops();
    for l in &mut loops {
        if params.mirror {
            for p in l.iter_mut() {
                p.x = -p.x;
            }
            l.reverse();
        }
        let rot = Quat::from_rotation_z(params.angle_deg.to_radians());
        for p in l.iter_mut() {
            let q = rot * Vec3::new(p.x + params.offset.x, p.y + params.offset.y, 0.0);
            *p = Vec2::new(q.x, q.y);
        }
    }
    if loops.is_empty() {
        return Err("the section has no outline".into());
    }
    // Loops as 3D profile points in the frame's XY (X = sideways, Y = up / path normal).
    let profiles: Vec<Vec<Vec3>> = loops.iter().map(|l| l.iter().map(|p| Vec3::new(p.x, p.y, 0.0)).collect()).collect();
    let mut im = IndexedMesh::default();
    let mut caps_added = false;
    let mut sides_only = IndexedMesh::default();
    for (s, pts) in shape.splines.iter().zip(shape.polylines()) {
        let n = pts.len();
        if n < 2 {
            continue;
        }
        let tangent = |i: usize| -> Vec3 {
            let prev = if i == 0 { if s.closed { pts[n - 1] } else { pts[0] } } else { pts[i - 1] };
            let next = if i + 1 == n { if s.closed { pts[0] } else { pts[n - 1] } } else { pts[i + 1] };
            (next - prev).normalize_or_zero()
        };
        let mut frames: Vec<Quat> = Vec::with_capacity(n);
        let t0 = tangent(0);
        let up = if t0.z.abs() < 0.9 { Vec3::Z } else { Vec3::X };
        let side = up.cross(t0).normalize_or_zero();
        let up2 = t0.cross(side).normalize_or_zero();
        frames.push(Quat::from_mat3(&glam::Mat3::from_cols(side, up2, t0)));
        let mut prev_t = t0;
        for i in 1..n {
            let t = tangent(i);
            let step = if params.banking && t.length_squared() > 0.5 && prev_t.length_squared() > 0.5 { Quat::from_rotation_arc(prev_t, t) } else { Quat::IDENTITY };
            frames.push((step * frames[i - 1]).normalize());
            if t.length_squared() > 0.5 {
                prev_t = t;
            }
        }
        let sg_side = if params.smooth_section { 1 } else { 0 };
        for (li, profile) in profiles.iter().enumerate() {
            let m = profile.len();
            let base = im.positions.len() as u32;
            for i in 0..n {
                for p in profile {
                    im.add_vertex(pts[i] + frames[i] * *p, None);
                }
            }
            let rings = if s.closed { n } else { n - 1 };
            let hole = li > 0;
            for i in 0..rings {
                let (a, b) = (base + (i as u32) * m as u32, base + (((i + 1) % n) as u32) * m as u32);
                // Smooth Path off: every strip along the path is its own smoothing group (hard rings).
                let sg = if params.smooth_path { sg_side } else { 1 << ((i % 30) + 1) };
                for k in 0..m as u32 {
                    let k2 = (k + 1) % m as u32;
                    let ids = if hole { vec![a + k2, b + k2, b + k, a + k] } else { vec![a + k, b + k, b + k2, a + k2] };
                    im.faces.push(IndexedFace { ids, smoothing_groups: sg, material_id: 1, hidden: false, uv: Default::default() });
                }
            }
        }
        if !s.closed {
            caps_added = true;
            sides_only = im.clone();
            // The side quads walk the first ring backwards and the last ring forwards.
            let outer: Vec<Vec3> = profiles[0].iter().map(|p| pts[0] + frames[0] * *p).collect();
            let holes: Vec<Vec<Vec3>> = profiles[1..].iter().map(|h| h.iter().map(|p| pts[0] + frames[0] * *p).collect()).collect();
            add_cap_3d(&mut im, &outer, &holes, Vec3::ZERO, false, 2);
            let last = frames[n - 1];
            let outer: Vec<Vec3> = profiles[0].iter().map(|p| pts[n - 1] + last * *p).collect();
            let holes: Vec<Vec<Vec3>> = profiles[1..].iter().map(|h| h.iter().map(|p| pts[n - 1] + last * *p).collect()).collect();
            add_cap_3d(&mut im, &outer, &holes, Vec3::ZERO, true, 3);
        }
    }
    // Orient outward.
    let flip = !faces_outward(&im);
    if flip {
        reverse_all(&mut im);
    }
    let _ = BTreeSet::<u32>::new();
    merge_vertices(&im).or_else(|e| if caps_added { build_or_uncapped(IndexedMesh::default(), sides_only, flip) } else { Err(e) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parametric::{circle, rectangle};
    use crate::spline::{Knot, Spline};

    #[test]
    fn extrude_a_rectangle_gives_sides_and_caps_and_holes_cap() {
        let shape = Shape::from_splines(vec![rectangle(10.0, 20.0, 0.0)]);
        let m = extrude(&shape, 5.0, 1, true, true).unwrap();
        m.check_invariants().unwrap();
        assert_eq!(m.face_count(), 4 + 2 + 2, "4 sides, two triangle caps each");
        let (lo, hi) = m.aabb();
        assert!((hi.z - 5.0).abs() < 1e-5 && lo.z.abs() < 1e-5);
        assert!(loom_mesh::select_ops::border_edges(&m).is_empty(), "closed solid");
        let two = extrude(&shape, 5.0, 3, false, false).unwrap();
        assert_eq!(two.face_count(), 12);
        // A donut outline caps with a hole.
        let donut = Shape::from_splines(vec![circle(10.0), circle(4.0)]);
        let d = extrude(&donut, 2.0, 1, true, true).unwrap();
        d.check_invariants().unwrap();
        assert!(loom_mesh::select_ops::border_edges(&d).is_empty(), "the hole is capped as a ring");
        assert!(d.face_count() >= 56 + 2 * 54, "{}", d.face_count());
    }

    #[test]
    fn lathe_welds_the_core_and_caps_partial_turns() {
        // A profile from the axis outward: lathed 360° about Y with Min alignment it is a closed surface.
        let profile = Spline::new(vec![Knot::corner(Vec3::ZERO), Knot::corner(Vec3::new(5.0, 0.0, 0.0)), Knot::corner(Vec3::new(5.0, 10.0, 0.0)), Knot::corner(Vec3::new(0.0, 10.0, 0.0))], false);
        let shape = Shape::from_splines(vec![profile]);
        let m = lathe(&shape, 360.0, 16, true, true, true, false, 1, 0).unwrap();
        m.check_invariants().unwrap();
        assert!(loom_mesh::select_ops::border_edges(&m).is_empty(), "a full lathe with welded core closes");
        assert_eq!(m.face_count(), 16 * 3, "{}", m.face_count());
        let half = lathe(&shape, 180.0, 8, false, false, true, false, 1, 0).unwrap();
        half.check_invariants().unwrap();
        assert_eq!(half.face_count(), 8 * 3);
        // Outward normals.
        let (lo, hi) = m.aabb();
        let c = (lo + hi) * 0.5;
        for f in 0..m.face_count() {
            let id = loom_mesh::FaceId(f as u32);
            let fc = m.face_positions(id).unwrap().iter().sum::<Vec3>() / 4.0;
            assert!(m.face_normal(id).dot(fc - c) > -1e-3, "face {f} points in");
        }
    }

    #[test]
    fn bevel_levels_and_sweep_bar() {
        let shape = Shape::from_splines(vec![rectangle(10.0, 10.0, 0.0)]);
        let b = bevel(&shape, 0.0, &[(2.0, -1.0), (1.0, 0.0), (2.0, 1.0)], true, true).unwrap();
        b.check_invariants().unwrap();
        assert_eq!(b.face_count(), 3 * 4 + 2 + 2);
        assert!((b.aabb().1.z - 5.0).abs() < 1e-5);
        let path = Shape::from_splines(vec![Spline::new(vec![Knot::corner(Vec3::ZERO), Knot::corner(Vec3::new(20.0, 0.0, 0.0)), Knot::corner(Vec3::new(20.0, 20.0, 0.0))], false)]);
        let s = sweep(&path, &Section::Bar { length: 2.0, width: 4.0 }, SweepParams::default()).unwrap();
        s.check_invariants().unwrap();
        assert_eq!(s.face_count(), 2 * 4 + 2 + 2, "{}", s.face_count());
        assert!(loom_mesh::select_ops::border_edges(&s).is_empty());
        let p = sweep(&path, &Section::Pipe { radius: 2.0, thickness: 0.5 }, SweepParams::default()).unwrap();
        p.check_invariants().unwrap();
        assert!(loom_mesh::select_ops::border_edges(&p).is_empty(), "pipe ends are capped rings");
        for name in Section::NAMES {
            assert!(!name.is_empty());
        }
    }
}
