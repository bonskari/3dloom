//! Biped rollouts: Create Biped (Create panel) and the Motion panel's Biped
//! rollout (Figure Mode) with its Structure rollout. Every edit dispatches a
//! command; the Create Biped values for the next biped are UI state.

use super::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;
use loom_scene::biped::BipedStructure;
use loom_scene::NodeId;

fn run(app: &mut LoomApp, cmd: Command) {
    if let Err(e) = app.dispatch(cmd) {
        app.state.status_prompt = e;
    }
}

/// The Structure controls; returns the parameter the user changed.
fn structure_grid(ui: &mut egui::Ui, salt: &str, s: &BipedStructure, height: bool) -> Option<(&'static str, f64)> {
    let mut out = None;
    egui::Grid::new(salt).num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        if height {
            ui.label("Height:");
            let mut h = s.height.get();
            if ui.add(crate::spinner::spin(ui, &mut h, loom_scene::biped::MIN_HEIGHT..=f32::MAX).speed(0.1)).changed() {
                out = Some(("Height", h as f64));
            }
            ui.end_row();
        }
        ui.label("Arms:");
        let mut arms = s.arms.get();
        if ui.checkbox(&mut arms, "").changed() {
            out = Some(("Arms", arms as u32 as f64));
        }
        ui.end_row();
        for (name, lo, hi) in BipedStructure::LINKS {
            ui.label(format!("{name}:"));
            let mut v = s.count(name).unwrap_or(lo);
            if ui.add(crate::spinner::spin(ui, &mut v, lo..=hi).speed(0.05)).changed() {
                out = Some((name, v as f64));
            }
            ui.end_row();
        }
    });
    out
}

fn biped_of(app: &LoomApp, id: NodeId) -> Option<(NodeId, BipedStructure)> {
    let root = loom_commands::biped::root_of(&app.state, id)?;
    let s = loom_commands::biped::structure(&app.state, root)?.clone();
    Some((root, s))
}

/// Create > Systems > Biped: Root Name and the structure of the next biped;
/// the biped just made stays editable here while it is selected.
pub fn create_rollout(ui: &mut egui::Ui, app: &mut LoomApp) {
    rollout(ui, "Create Biped", true, |ui| {
        ui.label("Drag in a viewport from the feet up to set the height.");
        let selected = app.state.selection.iter().next().copied();
        let current = app.last_biped.filter(|r| selected == Some(*r)).and_then(|r| biped_of(app, r));
        if let Some((root, s)) = &current {
            let mut name = app.state.scene.node(*root).map(|n| n.name.clone()).unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label("Root Name:");
                let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(120.0));
                if r.lost_focus() {
                    run(app, Command::BipedRootName { id: root.0, name: name.clone() });
                }
            });
            if let Some((name, value)) = structure_grid(ui, "create_biped_current", s, true) {
                let _ = app.biped_defaults.set(name, value);
                run(app, Command::SetParameter { id: root.0, name: name.into(), value });
            }
        } else {
            let d = app.biped_defaults.clone();
            if let Some((name, value)) = structure_grid(ui, "create_biped_defaults", &d, false) {
                let _ = app.biped_defaults.set(name, value);
            }
        }
    });
}

/// Motion panel: the Biped rollout (Figure Mode) and, in Figure Mode, Structure.
pub fn motion_rollouts(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId) {
    let Some((root, s)) = biped_of(app, id) else { return };
    rollout(ui, "Biped", true, |ui| {
        let label = if s.figure_mode { "Figure Mode (on)" } else { "Figure Mode" };
        if ui.add(egui::Button::new(label).selected(s.figure_mode)).clicked() {
            run(app, Command::BipedFigureMode { on: Some(!s.figure_mode) });
        }
    });
    rollout(ui, "Structure", true, |ui| {
        ui.add_enabled_ui(s.figure_mode, |ui| {
            if let Some((name, value)) = structure_grid(ui, "motion_biped_structure", &s, true) {
                run(app, Command::SetParameter { id: root.0, name: name.into(), value });
            }
        });
        if !s.figure_mode {
            ui.label("Turn on Figure Mode to edit the structure.");
        }
    });
}
