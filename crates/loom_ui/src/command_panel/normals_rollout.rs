//! Edit Normals rollout: Select By, Ignore Backfacing, the
//! normal count, Unify / Break / Average / Specify / Make Explicit / Reset,
//! Copy / Paste Value, Display Length. Every control dispatches a command.

use super::rollout;
use crate::shortcuts::HoverShortcut;
use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

fn run(app: &mut LoomApp, cmd: Command) {
    if let Err(e) = app.dispatch(cmd) {
        app.state.status_prompt = e;
    }
}

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    let st = loom_commands::normals::state_json(&app.state);
    if st.is_null() {
        return;
    }
    let by = st["select_by"].as_u64().unwrap_or(0) as u32;
    rollout(ui, "Parameters", true, |ui| {
        ui.label("Select By:");
        ui.horizontal(|ui| {
            for (i, l) in ["Normal", "Vertex", "Edge", "Face"].iter().enumerate() {
                if ui.radio(by == i as u32, *l).clicked() {
                    run(app, Command::EditNormalsSelectBy { level: i as u32 });
                }
            }
        });
        let mut ib = st["display_length"].is_number() && app.state.scene.node(loom_scene::NodeId(st["node"].as_u64().unwrap_or(0))).is_some_and(|n| matches!(&n.modifiers[st["index"].as_u64().unwrap_or(0) as usize].modifier, loom_scene::Modifier::EditNormals(d) if d.ignore_backfacing));
        if ui.checkbox(&mut ib, "Ignore Backfacing").changed() {
            let index = st["index"].as_u64().unwrap_or(0) as u32;
            run(app, Command::SetModifierParameter { index, name: "Ignore Backfacing".into(), value: if ib { 1.0 } else { 0.0 } });
        }
        ui.label(format!("{} normals, {} selected", st["normals"], st["selected"].as_array().map(|a| a.len()).unwrap_or(0)));
        ui.separator();
        let w = (ui.available_width() - 8.0) / 2.0;
        ui.horizontal(|ui| {
            if ui.add_sized([w, 20.0], egui::Button::new("Unify")).on_hover_name(&app.state, "Unify Normals").clicked() {
                run(app, Command::UnifyNormals);
            }
            if ui.add_sized([w, 20.0], egui::Button::new("Break")).on_hover_name(&app.state, "Break Normals").clicked() {
                run(app, Command::BreakNormals);
            }
        });
        ui.horizontal(|ui| {
            if ui.add_sized([w, 20.0], egui::Button::new("Specify")).on_hover_name(&app.state, "Specify Normals").clicked() {
                run(app, Command::SpecifyNormals);
            }
            if ui.add_sized([w, 20.0], egui::Button::new("Make Explicit")).on_hover_name(&app.state, "Make Explicit").clicked() {
                run(app, Command::MakeExplicit);
            }
        });
        if ui.add_sized([ui.available_width(), 20.0], egui::Button::new("Reset")).on_hover_name(&app.state, "Reset Normals").clicked() {
            run(app, Command::ResetNormals);
        }
        ui.label("Average:");
        ui.horizontal(|ui| {
            if ui.add_sized([w, 20.0], egui::Button::new("Selected")).on_hover_text("Average Normals").clicked() {
                run(app, Command::AverageNormals);
            }
            let mut ut = st["use_threshold"].as_bool().unwrap_or(false);
            if ui.checkbox(&mut ut, "Use Threshold").changed() {
                run(app, Command::AverageMode);
            }
        });
        let index = st["index"].as_u64().unwrap_or(0) as u32;
        let mut th = st["threshold"].as_f64().unwrap_or(0.1) as f32;
        ui.horizontal(|ui| {
            ui.label("Threshold:");
            if ui.add(crate::spinner::spin(ui, &mut th, 0.0..=1.0e6).speed(0.01)).changed() {
                run(app, Command::SetModifierParameter { index, name: "Threshold".into(), value: th as f64 });
            }
        });
        ui.horizontal(|ui| {
            if ui.add_sized([w, 20.0], egui::Button::new("Copy Value")).on_hover_name(&app.state, "Copy Normal").clicked() {
                run(app, Command::CopyNormal);
            }
            if ui.add_sized([w, 20.0], egui::Button::new("Paste Value")).on_hover_name(&app.state, "Paste Normal").clicked() {
                run(app, Command::PasteNormal);
            }
        });
        let mut dl = st["display_length"].as_f64().unwrap_or(10.0) as f32;
        ui.horizontal(|ui| {
            ui.label("Display Length:");
            if ui.add(crate::spinner::spin(ui, &mut dl, 0.0..=1.0e6).speed(0.1)).changed() {
                run(app, Command::SetModifierParameter { index, name: "Display Length".into(), value: dl as f64 });
            }
        });
    });
}
