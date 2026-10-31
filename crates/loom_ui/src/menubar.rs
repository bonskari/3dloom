//! The menu bar with the items that exist. Items of later
//! plans are added by those plans (no disabled placeholders).

use crate::LoomApp;
use eframe::egui;
use crate::shortcuts::menu_button;
use loom_commands::{Command, PrimitiveKind};

thread_local! {
    /// While set, the menu functions list their entries' labels here instead
    /// of hiding the ones the user deleted (the Menus panel shows them all).
    static RECORD: std::cell::RefCell<Option<Vec<String>>> = const { std::cell::RefCell::new(None) };
    /// While recording: the actions of the entries drawn (Customize User Interface > Category).
    static ACTIONS: std::cell::RefCell<Option<Vec<String>>> = const { std::cell::RefCell::new(None) };
    /// The menu bar menu being drawn (whose deleted entries are skipped).
    static CURRENT: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Notes an entry's label; false when the user deleted it from this menu (it is not drawn).
fn shown(app: &LoomApp, label: &str) -> bool {
    let plain = label.trim_start_matches(['\u{2714}', ' ']);
    if RECORD.with(|r| r.borrow_mut().as_mut().map(|v| v.push(plain.to_owned())).is_some()) {
        return true;
    }
    CURRENT.with(|c| !app.state.ui_custom.menus.is_hidden(&c.borrow(), plain))
}

fn record_action(action: &str) {
    ACTIONS.with(|r| {
        if let Some(v) = r.borrow_mut().as_mut() {
            v.push(action.to_owned());
        }
    });
}

fn item(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, action: &str) {
    if !shown(app, label) {
        return;
    }
    record_action(action);
    if menu_button(ui, &app.state, label, action).clicked() {
        app.dispatch_named(action);
        ui.close_menu();
    }
}

/// A toggle item: a check mark shows its state (as in the viewport label menus).
fn check_item(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, action: &str, on: bool) {
    let label = if on { format!("\u{2714} {label}") } else { format!("    {label}") };
    item(ui, app, &label, action);
}

fn cmd(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, c: Command) {
    if !shown(app, label) {
        return;
    }
    record_action(c.name());
    if menu_button(ui, &app.state, label, c.name()).clicked() {
        let _ = app.dispatch(c);
        ui.close_menu();
    }
}

/// A plain entry with no action of its own; true when clicked (the menu closes).
fn entry(ui: &mut egui::Ui, app: &LoomApp, label: &str) -> bool {
    if !shown(app, label) {
        return false;
    }
    let clicked = ui.button(label).clicked();
    if clicked {
        ui.close_menu();
    }
    clicked
}

/// A submenu entry.
fn sub(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, body: impl FnOnce(&mut egui::Ui, &mut LoomApp)) {
    if !shown(app, label) {
        return;
    }
    ui.menu_button(label, |ui| body(ui, app));
}

/// The labels of menu `name`'s own entries (submenus by their names), as the Menus panel lists them.
pub fn menu_labels(app: &mut LoomApp, name: &str) -> Vec<String> {
    record(app, name).0
}

/// The actions built-in menu `name` runs (submenus included).
pub fn menu_actions(app: &mut LoomApp, name: &str) -> Vec<String> {
    record(app, name).1
}

fn record(app: &mut LoomApp, name: &str) -> (Vec<String>, Vec<String>) {
    RECORD.with(|r| *r.borrow_mut() = Some(Vec::new()));
    ACTIONS.with(|r| *r.borrow_mut() = Some(Vec::new()));
    let ctx = egui::Context::default();
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| builtin_menu(ui, app, name));
    });
    (RECORD.with(|r| r.borrow_mut().take()).unwrap_or_default(), ACTIONS.with(|r| r.borrow_mut().take()).unwrap_or_default())
}

/// A title as a menu bar label: the accelerator letter underlined while Alt is held or the keyboard opened the bar.
fn title_text(ui: &egui::Ui, title: &str) -> egui::WidgetText {
    let (text, key) = loom_commands::ui_custom::accelerator(title);
    let underline = ui.input(|i| i.modifiers.alt) || ui.ctx().data(|d| d.get_temp::<bool>(egui::Id::new(KEYBOARD_ID))).unwrap_or(false);
    let Some(key) = key.filter(|_| underline) else { return text.into() };
    let at = text.char_indices().find(|(_, c)| c.to_ascii_lowercase() == key).map(|(i, _)| i).unwrap_or(0);
    let mut job = egui::text::LayoutJob::default();
    let font = egui::TextStyle::Button.resolve(ui.style());
    let color = ui.visuals().widgets.inactive.text_color();
    let plain = egui::TextFormat { font_id: font.clone(), color, ..Default::default() };
    let marked = egui::TextFormat { underline: egui::Stroke::new(1.0_f32, color), ..plain.clone() };
    let ch_len = text[at..].chars().next().map(char::len_utf8).unwrap_or(0);
    job.append(&text[..at], 0.0, plain.clone());
    job.append(&text[at..at + ch_len], 0.0, marked);
    job.append(&text[at + ch_len..], 0.0, plain);
    job.into()
}

/// Memory: the Alt key put the menu bar into keyboard mode (the next letter opens a menu).
const KEYBOARD_ID: &str = "menubar_keyboard";
/// Memory: the menu a keyboard accelerator opens next frame.
const OPEN_ID: &str = "menubar_open";

/// A menu bar menu: its title (the user's), its own entries without the
/// ones the user deleted, then the entries the user added.
fn menu(ui: &mut egui::Ui, app: &mut LoomApp, name: &str) {
    let title = app.state.ui_custom.menus.title(name);
    let text = title_text(ui, &title);
    let bar_id = ui.id();
    let r = ui.menu_button(text, |ui| {
        let previous = CURRENT.with(|c| std::mem::replace(&mut *c.borrow_mut(), name.to_owned()));
        if loom_commands::ui_custom::BUILTIN_MENUS.contains(&name) {
            builtin_menu(ui, app, name);
            let extra = app.state.ui_custom.menus.extra.get(name).cloned().unwrap_or_default();
            if !extra.is_empty() {
                ui.separator();
            }
            custom_items(ui, app, &extra);
        } else if let Some(m) = app.state.ui_custom.menus.user_menu(name).cloned() {
            custom_items(ui, app, &m.items);
        }
        CURRENT.with(|c| *c.borrow_mut() = previous);
    });
    // An accelerator pressed last frame opens this menu now.
    let open: Option<String> = ui.ctx().data(|d| d.get_temp(egui::Id::new(OPEN_ID)));
    if open.as_deref() == Some(name) {
        ui.ctx().data_mut(|d| d.remove::<String>(egui::Id::new(OPEN_ID)));
        let mut bar = egui::menu::BarState::load(ui.ctx(), bar_id);
        **bar = Some(egui::menu::MenuRoot::new(r.response.rect.left_bottom(), r.response.id));
        bar.store(ui.ctx(), bar_id);
        ui.ctx().request_repaint();
    }
}

/// Entries the user added to a menu (or a quad): actions, separators and menus.
pub(crate) fn custom_items(ui: &mut egui::Ui, app: &mut LoomApp, items: &[loom_commands::ui_custom::CustomItem]) {
    for it in items {
        if it.is_separator() {
            ui.separator();
        } else if let Some(m) = it.menu() {
            let title = if it.name.is_empty() { app.state.ui_custom.menus.title(m) } else { it.name.clone() };
            let m = m.to_owned();
            ui.menu_button(loom_commands::ui_custom::accelerator(&title).0, |ui| {
                if loom_commands::ui_custom::BUILTIN_MENUS.contains(&m.as_str()) {
                    let previous = CURRENT.with(|c| std::mem::replace(&mut *c.borrow_mut(), m.clone()));
                    builtin_menu(ui, app, &m);
                    CURRENT.with(|c| *c.borrow_mut() = previous);
                } else if let Some(u) = app.state.ui_custom.menus.user_menu(&m).cloned() {
                    custom_items(ui, app, &u.items);
                }
            });
        } else {
            let (label, key) = loom_commands::ui_custom::accelerator(it.label());
            // The accelerator of an entry the user named runs it while its menu is open.
            let pressed = key.is_some_and(|k| menu_key(ui, k));
            if menu_button(ui, &app.state, &label, &it.item).clicked() || pressed {
                app.dispatch_named(&it.item);
                ui.close_menu();
            }
        }
    }
}

/// The menu bar's menus in order: the program's, with the user's menus that are on the bar before Help.
fn bar_menus(app: &LoomApp) -> Vec<String> {
    let mut v: Vec<String> = loom_commands::ui_custom::BUILTIN_MENUS.iter().filter(|m| **m != "Help").map(|m| (*m).to_owned()).collect();
    v.extend(app.state.ui_custom.menus.user.iter().filter(|u| u.on_bar).map(|u| u.name.clone()));
    v.push("Help".into());
    v
}

/// Alt pressed and released alone puts the bar into keyboard mode; then (or
/// with Alt held, when that chord has no shortcut) a menu's accelerator
/// letter opens it. Esc or a click leaves keyboard mode. While a menu is
/// open its letters go to the menu, not to the shortcuts. Runs before the
/// shortcuts so the keys it takes never reach them.
pub(crate) fn keyboard(ctx: &egui::Context, app: &LoomApp) {
    let kb_id = egui::Id::new(KEYBOARD_ID);
    let alone_id = egui::Id::new("menubar_alt_alone");
    let mut keyboard: bool = ctx.data(|d| d.get_temp(kb_id)).unwrap_or(false);
    let mut alone: bool = ctx.data(|d| d.get_temp(alone_id)).unwrap_or(false);
    let menu_open = ctx.data(|d| d.get_temp::<egui::Id>(egui::Id::new(BAR_ID))).is_some_and(|bar| egui::menu::BarState::load(ctx, bar).is_some());
    let (alt, events, pointer) = ctx.input(|i| (i.modifiers.alt, i.events.clone(), i.pointer.any_pressed() || i.pointer.any_down() || i.raw_scroll_delta != egui::Vec2::ZERO));
    let mut open = None;
    let mut taken = Vec::new();
    let mut menu_keys = Vec::new();
    if !ctx.wants_keyboard_input() {
        for e in &events {
            let egui::Event::Key { key, pressed: true, modifiers, .. } = e else { continue };
            if *key == egui::Key::Escape {
                keyboard = false;
                continue;
            }
            alone = false;
            let name = key.name();
            let letter = name.chars().next().filter(|_| name.chars().count() == 1).map(|c| c.to_ascii_lowercase());
            let Some(letter) = letter else { continue };
            if menu_open && !modifiers.any() {
                menu_keys.push(letter);
                taken.push((*modifiers, *key));
                continue;
            }
            let with_alt = modifiers.alt && !modifiers.ctrl && !modifiers.shift;
            let bound = loom_commands::parse_chord(&format!("Alt+{}", letter.to_ascii_uppercase())).ok().and_then(|c| app.state.hotkey(c)).is_some();
            if (keyboard && !modifiers.any()) || (with_alt && !bound) {
                open = bar_menus(app).into_iter().find(|m| loom_commands::ui_custom::accelerator(&app.state.ui_custom.menus.title(m)).1 == Some(letter));
                if keyboard || open.is_some() {
                    taken.push((*modifiers, *key));
                }
                keyboard = false;
            }
        }
    }
    for (m, k) in taken {
        ctx.input_mut(|i| i.consume_key(m, k));
    }
    // Alt went down and up with nothing else pressed between.
    let was_alt: bool = ctx.data(|d| d.get_temp(egui::Id::new("menubar_alt_down"))).unwrap_or(false);
    if alt && !was_alt {
        alone = true;
    }
    if pointer {
        alone = false;
        if ctx.input(|i| i.pointer.any_pressed()) {
            keyboard = false;
        }
    }
    if !alt && was_alt && alone {
        keyboard = !keyboard;
        alone = false;
    }
    ctx.data_mut(|d| {
        d.insert_temp(egui::Id::new("menubar_alt_down"), alt);
        d.insert_temp(kb_id, keyboard);
        d.insert_temp(alone_id, alone);
        d.insert_temp(egui::Id::new(MENU_KEYS_ID), menu_keys);
        if let Some(m) = open {
            d.insert_temp(egui::Id::new(OPEN_ID), m);
        }
    });
    if open_pending(ctx) {
        ctx.request_repaint();
    }
}

/// Memory: the menu bar's id (to tell whether one of its menus is open).
const BAR_ID: &str = "menubar_bar_id";
/// Memory: letters pressed this frame while a menu was open (the accelerators of its entries).
const MENU_KEYS_ID: &str = "menubar_menu_keys";

/// Was accelerator `letter` pressed this frame while a menu is open?
fn menu_key(ui: &egui::Ui, letter: char) -> bool {
    ui.ctx().data(|d| d.get_temp::<Vec<char>>(egui::Id::new(MENU_KEYS_ID))).is_some_and(|v| v.contains(&letter))
}

fn open_pending(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<String>(egui::Id::new(OPEN_ID))).is_some()
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp) {
    if app.state.expert_mode {
        return;
    }
    let below = app.state.ui_custom.quick_access_below && app.state.show_ribbon;
    let hidden = app.state.ui_custom.menu_bar_hidden;
    if hidden && below {
        // Nothing left on the bar: the Quick Access toolbar keeps a way back.
        return;
    }
    egui::TopBottomPanel::top("menubar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(BAR_ID), ui.id()));
            if !below {
                quick_access(ui, app);
                ui.separator();
            }
            // Hide Menu Bar: only the Quick Access toolbar stays.
            for m in bar_menus(app).into_iter().filter(|_| !hidden) {
                menu(ui, app, &m);
            }
            if let Some(handle) = app.update.clone() {
                crate::update_menu::show(ui, &handle);
            }
        });
    });
}

/// The menu bar menu `name`'s own entries (what `menu` shows before the user's added entries).
pub(crate) fn builtin_menu(ui: &mut egui::Ui, app: &mut LoomApp, name: &str) {
    match name {
        "File" => file_menu(ui, app),
        "Edit" => edit_menu(ui, app),
        "Tools" => tools_menu(ui, app),
        "Group" => group_menu(ui, app),
        "Views" => views_menu(ui, app),
        "Create" => create_menu(ui, app),
        "Modifiers" => modifiers_menu(ui, app),
        "Animation" => animation_menu(ui, app),
        "Graph Editors" => graph_editors_menu(ui, app),
        "Rendering" => rendering_menu(ui, app),
        "Customize" => customize_menu(ui, app),
        "Help" => help_menu(ui, app),
        _ => {}
    }
}

fn file_menu(ui: &mut egui::Ui, app: &mut LoomApp) {
    use loom_commands::PendingFileAction as A;
    if menu_button(ui, &app.state, "New", "New Scene").clicked() {
        app.new_scene();
        ui.close_menu();
    }
    if entry(ui, app, "Reset") {
        app.request_file_action(A::Reset);
        ui.close_menu();
    }
    ui.separator();
    if menu_button(ui, &app.state, "Open...", "Open File").clicked() {
        app.request_file_action(A::Open);
        ui.close_menu();
    }
    sub(ui, app, "Recent", |ui, app| {
        if app.state.recent_files.is_empty() {
            ui.label("(none)");
        }
        for (i, path) in app.state.recent_files.clone().iter().enumerate() {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if ui.button(format!("{} {name}", i + 1)).on_hover_text(path.display().to_string()).clicked() {
                app.request_file_action(A::OpenRecent(i));
                ui.close_menu();
            }
        }
    });
    item(ui, app, "Save", "Save File");
    item(ui, app, "Save As...", "Save File As");
    item(ui, app, "Save Copy As...", "Save File As Copy");
    item(ui, app, "Save Incremental", "Save Incremental (+)");
    cmd(ui, app, "Save Selected...", Command::SaveSelected { path: None });
    cmd(ui, app, "Set Project Folder...", Command::SetProjectFolder { path: None });
    cmd(ui, app, "Archive...", Command::ArchiveFile { path: None });
    ui.separator();
    item(ui, app, "Merge...", "Merge File");
    cmd(ui, app, "Replace...", Command::ReplaceFile { path: None, names: None, materials: None });
    sub(ui, app, "References", |ui, app| {
        cmd(ui, app, "XRef Objects...", Command::XrefObjects { path: None, names: None, automatic: None });
        cmd(ui, app, "XRef Scene...", Command::XrefScene { path: None, automatic: None });
        cmd(ui, app, "Update XRefs", Command::UpdateXrefs { path: None });
    });
    item(ui, app, "Import...", "Import File");
    item(ui, app, "Export...", "Export File");
    item(ui, app, "Export Selected...", "Export Selected");
    ui.separator();
    cmd(ui, app, "Asset Tracking...", Command::AssetTracking);
    ui.separator();
    cmd(ui, app, "Summary Info...", Command::SummaryInfo);
    cmd(ui, app, "File Properties...", Command::FileProperties);
    ui.separator();
    if entry(ui, app, "Exit") {
        app.request_file_action(A::Exit);
        ui.close_menu();
    }
}

fn edit_menu(ui: &mut egui::Ui, app: &mut LoomApp) {
    let undo = app.state.undo.labels().last().cloned().unwrap_or_default();
    item(ui, app, &format!("Undo {undo}"), "Undo Scene Operation");
    item(ui, app, "Redo", "Redo Scene Operation");
    ui.separator();
    item(ui, app, "Hold", "Hold");
    item(ui, app, "Fetch", "Fetch");
    ui.separator();
    item(ui, app, "Delete", "Delete Objects");
    item(ui, app, "Clone", "Clone");
    ui.separator();
    item(ui, app, "Move", "Select and Move");
    item(ui, app, "Rotate", "Select and Rotate");
    item(ui, app, "Scale", "Select and Scale");
    item(ui, app, "Transform Type-In...", "Transform Type-In Dialog Toggle");
    ui.separator();
    item(ui, app, "Select All", "Select All");
    item(ui, app, "Select None", "Select None");
    item(ui, app, "Select Invert", "Select Invert");
    ui.separator();
    item(ui, app, "Select Similar", "Select Similar");
    // Unavailable unless one object with instances is selected, as in the reference.
    let instances = has_instances(&app.state);
    ui.add_enabled_ui(instances, |ui| item(ui, app, "Select Instances", "Select Instances"));
    sub(ui, app, "Select By", |ui, app| {
        item(ui, app, "Color", "Select By Color Pick");
        item(ui, app, "Name...", "Select By Name");
        cmd(ui, app, "Layer...", Command::SelectByLayer { layers: None });
    });
    sub(ui, app, "Region", |ui, app| {
        let window = app.state.window_selection;
        for (label, on) in [("Window", window), ("Crossing", !window)] {
            let label = if on { format!("✔ {label}") } else { format!("    {label}") };
            if ui.button(label).clicked() {
                if !on {
                    let _ = app.dispatch(Command::WindowCrossingSelectionToggle);
                }
                ui.close_menu();
            }
        }
        ui.separator();
        for shape in loom_commands::SelectionRegion::ALL {
            // "Rectangular Region" … "Paint Selection Region", as the reference's menu names them.
            let name = match shape {
                loom_commands::SelectionRegion::Paint => shape.label().to_owned(),
                _ => shape.label().replace(" Selection Region", " Region"),
            };
            let label = if app.state.selection_region == shape { format!("✔ {name}") } else { format!("    {name}") };
            if ui.button(label).clicked() {
                let _ = app.dispatch(Command::SelectionRegion { shape });
                ui.close_menu();
            }
        }
    });
    ui.separator();
    cmd(ui, app, "Edit Named Selection Sets...", Command::EditNamedSelectionSets);
    ui.separator();
    item(ui, app, "Object Properties...", "Object Properties");
}

fn tools_menu(ui: &mut egui::Ui, app: &mut LoomApp) {
    // The reference's order; its items Loom has no tool for are left out.
    item(ui, app, "Isolate Selection", "Isolate Selection");
    cmd(ui, app, "Manage Layers...", Command::LayerManager);
    ui.separator();
    cmd(ui, app, "Display Floater...", Command::DisplayFloater);
    cmd(ui, app, "Light Lister...", Command::LightLister);
    ui.separator();
    item(ui, app, "Mirror...", "Mirror");
    cmd(ui, app, "Array...", Command::Array);
    sub(ui, app, "Align", |ui, app| {
        item(ui, app, "Align...", "Align");
        item(ui, app, "Quick Align", "Quick Align");
        item(ui, app, "Spacing Tool...", "Spacing Tool");
        cmd(ui, app, "Clone and Align...", Command::CloneAndAlignDialog);
        ui.separator();
        item(ui, app, "Align to View...", "Align to View Dialog");
        item(ui, app, "Normal Align...", "Normal Align ");
        item(ui, app, "Align Camera", "Align Camera");
        cmd(ui, app, "Place Highlight", Command::PlaceHighlight);
    });
    cmd(ui, app, "Snapshot...", Command::SnapshotDialog);
    cmd(ui, app, "Rename Objects...", Command::RenameObjectsDialog);
    cmd(ui, app, "Manage Scene States...", Command::ManageSceneStates);
    sub(ui, app, "Preview - Grab Viewport", |ui, app| {
        item(ui, app, "Create Preview Animation...", "Create Animated Sequence File...");
        cmd(ui, app, "Capture Still Image...", Command::GrabViewport);
    });
    cmd(ui, app, "Grab Viewport", Command::GrabViewport);
    ui.separator();
    sub(ui, app, "Grids and Snaps", |ui, app| {
        cmd(ui, app, "Grid and Snap Settings...", Command::GridAndSnapSettings);
        ui.separator();
        let grid = app.state.active().show_grid;
        check_item(ui, app, "Show Home Grid", "Hide Grids Toggle", grid);
        cmd(ui, app, "Activate Home Grid", Command::ActivateHomeGrid);
        let grid_selected = app.state.selection.iter().any(|id| app.state.scene.node(*id).is_some_and(|n| matches!(n.object, loom_scene::ObjectKind::Grid { .. })));
        if ui.add_enabled(grid_selected, egui::Button::new("Activate Grid Object")).clicked() {
            let _ = app.dispatch(Command::ActivateGridObject);
            ui.close_menu();
        }
        if ui.add_enabled(app.state.active_grid().is_some(), egui::Button::new("Align Grid to View")).clicked() {
            let _ = app.dispatch(Command::AlignGridToView);
            ui.close_menu();
        }
        ui.separator();
        check_item(ui, app, "Snaps Toggle", "Snaps Toggle", app.state.snaps_on);
        check_item(ui, app, "Angle Snap Toggle", "Angle Snap Toggle", app.state.angle_snap_on);
        check_item(ui, app, "Percent Snap Toggle", "Percent Snap Toggle", app.state.percent_snap_on);
        check_item(ui, app, "Spinner Snap Toggle", "Spinner Snap Toggle", app.state.prefs.spinner_snap);
        let axis = app.state.snaps.use_axis_constraints;
        check_item(ui, app, "Snaps Use Axis Constraints", Command::EnableAxisConstraintsInSnapsToggle.name(), axis);
    });
    cmd(ui, app, "Measure Distance", Command::MeasureDistance);
    ui.separator();
    cmd(ui, app, "New Scene Explorer", Command::NewSceneExplorer);
    cmd(ui, app, "Manage Scene Explorer...", Command::ManageSceneExplorer);
    let explorers: Vec<String> = app.state.explorers.list.iter().map(|e| e.name.clone()).collect();
    if !explorers.is_empty() {
        sub(ui, app, "Saved Scene Explorers", |ui, app| {
            for n in &explorers {
                if ui.button(n).clicked() {
                    let _ = app.dispatch(Command::OpenSceneExplorer { name: Some(n.clone()) });
                    ui.close_menu();
                }
            }
        });
    }
    if let Some(last) = app.state.explorers.get(None).map(|e| e.name.clone()) {
        cmd(ui, app, &format!("Open Explorer: {last}"), Command::OpenSceneExplorer { name: None });
    }
    ui.separator();
    let ribbon = app.state.show_ribbon;
    check_item(ui, app, "Graphite Modeling Tools", "Show Ribbon Toggle", ribbon);
}

fn group_menu(ui: &mut egui::Ui, app: &mut LoomApp) {
    if menu_button(ui, &app.state, "Group...", "Group").clicked() {
        // The reference asks for the group name first.
        if app.state.selection.is_empty() {
            app.state.status_prompt = "select objects to group".into();
        } else {
            app.state.dialog = Some(loom_commands::Dialog::GroupName);
        }
        ui.close_menu();
    }
    cmd(ui, app, "Ungroup", Command::Ungroup);
    ui.separator();
    cmd(ui, app, "Open", Command::GroupOpen);
    cmd(ui, app, "Close", Command::GroupClose);
    ui.separator();
    cmd(ui, app, "Attach", Command::GroupAttach);
    cmd(ui, app, "Detach", Command::GroupDetach);
    ui.separator();
    cmd(ui, app, "Explode", Command::GroupExplode);
    ui.separator();
    ui.add_enabled(false, egui::Button::new("Assembly")).on_disabled_hover_text("Assemblies (luminaires) are out of scope");
}

fn views_menu(ui: &mut egui::Ui, app: &mut LoomApp) {
    let (undo_view, redo_view) = (app.state.active().view_undo.undo_label().unwrap_or("Change").to_owned(), app.state.active().view_undo.redo_label().unwrap_or("Change").to_owned());
    item(ui, app, &format!("Undo View {undo_view}"), "Undo Viewport Operation");
    item(ui, app, &format!("Redo View {redo_view}"), "Redo Viewport Operation");
    ui.separator();
    // The active view's name is part of the item, as in the reference.
    let view = app.state.active().kind.label();
    item(ui, app, &format!("Save Active {view} View"), "Save Active View");
    item(ui, app, &format!("Restore Active {view} View"), "Restore Active View");
    ui.separator();
    item(ui, app, "Viewport Configuration...", "Configure Viewports");
    item(ui, app, "Viewport Background...", "Viewport Background");
    item(ui, app, "Update Background Image", "Update Background Image");
    item(ui, app, "Reset Background Transform", "Reset Background Transform");
    item(ui, app, "Show Floating Dialogs", "Show Floating Dialogs");
    ui.separator();
    let (gizmo, shade) = (app.state.prefs.gizmos.enabled, app.state.shade_selected);
    check_item(ui, app, "Show Transform Gizmo", "Transform Gizmo Toggle", gizmo);
    check_item(ui, app, "Shade Selected", "Shade Selected", shade);
    let degrade = app.state.degradation.on;
    check_item(ui, app, "Adaptive Degradation", "Adaptive Degradation Toggle", degrade);
    let spinner = app.state.update_during_spinner_drag;
    check_item(ui, app, "Update During Spinner Drag", "Update During Spinner Drag", spinner);
    item(ui, app, "Create Camera From View", "Create Camera From View");
    item(ui, app, "Add Default Lights to Scene...", "Add Default Lights to Scene");
    item(ui, app, "Redraw All Views", "Redraw All Views");
    ui.separator();
    sub(ui, app, "Set Active Viewport", |ui, app| {
        for (label, action) in [
            ("Top", "Top View"), ("Bottom", "Bottom View"), ("Front", "Front View"), ("Back", "Back View"),
            ("Left", "Left View"), ("Right", "Right View"), ("Perspective", "Perspective User View"), ("Orthographic", "Orthographic User View"),
        ] {
            item(ui, app, label, action);
        }
    });
    ui.separator();
    item(ui, app, "Zoom Extents", "Zoom Extents");
    item(ui, app, "Zoom Extents All", "Zoom Extents All");
    item(ui, app, "Zoom Extents Selected", "Zoom Extents Selected");
    ui.separator();
    let vp = app.state.active();
    let (grid, edged, safe) = (vp.show_grid, vp.edged_faces, vp.safe_frames);
    check_item(ui, app, "Show Grid", "Hide Grids Toggle", grid);
    check_item(ui, app, "Show Statistics", "Show Statistics Toggle", app.state.show_statistics);
    check_item(ui, app, "Edged Faces", "View Edged Faces Toggle", edged);
    item(ui, app, "Wireframe / Smooth+Highlights", "Wireframe / Smooth+Highlights Toggle");
    check_item(ui, app, "Show Safe Frames", "Viewport Safeframes Toggle", safe);
    ui.separator();
    check_item(ui, app, "Expert Mode", "Expert Mode Toggle", app.state.expert_mode);
}

fn create_menu(ui: &mut egui::Ui, app: &mut LoomApp) {
    sub(ui, app, "Standard Primitives", |ui, app| {
        for kind in PrimitiveKind::ALL {
            if ui.button(kind.label()).clicked() {
                app.create_kind = Some(crate::CreateKind::Primitive(kind));
                app.create_category = crate::Category::Geometry;
                app.panel_tab = crate::Tab::Create;
                ui.close_menu();
            }
        }
    });
    sub(ui, app, "Lights", |ui, app| {
        ui.menu_button("Photometric Lights", |ui| {
            for kind in loom_commands::light::LightKind::PHOTOMETRIC {
                if ui.button(kind.label()).clicked() {
                    let _ = app.dispatch(Command::SelectObject);
                    app.photometric_lights = true;
                    app.create_kind = Some(crate::CreateKind::Light(kind));
                    app.create_category = crate::Category::Lights;
                    app.panel_tab = crate::Tab::Create;
                    ui.close_menu();
                }
            }
        });
        ui.menu_button("Standard Lights", |ui| {
            for (kind, label) in [
                (loom_commands::light::LightKind::TargetSpot, "Target Spotlight"), (loom_commands::light::LightKind::FreeSpot, "Free Spotlight"),
                (loom_commands::light::LightKind::TargetDirect, "Target Directional"), (loom_commands::light::LightKind::FreeDirect, "Directional"),
                (loom_commands::light::LightKind::Omni, "Omni"), (loom_commands::light::LightKind::Skylight, "Skylight"),
            ] {
                if ui.button(label).clicked() {
                    let _ = app.dispatch(Command::SelectObject);
                    app.photometric_lights = false;
                    app.create_kind = Some(crate::CreateKind::Light(kind));
                    app.create_category = crate::Category::Lights;
                    app.panel_tab = crate::Tab::Create;
                    ui.close_menu();
                }
            }
        });
        ui.separator();
        for (label, daylight) in [("Daylight System", true), ("Sunlight System", false)] {
            if ui.button(label).clicked() {
                app.create_category = crate::Category::Systems;
                app.panel_tab = crate::Tab::Create;
                app.start_create(Some(crate::CreateKind::SunSystem { daylight }));
                ui.close_menu();
            }
        }
    });
    sub(ui, app, "Helpers", |ui, app| {
        for (kind, label) in [(crate::HelperKind::Dummy, "Dummy"), (crate::HelperKind::Point, "Point"), (crate::HelperKind::Grid, "Grid"), (crate::HelperKind::Tape, "Tape Measure"), (crate::HelperKind::Protractor, "Protractor"), (crate::HelperKind::Compass, "Compass")] {
            if ui.button(label).clicked() {
                app.create_category = crate::Category::Helpers;
                app.panel_tab = crate::Tab::Create;
                app.start_create(Some(crate::CreateKind::Helper(kind)));
                ui.close_menu();
            }
        }
    });
    sub(ui, app, "Cameras", |ui, app| {
        // Turns on the creation tool: drag (Target) or click (Free) in a viewport.
        for (label, target) in [("Target Camera", true), ("Free Camera", false)] {
            if ui.button(label).clicked() {
                app.create_kind = Some(crate::CreateKind::Camera { target });
                app.create_category = crate::Category::Cameras;
                app.panel_tab = crate::Tab::Create;
                ui.close_menu();
            }
        }
        ui.separator();
        cmd(ui, app, "Create Camera From View", Command::CreateCameraFromView);
    });
    sub(ui, app, "Shapes", |ui, app| {
        let kinds = std::iter::once((crate::CreateKind::Line, "Line")).chain(loom_commands::ShapeKind::ALL.iter().map(|k| (crate::CreateKind::Shape(*k), k.label())));
        for (kind, label) in kinds {
            if ui.button(label).clicked() {
                if let Some(line) = app.line.take() {
                    line.finish(app);
                }
                app.create_category = crate::Category::Shapes;
                app.panel_tab = crate::Tab::Create;
                app.start_create(Some(kind));
                ui.close_menu();
            }
        }
    });
    sub(ui, app, "Compound", |ui, app| {
        for kind in crate::command_panel::compound::CompoundKind::ALL {
            if ui.button(kind.label()).clicked() {
                app.create_category = crate::Category::Geometry;
                app.create_compound = true;
                app.compound_kind = Some(kind);
                app.create_kind = None;
                app.panel_tab = crate::Tab::Create;
                let _ = app.dispatch(Command::SelectObject);
                ui.close_menu();
            }
        }
    });
    ui.separator();
    let mut keep = app.state.prefs.keep_creating;
    if ui.checkbox(&mut keep, "Keep Creating").changed() {
        let _ = app.dispatch(Command::KeepCreatingToggle);
        ui.close_menu();
    }
    ui.separator();
    item(ui, app, "Test Room", "Test Room");
}

fn modifiers_menu(ui: &mut egui::Ui, app: &mut LoomApp) {
    let has = app.has_selection();
    for (category, entries) in MODIFIERS_MENU {
        sub(ui, app, category, |ui, app| {
            for (label, name) in *entries {
                if ui.add_enabled(has, egui::Button::new(*label)).clicked() {
                    if let Err(e) = app.dispatch(Command::AddModifier { name: (*name).into() }) {
                        app.state.status_prompt = e;
                    }
                    ui.close_menu();
                }
            }
        });
    }
}

fn animation_menu(ui: &mut egui::Ui, app: &mut LoomApp) {
    cmd(ui, app, "Load Animation...", Command::LoadAnimation { path: None, offset: None });
    cmd(ui, app, "Save Animation...", Command::SaveAnimation { path: None });
    ui.separator();
    sub(ui, app, "Constraints", |ui, app| {
        for (label, kind) in [("Attachment Constraint", "Attachment Constraint"), ("Path Constraint", "Path Constraint"), ("Position Constraint", "Position Constraint"), ("Link Constraint", "Link Constraint"), ("LookAt Constraint", "LookAt Constraint"), ("Orientation Constraint", "Orientation Constraint")] {
            cmd(ui, app, label, Command::StartConstraintPick { kind: kind.into() });
        }
    });
    sub(ui, app, "Position Controllers", |ui, app| {
        for c in ["Position XYZ", "Bezier Position", "TCB Position", "Linear Position", "Noise Position", "Position List"] {
            controller_item(ui, app, "Position", c);
        }
    });
    sub(ui, app, "Rotation Controllers", |ui, app| {
        for c in ["Euler XYZ", "Bezier Rotation", "TCB Rotation", "Linear Rotation", "Noise Rotation", "Rotation List"] {
            controller_item(ui, app, "Rotation", c);
        }
    });
    sub(ui, app, "Scale Controllers", |ui, app| {
        for c in ["ScaleXYZ", "Bezier Scale", "TCB Scale", "Linear Scale", "Noise Scale", "Scale List"] {
            controller_item(ui, app, "Scale", c);
        }
    });
    ui.separator();
    cmd(ui, app, "Delete Selected Animation", Command::DeleteSelectedAnimation);
    ui.separator();
    sub(ui, app, "IK Solvers", |ui, app| {
        item(ui, app, "HI Solver", "History-Independent IK Solver");
        item(ui, app, "HD Solver", "History-Dependent IK Solver");
        item(ui, app, "IK Limb Solver", "IK Limb Solver");
        item(ui, app, "Spline IK Solver", "Spline IK Solver");
    });
    ui.separator();
    item(ui, app, "Bone Tools...", "Bone Tools...");
    ui.separator();
    item(ui, app, "Display Bones", "Display Bones");
    item(ui, app, "Display Bones and Objects", "Display Bones and Objects");
}

fn graph_editors_menu(ui: &mut egui::Ui, app: &mut LoomApp) {
    cmd(ui, app, "Track View - Curve Editor...", Command::OpenCurveEditor);
    cmd(ui, app, "Track View - Dope Sheet...", Command::OpenDopeSheet);
    ui.separator();
    cmd(ui, app, "New Schematic View...", Command::NewSchematicView);
    let views: Vec<String> = app.state.schematics.list.iter().map(|v| v.name.clone()).collect();
    if !views.is_empty() {
        sub(ui, app, "Delete Schematic View", |ui, app| {
            for n in &views {
                if ui.button(n).clicked() {
                    let _ = app.dispatch(Command::DeleteSchematicView { name: n.clone() });
                    ui.close_menu();
                }
            }
        });
        sub(ui, app, "Saved Schematic Views", |ui, app| {
            for n in &views {
                if ui.button(n).clicked() {
                    let _ = app.dispatch(Command::OpenSchematicView { name: Some(n.clone()) });
                    ui.close_menu();
                }
            }
        });
    }
}

fn rendering_menu(ui: &mut egui::Ui, app: &mut LoomApp) {
    cmd(ui, app, "Render", Command::Render { path: None, width: None, height: None });
    cmd(ui, app, "Render Setup...", Command::RenderSetup { width: None, height: None, image_aspect: None, pixel_aspect: None, lock_image_aspect: None, lock_pixel_aspect: None, render_hidden: None, force_two_sided: None, atmospherics: None, effects: None, save_file: None, output: None });
    cmd(ui, app, "Rendered Frame Window...", Command::RenderedFrameWindow);
    ui.separator();
    cmd(ui, app, "Render To Texture...", Command::RenderToTexture);
    cmd(ui, app, "Environment...", Command::EnvironmentDialogToggle);
    cmd(ui, app, "Effects...", Command::RenderingEffects);
    ui.separator();
    cmd(ui, app, "Material Editor...", Command::MaterialEditorToggle);
}

fn customize_menu(ui: &mut egui::Ui, app: &mut LoomApp) {
    cmd(ui, app, "Customize User Interface...", Command::CustomizeUserInterface);
    ui.separator();
    cmd(ui, app, "Custom UI and Defaults Switcher...", Command::CustomUiDefaultsSwitcher);
    cmd(ui, app, "Configure User Paths...", Command::ConfigureUserPaths);
    ui.separator();
    cmd(ui, app, "Load Custom UI Scheme...", Command::LoadCustomUiScheme { path: None });
    cmd(ui, app, "Save Custom UI Scheme...", Command::SaveCustomUiScheme { path: None, parts: None });
    item(ui, app, "Revert to Startup Layout", "Revert to Startup UI");
    ui.separator();
    sub(ui, app, "Show UI", |ui, app| {
        check_item(ui, app, "Show Command Panel", "Show Command Panel Toggle", app.state.show_command_panel);
        check_item(ui, app, "Show Main Toolbar", "Show Main Toolbar Toggle", app.state.show_main_toolbar);
        check_item(ui, app, "Show Track Bar", "Show Track Bar Toggle", app.state.show_track_bar);
        check_item(ui, app, "Show Ribbon", "Show Ribbon Toggle", app.state.show_ribbon);
        check_item(ui, app, "Show Layers Toolbar", "Show Layers Toolbar Toggle", app.state.show_layers_toolbar);
    });
    check_item(ui, app, "Lock UI Layout", "Lock User Interface Toggle", app.state.ui_locked);
    ui.separator();
    cmd(ui, app, "Grid and Snap Settings...", Command::GridAndSnapSettings);
    cmd(ui, app, "Units Setup...", Command::UnitsSetup);
    ui.separator();
    cmd(ui, app, "Preferences...", Command::Preferences);
}

fn help_menu(ui: &mut egui::Ui, app: &mut LoomApp) {
    if entry(ui, app, "Keyboard Shortcut Map...") {
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(SHORTCUT_MAP_ID), true));
    }
    if entry(ui, app, "About 3dLoom...") {
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(ABOUT_ID), true));
    }
    ui.separator();
    if entry(ui, app, "3dLoom on GitHub") {
        ui.ctx().open_url(egui::OpenUrl::new_tab(REPO_URL));
        ui.close_menu();
    }
    if entry(ui, app, "Releases") {
        ui.ctx().open_url(egui::OpenUrl::new_tab(format!("{REPO_URL}/releases")));
        ui.close_menu();
    }
    ui.separator();
    ui.label(format!("3dLoom {} — a 3D modelling application", env!("LOOM_VERSION")));
}

const SHORTCUT_MAP_ID: &str = "help_shortcut_map_open";
const ABOUT_ID: &str = "help_about_open";

/// Help > Keyboard Shortcut Map (every action of every group with its keys,
/// searchable) and Help > About (version, licence, home page).
pub(crate) fn help_windows(ctx: &egui::Context, app: &mut LoomApp) {
    let map_id = egui::Id::new(SHORTCUT_MAP_ID);
    let mut map: bool = ctx.data(|d| d.get_temp(map_id)).unwrap_or(false);
    if map {
        let search_id = egui::Id::new("help_shortcut_map_search");
        let mut search: String = ctx.data(|d| d.get_temp(search_id)).unwrap_or_default();
        let rows = loom_commands::keymap::keyboard_chart_with(&app.state.custom_keys);
        egui::Window::new("Keyboard Shortcut Map").open(&mut map).default_size(egui::vec2(520.0, 480.0)).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Search:");
                ui.add(egui::TextEdit::singleline(&mut search).desired_width(220.0));
            });
            let needle = search.to_lowercase();
            let shown: Vec<_> = rows.iter().filter(|r| !r.shortcut.is_empty() && (needle.is_empty() || r.action.to_lowercase().contains(&needle) || r.shortcut.to_lowercase().contains(&needle))).collect();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                egui::Grid::new("shortcut_map").striped(true).num_columns(3).show(ui, |ui| {
                    for r in shown {
                        ui.label(r.shortcut.as_str());
                        ui.label(r.action);
                        ui.label(egui::RichText::new(r.group).weak());
                        ui.end_row();
                    }
                });
            });
        });
        ctx.data_mut(|d| d.insert_temp(search_id, search));
    }
    ctx.data_mut(|d| d.insert_temp(map_id, map));
    let about_id = egui::Id::new(ABOUT_ID);
    let mut about: bool = ctx.data(|d| d.get_temp(about_id)).unwrap_or(false);
    if about {
        egui::Window::new("About 3dLoom").open(&mut about).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.heading("3dLoom");
            ui.label(format!("Version {}", env!("LOOM_VERSION")));
            ui.label("A 3D modelling application.");
            ui.label("Licence: MIT");
            ui.hyperlink(REPO_URL);
        });
    }
    ctx.data_mut(|d| d.insert_temp(about_id, about));
}

/// The Quick Access toolbar: New Scene, Open File, Save File, Undo, Redo.
/// The Quick Access toolbar below the ribbon (Show Below the Ribbon).
pub fn quick_access_below(ctx: &egui::Context, app: &mut LoomApp) {
    if app.state.expert_mode || !(app.state.ui_custom.quick_access_below && app.state.show_ribbon) {
        return;
    }
    egui::TopBottomPanel::top("quick_access_below").show(ctx, |ui| {
        ui.horizontal(|ui| quick_access(ui, app));
    });
}

/// The Quick Access toolbar: its buttons (New Scene, Open File, Save File,
/// Undo and Redo with their history lists, or the user's choice), each with
/// a right-click Remove from Quick Access Toolbar / Add Separator, and the
/// Customize Quick Access Toolbar drop-down.
fn quick_access(ui: &mut egui::Ui, app: &mut LoomApp) {
    use crate::icons::{self, Icon};
    use loom_commands::PendingFileAction as A;
    let items = app.state.ui_custom.quick_access();
    let hidden = app.state.ui_custom.quick_access_hidden.clone();
    let mut set_items: Option<Vec<String>> = None;
    for (i, it) in items.iter().enumerate() {
        if it == loom_commands::ui_custom::SEPARATOR {
            let r = ui.separator().interact(egui::Sense::click());
            r.context_menu(|ui| {
                if ui.button("Remove from Quick Access Toolbar").clicked() {
                    let mut v = items.clone();
                    v.remove(i);
                    set_items = Some(v);
                    ui.close_menu();
                }
            });
            continue;
        }
        if hidden.contains(it) {
            continue;
        }
        let r = match it.as_str() {
            "New Scene" => Icon::new(icons::NEW_SCENE, "New Scene").show(ui, &app.state),
            "Open File" => Icon::new(icons::OPEN_FILE, "Open File").show(ui, &app.state),
            "Save File" => Icon::new(icons::SAVE_FILE, "Save File").show(ui, &app.state),
            "Undo Scene Operation" => Icon::new(icons::UNDO, "Undo Scene Operation").enabled(app.state.undo.can_undo()).show(ui, &app.state),
            "Redo Scene Operation" => Icon::new(icons::REDO, "Redo Scene Operation").enabled(app.state.undo.can_redo()).show(ui, &app.state),
            other => crate::toolbars::action_button(ui, app, other),
        };
        if r.clicked() {
            match it.as_str() {
                "New Scene" => app.request_file_action(A::New),
                "Open File" => app.request_file_action(A::Open),
                other => app.dispatch_named(other),
            }
        }
        r.context_menu(|ui| {
            if ui.button("Remove from Quick Access Toolbar").clicked() {
                let _ = app.dispatch(Command::ShowQuickAccessItem { action: it.clone(), on: false });
                ui.close_menu();
            }
            if ui.button("Add Separator").clicked() {
                let mut v = items.clone();
                v.insert(i + 1, loom_commands::ui_custom::SEPARATOR.to_owned());
                set_items = Some(v);
                ui.close_menu();
            }
        });
        // Undo / Redo: the down arrow lists the operations to go back (or forward) to.
        let history = match it.as_str() {
            "Undo Scene Operation" => Some((app.state.undo.labels(), "Undo Scene Operation", "undo_history")),
            "Redo Scene Operation" => Some((app.state.undo.redo_labels(), "Redo Scene Operation", "redo_history")),
            _ => None,
        };
        if let Some((labels, action, id)) = history {
            let mut steps = None;
            let r = egui::ComboBox::from_id_salt(id).selected_text("").width(12.0).show_ui(ui, |ui| {
                ui.set_min_width(180.0);
                // The most recent first: picking one goes back through it.
                for (k, l) in labels.iter().rev().enumerate() {
                    if ui.selectable_label(false, l).clicked() {
                        steps = Some(k + 1);
                    }
                }
            });
            icons::named(&r.response, egui::WidgetType::ComboBox, if action.starts_with("Undo") { "Undo History" } else { "Redo History" });
            if let Some(n) = steps {
                for _ in 0..n {
                    app.dispatch_named(action);
                }
            }
        }
    }
    let r = ui.menu_button(icons::DOWN, |ui| {
        ui.label(egui::RichText::new("Customize Quick Access Toolbar").strong());
        for it in items.iter().filter(|i| *i != loom_commands::ui_custom::SEPARATOR) {
            let on = !hidden.contains(it);
            let label = if on { format!("\u{2714} {}", it.trim()) } else { format!("    {}", it.trim()) };
            if ui.button(label).clicked() {
                let _ = app.dispatch(Command::ShowQuickAccessItem { action: it.clone(), on: !on });
                ui.close_menu();
            }
        }
        ui.separator();
        if ui.button(if app.state.ui_custom.menu_bar_hidden { "Show Menu Bar" } else { "Hide Menu Bar" }).clicked() {
            let _ = app.dispatch(Command::ShowMenuBarToggle);
            ui.close_menu();
        }
        let below = app.state.ui_custom.quick_access_below;
        if ui.add_enabled(app.state.show_ribbon, egui::Button::new(if below { "Show Above the Ribbon" } else { "Show Below the Ribbon" })).clicked() {
            let _ = app.dispatch(Command::SetQuickAccessBelowRibbon { on: !below });
            ui.close_menu();
        }
    });
    icons::named(&r.response, egui::WidgetType::Button, "Customize Quick Access Toolbar");
    if let Some(v) = set_items {
        let _ = app.dispatch(Command::SetQuickAccess { items: v });
    }
}

/// One object is selected and it has instances (Edit > Select Instances).
fn has_instances(s: &loom_commands::AppState) -> bool {
    let ids: Vec<loom_scene::NodeId> = s.selection.iter().copied().collect();
    if ids.len() != 1 {
        return false;
    }
    let root = |mut n: loom_scene::NodeId| {
        while let Some(src) = s.scene.node(n).and_then(|x| x.instance_of) {
            n = src;
        }
        n
    };
    let r = root(ids[0]);
    s.scene.nodes().filter(|n| root(n.id) == r).count() > 1
}

/// Animation > Position / Rotation / Scale Controllers: assign to every selected object.
fn controller_item(ui: &mut egui::Ui, app: &mut LoomApp, track: &str, controller: &str) {
    if ui.button(controller).clicked() {
        let ids: Vec<u64> = app.state.selection.iter().map(|n| n.0).collect();
        for id in ids {
            if let Err(e) = app.dispatch(Command::AssignController { id, track: track.into(), controller: controller.into() }) {
                app.state.status_prompt = e;
            }
        }
        ui.close_menu();
    }
}

/// The project's public repository (Help menu).
const REPO_URL: &str = "https://github.com/bonskari/3dloom";

/// Modifiers menu: the reference's categories, each with the modifiers of
/// the Modifier List that belong to it (label, Modifier List name).
/// Categories with no modifier in Loom (NURBS Editing, Cameras) are left out.
pub const MODIFIERS_MENU: &[(&str, &[(&str, &str)])] = &[
    ("Selection Modifiers", &[("Mesh Select", "Mesh Select"), ("Poly Select", "Poly Select"), ("Spline Select", "Spline Select"), ("Vol. Select", "Vol. Select")]),
    ("Patch/Spline Editing", &[
        ("CrossSection", "CrossSection"), ("Delete Spline", "Delete Spline"), ("Edit Patch", "Edit Patch"), ("Edit Spline", "Edit Spline"), ("Fillet/Chamfer", "Fillet/Chamfer"), ("Lathe", "Lathe"),
        ("Normalize Spline", "Normalize Spline"), ("Renderable Spline", "Renderable Spline"), ("Surface", "Surface"), ("Sweep", "Sweep"), ("Trim/Extend", "Trim/Extend"),
    ]),
    ("Mesh Editing", &[
        ("Bevel", "Bevel"), ("Bevel Profile", "Bevel Profile"), ("Cap Holes", "Cap Holes"), ("DeleteMesh", "DeleteMesh"), ("Edit Mesh", "Edit Mesh"), ("Edit Normals", "Edit Normals"),
        ("Edit Poly", "Edit Poly"), ("Extrude", "Extrude"), ("Face Extrude", "Face Extrude"), ("MultiRes", "MultiRes"), ("Normal", "Normal"), ("Optimize", "Optimize"),
        ("ProOptimizer", "ProOptimizer"), ("Smooth", "Smooth"), ("STL Check", "STL Check"), ("Symmetry", "Symmetry"), ("Tessellate", "Tessellate"), ("VertexPaint", "VertexPaint"),
        ("Vertex Weld", "Vertex Weld"),
    ]),
    ("Conversion", &[("Turn To Mesh", "Turn To Mesh"), ("Turn To Poly", "Turn To Poly")]),
    ("Animation Modifiers", &[
        ("Flex", "Flex"), ("Linked XForm", "Linked XForm"), ("Melt", "Melt"), ("Morpher", "Morpher"), ("PathDeform", "PathDeform"), ("PathDeform (WSM)", "PathDeform (WSM)"),
        ("Skin", "Skin"), ("Skin Morph", "Skin Morph"), ("Skin Wrap", "Skin Wrap"), ("Skin Wrap Patch", "Skin Wrap Patch"), ("Spline IK Control", "Spline IK Control"),
    ]),
    ("UV Coordinates", &[
        ("Camera Map", "Camera Map"), ("Camera Map (WSM)", "Camera Map (WSM)"), ("MapScaler", "MapScaler"), ("MapScaler (WSM)", "MapScaler (WSM)"), ("Projection", "Projection"), ("Unwrap UVW", "Unwrap UVW"),
        ("UVW Map", "UVW Map"), ("UVW Mapping Add", "UVW Mapping Add"), ("UVW Mapping Clear", "UVW Mapping Clear"), ("UVW XForm", "UVW XForm"),
    ]),
    ("Cache Tools", &[("Point Cache", "Point Cache"), ("Point Cache (WSM)", "Point Cache (WSM)")]),
    ("Subdivision Surfaces", &[("HSDS Modifier", "HSDS"), ("MeshSmooth", "MeshSmooth"), ("TurboSmooth", "TurboSmooth")]),
    ("Free Form Deformers", &[("FFD 2x2x2", "FFD 2x2x2"), ("FFD 3x3x3", "FFD 3x3x3"), ("FFD 4x4x4", "FFD 4x4x4"), ("FFD(box)", "FFD(box)"), ("FFD(cyl)", "FFD(cyl)")]),
    ("Parametric Deformers", &[
        ("Affect Region", "Affect Region"), ("Bend", "Bend"), ("Displace", "Displace"), ("Lattice", "Lattice"), ("Mirror", "Mirror"), ("Noise", "Noise"), ("Preserve", "Preserve"),
        ("Push", "Push"), ("Relax", "Relax"), ("Ripple", "Ripple"), ("Shell", "Shell"), ("Skew", "Skew"), ("Slice", "Slice"), ("Spherify", "Spherify"), ("Squeeze", "Squeeze"),
        ("Stretch", "Stretch"), ("Substitute", "Substitute"), ("Taper", "Taper"), ("Twist", "Twist"), ("Wave", "Wave"), ("XForm", "XForm"),
    ]),
    ("Surface", &[("Material", "Material"), ("Material By Element", "Material By Element")]),
    ("Radiosity", &[("Subdivide", "Subdivide"), ("Subdivide (WSM)", "Subdivide (WSM)")]),
];

#[cfg(test)]
mod tests {
    use super::MODIFIERS_MENU;
    use loom_scene::Modifier;

    #[test]
    fn modifiers_menu_holds_every_modifier_list_entry_once() {
        let mut names: Vec<&str> = MODIFIERS_MENU.iter().flat_map(|(_, e)| e.iter().map(|(_, n)| *n)).collect();
        for n in &names {
            assert!(Modifier::by_name(n).is_some(), "{n} is not a modifier");
        }
        names.sort();
        let mut list: Vec<&str> = Modifier::LIST.iter().chain(Modifier::WSM_LIST.iter()).copied().collect();
        list.sort();
        assert_eq!(names, list);
    }
}
