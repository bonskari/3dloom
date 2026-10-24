//! The floating VertexPaint dialog: it opens while a VertexPaint entry is
//! active in the Modify panel. Paint, Pick Color From Object, the colour
//! swatch, brush Opacity and Size, Face Selection and Paint All, and the
//! Layer group (New Layer, layer Opacity, blend Mode). Every control
//! dispatches a command.

use crate::command_panel::Tab;
use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;
use loom_scene::modifier::VERTEX_PAINT_MODES;
use loom_scene::Modifier;

fn hidden_id() -> egui::Id {
    egui::Id::new("vertex_paint_hidden")
}

/// The dialog's close box hides it until the rollout's Edit button.
pub fn set_hidden(ctx: &egui::Context, hidden: bool) {
    ctx.data_mut(|d| d.insert_temp(hidden_id(), hidden));
}

fn run(app: &mut LoomApp, cmd: Command) {
    if let Err(e) = app.dispatch(cmd) {
        app.state.status_prompt = e;
    }
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp) {
    if app.panel_tab != Tab::Modify || ctx.data(|d| d.get_temp::<bool>(hidden_id())).unwrap_or(false) {
        return;
    }
    let Some((id, index)) = loom_commands::vertex_paint::entry(&app.state) else { return };
    let Some(Modifier::VertexPaint { brush_size, opacity, color, layer_opacity, mode, faces, .. }) = app.state.scene.node(id).map(|n| n.modifiers[index].modifier.clone()) else { return };
    let i = index as u32;
    let set = |app: &mut LoomApp, name: &str, value: f64| run(app, Command::SetModifierParameter { index: i, name: name.into(), value });
    let mut open = true;
    egui::Window::new("VertexPaint").id(egui::Id::new("VertexPaint")).open(&mut open).resizable(false).collapsible(false).default_pos(egui::pos2(300.0, 120.0)).show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.selectable_label(app.state.vertex_paint_brush, "Paint").on_hover_text("Paint vertex colours in the viewports").clicked() {
                run(app, Command::VertexPaintBrushToggle);
            }
            if ui.selectable_label(app.state.vertex_paint_pick, "Pick Color").on_hover_text("Pick Color From Object: drag over the object").clicked() {
                run(app, Command::VertexPaintPickColorToggle);
            }
            let mut rgb = [color[0].get(), color[1].get(), color[2].get()];
            if ui.color_edit_button_rgb(&mut rgb).changed() {
                for (k, n) in ["Color R", "Color G", "Color B"].iter().enumerate() {
                    set(app, n, rgb[k] as f64);
                }
            }
        });
        egui::Grid::new("vertex_paint_brush").num_columns(2).show(ui, |ui| {
            ui.label("Opacity:");
            let mut o = opacity.get() * 100.0;
            if ui.add(egui::DragValue::new(&mut o).range(0.0..=100.0).suffix(" %")).changed() {
                set(app, "Opacity", o as f64 / 100.0);
            }
            ui.end_row();
            ui.label("Size:");
            let mut s = brush_size.get();
            if ui.add(egui::DragValue::new(&mut s).speed(0.1).range(0.01..=1.0e6)).changed() {
                set(app, "Brush Size", s as f64);
            }
            ui.end_row();
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.selectable_label(app.state.vertex_paint_faces, "Face Selection").on_hover_text("Select the faces painting is limited to").clicked() {
                run(app, Command::VertexPaintFaceSelectToggle);
            }
            if ui.button("Paint All").on_hover_text("Fill the selected faces (every face when none is selected)").clicked() {
                run(app, Command::VertexPaintPaintAll);
            }
        });
        ui.label(format!("{} Faces Selected", faces.len()));
        ui.separator();
        ui.label("Layer");
        if ui.button("New Layer").on_hover_text("Add another VertexPaint above this one").clicked() {
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("vertex_paint_new_layer"), ("VertexPaint".to_owned(), 0u32)));
        }
        ui.horizontal(|ui| {
            ui.label("Opacity:");
            let mut o = layer_opacity.get() * 100.0;
            if ui.add(egui::Slider::new(&mut o, 0.0..=100.0).suffix(" %")).changed() {
                set(app, "Layer Opacity", o as f64 / 100.0);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Mode:");
            let cur = mode.get() as usize;
            egui::ComboBox::from_id_salt("vertex_paint_mode").selected_text(VERTEX_PAINT_MODES.get(cur).copied().unwrap_or("Normal")).show_ui(ui, |ui| {
                for (k, m) in VERTEX_PAINT_MODES.iter().enumerate() {
                    if ui.selectable_label(k == cur, *m).clicked() {
                        set(app, "Mode", k as f64);
                    }
                }
            });
        });
    });
    if !open {
        set_hidden(ctx, true);
    }
    new_layer(ctx, app);
}

/// Layer > New Layer: the new layer's name and blend mode, OK / Cancel.
fn new_layer(ctx: &egui::Context, app: &mut LoomApp) {
    let key = egui::Id::new("vertex_paint_new_layer");
    let Some((mut name, mut mode)) = ctx.data(|d| d.get_temp::<(String, u32)>(key)) else { return };
    let mut done = None;
    egui::Window::new("New Layer").id(key).resizable(false).collapsible(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.text_edit_singleline(&mut name);
        });
        ui.horizontal(|ui| {
            ui.label("Mode:");
            egui::ComboBox::from_id_salt("vertex_paint_new_layer_mode").selected_text(VERTEX_PAINT_MODES[mode as usize]).show_ui(ui, |ui| {
                for (k, m) in VERTEX_PAINT_MODES.iter().enumerate() {
                    ui.selectable_value(&mut mode, k as u32, *m);
                }
            });
        });
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
        Some(ok) => {
            ctx.data_mut(|d| d.remove::<(String, u32)>(key));
            if ok {
                run(app, Command::VertexPaintNewLayer { name, mode });
            }
        }
        None => ctx.data_mut(|d| d.insert_temp(key, (name, mode))),
    }
}
