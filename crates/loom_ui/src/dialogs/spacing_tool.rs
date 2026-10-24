//! Spacing Tool dialog (Shift+I): Pick Points, Count / Spacing / Offsets,
//! Context, Follow, Type of Object, Apply.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{CloneKind, Command, PickMode, SpacingContext};

#[derive(Clone, Copy)]
struct Settings {
    count: u32,
    use_spacing: bool,
    spacing: f32,
    start_offset: f32,
    end_offset: f32,
    context: SpacingContext,
    follow: bool,
    kind: CloneKind,
}

impl Default for Settings {
    fn default() -> Self {
        Self { count: 2, use_spacing: false, spacing: 10.0, start_offset: 0.0, end_offset: 0.0, context: SpacingContext::Centers, follow: false, kind: CloneKind::Copy }
    }
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("spacing_tool_dialog");
    let mut st: Settings = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
    let speed = app.spinner_speed(0.1);
    let picks = app.state.spacing_points;
    let picking = matches!(app.state.pick_mode, Some(PickMode::SpacingPointA | PickMode::SpacingPointB));
    let mut apply = false;
    let mut close = false;
    egui::Window::new("Spacing Tool").open(open).resizable(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.add_enabled(false, egui::Button::new("Pick Path")).on_disabled_hover_text("Spline paths arrive with shapes");
            if ui.selectable_label(picking, "Pick Points").clicked() {
                let _ = app.dispatch(Command::SpacingPickPoints);
            }
            let status = match (picks[0], picks[1]) {
                (Some(a), Some(b)) => format!("{:.1} units", a.distance(b)),
                (Some(_), None) => "pick the second point".to_owned(),
                _ => if picking { "pick the first point".to_owned() } else { "no path".to_owned() },
            };
            ui.label(status);
        });
        ui.label(egui::RichText::new("Parameters").strong());
        egui::Grid::new("spacing_params").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("Count:");
            ui.add(egui::DragValue::new(&mut st.count).speed(0.1).range(1..=10_000));
            ui.end_row();
            ui.checkbox(&mut st.use_spacing, "Spacing:");
            ui.add_enabled(st.use_spacing, egui::DragValue::new(&mut st.spacing).speed(speed).fixed_decimals(3));
            ui.end_row();
            ui.label("Start Offset:");
            ui.add(egui::DragValue::new(&mut st.start_offset).speed(speed).fixed_decimals(3));
            ui.end_row();
            ui.label("End Offset:");
            ui.add(egui::DragValue::new(&mut st.end_offset).speed(speed).fixed_decimals(3));
            ui.end_row();
        });
        ui.horizontal(|ui| {
            ui.label("Context:");
            ui.radio_value(&mut st.context, SpacingContext::Edges, "Edges");
            ui.radio_value(&mut st.context, SpacingContext::Centers, "Centers");
            ui.checkbox(&mut st.follow, "Follow");
        });
        ui.horizontal(|ui| {
            ui.label("Type of Object:");
            for (k, l) in [(CloneKind::Copy, "Copy"), (CloneKind::Instance, "Instance"), (CloneKind::Reference, "Reference")] {
                ui.radio_value(&mut st.kind, k, l);
            }
        });
        ui.horizontal(|ui| {
            let ready = picks[0].is_some() && picks[1].is_some() && !app.state.selection.is_empty();
            if ui.add_enabled(ready, egui::Button::new("Apply")).clicked() {
                apply = true;
            }
            if ui.button("Close").clicked() {
                close = true;
            }
        });
    });
    if apply {
        if let (Some(a), Some(b)) = (picks[0], picks[1]) {
            let _ = app.dispatch(Command::SpaceObjects { start: a.to_array(), end: b.to_array(), count: st.count, spacing: st.use_spacing.then_some(st.spacing), start_offset: st.start_offset, end_offset: st.end_offset, context: st.context, follow: st.follow, kind: st.kind });
        }
    }
    if close || !*open {
        *open = false;
        if picking {
            let _ = app.dispatch(Command::ExitPickMode);
        }
    }
    ctx.data_mut(|d| d.insert_temp(id, st));
}
