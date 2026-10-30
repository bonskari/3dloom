//! Render Setup settings saved with the scene: the Common panel (output
//! size, options, render output, area to render) and the Bevy renderer's own
//! panel.

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

/// Antialiasing > Filter: how the samples around a pixel are weighted into
/// it (a reconstruction filter Filter Size pixels wide).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AaFilter {
    /// A box: every sample under the filter counts the same.
    #[default]
    Area,
    /// Blackman window: sharp, little ringing.
    Blackman,
    /// Catmull-Rom cubic: sharpening.
    CatmullRom,
    /// Cubic B-spline: smooth, slightly blurring.
    Cubic,
    /// Mitchell-Netravali cubic (B = C = 1/3): balanced.
    MitchellNetravali,
    /// Quadratic B-spline: smooth.
    Quadratic,
    /// Gaussian: soft.
    Soften,
}

impl AaFilter {
    pub const ALL: [AaFilter; 7] = [AaFilter::Area, AaFilter::Blackman, AaFilter::CatmullRom, AaFilter::Cubic, AaFilter::MitchellNetravali, AaFilter::Quadratic, AaFilter::Soften];

    pub fn label(self) -> &'static str {
        match self {
            AaFilter::Area => "Area",
            AaFilter::Blackman => "Blackman",
            AaFilter::CatmullRom => "Catmull-Rom",
            AaFilter::Cubic => "Cubic",
            AaFilter::MitchellNetravali => "Mitchell-Netravali",
            AaFilter::Quadratic => "Quadratic",
            AaFilter::Soften => "Soften",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.label() == s)
    }

    /// The filter's weight at `x`, the distance from the pixel centre over
    /// the filter's radius (0 at the centre, 1 at the edge, 0 beyond).
    pub fn weight(self, x: f32) -> f32 {
        let x = x.abs();
        if x >= 1.0 {
            return 0.0;
        }
        // The cubics at their natural size span 2 pixels each side.
        let cubic = |b: f32, c: f32| {
            let t = 2.0 * x;
            if t < 1.0 {
                ((12.0 - 9.0 * b - 6.0 * c) * t * t * t + (-18.0 + 12.0 * b + 6.0 * c) * t * t + (6.0 - 2.0 * b)) / 6.0
            } else {
                ((-b - 6.0 * c) * t * t * t + (6.0 * b + 30.0 * c) * t * t + (-12.0 * b - 48.0 * c) * t + (8.0 * b + 24.0 * c)) / 6.0
            }
        };
        match self {
            AaFilter::Area => 1.0,
            AaFilter::Blackman => {
                let a = std::f32::consts::PI * (x + 1.0);
                0.42 - 0.5 * a.cos() + 0.08 * (2.0 * a).cos()
            }
            AaFilter::CatmullRom => cubic(0.0, 0.5),
            AaFilter::Cubic => cubic(1.0, 0.0),
            AaFilter::MitchellNetravali => cubic(1.0 / 3.0, 1.0 / 3.0),
            AaFilter::Quadratic => {
                let t = 1.5 * x;
                if t < 0.5 {
                    0.75 - t * t
                } else {
                    0.5 * (1.5 - t) * (1.5 - t)
                }
            }
            AaFilter::Soften => (-8.0 * x * x).exp(),
        }
    }
}

/// Exposure Control > the tone mapping curve from scene light to the image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Tonemapper {
    /// Clamp only.
    None,
    Reinhard,
    /// ACES filmic (fitted).
    Aces,
    /// AgX filmic: soft highlight roll-off.
    #[default]
    AgX,
    /// Tony McMapface: neutral filmic.
    TonyMcMapface,
    BlenderFilmic,
}

impl Tonemapper {
    pub const ALL: [Tonemapper; 6] = [Tonemapper::None, Tonemapper::Reinhard, Tonemapper::Aces, Tonemapper::AgX, Tonemapper::TonyMcMapface, Tonemapper::BlenderFilmic];

    pub fn label(self) -> &'static str {
        match self {
            Tonemapper::None => "None",
            Tonemapper::Reinhard => "Reinhard",
            Tonemapper::Aces => "ACES Filmic",
            Tonemapper::AgX => "AgX",
            Tonemapper::TonyMcMapface => "Tony McMapface",
            Tonemapper::BlenderFilmic => "Blender Filmic",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.label() == s)
    }
}

/// Ambient Occlusion quality: samples per pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AoQuality {
    Low,
    Medium,
    #[default]
    High,
    Ultra,
}

impl AoQuality {
    pub const ALL: [AoQuality; 4] = [AoQuality::Low, AoQuality::Medium, AoQuality::High, AoQuality::Ultra];

    pub fn label(self) -> &'static str {
        match self {
            AoQuality::Low => "Low",
            AoQuality::Medium => "Medium",
            AoQuality::High => "High",
            AoQuality::Ultra => "Ultra",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|q| q.label() == s)
    }
}

/// Render Setup > Renderer panel of the Bevy renderer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BevyRenderSettings {
    /// Antialiasing: `filter` over samples spread through each pixel (off:
    /// one sample at each pixel centre).
    pub antialiasing: bool,
    pub filter: AaFilter,
    /// Filter width in pixels (1–10).
    pub filter_size: f32,
    /// Each output pixel takes `supersampling`² samples per pass (1–4).
    pub supersampling: u32,
    /// Passes per bucket (1–64): each pass moves the samples within their
    /// pixel (and the lights over their Source Radius); the passes add up.
    pub passes: u32,
    /// The lights cast shadows.
    pub shadows: bool,
    /// Shadow map size in pixels (512–8192); renders give no light a smaller map.
    pub shadow_map_size: u32,
    /// Area Shadows: soft shadows from each light's Source Radius (over the passes).
    pub area_shadows: bool,
    /// Background colour, sRGB 0–1 (its alpha is 0: Display Alpha shows it black).
    pub background: [f32; 3],
    /// Ambient (environment) light colour, sRGB 0–1.
    pub ambient: [f32; 3],
    /// Ambient Occlusion: the ambient light darkens in creases and corners.
    pub ambient_occlusion: bool,
    pub ao_quality: AoQuality,
    /// How far around a point occluders darken it, in scene units.
    pub ao_radius: f32,
    /// Reflections: glossy surfaces reflect what the camera sees.
    pub reflections: bool,
    /// Sky Lighting: the ambient light comes from a sky dome (`ground` × as
    /// bright below the horizon) that glossy surfaces reflect, times `sky_intensity`.
    pub sky_lighting: bool,
    pub sky_intensity: f32,
    pub ground: f32,
    /// Exposure Control: tone mapping, exposure in stops, dithering.
    pub tonemapper: Tonemapper,
    pub exposure: f32,
    pub dither: bool,
}

impl Default for BevyRenderSettings {
    fn default() -> Self {
        Self {
            antialiasing: true,
            filter: AaFilter::Area,
            filter_size: 1.5,
            supersampling: 1,
            passes: 16,
            shadows: true,
            shadow_map_size: 4096,
            area_shadows: true,
            background: [0.0; 3],
            ambient: [1.0; 3],
            ambient_occlusion: true,
            ao_quality: AoQuality::High,
            ao_radius: 10.0,
            reflections: true,
            sky_lighting: true,
            sky_intensity: 1.0,
            ground: 0.5,
            tonemapper: Tonemapper::AgX,
            exposure: 0.0,
            dither: true,
        }
    }
}

#[cfg(test)]
mod quality_tests {
    use super::*;

    #[test]
    fn filters_peak_at_the_centre_and_end_at_the_edge() {
        for f in AaFilter::ALL {
            assert!(f.weight(0.0) > 0.5, "{f:?}");
            assert_eq!(f.weight(1.0), 0.0, "{f:?}");
            assert!(f.weight(0.0) >= f.weight(0.5), "{f:?}");
            assert_eq!(AaFilter::from_label(f.label()), Some(f));
        }
        // Catmull-Rom sharpens with a negative lobe; the B-spline never goes negative.
        assert!(AaFilter::CatmullRom.weight(0.7) < 0.0);
        assert!((0..100).all(|i| AaFilter::Cubic.weight(i as f32 / 100.0) >= 0.0));
    }

    #[test]
    fn old_scenes_load_with_the_new_quality_defaults() {
        let s: BevyRenderSettings = serde_json::from_str(r#"{"antialiasing":false,"supersampling":2}"#).unwrap();
        assert!(!s.antialiasing);
        assert_eq!(s.supersampling, 2);
        assert_eq!((s.passes, s.tonemapper, s.filter), (16, Tonemapper::AgX, AaFilter::Area));
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
