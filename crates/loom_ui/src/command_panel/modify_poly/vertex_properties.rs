//! Editable Poly > Vertex Properties rollout: Edit Vertex Colors
//! (Color, Illumination, Alpha) and Select Vertices By colour / illumination.

use crate::command_panel::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, SelectionMode};

#[derive(Clone)]
struct St {
    color: [u8; 3],
    illum: [u8; 3],
    alpha: f32,
    by_illum: bool,
    range: [u8; 3],
}

impl Default for St {
    fn default() -> Self {
        Self { color: [255, 255, 255], illum: [0, 0, 0], alpha: 100.0, by_illum: false, range: [10, 10, 10] }
    }
}

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    if app.state.selection_mode != SelectionMode::Vertex {
        return;
    }
    let id = egui::Id::new("vertex_properties");
    let mut st: St = ui.ctx().data(|d| d.get_temp(id)).unwrap_or_default();
    rollout(ui, "Vertex Properties", true, |ui| {
        let has = !app.state.sub_selection.is_empty();
        ui.label(egui::RichText::new("Edit Vertex Colors").strong());
        egui::Grid::new(id.with("colors")).num_columns(2).show(ui, |ui| {
            ui.label("Color:");
            if ui.add_enabled(has, egui::Button::new("").fill(egui::Color32::from_rgb(st.color[0], st.color[1], st.color[2])).min_size(egui::vec2(40.0, 16.0))).clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(id.with("pick"), 0u8));
            }
            ui.end_row();
            ui.label("Illumination:");
            if ui.add_enabled(has, egui::Button::new("").fill(egui::Color32::from_rgb(st.illum[0], st.illum[1], st.illum[2])).min_size(egui::vec2(40.0, 16.0))).clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(id.with("pick"), 1u8));
            }
            ui.end_row();
            ui.label("Alpha:");
            let r = ui.add_enabled(has, crate::spinner::spin(ui, &mut st.alpha, 0.0..=100.0).speed(1.0).suffix(" %"));
            if r.changed() {
                let _ = app.dispatch(Command::SetVertexAlpha { alpha: st.alpha });
            }
            ui.end_row();
        });
        // Colour pickers open as small popups under the swatches.
        let pick: Option<u8> = ui.ctx().data(|d| d.get_temp(id.with("pick")));
        if let Some(which) = pick {
            let target = if which == 0 { &mut st.color } else { &mut st.illum };
            let mut rgb = *target;
            let mut keep = true;
            egui::Window::new(if which == 0 { "Vertex Color" } else { "Vertex Illumination" }).id(id.with("picker")).collapsible(false).resizable(false).open(&mut keep).show(ui.ctx(), |ui| {
                if ui.color_edit_button_srgb(&mut rgb).changed() {
                    *target = rgb;
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
        ui.label(egui::RichText::new("Select Vertices By").strong());
        ui.horizontal(|ui| {
            ui.radio_value(&mut st.by_illum, false, "Color");
            ui.radio_value(&mut st.by_illum, true, "Illumination");
        });
        ui.horizontal(|ui| {
            ui.label("Range:");
            for (i, l) in ["R", "G", "B"].iter().enumerate() {
                ui.label(*l);
                ui.add(crate::spinner::spin(ui, &mut st.range[i], 0..=255).speed(1.0));
            }
        });
        if ui.button("Select").on_hover_text("Select the vertices whose colour is within the range of the swatch").clicked() {
            let rgb = if st.by_illum { st.illum } else { st.color };
            let _ = app.dispatch(Command::SelectByVertexColor { rgb, range: st.range, illumination: st.by_illum });
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(id, st));
}
