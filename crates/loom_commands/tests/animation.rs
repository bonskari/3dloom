//! Scene time and Auto Key through the ordinary editing commands (phase 8).

use glam::Vec3;
use loom_commands::{execute, AppState, Axis, Command, PrimitiveKind, SelectOp};
use loom_scene::{Modifier, NodeId};

fn boxed(s: &mut AppState) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn pos_at(s: &AppState, id: NodeId, t: f32) -> Vec3 {
    s.scene.node(id).unwrap().transform.position.get_at(t)
}

fn frame(s: &mut AppState, f: i32) {
    execute(s, Command::SetFrame { frame: f }).unwrap();
}

#[test]
fn auto_key_move_creates_position_keys() {
    let mut s = AppState::new();
    let b = boxed(&mut s);
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    frame(&mut s, 10);
    execute(&mut s, Command::MoveSelection { delta: [20.0, 0.0, 0.0], system: None }).unwrap();
    assert!((pos_at(&s, b, 0.0) - Vec3::ZERO).length() < 1e-4);
    assert!((pos_at(&s, b, 10.0) - Vec3::new(20.0, 0.0, 0.0)).length() < 1e-4);
    assert!((pos_at(&s, b, 5.0).x - 10.0).abs() < 1e-3);
    // The viewport follows the time slider.
    frame(&mut s, 5);
    assert!((s.scene.world_matrix(b).w_axis.x - 10.0).abs() < 1e-3);
    // Creating an object with Auto Key on does not animate it.
    let c = boxed(&mut s);
    assert!(!s.scene.node(c).unwrap().transform.position.is_animated());
    // Rotation keys too, per Euler component.
    frame(&mut s, 20);
    execute(&mut s, Command::SelectNodes { ids: vec![b.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: Axis::Z, degrees: 90.0, system: None }).unwrap();
    let r = &s.scene.node(b).unwrap().transform.rotation;
    assert!((r.get_at(20.0).z - 90.0).abs() < 1e-3);
    assert!(r.get_at(0.0).z.abs() < 1e-3);
}

#[test]
fn auto_key_off_edits_offset_existing_keys() {
    let mut s = AppState::new();
    let b = boxed(&mut s);
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    frame(&mut s, 10);
    execute(&mut s, Command::MoveSelection { delta: [20.0, 0.0, 0.0], system: None }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    // Between keys: the whole animation moves.
    frame(&mut s, 5);
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 0.0], system: None }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [5.0, 0.0, 0.0], system: None }).unwrap();
    assert!((pos_at(&s, b, 0.0).x - 5.0).abs() < 1e-3, "{:?}", pos_at(&s, b, 0.0));
    assert!((pos_at(&s, b, 10.0).x - 25.0).abs() < 1e-3);
    assert!(s.scene.node(b).unwrap().transform.position.is_animated(), "keys survive");
    // On a key: only that key changes.
    frame(&mut s, 10);
    execute(&mut s, Command::MoveSelection { delta: [5.0, 0.0, 0.0], system: None }).unwrap();
    assert!((pos_at(&s, b, 10.0).x - 30.0).abs() < 1e-3);
    assert!((pos_at(&s, b, 0.0).x - 5.0).abs() < 1e-3);
    // A typed absolute transform behaves the same.
    execute(&mut s, Command::SetTransform { id: b.0, position: Some([31.0, 0.0, 0.0]), rotation_deg: None, scale: None }).unwrap();
    assert!((pos_at(&s, b, 10.0).x - 31.0).abs() < 1e-3);
    assert!(s.scene.node(b).unwrap().transform.position.is_animated());
}

#[test]
fn undo_removes_auto_keys() {
    let mut s = AppState::new();
    let b = boxed(&mut s);
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    frame(&mut s, 10);
    execute(&mut s, Command::MoveSelection { delta: [20.0, 0.0, 0.0], system: None }).unwrap();
    frame(&mut s, 20);
    execute(&mut s, Command::MoveSelection { delta: [0.0, 20.0, 0.0], system: None }).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    let p = &s.scene.node(b).unwrap().transform.position;
    let k = p.keys().unwrap();
    assert_eq!(k.tracks[0].keys.len(), 2);
    assert!(k.tracks[1].keys.is_empty(), "the Y keys went with the undo");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(!s.scene.node(b).unwrap().transform.position.is_animated(), "both the frame-0 and frame-10 keys are gone");
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert!((pos_at(&s, b, 10.0).x - 20.0).abs() < 1e-4);
}

#[test]
fn animated_parameter_reevaluates_per_frame_and_static_stays_cached() {
    let mut s = AppState::new();
    let b = boxed(&mut s);
    execute(&mut s, Command::AddModifier { name: "Bend".into() }).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Angle".into(), value: 30.0 }).unwrap();
    let _ = s.scene.eval_mesh(b);
    let n0 = s.scene.eval_count();
    for f in [5, 10, 20] {
        frame(&mut s, f);
        let _ = s.scene.eval_mesh(b);
    }
    assert_eq!(s.scene.eval_count(), n0, "nothing animated: one evaluation for every frame");
    // Key the Bend angle: each frame now evaluates, and a frame's result is the right one.
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    frame(&mut s, 10);
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Angle".into(), value: 90.0 }).unwrap();
    frame(&mut s, 10);
    let at10 = s.scene.eval_mesh(b).value.mesh.aabb();
    frame(&mut s, 20);
    let at20 = s.scene.eval_mesh(b).value.mesh.aabb();
    assert_eq!(at10, at20, "frame 20 holds the frame-10 key");
    frame(&mut s, 0);
    let at0 = s.scene.eval_mesh(b).value.mesh.aabb();
    assert_ne!(at0, at10, "frame 0 is not served the frame-10 result");
    let n1 = s.scene.eval_count();
    let _ = s.scene.eval_mesh(b);
    assert_eq!(s.scene.eval_count(), n1, "the same frame stays cached");
}

#[test]
fn animated_box_height_and_bend_angle() {
    let mut s = AppState::new();
    let b = boxed(&mut s);
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    frame(&mut s, 10);
    execute(&mut s, Command::SetParameter { id: b.0, name: "Height".into(), value: 30.0 }).unwrap();
    frame(&mut s, 0);
    let (_, hi0) = s.scene.eval_mesh(b).value.mesh.aabb();
    frame(&mut s, 10);
    let (_, hi10) = s.scene.eval_mesh(b).value.mesh.aabb();
    frame(&mut s, 5);
    let (_, hi5) = s.scene.eval_mesh(b).value.mesh.aabb();
    assert!((hi0.z - 10.0).abs() < 1e-3 && (hi10.z - 30.0).abs() < 1e-3, "{hi0} {hi10}");
    assert!((hi5.z - 20.0).abs() < 1e-2, "{hi5}");
    let r = execute(&mut s, Command::State).unwrap();
    assert!(r.to_string().contains("Box"));
    let _ = Modifier::Bend { d: Default::default(), angle: loom_scene::Param::new(0.0), direction: loom_scene::Param::new(0.0) };
}

#[test]
fn skinned_mesh_follows_keyed_bones() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Cylinder, position: [0.0; 3], size: [2.0, 2.0, 20.0] }).unwrap();
    let arm = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SetParameter { id: arm.0, name: "Height Segments".into(), value: 8.0 }).unwrap();
    let r = execute(&mut s, Command::CreateBoneChain { points: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 10.0], [0.0, 0.0, 20.0], [0.0, 0.0, 21.0]], name: None }).unwrap();
    let bones: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![arm.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinModifier).unwrap();
    execute(&mut s, Command::SkinAddBones { ids: bones[..2].to_vec() }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    frame(&mut s, 10);
    execute(&mut s, Command::SelectNodes { ids: vec![bones[1]], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: Axis::X, degrees: 90.0, system: None }).unwrap();
    frame(&mut s, 0);
    let (_, hi0) = s.scene.eval_mesh(arm).value.mesh.aabb();
    frame(&mut s, 10);
    let (_, hi10) = s.scene.eval_mesh(arm).value.mesh.aabb();
    assert!(hi0.z > 19.0, "{hi0}");
    assert!(hi10.z < 17.0, "the top bends away at frame 10: {hi10}");
}

#[test]
fn importing_with_auto_key_on_does_not_animate() {
    let mut s = AppState::new();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    frame(&mut s, 10);
    let rig = concat!(env!("CARGO_MANIFEST_DIR"), "/../loom_fbx/tests/fixtures/character_for_mixamo.fbx");
    execute(&mut s, Command::ImportFile { path: Some(rig.into()) }).unwrap();
    assert!(s.scene.nodes().all(|n| !n.transform.position.is_animated() && !n.transform.rotation.is_animated() && !n.transform.scale.is_animated()));
}

#[test]
fn changing_frame_redraws_the_viewports() {
    let mut s = AppState::new();
    let _ = boxed(&mut s);
    s.mesh_dirty = false;
    frame(&mut s, 12);
    assert!(s.mesh_dirty, "a new frame rebuilds the viewport meshes");
    s.mesh_dirty = false;
    frame(&mut s, 12);
    assert!(!s.mesh_dirty, "the same frame does not");
}
