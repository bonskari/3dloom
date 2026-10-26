use super::*;
use loom_scene::{Node, ObjectKind};

fn character(h: &mut Harness) -> loom_scene::NodeId {
    fn bone(h: &mut Harness, name: &str, parent: Option<loom_scene::NodeId>) -> loom_scene::NodeId {
        let mut n = Node::new(name, ObjectKind::bone(10.0));
        n.parent = parent;
        n.transform.position.set(Vec3::new(0., 0., 10.));
        h.app.state.scene.add_node(n)
    }
    let root = bone(h, "root", None);
    let pelvis = bone(h, "pelvis", Some(root));
    let mut parent = pelvis;
    for i in 1..=5 {
        parent = bone(h, &format!("spine_{i:02}"), Some(parent));
    }
    bone(h, "head", Some(parent));
    for side in ["l", "r"] {
        let a = bone(h, &format!("upperarm_{side}"), Some(parent));
        let b = bone(h, &format!("lowerarm_{side}"), Some(a));
        bone(h, &format!("hand_{side}"), Some(b));
        let a = bone(h, &format!("thigh_{side}"), Some(pelvis));
        let b = bone(h, &format!("calf_{side}"), Some(a));
        bone(h, &format!("foot_{side}"), Some(b));
    }
    h.app.state.selection = [root].into_iter().collect();
    root
}

#[test]
fn motion_panel_builds_real_loomrig_and_selects_animatable_controls() {
    let mut h = Harness::new();
    h.screen.max.y = 1800.;
    let root = character(&mut h);
    h.app.clip_timeline.open=true;h.app.clip_timeline.tab=2;
    h.frames(3);
    assert!(h.has(egui::accesskit::Role::Button, "Create LoomRig"));
    h.click_text_anywhere("Create LoomRig");
    h.frames(3);
    assert_eq!(h.app.state.scene.loom_rigs.len(), 1);
    let rig = h.app.state.scene.loom_rigs[0].clone();
    assert_eq!(rig.mapping.root, root);
    h.click_text_anywhere("Select Master");
    h.frames(2);
    assert!(h.app.state.selection.contains(&rig.master));
    assert!(h.has(egui::accesskit::Role::Button, "Select Goal"));
    h.click_text_anywhere("Select Goal");
    h.frames(2);
    assert!(h.app.state.selection.contains(&rig.limbs[0].goal));
    assert!(h.has(egui::accesskit::Role::Label, "IK / FK"));
}

#[test]
fn motion_panel_applies_source_clip_to_existing_character() {
    let mut h = Harness::new();
    h.screen.max.y = 2000.;
    let root = character(&mut h);
    let path = std::env::temp_dir().join(format!("loom-retarget-ui-{}.bvh", std::process::id()));
    std::fs::write(&path,"HIERARCHY ROOT Hips { OFFSET 0 100 0 CHANNELS 6 Xposition Yposition Zposition Zrotation Yrotation Xrotation JOINT Head { OFFSET 0 20 0 CHANNELS 3 Zrotation Yrotation Xrotation End Site { OFFSET 0 5 0 } } } MOTION Frames: 2 Frame Time: 0.05 0 100 0 0 0 0 0 0 0 0 98 2 20 0 0 10 0 0").unwrap();
    h.cmd(Command::ImportAnimationClip {
        path: Some(path.to_string_lossy().into_owned()),
    });
    h.cmd(Command::CreateLoomRig {
        root: root.0,
        mapping: None,
    });
    h.app.clip_timeline.open=true;h.app.clip_timeline.tab=2;
    h.frames(3);
    assert!(h.has(egui::accesskit::Role::Button, "Apply Motion to Character"));
    h.click_text_anywhere("Apply Motion to Character");
    h.frames(3);
    assert_eq!(
        h.app.state.scene.animation_clips.len(),
        2,
        "source retained and new character clip added: {}",
        h.app.state.status_prompt
    );
    assert!(h.app.state.scene.animation_clips[1]
        .nodes
        .iter()
        .all(|id| *id == root || h.app.state.scene.ancestors(*id).contains(&root)));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn rig_timeline_moves_one_ik_fk_key_with_one_undo() {
    let mut h = Harness::new();
    let root = character(&mut h);
    h.cmd(Command::CreateLoomRig {
        root: root.0,
        mapping: None,
    });
    for (time, value) in [(0., 0.), (20., 1.)] {
        h.cmd(Command::SetLoomRigBlendKey {
            rig: 1,
            limb: "Left Arm".into(),
            time,
            value: Some(value),
        });
    }
    h.app.state.frame_range = (0, 100);
    h.frames(3);
    assert!(!h.has(egui::accesskit::Role::Button,"Left Arm IK/FK @ 20"));
    h.click_text_anywhere("Animation");h.click_text_anywhere("Animation Editor...");h.frames(3);
    let rect = h.find(egui::accesskit::Role::Button, "Left Arm IK/FK @ 20");
    h.drag(rect.center(), rect.center() + egui::vec2(75., 0.));
    let time = h.app.state.scene.loom_rigs[0].limbs[0]
        .blend
        .keys()
        .unwrap()
        .tracks[0]
        .keys[1]
        .time;
    assert!(time > 20.);
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(
        h.app.state.scene.loom_rigs[0].limbs[0]
            .blend
            .keys()
            .unwrap()
            .tracks[0]
            .keys[1]
            .time,
        20.
    );
}

#[test]
fn second_character_can_be_rigged_and_chain_rows_are_editable() {
    let mut h=Harness::new();h.screen.max.y=3000.;let first=character(&mut h);
    h.cmd(Command::CreateLoomRig{root:first.0,mapping:None});
    let second=character(&mut h);h.app.clip_timeline.open=true;h.app.clip_timeline.tab=2;h.frames(3);
    h.click_text_anywhere("Create Character Rig");h.frames(2);
    assert!(h.has(egui::accesskit::Role::Button,"Create LoomRig"));
    h.click_text_anywhere("Characterization");h.frames(2);
    assert!(h.has(egui::accesskit::Role::Button,"Add limb chain"));
    h.click_text_anywhere("Add limb chain");h.frames(2);
    assert_eq!(h.widgets.iter().filter(|(role,text,_)|*role==egui::accesskit::Role::Button&&text=="Remove chain").count(),5);
    let remove=h.widgets.iter().filter(|(role,text,_)|*role==egui::accesskit::Role::Button&&text=="Remove chain").last().unwrap().2;
    h.click(remove.center());h.frames(2);
    h.click_text_anywhere("Create LoomRig");h.frames(2);
    assert_eq!(h.app.state.scene.loom_rigs.len(),2);
    assert_eq!(h.app.state.scene.loom_rigs[1].mapping.root,second);
}
