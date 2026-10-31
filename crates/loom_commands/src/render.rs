//! Rendering: Render (Production / Iterative / Last), Render Setup and the
//! Rendered Frame Window. The command layer has no GPU: a render records a
//! request that the UI's renderer works through over several frames,
//! putting the image into `AppState::rendered`.

use crate::command::Command as C;
use crate::state::RenderRequest;
use crate::{AppState, Dialog};
use loom_scene::render::{numbered_path, RenderArea, RenderSettings, TimeOutput};
use loom_scene::Change;
use serde_json::{json, Value};
use std::sync::Arc;

/// Largest output side in pixels.
pub const MAX_RENDER_SIZE: u32 = 8192;
/// Image files Save Image writes, by extension.
pub const IMAGE_FORMATS: &[&str] = &["png", "jpg", "jpeg", "bmp", "tga", "tif", "tiff", "exr", "hdr"];
/// Bitmap formats maps and backgrounds load from (GIF read only).
pub const IMAGE_READ_FORMATS: &[&str] = &["png", "jpg", "jpeg", "bmp", "tga", "tif", "tiff", "exr", "hdr", "gif"];

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
    /// Bucket passes finished and in total.
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
    /// Time Output renders more than the current frame: the frames in turn.
    pub sequence: Option<Sequence>,
    /// Render Message Window: its log and whether it shows.
    pub messages: Vec<RenderMessage>,
    pub messages_open: bool,
    /// Render Shortcuts toolbar presets A, B, C (for the session).
    pub shortcuts: [Option<Box<RenderSettings>>; 3],
    pub ram_player: RamPlayer,
}

/// Rendering > RAM Player: two image channels, A and B, compared side by
/// side with a split line (A left of it, B right).
#[derive(Debug, Clone)]
pub struct RamPlayer {
    pub open: bool,
    pub channels: [Option<(String, Arc<image::RgbaImage>)>; 2],
    /// Split position, a fraction of the width (A left, B right).
    pub split: f32,
    /// Both channels show (off: only A).
    pub compare: bool,
    /// Display the alpha channel as grey.
    pub alpha: bool,
    /// Bumped whenever what the window shows changes.
    pub version: u64,
}

impl Default for RamPlayer {
    fn default() -> Self {
        Self { open: false, channels: [None, None], split: 0.5, compare: true, alpha: false, version: 0 }
    }
}

impl RamPlayer {
    /// What the RAM Player shows: A, with B right of the split line when
    /// comparing (each channel at its own size, the canvas fitting both).
    pub fn composed(&self) -> Option<image::RgbaImage> {
        let a = self.channels[0].as_ref().map(|c| &c.1);
        let b = self.channels[1].as_ref().map(|c| &c.1).filter(|_| self.compare);
        let (w, h) = match (a, b) {
            (None, None) => return None,
            (Some(a), None) => a.dimensions(),
            (None, Some(b)) => b.dimensions(),
            (Some(a), Some(b)) => (a.width().max(b.width()), a.height().max(b.height())),
        };
        let split = (self.split.clamp(0.0, 1.0) * w as f32).round() as u32;
        let ch = Channels { alpha: self.alpha, ..Default::default() };
        Some(image::RgbaImage::from_fn(w, h, |x, y| {
            let from = if b.is_some() && (x >= split || a.is_none()) { b } else { a };
            match from.filter(|i| x < i.width() && y < i.height()) {
                Some(i) => image::Rgba(ch.display(i.get_pixel(x, y).0)),
                None => image::Rgba([0, 0, 0, 255]),
            }
        }))
    }
}

/// A Production render of a frame sequence (Time Output other than Single).
#[derive(Debug, Clone)]
pub struct Sequence {
    pub frames: Vec<i32>,
    /// The frame rendering now, an index into `frames`.
    pub index: usize,
    /// What every frame renders; its `save_path` is the output file before numbering.
    pub request: RenderRequest,
    /// The sequence's first frame (File Number Base counts from it).
    pub first: i32,
    /// The current frame before the sequence, set back when it ends.
    pub restore: i32,
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
            | C::RenderTimeOutput { .. }
            | C::RenderOptions { .. }
            | C::RaytracerSettings { .. }
            | C::AdvancedLighting { .. }
            | C::RendererOptions { .. }
            | C::SetMotionBlur { .. }
            | C::RenderMessageWindow
            | C::RamPlayer
            | C::RamPlayerOpen { .. }
            | C::RamPlayerSave { .. }
            | C::RamPlayerView { .. }
            | C::SaveRenderPreset { .. }
            | C::LoadRenderPreset { .. }
            | C::RenderShortcut { .. }
            | C::AddRenderElement { .. }
            | C::DeleteRenderElement { .. }
            | C::RenderElementParameters { .. }
            | C::RenderElements { .. }
            | C::ClearRenderMessages
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

fn ranged(name: &str, v: f32, lo: f32, hi: f32) -> Result<f32, String> {
    if v.is_finite() && (lo..=hi).contains(&v) {
        Ok(v)
    } else {
        Err(format!("{name} {v}: {lo} to {hi}"))
    }
}

fn check_u32(name: &str, v: u32, lo: u32, hi: u32) -> Result<u32, String> {
    if (lo..=hi).contains(&v) {
        Ok(v)
    } else {
        Err(format!("{name} {v}: {lo} to {hi}"))
    }
}

fn color01(name: &str, c: [f32; 3]) -> Result<[f32; 3], String> {
    if c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)) {
        Ok(c)
    } else {
        Err(format!("{name} {c:?}: sRGB 0 to 1"))
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

/// Starts the frames of a Time Output sequence (`req.save_path` is the
/// output before numbering). Image File List writes its .ifl first, of all
/// the frames; Skip Existing Images then leaves out the files already there.
fn start_sequence(state: &mut AppState, req: RenderRequest, mut frames: Vec<i32>) -> Result<Value, String> {
    if state.rendered.progress.is_some() || state.pending_render.is_some() {
        return Err("a render is in progress".into());
    }
    let (lo, hi) = state.frame_range;
    if let Some(f) = frames.iter().find(|f| !(lo..=hi).contains(*f)) {
        return Err(format!("frame {f} is outside the time range {lo} to {hi}"));
    }
    let first = frames[0];
    let s = &req.settings;
    if let Some(path) = &req.save_path {
        if s.image_file_list {
            write_image_file_list(path, frames.iter().map(|f| s.file_number(*f, first)))?;
        }
        if s.skip_existing {
            frames.retain(|f| !std::path::Path::new(&numbered_path(path, s.file_number(*f, first))).exists());
            if frames.is_empty() {
                state.status_prompt = "Render: every frame's file exists (Skip Existing Images)".into();
                return Ok(json!({ "skipped": true, "frames": 0 }));
            }
        }
    }
    let restore = state.current_frame;
    state.rendered.sequence = Some(Sequence { frames, index: 0, request: req, first, restore });
    start_sequence_frame(state)
}

/// The .ifl Put Image File List writes next to the output: a sequence's file names, one per line.
pub fn image_file_list_path(path: &str) -> String {
    std::path::Path::new(path).with_extension("ifl").to_string_lossy().into_owned()
}

fn write_image_file_list(path: &str, numbers: impl Iterator<Item = i32>) -> Result<(), String> {
    let mut text = String::new();
    for n in numbers {
        let file = numbered_path(path, n);
        let name = std::path::Path::new(&file).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or(file);
        text.push_str(&name);
        text.push('\n');
    }
    let ifl = image_file_list_path(path);
    std::fs::write(&ifl, text).map_err(|e| format!("{ifl}: {e}"))
}

fn start_sequence_frame(state: &mut AppState) -> Result<Value, String> {
    let Some(seq) = state.rendered.sequence.as_ref() else { return Err("no sequence".into()) };
    let (frame, index, count) = (seq.frames[seq.index], seq.index, seq.frames.len());
    let mut req = seq.request.clone();
    req.save_path = req.save_path.map(|p| numbered_path(&p, req.settings.file_number(frame, seq.first)));
    let path = req.save_path.clone();
    crate::execute::set_frame(state, frame)?;
    match start(state, req) {
        Ok(mut out) => {
            out["frame"] = json!(frame);
            out["sequence"] = json!([index + 1, count]);
            out["path"] = json!(path);
            state.status_prompt = format!("Rendering frame {frame} ({} of {count})", index + 1);
            Ok(out)
        }
        Err(e) => {
            end_sequence(state);
            Err(e)
        }
    }
}

/// After a sequence frame rendered: the next one starts (None: the
/// sequence is over, or there is none, and the current frame is back).
pub fn sequence_next(state: &mut AppState) -> Option<Result<Value, String>> {
    let seq = state.rendered.sequence.as_mut()?;
    seq.index += 1;
    if seq.index >= seq.frames.len() {
        end_sequence(state);
        return None;
    }
    Some(start_sequence_frame(state))
}

/// Ends a sequence (finished, cancelled or failed): the current frame goes back.
pub fn end_sequence(state: &mut AppState) {
    if let Some(seq) = state.rendered.sequence.take() {
        let _ = crate::execute::set_frame(state, seq.restore);
    }
}

/// Video Color Check: whether an sRGB pixel's composite NTSC signal
/// (luma plus the chroma amplitude, in YIQ) goes past 120 IRE or below -20.
pub fn ntsc_unsafe(p: [u8; 3]) -> bool {
    let [r, g, b] = p.map(|c| c as f32 / 255.0);
    let y = 0.299 * r + 0.587 * g + 0.114 * b;
    let i = 0.596 * r - 0.274 * g - 0.322 * b;
    let q = 0.211 * r - 0.523 * g + 0.312 * b;
    let c = (i * i + q * q).sqrt();
    y + c > 1.2 || y - c < -0.2
}

/// Video Color Check (Flag with black): the unsafe pixels turn black.
/// Returns how many there were.
pub fn video_color_check(img: &mut image::RgbaImage) -> usize {
    let mut n = 0;
    for p in img.pixels_mut() {
        if p.0[3] > 0 && ntsc_unsafe([p.0[0], p.0[1], p.0[2]]) {
            p.0[0] = 0;
            p.0[1] = 0;
            p.0[2] = 0;
            n += 1;
        }
    }
    n
}

/// Super Black: geometry (alpha above 0) is no darker than `threshold` on
/// any channel; the background (alpha 0) keeps its colour.
pub fn super_black(img: &mut image::RgbaImage, threshold: u8) {
    for p in img.pixels_mut() {
        if p.0[3] > 0 {
            for c in &mut p.0[..3] {
                *c = (*c).max(threshold);
            }
        }
    }
}

/// Render Message Window entry.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderMessage {
    pub level: MessageLevel,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum MessageLevel {
    Info,
    Warning,
    Error,
}

impl MessageLevel {
    pub fn label(self) -> &'static str {
        match self {
            MessageLevel::Info => "INFO",
            MessageLevel::Warning => "WARNING",
            MessageLevel::Error => "ERROR",
        }
    }
}

/// Most entries the Render Message Window keeps (the oldest go first).
pub const MAX_MESSAGES: usize = 1000;

/// Adds an entry to the Render Message Window; a warning or error opens it.
pub fn message(state: &mut AppState, level: MessageLevel, text: impl Into<String>) {
    let m = &mut state.rendered.messages;
    m.push(RenderMessage { level, text: text.into() });
    if m.len() > MAX_MESSAGES {
        m.remove(0);
    }
    if level != MessageLevel::Info {
        state.rendered.messages_open = true;
    }
}

/// Writes `img` to `path`, the format from its extension.
pub fn save_image(img: &image::RgbaImage, path: &str) -> Result<(), String> {
    let ext = std::path::Path::new(path).extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    let err = |e: image::ImageError| format!("{path}: {e}");
    match ext.as_str() {
        "png" | "tga" | "tif" | "tiff" => img.save(path).map_err(err),
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
        // Radiance HDR: linear RGB floats, no alpha.
        "hdr" => {
            let lin = |c: u8| {
                let x = c as f32 / 255.0;
                if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
            };
            let mut f = image::Rgb32FImage::new(img.width(), img.height());
            for (o, p) in f.pixels_mut().zip(img.pixels()) {
                o.0 = [lin(p.0[0]), lin(p.0[1]), lin(p.0[2])];
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
            if !iterative && settings.time_output != TimeOutput::Single {
                let frames = settings.output_frames(state.frame_range, state.current_frame)?;
                if save_path.is_none() {
                    message(state, MessageLevel::Warning, "Rendering a sequence without an output file: the frames are not saved (Render Output > Files...)");
                }
                return start_sequence(state, RenderRequest { viewport: render_viewport(state), settings, selection: Vec::new(), save_path, iterative, preview: false }, frames);
            }
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
        C::BevyRendererSettings {
            antialiasing,
            filter,
            filter_size,
            supersampling,
            passes,
            shadows,
            shadow_map_size,
            area_shadows,
            background,
            ambient,
            ambient_occlusion,
            ao_quality,
            ao_radius,
            reflections,
            sky_lighting,
            sky_intensity,
            ground,
            tonemapper,
            exposure,
            dither,
        } => {
            let mut s = state.scene.render.clone();
            let b = &mut s.bevy;
            let ranged = |name: &str, v: f32, lo: f32, hi: f32| if v.is_finite() && (lo..=hi).contains(&v) { Ok(v) } else { Err(format!("{name} {v}: {lo} to {hi}")) };
            if let Some(v) = antialiasing {
                b.antialiasing = v;
            }
            if let Some(v) = filter {
                b.filter = v;
            }
            if let Some(v) = filter_size {
                b.filter_size = ranged("filter size", v, 1.0, 10.0)?;
            }
            if let Some(v) = passes {
                if !(1..=64).contains(&v) {
                    return Err(format!("passes {v}: 1 to 64"));
                }
                b.passes = v;
            }
            if let Some(v) = area_shadows {
                b.area_shadows = v;
            }
            if let Some(v) = background {
                b.background = v;
            }
            if let Some(v) = ambient {
                b.ambient = v;
            }
            if let Some(v) = ambient_occlusion {
                b.ambient_occlusion = v;
            }
            if let Some(v) = ao_quality {
                b.ao_quality = v;
            }
            if let Some(v) = ao_radius {
                b.ao_radius = ranged("AO radius", v, 0.001, 1.0e6)?;
            }
            if let Some(v) = reflections {
                b.reflections = v;
            }
            if let Some(v) = sky_lighting {
                b.sky_lighting = v;
            }
            if let Some(v) = sky_intensity {
                b.sky_intensity = ranged("sky intensity", v, 0.0, 10.0)?;
            }
            if let Some(v) = ground {
                b.ground = ranged("ground", v, 0.0, 1.0)?;
            }
            if let Some(v) = tonemapper {
                b.tonemapper = v;
            }
            if let Some(v) = exposure {
                b.exposure = ranged("exposure", v, -10.0, 10.0)?;
            }
            if let Some(v) = dither {
                b.dither = v;
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
        C::RenderTimeOutput { time_output, range, frames, every_nth, file_number_base, skip_existing, image_file_list } => {
            let mut s = state.scene.render.clone();
            if let Some(v) = time_output {
                s.time_output = v;
            }
            if let Some(r) = range {
                if r[0] > r[1] {
                    return Err(format!("range {} to {}: the first frame is after the last", r[0], r[1]));
                }
                s.range = r;
            }
            if let Some(f) = frames {
                loom_scene::render::parse_frames(&f)?;
                s.frames = f;
            }
            if let Some(n) = every_nth {
                if !(1..=100_000).contains(&n) {
                    return Err(format!("every Nth frame {n}: 1 to 100000"));
                }
                s.every_nth = n;
            }
            if let Some(b) = file_number_base {
                if !(-99_999..=99_999).contains(&b) {
                    return Err(format!("file number base {b}: -99999 to 99999"));
                }
                s.file_number_base = b;
            }
            if let Some(v) = skip_existing {
                s.skip_existing = v;
            }
            if let Some(v) = image_file_list {
                s.image_file_list = v;
            }
            set_settings(state, "Render Setup", s)?;
            let s = &state.scene.render;
            Ok(json!({ "time_output": s.time_output, "range": s.range, "frames": s.frames, "every_nth": s.every_nth, "file_number_base": s.file_number_base, "skip_existing": s.skip_existing, "image_file_list": s.image_file_list }))
        }
        C::AddRenderElement { kind, name } => {
            let mut s = state.scene.render.clone();
            let name = name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()).unwrap_or_else(|| kind.label().to_owned());
            s.elements.push(loom_scene::render::RenderElement { kind, name, ..Default::default() });
            let index = s.elements.len() - 1;
            set_settings(state, "Add Render Element", s)?;
            Ok(json!({ "index": index, "elements": state.scene.render.elements.len() }))
        }
        C::DeleteRenderElement { index } => {
            let mut s = state.scene.render.clone();
            if index as usize >= s.elements.len() {
                return Err(format!("no render element {index}"));
            }
            s.elements.remove(index as usize);
            set_settings(state, "Delete Render Element", s)?;
            Ok(json!({ "elements": state.scene.render.elements.len() }))
        }
        C::RenderElementParameters { index, name, enabled, path, z_min, z_max } => {
            let mut s = state.scene.render.clone();
            let e = s.elements.get_mut(index as usize).ok_or_else(|| format!("no render element {index}"))?;
            if let Some(n) = name {
                let n = n.trim();
                if n.is_empty() {
                    return Err("an element needs a name".into());
                }
                e.name = n.to_owned();
            }
            if let Some(v) = enabled {
                e.enabled = v;
            }
            if let Some(p) = path {
                let p = p.trim();
                e.path = (!p.is_empty()).then(|| p.to_owned());
            }
            for (field, v) in [(&mut e.z_min, z_min), (&mut e.z_max, z_max)] {
                if let Some(v) = v {
                    if !v.is_finite() {
                        return Err(format!("Z depth {v}"));
                    }
                    *field = v;
                }
            }
            if e.z_max <= e.z_min {
                return Err(format!("Z Max {} must be beyond Z Min {}", e.z_max, e.z_min));
            }
            set_settings(state, "Render Element", s)?;
            Ok(serde_json::to_value(&state.scene.render.elements[index as usize]).unwrap_or(Value::Null))
        }
        C::RenderElements { active, display } => {
            let mut s = state.scene.render.clone();
            if let Some(v) = active {
                s.elements_active = v;
            }
            if let Some(v) = display {
                s.display_elements = v;
            }
            set_settings(state, "Render Elements", s)?;
            Ok(json!({ "active": state.scene.render.elements_active, "display": state.scene.render.display_elements }))
        }
        C::RaytracerSettings { enabled, max_depth, cutoff, max_depth_color, max_depth_background, antialias, samples, blur, self_reflect, exclude } => {
            let mut s = state.scene.render.clone();
            let r = &mut s.raytracer;
            if let Some(v) = enabled {
                r.enabled = v;
            }
            if let Some(v) = max_depth {
                r.max_depth = check_u32("maximum depth", v, 0, 100)?;
            }
            if let Some(v) = cutoff {
                r.cutoff = ranged("cutoff threshold", v, 0.0, 1.0)?;
            }
            if let Some(c) = max_depth_color {
                r.max_depth_color = Some(color01("max depth color", c)?);
            }
            if max_depth_background == Some(true) {
                r.max_depth_color = None;
            }
            if let Some(v) = antialias {
                r.antialias = v;
            }
            if let Some(v) = samples {
                r.samples = check_u32("samples", v, 2, 64)?;
            }
            if let Some(v) = blur {
                r.blur = ranged("blur", v, 0.0, 45.0)?;
            }
            if let Some(v) = self_reflect {
                r.self_reflect = v;
            }
            if let Some(ids) = exclude {
                if let Some(id) = ids.iter().find(|id| state.scene.node(loom_scene::NodeId(**id)).is_none()) {
                    return Err(format!("exclude: no object {id}"));
                }
                r.exclude = ids;
            }
            set_settings(state, "Raytracer Settings", s)?;
            Ok(serde_json::to_value(&state.scene.render.raytracer).unwrap_or(Value::Null))
        }
        C::AdvancedLighting { plugin, active, use_advanced, global_multiplier, object_multiplier, sky_lights_on, sky_lights, color_bleed, rays_per_sample, color_filter, extra_ambient, filter_size, ray_bias, bounces, cone_angle } => {
            let mut s = state.scene.render.clone();
            let a = &mut s.advanced;
            if let Some(v) = plugin {
                a.plugin = v;
            }
            if let Some(v) = active {
                a.active = v;
            }
            if let Some(v) = use_advanced {
                a.use_advanced = v;
            }
            if let Some(v) = global_multiplier {
                a.global_multiplier = ranged("global multiplier", v, 0.0, 1000.0)?;
            }
            if let Some(v) = object_multiplier {
                a.object_multiplier = ranged("object multiplier", v, 0.0, 1000.0)?;
            }
            if let Some(v) = sky_lights_on {
                a.sky_lights_on = v;
            }
            if let Some(v) = sky_lights {
                a.sky_lights = ranged("sky lights", v, 0.0, 1000.0)?;
            }
            if let Some(v) = color_bleed {
                a.color_bleed = ranged("color bleed", v, 0.0, 1000.0)?;
            }
            if let Some(v) = rays_per_sample {
                a.rays_per_sample = check_u32("rays/sample", v, 1, 10000)?;
            }
            if let Some(c) = color_filter {
                a.color_filter = color01("color filter", c)?;
            }
            if let Some(c) = extra_ambient {
                a.extra_ambient = color01("extra ambient", c)?;
            }
            if let Some(v) = filter_size {
                a.filter_size = ranged("filter size", v, 0.0, 100.0)?;
            }
            if let Some(v) = ray_bias {
                a.ray_bias = ranged("ray bias", v, 0.0, 100.0)?;
            }
            if let Some(v) = bounces {
                a.bounces = check_u32("bounces", v, 0, 10)?;
            }
            if let Some(v) = cone_angle {
                a.cone_angle = ranged("cone angle", v, 33.0, 90.0)?;
            }
            set_settings(state, "Advanced Lighting", s)?;
            Ok(serde_json::to_value(&state.scene.render.advanced).unwrap_or(Value::Null))
        }
        C::RendererOptions { area_as_points, auto_reflect, reflect_iterations, filter_maps, disable_all_samplers, global_supersampler, object_motion_blur, object_blur_duration, object_blur_samples, object_blur_subdivisions, image_motion_blur, image_blur_duration, image_blur_environment, color_range } => {
            let mut s = state.scene.render.clone();
            if let Some(v) = area_as_points {
                s.area_as_points = v;
            }
            let b = &mut s.bevy;
            if let Some(v) = auto_reflect {
                b.auto_reflect = v;
            }
            if let Some(v) = reflect_iterations {
                b.reflect_iterations = check_u32("rendering iterations", v, 1, 10)?;
            }
            if let Some(v) = filter_maps {
                b.filter_maps = v;
            }
            if let Some(v) = disable_all_samplers {
                b.disable_all_samplers = v;
            }
            if let Some(v) = global_supersampler {
                b.global_supersampler = v;
            }
            if let Some(v) = object_motion_blur {
                b.object_motion_blur = v;
            }
            if let Some(v) = object_blur_duration {
                b.object_blur_duration = ranged("object blur duration", v, 0.0, 100.0)?;
            }
            if let Some(v) = object_blur_samples {
                b.object_blur_samples = check_u32("object blur samples", v, 1, 32)?;
            }
            if let Some(v) = object_blur_subdivisions {
                b.object_blur_subdivisions = check_u32("duration subdivisions", v, 1, 32)?;
            }
            if let Some(v) = image_motion_blur {
                b.image_motion_blur = v;
            }
            if let Some(v) = image_blur_duration {
                b.image_blur_duration = ranged("image blur duration", v, 0.0, 100.0)?;
            }
            if let Some(v) = image_blur_environment {
                b.image_blur_environment = v;
            }
            if let Some(v) = color_range {
                b.color_range = v;
            }
            set_settings(state, "Render Setup", s)?;
            let b = &state.scene.render.bevy;
            Ok(json!({
                "area_as_points": state.scene.render.area_as_points, "auto_reflect": b.auto_reflect, "reflect_iterations": b.reflect_iterations, "filter_maps": b.filter_maps,
                "disable_all_samplers": b.disable_all_samplers, "global_supersampler": b.global_supersampler,
                "object_motion_blur": b.object_motion_blur, "object_blur_duration": b.object_blur_duration, "object_blur_samples": b.object_blur_samples, "object_blur_subdivisions": b.object_blur_subdivisions,
                "image_motion_blur": b.image_motion_blur, "image_blur_duration": b.image_blur_duration, "image_blur_environment": b.image_blur_environment, "color_range": b.color_range,
            }))
        }
        C::SetMotionBlur { kind, enabled, multiplier } => {
            if let Some(m) = multiplier {
                ranged("multiplier", m, 0.0, 10000.0)?;
            }
            let mut changes = Vec::new();
            let mut ids: Vec<loom_scene::NodeId> = state.selection.iter().copied().collect();
            ids.sort();
            for id in ids {
                let Some(n) = state.scene.node(id) else { continue };
                let before = n.props();
                let mut after = before.clone();
                if let Some(k) = kind {
                    after.render.motion_blur = k;
                }
                if let Some(v) = enabled {
                    after.render.motion_blur_enabled = v;
                }
                if let Some(m) = multiplier {
                    after.render.motion_blur_multiplier = m;
                }
                if after != before {
                    changes.push(Change::SetNodeProps { id, before: Box::new(before), after: Box::new(after) });
                }
            }
            if !changes.is_empty() {
                state.undo.commit("Motion Blur", Change::Group(changes), &mut state.scene)?;
            }
            Ok(json!({}))
        }
        C::RenderOptions { video_color_check, super_black, super_black_threshold, force_wireframe, wire_thickness } => {
            let mut s = state.scene.render.clone();
            if let Some(v) = force_wireframe {
                s.force_wireframe = v;
            }
            if let Some(t) = wire_thickness {
                if !(t.is_finite() && (0.1..=100.0).contains(&t)) {
                    return Err(format!("wire thickness {t}: 0.1 to 100 pixels"));
                }
                s.wire_thickness = t;
            }
            if let Some(v) = video_color_check {
                s.video_color_check = v;
            }
            if let Some(v) = super_black {
                s.super_black = v;
            }
            if let Some(t) = super_black_threshold {
                s.super_black_threshold = u8::try_from(t).map_err(|_| format!("super black threshold {t}: 0 to 255"))?;
            }
            set_settings(state, "Render Setup", s)?;
            let s = &state.scene.render;
            Ok(json!({ "video_color_check": s.video_color_check, "super_black": s.super_black, "super_black_threshold": s.super_black_threshold, "force_wireframe": s.force_wireframe, "wire_thickness": s.wire_thickness }))
        }
        C::RamPlayer => {
            let r = &mut state.rendered.ram_player;
            r.open = !r.open;
            Ok(json!({ "open": r.open }))
        }
        C::RamPlayerOpen { channel, path } => {
            let c = channel as usize;
            if c > 1 {
                return Err(format!("RAM Player channel {channel}: 0 (A) or 1 (B)"));
            }
            let (name, img) = match path {
                Some(p) => {
                    let img = image::open(&p).map_err(|e| format!("{p}: {e}"))?.to_rgba8();
                    let name = std::path::Path::new(&p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(p.clone());
                    (name, Arc::new(img))
                }
                None => {
                    let img = state.rendered.image.clone().ok_or("the Rendered Frame Window has no image")?;
                    (state.rendered.title.clone(), img)
                }
            };
            let (w, h) = img.dimensions();
            let r = &mut state.rendered.ram_player;
            r.channels[c] = Some((name, img));
            r.open = true;
            r.version += 1;
            Ok(json!({ "channel": channel, "width": w, "height": h }))
        }
        C::RamPlayerSave { channel, path } => {
            let img = state.rendered.ram_player.channels.get(channel as usize).and_then(|c| c.as_ref()).map(|c| c.1.clone()).ok_or_else(|| format!("RAM Player channel {channel} is empty"))?;
            save_image(&img, &path)?;
            Ok(json!({ "path": path }))
        }
        C::RamPlayerView { split, compare, alpha, clear } => {
            let r = &mut state.rendered.ram_player;
            if let Some(v) = split {
                if !(0.0..=1.0).contains(&v) {
                    return Err(format!("split {v}: 0 to 1"));
                }
                r.split = v;
            }
            if let Some(v) = compare {
                r.compare = v;
            }
            if let Some(v) = alpha {
                r.alpha = v;
            }
            if let Some(c) = clear {
                *r.channels.get_mut(c as usize).ok_or_else(|| format!("RAM Player channel {c}: 0 (A) or 1 (B)"))? = None;
            }
            r.version += 1;
            Ok(json!({ "split": r.split, "compare": r.compare, "alpha": r.alpha }))
        }
        C::SaveRenderPreset { path } => {
            let text = serde_json::to_string_pretty(&state.scene.render).map_err(|e| e.to_string())?;
            std::fs::write(&path, text).map_err(|e| format!("{path}: {e}"))?;
            Ok(json!({ "path": path }))
        }
        C::LoadRenderPreset { path, common, renderer, elements } => {
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
            let from: RenderSettings = serde_json::from_str(&text).map_err(|e| format!("{path}: not a render preset: {e}"))?;
            let mut s = state.scene.render.clone();
            s.take_categories(&from, common.unwrap_or(true), renderer.unwrap_or(true), elements.unwrap_or(true));
            check_size(s.width, s.height)?;
            set_settings(state, "Load Render Preset", s)?;
            Ok(settings_json(&state.scene.render))
        }
        C::RenderShortcut { slot, store } => {
            let i = slot as usize;
            if i >= state.rendered.shortcuts.len() {
                return Err(format!("render shortcut {slot}: 0 to 2 (A, B, C)"));
            }
            if store == Some(true) {
                state.rendered.shortcuts[i] = Some(Box::new(state.scene.render.clone()));
                return Ok(json!({ "slot": slot, "stored": true }));
            }
            let Some(p) = state.rendered.shortcuts[i].clone() else {
                return Err(format!("render shortcut {} is empty", ["A", "B", "C"][i]));
            };
            let mut s = state.scene.render.clone();
            s.take_categories(&p, true, true, true);
            set_settings(state, "Render Shortcut", s)?;
            Ok(json!({ "slot": slot, "applied": true }))
        }
        C::RenderMessageWindow => {
            state.rendered.messages_open = !state.rendered.messages_open;
            Ok(json!({ "open": state.rendered.messages_open }))
        }
        C::ClearRenderMessages => {
            state.rendered.messages.clear();
            Ok(json!({ "cleared": true }))
        }
        C::CancelRender => {
            if state.pending_render.take().is_some() {
                end_sequence(state);
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
        "sequence": r.sequence.as_ref().map(|q| json!({ "frame": q.frames[q.index], "index": q.index, "count": q.frames.len() })),
        "clones": r.clones.len(),
        "settings": settings_json(&state.scene.render),
    })
}
