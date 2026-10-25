//! Skin the way a person uses it: the Vertices filter, Absolute /
//! Relative, Paste to Multiple Bones, adding a cross section on the bone
//! and dragging its ring, dragging an envelope end point, Painter Options
//! (pressure, falloff graph), the Weight Table (exclusions, Flip UI,
//! vertex sets, double-click highlight), the Advanced / Display / Mirror
//! controls, Load Envelopes, gizmos with Edit Lattice and the angle curves.

use super::*;

/// Every text `text` in the command panel, top to bottom.
fn panel_texts(h: &Harness, text: &str) -> Vec<egui::Pos2> {
    let right = h.app.viewport_rects[h.app.state.active_viewport].right();
    let mut v: Vec<egui::Pos2> = h.text_centers(text).into_iter().filter(|p| p.x > right).collect();
    v.sort_by(|a, b| a.y.total_cmp(&b.y));
    v
}

fn skin(h: &Harness) -> loom_scene::skin::SkinData {
    loom_commands::skin::entry(&h.app.state).unwrap().data
}

fn current_envelope(h: &Harness) -> loom_commands::skin::EnvelopeWorld {
    loom_commands::skin::shown_envelopes_world(&h.app.state).into_iter().find(|e| e.current).unwrap()
}

/// Opens a collapsed rollout by its title.
fn open_rollout(h: &mut Harness, title: &str) {
    let p = panel_text(h, title);
    h.click(p);
}

#[test]
fn absolute_relative_buttons_and_paste_to_multiple_bones() {
    let mut h = Harness::new();
    let (_, bones) = skinned_cylinder(&mut h);
    h.cmd(Command::SkinSelectBone { id: bones[0] });
    h.click(panel_text(&h, "R"));
    assert!(skin(&h).bones[0].envelope.relative, "R makes the envelope Relative");
    h.click(panel_text(&h, "A"));
    assert!(!skin(&h).bones[0].envelope.relative, "A makes it Absolute");
    h.cmd(Command::SkinSetEnvelope { inner_start: Some(3.0), inner_end: Some(3.0), outer_start: Some(9.0), outer_end: Some(9.0), falloff: None, abs_effect: None, squash: None });
    h.click(panel_text(&h, "Copy"));
    h.click(panel_text(&h, "Paste to Multiple Bones"));
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::PasteToMultipleBones));
    h.frames(2);
    let name = h.app.state.scene.node(loom_scene::NodeId(bones[1])).unwrap().name.clone();
    h.click(window_text(&h, "Paste to Multiple Bones", &name));
    h.click(window_text(&h, "Paste to Multiple Bones", "Paste"));
    let d = skin(&h);
    assert_eq!(d.bones[1].envelope.inner, [3.0, 3.0]);
    assert_eq!(d.bones[1].envelope.outer, [9.0, 9.0]);
    assert_ne!(h.app.state.dialog, Some(loom_commands::Dialog::PasteToMultipleBones), "Paste closes the dialog");
}

#[test]
fn add_cross_section_by_clicking_the_bone_then_drag_its_ring_and_an_end_point() {
    let mut h = Harness::new();
    let (arm, bones) = skinned_cylinder(&mut h);
    h.cmd(Command::SkinSelectBone { id: bones[0] });
    // Cross Sections > Add is the second "Add" in the panel (the first is the Bones list's).
    let adds = panel_texts(&h, "Add");
    assert_eq!(adds.len(), 2);
    h.click(adds[1]);
    assert!(h.app.state.skin_ui.adding_cross_section);
    let on_bone = h.screen_of(Vec3::new(0.0, 0.0, 10.0));
    h.click(on_bone);
    let d = skin(&h);
    assert_eq!(d.bones[0].cross_sections.len(), 1, "the click on the bone placed a cross section");
    assert!((d.bones[0].cross_sections[0].0 - 0.5).abs() < 0.05, "{:?}", d.bones[0].cross_sections);
    // Drag the new section's outer ring outwards: its radius grows, one undo entry.
    let env = current_envelope(&h);
    let (u, _, outer) = env.sections()[1];
    let pts = crate::skin_view::ring_points(&env, u, outer);
    let q = pts.iter().map(|w| h.screen_of(*w)).max_by(|a, b| (a.x - h.screen_of(env.point(u)).x).abs().total_cmp(&(b.x - h.screen_of(env.point(u)).x).abs())).unwrap();
    let centre = h.screen_of(env.point(u));
    let out = (q - centre).normalized() * 30.0;
    let undo = h.app.state.undo.len();
    h.drag(q, q + out);
    let d = skin(&h);
    assert!(d.bones[0].cross_sections[0].2 > outer + 0.5, "outer radius {} -> {}", outer, d.bones[0].cross_sections[0].2);
    assert_eq!(h.app.state.skin_ui.cross_section, Some((1, true)));
    assert_eq!(h.app.state.undo.len(), undo + 1);
    // Drag the envelope's end point sideways: it moves, the object stays.
    let before = h.app.state.scene.node(arm).unwrap().transform.clone();
    let end = h.screen_of(current_envelope(&h).end);
    h.drag(end, end + egui::vec2(40.0, 0.0));
    let moved = current_envelope(&h).end;
    assert!((moved - Vec3::new(0.0, 0.0, 20.0)).length() > 1.0, "{moved:?}");
    assert_eq!(h.app.state.skin_handle, Some(1));
    assert_eq!(h.app.state.scene.node(arm).unwrap().transform, before);
    // Remove takes the selected (added) cross section away.
    h.cmd(Command::SkinSelectCrossSection { index: 1, outer: false });
    let removes = panel_texts(&h, "Remove");
    h.click(*removes.last().unwrap());
    assert!(skin(&h).bones[0].cross_sections.is_empty());
}

#[test]
fn painter_options_pressure_and_the_falloff_graph() {
    let mut h = Harness::new();
    skinned_cylinder(&mut h);
    h.click(panel_text(&h, "..."));
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::PainterOptions));
    h.frames(2);
    window_text(&h, "Painter Options", "Min. Strength:");
    window_text(&h, "Painter Options", "Min. Size:");
    h.click(window_text(&h, "Painter Options", "Enable Pressure Sensitivity"));
    assert!(h.app.state.skin_painter.pressure_on);
    // Drag the graph's middle point down: the brush is weaker halfway out.
    let graph = h.widgets.iter().find(|w| w.1 == "Falloff graph").map(|w| w.2).expect("the falloff graph");
    let before = h.app.state.skin_painter.falloff_at(0.5);
    h.drag(graph.center(), graph.center() + egui::vec2(0.0, 25.0));
    assert_eq!(h.app.state.skin_painter.falloff, loom_commands::skin::Painter::CUSTOM);
    assert!(h.app.state.skin_painter.falloff_at(0.5) < before - 0.1, "{before} -> {}", h.app.state.skin_painter.falloff_at(0.5));
    // A falloff button puts its curve back.
    h.click(window_text(&h, "Painter Options", "Linear"));
    assert!((h.app.state.skin_painter.falloff_at(0.5) - 0.5).abs() < 1e-4);
}

#[test]
fn weight_table_exclusions_flip_ui_vertex_sets_and_highlight() {
    let mut h = Harness::new();
    let (_, bones) = skinned_cylinder(&mut h);
    h.click(panel_text(&h, "Weight Table"));
    h.frames(2);
    // Options > Show Exclusions: a box before each cell.
    h.click(window_text(&h, "Weight Table", "Options"));
    h.click_text_anywhere("Show Exclusions");
    h.key(egui::Key::Escape);
    h.frames(2);
    let row = window_text(&h, "Weight Table", "#1");
    let area = h.ctx.memory(|m| m.area_rect(egui::Id::new("Weight Table"))).unwrap();
    let mut boxes: Vec<egui::Rect> = h.widgets.iter().filter(|w| w.0 == egui::accesskit::Role::CheckBox && area.contains_rect(w.2) && (w.2.center().y - row.y).abs() < 6.0).map(|w| w.2).collect();
    boxes.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
    assert_eq!(boxes.len(), 2, "one exclusion box per bone");
    h.click(boxes[1].center());
    assert!(skin(&h).excluded[1].contains(&1), "vertex 1 excluded from the second bone");
    assert_eq!(skin_weight(&h, 1, 1), 0.0);
    // Double-click a vertex ID: it shows pink in the viewports.
    // Apart from the last click, so the two clicks make a double-click.
    h.frames(12);
    let id2 = window_text(&h, "Weight Table", "#2");
    h.click(id2);
    h.click(id2);
    assert_eq!(h.app.state.skin_ui.highlight, Some(2));
    // Vertex Sets > Create Set from the selected vertex.
    h.click(window_text(&h, "Weight Table", "Vertex Sets"));
    let field = h.widgets.iter().find(|w| w.0 == egui::accesskit::Role::TextInput && w.2.min.y > area.min.y).map(|w| w.2).expect("the set name field");
    h.click(field.center());
    h.run(vec![egui::Event::Text("joint".into())]);
    h.frames(1);
    h.click_text_anywhere("Create Set");
    assert_eq!(skin(&h).vertex_sets, vec![("joint".to_owned(), vec![2])]);
    // Options > Flip UI: the bones run down the left.
    h.click(window_text(&h, "Weight Table", "Options"));
    h.click_text_anywhere("Flip UI");
    h.key(egui::Key::Escape);
    h.frames(2);
    window_text(&h, "Weight Table", "Bone / Vertex ID");
    let names: Vec<String> = bones[..2].iter().map(|b| h.app.state.scene.node(loom_scene::NodeId(*b)).unwrap().name.clone()).collect();
    let (a, b) = (window_text(&h, "Weight Table", &names[0]), window_text(&h, "Weight Table", &names[1]));
    assert!(b.y > a.y + 5.0 && (a.x - b.x).abs() < 2.0, "bone names in a column: {a:?} {b:?}");
}

#[test]
fn advanced_display_and_mirror_controls_set_the_skin() {
    let mut h = Harness::new();
    let (_, _) = skinned_cylinder(&mut h);
    open_rollout(&mut h, "Advanced Parameters");
    for (label, get) in [
        ("Rigid Vertices (All)", (|d: &loom_scene::skin::SkinData| d.rigid_all) as fn(&loom_scene::skin::SkinData) -> bool),
        ("Ignore Bone Scale", |d| d.ignore_bone_scale),
        ("Fast Updates", |d| d.fast_updates),
        ("Animatable Envelopes", |d| d.animatable_envelopes),
        ("Update on mouse up", |d| d.update_on_mouse_up),
    ] {
        h.click(panel_text(&h, label));
        assert!(get(&skin(&h)), "{label}");
    }
    h.click(panel_text(&h, "Back Transform Vertices"));
    assert!(!skin(&h).back_transform, "Back Transform Vertices starts on");
    // Reset All Bones after a hand edit.
    h.cmd(Command::SkinSelectVertices { ids: vec![0, 1], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::SetWeightTo05Skin);
    assert!(skin(&h).explicit[0]);
    h.click(panel_text(&h, "Reset All Bones"));
    assert!(!skin(&h).explicit[0]);
    open_rollout(&mut h, "Display");
    h.click(panel_text(&h, "Show All Vertices"));
    assert!(skin(&h).display.all_vertices);
    h.click(panel_text(&h, "Show Hidden Vertices"));
    assert!(skin(&h).display.show_hidden_vertices);
    open_rollout(&mut h, "Mirror Parameters");
    h.click(panel_text(&h, "Mirror Mode"));
    assert!(skin(&h).mirror.mode);
    h.click(panel_text(&h, "Manual Update"));
    assert!(skin(&h).mirror.manual_update);
    h.click(panel_text(&h, "Update"));
    assert!(h.app.state.skin_ui.mirror_sides.is_some(), "Update stores the sides");
}

#[test]
fn load_envelopes_dialog_lines_up_by_name_and_loads() {
    let mut h = Harness::new();
    let (_, bones) = skinned_cylinder(&mut h);
    h.cmd(Command::SkinSelectBone { id: bones[1] });
    h.cmd(Command::SkinSetEnvelope { inner_start: Some(6.5), inner_end: None, outer_start: None, outer_end: None, falloff: None, abs_effect: None, squash: None });
    let dir = std::env::temp_dir().join(format!("loom-ui-env-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cyl.env").to_string_lossy().into_owned();
    h.cmd(Command::SkinSaveEnvelopes { path: Some(path.clone()) });
    h.cmd(Command::SkinSetEnvelope { inner_start: Some(1.0), inner_end: None, outer_start: None, outer_end: None, falloff: None, abs_effect: None, squash: None });
    h.cmd(Command::SkinLoadEnvelopes { path: Some(path) });
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::LoadEnvelopes));
    h.frames(2);
    // Swap the incoming rows by hand, then Match by Name puts them back.
    let name0 = h.app.state.scene.node(loom_scene::NodeId(bones[0])).unwrap().name.clone();
    let rows = h.text_centers(&name0);
    h.click(*rows.iter().min_by(|a, b| a.x.total_cmp(&b.x)).unwrap());
    h.click(window_text(&h, "Load Envelopes", "Move Down"));
    h.click(window_text(&h, "Load Envelopes", "Match by Name"));
    h.click(window_text(&h, "Load Envelopes", "OK"));
    assert_eq!(skin(&h).bones[1].envelope.inner[0], 6.5);
    assert!(h.app.state.skin_ui.load.is_none());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn gizmos_rollout_adds_deformers_and_edit_lattice_picks_and_curves_edit() {
    let mut h = Harness::new();
    let (_, bones) = skinned_cylinder(&mut h);
    h.cmd(Command::SkinSelectBone { id: bones[1] });
    open_rollout(&mut h, "Gizmos");
    // The deformer drop-down: Morph Angle Deformer, then Add Gizmo.
    h.click(panel_text(&h, "Joint Angle Deformer"));
    h.click_text_anywhere("Morph Angle Deformer");
    h.click(panel_text(&h, "Add Gizmo"));
    let d = skin(&h);
    assert_eq!(d.gizmos.len(), 1);
    assert_eq!(d.gizmos[0].kind, loom_scene::skin::GizmoKind::MorphAngle);
    panel_text(&h, "Add From Stack");
    // A Joint Angle deformer, its lattice picked in the viewport with Edit Lattice on.
    h.click(panel_text(&h, "Morph Angle Deformer"));
    h.click_text_anywhere("Joint Angle Deformer");
    h.click(panel_text(&h, "Add Gizmo"));
    assert_eq!(skin(&h).gizmos[1].kind, loom_scene::skin::GizmoKind::JointAngle);
    h.click(panel_text(&h, "Edit Lattice"));
    assert!(h.app.state.skin_ui.edit_lattice);
    let pts = loom_commands::skin::lattice_world_points(&h.app.state).unwrap();
    let screens: Vec<egui::Pos2> = pts.iter().map(|q| h.screen_of(*q)).collect();
    let k = (0..screens.len()).find(|i| (0..screens.len()).all(|j| j == *i || screens[j].distance(screens[*i]) > 10.0)).expect("a lattice point clear of the others");
    h.click(screens[k]);
    assert_eq!(h.app.state.skin_ui.lattice_selection.iter().copied().collect::<Vec<_>>(), vec![k as u32]);
    // Edit Angle Keys Curves: drag the key point up and the offset grows.
    h.click(panel_text(&h, "Edit Angle Keys Curves"));
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::AngleKeysCurves));
    h.frames(2);
    let plot = h.widgets.iter().find(|w| w.1 == "Angle keys curves").map(|w| w.2).expect("the curves plot");
    let key_point = egui::pos2(plot.left() + 1.0, plot.center().y);
    h.drag(key_point, key_point - egui::vec2(0.0, 20.0));
    let g = &skin(&h).gizmos[1];
    assert!(g.keys[0].lattice.get(k).is_some_and(|o| o.x > 0.0), "{:?}", g.keys[0].lattice.get(k));
}
