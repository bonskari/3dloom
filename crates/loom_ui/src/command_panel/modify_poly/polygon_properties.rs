//! Editable Poly > Polygon Properties rollout (Polygon and Element
//! levels): Material (Set ID / Select ID / Clear Selection), Smoothing
//! Groups (32 buttons, Select By SG, Clear All, Auto Smooth + threshold),
//! Edit Vertex Colors.

use crate::command_panel::rollout;
use crate::shortcuts::HoverShortcut;
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, SelectionMode};

#[derive(Clone)]
struct St {
    mat_id: u16,
    color: [u8; 3],
    illum: [u8; 3],
    alpha: f32,
}

impl Default for St {
    fn default() -> Self {
        Self { mat_id: 1, color: [255, 255, 255], illum: [0, 0, 0], alpha: 100.0 }
    }
}

/// Smoothing groups the selected polygons use: (any, all) bits.
fn selected_groups(app: &LoomApp) -> (u32, u32) {
    let mut any = 0u32;
    let mut all = u32::MAX;
    let mut seen = false;
    for (n, ids) in app.state.sub_selection.by_node() {
        let Some(node) = app.state.scene.node(n) else { continue };
        let mesh = node.cage_mesh();
        for id in ids {
            if let loom_commands::SubObjectId::Face(f) = id {
                if let Some(face) = mesh.faces().get(f.0 as usize) {
                    any |= face.smoothing_groups;
                    all &= face.smoothing_groups;
                    seen = true;
                }
            }
        }
    }
    if seen { (any, all) } else { (0, 0) }
}

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    if !app.state.selection_mode.is_face_level() {
        return;
    }
    let id = egui::Id::new("polygon_properties");
    let mut st: St = ui.ctx().data(|d| d.get_temp(id)).unwrap_or_default();
    let has = !app.state.sub_selection.is_empty();
    rollout(ui, "Polygon: Material IDs", true, |ui| {
        ui.horizontal(|ui| {
            if ui.add_enabled(has, egui::Button::new("Set ID:")).clicked() {
                let _ = app.dispatch(Command::SetMaterialId { id: st.mat_id });
            }
            ui.add(crate::spinner::spin(ui, &mut st.mat_id, 1..=65535).speed(0.1));
        });
        ui.horizontal(|ui| {
            if ui.button("Select ID").on_hover_text("Select the polygons with this material id (Ctrl adds)").clicked() {
                let add = ui.input(|i| i.modifiers.ctrl);
                let _ = app.dispatch(Command::SelectByMaterialId { id: st.mat_id, clear: !add });
            }
            if ui.add_enabled(has, egui::Button::new("Clear Selection")).with_shortcut(&app.state, Command::SelectNone.name()).clicked() {
                let _ = app.dispatch(Command::SelectNone);
            }
        });
    });
    rollout(ui, "Polygon: Smoothing Groups", true, |ui| {
        let (any, all) = selected_groups(app);
        // Narrow columns (the default minimum is the 40 px interact width): eight fit the panel.
        egui::Grid::new(id.with("sg")).num_columns(8).min_col_width(22.0).spacing(egui::vec2(2.0, 2.0)).show(ui, |ui| {
            for g in 0..32u32 {
                let bit = 1u32 << g;
                let on = all & bit != 0;
                let partial = !on && any & bit != 0;
                let text = if partial { egui::RichText::new(format!("{}", g + 1)).italics() } else { egui::RichText::new(format!("{}", g + 1)) };
                if ui.add_enabled(has, egui::SelectableLabel::new(on, text)).on_hover_text(format!("Smoothing group {} (Alt-click selects polygons with it)", g + 1)).clicked() {
                    if ui.input(|i| i.modifiers.alt) {
                        let _ = app.dispatch(Command::SelectBySmoothingGroup { bits: bit, clear: true });
                    } else if on {
                        // Remove the group: keep the other bits of every selected polygon.
                        let _ = app.dispatch(Command::SetSmoothingGroups { bits: any & !bit, add: false });
                    } else {
                        let _ = app.dispatch(Command::SetSmoothingGroups { bits: bit, add: true });
                    }
                }
                if g % 8 == 7 {
                    ui.end_row();
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(any != 0, egui::Button::new("Select By SG")).on_hover_text("Select every polygon that shares a group with the selection").clicked() {
                let _ = app.dispatch(Command::SelectBySmoothingGroup { bits: any, clear: true });
            }
            if ui.add_enabled(has, egui::Button::new("Clear All")).clicked() {
                let _ = app.dispatch(Command::ClearSmoothingGroups);
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Auto Smooth").on_hover_text("Assign smoothing groups by the angle between polygons").clicked() {
                let _ = app.dispatch(Command::AutoSmooth);
            }
            let mut t = app.state.poly_settings.auto_smooth_threshold;
            if ui.add(crate::spinner::spin(ui, &mut t, 0.0..=180.0).speed(0.5).suffix("°")).changed() {
                let _ = app.dispatch(Command::AutoSmoothThreshold { degrees: t });
            }
        });
    });
    rollout(ui, "Polygon: Vertex Colors", false, |ui| {
        egui::Grid::new(id.with("colors")).num_columns(2).show(ui, |ui| {
            ui.label("Color:");
            let mut c = st.color;
            if ui.add_enabled(has, egui::Button::new("").fill(egui::Color32::from_rgb(c[0], c[1], c[2])).min_size(egui::vec2(40.0, 16.0))).clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(id.with("pick"), 0u8));
            }
            ui.end_row();
            ui.label("Illumination:");
            let mut il = st.illum;
            if ui.add_enabled(has, egui::Button::new("").fill(egui::Color32::from_rgb(il[0], il[1], il[2])).min_size(egui::vec2(40.0, 16.0))).clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(id.with("pick"), 1u8));
            }
            ui.end_row();
            ui.label("Alpha:");
            if ui.add_enabled(has, crate::spinner::spin(ui, &mut st.alpha, 0.0..=100.0).speed(1.0).suffix(" %")).changed() {
                let _ = app.dispatch(Command::SetVertexAlpha { alpha: st.alpha });
            }
            ui.end_row();
            let pick: Option<u8> = ui.ctx().data(|d| d.get_temp(id.with("pick")));
            if let Some(which) = pick {
                let mut keep = true;
                let target = if which == 0 { &mut c } else { &mut il };
                egui::Window::new(if which == 0 { "Polygon Vertex Color" } else { "Polygon Vertex Illumination" }).id(id.with("picker")).collapsible(false).resizable(false).open(&mut keep).show(ui.ctx(), |ui| {
                    if ui.color_edit_button_srgb(target).changed() {
                        let rgb = *target;
                        let _ = app.dispatch(if which == 0 { Command::SetVertexColor { rgb } } else { Command::SetVertexIllumination { rgb } });
                    }
                    if ui.button("Close").clicked() {
                        ui.ctx().data_mut(|d| d.insert_temp(id.with("close"), true));
                    }
                });
                if !keep || ui.ctx().data(|d| d.get_temp::<bool>(id.with("close"))).unwrap_or(false) {
                    ui.ctx().data_mut(|d| d.remove::<bool>(id.with("close")));
                    ui.ctx().data_mut(|d| d.remove::<u8>(id.with("pick")));
                }
            }
            st.color = c;
            st.illum = il;
        });
    });
    let _ = SelectionMode::Face;
    ui.ctx().data_mut(|d| d.insert_temp(id, st));
}
