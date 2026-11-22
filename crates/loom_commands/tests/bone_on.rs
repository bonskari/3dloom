//! Bone behaviour outside Bone Edit Mode (Bone Tools > Object Properties):
//! a moved child turns its Bone On parent (Auto-Align) and stretches it
//! (Freeze Length off: Scale / Squash), for bones and for any object.

use glam::Vec3;
use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp};
use loom_scene::{NodeId, ObjectKind};

fn chain(s: &mut AppState) -> Vec<NodeId> {
    let r = execute(s, Command::CreateBoneChain { points: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 10.0, 0.0]], name: None }).unwrap();
    serde_json::from_value::<Vec<u64>>(r["nodes"].clone()).unwrap().into_iter().map(NodeId).collect()
}

fn pos(s: &AppState, id: NodeId) -> Vec3 {
    s.scene.world_matrix(id).transform_point3(Vec3::ZERO)
}

fn length(s: &AppState, id: NodeId) -> f32 {
    s.scene.node(id).unwrap().object.bone_length().unwrap()
}

fn param(s: &AppState, id: NodeId, name: &str) -> f64 {
    s.scene.node(id).unwrap().object.parameters().into_iter().find(|(n, _)| *n == name).unwrap().1.as_f64()
}

fn set(s: &mut AppState, id: NodeId, name: &str, value: f64) {
    execute(s, Command::SetParameter { id: id.0, name: name.into(), value }).unwrap();
}

fn move_node(s: &mut AppState, id: NodeId, delta: [f32; 3]) {
    execute(s, Command::SelectNodes { ids: vec![id.0], op: SelectOp::Replace }).unwrap();
    execute(s, Command::MoveSelection { delta, system: None }).unwrap();
}

/// The world direction of a bone's own axis.
fn aim(s: &AppState, id: NodeId) -> Vec3 {
    s.scene.world_matrix(id).transform_vector3(s.scene.node(id).unwrap().object.bone_axis().unwrap()).normalize()
}

#[test]
fn new_bones_take_the_reference_defaults() {
    let mut s = AppState::new();
    let ids = chain(&mut s);
    let ObjectKind::Bone { freeze_length, stretch, auto_align, correct_negative_stretch, .. } = &s.scene.node(ids[0]).unwrap().object else { panic!() };
    assert!(freeze_length.get(), "Freeze Length defaults on");
    assert_eq!(stretch.get(), 1, "Stretch defaults to Scale");
    assert!(auto_align.get() && correct_negative_stretch.get());
}

/// Freeze Length on (default): moving the child turns the parent to aim at
/// it and the child keeps its distance; nothing stretches.
#[test]
fn a_moved_child_bone_turns_its_parent_and_freeze_length_keeps_the_length() {
    let mut s = AppState::new();
    let ids = chain(&mut s);
    move_node(&mut s, ids[1], [0.0, 10.0, 0.0]);
    assert!((length(&s, ids[0]) - 10.0).abs() < 1e-4, "length kept");
    let want = Vec3::new(1.0, 1.0, 0.0).normalize();
    assert!(aim(&s, ids[0]).abs_diff_eq(want, 1e-3), "the parent aims at the child: {:?}", aim(&s, ids[0]));
    assert!(pos(&s, ids[1]).abs_diff_eq(want * 10.0, 1e-3), "the child stays 10 away, on the axis: {:?}", pos(&s, ids[1]));
    // The child turned with its parent, so the nub still sits at its tip.
    let tip = s.scene.world_matrix(ids[1]).transform_point3(s.scene.node(ids[1]).unwrap().object.bone_tip_local().unwrap());
    assert!(tip.abs_diff_eq(pos(&s, ids[2]), 1e-3), "{tip:?} vs {:?}", pos(&s, ids[2]));
    // One undo step puts both back.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(aim(&s, ids[0]).abs_diff_eq(Vec3::X, 1e-4));
    assert!(pos(&s, ids[1]).abs_diff_eq(Vec3::new(10.0, 0.0, 0.0), 1e-4));
}

/// Freeze Length off: Scale stretches the parent to reach the child, Squash
/// also thins it, None turns it without stretching.
#[test]
fn freeze_length_off_stretches_by_scale_squash_or_not_at_all() {
    for (stretch, want_len, want_width) in [(1u32, 200f32.sqrt(), 4.0f32), (2, 200f32.sqrt(), 4.0 / 2f32.sqrt().sqrt()), (0, 10.0, 4.0)] {
        let mut s = AppState::new();
        let ids = chain(&mut s);
        set(&mut s, ids[0], "Freeze Length", 0.0);
        set(&mut s, ids[0], "Stretch", stretch as f64);
        move_node(&mut s, ids[1], [0.0, 10.0, 0.0]);
        assert!(pos(&s, ids[1]).abs_diff_eq(Vec3::new(10.0, 10.0, 0.0), 1e-3), "stretch {stretch}: the child reaches the spot: {:?}", pos(&s, ids[1]));
        assert!((length(&s, ids[0]) - want_len).abs() < 1e-3, "stretch {stretch}: length {}", length(&s, ids[0]));
        assert!((param(&s, ids[0], "Width") as f32 - want_width).abs() < 1e-3, "stretch {stretch}: width {}", param(&s, ids[0], "Width"));
        assert!(aim(&s, ids[0]).abs_diff_eq(Vec3::new(1.0, 1.0, 0.0).normalize(), 1e-3));
    }
}

/// Auto-Align off: the parent does not turn and the child leaves its axis;
/// with Freeze Length off it stretches by the distance along its axis.
/// Bone Off: nothing at all happens to the parent.
#[test]
fn auto_align_off_and_bone_off() {
    let mut s = AppState::new();
    let ids = chain(&mut s);
    set(&mut s, ids[0], "Auto-Align", 0.0);
    set(&mut s, ids[0], "Freeze Length", 0.0);
    move_node(&mut s, ids[1], [5.0, 3.0, 0.0]);
    assert!(aim(&s, ids[0]).abs_diff_eq(Vec3::X, 1e-4), "no turn");
    assert!(pos(&s, ids[1]).abs_diff_eq(Vec3::new(15.0, 3.0, 0.0), 1e-3));
    assert!((length(&s, ids[0]) - 15.0).abs() < 1e-3, "stretched along its axis: {}", length(&s, ids[0]));
    let mut s = AppState::new();
    let ids = chain(&mut s);
    set(&mut s, ids[0], "Bone On", 0.0);
    move_node(&mut s, ids[1], [0.0, 10.0, 0.0]);
    assert!(aim(&s, ids[0]).abs_diff_eq(Vec3::X, 1e-4));
    assert!(pos(&s, ids[1]).abs_diff_eq(Vec3::new(10.0, 10.0, 0.0), 1e-3));
}

/// Two linked boxes, the parent with Bone On (a non-bone object).
fn boxes(s: &mut AppState) -> (NodeId, NodeId) {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let a = *s.selection.iter().next().unwrap();
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [20.0, 0.0, 0.0], size: [4.0; 3] }).unwrap();
    let b = *s.selection.iter().next().unwrap();
    execute(s, Command::SetParent { id: b.0, parent: Some(a.0) }).unwrap();
    (a, b)
}

/// The world extent of a node's evaluated mesh along `axis`.
fn extent(s: &AppState, id: NodeId, axis: Vec3) -> f32 {
    let m = s.scene.object_matrix(id);
    let mesh = &s.scene.eval_mesh(id).value.mesh;
    let d: Vec<f32> = mesh.vertices().iter().map(|v| m.transform_point3(v.position).dot(axis)).collect();
    d.iter().cloned().fold(f32::MIN, f32::max) - d.iter().cloned().fold(f32::MAX, f32::min)
}

/// Bone On for any object: a box behaves as a bone. Off by default; on, it
/// turns to aim at its moved child and (Freeze Length off) its geometry
/// stretches along the stretch axis without scaling the child. Reset Scale
/// reads 100 % again without changing the look; Reset Stretch reaches the
/// child again; undo takes the setting back.
#[test]
fn bone_on_makes_any_linked_object_aim_and_stretch() {
    let mut s = AppState::new();
    let (a, b) = boxes(&mut s);
    assert!(!s.scene.node(a).unwrap().bone.bone_on, "Bone Off by default for other objects");
    move_node(&mut s, b, [0.0, 20.0, 0.0]);
    assert!(s.scene.world_matrix(a).transform_vector3(Vec3::X).abs_diff_eq(Vec3::X, 1e-5), "Bone Off: the box does not turn");
    move_node(&mut s, b, [0.0, -20.0, 0.0]);
    set(&mut s, a, "Bone On", 1.0);
    let n = s.scene.node(a).unwrap();
    assert!(n.bone.bone_on && n.bone.freeze_length && n.bone.auto_align);
    assert!((n.bone.length - 20.0).abs() < 1e-4, "unstretched at the child's distance: {}", n.bone.length);
    set(&mut s, a, "Freeze Length", 0.0);
    move_node(&mut s, b, [20.0, 0.0, 0.0]);
    assert!((extent(&s, a, Vec3::X) - 20.0).abs() < 1e-3, "stretched 2x along X: {}", extent(&s, a, Vec3::X));
    assert!((extent(&s, a, Vec3::Y) - 10.0).abs() < 1e-3, "not across");
    assert!((extent(&s, b, Vec3::X) - 4.0).abs() < 1e-3, "the child is not scaled");
    assert!(pos(&s, b).abs_diff_eq(Vec3::new(40.0, 0.0, 0.0), 1e-3));
    move_node(&mut s, b, [-40.0, 40.0, 0.0]);
    let x = s.scene.world_matrix(a).transform_vector3(Vec3::X).normalize();
    assert!(x.abs_diff_eq(Vec3::Y, 1e-3), "the box aims at the child: {x:?}");
    assert!(pos(&s, b).abs_diff_eq(Vec3::new(0.0, 40.0, 0.0), 1e-3), "{:?}", pos(&s, b));
    assert!((s.scene.node(a).unwrap().bone.stretch_factor.x - 2.0).abs() < 1e-3);
    // Reset Scale: no visual change, the factor reads 1 at this distance.
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::BoneResetScale).unwrap();
    assert!((extent(&s, a, Vec3::Y) - 20.0).abs() < 1e-3);
    let n = s.scene.node(a).unwrap();
    assert!(n.bone.stretch_factor.abs_diff_eq(Vec3::ONE, 1e-6) && (n.bone.length - 40.0).abs() < 1e-3);
    // Freeze Length on again, then a move closer; Reset Stretch reaches it.
    set(&mut s, a, "Freeze Length", 1.0);
    move_node(&mut s, b, [0.0, -20.0, 0.0]);
    assert!((extent(&s, a, Vec3::Y) - 20.0).abs() < 1e-3, "frozen: no stretch");
    assert!(pos(&s, b).abs_diff_eq(Vec3::new(0.0, 40.0, 0.0), 1e-3), "frozen: the child keeps its distance");
    execute(&mut s, Command::SetParent { id: b.0, parent: None }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![b.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, -20.0, 0.0], system: None }).unwrap();
    execute(&mut s, Command::SetParent { id: b.0, parent: Some(a.0) }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::BoneResetStretch).unwrap();
    assert!((extent(&s, a, Vec3::Y) - 10.0).abs() < 1e-3, "half the distance, half the stretch: {}", extent(&s, a, Vec3::Y));
    // Squash thins the box as it lengthens.
    set(&mut s, a, "Freeze Length", 0.0);
    set(&mut s, a, "Stretch", 2.0);
    move_node(&mut s, b, [0.0, 20.0, 0.0]);
    assert!((extent(&s, a, Vec3::Y) - 20.0).abs() < 1e-3);
    assert!((extent(&s, a, Vec3::Z) - 10.0 / 2f32.sqrt()).abs() < 1e-3, "{}", extent(&s, a, Vec3::Z));
}

/// Auto-Align off: a child moved behind the pivot gives a negative stretch,
/// which Correct Negative Stretch turns positive; off, the box mirrors.
#[test]
fn correct_negative_stretch_on_a_non_bone() {
    for correct in [true, false] {
        let mut s = AppState::new();
        let (a, b) = boxes(&mut s);
        set(&mut s, a, "Bone On", 1.0);
        set(&mut s, a, "Freeze Length", 0.0);
        set(&mut s, a, "Auto-Align", 0.0);
        set(&mut s, a, "Correct Negative Stretch", if correct { 1.0 } else { 0.0 });
        move_node(&mut s, b, [-40.0, 0.0, 0.0]);
        let f = s.scene.node(a).unwrap().bone.stretch_factor.x;
        assert!((f - if correct { 1.0 } else { -1.0 }).abs() < 1e-4, "correct {correct}: {f}");
        assert!((extent(&s, a, Vec3::X) - 10.0).abs() < 1e-3);
    }
}

/// Bone On objects work with IK (an HI chain over three boxes reaches its
/// goal) and Skin (a skinned cylinder follows when the moved child turns
/// its Bone On parent), and the settings and stretch survive .loom and FBX.
#[test]
fn bone_on_objects_drive_ik_and_skin_and_survive_save_and_fbx() {
    let mut s = AppState::new();
    let (a, b) = boxes(&mut s);
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [40.0, 0.0, 0.0], size: [2.0; 3] }).unwrap();
    let c = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SetParent { id: c.0, parent: Some(b.0) }).unwrap();
    for id in [a, b] {
        set(&mut s, id, "Bone On", 1.0);
    }
    // Skin: a cylinder along the chain, weighted to the boxes.
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Cylinder, position: [0.0; 3], size: [2.0, 2.0, 40.0] }).unwrap();
    let arm = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::RotateSelection { axis: loom_commands::Axis::Y, degrees: 90.0, system: None }).unwrap();
    execute(&mut s, Command::SkinModifier).unwrap();
    execute(&mut s, Command::SkinAddBones { ids: vec![a.0, b.0] }).unwrap();
    let rest = s.scene.eval_mesh(arm).value.mesh.clone();
    move_node(&mut s, b, [0.0, 20.0, 0.0]);
    assert!(s.scene.world_matrix(a).transform_vector3(Vec3::X).normalize().abs_diff_eq(Vec3::new(1.0, 1.0, 0.0).normalize(), 1e-3), "a turned");
    let bent = s.scene.eval_mesh(arm).value.mesh.clone();
    let moved = rest.vertices().iter().zip(bent.vertices()).filter(|(p, q)| p.position.distance(q.position) > 0.5).count();
    assert!(moved > 4, "the skin follows the turned box ({moved} vertices moved)");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    // IK: an HI chain from a to c; the goal moved up, c reaches it.
    let r = execute(&mut s, Command::CreateIkChain { start: a.0, end: c.0, solver: "hi".into(), name: None }).unwrap();
    let goal = NodeId(r["goal"].as_u64().unwrap());
    execute(&mut s, Command::SelectNodes { ids: vec![goal.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [-10.0, 15.0, 0.0], system: None }).unwrap();
    let reached = pos(&s, c);
    assert!(reached.distance(Vec3::new(30.0, 15.0, 0.0)) < 0.5, "IK reaches the goal: {reached:?}");
    // Save / open keeps the settings and the stretch.
    let dir = std::env::temp_dir().join("loom-bone-on-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let mut t = AppState::new();
    let (ta, tb) = boxes(&mut t);
    set(&mut t, ta, "Bone On", 1.0);
    set(&mut t, ta, "Freeze Length", 0.0);
    set(&mut t, ta, "Stretch Axis", 0.0);
    move_node(&mut t, tb, [20.0, 0.0, 0.0]);
    let path = dir.join("bone_on.loom");
    execute(&mut t, Command::SaveFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let saved = t.scene.node(ta).unwrap().clone();
    let doc = loom_commands::file::read(&path).unwrap();
    let back = doc.scene.node(ta).unwrap();
    assert_eq!(back.bone, saved.bone, "Bone On settings saved");
    assert_eq!(back.object_offset, saved.object_offset, "the stretch saved");
    // FBX bakes the stretch into the exported geometry.
    let fbx = dir.join("bone_on.fbx");
    execute(&mut t, Command::SelectNodes { ids: vec![ta.0], op: SelectOp::Replace }).unwrap();
    execute(&mut t, Command::ExportSelected { path: Some(fbx.to_string_lossy().into_owned()) }).unwrap();
    let doc = loom_fbx::read(&std::fs::read(&fbx).unwrap(), 1.0).unwrap();
    let node = doc.nodes.iter().find(|n| n.name == saved.name).unwrap();
    let loom_fbx::FbxKind::Mesh { mesh, .. } = &node.kind else { panic!("not a mesh") };
    let span = |f: fn(Vec3) -> f32| {
        let v: Vec<f32> = mesh.vertices().iter().map(|v| f(v.position)).collect();
        v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min)
    };
    let ratio = span(|p| p.x) / span(|p| p.y);
    assert!((ratio - 2.0).abs() < 1e-3, "stretched 2x along X in the file: {ratio}");
}
