//! Spline prompts: the Line tool's "Close spline?" and Refine with Connect's
//! Refine / Connect Only warning on an existing vertex.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

pub fn close_spline(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let mut reply = None;
    egui::Window::new("3dLoom").id(egui::Id::new("close_spline_prompt")).open(open).resizable(false).collapsible(false).show(ctx, |ui| {
        ui.label("Close spline?");
        ui.horizontal(|ui| {
            if ui.button("Yes").clicked() {
                reply = Some(true);
            }
            if ui.button("No").clicked() {
                reply = Some(false);
            }
        });
    });
    if let Some(close) = reply {
        let _ = app.dispatch(Command::LineCloseReply { close });
        *open = false;
    }
}

pub fn refine_vertex(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("refine_vertex_prompt");
    let mut dont_show: bool = ctx.data(|d| d.get_temp(id)).unwrap_or(false);
    let mut reply = None;
    egui::Window::new("Refine & Connect").id(id).open(open).resizable(false).collapsible(false).show(ctx, |ui| {
        ui.label("There is already a vertex where you clicked.\nRefine adds another vertex next to it; Connect Only connects the existing vertex.");
        ui.checkbox(&mut dont_show, "Do not show this message again");
        ui.horizontal(|ui| {
            if ui.button("Refine").clicked() {
                reply = Some(false);
            }
            if ui.button("Connect Only").clicked() {
                reply = Some(true);
            }
        });
    });
    ctx.data_mut(|d| d.insert_temp(id, dont_show));
    if let Some(connect_only) = reply {
        let _ = app.dispatch(Command::SplineRefineReply { connect_only, dont_show });
        *open = false;
    }
}
