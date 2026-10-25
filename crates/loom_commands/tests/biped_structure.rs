//! Biped Structure rollout additions: Body Type, ponytails, props, Ankle
//! Attach, Triangle Pelvis / Neck, ForeFeet / Knuckles / Short Thumb, twist
//! links and Xtras.

use glam::Vec3;
use loom_commands::{execute, AppState, Command};
use loom_scene::biped::BodyType;
use loom_scene::{NodeId, ObjectKind};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn cmd(name: &str, args: Value) -> Command {
    serde_json::from_value(json!({ "cmd": name, "args": args })).unwrap()
}

fn create(s: &mut AppState) -> NodeId {
    let r = execute(s, cmd("Create Biped", json!({ "position": [0.0, 0.0, 0.0] }))).unwrap();
    NodeId(r["root"].as_u64().unwrap())
}

fn set(s: &mut AppState, root: NodeId, args: Value) -> Result<Value, String> {
    let mut a = args;
    a["id"] = json!(root.0);
    execute(s, cmd("Set Biped Structure", a))
}

/// Every node of the biped by name, with its parent's name.
fn tree(s: &AppState, root: NodeId) -> BTreeMap<String, Option<String>> {
    let mut ids = vec![root];
    ids.extend(s.scene.descendants(root));
    ids.into_iter()
        .map(|id| {
            let n = s.scene.node(id).unwrap();
            (n.name.clone(), n.parent.and_then(|p| s.scene.node(p)).map(|p| p.name.clone()))
        })
        .collect()
}

fn id_of(s: &AppState, name: &str) -> NodeId {
    s.scene.nodes().find(|n| n.name == name).unwrap_or_else(|| panic!("no node {name}")).id
}

fn pos(s: &AppState, name: &str) -> Vec3 {
    s.scene.world_matrix(id_of(s, name)).transform_point3(Vec3::ZERO)
}

fn param(s: &AppState, name: &str, p: &str) -> f32 {
    s.scene.node(id_of(s, name)).unwrap().object.parameters().into_iter().find(|(n, _)| *n == p).unwrap().1.as_f64() as f32
}

fn structure(s: &AppState, root: NodeId) -> loom_scene::biped::BipedStructure {
    match &s.scene.node(root).unwrap().object {
        ObjectKind::Biped(b) => (**b).clone(),
        _ => panic!("not a biped"),
    }
}

/// World bounding box of a part's shape.
fn bounds(s: &AppState, name: &str) -> (Vec3, Vec3) {
    let id = id_of(s, name);
    let mesh = s.scene.eval_mesh(id).value.mesh.clone();
    let m = s.scene.object_matrix(id);
    let pts: Vec<Vec3> = mesh.vertices().iter().map(|v| m.transform_point3(v.position)).collect();
    pts.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), p| (lo.min(*p), hi.max(*p)))
}

#[test]
fn body_types_change_the_shapes_but_not_the_names() {
    let mut s = AppState::new();
    let root = create(&mut s);
    assert_eq!(structure(&s, root).body_type, BodyType::Skeleton, "a new biped is a skeleton");
    let names = tree(&s, root);
    let mut widths = BTreeMap::new();
    let mut shoulders = BTreeMap::new();
    let mut pelvis = BTreeMap::new();
    for b in ["Classic", "Male", "Female", "Skeleton"] {
        set(&mut s, root, json!({ "body_type": b })).unwrap();
        assert_eq!(tree(&s, root), names, "{b}: the same parts");
        widths.insert(b, param(&s, "Bip001 L Thigh", "Width"));
        shoulders.insert(b, pos(&s, "Bip001 L UpperArm").x - pos(&s, "Bip001 R UpperArm").x);
        pelvis.insert(b, param(&s, "Bip001 Pelvis", "Width"));
        let foot_bottom = bounds(&s, "Bip001 L Foot").0.z;
        assert!(foot_bottom.abs() < 0.5, "{b}: the foot stands on the ground ({foot_bottom})");
    }
    assert!(widths["Skeleton"] < widths["Classic"] * 0.6, "skeleton bones are thin: {widths:?}");
    assert!(param(&s, "Bip001 L Thigh", "Taper") > 0.1, "and tapered");
    assert!(shoulders["Male"] > shoulders["Female"], "male shoulders are broader: {shoulders:?}");
    assert!(pelvis["Female"] > pelvis["Male"], "female hips are wider: {pelvis:?}");
    // Undo brings the previous body type back in one step.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(structure(&s, root).body_type, BodyType::Female);
    assert!(set(&mut s, root, json!({ "body_type": "Robot" })).is_err());
}

#[test]
fn ponytails_hang_from_the_head() {
    let mut s = AppState::new();
    let root = create(&mut s);
    set(&mut s, root, json!({ "ponytail1_links": 3, "ponytail2_links": 1 })).unwrap();
    let t = tree(&s, root);
    for (child, parent) in [("Bip001 Ponytail1", "Bip001 Head"), ("Bip001 Ponytail11", "Bip001 Ponytail1"), ("Bip001 Ponytail12", "Bip001 Ponytail11"), ("Bip001 Ponytail1Nub", "Bip001 Ponytail12"), ("Bip001 Ponytail2", "Bip001 Head"), ("Bip001 Ponytail2Nub", "Bip001 Ponytail2")] {
        assert_eq!(t.get(child).cloned().flatten().as_deref(), Some(parent), "{child} in {:?}", t.keys());
    }
    assert!(pos(&s, "Bip001 Ponytail1").y > pos(&s, "Bip001 Head").y, "at the back of the head");
    set(&mut s, root, json!({ "ponytail1_links": 99 })).unwrap();
    assert!(tree(&s, root).contains_key("Bip001 Ponytail124") && !tree(&s, root).contains_key("Bip001 Ponytail125"), "25 links at most");
    set(&mut s, root, json!({ "ponytail1_links": 0, "ponytail2_links": 0 })).unwrap();
    assert!(!tree(&s, root).keys().any(|n| n.contains("Ponytail")));
}

#[test]
fn props_appear_by_the_hands_and_in_front_of_the_torso() {
    let mut s = AppState::new();
    let root = create(&mut s);
    set(&mut s, root, json!({ "prop1": true, "prop2": true, "prop3": true })).unwrap();
    let t = tree(&s, root);
    assert_eq!(t["Bip001 Prop1"].as_deref(), Some("Bip001 R Hand"));
    assert_eq!(t["Bip001 Prop2"].as_deref(), Some("Bip001 L Hand"));
    assert_eq!(t["Bip001 Prop3"].as_deref(), Some("Bip001"));
    assert!(pos(&s, "Bip001 Prop1").distance(pos(&s, "Bip001 R Hand")) < 15.0 && pos(&s, "Bip001 Prop1").x < 0.0, "prop 1 by the right hand");
    assert!(pos(&s, "Bip001 Prop2").x > 0.0);
    let p3 = pos(&s, "Bip001 Prop3");
    assert!(p3.x.abs() < 0.01 && p3.y < pos(&s, "Bip001 Spine").y - 5.0, "prop 3 centred in front of the torso: {p3}");
    set(&mut s, root, json!({ "prop2": false })).unwrap();
    assert!(!tree(&s, root).contains_key("Bip001 Prop2"));
}

#[test]
fn ankle_attach_slides_the_foot_block_under_the_ankle() {
    let mut s = AppState::new();
    let root = create(&mut s);
    assert!((structure(&s, root).ankle_attach.get() - 0.2).abs() < 1e-6);
    let ankle = pos(&s, "Bip001 L Foot");
    let (lo, hi) = bounds(&s, "Bip001 L Foot");
    // The figure faces -Y: the heel is the block's +Y end.
    let heel = (hi.y - ankle.y) / (hi.y - lo.y);
    assert!((heel - 0.2).abs() < 0.02, "the ankle is a fifth of the way from the heel: {heel}");
    set(&mut s, root, json!({ "ankle_attach": 0.5 })).unwrap();
    assert!(pos(&s, "Bip001 L Foot").distance(ankle) < 1e-3, "the ankle stays");
    let (lo, hi) = bounds(&s, "Bip001 L Foot");
    assert!(((hi.y - ankle.y) / (hi.y - lo.y) - 0.5).abs() < 0.02);
    assert!(pos(&s, "Bip001 L Toe0").y > lo.y - 0.5, "the toes move with the block");
    set(&mut s, root, json!({ "ankle_attach": 3.0 })).unwrap();
    assert_eq!(structure(&s, root).ankle_attach.get(), 1.0, "clamped");
}

#[test]
fn triangle_pelvis_and_neck_reparent_the_thighs_and_clavicles() {
    let mut s = AppState::new();
    let root = create(&mut s);
    let thigh = pos(&s, "Bip001 L Thigh");
    set(&mut s, root, json!({ "triangle_pelvis": true, "triangle_neck": true })).unwrap();
    let t = tree(&s, root);
    assert_eq!(t["Bip001 L Thigh"].as_deref(), Some("Bip001 Spine"));
    assert_eq!(t["Bip001 R Thigh"].as_deref(), Some("Bip001 Spine"));
    assert_eq!(t["Bip001 L Clavicle"].as_deref(), Some("Bip001 Spine3"));
    assert!(pos(&s, "Bip001 L Thigh").distance(thigh) < 1e-3, "the thigh does not move");
    set(&mut s, root, json!({ "triangle_pelvis": false, "triangle_neck": false })).unwrap();
    let t = tree(&s, root);
    assert_eq!(t["Bip001 L Thigh"].as_deref(), Some("Bip001 Pelvis"));
    assert_eq!(t["Bip001 L Clavicle"].as_deref(), Some("Bip001 Neck"));
}

#[test]
fn knuckles_makes_an_anatomical_hand_and_excludes_forefeet() {
    let mut s = AppState::new();
    let root = create(&mut s);
    let classic_hand = param(&s, "Bip001 L Hand", "Length");
    set(&mut s, root, json!({ "knuckles": true })).unwrap();
    let b = structure(&s, root);
    assert_eq!((b.fingers.get(), b.finger_links.get(), b.short_thumb.get()), (5, 4, true), "Knuckles sets five fingers of four links and Short Thumb");
    let t = tree(&s, root);
    // The thumb has three links (Short Thumb), the other digits four.
    for n in ["Bip001 L Finger0", "Bip001 L Finger01", "Bip001 L Finger02", "Bip001 L Finger0Nub", "Bip001 L Finger1", "Bip001 L Finger13", "Bip001 L Finger4Nub", "Bip001 R Finger43"] {
        assert!(t.contains_key(n), "{n}");
    }
    assert!(!t.contains_key("Bip001 L Finger03"));
    assert!(param(&s, "Bip001 L Hand", "Length") < classic_hand * 0.5, "a small hand base");
    // Digits start at the hand base, not at its end.
    assert!(pos(&s, "Bip001 L Finger1").x < pos(&s, "Bip001 L Hand").x + classic_hand * 0.5);
    set(&mut s, root, json!({ "short_thumb": false })).unwrap();
    assert!(tree(&s, root).contains_key("Bip001 L Finger03"), "without Short Thumb the thumb has four links");
    assert!(set(&mut s, root, json!({ "forefeet": true })).is_err(), "ForeFeet is disabled while Knuckles is on");
    set(&mut s, root, json!({ "knuckles": false, "forefeet": true })).unwrap();
    assert!(structure(&s, root).forefeet.get());
    assert!(set(&mut s, root, json!({ "knuckles": true })).is_err(), "Knuckles is disabled while ForeFeet is on");
}

#[test]
fn twist_links_are_frozen_links_along_the_limbs() {
    let mut s = AppState::new();
    let root = create(&mut s);
    set(&mut s, root, json!({ "forearm_twists": 3 })).unwrap();
    assert!(!tree(&s, root).keys().any(|n| n.contains("Twist")), "nothing until Twists is on");
    set(&mut s, root, json!({ "twists": true, "upper_arm_twists": 1, "thigh_twists": 2, "calf_twists": 1, "horse_link_twists": 2 })).unwrap();
    let t = tree(&s, root);
    for (child, parent) in [
        ("Bip001 L ForeTwist", "Bip001 L Forearm"),
        ("Bip001 L ForeTwist1", "Bip001 L ForeTwist"),
        ("Bip001 L ForeTwist2", "Bip001 L ForeTwist1"),
        ("Bip001 R UpperArmTwist", "Bip001 R UpperArm"),
        ("Bip001 L ThighTwist1", "Bip001 L ThighTwist"),
        ("Bip001 R CalfTwist", "Bip001 R Calf"),
    ] {
        assert_eq!(t.get(child).cloned().flatten().as_deref(), Some(parent), "{child}");
    }
    assert!(!t.keys().any(|n| n.contains("HorseTwist")), "Horse Link needs four leg links");
    let twist = s.scene.node(id_of(&s, "Bip001 L ForeTwist1")).unwrap();
    assert!(twist.frozen, "twist links are created frozen");
    assert!(!s.scene.node(id_of(&s, "Bip001 L Forearm")).unwrap().frozen);
    // Along the forearm, inside it.
    let (elbow, wrist) = (pos(&s, "Bip001 L Forearm"), pos(&s, "Bip001 L Hand"));
    let p = pos(&s, "Bip001 L ForeTwist1");
    assert!(p.distance(elbow.lerp(wrist, 1.0 / 3.0)) < 0.1, "{p} between {elbow} and {wrist}");
    assert!(param(&s, "Bip001 L ForeTwist", "Width") < param(&s, "Bip001 L Forearm", "Width"));
    set(&mut s, root, json!({ "leg_links": 4 })).unwrap();
    let t = tree(&s, root);
    assert_eq!(t["Bip001 L HorseTwist"].as_deref(), Some("Bip001 L HorseLink"));
    assert!(t.contains_key("Bip001 L HorseTwist1"));
    set(&mut s, root, json!({ "twists": false })).unwrap();
    assert!(!tree(&s, root).keys().any(|n| n.contains("Twist")));
}

#[test]
fn xtras_create_mirror_reparent_rename_and_delete() {
    let mut s = AppState::new();
    let root = create(&mut s);
    let x = |s: &mut AppState, name: &str, args: Value| {
        let mut a = args;
        a["id"] = json!(root.0);
        execute(s, cmd(name, a))
    };
    let r = x(&mut s, "Biped Create Xtra", json!({ "links": 2 })).unwrap();
    assert_eq!(r["name"], "Xtra01");
    let t = tree(&s, root);
    assert_eq!(t["Bip001 Xtra01"].as_deref(), Some("Bip001"), "on the centre of mass");
    assert_eq!(t["Bip001 Xtra011"].as_deref(), Some("Bip001 Xtra01"));
    assert_eq!(t["Bip001 Xtra01Nub"].as_deref(), Some("Bip001 Xtra011"));
    // The opposite tail mirrors across the biped.
    let r = x(&mut s, "Biped Create Opposite Xtra", json!({ "name": "Xtra01" })).unwrap();
    assert_eq!(r["name"], "Xtra01Opp");
    let (a, b) = (pos(&s, "Bip001 Xtra01"), pos(&s, "Bip001 Xtra01Opp"));
    assert!((a.x + b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3 && a.x.abs() > 1.0, "{a} mirrors {b}");
    assert!(x(&mut s, "Biped Create Opposite Xtra", json!({ "name": "Xtra01" })).is_err(), "only one opposite");
    // Links and Pick Parent with Reorient: the tail moves to the end of the head.
    x(&mut s, "Biped Set Xtra", json!({ "name": "Xtra01", "links": 3, "parent": id_of(&s, "Bip001 Head").0 })).unwrap();
    let t = tree(&s, root);
    assert_eq!(t["Bip001 Xtra01"].as_deref(), Some("Bip001 Head"));
    assert!(t.contains_key("Bip001 Xtra012"));
    assert!(pos(&s, "Bip001 Xtra01").distance(pos(&s, "Bip001 HeadNub")) < 0.5, "reoriented to the head's end");
    // Without Reorient the tail keeps its place.
    let before = pos(&s, "Bip001 Xtra01Opp");
    x(&mut s, "Biped Set Xtra", json!({ "name": "Xtra01Opp", "parent": id_of(&s, "Bip001 Xtra011").0, "reorient": false })).unwrap();
    assert_eq!(tree(&s, root)["Bip001 Xtra01Opp"].as_deref(), Some("Bip001 Xtra011"));
    assert!(pos(&s, "Bip001 Xtra01Opp").distance(before) < 1e-3);
    // Renaming keeps the nodes and the child's parent link.
    let link = id_of(&s, "Bip001 Xtra011");
    x(&mut s, "Biped Set Xtra", json!({ "name": "Xtra01", "new_name": "Ear" })).unwrap();
    assert_eq!(id_of(&s, "Bip001 Ear1"), link, "the same node under the new name");
    assert_eq!(tree(&s, root)["Bip001 Xtra01Opp"].as_deref(), Some("Bip001 Ear1"));
    assert!(x(&mut s, "Biped Set Xtra", json!({ "name": "Ear", "new_name": "Head" })).is_err(), "names of other parts are taken");
    // Deleting the parent tail moves the child onto the centre of mass where it is.
    let before = pos(&s, "Bip001 Xtra01Opp");
    x(&mut s, "Biped Delete Xtra", json!({ "name": "Ear" })).unwrap();
    let t = tree(&s, root);
    assert!(!t.keys().any(|n| n.contains("Ear")));
    assert_eq!(t["Bip001 Xtra01Opp"].as_deref(), Some("Bip001"));
    assert!(pos(&s, "Bip001 Xtra01Opp").distance(before) < 1e-3, "not repositioned");
    assert_eq!(structure(&s, root).xtras[0].opposite, None);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(tree(&s, root).contains_key("Bip001 Ear"), "undo brings the tail back");
}

#[test]
fn pick_parent_takes_the_clicked_part() {
    let mut s = AppState::new();
    let root = create(&mut s);
    execute(&mut s, cmd("Biped Create Xtra", json!({ "id": root.0 }))).unwrap();
    execute(&mut s, cmd("Biped Pick Xtra Parent", json!({ "name": "Xtra01" }))).unwrap();
    assert_eq!(s.pick_mode, Some(loom_commands::PickMode::BipedXtraParent));
    execute(&mut s, Command::PickHit { id: id_of(&s, "Bip001 L Hand").0 }).unwrap();
    assert_eq!(s.pick_mode, None);
    assert_eq!(tree(&s, root)["Bip001 Xtra01"].as_deref(), Some("Bip001 L Hand"));
}

#[test]
fn create_biped_takes_every_structure_value() {
    let mut s = AppState::new();
    let mut b = loom_scene::biped::BipedStructure::new(1.0);
    b.set("Body Type", 2.0).unwrap();
    b.set("Knuckles", 1.0).unwrap();
    b.set("Prop3", 1.0).unwrap();
    b.set("Ponytail2 Links", 2.0).unwrap();
    b.set("Twists", 1.0).unwrap();
    b.set("Thigh", 2.0).unwrap();
    let r = execute(&mut s, loom_commands::biped::create_command(Vec3::ZERO, Some(70.0), None, &b)).unwrap();
    let root = NodeId(r["root"].as_u64().unwrap());
    let got = structure(&s, root);
    assert_eq!(got.body_type, BodyType::Female);
    assert!(got.knuckles.get() && got.prop3.get() && got.twists.get());
    assert_eq!((got.ponytail2_links.get(), got.thigh_twists.get(), got.fingers.get()), (2, 2, 5));
    assert!((got.height.get() - 70.0).abs() < 1e-4);
    assert!(tree(&s, root).contains_key("Bip001 L ThighTwist1"));
}
