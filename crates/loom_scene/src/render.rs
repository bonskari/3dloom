//! Render Setup settings saved with the scene: the Common panel (output
//! size, options, render output, area to render) and the Bevy renderer's own
//! panel. The background and ambient colours are the Environment's.

use serde::{Deserialize, Serialize};

/// Rendered Frame Window > Area to Render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RenderArea {
    /// The whole view.
    #[default]
    View,
    /// Only the selected objects.
    Selected,
    /// A rectangle of the view; the rest of the frame is left alone.
    Region,
    /// A rectangle of the view, output at its own size.
    Crop,
    /// A rectangle of the view enlarged to the output size.
    Blowup,
}

impl RenderArea {
    pub const ALL: [RenderArea; 5] = [RenderArea::View, RenderArea::Selected, RenderArea::Region, RenderArea::Crop, RenderArea::Blowup];

    pub fn label(self) -> &'static str {
        match self {
            RenderArea::View => "View",
            RenderArea::Selected => "Selected",
            RenderArea::Region => "Region",
            RenderArea::Crop => "Crop",
            RenderArea::Blowup => "Blowup",
        }
    }

    /// Whether the area is a rectangle of the view (Region, Crop, Blowup).
    pub fn uses_region(self) -> bool {
        matches!(self, RenderArea::Region | RenderArea::Crop | RenderArea::Blowup)
    }
}

/// Render Setup > Renderer panel of the Bevy renderer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BevyRenderSettings {
    /// 4x multisample antialiasing.
    pub antialiasing: bool,
    /// Each output pixel averages `supersampling`² rendered samples (1–4).
    pub supersampling: u32,
    /// The key light casts shadows.
    pub shadows: bool,
    /// Shadow map size in pixels (512–8192).
    pub shadow_map_size: u32,
}

impl Default for BevyRenderSettings {
    fn default() -> Self {
        Self { antialiasing: true, supersampling: 1, shadows: true, shadow_map_size: 4096 }
    }
}

/// Render Setup dialog values (Common and Renderer panels).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderSettings {
    /// Output Size in pixels.
    pub width: u32,
    pub height: u32,
    /// Pixel Aspect: the shape of the output device's pixels.
    pub pixel_aspect: f32,
    /// Image Aspect lock: Width and Height change together.
    pub lock_image_aspect: bool,
    /// Pixel Aspect lock.
    pub lock_pixel_aspect: bool,
    /// Options > Render Hidden Geometry.
    pub render_hidden: bool,
    /// Options > Force 2-Sided.
    pub force_two_sided: bool,
    /// Options > Atmospherics: the Environment's atmosphere list renders.
    pub atmospherics: bool,
    /// Options > Effects: the rendering effects list applies.
    pub effects: bool,
    /// Render Output > Save File, and the file Files... chose.
    pub save_file: bool,
    pub output_path: Option<String>,
    /// Rendered Frame Window > Area to Render.
    pub area: RenderArea,
    /// Region and Crop rectangle: x0, y0, x1, y1 as fractions of the frame (y down).
    pub region: [f32; 4],
    /// Blowup rectangle (kept apart from Region/Crop, as the reference does).
    pub blowup: [f32; 4],
    pub bevy: BevyRenderSettings,
    /// Render To Texture.
    pub bake: crate::bake::BakeSettings,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            width: 640,
            height: 480,
            pixel_aspect: 1.0,
            lock_image_aspect: false,
            lock_pixel_aspect: false,
            render_hidden: false,
            force_two_sided: false,
            atmospherics: true,
            effects: true,
            save_file: false,
            output_path: None,
            area: RenderArea::View,
            region: [0.25, 0.25, 0.75, 0.75],
            blowup: [0.25, 0.25, 0.75, 0.75],
            bevy: BevyRenderSettings::default(),
            bake: crate::bake::BakeSettings::default(),
        }
    }
}

impl RenderSettings {
    /// Image Aspect: width / height of the picture on the output device.
    pub fn image_aspect(&self) -> f32 {
        self.width as f32 * self.pixel_aspect / self.height.max(1) as f32
    }

    /// The rectangle the current area uses (Blowup has its own).
    pub fn area_rect(&self) -> [f32; 4] {
        if self.area == RenderArea::Blowup {
            self.blowup
        } else {
            self.region
        }
    }
}
