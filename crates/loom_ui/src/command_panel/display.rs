//! Display tab: Display Color, Hide by Category, Hide, Freeze, Display
//! Properties rollouts.

use super::rollout;
use crate::shortcuts::HoverShortcut;
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, PickMode};
use loom_scene::DisplayProps;

fn wide(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, enabled: bool, cmd: Command) {
    let w = ui.available_width();
    ui.add_enabled_ui(enabled, |ui| {
        if ui.add_sized([w, 22.0], egui::Button::new(label)).with_shortcut(&app.state, cmd.name()).clicked() {
            let _ = app.dispatch(cmd);
        }
    });
}

fn mode_button(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, mode: PickMode, cmd: Command) {
    let w = ui.available_width();
    let on = app.state.pick_mode == Some(mode);
    if ui.add_sized([w, 22.0], egui::SelectableLabel::new(on, label)).with_shortcut(&app.state, cmd.name()).clicked() {
        let _ = app.dispatch(if on { Command::ExitPickMode } else { cmd });
    }
}

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    let has = !app.state.selection.is_empty();
    rollout(ui, "Hide by Category", true, |ui| {
        let c = app.state.hide_category;
        for (label, on, cmd) in [
            ("Geometry", c.geometry, Command::HideGeometryToggle),
            ("Shapes", c.shapes, Command::HideShapesToggle),
            ("Lights", c.lights, Command::HideLightsToggle),
            ("Cameras", c.cameras, Command::HideCamerasToggle),
            ("Helpers", c.helpers, Command::HideHelpersToggle),
        ] {
            let mut v = on;
            if ui.checkbox(&mut v, label).with_shortcut(&app.state, cmd.name()).changed() {
                let _ = app.dispatch(cmd);
            }
        }
        ui.horizontal(|ui| {
            if ui.button("All").clicked() {
                for (on, cmd) in [(c.geometry, Command::HideGeometryToggle), (c.shapes, Command::HideShapesToggle), (c.lights, Command::HideLightsToggle), (c.cameras, Command::HideCamerasToggle), (c.helpers, Command::HideHelpersToggle)] {
                    if !on {
                        let _ = app.dispatch(cmd);
                    }
                }
            }
            if ui.button("None").clicked() {
                for (on, cmd) in [(c.geometry, Command::HideGeometryToggle), (c.shapes, Command::HideShapesToggle), (c.lights, Command::HideLightsToggle), (c.cameras, Command::HideCamerasToggle), (c.helpers, Command::HideHelpersToggle)] {
                    if on {
                        let _ = app.dispatch(cmd);
                    }
                }
            }
            if ui.button("Invert").clicked() {
                for cmd in [Command::HideGeometryToggle, Command::HideShapesToggle, Command::HideLightsToggle, Command::HideCamerasToggle, Command::HideHelpersToggle] {
                    let _ = app.dispatch(cmd);
                }
            }
        });
    });
    rollout(ui, "Hide", true, |ui| {
        wide(ui, app, "Hide Selected", has, Command::HideSelection);
        wide(ui, app, "Hide Unselected", true, Command::HideUnselected);
        wide(ui, app, "Hide by Name...", true, Command::HideByName);
        mode_button(ui, app, "Hide by Hit", PickMode::HideByHit, Command::HideByHit);
        wide(ui, app, "Unhide All", true, Command::UnhideAll);
        wide(ui, app, "Unhide by Name...", true, Command::UnhideByName);
    });
    rollout(ui, "Freeze", true, |ui| {
        wide(ui, app, "Freeze Selected", has, Command::FreezeSelection);
        wide(ui, app, "Freeze Unselected", true, Command::FreezeUnselected);
        wide(ui, app, "Freeze by Name...", true, Command::FreezeByName);
        mode_button(ui, app, "Freeze by Hit", PickMode::FreezeByHit, Command::FreezeByHit);
        wide(ui, app, "Unfreeze All", true, Command::UnfreezeAll);
        wide(ui, app, "Unfreeze by Name...", true, Command::UnfreezeByName);
        mode_button(ui, app, "Unfreeze by Hit", PickMode::UnfreezeByHit, Command::UnfreezeByHit);
    });
    rollout(ui, "Display Properties", true, |ui| {
        let Some(id) = app.state.selection.iter().next().copied() else {
            ui.label("Select an object.");
            return;
        };
        let Some(node) = app.state.scene.node(id) else { return };
        let props = node.display.clone();
        // Every selected object: a box shows mixed when they differ.
        let all: Vec<DisplayProps> = app.state.selection.iter().filter_map(|n| app.state.scene.node(*n)).map(|n| n.display.clone()).collect();
        for name in DisplayProps::NAMES {
            let on = all.iter().filter(|p| p.get(name) == Some(true)).count();
            let mut v = on == all.len();
            if ui.add(egui::Checkbox::new(&mut v, name).indeterminate(on > 0 && on < all.len())).changed() {
                let _ = app.dispatch(Command::SetSelectionDisplayProperty { name: name.into(), value: v });
            }
        }
        ui.separator();
        ui.label("Show Map in Viewport:");
        ui.horizontal_wrapped(|ui| {
            let is_none = props.texture == loom_scene::TextureDisplay::None;
            let is_checker = props.texture == loom_scene::TextureDisplay::Checker;
            let is_bitmap = matches!(props.texture, loom_scene::TextureDisplay::Bitmap(_));
            let ids: Vec<u64> = app.state.selection.iter().map(|n| n.0).collect();
            if ui.radio(is_none, "None").clicked() && !is_none {
                for id in &ids {
                    let _ = app.dispatch(Command::SetTextureDisplay { id: *id, kind: "none".into(), path: None });
                }
            }
            if ui.radio(is_checker, "Checker").clicked() && !is_checker {
                for id in &ids {
                    let _ = app.dispatch(Command::SetTextureDisplay { id: *id, kind: "checker".into(), path: None });
                }
            }
            if ui.radio(is_bitmap, "Bitmap...").clicked() {
                app.state.dialog = Some(loom_commands::Dialog::PickTexture);
            }
        });
        if let loom_scene::TextureDisplay::Bitmap(path) = &props.texture {
            ui.label(egui::RichText::new(path).small());
        }
    });
}
