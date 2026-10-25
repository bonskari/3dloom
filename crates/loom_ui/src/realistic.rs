//! Realistic viewport shading and Render, drawn by Bevy (`loom_bevy`) on the
//! UI's wgpu device. A Realistic viewport shows Bevy's image as its
//! background; the overlays are painted over it as in the other modes.
//! A Render works through the frame in buckets, one per UI frame, into the
//! Rendered Frame Window's image, so the UI keeps running while it renders.

use crate::LoomApp;
use eframe::egui;
use glam::Mat4;
use loom_scene::render::RenderArea;
use std::collections::HashMap;
use std::sync::Arc;

pub struct BevyView {
    renderer: loom_bevy::BevyRenderer,
    render_state: egui_wgpu::RenderState,
    /// `LoomApp::world_version` last synced into Bevy.
    synced: Option<u64>,
    /// Egui texture per viewport, with the Bevy view generation it shows.
    textures: HashMap<u32, (u64, egui::TextureId)>,
}

/// The GPU state the app was created with: Bevy's renderer starts on it
/// the first time a Realistic viewport or a Render needs it.
pub enum Gpu {
    None,
    Ready(egui_wgpu::RenderState),
    Running(Box<BevyView>),
    Failed,
}

/// Object Properties > Rendering Control of node `id` for the renderer.
fn render_flags(scene: &loom_scene::Scene, id: loom_scene::NodeId) -> loom_bevy::ObjectFlags {
    let Some(n) = scene.node(id) else { return Default::default() };
    let r = scene.render_of(n);
    loom_bevy::ObjectFlags { cast_shadows: r.cast_shadows, receive_shadows: r.receive_shadows, visible_to_camera: r.visible_to_camera, visibility: scene.render_visibility(id) }
}

/// Output pixels per bucket side.
pub const BUCKET: u32 = 256;
/// Frames the first bucket renders before it is kept (new pipelines, the
/// target image and the shadow map settle).
const WARMUP: u32 = 3;

/// A render in progress.
pub struct RenderJob {
    req: loom_commands::RenderRequest,
    camera: loom_bevy::ViewCamera,
    aspect: f32,
    /// The frame the view spans, in output pixels (larger than the output for Blowup).
    full: [f32; 2],
    /// Where the output image starts in that frame.
    origin: [f32; 2],
    out: [u32; 2],
    /// Bucket corners in output pixels, in render order.
    buckets: Vec<[u32; 2]>,
    next: usize,
    warmup: u32,
    /// Frames whose bucket could not be read back yet.
    misses: u32,
    /// Region: only these output pixels change (x0, y0, x1, y1).
    mask: Option<[u32; 4]>,
    started: std::time::Instant,
    /// A bucket was set up for this frame's update.
    queued: bool,
}

impl RenderJob {
    pub fn elapsed(&self) -> std::time::Duration {
        self.started.elapsed()
    }
}

/// Where a request's output lies in the view: (full frame, output origin,
/// output size, region mask), all in output pixels.
pub fn render_geometry(s: &loom_scene::render::RenderSettings) -> ([f32; 2], [f32; 2], [u32; 2], Option<[u32; 4]>) {
    let (w, h) = (s.width as f32, s.height as f32);
    let r = s.area_rect();
    match s.area {
        RenderArea::View | RenderArea::Selected => ([w, h], [0.0, 0.0], [s.width, s.height], None),
        RenderArea::Region => {
            let m = [(r[0] * w).floor() as u32, (r[1] * h).floor() as u32, ((r[2] * w).ceil() as u32).min(s.width), ((r[3] * h).ceil() as u32).min(s.height)];
            ([w, h], [0.0, 0.0], [s.width, s.height], Some(m))
        }
        RenderArea::Crop => {
            let (x0, y0) = ((r[0] * w).round(), (r[1] * h).round());
            let out = [(((r[2] - r[0]) * w).round() as u32).max(1), (((r[3] - r[1]) * h).round() as u32).max(1)];
            ([w, h], [x0, y0], out, None)
        }
        RenderArea::Blowup => {
            let full = [w / (r[2] - r[0]).max(1e-3), h / (r[3] - r[1]).max(1e-3)];
            ([full[0], full[1]], [r[0] * full[0], r[1] * full[1]], [s.width, s.height], None)
        }
    }
}

/// Bucket corners covering `area` (x0, y0, x1, y1), top row first.
pub fn buckets(area: [u32; 4]) -> Vec<[u32; 2]> {
    let mut v = Vec::new();
    let mut y = area[1];
    while y < area[3] {
        let mut x = area[0];
        while x < area[2] {
            v.push([x, y]);
            x += BUCKET;
        }
        y += BUCKET;
    }
    v
}

/// Averages `s` x `s` blocks (supersampling).
fn downsample(img: &image::RgbaImage, s: u32) -> image::RgbaImage {
    if s <= 1 {
        return img.clone();
    }
    let (w, h) = (img.width() / s, img.height() / s);
    image::RgbaImage::from_fn(w, h, |x, y| {
        let mut sum = [0u32; 4];
        for dy in 0..s {
            for dx in 0..s {
                let p = img.get_pixel(x * s + dx, y * s + dy).0;
                for i in 0..4 {
                    sum[i] += p[i] as u32;
                }
            }
        }
        let n = s * s;
        image::Rgba(sum.map(|v| ((v + n / 2) / n) as u8))
    })
}

/// A cleared frame: the background colour with alpha 0.
fn cleared(size: [u32; 2], bg: [f32; 3]) -> image::RgbaImage {
    let c = bg.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    image::RgbaImage::from_pixel(size[0], size[1], image::Rgba([c[0], c[1], c[2], 0]))
}

impl LoomApp {
    fn bevy(&mut self) -> Option<&mut BevyView> {
        if let Gpu::Ready(rs) = &self.gpu {
            self.gpu = if loom_bevy::supported(&rs.adapter) {
                let rs = rs.clone();
                let renderer = loom_bevy::BevyRenderer::new(rs.adapter.clone(), rs.device.clone(), rs.queue.clone());
                Gpu::Running(Box::new(BevyView { renderer, render_state: rs, synced: None, textures: HashMap::new() }))
            } else {
                self.last_error = Some("Realistic shading needs a Vulkan, Metal or DirectX 12 GPU".into());
                Gpu::Failed
            };
        }
        match &mut self.gpu {
            Gpu::Running(b) => Some(b),
            _ => None,
        }
    }

    /// Whether Realistic viewports can be drawn (a GPU, not the headless app).
    pub fn realistic_available(&self) -> bool {
        matches!(self.gpu, Gpu::Ready(_) | Gpu::Running(_))
    }

    /// Renders show geometry, not helpers or camera icons.
    fn renderable(object: &loom_scene::ObjectKind) -> bool {
        use loom_scene::ObjectKind as K;
        !matches!(object, K::Dummy { .. } | K::Point { .. } | K::IkGoal { .. } | K::Group { .. } | K::Camera { .. } | K::Light(_))
    }

    fn sync_bevy(&mut self) {
        let version = self.world_version;
        // Light icons and their targets are viewport proxies: not in Bevy at
        // all (they would block their own light), drawn as overlay lines.
        let objects: Vec<(u64, usize, [u8; 3], bool)> = self
            .world_meshes()
            .iter()
            .enumerate()
            .filter(|(_, (id, _))| !self.is_light_proxy(*id))
            .filter_map(|(i, (id, _))| self.state.scene.node(*id).map(|n| (id.0, i, self.state.scene.display_color(n), Self::renderable(&n.object) && self.state.scene.is_renderable(n))))
            .collect();
        let lights = loom_scene::light::scene_lights(&self.state.scene, |n| self.state.scene.is_renderable(n));
        let display = self.world_display.clone();
        let scene = &self.state.scene;
        let Gpu::Running(b) = &mut self.gpu else { return };
        if b.synced == Some(version) {
            return;
        }
        let list: Vec<loom_bevy::SceneObject> = objects
            .iter()
            .filter_map(|(id, i, wire, renderable)| {
                Some(loom_bevy::SceneObject { id: *id, mesh: display.get(*i)?, wire_color: *wire, material: scene.node_material(loom_scene::NodeId(*id)), renderable: *renderable, flags: render_flags(scene, loom_scene::NodeId(*id)) })
            })
            .collect();
        b.renderer.sync(&list);
        b.renderer.sync_lights(&lights);
        b.synced = Some(version);
    }

    /// A light node or a light's target.
    pub(crate) fn is_light_proxy(&self, id: loom_scene::NodeId) -> bool {
        let scene = &self.state.scene;
        matches!(scene.node(id).map(|n| &n.object), Some(loom_scene::ObjectKind::Light(_))) || loom_commands::light::is_light_target(scene, id)
    }

    /// Render Hidden Geometry: the world meshes of the hidden geometry.
    fn hidden_meshes(&self) -> Vec<(loom_scene::NodeId, loom_mesh::Mesh)> {
        let scene = &self.state.scene;
        scene
            .nodes()
            .filter(|n| !self.state.is_visible(n) && Self::renderable(&n.object) && !n.object.is_bone())
            .map(|n| {
                let mut mesh = scene.eval_mesh(n.id).value.mesh.clone();
                let _ = loom_mesh::edit_ops::transform_mesh(&mut mesh, scene.object_matrix(n.id));
                (n.id, mesh)
            })
            .collect()
    }

    /// Asks Bevy to render viewport `index` this frame at `size` physical
    /// pixels and returns the texture to draw (None on the first frame of a
    /// view, before Bevy made its image). Only called when `realistic_available`.
    pub(crate) fn realistic_texture(&mut self, index: usize, size: [u32; 2], view: Mat4, projection: Mat4) -> Option<egui::TextureId> {
        let b = self.bevy()?;
        let camera = loom_bevy::ViewCamera { view, projection };
        // Cleared to transparent: the viewport background shows through.
        b.renderer.set_view(index as u32, size, &camera, [0.0; 4], true);
        b.textures.get(&(index as u32)).map(|(_, id)| *id)
    }

    /// End of the frame: a requested render starts, Bevy renders the
    /// Realistic views requested this frame (before egui paints them) and
    /// the next bucket of the render in progress.
    pub(crate) fn finish_realistic_frame(&mut self, ctx: &egui::Context) {
        if let Some(req) = self.state.pending_render.take() {
            if let Err(e) = self.start_render(req) {
                self.last_error = Some(format!("Render: {e}"));
            }
        }
        if matches!(self.gpu, Gpu::Running(_)) {
            self.sync_bevy();
            self.queue_bucket();
            let Gpu::Running(b) = &mut self.gpu else { return };
            b.renderer.update();
            let slots: Vec<u32> = (0..self.state.viewports.len() as u32).collect();
            for slot in slots {
                let Some((generation, view)) = b.renderer.view_texture(slot) else { continue };
                match b.textures.get(&slot) {
                    Some((g, _)) if *g == generation => {}
                    Some((_, id)) => {
                        let id = *id;
                        b.render_state.renderer.write().update_egui_texture_from_wgpu_texture(&b.render_state.device, &view, wgpu::FilterMode::Linear, id);
                        b.textures.insert(slot, (generation, id));
                        ctx.request_repaint();
                    }
                    None => {
                        let id = b.render_state.renderer.write().register_native_texture(&b.render_state.device, &view, wgpu::FilterMode::Linear);
                        b.textures.insert(slot, (generation, id));
                        ctx.request_repaint();
                    }
                }
            }
            if b.renderer.needs_more_frames() {
                ctx.request_repaint();
            }
            self.collect_bucket();
        }
        if self.render_job.is_some() {
            ctx.request_repaint();
        }
    }

    fn start_render(&mut self, req: loom_commands::RenderRequest) -> Result<(), String> {
        if self.bevy().is_none() {
            return Err("no GPU renderer".into());
        }
        self.sync_bevy();
        let s = req.settings.clone();
        let hidden = if s.render_hidden { self.hidden_meshes() } else { Vec::new() };
        let scene = &self.state.scene;
        let hidden_objects: Vec<loom_bevy::SceneObject> = hidden
            .iter()
            .filter_map(|(id, mesh)| scene.node(*id).map(|n| loom_bevy::SceneObject { id: id.0, mesh, wire_color: n.wire_color, material: scene.node_material(*id), renderable: scene.is_renderable(n), flags: render_flags(scene, *id) }))
            .collect();
        let selected_only = (s.area == RenderArea::Selected).then(|| req.selection.iter().map(|n| n.0).collect());
        let options = loom_bevy::RenderOptions {
            msaa: s.bevy.antialiasing,
            shadows: s.bevy.shadows,
            shadow_map_size: s.bevy.shadow_map_size,
            background: s.bevy.background,
            ambient: s.bevy.ambient,
            force_two_sided: s.force_two_sided,
            selected_only,
            hidden: s.render_hidden,
        };
        let aspect = s.image_aspect();
        let cam = self.camera_for(req.viewport);
        let camera = loom_bevy::ViewCamera { view: cam.view_matrix(), projection: cam.projection_matrix(aspect) };
        let Gpu::Running(b) = &mut self.gpu else { return Err("no GPU renderer".into()) };
        b.renderer.sync_hidden(&hidden_objects);
        b.renderer.begin_render(options);

        let (full, origin, out, mask) = render_geometry(&s);
        // Iterative Region / Selected keep the rest of the window's image;
        // everything else starts from a cleared frame.
        let keep = req.iterative && matches!(s.area, RenderArea::Region | RenderArea::Selected);
        let base = match &self.state.rendered.image {
            Some(img) if keep && img.dimensions() == (out[0], out[1]) => (**img).clone(),
            _ => cleared(out, s.bevy.background),
        };
        self.state.rendered.set_image(Some(Arc::new(base)));
        let area = mask.unwrap_or([0, 0, out[0], out[1]]);
        let list = buckets(area);
        self.state.rendered.progress = Some(loom_commands::render::RenderProgress { done: 0, total: list.len() as u32, paused: false });
        self.state.rendered.cancel = false;
        self.state.rendered.open = true;
        self.state.status_prompt = format!("Rendering {}x{}...", out[0], out[1]);
        self.render_job = Some(RenderJob { req, camera, aspect, full, origin, out, buckets: list, next: 0, warmup: WARMUP, misses: 0, mask, started: std::time::Instant::now(), queued: false });
        Ok(())
    }

    /// Before Bevy's update: point the render camera at the next bucket.
    fn queue_bucket(&mut self) {
        let Some(job) = self.render_job.as_mut() else { return };
        job.queued = false;
        if self.state.rendered.cancel {
            self.finish_render(Err("cancelled".into()));
            return;
        }
        if self.state.rendered.progress.is_some_and(|p| p.paused) {
            return;
        }
        let Some(&[bx, by]) = job.buckets.get(job.next) else { return };
        let ss = job.req.settings.bevy.supersampling.clamp(1, 4);
        let sub = [
            (job.origin[0] + bx as f32) / job.full[0],
            (job.origin[1] + by as f32) / job.full[1],
            (job.origin[0] + (bx + BUCKET) as f32) / job.full[0],
            (job.origin[1] + (by + BUCKET) as f32) / job.full[1],
        ];
        let (camera, aspect) = (job.camera, job.aspect);
        let Gpu::Running(b) = &mut self.gpu else { return };
        if b.renderer.set_render_bucket(&camera, aspect, sub, [BUCKET * ss; 2]).is_ok() {
            if let Some(job) = self.render_job.as_mut() {
                job.queued = true;
            }
        }
    }

    /// After Bevy's update: put the bucket into the Rendered Frame Window.
    fn collect_bucket(&mut self) {
        let Some(job) = self.render_job.as_mut() else { return };
        if !job.queued {
            return;
        }
        if job.warmup > 0 {
            job.warmup -= 1;
            return;
        }
        let Gpu::Running(b) = &mut self.gpu else { return };
        let tile = match b.renderer.read_render() {
            Ok(t) => t,
            Err(e) => {
                job.misses += 1;
                if job.misses > 30 {
                    self.finish_render(Err(e));
                }
                return;
            }
        };
        let ss = job.req.settings.bevy.supersampling.clamp(1, 4);
        let tile = downsample(&tile, ss);
        let [bx, by] = job.buckets[job.next];
        let (out, mask) = (job.out, job.mask);
        let composite = job.req.settings.area == RenderArea::Selected;
        let bg = job.req.settings.bevy.background;
        let frame = &mut self.state.rendered;
        let img = frame.image.get_or_insert_with(|| Arc::new(cleared(out, bg)));
        if img.dimensions() != (out[0], out[1]) {
            *img = Arc::new(cleared(out, bg));
        }
        let img = Arc::make_mut(img);
        let [x0, y0, x1, y1] = mask.unwrap_or([0, 0, out[0], out[1]]);
        let (w, h) = ((bx + BUCKET).min(x1) - bx, (by + BUCKET).min(y1) - by);
        for y in 0..h {
            for x in 0..w {
                let (px, py) = (bx + x, by + y);
                if px < x0 || py < y0 {
                    continue;
                }
                let s = tile.get_pixel(x, y).0;
                let d = img.get_pixel_mut(px, py);
                d.0 = if composite {
                    // Selected: the objects over what is already there.
                    let a = s[3] as f32 / 255.0;
                    let mix = |s: u8, d: u8| (s as f32 * a + d as f32 * (1.0 - a)).round() as u8;
                    [mix(s[0], d.0[0]), mix(s[1], d.0[1]), mix(s[2], d.0[2]), (s[3] as f32 + d.0[3] as f32 * (1.0 - a)).round() as u8]
                } else {
                    s
                };
            }
        }
        frame.version += 1;
        let r = [bx, by, w, h];
        // None stays None (the whole image is still to be shown).
        frame.dirty = frame.dirty.map(|d| {
            if d[2] == 0 || d[3] == 0 {
                return r;
            }
            let (ax0, ay0) = (d[0].min(r[0]), d[1].min(r[1]));
            let (ax1, ay1) = ((d[0] + d[2]).max(r[0] + r[2]), (d[1] + d[3]).max(r[1] + r[3]));
            [ax0, ay0, ax1 - ax0, ay1 - ay0]
        });
        job.next += 1;
        if let Some(p) = frame.progress.as_mut() {
            p.done = job.next as u32;
        }
        if job.next >= job.buckets.len() {
            self.finish_render(Ok(()));
        }
    }

    fn finish_render(&mut self, result: Result<(), String>) {
        let Some(job) = self.render_job.take() else { return };
        if let Gpu::Running(b) = &mut self.gpu {
            b.renderer.end_render();
            b.renderer.sync_hidden(&[]);
        }
        self.state.rendered.progress = None;
        self.state.rendered.cancel = false;
        let secs = job.elapsed().as_secs_f32();
        match result {
            Ok(()) => {
                self.state.status_prompt = format!("Rendered {}x{} in {secs:.1} s", job.out[0], job.out[1]);
                if let (Some(path), Some(img)) = (&job.req.save_path, &self.state.rendered.image) {
                    match loom_commands::render::save_image(img, path) {
                        Ok(()) => self.state.status_prompt = format!("Rendered {}x{} in {secs:.1} s to {path}", job.out[0], job.out[1]),
                        Err(e) => self.last_error = Some(format!("Render: {e}")),
                    }
                }
            }
            Err(e) if e == "cancelled" => self.state.status_prompt = format!("Render cancelled after {secs:.1} s"),
            Err(e) => self.last_error = Some(format!("Render: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_scene::render::RenderSettings;

    #[test]
    fn areas_place_the_output_in_the_frame() {
        let mut s = RenderSettings { width: 400, height: 200, ..Default::default() };
        assert_eq!(render_geometry(&s), ([400.0, 200.0], [0.0, 0.0], [400, 200], None));
        s.region = [0.25, 0.5, 0.75, 1.0];
        s.area = RenderArea::Region;
        assert_eq!(render_geometry(&s).3, Some([100, 100, 300, 200]));
        s.area = RenderArea::Crop;
        assert_eq!(render_geometry(&s), ([400.0, 200.0], [100.0, 100.0], [200, 100], None));
        s.area = RenderArea::Blowup;
        s.blowup = [0.5, 0.5, 1.0, 1.0];
        assert_eq!(render_geometry(&s), ([800.0, 400.0], [400.0, 200.0], [400, 200], None));
    }

    #[test]
    fn buckets_cover_the_area_row_by_row() {
        let b = buckets([0, 0, 600, 300]);
        assert_eq!(b, vec![[0, 0], [256, 0], [512, 0], [0, 256], [256, 256], [512, 256]]);
        assert_eq!(buckets([100, 100, 110, 120]), vec![[100, 100]]);
    }

    #[test]
    fn supersampled_buckets_average_their_samples() {
        let img = image::RgbaImage::from_fn(4, 2, |x, _| if x % 2 == 0 { image::Rgba([200, 0, 0, 255]) } else { image::Rgba([0, 0, 100, 255]) });
        let d = downsample(&img, 2);
        assert_eq!(d.dimensions(), (2, 1));
        assert_eq!(d.get_pixel(0, 0).0, [100, 0, 50, 255]);
    }
}
