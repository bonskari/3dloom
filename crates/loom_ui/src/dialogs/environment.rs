//! Rendering > Environment and Effects (8): the Environment panel (Common
//! Parameters, Exposure Control, Atmosphere) and the Effects panel, each
//! with the selected entry's parameter rollout. Every change is a command
//! (one undo step; a spinner drag is one).

use crate::LoomApp;
use eframe::egui;
use glam::Vec3;
use loom_commands::environment::EnvironmentTab;
use loom_commands::{Command, Dialog};
use loom_scene::environment::{AtmosphereKind, BlurType, EffectKind, EnvMapping, ExposureKind, LayerFalloff};
use serde_json::{json, Value};

/// The window's title (also its egui id).
pub const TITLE: &str = "Environment and Effects";

fn run(app: &mut LoomApp, cmd: &str, args: Value) {
    let r = app.dispatch_json(&json!({ "cmd": cmd, "args": args }));
    if r["ok"] != true {
        app.state.status_prompt = r["error"].as_str().unwrap_or("failed").to_owned();
    }
}

/// A spinner drag is one undo step.
fn merge_drag(app: &mut LoomApp, r: &egui::Response) {
    if r.drag_started() {
        app.state.undo.begin("Environment");
    }
    if r.drag_stopped() {
        app.state.undo.end();
    }
}

/// A labelled spinner; the new value when it changed.
fn spin(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, value: f32, range: std::ops::RangeInclusive<f32>, speed: f32) -> Option<f32> {
    ui.label(label);
    let mut v = value;
    let r = ui.add(egui::DragValue::new(&mut v).range(range).speed(speed).max_decimals(3));
    merge_drag(app, &r);
    ui.end_row();
    r.changed().then_some(v)
}

/// A labelled colour swatch (sRGB 0–1); the new colour when it changed.
fn swatch(ui: &mut egui::Ui, label: &str, c: Vec3) -> Option<[f32; 3]> {
    ui.label(label);
    let mut rgb = c.to_array().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    let changed = ui.color_edit_button_srgb(&mut rgb).changed();
    ui.end_row();
    changed.then(|| rgb.map(|v| v as f32 / 255.0))
}

fn file_name(path: &str) -> String {
    std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_owned())
}

fn sel_id(what: &str) -> egui::Id {
    egui::Id::new("environment_dialog").with(what)
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp) {
    let Some(tab) = app.state.environment_ui.open else { return };
    let mut open = true;
    let mut tab_now = tab;
    egui::Window::new(TITLE).open(&mut open).resizable(true).default_width(380.0).default_height(560.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut tab_now, EnvironmentTab::Environment, "Environment");
            ui.selectable_value(&mut tab_now, EnvironmentTab::Effects, "Effects");
        });
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match tab {
            EnvironmentTab::Environment => environment_panel(ui, app),
            EnvironmentTab::Effects => effects_panel(ui, app),
        });
    });
    add_dialog(ctx, app, "atmosphere", "Add Atmospheric Effect", &AtmosphereKind::NAMES, "Add Atmosphere");
    add_dialog(ctx, app, "effect", "Add Effect", &EffectKind::NAMES, "Add Effect");
    if !open {
        app.state.environment_ui.open = None;
    } else if tab_now != tab {
        app.state.environment_ui.open = Some(tab_now);
    }
}

/// Add Atmospheric Effect / Add Effect: pick a type, OK appends it.
fn add_dialog(ctx: &egui::Context, app: &mut LoomApp, what: &str, title: &str, names: &[&str], cmd: &str) {
    let id = sel_id(what).with("add");
    let Some(mut pick) = ctx.data(|d| d.get_temp::<usize>(id)) else { return };
    let mut open = true;
    let mut done = None;
    egui::Window::new(title).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        for (i, n) in names.iter().enumerate() {
            let r = ui.selectable_label(pick == i, *n);
            if r.clicked() {
                pick = i;
            }
            if r.double_clicked() {
                done = Some(true);
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                done = Some(true);
            }
            if ui.button("Cancel").clicked() {
                done = Some(false);
            }
        });
    });
    match (done, open) {
        (Some(true), _) => {
            let before = match what {
                "atmosphere" => app.state.scene.environment.atmospheres.len(),
                _ => app.state.scene.environment.effects.len(),
            };
            run(app, cmd, json!({ "kind": names[pick] }));
            ctx.data_mut(|d| {
                d.remove::<usize>(id);
                d.insert_temp(sel_id(what), before);
            });
        }
        (Some(false), _) | (None, false) => ctx.data_mut(|d| d.remove::<usize>(id)),
        _ => ctx.data_mut(|d| d.insert_temp(id, pick)),
    }
}

fn environment_panel(ui: &mut egui::Ui, app: &mut LoomApp) {
    let e = app.state.scene.environment.clone();
    egui::CollapsingHeader::new("Common Parameters").default_open(true).show(ui, |ui| {
        ui.label(egui::RichText::new("Background").strong());
        egui::Grid::new("env_background").num_columns(2).show(ui, |ui| {
            if let Some(c) = swatch(ui, "Color:", e.background.get()) {
                run(app, "Environment Settings", json!({ "background": c }));
            }
            ui.label("Environment Map:");
            let label = e.background_map.as_deref().map(file_name).unwrap_or_else(|| "None".into());
            ui.horizontal(|ui| {
                if ui.button(label).on_hover_text("Choose a bitmap for the background").clicked() {
                    app.state.dialog = Some(Dialog::EnvironmentMap);
                }
                if e.background_map.is_some() && ui.small_button("x").on_hover_text("Remove the map").clicked() {
                    run(app, "Environment Settings", json!({ "map": "" }));
                }
            });
            ui.end_row();
            ui.label("");
            let mut use_map = e.use_map;
            if ui.add_enabled(e.background_map.is_some(), egui::Checkbox::new(&mut use_map, "Use Map")).changed() {
                run(app, "Environment Settings", json!({ "use_map": use_map }));
            }
            ui.end_row();
            ui.label("Mapping:");
            egui::ComboBox::from_id_salt("env_mapping").selected_text(e.map_mapping.label()).show_ui(ui, |ui| {
                for m in EnvMapping::ALL {
                    if ui.selectable_label(m == e.map_mapping, m.label()).clicked() {
                        run(app, "Environment Settings", json!({ "mapping": m }));
                    }
                }
            });
            ui.end_row();
        });
        ui.label(egui::RichText::new("Global Lighting").strong());
        egui::Grid::new("env_lighting").num_columns(2).show(ui, |ui| {
            if let Some(c) = swatch(ui, "Tint:", e.tint.get()) {
                run(app, "Environment Settings", json!({ "tint": c }));
            }
            if let Some(v) = spin(ui, app, "Level:", e.level.get(), 0.0..=100.0, 0.01) {
                run(app, "Environment Settings", json!({ "level": v }));
            }
            if let Some(c) = swatch(ui, "Ambient:", e.ambient.get()) {
                run(app, "Environment Settings", json!({ "ambient": c }));
            }
        });
    });
    exposure_rollout(ui, app, &e.exposure);
    egui::CollapsingHeader::new("Atmosphere").default_open(true).show(ui, |ui| {
        let names: Vec<(String, bool)> = e.atmospheres.iter().map(|a| (a.name.clone(), a.active)).collect();
        if let Some(i) = list(ui, app, "atmosphere", &names, "Atmosphere Entry", "Delete Atmosphere") {
            if let Some(a) = e.atmospheres.get(i) {
                atmosphere_rollout(ui, app, i, &a.kind);
            }
        }
    });
}

/// An Atmosphere / Effects list with Add, Delete, Active, Move Up / Down
/// and Name; returns the selected entry.
fn list(ui: &mut egui::Ui, app: &mut LoomApp, what: &str, names: &[(String, bool)], entry_cmd: &str, delete_cmd: &str) -> Option<usize> {
    let id = sel_id(what);
    let mut sel: Option<usize> = ui.ctx().data(|d| d.get_temp(id)).filter(|i: &usize| *i < names.len());
    ui.horizontal(|ui| {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_size(egui::vec2(180.0, 90.0));
            ui.vertical(|ui| {
                for (i, (n, active)) in names.iter().enumerate() {
                    let text = if *active { n.clone() } else { format!("{n} (off)") };
                    if ui.selectable_label(sel == Some(i), text).clicked() {
                        sel = Some(i);
                    }
                }
            });
        });
        ui.vertical(|ui| {
            if ui.button("Add...").clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(id.with("add"), 0usize));
            }
            if ui.add_enabled(sel.is_some(), egui::Button::new("Delete")).clicked() {
                if let Some(i) = sel {
                    run(app, delete_cmd, json!({ "index": i }));
                    sel = (i > 0).then(|| i - 1).or((names.len() > 1).then_some(0));
                }
            }
            let mut active = sel.and_then(|i| names.get(i)).is_some_and(|n| n.1);
            if ui.add_enabled(sel.is_some(), egui::Checkbox::new(&mut active, "Active")).changed() {
                if let Some(i) = sel {
                    run(app, entry_cmd, json!({ "index": i, "active": active }));
                }
            }
            if ui.add_enabled(sel.is_some_and(|i| i > 0), egui::Button::new("Move Up")).clicked() {
                if let Some(i) = sel {
                    run(app, entry_cmd, json!({ "index": i, "move_by": -1 }));
                    sel = Some(i - 1);
                }
            }
            if ui.add_enabled(sel.is_some_and(|i| i + 1 < names.len()), egui::Button::new("Move Down")).clicked() {
                if let Some(i) = sel {
                    run(app, entry_cmd, json!({ "index": i, "move_by": 1 }));
                    sel = Some(i + 1);
                }
            }
        });
    });
    if let Some(i) = sel {
        ui.horizontal(|ui| {
            ui.label("Name:");
            let mut name = names[i].0.clone();
            let r = ui.text_edit_singleline(&mut name);
            if r.lost_focus() && name != names[i].0 {
                run(app, entry_cmd, json!({ "index": i, "name": name }));
            }
        });
    }
    ui.ctx().data_mut(|d| match sel {
        Some(i) => d.insert_temp(id, i),
        None => d.remove::<usize>(id),
    });
    sel
}

fn exposure_rollout(ui: &mut egui::Ui, app: &mut LoomApp, x: &loom_scene::environment::ExposureControl) {
    egui::CollapsingHeader::new("Exposure Control").default_open(true).show(ui, |ui| {
        egui::ComboBox::from_id_salt("env_exposure").width(240.0).selected_text(x.kind.label()).show_ui(ui, |ui| {
            for k in ExposureKind::ALL {
                if ui.selectable_label(k == x.kind, k.label()).clicked() {
                    run(app, "Exposure Control", json!({ "kind": k }));
                }
            }
        });
        let mut active = x.active;
        if ui.checkbox(&mut active, "Active").changed() {
            run(app, "Exposure Control", json!({ "active": active }));
        }
        let mut pb = x.process_background;
        if ui.checkbox(&mut pb, "Process Background and Environment Maps").changed() {
            run(app, "Exposure Control", json!({ "process_background": pb }));
        }
        ui.horizontal(|ui| {
            match app.state.environment_ui.preview.clone() {
                Some(img) => {
                    let version = app.state.environment_ui.preview_version;
                    let tex = preview_texture(ui.ctx(), &img, version);
                    ui.image((tex.id(), egui::vec2(img.width() as f32, img.height() as f32)));
                }
                None => {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(160.0, 120.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, 0.0, egui::Color32::from_gray(30));
                }
            }
            let busy = app.render_job.is_some() || app.state.pending_render.is_some();
            if ui.add_enabled(!busy, egui::Button::new("Render Preview")).clicked() {
                if let Err(e) = app.dispatch(Command::ExposureRenderPreview) {
                    app.state.status_prompt = e;
                }
            }
        });
        if x.kind == ExposureKind::None {
            return;
        }
        ui.label(egui::RichText::new(format!("{} Parameters", x.kind.label())).strong());
        egui::Grid::new("env_exposure_params").num_columns(2).show(ui, |ui| {
            let bright_max = if x.kind == ExposureKind::Logarithmic { 200.0 } else { 100.0 };
            if let Some(v) = spin(ui, app, "Brightness:", x.brightness.get(), 0.0..=bright_max, 0.5) {
                run(app, "Exposure Control", json!({ "brightness": v }));
            }
            if let Some(v) = spin(ui, app, "Contrast:", x.contrast.get(), 0.0..=100.0, 0.5) {
                run(app, "Exposure Control", json!({ "contrast": v }));
            }
            match x.kind {
                ExposureKind::Logarithmic => {
                    if let Some(v) = spin(ui, app, "Mid Tones:", x.mid_tones.get(), 0.01..=20.0, 0.01) {
                        run(app, "Exposure Control", json!({ "mid_tones": v }));
                    }
                }
                _ => {
                    if let Some(v) = spin(ui, app, "Exposure Value:", x.exposure_value.get(), -5.0..=5.0, 0.05) {
                        run(app, "Exposure Control", json!({ "exposure_value": v }));
                    }
                }
            }
        });
    });
}

/// The preview thumbnail as an egui texture, uploaded once per version.
fn preview_texture(ctx: &egui::Context, img: &image::RgbaImage, version: u64) -> egui::TextureHandle {
    let id = sel_id("preview_texture");
    if let Some((v, t)) = ctx.data(|d| d.get_temp::<(u64, egui::TextureHandle)>(id)) {
        if v == version {
            return t;
        }
    }
    // Opaque, as the window shows renders (alpha 0 background in its colour).
    let px: Vec<u8> = img.pixels().flat_map(|p| [p.0[0], p.0[1], p.0[2], 255]).collect();
    let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], &px);
    let t = ctx.load_texture("exposure_preview", ci, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, (version, t.clone())));
    t
}

fn atmosphere_rollout(ui: &mut egui::Ui, app: &mut LoomApp, i: usize, kind: &AtmosphereKind) {
    match kind {
        AtmosphereKind::Fog(f) => {
            egui::CollapsingHeader::new("Fog Parameters").default_open(true).show(ui, |ui| {
                let set = |app: &mut LoomApp, k: &str, v: Value| run(app, "Fog Parameters", json!({ "index": i, k: v }));
                egui::Grid::new("env_fog").num_columns(2).show(ui, |ui| {
                    if let Some(c) = swatch(ui, "Color:", f.color.get()) {
                        set(app, "color", json!(c));
                    }
                });
                let mut fb = f.fog_background;
                if ui.checkbox(&mut fb, "Fog Background").changed() {
                    set(app, "fog_background", json!(fb));
                }
                ui.horizontal(|ui| {
                    ui.label("Type:");
                    if ui.radio(!f.layered, "Standard").clicked() {
                        set(app, "layered", json!(false));
                    }
                    if ui.radio(f.layered, "Layered").clicked() {
                        set(app, "layered", json!(true));
                    }
                });
                ui.add_enabled_ui(!f.layered, |ui| {
                    ui.label(egui::RichText::new("Standard").strong());
                    let mut ex = f.exponential;
                    if ui.checkbox(&mut ex, "Exponential").changed() {
                        set(app, "exponential", json!(ex));
                    }
                    egui::Grid::new("env_fog_std").num_columns(2).show(ui, |ui| {
                        if let Some(v) = spin(ui, app, "Near %:", f.near.get(), 0.0..=100.0, 0.5) {
                            set(app, "near", json!(v));
                        }
                        if let Some(v) = spin(ui, app, "Far %:", f.far.get(), 0.0..=100.0, 0.5) {
                            set(app, "far", json!(v));
                        }
                    });
                });
                ui.add_enabled_ui(f.layered, |ui| {
                    ui.label(egui::RichText::new("Layered").strong());
                    egui::Grid::new("env_fog_layer").num_columns(2).show(ui, |ui| {
                        if let Some(v) = spin(ui, app, "Top:", f.top.get(), f.bottom.get()..=1e6, 0.5) {
                            set(app, "top", json!(v));
                        }
                        if let Some(v) = spin(ui, app, "Bottom:", f.bottom.get(), -1e6..=f.top.get(), 0.5) {
                            set(app, "bottom", json!(v));
                        }
                        if let Some(v) = spin(ui, app, "Density:", f.density.get(), 0.0..=100.0, 0.5) {
                            set(app, "density", json!(v));
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Falloff:");
                        for (fall, label) in [(LayerFalloff::Top, "Top"), (LayerFalloff::Bottom, "Bottom"), (LayerFalloff::None, "None")] {
                            if ui.radio(f.falloff == fall, label).clicked() {
                                set(app, "falloff", json!(fall));
                            }
                        }
                    });
                });
            });
        }
        AtmosphereKind::VolumeFog(v) => {
            egui::CollapsingHeader::new("Volume Fog Parameters").default_open(true).show(ui, |ui| {
                ui.label("No gizmo: the fog fills the scene.");
                let set = |app: &mut LoomApp, k: &str, val: Value| run(app, "Volume Fog Parameters", json!({ "index": i, k: val }));
                egui::Grid::new("env_volume_fog").num_columns(2).show(ui, |ui| {
                    if let Some(c) = swatch(ui, "Color:", v.color.get()) {
                        set(app, "color", json!(c));
                    }
                    if let Some(d) = spin(ui, app, "Density:", v.density.get(), 0.0..=20.0, 0.05) {
                        set(app, "density", json!(d));
                    }
                });
                let mut fb = v.fog_background;
                if ui.checkbox(&mut fb, "Fog Background").changed() {
                    set(app, "fog_background", json!(fb));
                }
            });
        }
        AtmosphereKind::VolumeLight(l) => {
            egui::CollapsingHeader::new("Volume Light Parameters").default_open(true).show(ui, |ui| {
                let set = |app: &mut LoomApp, k: &str, val: Value| run(app, "Volume Light Parameters", json!({ "index": i, k: val }));
                ui.label(egui::RichText::new("Lights").strong());
                let lights: Vec<(u64, String)> = app.state.scene.nodes().filter(|n| matches!(n.object, loom_scene::ObjectKind::Light(_))).map(|n| (n.id.0, n.name.clone())).collect();
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("env_volume_light_pick").selected_text("Pick Light").show_ui(ui, |ui| {
                        for (id, name) in lights.iter().filter(|(id, _)| !l.lights.iter().any(|n| n.0 == *id)) {
                            if ui.selectable_label(false, name).clicked() {
                                set(app, "add_light", json!(id));
                            }
                        }
                    });
                });
                for n in &l.lights {
                    let name = lights.iter().find(|(id, _)| *id == n.0).map(|(_, s)| s.clone()).unwrap_or_else(|| format!("(deleted {})", n.0));
                    ui.horizontal(|ui| {
                        ui.label(name);
                        if ui.small_button("Remove Light").clicked() {
                            set(app, "remove_light", json!(n.0));
                        }
                    });
                }
                egui::Grid::new("env_volume_light").num_columns(2).show(ui, |ui| {
                    if let Some(c) = swatch(ui, "Fog Color:", l.fog_color.get()) {
                        set(app, "fog_color", json!(c));
                    }
                    if let Some(d) = spin(ui, app, "Density:", l.density.get(), 0.0..=100.0, 0.1) {
                        set(app, "density", json!(d));
                    }
                });
            });
        }
    }
}

fn effects_panel(ui: &mut egui::Ui, app: &mut LoomApp) {
    let e = app.state.scene.environment.clone();
    egui::CollapsingHeader::new("Effects").default_open(true).show(ui, |ui| {
        let names: Vec<(String, bool)> = e.effects.iter().map(|a| (a.name.clone(), a.active)).collect();
        if let Some(i) = list(ui, app, "effect", &names, "Effect Entry", "Delete Effect") {
            if let Some(fx) = e.effects.get(i) {
                effect_rollout(ui, app, i, &fx.kind);
            }
        }
    });
    ui.label("Effects apply to renders; Depth of Field shows in Realistic viewports too.");
}

fn effect_rollout(ui: &mut egui::Ui, app: &mut LoomApp, i: usize, kind: &EffectKind) {
    match kind {
        EffectKind::Glow(g) => {
            egui::CollapsingHeader::new("Glow Element").default_open(true).show(ui, |ui| {
                let set = |app: &mut LoomApp, k: &str, v: Value| run(app, "Glow Parameters", json!({ "index": i, k: v }));
                egui::Grid::new("env_glow").num_columns(2).show(ui, |ui| {
                    if let Some(v) = spin(ui, app, "Size:", g.size.get(), 0.0..=100.0, 0.1) {
                        set(app, "size", json!(v));
                    }
                    if let Some(v) = spin(ui, app, "Intensity:", g.intensity.get(), 0.0..=1000.0, 1.0) {
                        set(app, "intensity", json!(v));
                    }
                    if let Some(v) = spin(ui, app, "Use Source Color:", g.use_source_color.get(), 0.0..=100.0, 0.5) {
                        set(app, "use_source_color", json!(v));
                    }
                    if let Some(c) = swatch(ui, "Radial Color (inner):", g.inner_color.get()) {
                        set(app, "inner_color", json!(c));
                    }
                    if let Some(c) = swatch(ui, "Radial Color (outer):", g.outer_color.get()) {
                        set(app, "outer_color", json!(c));
                    }
                    if let Some(v) = spin(ui, app, "Bright:", g.bright.get(), 0.0..=255.0, 1.0) {
                        set(app, "bright", json!(v));
                    }
                });
                ui.label("Image source: Whole; filter: Bright.");
            });
        }
        EffectKind::Blur(b) => {
            egui::CollapsingHeader::new("Blur Parameters").default_open(true).show(ui, |ui| {
                let set = |app: &mut LoomApp, k: &str, v: Value| run(app, "Blur Parameters", json!({ "index": i, k: v }));
                ui.horizontal(|ui| {
                    for (t, label) in [(BlurType::Uniform, "Uniform"), (BlurType::Directional, "Directional"), (BlurType::Radial, "Radial")] {
                        if ui.radio(b.blur_type == t, label).clicked() {
                            set(app, "blur_type", json!(t));
                        }
                    }
                });
                egui::Grid::new("env_blur").num_columns(2).show(ui, |ui| match b.blur_type {
                    BlurType::Uniform => {
                        if let Some(v) = spin(ui, app, "Pixel Radius:", b.radius.get(), 0.0..=1000.0, 0.1) {
                            set(app, "radius", json!(v));
                        }
                    }
                    BlurType::Directional => {
                        if let Some(v) = spin(ui, app, "U Pixel Radius:", b.u_radius.get(), 0.0..=1000.0, 0.1) {
                            set(app, "u_radius", json!(v));
                        }
                        if let Some(v) = spin(ui, app, "U Trail:", b.u_trail.get(), -100.0..=100.0, 0.5) {
                            set(app, "u_trail", json!(v));
                        }
                        if let Some(v) = spin(ui, app, "V Pixel Radius:", b.v_radius.get(), 0.0..=1000.0, 0.1) {
                            set(app, "v_radius", json!(v));
                        }
                        if let Some(v) = spin(ui, app, "V Trail:", b.v_trail.get(), -100.0..=100.0, 0.5) {
                            set(app, "v_trail", json!(v));
                        }
                        if let Some(v) = spin(ui, app, "Rotation:", b.rotation.get(), -360.0..=360.0, 0.5) {
                            set(app, "rotation", json!(v));
                        }
                    }
                    BlurType::Radial => {
                        if let Some(v) = spin(ui, app, "Pixel Radius:", b.radius.get(), 0.0..=1000.0, 0.1) {
                            set(app, "radius", json!(v));
                        }
                        if let Some(v) = spin(ui, app, "Trail:", b.radial_trail.get(), -100.0..=100.0, 0.5) {
                            set(app, "radial_trail", json!(v));
                        }
                        if let Some(v) = spin(ui, app, "X Origin:", b.origin[0], -1e5..=1e5, 1.0) {
                            set(app, "origin", json!([v, b.origin[1]]));
                        }
                        if let Some(v) = spin(ui, app, "Y Origin:", b.origin[1], -1e5..=1e5, 1.0) {
                            set(app, "origin", json!([b.origin[0], v]));
                        }
                    }
                });
                let mut aa = b.affect_alpha;
                if ui.checkbox(&mut aa, "Affect Alpha").changed() {
                    set(app, "affect_alpha", json!(aa));
                }
                ui.label(egui::RichText::new("Pixel Selections").strong());
                ui.horizontal(|ui| {
                    if ui.radio(!b.non_background, "Whole Image").clicked() {
                        set(app, "non_background", json!(false));
                    }
                    if ui.radio(b.non_background, "Non-Background").clicked() {
                        set(app, "non_background", json!(true));
                    }
                });
                egui::Grid::new("env_blur_sel").num_columns(2).show(ui, |ui| {
                    if let Some(v) = spin(ui, app, "Brighten %:", b.brighten.get(), 0.0..=100.0, 0.5) {
                        set(app, "brighten", json!(v));
                    }
                    if let Some(v) = spin(ui, app, "Blend %:", b.blend.get(), 0.0..=100.0, 0.5) {
                        set(app, "blend", json!(v));
                    }
                });
            });
        }
        EffectKind::BrightnessContrast(b) => {
            egui::CollapsingHeader::new("Brightness and Contrast Parameters").default_open(true).show(ui, |ui| {
                let set = |app: &mut LoomApp, k: &str, v: Value| run(app, "Brightness and Contrast Parameters", json!({ "index": i, k: v }));
                egui::Grid::new("env_bc").num_columns(2).show(ui, |ui| {
                    if let Some(v) = spin(ui, app, "Brightness:", b.brightness.get(), 0.0..=1.0, 0.005) {
                        set(app, "brightness", json!(v));
                    }
                    if let Some(v) = spin(ui, app, "Contrast:", b.contrast.get(), 0.0..=1.0, 0.005) {
                        set(app, "contrast", json!(v));
                    }
                });
                let mut ib = b.ignore_background;
                if ui.checkbox(&mut ib, "Ignore Background").changed() {
                    set(app, "ignore_background", json!(ib));
                }
            });
        }
        EffectKind::ColorBalance(c) => {
            egui::CollapsingHeader::new("Color Balance Parameters").default_open(true).show(ui, |ui| {
                let set = |app: &mut LoomApp, k: &str, v: Value| run(app, "Color Balance Parameters", json!({ "index": i, k: v }));
                egui::Grid::new("env_cb").num_columns(2).show(ui, |ui| {
                    if let Some(v) = spin(ui, app, "Cyan / Red:", c.cyan_red.get(), -100.0..=100.0, 0.5) {
                        set(app, "cyan_red", json!(v));
                    }
                    if let Some(v) = spin(ui, app, "Magenta / Green:", c.magenta_green.get(), -100.0..=100.0, 0.5) {
                        set(app, "magenta_green", json!(v));
                    }
                    if let Some(v) = spin(ui, app, "Yellow / Blue:", c.yellow_blue.get(), -100.0..=100.0, 0.5) {
                        set(app, "yellow_blue", json!(v));
                    }
                });
                let mut pl = c.preserve_luminosity;
                if ui.checkbox(&mut pl, "Preserve Luminosity").changed() {
                    set(app, "preserve_luminosity", json!(pl));
                }
                let mut ib = c.ignore_background;
                if ui.checkbox(&mut ib, "Ignore Background").changed() {
                    set(app, "ignore_background", json!(ib));
                }
            });
        }
        EffectKind::FilmGrain(f) => {
            egui::CollapsingHeader::new("Film Grain Parameters").default_open(true).show(ui, |ui| {
                let set = |app: &mut LoomApp, k: &str, v: Value| run(app, "Film Grain Parameters", json!({ "index": i, k: v }));
                egui::Grid::new("env_grain").num_columns(2).show(ui, |ui| {
                    if let Some(v) = spin(ui, app, "Grain:", f.grain.get(), 0.0..=10.0, 0.01) {
                        set(app, "grain", json!(v));
                    }
                });
                let mut ib = f.ignore_background;
                if ui.checkbox(&mut ib, "Ignore Background").changed() {
                    set(app, "ignore_background", json!(ib));
                }
            });
        }
        EffectKind::DepthOfField(d) => {
            egui::CollapsingHeader::new("Depth of Field Parameters").default_open(true).show(ui, |ui| {
                let set = |app: &mut LoomApp, k: &str, v: Value| run(app, "Depth of Field Parameters", json!({ "index": i, k: v }));
                ui.label(egui::RichText::new("Focal Point").strong());
                let nodes: Vec<(u64, String)> = app.state.scene.nodes().map(|n| (n.id.0, n.name.clone())).collect();
                let current = d.focal_node.and_then(|n| nodes.iter().find(|(id, _)| *id == n.0)).map(|(_, s)| s.clone()).unwrap_or_else(|| "Use Camera".into());
                egui::ComboBox::from_id_salt("env_dof_node").selected_text(current).show_ui(ui, |ui| {
                    if ui.selectable_label(d.focal_node.is_none(), "Use Camera").clicked() {
                        set(app, "focal_node", json!(0));
                    }
                    for (id, name) in &nodes {
                        if ui.selectable_label(d.focal_node.is_some_and(|n| n.0 == *id), name).clicked() {
                            set(app, "focal_node", json!(id));
                        }
                    }
                });
                ui.label(egui::RichText::new("Focal Parameters").strong());
                egui::Grid::new("env_dof").num_columns(2).show(ui, |ui| {
                    if let Some(v) = spin(ui, app, "Horiz Focal Loss:", d.horiz_loss.get(), 0.0..=100.0, 0.1) {
                        set(app, "horiz_loss", json!(v));
                    }
                    if let Some(v) = spin(ui, app, "Vert Focal Loss:", d.vert_loss.get(), 0.0..=100.0, 0.1) {
                        set(app, "vert_loss", json!(v));
                    }
                    if let Some(v) = spin(ui, app, "Focal Range:", d.focal_range.get(), 0.0..=d.focal_limit.get(), 0.5) {
                        set(app, "focal_range", json!(v));
                    }
                    if let Some(v) = spin(ui, app, "Focal Limit:", d.focal_limit.get(), d.focal_range.get()..=1e6, 0.5) {
                        set(app, "focal_limit", json!(v));
                    }
                });
            });
        }
    }
}
