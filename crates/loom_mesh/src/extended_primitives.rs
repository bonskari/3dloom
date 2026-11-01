//! Extended Primitives (Create > Geometry > Extended Primitives): Hedra,
//! ChamferBox, OilTank, Spindle, Gengon, Prism, Torus Knot, ChamferCyl,
//! Capsule, L-Ext and C-Ext. Each kind lists its rollout parameters
//! ([`ExtendedKind::params`]) and builds its mesh from their values in that
//! order. Objects with a base sit on z = 0; Hedra and Torus Knot are centred
//! on the pivot.

use crate::standard_primitives::build_merged;
use crate::Mesh;
use glam::{Vec2, Vec3};
use std::f32::consts::{PI, TAU};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ExtendedKind {
    Hedra,
    ChamferBox,
    OilTank,
    Spindle,
    Gengon,
    Prism,
    TorusKnot,
    ChamferCyl,
    Capsule,
    LExt,
    CExt,
}

/// One rollout parameter: name, default, whether it is a whole number, and its minimum.
#[derive(Debug, Clone, Copy)]
pub struct ExtParam {
    pub name: &'static str,
    pub default: f32,
    pub int: bool,
    pub min: f32,
}

const fn fl(name: &'static str, default: f32) -> ExtParam {
    ExtParam { name, default, int: false, min: 0.0 }
}
const fn int(name: &'static str, default: f32, min: f32) -> ExtParam {
    ExtParam { name, default, int: true, min }
}

impl ExtendedKind {
    /// Object Type rollout order.
    pub const ALL: [ExtendedKind; 11] = [
        ExtendedKind::Hedra, ExtendedKind::ChamferBox, ExtendedKind::OilTank, ExtendedKind::Spindle, ExtendedKind::Gengon, ExtendedKind::Prism,
        ExtendedKind::TorusKnot, ExtendedKind::ChamferCyl, ExtendedKind::Capsule, ExtendedKind::LExt, ExtendedKind::CExt,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ExtendedKind::Hedra => "Hedra",
            ExtendedKind::ChamferBox => "ChamferBox",
            ExtendedKind::OilTank => "OilTank",
            ExtendedKind::Spindle => "Spindle",
            ExtendedKind::Gengon => "Gengon",
            ExtendedKind::Prism => "Prism",
            ExtendedKind::TorusKnot => "Torus Knot",
            ExtendedKind::ChamferCyl => "ChamferCyl",
            ExtendedKind::Capsule => "Capsule",
            ExtendedKind::LExt => "L-Ext",
            ExtendedKind::CExt => "C-Ext",
        }
    }

    pub fn parse(s: &str) -> Option<ExtendedKind> {
        let k = s.trim().replace(['-', ' '], "").to_ascii_lowercase();
        Self::ALL.into_iter().find(|e| e.label().replace(['-', ' '], "").to_ascii_lowercase() == k)
    }

    /// The Parameters rollout, in order.
    pub fn params(self) -> &'static [ExtParam] {
        match self {
            // Family: 0 Tetra, 1 Cube/Octa, 2 Dodec/Icos; Dual: 0 the triangle-faced solid, 1 its dual.
            ExtendedKind::Hedra => &[int("Family", 0.0, 0.0), int("Dual", 0.0, 0.0), fl("Radius", 25.0)],
            ExtendedKind::ChamferBox => &[
                fl("Length", 25.0), fl("Width", 25.0), fl("Height", 25.0), fl("Fillet", 2.0),
                int("Length Segs", 1.0, 1.0), int("Width Segs", 1.0, 1.0), int("Height Segs", 1.0, 1.0), int("Fillet Segs", 3.0, 1.0),
            ],
            ExtendedKind::OilTank => &[fl("Radius", 10.0), fl("Height", 40.0), fl("Cap Height", 5.0), int("Sides", 12.0, 3.0), int("Height Segs", 1.0, 1.0), int("Cap Segs", 4.0, 1.0)],
            ExtendedKind::Spindle => &[fl("Radius", 10.0), fl("Height", 40.0), fl("Cap Height", 10.0), int("Sides", 12.0, 3.0), int("Height Segs", 1.0, 1.0), int("Cap Segs", 1.0, 1.0)],
            ExtendedKind::Gengon => &[int("Sides", 5.0, 3.0), fl("Radius", 15.0), fl("Fillet", 0.0), fl("Height", 20.0), int("Height Segs", 1.0, 1.0), int("Fillet Segs", 1.0, 1.0)],
            ExtendedKind::Prism => &[fl("Side 1 Length", 25.0), fl("Side 2 Length", 25.0), fl("Side 3 Length", 25.0), fl("Height", 25.0), int("Height Segs", 1.0, 1.0)],
            ExtendedKind::TorusKnot => &[fl("Radius", 25.0), int("Segments", 120.0, 3.0), int("P", 2.0, 1.0), int("Q", 3.0, 1.0), fl("Cross Section Radius", 4.0), int("Sides", 12.0, 3.0)],
            ExtendedKind::ChamferCyl => &[fl("Radius", 10.0), fl("Height", 25.0), fl("Fillet", 2.0), int("Height Segs", 1.0, 1.0), int("Fillet Segs", 3.0, 1.0), int("Sides", 12.0, 3.0)],
            ExtendedKind::Capsule => &[fl("Radius", 10.0), fl("Height", 40.0), int("Sides", 12.0, 3.0), int("Height Segs", 1.0, 1.0)],
            ExtendedKind::LExt => &[fl("Side Length", 25.0), fl("Front Length", 25.0), fl("Side Width", 5.0), fl("Front Width", 5.0), fl("Height", 25.0), int("Height Segs", 1.0, 1.0)],
            ExtendedKind::CExt => &[
                fl("Back Length", 25.0), fl("Side Length", 25.0), fl("Front Length", 25.0),
                fl("Back Width", 5.0), fl("Side Width", 5.0), fl("Front Width", 5.0), fl("Height", 25.0), int("Height Segs", 1.0, 1.0),
            ],
        }
    }

    pub fn defaults(self) -> Vec<f32> {
        self.params().iter().map(|p| p.default).collect()
    }

    /// The mesh for `values` (in [`Self::params`] order; missing ones take the default).
    pub fn build(self, values: &[f32]) -> Mesh {
        let ps = self.params();
        let v = |name: &str| -> f32 {
            let i = ps.iter().position(|p| p.name == name).expect("a listed parameter");
            let p = ps[i];
            let x = values.get(i).copied().unwrap_or(p.default).max(p.min);
            if p.int { x.round() } else { x }
        };
        let n = |name: &str| v(name) as u32;
        match self {
            ExtendedKind::Hedra => hedra(n("Family"), n("Dual") != 0, v("Radius")),
            ExtendedKind::ChamferBox => chamfer_box(v("Width"), v("Length"), v("Height"), v("Fillet"), [n("Width Segs"), n("Length Segs"), n("Height Segs")], n("Fillet Segs")),
            ExtendedKind::OilTank => {
                let (r, h) = (v("Radius"), v("Height"));
                let c = v("Cap Height").min(r).min(h * 0.5);
                // A spherical cap of height c over radius r: sphere radius (r² + c²) / 2c.
                let cs = n("Cap Segs");
                let cap = |top: bool| -> Vec<(f32, f32)> {
                    if c <= 1e-6 {
                        let z = if top { h } else { 0.0 };
                        return vec![(r, z), (0.0, z)];
                    }
                    let s = (r * r + c * c) / (2.0 * c);
                    let a0 = (r / s).clamp(-1.0, 1.0).asin();
                    (0..=cs).map(|k| {
                        let a = a0 * (1.0 - k as f32 / cs as f32);
                        let z = c - s * (1.0 - a.cos());
                        (s * a.sin(), if top { h - c + z } else { c - z })
                    }).collect()
                };
                let mut prof: Vec<(f32, f32)> = cap(false).into_iter().rev().collect();
                prof.extend(lin((r, c), (r, h - c), n("Height Segs")).into_iter().skip(1));
                prof.extend(cap(true).into_iter().skip(1));
                revolve(&prof, n("Sides"))
            }
            ExtendedKind::Spindle => {
                let (r, h) = (v("Radius"), v("Height"));
                let c = v("Cap Height").min(h * 0.5);
                let mut prof = lin((0.0, 0.0), (r, c), n("Cap Segs"));
                prof.extend(lin((r, c), (r, h - c), n("Height Segs")).into_iter().skip(1));
                prof.extend(lin((r, h - c), (0.0, h), n("Cap Segs")).into_iter().skip(1));
                revolve(&prof, n("Sides"))
            }
            ExtendedKind::Gengon => {
                let (sides, r) = (n("Sides").max(3), v("Radius"));
                let corners: Vec<Vec2> = (0..sides).map(|i| Vec2::from_angle(i as f32 / sides as f32 * TAU) * r).collect();
                let outline = filleted(&corners, v("Fillet"), n("Fillet Segs"));
                extrude(&outline, v("Height"), n("Height Segs"))
            }
            ExtendedKind::Prism => {
                let (a, b, c) = (v("Side 1 Length"), v("Side 2 Length"), v("Side 3 Length"));
                // Side 1 along X from the origin; the third corner from the law of cosines.
                let x = if a > 1e-6 { (a * a + c * c - b * b) / (2.0 * a) } else { 0.0 };
                let y = (c * c - x * x).max(0.0).sqrt();
                extrude(&[Vec2::ZERO, Vec2::new(a, 0.0), Vec2::new(x, y)], v("Height"), n("Height Segs"))
            }
            ExtendedKind::TorusKnot => torus_knot(v("Radius"), n("Segments"), n("P"), n("Q"), v("Cross Section Radius"), n("Sides")),
            ExtendedKind::ChamferCyl => {
                let (r, h) = (v("Radius"), v("Height"));
                let f = v("Fillet").min(r).min(h * 0.5);
                let fs = n("Fillet Segs");
                let arc = |cz: f32, from: f32, to: f32| -> Vec<(f32, f32)> {
                    (0..=fs).map(|k| {
                        let a = from + (to - from) * k as f32 / fs as f32;
                        (r - f + f * a.cos(), cz + f * a.sin())
                    }).collect()
                };
                let mut prof = vec![(0.0, 0.0)];
                prof.extend(arc(f, -PI * 0.5, 0.0));
                prof.extend(lin((r, f), (r, h - f), n("Height Segs")).into_iter().skip(1));
                prof.extend(arc(h - f, 0.0, PI * 0.5).into_iter().skip(1));
                prof.push((0.0, h));
                revolve(&prof, n("Sides"))
            }
            ExtendedKind::Capsule => {
                let (r, h) = (v("Radius"), v("Height"));
                let r = r.min(h * 0.5);
                let sides = n("Sides").max(3);
                let rings = (sides / 4).max(2);
                let mut prof: Vec<(f32, f32)> = (0..=rings).map(|k| {
                    let a = -PI * 0.5 + PI * 0.5 * k as f32 / rings as f32;
                    (r * a.cos(), r + r * a.sin())
                }).collect();
                prof.extend(lin((r, r), (r, h - r), n("Height Segs")).into_iter().skip(1));
                prof.extend((1..=rings).map(|k| {
                    let a = PI * 0.5 * k as f32 / rings as f32;
                    (r * a.cos(), h - r + r * a.sin())
                }));
                revolve(&prof, sides)
            }
            ExtendedKind::LExt => {
                let (sl, fl_, sw, fw) = (v("Side Length"), v("Front Length"), v("Side Width").min(v("Front Length")), v("Front Width").min(v("Side Length")));
                // The corner at the origin; the side leg runs along +Y, the front leg along +X.
                let outline = [Vec2::ZERO, Vec2::new(fl_, 0.0), Vec2::new(fl_, fw), Vec2::new(sw, fw), Vec2::new(sw, sl), Vec2::new(0.0, sl)];
                extrude(&outline, v("Height"), n("Height Segs"))
            }
            ExtendedKind::CExt => {
                let (bl, sl, fl_) = (v("Back Length"), v("Side Length"), v("Front Length"));
                let (bw, sw, fw) = (v("Back Width"), v("Side Width"), v("Front Width"));
                // The side runs along Y at x = 0; the back leg along +X at the top, the front along +X at the bottom.
                let (bw, fw) = (bw.min(sl * 0.5), fw.min(sl * 0.5));
                let sw = sw.min(bl).min(fl_);
                let outline = [
                    Vec2::ZERO, Vec2::new(fl_, 0.0), Vec2::new(fl_, fw), Vec2::new(sw, fw),
                    Vec2::new(sw, sl - bw), Vec2::new(bl, sl - bw), Vec2::new(bl, sl), Vec2::new(0.0, sl),
                ];
                extrude(&outline, v("Height"), n("Height Segs"))
            }
        }
    }
}

/// `segs` steps from `a` to `b`, both ends included.
fn lin(a: (f32, f32), b: (f32, f32), segs: u32) -> Vec<(f32, f32)> {
    let s = segs.max(1);
    (0..=s).map(|k| {
        let t = k as f32 / s as f32;
        (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
    }).collect()
}

/// Turns a (radius, z) profile about Z with `sides` sides; points on the
/// axis close the ends.
fn revolve(profile: &[(f32, f32)], sides: u32) -> Mesh {
    let s = sides.max(3);
    let at = |(r, z): (f32, f32), i: u32| {
        let a = i as f32 / s as f32 * TAU;
        Vec3::new(r * a.cos(), r * a.sin(), z)
    };
    let mut polys = Vec::new();
    for w in profile.windows(2) {
        for i in 0..s {
            polys.push(vec![at(w[0], i), at(w[0], i + 1), at(w[1], i + 1), at(w[1], i)]);
        }
    }
    finish(polys)
}

/// A counter-clockwise outline extruded up `height` in `segs` rows, capped.
fn extrude(outline: &[Vec2], height: f32, segs: u32) -> Mesh {
    let s = segs.max(1);
    let n = outline.len();
    let at = |p: Vec2, z: f32| Vec3::new(p.x, p.y, z);
    let mut polys = Vec::new();
    for k in 0..s {
        let (z0, z1) = (height * k as f32 / s as f32, height * (k + 1) as f32 / s as f32);
        for i in 0..n {
            let (a, b) = (outline[i], outline[(i + 1) % n]);
            polys.push(vec![at(a, z0), at(b, z0), at(b, z1), at(a, z1)]);
        }
    }
    polys.push(outline.iter().map(|p| at(*p, height)).collect());
    polys.push(outline.iter().rev().map(|p| at(*p, 0.0)).collect());
    finish(polys)
}

/// A convex outline with each corner rounded by `radius` in `segs` steps.
fn filleted(corners: &[Vec2], radius: f32, segs: u32) -> Vec<Vec2> {
    if radius <= 1e-6 {
        return corners.to_vec();
    }
    let n = corners.len();
    let mut out = Vec::new();
    for i in 0..n {
        let (prev, p, next) = (corners[(i + n - 1) % n], corners[i], corners[(i + 1) % n]);
        let (d0, d1) = ((prev - p).normalize_or_zero(), (next - p).normalize_or_zero());
        let half = d0.perp_dot(d1).atan2(d0.dot(d1)).abs() * 0.5;
        // Keep the tangent points inside the half edges.
        let max_t = (prev - p).length().min((next - p).length()) * 0.5;
        let t = (radius / half.tan()).min(max_t);
        let r = t * half.tan();
        let centre = p + (d0 + d1).normalize_or_zero() * (r / half.sin());
        let (a, b) = (p + d0 * t - centre, p + d1 * t - centre);
        let (a0, a1) = (a.y.atan2(a.x), b.y.atan2(b.x));
        let mut sweep = a1 - a0;
        if sweep < 0.0 {
            sweep += TAU;
        }
        let s = segs.max(1);
        for k in 0..=s {
            out.push(centre + Vec2::from_angle(a0 + sweep * k as f32 / s as f32) * r);
        }
    }
    out
}

/// ChamferBox: a box on z = 0 whose edges are rounded by `fillet`.
fn chamfer_box(width: f32, length: f32, height: f32, fillet: f32, segs: [u32; 3], fillet_segs: u32) -> Mesh {
    let half = Vec3::new(width, length, height) * 0.5;
    let r = fillet.max(0.0).min(half.min_element());
    let inner = half - Vec3::splat(r);
    let fs = fillet_segs.max(1);
    // Grid coordinates along one axis: the fillet, the flat run, the fillet.
    let coords = |h: f32, i: f32, flat: u32| -> Vec<f32> {
        let mut c = Vec::new();
        if r > 1e-6 {
            c.extend((0..fs).map(|k| -h + (h - i) * k as f32 / fs as f32));
        }
        let fl = flat.max(1);
        c.extend((0..=fl).map(|k| -i + 2.0 * i * k as f32 / fl as f32));
        if r > 1e-6 {
            c.extend((1..=fs).map(|k| i + (h - i) * k as f32 / fs as f32));
        }
        c.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        c
    };
    let cs = [coords(half.x, inner.x, segs[0]), coords(half.y, inner.y, segs[1]), coords(half.z, inner.z, segs[2])];
    let map = |q: Vec3| {
        let c = q.clamp(-inner, inner);
        let d = q - c;
        let p = if r > 1e-6 && d.length_squared() > 1e-12 { c + d.normalize() * r } else { q };
        p + Vec3::Z * half.z
    };
    let mut polys = Vec::new();
    // Each face: (normal axis, sign, u axis, v axis) with u × v along the outward normal.
    for (n, sign) in [(0usize, 1.0f32), (0, -1.0), (1, 1.0), (1, -1.0), (2, 1.0), (2, -1.0)] {
        let (mut u, mut v) = ((n + 1) % 3, (n + 2) % 3);
        if sign < 0.0 {
            std::mem::swap(&mut u, &mut v);
        }
        let pt = |a: f32, b: f32| {
            let mut q = Vec3::ZERO;
            q[n] = half[n] * sign;
            q[u] = a;
            q[v] = b;
            map(q)
        };
        for i in 0..cs[u].len() - 1 {
            for j in 0..cs[v].len() - 1 {
                let (a0, a1, b0, b1) = (cs[u][i], cs[u][i + 1], cs[v][j], cs[v][j + 1]);
                polys.push(vec![pt(a0, b0), pt(a1, b0), pt(a1, b1), pt(a0, b1)]);
            }
        }
    }
    finish(polys)
}

/// Hedra: Tetra, Cube/Octa or Dodec/Icos with circumradius `radius`; `dual`
/// gives the cube or dodecahedron.
fn hedra(family: u32, dual: bool, radius: f32) -> Mesh {
    let (verts, tris): (Vec<Vec3>, Vec<[usize; 3]>) = match family.min(2) {
        0 => (
            vec![Vec3::new(1.0, 1.0, 1.0), Vec3::new(-1.0, -1.0, 1.0), Vec3::new(-1.0, 1.0, -1.0), Vec3::new(1.0, -1.0, -1.0)],
            vec![[0, 1, 2], [0, 3, 1], [0, 2, 3], [1, 3, 2]],
        ),
        1 => (
            vec![Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z],
            vec![[0, 2, 4], [2, 1, 4], [1, 3, 4], [3, 0, 4], [2, 0, 5], [1, 2, 5], [3, 1, 5], [0, 3, 5]],
        ),
        _ => {
            let t = (1.0 + 5f32.sqrt()) * 0.5;
            (
                vec![
                    Vec3::new(-1.0, t, 0.0), Vec3::new(1.0, t, 0.0), Vec3::new(-1.0, -t, 0.0), Vec3::new(1.0, -t, 0.0),
                    Vec3::new(0.0, -1.0, t), Vec3::new(0.0, 1.0, t), Vec3::new(0.0, -1.0, -t), Vec3::new(0.0, 1.0, -t),
                    Vec3::new(t, 0.0, -1.0), Vec3::new(t, 0.0, 1.0), Vec3::new(-t, 0.0, -1.0), Vec3::new(-t, 0.0, 1.0),
                ],
                vec![
                    [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11], [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
                    [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9], [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
                ],
            )
        }
    };
    let verts: Vec<Vec3> = verts.iter().map(|v| v.normalize()).collect();
    let polys: Vec<Vec<Vec3>> = if dual && family.min(2) != 0 {
        // The dual: a vertex per face (its centre), a face per vertex (the faces around it, in turn).
        let centres: Vec<Vec3> = tris.iter().map(|t| ((verts[t[0]] + verts[t[1]] + verts[t[2]]) / 3.0).normalize()).collect();
        (0..verts.len()).map(|vi| {
            let n = verts[vi];
            let around: Vec<usize> = (0..tris.len()).filter(|f| tris[*f].contains(&vi)).collect();
            let x = (centres[around[0]] - n * centres[around[0]].dot(n)).normalize();
            let y = n.cross(x);
            let mut ring: Vec<(f32, Vec3)> = around.iter().map(|f| {
                let d = centres[*f];
                (d.dot(y).atan2(d.dot(x)), d)
            }).collect();
            ring.sort_by(|a, b| a.0.total_cmp(&b.0));
            ring.into_iter().map(|(_, p)| p * radius).collect()
        }).collect()
    } else {
        tris.iter().map(|t| t.iter().map(|i| verts[*i] * radius).collect()).collect()
    };
    finish_oriented_each(polys)
}

/// Torus Knot: a (p, q) knot on a torus of base radius `radius`, swept by a
/// circle of `tube` radius.
fn torus_knot(radius: f32, segments: u32, p: u32, q: u32, tube: f32, sides: u32) -> Mesh {
    let (n, s) = (segments.max(3), sides.max(3));
    let (p, q) = (p.max(1) as f32, q.max(1) as f32);
    let curve = |t: f32| {
        let rr = radius * (2.0 + (q * t).cos()) / 3.0;
        Vec3::new(rr * (p * t).cos(), rr * (p * t).sin(), radius * (q * t).sin() / 3.0)
    };
    let h = TAU / n as f32 * 0.25;
    let ring = |i: u32| -> Vec<Vec3> {
        let t = i as f32 / n as f32 * TAU;
        let c = curve(t);
        let tan = (curve(t + h) - curve(t - h)).normalize();
        let acc = curve(t + h) - 2.0 * c + curve(t - h);
        let b = tan.cross(acc).try_normalize().unwrap_or_else(|| tan.any_orthonormal_vector());
        let nrm = b.cross(tan);
        (0..s).map(|k| {
            let a = k as f32 / s as f32 * TAU;
            c + (nrm * a.cos() + b * a.sin()) * tube
        }).collect()
    };
    let rings: Vec<Vec<Vec3>> = (0..n).map(ring).collect();
    let mut polys = Vec::new();
    for i in 0..n as usize {
        let (r0, r1) = (&rings[i], &rings[(i + 1) % n as usize]);
        for k in 0..s as usize {
            let k1 = (k + 1) % s as usize;
            polys.push(vec![r0[k], r0[k1], r1[k1], r1[k]]);
        }
    }
    finish(polys)
}

/// Closed solids outward: the whole polygon set flipped when it encloses a negative volume.
fn finish(mut polys: Vec<Vec<Vec3>>) -> Mesh {
    let mut volume = 0.0f32;
    for p in &polys {
        for i in 1..p.len().saturating_sub(1) {
            volume += p[0].dot(p[i].cross(p[i + 1]));
        }
    }
    if volume < 0.0 {
        for p in &mut polys {
            p.reverse();
        }
    }
    build_merged(&polys)
}

/// Convex solids around the origin: each polygon faces away from the centre.
fn finish_oriented_each(mut polys: Vec<Vec<Vec3>>) -> Mesh {
    for p in &mut polys {
        let c: Vec3 = p.iter().copied().sum::<Vec3>() / p.len() as f32;
        let n = (p[1] - p[0]).cross(p[2] - p[0]);
        if n.dot(c) < 0.0 {
            p.reverse();
        }
    }
    build_merged(&polys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_builds_a_closed_solid_with_its_defaults() {
        for k in ExtendedKind::ALL {
            let m = k.build(&k.defaults());
            m.check_invariants().unwrap_or_else(|e| panic!("{k:?}: {e}"));
            assert!(m.face_count() > 3, "{k:?}");
            assert!(m.half_edges().iter().all(|h| h.face.is_some()), "{k:?} is closed");
        }
    }

    #[test]
    fn hedra_families_have_the_platonic_face_counts() {
        let count = |f: f32, d: f32| ExtendedKind::Hedra.build(&[f, d, 10.0]).face_count();
        assert_eq!([count(0.0, 0.0), count(1.0, 0.0), count(1.0, 1.0), count(2.0, 0.0), count(2.0, 1.0)], [4, 8, 6, 20, 12]);
    }

    #[test]
    fn chamfer_box_keeps_its_size_and_sits_on_the_ground() {
        let m = ExtendedKind::ChamferBox.build(&[20.0, 30.0, 10.0, 2.0, 1.0, 1.0, 1.0, 3.0]);
        let (lo, hi) = m.aabb();
        assert!((hi - lo - Vec3::new(30.0, 20.0, 10.0)).length() < 1e-3, "{lo} {hi}");
        assert!(lo.z.abs() < 1e-4);
    }

    #[test]
    fn prism_sides_follow_their_lengths() {
        let m = ExtendedKind::Prism.build(&[30.0, 40.0, 50.0, 5.0, 1.0]);
        let (lo, hi) = m.aabb();
        // Side 1 runs 30 along X; the third corner lies where sides 2 and 3 meet.
        assert!((hi.z - 5.0).abs() < 1e-4 && hi.y > 0.0 && lo.x <= 1e-4);
        assert_eq!(m.face_count(), 5);
    }
}
