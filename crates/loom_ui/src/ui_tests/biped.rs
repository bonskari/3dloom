//! Biped the way a person uses it: the Motion panel's Structure rollout
//! (Body Type, check boxes, twist links, Xtras), Save / Load File of figure
//! files, and footsteps.

use super::*;

/// A biped made through the registry, selected, with the Motion panel open on a tall window.
fn biped(h: &mut Harness) -> loom_scene::NodeId {
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 2600.0));
    h.cmd(serde_json::from_value(serde_json::json!({ "cmd": "Create Biped", "args": { "position": [0.0, 0.0, 0.0] } })).unwrap());
    h.app.panel_tab = crate::Tab::Motion;
    h.frames(3);
    *h.app.state.selection.iter().next().unwrap()
}

fn structure(h: &Harness, root: loom_scene::NodeId) -> loom_scene::biped::BipedStructure {
    loom_commands::biped::structure(&h.app.state, root).unwrap().clone()
}

fn has_part(h: &Harness, name: &str) -> bool {
    h.app.state.scene.nodes().any(|n| n.name == name)
}

/// Clicks the check box labelled `label` in the Command Panel.
fn click_check(h: &mut Harness, label: &str) {
    let r = h.find(egui::accesskit::Role::CheckBox, label);
    h.click(r.center());
    h.frames(2);
}

#[test]
fn structure_rollout_body_type_and_check_boxes() {
    let mut h = Harness::new();
    let root = biped(&mut h);
    assert!(h.has(egui::accesskit::Role::Label, "Body Type:"), "the Structure rollout shows in Figure Mode");
    // Body Type: open the list and pick Classic.
    h.click_widget("Skeleton");
    h.frames(2);
    h.click_text_anywhere("Classic");
    h.frames(2);
    assert_eq!(structure(&h, root).body_type, loom_scene::biped::BodyType::Classic);
    // Knuckles: five fingers of four links, and ForeFeet greys out.
    click_check(&mut h, "Knuckles");
    let s = structure(&h, root);
    assert!(s.knuckles.get() && s.fingers.get() == 5 && s.finger_links.get() == 4);
    assert!(has_part(&h, "Bip001 L Finger43"));
    click_check(&mut h, "ForeFeet");
    assert!(!structure(&h, root).forefeet.get(), "ForeFeet is disabled while Knuckles is on");
    // Triangle Neck reparents the clavicles.
    click_check(&mut h, "Triangle Neck");
    let clav = h.app.state.scene.nodes().find(|n| n.name == "Bip001 L Clavicle").unwrap().parent;
    assert_eq!(h.app.state.scene.node(clav.unwrap()).unwrap().name, "Bip001 Spine3");
    // Props 1: the check box right of "Props:".
    let props = h.find(egui::accesskit::Role::Label, "Props:");
    let one = h.find_all(egui::accesskit::Role::CheckBox, "1").into_iter().find(|r| (r.center().y - props.center().y).abs() < 10.0).expect("Props 1");
    h.click(one.center());
    h.frames(2);
    assert!(has_part(&h, "Bip001 Prop1"));
    // Twists on, then a Forearm count typed into its spinner.
    click_check(&mut h, "Twists");
    let spin = h.spinner_after("Forearm:");
    h.click(spin.center());
    h.mods = egui::Modifiers::COMMAND;
    h.key(egui::Key::A);
    h.mods = egui::Modifiers::NONE;
    h.run(vec![egui::Event::Text("2".into())]);
    h.key(egui::Key::Enter);
    h.frames(2);
    assert_eq!(structure(&h, root).forearm_twists.get(), 2);
    assert!(has_part(&h, "Bip001 R ForeTwist1"));
    // Outside Figure Mode the rollout is locked.
    h.click_widget("Figure Mode (on)");
    h.frames(2);
    click_check(&mut h, "Triangle Pelvis");
    assert!(!structure(&h, root).triangle_pelvis.get());
}

/// Footstep Mode, Create Multiple Footsteps through its dialog, then Create
/// Keys for Inactive Footsteps: the biped walks when the time changes.
#[test]
fn footsteps_from_the_dialog_make_the_biped_walk() {
    let mut h = Harness::new();
    let root = biped(&mut h);
    h.click_widget("Footstep Mode");
    h.frames(2);
    assert!(structure(&h, root).footstep_mode && !structure(&h, root).figure_mode);
    h.click_widget("Create Multiple Footsteps");
    h.frames(2);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::BipedMultipleFootsteps));
    assert!(!h.text_rects("Create Multiple Footsteps: Walk").is_empty(), "the Walk dialog");
    h.click_text_anywhere("OK");
    h.frames(2);
    assert_eq!(h.app.state.dialog, None);
    assert_eq!(structure(&h, root).footsteps.len(), 4);
    let com0 = h.app.state.scene.world_matrix(root).w_axis.truncate();
    h.click_widget("Create Keys for Inactive Footsteps");
    h.frames(2);
    assert!(structure(&h, root).footsteps.iter().all(|f| f.active));
    h.cmd(Command::SetFrame { frame: 45 });
    h.frames(1);
    let com = h.app.state.scene.world_matrix(root).w_axis.truncate();
    assert!(com.y < com0.y - 20.0, "the biped walked forward: {com} from {com0}");
}

/// Create Footsteps (append): clicks in the Top view place footsteps there.
#[test]
fn create_footsteps_append_takes_viewport_clicks() {
    let mut h = Harness::new();
    let root = biped(&mut h);
    h.cmd(Command::BipedFootstepMode { on: Some(true) });
    h.cmd(Command::TopView);
    maximized(&mut h);
    let cam = &mut h.app.state.active_mut().camera;
    cam.target = Vec3::new(0.0, -40.0, 0.0);
    cam.distance = 250.0;
    h.frames(2);
    h.click_widget("Create Footsteps (append)");
    h.frames(1);
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::BipedFootstep));
    let pts = [Vec3::new(-5.0, -20.0, 0.0), Vec3::new(5.0, -45.0, 0.0)];
    for p in pts {
        let at = h.screen_of(p);
        h.click(at);
        h.frames(1);
    }
    let f = structure(&h, root).footsteps;
    assert_eq!(f.len(), 2, "{}", h.app.state.status_prompt);
    for (fs, p) in f.iter().zip(pts) {
        assert!(fs.position.distance(p) < 1.5, "{} vs {p}", fs.position);
    }
    assert!(!f[0].left && f[1].left);
}

/// Save File and Load File on the Biped rollout go through the file dialog.
#[test]
fn save_and_load_file_write_and_read_a_figure() {
    let mut h = Harness::new();
    h.app.native_file_dialogs = false;
    let root = biped(&mut h);
    click_check(&mut h, "Knuckles");
    let path = exchange_dir("biped_fig").join("hand.fig");
    h.click_widget("Save File");
    h.frames(2);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::BipedSaveFig));
    choose_file(&mut h, &path, "Save");
    assert!(path.exists(), "the figure file is written ({})", h.app.state.status_prompt);
    click_check(&mut h, "Knuckles");
    assert!(!structure(&h, root).knuckles.get());
    h.click_widget("Load File");
    h.frames(2);
    choose_file(&mut h, &path, "Open");
    h.frames(2);
    assert!(structure(&h, root).knuckles.get(), "the loaded figure has Knuckles again");
    assert!(has_part(&h, "Bip001 L Finger43"));
}

#[test]
fn xtras_group_creates_selects_and_picks_a_parent() {
    let mut h = Harness::new();
    let root = biped(&mut h);
    h.click_widget("Create Xtra");
    h.frames(2);
    assert!(has_part(&h, "Bip001 Xtra01"));
    h.click_widget("Create Opposite Xtra");
    h.frames(2);
    assert!(has_part(&h, "Bip001 Xtra01Opp"));
    // Synch Selection with Select Symmetrical: a click in the list selects both tails.
    click_check(&mut h, "Synch Selection");
    click_check(&mut h, "Select Symmetrical");
    // The list row is the lowest "Xtra01" (the name field shows it too).
    let row = *h.widgets_labelled("Xtra01").last().expect("Xtra01 in the list");
    h.click(row.center());
    h.frames(2);
    let names: Vec<String> = h.app.state.selection.iter().map(|id| h.app.state.scene.node(*id).unwrap().name.clone()).collect();
    assert!(names.contains(&"Bip001 Xtra01".to_owned()) && names.contains(&"Bip001 Xtra01Opp".to_owned()), "{names:?}");
    // Back to the biped, the current tail stays Xtra01: Pick Parent, then a pick of the head.
    h.cmd(Command::SelectNodes { ids: vec![root.0], op: loom_commands::SelectOp::Replace });
    click_check(&mut h, "Synch Selection");
    h.frames(1);
    h.click_widget("Pick Parent");
    h.frames(1);
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::BipedXtraParent));
    let head = h.app.state.scene.nodes().find(|n| n.name == "Bip001 Head").unwrap().id;
    h.cmd(Command::PickHit { id: head.0 });
    let x = &structure(&h, root).xtras;
    assert_eq!(x.iter().find(|x| x.name == "Xtra01").unwrap().parent.as_deref(), Some("Head"));
    h.click_widget("Delete Xtra");
    h.frames(2);
    assert!(!has_part(&h, "Bip001 Xtra011") && !has_part(&h, "Bip001 Xtra01"));
}
