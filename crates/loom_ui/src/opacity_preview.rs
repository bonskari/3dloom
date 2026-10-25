//! One bounded viewport bitmap: real diffuse RGB and Standard opacity-map intensity.
use loom_render::Bitmap;
use loom_scene::{
    maps::{self, Images},
    material::{MapType, Surface},
};

pub(crate) fn key(surface: &Surface) -> String {
    format!(
        "opacity:{:?}|{:?}|{:?}",
        surface.diffuse,
        surface.viewport_map(),
        surface.opacity_map
    )
}
fn linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}
fn srgb(value: f32) -> f32 {
    if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

/// Map coordinates/crop/mirror are baked at canonical channel-1 UVs. The
/// material's constant opacity stays in vertex alpha, not in these pixels.
/// None means an actual required bitmap is still pending or unavailable.
pub(crate) fn compose(surface: &Surface, images: &Images, max_side: u32) -> Option<Bitmap> {
    let opacity = surface.opacity_map.as_ref().filter(|map| map.enabled)?;
    if max_side == 0 {
        return None;
    }
    let diffuse = surface.viewport_map().filter(|map| map.enabled);
    let mut required = Images::new();
    for map in diffuse.into_iter().chain(std::iter::once(opacity)) {
        if map.kind == MapType::Bitmap && map.path.is_empty() {
            return None;
        }
        maps::load_images(map, &mut required, &mut |path| {
            images.get(path).cloned().flatten()
        });
    }
    if required.values().any(Option::is_none) {
        return None;
    }
    let width = required
        .values()
        .flatten()
        .map(|image| image.width)
        .max()
        .unwrap_or(256)
        .clamp(1, max_side);
    let height = required
        .values()
        .flatten()
        .map(|image| image.height)
        .max()
        .unwrap_or(256)
        .clamp(1, max_side);
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let uv = [
                (x as f32 + 0.5) / width as f32,
                1.0 - (y as f32 + 0.5) / height as f32,
            ];
            let mut color = surface.diffuse.to_array();
            if let Some(map) = diffuse {
                let pixel = maps::eval(map, uv, images);
                let strength = pixel[3] * (map.amount / 100.0).clamp(0.0, 1.0);
                // Match the old shader's linear-light diffuse/decal blend.
                for channel in 0..3 {
                    color[channel] = if strength >= 1.0 {
                        pixel[channel]
                    } else if strength <= 0.0 {
                        color[channel]
                    } else {
                        srgb(
                            linear(color[channel]) * (1.0 - strength)
                                + linear(pixel[channel]) * strength,
                        )
                    };
                }
            }
            let pixel = maps::eval(opacity, uv, images);
            let gray = (pixel[0] + pixel[1] + pixel[2]) / 3.0;
            let alpha = 1.0 + (gray - 1.0) * (opacity.amount / 100.0).clamp(0.0, 1.0);
            rgba.extend(
                [color[0], color[1], color[2], alpha]
                    .map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8),
            );
        }
    }
    Some(Bitmap {
        width,
        height,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_scene::{
        maps::Image,
        material::{Map, StandardMaterial},
    };
    use std::sync::Arc;
    fn image(rgba: Vec<u8>) -> Option<Arc<Image>> {
        Some(Arc::new(Image {
            width: rgba.len() as u32 / 4,
            height: 1,
            rgba,
        }))
    }
    fn surface() -> Surface {
        let mut material = StandardMaterial::new("Opacity test");
        material.diffuse.set([0.2, 0.4, 0.6].into());
        material.opacity_map = Some(Map::bitmap("opacity.png", 100.0));
        Surface::from_standard(&material)
    }
    #[test]
    fn opacity_retains_each_pixel_including_low_nonuniform_lacrimal_values() {
        let mut surface = surface();
        surface.opacity = 0.5;
        let images = [(
            "opacity.png".into(),
            image(vec![
                0, 0, 0, 255, 4, 4, 4, 255, 128, 128, 128, 255, 255, 255, 255, 255,
            ]),
        )]
        .into_iter()
        .collect();
        let result = compose(&surface, &images, 4).unwrap();
        assert_eq!(
            result
                .rgba
                .chunks_exact(4)
                .map(|pixel| pixel[3])
                .collect::<Vec<_>>(),
            vec![0, 4, 128, 255]
        );
        assert!(result
            .rgba
            .chunks_exact(4)
            .all(|pixel| pixel[..3] == [51, 102, 153]));
        surface.opacity_map.as_mut().unwrap().amount = 50.0;
        assert_eq!(compose(&surface, &images, 4).unwrap().rgba[3], 128);
    }
    #[test]
    fn diffuse_and_opacity_keep_independent_uv_crop_and_show_map_settings() {
        let mut surface = surface();
        surface.diffuse_map = Some(Map::bitmap("diffuse.png", 100.0));
        surface.show_map_in_viewport = true;
        let opacity = surface.opacity_map.as_mut().unwrap();
        opacity.crop.apply = true;
        opacity.crop.u = 0.5;
        opacity.crop.w = 0.5;
        let images = [
            (
                "opacity.png".into(),
                image(vec![0, 0, 0, 255, 255, 255, 255, 255]),
            ),
            (
                "diffuse.png".into(),
                image(vec![255, 0, 0, 255, 0, 255, 0, 255]),
            ),
        ]
        .into_iter()
        .collect();
        let result = compose(&surface, &images, 2).unwrap();
        assert_eq!(&result.rgba[..3], &[255, 0, 0]);
        assert_eq!(&result.rgba[4..7], &[0, 255, 0]);
        assert!(
            result.rgba[3] > 180 && result.rgba[7] > 180,
            "crop samples opaque half: {:?}",
            result.rgba
        );
        surface.show_map_in_viewport = false;
        let only_opacity = [("opacity.png".into(), images["opacity.png"].clone())]
            .into_iter()
            .collect();
        assert_eq!(
            &compose(&surface, &only_opacity, 2).unwrap().rgba[..3],
            &[51, 102, 153]
        );
    }
    #[test]
    fn missing_required_pixels_stay_pending_and_composition_is_bounded() {
        let surface = surface();
        assert!(compose(&surface, &Images::new(), 512).is_none());
        let images = [("opacity.png".into(), image(vec![255; 4096 * 4]))]
            .into_iter()
            .collect();
        let result = compose(&surface, &images, 128).unwrap();
        assert_eq!((result.width, result.height), (128, 1));
        assert!(compose(&surface, &images, 0).is_none());
        let mut unselected = surface.clone();
        unselected.opacity_map.as_mut().unwrap().path.clear();
        assert!(compose(&unselected, &Images::new(), 128).is_none());
    }
    #[test]
    fn composed_diffuse_keeps_rgba_decal_coverage_and_linear_light_blending() {
        let mut surface = surface();
        surface.show_map_in_viewport = true;
        surface.diffuse_map = Some(Map::bitmap("diffuse.png", 100.0));
        let images = [
            ("opacity.png".into(), image(vec![255; 8])),
            (
                "diffuse.png".into(),
                image(vec![255, 0, 0, 0, 255, 0, 0, 128]),
            ),
        ]
        .into_iter()
        .collect();
        let result = compose(&surface, &images, 2).unwrap();
        assert_eq!(&result.rgba[..4], &[51, 102, 153, 255]);
        assert_eq!(&result.rgba[4..], &[191, 73, 111, 255]);
    }
}
