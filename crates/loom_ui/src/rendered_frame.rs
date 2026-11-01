//! The Rendered Frame Window: the rendered image at 1:1 with zoom and pan,
//! the rendering controls (Area to Render, Viewport, Lock to Viewport,
//! Production / Iterative, Render), the toolbar (Save, Copy, Clone, Clear,
//! channels, pixel colour) and the render's progress with Pause / Cancel;
//! its clones; and the render region rectangle in the viewport.

use crate::LoomApp;
use eframe::egui;
use loom_commands::render::Channels;
use loom_commands::Command;
use loom_scene::render::RenderArea;
use std::collections::HashMap;

/// What one image window shows: zoom, pan and its texture.
pub struct FrameView {
    /// Screen pixels per image pixel (1 = 1:1).
    pub zoom: f32,
    pub pan: egui::Vec2,
    texture: Option<egui::TextureHandle>,
    /// (image version, channels, size) the texture shows.
    shown: Option<(u64, Channels, [usize; 2])>,
    /// The image pixel under the pointer and its RGBA.
    pub hover: Option<([u32; 2], [u8; 4])>,
    /// Screen rectangle of the image last frame (tests aim at it).
    pub image_rect: egui::Rect,
}

impl Default for FrameView {
    fn default() -> Self {
        Self { zoom: 1.0, pan: egui::Vec2::ZERO, texture: None, shown: None, hover: None, image_rect: egui::Rect::NOTHING }
    }
}

impl FrameView {
    /// The texture the window draws the image with.
    pub fn texture_id(&self) -> Option<egui::TextureId> {
        self.texture.as_ref().map(|t| t.id())
    }
}

/// The main window's view and one per clone.
#[derive(Default)]
pub struct FrameViews {
    pub main: FrameView,
    pub clones: HashMap<u32, FrameView>,
    /// The RAM Player's view.
    pub ram: FrameView,
    /// What the RAM Player shows, and the `RamPlayer::version` it was made at.
    ram_image: Option<(u64, Option<image::RgbaImage>)>,
    /// A region handle being dragged in the viewport: which, and the rectangle at the start.
    region_drag: Option<(Handle, [f32; 4])>,
    /// The G-buffer channel image shown, and the image version and channel it was made for.
    gimage: Option<(u64, loom_commands::render_output::GChannel, image::RgbaImage)>,
    /// The ActiveShade window's view.
    pub active_shade: FrameView,
    /// The Panorama viewer's view and the image it was made at (viewer version).
    pub panorama: FrameView,
    panorama_image: Option<(u64, image::RgbaImage)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Handle {
    Move,
    Corner(u8),
}

fn color_image(img: &image::RgbaImage, ch: Channels, area: [u32; 4]) -> egui::ColorImage {
    let [x0, y0, w, h] = area;
    let mut px = Vec::with_capacity((w * h) as usize);
    for y in y0..y0 + h {
        for x in x0..x0 + w {
            let p = ch.display(img.get_pixel(x, y).0);
            px.push(egui::Color32::from_rgb(p[0], p[1], p[2]));
        }
    }
    egui::ColorImage { size: [w as usize, h as usize], pixels: px }
}

const TEXTURE: egui::TextureOptions = egui::TextureOptions { magnification: egui::TextureFilter::Nearest, minification: egui::TextureFilter::Linear, wrap_mode: egui::TextureWrapMode::ClampToEdge, mipmap_mode: None };

/// Brings `view`'s texture up to date with the image (only the changed
/// rectangle when the channels and size are the same).
fn update_texture(ctx: &egui::Context, view: &mut FrameView, name: &str, img: Option<&image::RgbaImage>, version: u64, ch: Channels, dirty: &mut Option<[u32; 4]>) {
    let Some(img) = img else {
        view.texture = None;
        view.shown = None;
        return;
    };
    let size = [img.width() as usize, img.height() as usize];
    if view.shown == Some((version, ch, size)) {
        return;
    }
    match (&mut view.texture, view.shown, *dirty) {
        (Some(tex), Some((_, c, s)), Some(r)) if c == ch && s == size => {
            if r[2] > 0 && r[3] > 0 {
                tex.set_partial([r[0] as usize, r[1] as usize], color_image(img, ch, r), TEXTURE);
            }
        }
        _ => {
            let full = color_image(img, ch, [0, 0, img.width(), img.height()]);
            match &mut view.texture {
                Some(tex) => tex.set(full, TEXTURE),
                None => view.texture = Some(ctx.load_texture(name, full, TEXTURE)),
            }
        }
    }
    *dirty = Some([0, 0, 0, 0]);
    view.shown = Some((version, ch, size));
}

/// The image area: draws the texture at the view's zoom and pan; the wheel
/// (or Ctrl+wheel) zooms about the pointer, the middle button or
/// Shift+drag pans, Ctrl+click zooms in and Ctrl+right-click out.
fn image_area(ui: &mut egui::Ui, view: &mut FrameView, img: Option<&image::RgbaImage>) {
    let avail = ui.available_size();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(avail.x.max(64.0), avail.y.max(64.0)), egui::Sense::click_and_drag());
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, 0.0, egui::Color32::from_gray(40));
    if view.zoom <= 0.0 {
        view.zoom = 1.0;
    }
    view.hover = None;
    let Some(img) = img else {
        view.image_rect = egui::Rect::NOTHING;
        return;
    };
    let size = egui::vec2(img.width() as f32, img.height() as f32);
    let top_left = |zoom: f32, pan: egui::Vec2| rect.min + (rect.size() - size * zoom) * 0.5 + pan;
    let (mods, pointer) = ui.input(|i| (i.modifiers, i.pointer.clone()));
    if let Some(p) = pointer.hover_pos().filter(|p| rect.contains(*p) && resp.hovered()) {
        let (scroll, pinch) = ui.input(|i| (i.raw_scroll_delta.y, i.zoom_delta()));
        let mut factor = pinch * 1.25f32.powf(scroll / 50.0);
        if mods.ctrl && resp.clicked() {
            factor *= 2.0;
        }
        if mods.ctrl && resp.secondary_clicked() {
            factor *= 0.5;
        }
        if (factor - 1.0).abs() > 1e-4 {
            let new_zoom = (view.zoom * factor).clamp(1.0 / 32.0, 32.0);
            // Keep the image point under the pointer where it is.
            let at = (p - top_left(view.zoom, view.pan)) / view.zoom;
            let want = p - at * new_zoom;
            view.pan = want - (rect.min + (rect.size() - size * new_zoom) * 0.5);
            view.zoom = new_zoom;
        }
    }
    if resp.dragged_by(egui::PointerButton::Middle) || (mods.shift && resp.dragged_by(egui::PointerButton::Primary)) {
        view.pan += resp.drag_delta();
    }
    let r = egui::Rect::from_min_size(top_left(view.zoom, view.pan), size * view.zoom);
    view.image_rect = r;
    if let Some(tex) = &view.texture {
        painter.image(tex.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
    }
    if let Some(p) = pointer.hover_pos().filter(|p| r.contains(*p) && rect.contains(*p)) {
        let q = (p - r.min) / view.zoom;
        let (x, y) = ((q.x as u32).min(img.width() - 1), (q.y as u32).min(img.height() - 1));
        view.hover = Some(([x, y], img.get_pixel(x, y).0));
    }
}

/// Swatch and RGBA of the pixel under the pointer.
/// The same space is taken with or without a pixel, so the window's layout
/// does not move as the pointer enters and leaves the image.
fn pixel_readout(ui: &mut egui::Ui, view: &FrameView) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(16.0, 14.0), egui::Sense::hover());
    let text = match view.hover {
        Some(([x, y], p)) => {
            ui.painter().rect_filled(r, 0.0, egui::Color32::from_rgb(p[0], p[1], p[2]));
            format!("({x}, {y})  R {} G {} B {} A {}", p[0], p[1], p[2], p[3])
        }
        None => String::new(),
    };
    ui.allocate_ui_with_layout(egui::vec2(280.0, 16.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_size(egui::vec2(280.0, 16.0));
        ui.add(egui::Label::new(text).truncate());
    });
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp) {
    if app.state.rendered.copy_requested {
        app.state.rendered.copy_requested = false;
        if let Some(img) = &app.state.rendered.image {
            let shown = app.state.rendered.channels.displayed(img);
            let ci = egui::ColorImage::from_rgba_unmultiplied([shown.width() as usize, shown.height() as usize], shown.as_raw());
            ctx.copy_image(ci);
            app.state.status_prompt = "Copied the rendered image".into();
        }
    }
    if app.state.rendered.open {
        let mut open = true;
        main_window(ctx, app, &mut open);
        if !open {
            app.state.rendered.open = false;
        }
    }
    clone_windows(ctx, app);
    if app.state.rendered.ram_player.open {
        let mut open = true;
        ram_player(ctx, app, &mut open);
        if !open {
            app.state.rendered.ram_player.open = false;
        }
    }
    if app.state.rendered.messages_open {
        let mut open = true;
        message_window(ctx, app, &mut open);
        if !open {
            app.state.rendered.messages_open = false;
        }
    }
    if app.state.rendered.active_shade.open {
        let mut open = true;
        active_shade_window(ctx, app, &mut open);
        if !open {
            let _ = app.dispatch(Command::ActiveShadeFloater);
        }
    }
    if app.state.panorama.open {
        let mut open = true;
        panorama_viewer(ctx, app, &mut open);
        if !open {
            app.state.panorama.open = false;
        }
    }
    if app.state.rendered.batch.open {
        let mut open = true;
        batch_render(ctx, app, &mut open);
        if !open {
            app.state.rendered.batch.open = false;
        }
    }
}

/// ActiveShade: the render viewport rendered again as the scene changes;
/// its toolbar (Initialize, Update, Auto Update, the region) and the
/// right-click menu of the same commands.
fn active_shade_window(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let window = egui::Window::new("ActiveShade").id(egui::Id::new("ActiveShade")).resizable(true).default_size(egui::vec2(520.0, 440.0));
    crate::os_window::OsWindow::new("ActiveShade", "ActiveShade", [520.0, 440.0]).show(ctx, open, window, |ui| {
        let act = |app: &mut LoomApp, action: &str| {
            let _ = app.dispatch(Command::ActiveShadeCommand { action: action.into(), on: None, region: None });
        };
        if app.state.rendered.active_shade.toolbar {
            ui.horizontal(|ui| {
                if ui.button("Initialize").clicked() {
                    act(app, "Initialize");
                }
                if ui.button("Update").clicked() {
                    act(app, "Update");
                }
                if ui.selectable_label(app.state.rendered.active_shade.auto_update, "Auto Update").clicked() {
                    act(app, "Auto Update");
                }
                if ui.add_enabled(app.state.rendered.active_shade.region.is_some(), egui::Button::new("Full View")).on_hover_text("Clear the Draw Region").clicked() {
                    act(app, "Draw Region");
                }
                let busy = app.render_job.as_ref().is_some_and(|j| j.req.active_shade);
                ui.label(if busy { "Rendering..." } else { "" });
            });
        }
        let a = &app.state.rendered.active_shade;
        let (img, version) = (a.image.clone(), a.version);
        let mut dirty = None;
        update_texture(ui.ctx(), &mut app.rfw.active_shade, "active_shade", img.as_deref(), version, Channels::default(), &mut dirty);
        image_area(ui, &mut app.rfw.active_shade, img.as_deref());
        let resp = ui.interact(app.rfw.active_shade.image_rect, egui::Id::new("active_shade_menu"), egui::Sense::click());
        resp.context_menu(|ui| {
            for a in ["Initialize", "Update", "Auto Update", "Toggle Toolbar"] {
                if ui.button(a).clicked() {
                    act(app, a);
                    ui.close_menu();
                }
            }
            if ui.button("Draw Region (centre quarter)").clicked() {
                let _ = app.dispatch(Command::ActiveShadeCommand { action: "Draw Region".into(), on: None, region: Some([0.25, 0.25, 0.75, 0.75]) });
                ui.close_menu();
            }
        });
    });
}

/// Panorama Exporter > Viewer: a perspective view into the panorama;
/// dragging turns the view, the wheel changes the field of view.
fn panorama_viewer(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let window = egui::Window::new("Panorama Viewer").id(egui::Id::new("Panorama Viewer")).resizable(true).default_size(egui::vec2(560.0, 420.0));
    crate::os_window::OsWindow::new("Panorama Viewer", "Panorama Viewer", [560.0, 420.0]).show(ctx, open, window, |ui| {
        let v = &app.state.panorama;
        ui.label(format!("Yaw {:.0}  Pitch {:.0}  FOV {:.0}", v.yaw, v.pitch, v.fov));
        let Some(pano) = v.image.clone() else {
            ui.label("No panorama");
            return;
        };
        let version = v.version;
        if app.rfw.panorama_image.as_ref().map(|p| p.0) != Some(version) {
            app.rfw.panorama_image = Some((version, loom_commands::render_output::panorama_view(&pano, v.yaw, v.pitch, v.fov, 480, 320)));
        }
        let img = app.rfw.panorama_image.as_ref().map(|p| p.1.clone());
        let mut dirty = None;
        update_texture(ui.ctx(), &mut app.rfw.panorama, "panorama_viewer", img.as_ref(), version, Channels::default(), &mut dirty);
        image_area(ui, &mut app.rfw.panorama, img.as_ref());
        let resp = ui.interact(app.rfw.panorama.image_rect, egui::Id::new("panorama_drag"), egui::Sense::drag());
        let d = resp.drag_delta();
        let scroll = if resp.hovered() { ui.input(|i| i.raw_scroll_delta.y) } else { 0.0 };
        if d != egui::Vec2::ZERO || scroll != 0.0 {
            let v = &app.state.panorama;
            let (yaw, pitch, fov) = (v.yaw + d.x * 0.3, (v.pitch + d.y * 0.3).clamp(-90.0, 90.0), (v.fov - scroll * 0.05).clamp(10.0, 120.0));
            let _ = app.dispatch(Command::PanoramaViewer { path: None, yaw: Some(yaw), pitch: Some(pitch), fov: Some(fov) });
        }
    });
}

/// Rendering > Batch Render: the task queue, the selected task's
/// parameters, Render, and the warning list.
fn batch_render(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let window = egui::Window::new("Batch Render").id(egui::Id::new("Batch Render")).resizable(true).default_size(egui::vec2(560.0, 460.0));
    crate::os_window::OsWindow::new("Batch Render", "Batch Render", [560.0, 460.0]).show(ctx, open, window, |ui| {
        let sel = app.state.rendered.batch.selected;
        let n = app.state.scene.render.batch.len();
        ui.horizontal(|ui| {
            if ui.button("Add...").clicked() {
                let _ = app.dispatch(Command::BatchRenderAdd { duplicate: None });
            }
            if ui.add_enabled(sel < n, egui::Button::new("Duplicate...")).clicked() {
                let _ = app.dispatch(Command::BatchRenderAdd { duplicate: Some(sel as u32) });
            }
            if ui.add_enabled(sel < n, egui::Button::new("Delete")).clicked() {
                let _ = app.dispatch(Command::BatchRenderDelete { index: sel as u32 });
            }
        });
        egui::Grid::new("batch_queue").striped(true).num_columns(5).show(ui, |ui| {
            for h in ["", "Name", "Camera", "Output Path", "Range"] {
                ui.strong(h);
            }
            ui.end_row();
            let jobs = app.state.scene.render.batch.clone();
            for (i, j) in jobs.iter().enumerate() {
                let mut on = j.enabled;
                if ui.checkbox(&mut on, "").changed() {
                    let _ = app.dispatch(Command::BatchRenderTask { index: i as u32, name: None, enabled: Some(on), camera: None, preset: None, override_preset: None, frames: None, width: None, height: None, pixel_aspect: None, output: None });
                }
                if ui.selectable_label(i == sel, &j.name).clicked() {
                    app.state.rendered.batch.selected = i;
                }
                let cam = j.camera.and_then(|c| app.state.scene.node(loom_scene::NodeId(c))).map_or("Viewport".to_owned(), |n| n.name.clone());
                ui.label(cam);
                ui.label(j.output.clone().unwrap_or_default());
                ui.label(if j.override_preset { format!("{} - {}", j.frames[0], j.frames[1]) } else { "Default".into() });
                ui.end_row();
            }
        });
        ui.separator();
        if let Some(j) = app.state.scene.render.batch.get(sel).cloned() {
            let task = |app: &mut LoomApp, f: &dyn Fn(&mut Command)| {
                let mut c = Command::BatchRenderTask { index: sel as u32, name: None, enabled: None, camera: None, preset: None, override_preset: None, frames: None, width: None, height: None, pixel_aspect: None, output: None };
                f(&mut c);
                let _ = app.dispatch(c);
            };
            ui.label("Selected Batch Render Parameters");
            let mut ov = j.override_preset;
            if ui.checkbox(&mut ov, "Override Preset").changed() {
                task(app, &|c| if let Command::BatchRenderTask { override_preset, .. } = c { *override_preset = Some(ov) });
            }
            ui.add_enabled_ui(j.override_preset, |ui| {
                ui.horizontal(|ui| {
                    let (mut a, mut b, mut w, mut h) = (j.frames[0], j.frames[1], j.width, j.height);
                    ui.label("Frame Start:");
                    let ra = ui.add(egui::DragValue::new(&mut a));
                    ui.label("End:");
                    let rb = ui.add(egui::DragValue::new(&mut b));
                    ui.label("Width:");
                    let rw = ui.add(egui::DragValue::new(&mut w).range(1..=8192));
                    ui.label("Height:");
                    let rh = ui.add(egui::DragValue::new(&mut h).range(1..=8192));
                    if ra.changed() || rb.changed() {
                        task(app, &|c| if let Command::BatchRenderTask { frames, .. } = c { *frames = Some([a.min(b), b.max(a)]) });
                    }
                    if rw.changed() || rh.changed() {
                        task(app, &|c| if let Command::BatchRenderTask { width, height, .. } = c { (*width, *height) = (Some(w), Some(h)) });
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.label("Camera:");
                let cams: Vec<(u64, String)> = app.state.scene.nodes().filter(|n| matches!(n.object, loom_scene::ObjectKind::Camera { .. })).map(|n| (n.id.0, n.name.clone())).collect();
                let cur = j.camera.and_then(|c| cams.iter().find(|x| x.0 == c)).map_or("Viewport".to_owned(), |x| x.1.clone());
                egui::ComboBox::from_id_salt("batch_camera").selected_text(cur).show_ui(ui, |ui| {
                    if ui.selectable_label(j.camera.is_none(), "Viewport").clicked() {
                        task(app, &|c| if let Command::BatchRenderTask { camera, .. } = c { *camera = Some(0) });
                    }
                    for (id, name) in &cams {
                        if ui.selectable_label(j.camera == Some(*id), name).clicked() {
                            let id = *id;
                            task(app, &|c| if let Command::BatchRenderTask { camera, .. } = c { *camera = Some(id) });
                        }
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.label("Output Path:");
                let mut out = j.output.clone().unwrap_or_default();
                if ui.text_edit_singleline(&mut out).lost_focus() && Some(&out) != j.output.as_ref() {
                    task(app, &|c| if let Command::BatchRenderTask { output, .. } = c { *output = Some(out.clone()) });
                }
            });
            ui.horizontal(|ui| {
                ui.label("Preset:");
                let mut p = j.preset.clone().unwrap_or_default();
                if ui.text_edit_singleline(&mut p).lost_focus() && Some(&p) != j.preset.as_ref() {
                    task(app, &|c| if let Command::BatchRenderTask { preset, .. } = c { *preset = Some(p.clone()) });
                }
            });
        }
        ui.separator();
        if let Some(w) = app.state.rendered.batch.warning.clone() {
            ui.colored_label(egui::Color32::YELLOW, "Batch Render Warning");
            for l in &w {
                ui.label(l);
            }
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    if let Err(e) = app.dispatch(Command::BatchRenderRun { confirm: Some(true), update_path: None }) {
                        app.last_error = Some(format!("Batch Render: {e}"));
                    }
                }
                if ui.button("Cancel").clicked() {
                    app.state.rendered.batch.warning = None;
                }
            });
        }
        let busy = app.state.rendered.progress.is_some() || app.state.pending_render.is_some();
        if ui.add_enabled(!busy && n > 0, egui::Button::new("Render")).clicked() {
            if let Err(e) = app.dispatch(Command::BatchRenderRun { confirm: None, update_path: None }) {
                app.last_error = Some(format!("Batch Render: {e}"));
            }
        }
    });
}

/// RAM Player: channels A and B (open a file, the last render, save, clear),
/// compared side by side across the split line, with the alpha display.
fn ram_player(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let window = egui::Window::new("RAM Player").resizable(true).default_size(egui::vec2(640.0, 480.0));
    crate::os_window::OsWindow::new("RAM Player", "RAM Player", [640.0, 480.0]).show(ctx, open, window, |ui| {
        for (c, label) in ["A", "B"].into_iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("Channel {label}:"));
                if ui.button(format!("Open Channel {label}...")).clicked() {
                    app.state.dialog = Some(loom_commands::Dialog::RamPlayerFile { channel: c as u8, save: false });
                }
                if ui.button(format!("Open Last Rendered Image in Channel {label}")).clicked() {
                    if let Err(e) = app.dispatch(Command::RamPlayerOpen { channel: c as u32, path: None }) {
                        app.state.status_prompt = e;
                    }
                }
                let has = app.state.rendered.ram_player.channels[c].is_some();
                if ui.add_enabled(has, egui::Button::new(format!("Save Channel {label}..."))).clicked() {
                    app.state.dialog = Some(loom_commands::Dialog::RamPlayerFile { channel: c as u8, save: true });
                }
                if ui.add_enabled(has, egui::Button::new(format!("Close Channel {label}"))).clicked() {
                    let _ = app.dispatch(Command::RamPlayerView { split: None, compare: None, alpha: None, clear: Some(c as u32) });
                }
                if let Some((name, img)) = &app.state.rendered.ram_player.channels[c] {
                    ui.label(format!("{name} ({}x{})", img.width(), img.height()));
                }
            });
        }
        let r = app.state.rendered.ram_player.clone();
        egui::CollapsingHeader::new("Configuration").id_salt("ram_player_config").show(ui, |ui| {
            let c = &r.config;
            let cfg = |app: &mut LoomApp, width, height, lock_aspect, filter, alpha_to_other, original| {
                let _ = app.dispatch(Command::RamPlayerConfiguration { width, height, lock_aspect, filter, alpha_to_other, original });
            };
            ui.horizontal(|ui| {
                let [mut w, mut h] = c.size.unwrap_or([app.state.scene.render.width, app.state.scene.render.height]);
                ui.label("Width:");
                if ui.add(egui::DragValue::new(&mut w).range(1..=loom_commands::render::MAX_RENDER_SIZE)).changed() {
                    cfg(app, Some(w), None, None, None, None, None);
                }
                ui.label("Height:");
                if ui.add_enabled(!c.lock_aspect, egui::DragValue::new(&mut h).range(1..=loom_commands::render::MAX_RENDER_SIZE)).changed() {
                    cfg(app, None, Some(h), None, None, None, None);
                }
                if ui.add_enabled(c.size.is_some(), egui::Button::new("Original Size")).clicked() {
                    cfg(app, None, None, None, None, None, Some(true));
                }
            });
            ui.horizontal(|ui| {
                let (mut lock, mut filter, mut other) = (c.lock_aspect, c.filter, c.alpha_to_other);
                if ui.checkbox(&mut lock, "Lock Aspect Ratio").changed() {
                    cfg(app, None, None, Some(lock), None, None, None);
                }
                if ui.checkbox(&mut filter, "Filter Input").changed() {
                    cfg(app, None, None, None, Some(filter), None, None);
                }
                if ui.checkbox(&mut other, "Load Into Other Channels").on_hover_text("Alpha: the alpha loads, as grey, into the other channel").changed() {
                    cfg(app, None, None, None, None, Some(other), None);
                }
            });
        });
        ui.horizontal(|ui| {
            let mut compare = r.compare;
            if ui.checkbox(&mut compare, "A | B").on_hover_text("Both channels, split side by side").changed() {
                let _ = app.dispatch(Command::RamPlayerView { split: None, compare: Some(compare), alpha: None, clear: None });
            }
            let mut alpha = r.alpha;
            if ui.checkbox(&mut alpha, "Display Alpha Channel").changed() {
                let _ = app.dispatch(Command::RamPlayerView { split: None, compare: None, alpha: Some(alpha), clear: None });
            }
            ui.label("Split:");
            let mut split = r.split;
            if ui.add(egui::Slider::new(&mut split, 0.0..=1.0).show_value(false)).changed() {
                let _ = app.dispatch(Command::RamPlayerView { split: Some(split), compare: None, alpha: None, clear: None });
            }
            pixel_readout(ui, &app.rfw.ram);
        });
        if app.rfw.ram_image.as_ref().map(|c| c.0) != Some(r.version) {
            app.rfw.ram_image = Some((r.version, r.composed()));
        }
        let FrameViews { ram: view, ram_image, .. } = &mut app.rfw;
        let img = ram_image.as_ref().and_then(|c| c.1.as_ref());
        let mut dirty = None;
        update_texture(ui.ctx(), view, "ram_player", img, r.version, Channels::default(), &mut dirty);
        image_area(ui, view, img);
        if r.compare && r.channels.iter().all(|c| c.is_some()) {
            // The split line.
            let rect = view.image_rect;
            let x = rect.left() + rect.width() * r.split;
            ui.painter().vline(x, rect.y_range(), egui::Stroke::new(1.0_f32, egui::Color32::WHITE));
        }
    });
}

/// Render Message Window: the renders' information, warnings and errors,
/// with Clear and Copy.
fn message_window(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    use loom_commands::render::MessageLevel;
    let window = egui::Window::new("Render Message Window").resizable(true).default_size(egui::vec2(520.0, 260.0));
    crate::os_window::OsWindow::new("Render Message Window", "Render Message Window", [520.0, 260.0]).show(ctx, open, window, |ui| {
        ui.horizontal(|ui| {
            if ui.button("Clear").clicked() {
                let _ = app.dispatch(Command::ClearRenderMessages);
            }
            if ui.button("Copy").clicked() {
                let text: String = app.state.rendered.messages.iter().map(|m| format!("{}: {}\n", m.level.label(), m.text)).collect();
                ui.ctx().copy_text(text);
            }
        });
        // Write Messages to File, the log file and Append to File.
        ui.horizontal(|ui| {
            let p = &app.state.prefs;
            let (mut write, mut append, path) = (p.render_log, p.render_log_append, p.render_log_path.clone().unwrap_or_default());
            if ui.add_enabled(!path.is_empty(), egui::Checkbox::new(&mut write, "Write Messages to File")).changed() {
                let _ = app.dispatch(Command::RenderMessageLog { write: Some(write), path: None, append: None });
            }
            if ui.checkbox(&mut append, "Append to File").changed() {
                let _ = app.dispatch(Command::RenderMessageLog { write: None, path: None, append: Some(append) });
            }
            let mut text = path.clone();
            if ui.add(egui::TextEdit::singleline(&mut text).hint_text("log file (.log)").desired_width(200.0)).lost_focus() && text != path {
                let _ = app.dispatch(Command::RenderMessageLog { write: None, path: Some(text), append: None });
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false; 2]).show(ui, |ui| {
            for m in &app.state.rendered.messages {
                let color = match m.level {
                    MessageLevel::Info => ui.visuals().text_color(),
                    MessageLevel::Warning => egui::Color32::from_rgb(200, 150, 0),
                    MessageLevel::Error => egui::Color32::from_rgb(220, 60, 60),
                };
                ui.label(egui::RichText::new(format!("{}: {}", m.level.label(), m.text)).color(color));
            }
        });
    });
}

fn channel_toggle(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, on: bool, tip: &str, set: fn(bool) -> Command) {
    if ui.selectable_label(on, label).on_hover_text(tip).clicked() {
        let _ = app.dispatch(set(!on));
    }
}

fn main_window(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let title = if app.state.rendered.title.is_empty() { "Rendered Frame Window".to_owned() } else { app.state.rendered.title.clone() };
    // First shown at the image's size (1:1), within the screen.
    let screen = ctx.screen_rect().size();
    let (iw, ih) = app.state.rendered.image.as_ref().map_or((app.state.scene.render.width as f32, app.state.scene.render.height as f32), |i| (i.width() as f32, i.height() as f32));
    let size = egui::vec2((iw + 24.0).clamp(760.0, (screen.x - 40.0).max(760.0)), (ih + 170.0).clamp(320.0, (screen.y - 40.0).max(320.0)));
    let window = egui::Window::new(title.clone()).id(egui::Id::new("Rendered Frame Window")).resizable(true).default_size(size);
    crate::os_window::OsWindow::new("Rendered Frame Window", title, size).show(ctx, open, window, |ui| {
        if !app.state.rendered.ui_hidden {
            rendering_controls(ui, app);
        }
        // Toolbar.
        let has_image = app.state.rendered.image.is_some();
        ui.horizontal(|ui| {
            if ui.selectable_label(app.state.rendered.ui_hidden, "Toggle UI").on_hover_text("Hide or show the rendering controls").clicked() {
                let _ = app.dispatch(Command::RenderedFrameWindowToggleUi);
            }
            if ui.add_enabled(has_image, egui::Button::new("Save Image")).clicked() {
                let _ = app.dispatch(Command::SaveRenderedImage { path: None });
            }
            if ui.add_enabled(has_image, egui::Button::new("Copy Image")).clicked() {
                let _ = app.dispatch(Command::CopyRenderedImage);
            }
            if ui.add_enabled(has_image, egui::Button::new("Clone")).on_hover_text("Clone Rendered Frame Window").clicked() {
                let _ = app.dispatch(Command::CloneRenderedFrameWindow);
            }
            if ui.add_enabled(has_image, egui::Button::new("Clear")).clicked() {
                let _ = app.dispatch(Command::ClearRenderedFrameWindow);
            }
            ui.separator();
            let c = app.state.rendered.channels;
            channel_toggle(ui, app, "R", c.red, "Enable Red Channel", |v| Command::RenderedFrameWindowChannels { red: Some(v), green: None, blue: None, alpha: None, mono: None });
            channel_toggle(ui, app, "G", c.green, "Enable Green Channel", |v| Command::RenderedFrameWindowChannels { red: None, green: Some(v), blue: None, alpha: None, mono: None });
            channel_toggle(ui, app, "B", c.blue, "Enable Blue Channel", |v| Command::RenderedFrameWindowChannels { red: None, green: None, blue: Some(v), alpha: None, mono: None });
            channel_toggle(ui, app, "Alpha", c.alpha, "Display Alpha Channel", |v| Command::RenderedFrameWindowChannels { red: None, green: None, blue: None, alpha: Some(v), mono: None });
            channel_toggle(ui, app, "Mono", c.mono, "Monochrome", |v| Command::RenderedFrameWindowChannels { red: None, green: None, blue: None, alpha: None, mono: Some(v) });
            // The channel list: the colour image or a G-buffer channel.
            let cur = app.state.rendered.gchannel;
            let has_g = app.state.rendered.gbuffer.is_some();
            egui::ComboBox::from_id_salt("rfw_gchannel").selected_text(cur.map_or("RGB Alpha", |c| c.label())).show_ui(ui, |ui| {
                if ui.selectable_label(cur.is_none(), "RGB Alpha").clicked() {
                    let _ = app.dispatch(Command::RenderedFrameWindowGBuffer { channel: None });
                }
                for ch in loom_commands::render_output::GChannel::ALL {
                    if ui.add_enabled(has_g, egui::SelectableLabel::new(cur == Some(ch), ch.label())).clicked() {
                        let _ = app.dispatch(Command::RenderedFrameWindowGBuffer { channel: Some(ch) });
                    }
                }
            });
            // View Image File's frame arrows.
            if let Some(v) = app.state.rendered.viewer.as_ref().filter(|v| v.files.len() > 1) {
                let (i, n) = (v.index, v.files.len());
                ui.separator();
                if ui.add_enabled(i > 0, egui::Button::new("<")).on_hover_text("Previous frame").clicked() {
                    let _ = app.dispatch(Command::ViewImageFileStep { step: Some(-1), index: None });
                }
                ui.label(format!("{} / {n}", i + 1));
                if ui.add_enabled(i + 1 < n, egui::Button::new(">")).on_hover_text("Next frame").clicked() {
                    let _ = app.dispatch(Command::ViewImageFileStep { step: Some(1), index: None });
                }
            }
            ui.separator();
            pixel_readout(ui, &app.rfw.main);
        });
        // Progress with Pause / Cancel while rendering.
        if let Some(p) = app.state.rendered.progress {
            ui.horizontal(|ui| {
                let secs = app.render_job.as_ref().map(|j| j.elapsed().as_secs_f32()).unwrap_or(0.0);
                let label = format!("Rendering pass {} of {}  {secs:.1} s", (p.done + 1).min(p.total), p.total);
                ui.add(egui::ProgressBar::new(p.done as f32 / p.total.max(1) as f32).text(label).desired_width(320.0));
                if ui.button(if p.paused { "Resume" } else { "Pause" }).clicked() {
                    let _ = app.dispatch(Command::PauseRender);
                }
                if ui.button("Cancel").clicked() {
                    let _ = app.dispatch(Command::CancelRender);
                }
            });
        }
        let r = &mut app.state.rendered;
        let (version, channels) = (r.version, r.channels);
        let shown = match (r.gchannel, r.gbuffer.as_ref()) {
            (Some(ch), Some(g)) => {
                if app.rfw.gimage.as_ref().is_none_or(|(v, c, _)| *v != version || *c != ch) {
                    app.rfw.gimage = Some((version, ch, g.image(ch, r.image.as_deref())));
                }
                app.rfw.gimage.as_ref().map(|(_, _, i)| std::sync::Arc::new(i.clone()))
            }
            _ => r.image.clone(),
        };
        update_texture(ui.ctx(), &mut app.rfw.main, "rendered_frame", shown.as_deref(), version, channels, &mut r.dirty);
        image_area(ui, &mut app.rfw.main, shown.as_deref());
    });
}

/// The Rendered Frame Window's rendering controls (Toggle UI hides them).
fn rendering_controls(ui: &mut egui::Ui, app: &mut LoomApp) {
    {
        // Rendering controls.
        ui.horizontal(|ui| {
            ui.label("Area to Render:");
            let area = app.state.scene.render.area;
            egui::ComboBox::from_id_salt("rfw_area").selected_text(area.label()).show_ui(ui, |ui| {
                for a in RenderArea::ALL {
                    if ui.selectable_label(a == area, a.label()).clicked() {
                        let _ = app.dispatch(Command::AreaToRender { area: Some(a), region: None });
                    }
                }
            });
            if ui.selectable_label(app.state.rendered.edit_region, "Edit Region").on_hover_text("Edit the render region in the viewport").clicked() {
                let _ = app.dispatch(Command::EditRegion);
            }
        });
        viewport_controls(ui, app, "rfw_viewport", "Lock to Viewport");
        render_buttons(ui, app, "rfw_mode", true);
        ui.separator();
    }
}

/// The Viewport list and its lock (Lock to Viewport / Lock View).
pub fn viewport_controls(ui: &mut egui::Ui, app: &mut LoomApp, salt: &str, lock_label: &str) {
    ui.horizontal(|ui| {
        ui.label("Viewport:");
        let cur = loom_commands::render::render_viewport(&app.state);
        egui::ComboBox::from_id_salt(salt).selected_text(loom_commands::render::viewport_name(&app.state, cur)).show_ui(ui, |ui| {
            for i in visible_viewports(app) {
                if ui.selectable_label(i == cur, loom_commands::render::viewport_name(&app.state, i)).clicked() {
                    let _ = app.dispatch(Command::RenderViewport { viewport: Some(i as u32), lock: None, iterative: None });
                }
            }
        });
        let mut lock = app.state.rendered.lock_to_viewport;
        if ui.checkbox(&mut lock, lock_label).changed() {
            let _ = app.dispatch(Command::RenderViewport { viewport: None, lock: Some(lock), iterative: None });
        }
    });
}

/// Production / Iterative and the Render button (Rendered Frame Window and
/// Render Setup; the window also opens Render Setup).
pub fn render_buttons(ui: &mut egui::Ui, app: &mut LoomApp, salt: &str, setup_button: bool) {
    ui.horizontal(|ui| {
        if setup_button && ui.button("Render Setup...").clicked() {
            let _ = app.dispatch(Command::RenderSetup { width: None, height: None, image_aspect: None, pixel_aspect: None, lock_image_aspect: None, lock_pixel_aspect: None, render_hidden: None, force_two_sided: None, atmospherics: None, effects: None, save_file: None, output: None });
        }
        if setup_button && ui.button("Environment and Effects").on_hover_text("Environment and Effects dialog (8)").clicked() {
            let _ = app.dispatch(Command::EnvironmentDialogToggle);
        }
        let iterative = app.state.rendered.iterative;
        egui::ComboBox::from_id_salt(salt).selected_text(if iterative { "Iterative" } else { "Production" }).show_ui(ui, |ui| {
            for (label, it) in [("Production", false), ("Iterative", true)] {
                if ui.selectable_label(it == iterative, label).clicked() {
                    let _ = app.dispatch(Command::RenderViewport { viewport: None, lock: None, iterative: Some(it) });
                }
            }
        });
        let busy = app.state.rendered.progress.is_some() || app.state.pending_render.is_some();
        if ui.add_enabled(!busy, egui::Button::new("Render")).clicked() {
            let _ = if iterative { app.dispatch(Command::RenderIterative) } else { app.dispatch(Command::Render { path: None, width: None, height: None }) };
        }
    });
}

/// The viewports on screen (the Viewport list holds only those).
fn visible_viewports(app: &LoomApp) -> Vec<usize> {
    let n = app.state.viewports.len();
    if app.state.maximized {
        return vec![app.state.active_viewport];
    }
    let cells = app.state.layout.cells(&app.state.splits).len();
    (0..n.min(cells)).collect()
}

fn clone_windows(ctx: &egui::Context, app: &mut LoomApp) {
    let clones: Vec<(u32, String)> = app.state.rendered.clones.iter().map(|c| (c.id, c.title.clone())).collect();
    let mut closed = Vec::new();
    for (id, title) in clones {
        let mut open = true;
        let window = egui::Window::new(title.clone()).id(egui::Id::new(("Rendered Frame Window clone", id))).resizable(true).default_size(egui::vec2(520.0, 420.0));
        crate::os_window::OsWindow::new(format!("Rendered Frame Window clone {id}"), title, [520.0, 420.0]).show(ctx, &mut open, window, |ui| {
            let Some(c) = app.state.rendered.clones.iter().find(|c| c.id == id) else { return };
            let (img, channels) = (c.image.clone(), c.channels);
            let zoom = app.rfw.main.zoom;
            let view = app.rfw.clones.entry(id).or_insert_with(|| FrameView { zoom, ..Default::default() });
            ui.horizontal(|ui| pixel_readout(ui, view));
            let mut dirty = None;
            // A clone's image is replaced whole (Render Elements): its pointer is its version.
            update_texture(ui.ctx(), view, &format!("rendered_frame_clone_{id}"), Some(&img), std::sync::Arc::as_ptr(&img) as u64, channels, &mut dirty);
            image_area(ui, view, Some(&img));
        });
        if !open {
            closed.push(id);
        }
    }
    for id in closed {
        app.state.rendered.clones.retain(|c| c.id != id);
        app.rfw.clones.remove(&id);
    }
}

/// Where the rendered frame lies in viewport `index` drawn in `rect`: a
/// perspective or orthographic view keeps its height, a camera its width.
pub fn output_frame(app: &LoomApp, index: usize, rect: egui::Rect) -> egui::Rect {
    let aspect = app.state.scene.render.image_aspect().max(1e-3);
    let size = if app.camera_for(index).fov_x_radians.is_some() { egui::vec2(rect.width(), rect.width() / aspect) } else { egui::vec2(rect.height() * aspect, rect.height()) };
    egui::Rect::from_center_size(rect.center(), size)
}

/// Edit Region: the region (or Blowup) rectangle over the render viewport,
/// moved by dragging inside it and sized by its corner handles. Drawn
/// after the viewports so it takes the pointer before them.
pub fn region_overlay(ui: &mut egui::Ui, app: &mut LoomApp, rects: &[egui::Rect]) {
    let s = &app.state.scene.render;
    if !app.state.rendered.edit_region || !s.area.uses_region() {
        app.rfw.region_drag = None;
        return;
    }
    let index = loom_commands::render::render_viewport(&app.state);
    let Some(rect) = rects.get(index).copied().filter(|r| r.width() >= 1.0) else { return };
    let frame = output_frame(app, index, rect);
    let r = s.area_rect();
    let to_screen = |r: [f32; 4]| egui::Rect::from_min_max(frame.min + egui::vec2(r[0], r[1]) * frame.size(), frame.min + egui::vec2(r[2], r[3]) * frame.size());
    let area = to_screen(r);
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_stroke(frame, 0.0, egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(255, 230, 0)), egui::StrokeKind::Middle);
    let colour = if s.area == RenderArea::Blowup { egui::Color32::from_rgb(90, 200, 255) } else { egui::Color32::from_rgb(255, 255, 255) };
    painter.rect_stroke(area, 0.0, egui::Stroke::new(1.5_f32, colour), egui::StrokeKind::Middle);
    let corners = [area.left_top(), area.right_top(), area.right_bottom(), area.left_bottom()];
    let mut targets: Vec<(Handle, egui::Rect)> = corners.iter().enumerate().map(|(i, c)| (Handle::Corner(i as u8), egui::Rect::from_center_size(*c, egui::vec2(10.0, 10.0)))).collect();
    targets.push((Handle::Move, area.shrink(6.0)));
    for (h, hr) in &targets {
        if let Handle::Corner(_) = h {
            painter.rect_filled(hr.shrink(1.0), 0.0, colour);
        }
    }
    for (h, hr) in targets {
        let resp = ui.interact(hr, ui.id().with(("render_region", format!("{h:?}"))), egui::Sense::drag());
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(if h == Handle::Move { egui::CursorIcon::Move } else { egui::CursorIcon::ResizeNwSe });
        }
        if resp.drag_started() {
            app.state.undo.begin("Edit Region");
            app.rfw.region_drag = Some((h, r));
        }
        if resp.dragged() {
            if let (Some((dh, start)), Some(p)) = (app.rfw.region_drag, resp.interact_pointer_pos()) {
                let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or(p);
                let d = (p - origin) / frame.size();
                let mut n = start;
                match dh {
                    Handle::Move => {
                        let (w, h) = (start[2] - start[0], start[3] - start[1]);
                        let x0 = (start[0] + d.x).clamp(0.0, 1.0 - w);
                        let y0 = (start[1] + d.y).clamp(0.0, 1.0 - h);
                        n = [x0, y0, x0 + w, y0 + h];
                    }
                    Handle::Corner(i) => {
                        let (xi, yi) = match i {
                            0 => (0, 1),
                            1 => (2, 1),
                            2 => (2, 3),
                            _ => (0, 3),
                        };
                        n[xi] = (start[xi] + d.x).clamp(0.0, 1.0);
                        n[yi] = (start[yi] + d.y).clamp(0.0, 1.0);
                        let (x0, x1) = (n[0].min(n[2]), n[0].max(n[2]));
                        let (y0, y1) = (n[1].min(n[3]), n[1].max(n[3]));
                        n = [x0, y0, x1.max(x0 + 0.01).min(1.0), y1.max(y0 + 0.01).min(1.0)];
                    }
                }
                let _ = app.dispatch(Command::AreaToRender { area: None, region: Some(n) });
            }
        }
        if resp.drag_stopped() {
            app.rfw.region_drag = None;
            app.state.undo.end();
        }
    }
}
