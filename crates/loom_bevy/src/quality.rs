//! Offline render quality: passes that add up per bucket, each with its
//! samples moved within their pixel (antialiasing through the Render Setup
//! filter) and the lights moved over their Source Radius (area shadows),
//! and the sky dome a render's ambient light comes from.

use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageSampler, ImageSamplerDescriptor};
use bevy::render::render_resource::{Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureViewDescriptor, TextureViewDimension};
use loom_scene::render::AaFilter;

use crate::bake::{linear_to_srgb, srgb_to_linear};

/// Radical inverse of `i` in base `b`: a low-discrepancy sequence in 0–1.
pub fn halton(mut i: u32, b: u32) -> f32 {
    let (mut f, mut r) = (1.0, 0.0);
    while i > 0 {
        f /= b as f32;
        r += f * (i % b) as f32;
        i /= b;
    }
    r
}

/// Where pass `pass` of `passes` puts its samples within their cell,
/// -0.5–0.5 on each axis (the centre when there is one pass).
pub fn jitter(pass: u32, passes: u32) -> [f32; 2] {
    if passes <= 1 {
        return [0.0; 2];
    }
    [halton(pass + 1, 2) - 0.5, halton(pass + 1, 3) - 0.5]
}

/// Point `pass` of `passes` spread evenly over the unit disk (golden angle spiral).
pub fn disk(pass: u32, passes: u32) -> [f32; 2] {
    if passes <= 1 {
        return [0.0; 2];
    }
    let r = ((pass as f32 + 0.5) / passes as f32).sqrt();
    let a = pass as f32 * 2.399_963;
    [r * a.cos(), r * a.sin()]
}

/// The passes of one bucket added up through the antialiasing filter.
pub struct Accum {
    /// The bucket (frame part, output size) the passes belong to.
    pub key: ([f32; 4], [u32; 2]),
    /// Output pixels rendered around the bucket on each side (the filter's
    /// reach and the screen-space effects' view past the bucket's edge).
    pub margin: u32,
    /// Samples per output pixel side in each pass.
    pub ss: u32,
    /// Passes added so far.
    pub pass: u32,
    /// Per output pixel: weighted linear RGB, alpha and the weight.
    sum: Vec<[f32; 5]>,
}

impl Accum {
    pub fn new(key: ([f32; 4], [u32; 2]), margin: u32, ss: u32) -> Self {
        let n = (key.1[0] * key.1[1]) as usize;
        Self { key, margin, ss: ss.max(1), pass: 0, sum: vec![[0.0; 5]; n] }
    }

    /// The size each pass renders at.
    pub fn internal_size(&self) -> [u32; 2] {
        let s = self.key.1;
        [(s[0] + 2 * self.margin) * self.ss, (s[1] + 2 * self.margin) * self.ss]
    }

    /// Filter taps of one axis: for each rendered sample, the output pixels
    /// it reaches and their weights. Sample `i` sits at `(i + 0.5 + j) / ss
    /// - margin` output pixels; the filter reaches `radius` pixels.
    fn taps(&self, n_in: u32, n_out: u32, j: f32, filter: AaFilter, radius: f32) -> Vec<Vec<(u32, f32)>> {
        (0..n_in)
            .map(|i| {
                let x = (i as f32 + 0.5 + j) / self.ss as f32 - self.margin as f32;
                let lo = (x - radius - 0.5).ceil().max(0.0) as i64;
                let hi = ((x + radius - 0.5).floor() as i64).min(n_out as i64 - 1);
                (lo..=hi)
                    .filter_map(|o| {
                        let w = filter.weight(((o as f32 + 0.5) - x) / radius);
                        (w != 0.0).then_some((o as u32, w))
                    })
                    .collect()
            })
            .collect()
    }

    /// Adds a pass rendered `jitter` (in samples) off the sample centres,
    /// weighted by `filter` `size` pixels wide.
    pub fn add(&mut self, img: &image::RgbaImage, jitter: [f32; 2], filter: AaFilter, size: f32) -> Result<(), String> {
        let [iw, ih] = self.internal_size();
        if img.dimensions() != (iw, ih) {
            return Err(format!("pass is {:?}, expected {iw}x{ih}", img.dimensions()));
        }
        let [ow, oh] = self.key.1;
        let radius = (size * 0.5).max(0.5);
        let cols = self.taps(iw, ow, jitter[0], filter, radius);
        let rows = self.taps(ih, oh, jitter[1], filter, radius);
        let lut: Vec<f32> = (0..=255u8).map(srgb_to_linear).collect();
        // Rows first: each rendered row into the output columns.
        let mut tmp = vec![[0.0f32; 5]; (ow * ih) as usize];
        for y in 0..ih {
            let line = &mut tmp[(y * ow) as usize..((y + 1) * ow) as usize];
            for (x, taps) in cols.iter().enumerate() {
                let p = img.get_pixel(x as u32, y).0;
                let c = [lut[p[0] as usize], lut[p[1] as usize], lut[p[2] as usize], p[3] as f32 / 255.0, 1.0];
                for &(o, w) in taps {
                    let t = &mut line[o as usize];
                    for k in 0..5 {
                        t[k] += w * c[k];
                    }
                }
            }
        }
        for (y, taps) in rows.iter().enumerate() {
            for &(o, w) in taps {
                for x in 0..ow as usize {
                    let t = tmp[y * ow as usize + x];
                    let s = &mut self.sum[o as usize * ow as usize + x];
                    for k in 0..5 {
                        s[k] += w * t[k];
                    }
                }
            }
        }
        self.pass += 1;
        Ok(())
    }

    /// The passes so far as an sRGB image of the bucket over `background`
    /// (sRGB; the passes are cleared to nothing, so their colour is premultiplied).
    pub fn image(&self, background: [f32; 3]) -> image::RgbaImage {
        let [w, h] = self.key.1;
        let bg = background.map(|c| srgb_to_linear((c.clamp(0.0, 1.0) * 255.0).round() as u8));
        image::RgbaImage::from_fn(w, h, |x, y| {
            let s = self.sum[(y * w + x) as usize];
            let n = if s[4] > 1e-6 { s[4] } else { 1.0 };
            let a = (s[3] / n).clamp(0.0, 1.0);
            let c = |k: usize| s[k] / n + bg[k] * (1.0 - a);
            image::Rgba([linear_to_srgb(c(0)), linear_to_srgb(c(1)), linear_to_srgb(c(2)), (a * 255.0).round() as u8])
        })
    }
}

/// The z (up) component of the direction through texel centre (`sc`, `tc`)
/// (-1–1) of cube face `face` (+X, -X, +Y, -Y, +Z, -Z, as wgpu lays them out).
fn face_z(face: usize, sc: f32, tc: f32) -> f32 {
    let d = match face {
        0 => [1.0, -tc, -sc],
        1 => [-1.0, -tc, sc],
        2 => [sc, 1.0, tc],
        3 => [sc, -1.0, -tc],
        4 => [sc, -tc, 1.0],
        _ => [-sc, -tc, -1.0],
    };
    d[2] / (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// A cube map of `size` px faces with `mips` levels; `value(level, z)` is
/// the brightness 0–1 of `color` towards a direction of up component z.
fn cube(size: u32, mips: u32, color: [f32; 3], value: impl Fn(u32, f32) -> f32) -> Image {
    let mut data = Vec::new();
    for face in 0..6 {
        for level in 0..mips {
            let n = (size >> level).max(1);
            for y in 0..n {
                for x in 0..n {
                    let (sc, tc) = (2.0 * (x as f32 + 0.5) / n as f32 - 1.0, 2.0 * (y as f32 + 0.5) / n as f32 - 1.0);
                    let v = value(level, face_z(face, sc, tc));
                    data.extend_from_slice(&[linear_to_srgb(color[0] * v), linear_to_srgb(color[1] * v), linear_to_srgb(color[2] * v), 255]);
                }
            }
        }
    }
    let mut image = Image { data: Some(data), asset_usage: RenderAssetUsages::RENDER_WORLD, ..Default::default() };
    image.texture_descriptor = TextureDescriptor {
        label: None,
        size: Extent3d { width: size, height: size, depth_or_array_layers: 6 },
        mip_level_count: mips,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8UnormSrgb,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    };
    image.texture_view_descriptor = Some(TextureViewDescriptor { dimension: Some(TextureViewDimension::Cube), ..Default::default() });
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { mipmap_filter: bevy::image::ImageFilterMode::Linear, ..ImageSamplerDescriptor::linear() });
    image
}

/// Specular map levels: roughness 0 (level 0) to 1 (the last level).
const SKY_MIPS: u32 = 7;

/// The sky dome as the (diffuse, specular) maps of an environment map
/// light: `color` (linear, its largest channel 1) above the horizon,
/// `ground` × it below. Scaled so a vertical wall receives `color`, as it
/// would from a uniform ambient light of that colour: the dome only moves
/// light from the undersides to the tops.
pub fn sky_maps(color: [f32; 3], ground: f32) -> (Image, Image) {
    let g = ground.clamp(0.0, 1.0);
    // A wall sees half sky, half ground: (1 + g) / 2 of the sky's brightness.
    let k = 2.0 / (1.0 + g);
    // The renderer stores 0–1 per texel: the scale k goes into the colour,
    // which is then at most k (≤ 2) times the dome colour; the caller's
    // intensity carries the rest. Halve to stay in range.
    let color = color.map(|c| c * k * 0.5);
    let dome = |t: f32| g + (1.0 - g) * t;
    // Irradiance of a sky / ground split for a normal of up component z:
    // the cosine lobe sees (1 + z) / 2 of the sky.
    let diffuse = cube(16, 1, color, |_, z| dome((1.0 + z) * 0.5));
    let specular = cube(64, SKY_MIPS, color, |level, z| {
        let rough = level as f32 / (SKY_MIPS - 1) as f32;
        // The horizon blurs over about the GGX lobe width (alpha = roughness²).
        let w = (rough * rough).max(1.0 / (64 >> level).max(1) as f32);
        let t = ((z / w) * 0.5 + 0.5).clamp(0.0, 1.0);
        dome(t * t * (3.0 - 2.0 * t))
    });
    (diffuse, specular)
}

/// The intensity factor `sky_maps` leaves to the caller.
pub const SKY_MAP_SCALE: f32 = 2.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halton_spreads_samples_over_the_cell() {
        let js: Vec<[f32; 2]> = (0..16).map(|p| jitter(p, 16)).collect();
        assert!(js.iter().all(|j| j[0].abs() <= 0.5 && j[1].abs() <= 0.5));
        let quadrants = |sx: f32, sy: f32| js.iter().filter(|j| j[0].signum() == sx && j[1].signum() == sy).count();
        for (sx, sy) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
            assert!(quadrants(sx, sy) >= 2, "{js:?}");
        }
        assert_eq!(jitter(0, 1), [0.0; 2]);
    }

    #[test]
    fn disk_points_stay_inside_and_spread_out() {
        let ps: Vec<[f32; 2]> = (0..32).map(|p| disk(p, 32)).collect();
        assert!(ps.iter().all(|p| p[0].hypot(p[1]) <= 1.0));
        let mean = ps.iter().fold([0.0, 0.0], |a, p| [a[0] + p[0] / 32.0, a[1] + p[1] / 32.0]);
        assert!(mean[0].abs() < 0.1 && mean[1].abs() < 0.1, "{mean:?}");
    }

    fn flat(w: u32, h: u32, v: u8) -> image::RgbaImage {
        image::RgbaImage::from_pixel(w, h, image::Rgba([v, v, v, 255]))
    }

    #[test]
    fn a_flat_pass_stays_flat_through_every_filter() {
        for f in AaFilter::ALL {
            for ss in [1, 3] {
                let mut a = Accum::new(([0.0, 0.0, 1.0, 1.0], [8, 6]), 3, ss);
                let [w, h] = a.internal_size();
                for p in 0..4 {
                    a.add(&flat(w, h, 128), jitter(p, 4), f, 2.5).unwrap();
                }
                let img = a.image([1.0, 0.0, 0.0]);
                assert!(img.pixels().all(|p| (p.0[0] as i32 - 128).abs() <= 1 && p.0[1] == 128 && p.0[3] == 255), "{f:?} ss {ss}");
            }
        }
    }

    #[test]
    fn without_jitter_a_one_pixel_box_keeps_the_image() {
        let mut a = Accum::new(([0.0, 0.0, 1.0, 1.0], [4, 4]), 1, 1);
        let img = image::RgbaImage::from_fn(6, 6, |x, y| image::Rgba([(x * 40) as u8, (y * 40) as u8, 7, 255]));
        a.add(&img, [0.0; 2], AaFilter::Area, 1.0).unwrap();
        let out = a.image([0.0; 3]);
        for y in 0..4 {
            for x in 0..4 {
                let (o, i) = (out.get_pixel(x, y).0, img.get_pixel(x + 1, y + 1).0);
                assert!((0..3).all(|k| (o[k] as i32 - i[k] as i32).abs() <= 1), "{x},{y}: {o:?} vs {i:?}");
            }
        }
    }

    #[test]
    fn the_sky_is_bright_above_and_dim_below() {
        let (d, s) = sky_maps([1.0, 1.0, 1.0], 0.25);
        // +Z face (index 4) is up, -Z (5) down; one 16 px face of 4 bytes per texel.
        let face = |img: &Image, f: usize| img.data.as_ref().unwrap()[f * 16 * 16 * 4 + (8 * 16 + 8) * 4];
        assert!(face(&d, 4) > face(&d, 5) + 50, "{} vs {}", face(&d, 4), face(&d, 5));
        assert_eq!(s.texture_descriptor.mip_level_count, SKY_MIPS);
        assert_eq!(s.data.as_ref().unwrap().len(), 6 * 4 * (64 * 64 + 32 * 32 + 16 * 16 + 8 * 8 + 4 * 4 + 2 * 2 + 1));
    }
}
