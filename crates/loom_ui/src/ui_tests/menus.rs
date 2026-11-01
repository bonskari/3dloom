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
        (["Modifiers", "Animation Modifiers", "Skin"], "Skin"),
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

#[test]
fn tools_rename_objects_renames_the_selection() {
    let mut h = Harness::new();
    for x in [0.0, 30.0] {
        h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [10.0; 3] });
    }
    h.cmd(Command::SelectAll);
    menu_path(&mut h, &["Tools", "Rename Objects..."]);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::RenameObjects));
    let base = h.find(Role::CheckBox, "Base Name:");
    h.click(base.center());
    h.click(right_of(&h, Role::TextInput, base).center());
    h.run(vec![egui::Event::Text("Pillar".into())]);
    h.click(h.find(Role::CheckBox, "Numbered").center());
    h.click(h.find(Role::Button, "Rename").center());
    let mut names: Vec<String> = h.app.state.scene.nodes().map(|n| n.name.clone()).collect();
    names.sort();
    assert_eq!(names, ["Pillar0", "Pillar1"], "Base Number 0, Step 1");
}

#[test]
fn the_display_floater_freezes_and_hides_beside_any_panel() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let id = *h.app.state.selection.iter().next().unwrap();
    menu_path(&mut h, &["Tools", "Display Floater..."]);
    assert!(h.app.state.display_floater);
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("Display Floater"))).expect("the Display Floater window");
    // The Create panel stays on; the floater has its own Freeze Selected.
    let freeze = h.find_all(Role::Button, "Freeze Selected").into_iter().find(|r| win.contains_rect(*r)).expect("Freeze Selected on the floater");
    h.click(freeze.center());
    assert!(h.app.state.scene.node(id).unwrap().frozen, "frozen from the floater");
    // Object Level tab: Hide by Category.
    h.click(h.text_rect("Object Level", win).expect("the Object Level tab").center());
    let geometry = h.find_all(Role::CheckBox, "Geometry").into_iter().find(|r| win.contains_rect(*r) || r.min.x >= win.min.x).expect("Geometry category");
    h.click(geometry.center());
    assert!(h.app.state.hide_category.geometry);
}

/// File > References > XRef Objects...: the file dialog, then the file's
/// object list; the highlighted object comes in as a reference.
#[test]
fn file_references_xref_objects_picks_from_the_list() {
    let dir = std::env::temp_dir().join("loom-ui-xref");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("props.loom");
    {
        let mut src = Harness::new();
        src.cmd(Command::CreatePrimitive { kind: loom_commands::PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
        let id = *src.app.state.selection.iter().next().unwrap();
        src.cmd(Command::Rename { id: id.0, name: "Crate".into() });
        src.cmd(Command::CreatePrimitive { kind: loom_commands::PrimitiveKind::Box, position: [30.0, 0.0, 0.0], size: [10.0; 3] });
        src.cmd(Command::SaveFileAsCopy { path: Some(path.to_string_lossy().into_owned()) });
    }
    let mut h = Harness::new();
    h.app.native_file_dialogs = false;
    menu_path(&mut h, &["File", "References", "XRef Objects..."]);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::XrefObjectsFile));
    choose_file(&mut h, &path, "Open");
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::MergeObjects), "{}", h.app.state.status_prompt);
    h.frames(2);
    let title = "XRef Merge - props.loom";
    let p = window_text(&h, title, "Crate");
    h.click(p);
    let ok = window_text(&h, title, "OK");
    h.click(ok);
    let names: Vec<String> = h.app.state.scene.nodes().map(|n| n.name.clone()).collect();
    assert_eq!(names, ["Crate"]);
    assert_eq!(h.app.state.scene.xrefs.len(), 1);
    assert!(!h.app.state.merge_as_xref, "the next Merge is a plain merge again");
}

/// Configure Modifier Sets right-click > Show All Sets in List: the Modifier
/// List shows each set's name with its modifiers, which add as usual.
#[test]
fn show_all_sets_in_list_groups_the_modifier_list_by_set() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1400.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let id = selected(&h);
    h.app.panel_tab = crate::Tab::Modify;
    h.frames(2);
    let cfg = h.find(Role::Button, "Configure Modifier Sets").center();
    h.run(vec![egui::Event::PointerMoved(cfg)]);
    h.run(vec![egui::Event::PointerButton { pos: cfg, button: egui::PointerButton::Secondary, pressed: true, modifiers: egui::Modifiers::NONE }]);
    h.run(vec![egui::Event::PointerButton { pos: cfg, button: egui::PointerButton::Secondary, pressed: false, modifiers: egui::Modifiers::NONE }]);
    h.frames(1);
    h.click(h.find(Role::CheckBox, "Show All Sets in List").center());
    assert!(h.app.state.show_all_sets_in_list);
    let set_button = h.find(Role::Button, "Show / hide the modifier button set");
    let list = h.widgets.iter().filter(|(r, _, w)| *r == Role::ComboBox && w.max.x <= set_button.min.x + 1.0 && (w.center().y - set_button.center().y).abs() < set_button.height()).max_by(|a, b| a.2.min.x.total_cmp(&b.2.min.x)).map(|w| w.2).expect("the Modifier List");
    h.click(list.center());
    h.frames(1);
    let head = h.text_rects("MESH EDITING");
    assert_eq!(head.len(), 1, "the set names head their modifiers");
    // The set's first modifier sits right under its name.
    let edit_poly = h.find_all(Role::Button, "Edit Poly").into_iter().find(|r| r.min.y >= head[0].max.y - 2.0).expect("Edit Poly under Mesh Editing");
    assert!(edit_poly.min.y - head[0].max.y < 30.0);
    h.click(edit_poly.center());
    assert_eq!(h.app.state.scene.node(id).unwrap().modifiers.last().map(|m| m.name.as_str()), Some("Edit Poly"));
}

/// Make Unique with two of three instanced objects selected asks whether they
/// become unique with respect to each other: No gives the two one instance
/// of their own; Yes makes each unique.
#[test]
fn make_unique_with_several_selected_asks_yes_or_no() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1400.0));
    let mut ids = Vec::new();
    for x in [0.0, 30.0, 60.0] {
        h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [10.0; 3] });
        ids.push(selected(&h));
    }
    h.cmd(Command::SelectNodes { ids: vec![ids[0].0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::AddModifier { name: "Bend".into() });
    h.cmd(Command::SelectNodes { ids: vec![ids[1].0, ids[2].0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::InstanceModifier { source: ids[0].0, index: 0 });
    let inst = |h: &Harness, i: usize| h.app.state.scene.node(ids[i]).unwrap().modifiers[0].instance_of;
    assert_eq!((inst(&h, 1), inst(&h, 2)), (Some((ids[0], 0)), Some((ids[0], 0))));
    for i in [1, 2] {
        h.cmd(Command::SelectNodes { ids: vec![ids[i].0], op: loom_commands::SelectOp::Replace });
        h.cmd(Command::SelectStackEntry { index: Some(0) });
    }
    h.cmd(Command::SelectNodes { ids: vec![ids[1].0, ids[2].0], op: loom_commands::SelectOp::Replace });
    h.app.panel_tab = crate::Tab::Modify;
    h.frames(2);
    h.click(h.find(Role::Button, "Make Unique").center());
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::MakeUnique));
    h.click(h.find(Role::Button, "No").center());
    assert_eq!(h.app.state.dialog, None);
    assert_eq!(inst(&h, 1), None, "the first selected holds the new instance");
    assert_eq!(inst(&h, 2), Some((ids[1], 0)), "the other shares it");
    assert!(h.app.state.scene.node(ids[0]).unwrap().modifiers[0].instance_of.is_none());
    h.cmd(Command::UndoSceneOperation);
    assert_eq!((inst(&h, 1), inst(&h, 2)), (Some((ids[0], 0)), Some((ids[0], 0))), "one undo step");
    h.click(h.find(Role::Button, "Make Unique").center());
    h.click(h.find(Role::Button, "Yes").center());
    assert_eq!((inst(&h, 1), inst(&h, 2)), (None, None), "Yes: each unique");
}

/// Views > Show Dependencies: with the Modify panel on a box, its instance
/// shows magenta; an unrelated box keeps its colour; other tabs show none.
#[test]
fn views_show_dependencies_colours_instances_magenta() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let a = selected(&h);
    h.cmd(Command::CloneOptions { kind: loom_commands::CloneKind::Instance, copies: 1, name: None, offset: Some([30.0, 0.0, 0.0]), rotate_deg: None, scale: None });
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [-40.0, 0.0, 0.0], size: [10.0; 3] });
    h.cmd(Command::SelectNodes { ids: vec![a.0], op: loom_commands::SelectOp::Replace });
    h.app.panel_tab = crate::Tab::Modify;
    menu_path(&mut h, &["Views", "Show Dependencies"]);
    assert!(h.app.state.show_dependencies);
    h.frames(2);
    let magenta = |h: &Harness| h.app.world_styles().iter().filter(|s| s.color == [1.0, 0.0, 1.0]).count();
    assert_eq!(magenta(&h), 1, "the instance alone");
    h.app.panel_tab = crate::Tab::Create;
    h.frames(2);
    assert_eq!(magenta(&h), 0, "only with the Modify panel");
}

/// Create > Extended Primitives / Extended Shapes start their create tools.
#[test]
fn create_menu_extended_primitives_and_shapes_start_their_tools() {
    let mut h = Harness::new();
    menu_path(&mut h, &["Create", "Extended Primitives", "Capsule"]);
    assert_eq!(h.app.create_kind, Some(crate::CreateKind::Extended(loom_mesh::extended_primitives::ExtendedKind::Capsule)));
    menu_path(&mut h, &["Create", "Extended Shapes", "Wide Flange"]);
    assert_eq!(h.app.create_kind, Some(crate::CreateKind::ExtendedShape(loom_shape::parametric::ExtendedShape::WideFlange)));
}

/// Utilities: More... lists every utility and opens the chosen one;
/// Configure Button Sets puts a utility on a button and sets how many there are.
#[test]
fn utilities_more_and_configure_button_sets() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0));
    h.app.panel_tab = crate::Tab::Utilities;
    h.frames(2);
    h.click(h.find(Role::Button, "More...").center());
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("utilities_more"))).expect("the Utilities list");
    // A pause, so this click is not a double-click with the last one.
    h.frames(30);
    h.click(h.text_rect("Channel Info", win).unwrap().center());
    assert!(h.app.utilities_more, "a single click only picks it");
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("utilities_more"))).unwrap();
    h.click(h.text_rect("OK", win).unwrap_or_else(|| panic!("OK in {win:?}: {:?}", h.text_rects("OK"))).center());
    assert_eq!(h.app.utility, crate::command_panel::utilities::Utility::ChannelInfo);
    assert!(!h.app.utilities_more);
    h.click(h.find(Role::Button, "Configure Button Sets").center());
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("Configure Button Sets"))).expect("the dialog");
    // Total Buttons 2, then Panorama Exporter onto the first.
    let spin = h.spinner_after("Total Buttons:");
    h.drag(spin.center(), spin.center() + egui::vec2(0.0, 200.0));
    h.click(h.text_rect("Panorama Exporter", win).unwrap().center());
    let first = h.find_all(Role::Button, "Measure").into_iter().filter(|r| win.contains_rect(*r)).max_by(|a, b| a.min.x.total_cmp(&b.min.x)).expect("the first button (right of the list)");
    h.click(first.center());
    h.click(h.text_rect("OK", h.ctx.memory(|m| m.area_rect(egui::Id::new("Configure Button Sets"))).unwrap()).unwrap().center());
    assert_eq!(h.app.state.utility_buttons.first().map(String::as_str), Some("Panorama Exporter"));
    assert!(h.app.state.utility_buttons.len() < 10, "fewer buttons: {:?}", h.app.state.utility_buttons);
}

/// Views > Show Key Times: the key points of a shown trajectory get their frame numbers.
#[test]
fn views_show_key_times_numbers_the_trajectory_keys() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    h.cmd(Command::AutoKeyModeToggle);
    h.cmd(Command::SetFrame { frame: 37 });
    h.cmd(Command::MoveSelection { delta: [40.0, 0.0, 0.0], system: None });
    h.cmd(Command::AutoKeyModeToggle);
    h.cmd(Command::ShowTrajectoriesToggle);
    h.frames(2);
    let before = h.text_rects("37").len();
    menu_path(&mut h, &["Views", "Show Key Times"]);
    h.frames(2);
    assert_eq!(h.text_rects("37").len(), before + 1, "the key at frame 37 is numbered");
}

/// Views > Show Ghosting draws the selected box at the frame before in
/// yellow; Preferences > Ghosting > Before and After adds the frame after in cyan.
#[test]
fn views_show_ghosting_and_its_preferences() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    h.cmd(Command::AutoKeyModeToggle);
    h.cmd(Command::SetFrame { frame: 20 });
    h.cmd(Command::MoveSelection { delta: [40.0, 0.0, 0.0], system: None });
    h.cmd(Command::AutoKeyModeToggle);
    h.cmd(Command::SetFrame { frame: 10 });
    let lines = |h: &Harness, c: egui::Color32| {
        fn walk(s: &egui::Shape, c: egui::Color32, n: &mut usize) {
            match s {
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, c, n)),
                egui::Shape::LineSegment { stroke, .. } if stroke.color == c => *n += 1,
                _ => {}
            }
        }
        let mut n = 0;
        h.shapes.iter().for_each(|s| walk(&s.shape, c, &mut n));
        n
    };
    let (yellow, cyan) = (egui::Color32::from_rgb(230, 220, 40), egui::Color32::from_rgb(40, 210, 230));
    menu_path(&mut h, &["Views", "Show Ghosting"]);
    h.frames(2);
    assert_eq!(crate::viewport_widget::ghost_frames(&h.app), [(9, true)]);
    assert!(lines(&h, yellow) >= 12 && lines(&h, cyan) == 0, "a yellow ghost of the box");
    h.cmd(Command::Preferences);
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("Preference Settings"))).unwrap();
    h.click(h.text_rect("Viewports", win).unwrap().center());
    h.click(h.find(Role::RadioButton, "Ghost Before and After").center());
    assert_eq!(h.app.state.prefs.ghost_mode, 2);
    h.cmd(Command::CloseDialog);
    h.frames(2);
    assert!(lines(&h, cyan) >= 12, "and a cyan one after");
}

/// Dragging the bar under the modifier stack makes the stack display taller.
#[test]
fn the_stack_resize_bar_makes_the_stack_display_taller() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1400.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    h.app.panel_tab = crate::Tab::Modify;
    h.frames(2);
    let before = h.app.stack_height;
    let bar_rect = |h: &Harness| h.widgets.iter().find(|w| w.1 == "Stack Resize Bar").map(|w| w.2).expect("the bar");
    let bar = bar_rect(&h);
    h.drag(bar.center(), bar.center() + egui::vec2(0.0, 80.0));
    assert!(h.app.stack_height > before + 40.0, "{before} -> {}", h.app.stack_height);
    let bar2 = bar_rect(&h);
    assert!(bar2.center().y > bar.center().y, "the bar moved down with it");
}

/// Ctrl+click selects several stack entries; right-click > Delete removes
/// them together in one undo step, Off turns them off together; Show All
/// Subtrees opens every entry's sub-object levels.
#[test]
fn several_stack_entries_select_with_ctrl_and_delete_together() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1400.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let id = selected(&h);
    for m in ["Bend", "Taper", "Twist"] {
        h.cmd(Command::AddModifier { name: m.into() });
    }
    h.app.panel_tab = crate::Tab::Modify;
    h.frames(2);
    let row = |h: &Harness, name: &str| h.text_rects(name).into_iter().rfind(|r| r.min.x > SCREEN.max.x * 0.5 || r.min.x > 1000.0).unwrap_or_else(|| panic!("the {name} row"));
    h.click(row(&h, "Bend").center());
    h.mods = egui::Modifiers::CTRL;
    h.click(row(&h, "Twist").center());
    h.mods = egui::Modifiers::NONE;
    // Off for both.
    let t = row(&h, "Twist").center();
    right_click(&mut h, t);
    h.click(h.find(Role::Button, "Off").center());
    let on: Vec<bool> = h.app.state.scene.node(id).unwrap().modifiers.iter().map(|m| m.enabled).collect();
    assert_eq!(on, [false, true, false], "Bend and Twist off, Taper on");
    let t = row(&h, "Twist").center();
    right_click(&mut h, t);
    h.click(h.find(Role::Button, "Show All Subtrees").center());
    let t = row(&h, "Twist").center();
    right_click(&mut h, t);
    h.click(h.find(Role::Button, "Delete").center());
    let names: Vec<String> = h.app.state.scene.node(id).unwrap().modifiers.iter().map(|m| m.name.clone()).collect();
    assert_eq!(names, ["Taper"]);
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.node(id).unwrap().modifiers.len(), 3, "one undo step");
}

/// Right-click an instance's base object in the stack > Make Reference: a
/// modifier added to it afterwards stays its own (above the reference line).
#[test]
fn make_reference_from_the_stack_keeps_later_modifiers_apart() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1400.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let a = selected(&h);
    h.cmd(Command::CloneOptions { kind: loom_commands::CloneKind::Instance, copies: 1, name: None, offset: Some([30.0, 0.0, 0.0]), rotate_deg: None, scale: None });
    let b = selected(&h);
    assert_ne!(a, b);
    h.app.panel_tab = crate::Tab::Modify;
    h.frames(2);
    let base = h.text_rects("Box").into_iter().rfind(|r| r.min.x > 1000.0).expect("the base object row");
    right_click(&mut h, base.center());
    h.click(h.find(Role::Button, "Make Reference").center());
    assert!(h.app.state.scene.node(b).unwrap().reference_barrier.is_some());
    h.cmd(Command::AddModifier { name: "Bend".into() });
    assert!(h.app.state.scene.node(a).unwrap().modifiers.is_empty(), "the original stays without the Bend");
    h.cmd(Command::UndoSceneOperation);
    h.cmd(Command::UndoSceneOperation);
    assert!(h.app.state.scene.node(b).unwrap().reference_barrier.is_none(), "one undo step");
}
