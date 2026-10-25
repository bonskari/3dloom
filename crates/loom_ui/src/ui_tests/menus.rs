//! Spinners, the menu bar, the Quick Access and Main toolbars, clicked and
//! typed into as a person does.

use super::*;
use egui::accesskit::Role;

fn point_at(h: &mut Harness, p: egui::Pos2) {
    h.run(vec![egui::Event::PointerMoved(p)]);
    h.frames(2);
}

fn type_text(h: &mut Harness, text: &str) {
    h.run(vec![egui::Event::Text(text.to_owned())]);
    h.key(egui::Key::Enter);
}

/// Opens a menu bar menu and its submenus, then clicks the item.
fn menu_path(h: &mut Harness, path: &[&str]) {
    h.click(h.find(Role::Button, path[0]).center());
    // The entry in the open menu, not a Command Panel icon of the same name (Shapes).
    let in_menu = |h: &Harness, label: &str| {
        let cp = egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new("command_panel")).map(|p| p.rect);
        let all = h.find_all(Role::Button, label);
        all.iter().copied().find(|r| !cp.is_some_and(|c| c.contains(r.center()))).unwrap_or_else(|| panic!("no menu entry {label:?}: {all:?}")).center()
    };
    for sub in &path[1..path.len() - 1] {
        let p = in_menu(h, sub);
        point_at(h, p);
    }
    let p = in_menu(h, path[path.len() - 1]);
    h.click(p);
}

/// The widget with this role right of `rect` on the same row.
fn right_of(h: &Harness, role: Role, rect: egui::Rect) -> egui::Rect {
    h.widgets
        .iter()
        .filter(|(r, _, w)| *r == role && w.min.x >= rect.max.x - 1.0 && (w.center().y - rect.center().y).abs() < rect.height())
        .min_by(|a, b| a.2.min.x.total_cmp(&b.2.min.x))
        .map(|w| w.2)
        .unwrap_or_else(|| panic!("no {role:?} right of {rect:?}"))
}

fn selected(h: &Harness) -> loom_scene::NodeId {
    *h.app.state.selection.iter().next().expect("an object is selected")
}

#[test]
fn right_click_on_a_spinner_sets_it_to_zero() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [40.0, 20.0, 0.0], size: [10.0; 3] });
    h.key(egui::Key::W);
    let x = h.spinner_after("X:");
    right_click(&mut h, x.center());
    h.frames(2);
    let p = h.app.state.scene.node(selected(&h)).unwrap().transform.position.get();
    assert_eq!((p.x, p.y), (0.0, 20.0), "the status bar X field went to 0, Y stayed");
    assert!(h.app.quad_menu.is_none());
}

#[test]
fn right_click_on_a_spinner_with_a_minimum_sets_the_minimum_in_one_undo_step() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let id = selected(&h);
    h.cmd(Command::SetParameter { id: id.0, name: "Length Segs".into(), value: 5.0 });
    h.app.panel_tab = crate::Tab::Modify;
    h.frames(2);
    let s = h.spinner_after("Length Segs");
    right_click(&mut h, s.center());
    h.frames(2);
    assert_eq!(param(&h, "Length Segs"), 1.0, "Length Segs cannot be 0: its minimum is 1");
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(param(&h, "Length Segs"), 5.0);
}

#[test]
fn modifiers_menu_adds_modifiers_from_its_categories() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let id = selected(&h);
    for (path, name) in [
        (["Modifiers", "Mesh Editing", "Edit Mesh"], "Edit Mesh"),
        (["Modifiers", "Mesh Editing", "Vertex Weld"], "Vertex Weld"),
        (["Modifiers", "Subdivision Surfaces", "HSDS Modifier"], "HSDS"),
        (["Modifiers", "Parametric Deformers", "Taper"], "Taper"),
    ] {
        menu_path(&mut h, &path);
        let top = h.app.state.scene.node(id).unwrap().modifiers.last().map(|m| m.name.clone());
        assert_eq!(top.as_deref(), Some(name), "{path:?}");
        assert!(!h.has(Role::Button, path[1]), "the menu closed after {path:?}");
    }
}

#[test]
fn create_menu_shapes_and_compound_start_their_tools() {
    let mut h = Harness::new();
    menu_path(&mut h, &["Create", "Shapes", "Line"]);
    assert_eq!(h.app.create_kind, Some(crate::CreateKind::Line));
    assert_eq!((h.app.create_category, h.app.panel_tab), (crate::Category::Shapes, crate::Tab::Create));
    menu_path(&mut h, &["Create", "Shapes", "Circle"]);
    assert_eq!(h.app.create_kind, Some(crate::CreateKind::Shape(loom_commands::ShapeKind::Circle)));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Sphere, position: [0.0; 3], size: [20.0; 3] });
    menu_path(&mut h, &["Create", "Compound", "Boolean"]);
    assert!(h.app.create_compound && h.app.create_category == crate::Category::Geometry);
    assert_eq!(h.app.compound_kind, Some(crate::command_panel::compound::CompoundKind::Boolean));
    assert!(h.has(Role::Button, "Pick Operand B"), "the Create panel shows the Boolean rollouts");
}

#[test]
fn scale_flyout_chooses_uniform_non_uniform_or_squash() {
    let mut h = Harness::new();
    // The Scale button is named after the current type; the flyout list is right of it.
    for (button, item, kind) in [
        ("Select and Uniform Scale", "Select and Non-uniform Scale", loom_commands::ScaleKind::NonUniform),
        ("Select and Non-uniform Scale", "Select and Squash", loom_commands::ScaleKind::Squash),
        ("Select and Squash", "Select and Uniform Scale", loom_commands::ScaleKind::Uniform),
    ] {
        let flyout = right_of(&h, Role::ComboBox, h.find(Role::Button, button));
        h.click(flyout.center());
        h.click(h.find(Role::Button, item).center());
        assert_eq!((h.app.state.active_tool, h.app.state.scale_kind), (loom_commands::Tool::Scale, kind), "{item}");
    }
}

/// Drags the Scale gizmo's X axis handle outwards; returns the box's scale.
fn drag_scale_x_handle(h: &mut Harness) -> Vec3 {
    let i = h.app.state.active_viewport;
    let rect = h.app.viewport_rects[i];
    let cam = h.app.camera_for(i);
    let aspect = rect.width() / rect.height();
    let projection = crate::gizmo::Projection::new(cam.view_matrix(), cam.projection_matrix(aspect), rect);
    let layout = crate::gizmo::layout_for(&h.app, i, rect.height());
    let (o, x) = (h.screen_of(Vec3::ZERO), h.screen_of(Vec3::new(1.0, 0.0, 0.0)));
    let dir = (x - o).normalized();
    // Find the X handle along the projected X axis, as a person aims at it.
    let kind = h.app.state.scale_kind;
    h.app.state.scale_kind = loom_commands::ScaleKind::NonUniform;
    let p = (10..200).map(|d| o + dir * d as f32).find(|p| crate::gizmo::hit(&h.app, &layout, &projection, *p) == Some(crate::gizmo::Handle::Axis(loom_commands::Axis::X))).expect("the X handle");
    h.app.state.scale_kind = kind;
    h.drag(p, p + egui::vec2(60.0, -60.0));
    let s = h.app.state.scene.node(selected(h)).unwrap().transform.scale.get();
    h.cmd(Command::UndoSceneOperation);
    s
}

#[test]
fn the_scale_type_decides_what_an_axis_handle_drag_scales() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] });
    framed(&mut h);
    h.key(egui::Key::R);
    // The reference: axis and plane handles scale non-uniformly without
    // changing the flyout (its tutorial scales one local axis with Uniform Scale).
    let s = drag_scale_x_handle(&mut h);
    assert!(s.x > 1.01 && s.y == 1.0 && s.z == 1.0, "Uniform Scale tool, X handle: X only {s}");
    h.cmd(Command::ScaleType { kind: loom_commands::ScaleKind::NonUniform });
    let s = drag_scale_x_handle(&mut h);
    assert!(s.x > 1.01 && s.y == 1.0 && s.z == 1.0, "Non-Uniform: X only {s}");
    h.cmd(Command::ScaleType { kind: loom_commands::ScaleKind::Squash });
    let s = drag_scale_x_handle(&mut h);
    assert!(s.x > 1.01 && s.y < 1.0 && (s.y - s.z).abs() < 1e-4 && (s.x * s.y * s.z - 1.0).abs() < 1e-3, "Squash: the others shrink {s}");
}

#[test]
fn typing_a_name_in_the_named_selection_sets_field_makes_a_set() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let a = selected(&h);
    let field = right_of(&h, Role::TextInput, h.find(Role::Button, "Edit Named Selection Sets"));
    h.click(field.center());
    type_text(&mut h, "lit lamp");
    assert_eq!(h.app.state.named_sets.get("lit lamp"), Some(&vec![a]));
    // Choosing it from the list selects it again.
    h.cmd(Command::SelectNone);
    let list = right_of(&h, Role::ComboBox, field);
    h.click(list.center());
    h.click(h.find(Role::Button, "lit lamp").center());
    assert_eq!(h.app.state.selection.iter().copied().collect::<Vec<_>>(), vec![a]);
}

#[test]
fn at_sub_object_level_the_named_sets_field_names_and_lists_the_objects_sets() {
    let mut h = Harness::new();
    poly_box(&mut h);
    h.cmd(Command::NamedSelectionSet { name: "whole box".into() });
    h.key(egui::Key::Num1);
    let p = h.screen_of(Vec3::new(15.0, -15.0, 30.0));
    h.click(p);
    assert_eq!(h.sub_count(), 1);
    let field = right_of(&h, Role::TextInput, h.find(Role::Button, "Edit Named Selection Sets"));
    h.click(field.center());
    type_text(&mut h, "Vertex Lighting");
    assert!(h.app.state.named_sub_selections.contains_key("Vertex Lighting"), "a sub-object set, not an object set");
    assert!(!h.app.state.named_sets.contains_key("Vertex Lighting"));
    h.cmd(Command::SelectNone);
    assert_eq!(h.sub_count(), 0, "Select None at a sub-object level clears the vertices only");
    let list = right_of(&h, Role::ComboBox, field);
    h.click(list.center());
    assert!(!h.has(Role::Button, "whole box"), "object sets are not listed at sub-object level");
    h.click(h.find(Role::Button, "Vertex Lighting").center());
    assert_eq!(h.sub_count(), 1, "choosing the set restores the vertices");
}

#[test]
fn quick_access_toolbar_runs_new_undo_and_redo() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    // Icon buttons: found by their names (the topmost Undo is the Quick Access one).
    let undo = h.find(Role::Button, "Undo Scene Operation");
    assert!(undo.max.y <= h.find(Role::Button, "File").max.y + 1.0, "in the menu bar row");
    h.click(undo.center());
    assert_eq!(h.app.state.scene.len(), 0);
    h.click(h.find(Role::Button, "Redo Scene Operation").center());
    assert_eq!(h.app.state.scene.len(), 1);
    assert!(h.has(Role::Button, "Open File") && h.has(Role::Button, "Save File"));
    h.click(h.find(Role::Button, "New Scene").center());
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::SaveChanges(loom_commands::PendingFileAction::New)), "New asks to save the changed scene first");
}

#[test]
fn box_keyboard_entry_has_length_width_and_height() {
    let mut h = Harness::new();
    h.app.start_create(Some(crate::CreateKind::Primitive(PrimitiveKind::Box)));
    h.frames(2);
    h.click(h.find(Role::Button, "Keyboard Entry").center());
    for (label, value) in [("Length:", "11.532"), ("Width:", "15.85"), ("Height:", "3")] {
        let s = h.spinner_after(label);
        h.click(s.center());
        type_text(&mut h, value);
    }
    // The rollout's Create button (the menu bar's Create is above it).
    let create = *h.find_all(Role::Button, "Create").last().unwrap();
    h.click(create.center());
    assert!((param(&h, "Length") - 11.532).abs() < 1e-4, "{}", param(&h, "Length"));
    assert!((param(&h, "Width") - 15.85).abs() < 1e-4);
    assert!((param(&h, "Height") - 3.0).abs() < 1e-4);
}

#[test]
fn modifier_list_t_then_enter_adds_taper() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let id = selected(&h);
    h.app.panel_tab = crate::Tab::Modify;
    h.frames(2);
    // The Modifier List is left of the modifier-set button.
    let set_button = h.find(Role::Button, "Show / hide the modifier button set");
    let list = h.widgets.iter().filter(|(r, _, w)| *r == Role::ComboBox && w.max.x <= set_button.min.x + 1.0 && (w.center().y - set_button.center().y).abs() < set_button.height()).max_by(|a, b| a.2.min.x.total_cmp(&b.2.min.x)).map(|w| w.2).expect("the Modifier List");
    h.click(list.center());
    type_text(&mut h, "T");
    let top = h.app.state.scene.node(id).unwrap().modifiers.last().map(|m| m.name.clone());
    assert_eq!(top.as_deref(), Some("Taper"));
}
