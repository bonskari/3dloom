//! Realistic viewport shading and Render, drawn by Bevy (`loom_bevy`) on the
//! UI's wgpu device. A Realistic viewport shows Bevy's image as its
//! background; the overlays are painted over it as in the other modes.
//! A Render works through the frame in buckets, one per UI frame, into the
//! Rendered Frame Window's image, so the UI keeps running while it renders;
//! the Environment's rendering effects apply to the finished image.

use crate::LoomApp;
use eframe::egui;
use glam::Mat4;
use loom_scene::render::RenderArea;
use std::collections::HashMap;
use std::sync::Arc;

mod passes;

pub struct BevyView {
    renderer: loom_bevy::BevyRenderer,
    render_state: egui_wgpu::RenderState,
    /// `LoomApp::world_version` last synced into Bevy, and whether the
    /// objects show their render materials (Shell: Render) or viewport ones.
    synced: Option<(u64, bool)>,
    /// `AppState::bitmap_generation` Bevy last read its image files at.
    bitmap_generation: u64,
    /// Egui texture per viewport, with the Bevy view generation it shows.
    textures: HashMap<u32, (u64, egui::TextureId)>,
    /// The environment the Exposure Control preview was rendered with.
    preview_env: Option<loom_bevy::environment::EnvSettings>,
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
    let up = scene.object_matrix(id).transform_vector3(glam::Vec3::Z);
    loom_bevy::ObjectFlags { cast_shadows: r.cast_shadows, receive_shadows: r.receive_shadows, visible_to_camera: r.visible_to_camera, visibility: scene.render_visibility(id), local_up: up.to_array() }
}

/// Photometric Shape/Area Shadows > Light Shape Visible in Rendering: the
/// emitter shape as a self-illuminated object in the light's colour.
pub(crate) struct LightShape {
    /// The light node's id with the top bit set (no node has it).
    pub id: u64,
    pub mesh: loom_mesh::Mesh,
    pub surfaces: loom_scene::material::Surfaces,
}

impl LightShape {
    fn object(&self) -> loom_bevy::SceneObject<'_> {
        // Neither casts nor takes shadows: it would block its own light.
        let flags = loom_bevy::ObjectFlags { cast_shadows: false, receive_shadows: false, visible_to_camera: true, visibility: 1.0, local_up: [0.0, 0.0, 1.0] };
        loom_bevy::SceneObject { id: self.id, mesh: &self.mesh, wire_color: [255; 3], surfaces: Some(self.surfaces.clone()), renderable: true, flags }
    }
}

/// The visible shapes of the scene's photometric lights that are on: Line
/// and Cylinder along the light's X axis, Rectangle and Disc in its XY plane
/// facing down its -Z (the way it shines), Sphere round its pivot.
pub(crate) fn light_shapes(scene: &loom_scene::Scene, lights: &[loom_scene::light::LightSample]) -> Vec<LightShape> {
    use glam::Vec3;
    use loom_mesh::primitives::{cylinder_primitive, quad_primitive, sphere_primitive};
    let mut out = Vec::new();
    for l in lights {
        let Some(n) = scene.node(l.id) else { continue };
        let loom_scene::ObjectKind::Light(lo) = &n.object else { continue };
        let Some(ph) = lo.photometric.as_ref().filter(|p| p.render_shape.get()) else { continue };
        if out.iter().any(|s: &LightShape| s.id == l.id.0 | 1 << 63) {
            continue;
        }
        let (len, wid, r) = (ph.shape_length.get().max(1e-3), ph.shape_width.get().max(1e-3), ph.shape_radius.get().max(1e-3));
        let along_x = Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let (mut mesh, local) = match ph.shape.get() {
            1 => (cylinder_primitive(len * 0.005, len, 8), along_x),
            2 => (quad_primitive(Vec3::new(-len * 0.5, -wid * 0.5, 0.0), Vec3::new(0.0, wid, 0.0), Vec3::new(len, 0.0, 0.0)), Mat4::IDENTITY),
            3 => (cylinder_primitive(r, r * 0.01, 32), Mat4::IDENTITY),
            4 => (sphere_primitive(r, 24, 12), Mat4::IDENTITY),
            5 => (cylinder_primitive(r, len, 24), along_x),
            _ => continue,
        };
        if loom_mesh::edit_ops::transform_mesh(&mut mesh, scene.world_matrix(l.id) * local).is_err() {
            continue;
        }
        let c = l.color.max(Vec3::ZERO);
        let mut m = loom_scene::material::StandardMaterial::new("Light Shape");
        m.diffuse.set(if c.max_element() > 0.0 { c / c.max_element() } else { Vec3::ZERO });
        m.self_illum.set(100.0);
        m.two_sided = true;
        let surfaces = loom_scene::material::Surfaces::Single(Box::new(loom_scene::material::Surface::from_standard(&m)));
        out.push(LightShape { id: l.id.0 | 1 << 63, mesh, surfaces });
    }
    out
}

/// Output pixels per bucket side, for areas too large to render whole.
pub const BUCKET: u32 = 256;
/// Largest rendered pass side: an area whose supersampled size fits renders
/// as one bucket, so the screen-space effects see the whole frame.
const WHOLE: u32 = 4096;

/// Bucket side for `area` at `ss` samples per pixel side: the whole area when it fits.
pub fn bucket_size(area: [u32; 4], ss: u32) -> u32 {
    let side = (area[2] - area[0]).max(area[3] - area[1]);
    // Room for the pass's margin (filter reach and screen-space view).
    if (side + 48) * ss.max(1) <= WHOLE {
        side.max(1)
    } else {
        BUCKET
    }
}
/// Frames the first bucket renders before it is kept (new pipelines, the
/// target image and the shadow map settle).
const WARMUP: u32 = 3;

/// A render in progress.
pub struct RenderJob {
    pub(crate) req: loom_commands::RenderRequest,
    /// What the renderer was given (the Render Elements render with it again).
    options: loom_bevy::RenderOptions,
    camera: loom_bevy::ViewCamera,
    aspect: f32,
    /// The frame the view spans, in output pixels (larger than the output for Blowup).
    full: [f32; 2],
    /// Where the output image starts in that frame.
    origin: [f32; 2],
    out: [u32; 2],
    /// Bucket corners in output pixels, in render order, and their side.
    buckets: Vec<[u32; 2]>,
    bucket: u32,
    next: usize,
    /// Passes per bucket, and those of the current bucket so far.
    passes: u32,
    pass: u32,
    warmup: u32,
    /// Frames whose bucket could not be read back yet.
    misses: u32,
    /// Region: only these output pixels change (x0, y0, x1, y1).
    mask: Option<[u32; 4]>,
    started: std::time::Instant,
    /// A bucket was set up for this frame's update.
    queued: bool,
    /// The environment the render uses (its image effects apply at the end).
    env: loom_bevy::environment::EnvSettings,
    /// Exposure Control > Render Preview renders into this, not the window.
    preview: Option<image::RgbaImage>,
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
pub fn buckets(area: [u32; 4], side: u32) -> Vec<[u32; 2]> {
    let mut v = Vec::new();
    let mut y = area[1];
    while y < area[3] {
        let mut x = area[0];
        while x < area[2] {
            v.push([x, y]);
            x += side;
        }
        y += side;
    }
    v
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
                Gpu::Running(Box::new(BevyView { renderer, render_state: rs, synced: None, bitmap_generation: 0, textures: HashMap::new(), preview_env: None }))
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

    fn sync_bevy(&mut self) {
        let for_render = self.render_job.is_some();
        self.sync_bevy_for(for_render);
    }

    /// Syncs the scene into Bevy; `for_render`: Shell materials show their
    /// Render choice (a render in progress), else their Viewport one.
    fn sync_bevy_for(&mut self, for_render: bool) {
        let version = self.world_version;
        let view = if for_render { loom_scene::material::ShellView::Render } else { loom_scene::material::ShellView::Viewport };
        // Light icons and their targets are viewport proxies: not in Bevy at
        // all (they would block their own light), drawn as overlay lines.
        let objects: Vec<(u64, usize, [u8; 3], bool)> = self
            .world_meshes()
            .iter()
            .enumerate()
            .filter(|(_, (id, _))| !self.is_light_proxy(*id))
            .filter_map(|(i, (id, _))| self.state.scene.node(*id).map(|n| (id.0, i, self.state.scene.display_color(n), self.state.scene.renders(n))))
            .collect();
        let lights = loom_scene::light::scene_lights(&self.state.scene, |n| self.state.scene.is_renderable(n));
        let shapes = if for_render { light_shapes(&self.state.scene, &lights) } else { Vec::new() };
        let display = self.world_display.clone();
        let scene = &self.state.scene;
        let generation = self.state.bitmap_generation;
        let Gpu::Running(b) = &mut self.gpu else { return };
        if b.synced == Some((version, for_render)) {
            return;
        }
        if b.bitmap_generation != generation {
            // Bitmap > Reload: Bevy reads the image files again too.
            b.renderer.reload_bitmaps();
            b.bitmap_generation = generation;
        }
        let mut list: Vec<loom_bevy::SceneObject> = objects
            .iter()
            .filter_map(|(id, i, wire, renderable)| {
                Some(loom_bevy::SceneObject { id: *id, mesh: display.get(*i)?, wire_color: *wire, surfaces: scene.node_surfaces_for(loom_scene::NodeId(*id), view), renderable: *renderable, flags: render_flags(scene, loom_scene::NodeId(*id)) })
            })
            .collect();
        list.extend(shapes.iter().map(|s| s.object()));
        b.renderer.sync(&list);
        b.renderer.sync_lights(&lights);
        b.synced = Some((version, for_render));
    }

    /// Some Skin has Fast Updates on.
    fn fast_skins(&self) -> bool {
        self.state.scene.nodes().any(|n| n.modifiers.iter().any(|s| matches!(&s.modifier, loom_scene::Modifier::Skin(d) if d.fast_updates)))
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
            .filter(|n| !self.state.is_visible(n) && scene.renders(n))
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
        let view_env = self.view_env(index);
        let b = self.bevy()?;
        b.renderer.set_view_environment(index as u32, view_env);
        let camera = loom_bevy::ViewCamera { view, projection };
        // Cleared to transparent: the viewport background shows through.
        b.renderer.set_view(index as u32, size, &camera, [0.0; 4], true);
        b.textures.get(&(index as u32)).map(|(_, id)| *id)
    }

    /// End of the frame: a requested render starts, Bevy renders the
    /// Realistic views requested this frame (before egui paints them) and
    /// the next bucket of the render in progress.
    pub(crate) fn finish_realistic_frame(&mut self, ctx: &egui::Context) {
        if let Some(req) = self.state.pending_bake.take() {
            self.run_bake(req);
            ctx.request_repaint();
        }
        if self.render_job.is_none() {
            if let Some(req) = self.state.pending_render.take() {
                if let Err(e) = self.start_render(req) {
                    loom_commands::render::message(&mut self.state, loom_commands::render::MessageLevel::Error, format!("Render: {e}"));
                    loom_commands::render::end_sequence(&mut self.state);
                    self.last_error = Some(format!("Render: {e}"));
                }
            }
        }
        if matches!(self.gpu, Gpu::Running(_)) {
            self.sync_bevy();
            let env = self.environment();
            // The Exposure Control preview follows changes to the environment.
            let stale = matches!(&self.gpu, Gpu::Running(b) if b.preview_env.as_ref().is_some_and(|p| *p != env));
            if stale && self.state.environment_ui.preview.is_some() && self.render_job.is_none() && self.state.pending_render.is_none() {
                let _ = self.dispatch(loom_commands::Command::ExposureRenderPreview);
            }
            // ActiveShade's Auto Update: the scene changed since its image.
            if self.render_job.is_none() && loom_commands::render_output::active_shade_stale(&self.state) {
                let _ = self.dispatch(loom_commands::Command::ActiveShadeCommand { action: "Update".into(), on: None, region: None });
            }
            if let Gpu::Running(b) = &mut self.gpu {
                b.renderer.set_environment(&env);
            }
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

    /// The scene's environment at the current frame, as the renderer takes it.
    pub(crate) fn environment(&self) -> loom_bevy::environment::EnvSettings {
        let mut env = loom_bevy::environment::EnvSettings::from_scene(&self.state.scene.environment, self.state.current_frame as f32);
        env.use_alpha = self.state.prefs.use_environment_alpha;
        env.filter_background = self.state.prefs.filter_background;
        env
    }

    /// Viewport `index`'s part of the environment: a camera view's
    /// Environment Ranges, and the distance Depth of Field focuses at (the
    /// focal node's depth, else the camera's target distance).
    pub(crate) fn view_env(&self, index: usize) -> loom_bevy::environment::ViewEnv {
        let scene = &self.state.scene;
        let Some(vp) = self.state.viewports.get(index) else { return Default::default() };
        let ranges = match vp.kind {
            loom_commands::ViewKind::Camera(id) => match scene.node(id).map(|n| &n.object) {
                Some(loom_scene::ObjectKind::Camera { near_range, far_range, .. }) => Some([near_range.get(), far_range.get()]),
                _ => None,
            },
            _ => None,
        };
        let dof = scene.environment.effects.iter().filter(|e| e.active).find_map(|e| match &e.kind {
            loom_scene::environment::EffectKind::DepthOfField(d) => Some(d),
            _ => None,
        });
        let focus = dof.map(|d| {
            let cam = self.camera_for(index);
            let world = cam.view_matrix().inverse();
            let (eye, forward) = (world.w_axis.truncate(), -world.z_axis.truncate().normalize_or_zero());
            match d.focal_node.filter(|n| scene.node(*n).is_some()) {
                Some(n) => (scene.world_matrix(n).w_axis.truncate() - eye).dot(forward).max(0.0),
                None => cam.orbit.distance,
            }
        });
        loom_bevy::environment::ViewEnv { ranges, focus }
    }

    fn start_render(&mut self, req: loom_commands::RenderRequest) -> Result<(), String> {
        if self.bevy().is_none() {
            return Err("no GPU renderer".into());
        }
        if self.fast_skins() {
            // Skin > Fast Updates previews rigidly in the viewports only: the render gets the full deformation.
            loom_scene::anim::with_rendering(|| self.rebuild_world_meshes());
        }
        self.sync_bevy_for(true);
        let env = self.environment();
        let view = self.view_env(req.viewport);
        let s = req.settings.clone();
        let hidden = if s.render_hidden { self.hidden_meshes() } else { Vec::new() };
        let scene = &self.state.scene;
        let hidden_objects: Vec<loom_bevy::SceneObject> = hidden
            .iter()
            .filter_map(|(id, mesh)| scene.node(*id).map(|n| loom_bevy::SceneObject { id: id.0, mesh, wire_color: n.wire_color, surfaces: scene.node_surfaces_for(*id, loom_scene::material::ShellView::Render), renderable: scene.renders(n), flags: render_flags(scene, *id) }))
            .collect();
        let selected_only = (s.area == RenderArea::Selected).then(|| req.selection.iter().map(|n| n.0).collect());
        let options = loom_bevy::RenderOptions {
            msaa: s.bevy.antialiasing,
            shadows: s.bevy.shadows,
            shadow_map_size: s.bevy.shadow_map_size,
            background: if env.background != [0.0; 3] { env.background } else { s.bevy.background },
            ambient: if env.ambient != [0.0; 3] { env.ambient } else { s.bevy.ambient },
            force_two_sided: s.force_two_sided,
            selected_only,
            hidden: s.render_hidden,
            // Area Lights/Shadows as Points: no soft shadows.
            quality: loom_scene::render::BevyRenderSettings { area_shadows: s.bevy.area_shadows && !s.area_as_points, ..s.bevy.clone() },
            atmospherics: s.atmospherics,
            effects: s.effects,
            view,
            element: None,
            raytrace: s.raytracer.enabled,
            hide: Vec::new(),
            light_tracer: s.advanced.light_tracer(),
            // Ray Traced / Adv. Ray Traced / Area Shadows lights: traced on the CPU, no shadow maps.
            traced_shadows: loom_scene::light::scene_lights(&self.state.scene, |n| self.state.scene.is_renderable(n)).iter().filter(|l| l.traced.is_some()).map(|l| l.id.0).collect(),
            lights_off: Vec::new(),
            lens: None,
        };
        let mut options = options;
        // A camera's Multi-Pass Depth of Field: its passes move the lens.
        let through = req.camera.or(match self.state.viewports.get(req.viewport).map(|v| v.kind) {
            Some(loom_commands::ViewKind::Camera(id)) => Some(id),
            _ => None,
        });
        if let Some(id) = through.filter(|_| req.cube_face.is_none() && !req.preview && !req.active_shade) {
            if let Some(loom_scene::ObjectKind::Camera { multi_pass, mp_use_target, mp_focal_depth, mp_passes, mp_radius, target_distance, .. }) = self.state.scene.node(id).map(|n| &n.object) {
                if multi_pass.get() {
                    let focus = if mp_use_target.get() { loom_commands::camera::target_distance(&self.state.scene, id).unwrap_or(target_distance.get()) } else { mp_focal_depth.get() };
                    options.lens = Some([mp_radius.get(), focus]);
                    options.quality.passes = mp_passes.get().clamp(1, 64);
                }
            }
        }
        let aspect = s.image_aspect();
        let cam = self.camera_for_request(&req);
        let mut camera = loom_bevy::ViewCamera { view: cam.view_matrix(), projection: cam.projection_matrix(aspect) };
        if let (Some(face), Some(id)) = (req.cube_face, req.camera) {
            // A Panorama Exporter face: 90 degrees square from the camera's position.
            let eye = self.state.scene.world_matrix(id).w_axis.truncate();
            let (f, u) = loom_commands::render_output::cube_face_axes(face);
            camera = loom_bevy::ViewCamera { view: glam::Mat4::look_at_rh(eye, eye + glam::Vec3::from(f), glam::Vec3::from(u)), projection: glam::Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, cam.near, cam.far) };
        }
        let Gpu::Running(b) = &mut self.gpu else { return Err("no GPU renderer".into()) };
        b.renderer.set_environment(&env);
        b.renderer.sync_hidden(&hidden_objects);
        b.renderer.begin_render(options.clone());
        if req.preview {
            b.preview_env = Some(env.clone());
        }

        let (full, origin, out, mask) = render_geometry(&s);
        let area = mask.unwrap_or([0, 0, out[0], out[1]]);
        let bucket = bucket_size(area, s.bevy.samples_per_side());
        let list = buckets(area, bucket);
        let passes = if req.preview { 1 } else { options.quality.passes.clamp(1, 64) };
        let preview = if req.active_shade {
            // ActiveShade: a Draw Region renders over the image it shows.
            let a = &self.state.rendered.active_shade;
            Some(match &a.image {
                Some(img) if a.region.is_some() && img.dimensions() == (out[0], out[1]) => (**img).clone(),
                _ => cleared(out, env.background),
            })
        } else {
            req.preview.then(|| cleared(out, env.background))
        };
        if preview.is_none() {
            // Iterative Region / Selected keep the rest of the window's image;
            // everything else starts from a cleared frame.
            let keep = req.iterative && matches!(s.area, RenderArea::Region | RenderArea::Selected);
            let base = match &self.state.rendered.image {
                Some(img) if keep && img.dimensions() == (out[0], out[1]) => (**img).clone(),
                _ => cleared(out, env.background),
            };
            self.state.rendered.set_image(Some(Arc::new(base)));
            self.state.rendered.progress = Some(loom_commands::render::RenderProgress { done: 0, total: list.len() as u32 * passes, paused: false });
            self.state.rendered.cancel = false;
            self.state.rendered.open = true;
            self.state.status_prompt = format!("Rendering {}x{}...", out[0], out[1]);
        }
        self.render_job = Some(RenderJob { req, camera, aspect, full, origin, out, buckets: list, bucket, next: 0, passes, pass: 0, warmup: WARMUP, misses: 0, mask, started: std::time::Instant::now(), queued: false, env, preview, options });
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
        let side = job.bucket;
        let sub = [
            (job.origin[0] + bx as f32) / job.full[0],
            (job.origin[1] + by as f32) / job.full[1],
            (job.origin[0] + (bx + side) as f32) / job.full[0],
            (job.origin[1] + (by + side) as f32) / job.full[1],
        ];
        let (camera, aspect) = (job.camera, job.aspect);
        let Gpu::Running(b) = &mut self.gpu else { return };
        if b.renderer.set_render_bucket(&camera, aspect, sub, [side; 2]).is_ok() {
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
        // Every pass shows; the bucket is done once all its passes are in.
        let done = b.renderer.bucket_done();
        let [bx, by] = job.buckets[job.next];
        let (out, mask, side) = (job.out, job.mask, job.bucket);
        let composite = job.req.settings.area == RenderArea::Selected;
        let bg = job.env.background;
        let frame = &mut self.state.rendered;
        let img: &mut image::RgbaImage = match job.preview.as_mut() {
            Some(p) => p,
            None => {
                let img = frame.image.get_or_insert_with(|| Arc::new(cleared(out, bg)));
                if img.dimensions() != (out[0], out[1]) {
                    *img = Arc::new(cleared(out, bg));
                }
                Arc::make_mut(img)
            }
        };
        let [x0, y0, x1, y1] = mask.unwrap_or([0, 0, out[0], out[1]]);
        let (w, h) = ((bx + side).min(x1) - bx, (by + side).min(y1) - by);
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
        if job.preview.is_some() {
            job.next += 1;
            if job.next >= job.buckets.len() {
                self.finish_render(Ok(()));
            }
            return;
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
        job.pass += 1;
        if done {
            job.next += 1;
            job.pass = 0;
        }
        if let Some(p) = frame.progress.as_mut() {
            p.done = job.next as u32 * job.passes + job.pass;
        }
        if job.next >= job.buckets.len() {
            self.finish_render(Ok(()));
        }
    }

    /// Render To Texture's lit elements: each baked by Bevy in the object's
    /// texture space, padded and written to its file (blocks for a few
    /// frames per map); the maps then load again.
    fn run_bake(&mut self, req: loom_commands::bake::BakeRequest) {
        use loom_bevy::bake::{BakeLook, BakeObject, BakeOptions};
        use loom_scene::bake::ElementKind;
        if self.bevy().is_none() {
            self.last_error = Some("Render To Texture: the lit elements need a GPU renderer".into());
            return;
        }
        self.sync_bevy_for(false);
        let started = std::time::Instant::now();
        let mut done = 0;
        for job in &req.jobs {
            let Some(node) = self.state.scene.node(job.node) else { continue };
            let (name, wire) = (node.name.clone(), self.state.scene.display_color(node));
            let mesh = loom_commands::bake::world_mesh(&self.state, job.node);
            let surfaces = self.state.scene.node_surfaces_for(job.node, loom_scene::material::ShellView::Original);
            let e = &job.element;
            let look = match e.kind {
                ElementKind::Complete => BakeLook::Complete,
                ElementKind::Diffuse => BakeLook::Diffuse { lighting: e.lighting },
                ElementKind::Specular => BakeLook::Specular { lighting: e.lighting },
                ElementKind::Lighting => BakeLook::Lighting,
                ElementKind::Shadows => BakeLook::Shadows,
                ElementKind::Normals | ElementKind::Alpha | ElementKind::Height | ElementKind::AmbientOcclusion => continue,
            };
            let object = BakeObject { mesh: &mesh, channel: job.channel, surfaces, wire_color: wire };
            let options = BakeOptions { look, shadows: e.shadows, direct: e.direct, indirect: e.indirect, size: [e.width, e.height], background: e.background };
            let Gpu::Running(b) = &mut self.gpu else { return };
            let r = b.renderer.bake(&object, &options);
            match r {
                Ok(mut img) => {
                    let mut covered = loom_bake::alpha_coverage(&img);
                    loom_bake::dilate(&mut img, &mut covered, job.padding);
                    if let Err(err) = loom_commands::render::save_image(&img, &job.path) {
                        self.last_error = Some(format!("Render To Texture: {err}"));
                        continue;
                    }
                    if req.rendered_frame_window {
                        self.state.rendered.title = format!("{name} {}", e.name);
                        self.state.rendered.set_image(Some(Arc::new(img)));
                        self.state.rendered.open = true;
                    }
                    done += 1;
                }
                Err(err) => self.last_error = Some(format!("Render To Texture: {name} {}: {err}", e.name)),
            }
        }
        // The files changed on disk: maps and viewports read them again. The
        // baked material was assigned before its file existed: a viewport's
        // failed load of one of these files is no longer an error.
        self.state.bitmap_generation += 1;
        if self.last_error.as_deref().is_some_and(|e| req.jobs.iter().any(|j| e.contains(&j.path) && std::path::Path::new(&j.path).exists())) {
            self.last_error = None;
        }
        self.state.status_prompt = format!("Render To Texture: {done} map(s) rendered in {:.1} s", started.elapsed().as_secs_f32());
    }

    /// The render's geometry for the CPU passes (Force Wireframe, Z Depth):
    /// the triangles of what the camera sees in renders, with polygon edges,
    /// and each triangle's material diffuse colour (sRGB 0-1).
    fn camera_triangles(&self, options: &loom_bevy::RenderOptions) -> (Vec<loom_bake::gbuffer::EdgeTri>, Vec<[f32; 3]>) {
        let (tris, colors, _) = self.camera_triangle_nodes(options);
        (tris, colors)
    }

    /// The camera's triangles, their colours and the node of each.
    fn camera_triangle_nodes(&self, options: &loom_bevy::RenderOptions) -> (Vec<loom_bake::gbuffer::EdgeTri>, Vec<[f32; 3]>, Vec<loom_scene::NodeId>) {
        let scene = &self.state.scene;
        let hidden = if options.hidden { self.hidden_meshes() } else { Vec::new() };
        let mut meshes: Vec<(loom_scene::NodeId, &loom_mesh::Mesh)> = (0..self.world_meshes().len()).map(|i| (self.world_meshes()[i].0, self.drawn_mesh(i))).collect();
        meshes.extend(hidden.iter().map(|(id, m)| (*id, m)));
        let (mut tris, mut colors, mut nodes) = (Vec::new(), Vec::new(), Vec::new());
        for (id, mesh) in meshes {
            let Some(n) = scene.node(id) else { continue };
            let flags = render_flags(scene, id);
            if self.is_light_proxy(id) || !scene.renders(n) || !flags.visible_to_camera || flags.visibility <= 0.0 {
                continue;
            }
            if options.selected_only.as_ref().is_some_and(|sel| !sel.contains(&id.0)) {
                continue;
            }
            let surfaces = scene.node_surfaces_for(id, loom_scene::material::ShellView::Render);
            let wire = scene.display_color(n).map(|c| c as f32 / 255.0);
            for t in loom_bake::gbuffer::edge_triangles(mesh) {
                colors.push(surfaces.as_ref().map_or(wire, |s| s.for_face(t.tri.material_id).diffuse.to_array()));
                tris.push(t);
                nodes.push(id);
            }
        }
        (tris, colors, nodes)
    }

    /// The G-buffer of a finished frame: per pixel the nearest surface's
    /// depth, Object Channel, node and normal (ray cast on the CPU).
    fn gbuffer(&self, job: &RenderJob) -> loom_commands::render_output::GBuffer {
        let (tris, _, nodes) = self.camera_triangle_nodes(&job.options);
        let face_ids: Vec<u16> = tris.iter().map(|t| t.tri.material_id).collect();
        let bvh = loom_bake::Bvh::new(tris.into_iter().map(|t| t.tri).collect());
        let hits = loom_bake::gbuffer::surfaces(&bvh, &Self::gbuffer_view(job));
        let scene = &self.state.scene;
        let channel = |id: loom_scene::NodeId| scene.node(id).map_or(0, |n| n.render.object_channel);
        let mut g = loom_commands::render_output::GBuffer { size: job.out, ..Default::default() };
        for h in &hits {
            g.depth.push(h.map(|h| h.0));
            let node = h.map(|h| nodes[h.1 as usize]);
            g.object.push(node.map_or(0, channel));
            g.node.push(node.map_or(0, |n| n.0));
            g.material.push(h.and_then(|h| scene.node(nodes[h.1 as usize]).and_then(|n| n.material).map(|m| loom_scene::material::effects_channel(&scene.materials, m, face_ids[h.1 as usize]))).unwrap_or(0));
            g.normal.push(h.map_or([0.0; 3], |h| h.2.to_array()));
        }
        g
    }

    /// The camera view of a job for the CPU passes.
    fn gbuffer_view(job: &RenderJob) -> loom_bake::gbuffer::View {
        let rect = [job.origin[0] / job.full[0], job.origin[1] / job.full[1], (job.origin[0] + job.out[0] as f32) / job.full[0], (job.origin[1] + job.out[1] as f32) / job.full[1]];
        loom_bake::gbuffer::View { view: job.camera.view, clip_from_view: job.camera.projection, rect, size: job.out }
    }

    /// Force Wireframe: the finished frame becomes the polygon edges of what
    /// the camera sees, Wire Thickness pixels wide, each in its material's
    /// diffuse colour, over the background (a Region changes only its rectangle).
    fn force_wireframe(&mut self, job: &RenderJob) {
        let s = &job.req.settings;
        let (tris, colors) = self.camera_triangles(&job.options);
        let bvh = loom_bake::Bvh::new(tris.iter().map(|t| t.tri).collect());
        let ss = if s.bevy.antialiasing { 3 } else { 1 };
        let cov = loom_bake::gbuffer::wire_coverage(&bvh, &tris, &Self::gbuffer_view(job), s.wire_thickness, ss, s.force_two_sided, |i| colors[i as usize]);
        let bg = job.env.background;
        let [x0, y0, x1, y1] = job.mask.unwrap_or([0, 0, job.out[0], job.out[1]]);
        let Some(img) = self.state.rendered.image.as_mut() else { return };
        let img = Arc::make_mut(img);
        for y in y0..y1.min(img.height()) {
            for x in x0..x1.min(img.width()) {
                let (c, sum) = cov[(y * job.out[0] + x) as usize];
                // `sum` is the wires' colour already weighted by their coverage.
                let px = |k: usize| ((sum[k] + bg[k] * (1.0 - c)).clamp(0.0, 1.0) * 255.0).round() as u8;
                img.put_pixel(x, y, image::Rgba([px(0), px(1), px(2), (c * 255.0).round() as u8]));
            }
        }
        self.state.rendered.version += 1;
        self.state.rendered.dirty = None;
    }

    /// Render Elements: each enabled element of a finished Production frame,
    /// rendered again with only its part of the shading (or computed: Alpha,
    /// Z Depth), saved to its file and shown in its own window.
    fn render_elements(&mut self, job: &RenderJob, layers: &passes::PassLayers) {
        use loom_commands::render::{message, MessageLevel};
        use loom_scene::render::ElementKind;
        let s = job.req.settings.clone();
        let Some(main) = self.state.rendered.image.clone() else { return };
        // The unnumbered output (a sequence numbers each element's file too).
        let numbering = self.state.rendered.sequence.as_ref().map(|q| (q.request.save_path.clone(), s.file_number(q.frames[q.index], q.first, self.state.prefs.nth_serial_numbering)));
        let base = match &numbering {
            Some((p, _)) => p.clone(),
            None => job.req.save_path.clone(),
        };
        let rect = Self::gbuffer_view(job).rect;
        let premultiplied = |mut img: image::RgbaImage| {
            // A lit element is black where nothing is.
            for p in img.pixels_mut() {
                let a = p.0[3] as f32 / 255.0;
                for c in &mut p.0[..3] {
                    *c = (*c as f32 * a).round() as u8;
                }
            }
            img
        };
        // The frame again with only the element's part of the shading.
        fn again(g: &mut Gpu, job: &RenderJob, rect: [f32; 4], look: Option<loom_bevy::ElementLook>, edit: &dyn Fn(&mut loom_bevy::RenderOptions)) -> Result<image::RgbaImage, String> {
            let mut o = job.options.clone();
            o.element = look;
            edit(&mut o);
            let Gpu::Running(b) = g else { return Err("no GPU renderer".into()) };
            b.renderer.render_frame_with(job.out, &job.camera, job.aspect, rect, o)
        }
        let gbuffer = self.state.rendered.gbuffer.clone();
        let lights: Vec<u64> = loom_scene::light::scene_lights(&self.state.scene, |n| self.state.scene.is_renderable(n)).iter().map(|l| l.id.0).collect();
        // A lit image scaled in linear light (Illuminance / Luminance HDR Data's Scale Factor).
        let scaled = |mut img: image::RgbaImage, k: f32| {
            for p in img.pixels_mut() {
                for c in &mut p.0[..3] {
                    let lin = (*c as f32 / 255.0).powf(2.2) * k;
                    *c = (lin.powf(1.0 / 2.2).clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            }
            img
        };
        // Images added up channel by channel (Blend), alpha the most of them.
        let add = |a: &mut image::RgbaImage, b: &image::RgbaImage| {
            for (p, q) in a.pixels_mut().zip(b.pixels()) {
                for k in 0..3 {
                    p.0[k] = p.0[k].saturating_add(q.0[k]);
                }
                p.0[3] = p.0[3].max(q.0[3]);
            }
        };
        for e in s.elements.iter().filter(|e| e.enabled) {
            let mut gpu = |look, edit: &dyn Fn(&mut loom_bevy::RenderOptions)| again(&mut self.gpu, job, rect, look, edit);
            let img = match e.kind {
                ElementKind::Diffuse if !e.lighting => gpu(Some(loom_bevy::ElementLook::DiffuseTexture), &|_| {}).map(premultiplied),
                ElementKind::Lighting if !(e.direct && e.indirect && e.shadows) => {
                    let (direct, indirect, shadows) = (e.direct, e.indirect, e.shadows);
                    let lights = lights.clone();
                    gpu(Some(loom_bevy::ElementLook::Lighting), &move |o| {
                        if !direct {
                            o.lights_off = lights.clone();
                        }
                        if !indirect {
                            o.ambient = [0.0; 3];
                            o.light_tracer = false;
                        }
                        o.shadows &= shadows;
                    })
                    .map(premultiplied)
                }
                ElementKind::IlluminanceHdr => gpu(Some(loom_bevy::ElementLook::Lighting), &|_| {}).map(premultiplied).map(|i| scaled(i, e.scale)),
                ElementKind::LuminanceHdr => Ok(scaled(premultiplied((*main).clone()), e.scale)),
                ElementKind::Blend => {
                    let b = e.blend;
                    let edit = move |o: &mut loom_bevy::RenderOptions| {
                        o.shadows &= b.shadows;
                        o.atmospherics &= b.atmosphere;
                    };
                    let mut sum = image::RgbaImage::new(job.out[0], job.out[1]);
                    let mut err = None;
                    for (on, look) in [(b.diffuse, loom_bevy::ElementLook::Diffuse), (b.specular, loom_bevy::ElementLook::Specular), (b.self_illumination, loom_bevy::ElementLook::SelfIllumination)] {
                        if on {
                            match gpu(Some(look), &edit) {
                                Ok(i) => add(&mut sum, &premultiplied(i)),
                                Err(x) => err = Some(x),
                            }
                        }
                    }
                    if b.reflection {
                        add(&mut sum, &passes::layer_image(layers.traced.as_deref(), job.out, |p| p.reflection));
                    }
                    if b.refraction {
                        add(&mut sum, &passes::layer_image(layers.traced.as_deref(), job.out, |p| p.refraction));
                    }
                    match err {
                        Some(x) => Err(x),
                        None => Ok(sum),
                    }
                }
                ElementKind::ObjectId => match &gbuffer {
                    Some(g) => {
                        let mut img = g.image(loom_commands::render_output::GChannel::ObjectId, None);
                        for (p, d) in img.pixels_mut().zip(&g.depth) {
                            p.0[3] = if d.is_some() { 255 } else { 0 };
                        }
                        Ok(img)
                    }
                    None => Err("no G-buffer".into()),
                },
                ElementKind::Matte => match &gbuffer {
                    Some(g) => Ok(image::RgbaImage::from_fn(job.out[0], job.out[1], |x, y| {
                        let i = (y * job.out[0] + x) as usize;
                        let hit = g.depth[i].is_some();
                        let listed = hit && e.matte_objects.contains(&g.node[i]);
                        let white = if e.matte_exclude { !listed } else { hit && (listed || e.matte_gbuffer.is_some_and(|id| g.object[i] == id)) };
                        let v = if white { 255 } else { 0 };
                        image::Rgba([v, v, v, 255])
                    })),
                    None => Err("no G-buffer".into()),
                },
                ElementKind::Alpha => Ok(image::RgbaImage::from_fn(main.width(), main.height(), |x, y| {
                    let a = main.get_pixel(x, y).0[3];
                    image::Rgba([a, a, a, 255])
                })),
                ElementKind::Background => gpu(None, &|o| o.selected_only = Some(Vec::new())).map(|mut img| {
                    for p in img.pixels_mut() {
                        p.0[3] = 255;
                    }
                    img
                }),
                ElementKind::Diffuse => gpu(Some(loom_bevy::ElementLook::Diffuse), &|_| {}).map(premultiplied),
                ElementKind::Specular => gpu(Some(loom_bevy::ElementLook::Specular), &|_| {}).map(premultiplied),
                ElementKind::Lighting => gpu(Some(loom_bevy::ElementLook::Lighting), &|_| {}).map(premultiplied),
                ElementKind::SelfIllumination => gpu(Some(loom_bevy::ElementLook::SelfIllumination), &|_| {}).map(premultiplied),
                ElementKind::Shadow => gpu(Some(loom_bevy::ElementLook::Lighting), &|_| {}).and_then(|lit| {
                    let open = gpu(Some(loom_bevy::ElementLook::Lighting), &|o| o.shadows = false)?;
                    Ok(loom_bevy::bake::shadow_ratio(&lit, &open))
                }),
                ElementKind::ZDepth => {
                    let (tris, _) = self.camera_triangles(&job.options);
                    let bvh = loom_bake::Bvh::new(tris.into_iter().map(|t| t.tri).collect());
                    let depth = loom_bake::gbuffer::depth(&bvh, &Self::gbuffer_view(job));
                    Ok(loom_bake::gbuffer::depth_image(&depth, job.out, e.z_min, e.z_max))
                }
                ElementKind::Reflection => Ok(passes::layer_image(layers.traced.as_deref(), job.out, |p| p.reflection)),
                ElementKind::Refraction => Ok(passes::layer_image(layers.traced.as_deref(), job.out, |p| p.refraction)),
                ElementKind::Velocity => Ok(match &layers.velocity {
                    Some((vel, seen)) => loom_bevy::raytrace::velocity_image(vel, seen, job.out),
                    None => image::RgbaImage::new(job.out[0], job.out[1]),
                }),
            };
            let img = match img {
                Ok(i) => i,
                Err(err) => {
                    message(&mut self.state, MessageLevel::Error, format!("Render element {}: {err}", e.name));
                    continue;
                }
            };
            let path = e.output_path(base.as_deref()).map(|p| match &numbering {
                Some((_, n)) => loom_scene::render::numbered_path(&p, *n),
                None => p,
            });
            if let Some(p) = &path {
                match loom_commands::render::save_output(&img, p, &s) {
                    Ok(()) => message(&mut self.state, MessageLevel::Info, format!("Render element {} saved to {p}", e.name)),
                    Err(err) => message(&mut self.state, MessageLevel::Error, format!("Render element {}: {err}", e.name)),
                }
            }
            if s.display_elements {
                let r = &mut self.state.rendered;
                // One window per element, its image replaced by each render.
                match r.clones.iter_mut().find(|c| c.title == e.name) {
                    Some(c) => c.image = Arc::new(img),
                    None => {
                        r.next_clone += 1;
                        let id = r.next_clone;
                        r.clones.push(loom_commands::render::ClonedFrame { id, title: e.name.clone(), image: Arc::new(img), channels: Default::default() });
                    }
                }
            }
        }
    }

    fn finish_render(&mut self, result: Result<(), String>) {
        let Some(mut job) = self.render_job.take() else { return };
        if let Gpu::Running(b) = &mut self.gpu {
            b.renderer.end_render();
        }
        let effects = if job.req.settings.effects { job.env.effects.clone() } else { Vec::new() };
        if job.preview.is_some() {
            if let Gpu::Running(b) = &mut self.gpu {
                b.renderer.sync_hidden(&[]);
            }
        }
        let mut layers = passes::PassLayers::default();
        if result.is_ok() && job.preview.is_none() && job.req.settings.force_wireframe {
            self.force_wireframe(&job);
        } else if result.is_ok() && job.preview.is_none() {
            // Motion blur and ray tracing on the finished frame, before its effects.
            if let Err(e) = self.object_motion_blur(&job) {
                loom_commands::render::message(&mut self.state, loom_commands::render::MessageLevel::Error, format!("Object Motion Blur: {e}"));
            }
            if let Err(e) = self.traced_shadows(&job) {
                loom_commands::render::message(&mut self.state, loom_commands::render::MessageLevel::Error, format!("Ray-traced shadows: {e}"));
            }
            self.trace_passes(&job, &mut layers);
            if let Err(e) = self.image_motion_blur(&job, &mut layers) {
                loom_commands::render::message(&mut self.state, loom_commands::render::MessageLevel::Error, format!("Image Motion Blur: {e}"));
            }
        }
        if job.req.active_shade {
            if let (Ok(()), Some(mut img)) = (&result, job.preview.take()) {
                loom_bevy::effects::apply(&mut img, &loom_bevy::effects::without_files(&effects));
                let a = &mut self.state.rendered.active_shade;
                a.image = Some(Arc::new(img));
                a.version += 1;
                a.renders += 1;
            }
            return;
        }
        if let Some(mut img) = job.preview.take() {
            if result.is_ok() {
                loom_bevy::effects::apply(&mut img, &loom_bevy::effects::without_files(&effects));
                let ui = &mut self.state.environment_ui;
                ui.preview = Some(Arc::new(img));
                ui.preview_version += 1;
            }
            return;
        }
        if result.is_ok() && !effects.is_empty() {
            // Rendering effects work on the finished frame, in order; a
            // Motion Blur entry smears the Image-blurred objects at its place.
            let mut start = 0;
            for end in (0..effects.len()).filter(|i| matches!(effects[*i], loom_bevy::environment::ImageEffect::MotionBlur { .. })).chain([effects.len()]) {
                if let Some(img) = self.state.rendered.image.as_mut() {
                    for e in loom_bevy::effects::apply(Arc::make_mut(img), &effects[start..end]) {
                        loom_commands::render::message(&mut self.state, loom_commands::render::MessageLevel::Error, e);
                    }
                    self.state.rendered.version += 1;
                    self.state.rendered.dirty = None;
                }
                if let Some(loom_bevy::environment::ImageEffect::MotionBlur { duration }) = effects.get(end) {
                    if let Err(e) = self.motion_blur_effect(&job, *duration, &effects[..end]) {
                        loom_commands::render::message(&mut self.state, loom_commands::render::MessageLevel::Error, format!("Motion Blur: {e}"));
                    }
                }
                start = end + 1;
            }
        }
        if result.is_ok() && self.state.prefs.dont_antialias_background {
            if let Some(img) = self.state.rendered.image.as_mut() {
                loom_commands::render::dont_antialias_against_background(Arc::make_mut(img));
                self.state.rendered.version += 1;
            }
        }
        use loom_commands::render::{message, MessageLevel};
        let s = &job.req.settings;
        if result.is_ok() && (s.video_color_check || s.super_black) {
            // Options > Video Color Check and Super Black, on the finished frame.
            let (check, black, threshold) = (s.video_color_check, s.super_black, s.super_black_threshold);
            let mut flagged = 0;
            if let Some(img) = self.state.rendered.image.as_mut() {
                let img = Arc::make_mut(img);
                if check {
                    flagged = loom_commands::render::video_color_check(img);
                }
                if black {
                    loom_commands::render::super_black(img, threshold);
                }
                self.state.rendered.version += 1;
                self.state.rendered.dirty = None;
            }
            if flagged > 0 {
                message(&mut self.state, MessageLevel::Warning, format!("Video Color Check: {flagged} pixel(s) beyond the NTSC limit rendered black"));
            }
        }
        self.state.rendered.progress = None;
        self.state.rendered.cancel = false;
        if self.fast_skins() {
            // The viewports go back to the Fast Updates preview.
            self.rebuild_world_meshes();
        }
        let secs = job.elapsed().as_secs_f32();
        let ok = result.is_ok();
        let frame = self.state.current_frame;
        match result {
            Ok(()) => {
                self.state.status_prompt = format!("Rendered {}x{} in {secs:.1} s", job.out[0], job.out[1]);
                // Render Termination Alert: the taskbar entry asks for attention
                // (this build has no audio output, so no beep or sound file).
                self.render_attention = self.state.prefs.render_termination_alert;
                let mut saved = None;
                if let (Some(path), Some(img)) = (&job.req.save_path, &self.state.rendered.image) {
                    match loom_commands::render::save_output(img, path, &job.req.settings) {
                        Ok(()) => {
                            self.state.status_prompt = format!("Rendered {}x{} in {secs:.1} s to {path}", job.out[0], job.out[1]);
                            saved = Some(Ok(path.clone()));
                        }
                        Err(e) => saved = Some(Err(e)),
                    }
                }
                let text = format!("Frame {frame}: rendered {}x{} in {secs:.1} s", job.out[0], job.out[1]);
                match saved {
                    Some(Ok(p)) => message(&mut self.state, MessageLevel::Info, format!("{text}, saved to {p}")),
                    Some(Err(e)) => {
                        message(&mut self.state, MessageLevel::Error, format!("{text}, not saved: {e}"));
                        self.last_error = Some(format!("Render: {e}"));
                    }
                    None => message(&mut self.state, MessageLevel::Info, text),
                }
                if !job.req.iterative && job.req.cube_face.is_none() {
                    // The G-buffer: the window's channel list and the ID elements.
                    let g = self.gbuffer(&job);
                    self.state.rendered.gbuffer = Some(Arc::new(g));
                    self.state.rendered.viewer = None;
                }
                let s = &job.req.settings;
                if !job.req.iterative && s.elements_active && s.elements.iter().any(|e| e.enabled) {
                    self.render_elements(&job, &layers);
                }
            }
            Err(e) if e == "cancelled" => {
                self.state.status_prompt = format!("Render cancelled after {secs:.1} s");
                message(&mut self.state, MessageLevel::Info, format!("Frame {frame}: render cancelled after {secs:.1} s"));
            }
            Err(e) => {
                message(&mut self.state, MessageLevel::Error, format!("Frame {frame}: {e}"));
                self.last_error = Some(format!("Render: {e}"));
            }
        }
        if let Gpu::Running(b) = &mut self.gpu {
            b.renderer.sync_hidden(&[]);
        }
        if ok {
            // A Time Output sequence: the next frame (its meshes rebuilt at that frame).
            match loom_commands::render::sequence_next(&mut self.state) {
                Some(Err(e)) => self.last_error = Some(format!("Render: {e}")),
                Some(Ok(_)) | None => self.sync_after_command(),
            }
        } else {
            loom_commands::render::end_sequence(&mut self.state);
            loom_commands::render_output::end_runs(&mut self.state);
            self.sync_after_command();
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
        let b = buckets([0, 0, 600, 300], 256);
        assert_eq!(b, vec![[0, 0], [256, 0], [512, 0], [0, 256], [256, 256], [512, 256]]);
        assert_eq!(buckets([100, 100, 110, 120], 256), vec![[100, 100]]);
    }

    /// An area that fits renders whole (the screen-space effects then see
    /// all of it); a large or heavily supersampled one in 256 px buckets.
    #[test]
    fn small_enough_areas_render_as_one_bucket() {
        assert_eq!(bucket_size([0, 0, 1280, 720], 1), 1280);
        assert_eq!(buckets([0, 0, 1280, 720], 1280), vec![[0, 0]]);
        assert_eq!(bucket_size([0, 0, 1280, 720], 4), BUCKET);
        assert_eq!(bucket_size([0, 0, 8000, 100], 1), BUCKET);
        assert_eq!(bucket_size([100, 50, 300, 450], 2), 400);
    }
}
