//! Rendering effects applied to a finished render, in the effects list's
//! order, as the reference applies them after rendering: Lens Effects Glow,
//! Blur, Brightness and Contrast, Color Balance and Film Grain. The image is
//! sRGB RGBA; background pixels are the ones with alpha 0 ("Ignore
//! Background", "Non-Background").

use crate::environment::{Blur, ImageEffect};
use image::RgbaImage;
use loom_scene::environment::BlurType;

/// Applies `effects` to `img` in order.
pub fn apply(img: &mut RgbaImage, effects: &[ImageEffect]) {
    for fx in effects {
        match fx {
            ImageEffect::Glow { size, intensity, use_source_color, inner, outer, bright } => glow(img, *size, *intensity, *use_source_color, *inner, *outer, *bright),
            ImageEffect::Blur(b) => blur(img, b),
            ImageEffect::BrightnessContrast { brightness, contrast, ignore_background } => per_pixel(img, *ignore_background, |c| brightness_contrast(c, *brightness, *contrast)),
            ImageEffect::ColorBalance { cyan_red, magenta_green, yellow_blue, preserve_luminosity, ignore_background } => {
                per_pixel(img, *ignore_background, |c| color_balance(c, [*cyan_red, *magenta_green, *yellow_blue], *preserve_luminosity))
            }
            ImageEffect::FilmGrain { grain, ignore_background } => film_grain(img, *grain, *ignore_background),
        }
    }
}

fn to_f(p: [u8; 4]) -> [f32; 3] {
    [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]
}

fn to_u8(c: f32) -> u8 {
    (c.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn luma(c: [f32; 3]) -> f32 {
    0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
}

fn per_pixel(img: &mut RgbaImage, ignore_background: bool, f: impl Fn([f32; 3]) -> [f32; 3]) {
    for p in img.pixels_mut() {
        if ignore_background && p.0[3] == 0 {
            continue;
        }
        let c = f(to_f(p.0));
        p.0 = [to_u8(c[0]), to_u8(c[1]), to_u8(c[2]), p.0[3]];
    }
}

/// Brightness shifts every component (0.5 neutral); Contrast stretches
/// around mid grey (0.5 neutral, 0 flat grey, 1 a hard threshold).
pub fn brightness_contrast(c: [f32; 3], brightness: f32, contrast: f32) -> [f32; 3] {
    let k = if contrast >= 1.0 { 1e4 } else { contrast / (1.0 - contrast) };
    c.map(|v| (v - 0.5) * k + 0.5 + (brightness - 0.5))
}

/// Each slider (−100 to 100) moves its channel by up to half the range;
/// Preserve Luminosity scales the result back to the original's luma.
pub fn color_balance(c: [f32; 3], shift: [f32; 3], preserve_luminosity: bool) -> [f32; 3] {
    let mut o = [c[0] + shift[0] / 200.0, c[1] + shift[1] / 200.0, c[2] + shift[2] / 200.0].map(|v| v.clamp(0.0, 1.0));
    if preserve_luminosity {
        let (l0, l1) = (luma(c), luma(o));
        if l1 > 1e-5 {
            o = o.map(|v| v * l0 / l1);
        }
    }
    o
}

/// Deterministic per-pixel noise in −1..1.
fn noise(x: u32, y: u32) -> f32 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77) ^ 0x2545_F491;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h as f32 / u32::MAX as f32) * 2.0 - 1.0
}

/// Film Grain: grey noise of up to `grain` / 10 of the range (0–10).
fn film_grain(img: &mut RgbaImage, grain: f32, ignore_background: bool) {
    let amp = grain.max(0.0) / 10.0;
    for (x, y, p) in img.enumerate_pixels_mut() {
        if ignore_background && p.0[3] == 0 {
            continue;
        }
        let n = noise(x, y) * amp;
        let c = to_f(p.0);
        p.0 = [to_u8(c[0] + n), to_u8(c[1] + n), to_u8(c[2] + n), p.0[3]];
    }
}

/// A float RGBA copy (0–1).
fn float_image(img: &RgbaImage) -> Vec<[f32; 4]> {
    img.pixels().map(|p| [p.0[0] as f32 / 255.0, p.0[1] as f32 / 255.0, p.0[2] as f32 / 255.0, p.0[3] as f32 / 255.0]).collect()
}

/// One pass of a box blur of radius `r` along x (`horizontal`) or y.
fn box_pass(src: &[[f32; 4]], w: usize, h: usize, r: usize, horizontal: bool) -> Vec<[f32; 4]> {
    let mut out = vec![[0.0; 4]; src.len()];
    let (n, lines) = if horizontal { (w, h) } else { (h, w) };
    let at = |line: usize, i: usize| if horizontal { line * w + i } else { i * w + line };
    let norm = 1.0 / (2 * r + 1) as f32;
    for line in 0..lines {
        let mut sum = [0.0f32; 4];
        let clamp = |i: isize| i.clamp(0, n as isize - 1) as usize;
        for i in -(r as isize)..=(r as isize) {
            let s = src[at(line, clamp(i))];
            for k in 0..4 {
                sum[k] += s[k];
            }
        }
        for i in 0..n {
            out[at(line, i)] = sum.map(|v| v * norm);
            let (add, sub) = (src[at(line, clamp(i as isize + r as isize + 1))], src[at(line, clamp(i as isize - r as isize))]);
            for k in 0..4 {
                sum[k] += add[k] - sub[k];
            }
        }
    }
    out
}

/// Gaussian-like blur (three box passes each way) of standard deviation `sigma`.
fn gaussian(src: &[[f32; 4]], w: usize, h: usize, sigma: f32) -> Vec<[f32; 4]> {
    if sigma < 0.3 {
        return src.to_vec();
    }
    // Three boxes of radius r have variance 3 · ((2r+1)² − 1) / 12.
    let r = (((4.0 * sigma * sigma + 1.0).sqrt() - 1.0) / 2.0).round().max(1.0) as usize;
    let mut v = src.to_vec();
    for _ in 0..3 {
        v = box_pass(&v, w, h, r, true);
        v = box_pass(&v, w, h, r, false);
    }
    v
}

/// Lens Effects Glow (Image source Whole, Bright filter): pixels brighter
/// than `bright` glow in the radial colours (inner near them, outer in the
/// halo) mixed with their own colour by Use Source Color; Size is a
/// percentage of the image diagonal, Intensity scales the glow.
fn glow(img: &mut RgbaImage, size: f32, intensity: f32, use_source: f32, inner: [f32; 3], outer: [f32; 3], bright: f32) {
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w == 0 || h == 0 {
        return;
    }
    let radius = size.max(0.0) / 100.0 * ((w * w + h * h) as f32).sqrt();
    let threshold = bright / 255.0;
    let mix = (use_source / 100.0).clamp(0.0, 1.0);
    let src = float_image(img);
    let source = |inner_or_outer: [f32; 3]| -> Vec<[f32; 4]> {
        src.iter()
            .map(|p| {
                let c = [p[0], p[1], p[2]];
                if p[3] == 0.0 || c.iter().cloned().fold(0.0, f32::max) <= threshold {
                    return [0.0; 4];
                }
                let g = [0, 1, 2].map(|k| inner_or_outer[k] * (1.0 - mix) + c[k] * mix);
                [g[0], g[1], g[2], 1.0]
            })
            .collect()
    };
    let near = gaussian(&source(inner), w, h, radius * 0.25);
    let far = gaussian(&source(outer), w, h, radius * 0.5);
    let k = intensity.max(0.0) / 100.0;
    for (i, p) in img.pixels_mut().enumerate() {
        let c = to_f(p.0);
        // The outer colour where the wide halo reaches past the narrow one.
        let halo = (far[i][3] - near[i][3]).max(0.0);
        let g = [0, 1, 2].map(|j| near[i][j] + far[i][j] * halo / far[i][3].max(1e-6));
        p.0 = [to_u8(c[0] + g[0] * k), to_u8(c[1] + g[1] * k), to_u8(c[2] + g[2] * k), p.0[3]];
    }
}

/// Samples `src` at (x, y) with bilinear filtering, clamped to the edges.
fn sample(src: &[[f32; 4]], w: usize, h: usize, x: f32, y: f32) -> [f32; 4] {
    let x = x.clamp(0.0, (w - 1) as f32);
    let y = y.clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let p = |x: usize, y: usize| src[y * w + x];
    let (a, b, c, d) = (p(x0, y0), p(x1, y0), p(x0, y1), p(x1, y1));
    [0, 1, 2, 3].map(|k| (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy)
}

/// A blur along direction `dir` (unit, pixels) of half-length `r`, the
/// trail (−100 to 100) weighting the positive side more (or less).
fn line_blur(src: &[[f32; 4]], w: usize, h: usize, x: usize, y: usize, dir: [f32; 2], r: f32, trail: f32) -> [f32; 4] {
    if r < 0.5 {
        return src[y * w + x];
    }
    let taps = (r.ceil() as i32).min(64);
    let t = (trail / 100.0).clamp(-1.0, 1.0);
    let mut sum = [0.0f32; 4];
    let mut weight = 0.0;
    for k in -taps..=taps {
        let off = k as f32 * r / taps as f32;
        let side = if off > 0.0 { 1.0 + t } else if off < 0.0 { 1.0 - t } else { 1.0 };
        let wgt = (-0.5 * (off / (r * 0.5)).powi(2)).exp() * side;
        let s = sample(src, w, h, x as f32 + dir[0] * off, y as f32 + dir[1] * off);
        for i in 0..4 {
            sum[i] += s[i] * wgt;
        }
        weight += wgt;
    }
    sum.map(|v| v / weight.max(1e-6))
}

fn blur(img: &mut RgbaImage, b: &Blur) {
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w == 0 || h == 0 {
        return;
    }
    let src = float_image(img);
    let blurred: Vec<[f32; 4]> = match b.blur_type {
        BlurType::Uniform => gaussian(&src, w, h, b.radius.max(0.0) * 0.5),
        BlurType::Directional => {
            let a = b.rotation.to_radians();
            let (u, v) = ([a.cos(), a.sin()], [-a.sin(), a.cos()]);
            let pass1: Vec<[f32; 4]> = (0..w * h).map(|i| line_blur(&src, w, h, i % w, i / w, u, b.u_radius, b.u_trail)).collect();
            (0..w * h).map(|i| line_blur(&pass1, w, h, i % w, i / w, v, b.v_radius, b.v_trail)).collect()
        }
        BlurType::Radial => {
            let [ox, oy] = b.origin;
            let far = [(0.0f32, 0.0f32), (w as f32, 0.0), (0.0, h as f32), (w as f32, h as f32)].iter().map(|(x, y)| ((x - ox).powi(2) + (y - oy).powi(2)).sqrt()).fold(1.0, f32::max);
            (0..w * h)
                .map(|i| {
                    let (x, y) = (i % w, i / w);
                    let (dx, dy) = (x as f32 - ox, y as f32 - oy);
                    let d = (dx * dx + dy * dy).sqrt();
                    if d < 1e-3 {
                        return src[i];
                    }
                    // The least blur at the origin, the full radius at the farthest corner.
                    line_blur(&src, w, h, x, y, [dx / d, dy / d], b.radius * d / far, -b.radial_trail)
                })
                .collect()
        }
    };
    let blend = (b.blend / 100.0).clamp(0.0, 1.0);
    let brighten = 1.0 + b.brighten.max(0.0) / 100.0;
    for (i, p) in img.pixels_mut().enumerate() {
        if b.non_background && p.0[3] == 0 {
            continue;
        }
        let o = src[i];
        let mut c = [0, 1, 2, 3].map(|k| blurred[i][k] * (1.0 - blend) + o[k] * blend);
        for v in c.iter_mut().take(3) {
            *v *= brighten;
        }
        let a = if b.affect_alpha { to_u8(c[3]) } else { p.0[3] };
        p.0 = [to_u8(c[0]), to_u8(c[1]), to_u8(c[2]), a];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(f: impl Fn(u32, u32) -> [u8; 4]) -> RgbaImage {
        RgbaImage::from_fn(32, 32, |x, y| image::Rgba(f(x, y)))
    }

    #[test]
    fn neutral_brightness_and_contrast_leave_the_image() {
        let c = [0.2, 0.5, 0.9];
        let o = brightness_contrast(c, 0.5, 0.5);
        assert!(o.iter().zip(c).all(|(a, b)| (a - b).abs() < 1e-5));
        let o = brightness_contrast(c, 0.7, 0.5);
        assert!((o[0] - 0.4).abs() < 1e-5, "brightness adds: {o:?}");
        let flat = brightness_contrast(c, 0.5, 0.0);
        assert!(flat.iter().all(|v| (v - 0.5).abs() < 1e-5), "no contrast is flat grey");
    }

    #[test]
    fn color_balance_moves_channels_and_can_keep_luminosity() {
        let c = [0.4, 0.4, 0.4];
        let o = color_balance(c, [100.0, 0.0, -100.0], false);
        assert!((o[0] - 0.9).abs() < 1e-5 && (o[2] - 0.0).abs() < 1e-5);
        let p = color_balance(c, [60.0, 0.0, 0.0], true);
        assert!((luma(p) - luma(c)).abs() < 1e-4 && p[0] > p[1], "{p:?}");
    }

    #[test]
    fn ignore_background_leaves_alpha_zero_pixels() {
        let mut i = img(|x, _| if x < 16 { [100, 100, 100, 0] } else { [100, 100, 100, 255] });
        apply(&mut i, &[ImageEffect::BrightnessContrast { brightness: 0.8, contrast: 0.5, ignore_background: true }]);
        assert_eq!(i.get_pixel(2, 2).0, [100, 100, 100, 0]);
        assert!(i.get_pixel(20, 2).0[0] > 150);
        let mut g = img(|_, _| [128, 128, 128, 255]);
        apply(&mut g, &[ImageEffect::FilmGrain { grain: 2.0, ignore_background: false }]);
        let spread = g.pixels().map(|p| p.0[0]).max().unwrap() - g.pixels().map(|p| p.0[0]).min().unwrap();
        assert!(spread > 20, "grain adds noise: {spread}");
    }

    #[test]
    fn uniform_blur_softens_an_edge_and_non_background_keeps_the_background() {
        let mut i = img(|x, _| if x < 16 { [0, 0, 0, 255] } else { [255, 255, 255, 255] });
        let b = Blur { blur_type: BlurType::Uniform, radius: 6.0, u_radius: 0.0, v_radius: 0.0, u_trail: 0.0, v_trail: 0.0, rotation: 0.0, radial_trail: 0.0, origin: [0.0; 2], affect_alpha: false, non_background: false, brighten: 0.0, blend: 0.0 };
        apply(&mut i, &[ImageEffect::Blur(b)]);
        let edge = i.get_pixel(16, 10).0[0];
        assert!(edge > 60 && edge < 200, "the edge is soft: {edge}");
        let mut j = img(|x, _| if x < 16 { [0, 0, 0, 0] } else { [255, 255, 255, 255] });
        apply(&mut j, &[ImageEffect::Blur(Blur { non_background: true, ..b })]);
        assert_eq!(j.get_pixel(14, 10).0, [0, 0, 0, 0], "Non-Background leaves the background alone");
        assert!(j.get_pixel(16, 10).0[0] < 250, "the object's edge blurs");
    }

    #[test]
    fn directional_blur_follows_its_rotation() {
        // A vertical white line: a horizontal (0°) blur spreads it, a 90° one does not.
        let line = || img(|x, _| if x == 16 { [255, 255, 255, 255] } else { [0, 0, 0, 255] });
        let b = Blur { blur_type: BlurType::Directional, radius: 0.0, u_radius: 6.0, v_radius: 0.0, u_trail: 0.0, v_trail: 0.0, rotation: 0.0, radial_trail: 0.0, origin: [0.0; 2], affect_alpha: false, non_background: false, brighten: 0.0, blend: 0.0 };
        let mut h = line();
        apply(&mut h, &[ImageEffect::Blur(b)]);
        assert!(h.get_pixel(18, 10).0[0] > 10, "spread sideways");
        let mut v = line();
        apply(&mut v, &[ImageEffect::Blur(Blur { rotation: 90.0, ..b })]);
        assert!(v.get_pixel(18, 10).0[0] < 3 && v.get_pixel(16, 10).0[0] > 200, "a blur along the line keeps it");
    }

    #[test]
    fn glow_spreads_light_around_bright_pixels_only() {
        let mut i = img(|x, y| if (14..18).contains(&x) && (14..18).contains(&y) { [255, 255, 255, 255] } else { [40, 40, 40, 255] });
        apply(&mut i, &[ImageEffect::Glow { size: 10.0, intensity: 100.0, use_source_color: 0.0, inner: [1.0, 0.0, 0.0], outer: [1.0, 0.0, 0.0], bright: 200.0 }]);
        let near = i.get_pixel(20, 16).0;
        assert!(near[0] > 60 && near[1] < 50, "a red glow beside the bright square: {near:?}");
        assert_eq!(i.get_pixel(0, 0).0, [40, 40, 40, 255], "far away nothing changes");
        let mut dim = img(|_, _| [150, 150, 150, 255]);
        apply(&mut dim, &[ImageEffect::Glow { size: 10.0, intensity: 100.0, use_source_color: 0.0, inner: [1.0, 0.0, 0.0], outer: [1.0, 0.0, 0.0], bright: 200.0 }]);
        assert_eq!(dim.get_pixel(16, 16).0, [150, 150, 150, 255], "nothing is above Bright");
    }
}
