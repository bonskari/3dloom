//! Clone Options dialog (Edit > Clone, Ctrl+V, release of a Shift+drag):
//! Copy / Instance / Reference, Number of Copies for Shift+drag, Name.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{CloneKind, Command};

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let kind_id = egui::Id::new("clone_options_kind");
    let copies_id = egui::Id::new("clone_options_copies");
    let name_id = egui::Id::new("clone_options_name");
    let mut kind: CloneKind = ctx.data(|d| d.get_temp(kind_id)).unwrap_or_default();
    let mut copies: u32 = ctx.data(|d| d.get_temp(copies_id)).unwrap_or(1);
    let shift = app.shift_clone.clone();
    // Suggest the next free name; a Shift+drag already holds one copy, so
    // name after the original to keep the sequence (Box002, Box003, ...).
    let first_id = shift.as_ref().and_then(|s| s.sources.first().copied()).or_else(|| app.state.selection.iter().next().copied());
    let first = first_id.and_then(|id| app.state.scene.node(id)).map(|n| n.name.clone()).unwrap_or_default();
    let default_name = app.state.scene.next_name(first.trim_end_matches(|c: char| c.is_ascii_digit()));
    let mut name: String = ctx.data(|d| d.get_temp(name_id)).unwrap_or_else(|| default_name.clone());
    let mut done = false;
    egui::Window::new("Clone Options").open(open).resizable(false).show(ctx, |ui| {
        ui.horizontal_top(|ui| {
            ui.group(|ui| {
                ui.label(egui::RichText::new("Object").strong());
                ui.radio_value(&mut kind, CloneKind::Copy, "Copy");
                ui.radio_value(&mut kind, CloneKind::Instance, "Instance");
                ui.radio_value(&mut kind, CloneKind::Reference, "Reference");
            });
            ui.group(|ui| {
                ui.label(egui::RichText::new("Controller").strong());
                ui.add_enabled(false, egui::RadioButton::new(true, "Copy"));
                ui.add_enabled(false, egui::RadioButton::new(false, "Instance"));
            });
        });
        ui.horizontal(|ui| {
            ui.label("Number of Copies:");
            ui.add_enabled(shift.is_some(), crate::spinner::spin(ui, &mut copies, 1..=1000).speed(app.spinner_speed(0.1)));
        });
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.add(egui::TextEdit::singleline(&mut name).desired_width(200.0));
        });
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                if let Some(sc) = &shift {
                    // The drag already moved a Copy inside one undo step; redo it as asked
                    // (only if that step is still on top of the undo stack).
                    if app.state.undo.len() == sc.undo_len {
                        let _ = app.dispatch(Command::UndoSceneOperation);
                    }
                    let _ = app.dispatch(Command::SelectNodes { ids: sc.sources.iter().map(|i| i.0).collect(), op: loom_commands::SelectOp::Replace });
                    let rotate = if sc.rotate_deg.length_squared() > 0.0 {
                        sc.rotate_deg
                    } else {
                        let (x, y, z) = sc.quat.to_euler(glam::EulerRot::XYZ);
                        glam::Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
                    };
                    let (offset, rotate_deg, scale) = match sc.tool {
                        loom_commands::Tool::Rotate => (None, Some(rotate.to_array()), None),
                        loom_commands::Tool::Scale => (None, None, Some(sc.scale.to_array())),
                        _ => (Some(sc.offset.to_array()), None, None),
                    };
                    // The suggested name is only valid once the drag copy is undone; let the command number the copies.
                    let custom = (name != default_name).then(|| name.clone());
                    let _ = app.dispatch(Command::CloneOptions { kind, copies, name: custom, offset, rotate_deg, scale });
                } else {
                    let _ = app.dispatch(Command::CloneOptions { kind, copies: 1, name: Some(name.clone()), offset: None, rotate_deg: None, scale: None });
                }
                done = true;
            }
            if ui.button("Cancel").clicked() {
                if let Some(sc) = &shift {
                    if app.state.undo.len() == sc.undo_len {
                        let _ = app.dispatch(Command::UndoSceneOperation);
                    }
                }
                done = true;
            }
        });
    });
    if done || !*open {
        if let (false, Some(sc)) = (done, &shift) {
            if app.state.undo.len() == sc.undo_len {
                let _ = app.dispatch(Command::UndoSceneOperation);
            }
        }
        app.shift_clone = None;
        *open = false;
        ctx.data_mut(|d| {
            d.remove::<String>(name_id);
            d.remove::<u32>(copies_id);
        });
    } else {
        ctx.data_mut(|d| {
            d.insert_temp(kind_id, kind);
            d.insert_temp(copies_id, copies);
            d.insert_temp(name_id, name);
        });
    }
}
