//! Around the renders: the output files' format options and gamma, the
//! Output Size formats, the renderer's log file, View Image File, the
//! Rendered Frame Window's G-buffer channels and Toggle UI, Batch Render,
//! ActiveShade and the Panorama Exporter.

use crate::command::Command as C;
use crate::render::{message, render_viewport, set_settings, start, start_sequence, MessageLevel};
use crate::state::RenderRequest;
use crate::{AppState, Dialog};
use loom_scene::render::{film_format, BatchJob, RenderArea, RenderSettings, TimeOutput, DEFAULT_APERTURE};
use loom_scene::{Change, NodeId};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

pub fn handles(c: &C) -> bool {
    matches!(
        c,
        C::RenderOutputSetup { .. }
            | C::GammaSettings { .. }
            | C::OutputSizeFormat { .. }
            | C::RenderMessageLog { .. }
            | C::ViewImageFile { .. }
            | C::ViewImageFileStep { .. }
            | C::RenderedFrameWindowToggleUi
            | C::RenderedFrameWindowGBuffer { .. }
            | C::SetObjectChannel { .. }
            | C::BatchRender
            | C::BatchRenderAdd { .. }
            | C::BatchRenderDelete { .. }
            | C::BatchRenderTask { .. }
            | C::BatchRenderRun { .. }
            | C::ActiveShadeFloater
            | C::ActiveShadeCommand { .. }
            | C::PanoramaExport { .. }
            | C::PanoramaViewer { .. }
            | C::RenderElementOptions { .. }
            | C::AmbientOcclusionMapSettings { .. }
    )
}

// ---- Renderer log file ----

/// Write Messages to File: the message goes to the .log file too (the
/// session's first one starts the file unless Append to File is on).
pub fn log_message(state: &mut AppState, level: MessageLevel, text: &str) {
    let p = &state.prefs;
    let Some(path) = p.render_log_path.clone().filter(|_| p.render_log) else { return };
    let append = p.render_log_append || state.rendered.log_started;
    state.rendered.log_started = true;
    use std::io::Write;
    let file = std::fs::OpenOptions::new().create(true).write(true).append(append).truncate(!append).open(&path);
    if let Ok(mut f) = file {
        let _ = writeln!(f, "{}: {}", level.label(), text);
    }
}

// ---- G-buffer ----

/// What the CPU ray caster found per pixel of the last Production render.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GBuffer {
    pub size: [u32; 2],
    /// Camera depth of the nearest surface (None: nothing).
    pub depth: Vec<Option<f32>>,
    /// The surface's G-buffer Object Channel.
    pub object: Vec<u32>,
    /// The surface's node id (0: none).
    pub node: Vec<u64>,
    /// The surface's world normal.
    pub normal: Vec<[f32; 3]>,
}

/// Rendered Frame Window channel list entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GChannel {
    ZDepth,
    ObjectId,
    NodeRenderId,
    Normal,
    Coverage,
}

impl GChannel {
    pub const ALL: [GChannel; 5] = [GChannel::ZDepth, GChannel::ObjectId, GChannel::NodeRenderId, GChannel::Normal, GChannel::Coverage];

    pub fn label(self) -> &'static str {
        match self {
            GChannel::ZDepth => "Z",
            GChannel::ObjectId => "Object",
            GChannel::NodeRenderId => "Node Render ID",
            GChannel::Normal => "Normal",
            GChannel::Coverage => "Coverage",
        }
    }
}

/// The colour an ID is always shown in (0: black).
pub fn id_color(id: u64) -> [u8; 3] {
    if id == 0 {
        return [0, 0, 0];
    }
    let h = (id as f64 * 0.618_033_988_749_895).fract() as f32 * 6.0;
    let (s, v) = (0.8f32, 1.0f32);
    let i = h.floor();
    let f = h - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match i as i32 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [r, g, b].map(|c| (c * 255.0).round() as u8)
}

impl GBuffer {
    /// Channel `ch` as an image: Z near white to far black over the depths
    /// in the frame; Object and Node Render ID in their IDs' colours;
    /// normals as RGB; Coverage the render's alpha (`color`).
    pub fn image(&self, ch: GChannel, color: Option<&image::RgbaImage>) -> image::RgbaImage {
        let [w, h] = self.size;
        let mut img = image::RgbaImage::new(w, h);
        let (lo, hi) = self.depth.iter().flatten().fold((f32::MAX, f32::MIN), |(a, b), d| (a.min(*d), b.max(*d)));
        for (i, p) in img.pixels_mut().enumerate() {
            let hit = self.depth.get(i).copied().flatten();
            p.0 = match ch {
                GChannel::ZDepth => match hit {
                    Some(z) => {
                        let t = if hi > lo { (z - lo) / (hi - lo) } else { 0.0 };
                        let g = ((1.0 - t) * 255.0).round() as u8;
                        [g, g, g, 255]
                    }
                    None => [0, 0, 0, 255],
                },
                GChannel::ObjectId => {
                    let c = if hit.is_some() { id_color(self.object[i] as u64) } else { [0; 3] };
                    [c[0], c[1], c[2], 255]
                }
                GChannel::NodeRenderId => {
                    let c = if hit.is_some() { id_color(self.node[i]) } else { [0; 3] };
                    [c[0], c[1], c[2], 255]
                }
                GChannel::Normal => match hit {
                    Some(_) => {
                        let n = self.normal[i];
                        let c = n.map(|v| ((v * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0).round() as u8);
                        [c[0], c[1], c[2], 255]
                    }
                    None => [0, 0, 0, 255],
                },
                GChannel::Coverage => {
                    let a = color.and_then(|c| c.as_raw().get(i * 4 + 3).copied()).unwrap_or(if hit.is_some() { 255 } else { 0 });
                    [a, a, a, 255]
                }
            };
        }
        img
    }
}

// ---- View Image File ----

/// View Image File: the files the Rendered Frame Window steps through.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageViewer {
    pub files: Vec<String>,
    pub index: usize,
}

/// The trailing frame number of a file stem: ("anim", "0012").
fn split_number(stem: &str) -> Option<(&str, &str)> {
    let digits = stem.len() - stem.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    (digits > 0).then(|| stem.split_at(stem.len() - digits))
}

/// The files View Image File shows for `path`: a .ifl's list, with
/// `sequence` the numbered files beside it (same name and extension), else
/// the file alone; and the index of `path` among them.
pub fn sequence_files(path: &str, sequence: bool) -> Result<(Vec<String>, usize), String> {
    let p = std::path::Path::new(path);
    let dir = p.parent().map(|d| d.to_path_buf()).unwrap_or_default();
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if ext == "ifl" {
        let text = std::fs::read_to_string(p).map_err(|e| format!("{path}: {e}"))?;
        let files: Vec<String> = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with(';')).map(|l| dir.join(l).to_string_lossy().into_owned()).collect();
        if files.is_empty() {
            return Err(format!("{path}: lists no images"));
        }
        return Ok((files, 0));
    }
    if !p.is_file() {
        return Err(format!("{path}: no such file"));
    }
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let Some((prefix, _)) = split_number(stem).filter(|_| sequence) else {
        return Ok((vec![path.to_owned()], 0));
    };
    let mut found: Vec<(i64, String)> = std::fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
        .filter_map(|e| {
            let f = e.path();
            let x = f.extension().and_then(|e| e.to_str())?.to_ascii_lowercase();
            let s = f.file_stem()?.to_str()?.to_owned();
            let (pre, num) = split_number(&s)?;
            (x == ext && pre == prefix).then(|| Some((num.parse::<i64>().ok()?, f.to_string_lossy().into_owned())))?
        })
        .collect();
    found.sort();
    let files: Vec<String> = found.into_iter().map(|(_, f)| f).collect();
    let here = files.iter().position(|f| std::path::Path::new(f) == p).unwrap_or(0);
    Ok((files, here))
}

/// Shows the viewer's current file in the Rendered Frame Window.
fn show_viewed(state: &mut AppState) -> Result<Value, String> {
    let v = state.rendered.viewer.as_ref().ok_or("no image file is being viewed")?;
    let (path, index, count) = (v.files[v.index].clone(), v.index, v.files.len());
    let img = image::open(&path).map_err(|e| format!("{path}: {e}"))?.to_rgba8();
    let (w, h) = img.dimensions();
    let r = &mut state.rendered;
    r.set_image(Some(Arc::new(img)));
    let name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(path.clone());
    r.title = if count > 1 { format!("{name} ({} of {count}), {w}x{h}", index + 1) } else { format!("{name}, {w}x{h}") };
    r.open = true;
    r.gbuffer = None;
    r.gchannel = None;
    Ok(json!({ "path": path, "index": index, "count": count, "width": w, "height": h }))
}

// ---- Batch Render ----

/// A Batch Render in progress: the tasks to render and the one rendering.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BatchRun {
    pub tasks: Vec<usize>,
    pub pos: usize,
    pub done: usize,
}

/// Batch Render dialog state (the queue itself is in `RenderSettings::batch`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BatchRender {
    pub open: bool,
    pub selected: usize,
    pub run: Option<BatchRun>,
    /// The Batch Render Warning dialog: the tasks it lists.
    pub warning: Option<Vec<String>>,
    pub panorama: Option<PanoramaRun>,
}

/// The settings task `job` renders with: Render Setup, its preset, then Override Preset.
pub fn task_settings(base: &RenderSettings, job: &BatchJob) -> Result<RenderSettings, String> {
    let mut s = base.clone();
    if let Some(p) = &job.preset {
        let text = std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?;
        let from: RenderSettings = serde_json::from_str(&text).map_err(|e| format!("{p}: not a render preset: {e}"))?;
        s.take_categories(&from, true, true, true);
    }
    if job.override_preset {
        s.width = job.width;
        s.height = job.height;
        s.pixel_aspect = job.pixel_aspect;
        s.time_output = if job.frames[0] == job.frames[1] { TimeOutput::Frames } else { TimeOutput::Range };
        s.range = job.frames;
        s.frames = job.frames[0].to_string();
        s.every_nth = 1;
        s.file_number_base = 0;
    }
    s.save_file = job.output.is_some();
    s.output_path = job.output.clone();
    Ok(s)
}

fn start_task(state: &mut AppState, index: usize) -> Result<Value, String> {
    let job = state.scene.render.batch.get(index).cloned().ok_or_else(|| format!("no batch task {index}"))?;
    let settings = task_settings(&state.scene.render, &job)?;
    let camera = match job.camera {
        Some(id) if state.scene.node(NodeId(id)).is_none() => return Err(format!("batch task {}: its camera is gone", job.name)),
        c => c.map(NodeId),
    };
    message(state, MessageLevel::Info, format!("Batch Render: task {}", job.name));
    let req = RenderRequest { viewport: render_viewport(state), settings: settings.clone(), save_path: job.output.clone(), camera, ..Default::default() };
    if settings.time_output != TimeOutput::Single {
        let frames = settings.output_frames(state.frame_range, state.current_frame)?;
        // One frame other than the current one is a sequence of one (its file numbered).
        if frames.len() > 1 || frames[0] != state.current_frame {
            return start_sequence(state, req, frames);
        }
    }
    start(state, req)
}

/// Ends Batch Render and Panorama Exporter runs (a render failed or was cancelled).
pub fn end_runs(state: &mut AppState) {
    if state.rendered.batch.run.take().is_some() {
        message(state, MessageLevel::Warning, "Batch Render stopped");
    }
    state.rendered.batch.panorama = None;
}

/// After a render (and its sequence) finished: a Panorama Exporter face
/// is kept and the next starts, or a Batch Render's next task.
pub fn after_render(state: &mut AppState) -> Option<Result<Value, String>> {
    if state.rendered.batch.panorama.is_some() {
        return panorama_next(state);
    }
    let run = state.rendered.batch.run.as_mut()?;
    run.done += 1;
    run.pos += 1;
    if run.pos >= run.tasks.len() {
        let done = run.done;
        state.rendered.batch.run = None;
        message(state, MessageLevel::Info, format!("Batch Render: {done} task(s) rendered"));
        state.status_prompt = format!("Batch Render: {done} task(s) rendered");
        return None;
    }
    let index = run.tasks[run.pos];
    Some(start_task(state, index).inspect_err(|_| end_runs(state)))
}

// ---- ActiveShade ----

/// The ActiveShade window: the render viewport rendered again as the scene changes.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveShade {
    pub open: bool,
    pub auto_update: bool,
    pub toolbar: bool,
    /// Draw Region: only this rectangle renders again (fractions, y down).
    pub region: Option<[f32; 4]>,
    pub image: Option<Arc<image::RgbaImage>>,
    pub version: u64,
    /// The scene's history version the image shows.
    pub rendered_at: Option<u64>,
    pub renders: u32,
}

impl Default for ActiveShade {
    fn default() -> Self {
        Self { open: false, auto_update: true, toolbar: true, region: None, image: None, version: 0, rendered_at: None, renders: 0 }
    }
}

/// Starts an ActiveShade render of the render viewport (Draw Region: only the region).
pub fn active_shade_render(state: &mut AppState) -> Result<Value, String> {
    let mut settings = state.scene.render.clone();
    let a = &state.rendered.active_shade;
    match a.region {
        Some(r) => {
            settings.area = RenderArea::Region;
            settings.region = r;
        }
        None => settings.area = RenderArea::View,
    }
    // Interactive: one pass per bucket.
    settings.bevy.passes = 1;
    let at = state.undo.version();
    let req = RenderRequest { viewport: render_viewport(state), settings, active_shade: true, iterative: true, ..Default::default() };
    let out = start(state, req)?;
    state.rendered.active_shade.rendered_at = Some(at);
    Ok(out)
}

/// Whether ActiveShade should render again now (open, Auto Update, the scene changed, nothing rendering).
pub fn active_shade_stale(state: &AppState) -> bool {
    let a = &state.rendered.active_shade;
    a.open && a.auto_update && a.rendered_at != Some(state.undo.version()) && state.rendered.progress.is_none() && state.pending_render.is_none()
}

// ---- Panorama Exporter ----

/// A Panorama Exporter render: the cube faces rendered so far.
#[derive(Debug, Clone, PartialEq)]
pub struct PanoramaRun {
    pub camera: NodeId,
    pub size: u32,
    pub output: Option<String>,
    pub faces: Vec<Arc<image::RgbaImage>>,
}

/// Cube face `i` (0-5: +X, -X, +Y, -Y, +Z, -Z): its forward and up directions.
pub fn cube_face_axes(i: u8) -> ([f32; 3], [f32; 3]) {
    match i {
        0 => ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        1 => ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        2 => ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        3 => ([0.0, -1.0, 0.0], [0.0, 0.0, 1.0]),
        4 => ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        _ => ([0.0, 0.0, -1.0], [0.0, -1.0, 0.0]),
    }
}

/// The colour seen along `dir` (world, Z up) in the six cube faces (bilinear).
fn cube_sample(faces: &[Arc<image::RgbaImage>], dir: glam::Vec3) -> [f32; 4] {
    let a = dir.abs();
    let face = if a.x >= a.y && a.x >= a.z {
        if dir.x > 0.0 { 0 } else { 1 }
    } else if a.y >= a.z {
        if dir.y > 0.0 { 2 } else { 3 }
    } else if dir.z > 0.0 {
        4
    } else {
        5
    };
    let (f, u) = cube_face_axes(face);
    let (f, u) = (glam::Vec3::from(f), glam::Vec3::from(u));
    let r = f.cross(u);
    let d = dir / dir.dot(f);
    let img = &faces[face as usize];
    let (w, h) = (img.width() as f32, img.height() as f32);
    let x = ((d.dot(r) + 1.0) * 0.5 * w - 0.5).clamp(0.0, w - 1.0);
    let y = ((1.0 - d.dot(u)) * 0.5 * h - 0.5).clamp(0.0, h - 1.0);
    bilinear(img, x, y)
}

fn bilinear(img: &image::RgbaImage, x: f32, y: f32) -> [f32; 4] {
    let (x0, y0) = (x.floor() as u32, y.floor() as u32);
    let (x1, y1) = ((x0 + 1).min(img.width() - 1), (y0 + 1).min(img.height() - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let p = |x, y| img.get_pixel(x, y).0.map(|c| c as f32);
    let (a, b, c, d) = (p(x0, y0), p(x1, y0), p(x0, y1), p(x1, y1));
    std::array::from_fn(|k| (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy)
}

/// The direction of equirectangular position (u, v) (0-1, v down): the
/// centre looks along +X, the image's right turns right (toward -Y).
fn equirect_dir(u: f32, v: f32) -> glam::Vec3 {
    let lon = std::f32::consts::PI - u * std::f32::consts::TAU;
    let lat = std::f32::consts::FRAC_PI_2 - v * std::f32::consts::PI;
    glam::Vec3::new(lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin())
}

/// The six cube faces joined into a spherical panorama `2 * size` x `size`.
pub fn cube_to_equirect(faces: &[Arc<image::RgbaImage>], size: u32) -> image::RgbaImage {
    let (w, h) = (size * 2, size);
    image::RgbaImage::from_fn(w, h, |x, y| {
        let c = cube_sample(faces, equirect_dir((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32));
        image::Rgba(c.map(|v| v.round().clamp(0.0, 255.0) as u8))
    })
}

/// The Panorama viewer: a perspective view `w` x `h` into the spherical
/// panorama `pano`, looking `yaw` degrees left of its centre and `pitch` up,
/// `fov` degrees wide.
pub fn panorama_view(pano: &image::RgbaImage, yaw: f32, pitch: f32, fov: f32, w: u32, h: u32) -> image::RgbaImage {
    let (yaw, pitch) = (yaw.to_radians(), pitch.to_radians());
    let forward = glam::Vec3::new(pitch.cos() * yaw.cos(), pitch.cos() * yaw.sin(), pitch.sin());
    let right = forward.cross(glam::Vec3::Z).normalize_or(glam::Vec3::NEG_Y);
    let up = right.cross(forward);
    let t = (fov.to_radians() * 0.5).tan();
    let (pw, ph) = (pano.width() as f32, pano.height() as f32);
    image::RgbaImage::from_fn(w, h, |x, y| {
        let sx = ((x as f32 + 0.5) / w as f32 * 2.0 - 1.0) * t;
        let sy = (1.0 - (y as f32 + 0.5) / h as f32 * 2.0) * t * h as f32 / w as f32;
        let d = (forward + right * sx + up * sy).normalize();
        let lon = d.y.atan2(d.x);
        let lat = d.z.clamp(-1.0, 1.0).asin();
        let u = (std::f32::consts::PI - lon) / std::f32::consts::TAU;
        let v = (std::f32::consts::FRAC_PI_2 - lat) / std::f32::consts::PI;
        let c = bilinear(pano, (u * pw - 0.5).rem_euclid(pw).min(pw - 1.0), (v * ph - 0.5).clamp(0.0, ph - 1.0));
        image::Rgba(c.map(|v| v.round().clamp(0.0, 255.0) as u8))
    })
}

fn start_face(state: &mut AppState) -> Result<Value, String> {
    let run = state.rendered.batch.panorama.as_ref().ok_or("no panorama")?;
    let (camera, size, face) = (run.camera, run.size, run.faces.len() as u8);
    let mut settings = state.scene.render.clone();
    settings.width = size;
    settings.height = size;
    settings.pixel_aspect = 1.0;
    settings.area = RenderArea::View;
    settings.elements_active = false;
    state.status_prompt = format!("Panorama Exporter: face {} of 6", face + 1);
    start(state, RenderRequest { viewport: render_viewport(state), settings, camera: Some(camera), cube_face: Some(face), ..Default::default() })
}

fn panorama_next(state: &mut AppState) -> Option<Result<Value, String>> {
    let img = state.rendered.image.clone();
    let run = state.rendered.batch.panorama.as_mut()?;
    match img {
        Some(i) => run.faces.push(i),
        None => {
            state.rendered.batch.panorama = None;
            return Some(Err("Panorama Exporter: a face did not render".into()));
        }
    }
    if run.faces.len() < 6 {
        return Some(start_face(state).inspect_err(|_| end_runs(state)));
    }
    let run = state.rendered.batch.panorama.take()?;
    let pano = cube_to_equirect(&run.faces, run.size);
    let (w, h) = pano.dimensions();
    let pano = Arc::new(pano);
    if let Some(p) = &run.output {
        if let Err(e) = crate::render::save_output(&pano, p, &state.scene.render) {
            message(state, MessageLevel::Error, format!("Panorama Exporter: {e}"));
            state.rendered.set_image(Some(pano));
            return Some(Err(e));
        }
        message(state, MessageLevel::Info, format!("Panorama Exporter: {w}x{h} panorama saved to {p}"));
    }
    state.rendered.set_image(Some(pano.clone()));
    state.rendered.title = format!("Panorama, {w}x{h}");
    state.rendered.gbuffer = None;
    state.panorama.image = Some(pano);
    state.panorama.version += 1;
    None
}

/// The Panorama Exporter viewer: the panorama and where it looks.
#[derive(Debug, Clone, Default)]
pub struct PanoramaViewer {
    pub open: bool,
    pub image: Option<Arc<image::RgbaImage>>,
    pub yaw: f32,
    pub pitch: f32,
    pub fov: f32,
    pub version: u64,
    /// The Panorama Exporter rollout: camera, face size and output file.
    pub camera: Option<u64>,
    pub size: u32,
    pub output: String,
}

fn ranged(name: &str, v: f32, lo: f32, hi: f32) -> Result<f32, String> {
    if v.is_finite() && (lo..=hi).contains(&v) {
        Ok(v)
    } else {
        Err(format!("{name} {v}: {lo} to {hi}"))
    }
}

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    match cmd {
        C::RenderOutputSetup { jpeg_quality, png_type, png_alpha, tga_bits, tga_compress, tga_alpha_split, tga_premultiplied, exr_full_float, exr_alpha } => {
            let mut s = state.scene.render.clone();
            let f = &mut s.output_format;
            if let Some(q) = jpeg_quality {
                if !(1..=100).contains(&q) {
                    return Err(format!("JPEG quality {q}: 1 to 100"));
                }
                f.jpeg_quality = q as u8;
            }
            if let Some(v) = png_type {
                f.png_type = v;
            }
            if let Some(v) = png_alpha {
                f.png_alpha = v;
            }
            if let Some(b) = tga_bits {
                if b != 24 && b != 32 {
                    return Err(format!("Targa bits per pixel {b}: 24 or 32"));
                }
                f.tga_bits = b;
            }
            for (field, v) in [(&mut f.tga_compress, tga_compress), (&mut f.tga_alpha_split, tga_alpha_split), (&mut f.tga_premultiplied, tga_premultiplied), (&mut f.exr_full_float, exr_full_float), (&mut f.exr_alpha, exr_alpha)] {
                if let Some(v) = v {
                    *field = v;
                }
            }
            set_settings(state, "Render Output Setup", s)?;
            Ok(serde_json::to_value(&state.scene.render.output_format).unwrap_or(Value::Null))
        }
        C::GammaSettings { enabled, output_gamma, override_gamma, use_system } => {
            let mut s = state.scene.render.clone();
            let g = &mut s.gamma;
            if let Some(v) = enabled {
                g.enabled = v;
            }
            if let Some(v) = output_gamma {
                g.output = ranged("output gamma", v, 0.01, 5.0)?;
            }
            if let Some(v) = override_gamma {
                g.override_output = Some(ranged("override gamma", v, 0.01, 5.0)?);
            }
            if use_system == Some(true) {
                g.override_output = None;
            }
            set_settings(state, "Gamma and LUT", s)?;
            let g = &state.scene.render.gamma;
            Ok(json!({ "enabled": g.enabled, "output_gamma": g.output, "override_gamma": g.override_output, "applied": g.output_gamma() }))
        }
        C::OutputSizeFormat { format, aperture_width } => {
            let mut s = state.scene.render.clone();
            if let Some(name) = format {
                if name.trim().eq_ignore_ascii_case("custom") {
                    s.film_format = None;
                } else {
                    let f = film_format(&name).ok_or_else(|| format!("unknown output size format {name:?}"))?;
                    s.film_format = Some(f.name.to_owned());
                    if let Some(a) = f.aperture {
                        s.aperture_width = a;
                    }
                    s.pixel_aspect = f.pixel_aspect;
                    match f.size {
                        Some([w, h]) => {
                            s.width = w;
                            s.height = h;
                        }
                        None => s.height = ((s.width as f32 * f.pixel_aspect / f.image_aspect).round() as u32).max(1),
                    }
                    // The format keeps Width and Height at its aspect.
                    s.lock_image_aspect = true;
                }
            }
            if let Some(a) = aperture_width {
                if s.film_format.is_some() {
                    return Err("the Aperture Width is the format's (choose Custom to set it)".into());
                }
                s.aperture_width = ranged("aperture width", a, 0.1, 1000.0)?;
            }
            crate::render::check_size(s.width, s.height)?;
            set_settings(state, "Output Size", s)?;
            let s = &state.scene.render;
            Ok(json!({ "format": s.film_format.clone().unwrap_or_else(|| "Custom".into()), "aperture_width": s.aperture_width, "width": s.width, "height": s.height, "pixel_aspect": s.pixel_aspect, "image_aspect": s.image_aspect() }))
        }
        C::RenderMessageLog { write, path, append } => {
            let p = &mut state.prefs;
            if let Some(path) = path {
                let path = path.trim().to_owned();
                p.render_log_path = (!path.is_empty()).then_some(path);
                state.rendered.log_started = false;
            }
            if let Some(v) = write {
                if v && p.render_log_path.is_none() {
                    return Err("Write Messages to File needs a log file (Specify File)".into());
                }
                p.render_log = v;
            }
            if let Some(v) = append {
                p.render_log_append = v;
            }
            Ok(json!({ "write": p.render_log, "path": p.render_log_path, "append": p.render_log_append }))
        }
        C::ViewImageFile { path, sequence } => {
            let Some(path) = path else {
                state.dialog = Some(Dialog::ViewImageFile);
                return Ok(json!({ "dialog": "ViewImageFile" }));
            };
            let (files, index) = sequence_files(&path, sequence.unwrap_or(false))?;
            state.rendered.viewer = Some(ImageViewer { files, index });
            show_viewed(state)
        }
        C::ViewImageFileStep { step, index } => {
            let v = state.rendered.viewer.as_mut().ok_or("no image file is being viewed (View Image File)")?;
            let last = v.files.len() - 1;
            if let Some(i) = index {
                v.index = (i as usize).min(last);
            }
            if let Some(s) = step {
                v.index = (v.index as i64 + s as i64).clamp(0, last as i64) as usize;
            }
            show_viewed(state)
        }
        C::RenderedFrameWindowToggleUi => {
            state.rendered.ui_hidden = !state.rendered.ui_hidden;
            Ok(json!({ "ui_hidden": state.rendered.ui_hidden }))
        }
        C::RenderedFrameWindowGBuffer { channel } => {
            if channel.is_some() && state.rendered.gbuffer.is_none() {
                return Err("no G-buffer: render a Production frame first".into());
            }
            state.rendered.gchannel = channel;
            state.rendered.version += 1;
            state.rendered.dirty = None;
            Ok(json!({ "channel": channel.map(|c| c.label()).unwrap_or("RGB Alpha") }))
        }
        C::SetObjectChannel { channel } => {
            if channel > 65535 {
                return Err(format!("object channel {channel}: 0 to 65535"));
            }
            let mut ids: Vec<NodeId> = state.selection.iter().copied().collect();
            ids.sort();
            let mut changes = Vec::new();
            for id in ids {
                let Some(n) = state.scene.node(id) else { continue };
                let before = n.props();
                let mut after = before.clone();
                after.render.object_channel = channel;
                if after != before {
                    changes.push(Change::SetNodeProps { id, before: Box::new(before), after: Box::new(after) });
                }
            }
            let n = changes.len();
            if n > 0 {
                state.undo.commit("Object Channel", Change::Group(changes), &mut state.scene)?;
            }
            Ok(json!({ "channel": channel, "objects": n }))
        }
        C::BatchRender => {
            let b = &mut state.rendered.batch;
            b.open = !b.open;
            Ok(json!({ "open": b.open }))
        }
        C::BatchRenderAdd { duplicate } => {
            let mut s = state.scene.render.clone();
            let job = match duplicate {
                Some(i) => {
                    let mut j = s.batch.get(i as usize).cloned().ok_or_else(|| format!("no batch task {i}"))?;
                    j.name = format!("{}_copy", j.name);
                    j
                }
                None => BatchJob { name: format!("View{:02}", s.batch.len() + 1), width: s.width, height: s.height, pixel_aspect: s.pixel_aspect, frames: [state.current_frame; 2], ..Default::default() },
            };
            s.batch.push(job);
            let index = s.batch.len() - 1;
            set_settings(state, "Batch Render Add", s)?;
            state.rendered.batch.selected = index;
            Ok(json!({ "index": index, "tasks": index + 1 }))
        }
        C::BatchRenderDelete { index } => {
            let mut s = state.scene.render.clone();
            if index as usize >= s.batch.len() {
                return Err(format!("no batch task {index}"));
            }
            s.batch.remove(index as usize);
            let n = s.batch.len();
            set_settings(state, "Batch Render Delete", s)?;
            state.rendered.batch.selected = state.rendered.batch.selected.min(n.saturating_sub(1));
            Ok(json!({ "tasks": n }))
        }
        C::BatchRenderTask { index, name, enabled, camera, preset, override_preset, frames, width, height, pixel_aspect, output } => {
            let mut s = state.scene.render.clone();
            let j = s.batch.get_mut(index as usize).ok_or_else(|| format!("no batch task {index}"))?;
            if let Some(n) = name {
                let n = n.trim();
                if n.is_empty() {
                    return Err("a task needs a name".into());
                }
                j.name = n.to_owned();
            }
            if let Some(v) = enabled {
                j.enabled = v;
            }
            if let Some(c) = camera {
                if c == 0 {
                    j.camera = None;
                } else {
                    match state.scene.node(NodeId(c)).map(|n| &n.object) {
                        Some(loom_scene::ObjectKind::Camera { .. }) => j.camera = Some(c),
                        _ => return Err(format!("object {c} is not a camera")),
                    }
                }
            }
            if let Some(p) = preset {
                let p = p.trim();
                j.preset = (!p.is_empty()).then(|| p.to_owned());
            }
            if let Some(v) = override_preset {
                j.override_preset = v;
            }
            if let Some(f) = frames {
                if f[0] > f[1] {
                    return Err(format!("frames {} to {}: the start is after the end", f[0], f[1]));
                }
                j.frames = f;
            }
            if let Some(w) = width {
                j.width = w;
            }
            if let Some(h) = height {
                j.height = h;
            }
            crate::render::check_size(j.width, j.height)?;
            if let Some(a) = pixel_aspect {
                j.pixel_aspect = ranged("pixel aspect", a, 0.001, 1000.0)?;
            }
            if let Some(o) = output {
                let o = o.trim();
                j.output = (!o.is_empty()).then(|| o.to_owned());
            }
            set_settings(state, "Batch Render Task", s)?;
            state.rendered.batch.selected = index as usize;
            Ok(serde_json::to_value(&state.scene.render.batch[index as usize]).unwrap_or(Value::Null))
        }
        C::BatchRenderRun { confirm, update_path } => {
            if state.rendered.progress.is_some() || state.pending_render.is_some() {
                return Err("a render is in progress".into());
            }
            let tasks: Vec<usize> = state.scene.render.batch.iter().enumerate().filter(|(_, j)| j.enabled).map(|(i, _)| i).collect();
            if tasks.is_empty() {
                return Err("no task is enabled in the Batch Render queue".into());
            }
            // Invalid Output Path: Update Path gives those tasks this folder.
            let bad_dir = |j: &BatchJob| j.output.as_ref().and_then(|o| std::path::Path::new(o).parent()).is_some_and(|d| !d.as_os_str().is_empty() && !d.is_dir());
            if let Some(dir) = update_path {
                let mut s = state.scene.render.clone();
                for j in s.batch.iter_mut().filter(|j| bad_dir(j)) {
                    let name = std::path::Path::new(j.output.as_deref().unwrap_or_default()).file_name().map(|n| n.to_owned()).unwrap_or_default();
                    j.output = Some(std::path::Path::new(&dir).join(name).to_string_lossy().into_owned());
                }
                set_settings(state, "Batch Render Update Path", s)?;
            }
            let jobs = state.scene.render.batch.clone();
            let invalid: Vec<String> = tasks.iter().map(|i| &jobs[*i]).filter(|j| bad_dir(j)).map(|j| format!("{}: {} (the folder does not exist)", j.name, j.output.as_deref().unwrap_or_default())).collect();
            if !invalid.is_empty() {
                state.rendered.batch.warning = Some(invalid.clone());
                return Err(format!("Invalid Output Path: {}", invalid.join("; ")));
            }
            let mut warn = Vec::new();
            for i in &tasks {
                let j = &jobs[*i];
                match &j.output {
                    None => warn.push(format!("{}: no output path, the task is not saved", j.name)),
                    Some(o) => {
                        let s = task_settings(&state.scene.render, j)?;
                        let frames = s.output_frames(state.frame_range, state.current_frame)?;
                        let files: Vec<String> = if frames.len() > 1 || j.override_preset { frames.iter().map(|f| loom_scene::render::numbered_path(o, s.file_number(*f, frames[0]))).collect() } else { vec![o.clone()] };
                        if files.iter().any(|f| std::path::Path::new(f).exists()) {
                            warn.push(format!("{}: {o} would be overwritten", j.name));
                        }
                    }
                }
            }
            if !warn.is_empty() && confirm != Some(true) {
                state.rendered.batch.warning = Some(warn.clone());
                return Ok(json!({ "warning": warn, "started": false }));
            }
            state.rendered.batch.warning = None;
            state.rendered.batch.run = Some(BatchRun { tasks: tasks.clone(), pos: 0, done: 0 });
            match start_task(state, tasks[0]) {
                Ok(v) => Ok(json!({ "started": true, "tasks": tasks.len(), "first": v })),
                Err(e) => {
                    state.rendered.batch.run = None;
                    Err(e)
                }
            }
        }
        C::ActiveShadeFloater => {
            let open = !state.rendered.active_shade.open;
            state.rendered.active_shade.open = open;
            if open && state.rendered.progress.is_none() && state.pending_render.is_none() {
                active_shade_render(state)?;
            }
            Ok(json!({ "open": open }))
        }
        C::ActiveShadeCommand { action, on, region } => {
            let a = &mut state.rendered.active_shade;
            match action.as_str() {
                "Initialize" => {
                    a.image = None;
                    a.region = None;
                    a.version += 1;
                    active_shade_render(state)?;
                }
                "Update" => {
                    active_shade_render(state)?;
                }
                "Auto Update" => a.auto_update = on.unwrap_or(!a.auto_update),
                "Toggle Toolbar" => a.toolbar = !a.toolbar,
                "Draw Region" => {
                    if let Some(r) = region {
                        if !(r.iter().all(|v| (0.0..=1.0).contains(v)) && r[2] > r[0] && r[3] > r[1]) {
                            return Err(format!("region {r:?}: x0 < x1, y0 < y1 inside 0 to 1"));
                        }
                    }
                    a.region = region;
                    active_shade_render(state)?;
                }
                other => return Err(format!("ActiveShade {other:?}: Initialize, Update, Auto Update, Toggle Toolbar or Draw Region")),
            }
            let a = &state.rendered.active_shade;
            Ok(json!({ "open": a.open, "auto_update": a.auto_update, "toolbar": a.toolbar, "region": a.region, "rendering": state.pending_render.is_some() }))
        }
        C::PanoramaExport { camera, size, output } => {
            if state.rendered.progress.is_some() || state.pending_render.is_some() {
                return Err("a render is in progress".into());
            }
            if !matches!(state.scene.node(NodeId(camera)).map(|n| &n.object), Some(loom_scene::ObjectKind::Camera { .. })) {
                return Err(format!("object {camera} is not a camera"));
            }
            let size = size.unwrap_or(512);
            if !(64..=4096).contains(&size) {
                return Err(format!("face size {size}: 64 to 4096"));
            }
            state.rendered.batch.panorama = Some(PanoramaRun { camera: NodeId(camera), size, output: output.map(|o| o.trim().to_owned()).filter(|o| !o.is_empty()), faces: Vec::new() });
            start_face(state).inspect_err(|_| end_runs(state))
        }
        C::PanoramaViewer { path, yaw, pitch, fov } => {
            let v = &mut state.panorama;
            if let Some(p) = path {
                v.image = Some(Arc::new(image::open(&p).map_err(|e| format!("{p}: {e}"))?.to_rgba8()));
            }
            if v.image.is_none() {
                return Err("no panorama: render one (Panorama Exporter) or give its file".into());
            }
            if v.fov == 0.0 {
                v.fov = 60.0;
            }
            if let Some(y) = yaw {
                v.yaw = y.rem_euclid(360.0);
            }
            if let Some(p) = pitch {
                v.pitch = ranged("pitch", p, -90.0, 90.0)?;
            }
            if let Some(f) = fov {
                v.fov = ranged("field of view", f, 10.0, 120.0)?;
            }
            v.open = true;
            v.version += 1;
            Ok(json!({ "yaw": v.yaw, "pitch": v.pitch, "fov": v.fov }))
        }
        C::RenderElementOptions { index, lighting, direct, indirect, shadows, scale, matte_gbuffer, matte_objects, matte_exclude, blend_parts } => {
            let mut s = state.scene.render.clone();
            let e = s.elements.get_mut(index as usize).ok_or_else(|| format!("no render element {index}"))?;
            for (field, v) in [(&mut e.lighting, lighting), (&mut e.direct, direct), (&mut e.indirect, indirect), (&mut e.shadows, shadows), (&mut e.matte_exclude, matte_exclude)] {
                if let Some(v) = v {
                    *field = v;
                }
            }
            if let Some(v) = scale {
                e.scale = ranged("scale factor", v, 0.0001, 1000.0)?;
            }
            if let Some(g) = matte_gbuffer {
                if g > 65535 {
                    return Err(format!("G-Buffer ID {g}: 0 to 65535"));
                }
                e.matte_gbuffer = u32::try_from(g).ok();
            }
            if let Some(ids) = matte_objects {
                if let Some(id) = ids.iter().find(|id| state.scene.node(NodeId(**id)).is_none()) {
                    return Err(format!("matte objects: no object {id}"));
                }
                e.matte_objects = ids;
            }
            if let Some(parts) = blend_parts {
                let mut b = loom_scene::render::BlendParts { diffuse: false, specular: false, self_illumination: false, reflection: false, refraction: false, atmosphere: false, shadows: false };
                for p in &parts {
                    *b.get_mut(p).ok_or_else(|| format!("Blend part {p:?}: {}", loom_scene::render::BlendParts::NAMES.join(", ")))? = true;
                }
                e.blend = b;
            }
            set_settings(state, "Render Element", s)?;
            Ok(serde_json::to_value(&state.scene.render.elements[index as usize]).unwrap_or(Value::Null))
        }
        C::AmbientOcclusionMapSettings { index, samples, spread, max_distance, bright, dark } => {
            let mut s = state.scene.render.clone();
            let e = s.bake.elements.get_mut(index as usize).ok_or_else(|| format!("no texture element {index}"))?;
            if e.kind != loom_scene::bake::ElementKind::AmbientOcclusion {
                return Err(format!("texture element {index} is not an AmbientOcclusionMap"));
            }
            if let Some(n) = samples {
                if !(1..=1024).contains(&n) {
                    return Err(format!("samples {n}: 1 to 1024"));
                }
                e.ao_samples = n;
            }
            if let Some(v) = spread {
                e.ao_spread = ranged("spread", v, 0.0, 10.0)?;
            }
            if let Some(v) = max_distance {
                e.ao_max_distance = ranged("max distance", v, 0.0, 1.0e9)?;
            }
            for (field, c) in [(&mut e.ao_bright, bright), (&mut e.ao_dark, dark)] {
                if let Some(c) = c {
                    if !c.iter().all(|v| (0.0..=1.0).contains(v)) {
                        return Err(format!("colour {c:?}: sRGB 0 to 1"));
                    }
                    *field = c;
                }
            }
            set_settings(state, "Ambient Occlusion", s)?;
            let e = &state.scene.render.bake.elements[index as usize];
            Ok(json!({ "samples": e.ao_samples, "spread": e.ao_spread, "max_distance": e.ao_max_distance, "bright": e.ao_bright, "dark": e.ao_dark }))
        }
        other => Err(format!("{} is not a render output command", other.name())),
    }
}

/// Aperture Width: the Lens (mm) a camera's horizontal field of view reads as.
pub fn lens_for(fov_deg: f32, aperture: f32) -> f32 {
    loom_scene::camera::fov_to_lens(fov_deg) * aperture / DEFAULT_APERTURE
}

/// The horizontal field of view a Lens (mm) gives at the Aperture Width.
pub fn fov_for(lens_mm: f32, aperture: f32) -> f32 {
    loom_scene::camera::lens_to_fov(lens_mm * DEFAULT_APERTURE / aperture.max(0.1))
}
