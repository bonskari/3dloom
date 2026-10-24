//! "Save changes to <name>?" before New / Reset / Open / Exit discards the scene.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, PendingFileAction, SaveChangesReply};

pub fn show(ctx: &egui::Context, app: &mut LoomApp, _action: PendingFileAction, open: &mut bool) {
    let name = app.state.file.as_ref().and_then(|f| f.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into());
    let mut done = false;
    egui::Window::new("3dLoom").open(open).resizable(false).collapsible(false).show(ctx, |ui| {
        ui.label(format!("The scene has been modified.\nDo you want to save your changes to {name}?"));
        ui.horizontal(|ui| {
            for (label, reply) in [("Yes", SaveChangesReply::Yes), ("No", SaveChangesReply::No), ("Cancel", SaveChangesReply::Cancel)] {
                if ui.button(label).clicked() {
                    let _ = app.dispatch(Command::SaveChangesReply { reply });
                    done = true;
                }
            }
        });
    });
    if done {
        *open = false;
    }
}
