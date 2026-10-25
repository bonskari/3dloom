//! Morpher and Skin Morph through commands.

use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp};
use loom_scene::{Modifier, NodeId};

#[test]
fn morpher_picks_a_target_and_extracts_one() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let head = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let target = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SetParameter { id: target.0, name: "Height".into(), value: 15.0 }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![head.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MorpherModifier).unwrap();
    assert_eq!(s.scene.node(head).unwrap().modifiers[0].name, "Morpher");
    execute(&mut s, Command::MorpherPickTarget { channel: 0, node: target.0 }).unwrap();
    let Modifier::Morpher(d) = &s.scene.node(head).unwrap().modifiers[0].modifier else { panic!() };
    assert_eq!(d.channels[0].name, "Box002");
    assert!(d.channels[0].valid);
    execute(&mut s, Command::MorpherSetChannel { channel: 0, name: "Value".into(), value: 100.0 }).unwrap();
    let m = s.scene.eval_mesh(head).value.mesh.clone();
    let (_, hi) = m.aabb();
    assert!((hi.z - 15.0).abs() < 1e-3, "morphed to the taller box: {hi:?}");
    let r = execute(&mut s, Command::State).unwrap();
    assert_eq!(r["morpher"]["channels"][0]["value"], 100.0);
    execute(&mut s, Command::MorpherSetChannel { channel: 0, name: "Value".into(), value: 50.0 }).unwrap();
    let r = execute(&mut s, Command::MorpherExtractTarget { channel: 0 }).unwrap();
    let extracted = NodeId(r["node"].as_u64().unwrap());
    let n = s.scene.node(extracted).unwrap();
    assert_eq!(n.name, "Box002_morph");
    let (_, hi) = s.scene.eval_mesh(extracted).value.mesh.aabb();
    assert!((hi.z - 15.0).abs() < 1e-3, "the extracted target is the channel's full target: {hi:?}");
    // Progressive target at 50 %, then delete the channel.
    execute(&mut s, Command::SelectNodes { ids: vec![head.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MorpherAddProgressiveTarget { channel: 0, node: extracted.0 }).unwrap();
    let Modifier::Morpher(d) = &s.scene.node(head).unwrap().modifiers[0].modifier else { panic!() };
    assert_eq!(d.channels[0].targets.len(), 2);
    execute(&mut s, Command::MorpherDeleteChannel { channel: 0 }).unwrap();
    let Modifier::Morpher(d) = &s.scene.node(head).unwrap().modifiers[0].modifier else { panic!() };
    assert!(d.channels[0].targets.is_empty());
    assert!(loom_commands::registry().iter().any(|i| i.name == "Morpher Modifier"));
}

#[test]
fn skin_morph_creates_a_morph_at_the_bone_angle() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Cylinder, position: [0.0; 3], size: [2.0, 2.0, 20.0] }).unwrap();
    let arm = *s.selection.iter().next().unwrap();
    let r = execute(&mut s, Command::CreateBoneChain { points: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 10.0], [0.0, 0.0, 20.0], [0.0, 0.0, 21.0]], name: None }).unwrap();
    let bones: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![arm.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinMorphModifier).unwrap();
    execute(&mut s, Command::SkinMorphAddBones { ids: bones[..2].to_vec() }).unwrap();
    // Bend, create the morph at this angle, give vertex 0 a delta.
    execute(&mut s, Command::SelectNodes { ids: vec![bones[1]], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: loom_commands::Axis::X, degrees: 90.0, system: None }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![arm.0], op: SelectOp::Replace }).unwrap();
    let r = execute(&mut s, Command::SkinMorphCreateMorph { bone: bones[1] }).unwrap();
    assert_eq!(r["morph"], 0);
    let Modifier::SkinMorph(d) = &s.scene.node(arm).unwrap().modifiers[0].modifier else { panic!() };
    assert!((d.morphs[0].angle_deg - 90.0).abs() < 1.0, "{}", d.morphs[0].angle_deg);
    execute(&mut s, Command::SkinMorphSetVertexDelta { morph: 0, vertex: 0, delta: [0.0, 3.0, 0.0] }).unwrap();
    let r = execute(&mut s, Command::State).unwrap();
    assert_eq!(r["skin_morph"]["morphs"][0]["deltas"], 1);
    let bent = s.scene.eval_mesh(arm).value.mesh.vertices()[0].position;
    execute(&mut s, Command::SelectNodes { ids: vec![bones[1]], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: loom_commands::Axis::X, degrees: -90.0, system: None }).unwrap();
    let straight = s.scene.eval_mesh(arm).value.mesh.vertices()[0].position;
    assert!((bent - straight).length() > 2.9, "the morph only fires at its angle: {:?}", bent - straight);
    // Undo: the rotation, the bone selection, then the delta.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    let Modifier::SkinMorph(d) = &s.scene.node(arm).unwrap().modifiers[0].modifier else { panic!() };
    assert!(d.morphs[0].deltas.is_empty(), "undo removes the delta");
}
