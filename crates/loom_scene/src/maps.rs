//! Evaluating maps to pixels: Bitmap (with Cropping/Placement, Mirror and
//! Tile), Checker, Gradient, Noise, Mix and the other 2D, 3D, compositor,
//! colour modifier and Falloff maps (`procedural`). The viewport, the
//! Realistic renderer and the Material Editor's sample slots all bake or
//! evaluate maps here, so they show the same thing. Colours are 0–1 sRGB
//! values as the editor's swatches hold them; bitmaps are 8-bit sRGB RGBA as
//! image files store them.

use crate::material::{Map, MapType};
use glam::Vec3;
use std::collections::HashMap;
use std::sync::Arc;

mod procedural;

/// Units across a map channel's 0–1 square when a 3D map reads UVW
/// (Explicit Map Channel, and every 3D map in a bake or a flat sample):
/// Noise's default Size 25 then shows four features across.
pub const UV_UNITS: f32 = 100.0;

/// What a map sees at the point being shaded. 3D maps read the object or
/// world point, Falloff the normal and view; with only a UV (`Ctx::from_uv`,
/// as in a bake) the points are the UV square scaled by `UV_UNITS`, the
/// normal faces the view and the point is fully lit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ctx {
    /// Surface UV (map channel coordinates).
    pub uv: [f32; 2],
    pub p_obj: Vec3,
    pub p_world: Vec3,
    /// World-space unit normal.
    pub normal: Vec3,
    /// World-space unit vector from the point towards the eye (the camera's Z axis).
    pub view: Vec3,
    /// The camera's X and Y axes in world space.
    pub cam_x: Vec3,
    pub cam_y: Vec3,
    /// The object's local axes in world space (columns).
    pub obj_axes: glam::Mat3,
    /// Distance from the eye.
    pub dist: f32,
    /// Light falling on the point, 0–1 (Falloff's Shadow / Light).
    pub light: f32,
    /// The UV extent the sample covers, across U and V (0: a point); a
    /// bitmap's Filtering averages its pixels over it.
    pub footprint: [f32; 2],
}

impl Ctx {
    pub fn from_uv(uv: [f32; 2]) -> Self {
        let p = Vec3::new(uv[0], uv[1], 0.0) * UV_UNITS;
        Self { uv, p_obj: p, p_world: p, normal: Vec3::Z, view: Vec3::Z, cam_x: Vec3::X, cam_y: Vec3::Y, obj_axes: glam::Mat3::IDENTITY, dist: 0.0, light: 1.0, footprint: [0.0; 2] }
    }
}

/// An RGBA8 image, rows from the top. Its filtering tables (the halved
/// levels of Pyramidal filtering, the summed-area table of Summed Area) are
/// made the first time a sample needs them.
#[derive(Debug, Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pyramid: std::sync::OnceLock<Vec<Image>>,
    sat: std::sync::OnceLock<Vec<[u64; 4]>>,
}

impl PartialEq for Image {
    fn eq(&self, o: &Self) -> bool {
        self.width == o.width && self.height == o.height && self.rgba == o.rgba
    }
}

impl Eq for Image {}

/// Bitmap Parameters > Filtering: how a bitmap's pixels are averaged
/// over the area a sample covers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Filtering {
    /// Halved copies of the image, read at the level of the sample's size.
    #[default]
    Pyramidal,
    /// The exact average over the sample's rectangle (a summed-area table).
    SummedArea,
    /// No filtering: the nearest pixel.
    None,
}

impl Image {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self { width, height, rgba, pyramid: Default::default(), sat: Default::default() }
    }

    fn texel(&self, x: i64, y: i64) -> [f32; 4] {
        let (w, h) = (self.width as i64, self.height as i64);
        let i = ((y.rem_euclid(h) * w + x.rem_euclid(w)) * 4) as usize;
        [0, 1, 2, 3].map(|c| self.rgba[i + c] as f32 / 255.0)
    }

    /// Bilinear, repeat-wrapped sample at image coordinates `x`, `y` in 0–1 (y down), 0–1 RGBA.
    pub fn sample(&self, x: f32, y: f32) -> [f32; 4] {
        if self.width == 0 || self.height == 0 {
            return [0.0; 4];
        }
        let fx = x * self.width as f32 - 0.5;
        let fy = y * self.height as f32 - 0.5;
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let (a, b, c, d) = (self.texel(x0, y0), self.texel(x0 + 1, y0), self.texel(x0, y0 + 1), self.texel(x0 + 1, y0 + 1));
        [0, 1, 2, 3].map(|k| (a[k] * (1.0 - tx) + b[k] * tx) * (1.0 - ty) + (c[k] * (1.0 - tx) + d[k] * tx) * ty)
    }

    /// The sample at `x`, `y` (0–1, y down) over a footprint `extent`
    /// pixels across and down, filtered by `filtering`. A footprint of a
    /// pixel or less reads bilinearly (None: the nearest pixel).
    pub fn sample_filtered(&self, x: f32, y: f32, extent: [f32; 2], filtering: Filtering) -> [f32; 4] {
        if self.width == 0 || self.height == 0 {
            return [0.0; 4];
        }
        let size = extent[0].max(extent[1]);
        match filtering {
            Filtering::None => self.texel((x * self.width as f32).floor() as i64, (y * self.height as f32).floor() as i64),
            _ if size <= 1.0 => self.sample(x, y),
            Filtering::Pyramidal => {
                let levels = self.pyramid();
                let lod = size.log2().clamp(0.0, levels.len() as f32);
                let l = lod.floor() as usize;
                let t = lod - l as f32;
                let at = |l: usize| if l == 0 { self.sample(x, y) } else { levels.get(l - 1).or(levels.last()).map_or_else(|| self.sample(x, y), |i| i.sample(x, y)) };
                let (a, b) = (at(l), at(l + 1));
                [0, 1, 2, 3].map(|k| a[k] + (b[k] - a[k]) * t)
            }
            Filtering::SummedArea => {
                let (w, h) = (self.width as f32, self.height as f32);
                let (cx, cy) = (x * w, y * h);
                let (ex, ey) = (extent[0].clamp(1.0, w), extent[1].clamp(1.0, h));
                let x0 = (cx - ex * 0.5).round() as i64;
                let y0 = (cy - ey * 0.5).round() as i64;
                let (x1, y1) = (x0 + (ex.round() as i64).max(1), y0 + (ey.round() as i64).max(1));
                let sat = self.sat();
                let sum = |k: usize| self.area_sum(sat, x1, y1, k) - self.area_sum(sat, x0, y1, k) - self.area_sum(sat, x1, y0, k) + self.area_sum(sat, x0, y0, k);
                let n = ((x1 - x0) * (y1 - y0)) as f64 * 255.0;
                [0, 1, 2, 3].map(|k| (sum(k) as f64 / n) as f32)
            }
        }
    }

    /// The halved levels below the image (Pyramidal), down to 1 × 1.
    fn pyramid(&self) -> &Vec<Image> {
        self.pyramid.get_or_init(|| {
            let mut out: Vec<Image> = Vec::new();
            let (mut w, mut h) = (self.width, self.height);
            while w > 1 || h > 1 {
                let src = out.last().unwrap_or(self);
                let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
                out.push(Image::new(nw, nh, half(src, nw, nh)));
                (w, h) = (nw, nh);
            }
            out
        })
    }

    /// Per corner (x, y), 0..=w by 0..=h: the channel sums of the pixels
    /// above and left of it.
    fn sat(&self) -> &Vec<[u64; 4]> {
        self.sat.get_or_init(|| {
            let (w, h) = (self.width as usize, self.height as usize);
            let mut t = vec![[0u64; 4]; (w + 1) * (h + 1)];
            for y in 0..h {
                let mut row = [0u64; 4];
                for x in 0..w {
                    let i = (y * w + x) * 4;
                    for k in 0..4 {
                        row[k] += self.rgba[i + k] as u64;
                        t[(y + 1) * (w + 1) + x + 1][k] = t[y * (w + 1) + x + 1][k] + row[k];
                    }
                }
            }
            t
        })
    }

    /// The channel `k` sum over pixels [0, x) × [0, y) of the image repeated
    /// across the plane (x, y may lie outside it).
    fn area_sum(&self, sat: &[[u64; 4]], x: i64, y: i64, k: usize) -> i64 {
        let (w, h) = (self.width as i64, self.height as i64);
        let (qx, rx, qy, ry) = (x.div_euclid(w), x.rem_euclid(w), y.div_euclid(h), y.rem_euclid(h));
        let at = |a: i64, b: i64| sat[(b * (w + 1) + a) as usize][k] as i64;
        qx * qy * at(w, h) + qx * at(w, ry) + qy * at(rx, h) + at(rx, ry)
    }
}

/// `src` averaged down to `w` × `h` (each pixel the 2 × 2 block, or the
/// pair, it covers).
fn half(src: &Image, w: u32, h: u32) -> Vec<u8> {
    let (sx, sy) = ((src.width / w).max(1), (src.height / h).max(1));
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0u32; 4];
            for j in 0..sy {
                for i in 0..sx {
                    let p = (((y * sy + j).min(src.height - 1) * src.width + (x * sx + i).min(src.width - 1)) * 4) as usize;
                    for k in 0..4 {
                        acc[k] += src.rgba[p + k] as u32;
                    }
                }
            }
            let n = sx * sy;
            out.extend(acc.map(|a| ((a + n / 2) / n) as u8));
        }
    }
    out
}

/// The image's mip levels after level 0, as `half` makes them (the GPU's
/// Pyramidal filtering reads the same levels).
pub fn mip_chain(img: &Image) -> Vec<&Image> {
    img.pyramid().iter().collect()
}

/// Images by path, loaded once for a map tree.
pub type Images = HashMap<String, Option<Arc<Image>>>;

/// Loads every bitmap the map tree uses through `load`.
pub fn load_images(map: &Map, images: &mut Images, load: &mut dyn FnMut(&str) -> Option<Arc<Image>>) {
    if map.kind == MapType::Bitmap && !map.path.is_empty() && !images.contains_key(&map.path) {
        let img = load(&map.path);
        images.insert(map.path.clone(), img);
    }
    for sub in map.components() {
        load_images(sub, images, load);
    }
}

/// The map's colour at surface UV `uv` (map channel coordinates), 0–1 RGBA;
/// alpha 0 where a decal or a placed bitmap leaves the surface uncovered.
pub fn eval(map: &Map, uv: [f32; 2], images: &Images) -> [f32; 4] {
    eval_ctx(map, &Ctx::from_uv(uv), images)
}

/// The map's colour at a shaded point: 3D maps read its object / world
/// point, Falloff its normal and view direction.
pub fn eval_ctx(map: &Map, ctx: &Ctx, images: &Images) -> [f32; 4] {
    eval_at(map, map.coords.apply(ctx.uv), ctx, images)
}

/// A Bitmap the GPU can sample as it is (no cropping, mirroring or Output change).
pub fn is_plain_bitmap(map: &Map) -> bool {
    map.kind == MapType::Bitmap && !map.crop.apply && !map.coords.mirror[0] && !map.coords.mirror[1] && map.output == crate::map_params::MapOutput::default() && map.channels.plain()
}

/// The map's mono output at a shaded point (what a mono slot such as
/// Opacity or Specular Level reads): a Bitmap's Mono Channel Output (RGB
/// Intensity or Alpha), any other map's intensity.
pub fn eval_mono(map: &Map, ctx: &Ctx, images: &Images) -> f32 {
    let c = eval_ctx(map, ctx, images);
    if map.kind == MapType::Bitmap && map.channels.mono_alpha { c[3] } else { (c[0] + c[1] + c[2]) / 3.0 }
}

/// Mirror / Tile / decal fold of a map-space coordinate to the image's 0–1.
fn fold(m: f32, mirror: bool, tile: bool) -> Option<f32> {
    if mirror {
        let f = m.rem_euclid(1.0);
        Some(if f < 0.5 { 2.0 * f } else { 2.0 - 2.0 * f })
    } else if tile {
        Some(m.rem_euclid(1.0))
    } else {
        (0.0..=1.0).contains(&m).then_some(m)
    }
}

/// `m` in map space (after Offset/Tiling/Angle), `ctx` the shaded point the
/// colour components' own maps read; then the Output rollout.
fn eval_at(map: &Map, m: [f32; 2], ctx: &Ctx, images: &Images) -> [f32; 4] {
    let c = eval_raw(map, m, ctx, images);
    if map.kind.has_output() { map.output.apply(c) } else { c }
}

/// The pattern-space point a 3D map reads (Coordinates rollout Source, Offset, Tiling, Angle).
fn point3(map: &Map, ctx: &Ctx) -> Vec3 {
    let p = match map.coords3.source {
        0 => ctx.p_obj,
        1 => ctx.p_world,
        _ => Vec3::new(ctx.uv[0], ctx.uv[1], 0.0) * UV_UNITS,
    };
    map.coords3.apply(p)
}

fn eval_raw(map: &Map, m: [f32; 2], ctx: &Ctx, images: &Images) -> [f32; 4] {
    let c = &map.coords;
    let (u, v) = if map.kind.has_2d_coords() {
        let (Some(u), Some(v)) = (fold(m[0], c.mirror[0], c.tile[0]), fold(m[1], c.mirror[1], c.tile[1])) else { return [0.0; 4] };
        (u, v)
    } else {
        (m[0], m[1])
    };
    let p = &map.params;
    let sub = |i: usize| -> Option<[f32; 4]> { map.sub_map(i).filter(|s| s.enabled).map(|s| eval_ctx(s, ctx, images)) };
    let component = |i: usize| -> Vec3 {
        let col = map.color(i);
        match sub(i) {
            Some(t) => col.lerp(Vec3::new(t[0], t[1], t[2]), t[3]),
            None => col,
        }
    };
    let rgb = |c: Vec3| [c.x, c.y, c.z, 1.0];
    match map.kind {
        MapType::Bitmap => {
            let Some(img) = images.get(&map.path).cloned().flatten() else { return [0.0; 4] };
            let k = &map.crop;
            let ch = &map.channels;
            // Image coordinates from the top left.
            let (x, y) = (u, 1.0 - v);
            // The sample's footprint in map space (through Tiling and Angle,
            // widened by Blur), then in image pixels.
            let [a, b, _, d, e, _] = c.matrix();
            let f = ctx.footprint;
            let blur = c.blur.max(0.0);
            let ext = [(a.abs() * f[0] + b.abs() * f[1]) * blur * img.width as f32, (d.abs() * f[0] + e.abs() * f[1]) * blur * img.height as f32];
            let at = |x: f32, y: f32, scale: [f32; 2]| ch.apply(img.sample_filtered(x, y, [ext[0] * scale[0], ext[1] * scale[1]], ch.filtering));
            if !k.apply {
                return at(x, y, [1.0; 2]);
            }
            if k.place {
                // Jitter Placement: each tile gets its own random size and place.
                let (w, h, pu, pv) = match ch.jitter_amount() {
                    Some(j) => {
                        let tile = [m[0].floor() as i32, m[1].floor() as i32];
                        let r = |n: u32| procedural::hash01(tile[0], tile[1], n);
                        let size = 1.0 - j * r(1);
                        (size, size, r(2) * (1.0 - size), r(3) * (1.0 - size))
                    }
                    None => (k.w, k.h, k.u, k.v),
                };
                let (px, py) = ((x - pu) / w.max(1e-6), (y - pv) / h.max(1e-6));
                if !(0.0..=1.0).contains(&px) || !(0.0..=1.0).contains(&py) {
                    return [0.0; 4];
                }
                at(px, py, [1.0 / w.max(1e-6), 1.0 / h.max(1e-6)])
            } else {
                at(k.u + x * k.w, k.v + y * k.h, [k.w, k.h])
            }
        }
        MapType::Checker => {
            // Two checks per tile each way; Soften blurs the edges.
            let w = p.soften * 0.5;
            let edge = |x: f32| {
                // Signed distance to the nearest check edge: < 0 in the first check, > 0 in the second.
                let g = (x * 2.0).rem_euclid(2.0);
                let d = if g < 1.0 { -(0.5 - (g - 0.5).abs()) } else { 0.5 - (g - 1.5).abs() };
                if w <= 0.0 {
                    if d >= 0.0 { 1.0 } else { 0.0 }
                } else {
                    smoothstep(-w, w, d)
                }
            };
            let (su, sv) = (edge(u), edge(v));
            let t = su + sv - 2.0 * su * sv;
            rgb(component(0).lerp(component(1), t))
        }
        MapType::Gradient => {
            // Color #3 at the bottom (radial: the centre), #1 at the top (the rim).
            let t = if p.radial { ((u - 0.5).hypot(v - 0.5) * 2.0).min(1.0) } else { v };
            // Noise group: 3D noise over (U, V, Phase) scaled like the Noise
            // map's Size, through the Low / High thresholds (Smooth eases the
            // cut), moves the interpolation by up to ±Amount.
            // Noise Amount and Smooth are kept where Gradient Ramp keeps its own.
            let (amount, smooth) = (map.extra.noise_amount, map.extra.smooth);
            let t = if amount > 0.0 {
                let q = Vec3::new(u * UV_UNITS, v * UV_UNITS, 0.0) / p.size.max(1e-3) + Vec3::new(0.0, 0.0, p.phase);
                let n = fractal_noise(q, p.noise_type, p.levels);
                let hard = ((n - p.low) / (p.high - p.low).max(1e-4)).clamp(0.0, 1.0);
                let nt = hard + (smoothstep(0.0, 1.0, hard) - hard) * smooth;
                (t + amount * (nt * 2.0 - 1.0)).clamp(0.0, 1.0)
            } else {
                t
            };
            let pos = p.position.clamp(1e-4, 1.0 - 1e-4);
            let c = if t < pos { component(2).lerp(component(1), t / pos) } else { component(1).lerp(component(0), (t - pos) / (1.0 - pos)) };
            rgb(c)
        }
        MapType::Noise => {
            // 3D at the Coordinates rollout's point; Size 25 gives four
            // features across a UV square (`UV_UNITS`).
            let q = point3(map, ctx) / p.size.max(1e-3) + Vec3::new(0.0, 0.0, p.phase);
            let n = fractal_noise(q, p.noise_type, p.levels);
            let t = ((n - p.low) / (p.high - p.low).max(1e-4)).clamp(0.0, 1.0);
            rgb(component(0).lerp(component(1), t))
        }
        MapType::Mix => {
            let t = match sub(2) {
                Some(t) => (t[0] + t[1] + t[2]) / 3.0,
                None => p.mix_amount / 100.0,
            };
            let e = &map.extra;
            // Mixing Curve: the transition zone between Lower and Upper.
            let t = if e.use_curve {
                let (lo, hi) = (e.lower.min(e.upper), e.upper.max(e.lower));
                if hi - lo < 1e-4 { if t < lo { 0.0 } else { 1.0 } } else { smoothstep(lo, hi, t) }
            } else {
                t
            };
            rgb(component(0).lerp(component(1), t.clamp(0.0, 1.0)))
        }
        MapType::NormalBump => rgb(component(0)),
        _ => procedural::eval(map, [u, v], point3(map, ctx), ctx, &component, &sub),
    }
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Improved Perlin noise, about -1–1.
fn perlin(p: Vec3) -> f32 {
    const PERM: [u8; 256] = {
        let mut t = [0u8; 256];
        let mut i = 0;
        let mut s: u32 = 0x9E37_79B9;
        while i < 256 {
            t[i] = i as u8;
            i += 1;
        }
        let mut i = 255;
        while i > 0 {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let j = (s % (i as u32 + 1)) as usize;
            let x = t[i];
            t[i] = t[j];
            t[j] = x;
            i -= 1;
        }
        t
    };
    let h = |i: i32| PERM[(i & 255) as usize] as i32;
    let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let grad = |hash: i32, x: f32, y: f32, z: f32| {
        let hh = hash & 15;
        let u = if hh < 8 { x } else { y };
        let v = if hh < 4 { y } else if hh == 12 || hh == 14 { x } else { z };
        (if hh & 1 == 0 { u } else { -u }) + (if hh & 2 == 0 { v } else { -v })
    };
    let (xi, yi, zi) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    let (x, y, z) = (p.x - p.x.floor(), p.y - p.y.floor(), p.z - p.z.floor());
    let (u, v, w) = (fade(x), fade(y), fade(z));
    let a = h(xi) + yi;
    let (aa, ab) = (h(a) + zi, h(a + 1) + zi);
    let b = h(xi + 1) + yi;
    let (ba, bb) = (h(b) + zi, h(b + 1) + zi);
    let lerp = |t: f32, a: f32, b: f32| a + t * (b - a);
    lerp(
        w,
        lerp(v, lerp(u, grad(h(aa), x, y, z), grad(h(ba), x - 1.0, y, z)), lerp(u, grad(h(ab), x, y - 1.0, z), grad(h(bb), x - 1.0, y - 1.0, z))),
        lerp(v, lerp(u, grad(h(aa + 1), x, y, z - 1.0), grad(h(ba + 1), x - 1.0, y, z - 1.0)), lerp(u, grad(h(ab + 1), x, y - 1.0, z - 1.0), grad(h(bb + 1), x - 1.0, y - 1.0, z - 1.0))),
    )
}

/// The texture for one map-space tile of `map`: the GPU applies the map's
/// Offset / Tiling / Angle as a UV transform and repeats it (a decal axis is
/// clamped by the caller). Colour components' own maps are evaluated over the
/// first tile. None when the map is a Bitmap that cannot be loaded.
pub fn bake(map: &Map, images: &Images) -> Option<Image> {
    if map.kind == MapType::Bitmap && images.get(&map.path).cloned().flatten().is_none() {
        return None;
    }
    let bitmap_size = |m: &Map| images.get(&m.path).cloned().flatten().map(|i| (i.width, i.height));
    let mut size = match map.kind {
        MapType::Bitmap => bitmap_size(map).unwrap_or((256, 256)),
        _ => {
            let mut s = (256u32, 256u32);
            for sub in map.components() {
                if let Some((w, h)) = bitmap_size(sub) {
                    s = (s.0.max(w), s.1.max(h));
                }
            }
            s
        }
    };
    if map.kind == MapType::Bitmap && map.crop.apply && map.crop.place {
        // A placed image keeps its pixels: the tile grows by the scale-down.
        size = ((size.0 as f32 / map.crop.w.max(0.05)) as u32, (size.1 as f32 / map.crop.h.max(0.05)) as u32);
    }
    let (w, h) = (size.0.clamp(1, 2048), size.1.clamp(1, 2048));
    let inv = invert(map.coords.matrix());
    // One baked texel's extent in UV.
    let footprint = [inv[0].abs() / w as f32 + inv[1].abs() / h as f32, inv[3].abs() / w as f32 + inv[4].abs() / h as f32];
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let m = [(x as f32 + 0.5) / w as f32, 1.0 - (y as f32 + 0.5) / h as f32];
            let uv = [inv[0] * m[0] + inv[1] * m[1] + inv[2], inv[3] * m[0] + inv[4] * m[1] + inv[5]];
            let ctx = Ctx { footprint, ..Ctx::from_uv(uv) };
            let c = eval_at(map, m, &ctx, images);
            rgba.extend(c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
        }
    }
    Some(Image::new(w, h, rgba))
}

/// The inverse of a 2×3 affine [a, b, c, d, e, f].
fn invert(m: [f32; 6]) -> [f32; 6] {
    let det = m[0] * m[4] - m[1] * m[3];
    if det.abs() < 1e-12 {
        return [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    }
    let (a, b, d, e) = (m[4] / det, -m[1] / det, -m[3] / det, m[0] / det);
    [a, b, -(a * m[2] + b * m[5]), d, e, -(d * m[2] + e * m[5])]
}

/// A key that is equal for maps that bake to the same texture (the GPU-side
/// Offset / Tiling / Angle and the Amount do not change the pixels).
pub fn texture_key(map: &Map) -> String {
    let mut m = map.clone();
    m.amount = 100.0;
    m.enabled = true;
    m.name.clear();
    m.coords.offset = [0.0; 2];
    m.coords.tiling = [1.0; 2];
    m.coords.angle = 0.0;
    serde_json::to_string(&m).unwrap_or_default()
}

/// A tangent-space normal map from a bump (height) image: the grey level is
/// the height; `strength` is the Bump Amount / 100.
pub fn bump_to_normal(img: &Image, strength: f32) -> Image {
    let (w, h) = (img.width as i64, img.height as i64);
    let height = |x: i64, y: i64| {
        let i = ((y.rem_euclid(h) * w + x.rem_euclid(w)) * 4) as usize;
        (img.rgba[i] as f32 + img.rgba[i + 1] as f32 + img.rgba[i + 2] as f32) / (3.0 * 255.0)
    };
    let mut rgba = Vec::with_capacity(img.rgba.len());
    let k = strength * 4.0;
    for y in 0..h {
        for x in 0..w {
            let dx = (height(x + 1, y) - height(x - 1, y)) * k;
            // Rows run down; +V (the map's up) is -y.
            let dy = (height(x, y - 1) - height(x, y + 1)) * k;
            let n = Vec3::new(-dx, -dy, 1.0).normalize();
            rgba.extend([n.x, n.y, n.z].map(|c| ((c * 0.5 + 0.5) * 255.0).round() as u8));
            rgba.push(255);
        }
    }
    Image::new(img.width, img.height, rgba)
}

/// Regular (one octave), Fractal or Turbulence (absolute octaves) noise at
/// `q`, 0–1: the Noise map's and the Gradient noise group's.
fn fractal_noise(q: Vec3, noise_type: u32, levels: f32) -> f32 {
    let levels = if noise_type == 0 { 1 } else { levels.max(1.0).round() as u32 };
    let (mut n, mut amp, mut norm) = (0.0, 1.0, 0.0);
    for l in 0..levels {
        let f = (2u32.pow(l)) as f32;
        let x = perlin(q * f);
        n += amp * if noise_type == 2 { x.abs() } else { x };
        norm += amp;
        amp *= 0.5;
    }
    let n = n / norm;
    if noise_type == 2 { n } else { n * 0.5 + 0.5 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::{Map, MapType};

    fn quad_image() -> Arc<Image> {
        // 4 × 4 in 2 × 2 blocks: red, green / blue, white (rows from the top).
        let block = |x: usize, y: usize| match (x / 2, y / 2) {
            (0, 0) => [255, 0, 0, 255],
            (1, 0) => [0, 255, 0, 255],
            (0, 1) => [0, 0, 255, 255],
            _ => [255, 255, 255, 255],
        };
        Arc::new(Image::new(4, 4, (0..16).flat_map(|i| block(i % 4, i / 4)).collect()))
    }

    fn images_for(map: &Map) -> Images {
        let mut images = Images::new();
        load_images(map, &mut images, &mut |_| Some(quad_image()));
        images
    }

    fn near(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.02)
    }

    #[test]
    fn bitmap_crop_and_place() {
        let mut m = Map::bitmap("/x/q.png", 100.0);
        let im = images_for(&m);
        // V up: the top-left texel is at u 0.25, v 0.75.
        assert!(near(eval(&m, [0.25, 0.75], &im), [1.0, 0.0, 0.0, 1.0]));
        assert!(near(eval(&m, [0.75, 0.25], &im), [1.0, 1.0, 1.0, 1.0]));
        // Crop to the image's right half: the left of the map shows green.
        m.crop = crate::material::Cropping { apply: true, place: false, u: 0.5, v: 0.0, w: 0.5, h: 1.0 };
        assert!(near(eval(&m, [0.3, 0.75], &im), [0.0, 1.0, 0.0, 1.0]));
        // Place into the top-left quarter: elsewhere the map is clear.
        m.crop = crate::material::Cropping { apply: true, place: true, u: 0.0, v: 0.0, w: 0.5, h: 0.5 };
        assert_eq!(eval(&m, [0.75, 0.25], &im)[3], 0.0);
        assert!(near(eval(&m, [0.1, 0.9], &im), [1.0, 0.0, 0.0, 1.0]));
        let baked = bake(&m, &im).unwrap();
        assert_eq!((baked.width, baked.height), (8, 8), "a placed image keeps its pixel size");
        assert_eq!(baked.rgba[3], 255, "top left is the image");
        assert_eq!(baked.rgba[(8 * 8 - 1) * 4 + 3], 0, "bottom right is clear");
    }

    #[test]
    fn bitmap_alpha_source_and_rgb_channel_output() {
        let mut m = Map::bitmap("/x/q.png", 100.0);
        let im = images_for(&m);
        // The top-left texel is opaque red.
        m.set_parameter("Alpha Source", 1.0).unwrap();
        assert!(near(eval(&m, [0.25, 0.75], &im), [1.0, 0.0, 0.0, 1.0 / 3.0]), "RGB Intensity: the grey is the alpha");
        m.set_parameter("Alpha Source", 2.0).unwrap();
        m.set_parameter("Alpha as Gray", 1.0).unwrap();
        assert!(near(eval(&m, [0.25, 0.75], &im), [1.0, 1.0, 1.0, 1.0]), "Alpha as Gray shows the image's alpha");
        assert!(!is_plain_bitmap(&m), "the GPU gets the changed pixels");
        let baked = bake(&m, &im).unwrap();
        assert_eq!(&baked.rgba[..4], &[255, 255, 255, 255]);
    }

    /// A 64 × 64 checkerboard of black and white pixels (opaque).
    fn pixel_checker() -> Arc<Image> {
        let rgba = (0..64 * 64).flat_map(|i| { let v = if (i % 64 + i / 64) % 2 == 0 { 0 } else { 255 }; [v, v, v, 255] }).collect();
        Arc::new(Image::new(64, 64, rgba))
    }

    #[test]
    fn bitmap_filtering_averages_over_the_footprint() {
        let mut m = Map::bitmap("/x/c.png", 100.0);
        let mut im = Images::new();
        load_images(&m, &mut im, &mut |_| Some(pixel_checker()));
        let at = |m: &Map, f: f32| eval_ctx(m, &Ctx { footprint: [f; 2], ..Ctx::from_uv([0.3, 0.6]) }, &im)[0];
        // A point sample reads one pixel (bilinear at its centre).
        let point = at(&m, 0.0);
        // Pyramidal: 8 pixels across reads the level that averages them.
        assert!((at(&m, 8.0 / 64.0) - 0.5).abs() < 0.02, "Pyramidal averages the checks: {}", at(&m, 8.0 / 64.0));
        m.set_parameter("Filtering", 1.0).unwrap();
        assert_eq!(m.channels.filtering, Filtering::SummedArea);
        assert!((at(&m, 6.0 / 64.0) - 0.5).abs() < 0.02, "Summed Area: the exact average of the area");
        assert!(is_plain_bitmap(&m), "the GPU filters the image itself");
        m.set_parameter("Filtering", 2.0).unwrap();
        let none = at(&m, 8.0 / 64.0);
        assert!(none == 0.0 || none == 1.0, "None: the nearest pixel ({none}, point {point})");
        // Blur widens the footprint: a pixel's footprint at Blur 8 averages too.
        m.set_parameter("Filtering", 0.0).unwrap();
        m.set_parameter("Blur", 8.0).unwrap();
        assert!((at(&m, 1.0 / 64.0) - 0.5).abs() < 0.02);
    }

    #[test]
    fn summed_area_wraps_across_the_tile_edge() {
        let img = pixel_checker();
        let c = img.sample_filtered(0.0, 0.0, [4.0, 4.0], Filtering::SummedArea);
        assert!(near(c, [0.5, 0.5, 0.5, 1.0]), "{c:?}");
    }

    #[test]
    fn mono_channel_output_premultiplied_alpha_and_jitter_placement() {
        let mut m = Map::bitmap("/x/q.png", 100.0);
        let im = images_for(&m);
        let ctx = Ctx::from_uv([0.25, 0.75]);
        // The top-left texel is opaque red: intensity 1/3, alpha 1.
        assert!((eval_mono(&m, &ctx, &im) - 1.0 / 3.0).abs() < 0.02, "RGB Intensity by default");
        m.set_parameter("Mono Channel Output", 1.0).unwrap();
        assert!((eval_mono(&m, &ctx, &im) - 1.0).abs() < 0.02, "Alpha");
        assert!(is_plain_bitmap(&m), "the GPU converters read the alpha themselves");
        // Premultiplied Alpha off: the colour counts as far as the alpha covers.
        let half = Arc::new(Image::new(1, 1, vec![200, 100, 0, 128]));
        let mut p = Map::bitmap("/x/h.png", 100.0);
        let mut hi = Images::new();
        load_images(&p, &mut hi, &mut |_| Some(half.clone()));
        let on = eval(&p, [0.5, 0.5], &hi);
        p.set_parameter("Premultiplied Alpha", 0.0).unwrap();
        let off = eval(&p, [0.5, 0.5], &hi);
        assert!((on[0] - 200.0 / 255.0).abs() < 0.01 && (off[0] - on[0] * 128.0 / 255.0).abs() < 0.01 && off[3] == on[3], "{on:?} {off:?}");
        assert!(!is_plain_bitmap(&p));
        // Jitter Placement: with Place, each tile its own size and place.
        let mut j = Map::bitmap("/x/q.png", 100.0);
        j.set_parameter("Tiling U", 4.0).unwrap();
        j.set_parameter("Tiling V", 4.0).unwrap();
        j.crop = crate::material::Cropping { apply: true, place: true, u: 0.0, v: 0.0, w: 1.0, h: 1.0 };
        let covered = |m: &Map, tile: usize| (0..64).filter(|i| eval(m, [(tile as f32 + (i % 8) as f32 / 8.0 + 0.06) / 4.0, ((i / 8) as f32 / 8.0 + 0.06) / 4.0], &im)[3] > 0.5).count();
        assert_eq!(covered(&j, 0), 64, "placed over the whole tile");
        j.set_parameter("Jitter Placement", 1.0).unwrap();
        let tiles: Vec<usize> = (0..4).map(|t| covered(&j, t)).collect();
        assert!(tiles.iter().any(|&n| n < 64) && tiles.windows(2).any(|w| w[0] != w[1]), "random sizes per tile: {tiles:?}");
        assert_eq!(tiles, (0..4).map(|t| covered(&j, t)).collect::<Vec<_>>(), "the same each time");
        j.set_parameter("Jitter Placement Amount", 0.0).unwrap();
        assert_eq!(covered(&j, 1), 64, "amount 0: no random offset");
    }

    #[test]
    fn decal_and_mirror() {
        let mut m = Map::bitmap("/x/q.png", 100.0);
        let im = images_for(&m);
        m.set_parameter("Tile U", 0.0).unwrap();
        m.set_parameter("Offset U", 0.5).unwrap();
        assert_eq!(eval(&m, [0.25, 0.5], &im)[3], 0.0, "the decal moved right: its left is uncovered");
        let mut m = Map::bitmap("/x/q.png", 100.0);
        m.set_parameter("Mirror U", 1.0).unwrap();
        // Tiling 1 with Mirror: the second half is the image flipped.
        let a = eval(&m, [0.1, 0.75], &im);
        let b = eval(&m, [0.9, 0.75], &im);
        assert!(near(a, b), "{a:?} {b:?}");
    }

    #[test]
    fn checker_gradient_noise_and_mix() {
        let checker = Map::new(MapType::Checker, 100.0);
        let im = Images::new();
        assert_eq!(eval(&checker, [0.25, 0.25], &im), [0.0, 0.0, 0.0, 1.0], "Color #1 black at the bottom left");
        assert_eq!(eval(&checker, [0.75, 0.25], &im), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(eval(&checker, [0.75, 0.75], &im), [0.0, 0.0, 0.0, 1.0]);
        let mut soft = checker.clone();
        soft.set_parameter("Soften", 1.0).unwrap();
        let edge = eval(&soft, [0.5, 0.25], &im)[0];
        assert!(edge > 0.2 && edge < 0.8, "softened edge is grey: {edge}");

        let g = Map::new(MapType::Gradient, 100.0);
        assert!(eval(&g, [0.5, 0.01], &im)[0] > 0.95, "Color #3 white at the bottom");
        assert!(eval(&g, [0.5, 0.99], &im)[0] < 0.05, "Color #1 black at the top");
        assert!((eval(&g, [0.5, 0.5], &im)[0] - 0.5).abs() < 0.02);

        let n = Map::new(MapType::Noise, 100.0);
        let vals: Vec<f32> = (0..64).map(|i| eval(&n, [i as f32 / 64.0, 0.37], &im)[0]).collect();
        let (lo, hi) = vals.iter().fold((1.0f32, 0.0f32), |(l, h), v| (l.min(*v), h.max(*v)));
        assert!(hi - lo > 0.3, "noise varies: {lo}..{hi}");

        let mut mix = Map::new(MapType::Mix, 100.0);
        mix.set_parameter("Mix Amount", 25.0).unwrap();
        assert!((eval(&mix, [0.5, 0.5], &im)[0] - 0.25).abs() < 1e-4);
        mix.sub_maps[2] = Some(Box::new(checker));
        assert_eq!(eval(&mix, [0.75, 0.25], &im)[0], 1.0, "white in the mask shows Color #2");
    }

    #[test]
    fn bump_heights_tilt_the_normals() {
        // A ramp rising to the right: normals lean left (-X).
        let mut rgba = Vec::new();
        for _y in 0..4 {
            for x in 0..4u8 {
                rgba.extend([x * 60, x * 60, x * 60, 255]);
            }
        }
        let n = bump_to_normal(&Image::new(4, 4, rgba), 1.0);
        let px = &n.rgba[(4 + 1) * 4..(4 + 1) * 4 + 3];
        assert!(px[0] < 128 && (px[1] as i32 - 128).abs() <= 1 && px[2] > 128, "{px:?}");
    }
}

