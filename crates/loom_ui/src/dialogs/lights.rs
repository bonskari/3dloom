//! Tools > Light Lister, and a light's Exclude/Include dialog.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;
use loom_scene::light::{LightObject, LightType, DECAY_TYPES, SHADOW_TYPES};
use loom_scene::{NodeId, ObjectKind};

fn set(app: &mut LoomApp, id: NodeId, name: &str, value: f64) {
    let _ = app.dispatch(Command::SetParameter { id: id.0, name: name.into(), value });
}

/// A spinner whose drag is one undo step.
fn spinner(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, name: &str, value: f32, speed: f64) {
    let mut v = value;
    let r = ui.add(crate::spinner::spin(&mut v, ..).speed(speed).fixed_decimals(2));
    if r.drag_started() {
        app.state.undo.begin(name);
    }
    if r.changed() && (v - value).abs() > 1e-9 {
        set(app, id, name, v as f64);
    }
    if r.drag_stopped() {
        app.state.undo.end();
    }
}

#[derive(Clone, Copy, PartialEq, Default)]
struct ListerState {
    selected_only: bool,
    /// Configuration > General Settings instead of the light table.
    general: bool,
    /// General Settings > Selected Lights (else All Lights).
    general_selected: bool,
}

/// Light Lister: every light (or the selected ones) with its main settings
/// in one table; a name button selects the light.
pub fn light_lister(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let key = egui::Id::new("light_lister_state");
    let mut st: ListerState = ctx.data(|d| d.get_temp(key)).unwrap_or_default();
    let window = egui::Window::new("Light Lister").resizable(true).default_width(1240.0);
    crate::os_window::OsWindow::new("Light Lister", "Light Lister", [1260.0, 420.0]).show(ctx, open, window, |ui| {
        ui.horizontal(|ui| {
            ui.label("Configuration:");
            if ui.radio(!st.general && !st.selected_only, "All Lights").clicked() {
                (st.general, st.selected_only) = (false, false);
            }
            if ui.radio(!st.general && st.selected_only, "Selected Lights").clicked() {
                (st.general, st.selected_only) = (false, true);
            }
            if ui.radio(st.general, "General Settings").clicked() {
                st.general = true;
            }
        });
        ui.separator();
        if st.general {
            general_settings(ui, app, &mut st.general_selected);
            return;
        }
        let ids: Vec<NodeId> = loom_commands::light::lights(&app.state.scene).into_iter().filter(|id| !st.selected_only || app.state.selection.contains(id)).collect();
        if ids.is_empty() {
            ui.label("No lights.");
            return;
        }
        // Two lists, as in the reference: the standard lights, then the photometric ones.
        let lights: Vec<(NodeId, String, LightObject)> = ids.iter().filter_map(|id| app.state.scene.node(*id).and_then(|n| Some((*id, n.name.clone(), n.object.light()?.clone())))).collect();
        let (photometric, standard): (Vec<_>, Vec<_>) = lights.into_iter().partition(|(_, _, l)| l.photometric.is_some());
        egui::ScrollArea::both().show(ui, |ui| {
            for (title, list, columns) in [("Standard Lights", standard, STANDARD_COLUMNS), ("Photometric Lights", photometric, PHOTOMETRIC_COLUMNS)] {
                if list.is_empty() {
                    continue;
                }
                ui.label(egui::RichText::new(title).strong());
                egui::Grid::new(("light_lister", title)).striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
                    for h in columns {
                        ui.label(egui::RichText::new(h).strong());
                    }
                    ui.end_row();
                    for (id, name, l) in &list {
                        lister_row(ui, app, *id, name, l);
                        ui.end_row();
                    }
                });
                ui.add_space(6.0);
            }
        });
    });
    ctx.data_mut(|d| d.insert_temp(key, st));
}

/// The Lights rollout's columns: standard lights, photometric lights.
const STANDARD_COLUMNS: [&str; 14] = ["On", "Name", "Multiplier", "Color", "Shadows", "", "Map Size", "Bias", "Sm.Range", "Transp.", "Int.", "Qual.", "Decay", "Start"];
const PHOTOMETRIC_COLUMNS: [&str; 14] = ["On", "Name", "Intensity", "Color", "Shadows", "", "Map Size", "Bias", "Sm.Range", "Transp.", "Int.", "Qual.", "Length", "Width"];

/// An integer spinner whose drag is one undo step.
fn int_spinner(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, name: &str, value: u32, range: std::ops::RangeInclusive<u32>, speed: f64) {
    let mut v = value;
    let r = ui.add(crate::spinner::spin(&mut v, range).speed(speed));
    if r.drag_started() {
        app.state.undo.begin(name);
    }
    if r.changed() && v != value {
        set(app, id, name, v as f64);
    }
    if r.drag_stopped() {
        app.state.undo.end();
    }
}

/// The shadow columns of a light's row: Shadows, the shadow type, Map Size
/// (shadow maps only), Bias (the type's own), Sm.Range (shadow maps only),
/// and Transp. / Int. / Qual. (advanced ray-traced and area shadows only).
fn shadow_cells(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, l: &LightObject, shadows_name: &str) {
    let mut sh = l.shadows_on.get();
    if ui.checkbox(&mut sh, "").changed() {
        set(app, id, shadows_name, sh as u32 as f64);
    }
    let st = l.shadow_type.get();
    egui::ComboBox::from_id_salt(("lister_shadow_type", id.0)).selected_text(SHADOW_TYPES[(st as usize).min(SHADOW_TYPES.len() - 1)]).show_ui(ui, |ui| {
        for (i, t) in SHADOW_TYPES.iter().enumerate() {
            if ui.selectable_label(st == i as u32, *t).clicked() && st != i as u32 {
                set(app, id, "Shadow Type", i as f64);
            }
        }
    });
    let map = st == loom_scene::light::SHADOW_MAP;
    let advanced = st >= loom_scene::light::ADV_RAY_TRACED;
    ui.add_enabled_ui(map, |ui| int_spinner(ui, app, id, "Map Size", l.map_size.get(), 1..=10_000, 4.0));
    if advanced {
        spinner(ui, app, id, "Shadow Bias", l.area_bias.get(), 0.01);
    } else {
        spinner(ui, app, id, "Map Bias", l.map_bias.get(), 0.01);
    }
    ui.add_enabled_ui(map && loom_bevy::lights::supports("Sample Range"), |ui| spinner(ui, app, id, "Sample Range", l.sample_range.get(), 0.05));
    ui.add_enabled_ui(advanced, |ui| {
        let mut t = l.transparent_shadows.get();
        if ui.checkbox(&mut t, "").changed() {
            set(app, id, "Transparent Shadows", t as u32 as f64);
        }
    });
    ui.add_enabled_ui(advanced, |ui| int_spinner(ui, app, id, "Shadow Integrity", l.shadow_integrity.get(), 1..=15, 0.1));
    ui.add_enabled_ui(advanced, |ui| int_spinner(ui, app, id, "Shadow Quality", l.shadow_quality.get(), 1..=15, 0.1));
}

fn lister_row(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, name: &str, l: &LightObject) {
    let mut on = l.on.get();
    if ui.checkbox(&mut on, "").changed() {
        set(app, id, "On", on as u32 as f64);
    }
    if ui.selectable_label(app.state.selection.contains(&id), name).clicked() {
        let _ = app.dispatch(Command::SelectNodes { ids: vec![id.0], op: loom_commands::SelectOp::Replace });
    }
    if let Some(ph) = &l.photometric {
        // Photometric lights list their intensity (in the light's unit) and filter colour.
        let unit = ph.intensity_unit.get();
        ui.horizontal(|ui| {
            spinner(ui, app, id, "Intensity", ph.from_candela(ph.candela.get(), unit, l.falloff.get()), 1.0);
            ui.label(loom_scene::light::INTENSITY_UNITS[unit.min(2) as usize]);
        });
        let c = ph.filter_color.get();
        let mut rgb = [0, 1, 2].map(|i| (c[i].clamp(0.0, 1.0) * 255.0).round() as u8);
        if crate::color_selector::srgb(ui, &mut rgb).changed() {
            app.state.undo.begin("Filter Color");
            for (ch, v) in ["R", "G", "B"].iter().zip(rgb) {
                set(app, id, &format!("Filter Color {ch}"), v as f64 / 255.0);
            }
            app.state.undo.end();
        }
        shadow_cells(ui, app, id, l, "Shadows On");
        // Length for Line and Rectangle shapes, Width for Rectangle.
        let shape = loom_scene::light::LIGHT_SHAPES[(ph.shape.get() as usize).min(5)];
        ui.add_enabled_ui(matches!(shape, "Line" | "Rectangle"), |ui| spinner(ui, app, id, "Shape Length", ph.shape_length.get(), 0.1));
        ui.add_enabled_ui(shape == "Rectangle", |ui| spinner(ui, app, id, "Shape Width", ph.shape_width.get(), 0.1));
        return;
    }
    spinner(ui, app, id, "Multiplier", l.multiplier.get(), 0.01);
    let c = l.color.get();
    let mut rgb = [0, 1, 2].map(|i| (c[i].clamp(0.0, 1.0) * 255.0).round() as u8);
    if crate::color_selector::srgb(ui, &mut rgb).changed() {
        app.state.undo.begin("Color");
        for (ch, v) in ["R", "G", "B"].iter().zip(rgb) {
            set(app, id, &format!("Color {ch}"), v as f64 / 255.0);
        }
        app.state.undo.end();
    }
    if l.light_type == LightType::Skylight {
        let mut sh = l.shadows_on.get();
        if ui.checkbox(&mut sh, "").changed() {
            set(app, id, "Cast Shadows", sh as u32 as f64);
        }
        for _ in 0..9 {
            ui.label("");
        }
        return;
    }
    shadow_cells(ui, app, id, l, "Shadows On");
    let d = l.decay.get();
    egui::ComboBox::from_id_salt(("lister_decay", id.0)).selected_text(DECAY_TYPES[d.min(2) as usize]).show_ui(ui, |ui| {
        for (i, t) in DECAY_TYPES.iter().enumerate() {
            if ui.selectable_label(d == i as u32, *t).clicked() && d != i as u32 {
                set(app, id, "Decay Type", i as f64);
            }
        }
    });
    spinner(ui, app, id, "Decay Start", l.decay_start.get(), 0.5);
}

/// Sets `name` to `value` on every light in `ids`, one undo step (or part
/// of the drag's step).
fn set_all(app: &mut LoomApp, ids: &[NodeId], name: &str, value: f64) {
    app.state.undo.begin(name);
    for id in ids {
        set(app, *id, name, value);
    }
    app.state.undo.end();
}

/// A General Settings spinner: shows the first light's value, sets every
/// light's; a drag is one undo step.
fn spinner_all(ui: &mut egui::Ui, app: &mut LoomApp, ids: &[NodeId], name: &str, value: f32, speed: f64, decimals: usize) {
    let mut v = value;
    let r = ui.add(crate::spinner::spin(&mut v, ..).speed(speed).fixed_decimals(decimals));
    if r.drag_started() {
        app.state.undo.begin(name);
    }
    if r.changed() && (v - value).abs() > 1e-9 {
        set_all(app, ids, name, v as f64);
    }
    if r.drag_stopped() {
        app.state.undo.end();
    }
}

/// Light Lister > General Settings: one row of settings applied to every
/// light (or the selected ones), and the Environment's Global Tint, Global
/// Level and Ambient Color. The row shows the first affected light's values.
fn general_settings(ui: &mut egui::Ui, app: &mut LoomApp, selected: &mut bool) {
    ui.horizontal(|ui| {
        ui.radio_value(selected, true, "Selected Lights");
        ui.radio_value(selected, false, "All Lights");
    });
    let ids: Vec<NodeId> = loom_commands::light::lights(&app.state.scene).into_iter().filter(|id| !*selected || app.state.selection.contains(id)).collect();
    let light_of = |id: &NodeId| app.state.scene.node(*id).and_then(|n| n.object.light());
    // Multiplier, Color and decay belong to the standard lights, the
    // shadow settings to every light but skylights, Length and Width to the
    // photometric lights.
    let standard: Vec<NodeId> = ids.iter().copied().filter(|id| light_of(id).is_some_and(|l| l.photometric.is_none() && l.light_type != LightType::Skylight)).collect();
    let shadowing: Vec<NodeId> = ids.iter().copied().filter(|id| light_of(id).is_some_and(|l| l.light_type != LightType::Skylight)).collect();
    let photometric: Vec<NodeId> = ids.iter().copied().filter(|id| light_of(id).is_some_and(|l| l.photometric.is_some())).collect();
    let first = |list: &[NodeId]| list.first().and_then(|id| app.state.scene.node(*id)).and_then(|n| n.object.light()).cloned();
    let Some(any) = first(&ids) else {
        ui.label("No lights.");
        return;
    };
    let (l, sh, ph) = (first(&standard), first(&shadowing), first(&photometric));
    egui::Grid::new("light_lister_general").striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
        for h in ["On", "Multiplier", "Color", "Shadows", "", "Map Size", "Bias", "Sm.Range", "Transp.", "Int.", "Qual.", "Decay", "Start", "Length", "Width"] {
            ui.label(egui::RichText::new(h).strong());
        }
        ui.end_row();
        let mut on = any.on.get();
        if ui.checkbox(&mut on, "").changed() {
            set_all(app, &ids, "On", on as u32 as f64);
        }
        ui.add_enabled_ui(l.is_some(), |ui| {
            let multiplier = l.as_ref().map_or(1.0, |l| l.multiplier.get());
            spinner_all(ui, app, &standard, "Multiplier", multiplier, 0.01, 2);
        });
        ui.add_enabled_ui(l.is_some(), |ui| {
            let c = l.as_ref().map_or(glam::Vec3::ONE, |l| l.color.get());
            let mut rgb = [0, 1, 2].map(|i| (c[i].clamp(0.0, 1.0) * 255.0).round() as u8);
            if crate::color_selector::srgb(ui, &mut rgb).changed() {
                app.state.undo.begin("Color");
                for (ch, v) in ["R", "G", "B"].iter().zip(rgb) {
                    set_all(app, &standard, &format!("Color {ch}"), v as f64 / 255.0);
                }
                app.state.undo.end();
            }
        });
        let d = LightObject::default();
        let s = sh.as_ref().unwrap_or(&d);
        ui.add_enabled_ui(sh.is_some(), |ui| {
            let mut on = s.shadows_on.get();
            if ui.checkbox(&mut on, "").changed() {
                set_all(app, &shadowing, "Shadows On", on as u32 as f64);
            }
        });
        ui.add_enabled_ui(sh.is_some(), |ui| {
            let st = s.shadow_type.get();
            egui::ComboBox::from_id_salt("lister_general_shadow_type").selected_text(SHADOW_TYPES[(st as usize).min(SHADOW_TYPES.len() - 1)]).show_ui(ui, |ui| {
                for (i, t) in SHADOW_TYPES.iter().enumerate() {
                    if ui.selectable_label(st == i as u32, *t).clicked() && st != i as u32 {
                        set_all(app, &shadowing, "Shadow Type", i as f64);
                    }
                }
            });
        });
        // Each shadow value goes to the lights whose shadow type uses it.
        let of_type = |app: &LoomApp, advanced: bool| -> Vec<NodeId> { shadowing.iter().copied().filter(|id| app.state.scene.node(*id).and_then(|n| n.object.light()).is_some_and(|l| (l.shadow_type.get() >= loom_scene::light::ADV_RAY_TRACED) == advanced)).collect() };
        let (basic, advanced) = (of_type(app, false), of_type(app, true));
        let maps: Vec<NodeId> = basic.iter().copied().filter(|id| app.state.scene.node(*id).and_then(|n| n.object.light()).is_some_and(|l| l.shadow_type.get() == loom_scene::light::SHADOW_MAP)).collect();
        spinner_all(ui, app, &maps, "Map Size", s.map_size.get() as f32, 4.0, 0);
        // Bias: the shadow map's / ray-traced bias, or the advanced types' Shadow Bias.
        let bias = if s.shadow_type.get() >= loom_scene::light::ADV_RAY_TRACED { s.area_bias.get() } else { s.map_bias.get() };
        let mut v = bias;
        let r = ui.add(crate::spinner::spin(&mut v, ..).speed(0.01).fixed_decimals(2));
        if r.drag_started() {
            app.state.undo.begin("Bias");
        }
        if r.changed() && (v - bias).abs() > 1e-9 {
            app.state.undo.begin("Bias");
            set_all(app, &basic, "Map Bias", v as f64);
            set_all(app, &advanced, "Shadow Bias", v as f64);
            app.state.undo.end();
        }
        if r.drag_stopped() {
            app.state.undo.end();
        }
        ui.add_enabled_ui(loom_bevy::lights::supports("Sample Range"), |ui| spinner_all(ui, app, &maps, "Sample Range", s.sample_range.get(), 0.05, 2));
        let mut t = s.transparent_shadows.get();
        if ui.checkbox(&mut t, "").changed() {
            set_all(app, &advanced, "Transparent Shadows", t as u32 as f64);
        }
        spinner_all(ui, app, &advanced, "Shadow Integrity", s.shadow_integrity.get() as f32, 0.1, 0);
        spinner_all(ui, app, &advanced, "Shadow Quality", s.shadow_quality.get() as f32, 0.1, 0);
        let decay = l.as_ref().map_or(0, |l| l.decay.get());
        ui.add_enabled_ui(l.is_some(), |ui| {
            egui::ComboBox::from_id_salt("lister_general_decay").selected_text(DECAY_TYPES[decay.min(2) as usize]).show_ui(ui, |ui| {
                for (i, t) in DECAY_TYPES.iter().enumerate() {
                    if ui.selectable_label(decay == i as u32, *t).clicked() && decay != i as u32 {
                        set_all(app, &standard, "Decay Type", i as f64);
                    }
                }
            });
        });
        ui.add_enabled_ui(l.is_some(), |ui| spinner_all(ui, app, &standard, "Decay Start", l.as_ref().map_or(0.0, |l| l.decay_start.get()), 0.5, 2));
        let shape = ph.as_ref().and_then(|l| l.photometric.as_ref());
        ui.add_enabled_ui(shape.is_some(), |ui| spinner_all(ui, app, &photometric, "Shape Length", shape.map_or(0.0, |p| p.shape_length.get()), 0.1, 2));
        ui.add_enabled_ui(shape.is_some(), |ui| spinner_all(ui, app, &photometric, "Shape Width", shape.map_or(0.0, |p| p.shape_width.get()), 0.1, 2));
        ui.end_row();
    });
    ui.separator();
    // The Environment's Global Lighting, as in Environment and Effects.
    let e = &app.state.scene.environment;
    let (tint, level, ambient) = (e.tint.get(), e.level.get(), e.ambient.get());
    let run = |app: &mut LoomApp, args: serde_json::Value| {
        let _ = app.dispatch_json(&serde_json::json!({ "cmd": "Environment Settings", "args": args }));
    };
    ui.horizontal(|ui| {
        ui.label("Global Tint:");
        let mut rgb = tint.to_array().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
        if crate::color_selector::srgb(ui, &mut rgb).changed() {
            run(app, serde_json::json!({ "tint": rgb.map(|v| v as f32 / 255.0) }));
        }
        ui.label("Global Level:");
        let mut v = level;
        if ui.add(crate::spinner::spin(&mut v, 0.0..=100.0).speed(0.01).fixed_decimals(2)).changed() {
            run(app, serde_json::json!({ "level": v }));
        }
        ui.label("Ambient Color:");
        let mut rgb = ambient.to_array().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
        if crate::color_selector::srgb(ui, &mut rgb).changed() {
            run(app, serde_json::json!({ "ambient": rgb.map(|v| v as f32 / 255.0) }));
        }
    });
}

/// The Exclude/Include dialog's working copy until OK.
#[derive(Clone, Default)]
struct ExcludeState {
    include: bool,
    scope: u32,
    nodes: Vec<NodeId>,
    /// Highlighted rows of the scene list and of the exclusion list.
    left: Vec<NodeId>,
    right: Vec<NodeId>,
    filter: String,
}

/// Exclude/Include: move scene objects into the light's list, choose
/// Exclude or Include and what it applies to; OK applies it as one undo step.
pub fn light_exclude(ctx: &egui::Context, app: &mut LoomApp, light: u64, open: &mut bool) {
    let key = egui::Id::new(("light_exclude_state", light));
    let Some(l) = app.state.scene.node(NodeId(light)).and_then(|n| n.object.light()).cloned() else {
        *open = false;
        return;
    };
    let mut st: ExcludeState = ctx.data(|d| d.get_temp(key)).unwrap_or_else(|| ExcludeState { include: l.exclusion.include, scope: l.exclusion.scope, nodes: l.exclusion.nodes.clone(), ..Default::default() });
    let mut done: Option<bool> = None;
    let scene = &app.state.scene;
    let objects: Vec<(NodeId, String)> = scene
        .nodes()
        .filter(|n| !matches!(n.object, ObjectKind::Light(_)) && !loom_commands::light::is_light_target(scene, n.id))
        .map(|n| (n.id, n.name.clone()))
        .collect();
    let name_of = |id: NodeId| objects.iter().find(|(i, _)| *i == id).map(|(_, n)| n.clone()).unwrap_or_else(|| format!("#{}", id.0));
    egui::Window::new("Exclude/Include").open(open).resizable(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.radio_value(&mut st.include, false, "Exclude");
            ui.radio_value(&mut st.include, true, "Include");
        });
        ui.horizontal(|ui| {
            for (i, s) in loom_scene::light::Exclusion::SCOPES.iter().enumerate() {
                ui.radio_value(&mut st.scope, i as u32, *s);
            }
        });
        if st.scope != 1 {
            ui.label(egui::RichText::new("The Bevy renderer applies Shadow Casting only.").small());
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label("Scene Objects");
                ui.add(egui::TextEdit::singleline(&mut st.filter).hint_text("Search").desired_width(160.0));
                egui::ScrollArea::vertical().id_salt("exclude_left").max_height(220.0).show(ui, |ui| {
                    let f = st.filter.to_lowercase();
                    for (id, name) in objects.iter().filter(|(id, n)| !st.nodes.contains(id) && n.to_lowercase().contains(&f)) {
                        let on = st.left.contains(id);
                        if ui.selectable_label(on, name).clicked() {
                            if on {
                                st.left.retain(|x| x != id);
                            } else {
                                st.left.push(*id);
                            }
                        }
                    }
                });
            });
            ui.vertical(|ui| {
                ui.add_space(60.0);
                if ui.button(">>").clicked() {
                    let moved: Vec<NodeId> = std::mem::take(&mut st.left);
                    st.nodes.extend(moved);
                }
                if ui.button("<<").clicked() {
                    let moved: Vec<NodeId> = std::mem::take(&mut st.right);
                    st.nodes.retain(|n| !moved.contains(n));
                }
            });
            ui.vertical(|ui| {
                ui.label(if st.include { "Included" } else { "Excluded" });
                egui::ScrollArea::vertical().id_salt("exclude_right").max_height(240.0).show(ui, |ui| {
                    for id in st.nodes.clone() {
                        let on = st.right.contains(&id);
                        if ui.selectable_label(on, name_of(id)).clicked() {
                            if on {
                                st.right.retain(|x| *x != id);
                            } else {
                                st.right.push(id);
                            }
                        }
                    }
                });
                if ui.button("Clear").clicked() {
                    st.nodes.clear();
                    st.right.clear();
                }
            });
        });
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
    match done {
        Some(apply) => {
            if apply {
                let _ = app.dispatch(Command::SetLightExclusion { id: light, include: st.include, scope: st.scope, nodes: st.nodes.iter().map(|n| n.0).collect() });
            }
            ctx.data_mut(|d| d.remove::<ExcludeState>(key));
            *open = false;
        }
        None if !*open => ctx.data_mut(|d| d.remove::<ExcludeState>(key)),
        None => ctx.data_mut(|d| d.insert_temp(key, st)),
    }
}

/// The Light Include/Exclude tool: Geometry group (Include / Exclude, Assign
/// to Light picks the light for the selected objects) and List Light
/// Properties (the displayed light, Choose Light, Clear Light, its list's
/// Include / Exclude and Objects).
pub fn light_include_exclude(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    use loom_commands::PickMode;
    let shown = app.state.incl_excl_light.and_then(|id| app.state.scene.node(id).and_then(|n| n.object.light().map(|l| (id, n.name.clone(), l.exclusion.clone()))));
    egui::Window::new("Light Incl/Excl").open(open).resizable(false).show(ctx, |ui| {
        ui.label(egui::RichText::new("Geometry").strong());
        ui.horizontal(|ui| {
            ui.radio_value(&mut app.state.incl_excl_exclude, false, "Include");
            ui.radio_value(&mut app.state.incl_excl_exclude, true, "Exclude");
        });
        let assigning = app.state.pick_mode == Some(PickMode::LightInclExclAssign);
        if ui.selectable_label(assigning, "Assign to Light").clicked() {
            app.state.pick_mode = if assigning { None } else { Some(PickMode::LightInclExclAssign) };
        }
        ui.separator();
        ui.label(egui::RichText::new("List Light Properties").strong());
        ui.label(shown.as_ref().map_or("None".to_owned(), |(_, name, _)| name.clone()));
        ui.horizontal(|ui| {
            let choosing = app.state.pick_mode == Some(PickMode::LightInclExclChoose);
            if ui.selectable_label(choosing, "Choose Light").clicked() {
                app.state.pick_mode = if choosing { None } else { Some(PickMode::LightInclExclChoose) };
            }
            if let Some((id, _, ex)) = &shown {
                if ui.button("Clear Light").clicked() {
                    let _ = app.dispatch(Command::SetLightExclusion { id: id.0, include: ex.include, scope: ex.scope, nodes: Vec::new() });
                }
            }
        });
        let Some((id, _, ex)) = shown else { return };
        ui.horizontal(|ui| {
            for (include, label) in [(true, "Include"), (false, "Exclude")] {
                if ui.radio(ex.include == include, label).clicked() && ex.include != include {
                    let _ = app.dispatch(Command::SetLightExclusion { id: id.0, include, scope: ex.scope, nodes: ex.nodes.iter().map(|n| n.0).collect() });
                }
            }
        });
        ui.label("Objects");
        egui::ScrollArea::vertical().id_salt("incl_excl_objects").max_height(160.0).show(ui, |ui| {
            for n in &ex.nodes {
                ui.label(app.state.scene.node(*n).map_or_else(|| format!("#{}", n.0), |n| n.name.clone()));
            }
        });
    });
}

#[derive(Clone, Copy, PartialEq)]
struct DefaultLightsState {
    key: bool,
    fill: bool,
    scale: f32,
}

/// Views > Add Default Lights to Scene: Add Default Key Light / Fill Light
/// and Distance Scaling; OK adds them as omni lights.
pub fn add_default_lights(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let key = egui::Id::new("add_default_lights_state");
    let mut st: DefaultLightsState = ctx.data(|d| d.get_temp(key)).unwrap_or(DefaultLightsState { key: true, fill: true, scale: 1.0 });
    let mut done = None;
    egui::Window::new("Add Default Lights to Scene").open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.checkbox(&mut st.key, "Add Default Key Light");
        ui.checkbox(&mut st.fill, "Add Default Fill Light");
        ui.horizontal(|ui| {
            ui.label("Distance Scaling:");
            ui.add(crate::spinner::spin(&mut st.scale, 0.0..=1000.0).speed(0.01).fixed_decimals(2));
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(st.key || st.fill, egui::Button::new("OK")).clicked() {
                done = Some(true);
            }
            if ui.button("Cancel").clicked() {
                done = Some(false);
            }
        });
    });
    if let Some(apply) = done {
        if apply {
            let _ = app.dispatch(Command::AddDefaultLightsToScene { key: Some(st.key), fill: Some(st.fill), distance_scale: Some(st.scale) });
        }
        *open = false;
    }
    ctx.data_mut(|d| d.insert_temp(key, st));
}

/// Sunlight > Get Location: the Geographic Location city list; OK (or a
/// double-click) puts the city's latitude, longitude and time zone on the sun.
pub fn geographic_location(ctx: &egui::Context, app: &mut LoomApp, sun: u64, open: &mut bool) {
    let key = egui::Id::new(("geographic_location_pick", sun));
    let current = app.state.scene.node(NodeId(sun)).and_then(|n| n.object.light()).and_then(|l| l.sun.as_ref()).map(|s| s.location.clone()).unwrap_or_default();
    let mut pick: String = ctx.data(|d| d.get_temp(key)).unwrap_or(current);
    let mut chosen = None;
    let mut cancel = false;
    egui::Window::new("Geographic Location").open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.label("City:");
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            for (name, ..) in loom_scene::sun::CITIES {
                let r = ui.selectable_label(pick == name, name);
                if r.clicked() {
                    pick = name.to_owned();
                }
                if r.double_clicked() {
                    chosen = Some(name.to_owned());
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() && !pick.is_empty() {
                chosen = Some(pick.clone());
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
    });
    if let Some(city) = chosen {
        let _ = app.dispatch(Command::SunlightParameters { id: sun, location: Some(city), latitude: None, longitude: None, time_zone: None, year: None, month: None, day: None, hour: None, minute: None, dst: None, north: None, orbital_scale: None });
        *open = false;
    }
    if cancel {
        *open = false;
    }
    if *open {
        ctx.data_mut(|d| d.insert_temp(key, pick));
    } else {
        ctx.data_mut(|d| d.remove::<String>(key));
    }
}
