//! Controller dialogs added by the parity23 round (Expression, Noise,
//! Spring, Audio), clicked and typed as a person does.

use super::*;
use egui::accesskit::Role;

/// Opens a menu bar menu and its submenus, then clicks the item.
fn menu_path(h: &mut Harness, path: &[&str]) {
    h.click(h.find(Role::Button, path[0]).center());
    let in_menu = |h: &Harness, label: &str| {
        let cp = egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new("command_panel")).map(|p| p.rect);
        let all = h.find_all(Role::Button, label);
        all.iter().copied().find(|r| !cp.is_some_and(|c| c.contains(r.center()))).unwrap_or_else(|| panic!("no menu entry {label:?}: {all:?}")).center()
    };
    for sub in &path[1..path.len() - 1] {
        let p = in_menu(h, sub);
        h.run(vec![egui::Event::PointerMoved(p)]);
        h.frames(2);
    }
    let p = in_menu(h, path[path.len() - 1]);
    h.click(p);
}

/// Clicks into the text field `id` (an egui Id source), selects all and types `text`.
fn type_into(h: &mut Harness, rect: egui::Rect, text: &str) {
    h.click(rect.center());
    h.mods = egui::Modifiers::COMMAND;
    h.key(egui::Key::A);
    h.mods = egui::Modifiers::NONE;
    h.run(vec![egui::Event::Text(text.into())]);
    h.frames(1);
}

/// The text field right of the label `label` on its row.
fn field_after(h: &Harness, label: &str) -> egui::Rect {
    let l = h.find(Role::Label, label);
    h.widgets
        .iter()
        .filter(|(r, _, w)| matches!(r, Role::TextInput | Role::MultilineTextInput) && w.min.x >= l.max.x - 1.0 && (w.center().y - l.center().y).abs() < l.height())
        .min_by(|a, b| a.2.min.x.total_cmp(&b.2.min.x))
        .map(|w| w.2)
        .unwrap_or_else(|| panic!("no text field after {label:?}"))
}

/// The text field just below the label `label`.
fn field_below(h: &Harness, label: &str) -> egui::Rect {
    let l = h.find(Role::Label, label);
    h.widgets
        .iter()
        .filter(|(r, _, w)| matches!(r, Role::TextInput | Role::MultilineTextInput) && w.min.y >= l.max.y - 1.0 && w.max.x > l.min.x && w.min.x < l.max.x + 40.0)
        .min_by(|a, b| a.2.min.y.total_cmp(&b.2.min.y))
        .map(|w| w.2)
        .unwrap_or_else(|| panic!("no text field below {label:?}"))
}

/// The button `text` inside the window with the id `window`.
fn button_in(h: &Harness, window: &str, text: &str) -> egui::Rect {
    let area = h.ctx.memory(|m| m.area_rect(egui::Id::new(window))).unwrap_or_else(|| panic!("no window {window:?}"));
    *h.find_all(Role::Button, text).iter().find(|r| area.contains(r.center())).unwrap_or_else(|| panic!("no {text:?} in {window:?}"))
}

fn new_box(h: &mut Harness) -> loom_scene::NodeId {
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    *h.app.state.selection.iter().next().unwrap()
}

#[test]
fn position_expression_from_the_animation_menu_evaluates_typed_text() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0));
    let b = new_box(&mut h);
    menu_path(&mut h, &["Animation", "Position Controllers", "Position Expression"]);
    h.frames(2);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::ExpressionController));
    for b in ["Create", "Delete", "Rename", "Assign to Constant", "Assign to Controller", "Function List", "Save", "Load", "Debug", "Evaluate", "Close"] {
        assert!(h.has(Role::Button, b), "the dialog has {b}");
    }
    // Name: radius, Scalar, Create.
    let name = field_after(&h, "Name:");
    type_into(&mut h, name, "radius");
    h.click(button_in(&h, "Expression Controller", "Create").center());
    h.frames(2);
    let e: loom_scene::expr::ExprCtrl = loom_commands::ctl_props::read_ctl(&h.app.state, b, "Position", "Expression").unwrap().1;
    assert_eq!(e.vars.len(), 1);
    assert!(!e.vars[0].vector, "Scalar is the default");
    // Assign to Constant: 150.
    h.click(button_in(&h, "Expression Controller", "Assign to Constant").center());
    h.frames(2);
    let value = h.spinner_after("Value:");
    type_into(&mut h, value, "150");
    h.key(egui::Key::Enter);
    h.click(button_in(&h, "expr_constant", "OK").center());
    h.frames(2);
    let e: loom_scene::expr::ExprCtrl = loom_commands::ctl_props::read_ctl(&h.app.state, b, "Position", "Expression").unwrap().1;
    assert_eq!(e.vars[0].constant[0], 150.0);
    // The expression field: a circle of that radius.
    let expr = field_below(&h, "Expression");
    type_into(&mut h, expr, "[radius*cos(360*NT), radius*sin(360*NT), 0]");
    h.click(button_in(&h, "Expression Controller", "Evaluate").center());
    h.frames(2);
    h.cmd(Command::SetFrame { frame: 25 });
    let p = h.app.state.scene.world_matrix(b).w_axis.truncate();
    assert!(p.distance(Vec3::new(0.0, 150.0, 0.0)) < 1e-2, "{p:?}");
    // A typo shows the error and changes nothing.
    let expr = field_below(&h, "Expression");
    type_into(&mut h, expr, "[radius*, 0, 0]");
    h.click(button_in(&h, "Expression Controller", "Evaluate").center());
    h.frames(2);
    assert!(h.has(Role::Label, "Error: [radius*,"), "{:?}", h.widgets.iter().filter(|w| w.0 == Role::Label).map(|w| &w.1).collect::<Vec<_>>());
    // Function List and Debug open their windows.
    h.click(button_in(&h, "Expression Controller", "Function List").center());
    h.click(button_in(&h, "Expression Controller", "Debug").center());
    h.frames(2);
    assert!(h.has(Role::Label, "noise(p,q,r)"), "the Function List");
    assert!(h.has(Role::Label, "NT") && h.has(Role::Label, "0.250"), "the Debug window shows NT at frame 25");
    h.click(button_in(&h, "Expression Controller", "Close").center());
    h.frames(2);
    assert_eq!(h.app.state.dialog, None);
}

#[test]
fn expression_assign_to_controller_picks_a_track_in_the_track_view_pick_window() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0));
    let leader = new_box(&mut h);
    let lname = h.app.state.scene.node(leader).unwrap().name.clone();
    h.cmd(Command::MoveSelection { delta: [40.0, 0.0, 0.0], system: None });
    let b = new_box(&mut h);
    h.cmd(Command::AssignController { id: b.0, track: "Position".into(), controller: "Position Expression".into() });
    let name = field_after(&h, "Name:");
    type_into(&mut h, name, "lead");
    h.click(h.find(Role::RadioButton, "Vector").center());
    h.click(button_in(&h, "Expression Controller", "Create").center());
    h.click(button_in(&h, "Expression Controller", "Assign to Controller").center());
    h.frames(2);
    h.click_text_anywhere(&format!("+ {lname}"));
    h.frames(2);
    // The leader's tracks; its Position is the lowest "Position" on screen.
    let pos = h.find_all(Role::Button, "Position");
    h.click(pos.last().unwrap().center());
    h.click(button_in(&h, "expr_pick", "OK").center());
    h.frames(2);
    let e: loom_scene::expr::ExprCtrl = loom_commands::ctl_props::read_ctl(&h.app.state, b, "Position", "Expression").unwrap().1;
    assert_eq!(e.vars[0].source.as_ref().map(|s| s.label.as_str()), Some(format!("{lname} \\ Position").as_str()));
    let expr = field_below(&h, "Expression");
    type_into(&mut h, expr, "lead + [0, 0, 5]");
    h.click(button_in(&h, "Expression Controller", "Evaluate").center());
    h.frames(2);
    assert!(h.app.state.scene.world_matrix(b).w_axis.truncate().distance(Vec3::new(40.0, 0.0, 5.0)) < 1e-3);
}

#[test]
fn noise_spring_and_audio_dialogs_edit_their_controllers() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0));
    h.app.native_file_dialogs = false;
    let b = new_box(&mut h);
    // Noise: Fractal Noise turns on Roughness.
    h.cmd(Command::AssignController { id: b.0, track: "Scale".into(), controller: "Noise Scale".into() });
    assert!(h.has(Role::CheckBox, "Fractal Noise") && h.has(Role::CheckBox, ">0"));
    h.click(h.find(Role::CheckBox, "Fractal Noise").center());
    h.frames(2);
    let n: loom_scene::param::NoiseCtrl = loom_commands::ctl_props::read_ctl(&h.app.state, b, "Scale", "Noise").unwrap().1;
    assert!(n.fractal);
    let s = h.spinner_after("X Strength:");
    type_into(&mut h, s, "20");
    h.key(egui::Key::Enter);
    let n: loom_scene::param::NoiseCtrl = loom_commands::ctl_props::read_ctl(&h.app.state, b, "Scale", "Noise").unwrap().1;
    assert!((n.strength[0] - 0.2).abs() < 1e-5, "scale strengths are percent: {:?}", n.strength);
    // Spring: Self Influence listed; Add picks springs in the viewports.
    h.cmd(Command::AssignController { id: b.0, track: "Position".into(), controller: "Spring".into() });
    assert!(h.has(Role::Button, "Self Influence  2.00/0.50"), "{:?}", h.widgets.iter().filter(|w| w.0 == Role::Button).map(|w| &w.1).collect::<Vec<_>>());
    h.click(h.find(Role::Button, "Add").center());
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::SpringAdd));
    h.click(h.find(Role::Button, "Add").center());
    assert_eq!(h.app.state.pick_mode, None);
    let m = h.spinner_after("Mass:");
    type_into(&mut h, m, "500");
    h.key(egui::Key::Enter);
    let sp: loom_scene::param::SpringCtrl<Vec3> = loom_commands::ctl_props::read_ctl(&h.app.state, b, "Position", "Spring").unwrap().1;
    assert_eq!(sp.mass, 500.0);
    // Audio: Choose Sound opens the in-app file dialog for a WAV.
    h.cmd(Command::AssignController { id: b.0, track: "Rotation".into(), controller: "Audio Rotation".into() });
    assert!(h.has(Role::Label, "No sound"));
    h.click(h.find(Role::CheckBox, "Absolute Value").center());
    h.frames(2);
    let a: loom_scene::audio::AudioCtrl = loom_commands::ctl_props::read_ctl(&h.app.state, b, "Rotation", "Audio").unwrap().1;
    assert!(a.absolute);
    h.click(h.find(Role::Button, "Choose Sound").center());
    h.frames(2);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::AudioSoundFile));
}

#[test]
fn xref_scenes_dialog_visible_box_and_ignore_checkboxes() {
    let d = std::env::temp_dir().join(format!("loom-ui-xref-p23-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    let a = d.join("set.loom");
    let mut src = loom_commands::AppState::new();
    loom_commands::execute(&mut src, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    loom_commands::execute(&mut src, Command::CreateLight { kind: loom_commands::light::LightKind::Omni, position: [0.0, 0.0, 50.0], target: [0.0; 3] }).unwrap();
    loom_commands::execute(&mut src, Command::SaveFileAsCopy { path: Some(a.to_string_lossy().into_owned()) }).unwrap();
    let ap = a.to_string_lossy().into_owned();
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0));
    h.app.native_file_dialogs = false;
    h.cmd(Command::XrefScene { path: Some(ap.clone()), automatic: Some(false) });
    h.cmd(Command::XrefRecords { scene: true });
    h.click_text_anywhere(&ap);
    h.frames(1);
    for c in ["Visible", "Box", "Lights", "Cameras", "Shapes", "Helpers", "Animation"] {
        assert!(h.has(Role::CheckBox, c), "{c}");
    }
    h.click(h.find(Role::CheckBox, "Box").center());
    h.frames(1);
    assert!(h.app.state.scene.xrefs[0].box_display);
    h.click(h.find(Role::CheckBox, "Visible").center());
    h.frames(1);
    assert!(!h.app.state.scene.xrefs[0].visible);
    assert_eq!(h.app.state.scene.nodes().count(), 2);
    h.click(h.find(Role::CheckBox, "Lights").center());
    h.frames(1);
    assert!(h.app.state.scene.xrefs[0].ignore.lights);
    assert_eq!(h.app.state.scene.nodes().count(), 1, "the light is left out");
}

#[test]
fn ring_array_drags_out_from_the_create_panel_and_edits_on_the_motion_panel() {
    let mut h = Harness::new();
    wide_view(&mut h);
    h.app.panel_tab = crate::Tab::Create;
    h.app.create_category = crate::Category::Systems;
    h.frames(2);
    let cp = panel(&h, "command_panel");
    click_text(&mut h, "Ring Array", cp, 0.0);
    assert_eq!(h.app.create_kind, Some(crate::CreateKind::RingArray));
    let a = h.screen_of(Vec3::ZERO);
    let b = h.screen_of(Vec3::new(40.0, 0.0, 0.0));
    h.drag(a, b);
    let dummy = *h.app.state.selection.iter().next().expect("the dummy is selected");
    let (_, boxes) = loom_commands::ring::members(&h.app.state, dummy).expect("a ring");
    assert_eq!(boxes.len(), 4);
    let r = loom_commands::ring::ring_of(&h.app.state, dummy).unwrap().radius.get();
    assert!(r > 20.0, "the drag set the radius: {r}");
    // Motion panel of a box: Number 6.
    h.cmd(Command::SelectNodes { ids: vec![boxes[0].0], op: loom_commands::SelectOp::Replace });
    h.app.panel_tab = crate::Tab::Motion;
    h.frames(2);
    let n = h.spinner_after("Number:");
    h.click(n.center());
    h.mods = egui::Modifiers::COMMAND;
    h.key(egui::Key::A);
    h.mods = egui::Modifiers::NONE;
    h.run(vec![egui::Event::Text("6".into())]);
    h.key(egui::Key::Enter);
    assert_eq!(loom_commands::ring::members(&h.app.state, dummy).unwrap().1.len(), 6);
}
