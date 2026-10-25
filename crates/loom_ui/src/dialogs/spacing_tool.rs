//! Spacing Tool dialog (Shift+I): Pick Path / Pick Points, Count / Spacing /
//! Offsets (with locks), Distribution, Context, Follow, Type of Object, Apply.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{CloneKind, Command, PickMode, SpacingContext};

#[derive(Clone, Copy)]
struct Settings {
    count: u32,
    spacing: f32,
    start_offset: f32,
    end_offset: f32,
    context: SpacingContext,
    follow: bool,
    kind: CloneKind,
    /// Distribution drop-down (loom_commands::spacing::DISTRIBUTIONS).
    distribution: u32,
    /// The lock buttons: the offset follows the spacing.
    lock_start: bool,
    lock_end: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { count: 3, spacing: 10.0, start_offset: 0.0, end_offset: 0.0, context: SpacingContext::Centers, follow: false, kind: CloneKind::Copy, distribution: loom_commands::spacing::DEFAULT_DISTRIBUTION, lock_start: false, lock_end: false }
    }
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("spacing_tool_dialog");
    let mut st: Settings = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
    let speed = app.spinner_speed(0.1);
    let picks = app.state.spacing_points;
    let picking = matches!(app.state.pick_mode, Some(PickMode::SpacingPointA | PickMode::SpacingPointB));
    let picking_path = app.state.pick_mode == Some(PickMode::SpacingPath);
    let path = app.state.spacing_path.filter(|p| app.state.scene.node(*p).is_some());
    let uses = loom_commands::spacing::uses(st.distribution);
    let mut apply = false;
    let mut close = false;
    egui::Window::new("Spacing Tool").open(open).resizable(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.selectable_label(picking_path, "Pick Path").clicked() {
                let _ = app.dispatch(Command::SpacingPickPath);
            }
            if ui.selectable_label(picking, "Pick Points").clicked() {
                let _ = app.dispatch(Command::SpacingPickPoints);
            }
            let status = match (path, picks[0], picks[1]) {
                (Some(p), _, _) => app.state.scene.node(p).map(|n| n.name.clone()).unwrap_or_default(),
                (None, Some(a), Some(b)) => format!("{:.1} units", a.distance(b)),
                (None, Some(_), None) => "pick the second point".to_owned(),
                _ if picking_path => "pick a shape".to_owned(),
                _ => if picking { "pick the first point".to_owned() } else { "no path".to_owned() },
            };
            ui.label(status);
        });
        ui.label(egui::RichText::new("Parameters").strong());
        egui::Grid::new("spacing_params").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("Count:");
            ui.add_enabled(uses[0], crate::spinner::spin(ui, &mut st.count, 1..=10_000).speed(0.1));
            ui.end_row();
            ui.label("Spacing:");
            ui.add_enabled(uses[1], crate::spinner::spin(ui, &mut st.spacing, ..).speed(speed).fixed_decimals(3));
            ui.end_row();
            ui.label("Start Offset:");
            ui.horizontal(|ui| {
                ui.add_enabled(uses[2] && !st.lock_start, crate::spinner::spin(ui, &mut st.start_offset, ..).speed(speed).fixed_decimals(3));
                ui.add_enabled(uses[2], egui::SelectableLabel::new(st.lock_start, "Lock")).on_hover_text("Lock the start offset to the spacing").clicked().then(|| st.lock_start = !st.lock_start);
            });
            ui.end_row();
            ui.label("End Offset:");
            ui.horizontal(|ui| {
                ui.add_enabled(uses[3] && !st.lock_end, crate::spinner::spin(ui, &mut st.end_offset, ..).speed(speed).fixed_decimals(3));
                ui.add_enabled(uses[3], egui::SelectableLabel::new(st.lock_end, "Lock ")).on_hover_text("Lock the end offset to the spacing").clicked().then(|| st.lock_end = !st.lock_end);
            });
            ui.end_row();
        });
        egui::ComboBox::from_id_salt("spacing_distribution").width(260.0).selected_text(loom_commands::spacing::DISTRIBUTIONS[st.distribution as usize % loom_commands::spacing::DISTRIBUTIONS.len()]).show_ui(ui, |ui| {
            for (i, name) in loom_commands::spacing::DISTRIBUTIONS.iter().enumerate() {
                ui.selectable_value(&mut st.distribution, i as u32, *name);
            }
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
            let ready = (path.is_some() || (picks[0].is_some() && picks[1].is_some())) && !app.state.selection.is_empty();
            if ui.add_enabled(ready, egui::Button::new("Apply")).clicked() {
                apply = true;
            }
            if ui.button("Close").clicked() {
                close = true;
            }
        });
    });
    if st.lock_start {
        st.start_offset = st.spacing;
    }
    if st.lock_end {
        st.end_offset = st.spacing;
    }
    if apply {
        let (a, b) = (picks[0].unwrap_or_default(), picks[1].unwrap_or_default());
        let r = app.dispatch(Command::SpaceAlongPath { path: path.map(|p| p.0), start: a.to_array(), end: b.to_array(), distribution: st.distribution, count: st.count, spacing: st.spacing, start_offset: st.start_offset, end_offset: st.end_offset, context: st.context, follow: st.follow, kind: st.kind });
        if let Err(e) = r {
            app.state.status_prompt = e;
        }
    }
    if close || !*open {
        *open = false;
        if picking || picking_path {
            let _ = app.dispatch(Command::ExitPickMode);
        }
    }
    ctx.data_mut(|d| d.insert_temp(id, st));
}
