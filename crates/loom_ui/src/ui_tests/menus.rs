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

/// In the Named Selection Sets list Ctrl+click adds a set to the selection
/// and Alt+click takes one away.
#[test]
fn ctrl_and_alt_click_in_the_named_sets_list_add_and_remove_sets() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let a = selected(&h);
    h.cmd(Command::NamedSelectionSet { name: "first".into() });
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [30.0, 0.0, 0.0], size: [10.0; 3] });
    let b = selected(&h);
    h.cmd(Command::NamedSelectionSet { name: "second".into() });
    let field = right_of(&h, Role::TextInput, h.find(Role::Button, "Edit Named Selection Sets"));
    let list = right_of(&h, Role::ComboBox, field);
    h.click(list.center());
    h.click(h.find(Role::Button, "first").center());
    assert_eq!(h.app.state.selection.iter().copied().collect::<Vec<_>>(), vec![a], "a plain click replaces");
    h.click(list.center());
    h.mods = egui::Modifiers::CTRL;
    h.click(h.find(Role::Button, "second").center());
    h.mods = egui::Modifiers::NONE;
    assert_eq!(h.app.state.selection.iter().copied().collect::<Vec<_>>(), vec![a, b], "Ctrl+click adds");
    h.click(list.center());
    h.mods = egui::Modifiers::ALT;
    h.click(h.find(Role::Button, "first").center());
    h.mods = egui::Modifiers::NONE;
    assert_eq!(h.app.state.selection.iter().copied().collect::<Vec<_>>(), vec![b], "Alt+click removes");
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

/// The open Modifier List takes the keyboard: a letter jumps to the first
/// modifier starting with it and again to the next, arrows / Home / End move
/// the highlight (its name shows at the top of the list), Enter applies it;
/// the keys trigger no shortcuts meanwhile.
#[test]
fn modifier_list_letters_cycle_and_arrows_move_the_highlight() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let id = selected(&h);
    h.app.panel_tab = crate::Tab::Modify;
    h.frames(2);
    let set_button = h.find(Role::Button, "Show / hide the modifier button set");
    let list = h.widgets.iter().filter(|(r, _, w)| *r == Role::ComboBox && w.max.x <= set_button.min.x + 1.0 && (w.center().y - set_button.center().y).abs() < set_button.height()).max_by(|a, b| a.2.min.x.total_cmp(&b.2.min.x)).map(|w| w.2).expect("the Modifier List");
    let views = format!("{:?}", h.app.state.viewports.iter().map(|v| &v.kind).collect::<Vec<_>>());
    h.click(list.center());
    // The name the list shows at its top (painted on the closed-list button).
    let title = |h: &Harness| {
        let names = loom_scene::Modifier::LIST.iter().copied().chain(["Modifier List"]);
        names.filter(|n| h.text_rects(n).iter().any(|r| list.expand(2.0).contains(r.center()))).map(str::to_owned).next().unwrap_or_default()
    };
    h.run(vec![egui::Event::Text("t".into())]);
    h.frames(1);
    assert_eq!(title(&h), "Taper");
    h.run(vec![egui::Event::Text("t".into())]);
    h.frames(1);
    assert_eq!(title(&h), "Tessellate", "pressed again, the next one");
    h.key(egui::Key::ArrowDown);
    h.frames(1);
    assert_eq!(title(&h), "Trim/Extend");
    h.key(egui::Key::ArrowUp);
    h.key(egui::Key::ArrowUp);
    h.frames(1);
    assert_eq!(title(&h), "Taper");
    h.key(egui::Key::End);
    h.frames(1);
    assert_eq!(title(&h), *loom_scene::Modifier::LIST.last().unwrap());
    h.key(egui::Key::Home);
    h.run(vec![egui::Event::Text("b".into())]);
    h.frames(1);
    assert_eq!(title(&h), "Bend");
    assert_eq!(format!("{:?}", h.app.state.viewports.iter().map(|v| &v.kind).collect::<Vec<_>>()), views, "T, arrows and Home are not shortcuts while the list is open");
    h.key(egui::Key::Enter);
    h.frames(1);
    assert_eq!(h.app.state.scene.node(id).unwrap().modifiers.last().map(|m| m.name.as_str()), Some("Bend"));
    assert_eq!(title(&h), "Modifier List", "the list closed");
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
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::XrefRecords { scene: false }), "the XRef Objects dialog");
    h.click(h.find(Role::Button, "Create XRef Record from File").center());
    h.frames(1);
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
    // Back in the XRef Objects dialog: the record, its objects, Automatic and Remove.
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::XrefRecords { scene: false }));
    h.frames(1);
    let record = path.to_string_lossy().into_owned();
    h.click_text_anywhere(&record);
    assert!(!h.text_rects("Crate").is_empty(), "the record's objects are listed");
    assert!(h.app.state.scene.xrefs[0].automatic);
    h.click(h.find(Role::CheckBox, "Automatic").center());
    assert!(!h.app.state.scene.xrefs[0].automatic, "Automatic off");
    h.click(h.find(Role::Button, "Remove").center());
    assert!(h.app.state.scene.xrefs.is_empty() && h.app.state.scene.nodes().next().is_none(), "the reference and its object are gone");
    // The XRef Scene menu entry opens the XRef Scenes dialog.
    menu_path(&mut h, &["File", "References", "XRef Scene..."]);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::XrefRecords { scene: true }));
    assert!(h.find_all(Role::Button, "Add...").len() == 1);
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

/// Track View name + Enter saves it; Graph Editors > Saved Track Views opens
/// it again with its settings, New Track View starts a fresh one, Delete
/// Track View forgets it.
#[test]
fn saved_track_views_from_the_graph_editors_menu() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0));
    menu_path(&mut h, &["Graph Editors", "Track View - Dope Sheet..."]);
    h.frames(30);
    let field = h.widgets.iter().find(|w| w.0 == Role::TextInput && w.1.starts_with("Untitled")).map(|w| w.2).expect("the name field");
    h.click(field.center());
    assert!(h.ctx.wants_keyboard_input(), "the name field has the keyboard after the click: {:?}", h.widgets.iter().filter(|w| w.2.contains(field.center()) || w.0 == Role::Window || w.0 == Role::TextInput).map(|w| (w.0, w.1.clone(), w.2)).collect::<Vec<_>>());
    h.key(egui::Key::End);
    for _ in 0..12 {
        h.key(egui::Key::Backspace);
    }
    h.run(vec![egui::Event::Text("Keys".into())]);
    h.key(egui::Key::Enter);
    assert_eq!(h.app.state.saved_track_views.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["Keys"], "name {:?} field {field:?}", h.app.state.track_view.name);
    menu_path(&mut h, &["Graph Editors", "New Track View..."]);
    assert_eq!((h.app.state.track_view.mode, h.app.state.track_view.name.as_str()), (loom_commands::track_view::TvMode::CurveEditor, "Untitled 1"));
    menu_path(&mut h, &["Graph Editors", "Saved Track Views", "Keys"]);
    assert_eq!((h.app.state.track_view.mode, h.app.state.track_view.name.as_str()), (loom_commands::track_view::TvMode::DopeSheet, "Keys"));
    menu_path(&mut h, &["Graph Editors", "Delete Track View", "Keys"]);
    assert!(h.app.state.saved_track_views.is_empty());
}

/// The Dope Sheet window keeps its size over the frames (its two-row bottom
/// bar made it grow until it covered the menu bar) and the menus still open.
#[test]
fn the_dope_sheet_window_does_not_grow_over_the_menu_bar() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0));
    h.cmd(Command::OpenDopeSheet);
    h.frames(5);
    let size = |h: &Harness| h.ctx.memory(|m| m.area_rect(egui::Id::new("track_view"))).expect("the window").size();
    let first = size(&h);
    h.frames(30);
    assert!((size(&h).y - first.y).abs() < 1.0, "{first:?} -> {:?}", size(&h));
    menu_path(&mut h, &["Graph Editors", "New Track View..."]);
    assert_eq!(h.app.state.track_view.mode, loom_commands::track_view::TvMode::CurveEditor);
}

/// Views > xView > Open Edges on an Editable Poly plane: its four border
/// edges are flagged, the active view shows "Open Edges: 4 Edges", and
/// Select Results makes them the edge selection.
#[test]
fn xview_open_edges_and_select_results() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [20.0, 20.0, 0.0] });
    h.cmd(Command::ConvertToEditablePoly);
    menu_path(&mut h, &["Views", "xView", "Open Edges"]);
    assert_eq!(h.app.state.xview.test, Some(loom_commands::xview::XViewTest::OpenEdges));
    h.frames(2);
    let (_, msg) = loom_commands::xview::results(&h.app.state, None);
    assert!(!h.text_rects(&format!("Open Edges: {msg}")).is_empty(), "the result in the active view: {msg}");
    let n: usize = msg.split_whitespace().next().unwrap().parse().unwrap();
    assert!(n >= 4, "the border edges: {msg}");
    menu_path(&mut h, &["Views", "xView", "Select Results"]);
    assert_eq!(h.app.state.selection_mode, loom_commands::SelectionMode::Edge);
    assert_eq!(h.sub_count(), n);
}

/// xView > Overlapping Vertices > Configure: the Tolerance spinner widens the
/// check; a box's corners overlap at a tolerance larger than its size.
#[test]
fn xview_configure_sets_the_overlapping_vertices_tolerance() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [2.0; 3] });
    h.cmd(Command::ConvertToEditablePoly);
    menu_path(&mut h, &["Views", "xView", "Overlapping Vertices"]);
    assert_eq!(loom_commands::xview::results(&h.app.state, None).1, "0 Vertices");
    menu_path(&mut h, &["Views", "xView", "Configure..."]);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::XViewConfigure));
    let spin = h.spinner_after("Tolerance:");
    h.click(spin.center());
    h.run(vec![egui::Event::Text("50".into())]);
    h.key(egui::Key::Enter);
    h.frames(1);
    assert!(h.app.state.xview.tolerance > 2.0, "{}", h.app.state.xview.tolerance);
    assert_eq!(loom_commands::xview::results(&h.app.state, None).1, "8 Vertices");
}

/// Animation > Reaction Manager: box A's X Position is the master, box B's Z
/// Position the slave; two states (A at 0 → B at 0, A at 100 → B at 50) make
/// B follow A in between, and Delete State in the dialog removes one.
#[test]
fn reaction_manager_master_slave_and_states() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let a = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [40.0, 0.0, 0.0], size: [10.0; 3] });
    let b = *h.app.state.selection.iter().next().unwrap();
    menu_path(&mut h, &["Animation", "Reaction Manager..."]);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::ReactionManager));
    h.cmd(Command::AddReactionMaster { node: a.0, param: "Position X".into() });
    h.cmd(Command::AddReactionSlave { reaction: 0, node: b.0, param: "Position Z".into() });
    h.cmd(Command::SetReactionState { reaction: 0, slave: 0, state: None, master_value: Some(100.0), slave_value: Some(50.0) });
    h.cmd(Command::SetTransform { id: a.0, position: Some([50.0, 0.0, 0.0]), rotation_deg: None, scale: None });
    let z = h.app.state.scene.node(b).unwrap().transform.position.get().z;
    assert!((z - 25.0).abs() < 1e-3, "halfway: {z}");
    h.cmd(Command::SetTransform { id: a.0, position: Some([200.0, 0.0, 0.0]), rotation_deg: None, scale: None });
    assert!((h.app.state.scene.node(b).unwrap().transform.position.get().z - 50.0).abs() < 1e-3, "past the last state: its value");
    // The dialog: highlight the slave, delete its second state.
    h.frames(2);
    let slave = h.text_rects("    Box002 / Position Z");
    assert!(!slave.is_empty(), "the slave row");
    h.click(slave[0].center());
    h.frames(1);
    let del = h.find_all(egui::accesskit::Role::Button, "Delete State");
    assert_eq!(del.len(), 2, "a Delete State per state");
    h.click(del[1].center());
    assert_eq!(h.app.state.scene.reactions[0].slaves[0].states.len(), 1);
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.reactions[0].slaves[0].states.len(), 2, "undoable");
}

/// Animation > Animation Layers: Enable, Add Layer, a move goes into the
/// layer (the Base Layer keeps its position), the light bulb hides it, the
/// Base Layer made active takes moves again, Collapse merges, undoable.
#[test]
fn animation_layers_toolbar_enable_add_move_toggle_collapse() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let a = *h.app.state.selection.iter().next().unwrap();
    menu_path(&mut h, &["Animation", "Animation Layers..."]);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::AnimationLayers));
    h.frames(2);
    let enable = h.find_all(egui::accesskit::Role::Button, "Enable Anim Layers");
    h.click(enable[0].center());
    h.frames(30);
    let add = h.find_all(egui::accesskit::Role::Button, "Add Layer");
    h.click(add[0].center());
    h.frames(30);
    let al = h.app.state.scene.node(a).unwrap().anim_layers.clone().expect("enabled");
    assert_eq!((al.layers.len(), al.active), (1, 1), "the new layer is active");
    h.cmd(Command::SetTransform { id: a.0, position: Some([20.0, 0.0, 0.0]), rotation_deg: None, scale: None });
    let world_x = |h: &Harness| h.app.state.scene.world_matrix(a).w_axis.x;
    assert_eq!(h.app.state.scene.node(a).unwrap().transform.position.get().x, 0.0, "the Base Layer keeps its position");
    assert!((world_x(&h) - 20.0).abs() < 1e-3, "the layer moves it: {}", world_x(&h));
    h.frames(2);
    let on = h.text_rects("On");
    h.click(on[0].center());
    h.frames(30);
    assert!(world_x(&h).abs() < 1e-3, "light bulb off: hidden");
    h.click(on[0].center());
    h.frames(30);
    assert!((world_x(&h) - 20.0).abs() < 1e-3, "on again");
    let collapse = h.find_all(egui::accesskit::Role::Button, "Collapse");
    h.click(collapse[0].center());
    h.frames(30);
    let n = h.app.state.scene.node(a).unwrap();
    assert!(n.anim_layers.as_ref().unwrap().layers.is_empty());
    assert!((n.transform.position.get().x - 20.0).abs() < 1e-3, "merged into the Base Layer");
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.node(a).unwrap().anim_layers.as_ref().unwrap().layers.len(), 1, "undoable");
}

/// Reaction Manager: a Bend modifier's Angle as a slave (any animatable
/// track), Curve off with Strength / Influence / Falloff, Create Mode stops
/// the slave following, the master picked in a viewport, a graph point
/// dragged moves its state.
#[test]
fn reaction_manager_any_track_blend_create_mode_pick_and_graph() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let a = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [40.0, 0.0, 0.0], size: [10.0; 3] });
    let b = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::AddModifier { name: "Bend".into() });
    let params = loom_commands::extras::reaction_parameters(h.app.state.scene.node(b).unwrap());
    let bend = params.iter().find(|p| p.starts_with("Bend") && p.ends_with("Angle")).cloned().expect("the Bend Angle track offered");
    menu_path(&mut h, &["Animation", "Reaction Manager..."]);
    // The master from a viewport pick.
    h.frames(2);
    let pick = h.text_rects("Pick from Viewport");
    h.click(pick[0].center());
    h.frames(30);
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::ReactionTrack));
    h.cmd(Command::PickHit { id: a.0 });
    assert_eq!(h.app.state.reaction_picked_node, Some(a));
    h.cmd(Command::AddReactionMaster { node: a.0, param: "Position X".into() });
    h.cmd(Command::AddReactionSlave { reaction: 0, node: b.0, param: bend.clone() });
    h.cmd(Command::SetReactionState { reaction: 0, slave: 0, state: None, master_value: Some(100.0), slave_value: Some(90.0) });
    h.cmd(Command::SetTransform { id: a.0, position: Some([50.0, 0.0, 0.0]), rotation_deg: None, scale: None });
    let angle = |h: &Harness| loom_commands::extras::read_node_wire_param(&h.app.state.scene, b, &bend).unwrap();
    assert!((angle(&h) - 45.0).abs() < 1e-3, "the Bend Angle follows: {}", angle(&h));
    // Curve off, the second state stronger: pulled towards it.
    h.frames(2);
    let slave = h.text_rects(&format!("    Box002 / {bend}"));
    h.click(slave[0].center());
    h.frames(30);
    let curve = h.text_rects("Curve");
    h.click(curve[0].center());
    h.frames(30);
    assert!(!h.app.state.scene.reactions[0].slaves[0].curve);
    h.cmd(Command::SetReactionStateBlend { reaction: 0, slave: 0, state: 1, strength: Some(3.0), influence: None, falloff: None });
    assert!(angle(&h) > 45.0 + 1.0, "Strength pulls towards the state: {}", angle(&h));
    // Create Mode: the slave stays put while the master moves.
    let cm = h.text_rects("Create Mode");
    h.click(cm[0].center());
    h.frames(30);
    assert!(h.app.state.reaction_create_mode);
    let held = angle(&h);
    h.cmd(Command::SetTransform { id: a.0, position: Some([0.0, 0.0, 0.0]), rotation_deg: None, scale: None });
    assert!((angle(&h) - held).abs() < 1e-4, "Create Mode: not following");
    h.click(cm[0].center());
    h.frames(30);
    assert!(angle(&h).abs() < 1e-3, "following again: {}", angle(&h));
    // The graph: drag the second state's point straight down; its slave value drops.
    h.frames(2);
    let pts: Vec<egui::Pos2> = h.ctx.data(|d| d.get_temp(egui::Id::new("reaction_graph_points"))).expect("the graph");
    let before = h.app.state.scene.reactions[0].slaves[0].states[1];
    h.drag(pts[1], pts[1] + egui::vec2(0.0, 40.0));
    h.frames(2);
    let after = h.app.state.scene.reactions[0].slaves[0].states[1];
    assert!(after.1 < before.1 - 1.0 && (after.0 - before.0).abs() < 1.0, "dragged down: {before:?} -> {after:?}");
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.reactions[0].slaves[0].states[1], before, "undoable");
}

/// Reaction Manager: a state renamed in place, a point set to Corner (straight
/// lines) and back to Bezier with its handle dragged, the graph's Move V and
/// Add Corner tools, and Start / End limiting the frames the slave follows.
#[test]
fn reaction_manager_names_point_types_graph_tools_and_start_end() {
    use loom_scene::PointKind;
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let a = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [40.0, 0.0, 0.0], size: [10.0; 3] });
    let b = *h.app.state.selection.iter().next().unwrap();
    menu_path(&mut h, &["Animation", "Reaction Manager..."]);
    h.cmd(Command::AddReactionMaster { node: a.0, param: "Position X".into() });
    h.cmd(Command::AddReactionSlave { reaction: 0, node: b.0, param: "Position Z".into() });
    assert_eq!(h.app.state.scene.reactions[0].slaves[0].range, Some([h.app.state.frame_range.0, h.app.state.frame_range.1]), "Start / End default to the active segment");
    h.cmd(Command::SetReactionState { reaction: 0, slave: 0, state: Some(0), master_value: Some(0.0), slave_value: Some(0.0) });
    h.cmd(Command::SetReactionState { reaction: 0, slave: 0, state: None, master_value: Some(50.0), slave_value: Some(50.0) });
    h.cmd(Command::SetReactionState { reaction: 0, slave: 0, state: None, master_value: Some(100.0), slave_value: Some(0.0) });
    h.frames(2);
    h.click(h.text_rects("    Box002 / Position Z")[0].center());
    h.frames(30);
    assert!(!h.text_rects("Start").is_empty() && !h.text_rects("End").is_empty(), "the Start / End columns");
    let slave = |h: &Harness| h.app.state.scene.reactions[0].slaves[0].clone();
    // Rename State02 in place.
    // The name fields are the text inputs under Create State, one per state.
    let cs = h.text_rects("Create State")[0];
    let mut names: Vec<egui::Rect> = h.widgets.iter().filter(|w| w.0 == egui::accesskit::Role::TextInput && w.2.min.y > cs.max.y && (w.2.min.x - cs.min.x).abs() < 120.0 && w.2.width() < 120.0).map(|w| w.2).collect();
    names.sort_by(|a, b| a.min.y.total_cmp(&b.min.y));
    assert_eq!(names.len(), 3, "a name field per state");
    h.click(names[1].center());
    let select_all = egui::Event::Key { key: egui::Key::A, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND };
    h.run(vec![select_all]);
    type_text(&mut h, "Peak");
    h.frames(2);
    assert_eq!(slave(&h).name_of(1), "Peak");
    // Right-click the middle point: Corner gives straight lines.
    let pts: Vec<egui::Pos2> = h.ctx.data(|d| d.get_temp(egui::Id::new("reaction_graph_points"))).expect("the graph");
    let smooth25 = slave(&h).value(25.0).unwrap();
    right_click(&mut h, pts[1]);
    h.frames(2);
    h.click(h.text_rects("Corner")[0].center());
    h.frames(2);
    assert_eq!(slave(&h).point_of(1).kind, PointKind::Corner);
    assert!((slave(&h).value(25.0).unwrap() - 25.0).abs() < 1e-2, "Corner: straight, not {smooth25}");
    assert!((smooth25 - 25.0).abs() > 0.5, "the smooth curve bowed");
    // Back to Bezier Smooth: the selected point shows its handles; drag the out handle up.
    right_click(&mut h, pts[1]);
    h.frames(2);
    h.click(h.text_rects("Bezier Smooth")[0].center());
    h.frames(3);
    assert_eq!(slave(&h).point_of(1).kind, PointKind::Bezier);
    let handles: Vec<(usize, egui::Pos2)> = h.ctx.data(|d| d.get_temp(egui::Id::new("reaction_graph_handles"))).expect("handles");
    let out = handles.iter().find(|(s, _)| *s == 1).expect("the out handle").1;
    let before75 = slave(&h).value(75.0).unwrap();
    h.drag(out, out + egui::vec2(0.0, -40.0));
    h.frames(2);
    let hs = slave(&h).handles(1);
    assert!(hs[1][1] > 0.0 && hs[0][1] < 0.0, "out up, the in handle turned down with it: {hs:?}");
    assert!(slave(&h).value(75.0).unwrap() > before75 + 1.0, "the curve follows the handle");
    // Move V: dragging diagonally keeps the master value.
    h.click(h.text_rects("Move V")[0].center());
    let pts: Vec<egui::Pos2> = h.ctx.data(|d| d.get_temp(egui::Id::new("reaction_graph_points"))).expect("the graph");
    let s0 = slave(&h).states[0];
    h.drag(pts[0], pts[0] + egui::vec2(30.0, -30.0));
    h.frames(2);
    let s1 = slave(&h).states[0];
    assert!((s1.0 - s0.0).abs() < 1e-4 && s1.1 > s0.1 + 1.0, "{s0:?} -> {s1:?}");
    // Add Corner: a click on an empty part of the graph adds a Corner state.
    h.click(h.text_rects("Add Corner")[0].center());
    let pts: Vec<egui::Pos2> = h.ctx.data(|d| d.get_temp(egui::Id::new("reaction_graph_points"))).expect("the graph");
    h.click(egui::pos2((pts[1].x + pts[2].x) * 0.5, pts[2].y - 10.0));
    h.frames(2);
    assert_eq!(slave(&h).states.len(), 4);
    assert_eq!(slave(&h).point_of(3).kind, PointKind::Corner);
    // Start / End: outside them the slave stops following.
    h.cmd(Command::SetReactionSlaveRange { reaction: 0, slave: 0, start: Some(10), end: Some(20) });
    let z = |h: &Harness| h.app.state.scene.node(b).unwrap().transform.position.get().z;
    h.cmd(Command::SetFrame { frame: 15 });
    h.cmd(Command::SetTransform { id: a.0, position: Some([50.0, 0.0, 0.0]), rotation_deg: None, scale: None });
    let inside = z(&h);
    assert!((inside - slave(&h).value(50.0).unwrap()).abs() < 1e-3, "inside Start / End it follows");
    h.cmd(Command::SetFrame { frame: 30 });
    h.cmd(Command::SetTransform { id: a.0, position: Some([0.0, 0.0, 0.0]), rotation_deg: None, scale: None });
    assert!((z(&h) - inside).abs() < 1e-3, "outside it keeps its value");
    assert!(h.app.dispatch(Command::SetReactionSlaveRange { reaction: 0, slave: 0, start: Some(30), end: None }).is_err(), "Start after End");
}

/// Reaction Manager graph: a region drag selects several points, dragging
/// one moves them all (one undo step), the wheel zooms, Pan drags the view
/// and Zoom Extents fits it again.
#[test]
fn reaction_graph_region_select_moves_together_and_pans_and_zooms() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let a = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [40.0, 0.0, 0.0], size: [10.0; 3] });
    let b = *h.app.state.selection.iter().next().unwrap();
    menu_path(&mut h, &["Animation", "Reaction Manager..."]);
    h.cmd(Command::AddReactionMaster { node: a.0, param: "Position X".into() });
    h.cmd(Command::AddReactionSlave { reaction: 0, node: b.0, param: "Position Z".into() });
    h.cmd(Command::SetReactionState { reaction: 0, slave: 0, state: Some(0), master_value: Some(0.0), slave_value: Some(0.0) });
    h.cmd(Command::SetReactionState { reaction: 0, slave: 0, state: None, master_value: Some(50.0), slave_value: Some(50.0) });
    h.cmd(Command::SetReactionState { reaction: 0, slave: 0, state: None, master_value: Some(100.0), slave_value: Some(0.0) });
    h.frames(2);
    h.click(h.text_rects("    Box002 / Position Z")[0].center());
    h.frames(30);
    let pts = |h: &Harness| -> Vec<egui::Pos2> { h.ctx.data(|d| d.get_temp(egui::Id::new("reaction_graph_points"))).expect("the graph") };
    let states = |h: &Harness| h.app.state.scene.reactions[0].slaves[0].states.clone();
    // A region around the first two points (from empty graph below-left of them).
    let p = pts(&h);
    let from = egui::pos2(p[0].x - 5.0, p[1].y - 8.0);
    let to = egui::pos2((p[1].x + p[2].x) * 0.5, p[0].y + 4.0);
    h.drag(from, to);
    h.frames(2);
    let before = states(&h);
    let p = pts(&h);
    h.drag(p[1], p[1] + egui::vec2(0.0, 30.0));
    h.frames(2);
    let after = states(&h);
    assert!(after[0].1 < before[0].1 - 1.0 && after[1].1 < before[1].1 - 1.0, "both selected points moved down: {before:?} -> {after:?}");
    assert_eq!(after[2], before[2], "the unselected point stays");
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(states(&h), before, "one undo step");
    // The wheel zooms in about the pointer: the points spread apart.
    let p = pts(&h);
    let span = p[2].x - p[0].x;
    h.run(vec![egui::Event::PointerMoved(p[1]), egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, 120.0), modifiers: egui::Modifiers::NONE }]);
    h.frames(2);
    let q = pts(&h);
    assert!(q[2].x - q[0].x > span * 1.2, "zoomed in: {span} -> {}", q[2].x - q[0].x);
    // Pan: drag the empty graph; the points follow the pointer.
    h.click(h.text_rects("Pan")[0].center());
    let q = pts(&h);
    let empty = egui::pos2(q[1].x + 15.0, q[1].y + 40.0);
    h.drag(empty, empty + egui::vec2(-20.0, 0.0));
    h.frames(2);
    let r = pts(&h);
    assert!((r[1].x - (q[1].x - 20.0)).abs() < 3.0, "panned left: {} -> {}", q[1].x, r[1].x);
    assert_eq!(states(&h), before, "panning moves no point");
    // Zoom Extents fits the states again.
    h.click(h.text_rects("Zoom Extents")[0].center());
    h.frames(2);
    let z = pts(&h);
    assert!((z[2].x - z[0].x - span).abs() < 1.0, "fit again: {span} vs {}", z[2].x - z[0].x);
}

/// Views > SteeringWheels: the wheel opens at the cursor, a drag on its Pan
/// wedge pans the active view, Zoom zooms, Rewind (a click) goes back, the
/// Wheel menu switches to a mini wheel, Esc closes it, the panel sets options.
#[test]
fn steering_wheels_open_pan_zoom_rewind_switch_close() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0));
    h.frames(2);
    let c = h.app.viewport_rects[h.app.state.active_viewport].center();
    menu_path(&mut h, &["Views", "SteeringWheels", "Toggle SteeringWheels"]);
    assert!(h.app.state.wheel.open);
    h.run(vec![egui::Event::PointerMoved(c)]);
    h.frames(30);
    let at = |h: &Harness, name: &str| crate::steering_wheel::wedge_centres(&h.app, &h.ctx).into_iter().find(|(t, _)| t == name).map(|(_, p)| p).unwrap();
    let t0 = h.app.state.active().camera.target;
    let d0 = h.app.state.active().camera.distance;
    let pan = at(&h, "Pan");
    h.drag(pan, pan + egui::vec2(60.0, 0.0));
    h.frames(30);
    assert_ne!(h.app.state.active().camera.target, t0, "Pan moved the view");
    let zoom = at(&h, "Zoom");
    h.drag(zoom, zoom + egui::vec2(0.0, -40.0));
    h.frames(30);
    assert!(h.app.state.active().camera.distance < d0, "Zoom in");
    let rewind = at(&h, "Rewind");
    h.click(rewind);
    h.frames(30);
    assert!((h.app.state.active().camera.distance - d0).abs() < 1e-3, "Rewind undid the zoom");
    let rewind = at(&h, "Rewind");
    h.click(rewind);
    h.frames(30);
    assert!((h.app.state.active().camera.target - t0).length() < 1e-3, "and the pan");
    h.cmd(Command::SteeringWheel { wheel: "Mini Tour Building".into() });
    assert!(h.app.state.wheel.mini);
    h.key(egui::Key::Escape);
    assert!(!h.app.state.wheel.open, "Esc closes");
    menu_path(&mut h, &["Views", "SteeringWheels", "Configure..."]);
    h.frames(2);
    let speed = h.app.state.prefs.wheels.walk_speed;
    h.cmd(Command::SteeringWheelWalkSpeed { increase: false });
    assert_eq!(h.app.state.prefs.wheels.walk_speed, speed * 0.5);
    let look = h.text_rects("Invert Vertical Axis");
    h.click(look[0].center());
    assert!(h.app.state.prefs.wheels.look_invert, "the SteeringWheels panel");
    // The [+] label menu carries the same submenu; its Toggle item opens the wheel.
    let entries = crate::label_menus::general_entries(&h.app, 0);
    let sub = entries.iter().find(|e| e.label == "SteeringWheels").expect("[+] > SteeringWheels");
    let crate::label_menus::Action::Submenu(items) = &sub.action else { panic!("a submenu") };
    let crate::label_menus::Action::Command(c) = &items[0].action else { panic!("Toggle SteeringWheels") };
    h.cmd(c.clone());
    assert!(h.app.state.wheel.open);
}

/// Brush Presets toolbar: shown from the toolbar right-click list, its
/// buttons enabled while a paint tool is on; a preset click sets the Painter
/// size, + adds one, the Manager renames (double-click) and deletes.
#[test]
fn brush_presets_toolbar_and_manager() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0));
    h.cmd(Command::ShowToolbar { name: "Brush Presets".into(), on: Some(true) });
    h.app.state.skin_paint = true;
    h.frames(3);
    let presets = h.app.state.prefs.brush_presets.presets.len();
    let add = h.find_all(egui::accesskit::Role::Button, "+");
    h.click(add[0].center());
    h.frames(30);
    assert_eq!(h.app.state.prefs.brush_presets.presets.len(), presets + 1, "Add New Preset");
    let manager = h.find_all(egui::accesskit::Role::Button, "Manager");
    h.click(manager[0].center());
    h.frames(30);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::BrushPresetManager));
    let first = h.app.state.prefs.brush_presets.presets[0].name.clone();
    let row = h.text_rects(&first);
    h.click(row[0].center());
    h.frames(30);
    let del = h.find_all(egui::accesskit::Role::Button, "Delete");
    h.click(del[0].center());
    h.frames(30);
    assert_eq!(h.app.state.prefs.brush_presets.presets.len(), presets, "Delete");
    h.cmd(Command::ApplyBrushPreset { index: 0 });
    assert_eq!(h.app.state.skin_painter.size, h.app.state.prefs.brush_presets.presets[0].painter.size);
}

/// The Play flyout: hold Play, pick Play Selected; while it plays only the
/// selection shows; the button then repeats Play Selected. Time Slider
/// Capture: the mouse's x scrubs the slider, a click ends it.
#[test]
fn play_selected_flyout_and_time_slider_capture() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let a = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [40.0, 0.0, 0.0], size: [10.0; 3] });
    h.cmd(Command::SelectNodes { ids: vec![a.0], op: loom_commands::SelectOp::Replace });
    h.frames(2);
    let play = h.find_all(egui::accesskit::Role::Button, "Play / Stop Animation");
    let p = play[0].center();
    // Hold the button to open the flyout.
    h.run(vec![egui::Event::PointerMoved(p)]);
    h.run(vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE }]);
    for _ in 0..30 {
        h.time += 0.05;
        h.frames(1);
    }
    h.run(vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE }]);
    h.frames(2);
    let entry = h.find_all(egui::accesskit::Role::Button, "Play Selected");
    assert!(!entry.is_empty(), "the flyout opened");
    h.click(entry[0].center());
    h.frames(2);
    assert!(h.app.state.playing && h.app.state.play_selected);
    let other = h.app.state.scene.nodes().find(|n| n.id != a).unwrap().clone();
    assert!(!h.app.state.is_visible(&other), "only the selection shows");
    h.cmd(Command::PlaySelected);
    assert!(!h.app.state.playing && h.app.state.is_visible(&other), "stopped: all back");
    assert!(h.app.state.play_flyout_selected, "the flyout keeps Play Selected");
    // Capture: moving to the slider's right end goes to the last frame.
    h.cmd(Command::TimeSliderCaptureToggle);
    let r = h.screen;
    h.run(vec![egui::Event::PointerMoved(egui::pos2(r.right() - 45.0, r.bottom() - 10.0))]);
    h.frames(2);
    assert_eq!(h.app.state.current_frame, h.app.state.frame_range.1, "scrubbed to the end");
    h.run(vec![egui::Event::PointerMoved(egui::pos2(r.left() + 20.0, r.bottom() - 10.0))]);
    h.frames(2);
    assert_eq!(h.app.state.current_frame, h.app.state.frame_range.0, "and to the start");
    h.click(egui::pos2(800.0, 400.0));
    assert!(!h.app.state.time_capture, "a click ends it");
}

/// Time Configuration > Active Viewport Only: playback starting asks for a
/// screenshot; the other views then show their part of it until it stops.
#[test]
fn active_viewport_only_freezes_the_inactive_views() {
    let mut h = Harness::new();
    h.frames(2);
    assert!(h.app.state.playback.active_viewport_only, "on by default");
    h.cmd(Command::TimeConfiguration);
    h.frames(2);
    let avo = h.text_rects("Active Viewport Only");
    h.click(avo[0].center());
    assert!(!h.app.state.playback.active_viewport_only);
    h.click(avo[0].center());
    assert!(h.app.state.playback.active_viewport_only);
    h.cmd(Command::PlayAnimation);
    h.frames(1);
    assert!(h.app.freeze_pending.is_some(), "a screenshot was asked for");
    let size = [(h.screen.width()) as usize, (h.screen.height()) as usize];
    let image = std::sync::Arc::new(egui::ColorImage::new(size, egui::Color32::from_rgb(10, 200, 30)));
    h.run(vec![egui::Event::Screenshot { viewport_id: egui::ViewportId::ROOT, user_data: egui::UserData::new("freeze"), image }]);
    let shown = h.app.viewport_rects.iter().enumerate().filter(|(i, r)| *i != h.app.state.active_viewport && r.width() >= 2.0).count();
    assert_eq!(h.app.frozen_views.len(), shown, "every other view frozen");
    assert!(!h.app.frozen_views.contains_key(&h.app.state.active_viewport), "not the active one");
    h.cmd(Command::PlayAnimation);
    h.frames(1);
    assert!(h.app.frozen_views.is_empty(), "stopped: live again");
}

/// Preferences > Gizmos: Rotation Method (Linear Roll turns by Rotation
/// Increment per pixel along the tangent; Legacy R4 by vertical motion only)
/// and Move/Rotate Transforms Intersection / Projection.
#[test]
fn preferences_rotation_method_and_projection() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0));
    h.cmd(Command::Preferences);
    h.frames(2);
    let tab = h.text_rects("Gizmos");
    h.click(tab[0].center());
    h.frames(1);
    h.click(h.text_rects("Linear Roll")[0].center());
    assert_eq!(h.app.state.prefs.gizmos.rotation_method, 0);
    h.click(h.text_rects("Projection")[0].center());
    assert!(h.app.state.prefs.gizmos.move_projection);
    assert_eq!(h.app.state.prefs.gizmos.rotation_increment_per_px, 0.5);
    // Rendering > Nth Serial Numbering.
    let tab = h.text_rects("Rendering").into_iter().max_by(|a, b| a.min.y.total_cmp(&b.min.y)).expect("the Rendering tab");
    h.click(tab.center());
    h.frames(1);
    h.click(h.text_rects("Nth Serial Numbering")[0].center());
    assert!(h.app.state.prefs.nth_serial_numbering);
    h.click(h.text_rects("Use Environment Alpha")[0].center());
    assert!(h.app.state.prefs.use_environment_alpha, "Rendering > Background > Use Environment Alpha");
    h.cmd(Command::CloseDialog);
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [20.0; 3] });
    let id = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::SelectAndRotate);
    h.frames(1);
    let i = h.app.state.active_viewport;
    let rect = h.app.viewport_rects[i];
    let cam = h.app.camera_for(i);
    let projection = crate::gizmo::Projection::new(cam.view_matrix(), cam.projection_matrix(rect.width() / rect.height()), rect);
    let layout = crate::gizmo::layout_for(&h.app, i, rect.height());
    let o = projection.screen(layout.origin).unwrap();
    let start = (0..360)
        .flat_map(|deg| (10..800).step_by(2).map(move |r| (deg as f32, r as f32)))
        .map(|(deg, r)| o + egui::vec2(deg.to_radians().cos(), deg.to_radians().sin()) * r)
        .find(|p| matches!(crate::gizmo::hit(&h.app, &layout, &projection, *p), Some(crate::gizmo::Handle::Axis(_))))
        .expect("an axis ring on screen");
    let turned = |h: &Harness| {
        h.app.state.scene.node(id).unwrap().euler_deg().abs().max_element()
    };
    // Linear Roll: 40 px along the tangent at 0.5 degrees a pixel.
    let tangent = egui::vec2(start.y - o.y, -(start.x - o.x)).normalized() * 40.0;
    h.drag(start, start + tangent);
    assert!((turned(&h) - 20.0).abs() < 0.5, "{}", turned(&h));
    h.cmd(Command::UndoSceneOperation);
    // Legacy R4: sideways motion turns nothing.
    h.cmd(Command::SetPreference { name: "Rotation Method".into(), value: 2.0 });
    h.drag(start, start + egui::vec2(40.0, 0.0));
    assert!(turned(&h) < 1e-3, "{}", turned(&h));
}

/// Preferences > General: Layer Defaults (Propagate ... Ask: Unhide All on
/// a hidden layer asks, Yes unhides the layer; Default to By Layer for New
/// Nodes) and Sub-Materials Assign Automatically (polygons selected: the
/// material goes on through a Multi/Sub-Object).
#[test]
fn preferences_layer_defaults_and_sub_material_assign() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0));
    h.cmd(Command::Preferences);
    h.frames(2);
    let ask = h.text_rects("Ask");
    h.click(ask[0].center());
    assert_eq!(h.app.state.prefs.propagate_layers, 2);
    let by_layer = h.text_rects("Default to By Layer for New Nodes");
    h.click(by_layer[0].center());
    assert!(h.app.state.prefs.default_by_layer);
    h.cmd(Command::CloseDialog);
    h.cmd(Command::CreateLayer { name: Some("L1".into()) });
    let layer = h.app.state.scene.current_layer;
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let a = *h.app.state.selection.iter().next().unwrap();
    let n = h.app.state.scene.node(a).unwrap();
    assert!(n.display.by_layer && n.render.by_layer, "a new node is By Layer");
    // New Lights Renderable By Layer: a light on a non-renderable layer is not renderable.
    h.cmd(Command::Preferences);
    h.frames(2);
    let nl = h.text_rects("New Lights Renderable By Layer");
    h.click(nl[0].center());
    assert!(h.app.state.prefs.new_lights_renderable_by_layer);
    // Default Ambient Light Color: a reset scene starts with it.
    h.cmd(Command::SetPreference { name: "Default Ambient Light Color G".into(), value: 0.25 });
    let keep = h.app.state.scene.clone();
    h.cmd(Command::ResetFile);
    assert!((h.app.state.scene.environment.ambient.get().y - 0.25).abs() < 1e-6, "the reset scene's ambient");
    h.app.state.scene = keep;
    h.cmd(Command::CloseDialog);
    let mut layers = h.app.state.scene.layers.clone();
    layers.iter_mut().find(|l| l.id == layer).unwrap().render.renderable = false;
    h.app.state.scene.layers = layers;
    h.cmd(Command::CreateLight { kind: loom_commands::light::LightKind::Omni, position: [0.0, 0.0, 50.0], target: [0.0; 3] });
    let light = *h.app.state.selection.iter().next().unwrap();
    assert!(!h.app.state.scene.node(light).unwrap().render.renderable, "Renderable from the layer");
    h.cmd(Command::DeleteObjects);
    h.cmd(Command::SetLayerHidden { layer, on: true });
    h.cmd(Command::UnhideAll);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::PropagateLayers), "Ask");
    h.frames(2);
    let yes = h.find_all(egui::accesskit::Role::Button, "Yes");
    h.click(yes[0].center());
    assert!(!h.app.state.scene.layer(layer).unwrap().hidden, "Yes: the layer is unhidden");
    // Sub-Materials > Assign Automatically: polygons selected.
    h.cmd(Command::SelectNodes { ids: vec![a.0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::ConvertToEditablePoly);
    h.cmd(Command::SelectionMode { mode: loom_commands::SelectionMode::Face });
    h.cmd(Command::SelectSub { node: None, ids: vec![0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::AssignMaterialToSelection);
    let m = h.app.state.scene.node(a).unwrap().material.expect("assigned");
    assert!(matches!(h.app.state.scene.materials.get(&m).map(|x| &x.kind), Some(loom_scene::material::MaterialKind::MultiSub(_))), "a Multi/Sub-Object on the faces");
}

/// The command panel widened by its edge holds several columns: the Display
/// panel's rollouts flow into a second one once they run past the panel's
/// bottom by more than Preferences > Rollout Threshold.
#[test]
fn command_panel_columns_and_rollout_threshold() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 700.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    h.app.panel_tab = crate::command_panel::Tab::Display;
    h.frames(3);
    let p = egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new("command_panel")).expect("panel").rect;
    let edge = egui::pos2(p.left() + 1.0, p.center().y);
    h.drag(edge, edge - egui::vec2(crate::command_panel::rollouts::COLUMN_WIDTH * 1.2, 0.0));
    h.frames(5);
    let p = egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new("command_panel")).expect("panel").rect;
    assert!(p.width() >= crate::command_panel::rollouts::COLUMN_WIDTH * 2.0, "widened: {}", p.width());
    let headers: Vec<egui::Rect> = ["Display Color", "Hide by Category", "Hide", "Freeze", "Display Properties", "Link Display"].iter().filter_map(|t| h.text_rects(t).into_iter().find(|r| p.contains(r.center()))).collect();
    assert!(headers.len() >= 4, "the Display rollouts: {headers:?}");
    let second = headers.iter().any(|r| r.left() > p.left() + crate::command_panel::rollouts::COLUMN_WIDTH * 0.9);
    assert!(second, "a rollout in the second column: {headers:?}");
    // A huge threshold keeps them in one column.
    h.cmd(Command::SetPreference { name: "Rollout Threshold".into(), value: 2000.0 });
    h.frames(5);
    let headers: Vec<egui::Rect> = ["Display Color", "Hide by Category", "Hide", "Freeze", "Display Properties", "Link Display"].iter().filter_map(|t| h.text_rects(t).into_iter().find(|r| p.contains(r.center()))).collect();
    assert!(headers.iter().all(|r| r.left() < p.left() + crate::command_panel::rollouts::COLUMN_WIDTH * 0.9), "one column: {headers:?}");
    // The Modify panel: the modifier stack sits between Name and Color and
    // the Editable Poly rollouts, which still flow into columns.
    h.cmd(Command::SetPreference { name: "Rollout Threshold".into(), value: 50.0 });
    h.cmd(Command::ConvertToEditablePoly);
    h.app.panel_tab = crate::command_panel::Tab::Modify;
    h.frames(5);
    let headers: Vec<egui::Rect> = ["Selection", "Soft Selection", "Edit Geometry", "Subdivision Surface", "Paint Deformation"].iter().filter_map(|t| h.text_rects(t).into_iter().find(|r| p.contains(r.center()))).collect();
    assert!(headers.iter().any(|r| r.left() > p.left() + crate::command_panel::rollouts::COLUMN_WIDTH * 0.9), "Editable Poly rollouts in a second column: {headers:?}");
    // A user order applies in columns too: Paint Deformation first.
    h.cmd(Command::SetRolloutOrder { panel: "Modify".into(), order: vec!["Paint Deformation".into()] });
    h.frames(5);
    let pd = h.text_rects("Paint Deformation").into_iter().find(|r| p.contains(r.center())).expect("Paint Deformation");
    let sel = h.text_rects("Selection").into_iter().find(|r| p.contains(r.center())).expect("Selection");
    assert!(pd.left() < p.left() + crate::command_panel::rollouts::COLUMN_WIDTH * 0.9 && pd.top() < sel.top(), "Paint Deformation leads the first column: {pd:?} {sel:?}");
}

/// Reaction Manager, Curve off, master a Position axis: with Select and
/// Manipulate on, the selected slave shows an Influence sphere per state.
/// Dragging its handle along the master axis grows the state's Influence,
/// dragging the centre (Reaction Value) moves the state's master value; each
/// drag is one undo step.
#[test]
fn reaction_manager_influence_manipulators_in_viewport() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let a = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [30.0, 0.0, 0.0], size: [10.0; 3] });
    let b = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::AddReactionMaster { node: a.0, param: "Position X".into() });
    h.cmd(Command::AddReactionSlave { reaction: 0, node: b.0, param: "Position Z".into() });
    h.cmd(Command::SetReactionState { reaction: 0, slave: 0, state: None, master_value: Some(10.0), slave_value: Some(20.0) });
    h.cmd(Command::SetReactionSlaveCurve { reaction: 0, slave: 0, curve: false });
    h.cmd(Command::SetReactionStateBlend { reaction: 0, slave: 0, state: 0, strength: None, influence: Some(3.0), falloff: None });
    assert_eq!(h.app.state.selection.iter().copied().collect::<Vec<_>>(), vec![b], "the slave is selected");
    h.cmd(Command::SelectAndManipulate);
    // A maximized Perspective framing both boxes keeps the handles clear of the ViewCube.
    h.cmd(Command::MaximizeViewportToggle);
    h.cmd(Command::ZoomExtents);
    h.frames(60);
    let ms = crate::viewport_widget::manipulators(&h.app);
    let infl: Vec<_> = ms.iter().filter(|m| m.name == "Influence").collect();
    assert_eq!(infl.len(), 2, "an Influence sphere per state");
    assert!((infl[0].origin - Vec3::ZERO).length() < 1e-3 && (infl[0].ring.unwrap() - 3.0).abs() < 1e-3, "state 1 at master X 0, Influence 3");
    assert!((infl[1].origin - Vec3::new(10.0, 0.0, 0.0)).length() < 1e-3 && (infl[1].ring.unwrap() - 10.0).abs() < 1e-3, "state 2 at X 10, Influence 0 = the span");
    // Drag state 2's Influence handle further out along X.
    let from = h.screen_of(Vec3::new(20.0, 0.0, 0.0));
    let to = h.screen_of(Vec3::new(28.0, 0.0, 0.0));
    h.drag(from, to);
    let inf = h.app.state.scene.reactions[0].slaves[0].blend_of(1).influence;
    assert!(inf > 13.0, "the drag grew the Influence: {inf}");
    assert_eq!(h.app.state.scene.reactions[0].slaves[0].states[1].0, 10.0, "the state stays");
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.reactions[0].slaves[0].blend_of(1).influence, 0.0, "one undo step");
    // Drag state 2's centre: its master value moves along X.
    h.drag(h.screen_of(Vec3::new(10.0, 0.0, 0.0)), h.screen_of(Vec3::new(16.0, 0.0, 0.0)));
    let mv = h.app.state.scene.reactions[0].slaves[0].states[1].0;
    assert!(mv > 13.0, "the Reaction Value moved: {mv}");
    assert_eq!(h.app.state.selection.iter().copied().collect::<Vec<_>>(), vec![b], "the drag kept the selection");
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.reactions[0].slaves[0].states[1].0, 10.0, "one undo step");
}

/// Display > Hide: Unhide by Name... is unavailable while nothing is
/// hidden; after Hide Selected it opens its dialog.
#[test]
fn unhide_buttons_wait_for_something_hidden() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    h.app.panel_tab = crate::Tab::Display;
    h.frames(3);
    h.click(h.find(Role::Button, "Unhide by Name...").center());
    assert!(h.app.state.dialog.is_none(), "nothing hidden: the button is unavailable");
    h.cmd(Command::HideSelection);
    h.frames(2);
    h.click(h.find(Role::Button, "Unhide by Name...").center());
    assert!(h.app.state.dialog.is_some(), "now it opens Unhide Objects");
}

/// Edit > Redo names the operation it redoes.
#[test]
fn redo_menu_item_names_its_operation() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let label = h.app.state.undo.labels().last().cloned().unwrap();
    h.cmd(Command::UndoSceneOperation);
    h.click(h.find(Role::Button, "Edit").center());
    h.frames(2);
    assert!(h.has(Role::Button, &format!("Redo {label}")), "Redo {label} in the Edit menu");
}

/// Edit Named Selection Sets: Add Selected / Subtract Selected change a set's members.
#[test]
fn named_set_add_and_subtract_selected() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let a = selected(&h);
    h.cmd(Command::NamedSelectionSet { name: "props".into() });
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [30.0, 0.0, 0.0], size: [10.0; 3] });
    let b = selected(&h);
    h.cmd(Command::EditNamedSelectionSets);
    h.frames(2);
    h.click(h.find(Role::Button, "Add Selected").center());
    assert_eq!(h.app.state.named_sets.get("props"), Some(&vec![a, b]));
    h.cmd(Command::SelectNodes { ids: vec![a.0], op: loom_commands::SelectOp::Replace });
    h.click(h.find(Role::Button, "Subtract Selected").center());
    assert_eq!(h.app.state.named_sets.get("props"), Some(&vec![b]));
}

/// Create > Systems: Bones IK Chain, Biped and Daylight System start their creation tools.
#[test]
fn create_menu_systems_submenu() {
    let mut h = Harness::new();
    menu_path(&mut h, &["Create", "Systems", "Biped"]);
    assert_eq!(h.app.create_kind, Some(crate::CreateKind::Biped));
    menu_path(&mut h, &["Create", "Systems", "Daylight System"]);
    assert_eq!(h.app.create_kind, Some(crate::CreateKind::SunSystem { daylight: true }));
    menu_path(&mut h, &["Create", "Systems", "Bones IK Chain"]);
    assert_eq!(h.app.create_kind, Some(crate::CreateKind::Bones));
    assert!(h.app.state.bones_ik_chain, "with an IK chain");
    assert!(h.app.state.bones_ik_root, "Assign To Children turns Assign To Root on");
}

/// Systems > Bones > IK Chain Assignment: the solver list and Assign To
/// Children / Root; two clicked bones then get an IK Limb chain from the
/// root, and with Assign To Root off the chain starts at the second bone.
#[test]
fn bones_ik_chain_assignment_rollout() {
    let mut h = Harness::new();
    h.app.panel_tab = crate::Tab::Create;
    h.app.create_category = crate::Category::Systems;
    h.app.start_create(Some(crate::CreateKind::Bones));
    h.frames(3);
    assert!(!h.app.state.bones_ik_chain && !h.app.state.bones_ik_root, "both off by default");
    h.click(h.find(Role::CheckBox, "Assign To Children").center());
    assert!(h.app.state.bones_ik_chain && h.app.state.bones_ik_root);
    h.click_text_anywhere("HI Solver");
    h.frames(2);
    h.click_text_anywhere("IK Limb Solver");
    assert_eq!(h.app.state.bones_ik_solver, loom_scene::ik::IkSolver::Limb);
    h.cmd(Command::CreateBoneChain { points: vec![[0.0; 3], [0.0, 0.0, 20.0], [0.0, 0.0, 40.0]], name: None });
    let c = h.app.state.scene.ik_chains.last().expect("a chain").clone();
    assert_eq!(c.solver, loom_scene::ik::IkSolver::Limb);
    let root = h.app.state.scene.nodes().find(|n| n.object.is_bone() && n.parent.is_none()).unwrap().id;
    assert_eq!(c.start, root, "Assign To Root: from the root bone");
    h.click(h.find(Role::CheckBox, "Assign To Root").center());
    assert!(!h.app.state.bones_ik_root);
    let n = h.app.state.scene.ik_chains.len();
    h.cmd(Command::CreateBoneChain { points: vec![[50.0, 0.0, 0.0], [50.0, 0.0, 20.0], [50.0, 0.0, 40.0], [50.0, 0.0, 60.0]], name: None });
    let c = h.app.state.scene.ik_chains.last().unwrap().clone();
    assert_eq!(h.app.state.scene.ik_chains.len(), n + 1);
    assert!(h.app.state.scene.node(c.start).unwrap().parent.is_some(), "the chain starts below the root");
}

/// Bone Tools > Delete: the selected bone goes with its links (its child
/// stays where it is, unlinked) and its parent bone ends in a new nub;
/// Remove instead links the children to the parent.
#[test]
fn bone_tools_delete_unlinks_and_ends_the_parent_with_a_nub() {
    let mut h = Harness::new();
    let r = h.app.dispatch(Command::CreateBoneChain { points: vec![[0.0; 3], [0.0, 0.0, 20.0], [0.0, 0.0, 40.0], [0.0, 0.0, 60.0]], name: None }).unwrap();
    let ids: Vec<loom_scene::NodeId> = r["nodes"].as_array().unwrap().iter().map(|v| loom_scene::NodeId(v.as_u64().unwrap())).collect();
    let (root, mid, child) = (ids[0], ids[1], ids[2]);
    let child_world = h.app.state.scene.world_matrix(child);
    h.cmd(Command::SelectNodes { ids: vec![mid.0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::BoneTools);
    h.frames(2);
    h.click_text_anywhere("Delete");
    let s = &h.app.state.scene;
    assert!(s.node(mid).is_none(), "deleted");
    assert_eq!(s.node(child).unwrap().parent, None, "the child is unlinked");
    assert!(s.world_matrix(child).abs_diff_eq(child_world, 1e-3), "and stays where it was");
    let kids = s.children(root);
    assert_eq!(kids.len(), 1, "a nub ends the parent");
    assert!(s.node(kids[0]).unwrap().object.is_bone());
}
