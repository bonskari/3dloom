//! Customizing the UI as a person does: the toolbar right-click menu (dock,
//! float, the hidden toolbars), user toolbars filled by dragging actions,
//! the Customize User Interface Toolbars / Quads / Menus / Colors panels,
//! menu accelerators and the UI colours.

use super::*;
use egui::accesskit::Role;

fn panel_rect(h: &Harness, id: &str) -> egui::Rect {
    egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new(id)).unwrap_or_else(|| panic!("no panel {id}")).rect
}

/// The window `title`'s rect (floating toolbars and dialogs are windows).
fn window_rect(h: &Harness, id: egui::Id) -> Option<egui::Rect> {
    h.ctx.memory(|m| m.area_rect(id)).filter(|_| h.ctx.memory(|m| m.areas().visible_layer_ids().iter().any(|l| l.id == id)))
}

/// The topmost entry `label` of an open popup menu (a context menu or a submenu).
fn menu_entry(h: &Harness, label: &str) -> egui::Pos2 {
    let open = menus(h);
    let all = h.find_all(Role::Button, label);
    all.iter().copied().find(|r| open.iter().any(|m| m.contains_rect(*r))).unwrap_or_else(|| panic!("no menu entry {label:?} in {open:?}: {all:?}")).center()
}

fn point_at(h: &mut Harness, p: egui::Pos2) {
    h.run(vec![egui::Event::PointerMoved(p)]);
    h.frames(2);
}

/// An empty spot of the Main Toolbar (its bottom-right corner, where no button sits).
fn main_toolbar_blank(h: &Harness) -> egui::Pos2 {
    let r = panel_rect(h, "main_toolbar");
    egui::pos2(r.max.x - 6.0, r.max.y - 4.0)
}

/// Customize User Interface on panel `tab`.
fn customize(h: &mut Harness, tab: &str) -> egui::Rect {
    h.cmd(Command::CustomizeUserInterface);
    h.frames(2);
    let win = customize_ui_rect(h);
    let r = h.text_rect(tab, win).unwrap_or_else(|| panic!("no {tab} tab"));
    h.click(r.center());
    h.frames(2);
    customize_ui_rect(h)
}

/// Types `text` into the field whose hint or label is near `anchor` (clicks it first).
fn type_into(h: &mut Harness, at: egui::Pos2, text: &str) {
    h.click(at);
    h.run(vec![egui::Event::Text(text.to_owned())]);
    h.frames(1);
}

#[test]
fn the_toolbar_right_click_menu_docks_the_main_toolbar_left_and_floats_it() {
    let mut h = Harness::new();
    right_click(&mut h, main_toolbar_blank(&h));
    point_at(&mut h, menu_entry(&h, "Dock"));
    h.click(menu_entry(&h, "    Left"));
    assert_eq!(h.app.state.ui_custom.dock("Main Toolbar"), loom_commands::ui_custom::Dock::Left);
    h.frames(2);
    let r = panel_rect(&h, "main_toolbar");
    assert!(r.min.x < 1.0 && r.height() > r.width() * 3.0, "a column at the window's left edge: {r:?}");
    assert!(h.has(Role::Button, "Select and Move"), "its buttons are still there");
    // Right-click its empty foot: Float.
    right_click(&mut h, egui::pos2(r.center().x, r.max.y - 6.0));
    h.click(menu_entry(&h, "Float"));
    h.frames(2);
    assert_eq!(h.app.state.ui_custom.dock("Main Toolbar"), loom_commands::ui_custom::Dock::Float);
    let w = window_rect(&h, crate::toolbars::window_id("Main Toolbar")).expect("a floating Main Toolbar window");
    let mv = h.find(Role::Button, "Select and Move");
    assert!(w.contains_rect(mv), "Select and Move is in the floating toolbar");
    h.click(mv.center());
    assert_eq!(h.app.state.active_tool, loom_commands::Tool::Move, "its buttons work there");
}

#[test]
fn the_command_panel_docks_left_from_its_right_click_menu() {
    let mut h = Harness::new();
    let r = panel_rect(&h, "command_panel");
    assert!(r.max.x > 1200.0, "at the right edge first");
    // Above the tabs: the panel's own empty area.
    right_click(&mut h, egui::pos2(r.max.x - 8.0, r.min.y + 2.0));
    point_at(&mut h, menu_entry(&h, "Dock"));
    h.click(menu_entry(&h, "    Left"));
    h.frames(2);
    let r = panel_rect(&h, "command_panel");
    assert!(r.min.x < 1.0, "now at the left edge: {r:?}");
    assert!(h.has(Role::Button, "Create"), "with its tabs");
}

#[test]
fn the_right_click_menu_shows_the_axis_constraints_toolbar_whose_buttons_restrict_axes() {
    let mut h = Harness::new();
    right_click(&mut h, main_toolbar_blank(&h));
    h.click(menu_entry(&h, "    Axis Constraints"));
    h.frames(2);
    assert!(loom_commands::ui_custom::is_shown(&h.app.state, "Axis Constraints"));
    h.click(h.find(Role::Button, "Restrict to Z").center());
    assert_eq!(h.app.state.axis_constraint, loom_commands::AxisConstraint::Z);
    h.click(h.find(Role::Button, "Restrict to XY Plane").center());
    assert_eq!(h.app.state.axis_constraint, loom_commands::AxisConstraint::XY);
    // The Snaps toolbar's targets are toggles.
    right_click(&mut h, main_toolbar_blank(&h));
    h.click(menu_entry(&h, "    Snaps"));
    h.frames(2);
    let was = h.app.state.snaps.targets.get("Vertex").unwrap_or(false);
    h.click(h.find(Role::Button, "Vertex").center());
    assert_eq!(h.app.state.snaps.targets.get("Vertex"), Some(!was));
}

/// A user toolbar named `name`, made in the Toolbars panel; returns its floating window.
fn new_toolbar(h: &mut Harness, name: &str) -> egui::Rect {
    let win = customize(h, "Toolbars");
    let field = h.text_rect("Toolbar name", win).expect("the new toolbar's name field");
    type_into(h, field.center(), name);
    let win = customize_ui_rect(h);
    let new = h.text_rect("New", win).expect("New");
    h.click(new.center());
    h.frames(3);
    assert!(h.app.state.ui_custom.toolbar(name).is_some(), "the toolbar exists");
    window_rect(h, crate::toolbars::window_id(name)).expect("it floats")
}

/// Drags action `action` from the Customize dialog's action list to `to`.
fn drag_action(h: &mut Harness, action: &str, to: egui::Pos2) {
    let win = customize_ui_rect(h);
    let search = h.text_rect("Search:", win).expect("the search field");
    type_into(h, search.center() + egui::vec2(search.width() / 2.0 + 40.0, 0.0), action);
    h.frames(1);
    let row = h.find_all(Role::Button, action).into_iter().find(|r| customize_ui_rect(h).contains_rect(*r)).unwrap_or_else(|| panic!("{action} in the action list"));
    h.drag(row.center(), to);
    h.frames(2);
    // Clear the search for the next drag.
    let win = customize_ui_rect(h);
    let search = h.text_rect("Search:", win).unwrap();
    h.click(search.center() + egui::vec2(search.width() / 2.0 + 40.0, 0.0));
    for _ in 0..action.len() {
        h.key(egui::Key::Backspace);
    }
}

#[test]
fn actions_dragged_onto_a_new_toolbar_become_its_buttons() {
    let mut h = Harness::new();
    let bar = new_toolbar(&mut h, "Mine");
    drag_action(&mut h, "Select All", bar.center());
    assert_eq!(h.app.state.ui_custom.toolbar("Mine").unwrap().items, ["Select All"]);
    drag_action(&mut h, "Select None", bar.center() + egui::vec2(bar.width() / 2.0 - 4.0, 0.0));
    assert_eq!(h.app.state.ui_custom.toolbar("Mine").unwrap().items, ["Select All", "Select None"], "dropped after the first button");
    h.cmd(Command::CloseDialog);
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let bar = window_rect(&h, crate::toolbars::window_id("Mine")).unwrap();
    let none = h.find_all(Role::Button, "Select None").into_iter().find(|r| bar.contains_rect(*r)).expect("Select None on the toolbar");
    h.click(none.center());
    assert!(h.app.state.selection.is_empty(), "the toolbar button runs its action");
    let all = h.find_all(Role::Button, "Select All").into_iter().find(|r| bar.contains_rect(*r)).unwrap();
    h.click(all.center());
    assert_eq!(h.app.state.selection.len(), 1);
    // Right-click a button: Delete Button.
    right_click(&mut h, all.center());
    h.click(menu_entry(&h, "Delete Button"));
    assert_eq!(h.app.state.ui_custom.toolbar("Mine").unwrap().items, ["Select None"]);
}

#[test]
fn ctrl_drag_copies_a_button_to_another_toolbar_and_alt_drag_moves_it() {
    let mut h = Harness::new();
    h.cmd(Command::NewToolbar { name: "A".into() });
    h.cmd(Command::NewToolbar { name: "B".into() });
    h.cmd(Command::DockToolbar { name: "A".into(), dock: "Float".into(), pos: Some([300.0, 300.0]) });
    h.cmd(Command::DockToolbar { name: "B".into(), dock: "Float".into(), pos: Some([300.0, 450.0]) });
    h.cmd(Command::AddToolbarButton { toolbar: "A".into(), action: "Select All".into(), index: None });
    h.cmd(Command::AddToolbarButton { toolbar: "A".into(), action: "Select None".into(), index: None });
    h.frames(3);
    let a = window_rect(&h, crate::toolbars::window_id("A")).unwrap();
    let b = window_rect(&h, crate::toolbars::window_id("B")).unwrap();
    let all = h.find_all(Role::Button, "Select All").into_iter().find(|r| a.contains_rect(*r)).unwrap();
    h.mods = egui::Modifiers::CTRL;
    h.drag(all.center(), b.center());
    h.mods = egui::Modifiers::NONE;
    assert_eq!(h.app.state.ui_custom.toolbar("A").unwrap().items, ["Select All", "Select None"]);
    assert_eq!(h.app.state.ui_custom.toolbar("B").unwrap().items, ["Select All"], "Ctrl+drag copies");
    h.frames(2);
    let a = window_rect(&h, crate::toolbars::window_id("A")).unwrap();
    let b = window_rect(&h, crate::toolbars::window_id("B")).unwrap();
    let none = h.find_all(Role::Button, "Select None").into_iter().find(|r| a.contains_rect(*r)).unwrap();
    h.mods = egui::Modifiers::ALT;
    h.drag(none.center(), b.center() + egui::vec2(b.width() / 2.0 - 4.0, 0.0));
    h.mods = egui::Modifiers::NONE;
    assert_eq!(h.app.state.ui_custom.toolbar("A").unwrap().items, ["Select All"], "Alt+drag moves");
    assert_eq!(h.app.state.ui_custom.toolbar("B").unwrap().items, ["Select All", "Select None"]);
}

#[test]
fn edit_button_appearance_makes_a_text_button_with_its_own_tooltip() {
    let mut h = Harness::new();
    h.cmd(Command::NewToolbar { name: "Mine".into() });
    h.cmd(Command::AddToolbarButton { toolbar: "Mine".into(), action: "Select and Move".into(), index: None });
    h.frames(2);
    let bar = window_rect(&h, crate::toolbars::window_id("Mine")).unwrap();
    let mv = h.find_all(Role::Button, "Select and Move").into_iter().find(|r| bar.contains_rect(*r)).expect("an icon button");
    right_click(&mut h, mv.center());
    h.click(menu_entry(&h, "Edit Button Appearance..."));
    h.frames(2);
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("Edit Button Appearance"))).expect("the dialog");
    let text_radio = h.text_rect("Text Button", win).unwrap();
    h.click(text_radio.center());
    let field = h.text_rect("Select and Move", win).into_iter().find(|r| r.min.y > text_radio.max.y).expect("the label field under Text Button");
    type_into(&mut h, field.center(), "MOVE");
    let ok = h.text_rect("OK", win).unwrap();
    h.click(ok.center());
    h.frames(2);
    let look = &h.app.state.ui_custom.looks["Select and Move"];
    assert!(!look.image && look.text == "MOVE", "{look:?}");
    let bar = window_rect(&h, crate::toolbars::window_id("Mine")).unwrap();
    assert!(h.text_rect("MOVE", bar).is_some(), "the button shows its text");
}

#[test]
fn an_action_dropped_into_a_quadrant_shows_in_the_quad_menu_and_a_deleted_entry_does_not() {
    let mut h = Harness::new();
    let win = customize(&mut h, "Quads");
    // The default set's Lower Right quadrant (transform).
    let lr = h.text_rect("Lower Right", win).unwrap();
    h.click(lr.center());
    let list = h.text_rect("Clone", customize_ui_rect(&h)).expect("the program's Clone entry is listed");
    drag_action(&mut h, "Select All", list.center() + egui::vec2(0.0, 30.0));
    let q = h.app.state.ui_custom.quad("Default Viewport Quad");
    assert_eq!(q.items[1].iter().map(|i| i.item.as_str()).collect::<Vec<_>>(), ["Select All"]);
    // Right-click the program's Clone entry: Delete Menu Item.
    let clone = h.text_rect("Clone", customize_ui_rect(&h)).unwrap();
    right_click(&mut h, clone.center());
    h.click(menu_entry(&h, "Delete Menu Item"));
    assert_eq!(h.app.state.ui_custom.quad("Default Viewport Quad").hidden[1], ["Clone"]);
    // The Label field renames the quadrant.
    let label = h.text_rect("Label:", customize_ui_rect(&h)).unwrap();
    let field = label.center() + egui::vec2(label.width() / 2.0 + 60.0, 0.0);
    h.click(field);
    h.mods = egui::Modifiers::CTRL;
    h.key(egui::Key::A);
    h.mods = egui::Modifiers::NONE;
    h.run(vec![egui::Event::Text("moves".into())]);
    h.key(egui::Key::Enter);
    h.click(h.text_rect("Lower Right", customize_ui_rect(&h)).unwrap().center());
    assert_eq!(h.app.state.ui_custom.quad("Default Viewport Quad").labels[1], "moves");
    h.cmd(Command::CloseDialog);
    maximized(&mut h);
    let at = h.app.viewport_rects[h.app.state.active_viewport].center() + egui::vec2(-150.0, 50.0);
    right_click(&mut h, at);
    assert!(h.has(Role::Button, "moves"), "the quadrant shows its label");
    let open = menus(&h);
    assert!(!h.find_all(Role::Button, "Clone").iter().any(|r| open.iter().any(|m| m.contains_rect(*r))), "Clone is gone from the quad");
    let entry = h.find_all(Role::Button, "Select All").into_iter().find(|r| open.iter().any(|m| m.contains_rect(*r))).expect("Select All in the quad");
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    h.cmd(Command::SelectNone);
    h.click(entry);
    assert_eq!(h.app.state.selection.len(), 1, "the added entry runs");
}

#[test]
fn a_quad_shortcut_opens_a_user_quad_set_and_show_all_quads_off_shows_one_quadrant() {
    let mut h = Harness::new();
    h.cmd(Command::NewQuadSet { name: "Mine".into() });
    h.cmd(Command::AddQuadItem { set: "Mine".into(), quadrant: 0, item: "Select All".into(), index: None });
    h.cmd(Command::AddQuadItem { set: "Mine".into(), quadrant: 1, item: "Select None".into(), index: None });
    h.cmd(Command::SetQuadShortcut { set: "Mine".into(), chord: "Shift+Alt+Q".into() });
    maximized(&mut h);
    let at = h.app.viewport_rects[h.app.state.active_viewport].center();
    point_at(&mut h, at);
    h.mods = egui::Modifiers::SHIFT | egui::Modifiers::ALT;
    h.key(egui::Key::Q);
    h.mods = egui::Modifiers::NONE;
    let open = h.app.quad_menu.expect("the shortcut opens a quad menu");
    assert_eq!(open.kind.set_name(&h.app), "Mine");
    assert!(h.has(Role::Button, "Select All") && h.has(Role::Button, "Select None"), "both quadrants show");
    h.key(egui::Key::Escape);
    h.cmd(Command::SetShowAllQuads { set: "Mine".into(), on: false });
    // The pointer toward the upper right: only that quadrant's entries show.
    point_at(&mut h, at + egui::vec2(30.0, -30.0));
    h.cmd(Command::ShowQuadMenu { set: "Mine".into() });
    point_at(&mut h, at + egui::vec2(30.0, -30.0));
    let open = menus(&h);
    let shown = |h: &Harness, l: &str| h.find_all(Role::Button, l).iter().any(|r| open.iter().any(|m| m.contains_rect(*r)));
    assert!(shown(&h, "Select All") && !shown(&h, "Select None"), "one quadrant at a time");
}

#[test]
fn a_new_menu_on_the_menu_bar_runs_its_actions_and_a_deleted_entry_leaves_its_menu() {
    let mut h = Harness::new();
    let win = customize(&mut h, "Menus");
    let field = h.text_rect("New menu name", win).unwrap();
    type_into(&mut h, field.center(), "Game");
    h.click(h.text_rect("New", customize_ui_rect(&h)).unwrap().center());
    h.frames(2);
    assert!(h.app.state.ui_custom.menus.user_menu("Game").is_some());
    let entries = h.text_rect("Entries (drop actions, separators and menus here):", customize_ui_rect(&h)).unwrap();
    drag_action(&mut h, "Select All", entries.center() + egui::vec2(0.0, 40.0));
    assert_eq!(h.app.state.ui_custom.menus.user_menu("Game").unwrap().items[0].item, "Select All");
    h.click(h.text_rect("On Menu Bar", customize_ui_rect(&h)).unwrap().center());
    assert!(h.app.state.ui_custom.menus.user_menu("Game").unwrap().on_bar);
    // The Edit menu: right-click its own Clone entry > Delete Menu Item.
    let combo = h.find(Role::ComboBox, "Game");
    h.click(combo.center());
    h.click(menu_entry(&h, "Edit"));
    h.frames(2);
    let clone = h.text_rect("Clone", customize_ui_rect(&h)).expect("Edit's own Clone entry");
    right_click(&mut h, clone.center());
    h.click(menu_entry(&h, "Delete Menu Item"));
    assert!(h.app.state.ui_custom.menus.is_hidden("Edit", "Clone"));
    h.cmd(Command::CloseDialog);
    h.click(h.find(Role::Button, "Edit").center());
    h.frames(1);
    assert!(h.has(Role::Button, "Select All") && !h.has(Role::Button, "Clone"), "the Edit menu has no Clone");
    h.key(egui::Key::Escape);
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    h.cmd(Command::SelectNone);
    h.click(h.find(Role::Button, "Game").center());
    h.frames(1);
    h.click(menu_entry(&h, "Select All"));
    assert_eq!(h.app.state.selection.len(), 1, "the Game menu's entry runs");
}

#[test]
fn alt_then_a_letter_opens_that_menu() {
    let mut h = Harness::new();
    // Alt pressed and released alone, then E.
    h.mods = egui::Modifiers::ALT;
    h.frames(1);
    h.mods = egui::Modifiers::NONE;
    h.frames(1);
    h.key(egui::Key::E);
    h.frames(2);
    assert_eq!(h.app.state.active_tool, loom_commands::Tool::Select, "E did not pick Select and Rotate");
    assert!(h.has(Role::Button, "Select Invert"), "the Edit menu is open");
    // A letter while a menu is open goes to the menu, not to the shortcuts.
    h.key(egui::Key::W);
    assert_ne!(h.app.state.active_tool, loom_commands::Tool::Move);
    h.key(egui::Key::Escape);
    // A renamed menu takes its new accelerator.
    h.cmd(Command::RenameMenu { menu: "Group".into(), title: "Gro&up".into() });
    h.mods = egui::Modifiers::ALT;
    h.frames(1);
    h.mods = egui::Modifiers::NONE;
    h.frames(1);
    h.key(egui::Key::U);
    h.frames(2);
    assert!(h.has(Role::Button, "Ungroup"), "Alt, U opens the Group menu");
}

fn has_fill(h: &Harness, c: egui::Color32) -> bool {
    fn walk(s: &egui::Shape, c: egui::Color32) -> bool {
        match s {
            egui::Shape::Vec(v) => v.iter().any(|s| walk(s, c)),
            egui::Shape::Rect(r) => r.fill == c,
            egui::Shape::Mesh(m) => m.vertices.iter().any(|v| v.color == c),
            _ => false,
        }
    }
    h.shapes.iter().any(|s| walk(&s.shape, c))
}

#[test]
fn colors_panel_colours_the_viewport_background_and_the_scheme_turns_the_ui_light() {
    let mut h = Harness::new();
    let win = customize(&mut h, "Colors");
    assert!(h.text_rect("Viewport Background", win).is_some(), "the Viewports elements are listed");
    let teal = egui::Color32::from_rgb(10, 120, 130);
    assert!(!has_fill(&h, teal));
    h.cmd(Command::SetUiColor { element: "Viewport Background".into(), color: [10, 120, 130] });
    assert!(has_fill(&h, teal), "the viewports' background is the new colour");
    h.cmd(Command::SetUiColor { element: "Perspective Background Top".into(), color: [200, 10, 10] });
    assert!(has_fill(&h, egui::Color32::from_rgb(200, 10, 10)), "the Perspective gradient starts with it");
    assert!(h.ctx.style().visuals.dark_mode);
    let scheme = h.find(Role::ComboBox, "Dark");
    h.click(scheme.center());
    h.click(menu_entry(&h, "Light"));
    h.frames(2);
    assert!(!h.ctx.style().visuals.dark_mode, "the Light scheme");
}

#[test]
fn track_bar_keys_take_the_colour_of_what_they_key() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    h.cmd(Command::AutoKeyModeToggle);
    h.cmd(Command::SetFrame { frame: 10 });
    h.cmd(Command::MoveSelection { delta: [5.0, 0.0, 0.0], system: None });
    h.cmd(Command::SetFrame { frame: 20 });
    h.cmd(Command::RotateSelection { axis: loom_commands::Axis::Z, degrees: 30.0, system: None });
    h.cmd(Command::SetFrame { frame: 0 });
    h.frames(2);
    let pos = egui::Color32::from_rgb(220, 60, 60);
    let rot = egui::Color32::from_rgb(70, 200, 70);
    assert!(has_fill(&h, pos), "a position key is red");
    assert!(has_fill(&h, rot), "a rotation key is green");
    h.cmd(Command::SetUiColor { element: "Rotation Keys".into(), color: [1, 2, 250] });
    assert!(has_fill(&h, egui::Color32::from_rgb(1, 2, 250)) && !has_fill(&h, rot), "Colors > Track Bar sets it");
}
