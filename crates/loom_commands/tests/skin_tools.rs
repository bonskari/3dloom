//! Skin mirror mode and gizmo commands.

use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp};
use loom_scene::{Modifier, NodeId};

fn skin_data(s: &AppState, node: NodeId) -> loom_scene::skin::SkinData {
    let n = s.scene.node(node).unwrap();
    let Modifier::Skin(d) = &n.modifiers[0].modifier else { panic!("no Skin") };
    (**d).clone()
}

#[test]
fn mirror_mode_pastes_green_to_blue_through_commands() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0, 0.0, 0.0], size: [20.0, 20.0, 20.0] }).unwrap();
    let body = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SetParameter { id: body.0, name: "Width Segs".into(), value: 4.0 }).unwrap();
    let r = execute(&mut s, Command::CreateBoneChain { points: vec![[-8.0, 0.0, 5.0], [-3.0, 0.0, 5.0], [-2.0, 0.0, 5.0]], name: Some("Bone_L".into()) }).unwrap();
    let left: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    let r = execute(&mut s, Command::CreateBoneChain { points: vec![[8.0, 0.0, 5.0], [3.0, 0.0, 5.0], [2.0, 0.0, 5.0]], name: Some("Bone_R".into()) }).unwrap();
    let right: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![body.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinModifier).unwrap();
    execute(&mut s, Command::SkinAddBones { ids: vec![left[0], right[0]] }).unwrap();
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    let mesh = s.scene.eval_mesh(body).value.mesh.clone();
    let left_verts: Vec<u32> = (0..mesh.vertex_count() as u32).filter(|v| mesh.vertex_position(loom_mesh::VertexId(*v)).unwrap().x < -1.0).collect();
    execute(&mut s, Command::SkinSelectBone { id: left[0] }).unwrap();
    execute(&mut s, Command::SkinSelectVertices { ids: left_verts, op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SetWeightTo10Skin).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Mirror Mode".into(), value: 1.0 }).unwrap();
    assert!(skin_data(&s, body).mirror.mode);
    execute(&mut s, Command::PasteGreenToBlue).unwrap();
    let d = skin_data(&s, body);
    for (i, v) in mesh.vertices().iter().enumerate() {
        if v.position.x > 1.0 {
            assert_eq!(d.weights[i], vec![(1, 1.0)], "vertex {i} at {:?}", v.position);
        }
    }
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    let d = skin_data(&s, body);
    let right_vertex = mesh.vertices().iter().position(|v| v.position.x > 1.0).unwrap();
    assert!(!d.explicit[right_vertex], "undo restores the envelope weights on the right");
    for name in ["Mirror Mode (Skin)", "Paste Green to Blue", "Paste Blue to Green", "Mirror Paste"] {
        assert!(loom_commands::registry().iter().any(|i| i.name == name), "{name} missing");
    }
}

#[test]
fn gizmo_commands_add_keys_and_reset() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Cylinder, position: [0.0; 3], size: [2.0, 2.0, 20.0] }).unwrap();
    let arm = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SetParameter { id: arm.0, name: "Height Segments".into(), value: 8.0 }).unwrap();
    let r = execute(&mut s, Command::CreateBoneChain { points: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 10.0], [0.0, 0.0, 20.0], [0.0, 0.0, 21.0]], name: None }).unwrap();
    let bones: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![arm.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinModifier).unwrap();
    execute(&mut s, Command::SkinAddBones { ids: bones[..2].to_vec() }).unwrap();
    execute(&mut s, Command::SkinSelectBone { id: bones[1] }).unwrap();
    let r = execute(&mut s, Command::SkinAddGizmo { kind: "bulge".into() }).unwrap();
    assert_eq!(r["gizmos"].as_array().unwrap().len(), 1);
    let d = skin_data(&s, arm);
    assert_eq!(d.gizmos[0].bone, 1);
    assert_eq!(d.gizmos[0].parent_bone, 0);
    assert_eq!(d.gizmos[0].keys.len(), 1, "a key at the current angle");
    // Bend the bone, add a key with a bulge, then the joint vertices bulge.
    execute(&mut s, Command::SelectNodes { ids: vec![bones[1]], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: loom_commands::Axis::X, degrees: 90.0, system: None }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![arm.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinAddGizmoKey).unwrap();
    execute(&mut s, Command::SkinSetGizmoKey { key: 1, bulge: Some(1.0), angle: None }).unwrap();
    let d = skin_data(&s, arm);
    assert_eq!(d.gizmos[0].keys.len(), 2);
    assert!((d.gizmos[0].keys[1].angle_deg - 90.0).abs() < 1.0, "{}", d.gizmos[0].keys[1].angle_deg);
    let state = execute(&mut s, Command::State).unwrap();
    assert_eq!(state["skin"]["gizmos"][0]["keys"], 2);
    execute(&mut s, Command::GizmoResetResetRotationPlaneSkin).unwrap();
    let d = skin_data(&s, arm);
    assert!(d.gizmos[0].keys.iter().all(|k| k.bulge == 0.0));
    execute(&mut s, Command::SkinRemoveGizmo).unwrap();
    assert!(skin_data(&s, arm).gizmos.is_empty());
}
