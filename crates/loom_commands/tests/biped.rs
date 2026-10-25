//! Systems > Biped: names and hierarchy, Structure rebuilds, undo, Figure
//! Mode, skinning to biped parts, FBX and .loom round trips.

use glam::Vec3;
use loom_commands::{execute, AppState, Axis, Command, PrimitiveKind, SelectOp};
use loom_scene::{Modifier, NodeId, ObjectKind};
use serde_json::json;
use std::collections::BTreeMap;

/// A command from its registry name and JSON arguments (absent options are None).
fn cmd(name: &str, args: serde_json::Value) -> Command {
    serde_json::from_value(json!({ "cmd": name, "args": args })).unwrap()
}

fn create(s: &mut AppState) -> NodeId {
    let r = execute(s, cmd("Create Biped", json!({ "position": [0.0, 0.0, 0.0] }))).unwrap();
    NodeId(r["root"].as_u64().unwrap())
}

fn structure(s: &mut AppState, root: NodeId, f: impl FnOnce(&mut Command)) -> Result<serde_json::Value, String> {
    let mut c = cmd("Set Biped Structure", json!({ "id": root.0 }));
    f(&mut c);
    execute(s, c)
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

fn figure(s: &AppState, root: NodeId) -> bool {
    match &s.scene.node(root).unwrap().object {
        ObjectKind::Biped(b) => b.figure_mode,
        _ => panic!("not a biped"),
    }
}

#[test]
fn a_new_biped_has_the_reference_names_hierarchy_colours_and_height() {
    let mut s = AppState::new();
    let root = create(&mut s);
    let t = tree(&s, root);
    let parent = |n: &str| t.get(n).unwrap_or_else(|| panic!("no part {n}: {:?}", t.keys())).clone();
    assert_eq!(parent("Bip001"), None);
    assert!(matches!(s.scene.node(root).unwrap().object, ObjectKind::Biped(_)));
    for (child, p) in [
        ("Bip001 Pelvis", "Bip001"),
        ("Bip001 Spine", "Bip001 Pelvis"),
        ("Bip001 Spine1", "Bip001 Spine"),
        ("Bip001 Spine2", "Bip001 Spine1"),
        ("Bip001 Spine3", "Bip001 Spine2"),
        ("Bip001 Neck", "Bip001 Spine3"),
        ("Bip001 Head", "Bip001 Neck"),
        ("Bip001 HeadNub", "Bip001 Head"),
        ("Bip001 L Clavicle", "Bip001 Neck"),
        ("Bip001 L UpperArm", "Bip001 L Clavicle"),
        ("Bip001 L Forearm", "Bip001 L UpperArm"),
        ("Bip001 L Hand", "Bip001 L Forearm"),
        ("Bip001 L Finger0", "Bip001 L Hand"),
        ("Bip001 L Finger0Nub", "Bip001 L Finger0"),
        ("Bip001 L Thigh", "Bip001 Pelvis"),
        ("Bip001 L Calf", "Bip001 L Thigh"),
        ("Bip001 L Foot", "Bip001 L Calf"),
        ("Bip001 L Toe0", "Bip001 L Foot"),
        ("Bip001 L Toe01", "Bip001 L Toe0"),
        ("Bip001 L Toe02", "Bip001 L Toe01"),
        ("Bip001 L Toe0Nub", "Bip001 L Toe02"),
        ("Bip001 R Clavicle", "Bip001 Neck"),
        ("Bip001 R Hand", "Bip001 R Forearm"),
        ("Bip001 R Thigh", "Bip001 Pelvis"),
        ("Bip001 R Toe0Nub", "Bip001 R Toe02"),
    ] {
        assert_eq!(parent(child).as_deref(), Some(p), "{child}");
    }
    assert_eq!(t.len(), 35, "the centre of mass and 34 parts: {:?}", t.keys());
    // Only the centre of mass is selected, Figure Mode is on.
    assert_eq!(s.selection.iter().copied().collect::<Vec<_>>(), vec![root]);
    assert!(figure(&s, root));
    // Parts are bones: left side blue, right side green, the spine yellow.
    let n = |name: &str| s.scene.node(id_of(&s, name)).unwrap().clone();
    assert!(n("Bip001 L UpperArm").object.is_bone());
    assert_eq!(n("Bip001 L Thigh").wire_color, loom_scene::biped::LEFT_COLOR);
    assert_eq!(n("Bip001 R Thigh").wire_color, loom_scene::biped::RIGHT_COLOR);
    assert_eq!(n("Bip001 Spine").wire_color, loom_scene::biped::CENTRE_COLOR);
    // The left side is +X, the figure faces -Y.
    assert!(pos(&s, "Bip001 L Hand").x > 10.0 && pos(&s, "Bip001 R Hand").x < -10.0);
    assert!(pos(&s, "Bip001 L Toe0").y < pos(&s, "Bip001 L Foot").y);
    // 1.8 m in the default inch units, feet on the ground at the creation point.
    let h = 1.8 / 0.0254;
    let top = pos(&s, "Bip001 HeadNub").z;
    assert!((top - h).abs() < 0.01 * h, "head top at {top}, height {h}");
    let (lo, _) = s.scene.eval_mesh(id_of(&s, "Bip001 L Foot")).value.mesh.aabb();
    let foot_bottom = s.scene.object_matrix(id_of(&s, "Bip001 L Foot")).transform_point3(lo).z;
    assert!(foot_bottom.abs() < 0.01 * h, "foot on the ground: {foot_bottom}");
    // A second biped is Bip002.
    let r = execute(&mut s, cmd("Create Biped", json!({ "position": [100.0, 0.0, 0.0], "height": 50.0 }))).unwrap();
    assert_eq!(r["name"], "Bip002");
    assert!((pos(&s, "Bip002 HeadNub").z - 50.0).abs() < 0.5);
}

#[test]
fn structure_changes_rebuild_the_parts_and_keep_the_names() {
    let mut s = AppState::new();
    let root = create(&mut s);
    let pelvis = id_of(&s, "Bip001 Pelvis");
    let hand = id_of(&s, "Bip001 L Hand");
    structure(&mut s, root, |c| if let Command::SetBipedStructure { spine_links, .. } = c { *spine_links = Some(2) }).unwrap();
    let t = tree(&s, root);
    assert!(t.contains_key("Bip001 Spine1") && !t.contains_key("Bip001 Spine2") && !t.contains_key("Bip001 Spine3"));
    assert_eq!(t["Bip001 Neck"].as_deref(), Some("Bip001 Spine1"), "the neck moves onto the top link");
    assert_eq!(id_of(&s, "Bip001 Pelvis"), pelvis, "kept parts keep their nodes");
    assert_eq!(id_of(&s, "Bip001 L Hand"), hand);
    // The spine still reaches the neck.
    let neck = pos(&s, "Bip001 Neck");
    let spine1 = s.scene.world_matrix(id_of(&s, "Bip001 Spine1"));
    let tip = spine1.transform_point3(s.scene.node(id_of(&s, "Bip001 Spine1")).unwrap().object.bone_tip_local().unwrap());
    assert!(tip.distance(neck) < 0.01, "{tip} vs {neck}");
    // Fingers, finger links, toes, tail, horse link, neck through SetParameter.
    structure(&mut s, root, |c| {
        if let Command::SetBipedStructure { fingers, finger_links, toes, toe_links, tail_links, leg_links, .. } = c {
            *fingers = Some(5);
            *finger_links = Some(3);
            *toes = Some(2);
            *toe_links = Some(1);
            *tail_links = Some(3);
            *leg_links = Some(4);
        }
    })
    .unwrap();
    execute(&mut s, Command::SetParameter { id: root.0, name: "Neck Links".into(), value: 2.0 }).unwrap();
    let t = tree(&s, root);
    for n in ["Bip001 L Finger4", "Bip001 L Finger41", "Bip001 L Finger42", "Bip001 L Finger4Nub", "Bip001 R Finger02", "Bip001 L Toe1", "Bip001 L Toe1Nub", "Bip001 Tail", "Bip001 Tail2", "Bip001 TailNub", "Bip001 L HorseLink", "Bip001 Neck1"] {
        assert!(t.contains_key(n), "{n} in {:?}", t.keys());
    }
    assert!(!t.contains_key("Bip001 L Toe01"), "one toe link now");
    assert_eq!(t["Bip001 L Foot"].as_deref(), Some("Bip001 L HorseLink"));
    assert_eq!(t["Bip001 Head"].as_deref(), Some("Bip001 Neck1"));
    assert_eq!(t["Bip001 L Clavicle"].as_deref(), Some("Bip001 Neck"), "the clavicles stay on the first neck link");
    // centre 1 + 2 spine + 2 neck + head + nub + tail 3 + nub; per side arm 4 + 5×4 fingers, leg 4 + 2×2 toes.
    assert_eq!(t.len(), 1 + (1 + 2 + 2 + 2 + 4) + 2 * (4 + 20 + 4 + 4));
    // Arms off and on again.
    structure(&mut s, root, |c| if let Command::SetBipedStructure { arms, .. } = c { *arms = Some(false) }).unwrap();
    assert!(!tree(&s, root).keys().any(|n| n.contains("Clavicle") || n.contains("Finger")));
    structure(&mut s, root, |c| if let Command::SetBipedStructure { arms, .. } = c { *arms = Some(true) }).unwrap();
    assert!(tree(&s, root).contains_key("Bip001 R Finger4Nub"));
    // Ranges clamp.
    structure(&mut s, root, |c| if let Command::SetBipedStructure { spine_links, .. } = c { *spine_links = Some(99) }).unwrap();
    assert!(tree(&s, root).contains_key("Bip001 Spine9") && !tree(&s, root).contains_key("Bip001 Spine10"));
    // Outside Figure Mode the structure is locked.
    execute(&mut s, Command::BipedFigureMode { on: Some(false) }).unwrap();
    assert!(structure(&mut s, root, |c| if let Command::SetBipedStructure { spine_links, .. } = c { *spine_links = Some(3) }).is_err());
}

#[test]
fn rebuilds_and_creation_undo_and_redo() {
    let mut s = AppState::new();
    let root = create(&mut s);
    let before = tree(&s, root);
    let thigh_pos = pos(&s, "Bip001 L Thigh");
    structure(&mut s, root, |c| {
        if let Command::SetBipedStructure { spine_links, fingers, height, .. } = c {
            *spine_links = Some(1);
            *fingers = Some(3);
            *height = Some(100.0);
        }
    })
    .unwrap();
    let after = tree(&s, root);
    assert_ne!(before, after);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(tree(&s, root), before, "one undo brings the whole structure back");
    assert!(pos(&s, "Bip001 L Thigh").distance(thigh_pos) < 1e-3);
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert_eq!(tree(&s, root), after);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.len(), 0, "undoing the creation removes every part");
}

#[test]
fn height_scales_the_pose_about_the_feet() {
    let mut s = AppState::new();
    let root = create(&mut s);
    let arm = id_of(&s, "Bip001 L UpperArm");
    execute(&mut s, Command::SelectNodes { ids: vec![arm.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: Axis::Y, degrees: 40.0, system: None }).unwrap();
    let hand = pos(&s, "Bip001 L Hand");
    let h0 = 1.8 / 0.0254;
    structure(&mut s, root, |c| if let Command::SetBipedStructure { height, .. } = c { *height = Some(h0 * 2.0) }).unwrap();
    let top = pos(&s, "Bip001 HeadNub").z;
    assert!((top - 2.0 * h0).abs() < 0.02 * h0, "{top}");
    // The bent arm keeps its pose, twice as big.
    let scaled = pos(&s, "Bip001 L Hand");
    assert!((scaled - hand * 2.0).length() < 0.02 * h0, "{scaled} vs {}", hand * 2.0);
    let (lo, _) = s.scene.eval_mesh(id_of(&s, "Bip001 L Foot")).value.mesh.aabb();
    let foot_bottom = s.scene.object_matrix(id_of(&s, "Bip001 L Foot")).transform_point3(lo).z;
    assert!(foot_bottom.abs() < 0.02 * h0, "the feet stay on the ground: {foot_bottom}");
}

#[test]
fn figure_mode_keeps_the_figure_pose_apart_from_the_animated_pose() {
    let mut s = AppState::new();
    let root = create(&mut s);
    let fore = id_of(&s, "Bip001 L Forearm");
    let rot = |s: &AppState| s.scene.node(fore).unwrap().transform.rotation.get();
    // The figure: the forearm bent 30°.
    execute(&mut s, Command::SelectNodes { ids: vec![fore.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: Axis::Z, degrees: 30.0, system: None }).unwrap();
    let figure_rot = rot(&s);
    execute(&mut s, Command::BipedFigureMode { on: None }).unwrap();
    assert!(!figure(&s, root));
    assert_eq!(rot(&s), figure_rot, "with no animation the biped stays in its figure pose");
    // The animated pose: bent further.
    execute(&mut s, Command::RotateSelection { axis: Axis::Z, degrees: 45.0, system: None }).unwrap();
    let anim_rot = rot(&s);
    assert_ne!(anim_rot, figure_rot);
    execute(&mut s, Command::BipedFigureMode { on: Some(true) }).unwrap();
    assert!(figure(&s, root));
    assert!((rot(&s) - figure_rot).length() < 1e-3, "Figure Mode shows the figure pose");
    execute(&mut s, Command::BipedFigureMode { on: Some(false) }).unwrap();
    assert!((rot(&s) - anim_rot).length() < 1e-3, "and leaving it brings the animation back");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(figure(&s, root) && (rot(&s) - figure_rot).length() < 1e-3);
}

#[test]
fn root_name_renames_the_whole_biped() {
    let mut s = AppState::new();
    let root = create(&mut s);
    let hand = id_of(&s, "Bip001 L Hand");
    execute(&mut s, Command::BipedRootName { id: hand.0, name: "Hero".into() }).unwrap();
    let t = tree(&s, root);
    assert!(t.contains_key("Hero") && t.contains_key("Hero L Hand") && t.contains_key("Hero Spine3"));
    assert!(!t.keys().any(|n| n.starts_with("Bip001")));
    // The renamed biped still rebuilds.
    structure(&mut s, root, |c| if let Command::SetBipedStructure { neck_links, .. } = c { *neck_links = Some(2) }).unwrap();
    assert!(tree(&s, root).contains_key("Hero Neck1"));
}

/// A box around the left forearm, skinned to the left arm's parts.
fn skinned_forearm(s: &mut AppState) -> (NodeId, NodeId) {
    create(s);
    let elbow = pos(s, "Bip001 L Forearm");
    let wrist = pos(s, "Bip001 L Hand");
    let mid = (elbow + wrist) * 0.5;
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [mid.x, mid.y, mid.z - 1.5], size: [(wrist - elbow).length() * 0.9, 3.0, 3.0] }).unwrap();
    let mesh = *s.selection.iter().next().unwrap();
    execute(s, Command::SetParameter { id: mesh.0, name: "Width Segs".into(), value: 6.0 }).unwrap();
    execute(s, Command::SkinModifier).unwrap();
    let ids = vec![id_of(s, "Bip001 L UpperArm").0, id_of(s, "Bip001 L Forearm").0];
    execute(s, Command::SkinAddBones { ids }).unwrap();
    (mesh, id_of(s, "Bip001 L UpperArm"))
}

#[test]
fn skin_binds_to_biped_parts() {
    let mut s = AppState::new();
    let (mesh, upper) = skinned_forearm(&mut s);
    let Modifier::Skin(d) = &s.scene.node(mesh).unwrap().modifiers[0].modifier else { panic!("no Skin") };
    assert_eq!(d.bones.len(), 2);
    let rest = s.scene.eval_mesh(mesh).value.mesh.clone();
    execute(&mut s, Command::SelectNodes { ids: vec![upper.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: Axis::Y, degrees: 60.0, system: None }).unwrap();
    let bent = s.scene.eval_mesh(mesh).value.mesh.clone();
    let moved = rest.vertices().iter().zip(bent.vertices()).filter(|(a, b)| a.position.distance(b.position) > 1.0).count();
    assert!(moved * 2 > rest.vertex_count(), "{moved} of {} vertices follow the arm", rest.vertex_count());
}

#[test]
fn fbx_export_writes_the_biped_as_a_skeleton_and_reads_back() {
    let d = std::env::temp_dir().join("loom-biped-tests");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let mut s = AppState::new();
    let (mesh, _) = skinned_forearm(&mut s);
    let root = id_of(&s, "Bip001");
    let names = tree(&s, root);
    // Export Selected on the skinned mesh brings the skeleton up to the root.
    execute(&mut s, Command::SelectNodes { ids: vec![mesh.0], op: SelectOp::Replace }).unwrap();
    let path = d.join("biped.fbx");
    execute(&mut s, Command::ExportSelected { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let doc = loom_fbx::read(&std::fs::read(&path).unwrap(), 0.0254).unwrap();
    let bones: Vec<&loom_fbx::FbxNode> = doc.nodes.iter().filter(|n| matches!(n.kind, loom_fbx::FbxKind::Bone { .. })).collect();
    assert!(bones.iter().any(|b| b.name == "Bip001" && b.parent.is_none()), "the centre of mass is the root joint");
    for b in &bones {
        if let Some(p) = b.parent {
            assert_eq!(names.get(&b.name).cloned().flatten().as_deref(), Some(doc.nodes[p].name.as_str()), "{} keeps its parent", b.name);
        }
    }
    assert!(bones.iter().any(|b| b.name == "Bip001 L Forearm"));
    // The whole file back into a scene: the same names, parents and world positions.
    let mut t = AppState::new();
    execute(&mut t, Command::ImportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let back = tree(&t, id_of(&t, "Bip001"));
    for (n, p) in &back {
        assert_eq!(names.get(n), Some(p), "{n}");
    }
    for n in ["Bip001 Pelvis", "Bip001 L UpperArm", "Bip001 L Forearm"] {
        assert!(pos(&t, n).distance(pos(&s, n)) < 0.01, "{n}: {} vs {}", pos(&t, n), pos(&s, n));
    }
    let m = t.scene.nodes().find(|n| matches!(n.object, ObjectKind::EditablePoly(_))).unwrap();
    assert!(m.modifiers.iter().any(|x| matches!(x.modifier, Modifier::Skin(_))), "the skin comes back");
}

#[test]
fn a_biped_round_trips_through_loom_files() {
    use loom_commands::file::{read, write, Document, SceneSettings, FORMAT};
    let mut s = AppState::new();
    let root = create(&mut s);
    structure(&mut s, root, |c| if let Command::SetBipedStructure { tail_links, .. } = c { *tail_links = Some(2) }).unwrap();
    let path = std::env::temp_dir().join("loom-biped-tests-loom").join("biped.loom");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write(&path, &Document { format: FORMAT, app: "test".into(), scene: s.scene.clone(), settings: SceneSettings::from_state(&s) }).unwrap();
    let doc = read(&path).unwrap();
    for n in s.scene.nodes() {
        assert_eq!(Some(n), doc.scene.node(n.id), "{}", n.name);
    }
    let ObjectKind::Biped(b) = &doc.scene.node(root).unwrap().object else { panic!("the root is a biped") };
    assert_eq!(b.tail_links.get(), 2);
    assert!(b.figure_mode);
}
