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
#[derive(Default)]
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
    /// A region handle being dragged in the viewport: which, and the rectangle at the start.
    region_drag: Option<(Handle, [f32; 4])>,
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
fn pixel_readout(ui: &mut egui::Ui, view: &FrameView) {
    if let Some(([x, y], p)) = view.hover {
        let (r, _) = ui.allocate_exact_size(egui::vec2(16.0, 14.0), egui::Sense::hover());
        ui.painter().rect_filled(r, 0.0, egui::Color32::from_rgb(p[0], p[1], p[2]));
        ui.label(format!("({x}, {y})  R {} G {} B {} A {}", p[0], p[1], p[2], p[3]));
    }
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
}

fn channel_toggle(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, on: bool, tip: &str, set: fn(bool) -> Command) {
    if ui.selectable_label(on, label).on_hover_text(tip).clicked() {
        let _ = app.dispatch(set(!on));
    }
}

fn main_window(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let title = if app.state.rendered.title.is_empty() { "Rendered Frame Window".to_owned() } else { app.state.rendered.title.clone() };
    egui::Window::new(title).id(egui::Id::new("Rendered Frame Window")).open(open).resizable(true).default_size(egui::vec2(700.0, 560.0)).show(ctx, |ui| {
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
        // Toolbar.
        let has_image = app.state.rendered.image.is_some();
        ui.horizontal(|ui| {
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
            ui.separator();
            pixel_readout(ui, &app.rfw.main);
        });
        // Progress with Pause / Cancel while rendering.
        if let Some(p) = app.state.rendered.progress {
            ui.horizontal(|ui| {
                let secs = app.render_job.as_ref().map(|j| j.elapsed().as_secs_f32()).unwrap_or(0.0);
                let label = format!("Rendering bucket {} of {}  {secs:.1} s", (p.done + 1).min(p.total), p.total);
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
        update_texture(ui.ctx(), &mut app.rfw.main, "rendered_frame", r.image.as_deref(), version, channels, &mut r.dirty);
        let img = app.state.rendered.image.clone();
        image_area(ui, &mut app.rfw.main, img.as_deref());
    });
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
            let _ = app.dispatch(Command::RenderSetup { width: None, height: None, image_aspect: None, pixel_aspect: None, lock_image_aspect: None, lock_pixel_aspect: None, render_hidden: None, force_two_sided: None, save_file: None, output: None });
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
        egui::Window::new(title).id(egui::Id::new(("Rendered Frame Window clone", id))).open(&mut open).resizable(true).default_size(egui::vec2(520.0, 420.0)).show(ctx, |ui| {
            let Some(c) = app.state.rendered.clones.iter().find(|c| c.id == id) else { return };
            let (img, channels) = (c.image.clone(), c.channels);
            let zoom = app.rfw.main.zoom;
            let view = app.rfw.clones.entry(id).or_insert_with(|| FrameView { zoom, ..Default::default() });
            ui.horizontal(|ui| pixel_readout(ui, view));
            let mut dirty = None;
            update_texture(ui.ctx(), view, &format!("rendered_frame_clone_{id}"), Some(&img), 0, channels, &mut dirty);
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
