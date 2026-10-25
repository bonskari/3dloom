//! Customize User Interface > Toolbars, Quads, Menus and Colors, the
//! toolbar right-click menu (dock / float / show), UI scheme parts and the
//! rollout order.

use loom_commands::settings::Settings;
use loom_commands::ui_custom::{Dock, SEPARATOR};
use loom_commands::{execute, AppState, Command, Dialog, UiFileKind};
use std::path::PathBuf;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join("loom-ui-custom-tests").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn p(path: &std::path::Path) -> Option<String> {
    Some(path.to_string_lossy().into_owned())
}

fn ok(s: &mut AppState, c: Command) -> serde_json::Value {
    execute(s, c).unwrap()
}

fn add(s: &mut AppState, toolbar: &str, action: &str) {
    ok(s, Command::AddToolbarButton { toolbar: toolbar.into(), action: action.into(), index: None });
}

fn items(s: &AppState, toolbar: &str) -> Vec<String> {
    s.ui_custom.toolbar(toolbar).unwrap().items.clone()
}

#[test]
fn a_new_toolbar_floats_and_takes_actions_and_separators() {
    let mut s = AppState::new();
    ok(&mut s, Command::NewToolbar { name: "Mine".into() });
    let t = s.ui_custom.toolbar("Mine").unwrap();
    assert_eq!(t.dock, Dock::Float, "a new toolbar appears as a small floater");
    assert!(t.shown && t.items.is_empty());
    add(&mut s, "Mine", "Select and Move");
    add(&mut s, "Mine", SEPARATOR);
    ok(&mut s, Command::AddToolbarButton { toolbar: "Mine".into(), action: "Hold".into(), index: Some(0) });
    assert_eq!(items(&s, "Mine"), ["Hold", "Select and Move", "-"]);
    assert!(execute(&mut s, Command::AddToolbarButton { toolbar: "Mine".into(), action: "No Such Action".into(), index: None }).is_err());
    assert!(execute(&mut s, Command::NewToolbar { name: "Mine".into() }).is_err(), "names are unique");
    assert!(execute(&mut s, Command::NewToolbar { name: "Main Toolbar".into() }).is_err());
    ok(&mut s, Command::RemoveToolbarButton { toolbar: "Mine".into(), index: 2 });
    assert_eq!(items(&s, "Mine"), ["Hold", "Select and Move"]);
    ok(&mut s, Command::RenameToolbar { name: "Mine".into(), to: "Modeling".into() });
    assert!(s.ui_custom.toolbar("Mine").is_none() && s.ui_custom.toolbar("Modeling").is_some());
    ok(&mut s, Command::DeleteToolbar { name: "Modeling".into() });
    assert!(s.ui_custom.toolbars.is_empty());
    assert!(execute(&mut s, Command::DeleteToolbar { name: "Main Toolbar".into() }).is_err(), "the program's toolbars are hidden, not deleted");
}

#[test]
fn ctrl_drag_copies_and_alt_drag_moves_a_button_between_toolbars() {
    let mut s = AppState::new();
    ok(&mut s, Command::NewToolbar { name: "A".into() });
    ok(&mut s, Command::NewToolbar { name: "B".into() });
    add(&mut s, "A", "Hold");
    add(&mut s, "A", "Fetch");
    ok(&mut s, Command::MoveToolbarButton { toolbar: "A".into(), index: 0, to_toolbar: "B".into(), to_index: None, copy: true });
    assert_eq!((items(&s, "A"), items(&s, "B")), (vec!["Hold".to_owned(), "Fetch".to_owned()], vec!["Hold".to_owned()]));
    ok(&mut s, Command::MoveToolbarButton { toolbar: "A".into(), index: 1, to_toolbar: "B".into(), to_index: Some(0), copy: false });
    assert_eq!((items(&s, "A"), items(&s, "B")), (vec!["Hold".to_owned()], vec!["Fetch".to_owned(), "Hold".to_owned()]));
    // Within one toolbar a move reorders.
    ok(&mut s, Command::MoveToolbarButton { toolbar: "B".into(), index: 0, to_toolbar: "B".into(), to_index: Some(1), copy: false });
    assert_eq!(items(&s, "B"), ["Hold", "Fetch"]);
}

#[test]
fn toolbars_and_the_command_panel_dock_float_and_hide() {
    let mut s = AppState::new();
    assert_eq!(s.ui_custom.dock("Main Toolbar"), Dock::Top);
    assert_eq!(s.ui_custom.dock("Command Panel"), Dock::Right);
    ok(&mut s, Command::DockToolbar { name: "Command Panel".into(), dock: "Left".into(), pos: None });
    assert_eq!(s.ui_custom.dock("Command Panel"), Dock::Left);
    assert!(execute(&mut s, Command::DockToolbar { name: "Command Panel".into(), dock: "Top".into(), pos: None }).is_err());
    ok(&mut s, Command::DockToolbar { name: "Main Toolbar".into(), dock: "Float".into(), pos: Some([40.0, 50.0]) });
    assert_eq!((s.ui_custom.dock("Main Toolbar"), s.ui_custom.float_pos("Main Toolbar")), (Dock::Float, [40.0, 50.0]));
    // The hidden program toolbars: shown from the right-click menu.
    assert!(!loom_commands::ui_custom::is_shown(&s, "Axis Constraints"));
    let r = ok(&mut s, Command::ShowToolbar { name: "Axis Constraints".into(), on: None });
    assert_eq!(r["shown"], true);
    assert!(loom_commands::ui_custom::is_shown(&s, "Axis Constraints"));
    ok(&mut s, Command::ShowToolbar { name: "Main Toolbar".into(), on: Some(false) });
    assert!(!s.show_main_toolbar, "the Main Toolbar's Show UI toggle is the same switch");
    // Docking a hidden toolbar shows it.
    ok(&mut s, Command::DockToolbar { name: "Main Toolbar".into(), dock: "Left".into(), pos: None });
    assert!(s.show_main_toolbar);
    ok(&mut s, Command::ResetToolbars);
    assert_eq!(s.ui_custom.dock("Command Panel"), Dock::Right);
    assert!(!loom_commands::ui_custom::is_shown(&s, "Axis Constraints"));
}

#[test]
fn button_appearance_sets_text_icon_and_tooltip() {
    let mut s = AppState::new();
    ok(&mut s, Command::SetButtonAppearance { action: "Hold".into(), image: Some(false), text: Some("Hold!".into()), icon: None, tooltip: Some("Keep a copy".into()) });
    let look = &s.ui_custom.looks["Hold"];
    assert_eq!((look.image, look.text.as_str(), look.tooltip.as_str()), (false, "Hold!", "Keep a copy"));
    ok(&mut s, Command::SetButtonAppearance { action: "Hold".into(), image: Some(false), text: Some(String::new()), icon: Some(String::new()), tooltip: Some(String::new()) });
    assert!(!s.ui_custom.looks.contains_key("Hold"), "back to the action's own look");
}

#[test]
fn toolbars_save_and_load_as_a_cui_file() {
    let d = dir("cui");
    let mut s = AppState::new();
    let r = ok(&mut s, Command::SaveToolbars { path: None });
    assert_eq!(s.dialog, Some(Dialog::UiFile { kind: UiFileKind::Toolbars, save: true }), "{r}");
    ok(&mut s, Command::NewToolbar { name: "Mine".into() });
    add(&mut s, "Mine", "Hold");
    ok(&mut s, Command::DockToolbar { name: "Command Panel".into(), dock: "Float".into(), pos: Some([10.0, 20.0]) });
    let file = d.join("mine.cui");
    ok(&mut s, Command::SaveToolbars { path: p(&file) });
    let mut t = AppState::new();
    ok(&mut t, Command::LoadToolbars { path: p(&file) });
    assert_eq!(items(&t, "Mine"), ["Hold"]);
    assert_eq!(t.ui_custom.dock("Command Panel"), Dock::Float);
    std::fs::write(d.join("bad.cui"), "{\"format\":\"something-else\",\"toolbars\":[],\"docks\":{},\"float_pos\":{},\"shown\":{},\"looks\":{}}").unwrap();
    assert!(execute(&mut t, Command::LoadToolbars { path: p(&d.join("bad.cui")) }).is_err());
}

#[test]
fn quads_take_labels_items_hidden_entries_and_user_sets() {
    let mut s = AppState::new();
    let set = "Default Viewport Quad";
    ok(&mut s, Command::SetQuadLabel { set: set.into(), quadrant: 0, label: "show".into() });
    ok(&mut s, Command::AddQuadItem { set: set.into(), quadrant: 1, item: "Hold".into(), index: None });
    ok(&mut s, Command::AddQuadItem { set: set.into(), quadrant: 1, item: SEPARATOR.into(), index: Some(0) });
    ok(&mut s, Command::RenameQuadItem { set: set.into(), quadrant: 1, index: 1, name: "Hold Scene".into() });
    ok(&mut s, Command::HideQuadItem { set: set.into(), quadrant: 0, label: "Unhide All".into(), hidden: true });
    let q = s.ui_custom.quad(set);
    assert_eq!(q.labels[0], "show");
    assert_eq!(q.items[1].iter().map(|i| i.label().to_owned()).collect::<Vec<_>>(), ["-", "Hold Scene"]);
    assert_eq!(q.hidden[0], ["Unhide All"]);
    ok(&mut s, Command::HideQuadItem { set: set.into(), quadrant: 0, label: "Unhide All".into(), hidden: false });
    assert!(s.ui_custom.quad(set).hidden[0].is_empty());
    ok(&mut s, Command::RemoveQuadItem { set: set.into(), quadrant: 1, index: 0 });
    assert_eq!(s.ui_custom.quad(set).items[1].len(), 1);
    assert!(execute(&mut s, Command::SetQuadLabel { set: set.into(), quadrant: 4, label: "x".into() }).is_err());
    assert!(execute(&mut s, Command::DeleteQuadSet { name: set.into() }).is_err(), "the program's quad sets cannot be deleted");
    ok(&mut s, Command::NewQuadSet { name: "Modelling Tools".into() });
    ok(&mut s, Command::AddQuadItem { set: "Modelling Tools".into(), quadrant: 2, item: "Hold".into(), index: None });
    ok(&mut s, Command::SetShowAllQuads { set: "Modelling Tools".into(), on: false });
    assert!(s.ui_custom.quad_sets().contains(&"Modelling Tools".to_owned()));
    ok(&mut s, Command::ShowQuadMenu { set: "Modelling Tools".into() });
    assert_eq!(s.quad_request.as_deref(), Some("Modelling Tools"), "the UI opens it at the pointer");
    ok(&mut s, Command::RenameQuadSet { name: "Modelling Tools".into(), to: "Mine".into() });
    assert!(!s.ui_custom.quad("Mine").show_all);
    ok(&mut s, Command::DeleteQuadSet { name: "Mine".into() });
    assert!(!s.ui_custom.quad_sets().contains(&"Mine".to_owned()));
    assert!(execute(&mut s, Command::ShowQuadMenu { set: "Mine".into() }).is_err());
}

#[test]
fn advanced_quad_options_and_their_qop_file() {
    let d = dir("qop");
    let mut s = AppState::new();
    let o = |s: &mut AppState, n: &str, v: &str| execute(s, Command::SetQuadOption { name: n.into(), value: v.into() });
    o(&mut s, "Opacity Amount", "60").unwrap();
    o(&mut s, "Mirror Quads", "on").unwrap();
    o(&mut s, "Animation Type", "Fade").unwrap();
    o(&mut s, "Starting Quadrant", "Upper Left").unwrap();
    assert!(o(&mut s, "Animation Type", "Spin").is_err());
    assert!(o(&mut s, "Nonsense", "1").is_err());
    assert_eq!(s.ui_custom.quad_options.starting_quadrant, 2);
    // Locked colours set every quadrant; unlocked, just one.
    ok(&mut s, Command::SetQuadColor { element: "Background".into(), quadrant: 1, color: Some([10, 20, 30]), locked: None });
    assert_eq!(s.ui_custom.quad_options.colors["Background"], [[10, 20, 30]; 4]);
    ok(&mut s, Command::SetQuadColor { element: "Background".into(), quadrant: 3, color: Some([1, 2, 3]), locked: Some(false) });
    assert_eq!(s.ui_custom.quad_options.colors["Background"], [[10, 20, 30], [10, 20, 30], [10, 20, 30], [1, 2, 3]]);
    let file = d.join("q.qop");
    ok(&mut s, Command::SaveQuadOptions { path: p(&file) });
    let mut t = AppState::new();
    ok(&mut t, Command::LoadQuadOptions { path: p(&file) });
    assert_eq!(t.ui_custom.quad_options, s.ui_custom.quad_options);
    assert_eq!(t.ui_custom.quad_options.opacity, 60.0);
    ok(&mut t, Command::ResetQuadOptions);
    assert_eq!(t.ui_custom.quad_options.opacity, 100.0);
}

#[test]
fn menus_take_new_menus_items_hidden_entries_and_titles() {
    let d = dir("mnu");
    let mut s = AppState::new();
    ok(&mut s, Command::NewMenu { name: "Game".into() });
    ok(&mut s, Command::AddMenuItem { menu: "Game".into(), item: "Hold".into(), index: None });
    ok(&mut s, Command::AddMenuItem { menu: "Game".into(), item: SEPARATOR.into(), index: None });
    ok(&mut s, Command::AddMenuItem { menu: "Game".into(), item: "menu:Tools".into(), index: None });
    assert!(execute(&mut s, Command::AddMenuItem { menu: "Game".into(), item: "menu:Game".into(), index: None }).is_err());
    assert!(execute(&mut s, Command::AddMenuItem { menu: "Game".into(), item: "menu:Nope".into(), index: None }).is_err());
    ok(&mut s, Command::SetMenuOnBar { menu: "Game".into(), on: true });
    ok(&mut s, Command::RenameMenu { menu: "Game".into(), title: "&Game".into() });
    assert_eq!(s.ui_custom.menus.title("Game"), "&Game");
    assert_eq!(s.ui_custom.menus.title("Modifiers"), "Mo&difiers", "the program's default title");
    ok(&mut s, Command::AddMenuItem { menu: "Edit".into(), item: "Hold".into(), index: None });
    ok(&mut s, Command::RenameMenuItem { menu: "Edit".into(), index: 0, name: "&Hold It".into() });
    ok(&mut s, Command::HideMenuItem { menu: "Edit".into(), label: "Clone".into(), hidden: true });
    assert!(s.ui_custom.menus.is_hidden("Edit", "Clone"));
    assert!(execute(&mut s, Command::HideMenuItem { menu: "Game".into(), label: "Hold".into(), hidden: true }).is_err(), "a user menu's entries are removed, not hidden");
    ok(&mut s, Command::AddQuadItem { set: "Snap".into(), quadrant: 0, item: "menu:Game".into(), index: None });
    let file = d.join("m.mnu");
    ok(&mut s, Command::SaveMenus { path: p(&file) });
    let mut t = AppState::new();
    ok(&mut t, Command::LoadMenus { path: p(&file) });
    assert_eq!(t.ui_custom.menus, s.ui_custom.menus);
    assert_eq!(t.ui_custom.quad("Snap").items[0][0].item, "menu:Game", "quads are in the .mnu too");
    // Deleting a menu removes the entries that open it.
    ok(&mut t, Command::DeleteMenu { name: "Game".into() });
    assert!(t.ui_custom.quad("Snap").items[0].is_empty());
    ok(&mut t, Command::ResetMenus);
    assert!(!t.ui_custom.menus.is_hidden("Edit", "Clone"));
}

#[test]
fn colors_set_reset_and_round_trip_through_a_clr_file() {
    let d = dir("clr");
    let mut s = AppState::new();
    assert_eq!(s.ui_custom.colors.get("Viewport Background"), [80, 80, 80]);
    ok(&mut s, Command::SetUiColor { element: "Viewport Background".into(), color: [10, 20, 30] });
    ok(&mut s, Command::SetUiColor { element: "Text".into(), color: [250, 250, 0] });
    ok(&mut s, Command::SetColorScheme { scheme: "custom colors".into() });
    assert!(execute(&mut s, Command::SetUiColor { element: "Nope".into(), color: [0, 0, 0] }).is_err());
    assert_eq!(s.ui_custom.colors.get("Viewport Background"), [10, 20, 30]);
    assert!(s.grid_dirty, "the viewports redraw with the colour");
    let file = d.join("c.clr");
    ok(&mut s, Command::SaveUiColors { path: p(&file) });
    let mut t = AppState::new();
    ok(&mut t, Command::LoadUiColors { path: p(&file) });
    assert_eq!(t.ui_custom.colors, s.ui_custom.colors);
    assert_eq!(t.ui_custom.colors.scheme, "Custom Colors");
    assert_eq!(t.ui_custom.colors.appearance("Text"), [250, 250, 0]);
    ok(&mut t, Command::ResetUiColors);
    assert_eq!(t.ui_custom.colors.get("Viewport Background"), [80, 80, 80]);
}

#[test]
fn customizations_live_in_the_settings_file_and_revert_to_startup() {
    let mut s = AppState::new();
    ok(&mut s, Command::NewToolbar { name: "Mine".into() });
    ok(&mut s, Command::SetRolloutOrder { panel: "Modify/Box".into(), order: vec!["Parameters".into(), "Keyboard Entry".into()] });
    ok(&mut s, Command::SetUiColor { element: "Selection".into(), color: [255, 0, 0] });
    let loaded = Settings::from_json(&Settings::capture(&s).to_json()).unwrap();
    let mut t = AppState::new();
    loaded.apply(&mut t);
    assert_eq!(t.ui_custom, s.ui_custom);
    // Changes after the start go back with Revert to Startup Layout.
    ok(&mut t, Command::DeleteToolbar { name: "Mine".into() });
    ok(&mut t, Command::DockToolbar { name: "Command Panel".into(), dock: "Left".into(), pos: None });
    ok(&mut t, Command::RevertToStartupUi);
    assert!(t.ui_custom.toolbar("Mine").is_some());
    assert_eq!(t.ui_custom.dock("Command Panel"), Dock::Right);
    ok(&mut t, Command::SetRolloutOrder { panel: "Modify/Box".into(), order: Vec::new() });
    assert!(!t.ui_custom.rollout_order.contains_key("Modify/Box"));
}

#[test]
fn a_ui_scheme_saves_only_the_ticked_parts() {
    let d = dir("scheme_parts");
    let mut s = AppState::new();
    ok(&mut s, Command::NewToolbar { name: "Mine".into() });
    ok(&mut s, Command::SetUiColor { element: "Selection".into(), color: [255, 0, 0] });
    ok(&mut s, Command::NewMenu { name: "Game".into() });
    let file = d.join("colors_only.ui");
    ok(&mut s, Command::SaveCustomUiScheme { path: p(&file), parts: Some(vec!["Colors".into()]) });
    assert!(execute(&mut s, Command::SaveCustomUiScheme { path: p(&file), parts: Some(vec!["Fonts".into()]) }).is_err());
    let mut t = AppState::new();
    ok(&mut t, Command::NewMenu { name: "Other".into() });
    ok(&mut t, Command::LoadCustomUiScheme { path: p(&file) });
    assert_eq!(t.ui_custom.colors.get("Selection"), [255, 0, 0]);
    assert!(t.ui_custom.toolbar("Mine").is_none(), "the layout was not in the file");
    assert!(t.ui_custom.menus.user_menu("Other").is_some(), "nor the menus: the loaded state keeps its own");
    let all = d.join("all.ui");
    ok(&mut s, Command::SaveCustomUiScheme { path: p(&all), parts: None });
    ok(&mut t, Command::LoadCustomUiScheme { path: p(&all) });
    assert!(t.ui_custom.toolbar("Mine").is_some());
    assert!(t.ui_custom.menus.user_menu("Game").is_some() && t.ui_custom.menus.user_menu("Other").is_none());
}
