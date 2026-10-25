//! Evaluating maps to pixels: Bitmap (with Cropping/Placement, Mirror and
//! Tile), Checker, Gradient, Noise and Mix. The viewport, the Realistic
//! renderer and the Material Editor's sample slots all bake maps here, so
//! they show the same thing. Colours are 0–1 sRGB values as the editor's
//! swatches hold them; bitmaps are 8-bit sRGB RGBA as image files store them.

use crate::material::{Map, MapType};
use glam::Vec3;
use std::collections::HashMap;
use std::sync::Arc;

/// An RGBA8 image, rows from the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    /// Bilinear, repeat-wrapped sample at image coordinates `x`, `y` in 0–1 (y down), 0–1 RGBA.
    pub fn sample(&self, x: f32, y: f32) -> [f32; 4] {
        if self.width == 0 || self.height == 0 {
            return [0.0; 4];
        }
        let (w, h) = (self.width as i64, self.height as i64);
        let fx = x * w as f32 - 0.5;
        let fy = y * h as f32 - 0.5;
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let px = |xi: i64, yi: i64| {
            let i = ((yi.rem_euclid(h) * w + xi.rem_euclid(w)) * 4) as usize;
            [0, 1, 2, 3].map(|c| self.rgba[i + c] as f32 / 255.0)
        };
        let (x0, y0) = (x0 as i64, y0 as i64);
        let (a, b, c, d) = (px(x0, y0), px(x0 + 1, y0), px(x0, y0 + 1), px(x0 + 1, y0 + 1));
        [0, 1, 2, 3].map(|k| (a[k] * (1.0 - tx) + b[k] * tx) * (1.0 - ty) + (c[k] * (1.0 - tx) + d[k] * tx) * ty)
    }
}

/// Images by path, loaded once for a map tree.
pub type Images = HashMap<String, Option<Arc<Image>>>;

/// Loads every bitmap the map tree uses through `load`.
pub fn load_images(map: &Map, images: &mut Images, load: &mut dyn FnMut(&str) -> Option<Arc<Image>>) {
    if map.kind == MapType::Bitmap && !map.path.is_empty() && !images.contains_key(&map.path) {
        let img = load(&map.path);
        images.insert(map.path.clone(), img);
    }
    for sub in map.sub_maps.iter().flatten() {
        load_images(sub, images, load);
    }
}

/// The map's colour at surface UV `uv` (map channel coordinates), 0–1 RGBA;
/// alpha 0 where a decal or a placed bitmap leaves the surface uncovered.
pub fn eval(map: &Map, uv: [f32; 2], images: &Images) -> [f32; 4] {
    eval_at(map, map.coords.apply(uv), uv, images)
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

/// `m` in map space (after Offset/Tiling/Angle), `uv` the surface UV the
/// colour components' own maps read.
fn eval_at(map: &Map, m: [f32; 2], uv: [f32; 2], images: &Images) -> [f32; 4] {
    let c = &map.coords;
    let (Some(u), Some(v)) = (fold(m[0], c.mirror[0], c.tile[0]), fold(m[1], c.mirror[1], c.tile[1])) else { return [0.0; 4] };
    let p = &map.params;
    let component = |i: usize| -> Vec3 {
        let col = map.colors[i];
        match map.sub_maps[i].as_deref().filter(|s| s.enabled) {
            Some(s) => {
                let t = eval(s, uv, images);
                col.lerp(Vec3::new(t[0], t[1], t[2]), t[3])
            }
            None => col,
        }
    };
    let rgb = |c: Vec3| [c.x, c.y, c.z, 1.0];
    match map.kind {
        MapType::Bitmap => {
            let Some(img) = images.get(&map.path).cloned().flatten() else { return [0.0; 4] };
            let k = &map.crop;
            // Image coordinates from the top left.
            let (x, y) = (u, 1.0 - v);
            if !k.apply {
                return img.sample(x, y);
            }
            if k.place {
                let (px, py) = ((x - k.u) / k.w.max(1e-6), (y - k.v) / k.h.max(1e-6));
                if !(0.0..=1.0).contains(&px) || !(0.0..=1.0).contains(&py) {
                    return [0.0; 4];
                }
                img.sample(px, py)
            } else {
                img.sample(k.u + x * k.w, k.v + y * k.h)
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
            let pos = p.position.clamp(1e-4, 1.0 - 1e-4);
            let c = if t < pos { component(2).lerp(component(1), t / pos) } else { component(1).lerp(component(0), (t - pos) / (1.0 - pos)) };
            rgb(c)
        }
        MapType::Noise => {
            // The reference's noise is 3D in object space; here it runs over the
            // map's UV square, Size 25 giving four features across.
            let s = 100.0 / p.size.max(1e-3);
            let q = Vec3::new(u * s, v * s, p.phase);
            let levels = if p.noise_type == 0 { 1 } else { p.levels.max(1.0).round() as u32 };
            let (mut n, mut amp, mut norm) = (0.0, 1.0, 0.0);
            for l in 0..levels {
                let f = (2u32.pow(l)) as f32;
                let x = perlin(q * f);
                n += amp * if p.noise_type == 2 { x.abs() } else { x };
                norm += amp;
                amp *= 0.5;
            }
            let n = n / norm;
            let n = if p.noise_type == 2 { n } else { n * 0.5 + 0.5 };
            let t = ((n - p.low) / (p.high - p.low).max(1e-4)).clamp(0.0, 1.0);
            rgb(component(0).lerp(component(1), t))
        }
        MapType::Mix => {
            let t = match map.sub_maps[2].as_deref().filter(|s| s.enabled) {
                Some(mask) => {
                    let t = eval(mask, uv, images);
                    (t[0] + t[1] + t[2]) / 3.0
                }
                None => p.mix_amount / 100.0,
            };
            rgb(component(0).lerp(component(1), t.clamp(0.0, 1.0)))
        }
        MapType::NormalBump => rgb(component(0)),
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
    bake_with_limit(map, images, 2048)
}

/// Bake directly within the caller's pixel-size cap, including placed images
/// and procedural maps. A zero cap disables output without allocating pixels.
pub fn bake_with_limit(map: &Map, images: &Images, max_side: u32) -> Option<Image> {
    if max_side == 0 {
        return None;
    }
    if map.kind == MapType::Bitmap && images.get(&map.path).cloned().flatten().is_none() {
        return None;
    }
    let bitmap_size = |m: &Map| images.get(&m.path).cloned().flatten().map(|i| (i.width, i.height));
    let mut size = match map.kind {
        MapType::Bitmap => bitmap_size(map).unwrap_or((256, 256)),
        _ => {
            let mut s = (256u32, 256u32);
            for sub in map.sub_maps.iter().flatten() {
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
    let (w, h) = (size.0.clamp(1, max_side), size.1.clamp(1, max_side));
    let inv = invert(map.coords.matrix());
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let m = [(x as f32 + 0.5) / w as f32, 1.0 - (y as f32 + 0.5) / h as f32];
            let uv = [inv[0] * m[0] + inv[1] * m[1] + inv[2], inv[3] * m[0] + inv[4] * m[1] + inv[5]];
            let c = eval_at(map, m, uv, images);
            rgba.extend(c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
        }
    }
    Some(Image { width: w, height: h, rgba })
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
    Image { width: img.width, height: img.height, rgba }
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
        Arc::new(Image { width: 4, height: 4, rgba: (0..16).flat_map(|i| block(i % 4, i / 4)).collect() })
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
        let n = bump_to_normal(&Image { width: 4, height: 4, rgba }, 1.0);
        let px = &n.rgba[(4 + 1) * 4..(4 + 1) * 4 + 3];
        assert!(px[0] < 128 && (px[1] as i32 - 128).abs() <= 1 && px[2] > 128, "{px:?}");
    }
}
