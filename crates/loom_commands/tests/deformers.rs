use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp};
use loom_scene::NodeId;

fn tall_box(s: &mut AppState) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0, 10.0, 40.0] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    execute(s, Command::SetParameter { id: id.0, name: "Height Segs".into(), value: 8.0 }).unwrap();
    id
}

#[test]
fn bend_through_commands_and_gizmo_move() {
    let mut s = AppState::new();
    let a = tall_box(&mut s);
    execute(&mut s, Command::AddModifier { name: "Bend".into() }).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Angle".into(), value: 90.0 }).unwrap();
    let (lo, hi) = s.scene.eval_mesh(a).value.mesh.aabb();
    let r = 40.0 / std::f32::consts::FRAC_PI_2;
    // At 90° the top face stands vertical at x = r, spanning z = r ± 5.
    assert!((hi.x - r).abs() < 0.5, "bent toward +X to the radius: {hi:?}");
    assert!((hi.z - (r + 5.0)).abs() < 0.5 && hi.z < 40.0, "the top came down: {hi:?}");
    assert!((lo.z - 0.0).abs() < 1e-3);
    assert_eq!(s.scene.node(a).unwrap().object.mesh().face_count(), 6 + 4 * 7 + 0, "base untouched: {}", s.scene.node(a).unwrap().object.mesh().face_count());
    // Rotating the gizmo 180° about Z bends the other way.
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: loom_commands::Axis::Z, degrees: 180.0, system: None }).unwrap();
    let (lo2, _) = s.scene.eval_mesh(a).value.mesh.aabb();
    assert!(lo2.x < -(r - 5.0) + 0.5, "{lo2:?}");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    let (_, hi3) = s.scene.eval_mesh(a).value.mesh.aabb();
    assert!((hi3.x - r).abs() < 0.5);
    // Limit Effect: bend only the lower half.
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Limit Effect".into(), value: 1.0 }).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Upper Limit".into(), value: 20.0 }).unwrap();
    let e = s.scene.eval_mesh(a);
    assert!(e.warnings.is_empty());
}

#[test]
fn symmetry_slice_and_mirror_change_topology() {
    let mut s = AppState::new();
    let a = tall_box(&mut s);
    execute(&mut s, Command::AddModifier { name: "Symmetry".into() }).unwrap();
    let m = s.scene.eval_mesh(a).value.mesh.clone();
    m.check_invariants().unwrap();
    let (lo, hi) = m.aabb();
    assert!((lo.x + 5.0).abs() < 1e-3 && (hi.x - 5.0).abs() < 1e-3);
    execute(&mut s, Command::AddModifier { name: "Slice".into() }).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 1, name: "Slice Type".into(), value: 2.0 }).unwrap();
    execute(&mut s, Command::ModifierSubobjectLevel { index: 1, level: 1 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 10.0], system: None }).unwrap();
    let (_, hi2) = s.scene.eval_mesh(a).value.mesh.aabb();
    assert!((hi2.z - 10.0).abs() < 1e-3, "Remove Top above the moved plane: {hi2:?}");
    let m2 = s.scene.eval_mesh(a).value.mesh.clone();
    execute(&mut s, Command::AddModifier { name: "Mirror".into() }).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 2, name: "Copy".into(), value: 1.0 }).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 2, name: "Offset".into(), value: 10.0 }).unwrap();
    let m3 = s.scene.eval_mesh(a).value.mesh.clone();
    m3.check_invariants().unwrap();
    assert_eq!(m3.face_count(), m2.face_count() * 2, "Copy doubles the faces");
}

#[test]
fn linked_xform_follows_its_control() {
    let mut s = AppState::new();
    let a = tall_box(&mut s);
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [50.0, 0.0, 0.0], size: [5.0; 3] }).unwrap();
    let c = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::AddModifier { name: "Linked XForm".into() }).unwrap();
    execute(&mut s, Command::PickModifierNode { index: 0, node: c.0 }).unwrap();
    let before = s.scene.eval_mesh(a).value.mesh.aabb();
    execute(&mut s, Command::SelectNodes { ids: vec![c.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 7.0], system: None }).unwrap();
    let after = s.scene.eval_mesh(a).value.mesh.aabb();
    assert!((after.0.z - before.0.z - 7.0).abs() < 1e-3, "{before:?} {after:?}");
}
