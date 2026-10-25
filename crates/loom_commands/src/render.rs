//! Rendering: Render (Production / Iterative / Last), Render Setup and the
//! Rendered Frame Window. The command layer has no GPU: a render records a
//! request that the UI's renderer works through over several frames,
//! putting the image into `AppState::rendered`.

use crate::command::Command as C;
use crate::state::RenderRequest;
use crate::{AppState, Dialog};
use loom_scene::render::{RenderArea, RenderSettings};
use loom_scene::Change;
use serde_json::{json, Value};
use std::sync::Arc;

/// Largest output side in pixels.
pub const MAX_RENDER_SIZE: u32 = 8192;
/// Image files Save Image writes, by extension.
pub const IMAGE_FORMATS: &[&str] = &["png", "jpg", "jpeg", "bmp", "tga", "exr"];

/// Rendered Frame Window channel toggles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Channels {
    pub red: bool,
    pub green: bool,
    pub blue: bool,
    /// Display Alpha Channel: the alpha as grey.
    pub alpha: bool,
    /// Monochrome: an 8-bit grey of the image.
    pub mono: bool,
}

impl Default for Channels {
    fn default() -> Self {
        Self { red: true, green: true, blue: true, alpha: false, mono: false }
    }
}

impl Channels {
    /// One pixel as the window shows it (opaque; the background is black).
    pub fn display(self, p: [u8; 4]) -> [u8; 4] {
        if self.alpha {
            return [p[3], p[3], p[3], 255];
        }
        let c = [if self.red { p[0] } else { 0 }, if self.green { p[1] } else { 0 }, if self.blue { p[2] } else { 0 }];
        if self.mono {
            let g = mono(c);
            return [g, g, g, 255];
        }
        [c[0], c[1], c[2], 255]
    }

    /// The image as displayed (what Copy Image puts on the clipboard).
    pub fn displayed(self, img: &image::RgbaImage) -> image::RgbaImage {
        let mut out = img.clone();
        for p in out.pixels_mut() {
            p.0 = self.display(p.0);
        }
        out
    }
}

/// Grey value of an sRGB colour (Rec. 601 luma, as monochrome map channels use).
pub fn mono(c: [u8; 3]) -> u8 {
    (0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32).round().clamp(0.0, 255.0) as u8
}

/// A Clone Rendered Frame Window copy: the image only.
#[derive(Debug, Clone)]
pub struct ClonedFrame {
    pub id: u32,
    pub title: String,
    pub image: Arc<image::RgbaImage>,
    pub channels: Channels,
}

/// A render in progress (set by the UI's renderer every frame it works).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderProgress {
    /// Buckets finished and in total.
    pub done: u32,
    pub total: u32,
    pub paused: bool,
}

/// The Rendered Frame Window and the render it shows.
#[derive(Debug, Clone, Default)]
pub struct RenderedFrame {
    pub open: bool,
    pub image: Option<Arc<image::RgbaImage>>,
    /// Bumped whenever `image` changes; `dirty` is the pixel rectangle
    /// (x, y, w, h) changed since the UI last showed it (None: all of it;
    /// an empty one: nothing).
    pub version: u64,
    pub dirty: Option<[u32; 4]>,
    /// Title bar: viewport, frame, image type, aspect.
    pub title: String,
    pub channels: Channels,
    /// Viewport list + Lock To Viewport.
    pub viewport: usize,
    pub lock_to_viewport: bool,
    /// Production (false) or Iterative.
    pub iterative: bool,
    /// Edit Region: the region rectangle is editable in the viewport.
    pub edit_region: bool,
    pub clones: Vec<ClonedFrame>,
    pub next_clone: u32,
    pub progress: Option<RenderProgress>,
    /// Cancel was asked for the render in progress.
    pub cancel: bool,
    /// Copy Image is waiting for the UI (the clipboard is the window system's).
    pub copy_requested: bool,
    /// What Render Last repeats.
    pub last: Option<RenderRequest>,
}

impl RenderedFrame {
    /// Replaces the image (the whole of it changed).
    pub fn set_image(&mut self, image: Option<Arc<image::RgbaImage>>) {
        self.image = image;
        self.version += 1;
        self.dirty = None;
    }
}

pub fn handles(c: &C) -> bool {
    matches!(
        c,
        C::Render { .. }
            | C::RenderIterative
            | C::RenderLast
            | C::RenderSetup { .. }
            | C::BevyRendererSettings { .. }
            | C::AreaToRender { .. }
            | C::EditRegion
            | C::RenderViewport { .. }
            | C::RenderedFrameWindow
            | C::SaveRenderedImage { .. }
            | C::CopyRenderedImage
            | C::CloneRenderedFrameWindow
            | C::ClearRenderedFrameWindow
            | C::RenderedFrameWindowChannels { .. }
            | C::CancelRender
            | C::PauseRender
    )
}

fn check_size(width: u32, height: u32) -> Result<(), String> {
    if width == 0 || height == 0 || width > MAX_RENDER_SIZE || height > MAX_RENDER_SIZE {
        return Err(format!("render size {width}x{height}: 1 to {MAX_RENDER_SIZE} pixels"));
    }
    Ok(())
}

/// The viewport a render uses: the locked one, else the active one.
pub fn render_viewport(state: &AppState) -> usize {
    let i = if state.rendered.lock_to_viewport { state.rendered.viewport } else { state.active_viewport };
    i.min(state.viewports.len().saturating_sub(1))
}

/// Viewport name as the Rendered Frame Window lists it.
pub fn viewport_name(state: &AppState, index: usize) -> String {
    match state.viewports.get(index).map(|v| v.kind) {
        Some(crate::ViewKind::Camera(id)) => state.scene.node(id).map(|n| n.name.clone()).unwrap_or_else(|| "Camera".into()),
        Some(k) => k.label().to_owned(),
        None => String::new(),
    }
}

/// One undo step for Render Setup values.
fn set_settings(state: &mut AppState, label: &str, after: RenderSettings) -> Result<(), String> {
    if after == state.scene.render {
        return Ok(());
    }
    let before = Box::new(state.scene.render.clone());
    state.undo.commit(label, Change::SetRenderSettings { before, after: Box::new(after) }, &mut state.scene)
}

fn settings_json(s: &RenderSettings) -> Value {
    serde_json::to_value(s).unwrap_or(Value::Null)
}

/// A rectangle x0, y0, x1, y1 inside the frame, at least one pixel's worth.
fn check_rect(r: [f32; 4]) -> Result<[f32; 4], String> {
    let [x0, y0, x1, y1] = r;
    if !r.iter().all(|v| v.is_finite()) || x0 < 0.0 || y0 < 0.0 || x1 > 1.0 || y1 > 1.0 || x1 - x0 < 1e-3 || y1 - y0 < 1e-3 {
        return Err(format!("region {r:?}: x0 < x1 and y0 < y1, fractions of the frame 0-1"));
    }
    Ok(r)
}

/// A Blowup rectangle keeps the output's shape: the same fraction of the
/// width and of the height, centred where it was asked for.
fn blowup_rect(r: [f32; 4]) -> [f32; 4] {
    let s = ((r[2] - r[0]) + (r[3] - r[1])) * 0.5;
    let cx = ((r[0] + r[2]) * 0.5).clamp(s * 0.5, 1.0 - s * 0.5);
    let cy = ((r[1] + r[3]) * 0.5).clamp(s * 0.5, 1.0 - s * 0.5);
    [cx - s * 0.5, cy - s * 0.5, cx + s * 0.5, cy + s * 0.5]
}

pub(crate) fn start(state: &mut AppState, mut req: RenderRequest) -> Result<Value, String> {
    if state.rendered.progress.is_some() || state.pending_render.is_some() {
        return Err("a render is in progress".into());
    }
    check_size(req.settings.width, req.settings.height)?;
    req.selection = state.selection.iter().copied().collect();
    if req.settings.area == RenderArea::Selected && req.selection.is_empty() {
        return Err("Area to Render is Selected but nothing is selected".into());
    }
    if req.preview {
        // Exposure Control's thumbnail: the Rendered Frame Window and Render Last stay as they are.
        let out = json!({ "pending": true, "preview": true, "width": req.settings.width, "height": req.settings.height });
        state.pending_render = Some(req);
        return Ok(out);
    }
    let s = &req.settings;
    let name = viewport_name(state, req.viewport);
    state.rendered.title = format!("{name}, frame {}, RGBA Color 8 Bits/Channel (1:{:.3})", state.current_frame, s.image_aspect());
    state.rendered.open = true;
    let out = json!({ "pending": true, "viewport": req.viewport, "width": s.width, "height": s.height, "area": s.area.label(), "iterative": req.iterative, "path": req.save_path });
    state.rendered.last = Some(RenderRequest { save_path: None, ..req.clone() });
    state.pending_render = Some(req);
    Ok(out)
}

/// Writes `img` to `path`, the format from its extension.
pub fn save_image(img: &image::RgbaImage, path: &str) -> Result<(), String> {
    let ext = std::path::Path::new(path).extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    let err = |e: image::ImageError| format!("{path}: {e}");
    match ext.as_str() {
        "png" | "tga" => img.save(path).map_err(err),
        // No alpha in these formats.
        "jpg" | "jpeg" | "bmp" => image::DynamicImage::ImageRgba8(img.clone()).to_rgb8().save(path).map_err(err),
        "exr" => {
            // OpenEXR holds linear light.
            let lin = |c: u8| {
                let x = c as f32 / 255.0;
                if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
            };
            let mut f = image::Rgba32FImage::new(img.width(), img.height());
            for (o, p) in f.pixels_mut().zip(img.pixels()) {
                o.0 = [lin(p.0[0]), lin(p.0[1]), lin(p.0[2]), p.0[3] as f32 / 255.0];
            }
            f.save(path).map_err(err)
        }
        _ => Err(format!("{path}: save as {}", IMAGE_FORMATS.join(", "))),
    }
}

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    match cmd {
        C::Render { path, width, height } => {
            let mut settings = state.scene.render.clone();
            settings.width = width.unwrap_or(settings.width);
            settings.height = height.unwrap_or(settings.height);
            if path.as_deref().is_some_and(|p| p.trim().is_empty()) {
                return Err("render path is empty".into());
            }
            let iterative = state.rendered.iterative;
            // Production saves the Render Output file when Save File is on;
            // Iterative never writes files.
            let save_path = path.or_else(|| (!iterative && settings.save_file).then(|| settings.output_path.clone()).flatten());
            start(state, RenderRequest { viewport: render_viewport(state), settings, selection: Vec::new(), save_path, iterative, preview: false })
        }
        C::RenderIterative => {
            let settings = state.scene.render.clone();
            start(state, RenderRequest { viewport: render_viewport(state), settings, selection: Vec::new(), save_path: None, iterative: true, preview: false })
        }
        C::RenderLast => {
            let Some(last) = state.rendered.last.clone() else {
                return Err("nothing has been rendered yet".into());
            };
            // The same view, area and settings; never saved to a file.
            start(state, RenderRequest { save_path: None, ..last })
        }
        C::RenderSetup { width, height, image_aspect, pixel_aspect, lock_image_aspect, lock_pixel_aspect, render_hidden, force_two_sided, atmospherics, effects, save_file, output } => {
            if [width.is_none(), height.is_none(), image_aspect.is_none(), pixel_aspect.is_none(), lock_image_aspect.is_none(), lock_pixel_aspect.is_none(), render_hidden.is_none(), force_two_sided.is_none(), atmospherics.is_none(), effects.is_none(), save_file.is_none(), output.is_none()].iter().all(|n| *n) {
                state.dialog = Some(Dialog::RenderSetup);
                return Ok(json!({ "dialog": "RenderSetup" }));
            }
            let mut s = state.scene.render.clone();
            let aspect = s.image_aspect();
            if let Some(v) = lock_image_aspect {
                s.lock_image_aspect = v;
            }
            if let Some(v) = lock_pixel_aspect {
                s.lock_pixel_aspect = v;
            }
            if let Some(pa) = pixel_aspect {
                if !(0.001..=1000.0).contains(&pa) {
                    return Err(format!("pixel aspect {pa}: 0.001 to 1000"));
                }
                if s.lock_pixel_aspect && (pa - s.pixel_aspect).abs() > 1e-6 {
                    return Err("Pixel Aspect is locked".into());
                }
                s.pixel_aspect = pa;
                // A locked image aspect keeps the picture's shape: Height follows.
                if s.lock_image_aspect {
                    s.height = ((s.width as f32 * pa / aspect).round() as u32).max(1);
                }
            }
            match (width, height) {
                (Some(w), None) if s.lock_image_aspect => {
                    s.width = w;
                    s.height = ((w as f32 * s.pixel_aspect / aspect).round() as u32).max(1);
                }
                (None, Some(h)) if s.lock_image_aspect => {
                    s.height = h;
                    s.width = ((h as f32 * aspect / s.pixel_aspect).round() as u32).max(1);
                }
                (w, h) => {
                    s.width = w.unwrap_or(s.width);
                    s.height = h.unwrap_or(s.height);
                }
            }
            if let Some(a) = image_aspect {
                if !(0.001..=1000.0).contains(&a) {
                    return Err(format!("image aspect {a}: 0.001 to 1000"));
                }
                s.height = ((s.width as f32 * s.pixel_aspect / a).round() as u32).max(1);
            }
            check_size(s.width, s.height)?;
            if let Some(v) = render_hidden {
                s.render_hidden = v;
            }
            if let Some(v) = force_two_sided {
                s.force_two_sided = v;
            }
            if let Some(v) = atmospherics {
                s.atmospherics = v;
            }
            if let Some(v) = effects {
                s.effects = v;
            }
            if let Some(p) = output {
                let p = p.trim().to_owned();
                // Choosing a file turns Save File on; an empty name clears it.
                s.save_file = !p.is_empty();
                s.output_path = (!p.is_empty()).then_some(p);
            }
            if let Some(v) = save_file {
                if v && s.output_path.is_none() {
                    return Err("Save File needs an output file (Files...)".into());
                }
                s.save_file = v;
            }
            set_settings(state, "Render Setup", s)?;
            Ok(settings_json(&state.scene.render))
        }
        C::BevyRendererSettings { antialiasing, supersampling, shadows, shadow_map_size } => {
            let mut s = state.scene.render.clone();
            let b = &mut s.bevy;
            if let Some(v) = antialiasing {
                b.antialiasing = v;
            }
            if let Some(v) = supersampling {
                if !(1..=4).contains(&v) {
                    return Err(format!("supersampling {v}: 1 to 4"));
                }
                b.supersampling = v;
            }
            if let Some(v) = shadows {
                b.shadows = v;
            }
            if let Some(v) = shadow_map_size {
                if !(512..=8192).contains(&v) {
                    return Err(format!("shadow map size {v}: 512 to 8192"));
                }
                b.shadow_map_size = v;
            }
            set_settings(state, "Render Setup", s)?;
            Ok(settings_json(&state.scene.render))
        }
        C::AreaToRender { area, region } => {
            let mut s = state.scene.render.clone();
            if let Some(a) = area {
                s.area = a;
            }
            if let Some(r) = region {
                let r = check_rect(r)?;
                if s.area == RenderArea::Blowup {
                    s.blowup = blowup_rect(r);
                } else {
                    s.region = r;
                }
            }
            if !s.area.uses_region() {
                state.rendered.edit_region = false;
            }
            set_settings(state, "Area to Render", s)?;
            let s = &state.scene.render;
            Ok(json!({ "area": s.area.label(), "region": s.region, "blowup": s.blowup }))
        }
        C::EditRegion => {
            state.rendered.edit_region = !state.rendered.edit_region;
            // View or Selected switch to Region, as the reference does.
            if state.rendered.edit_region && !state.scene.render.area.uses_region() {
                let mut s = state.scene.render.clone();
                s.area = RenderArea::Region;
                set_settings(state, "Area to Render", s)?;
            }
            Ok(json!({ "edit_region": state.rendered.edit_region, "area": state.scene.render.area.label() }))
        }
        C::RenderViewport { viewport, lock, iterative } => {
            if let Some(v) = viewport {
                let v = v as usize;
                if v >= state.viewports.len() {
                    return Err(format!("no viewport {v}"));
                }
                state.rendered.viewport = v;
                if !state.rendered.lock_to_viewport && lock != Some(true) {
                    // Unlocked, the list follows the active viewport: choosing activates it.
                    state.active_viewport = v;
                }
            }
            if let Some(l) = lock {
                if l && !state.rendered.lock_to_viewport && viewport.is_none() {
                    state.rendered.viewport = state.active_viewport;
                }
                state.rendered.lock_to_viewport = l;
            }
            if let Some(i) = iterative {
                state.rendered.iterative = i;
            }
            let v = render_viewport(state);
            Ok(json!({ "viewport": v, "name": viewport_name(state, v), "lock": state.rendered.lock_to_viewport, "iterative": state.rendered.iterative }))
        }
        C::RenderedFrameWindow => {
            state.rendered.open = !state.rendered.open;
            Ok(json!({ "open": state.rendered.open }))
        }
        C::SaveRenderedImage { path } => {
            let Some(img) = state.rendered.image.clone() else {
                return Err("the Rendered Frame Window has no image".into());
            };
            let Some(path) = path else {
                state.dialog = Some(Dialog::SaveRenderedImage);
                return Ok(json!({ "dialog": "SaveRenderedImage" }));
            };
            save_image(&img, &path)?;
            state.status_prompt = format!("Saved {}", path);
            Ok(json!({ "path": path, "width": img.width(), "height": img.height() }))
        }
        C::CopyRenderedImage => {
            if state.rendered.image.is_none() {
                return Err("the Rendered Frame Window has no image".into());
            }
            state.rendered.copy_requested = true;
            Ok(json!({ "copied": true }))
        }
        C::CloneRenderedFrameWindow => {
            let Some(img) = state.rendered.image.clone() else {
                return Err("the Rendered Frame Window has no image".into());
            };
            state.rendered.next_clone += 1;
            let id = state.rendered.next_clone;
            let title = format!("{} (clone {id})", state.rendered.title);
            state.rendered.clones.push(ClonedFrame { id, title, image: img, channels: state.rendered.channels });
            Ok(json!({ "clone": id }))
        }
        C::ClearRenderedFrameWindow => {
            state.rendered.set_image(None);
            Ok(json!({ "cleared": true }))
        }
        C::RenderedFrameWindowChannels { red, green, blue, alpha, mono } => {
            let c = &mut state.rendered.channels;
            for (field, v) in [(&mut c.red, red), (&mut c.green, green), (&mut c.blue, blue), (&mut c.alpha, alpha), (&mut c.mono, mono)] {
                if let Some(v) = v {
                    *field = v;
                }
            }
            let c = state.rendered.channels;
            Ok(json!({ "red": c.red, "green": c.green, "blue": c.blue, "alpha": c.alpha, "mono": c.mono }))
        }
        C::CancelRender => {
            if state.pending_render.take().is_some() {
                return Ok(json!({ "cancelled": true }));
            }
            if state.rendered.progress.is_none() {
                return Err("nothing is rendering".into());
            }
            state.rendered.cancel = true;
            Ok(json!({ "cancelled": true }))
        }
        C::PauseRender => {
            let Some(p) = state.rendered.progress.as_mut() else {
                return Err("nothing is rendering".into());
            };
            p.paused = !p.paused;
            Ok(json!({ "paused": p.paused }))
        }
        _ => unreachable!("routed by execute_inner"),
    }
}

/// `State` report of the render and the Rendered Frame Window.
pub fn state_json(state: &AppState) -> Value {
    let r = &state.rendered;
    let v = render_viewport(state);
    json!({
        "open": r.open,
        "image": r.image.as_ref().map(|i| [i.width(), i.height()]),
        "title": r.title,
        "viewport": v,
        "viewport_name": viewport_name(state, v),
        "lock_to_viewport": r.lock_to_viewport,
        "iterative": r.iterative,
        "edit_region": r.edit_region,
        "rendering": r.progress.map(|p| json!({ "done": p.done, "total": p.total, "paused": p.paused })),
        "pending": state.pending_render.is_some(),
        "clones": r.clones.len(),
        "settings": settings_json(&state.scene.render),
    })
}
