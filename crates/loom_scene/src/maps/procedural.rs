//! The 2D maps Swirl, Tiles and Gradient Ramp; the 3D maps Cellular, Dent,
//! Marble, Perlin Marble, Planet, Smoke, Speckle, Splat, Stucco, Waves and
//! Wood; the compositors Composite, Mask, RGB Multiply; the colour
//! modifiers RGB Tint, Output, Color Correction, Vertex Color; and Falloff.

use super::{perlin, smoothstep, Ctx};
use crate::map_params::BLEND_MODES;
use crate::material::{Map, MapType};
use glam::Vec3;

/// `uv` the folded map-space UV (2D maps), `q` the pattern-space point (3D maps).
pub(super) fn eval(map: &Map, uv: [f32; 2], q: Vec3, ctx: &Ctx, component: &dyn Fn(usize) -> Vec3, sub: &dyn Fn(usize) -> Option<[f32; 4]>) -> [f32; 4] {
    let p = &map.params;
    let e = &map.extra;
    let rgb = |c: Vec3| [c.x, c.y, c.z, 1.0];
    let two = |t: f32| rgb(component(0).lerp(component(1), t.clamp(0.0, 1.0)));
    let size = p.size.max(1e-3);
    match map.kind {
        MapType::Swirl => rgb(component(0).lerp(component(1), swirl(map, uv))),
        MapType::Tiles => tiles(map, uv, component),
        MapType::GradientRamp => gradient_ramp(map, uv, ctx, component, sub),
        MapType::Cellular => rgb(cellular(map, q / size, component)),
        MapType::Dent => {
            let n = fbm(q * (20.0 / size), e.iterations.round().max(1.0) as u32) * 0.5 + 0.5;
            // Strength 20 shows the noise as it is; more covers more with Color #2.
            two(1.0 - (1.0 - n) * 20.0 / e.strength.max(1e-3))
        }
        MapType::Marble => {
            // Veins across X every Size / 8 units, a third colour between vein and stone.
            let x = q.x / size * 8.0 + 2.0 * turbulence(q / size * 4.0, 4);
            let band = (std::f32::consts::PI * x).sin().abs();
            let w = (e.vein_width * 10.0).clamp(1e-3, 0.9);
            let (c1, c2) = (component(0), component(1));
            let mid = (c1 + c2) * 0.5;
            let vein = 1.0 - smoothstep(w * 0.6, w, band);
            let edge = 1.0 - smoothstep(w * 1.6, w * 2.0, band);
            rgb(c1.lerp(mid, edge).lerp(c2, vein))
        }
        MapType::PerlinMarble => {
            let t = 0.5 + 0.5 * (q.x / size * 6.0 + 6.0 * turbulence(q / size, p.levels.round().max(1.0) as u32)).sin();
            // Saturation scales each colour; the defaults (85, 70) show the swatches as they are.
            let c1 = component(0) * (e.saturation[0] / 85.0);
            let c2 = component(1) * (e.saturation[1] / 70.0);
            rgb(c1.lerp(c2, t))
        }
        MapType::Planet => rgb(planet(map, q, component)),
        MapType::Smoke => {
            let f = turbulence(q / size + Vec3::new(0.0, 0.0, p.phase), e.iterations.round().max(1.0) as u32);
            two((f * 1.6).clamp(0.0, 1.0).powf(e.exponent.max(1e-3)))
        }
        MapType::Speckle => {
            // Size 0.1 gives about twenty speckles across 100 units.
            let s = q * (0.02 / size);
            let (f1, _, h) = worley(s, 7);
            let r = 0.22 + 0.18 * unit(h);
            two(smoothstep(r * 0.75, r, f1))
        }
        MapType::Splat => {
            let f = (fbm(q / size, e.iterations.round().max(1.0) as u32) * 0.9 + 0.5).clamp(0.0, 1.0);
            let edge = 1.0 - e.threshold;
            two(smoothstep(edge - 0.02, edge + 0.02, f))
        }
        MapType::Stucco => {
            let n = (perlin(q / size) * 0.9 + 0.5).clamp(0.0, 1.0);
            let h = (e.thickness * 0.5).max(1e-4);
            // Color #1 (the indentations) covers more as Threshold rises.
            let t1 = 1.0 - smoothstep(e.threshold - h, e.threshold + h, n);
            rgb(component(1).lerp(component(0), t1))
        }
        MapType::Waves => two(waves(map, q)),
        MapType::Wood => {
            // Rings about the X axis; Radial / Axial Noise wobble them.
            let r = (q.y * q.y + q.z * q.z).sqrt() + e.radial_noise * 2.0 * perlin(q * 0.08) + e.axial_noise * 2.0 * perlin(Vec3::new(q.x * 0.02, 0.37, 0.71));
            if e.grain_thickness <= 1e-4 {
                return two(0.5);
            }
            let f = (r / e.grain_thickness).rem_euclid(1.0);
            two(smoothstep(0.35, 0.65, (2.0 * f - 1.0).abs()))
        }
        MapType::Composite => composite(map, sub),
        MapType::Mask => {
            let m = sub(0).unwrap_or_else(|| rgb(map.color(0)));
            let k = sub(1).map_or(1.0, |t| (t[0] + t[1] + t[2]) / 3.0);
            let k = if e.invert_mask { 1.0 - k } else { k };
            [m[0], m[1], m[2], m[3] * k]
        }
        MapType::RgbMultiply => {
            let a = sub(0).map_or_else(|| rgb(map.color(0)), |t| { let c = map.color(0).lerp(Vec3::new(t[0], t[1], t[2]), t[3]); [c.x, c.y, c.z, t[3]] });
            let b = sub(1).map_or_else(|| rgb(map.color(1)), |t| { let c = map.color(1).lerp(Vec3::new(t[0], t[1], t[2]), t[3]); [c.x, c.y, c.z, t[3]] });
            let alpha = match e.alpha_from {
                0 => a[3],
                1 => b[3],
                _ => a[3] * b[3],
            };
            [a[0] * b[0], a[1] * b[1], a[2] * b[2], alpha]
        }
        MapType::RgbTint => {
            let m = sub(3).unwrap_or([1.0; 4]);
            let c = map.color(0) * m[0] + map.color(1) * m[1] + map.color(2) * m[2];
            [c.x, c.y, c.z, m[3]]
        }
        // The Output rollout applies after this (`MapType::has_output`).
        MapType::Output => sub(0).unwrap_or_else(|| rgb(map.color(0))),
        MapType::ColorCorrection => color_correction(map, sub(0).unwrap_or_else(|| rgb(map.color(0)))),
        // The renderers multiply by the mesh's vertex colours.
        MapType::VertexColor => [1.0; 4],
        MapType::Falloff => falloff(map, ctx, sub),
        // What these see is the renderer's scene: on the CPU the sample's grey surroundings.
        MapType::Raytrace | MapType::ReflectRefract | MapType::FlatMirror | MapType::ThinWall => [0.5, 0.5, 0.5, 1.0],
        MapType::Bitmap | MapType::Checker | MapType::Gradient | MapType::Noise | MapType::Mix | MapType::NormalBump => unreachable!("evaluated in maps.rs"),
    }
}

/// A well-mixed 32-bit hash of integer coordinates and a seed.
fn hash(x: i32, y: i32, z: i32, seed: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343) ^ (y as u32).wrapping_mul(0xD816_3841) ^ (z as u32).wrapping_mul(0xCB1A_B31F) ^ seed.wrapping_mul(0x9E37_79B9);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^ (h >> 16)
}

/// A hash as 0–1.
fn unit(h: u32) -> f32 {
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// Fractal noise, about -1–1.
fn fbm(p: Vec3, octaves: u32) -> f32 {
    let (mut n, mut amp, mut norm, mut f) = (0.0, 1.0, 0.0, 1.0);
    for _ in 0..octaves.clamp(1, 12) {
        n += amp * perlin(p * f);
        norm += amp;
        amp *= 0.5;
        f *= 2.0;
    }
    n / norm
}

/// Turbulence (fractal absolute noise), 0–about 1.
fn turbulence(p: Vec3, octaves: u32) -> f32 {
    let (mut n, mut amp, mut norm, mut f) = (0.0, 1.0, 0.0, 1.0);
    for _ in 0..octaves.clamp(1, 12) {
        n += amp * perlin(p * f).abs();
        norm += amp;
        amp *= 0.5;
        f *= 2.0;
    }
    n / norm
}

/// Cellular (Worley) noise: distances to the nearest and second nearest
/// feature points (one per unit cell) and the nearest one's hash.
fn worley(p: Vec3, seed: u32) -> (f32, f32, u32) {
    let c = p.floor();
    let (mut f1, mut f2, mut id) = (f32::MAX, f32::MAX, 0u32);
    for dz in -1..=1 {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (x, y, z) = (c.x as i32 + dx, c.y as i32 + dy, c.z as i32 + dz);
                let h = hash(x, y, z, seed);
                let feature = Vec3::new(x as f32 + unit(h), y as f32 + unit(hash(x, y, z, seed ^ 0x5bd1_e995)), z as f32 + unit(hash(x, y, z, seed ^ 0x2f8a_17c3)));
                let d = feature.distance(p);
                if d < f1 {
                    f2 = f1;
                    f1 = d;
                    id = h;
                } else if d < f2 {
                    f2 = d;
                }
            }
        }
    }
    (f1, f2, id)
}

/// Swirl: Base 0 – Swirl 1 at a UV.
fn swirl(map: &Map, uv: [f32; 2]) -> f32 {
    let e = &map.extra;
    let d = glam::Vec2::new(uv[0] - e.center[0], uv[1] - e.center[1]);
    let r = d.length();
    // Twist winds the pattern about the centre; at 0 it is plain noise.
    let a = d.y.atan2(d.x) + e.twist * r * std::f32::consts::TAU;
    let s = glam::Vec2::new(a.cos(), a.sin()) * r * 6.0;
    let levels = e.constant_detail.round().max(1.0) as u32;
    let f = fbm(Vec3::new(s.x, s.y, e.seed as f32 * 0.0137), levels);
    let t = 0.5 + f * e.swirl_intensity * 0.5 * (1.0 + e.color_contrast);
    (t.clamp(0.0, 1.0) * e.swirl_amount).clamp(0.0, 1.0)
}

/// Tiles: brick bonds of tiles and grout.
fn tiles(map: &Map, uv: [f32; 2], component: &dyn Fn(usize) -> Vec3) -> [f32; 4] {
    let e = &map.extra;
    let (u, v) = (uv[0], uv[1]);
    let fine = matches!(e.preset, 6 | 7);
    let hc = e.horiz_count.max(0.01);
    let mut vc = e.vert_count.max(0.01) * if fine { 2.0 } else { 1.0 };
    let row = (v * vc).floor() as i32;
    let odd = row.rem_euclid(2) == 1;
    let rnd = |a: i32, b: i32, k: u32| unit(hash(a, b, k as i32, e.seed));
    // Tile widths along a row (in tiles) and the row's shift.
    let (widths, shift): (&[f32], f32) = match e.preset {
        1 | 6 => (&[1.0], if odd { 0.5 } else { 0.0 }),
        2 => (&[1.0, 0.5], if odd { 0.75 } else { 0.0 }),
        3 => (if odd { &[0.5] } else { &[1.0] }, if odd { 0.25 } else { 0.0 }),
        4 => (&[1.0], if odd { 0.25 } else { 0.0 }),
        5 | 7 => (&[1.0], 0.0),
        _ => (&[1.0], if odd { e.line_shift } else { 0.0 } + e.random_shift * rnd(row, 0, 1)),
    };
    let mut row_scale = 1.0;
    if e.preset == 0 && e.row_modify && e.per_row > 0 && (row + 1).rem_euclid(e.per_row as i32) == 0 {
        if e.row_change <= 0.0 {
            return [0.0; 4];
        }
        row_scale = e.row_change;
    }
    let x = u * hc / row_scale + shift;
    let total: f32 = widths.iter().sum();
    let k = (x / total).floor();
    let mut r = x - k * total;
    let mut col = (k as i32) * widths.len() as i32;
    let mut w = widths[0];
    for wj in widths {
        w = *wj;
        if r < w {
            break;
        }
        r -= w;
        col += 1;
    }
    let fx = (r / w).clamp(0.0, 1.0);
    if e.preset == 0 && e.column_modify && e.per_column > 0 && (col + 1).rem_euclid(e.per_column as i32) == 0 {
        if e.column_change <= 0.0 {
            return [0.0; 4];
        }
        vc /= e.column_change;
    }
    let row = (v * vc).floor() as i32;
    let fy = (v * vc).rem_euclid(1.0);
    // Distances to the tile's edges in UV units; the gaps are in 1/50 of the map.
    let dx = fx.min(1.0 - fx) * w * row_scale / hc;
    let dy = fy.min(1.0 - fy) / vc;
    let rough = e.rough / 100.0 * perlin(Vec3::new(u * 60.0, v * 60.0, 0.5));
    let (gx, gy) = (e.gap[0] / 100.0 * (1.0 + rough), e.gap[1] / 100.0 * (1.0 + rough));
    let hole = e.holes > 0.0 && rnd(col, row, 2) * 100.0 < e.holes;
    if dx < gx || dy < gy || hole {
        let g = component(1);
        return [g.x, g.y, g.z, 1.0];
    }
    let mut c = component(0);
    if e.color_variance > 0.0 {
        let jitter = Vec3::new(rnd(col, row, 3), rnd(col, row, 4), rnd(col, row, 5)) * 2.0 - Vec3::ONE;
        c += jitter * (e.color_variance * 0.1).min(1.0) * 0.5;
    }
    c *= 1.0 - e.fade_variance.clamp(0.0, 1.0) * rnd(col, row, 6);
    let c = c.clamp(Vec3::ZERO, Vec3::ONE);
    [c.x, c.y, c.z, 1.0]
}

/// Gradient Ramp: the flags' colours (or maps) along the gradient type.
fn gradient_ramp(map: &Map, uv: [f32; 2], ctx: &Ctx, component: &dyn Fn(usize) -> Vec3, sub: &dyn Fn(usize) -> Option<[f32; 4]>) -> [f32; 4] {
    let e = &map.extra;
    let p = &map.params;
    let (u, v) = (uv[0], uv[1]);
    let (cu, cv) = (u - 0.5, v - 0.5);
    let tri = |x: f32| 1.0 - (2.0 * x - 1.0).abs();
    let t = match e.ramp_type {
        0 => (u + u * v) * 0.5,
        1 => cu.abs().max(cv.abs()) * 2.0,
        2 => (u + v) * 0.5,
        3 => ctx.light,
        5 => sub(0).map_or(u, |c| (c[0] + c[1] + c[2]) / 3.0),
        6 => ctx.normal.dot(ctx.view).clamp(-1.0, 1.0).acos() / std::f32::consts::FRAC_PI_2,
        7 => 1.0 - (1.0 - (u + v)).abs(),
        8 => (cu * cu + cv * cv).sqrt() * 2.0,
        9 => cv.atan2(cu) / std::f32::consts::TAU + 0.5,
        10 => v.atan2(u) / std::f32::consts::FRAC_PI_2,
        11 => (tri(u) + tri(v)) * 0.5,
        _ => u,
    };
    let mut t = t.clamp(0.0, 1.0);
    if e.noise_amount > 0.0 {
        let q = Vec3::new(u, v, p.phase) / p.size.max(1e-3);
        let levels = if p.noise_type == 0 { 1 } else { p.levels.round().max(1.0) as u32 };
        let n = if p.noise_type == 2 { turbulence(q, levels) } else { fbm(q, levels) * 0.5 + 0.5 };
        let lo = p.low.min(p.high);
        let hi = p.high.max(p.low + 1e-4);
        let n = if e.smooth > 0.0 { smoothstep(lo - e.smooth * 0.1, hi + e.smooth * 0.1, n) } else { ((n - lo) / (hi - lo)).clamp(0.0, 1.0) };
        t = (t + e.noise_amount * (n - 0.5) * 2.0).clamp(0.0, 1.0);
    }
    let c = map.ramp_sample(t * 100.0, &|i| component(1 + i));
    [c.x, c.y, c.z, 1.0]
}

/// Cellular: cells of Cell Color and the divisions' ramp between Division #1 and #2.
fn cellular(map: &Map, q: Vec3, component: &dyn Fn(usize) -> Vec3) -> Vec3 {
    let e = &map.extra;
    let spread = e.spread.max(1e-3);
    let one = |q: Vec3| {
        let (f1, f2, h) = worley(q, e.seed);
        let v = if e.chips { 1.0 - ((f2 - f1) * 1.5 * spread).min(1.0) } else { (f1 * 0.5 / spread).min(1.0) };
        (v, h)
    };
    let (mut v, h) = one(q);
    if e.fractal {
        let (mut amp, mut norm, mut f) = (1.0f32, 1.0f32, 2.0f32);
        let falloff = 0.5 + 0.5 * e.roughness;
        let n = e.iterations.max(1.0);
        let mut k = 1.0;
        let mut total = v;
        while k < n {
            amp *= falloff;
            let w = amp * (n - k).min(1.0);
            total += one(q * f).0 * w;
            norm += w;
            f *= 2.0;
            k += 1.0;
        }
        v = total / norm;
    }
    let mut cell = component(0);
    if e.variation > 0.0 {
        let j = Vec3::new(unit(h), unit(hash(h as i32, 1, 0, 0)), unit(hash(h as i32, 2, 0, 0))) * 2.0 - Vec3::ONE;
        cell = (cell + j * e.variation / 100.0).clamp(Vec3::ZERO, Vec3::ONE);
    }
    let (lo, mid, hi) = (map.params.low, e.mid, map.params.high);
    if v <= lo {
        cell
    } else if v <= mid {
        cell.lerp(component(1), ((v - lo) / (mid - lo).max(1e-4)).clamp(0.0, 1.0))
    } else if v <= hi {
        component(1).lerp(component(2), ((v - mid) / (hi - mid).max(1e-4)).clamp(0.0, 1.0))
    } else {
        component(2)
    }
}

/// Planet: water colours 1–3 by depth, land colours 4–8 by height.
fn planet(map: &Map, q: Vec3, component: &dyn Fn(usize) -> Vec3) -> Vec3 {
    let e = &map.extra;
    let s = q / e.continent_size.max(1e-3) + Vec3::splat(e.seed as f32 * 0.173);
    let h = fbm(s, 6) + e.island_factor.min(5.0) * 0.15 * perlin(s * 4.0);
    let h = (h * 0.8 + 0.5).clamp(0.0, 1.0);
    let sea = 0.5 + (e.ocean / 100.0 - 0.5) * 0.6;
    let bands = |t: f32, first: usize, n: usize| -> Vec3 {
        let x = t.clamp(0.0, 1.0) * (n - 1) as f32;
        let i = (x.floor() as usize).min(n - 2);
        let f = if e.blend_water_land { x - i as f32 } else if x - (i as f32) < 0.5 { 0.0 } else { 1.0 };
        component(first + i).lerp(component(first + i + 1), f)
    };
    if h < sea {
        // Deep water is Color #1.
        bands(h / sea.max(1e-4), 0, 3)
    } else {
        bands((h - sea) / (1.0 - sea).max(1e-4), 3, 5)
    }
}

/// Waves: wave sets from centres on a sphere (3D) or a circle (2D).
fn waves(map: &Map, q: Vec3) -> f32 {
    let e = &map.extra;
    let n = e.wave_sets.max(1);
    let mut s = 0.0;
    for i in 0..n {
        let h = |k: u32| unit(hash(i as i32, k as i32, 0, e.seed));
        let theta = h(1) * std::f32::consts::TAU;
        let dir = if e.distribution_2d {
            Vec3::new(theta.cos(), theta.sin(), 0.0)
        } else {
            let z = h(2) * 2.0 - 1.0;
            let r = (1.0 - z * z).sqrt();
            Vec3::new(r * theta.cos(), r * theta.sin(), z)
        };
        let c = dir * e.wave_radius;
        let (lo, hi) = (e.wave_len_min.min(e.wave_len_max), e.wave_len_max.max(e.wave_len_min));
        let len = (lo + (hi - lo) * h(3)).max(1e-3);
        s += (std::f32::consts::TAU * (q.distance(c) / len - map.params.phase)).cos();
    }
    0.5 + 0.5 * e.amplitude * s / (n as f32).sqrt()
}

fn lum(c: Vec3) -> f32 {
    (c.x + c.y + c.z) / 3.0
}

fn rgb_to_hsv(c: Vec3) -> Vec3 {
    let max = c.max_element();
    let min = c.min_element();
    let d = max - min;
    let h = if d <= 1e-6 {
        0.0
    } else if max == c.x {
        ((c.y - c.z) / d).rem_euclid(6.0)
    } else if max == c.y {
        (c.z - c.x) / d + 2.0
    } else {
        (c.x - c.y) / d + 4.0
    } / 6.0;
    Vec3::new(h, if max <= 1e-6 { 0.0 } else { d / max }, max)
}

fn hsv_to_rgb(c: Vec3) -> Vec3 {
    let h = c.x.rem_euclid(1.0) * 6.0;
    let (s, v) = (c.y, c.z);
    let i = h.floor();
    let f = h - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i as i32 {
        0 => Vec3::new(v, t, p),
        1 => Vec3::new(q, v, p),
        2 => Vec3::new(p, v, t),
        3 => Vec3::new(p, q, v),
        4 => Vec3::new(t, p, v),
        _ => Vec3::new(v, p, q),
    }
}

/// One Composite blend mode: `a` the layer, `b` what is under it.
fn blend(mode: u8, a: Vec3, b: Vec3) -> Vec3 {
    let per = |f: &dyn Fn(f32, f32) -> f32| Vec3::new(f(a.x, b.x), f(a.y, b.y), f(a.z, b.z));
    match BLEND_MODES.get(mode as usize).copied().unwrap_or("Normal") {
        "Average" => (a + b) * 0.5,
        "Addition" => a + b,
        "Subtract" => b - a,
        "Darken" => a.min(b),
        "Multiply" => a * b,
        "Color Burn" => per(&|a, b| if a <= 0.0 { 0.0 } else { 1.0 - ((1.0 - b) / a).min(1.0) }),
        "Linear Burn" => a + b - Vec3::ONE,
        "Lighten" => a.max(b),
        "Screen" => Vec3::ONE - (Vec3::ONE - a) * (Vec3::ONE - b),
        "Color Dodge" => per(&|a, b| if a >= 1.0 { 1.0 } else { (b / (1.0 - a)).min(1.0) }),
        "Linear Dodge" => a + b,
        "Spotlight" => a * b * 2.0,
        "Spotlight Blend" => a * b * 2.0 + b,
        "Overlay" => per(&|a, b| if b < 0.5 { 2.0 * a * b } else { 1.0 - 2.0 * (1.0 - a) * (1.0 - b) }),
        "Soft Light" => per(&|a, b| if a < 0.5 { b - (1.0 - 2.0 * a) * b * (1.0 - b) } else { b + (2.0 * a - 1.0) * (b.sqrt() - b) }),
        "Hard Light" => per(&|a, b| if a < 0.5 { 2.0 * a * b } else { 1.0 - 2.0 * (1.0 - a) * (1.0 - b) }),
        "Pin Light" => per(&|a, b| if a < 0.5 { b.min(2.0 * a) } else { b.max(2.0 * a - 1.0) }),
        "Hard Mix" => per(&|a, b| if a + b >= 1.0 { 1.0 } else { 0.0 }),
        "Difference" => (a - b).abs(),
        "Exclusion" => a + b - 2.0 * a * b,
        "Hue" => {
            let (ha, hb) = (rgb_to_hsv(a), rgb_to_hsv(b));
            hsv_to_rgb(Vec3::new(ha.x, hb.y, hb.z))
        }
        "Saturation" => {
            let (ha, hb) = (rgb_to_hsv(a), rgb_to_hsv(b));
            hsv_to_rgb(Vec3::new(hb.x, ha.y, hb.z))
        }
        "Color" => {
            let (ha, hb) = (rgb_to_hsv(a), rgb_to_hsv(b));
            hsv_to_rgb(Vec3::new(ha.x, ha.y, hb.z))
        }
        "Value" => {
            let (ha, hb) = (rgb_to_hsv(a), rgb_to_hsv(b));
            hsv_to_rgb(Vec3::new(hb.x, hb.y, ha.z))
        }
        _ => a,
    }
}

/// Composite: layer 1 at the bottom, each above it blended by its mode
/// through its opacity, its map's alpha and its mask.
fn composite(map: &Map, sub: &dyn Fn(usize) -> Option<[f32; 4]>) -> [f32; 4] {
    let mut out = Vec3::ZERO;
    let mut alpha = 0.0f32;
    for (i, layer) in map.layers.iter().enumerate() {
        if !layer.visible {
            continue;
        }
        let Some(a) = sub(2 * i) else { continue };
        let mask = if layer.mask_visible { sub(2 * i + 1).map_or(1.0, |m| lum(Vec3::new(m[0], m[1], m[2]))) } else { 1.0 };
        let k = (a[3] * layer.opacity / 100.0 * mask).clamp(0.0, 1.0);
        let ac = Vec3::new(a[0], a[1], a[2]);
        // The first layer drawn has nothing under it to blend with.
        let mixed = if alpha <= 0.0 { ac } else { blend(layer.blend, ac, out) };
        out = out.lerp(mixed, k);
        alpha = k + alpha * (1.0 - k);
    }
    [out.x, out.y, out.z, alpha]
}

/// Color Correction: Channels, then Color (Hue Shift, Saturation, Hue
/// Tint), then Lightness (Standard: Brightness, Contrast).
fn color_correction(map: &Map, c: [f32; 4]) -> [f32; 4] {
    let e = &map.extra;
    let src = |k: u8| -> f32 {
        let m = (c[0] + c[1] + c[2]) / 3.0;
        match k {
            0..=3 => c[k as usize],
            4..=7 => 1.0 - c[(k - 4) as usize],
            8 => m,
            9 => 1.0,
            _ => 0.0,
        }
    };
    let mut ch = match e.channels {
        1 => [8, 8, 8, 3],
        2 => [4, 5, 6, 3],
        3 => e.custom_channels,
        _ => [0, 1, 2, 3],
    }
    .map(src);
    let mut col = Vec3::new(ch[0], ch[1], ch[2]);
    if e.hue_shift != 0.0 || e.cc_saturation != 0.0 {
        let mut hsv = rgb_to_hsv(col);
        hsv.x += e.hue_shift / 360.0;
        hsv.y = (hsv.y * (1.0 + e.cc_saturation / 100.0)).clamp(0.0, 1.0);
        col = hsv_to_rgb(hsv);
    }
    if e.tint_strength > 0.0 {
        // Hue Tint colours the pixels by their saturation; greys stay.
        let hsv = rgb_to_hsv(col);
        let tint = rgb_to_hsv(map.color(1));
        let tinted = hsv_to_rgb(Vec3::new(tint.x, hsv.y, hsv.z));
        col = col.lerp(tinted, e.tint_strength / 100.0);
    }
    col += Vec3::splat(e.brightness / 100.0);
    let k = 1.0 + e.contrast / 100.0;
    col = (col - Vec3::splat(0.5)) * k + Vec3::splat(0.5);
    ch[0] = col.x;
    ch[1] = col.y;
    ch[2] = col.z;
    ch
}

/// Falloff: Front (component 0) to Side (1) by the falloff type along the falloff direction.
fn falloff(map: &Map, ctx: &Ctx, sub: &dyn Fn(usize) -> Option<[f32; 4]>) -> [f32; 4] {
    let e = &map.extra;
    let n = ctx.normal;
    let dir = match e.falloff_direction {
        1 => ctx.cam_x,
        2 => ctx.cam_y,
        // Object without a picked object: the shaded object's local Z, as the reference does.
        3 | 6 => ctx.obj_axes.z_axis,
        4 => ctx.obj_axes.x_axis,
        5 => ctx.obj_axes.y_axis,
        7 => Vec3::X,
        8 => Vec3::Y,
        9 => Vec3::Z,
        _ => ctx.view,
    }
    .normalize_or_zero();
    let t = match e.falloff_type {
        0 => (1.0 - n.dot(dir)) * 0.5,
        2 => {
            let cos = n.dot(ctx.view).abs().clamp(0.0, 1.0);
            // Without Override Material IOR: a Standard material's default index, 1.5.
            fresnel(cos, if e.override_ior { e.ior } else { 1.5 })
        }
        3 => ctx.light,
        4 => {
            let t = (ctx.dist - e.near_distance) / (e.far_distance - e.near_distance).max(1e-4);
            if e.extrapolate { t } else { t.clamp(0.0, 1.0) }
        }
        _ => 1.0 - n.dot(dir).abs(),
    };
    let side = |i: usize, amount: f32| -> Vec3 {
        let col = map.color(i);
        match sub(i) {
            Some(m) => col.lerp(Vec3::new(m[0], m[1], m[2]), m[3] * amount / 100.0),
            None => col,
        }
    };
    let c = side(0, e.front_amount).lerp(side(1, e.side_amount), t.clamp(0.0, 1.0));
    [c.x, c.y, c.z, 1.0]
}

/// Fresnel reflectance for an unpolarised ray at cosine `cos` into index `ior`.
fn fresnel(cos: f32, ior: f32) -> f32 {
    let eta = ior.max(1.0);
    let g2 = eta * eta - 1.0 + cos * cos;
    let g = g2.max(0.0).sqrt();
    let a = (g - cos) / (g + cos);
    let b = (cos * (g + cos) - 1.0) / (cos * (g - cos) + 1.0);
    (0.5 * a * a * (1.0 + b * b)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::super::{bake, eval, eval_ctx, texture_key, Ctx, Images};
    use crate::material::{Map, MapType};
    use glam::Vec3;

    fn near(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.02)
    }

    /// The lowest and highest red over a 24 × 24 grid of the UV square
    /// (every channel checked to stay in 0–1).
    fn range(f: impl Fn(f32, f32) -> [f32; 4]) -> (f32, f32) {
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        for i in 0..24 {
            for j in 0..24 {
                let c = f(i as f32 / 24.0, j as f32 / 24.0);
                assert!(c.iter().all(|v| v.is_finite() && (-1e-4..=1.0001).contains(v)), "{c:?}");
                let l = c[0];
                lo = lo.min(l);
                hi = hi.max(l);
            }
        }
        (lo, hi)
    }

    #[test]
    fn every_procedural_map_varies_and_stays_in_range() {
        let im = Images::new();
        for kind in [MapType::Swirl, MapType::Tiles, MapType::GradientRamp, MapType::Cellular, MapType::Dent, MapType::Marble, MapType::PerlinMarble, MapType::Planet, MapType::Smoke, MapType::Speckle, MapType::Splat, MapType::Stucco, MapType::Waves, MapType::Wood, MapType::Noise] {
            let m = Map::new(kind, 100.0);
            let (lo, hi) = range(|u, v| eval(&m, [u, v], &im));
            assert!(lo >= -1e-4 && hi <= 1.0001, "{kind:?} in 0–1: {lo}..{hi}");
            assert!(hi - lo > 0.1, "{kind:?} varies over the UV square: {lo}..{hi}");
        }
    }

    #[test]
    fn three_d_maps_read_the_object_point() {
        let im = Images::new();
        for kind in [MapType::Cellular, MapType::Dent, MapType::Marble, MapType::PerlinMarble, MapType::Planet, MapType::Smoke, MapType::Speckle, MapType::Splat, MapType::Stucco, MapType::Waves, MapType::Wood, MapType::Noise] {
            let mut m = Map::new(kind, 100.0);
            let at = |m: &Map, z: f32| {
                let mut c = Ctx::from_uv([0.3, 0.6]);
                c.p_obj = Vec3::new(13.0, 27.0, z);
                eval_ctx(m, &c, &im)
            };
            let vals: Vec<f32> = (0..40).map(|i| at(&m, i as f32 * 3.7)[0]).collect();
            let (lo, hi) = vals.iter().fold((1.0f32, 0.0f32), |(l, h), v| (l.min(*v), h.max(*v)));
            assert!(hi - lo > 0.05, "{kind:?} changes along Z in object space: {lo}..{hi}");
            // Explicit Map Channel reads the UV, whatever the object point.
            m.set_parameter("Source", 2.0).unwrap();
            assert_eq!(at(&m, 0.0), at(&m, 50.0), "{kind:?} Explicit Map Channel");
            assert_eq!(at(&m, 0.0), eval(&m, [0.3, 0.6], &im));
            // Offset moves the pattern: sampling at p + offset matches p.
            let mut o = Map::new(kind, 100.0);
            o.set_parameter("Offset X", 10.0).unwrap();
            let mut c = Ctx::from_uv([0.0, 0.0]);
            c.p_obj = Vec3::new(20.0, 5.0, 3.0);
            let shifted = eval_ctx(&o, &c, &im);
            c.p_obj.x -= 10.0;
            assert!(near(shifted, eval_ctx(&Map::new(kind, 100.0), &c, &im)), "{kind:?} Offset X");
        }
    }

    #[test]
    fn tiles_bonds_gaps_and_holes() {
        let im = Images::new();
        let mut t = Map::new(MapType::Tiles, 100.0);
        t.set_parameter("Fade Variance", 0.0).unwrap();
        let tile = [t.colors[0].x, t.colors[0].y, t.colors[0].z, 1.0];
        let grout = [t.colors[1].x, t.colors[1].y, t.colors[1].z, 1.0];
        assert!(near(eval(&t, [0.125, 0.125], &im), tile), "a tile's middle");
        assert!(near(eval(&t, [0.25, 0.125], &im), grout), "between two tiles");
        // Stack Bond: the second row's joints line up; Running Bond shifts them half a tile.
        assert!(near(eval(&t, [0.001, 0.375], &im), grout));
        t.set_parameter("Preset Type", 1.0).unwrap();
        assert!(near(eval(&t, [0.001, 0.375], &im), tile), "Running Bond shifts the odd rows");
        assert!(near(eval(&t, [0.001, 0.125], &im), grout));
        t.set_parameter("% Holes", 100.0).unwrap();
        assert!(near(eval(&t, [0.125, 0.125], &im), grout), "every tile a hole");
        t.set_parameter("% Holes", 0.0).unwrap();
        t.set_parameter("Horizontal Gap", 5.0).unwrap();
        assert_eq!(t.extra.gap, [5.0, 5.0], "the gaps are locked");
        assert!(near(eval(&t, [0.125, 0.22], &im), grout), "a wider gap");
        t.set_parameter("Color Variance", 50.0).unwrap();
        t.set_parameter("Horizontal Gap", 0.5).unwrap();
        let mut tiles = Vec::new();
        for i in 0..4 {
            for j in 0..4 {
                tiles.push(eval(&t, [0.125 + 0.25 * i as f32, 0.125 + 0.25 * j as f32], &im)[0]);
            }
        }
        let (lo, hi) = tiles.iter().fold((1.0f32, 0.0f32), |(l, h), v| (l.min(*v), h.max(*v)));
        assert!(hi - lo > 0.05, "Color Variance varies the tiles: {lo}..{hi}");
    }

    #[test]
    fn gradient_ramp_types_and_interpolation() {
        let im = Images::new();
        let mut g = Map::new(MapType::GradientRamp, 100.0);
        assert!(near(eval(&g, [0.0, 0.5], &im), [1.0, 0.0, 0.0, 1.0]), "Flag #1 red at the left");
        assert!(near(eval(&g, [0.5, 0.5], &im), [0.0, 1.0, 0.0, 1.0]), "Flag #3 green in the middle");
        assert!(near(eval(&g, [0.999, 0.5], &im), [0.0, 0.0, 1.0, 1.0]), "Flag #2 blue at the right");
        assert!(near(eval(&g, [0.25, 0.5], &im), [0.5, 0.5, 0.0, 1.0]), "Linear between flags");
        g.set_parameter("Interpolation", 5.0).unwrap();
        assert!(near(eval(&g, [0.25, 0.5], &im), [1.0, 0.0, 0.0, 1.0]), "Solid: no blend");
        g.set_parameter("Interpolation", 4.0).unwrap();
        g.set_parameter("Gradient Type", 8.0).unwrap();
        assert!(near(eval(&g, [0.5, 0.5], &im), [1.0, 0.0, 0.0, 1.0]), "Radial: Flag #1 at the centre");
        g.set_color("Flag #3", [255, 255, 255]).unwrap();
        g.set_parameter("Gradient Type", 4.0).unwrap();
        assert!(near(eval(&g, [0.5, 0.5], &im), [1.0; 4]));
        // A flag's map replaces its colour.
        *g.sub_map_slot_mut(1) = Some(Box::new(Map::new(MapType::Checker, 100.0)));
        assert!(near(eval(&g, [0.01, 0.1], &im), [0.0, 0.0, 0.0, 1.0]), "the checker's black under Flag #1");
        g.set_parameter("Noise Amount", 1.0).unwrap();
        let (lo, hi) = range(|u, v| eval(&g, [u, v], &im));
        assert!(hi - lo > 0.2);
    }

    #[test]
    fn composite_mask_multiply_tint_and_correction() {
        let im = Images::new();
        // RGB Tint of white (no map): R + G + B swatches.
        assert!(near(eval(&Map::new(MapType::RgbTint, 100.0), [0.5, 0.5], &im), [1.0; 4]));
        let mut red = Map::new(MapType::RgbTint, 100.0);
        red.set_color("G", [0, 0, 0]).unwrap();
        red.set_color("B", [0, 0, 0]).unwrap();
        assert!(near(eval(&red, [0.5, 0.5], &im), [1.0, 0.0, 0.0, 1.0]));
        let mut tint = Map::new(MapType::RgbTint, 100.0);
        tint.set_color("R", [0, 0, 255]).unwrap();
        *tint.sub_map_slot_mut(3) = Some(Box::new(red.clone()));
        assert!(near(eval(&tint, [0.5, 0.5], &im), [0.0, 0.0, 1.0, 1.0]), "the map's red becomes the R swatch");
        assert!(tint.set_color("Map", [1, 2, 3]).is_err(), "the map component has no swatch");

        let mut c = Map::new(MapType::Composite, 100.0);
        *c.sub_map_slot_mut(0) = Some(Box::new(Map::new(MapType::Checker, 100.0)));
        assert!(near(eval(&c, [0.75, 0.25], &im), [1.0; 4]));
        c.set_parameter("Add Layer", 1.0).unwrap();
        *c.sub_map_slot_mut(2) = Some(Box::new(red.clone()));
        assert!(near(eval(&c, [0.75, 0.25], &im), [1.0, 0.0, 0.0, 1.0]), "Normal: layer 2 covers layer 1");
        c.set_parameter("Layer 2 Blend Mode", 5.0).unwrap();
        assert!(near(eval(&c, [0.25, 0.25], &im), [0.0, 0.0, 0.0, 1.0]), "Multiply over black");
        assert!(near(eval(&c, [0.75, 0.25], &im), [1.0, 0.0, 0.0, 1.0]), "Multiply over white");
        c.set_parameter("Layer 2 Blend Mode", 0.0).unwrap();
        c.set_parameter("Layer 2 Opacity", 50.0).unwrap();
        assert!(near(eval(&c, [0.75, 0.25], &im), [1.0, 0.5, 0.5, 1.0]), "half opacity");
        *c.sub_map_slot_mut(3) = Some(Box::new(Map::new(MapType::Checker, 100.0)));
        assert!(near(eval(&c, [0.25, 0.25], &im), [0.0, 0.0, 0.0, 1.0]), "the mask's black hides layer 2");
        c.set_parameter("Mask 2 Visible", 0.0).unwrap();
        assert!(near(eval(&c, [0.25, 0.25], &im), [0.5, 0.0, 0.0, 1.0]));
        c.set_parameter("Layer 2 Visible", 0.0).unwrap();
        assert!(near(eval(&c, [0.75, 0.25], &im), [1.0; 4]), "a hidden layer");

        let mut mask = Map::new(MapType::Mask, 100.0);
        *mask.sub_map_slot_mut(0) = Some(Box::new(red.clone()));
        *mask.sub_map_slot_mut(1) = Some(Box::new(Map::new(MapType::Checker, 100.0)));
        assert_eq!(eval(&mask, [0.25, 0.25], &im)[3], 0.0, "black in the mask is clear");
        assert_eq!(eval(&mask, [0.75, 0.25], &im)[3], 1.0);
        mask.set_parameter("Invert Mask", 1.0).unwrap();
        assert_eq!(eval(&mask, [0.25, 0.25], &im)[3], 1.0);

        let mut mul = Map::new(MapType::RgbMultiply, 100.0);
        mul.set_color("Color #1", [255, 128, 0]).unwrap();
        mul.set_color("Color #2", [128, 255, 255]).unwrap();
        let c = eval(&mul, [0.5, 0.5], &im);
        assert!((c[0] - 0.502).abs() < 0.01 && (c[1] - 0.502).abs() < 0.01 && c[2] == 0.0, "{c:?}");

        let mut cc = Map::new(MapType::ColorCorrection, 100.0);
        *cc.sub_map_slot_mut(0) = Some(Box::new(red));
        cc.set_parameter("Channels", 2.0).unwrap();
        assert!(near(eval(&cc, [0.5, 0.5], &im), [0.0, 1.0, 1.0, 1.0]), "Invert");
        cc.set_parameter("Channels", 1.0).unwrap();
        let g = 1.0 / 3.0;
        assert!(near(eval(&cc, [0.5, 0.5], &im), [g, g, g, 1.0]), "Monochrome");
        cc.set_parameter("Channels", 3.0).unwrap();
        cc.set_parameter("Green", 0.0).unwrap();
        assert!(near(eval(&cc, [0.5, 0.5], &im), [1.0, 1.0, 0.0, 1.0]), "Custom: Green = Red");
        cc.set_parameter("Channels", 0.0).unwrap();
        cc.set_parameter("Hue Shift", 120.0).unwrap();
        assert!(near(eval(&cc, [0.5, 0.5], &im), [0.0, 1.0, 0.0, 1.0]), "red shifted 120° is green");
        cc.set_parameter("Hue Shift", 0.0).unwrap();
        cc.set_parameter("Saturation", -100.0).unwrap();
        assert!(near(eval(&cc, [0.5, 0.5], &im), [1.0, 1.0, 1.0, 1.0]), "no saturation: the value");
        cc.set_parameter("Brightness", -50.0).unwrap();
        assert!(near(eval(&cc, [0.5, 0.5], &im), [0.5, 0.5, 0.5, 1.0]));
        cc.set_parameter("Contrast", 100.0).unwrap();
        assert!(near(eval(&cc, [0.5, 0.5], &im), [0.5, 0.5, 0.5, 1.0]), "mid grey stays under contrast");

        assert_eq!(eval(&Map::new(MapType::VertexColor, 100.0), [0.3, 0.3], &im), [1.0; 4]);
    }

    #[test]
    fn output_rollout_and_output_map() {
        let im = Images::new();
        let mut n = Map::new(MapType::Noise, 100.0);
        let before = eval(&n, [0.3, 0.4], &im);
        n.set_parameter("Invert", 1.0).unwrap();
        let after = eval(&n, [0.3, 0.4], &im);
        assert!((after[0] - (1.0 - before[0])).abs() < 1e-5);
        n.set_parameter("Invert", 0.0).unwrap();
        n.set_parameter("RGB Level", 0.5).unwrap();
        n.set_parameter("RGB Offset", 0.25).unwrap();
        assert!((eval(&n, [0.3, 0.4], &im)[0] - (before[0] * 0.5 + 0.25)).abs() < 1e-5);
        n.set_parameter("Alpha from RGB Intensity", 1.0).unwrap();
        let c = eval(&n, [0.3, 0.4], &im);
        assert!((c[3] - c[0]).abs() < 1e-5);
        // Checker has no Output rollout; an Output map gives it one.
        let mut o = Map::new(MapType::Output, 100.0);
        *o.sub_map_slot_mut(0) = Some(Box::new(Map::new(MapType::Checker, 100.0)));
        o.set_parameter("Invert", 1.0).unwrap();
        assert!(near(eval(&o, [0.25, 0.25], &im), [1.0; 4]));
        o.set_parameter("Output Amount", 0.5).unwrap();
        o.set_parameter("Clamp", 1.0).unwrap();
        assert!(near(eval(&o, [0.25, 0.25], &im), [0.5, 0.5, 0.5, 0.5]));
    }

    #[test]
    fn falloff_types_and_directions() {
        let im = Images::new();
        let mut f = Map::new(MapType::Falloff, 100.0);
        let at = |f: &Map, n: Vec3| {
            let mut c = Ctx::from_uv([0.5, 0.5]);
            c.normal = n.normalize();
            eval_ctx(f, &c, &im)
        };
        // Perpendicular / Parallel along the view: Front black facing, Side white edge-on.
        assert!(near(at(&f, Vec3::Z), [0.0, 0.0, 0.0, 1.0]));
        assert!(near(at(&f, Vec3::X), [1.0; 4]));
        assert!(near(eval(&f, [0.2, 0.7], &im), [0.0, 0.0, 0.0, 1.0]), "a bake faces the view: Front");
        assert!(near(at(&f, -Vec3::Z), [0.0, 0.0, 0.0, 1.0]), "parallel either way");
        f.set_parameter("Falloff Type", 0.0).unwrap();
        assert!(near(at(&f, -Vec3::Z), [1.0; 4]), "Towards / Away: facing away is Side");
        assert!(near(at(&f, Vec3::X), [0.5, 0.5, 0.5, 1.0]));
        f.set_parameter("Falloff Type", 2.0).unwrap();
        let front = at(&f, Vec3::Z)[0];
        let grazing = at(&f, Vec3::new(1.0, 0.0, 0.02))[0];
        assert!(front < 0.1 && grazing > 0.8, "Fresnel: {front} {grazing}");
        f.set_parameter("Index of Refraction", 3.0).unwrap();
        assert_eq!(at(&f, Vec3::Z)[0], front, "the IOR spinner needs Override Material IOR");
        f.set_parameter("Override Material IOR", 1.0).unwrap();
        assert!(at(&f, Vec3::Z)[0] > front, "a higher IOR reflects more facing");
        // World X: a normal along X is parallel (Front), one along Z perpendicular (Side).
        f.set_parameter("Falloff Type", 1.0).unwrap();
        f.set_parameter("Falloff Direction", 7.0).unwrap();
        assert!(near(at(&f, Vec3::X), [0.0, 0.0, 0.0, 1.0]));
        assert!(near(at(&f, Vec3::Z), [1.0; 4]));
        // Shadow / Light: lit is the bottom (Side) colour.
        f.set_parameter("Falloff Type", 3.0).unwrap();
        let mut c = Ctx::from_uv([0.5, 0.5]);
        c.light = 0.0;
        assert!(near(eval_ctx(&f, &c, &im), [0.0, 0.0, 0.0, 1.0]));
        // Distance Blend between Near and Far.
        f.set_parameter("Falloff Type", 4.0).unwrap();
        c.dist = 50.0;
        assert!(near(eval_ctx(&f, &c, &im), [0.5, 0.5, 0.5, 1.0]));
        // A map in Front at Front Amount 50 is half over the colour.
        f.set_parameter("Falloff Type", 1.0).unwrap();
        f.set_parameter("Falloff Direction", 0.0).unwrap();
        f.set_parameter("Front Amount", 50.0).unwrap();
        let mut white = Map::new(MapType::Checker, 100.0);
        white.set_color("Color #1", [255, 255, 255]).unwrap();
        *f.sub_map_slot_mut(0) = Some(Box::new(white));
        assert!(near(at(&f, Vec3::Z), [0.5, 0.5, 0.5, 1.0]));
    }

    #[test]
    fn mix_curve_and_texture_keys() {
        let im = Images::new();
        let mut mix = Map::new(MapType::Mix, 100.0);
        mix.set_parameter("Mix Amount", 60.0).unwrap();
        assert!((eval(&mix, [0.5, 0.5], &im)[0] - 0.6).abs() < 1e-4);
        mix.set_parameter("Use Curve", 1.0).unwrap();
        mix.set_parameter("Upper", 0.5).unwrap();
        mix.set_parameter("Lower", 0.5).unwrap();
        assert_eq!(eval(&mix, [0.5, 0.5], &im)[0], 1.0, "a zero transition zone is a hard edge");
        mix.set_parameter("Upper", 0.9).unwrap();
        mix.set_parameter("Lower", 0.1).unwrap();
        let t = eval(&mix, [0.5, 0.5], &im)[0];
        assert!(t > 0.6 && t < 0.8, "{t}");
        // Changing a procedural parameter changes the texture key; the bake is 256².
        let a = Map::new(MapType::Wood, 100.0);
        let mut b = a.clone();
        b.set_parameter("Grain Thickness", 3.0).unwrap();
        assert_ne!(texture_key(&a), texture_key(&b));
        let img = bake(&a, &im).unwrap();
        assert_eq!((img.width, img.height), (256, 256));
    }

    #[test]
    fn old_maps_load_with_the_new_fields_defaulted() {
        let m = Map::new(MapType::Checker, 100.0);
        let mut v = serde_json::to_value(&m).unwrap();
        for k in ["coords3", "output", "extra", "layers", "flags", "extra_colors", "extra_maps"] {
            v.as_object_mut().unwrap().remove(k);
        }
        let back: Map = serde_json::from_value(v).unwrap();
        assert_eq!(back, m);
        let mut ramp = Map::new(MapType::GradientRamp, 100.0);
        ramp.set_parameter("Add Flag", 30.0).unwrap();
        *ramp.sub_map_slot_mut(4) = Some(Box::new(Map::new(MapType::Planet, 100.0)));
        let back: Map = serde_json::from_str(&serde_json::to_string(&ramp).unwrap()).unwrap();
        assert_eq!(back, ramp);
    }
}
