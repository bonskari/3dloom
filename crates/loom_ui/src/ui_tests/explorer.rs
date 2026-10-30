//! Scene Explorer and Schematic View driven like a person: menus, clicks,
//! icon drags, column headings, F2 and the Connect tool.

use super::*;
use egui::accesskit::Role;
use loom_scene::NodeId;

fn named_box(h: &mut Harness, name: &str, x: f32) -> NodeId {
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [10.0; 3] });
    let id = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::Rename { id: id.0, name: name.into() });
    id
}

fn area(h: &Harness, id: egui::Id) -> egui::Rect {
    h.ctx.memory(|m| m.area_rect(id)).expect("the window is shown")
}

fn explorer_area(h: &Harness) -> egui::Rect {
    area(h, crate::scene_explorer::window_id(h.app.state.explorers.list[0].id))
}

fn schematic_area(h: &Harness) -> egui::Rect {
    area(h, crate::schematic_view::window_id(h.app.state.schematics.list[0].id))
}

/// The one text `text` painted inside `within`.
fn text_in(h: &Harness, within: egui::Rect, text: &str) -> egui::Pos2 {
    let all: Vec<egui::Pos2> = h.text_centers(text).into_iter().filter(|p| within.contains(*p)).collect();
    assert_eq!(all.len(), 1, "{text:?} shown {} times in {within:?}", all.len());
    all[0]
}

/// The one widget with this role and name inside `within`.
fn widget_in(h: &Harness, within: egui::Rect, role: Role, name: &str) -> egui::Rect {
    let all: Vec<egui::Rect> = h.find_all(role, name).into_iter().filter(|r| within.contains(r.center())).collect();
    assert_eq!(all.len(), 1, "{role:?} {name:?} shown {} times", all.len());
    all[0]
}

fn sel(h: &Harness) -> Vec<NodeId> {
    h.app.state.selection.iter().copied().collect()
}

fn open_explorer(h: &mut Harness) {
    h.cmd(Command::NewSceneExplorer);
    h.frames(3);
}

#[test]
fn tools_new_scene_explorer_opens_a_window_whose_rows_select_in_the_scene() {
    let mut h = Harness::new();
    named_box(&mut h, "Alpha", 0.0);
    let b = named_box(&mut h, "Beta", 20.0);
    h.cmd(Command::SelectNone);
    h.click(h.find(Role::Button, "Tools").center());
    h.click(h.find(Role::Button, "New Scene Explorer").center());
    h.frames(3);
    assert_eq!(h.app.state.explorers.list.len(), 1);
    assert!(h.app.state.explorers.list[0].open);
    let p = text_in(&h, explorer_area(&h), "Beta");
    h.click(p);
    assert_eq!(sel(&h), vec![b], "Sync Selection: the row's object is selected");
    // Typing in Find highlights the matching names.
    let find = widget_in(&h, explorer_area(&h), Role::TextInput, "Find");
    h.click(find.center());
    h.run(vec![egui::Event::Text("al".into())]);
    h.frames(2);
    assert_eq!(h.app.state.scene.node(sel(&h)[0]).unwrap().name, "Alpha");
}

#[test]
fn dragging_an_icon_onto_a_row_links_and_onto_scene_root_unlinks() {
    let mut h = Harness::new();
    let a = named_box(&mut h, "Alpha", 0.0);
    let b = named_box(&mut h, "Beta", 20.0);
    h.cmd(Command::SelectNone);
    open_explorer(&mut h);
    let icon = widget_in(&h, explorer_area(&h), Role::Button, "Beta icon");
    let target = text_in(&h, explorer_area(&h), "Alpha");
    let undo = h.app.state.undo.len();
    h.drag(icon.center(), target);
    h.frames(2);
    assert_eq!(h.app.state.scene.node(b).unwrap().parent, Some(a), "Beta is Alpha's child");
    assert_eq!(h.app.state.undo.len(), undo + 1, "one undo step");
    // The child now sits under its parent, indented.
    let (pa, pb) = (text_in(&h, explorer_area(&h), "Alpha"), text_in(&h, explorer_area(&h), "Beta"));
    assert!(pb.y > pa.y && pb.x > pa.x, "Beta indented below Alpha: {pa:?} {pb:?}");
    let icon = widget_in(&h, explorer_area(&h), Role::Button, "Beta icon");
    let root = text_in(&h, explorer_area(&h), "Scene Root");
    h.drag(icon.center(), root);
    h.frames(2);
    assert_eq!(h.app.state.scene.node(b).unwrap().parent, None, "dropped on Scene Root: unlinked");
}

#[test]
fn configure_columns_adds_a_column_whose_cells_set_every_highlighted_row() {
    let mut h = Harness::new();
    let a = named_box(&mut h, "Alpha", 0.0);
    let b = named_box(&mut h, "Beta", 20.0);
    open_explorer(&mut h);
    let heading = widget_in(&h, explorer_area(&h), Role::Button, "Name");
    right_click(&mut h, heading.center());
    h.frames(2);
    h.click(h.find(Role::Button, "Configure Columns...").center());
    h.frames(2);
    h.click(h.find(Role::CheckBox, "Renderable").center());
    h.frames(2);
    assert_eq!(h.app.state.explorers.list[0].columns, vec![loom_commands::explorer::Column::Name, loom_commands::explorer::Column::Renderable]);
    h.cmd(Command::SelectNodes { ids: vec![a.0, b.0], op: loom_commands::SelectOp::Replace });
    let cell = widget_in(&h, explorer_area(&h), Role::CheckBox, "Alpha Renderable");
    h.click(cell.center());
    h.frames(2);
    assert!(!h.app.state.scene.node(a).unwrap().render.renderable && !h.app.state.scene.node(b).unwrap().render.renderable, "both highlighted rows changed");
    h.cmd(Command::UndoSceneOperation);
    assert!(h.app.state.scene.node(b).unwrap().render.renderable, "in one undo step");
    // A click on the heading sorts by it, a second one reverses.
    let heading = widget_in(&h, explorer_area(&h), Role::Button, "Name");
    h.click(heading.center());
    h.frames(2);
    assert_eq!(h.app.state.explorers.list[0].sort, vec![(loom_commands::explorer::Column::Name, true)]);
    pause(&mut h);
    h.click(heading.center());
    h.frames(2);
    assert_eq!(h.app.state.explorers.list[0].sort, vec![(loom_commands::explorer::Column::Name, false)]);
    let (pa, pb) = (text_in(&h, explorer_area(&h), "Alpha"), text_in(&h, explorer_area(&h), "Beta"));
    assert!(pb.y < pa.y, "descending: Beta above Alpha");
}

#[test]
fn f2_edits_the_active_rows_name_and_the_other_highlighted_rows_are_numbered() {
    let mut h = Harness::new();
    let a = named_box(&mut h, "Alpha", 0.0);
    let b = named_box(&mut h, "Beta", 20.0);
    h.cmd(Command::SelectNone);
    open_explorer(&mut h);
    h.click(text_in(&h, explorer_area(&h), "Alpha"));
    pause(&mut h);
    h.mods = egui::Modifiers::CTRL;
    h.click(text_in(&h, explorer_area(&h), "Beta"));
    h.mods = egui::Modifiers::NONE;
    assert_eq!(sel(&h).len(), 2);
    let shading = h.app.state.shade_selected_faces;
    h.key(egui::Key::F2);
    h.frames(2);
    assert_eq!(h.app.state.shade_selected_faces, shading, "F2 over the explorer is not Shade Selected Faces");
    h.run(vec![egui::Event::Text("door".into())]);
    h.key(egui::Key::Enter);
    h.frames(2);
    assert_eq!(h.app.state.scene.node(b).unwrap().name, "door", "the active (last clicked) row takes the name");
    assert_eq!(h.app.state.scene.node(a).unwrap().name, "door01");
}

#[test]
fn display_toolbar_hides_a_type_from_the_list() {
    let mut h = Harness::new();
    named_box(&mut h, "Alpha", 0.0);
    h.cmd(Command::CreateShape { kind: loom_commands::ShapeKind::Circle, position: [0.0; 3], size: [5.0; 3] });
    let circle = h.app.state.scene.node(*h.app.state.selection.iter().next().unwrap()).unwrap().name.clone();
    open_explorer(&mut h);
    assert_eq!(h.text_centers(&circle).into_iter().filter(|p| explorer_area(&h).contains(*p)).count(), 1);
    h.click(widget_in(&h, explorer_area(&h), Role::Button, "Display Shapes").center());
    h.frames(2);
    assert_eq!(h.text_centers(&circle).into_iter().filter(|p| explorer_area(&h).contains(*p)).count(), 0, "shapes are no longer listed");
}

#[test]
fn schematic_view_button_opens_it_and_selection_syncs_both_ways() {
    let mut h = Harness::new();
    let a = named_box(&mut h, "Alpha", 0.0);
    let b = named_box(&mut h, "Beta", 20.0);
    h.cmd(Command::SelectNone);
    h.click(h.find(Role::Button, "Schematic View (Open)").center());
    h.frames(3);
    assert_eq!(h.app.state.schematics.list.len(), 1);
    let beta = widget_in(&h, schematic_area(&h), Role::Button, "Beta");
    h.click(beta.center());
    assert_eq!(sel(&h), vec![b], "a click in the Schematic View selects in the viewports");
    h.cmd(Command::SelectNodes { ids: vec![a.0], op: loom_commands::SelectOp::Replace });
    let v = &h.app.state.schematics.list[0];
    assert_eq!(loom_commands::schematic::selected(&h.app.state, v).into_iter().collect::<Vec<_>>(), vec![format!("obj:{}", a.0)], "and the viewport selection shows in it");
    // An empty click clears both.
    let r = schematic_area(&h);
    h.click(egui::pos2(r.right() - 40.0, r.bottom() - 60.0));
    assert!(sel(&h).is_empty());
}

#[test]
fn schematic_connect_drags_a_child_onto_its_parent_and_select_drags_move_boxes() {
    let mut h = Harness::new();
    let a = named_box(&mut h, "Alpha", 0.0);
    let b = named_box(&mut h, "Beta", 20.0);
    h.cmd(Command::SelectNone);
    h.cmd(Command::NewSchematicView);
    h.frames(2);
    // Select: dragging a box moves it and frees it.
    let beta = widget_in(&h, schematic_area(&h), Role::Button, "Beta");
    h.drag(beta.center(), beta.center() + egui::vec2(120.0, 40.0));
    h.frames(2);
    let key = format!("obj:{}", b.0);
    assert!(h.app.state.schematics.list[0].free.contains_key(&key), "the moved box is free");
    let moved = widget_in(&h, schematic_area(&h), Role::Button, "Beta");
    assert!((moved.center() - (beta.center() + egui::vec2(120.0, 40.0))).length() < 3.0, "it stays where it was dropped: {moved:?}");
    // Connect: drag from the child to the parent.
    h.click(widget_in(&h, schematic_area(&h), Role::Button, "Connect").center());
    let alpha = widget_in(&h, schematic_area(&h), Role::Button, "Alpha");
    h.drag(moved.center(), alpha.center());
    h.frames(2);
    assert_eq!(h.app.state.scene.node(b).unwrap().parent, Some(a), "connected: Beta is Alpha's child");
}
