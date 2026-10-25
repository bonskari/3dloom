//! Snapshot dialog (Tools > Snapshot): Single / Range (From, To, Copies)
//! and the Clone Method (Copy, Instance, Reference, Mesh).

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

#[derive(Clone, Copy)]
struct Settings {
    range: bool,
    from: i32,
    to: i32,
    copies: u32,
    /// 0 Copy, 1 Instance, 2 Reference, 3 Mesh.
    method: u32,
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("snapshot_dialog");
    let (start, end) = app.state.frame_range;
    let mut st: Settings = ctx.data(|d| d.get_temp(id)).unwrap_or(Settings { range: false, from: start, to: end, copies: 10, method: 0 });
    let mut ok = false;
    let mut cancel = false;
    egui::Window::new("Snapshot").open(open).resizable(false).collapsible(false).show(ctx, |ui| {
        ui.group(|ui| {
            ui.label("Snapshot");
            ui.radio_value(&mut st.range, false, "Single");
            ui.radio_value(&mut st.range, true, "Range");
            ui.add_enabled_ui(st.range, |ui| {
                egui::Grid::new("snapshot_range").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                    ui.label("From:");
                    ui.add(crate::spinner::spin(ui, &mut st.from, ..).speed(0.2));
                    ui.end_row();
                    ui.label("To:");
                    ui.add(crate::spinner::spin(ui, &mut st.to, ..).speed(0.2));
                    ui.end_row();
                    ui.label("Copies:");
                    ui.add(crate::spinner::spin(ui, &mut st.copies, 1..=10_000).speed(0.1));
                    ui.end_row();
                });
            });
        });
        ui.group(|ui| {
            ui.label("Clone Method");
            for (i, label) in ["Copy", "Instance", "Reference", "Mesh"].iter().enumerate() {
                ui.radio_value(&mut st.method, i as u32, *label);
            }
        });
        ui.horizontal(|ui| {
            ok = ui.button("OK").clicked();
            cancel = ui.button("Cancel").clicked();
        });
    });
    ctx.data_mut(|d| d.insert_temp(id, st));
    if ok {
        if let Err(e) = app.dispatch(Command::Snapshot { range: st.range, from: st.from, to: st.to, copies: st.copies, method: st.method }) {
            app.state.status_prompt = e;
        }
    }
    if ok || cancel {
        *open = false;
    }
}
