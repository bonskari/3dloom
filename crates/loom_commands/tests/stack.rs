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
    assert!(!s.show_end_result, "Show End Result starts off, as in the reference");
    execute(&mut s, Command::ShowEndResultToggle).unwrap();
    assert!(s.show_end_result);
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

fn set(s: &mut AppState, index: u32, name: &str, value: f64) {
    execute(s, Command::SetModifierParameter { index, name: name.into(), value }).unwrap();
}

/// Smooth > Prevent Indirect Smoothing: faces that only share a vertex get
/// different groups too, so no smoothing leaks across that vertex.
#[test]
fn smooth_prevent_indirect_smoothing_separates_faces_meeting_at_a_vertex() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Pyramid, position: [0.0; 3], size: [10.0, 10.0, 10.0] }).unwrap();
    let a = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::AddModifier { name: "Smooth".into() }).unwrap();
    set(&mut s, 0, "Auto Smooth", 1.0);
    set(&mut s, 0, "Threshold", 5.0);
    // Two faces that share a vertex but no edge, sharing a group bit.
    let leaks = |s: &AppState| {
        let m = s.scene.eval_mesh(a).value.mesh.clone();
        let mut n = 0;
        for v in 0..m.vertex_count() as u32 {
            let faces = m.faces_around_vertex(loom_mesh::VertexId(v));
            for (i, f) in faces.iter().enumerate() {
                for g in &faces[i + 1..] {
                    if m.faces()[f.0 as usize].smoothing_groups & m.faces()[g.0 as usize].smoothing_groups != 0 {
                        n += 1;
                    }
                }
            }
        }
        n
    };
    assert!(leaks(&s) > 0, "edge-only colouring reuses a group across the apex");
    set(&mut s, 0, "Prevent Indirect Smoothing", 1.0);
    assert_eq!(leaks(&s), 0, "no two faces around a vertex share a group");
    let Modifier::Smooth { .. } = &s.scene.node(a).unwrap().modifiers[0].modifier else { panic!() };
    assert!(s.scene.node(a).unwrap().modifiers[0].modifier.parameters().iter().any(|(n, _)| *n == "Prevent Indirect Smoothing"));
}

/// Taper > Center: moving the center shifts where the taper is measured
/// from; the gizmo itself stays where it is.
#[test]
fn taper_center_level_moves_only_the_center() {
    let mut s = AppState::new();
    let a = boxed(&mut s);
    execute(&mut s, Command::AddModifier { name: "Taper".into() }).unwrap();
    set(&mut s, 0, "Amount", 1.0);
    let top_y = |s: &AppState| {
        let m = s.scene.eval_mesh(a).value.mesh.clone();
        let ys: Vec<f32> = m.vertices().iter().filter(|v| v.position.z > 9.9).map(|v| v.position.y).collect();
        (ys.iter().cloned().fold(f32::INFINITY, f32::min), ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max))
    };
    let (lo, hi) = top_y(&s);
    assert!((lo + 10.0).abs() < 1e-4 && (hi - 10.0).abs() < 1e-4, "the top doubles: {lo} {hi}");
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 2 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 5.0, 0.0], system: None }).unwrap();
    let gizmo = s.scene.node(a).unwrap().modifiers[0].modifier.gizmo().unwrap().clone();
    assert!(gizmo.position.get().length() < 1e-6, "the gizmo did not move: {}", gizmo.position.get());
    let (lo, hi) = top_y(&s);
    assert!((lo + 15.0).abs() < 1e-4 && (hi - 5.0).abs() < 1e-4, "the taper scales about the moved center: {lo} {hi}");
    // Gizmo level: the gizmo moves and takes the center with it.
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 5.0, 0.0], system: None }).unwrap();
    let gizmo = s.scene.node(a).unwrap().modifiers[0].modifier.gizmo().unwrap().clone();
    assert!((gizmo.position.get().y - 5.0).abs() < 1e-4);
    let (lo, hi) = top_y(&s);
    assert!((lo + 20.0).abs() < 1e-4 && (hi - 0.0).abs() < 1e-4, "center now at y = 10: {lo} {hi}");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    let (lo, hi) = top_y(&s);
    assert!((lo + 10.0).abs() < 1e-4 && (hi - 10.0).abs() < 1e-4, "both moves undone: {lo} {hi}");
}

/// Shift-drag in the stack copies an entry, Ctrl-drag instances it
/// (the instanced pair shares parameters).
#[test]
fn copy_and_instance_a_modifier_within_the_stack() {
    let mut s = AppState::new();
    let a = boxed(&mut s);
    execute(&mut s, Command::AddModifier { name: "Bend".into() }).unwrap();
    execute(&mut s, Command::AddModifier { name: "Taper".into() }).unwrap();
    set(&mut s, 1, "Amount", 0.5);
    let undo_before = s.undo.len();
    // Copy Taper (1) to the top.
    execute(&mut s, Command::CopyModifier { from: 1, to: 2, instance: false }).unwrap();
    assert_eq!(names(&s, a), vec!["Bend", "Taper", "Taper"]);
    assert_eq!(s.undo.len(), undo_before + 1);
    assert_eq!(s.scene.node(a).unwrap().modifiers[2].instance_of, None);
    set(&mut s, 2, "Amount", 0.25);
    let amount = |s: &AppState, i: usize| match s.scene.resolved_modifier(a, i).unwrap() {
        Modifier::Taper { amount, .. } => amount.get(),
        _ => panic!(),
    };
    assert_eq!((amount(&s, 1), amount(&s, 2)), (0.5, 0.25), "a copy is independent");
    // Instance Bend (0) below everything else: index 0, above it the source moves up.
    execute(&mut s, Command::CopyModifier { from: 0, to: 0, instance: true }).unwrap();
    assert_eq!(names(&s, a), vec!["Bend", "Bend", "Taper", "Taper"]);
    let inst = s.scene.node(a).unwrap().modifiers[0].instance_of;
    assert_eq!(inst, Some((a, 1)), "the new entry instances the original, now at 1");
    set(&mut s, 0, "Angle", 30.0);
    let angle = |s: &AppState, i: usize| match s.scene.resolved_modifier(a, i).unwrap() {
        Modifier::Bend { angle, .. } => angle.get(),
        _ => panic!(),
    };
    assert_eq!((angle(&s, 0), angle(&s, 1)), (30.0, 30.0), "instances share their parameters");
    // Moving entries keeps the instance pointing at its source.
    execute(&mut s, Command::MoveModifier { from: 1, to: 3 }).unwrap();
    assert_eq!(names(&s, a), vec!["Bend", "Taper", "Taper", "Bend"]);
    assert_eq!(s.scene.node(a).unwrap().modifiers[0].instance_of, Some((a, 3)));
    // Deleting the source leaves the instance its own copy.
    execute(&mut s, Command::DeleteModifier { index: 3 }).unwrap();
    assert_eq!(s.scene.node(a).unwrap().modifiers[0].instance_of, None);
    assert_eq!(angle(&s, 0), 30.0);
    assert!(execute(&mut s, Command::CopyModifier { from: 7, to: 0, instance: false }).is_err());
}

#[test]
fn reference_clone_barrier_and_clipboard_ops() {
    let mut s = AppState::new();
    let a = boxed(&mut s);
    execute(&mut s, Command::AddModifier { name: "Bend".into() }).unwrap();
    // Create a Reference clone of `a`: `b` starts with `reference_barrier == Some(1)` and shares Bend.
    execute(&mut s, Command::CloneOptions { kind: loom_commands::CloneKind::Reference, copies: 1, name: Some("RefBox".into()), offset: None, rotate_deg: None, scale: None }).unwrap();
    let b = s.selection.iter().next().copied().unwrap();
    assert_eq!(s.scene.node(b).unwrap().reference_barrier, Some(1));
    assert_eq!(s.scene.node(b).unwrap().modifiers[0].instance_of, Some((a, 0)));

    // Adding a modifier to the Reference clone `b` while at the base object puts it ABOVE the barrier.
    execute(&mut s, Command::SelectStackEntry { index: None }).unwrap();
    execute(&mut s, Command::AddModifier { name: "Taper".into() }).unwrap();
    assert_eq!(names(&s, b), vec!["Bend", "Taper"]);
    assert_eq!(s.scene.node(b).unwrap().reference_barrier, Some(1));
    assert_eq!(names(&s, a), vec!["Bend"], "modifier above the barrier is local to the Reference clone");

    // Adding a modifier to the source `a` inserts it BELOW `b`'s barrier and shifts `b`'s barrier up.
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::AddModifier { name: "Twist".into() }).unwrap();
    assert_eq!(names(&s, a), vec!["Bend", "Twist"]);
    assert_eq!(names(&s, b), vec!["Bend", "Twist", "Taper"]);
    assert_eq!(s.scene.node(b).unwrap().reference_barrier, Some(2));

    // Clipboard Copy / Cut / Paste / Paste Instanced and Off in Renderer.
    execute(&mut s, Command::ToggleModifierInRenderer { index: 1, on: false }).unwrap();
    assert!(!s.scene.node(a).unwrap().modifiers[1].enabled_in_renderer);
    execute(&mut s, Command::CopyModifierToClipboard { index: 0 }).unwrap();
    execute(&mut s, Command::PasteModifier { instance: true }).unwrap();
    assert_eq!(names(&s, a), vec!["Bend", "Twist", "Bend"]);
    assert_eq!(s.scene.node(a).unwrap().modifiers[2].instance_of, Some((a, 0)));
    execute(&mut s, Command::CutModifier { index: 2 }).unwrap();
    assert_eq!(names(&s, a), vec!["Bend", "Twist"]);
}

