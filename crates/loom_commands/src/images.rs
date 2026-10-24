//! Image files for maps and render output: the formats the `image` crate
//! reads and writes here (PNG, JPEG, BMP, Targa, TIFF, OpenEXR, Radiance
//! HDR; DDS and GIF read only), and saving a rendered RGBA image in the
//! layout each format takes.

use std::path::Path;

/// Extensions a bitmap can be loaded from.
pub const READ_EXTENSIONS: [&str; 11] = ["png", "jpg", "jpeg", "bmp", "tga", "tif", "tiff", "exr", "hdr", "dds", "gif"];
/// Extensions render output and templates can be saved to.
pub const WRITE_EXTENSIONS: [&str; 9] = ["png", "jpg", "jpeg", "bmp", "tga", "tif", "tiff", "exr", "hdr"];

/// Saves `img` by `path`'s extension: JPEG drops alpha, OpenEXR stores
/// 32-bit float RGBA, Radiance HDR 32-bit float RGB; the rest keep RGBA 8.
pub fn save_rgba(img: &image::RgbaImage, path: &Path) -> Result<(), String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if !WRITE_EXTENSIONS.contains(&ext.as_str()) {
        return Err(format!("{}: can't save .{ext} images ({})", path.display(), WRITE_EXTENSIONS.join(", ")));
    }
    let dynamic = image::DynamicImage::ImageRgba8(img.clone());
    let r = match ext.as_str() {
        "jpg" | "jpeg" => dynamic.to_rgb8().save(path),
        "exr" => dynamic.to_rgba32f().save(path),
        "hdr" => dynamic.to_rgb32f().save(path),
        _ => img.save(path),
    };
    r.map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_write_format_saves_and_reads_back() {
        let dir = std::env::temp_dir().join("loom-image-formats");
        let _ = std::fs::create_dir_all(&dir);
        let mut img = image::RgbaImage::new(8, 4);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([(x * 30) as u8, (y * 60) as u8, 128, 255]);
        }
        for ext in WRITE_EXTENSIONS {
            let path = dir.join(format!("t.{ext}"));
            save_rgba(&img, &path).unwrap_or_else(|e| panic!("{ext}: {e}"));
            let back = image::open(&path).unwrap_or_else(|e| panic!("{ext}: {e}")).to_rgba8();
            assert_eq!(back.dimensions(), (8, 4), "{ext}");
            let p = back.get_pixel(7, 3);
            // JPEG is lossy; the float formats round-trip through linear floats.
            assert!((p[0] as i32 - 210).abs() <= 12 && (p[1] as i32 - 180).abs() <= 12, "{ext}: {p:?}");
        }
        assert!(save_rgba(&img, &dir.join("t.dds")).is_err());
    }
}
