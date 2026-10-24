//! Skin Wrap through commands: bind to a skinned arm, follow its bend, Convert To Skin.

use loom_commands::{execute, AppState, Axis, Command, PrimitiveKind, SelectOp};
use loom_scene::{Modifier, NodeId};

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

#[test]
fn skin_wrap_follows_the_skinned_arm_and_converts_to_skin() {
    let mut s = AppState::new();
    let (arm, bones) = skinned_arm(&mut s);
    // A sleeve: a wider, shorter cylinder around the upper half of the arm.
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Cylinder, position: [0.0, 0.0, 12.0], size: [3.0, 3.0, 8.0] }).unwrap();
    let sleeve = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SetParameter { id: sleeve.0, name: "Height Segments".into(), value: 4.0 }).unwrap();
    execute(&mut s, Command::SkinWrapModifier).unwrap();
    assert_eq!(s.scene.node(sleeve).unwrap().modifiers[0].name, "Skin Wrap");
    execute(&mut s, Command::SkinWrapAddBase { id: arm.0 }).unwrap();
    let r = execute(&mut s, Command::State).unwrap();
    assert_eq!(r["skin_wrap"]["bases"].as_array().unwrap().len(), 1);
    let rest = s.scene.eval_mesh(sleeve).value.mesh.clone();
    execute(&mut s, Command::SelectNodes { ids: vec![bones[1]], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: Axis::X, degrees: 90.0, system: None }).unwrap();
    let bent = s.scene.eval_mesh(sleeve).value.mesh.clone();
    let moved = rest.vertices().iter().zip(bent.vertices()).filter(|(a, b)| (a.position - b.position).length() > 1.0).count();
    assert!(moved > 0, "the sleeve follows the bent arm");
    // Convert To Skin: the sleeve gets a Skin with the arm's bones and hand-set weights.
    execute(&mut s, Command::SelectNodes { ids: vec![sleeve.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinWrapConvertToSkin).unwrap();
    let n = s.scene.node(sleeve).unwrap();
    let skin = n.modifiers.iter().find_map(|m| match &m.modifier { Modifier::Skin(d) => Some((**d).clone()), _ => None }).expect("a Skin modifier");
    assert_eq!(skin.bones.len(), 2);
    assert!(skin.explicit.iter().filter(|x| **x).count() > 0);
    assert!(skin.weights.iter().any(|w| !w.is_empty()));
    for name in ["Skin Wrap Modifier", "Skin Wrap Add Base", "Skin Wrap Set Option", "Skin Wrap Reset", "Skin Wrap Convert To Skin"] {
        assert!(loom_commands::registry().iter().any(|i| i.name == name), "{name} missing");
    }
}
