//! The passes a finished Production frame goes through on the CPU before
//! its effects: Object Motion Blur (the frame again at the shutter's
//! times, averaged), ray-traced reflections and refractions, the Light
//! Tracer's light and Image Motion Blur, each traced through the render's
//! triangles (`loom_bevy::raytrace`).

use super::{render_flags, Gpu, RenderJob};
use crate::LoomApp;
use glam::{Mat4, Vec3};
use loom_bevy::raytrace::{self, TraceObject, TraceScene, TraceSettings, TraceSurface, TracedPixel, TracerSettings};
use loom_scene::render::ColorRange;
use loom_scene::{MotionBlur, NodeId};
use std::collections::HashMap;
use std::sync::Arc;

/// Per output pixel: the screen motion, and whether something is seen there.
pub type Motion = (Vec<Option<[f32; 2]>>, Vec<bool>);

/// What the passes keep for the render elements.
#[derive(Default)]
pub struct PassLayers {
    /// Per output pixel: the traced reflection and refraction.
    pub traced: Option<Vec<TracedPixel>>,
    pub velocity: Option<Motion>,
}

use raytrace::lin3;

fn lin_px(p: [u8; 4]) -> Vec3 {
    Vec3::new(raytrace::to_linear(p[0]), raytrace::to_linear(p[1]), raytrace::to_linear(p[2]))
}

fn srgb_px(c: [f32; 3], alpha: u8) -> image::Rgba<u8> {
    image::Rgba([raytrace::to_srgb(c[0]), raytrace::to_srgb(c[1]), raytrace::to_srgb(c[2]), alpha])
}

impl LoomApp {
    /// Node ids of the render's objects whose Motion Blur is `kind`.
    fn blurred(&self, kind: MotionBlur) -> Vec<u64> {
        let scene = &self.state.scene;
        self.world_meshes().iter().map(|(id, _)| *id).filter(|id| !self.is_light_proxy(*id)).filter(|id| scene.node(*id).is_some_and(|n| scene.renders(n) && scene.render_of(n).blur() == kind)).map(|id| id.0).collect()
    }

    /// The render's objects for Bevy, those in `moved` with the mesh given there.
    fn bevy_objects<'a>(&'a self, moved: &'a HashMap<u64, loom_mesh::Mesh>) -> Vec<loom_bevy::SceneObject<'a>> {
        let scene = &self.state.scene;
        self.world_meshes()
            .iter()
            .enumerate()
            .filter(|(_, (id, _))| !self.is_light_proxy(*id))
            .filter_map(|(i, (id, _))| {
                let n = scene.node(*id)?;
                let mesh = moved.get(&id.0).or_else(|| self.world_display().get(i).map(|m| &**m))?;
                Some(loom_bevy::SceneObject { id: id.0, mesh, wire_color: scene.display_color(n), surfaces: scene.node_surfaces_for(*id, loom_scene::material::ShellView::Render), renderable: scene.renders(n), flags: render_flags(scene, *id) })
            })
            .collect()
    }

    /// Object Motion Blur: the frame rendered again at each shutter time of
    /// Samples out of Duration Subdivisions, the blurred objects moved to
    /// that time, and the renders averaged.
    pub(super) fn object_motion_blur(&mut self, job: &RenderJob) -> Result<(), String> {
        let s = job.req.settings.clone();
        if !s.bevy.object_motion_blur {
            return Ok(());
        }
        let ids = self.blurred(MotionBlur::Object);
        if ids.is_empty() {
            return Ok(());
        }
        let frame = self.state.current_frame as f32;
        let times = s.bevy.object_blur_times(frame);
        let rect = Self::gbuffer_view(job).rect;
        let mut options = job.options.clone();
        // The samples share the passes: as many renders in all as one frame takes.
        options.quality.passes = s.bevy.passes.clamp(1, 64).div_ceil(times.len() as u32).max(1);
        let mut sum = vec![[0.0f32; 4]; (job.out[0] * job.out[1]) as usize];
        // The renderer is set aside while the object lists borrow the app.
        let mut gpu = std::mem::replace(&mut self.gpu, Gpu::None);
        let result = self.blur_samples(&mut gpu, job, &ids, &times, rect, &options, &mut |_, img| {
            for (acc, p) in sum.iter_mut().zip(img.pixels()) {
                let c = lin_px(p.0);
                // The renders' colour already lies over the background.
                acc[0] += c.x;
                acc[1] += c.y;
                acc[2] += c.z;
                acc[3] += p.0[3] as f32 / 255.0;
            }
        });
        // The objects go back to the frame.
        let none = HashMap::new();
        if let Gpu::Running(b) = &mut gpu {
            b.renderer.sync(&self.bevy_objects(&none));
        }
        self.gpu = gpu;
        result?;
        let n = times.len() as f32;
        let [x0, y0, x1, y1] = job.mask.unwrap_or([0, 0, job.out[0], job.out[1]]);
        let Some(img) = self.state.rendered.image.as_mut() else { return Ok(()) };
        let img = Arc::make_mut(img);
        for y in y0..y1.min(img.height()) {
            for x in x0..x1.min(img.width()) {
                let acc = sum[(y * job.out[0] + x) as usize];
                let a = acc[3] / n;
                let c = [0, 1, 2].map(|k| acc[k] / n);
                img.put_pixel(x, y, srgb_px(c, (a * 255.0).round() as u8));
            }
        }
        self.state.rendered.version += 1;
        self.state.rendered.dirty = None;
        Ok(())
    }

    /// Renders the frame at each of `times` with the objects `ids` moved
    /// there, handing each render to `add` with its index.
    #[allow(clippy::too_many_arguments)]
    fn blur_samples(&self, gpu: &mut Gpu, job: &RenderJob, ids: &[u64], times: &[f32], rect: [f32; 4], options: &loom_bevy::RenderOptions, add: &mut dyn FnMut(u32, &image::RgbaImage)) -> Result<(), String> {
        let Gpu::Running(b) = gpu else { return Err("no GPU renderer".into()) };
        for (i, &t) in times.iter().enumerate() {
            let scene = &self.state.scene;
            let moved: HashMap<u64, loom_mesh::Mesh> = ids
                .iter()
                .filter_map(|id| {
                    let id = NodeId(*id);
                    scene.at_time(t, || {
                        let mut m = scene.eval_mesh(id).value.mesh.clone();
                        loom_mesh::edit_ops::transform_mesh(&mut m, scene.object_matrix(id)).ok()?;
                        Some((id.0, m))
                    })
                })
                .collect();
            let camera = self.state.scene.at_time(t, || {
                let c = self.camera_for(job.req.viewport);
                loom_bevy::ViewCamera { view: c.view_matrix(), projection: c.projection_matrix(job.aspect) }
            });
            b.renderer.sync(&self.bevy_objects(&moved));
            let img = b.renderer.render_frame_with(job.out, &camera, job.aspect, rect, options.clone())?;
            add(i as u32, &img);
        }
        Ok(())
    }

    /// A camera's Multi-Pass Motion Blur: the frame rendered at each pass's
    /// time with every object and the camera there, blended by the passes'
    /// dithered weights (normalized, or over the pass count). False when
    /// the render's camera has no such effect.
    pub(super) fn camera_motion_blur(&mut self, job: &RenderJob) -> Result<bool, String> {
        let Some(id) = self.multi_pass_camera(&job.req) else { return Ok(false) };
        let Some(loom_scene::ObjectKind::Camera { mp_effect, mp_blur, .. }) = self.state.scene.node(id).map(|n| &n.object) else { return Ok(false) };
        if mp_effect.get() != 1 {
            return Ok(false);
        }
        let blur = mp_blur.clone();
        let times = blur.times(self.state.current_frame as f32);
        let ids: Vec<u64> = self.world_meshes().iter().map(|(id, _)| *id).filter(|id| !self.is_light_proxy(*id)).map(|id| id.0).collect();
        let rect = Self::gbuffer_view(job).rect;
        let mut options = job.options.clone();
        options.quality.passes = 1;
        if blur.disable_antialiasing.get() {
            options.quality.antialiasing = false;
        }
        if blur.disable_filtering.get() {
            options.quality.filter = loom_scene::render::AaFilter::Area;
            options.quality.filter_size = 1.0;
        }
        let w = job.out[0];
        let mut sum = vec![[0.0f32; 5]; (job.out[0] * job.out[1]) as usize];
        let mut gpu = std::mem::replace(&mut self.gpu, Gpu::None);
        let result = self.blur_samples(&mut gpu, job, &ids, &times, rect, &options, &mut |pass, img| {
            for (i, (acc, p)) in sum.iter_mut().zip(img.pixels()).enumerate() {
                let k = blur.weight(pass, i as u32 % w, i as u32 / w);
                let c = lin_px(p.0);
                acc[0] += k * c.x;
                acc[1] += k * c.y;
                acc[2] += k * c.z;
                acc[3] += k * p.0[3] as f32 / 255.0;
                acc[4] += k;
            }
        });
        let none = HashMap::new();
        if let Gpu::Running(b) = &mut gpu {
            b.renderer.sync(&self.bevy_objects(&none));
        }
        self.gpu = gpu;
        result?;
        let n = times.len() as f32;
        let [x0, y0, x1, y1] = job.mask.unwrap_or([0, 0, job.out[0], job.out[1]]);
        let Some(img) = self.state.rendered.image.as_mut() else { return Ok(true) };
        let img = Arc::make_mut(img);
        for y in y0..y1.min(img.height()) {
            for x in x0..x1.min(img.width()) {
                let acc = sum[(y * w + x) as usize];
                let total = if blur.normalize_weights.get() { acc[4].max(1e-6) } else { n };
                let c = [0, 1, 2].map(|k| acc[k] / total);
                img.put_pixel(x, y, srgb_px(c, ((acc[3] / total).clamp(0.0, 1.0) * 255.0).round() as u8));
            }
        }
        self.state.rendered.version += 1;
        self.state.rendered.dirty = None;
        Ok(true)
    }

    /// The render's scene for the CPU passes: every renderable object's
    /// triangles with its surfaces and rendering control, and the light.
    fn trace_scene(&self, job: &RenderJob) -> TraceScene {
        let scene = &self.state.scene;
        let options = &job.options;
        let s = &job.req.settings;
        let hidden = self.hidden_meshes(options.hidden);
        let mut meshes: Vec<(NodeId, &loom_mesh::Mesh)> = (0..self.world_meshes().len()).map(|i| (self.world_meshes()[i].0, self.drawn_mesh(i))).collect();
        meshes.extend(hidden.iter().map(|(id, m)| (*id, m)));
        let (mut tris, mut info, mut objects, mut surfaces) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for (id, mesh) in meshes {
            let Some(n) = scene.node(id) else { continue };
            let flags = render_flags(scene, id);
            if self.is_light_proxy(id) || !scene.renders(n) || flags.visibility <= 0.0 {
                continue;
            }
            let r = scene.render_of(n);
            let camera = flags.visible_to_camera && options.selected_only.as_ref().is_none_or(|sel| sel.contains(&id.0));
            let rays = r.visible_to_reflection && !s.raytracer.exclude.contains(&id.0);
            let obj = objects.len() as u32;
            objects.push(TraceObject { id: id.0, camera, rays, cast_shadows: flags.cast_shadows, receive_shadows: flags.receive_shadows });
            let looks = scene.node_surfaces_for(id, loom_scene::material::ShellView::Render);
            let wire = scene.display_color(n).map(|c| c as f32 / 255.0);
            let mut by_id: HashMap<u16, u32> = HashMap::new();
            for t in loom_bake::triangles(mesh, 1) {
                let surf = *by_id.entry(t.material_id).or_insert_with(|| {
                    let look = match &looks {
                        Some(l) => {
                            let f = l.for_face(t.material_id);
                            TraceSurface::from_surface(&f, &self.trace_images(&f))
                        }
                        None => TraceSurface { diffuse: lin3(wire), glow: Vec3::ZERO, trace: None, transmit: Vec3::ZERO, maps: None },
                    };
                    surfaces.push(look);
                    surfaces.len() as u32 - 1
                });
                tris.push(t);
                info.push((obj, surf));
            }
        }
        let lighting = match &self.gpu {
            Gpu::Running(b) => b.renderer.trace_lighting(options.ambient),
            _ => Default::default(),
        };
        let scene = TraceScene::new(tris, info, objects, surfaces, lighting, lin3(job.env.background));
        // Reflections and refractions that leave the scene see the Environment Map.
        match job.env.map.as_ref().and_then(|(path, mapping)| image::open(path).ok().map(|i| (std::sync::Arc::new(i.to_rgba8()), *mapping))) {
            Some((img, mapping)) => scene.with_environment(img, mapping),
            None => scene,
        }
    }

    /// The bitmaps a surface's traced maps read: those the renderer has
    /// loaded, any other read from disk.
    fn trace_images(&self, f: &loom_scene::material::Surface) -> Arc<loom_scene::maps::Images> {
        if f.diffuse_map.is_none() && f.self_illum_map.is_none() && f.opacity_map.is_none() {
            return Arc::default();
        }
        let mut images = match &self.gpu {
            Gpu::Running(b) => b.renderer.images().clone(),
            _ => Default::default(),
        };
        for map in [&f.diffuse_map, &f.self_illum_map, &f.opacity_map].into_iter().flatten() {
            loom_scene::maps::load_images(map, &mut images, &mut |path| {
                image::open(path).ok().map(|img| {
                    let rgba = img.to_rgba8();
                    Arc::new(loom_scene::maps::Image::new(rgba.width(), rgba.height(), rgba.into_raw()))
                })
            });
        }
        Arc::new(images)
    }

    fn trace_settings(job: &RenderJob) -> TraceSettings {
        let s = &job.req.settings;
        let r = &s.raytracer;
        TraceSettings {
            max_depth: r.max_depth,
            cutoff: r.cutoff,
            max_depth_color: r.max_depth_color.map(lin3),
            antialias: r.antialias,
            samples: r.samples,
            blur: r.blur,
            self_reflect: r.self_reflect,
            auto_reflect: s.bevy.auto_reflect,
            reflect_iterations: s.bevy.reflect_iterations,
            shadows: s.bevy.shadows,
            pixel_samples: if s.bevy.antialiasing { 2 } else { 1 },
        }
    }

    /// Ray-traced reflections and refractions over the finished frame, and
    /// the Light Tracer's light added to it (Color Range Limiting applies).
    pub(super) fn trace_passes(&mut self, job: &RenderJob, layers: &mut PassLayers) {
        let s = job.req.settings.clone();
        let tracer = s.advanced.light_tracer();
        // Nothing to trace without a traced surface or the Light Tracer.
        let any_traced = || {
            let scene = &self.state.scene;
            self.world_meshes().iter().any(|(id, _)| match scene.node_surfaces_for(*id, loom_scene::material::ShellView::Render) {
                Some(loom_scene::material::Surfaces::Single(s)) => s.trace.is_some(),
                Some(loom_scene::material::Surfaces::PerId(m)) => m.values().any(|s| s.trace.is_some()),
                None => false,
            })
        };
        if !tracer && !(s.raytracer.enabled && any_traced()) {
            return;
        }
        let scene = self.trace_scene(job);
        let view = Self::gbuffer_view(job);
        let ts = Self::trace_settings(job);
        let traced = (s.raytracer.enabled && scene.traces(&ts)).then(|| scene.reflections(&view, &ts));
        let gathered = tracer.then(|| {
            let a = &s.advanced;
            let t = TracerSettings {
                rays: a.rays_per_sample,
                bounces: a.bounces,
                cone_angle: a.cone_angle,
                ray_bias: a.ray_bias,
                color_bleed: a.color_bleed,
                object_multiplier: a.object_multiplier,
                sky: if a.sky_lights_on { scene.lighting.sky * a.sky_lights } else { Vec3::ZERO },
                shadows: s.bevy.shadows,
            };
            raytrace::filter_gathered(&scene.gather(&view, &t), job.out, a.filter_size)
        });
        let (filter, extra) = (lin3(s.advanced.color_filter) * s.advanced.global_multiplier, lin3(s.advanced.extra_ambient));
        let range = s.bevy.color_range;
        let [x0, y0, x1, y1] = job.mask.unwrap_or([0, 0, job.out[0], job.out[1]]);
        if let Some(img) = self.state.rendered.image.as_mut() {
            let img = Arc::make_mut(img);
            for y in y0..y1.min(img.height()) {
                for x in x0..x1.min(img.width()) {
                    let i = (y * job.out[0] + x) as usize;
                    let p = img.get_pixel(x, y).0;
                    let mut c = lin_px(p);
                    let mut changed = false;
                    if let Some(g) = gathered.as_ref().and_then(|g| g[i]) {
                        let (albedo, light) = g;
                        c += albedo * (light * filter + extra);
                        changed = true;
                    }
                    if let Some(t) = traced.as_ref().map(|t| t[i]).filter(|t| t.coverage > 0.0) {
                        c = c * t.keep + t.reflection + t.refraction;
                        changed = true;
                    }
                    if changed {
                        img.put_pixel(x, y, srgb_px(range.limit(c.to_array()), p[3]));
                    }
                }
            }
            self.state.rendered.version += 1;
            self.state.rendered.dirty = None;
        }
        layers.traced = traced;
    }

    /// Ray-traced shadows (Ray Traced, Adv. Ray Traced, Area Shadows): the
    /// frame rendered with each such light off gives its share of the
    /// light; the share the traced shadow rays find blocked comes off.
    pub(super) fn traced_shadows(&mut self, job: &RenderJob) -> Result<(), String> {
        let s = job.req.settings.clone();
        if !s.bevy.shadows || job.options.traced_shadows.is_empty() {
            return Ok(());
        }
        let lights: Vec<loom_scene::light::LightSample> = match &self.gpu {
            Gpu::Running(b) => b.renderer.trace_lighting(job.options.ambient).lights,
            _ => return Ok(()),
        };
        let scene = self.trace_scene(job);
        let view = Self::gbuffer_view(job);
        let ss = if s.bevy.antialiasing { 2 } else { 1 };
        let Some(lit) = self.state.rendered.image.as_ref().map(|i| (**i).clone()) else { return Ok(()) };
        let mut darken = vec![Vec3::ZERO; (job.out[0] * job.out[1]) as usize];
        for l in lights.iter().filter(|l| job.options.traced_shadows.contains(&l.id.0)) {
            let Some(mut shadow) = l.traced else { continue };
            if s.area_as_points {
                shadow = loom_scene::light::TracedShadow { radius: 0.0, samples: 1, ..shadow };
            }
            let open = scene.shadows(&view, l, shadow, ss);
            let mut options = job.options.clone();
            options.lights_off = vec![l.id.0];
            let Gpu::Running(b) = &mut self.gpu else { return Err("no GPU renderer".into()) };
            let without = b.renderer.render_frame_with(job.out, &job.camera, job.aspect, view.rect, options)?;
            for (i, (a, b)) in lit.pixels().zip(without.pixels()).enumerate() {
                // The light's own share of the pixel, and how much of it the shadow stops.
                let share = (lin_px(a.0) - lin_px(b.0)).max(Vec3::ZERO);
                darken[i] += share * (Vec3::ONE - open[i]);
            }
        }
        let [x0, y0, x1, y1] = job.mask.unwrap_or([0, 0, job.out[0], job.out[1]]);
        if let Some(img) = self.state.rendered.image.as_mut() {
            let img = Arc::make_mut(img);
            for y in y0..y1.min(img.height()) {
                for x in x0..x1.min(img.width()) {
                    let d = darken[(y * job.out[0] + x) as usize];
                    if d != Vec3::ZERO {
                        let p = img.get_pixel(x, y).0;
                        img.put_pixel(x, y, srgb_px((lin_px(p) - d).max(Vec3::ZERO).to_array(), p[3]));
                    }
                }
            }
            self.state.rendered.version += 1;
            self.state.rendered.dirty = None;
        }
        Ok(())
    }

    /// World transforms taking each traced object from frame `t` to `t0`
    /// and `t1` (None for objects outside `ids`, or that do not move).
    fn motion(&self, scene: &TraceScene, ids: Option<&[u64]>, t: f32, half: impl Fn(u64) -> f32) -> Vec<Option<(Mat4, Mat4)>> {
        let sc = &self.state.scene;
        scene
            .objects
            .iter()
            .map(|o| {
                if ids.is_some_and(|ids| !ids.contains(&o.id)) {
                    return None;
                }
                let id = NodeId(o.id);
                let h = half(o.id);
                let at = |time: f32| sc.at_time(time, || sc.object_matrix(id));
                let now = at(t).inverse();
                let (a, b) = (at(t - h) * now, at(t + h) * now);
                (a != Mat4::IDENTITY || b != Mat4::IDENTITY).then_some((a, b))
            })
            .collect()
    }

    /// Image Motion Blur over the finished frame: each Image-blurred
    /// object's pixels smeared along their motion over the shutter
    /// (Duration × the object's Multiplier) over the frame rendered without
    /// them; with Apply to Environment Map, the background smears by the
    /// camera's turn. The Velocity element's motion is kept too.
    pub(super) fn image_motion_blur(&mut self, job: &RenderJob, layers: &mut PassLayers) -> Result<(), String> {
        let s = job.req.settings.clone();
        let wants_velocity = s.elements_active && s.elements.iter().any(|e| e.enabled && e.kind == loom_scene::render::ElementKind::Velocity) && !job.req.iterative;
        let ids = if s.bevy.image_motion_blur { self.blurred(MotionBlur::Image) } else { Vec::new() };
        let env = s.bevy.image_motion_blur && s.bevy.image_blur_environment;
        if ids.is_empty() && !env && !wants_velocity {
            return Ok(());
        }
        let scene = self.trace_scene(job);
        let view = Self::gbuffer_view(job);
        let visible = scene.visible(&view);
        let t = self.state.current_frame as f32;
        let d = s.bevy.image_blur_duration.max(0.0);
        if wants_velocity {
            // The Velocity element: every object's motion over one frame.
            let motion = self.motion(&scene, None, t, |_| 0.5);
            let vel = raytrace::velocities(&visible, &motion, &view);
            layers.velocity = Some((vel, visible.iter().map(|v| v.is_some()).collect()));
        }
        let mut img = match self.state.rendered.image.as_ref() {
            Some(i) => (**i).clone(),
            None => return Ok(()),
        };
        if env {
            // The background (where nothing is seen) smears by the camera's turn.
            let cam = |time: f32| {
                self.state.scene.at_time(time, || {
                    let c = self.camera_for(job.req.viewport);
                    loom_bake::gbuffer::View { view: c.view_matrix(), ..view }
                })
            };
            let (v0, v1) = (cam(t - d * 0.5), cam(t + d * 0.5));
            let world_from_clip = (view.clip_from_view * view.view).inverse();
            let (c0, c1) = (v0.clip_from_view * v0.view, v1.clip_from_view * v1.view);
            let (e0, e1) = (v0.view.inverse().w_axis.truncate(), v1.view.inverse().w_axis.truncate());
            let vel: Vec<Option<[f32; 2]>> = (0..job.out[0] * job.out[1])
                .map(|i| {
                    if visible[i as usize].is_some() {
                        return None;
                    }
                    let (x, y) = ((i % job.out[0]) as f32 + 0.5, (i / job.out[0]) as f32 + 0.5);
                    let (_, dir, _) = view.ray(world_from_clip, x, y);
                    // A direction far away: only the camera's turn moves it on the image.
                    let p0 = v0.project(c0, e0 + dir * 1.0e6)?;
                    let p1 = v1.project(c1, e1 + dir * 1.0e6)?;
                    let v = [p1[0] - p0[0], p1[1] - p0[1]];
                    (v[0].abs() + v[1].abs() > 1e-3).then_some(v)
                })
                .collect();
            img = raytrace::smear_background(&img, &vel);
        }
        if !ids.is_empty() {
            img = self.smear_objects(job, &scene, &view, &visible, &ids, d, img, &[])?;
        }
        self.put_rendered(job, &img);
        Ok(())
    }

    /// The Image-blurred objects `ids` in `img` smeared along their motion
    /// over `duration` frames (× each one's Multiplier) over the frame
    /// rendered without them, which first gets the effects `before`.
    #[allow(clippy::too_many_arguments)]
    fn smear_objects(&mut self, job: &RenderJob, scene: &TraceScene, view: &loom_bake::gbuffer::View, visible: &[Option<(u32, Vec3)>], ids: &[u64], duration: f32, img: image::RgbaImage, before: &[loom_bevy::environment::ImageEffect]) -> Result<image::RgbaImage, String> {
        let t = self.state.current_frame as f32;
        let mult = |id: u64| self.state.scene.node(NodeId(id)).map_or(1.0, |n| n.render.motion_blur_multiplier);
        let motion = self.motion(scene, Some(ids), t, |id| duration * 0.5 * mult(id));
        let vel = raytrace::velocities(visible, &motion, view);
        if !vel.iter().any(|v| v.is_some()) {
            return Ok(img);
        }
        let mut options = job.options.clone();
        options.hide = ids.to_vec();
        let Gpu::Running(b) = &mut self.gpu else { return Err("no GPU renderer".into()) };
        let mut behind = b.renderer.render_frame_with(job.out, &job.camera, job.aspect, view.rect, options)?;
        loom_bevy::effects::apply(&mut behind, &loom_bevy::effects::without_files(before));
        Ok(raytrace::smear(&img, &behind, &vel))
    }

    /// Rendering Effects > Motion Blur at its place in the effects list:
    /// the objects with Image Motion Blur on smeared over `duration`
    /// frames; the frame behind them gets the effects `before` it.
    pub(super) fn motion_blur_effect(&mut self, job: &RenderJob, duration: f32, before: &[loom_bevy::environment::ImageEffect]) -> Result<(), String> {
        let ids = self.blurred(MotionBlur::Image);
        let Some(img) = self.state.rendered.image.as_ref().map(|i| (**i).clone()) else { return Ok(()) };
        if ids.is_empty() {
            return Ok(());
        }
        let scene = self.trace_scene(job);
        let view = Self::gbuffer_view(job);
        let visible = scene.visible(&view);
        let img = self.smear_objects(job, &scene, &view, &visible, &ids, duration.max(0.0), img, before)?;
        self.put_rendered(job, &img);
        Ok(())
    }

    /// Writes `img` into the rendered frame (inside the region mask).
    fn put_rendered(&mut self, job: &RenderJob, img: &image::RgbaImage) {
        let [x0, y0, x1, y1] = job.mask.unwrap_or([0, 0, job.out[0], job.out[1]]);
        if let Some(out) = self.state.rendered.image.as_mut() {
            let out = Arc::make_mut(out);
            for y in y0..y1.min(out.height()) {
                for x in x0..x1.min(out.width()) {
                    out.put_pixel(x, y, *img.get_pixel(x, y));
                }
            }
            self.state.rendered.version += 1;
            self.state.rendered.dirty = None;
        }
    }
}

/// A traced layer as an element image: its colour where traced surfaces
/// are, black and transparent elsewhere.
pub fn layer_image(px: Option<&[TracedPixel]>, size: [u32; 2], pick: impl Fn(&TracedPixel) -> Vec3) -> image::RgbaImage {
    image::RgbaImage::from_fn(size[0], size[1], |x, y| {
        let Some(p) = px.map(|v| v[(y * size[0] + x) as usize]) else { return image::Rgba([0, 0, 0, 0]) };
        let c = ColorRange::Clamp.limit(pick(&p).to_array());
        srgb_px(c, (p.coverage * 255.0).round() as u8)
    })
}
