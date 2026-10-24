//! Customize User Interface dialog, Keyboard panel: group, Active, search,
//! action list with shortcuts, Hotkey field with Assigned To, Assign /
//! Remove, Write Keyboard Chart, Load / Save / Reset.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Chord, Command};

/// The Hotkey field's id (tests and the key capture find it by this).
pub const HOTKEY_FIELD: &str = "customize_ui_hotkey";

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let group_id = egui::Id::new("customize_ui_group");
    let search_id = egui::Id::new("customize_ui_search");
    let written_id = egui::Id::new("customize_ui_written");
    let action_id = egui::Id::new("customize_ui_action");
    let chord_id = egui::Id::new("customize_ui_chord");
    let hotkey_id = egui::Id::new(HOTKEY_FIELD);
    let mut group: String = ctx.data(|d| d.get_temp(group_id)).unwrap_or_else(|| "Main UI".to_owned());
    let mut search: String = ctx.data(|d| d.get_temp(search_id)).unwrap_or_default();
    let mut written: Option<String> = ctx.data(|d| d.get_temp(written_id));
    let mut action: Option<String> = ctx.data(|d| d.get_temp(action_id));
    let mut chord: Option<Chord> = ctx.data(|d| d.get_temp(chord_id));
    // The Hotkey field takes the next key press, modifiers and all.
    if ctx.memory(|m| m.has_focus(hotkey_id)) {
        let pressed = ctx.input(|i| crate::hotkeys::pressed_keys(&i.events));
        if let Some(c) = pressed.into_iter().filter_map(|(k, m)| crate::hotkeys::chord_of(k, m)).next_back() {
            chord = Some(c);
        }
    }
    let keys = app.state.custom_keys.get(&group).clone();
    let mut shortcuts: std::collections::HashMap<&str, Vec<String>> = std::collections::HashMap::new();
    for (c, a) in keys.bindings() {
        shortcuts.entry(a).or_default().push(c.label());
    }
    for v in shortcuts.values_mut() {
        v.sort();
    }
    let needle = search.to_lowercase();
    let actions: Vec<&'static str> = loom_commands::keymap::group_actions(&group).into_iter().filter(|a| needle.is_empty() || a.to_lowercase().contains(&needle)).collect();
    egui::Window::new("Customize User Interface").open(open).default_width(660.0).default_height(540.0).show(ctx, |ui| {
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
                for g in loom_commands::keymap::GROUPS {
                    if ui.selectable_value(&mut group, g.to_owned(), g).clicked() {
                        action = None;
                    }
                }
            });
            let mut active = app.state.keyboard_override;
            if ui.checkbox(&mut active, "Active").on_hover_text("Context shortcuts (Keyboard Shortcut Override Toggle)").changed() {
                let _ = app.dispatch(Command::KeyboardShortcutOverrideToggle);
            }
            ui.label("Search:");
            ui.add(egui::TextEdit::singleline(&mut search).desired_width(160.0));
        });
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(430.0);
                let row_h = ui.spacing().interact_size.y;
                egui::ScrollArea::vertical().max_height(380.0).auto_shrink([false, false]).show_rows(ui, row_h, actions.len(), |ui, range| {
                    for a in &actions[range] {
                        ui.horizontal(|ui| {
                            let selected = action.as_deref() == Some(*a);
                            let clicked = ui
                                .allocate_ui_with_layout(egui::vec2(290.0, row_h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    ui.set_min_width(290.0);
                                    ui.add(egui::SelectableLabel::new(selected, *a)).clicked()
                                })
                                .inner;
                            if clicked {
                                action = Some((*a).to_owned());
                            }
                            ui.label(shortcuts.get(a).map(|v| v.join(", ")).unwrap_or_default());
                        });
                    }
                });
            });
            ui.vertical(|ui| {
                ui.label("Hotkey:");
                let mut text = chord.map(|c| c.label()).unwrap_or_default();
                ui.add(egui::TextEdit::singleline(&mut text).id(hotkey_id).desired_width(170.0).hint_text("press a key"));
                let owner = chord.and_then(|c| keys.lookup(c));
                ui.label("Assigned to:");
                ui.label(owner.unwrap_or(""));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(action.is_some() && chord.is_some(), egui::Button::new("Assign")).clicked() {
                        if let (Some(a), Some(c)) = (action.clone(), chord) {
                            let _ = app.dispatch(Command::AssignShortcut { group: group.clone(), action: a, chord: c.chart_text() });
                        }
                    }
                    if ui.add_enabled(action.is_some(), egui::Button::new("Remove")).clicked() {
                        if let Some(a) = action.clone() {
                            let _ = app.dispatch(Command::RemoveShortcut { group: group.clone(), action: a });
                        }
                    }
                });
                if let Some(e) = &app.last_error {
                    ui.label(egui::RichText::new(e).small().color(egui::Color32::LIGHT_RED));
                }
                ui.add_space(12.0);
                if ui.button("Write Keyboard Chart...").clicked() {
                    let path = chart_path();
                    written = Some(match loom_commands::keymap::write_keyboard_chart_with(&app.state.custom_keys, &path) {
                        Ok(()) => format!("Wrote {}", path.display()),
                        Err(e) => format!("Could not write {}: {e}", path.display()),
                    });
                }
                if let Some(m) = &written {
                    ui.label(egui::RichText::new(m).small());
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Load...").clicked() {
                        let _ = app.dispatch(Command::LoadShortcuts { path: None });
                    }
                    if ui.button("Save...").clicked() {
                        let _ = app.dispatch(Command::SaveShortcuts { path: None });
                    }
                    if ui.button("Reset").clicked() {
                        let _ = app.dispatch(Command::ResetShortcuts);
                    }
                });
            });
        });
        if ui.button("Close").clicked() {
            let _ = app.dispatch(Command::CloseDialog);
        }
    });
    ctx.data_mut(|d| {
        d.insert_temp(group_id, group);
        d.insert_temp(search_id, search);
        match action {
            Some(a) => d.insert_temp(action_id, a),
            None => d.remove::<String>(action_id),
        }
        match chord {
            Some(c) => d.insert_temp(chord_id, c),
            None => d.remove::<Chord>(chord_id),
        }
        match written {
            Some(w) => d.insert_temp(written_id, w),
            None => d.remove::<String>(written_id),
        }
    });
}

/// `keyboard-chart.txt` in the config dir.
fn chart_path() -> std::path::PathBuf {
    loom_commands::settings::config_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join("keyboard-chart.txt")
}
