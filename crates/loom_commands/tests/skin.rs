//! Skin modifier commands: binding bones, Weight Tool edits, skin pose.

use loom_commands::{execute, AppState, Axis, Command, PrimitiveKind, SelectOp};
use loom_scene::{Modifier, NodeId};

/// A 20-unit cylinder along Z skinned to two 10-unit bones.
fn skinned_arm(s: &mut AppState) -> (NodeId, Vec<u64>) {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Cylinder, position: [0.0; 3], size: [2.0, 2.0, 20.0] }).unwrap();
    let arm = *s.selection.iter().next().unwrap();
    execute(s, Command::SetParameter { id: arm.0, name: "Height Segments".into(), value: 8.0 }).unwrap();
    let r = execute(s, Command::CreateBoneChain { points: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 10.0], [0.0, 0.0, 20.0], [0.0, 0.0, 21.0]], name: None }).unwrap();
    let bones: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    execute(s, Command::SelectNodes { ids: vec![arm.0], op: SelectOp::Replace }).unwrap();
    execute(s, Command::SkinModifier).unwrap();
    execute(s, Command::SkinAddBones { ids: bones[..2].to_vec() }).unwrap();
    (arm, bones)
}

fn skin_data(s: &AppState, arm: NodeId) -> loom_scene::skin::SkinData {
    let n = s.scene.node(arm).unwrap();
    let Modifier::Skin(d) = &n.modifiers[0].modifier else { panic!("no Skin on the arm") };
    (**d).clone()
}

#[test]
fn skin_modifier_binds_bones_and_weight_tool_edits_vertices() {
    let mut s = AppState::new();
    let (arm, bones) = skinned_arm(&mut s);
    let d = skin_data(&s, arm);
    assert_eq!(d.bones.len(), 2);
    assert_eq!(s.scene.node(arm).unwrap().modifiers[0].name, "Skin");
    let r = execute(&mut s, Command::State).unwrap();
    assert_eq!(r["skin"]["bones"].as_array().unwrap().len(), 2);
    // Edit Envelopes level, select the top ring and give it fully to bone 2.
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    execute(&mut s, Command::SkinSelectBone { id: bones[1] }).unwrap();
    let mesh = s.scene.eval_mesh(arm).value.mesh.clone();
    let top: Vec<u32> = (0..mesh.vertex_count() as u32).filter(|v| mesh.vertex_position(loom_mesh::VertexId(*v)).unwrap().z > 19.0).collect();
    assert!(!top.is_empty());
    execute(&mut s, Command::SkinSelectVertices { ids: top.clone(), op: SelectOp::Replace }).unwrap();
    assert_eq!(s.skin_selection.len(), top.len());
    execute(&mut s, Command::SetWeightTo10Skin).unwrap(); // chart: "Set Weight To 1.0 (Skin)"
    let d = skin_data(&s, arm);
    for v in &top {
        assert_eq!(d.weights[*v as usize], vec![(1, 1.0)], "vertex {v}");
        assert!(d.explicit[*v as usize]);
    }
    // Half to bone 1 through the custom weight, then Blend averages with the neighbours.
    execute(&mut s, Command::SkinSelectBone { id: bones[0] }).unwrap();
    execute(&mut s, Command::SetWeightTo05Skin).unwrap();
    let d = skin_data(&s, arm);
    let w = &d.weights[top[0] as usize];
    assert!((w.iter().find(|(b, _)| *b == 0).unwrap().1 - 0.5).abs() < 1e-5, "{w:?}");
    assert!((w.iter().find(|(b, _)| *b == 1).unwrap().1 - 0.5).abs() < 1e-5, "{w:?}");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    let d = skin_data(&s, arm);
    assert!(!d.explicit[top[0] as usize], "undo restores the envelope weights");
    // Every chart row exists.
    for name in ["Add Bones (Skin)", "Blend Weights (Skin)", "Weight Table Dialog (Skin)", "Weight Tool Dialog (Skin)", "Set Skin Pose", "Assume Skin Pose", "Skin Pose Mode, On/Off Toggle", "Skin Loop", "Skin Ring", "Skin Grow", "Skin Shrink", "Skin Element", "Remove Zero Weights (Skin)", "Select Vertices By Bone (Skin)", "Exclude Verts (Skin)", "Zoom To Selected Bone (Skin)", "Scale Weight Up (Skin)", "Set Weight To 0.0 (Skin)", "Select Parent Bone (Skin)", "Filter Vertices (Skin)", "Draw Envelope On Top (Skin)", "Skin Modifier"] {
        assert!(loom_commands::registry().iter().any(|i| i.name == name), "{name} missing");
    }
}

#[test]
fn moving_a_bone_bends_the_skinned_mesh_and_skin_pose_restores() {
    let mut s = AppState::new();
    let (arm, bones) = skinned_arm(&mut s);
    execute(&mut s, Command::SetSkinPose).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![bones[1]], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: Axis::X, degrees: 90.0, system: None }).unwrap();
    let bent = s.scene.eval_mesh(arm).value.mesh.clone();
    let w = s.scene.object_matrix(arm);
    assert!(bent.vertices().iter().any(|v| w.transform_point3(v.position).y.abs() > 7.0), "bent: max |y| {}", bent.vertices().iter().map(|v| w.transform_point3(v.position).y.abs()).fold(0.0, f32::max));
    execute(&mut s, Command::AssumeSkinPose).unwrap();
    let back = s.scene.eval_mesh(arm).value.mesh.clone();
    assert!(back.vertices().iter().all(|v| w.transform_point3(v.position).y.abs() < 2.1), "straight again");
}

#[test]
fn selection_tools_and_vertex_by_bone_and_exclude() {
    let mut s = AppState::new();
    let (arm, bones) = skinned_arm(&mut s);
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    execute(&mut s, Command::SkinSelectBone { id: bones[1] }).unwrap();
    execute(&mut s, Command::SelectVerticesByBoneSkin).unwrap();
    let mesh = s.scene.eval_mesh(arm).value.mesh.clone();
    assert!(!s.skin_selection.is_empty());
    assert!(s.skin_selection.iter().all(|v| mesh.vertex_position(loom_mesh::VertexId(*v)).unwrap().z > 5.0), "bone 2 weights the upper half");
    let before = s.skin_selection.len();
    execute(&mut s, Command::SkinGrow).unwrap();
    assert!(s.skin_selection.len() > before);
    execute(&mut s, Command::SkinShrink).unwrap();
    assert_eq!(s.skin_selection.len(), before);
    // Exclude the selection from bone 2: those vertices lose bone 2.
    execute(&mut s, Command::ExcludeVertsSkin).unwrap();
    let d = skin_data(&s, arm);
    assert_eq!(d.excluded[1].len(), before);
    let v = *s.skin_selection.iter().next().unwrap();
    let p = d.mesh_bind.transform_point3(mesh.vertex_position(loom_mesh::VertexId(v)).unwrap());
    assert!(d.vertex_weights(v as usize, p).iter().all(|(b, _)| *b != 1));
    execute(&mut s, Command::IncludeVertsSkin).unwrap();
    assert!(skin_data(&s, arm).excluded[1].is_empty());
    // Loop / ring on a vertex selection: one vertex → its ring around the cylinder.
    execute(&mut s, Command::SkinSelectVertices { ids: vec![v], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinRing).unwrap();
    assert!(s.skin_selection.len() >= 2, "{:?}", s.skin_selection);
}
