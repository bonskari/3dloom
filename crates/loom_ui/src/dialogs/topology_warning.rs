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

/// Display Stack Collapse Warning: before Collapse To / Collapse All. Yes
/// collapses, Hold/Yes holds the scene first, No keeps the stack; "Do not
/// show this message again" with Yes or Hold/Yes turns the preference off.
pub fn collapse(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let mut answer: Option<(bool, bool)> = None;
    let id = egui::Id::new("collapse_warning_dont_show");
    let mut dont_show = ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    egui::Window::new("Warning: Collapse").open(open).resizable(false).collapsible(false).show(ctx, |ui| {
        ui.label("Collapsing the modifier stack applies its modifiers to the object and removes them.\nAre you sure?");
        ui.checkbox(&mut dont_show, "Do not show this message again");
        ui.horizontal(|ui| {
            if ui.button("Hold/Yes").clicked() {
                answer = Some((true, true));
            }
            if ui.button("Yes").clicked() {
                answer = Some((true, false));
            }
            if ui.button("No").clicked() {
                answer = Some((false, false));
            }
        });
    });
    ctx.data_mut(|d| d.insert_temp(id, dont_show));
    if answer.is_none() {
        if !*open {
            // Closed with the window's X: as No.
            app.pending_collapse = None;
        }
        return;
    }
    *open = false;
    ctx.data_mut(|d| d.remove::<bool>(id));
    let Some((yes, hold)) = answer else { return };
    if !yes {
        app.pending_collapse = None;
        return;
    }
    if dont_show {
        let _ = app.dispatch(Command::SetPreference { name: "Stack Collapse Warning".into(), value: 0.0 });
    }
    if hold {
        let _ = app.dispatch(Command::Hold);
    }
    // Dispatched while still pending, so the warning does not ask again.
    if let Some(cmd) = app.pending_collapse.clone() {
        let _ = app.dispatch(cmd);
    }
    app.pending_collapse = None;
}
