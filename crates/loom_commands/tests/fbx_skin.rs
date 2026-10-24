//! Import / export of rigs: bones, Skin weights, Morpher channels.

use loom_commands::{execute, AppState, Axis, Command, PrimitiveKind, SelectOp};
use loom_scene::{Modifier, NodeId, ObjectKind};
use std::path::PathBuf;

const RIG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../loom_fbx/tests/fixtures/sitting_idle.fbx");

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join("loom-fbx-skin-tests").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn imports_the_mixamo_rig_with_bones_and_skin_and_exports_it_back() {
    let d = dir("mixamo");
    let mut s = AppState::new();
    execute(&mut s, Command::ImportFile { path: Some(RIG.into()) }).unwrap();
    let bones: Vec<&loom_scene::Node> = s.scene.nodes().filter(|n| n.object.is_bone()).collect();
    assert_eq!(bones.len(), 65);
    let hips = s.scene.nodes().find(|n| n.name == "mixamorig:Hips").expect("bone names kept exactly");
    assert!(hips.object.is_bone());
    let mesh_id = s.scene.nodes().find(|n| matches!(n.object, ObjectKind::EditablePoly(_))).unwrap().id;
    let skin = s.scene.node(mesh_id).unwrap().modifiers.iter().find_map(|m| match &m.modifier { Modifier::Skin(d) => Some((**d).clone()), _ => None }).expect("Skin modifier");
    assert_eq!(skin.bones.len(), 52);
    assert!(skin.explicit.iter().all(|x| *x), "file weights are hand-set");
    let r = execute(&mut s, Command::State).unwrap();
    assert!(r["nodes"].as_array().unwrap().len() >= 66);
    // Rotating the left arm moves vertices of the mesh.
    let rest = s.scene.eval_mesh(mesh_id).value.mesh.clone();
    let arm = s.scene.nodes().find(|n| n.name == "mixamorig:LeftArm").unwrap().id;
    execute(&mut s, Command::SelectNodes { ids: vec![arm.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: Axis::Z, degrees: 60.0, system: None }).unwrap();
    let bent = s.scene.eval_mesh(mesh_id).value.mesh.clone();
    // Mixamo meshes sit under a 100× node scale: measure in world (centimetre) units.
    let w = s.scene.object_matrix(mesh_id);
    let moved = rest.vertices().iter().zip(bent.vertices()).filter(|(a, b)| (w.transform_point3(a.position) - w.transform_point3(b.position)).length() > 0.5).count();
    assert!(moved > 50, "{moved} vertices moved with the arm");
    assert!(moved < rest.vertex_count() / 2, "but not the whole body ({moved} of {})", rest.vertex_count());
    execute(&mut s, Command::RotateSelection { axis: Axis::Z, degrees: -60.0, system: None }).unwrap();
    // Export everything and read it back.
    let path = d.join("rig.fbx");
    execute(&mut s, Command::ExportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let doc = loom_fbx::read(&std::fs::read(&path).unwrap(), 0.01).unwrap();
    let names: Vec<&str> = doc.nodes.iter().filter(|n| matches!(n.kind, loom_fbx::FbxKind::Bone { .. })).map(|n| n.name.as_str()).collect();
    assert_eq!(names.len(), 65);
    assert!(names.contains(&"mixamorig:Hips") && names.contains(&"mixamorig:LeftHandIndex4"));
    let mesh = doc.nodes.iter().find(|n| matches!(n.kind, loom_fbx::FbxKind::Mesh { .. })).unwrap();
    assert_eq!(mesh.skin.as_ref().unwrap().bones.len(), 52);
    // Export Selected on the mesh alone still brings its bones.
    execute(&mut s, Command::SelectNodes { ids: vec![mesh_id.0], op: SelectOp::Replace }).unwrap();
    let path = d.join("mesh_only.fbx");
    execute(&mut s, Command::ExportSelected { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let doc = loom_fbx::read(&std::fs::read(&path).unwrap(), 0.01).unwrap();
    assert!(doc.nodes.iter().filter(|n| matches!(n.kind, loom_fbx::FbxKind::Bone { .. })).count() >= 52);
}

#[test]
fn morpher_channels_round_trip_as_blend_shapes() {
    let d = dir("morph");
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let head = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let target = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SetParameter { id: target.0, name: "Height".into(), value: 15.0 }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![head.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MorpherModifier).unwrap();
    execute(&mut s, Command::MorpherPickTarget { channel: 0, node: target.0 }).unwrap();
    execute(&mut s, Command::MorpherSetChannel { channel: 0, name: "Value".into(), value: 40.0 }).unwrap();
    let path = d.join("morph.fbx");
    execute(&mut s, Command::ExportSelected { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    execute(&mut s, Command::NewScene).unwrap();
    execute(&mut s, Command::ImportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let node = s.scene.nodes().find(|n| n.name == "Box001").unwrap();
    let morpher = node.modifiers.iter().find_map(|m| match &m.modifier { Modifier::Morpher(d) => Some((**d).clone()), _ => None }).expect("Morpher");
    assert_eq!(morpher.channels[0].name, "Box002");
    assert!((morpher.channels[0].value.get() - 40.0).abs() < 1e-3);
    assert!(morpher.channels[0].valid);
    let (_, hi) = s.scene.eval_mesh(node.id).value.mesh.aabb();
    assert!((hi.z - 12.0).abs() < 1e-2, "40 % of the way to 15: {hi:?}");
    let _ = NodeId(0);
}
