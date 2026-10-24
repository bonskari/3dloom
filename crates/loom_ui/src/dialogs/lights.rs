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
    let r = ui.add(crate::spinner::spin(ui, &mut v, ..).speed(speed).fixed_decimals(2));
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
}

/// Light Lister: every light (or the selected ones) with its main settings
/// in one table; a name button selects the light.
pub fn light_lister(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let key = egui::Id::new("light_lister_state");
    let mut st: ListerState = ctx.data(|d| d.get_temp(key)).unwrap_or_default();
    egui::Window::new("Light Lister").open(open).resizable(true).default_width(760.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Configuration:");
            ui.radio_value(&mut st.selected_only, false, "All Lights");
            ui.radio_value(&mut st.selected_only, true, "Selected Lights");
        });
        ui.separator();
        let ids: Vec<NodeId> = loom_commands::light::lights(&app.state.scene).into_iter().filter(|id| !st.selected_only || app.state.selection.contains(id)).collect();
        if ids.is_empty() {
            ui.label("No lights.");
            return;
        }
        egui::ScrollArea::both().show(ui, |ui| {
            egui::Grid::new("light_lister").striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
                for h in ["On", "Name", "Multiplier", "Color", "Shadows", "", "Map Size", "Bias", "Sm.Range", "Decay", "Start"] {
                    ui.label(egui::RichText::new(h).strong());
                }
                ui.end_row();
                for id in ids {
                    let Some(n) = app.state.scene.node(id) else { continue };
                    let name = n.name.clone();
                    let Some(l) = n.object.light().cloned() else { continue };
                    lister_row(ui, app, id, &name, &l);
                    ui.end_row();
                }
            });
        });
    });
    ctx.data_mut(|d| d.insert_temp(key, st));
}

fn lister_row(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, name: &str, l: &LightObject) {
    let mut on = l.on.get();
    if ui.checkbox(&mut on, "").changed() {
        set(app, id, "On", on as u32 as f64);
    }
    if ui.selectable_label(app.state.selection.contains(&id), name).clicked() {
        let _ = app.dispatch(Command::SelectNodes { ids: vec![id.0], op: loom_commands::SelectOp::Replace });
    }
    spinner(ui, app, id, "Multiplier", l.multiplier.get(), 0.01);
    let c = l.color.get();
    let mut rgb = [0, 1, 2].map(|i| (c[i].clamp(0.0, 1.0) * 255.0).round() as u8);
    if ui.color_edit_button_srgb(&mut rgb).changed() {
        app.state.undo.begin("Color");
        for (ch, v) in ["R", "G", "B"].iter().zip(rgb) {
            set(app, id, &format!("Color {ch}"), v as f64 / 255.0);
        }
        app.state.undo.end();
    }
    let shadows_name = if l.light_type == LightType::Skylight { "Cast Shadows" } else { "Shadows On" };
    let mut sh = l.shadows_on.get();
    if ui.checkbox(&mut sh, "").changed() {
        set(app, id, shadows_name, sh as u32 as f64);
    }
    if l.light_type == LightType::Skylight {
        for _ in 0..6 {
            ui.label("");
        }
        return;
    }
    let st = l.shadow_type.get();
    egui::ComboBox::from_id_salt(("lister_shadow_type", id.0)).selected_text(SHADOW_TYPES[st.min(1) as usize]).show_ui(ui, |ui| {
        for (i, t) in SHADOW_TYPES.iter().enumerate() {
            if ui.selectable_label(st == i as u32, *t).clicked() && st != i as u32 {
                set(app, id, "Shadow Type", i as f64);
            }
        }
    });
    let mut size = l.map_size.get();
    let r = ui.add(crate::spinner::spin(ui, &mut size, 1..=10_000).speed(4.0));
    if r.drag_started() {
        app.state.undo.begin("Map Size");
    }
    if r.changed() && size != l.map_size.get() {
        set(app, id, "Map Size", size as f64);
    }
    if r.drag_stopped() {
        app.state.undo.end();
    }
    spinner(ui, app, id, "Map Bias", l.map_bias.get(), 0.01);
    ui.add_enabled_ui(loom_bevy::lights::supports("Sample Range"), |ui| spinner(ui, app, id, "Sample Range", l.sample_range.get(), 0.05));
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
