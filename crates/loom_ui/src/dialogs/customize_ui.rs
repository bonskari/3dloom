//! Customize User Interface dialog, Keyboard panel (read-only): group,
//! category search, action list with shortcuts, Write Keyboard Chart.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let group_id = egui::Id::new("customize_ui_group");
    let search_id = egui::Id::new("customize_ui_search");
    let written_id = egui::Id::new("customize_ui_written");
    let mut group: String = ctx.data(|d| d.get_temp(group_id)).unwrap_or_else(|| "Main UI".to_owned());
    let mut search: String = ctx.data(|d| d.get_temp(search_id)).unwrap_or_default();
    let written: Option<String> = ctx.data(|d| d.get_temp(written_id));
    let rows = loom_commands::keyboard_chart();
    egui::Window::new("Customize User Interface").open(open).default_width(620.0).default_height(520.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            let _ = ui.selectable_label(true, "Keyboard");
            for t in ["Toolbars", "Quads", "Menus", "Colors"] {
                ui.add_enabled(false, egui::SelectableLabel::new(false, t));
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Group:");
            egui::ComboBox::from_id_salt("customize_group").selected_text(group.clone()).show_ui(ui, |ui| {
                for g in ["Main UI", "Loom"] {
                    ui.selectable_value(&mut group, g.to_owned(), g);
                }
            });
            ui.label("Category:");
            ui.add_enabled(false, egui::Button::new("All Commands"));
            ui.label("Search:");
            ui.add(egui::TextEdit::singleline(&mut search).desired_width(160.0));
        });
        ui.label(egui::RichText::new("Shortcuts are the defaults and cannot be edited yet.").small().weak());
        egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
            egui::Grid::new("customize_actions").num_columns(2).striped(true).spacing([24.0, 2.0]).show(ui, |ui| {
                ui.label(egui::RichText::new("Action").strong());
                ui.label(egui::RichText::new("Shortcut").strong());
                ui.end_row();
                let needle = search.to_lowercase();
                for r in rows.iter().filter(|r| r.group == group && (needle.is_empty() || r.action.to_lowercase().contains(&needle))) {
                    ui.label(r.action);
                    ui.label(&r.shortcut);
                    ui.end_row();
                }
            });
        });
        ui.horizontal(|ui| {
            if ui.button("Write Keyboard Chart...").clicked() {
                let path = dirs_path();
                let msg = match loom_commands::write_keyboard_chart(&path) {
                    Ok(()) => format!("Wrote {}", path.display()),
                    Err(e) => format!("Could not write {}: {e}", path.display()),
                };
                ctx.data_mut(|d| d.insert_temp(written_id, msg));
            }
            if let Some(m) = &written {
                ui.label(m);
            }
            if ui.button("Close").clicked() {
                let _ = app.dispatch(Command::CloseDialog);
            }
        });
    });
    ctx.data_mut(|d| {
        d.insert_temp(group_id, group);
        d.insert_temp(search_id, search);
    });
}

/// `~/.config/3dloom/keyboard-chart.txt`.
fn dirs_path() -> std::path::PathBuf {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from("."));
    home.join(".config").join("3dloom").join("keyboard-chart.txt")
}
