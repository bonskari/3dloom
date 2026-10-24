use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp, SelectionMode};
use loom_scene::{Modifier, NodeId};

fn boxed(s: &mut AppState) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn names(s: &AppState, id: NodeId) -> Vec<String> {
    s.scene.node(id).unwrap().modifiers.iter().map(|m| m.name.clone()).collect()
}

#[test]
fn add_delete_reorder_toggle_are_undoable() {
    let mut s = AppState::new();
    let a = boxed(&mut s);
    execute(&mut s, Command::AddModifier { name: "XForm".into() }).unwrap();
    execute(&mut s, Command::AddModifier { name: "Normal".into() }).unwrap();
    assert_eq!(names(&s, a), vec!["XForm", "Normal"]);
    assert_eq!(s.scene.node(a).unwrap().active_entry, Some(1), "the new entry is active");
    execute(&mut s, Command::SelectStackEntry { index: Some(0) }).unwrap();
    execute(&mut s, Command::AddModifier { name: "Smooth".into() }).unwrap();
    assert_eq!(names(&s, a), vec!["XForm", "Smooth", "Normal"], "inserted above the active entry");
    execute(&mut s, Command::MoveModifier { from: 1, to: 2 }).unwrap();
    assert_eq!(names(&s, a), vec!["XForm", "Normal", "Smooth"]);
    execute(&mut s, Command::ToggleModifier { index: 0, on: false }).unwrap();
    assert!(!s.scene.node(a).unwrap().modifiers[0].enabled);
    execute(&mut s, Command::DeleteModifier { index: 1 }).unwrap();
    assert_eq!(names(&s, a), vec!["XForm", "Smooth"]);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(names(&s, a), vec!["XForm", "Normal", "Smooth"]);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.node(a).unwrap().modifiers[0].enabled);
    assert!(execute(&mut s, Command::AddModifier { name: "Nope".into() }).is_err());
    assert!(Modifier::LIST.contains(&"XForm"));
}

#[test]
fn evaluation_is_cached_until_changed() {
    let mut s = AppState::new();
    let a = boxed(&mut s);
    execute(&mut s, Command::AddModifier { name: "Normal".into() }).unwrap();
    let before = s.scene.eval_count.get();
    let _ = s.scene.eval_mesh(a);
    let _ = s.scene.eval_mesh(a);
    let _ = s.scene.eval_mesh(a);
    assert!(s.scene.eval_count.get() <= before + 1, "three reads, at most one evaluation");
    let after_reads = s.scene.eval_count.get();
    let _ = s.scene.eval_mesh(a);
    assert_eq!(s.scene.eval_count.get(), after_reads, "cached");
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Flip Normals".into(), value: 1.0 }).unwrap();
    let _ = s.scene.eval_mesh(a);
    let _ = s.scene.eval_mesh(a);
    assert_eq!(s.scene.eval_count.get(), after_reads + 1, "a parameter change re-evaluates once");
    let m = s.scene.eval_mesh(a);
    assert!(m.value.mesh.face_normal(loom_mesh::FaceId(0)).dot(s.scene.node(a).unwrap().object.mesh().face_normal(loom_mesh::FaceId(0))) < 0.0, "Flip Normals reverses the result");
    assert_eq!(s.scene.node(a).unwrap().object.mesh().face_count(), 6, "the base object is untouched");
}

#[test]
fn topology_change_below_drops_upper_selection_with_warning() {
    let mut s = AppState::new();
    let a = boxed(&mut s);
    execute(&mut s, Command::AddModifier { name: "Poly Select".into() }).unwrap();
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 4 }).unwrap();
    assert_eq!(s.selection_mode, SelectionMode::Face);
    execute(&mut s, Command::SelectSub { node: Some(a.0), ids: vec![0, 1], op: SelectOp::Replace }).unwrap();
    let stored = s.scene.node(a).unwrap().modifiers[0].modifier.stored_selection().cloned().unwrap();
    assert_eq!(stored.ids, vec![0, 1]);
    assert_eq!(stored.faces, 6);
    execute(&mut s, Command::AddModifier { name: "XForm".into() }).unwrap();
    // The selection flows up: XForm moves only the two polygons.
    let e = s.scene.eval_mesh(a);
    assert_eq!(e.value.selection.as_ref().map(|(_, ids)| ids.len()), Some(2));
    // Change the base topology below the Poly Select.
    execute(&mut s, Command::SetParameter { id: a.0, name: "Length Segs".into(), value: 3.0 }).unwrap();
    let e = s.scene.eval_mesh(a);
    assert!(e.warnings.iter().any(|w| w.contains("Topology changed")), "{:?}", e.warnings);
    assert!(e.value.selection.as_ref().is_none_or(|(_, ids)| ids.is_empty()), "the stale selection is dropped");
}

#[test]
fn collapse_to_keeps_instances_elsewhere() {
    let mut s = AppState::new();
    let a = boxed(&mut s);
    execute(&mut s, Command::AddModifier { name: "Normal".into() }).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Flip Normals".into(), value: 1.0 }).unwrap();
    execute(&mut s, Command::AddModifier { name: "Smooth".into() }).unwrap();
    let b = boxed(&mut s);
    execute(&mut s, Command::InstanceModifier { source: a.0, index: 0 }).unwrap();
    assert_eq!(s.scene.node(b).unwrap().modifiers[0].instance_of, Some((a, 0)));
    // Editing through the instance edits the source.
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Flip Normals".into(), value: 0.0 }).unwrap();
    assert!(matches!(&s.scene.node(a).unwrap().modifiers[0].modifier, Modifier::Normal { flip } if !flip.get()));
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    let undo_before = s.undo.len();
    execute(&mut s, Command::CollapseTo { index: 0 }).unwrap();
    assert_eq!(names(&s, a), vec!["Smooth"], "entries above the collapsed one stay");
    assert!(matches!(s.scene.node(a).unwrap().object, loom_scene::ObjectKind::EditablePoly(_)));
    assert_eq!(s.undo.len(), undo_before + 1);
    assert_eq!(s.scene.node(b).unwrap().modifiers.len(), 1, "the other node keeps the modifier as its own copy");
    assert_eq!(s.scene.node(b).unwrap().modifiers[0].instance_of, None);
    execute(&mut s, Command::SelectNodes { ids: vec![b.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SelectStackEntry { index: Some(0) }).unwrap();
    execute(&mut s, Command::MakeUnique).unwrap();
    assert_eq!(s.scene.node(b).unwrap().modifiers[0].instance_of, None);
    execute(&mut s, Command::CollapseAll).unwrap();
    assert!(s.scene.node(b).unwrap().modifiers.is_empty());
}

#[test]
fn show_end_result_off_and_set_parameter_routing() {
    let mut s = AppState::new();
    let a = boxed(&mut s);
    execute(&mut s, Command::AddModifier { name: "XForm".into() }).unwrap();
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [5.0, 0.0, 0.0], system: None }).unwrap();
    let node = s.scene.node(a).unwrap();
    assert!((node.transform.position.get().x).abs() < 1e-6, "the node itself did not move");
    let Modifier::XForm { gizmo } = &node.modifiers[0].modifier else { panic!() };
    assert!((gizmo.position.get().x - 5.0).abs() < 1e-4, "the gizmo moved");
    let (lo, _) = s.scene.eval_mesh(a).value.mesh.aabb();
    assert!((lo.x + 5.0 - 5.0).abs() < 1e-4, "the result moved by the gizmo: {lo:?}");
    execute(&mut s, Command::ShowEndResultToggle).unwrap();
    assert!(!s.show_end_result);
    // SetParameter on the base object still edits the primitive.
    execute(&mut s, Command::SetParameter { id: a.0, name: "Height".into(), value: 20.0 }).unwrap();
    let (_, hi) = s.scene.node(a).unwrap().object.mesh().aabb();
    assert!((hi.z - 20.0).abs() < 1e-4);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    let Modifier::XForm { gizmo } = &s.scene.node(a).unwrap().modifiers[0].modifier else { panic!() };
    assert!(gizmo.position.get().x.abs() < 1e-6, "gizmo move undone");
}

#[test]
fn state_lists_the_stack_with_modifier_names_and_levels() {
    let mut s = AppState::new();
    let _a = boxed(&mut s);
    execute(&mut s, Command::AddModifier { name: "Bend".into() }).unwrap();
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    let st = execute(&mut s, Command::State).unwrap();
    let stack = &st["nodes"][0]["stack"];
    assert_eq!(stack[0]["modifier"], "Bend");
    assert_eq!(stack[0]["sub_level"], 1);
}
