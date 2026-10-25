//! Customize User Interface dialog: the Keyboard, Toolbars, Quads, Menus and
//! Colors panels.
//!
//! Keyboard: group, Active, search, action list with shortcuts, Hotkey field
//! with Assigned To, Assign / Remove, Write Keyboard Chart, Load / Save / Reset.
//! Toolbars: New / Delete / Rename / Hide toolbars, drag actions onto a
//! toolbar (on screen or onto its list here), Load / Save / Reset (.cui).
//! Quads: quad sets (New / Delete / Rename), Quad Shortcut, Show All Quads,
//! the four quadrants with their labels and entries (drag actions,
//! separators and menus in; right-click to delete or rename), Advanced
//! Options, Load / Save / Reset (.mnu). Menus: the menu list (New / Delete /
//! Rename, & accelerators, On Menu Bar), a menu's entries (its own, which
//! can be deleted, then the added ones), Load / Save / Reset (.mnu).
//! Colors: element colours by category, the UI scheme and its colours,
//! Load / Save / Reset (.clr).

use crate::toolbars::ActionDrag;
use crate::LoomApp;
use eframe::egui;
use loom_commands::ui_custom::{self, CustomItem, MENU_PREFIX, SEPARATOR};
use loom_commands::{Chord, Command};

/// The Hotkey field's id (tests and the key capture find it by this).
pub const HOTKEY_FIELD: &str = "customize_ui_hotkey";
/// The Quad Shortcut field's id.
pub const QUAD_SHORTCUT_FIELD: &str = "customize_ui_quad_shortcut";

/// The dialog's panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Panel {
    #[default]
    Keyboard,
    Toolbars,
    Quads,
    Menus,
    Colors,
}

impl Panel {
    const ALL: [Panel; 5] = [Panel::Keyboard, Panel::Toolbars, Panel::Quads, Panel::Menus, Panel::Colors];
    fn label(self) -> &'static str {
        match self {
            Panel::Keyboard => "Keyboard",
            Panel::Toolbars => "Toolbars",
            Panel::Quads => "Quads",
            Panel::Menus => "Menus",
            Panel::Colors => "Colors",
        }
    }
}

fn panel_id() -> egui::Id {
    egui::Id::new("customize_ui_panel")
}

/// The dialog opens on `panel` next time it shows.
pub fn open_on(ctx: &egui::Context, panel: Panel) {
    ctx.data_mut(|d| d.insert_temp(panel_id(), panel));
}

/// The Toolbars panel shows toolbar `name`.
pub fn pick_toolbar(ctx: &egui::Context, name: &str) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("customize_ui_toolbar"), name.to_owned()));
}

/// Per-panel picks and text fields kept between frames.
#[derive(Debug, Clone, Default)]
struct Picks {
    toolbar: Option<String>,
    new_name: String,
    rename: String,
    quad_set: String,
    quadrant: usize,
    quad_label: Option<(String, usize, String)>,
    quad_chord: Option<Chord>,
    menu: String,
    menu_title: Option<(String, String)>,
    /// An added entry being renamed: (where, index, text).
    item_name: Option<(String, usize, String)>,
    color_category: String,
    color_element: String,
    appearance: String,
    advanced: bool,
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let mut panel: Panel = ctx.data(|d| d.get_temp(panel_id())).unwrap_or_default();
    let picks_id = egui::Id::new("customize_ui_picks");
    let mut picks: Picks = ctx.data(|d| d.get_temp(picks_id)).unwrap_or_default();
    if let Some(t) = ctx.data_mut(|d| d.remove_temp::<String>(egui::Id::new("customize_ui_toolbar"))) {
        picks.toolbar = Some(t);
    }
    egui::Window::new("Customize User Interface").open(open).default_width(760.0).default_height(560.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            for p in Panel::ALL {
                if ui.selectable_label(panel == p, p.label()).clicked() {
                    panel = p;
                }
            }
        });
        ui.separator();
        match panel {
            Panel::Keyboard => keyboard(ui, app),
            Panel::Toolbars => toolbars(ui, app, &mut picks),
            Panel::Quads => quads(ui, app, &mut picks),
            Panel::Menus => menus(ui, app, &mut picks),
            Panel::Colors => colors(ui, app, &mut picks),
        }
        if let Some(e) = &app.last_error {
            ui.label(egui::RichText::new(e).small().color(egui::Color32::LIGHT_RED));
        }
        if ui.button("Close").clicked() {
            let _ = app.dispatch(Command::CloseDialog);
        }
    });
    if picks.advanced {
        advanced_quad_options(ctx, app, &mut picks.advanced);
    }
    ctx.data_mut(|d| {
        d.insert_temp(panel_id(), panel);
        d.insert_temp(picks_id, picks);
    });
}

/// The action list the Toolbars, Quads and Menus panels share: each action
/// with its icon, filtered by the search text; drag one onto a toolbar, a
/// quadrant or a menu. Double-click does what a drop on `target` would.
fn action_list(ui: &mut egui::Ui, height: f32) -> Option<String> {
    let group_id = egui::Id::new("customize_ui_group");
    let search_id = egui::Id::new("customize_ui_search");
    let mut group: String = ui.ctx().data(|d| d.get_temp(group_id)).unwrap_or_else(|| "Main UI".to_owned());
    let mut search: String = ui.ctx().data(|d| d.get_temp(search_id)).unwrap_or_default();
    let mut double = None;
    ui.vertical(|ui| {
        ui.set_width(300.0);
        ui.horizontal(|ui| {
            ui.label("Group:");
            egui::ComboBox::from_id_salt("customize_actions_group").selected_text(group.clone()).show_ui(ui, |ui| {
                for g in loom_commands::keymap::GROUPS {
                    ui.selectable_value(&mut group, g.to_owned(), g);
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label("Search:");
            ui.add(egui::TextEdit::singleline(&mut search).desired_width(200.0));
        });
        let needle = search.to_lowercase();
        let actions: Vec<&'static str> = loom_commands::keymap::group_actions(&group).into_iter().filter(|a| needle.is_empty() || a.to_lowercase().contains(&needle)).collect();
        let row_h = ui.spacing().interact_size.y;
        egui::ScrollArea::vertical().id_salt("customize_action_list").max_height(height).auto_shrink([false, false]).show_rows(ui, row_h, actions.len(), |ui, range| {
            for a in &actions[range] {
                let text = match crate::icons::for_action(a) {
                    Some(g) => crate::icons::labeled(g, a.trim()),
                    None => format!("      {}", a.trim()),
                };
                let r = ui.add(egui::SelectableLabel::new(false, text)).interact(egui::Sense::drag());
                crate::icons::named(&r, egui::WidgetType::SelectableLabel, a.trim());
                r.dnd_set_drag_payload(ActionDrag((*a).to_owned()));
                if r.double_clicked() {
                    double = Some((*a).to_owned());
                }
            }
        });
    });
    ui.ctx().data_mut(|d| {
        d.insert_temp(group_id, group);
        d.insert_temp(search_id, search);
    });
    double
}

/// A draggable source row (the Separator and Menus windows).
fn source_row(ui: &mut egui::Ui, label: &str, payload: &str) -> egui::Response {
    let r = ui.add(egui::SelectableLabel::new(false, label)).interact(egui::Sense::drag());
    r.dnd_set_drag_payload(ActionDrag(payload.to_owned()));
    r
}

/// A drop zone: a framed list; returns the dropped item and where (its index among `rows`).
fn drop_list(ui: &mut egui::Ui, id: &str, height: f32, body: impl FnOnce(&mut egui::Ui) -> Vec<egui::Rect>) -> Option<(String, u32)> {
    let frame = egui::Frame::group(ui.style());
    let (r, rows) = {
        let inner = frame.show(ui, |ui| {
            ui.set_min_size(egui::vec2(ui.available_width().min(380.0), height));
            egui::ScrollArea::vertical().id_salt(id).max_height(height).auto_shrink([false, false]).show(ui, body).inner
        });
        (inner.response, inner.inner)
    };
    let hovering = r.dnd_hover_payload::<ActionDrag>().is_some();
    if hovering {
        ui.painter().rect_stroke(r.rect, 2.0, egui::Stroke::new(1.5, ui.visuals().selection.stroke.color), egui::StrokeKind::Inside);
    }
    let at = ui.input(|i| i.pointer.interact_pos());
    r.dnd_release_payload::<ActionDrag>().map(|p| {
        let index = at.map(|p| rows.iter().filter(|r| r.center().y < p.y).count()).unwrap_or(rows.len());
        (p.0.clone(), index as u32)
    })
}

/// Keyboard panel.
fn keyboard(ui: &mut egui::Ui, app: &mut LoomApp) {
    let ctx = ui.ctx().clone();
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

/// Load... / Save... / Reset for a panel's file.
fn file_buttons(ui: &mut egui::Ui, app: &mut LoomApp, load: Command, save: Command, reset: Command) {
    ui.horizontal(|ui| {
        if ui.button("Load...").clicked() {
            let _ = app.dispatch(load);
        }
        if ui.button("Save...").clicked() {
            let _ = app.dispatch(save);
        }
        if ui.button("Reset").clicked() {
            let _ = app.dispatch(reset);
        }
    });
}

/// A small name field with an OK button; returns the name when OK is clicked or Enter pressed.
fn name_field(ui: &mut egui::Ui, text: &mut String, hint: &str, ok: &str) -> Option<String> {
    let mut out = None;
    ui.horizontal(|ui| {
        let r = ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(170.0));
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (ui.add_enabled(!text.trim().is_empty(), egui::Button::new(ok)).clicked() || enter) && !text.trim().is_empty() {
            out = Some(text.trim().to_owned());
            text.clear();
        }
    });
    out
}

/// Toolbars panel.
fn toolbars(ui: &mut egui::Ui, app: &mut LoomApp, picks: &mut Picks) {
    let all = crate::toolbars::all_toolbars(app);
    if picks.toolbar.as_ref().is_none_or(|t| !all.contains(t)) {
        picks.toolbar = app.state.ui_custom.toolbars.first().map(|t| t.name.clone()).or_else(|| all.first().cloned());
    }
    ui.horizontal_top(|ui| {
        let double = action_list(ui, 360.0);
        ui.separator();
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label("Toolbar:");
                let current = picks.toolbar.clone().unwrap_or_default();
                egui::ComboBox::from_id_salt("customize_toolbar_list").selected_text(current.clone()).width(180.0).show_ui(ui, |ui| {
                    for t in &all {
                        if ui.selectable_label(current == *t, t).clicked() {
                            picks.toolbar = Some(t.clone());
                        }
                    }
                });
            });
            ui.label("New toolbar:");
            if let Some(name) = name_field(ui, &mut picks.new_name, "Toolbar name", "New") {
                if app.dispatch(Command::NewToolbar { name: name.clone() }).is_ok() {
                    picks.toolbar = Some(name);
                }
            }
            let current = picks.toolbar.clone().unwrap_or_default();
            let user = app.state.ui_custom.toolbar(&current).is_some();
            ui.horizontal(|ui| {
                if ui.add_enabled(user, egui::Button::new("Delete")).clicked() {
                    let _ = app.dispatch(Command::DeleteToolbar { name: current.clone() });
                }
                let mut hide = !ui_custom::is_shown(&app.state, &current);
                if ui.checkbox(&mut hide, "Hide").changed() {
                    let _ = app.dispatch(Command::ShowToolbar { name: current.clone(), on: Some(!hide) });
                }
            });
            if user {
                ui.label("Rename to:");
                if let Some(to) = name_field(ui, &mut picks.rename, &current, "Rename") {
                    if app.dispatch(Command::RenameToolbar { name: current.clone(), to: to.clone() }).is_ok() {
                        picks.toolbar = Some(to);
                    }
                }
                ui.label("Buttons (drop actions here, or onto the toolbar itself):");
                let items = app.state.ui_custom.toolbar(&current).map(|t| t.items.clone()).unwrap_or_default();
                let mut remove = None;
                let mut swap = None;
                let dropped = drop_list(ui, "customize_toolbar_items", 170.0, |ui| {
                    let mut rows = Vec::new();
                    for (i, it) in items.iter().enumerate() {
                        let r = ui.horizontal(|ui| {
                            ui.label(if it == SEPARATOR { "---- separator ----".to_owned() } else { it.trim().to_owned() });
                            if crate::icons::small(ui, crate::icons::UP, "Move Up").clicked() && i > 0 {
                                swap = Some((i, i - 1));
                            }
                            if crate::icons::small(ui, crate::icons::DOWN, "Move Down").clicked() && i + 1 < items.len() {
                                swap = Some((i, i + 1));
                            }
                            if crate::icons::small(ui, crate::icons::CLEAR, "Remove").clicked() {
                                remove = Some(i);
                            }
                        });
                        rows.push(r.response.rect);
                    }
                    rows
                });
                if let Some((item, index)) = dropped {
                    if let Err(e) = app.dispatch(Command::AddToolbarButton { toolbar: current.clone(), action: item, index: Some(index) }) {
                        app.state.status_prompt = e;
                    }
                }
                if let Some(a) = double {
                    let _ = app.dispatch(Command::AddToolbarButton { toolbar: current.clone(), action: a, index: None });
                }
                if let Some(i) = remove {
                    let _ = app.dispatch(Command::RemoveToolbarButton { toolbar: current.clone(), index: i as u32 });
                }
                if let Some((from, to)) = swap {
                    let _ = app.dispatch(Command::MoveToolbarButton { toolbar: current.clone(), index: from as u32, to_toolbar: current.clone(), to_index: Some(to as u32), copy: false });
                }
                ui.horizontal(|ui| {
                    ui.label("Separator:");
                    source_row(ui, "---- separator ----", SEPARATOR);
                });
            }
            ui.add_space(8.0);
            file_buttons(ui, app, Command::LoadToolbars { path: None }, Command::SaveToolbars { path: None }, Command::ResetToolbars);
        });
    });
}

/// The program's entries quadrant `q` of quad set `set` shows now (they depend on the selection).
fn program_entries(app: &LoomApp, set: &str, q: usize) -> Vec<String> {
    let kind = match set {
        "Snap" => crate::quad_menu::QuadKind::Snaps,
        "Modeling" => crate::quad_menu::QuadKind::Create,
        "Animation" => crate::quad_menu::QuadKind::Animation,
        "Lighting | Render" => crate::quad_menu::QuadKind::Render,
        "Default Viewport Quad" => crate::quad_menu::QuadKind::Default,
        _ => return Vec::new(),
    };
    let quads = crate::quad_menu::program_quadrants(app, kind);
    let quad = match q {
        0 => Some(&quads.display),
        1 => Some(&quads.transform),
        2 => quads.tools1.as_ref(),
        _ => quads.tools2.as_ref(),
    };
    quad.map(|q| q.items.iter().map(|e| crate::quad_menu::plain_label(&e.label).to_owned()).collect()).unwrap_or_default()
}

/// The menus that can go into a quad or a menu: the program's and the user's.
fn menu_names(app: &LoomApp) -> Vec<String> {
    ui_custom::BUILTIN_MENUS.iter().map(|m| (*m).to_owned()).chain(app.state.ui_custom.menus.user.iter().map(|u| u.name.clone())).collect()
}

/// Added entries of a quadrant or a menu, each with right-click Delete Menu
/// Item / Edit Menu Item Name; returns their rects.
fn added_entries(ui: &mut egui::Ui, items: &[CustomItem], key: &str, picks: &mut Picks, remove: &mut Option<usize>, rename: &mut Option<(usize, String)>) -> Vec<egui::Rect> {
    let mut rows = Vec::new();
    for (i, it) in items.iter().enumerate() {
        let editing = picks.item_name.as_ref().is_some_and(|(k, j, _)| k == key && *j == i);
        if editing {
            let mut done = false;
            if let Some((_, _, text)) = picks.item_name.as_mut() {
                let r = ui.add(egui::TextEdit::singleline(text).desired_width(200.0));
                r.request_focus();
                if r.lost_focus() {
                    *rename = Some((i, text.clone()));
                    done = true;
                }
                rows.push(r.rect);
            }
            if done {
                picks.item_name = None;
            }
            continue;
        }
        let label = if it.is_separator() {
            "---- separator ----".to_owned()
        } else if let Some(m) = it.menu() {
            let _ = m;
            format!("{} (menu) {}", it.label(), crate::icons::SUBMENU)
        } else {
            it.label().to_owned()
        };
        let r = ui.add(egui::SelectableLabel::new(false, label));
        r.context_menu(|ui| {
            if ui.button("Delete Menu Item").clicked() {
                *remove = Some(i);
                ui.close_menu();
            }
            if !it.is_separator() && ui.button("Edit Menu Item Name...").clicked() {
                picks.item_name = Some((key.to_owned(), i, it.name.clone()));
                ui.close_menu();
            }
        });
        rows.push(r.rect);
    }
    rows
}

/// Quads panel.
fn quads(ui: &mut egui::Ui, app: &mut LoomApp, picks: &mut Picks) {
    let sets = app.state.ui_custom.quad_sets();
    if !sets.contains(&picks.quad_set) {
        picks.quad_set = sets[0].clone();
    }
    let set = picks.quad_set.clone();
    let q = picks.quadrant.min(3);
    let custom = app.state.ui_custom.quad(&set);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            let double = action_list(ui, 250.0);
            ui.label("Separator:");
            source_row(ui, "---- separator ----", SEPARATOR);
            ui.label("Menus:");
            egui::ScrollArea::vertical().id_salt("customize_quad_menus").max_height(80.0).show(ui, |ui| {
                for m in menu_names(app) {
                    source_row(ui, &m, &format!("{MENU_PREFIX}{m}"));
                }
            });
            if let Some(a) = double {
                let _ = app.dispatch(Command::AddQuadItem { set: set.clone(), quadrant: q as u32, item: a, index: None });
            }
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label("Quad set:");
                egui::ComboBox::from_id_salt("customize_quad_sets").selected_text(set.clone()).width(180.0).show_ui(ui, |ui| {
                    for s in &sets {
                        if ui.selectable_label(*s == set, s).clicked() {
                            picks.quad_set = s.clone();
                        }
                    }
                });
            });
            if let Some(name) = name_field(ui, &mut picks.new_name, "New quad set name", "New") {
                if app.dispatch(Command::NewQuadSet { name: name.clone() }).is_ok() {
                    picks.quad_set = name;
                }
            }
            if custom.user {
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        let _ = app.dispatch(Command::DeleteQuadSet { name: set.clone() });
                    }
                });
                if let Some(to) = name_field(ui, &mut picks.rename, &set, "Rename") {
                    if app.dispatch(Command::RenameQuadSet { name: set.clone(), to: to.clone() }).is_ok() {
                        picks.quad_set = to;
                    }
                }
            }
            // Quad Shortcut: press a chord in the field, then Assign.
            let field = egui::Id::new(QUAD_SHORTCUT_FIELD);
            if ui.ctx().memory(|m| m.has_focus(field)) {
                let pressed = ui.input(|i| crate::hotkeys::pressed_keys(&i.events));
                if let Some(c) = pressed.into_iter().filter_map(|(k, m)| crate::hotkeys::chord_of(k, m)).next_back() {
                    picks.quad_chord = Some(c);
                }
            }
            ui.horizontal(|ui| {
                ui.label("Quad Shortcut:");
                let mut text = picks.quad_chord.map(|c| c.label()).unwrap_or_else(|| custom.shortcut.clone());
                ui.add(egui::TextEdit::singleline(&mut text).id(field).desired_width(110.0).hint_text("press a key"));
                if ui.add_enabled(picks.quad_chord.is_some(), egui::Button::new("Assign")).clicked() {
                    if let Some(c) = picks.quad_chord.take() {
                        let _ = app.dispatch(Command::SetQuadShortcut { set: set.clone(), chord: c.chart_text() });
                    }
                }
                if ui.add_enabled(!custom.shortcut.is_empty(), egui::Button::new("Remove")).clicked() {
                    let _ = app.dispatch(Command::SetQuadShortcut { set: set.clone(), chord: String::new() });
                }
            });
            let mut all = custom.show_all;
            if ui.checkbox(&mut all, "Show All Quads").changed() {
                let _ = app.dispatch(Command::SetShowAllQuads { set: set.clone(), on: all });
            }
            // The quadrants as on screen: upper left, upper right / lower left, lower right.
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    for (i, name) in [(2usize, "Upper Left"), (3, "Lower Left")] {
                        quadrant_button(ui, picks, i, name);
                    }
                });
                ui.vertical(|ui| {
                    for (i, name) in [(0usize, "Upper Right"), (1, "Lower Right")] {
                        quadrant_button(ui, picks, i, name);
                    }
                });
            });
            let q = picks.quadrant.min(3);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Label:").color(egui::Color32::from_rgb(255, 200, 80)));
                let editing = picks.quad_label.as_ref().is_some_and(|(s, i, _)| *s == set && *i == q);
                if !editing {
                    picks.quad_label = Some((set.clone(), q, custom.labels[q].clone()));
                }
                if let Some((_, _, text)) = picks.quad_label.as_mut() {
                    let r = ui.add(egui::TextEdit::singleline(text).desired_width(160.0).hint_text("program's title"));
                    if r.lost_focus() && *text != custom.labels[q] {
                        let _ = app.dispatch(Command::SetQuadLabel { set: set.clone(), quadrant: q as u32, label: text.clone() });
                    }
                }
            });
            ui.label("Entries (the program's, then yours; drop actions, separators and menus here):");
            let program = program_entries(app, &set, q);
            let hidden = custom.hidden[q].clone();
            let mut toggle_hidden: Option<(String, bool)> = None;
            let mut remove = None;
            let mut rename = None;
            let key = format!("quad:{set}:{q}");
            let dropped = drop_list(ui, "customize_quad_items", 180.0, |ui| {
                for label in &program {
                    let gone = hidden.contains(label);
                    let text = if gone { egui::RichText::new(format!("{label} (deleted)")).weak() } else { egui::RichText::new(label) };
                    let r = ui.add(egui::SelectableLabel::new(false, text));
                    r.context_menu(|ui| {
                        if !gone && ui.button("Delete Menu Item").clicked() {
                            toggle_hidden = Some((label.clone(), true));
                            ui.close_menu();
                        }
                        if gone && ui.button("Restore Menu Item").clicked() {
                            toggle_hidden = Some((label.clone(), false));
                            ui.close_menu();
                        }
                    });
                }
                ui.separator();
                added_entries(ui, &custom.items[q], &key, picks, &mut remove, &mut rename)
            });
            if let Some((label, hide)) = toggle_hidden {
                let _ = app.dispatch(Command::HideQuadItem { set: set.clone(), quadrant: q as u32, label, hidden: hide });
            }
            if let Some(i) = remove {
                let _ = app.dispatch(Command::RemoveQuadItem { set: set.clone(), quadrant: q as u32, index: i as u32 });
            }
            if let Some((i, name)) = rename {
                let _ = app.dispatch(Command::RenameQuadItem { set: set.clone(), quadrant: q as u32, index: i as u32, name });
            }
            if let Some((item, index)) = dropped {
                if let Err(e) = app.dispatch(Command::AddQuadItem { set: set.clone(), quadrant: q as u32, item, index: Some(index) }) {
                    app.state.status_prompt = e;
                }
            }
            if ui.button("Advanced Options...").clicked() {
                picks.advanced = true;
            }
            file_buttons(ui, app, Command::LoadMenus { path: None }, Command::SaveMenus { path: None }, Command::ResetMenus);
        });
    });
}

fn quadrant_button(ui: &mut egui::Ui, picks: &mut Picks, i: usize, name: &str) {
    let on = picks.quadrant == i;
    let text = if on { egui::RichText::new(name).color(egui::Color32::from_rgb(255, 200, 80)) } else { egui::RichText::new(name) };
    if ui.add(egui::Button::new(text).selected(on).min_size(egui::vec2(100.0, 0.0))).clicked() {
        picks.quadrant = i;
    }
}

/// Menus panel.
fn menus(ui: &mut egui::Ui, app: &mut LoomApp, picks: &mut Picks) {
    let names = menu_names(app);
    if !names.contains(&picks.menu) {
        picks.menu = names[0].clone();
    }
    let menu = picks.menu.clone();
    let builtin = ui_custom::BUILTIN_MENUS.contains(&menu.as_str());
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            let double = action_list(ui, 250.0);
            ui.label("Separator:");
            source_row(ui, "---- separator ----", SEPARATOR);
            ui.label("Menus:");
            egui::ScrollArea::vertical().id_salt("customize_menu_menus").max_height(80.0).show(ui, |ui| {
                for m in &names {
                    if *m != menu {
                        source_row(ui, m, &format!("{MENU_PREFIX}{m}"));
                    }
                }
            });
            if let Some(a) = double {
                let _ = app.dispatch(Command::AddMenuItem { menu: menu.clone(), item: a, index: None });
            }
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label("Menu:");
                egui::ComboBox::from_id_salt("customize_menu_list").selected_text(menu.clone()).width(180.0).show_ui(ui, |ui| {
                    for m in &names {
                        if ui.selectable_label(*m == menu, m).clicked() {
                            picks.menu = m.clone();
                        }
                    }
                });
            });
            if let Some(name) = name_field(ui, &mut picks.new_name, "New menu name", "New") {
                if app.dispatch(Command::NewMenu { name: name.clone() }).is_ok() {
                    picks.menu = name;
                }
            }
            if !builtin {
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        let _ = app.dispatch(Command::DeleteMenu { name: menu.clone() });
                    }
                    let mut on = app.state.ui_custom.menus.user_menu(&menu).is_some_and(|u| u.on_bar);
                    if ui.checkbox(&mut on, "On Menu Bar").changed() {
                        let _ = app.dispatch(Command::SetMenuOnBar { menu: menu.clone(), on });
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.label("Title (& marks the Alt key):");
                let editing = picks.menu_title.as_ref().is_some_and(|(m, _)| *m == menu);
                let title = app.state.ui_custom.menus.title(&menu);
                if !editing {
                    picks.menu_title = Some((menu.clone(), title.clone()));
                }
                if let Some((_, text)) = picks.menu_title.as_mut() {
                    let r = ui.add(egui::TextEdit::singleline(text).desired_width(140.0));
                    if r.lost_focus() && *text != title {
                        let _ = app.dispatch(Command::RenameMenu { menu: menu.clone(), title: text.clone() });
                    }
                }
            });
            ui.label("Entries (drop actions, separators and menus here):");
            let own = if builtin { crate::menubar::menu_labels(app, &menu) } else { Vec::new() };
            let added = if builtin { app.state.ui_custom.menus.extra.get(&menu).cloned().unwrap_or_default() } else { app.state.ui_custom.menus.user_menu(&menu).map(|u| u.items.clone()).unwrap_or_default() };
            let mut toggle_hidden: Option<(String, bool)> = None;
            let mut remove = None;
            let mut rename = None;
            let key = format!("menu:{menu}");
            let dropped = drop_list(ui, "customize_menu_items", 220.0, |ui| {
                for label in &own {
                    let gone = app.state.ui_custom.menus.is_hidden(&menu, label);
                    let text = if gone { egui::RichText::new(format!("{label} (deleted)")).weak() } else { egui::RichText::new(label.as_str()) };
                    let r = ui.add(egui::SelectableLabel::new(false, text));
                    r.context_menu(|ui| {
                        if !gone && ui.button("Delete Menu Item").clicked() {
                            toggle_hidden = Some((label.clone(), true));
                            ui.close_menu();
                        }
                        if gone && ui.button("Restore Menu Item").clicked() {
                            toggle_hidden = Some((label.clone(), false));
                            ui.close_menu();
                        }
                    });
                }
                if builtin {
                    ui.separator();
                }
                added_entries(ui, &added, &key, picks, &mut remove, &mut rename)
            });
            if let Some((label, hide)) = toggle_hidden {
                let _ = app.dispatch(Command::HideMenuItem { menu: menu.clone(), label, hidden: hide });
            }
            if let Some(i) = remove {
                let _ = app.dispatch(Command::RemoveMenuItem { menu: menu.clone(), index: i as u32 });
            }
            if let Some((i, name)) = rename {
                let _ = app.dispatch(Command::RenameMenuItem { menu: menu.clone(), index: i as u32, name });
            }
            if let Some((item, index)) = dropped {
                if let Err(e) = app.dispatch(Command::AddMenuItem { menu: menu.clone(), item, index: Some(index) }) {
                    app.state.status_prompt = e;
                }
            }
            file_buttons(ui, app, Command::LoadMenus { path: None }, Command::SaveMenus { path: None }, Command::ResetMenus);
        });
    });
}

/// Colors panel.
fn colors(ui: &mut egui::Ui, app: &mut LoomApp, picks: &mut Picks) {
    let categories: Vec<&str> = {
        let mut v: Vec<&str> = Vec::new();
        for (c, _, _) in ui_custom::COLOR_ELEMENTS {
            if !v.contains(c) {
                v.push(c);
            }
        }
        v
    };
    if !categories.contains(&picks.color_category.as_str()) {
        picks.color_category = categories[0].to_owned();
    }
    let elements: Vec<&str> = ui_custom::COLOR_ELEMENTS.iter().filter(|(c, _, _)| *c == picks.color_category).map(|(_, e, _)| *e).collect();
    if !elements.contains(&picks.color_element.as_str()) {
        picks.color_element = elements[0].to_owned();
    }
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(330.0);
            ui.horizontal(|ui| {
                ui.label("Elements:");
                egui::ComboBox::from_id_salt("customize_color_category").selected_text(picks.color_category.clone()).show_ui(ui, |ui| {
                    for c in &categories {
                        ui.selectable_value(&mut picks.color_category, (*c).to_owned(), *c);
                    }
                });
            });
            egui::ScrollArea::vertical().id_salt("customize_color_elements").max_height(220.0).show(ui, |ui| {
                for e in &elements {
                    let changed = app.state.ui_custom.colors.elements.contains_key(*e);
                    let text = if changed { format!("{e} *") } else { (*e).to_owned() };
                    if ui.selectable_label(picks.color_element == *e, text).clicked() {
                        picks.color_element = (*e).to_owned();
                    }
                }
            });
            let element = picks.color_element.clone();
            ui.horizontal(|ui| {
                ui.label("Color:");
                let mut rgb = app.state.ui_custom.colors.get(&element);
                let r = ui.color_edit_button_srgb(&mut rgb);
                crate::icons::named(&r, egui::WidgetType::ColorButton, &format!("{element} color"));
                if r.changed() {
                    let _ = app.dispatch(Command::SetUiColor { element: element.clone(), color: rgb });
                }
                if ui.button("Reset").on_hover_text("Back to the program's colour").clicked() {
                    if let Some(d) = ui_custom::default_color(&element) {
                        let _ = app.dispatch(Command::SetUiColor { element: element.clone(), color: d });
                    }
                }
            });
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label("Scheme:");
                let scheme = app.state.ui_custom.colors.scheme.clone();
                egui::ComboBox::from_id_salt("customize_color_scheme").selected_text(scheme.clone()).show_ui(ui, |ui| {
                    for s in ui_custom::SCHEMES {
                        if ui.selectable_label(scheme == s, s).clicked() {
                            let _ = app.dispatch(Command::SetColorScheme { scheme: s.into() });
                        }
                    }
                });
            });
            let custom = app.state.ui_custom.colors.scheme == "Custom Colors";
            ui.add_enabled_ui(custom, |ui| {
                ui.label("UI Appearance:");
                if !ui_custom::APPEARANCE_ELEMENTS.iter().any(|(n, _)| *n == picks.appearance) {
                    picks.appearance = ui_custom::APPEARANCE_ELEMENTS[0].0.to_owned();
                }
                egui::ScrollArea::vertical().id_salt("customize_color_appearance").max_height(200.0).show(ui, |ui| {
                    for (n, _) in ui_custom::APPEARANCE_ELEMENTS {
                        if ui.selectable_label(picks.appearance == *n, *n).clicked() {
                            picks.appearance = (*n).to_owned();
                        }
                    }
                });
                let element = picks.appearance.clone();
                ui.horizontal(|ui| {
                    ui.label("Color:");
                    let mut rgb = app.state.ui_custom.colors.appearance(&element);
                    let r = ui.color_edit_button_srgb(&mut rgb);
                    crate::icons::named(&r, egui::WidgetType::ColorButton, &format!("{element} color"));
                    if r.changed() {
                        let _ = app.dispatch(Command::SetUiColor { element: element.clone(), color: rgb });
                    }
                });
            });
        });
    });
    ui.add_space(8.0);
    file_buttons(ui, app, Command::LoadUiColors { path: None }, Command::SaveUiColors { path: None }, Command::ResetUiColors);
}

/// Advanced Quad Menu Options (Quads panel > Advanced Options).
fn advanced_quad_options(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let o = app.state.ui_custom.quad_options.clone();
    let set = |app: &mut LoomApp, name: &str, value: String| {
        let _ = app.dispatch(Command::SetQuadOption { name: name.into(), value });
    };
    let flag = |b: bool| if b { "on".to_owned() } else { "off".to_owned() };
    let q_id = egui::Id::new("quad_options_quadrant");
    let mut q: usize = ctx.data(|d| d.get_temp(q_id)).unwrap_or(0);
    egui::Window::new("Advanced Quad Menu Options").open(open).resizable(false).default_width(420.0).show(ctx, |ui| {
        ui.heading("Colors");
        ui.horizontal(|ui| {
            ui.label("Quadrant:");
            for (i, n) in ui_custom::QUADRANTS.iter().enumerate() {
                ui.selectable_value(&mut q, i, *n);
            }
        });
        egui::Grid::new("quad_colors").num_columns(3).show(ui, |ui| {
            for (element, _) in ui_custom::QUAD_COLOR_ELEMENTS {
                ui.label(element);
                let mut rgb = o.color(element, q);
                let r = ui.color_edit_button_srgb(&mut rgb);
                crate::icons::named(&r, egui::WidgetType::ColorButton, &format!("{element} color"));
                if r.changed() {
                    let _ = app.dispatch(Command::SetQuadColor { element: element.into(), quadrant: q as u32, color: Some(rgb), locked: None });
                }
                let locked = o.locked.get(element).copied().unwrap_or(true);
                if ui.add(egui::Button::new("L").selected(locked)).on_hover_text("Locked: the four quadrants share the colour").clicked() {
                    let _ = app.dispatch(Command::SetQuadColor { element: element.into(), quadrant: q as u32, color: None, locked: Some(!locked) });
                }
                ui.end_row();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Starting Quadrant:");
            egui::ComboBox::from_id_salt("quad_start").selected_text(ui_custom::QUADRANTS[o.starting_quadrant.min(3) as usize]).show_ui(ui, |ui| {
                for n in ui_custom::QUADRANTS {
                    if ui.selectable_label(ui_custom::QUADRANTS[o.starting_quadrant.min(3) as usize] == n, n).clicked() {
                        set(app, "Starting Quadrant", n.into());
                    }
                }
            });
        });
        ui.separator();
        ui.heading("Display");
        let mut b = o.uniform_width;
        if ui.checkbox(&mut b, "Uniform Quad Width").changed() {
            set(app, "Uniform Quad Width", flag(b));
        }
        let mut b = o.mirror;
        if ui.checkbox(&mut b, "Mirror Quads").changed() {
            set(app, "Mirror Quads", flag(b));
        }
        ui.horizontal(|ui| {
            ui.label("Vertical Margins:");
            let mut v = o.vertical_margins;
            if ui.add(crate::spinner::spin(ui, &mut v, 0.0..=20.0).speed(0.1)).changed() {
                set(app, "Vertical Margins", v.to_string());
            }
            ui.label("Opacity Amount:");
            let mut v = o.opacity;
            if ui.add(crate::spinner::spin(ui, &mut v, 10.0..=100.0).speed(1.0)).changed() {
                set(app, "Opacity Amount", v.to_string());
            }
        });
        ui.separator();
        ui.heading("Positioning");
        for (label, on) in [("Reposition Quad When Off Screen", o.reposition_off_screen), ("Move Cursor When Repositioned", o.move_cursor_when_repositioned), ("Return Cursor After Repositioned", o.return_cursor_after)] {
            let mut b = on;
            if ui.checkbox(&mut b, label).changed() {
                set(app, label, flag(b));
            }
        }
        ui.separator();
        ui.heading("Fonts");
        for (font_name, size_name, font, size) in [("Title Font", "Title Size", o.title_font.clone(), o.title_size), ("Menu Font", "Menu Size", o.menu_font.clone(), o.menu_size)] {
            ui.horizontal(|ui| {
                ui.label(format!("{font_name}:"));
                egui::ComboBox::from_id_salt(font_name).selected_text(font.clone()).show_ui(ui, |ui| {
                    for f in ["Proportional", "Monospace"] {
                        if ui.selectable_label(font == f, f).clicked() {
                            set(app, font_name, f.into());
                        }
                    }
                });
                ui.label("Size:");
                let mut v = size;
                if ui.add(crate::spinner::spin(ui, &mut v, 6.0..=32.0).speed(0.1)).changed() {
                    set(app, size_name, v.to_string());
                }
            });
        }
        ui.separator();
        ui.heading("Animation");
        ui.horizontal(|ui| {
            ui.label("Type:");
            for t in ["None", "Stretch", "Fade"] {
                if ui.radio(o.animation == t, t).clicked() {
                    set(app, "Animation Type", t.into());
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Steps:");
            let mut v = o.steps as f32;
            if ui.add(crate::spinner::spin(ui, &mut v, 1.0..=30.0).speed(0.1)).changed() {
                set(app, "Steps", (v.round() as u32).to_string());
            }
            ui.label("Pause (ms):");
            let mut v = o.pause as f32;
            if ui.add(crate::spinner::spin(ui, &mut v, 0.0..=500.0).speed(1.0)).changed() {
                set(app, "Pause", (v.round() as u32).to_string());
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Load...").clicked() {
                let _ = app.dispatch(Command::LoadQuadOptions { path: None });
            }
            if ui.button("Save...").clicked() {
                let _ = app.dispatch(Command::SaveQuadOptions { path: None });
            }
            if ui.button("Reset to Startup").clicked() {
                let _ = app.dispatch(Command::ResetQuadOptions);
            }
        });
    });
    ctx.data_mut(|d| d.insert_temp(q_id, q));
}

/// `keyboard-chart.txt` in the config dir.
fn chart_path() -> std::path::PathBuf {
    loom_commands::settings::config_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join("keyboard-chart.txt")
}
