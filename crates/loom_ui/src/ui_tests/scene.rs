//! Scene dialogs driven like a person: Select From Scene, Attach List,
//! Merge, Layer Manager, Layers toolbar, Display Properties, Select and Link.

use super::*;
use loom_scene::NodeId;

/// A named box at `x`, selected.
fn named_box(h: &mut Harness, name: &str, x: f32) -> NodeId {
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [10.0; 3] });
    let id = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::Rename { id: id.0, name: name.into() });
    id
}

fn sel(h: &Harness) -> Vec<NodeId> {
    h.app.state.selection.iter().copied().collect()
}

fn open_dialog(h: &Harness) -> Option<loom_commands::Dialog> {
    h.app.state.dialog
}

/// The texts reading `text` inside the window titled `window`.
fn in_window(h: &Harness, window: &str, text: &str) -> Vec<egui::Pos2> {
    let area = h.ctx.memory(|m| m.area_rect(egui::Id::new(window))).unwrap_or_else(|| panic!("no {window} window"));
    h.texts(text).into_iter().filter(|p| area.contains(*p)).collect()
}

/// The lowest text `text` in the window (its OK-row button, below a menu of the same name).
fn button(h: &Harness, window: &str, text: &str) -> egui::Pos2 {
    let all = in_window(h, window, text);
    *all.iter().max_by(|a, b| a.y.total_cmp(&b.y)).unwrap_or_else(|| panic!("no {text:?} in {window}"))
}

/// Clicks the menu `menu` of the window `window`, then its item `item`.
fn window_menu(h: &mut Harness, window: &str, menu: &str, item: &str) {
    let p = *in_window(h, window, menu).iter().min_by(|a, b| a.y.total_cmp(&b.y)).unwrap();
    h.click(p);
    let q = h.texts(item);
    assert!(!q.is_empty(), "no {item:?} in the {menu} menu");
    h.click(q[q.len() - 1]);
}

#[test]
fn select_from_scene_highlights_the_selection_click_replaces_ctrl_adds_and_select_applies() {
    let mut h = Harness::new();
    let a = named_box(&mut h, "Alpha", 0.0);
    let b = named_box(&mut h, "Beta", 20.0);
    let c = named_box(&mut h, "Gamma", 40.0);
    h.key(egui::Key::H);
    assert_eq!(open_dialog(&h), Some(loom_commands::Dialog::SelectFromScene));
    // Gamma (the selection) starts highlighted: Select right away keeps it.
    let p = window_text(&h, "Select From Scene", "Alpha");
    h.click(p);
    pause(&mut h);
    let p = window_text(&h, "Select From Scene", "Beta");
    h.mods = egui::Modifiers::CTRL;
    h.click(p);
    h.mods = egui::Modifiers::NONE;
    let p = button(&h, "Select From Scene", "Select");
    h.click(p);
    assert_eq!(sel(&h), vec![a, b], "a click replaces the highlight, Ctrl+click adds");
    assert_eq!(open_dialog(&h), None);
    let _ = c;
}

#[test]
fn select_from_scene_starts_with_the_selection_and_ctrl_click_removes_from_it() {
    let mut h = Harness::new();
    let a = named_box(&mut h, "Alpha", 0.0);
    let b = named_box(&mut h, "Beta", 20.0);
    h.cmd(Command::SelectNodes { ids: vec![a.0, b.0], op: loom_commands::SelectOp::Replace });
    h.key(egui::Key::H);
    let p = window_text(&h, "Select From Scene", "Beta");
    h.mods = egui::Modifiers::CTRL;
    h.click(p);
    h.mods = egui::Modifiers::NONE;
    let p = button(&h, "Select From Scene", "Select");
    h.click(p);
    assert_eq!(sel(&h), vec![a]);
}

#[test]
fn double_clicking_a_name_selects_it_and_closes() {
    let mut h = Harness::new();
    let a = named_box(&mut h, "Alpha", 0.0);
    named_box(&mut h, "Beta", 20.0);
    h.key(egui::Key::H);
    let p = window_text(&h, "Select From Scene", "Alpha");
    h.click(p);
    h.click(p);
    assert_eq!(open_dialog(&h), None, "the double-click closed the dialog");
    assert_eq!(sel(&h), vec![a]);
}

#[test]
fn display_children_shows_the_hierarchy_and_expand_all_opens_it() {
    let mut h = Harness::new();
    let parent = named_box(&mut h, "Gondola", 0.0);
    let child = named_box(&mut h, "Wing", 20.0);
    let grandchild = named_box(&mut h, "Aileron", 40.0);
    h.cmd(Command::SetParent { id: child.0, parent: Some(parent.0) });
    h.cmd(Command::SetParent { id: grandchild.0, parent: Some(child.0) });
    h.key(egui::Key::H);
    assert!(in_window(&h, "Select From Scene", "Wing").is_empty(), "children start collapsed under their parent");
    window_menu(&mut h, "Select From Scene", "Display", "Expand All");
    let g = window_text(&h, "Select From Scene", "Gondola");
    let w = window_text(&h, "Select From Scene", "Wing");
    let ai = window_text(&h, "Select From Scene", "Aileron");
    assert!(g.y < w.y && w.y < ai.y, "parent above child above grandchild");
    let area = h.ctx.memory(|m| m.area_rect(egui::Id::new("Select From Scene"))).unwrap();
    let left = |t: &str| h.text_rect(t, area).unwrap().min.x;
    assert!(left("Gondola") < left("Wing") && left("Wing") < left("Aileron"), "each level indented");
    // Shift+click selects the range Gondola..Aileron.
    h.click(g);
    pause(&mut h);
    h.mods = egui::Modifiers::SHIFT;
    h.click(ai);
    h.mods = egui::Modifiers::NONE;
    let p = button(&h, "Select From Scene", "Select");
    h.click(p);
    assert_eq!(sel(&h), vec![parent, child, grandchild]);
}

#[test]
fn select_from_scene_does_not_list_hidden_or_frozen_objects() {
    let mut h = Harness::new();
    let a = named_box(&mut h, "Alpha", 0.0);
    let b = named_box(&mut h, "Beta", 20.0);
    let c = named_box(&mut h, "Gamma", 40.0);
    h.cmd(Command::HideNodes { ids: vec![b.0] });
    h.cmd(Command::FreezeNodes { ids: vec![c.0] });
    h.cmd(Command::SelectNodes { ids: vec![], op: loom_commands::SelectOp::Replace });
    h.key(egui::Key::H);
    assert!(in_window(&h, "Select From Scene", "Beta").is_empty() && in_window(&h, "Select From Scene", "Gamma").is_empty());
    let p = window_text(&h, "Select From Scene", "Select All");
    h.click(p);
    let p = button(&h, "Select From Scene", "Select");
    h.click(p);
    assert_eq!(sel(&h), vec![a], "Select All takes only the listed objects");
}

#[test]
fn attach_list_has_a_scene_root_row_and_select_all_attaches_only_listed_objects() {
    let mut h = Harness::new();
    let target = named_box(&mut h, "Lightning", 0.0);
    h.cmd(Command::ConvertToEditablePoly);
    let b1 = named_box(&mut h, "Blade01", 20.0);
    let b2 = named_box(&mut h, "Blade02", 40.0);
    let hidden = named_box(&mut h, "Hidden", 60.0);
    h.cmd(Command::HideNodes { ids: vec![hidden.0] });
    h.cmd(Command::CreateCamera { target: false });
    h.cmd(Command::SelectNodes { ids: vec![target.0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::AttachList);
    let root = window_text(&h, "Attach List", "Scene Root");
    let blade = window_text(&h, "Attach List", "Blade01");
    assert!(root.y < blade.y && root.x < blade.x, "the objects hang under Scene Root");
    let p = window_text(&h, "Attach List", "Select All");
    h.click(p);
    h.mods = egui::Modifiers::CTRL;
    h.click(root);
    h.mods = egui::Modifiers::NONE;
    let p = window_text(&h, "Attach List", "Attach");
    h.click(p);
    assert_eq!(open_dialog(&h), None);
    assert!(h.app.state.scene.node(b1).is_none() && h.app.state.scene.node(b2).is_none(), "both blades attached");
    assert!(h.app.state.scene.node(hidden).is_some(), "the hidden box was not listed and stays");
    assert_eq!(h.app.state.scene.nodes().filter(|n| matches!(n.object, loom_scene::ObjectKind::Camera { .. })).count(), 1, "the camera stays");
}

#[test]
fn h_in_select_and_link_is_select_parent_and_links_the_selection() {
    let mut h = Harness::new();
    let parent = named_box(&mut h, "Gondola", 0.0);
    let child = named_box(&mut h, "Wing", 20.0);
    h.cmd(Command::SelectAndLink);
    h.key(egui::Key::H);
    assert!(in_window(&h, "Select Parent", "Wing").is_empty(), "the child is not offered as its own parent");
    let p = window_text(&h, "Select Parent", "Gondola");
    h.click(p);
    let p = window_text(&h, "Select Parent", "Link");
    h.click(p);
    assert_eq!(h.app.state.scene.node(child).unwrap().parent, Some(parent));
}

/// The texts reading `text` inside the area `id`.
fn in_area(h: &Harness, id: egui::Id, text: &str) -> Vec<egui::Pos2> {
    let area = h.ctx.memory(|m| m.area_rect(id)).expect("the window is open");
    h.texts(text).into_iter().filter(|p| area.contains(*p)).collect()
}

fn one(v: Vec<egui::Pos2>, what: &str) -> egui::Pos2 {
    assert_eq!(v.len(), 1, "{what}: {} matches", v.len());
    v[0]
}

#[test]
fn select_and_link_drags_from_the_child_to_the_parent() {
    let mut h = Harness::new();
    let parent = named_box(&mut h, "Parent", 0.0);
    let child = named_box(&mut h, "Child", 40.0);
    h.cmd(Command::SelectAll);
    framed(&mut h);
    h.cmd(Command::SelectNodes { ids: vec![parent.0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::SelectAndLink);
    let from = h.screen_of(Vec3::new(42.0, 2.0, 10.0));
    let to = h.screen_of(Vec3::new(2.0, 2.0, 10.0));
    h.drag(from, to);
    assert_eq!(h.app.state.scene.node(child).unwrap().parent, Some(parent), "the dragged-from object is linked to the one under the release");
    assert_eq!(sel(&h), vec![child], "the child is the one selected");
    assert!((h.app.state.scene.world_matrix(child).w_axis.x - 40.0).abs() < 1e-3, "linking keeps it in place");
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::Link), "the tool stays on");
}

#[test]
fn display_properties_apply_to_every_selected_object() {
    let mut h = Harness::new();
    let a = named_box(&mut h, "A", 0.0);
    let b = named_box(&mut h, "B", 20.0);
    h.cmd(Command::SelectNodes { ids: vec![a.0, b.0], op: loom_commands::SelectOp::Replace });
    maximized(&mut h);
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 1400.0));
    h.app.panel_tab = crate::Tab::Display;
    h.frames(3);
    let undo = h.app.state.undo.len();
    let p = panel_text(&h, "See-Through");
    h.click(p);
    let s = &h.app.state.scene;
    assert!(s.node(a).unwrap().display.see_through && s.node(b).unwrap().display.see_through, "both objects");
    assert_eq!(h.app.state.undo.len(), undo + 1, "one undo step");
}

#[test]
fn merge_starts_with_nothing_highlighted_and_asks_about_duplicate_names() {
    let dir = std::env::temp_dir().join("loom-ui-merge-dup");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("props.loom");
    {
        let mut src = Harness::new();
        named_box(&mut src, "Blade", 100.0);
        named_box(&mut src, "Hub", 120.0);
        named_box(&mut src, "Nose", 140.0);
        src.cmd(Command::SaveFile { path: Some(path.to_string_lossy().into_owned()) });
    }
    let mut h = Harness::new();
    let old = named_box(&mut h, "Blade", 0.0);
    h.app.state.merge_pending = Some((path.clone(), vec!["Blade".into(), "Hub".into(), "Nose".into()]));
    h.app.state.dialog = Some(loom_commands::Dialog::MergeObjects);
    h.frames(2);
    let title = "Merge - props.loom";
    let ok = window_text(&h, title, "OK");
    h.click(ok);
    assert_eq!(h.app.state.scene.len(), 1, "OK does nothing with nothing highlighted");
    let p = window_text(&h, title, "Blade");
    h.click(p);
    pause(&mut h);
    let p = window_text(&h, title, "Hub");
    h.mods = egui::Modifiers::CTRL;
    h.click(p);
    h.mods = egui::Modifiers::NONE;
    let ok = window_text(&h, title, "OK");
    h.click(ok);
    // Blade is taken: the Duplicate Name question (a new window shows after its sizing frame).
    h.frames(2);
    let p = window_text(&h, "Duplicate Name", "Delete Old");
    h.click(p);
    let mut names: Vec<String> = h.app.state.scene.nodes().map(|n| n.name.clone()).collect();
    names.sort();
    assert_eq!(names, ["Blade", "Hub"], "Nose was not highlighted; the old Blade was replaced");
    assert!(h.app.state.scene.node(old).is_none());
    assert_eq!(h.app.state.dialog, None);
}

#[test]
fn layer_manager_render_colour_hide_all_and_the_object_list() {
    let mut h = Harness::new();
    let a = named_box(&mut h, "Crate", 0.0);
    h.cmd(Command::CreateLayer { name: Some("Props".into()) });
    let props = h.app.state.scene.current_layer;
    h.cmd(Command::SelectNodes { ids: vec![], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::LayerManager);
    let win = egui::Id::new("layer_manager").with("window");
    assert!(h.ctx.memory(|m| m.area_rect(win)).is_some());
    assert!(!h.texts("Layer: Props").is_empty(), "the title names the current layer");
    // Expand Props: its object is listed; a click selects it.
    let plus: Vec<egui::Pos2> = in_area(&h, win, "+");
    assert_eq!(plus.len(), 2, "one expand button per layer");
    let props_plus = *plus.iter().max_by(|a, b| a.y.total_cmp(&b.y)).unwrap();
    h.click(props_plus);
    let p = one(in_area(&h, win, "Crate"), "Crate row");
    h.click(p);
    assert_eq!(sel(&h), vec![a]);
    let p = one(in_area(&h, win, "Hide All Layers"), "Hide All");
    h.click(p);
    assert!(h.app.state.scene.layers.iter().all(|l| l.hidden));
    let p = one(in_area(&h, win, "Unhide All Layers"), "Unhide All");
    h.click(p);
    assert!(h.app.state.scene.layers.iter().all(|l| !l.hidden));
    h.cmd(Command::SetLayerRenderable { layer: props, on: false });
    assert!(!h.app.state.scene.layer(props).unwrap().renderable);
}

#[test]
fn layers_toolbar_sets_the_current_layer_and_adds_the_selection() {
    let mut h = Harness::new();
    let a = named_box(&mut h, "Crate", 0.0);
    h.cmd(Command::CreateLayer { name: Some("Props".into()) });
    let props = h.app.state.scene.current_layer;
    h.cmd(Command::SetCurrentLayer { layer: 0 });
    assert!(h.texts("Set Current Layer to Selection's Layer").is_empty(), "the toolbar starts hidden");
    h.cmd(Command::ShowLayersToolbarToggle);
    let p = one(h.texts("Set Current Layer to Selection's Layer"), "toolbar button");
    h.click(p);
    assert_eq!(h.app.state.scene.current_layer, props);
    // The layer list: pick 0 (default) as current, then add the selection to it.
    let p = one(h.texts("Props"), "the list shows the current layer");
    h.click(p);
    let rows = h.texts("0 (default)");
    h.click(rows[rows.len() - 1]);
    assert_eq!(h.app.state.scene.current_layer, 0);
    let p = one(h.texts("Add Selection to Current Layer"), "toolbar button");
    h.click(p);
    assert_eq!(h.app.state.scene.node(a).unwrap().layer, 0);
    h.cmd(Command::SelectNodes { ids: vec![], op: loom_commands::SelectOp::Replace });
    let p = one(h.texts("Select Objects in Current Layer"), "toolbar button");
    h.click(p);
    assert_eq!(sel(&h), vec![a]);
}

#[test]
fn new_on_a_scene_with_objects_asks_what_to_keep() {
    let mut h = Harness::new();
    let parent = named_box(&mut h, "P", 0.0);
    let child = named_box(&mut h, "C", 20.0);
    h.cmd(Command::SetParent { id: child.0, parent: Some(parent.0) });
    h.app.state.undo.mark_saved();
    h.mods = egui::Modifiers::CTRL;
    h.key(egui::Key::N);
    h.mods = egui::Modifiers::NONE;
    assert_eq!(open_dialog(&h), Some(loom_commands::Dialog::NewScene));
    let p = window_text(&h, "New Scene", "Keep Objects and Hierarchy");
    h.click(p);
    let p = window_text(&h, "New Scene", "OK");
    h.click(p);
    assert_eq!(open_dialog(&h), None);
    assert_eq!(h.app.state.scene.len(), 2);
    assert_eq!(h.app.state.scene.node(child).unwrap().parent, Some(parent));
    // New All on the kept scene.
    h.mods = egui::Modifiers::CTRL;
    h.key(egui::Key::N);
    h.mods = egui::Modifiers::NONE;
    let p = window_text(&h, "New Scene", "New All");
    h.click(p);
    let p = window_text(&h, "New Scene", "OK");
    h.click(p);
    assert_eq!(h.app.state.scene.len(), 0);
}

#[test]
fn reset_asks_do_you_really_want_to_reset() {
    let mut h = Harness::new();
    named_box(&mut h, "A", 0.0);
    h.app.state.undo.mark_saved();
    h.app.request_file_action(loom_commands::PendingFileAction::Reset);
    h.frames(2);
    assert_eq!(open_dialog(&h), Some(loom_commands::Dialog::ConfirmReset));
    let win = egui::Id::new("confirm_reset");
    let p = one(in_area(&h, win, "No"), "No");
    h.click(p);
    assert_eq!(h.app.state.scene.len(), 1, "No keeps the scene");
    h.app.request_file_action(loom_commands::PendingFileAction::Reset);
    h.frames(2);
    let p = one(in_area(&h, win, "Yes"), "Yes");
    h.click(p);
    assert_eq!(h.app.state.scene.len(), 0);
}

#[test]
fn group_asks_for_the_group_name() {
    let mut h = Harness::new();
    let a = named_box(&mut h, "A", 0.0);
    let b = named_box(&mut h, "B", 20.0);
    h.cmd(Command::SelectNodes { ids: vec![a.0, b.0], op: loom_commands::SelectOp::Replace });
    h.app.state.dialog = Some(loom_commands::Dialog::GroupName);
    h.frames(2);
    let field = one(in_window(&h, "Group", "Group001"), "the proposed name");
    h.click(field);
    h.key(egui::Key::End);
    for _ in 0.."Group001".len() {
        h.key(egui::Key::Backspace);
    }
    h.run(vec![egui::Event::Text("Propeller".into())]);
    h.frames(1);
    let p = window_text(&h, "Group", "OK");
    h.click(p);
    let head = sel(&h)[0];
    let n = h.app.state.scene.node(head).unwrap();
    assert!(matches!(n.object, loom_scene::ObjectKind::Group { .. }));
    assert_eq!(n.name, "Propeller");
    assert_eq!(open_dialog(&h), None);
}

#[test]
fn summary_info_shows_totals_and_keeps_the_description() {
    let mut h = Harness::new();
    named_box(&mut h, "A", 0.0);
    h.cmd(Command::SummaryInfo);
    assert!(!in_window(&h, "Summary Info", "Faces").is_empty());
    h.cmd(Command::SetFileProperty { name: "Description".into(), value: "Crates".into() });
    let p = window_text(&h, "Summary Info", "OK");
    h.click(p);
    assert_eq!(h.app.state.file_properties.description, "Crates");
    assert_eq!(open_dialog(&h), None);
}

#[test]
fn fbx_export_dialog_sets_the_options_and_ok_writes_the_file() {
    let dir = std::env::temp_dir().join("loom-ui-fbx-export");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("out.fbx");
    let _ = std::fs::remove_file(&path);
    let mut h = Harness::new();
    named_box(&mut h, "A", 0.0);
    h.app.state.fbx_pending = Some(path.clone());
    h.app.state.dialog = Some(loom_commands::Dialog::FbxExport { selected: false });
    h.frames(2);
    let p = window_text(&h, "FBX Export", "Y-up");
    h.click(p);
    let p = window_text(&h, "FBX Export", "Animation");
    h.click(p);
    assert!(h.app.state.fbx_options.y_up && !h.app.state.fbx_options.animation);
    let p = window_text(&h, "FBX Export", "OK");
    h.click(p);
    assert!(path.is_file(), "OK wrote the file");
    assert_eq!(open_dialog(&h), None);
}
