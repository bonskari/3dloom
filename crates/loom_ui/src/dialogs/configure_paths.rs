//! Customize > Configure User Paths: the File I/O panel (each entry's folder,
//! Modify / Clear) and the External Files panel (bitmap search folders: Add,
//! Delete, Move Up / Move Down).

use crate::LoomApp;
use eframe::egui;
use loom_commands::paths::FILE_IO;
use loom_commands::Command;

#[derive(Debug, Clone, Default)]
struct St {
    external: bool,
    picked: usize,
    /// The folder being typed for the picked entry (File I/O) or to add (External Files).
    text: String,
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("configure_paths");
    let mut st: St = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
    egui::Window::new("Configure User Paths").open(open).default_width(560.0).resizable(true).show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.selectable_label(!st.external, "File I/O").clicked() {
                st.external = false;
                st.picked = 0;
            }
            if ui.selectable_label(st.external, "External Files").clicked() {
                st.external = true;
                st.picked = 0;
            }
        });
        ui.separator();
        if st.external {
            external(ui, app, &mut st);
        } else {
            file_io(ui, app, &mut st);
        }
        if let Some(e) = &app.last_error {
            ui.label(egui::RichText::new(e).small().color(egui::Color32::LIGHT_RED));
        }
        if ui.button("OK").clicked() {
            let _ = app.dispatch(Command::CloseDialog);
        }
    });
    ctx.data_mut(|d| d.insert_temp(id, st));
}

fn file_io(ui: &mut egui::Ui, app: &mut LoomApp, st: &mut St) {
    egui::Grid::new("file_io_paths").num_columns(2).striped(true).show(ui, |ui| {
        for (i, name) in FILE_IO.iter().enumerate() {
            if ui.selectable_label(st.picked == i, *name).clicked() {
                st.picked = i;
                st.text = app.state.paths.file_io.get(*name).map(|p| p.display().to_string()).unwrap_or_default();
            }
            let shown = app.state.paths.file_io.get(*name).map(|p| p.display().to_string()).unwrap_or_else(|| "(default)".to_owned());
            ui.label(shown);
            ui.end_row();
        }
    });
    let name = FILE_IO[st.picked.min(FILE_IO.len() - 1)];
    ui.horizontal(|ui| {
        ui.label(format!("{name}:"));
        ui.add(egui::TextEdit::singleline(&mut st.text).desired_width(320.0).hint_text("folder"));
        if ui.button("Modify").clicked() {
            let _ = app.dispatch(Command::SetUserPath { name: name.into(), path: st.text.clone() });
        }
        if ui.button("Clear").on_hover_text("Back to the program's folder").clicked() {
            st.text.clear();
            let _ = app.dispatch(Command::SetUserPath { name: name.into(), path: String::new() });
        }
    });
}

fn external(ui: &mut egui::Ui, app: &mut LoomApp, st: &mut St) {
    ui.label("Bitmaps not found at their path are looked for here, in this order:");
    let list: Vec<String> = app.state.paths.bitmaps.iter().map(|p| p.display().to_string()).collect();
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_size(egui::vec2(480.0, 120.0));
        for (i, p) in list.iter().enumerate() {
            if ui.selectable_label(st.picked == i, p).clicked() {
                st.picked = i;
            }
        }
    });
    let set = |app: &mut LoomApp, v: Vec<String>| {
        let _ = app.dispatch(Command::SetBitmapPaths { paths: v });
    };
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut st.text).desired_width(320.0).hint_text("folder"));
        if ui.button("Add").clicked() && !st.text.trim().is_empty() {
            let mut v = list.clone();
            v.push(st.text.trim().to_owned());
            set(app, v);
            st.text.clear();
        }
    });
    ui.horizontal(|ui| {
        let i = st.picked;
        let has = i < list.len();
        if ui.add_enabled(has, egui::Button::new("Delete")).clicked() {
            let mut v = list.clone();
            v.remove(i);
            set(app, v);
        }
        if ui.add_enabled(has && i > 0, egui::Button::new("Move Up")).clicked() {
            let mut v = list.clone();
            v.swap(i, i - 1);
            set(app, v);
            st.picked = i - 1;
        }
        if ui.add_enabled(has && i + 1 < list.len(), egui::Button::new("Move Down")).clicked() {
            let mut v = list.clone();
            v.swap(i, i + 1);
            set(app, v);
            st.picked = i + 1;
        }
    });
}
