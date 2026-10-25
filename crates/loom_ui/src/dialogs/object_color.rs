//! Object Color dialog: opened by the object colour swatch (Name and Color,
//! Object Properties). Basic and custom colour swatches, Add Custom Colors,
//! Assign Random Colors, Select By Color, the current colour, OK / Cancel.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

/// A colour swatch button (named `label` for screen readers and tests).
fn swatch_named(ui: &mut egui::Ui, rgb: [u8; 3], size: egui::Vec2, selected: bool, label: &str) -> egui::Response {
    let fill = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
    let stroke = if selected { egui::Stroke::new(2.0_f32, egui::Color32::WHITE) } else { egui::Stroke::new(1.0_f32, egui::Color32::from_gray(40)) };
    let r = ui.add(egui::Button::new("").fill(fill).stroke(stroke).min_size(size));
    let label = label.to_owned();
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label.clone()));
    r
}

/// The object colour swatch beside an object's name: a click opens the
/// Object Color dialog.
pub fn swatch(ui: &mut egui::Ui, rgb: [u8; 3]) -> egui::Response {
    swatch_named(ui, rgb, egui::vec2(28.0, 18.0), false, "Object Color")
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let pending_id = egui::Id::new("object_color_pending");
    let start = app.state.selection.iter().next().and_then(|id| app.state.scene.node(*id)).map(|n| n.wire_color).unwrap_or(app.state.display.current_color);
    let mut pending: [u8; 3] = ctx.data(|d| d.get_temp(pending_id)).unwrap_or(start);
    let add_id = egui::Id::new("object_color_add");
    let mut adding: Option<[u8; 3]> = ctx.data(|d| d.get_temp(add_id)).unwrap_or(None);
    let mut close = false;
    egui::Window::new("Object Color").open(open).resizable(false).collapsible(false).show(ctx, |ui| {
        ui.label("Basic Colors:");
        egui::Grid::new("object_color_basic").spacing([3.0, 3.0]).show(ui, |ui| {
            for (i, c) in loom_scene::OBJECT_COLORS.iter().enumerate() {
                if swatch_named(ui, *c, egui::vec2(22.0, 16.0), *c == pending, &format!("Basic Color {}", i + 1)).clicked() {
                    pending = *c;
                }
                if i % 8 == 7 {
                    ui.end_row();
                }
            }
        });
        ui.add_space(4.0);
        ui.label("Custom Colors:");
        let custom = app.state.display.custom_colors.clone();
        egui::Grid::new("object_color_custom").spacing([3.0, 3.0]).show(ui, |ui| {
            for (i, c) in custom.iter().enumerate() {
                if swatch_named(ui, *c, egui::vec2(22.0, 16.0), *c == pending, &format!("Custom Color {}", i + 1)).clicked() {
                    pending = *c;
                }
                if i % 8 == 7 {
                    ui.end_row();
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Add Custom Colors...").clicked() {
                adding = Some(pending);
            }
            if let Some(c) = adding.as_mut() {
                ui.color_edit_button_srgb(c);
                if ui.button("Add Color").clicked() {
                    // Into the first slot not holding this colour yet, after the used ones.
                    let slot = custom.iter().position(|s| *s == [255, 255, 255]).unwrap_or(0);
                    let _ = app.dispatch(Command::SetCustomColor { index: slot as u32, rgb: *c });
                    pending = *c;
                    adding = None;
                }
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            let mut random = app.state.display.assign_random_colors;
            if ui.checkbox(&mut random, "Assign Random Colors").changed() {
                let _ = app.dispatch(Command::SetAssignRandomColors { on: random });
            }
            if ui.button("Select By Color").clicked() {
                let _ = app.dispatch(Command::SelectByColor { rgb: pending });
            }
        });
        ui.horizontal(|ui| {
            ui.label("Current Color:");
            swatch_named(ui, pending, egui::vec2(48.0, 20.0), false, "Current Color");
        });
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                let _ = app.dispatch(Command::SetObjectColor { rgb: pending });
                close = true;
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
        });
    });
    if close {
        *open = false;
    }
    ctx.data_mut(|d| {
        if *open {
            d.insert_temp(pending_id, pending);
            d.insert_temp(add_id, adding);
        } else {
            d.remove::<[u8; 3]>(pending_id);
            d.remove::<Option<[u8; 3]>>(add_id);
        }
    });
}
