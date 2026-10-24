//! The warning before a base parameter change under a topology-dependent
//! modifier: Yes applies it, Hold/Yes holds the scene first, No drops it.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let mut answer: Option<(bool, bool)> = None;
    egui::Window::new("Warning").open(open).resizable(false).collapsible(false).show(ctx, |ui| {
        ui.label("A modifier exists in the stack that depends on topology which may change.\nChanging parameters may have undesirable effects.\nAre you sure?");
        ui.horizontal(|ui| {
            if ui.button("Yes").clicked() {
                answer = Some((true, false));
            }
            if ui.button("Hold/Yes").clicked() {
                answer = Some((true, true));
            }
            if ui.button("No").clicked() {
                answer = Some((false, false));
            }
        });
    });
    let pending = match answer {
        Some(_) => app.pending_topology.take(),
        None if !*open => {
            // Closed with the window's X: as No.
            app.pending_topology = None;
            return;
        }
        None => return,
    };
    *open = false;
    let Some((yes, hold)) = answer else { return };
    if !yes {
        return;
    }
    if hold {
        let _ = app.dispatch(Command::Hold);
    }
    if let Some(cmd) = pending {
        if let Command::SetParameter { id, .. } = &cmd {
            app.topology_ack = Some(loom_scene::NodeId(*id));
        }
        let _ = app.dispatch(cmd);
    }
}
