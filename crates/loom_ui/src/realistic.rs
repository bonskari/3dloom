//! Realistic viewport shading and Render, drawn by Bevy (`loom_bevy`) on the
//! UI's wgpu device. A Realistic viewport shows Bevy's image as its
//! background; the overlays are painted over it as in the other modes.

use crate::LoomApp;
use eframe::egui;
use glam::Mat4;
use std::collections::HashMap;

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

    fn sync_bevy(&mut self) {
        let version = self.world_version;
        let objects: Vec<(u64, usize, [u8; 3])> = self
            .world_meshes()
            .iter()
            .enumerate()
            .filter_map(|(i, (id, _))| self.state.scene.node(*id).map(|n| (id.0, i, n.wire_color)))
            .collect();
        let display = self.world_display.clone();
        let scene = &self.state.scene;
        let Gpu::Running(b) = &mut self.gpu else { return };
        if b.synced == Some(version) {
            return;
        }
        let list: Vec<loom_bevy::SceneObject> = objects
            .iter()
            .filter_map(|(id, i, wire)| {
                Some(loom_bevy::SceneObject { id: *id, mesh: display.get(*i)?, wire_color: *wire, material: scene.node_material(loom_scene::NodeId(*id)) })
            })
            .collect();
        b.renderer.sync(&list);
        b.synced = Some(version);
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

    /// End of the frame: Bevy renders the Realistic views requested this
    /// frame (before egui paints them), then any pending Render runs.
    pub(crate) fn finish_realistic_frame(&mut self, ctx: &egui::Context) {
        if matches!(self.gpu, Gpu::Running(_)) {
            self.sync_bevy();
            let Gpu::Running(b) = &mut self.gpu else { return };
            b.renderer.update();
            if let Ok(p) = std::env::var("LOOM_BEVY_DEBUG") {
                for slot in 0..4u32 {
                    if let Ok(img) = b.renderer.debug_read(slot) {
                        let _ = img.save(format!("{p}-{slot}.png"));
                    }
                }
            }
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
        }
        if let Some(req) = self.state.pending_render.take() {
            match self.render_to_file(&req) {
                Ok(()) => {
                    self.last_error = None;
                    self.state.status_prompt = format!("Rendered {}x{} to {}", req.width, req.height, req.path);
                }
                Err(e) => self.last_error = Some(format!("Render: {e}")),
            }
        }
    }

    fn render_to_file(&mut self, req: &loom_commands::RenderRequest) -> Result<(), String> {
        let camera = self.camera_for(req.viewport);
        let aspect = req.width as f32 / req.height as f32;
        let (view, projection) = (camera.view_matrix(), camera.projection_matrix(aspect));
        if self.bevy().is_none() {
            return Err("no GPU renderer".into());
        }
        self.sync_bevy();
        let Gpu::Running(b) = &mut self.gpu else { return Err("no GPU renderer".into()) };
        let img = b.renderer.render_image(req.width, req.height, &loom_bevy::ViewCamera { view, projection })?;
        img.save(&req.path).map_err(|e| format!("{}: {e}", req.path))
    }
}
