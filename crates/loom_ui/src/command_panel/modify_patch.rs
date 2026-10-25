//! Editable Patch / Edit Patch rollouts: Selection (Vertex, Handle, Edge,
//! Patch, Element), Geometry (Add Tri / Add Quad, Weld, Delete, Extrude &
//! Bevel, Flip) and Surface (View Steps, Show Interior Edges, Relax).

use crate::command_panel::modify_poly::action;
use crate::command_panel::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::mesh_edit::current_level;
use loom_commands::Command;

/// The five Editable Patch levels in order.
pub const LEVELS: [&str; 5] = ["Vertex", "Handle", "Edge", "Patch", "Element"];

/// `entry`: the active Edit Patch entry (its Surface settings are modifier parameters).
pub fn show(ui: &mut egui::Ui, app: &mut LoomApp, entry: Option<usize>) {
    selection(ui, app);
    geometry(ui, app);
    surface(ui, app, entry);
}

fn selection(ui: &mut egui::Ui, app: &mut LoomApp) {
    rollout(ui, "Selection", true, |ui| {
        let level = current_level(&app.state);
        ui.horizontal_wrapped(|ui| {
            for (i, l) in LEVELS.iter().enumerate() {
                let lvl = i as u32 + 1;
                if ui.selectable_label(level == lvl, *l).on_hover_text(format!("{l} ({lvl})")).clicked() {
                    let _ = app.dispatch(Command::SubObjectLevel { level: if level == lvl { 0 } else { lvl } });
                }
            }
        });
        if level > 0 {
            let what = ["Vertices", "Handles", "Edges", "Patches", "Patches"][level as usize - 1];
            ui.label(format!("{} {what} Selected", app.state.sub_selection.len()));
        }
    });
}

fn geometry(ui: &mut egui::Ui, app: &mut LoomApp) {
    rollout(ui, "Geometry", true, |ui| {
        let level = current_level(&app.state);
        let has = level > 0 && !app.state.sub_selection.is_empty();
        let mp = app.state.mesh_patch.clone();
        ui.horizontal(|ui| {
            ui.add_enabled_ui(has && level == 3, |ui| {
                action(ui, app, "Add Tri", "Add a tri patch on each selected open edge", Command::PatchAddTri);
                action(ui, app, "Add Quad", "Add a quad patch on each selected open edge", Command::PatchAddQuad);
            });
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(has && level == 1, |ui| action(ui, app, "Weld Selected", "Weld the selected vertices within the threshold", Command::PatchWeld));
            let mut t = mp.patch_weld_threshold;
            if ui.add(crate::spinner::spin(ui, &mut t, 0.0..=f32::MAX).speed(app.spinner_speed(0.01))).changed() {
                let _ = app.dispatch(Command::MeshPatchSetting { name: "Patch Weld Threshold".into(), value: t as f64 });
            }
        });
        ui.add_enabled_ui(has && level != 2, |ui| action(ui, app, "Delete", "Delete the patches of the selection", Command::PatchDelete));
        ui.label("Extrude & Bevel");
        ui.add_enabled_ui(matches!(level, 4 | 5), |ui| {
            ui.horizontal(|ui| {
                if ui.add_enabled(has, egui::Button::new("Extrude")).on_hover_text("Extrude the selected patches by Extrusion, then Outlining").clicked() {
                    let _ = app.dispatch(Command::PatchExtrude { amount: mp.patch_extrude, outline: mp.patch_outline });
                }
            });
            ui.horizontal(|ui| {
                ui.label("Extrusion:");
                let mut e = mp.patch_extrude;
                if ui.add(crate::spinner::spin(ui, &mut e, f32::MIN..=f32::MAX).speed(app.spinner_speed(0.1))).changed() {
                    let _ = app.dispatch(Command::MeshPatchSetting { name: "Patch Extrude".into(), value: e as f64 });
                }
            });
            ui.horizontal(|ui| {
                ui.label("Outlining:");
                let mut o = mp.patch_outline;
                if ui.add(crate::spinner::spin(ui, &mut o, f32::MIN..=f32::MAX).speed(app.spinner_speed(0.1))).changed() {
                    let _ = app.dispatch(Command::MeshPatchSetting { name: "Patch Outline".into(), value: o as f64 });
                }
            });
            ui.add_enabled_ui(has, |ui| action(ui, app, "Flip", "Flip the normals of the selected patches", Command::PatchFlipNormals));
        });
    });
}

fn surface(ui: &mut egui::Ui, app: &mut LoomApp, entry: Option<usize>) {
    let Some(id) = app.state.selection.iter().next().copied() else { return };
    let Some(obj) = app.state.scene.node(id).and_then(|n| n.edit_patch()).cloned() else { return };
    rollout(ui, "Surface", true, |ui| {
        let set = |app: &mut LoomApp, name: &str, value: f64| {
            let cmd = match entry {
                Some(i) => Command::SetModifierParameter { index: i as u32, name: name.into(), value },
                None => Command::SetParameter { id: id.0, name: name.into(), value },
            };
            let _ = app.dispatch(cmd);
        };
        ui.horizontal(|ui| {
            ui.label("View Steps:");
            let mut steps = obj.steps.get();
            if ui.add(crate::spinner::spin(ui, &mut steps, 0..=100)).changed() {
                set(app, "View Steps", steps as f64);
            }
        });
        let mut interior = obj.show_interior;
        if ui.checkbox(&mut interior, "Show Interior Edges").changed() {
            set(app, "Show Interior Edges", interior as u8 as f64);
        }
        let mp = app.state.mesh_patch.clone();
        ui.separator();
        ui.horizontal(|ui| {
            action(ui, app, "Relax", "Relax the selected vertices (all when none)", Command::PatchRelax);
        });
        ui.horizontal(|ui| {
            ui.label("Relax Value:");
            let mut v = mp.relax_amount;
            if ui.add(crate::spinner::spin(ui, &mut v, -1.0..=1.0).speed(app.spinner_speed(0.01))).changed() {
                let _ = app.dispatch(Command::MeshPatchSetting { name: "Relax Value".into(), value: v as f64 });
            }
        });
        ui.horizontal(|ui| {
            ui.label("Iterations:");
            let mut it = mp.relax_iterations;
            if ui.add(crate::spinner::spin(ui, &mut it, 0..=1000)).changed() {
                let _ = app.dispatch(Command::MeshPatchSetting { name: "Relax Iterations".into(), value: it as f64 });
            }
        });
        let mut keep = mp.relax_keep_boundary;
        if ui.checkbox(&mut keep, "Keep Boundary Points Fixed").changed() {
            let _ = app.dispatch(Command::MeshPatchSetting { name: "Keep Boundary Points Fixed".into(), value: keep as u8 as f64 });
        }
    });
}
