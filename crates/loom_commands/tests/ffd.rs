use loom_commands::keymap::{parse_chord, Keymap};
use loom_commands::{execute, AppState, Command, PrimitiveKind};
use loom_scene::{Modifier, NodeId};

fn tall_box(s: &mut AppState) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0, 10.0, 40.0] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    execute(s, Command::SetParameter { id: id.0, name: "Height Segs".into(), value: 8.0 }).unwrap();
    id
}

#[test]
fn ffd_keymap_group_is_live_only_with_an_ffd_entry_active() {
    let km = Keymap::ffd();
    assert_eq!(km.lookup(parse_chord("Alt+Shift+C").unwrap()), Some("Switch To Control Point Level"));
    assert_eq!(km.lookup(parse_chord("Alt+Shift+L").unwrap()), Some("Switch To Lattice Level"));
    assert_eq!(km.lookup(parse_chord("Alt+Shift+S").unwrap()), Some("Switch To Set Volume Level"));
    assert_eq!(km.lookup(parse_chord("Alt+Shift+T").unwrap()), Some("Switch To Top Level"));
    let mut s = AppState::new();
    let a = tall_box(&mut s);
    assert_eq!(s.hotkey(parse_chord("Alt+Shift+C").unwrap()), None);
    execute(&mut s, Command::AddModifier { name: "FFD 3x3x3".into() }).unwrap();
    assert_eq!(s.hotkey(parse_chord("Alt+Shift+C").unwrap()), Some("Switch To Control Point Level"));
    execute(&mut s, Command::SwitchToControlPointLevel).unwrap();
    assert_eq!(s.scene.node(a).unwrap().modifiers[0].sub_level, 1);
    execute(&mut s, Command::SwitchToLatticeLevel).unwrap();
    assert_eq!(s.scene.node(a).unwrap().modifiers[0].sub_level, 2);
    execute(&mut s, Command::SwitchToSetVolumeLevel).unwrap();
    assert_eq!(s.scene.node(a).unwrap().modifiers[0].sub_level, 3);
    execute(&mut s, Command::SwitchToTopLevel).unwrap();
    assert_eq!(s.scene.node(a).unwrap().modifiers[0].sub_level, 0);
    assert!(loom_commands::keymap::keyboard_chart().iter().any(|r| r.group == "FFD" && r.action == "Switch To Lattice Level" && r.shortcut == "Alt+Shift+L"));
}

#[test]
fn control_point_move_deforms_nearby_and_undoes() {
    let mut s = AppState::new();
    let a = tall_box(&mut s);
    execute(&mut s, Command::AddModifier { name: "FFD 2x2x2".into() }).unwrap();
    let Modifier::FFD { dims, points, rest, .. } = &s.scene.node(a).unwrap().modifiers[0].modifier else { panic!("not an FFD") };
    assert_eq!(*dims, [2, 2, 2]);
    assert_eq!(points.len(), 8);
    // The lattice is fitted to the box: corners at its bounds.
    assert!(points.iter().any(|p| (*p - glam::Vec3::new(5.0, 5.0, 40.0)).length() < 1e-3), "{points:?}");
    assert_eq!(points, rest);
    let before = s.scene.eval_mesh(a).value.mesh.aabb();
    // Pick the top +X+Y corner point and move it up 10.
    execute(&mut s, Command::SwitchToControlPointLevel).unwrap();
    execute(&mut s, Command::SelectControlPoints { op: loom_commands::SelectOp::Replace, indices: vec![7] }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 10.0], system: None }).unwrap();
    let after = s.scene.eval_mesh(a).value.mesh.clone();
    let (_, hi) = after.aabb();
    assert!((hi.z - 50.0).abs() < 1e-3, "the corner vertex followed its control point: {hi:?}");
    // The opposite bottom corner (weight 0) stays; a top vertex near the moved corner moves more than one far from it.
    let column = |x: f32, y: f32| after.vertices().iter().map(|v| v.position).filter(move |p| (p.x - x).abs() < 1e-3 && (p.y - y).abs() < 1e-3).map(|p| p.z);
    assert!((column(-5.0, -5.0).fold(f32::INFINITY, f32::min) - 0.0).abs() < 1e-3);
    assert!(column(5.0, 5.0).fold(f32::NEG_INFINITY, f32::max) > column(-5.0, -5.0).fold(f32::NEG_INFINITY, f32::max) + 5.0);
    assert_eq!(s.scene.node(a).unwrap().object.mesh().aabb(), before, "the base object is untouched");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.eval_mesh(a).value.mesh.aabb(), before);
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert!((s.scene.eval_mesh(a).value.mesh.aabb().1.z - 50.0).abs() < 1e-3);
    // Reset Lattice puts the points back; Set Number of Points rebuilds the grid.
    execute(&mut s, Command::ResetLattice).unwrap();
    assert_eq!(s.scene.eval_mesh(a).value.mesh.aabb(), before);
    execute(&mut s, Command::AddModifier { name: "FFD(box)".into() }).unwrap();
    execute(&mut s, Command::SetNumberOfPoints { length: 3, width: 2, height: 5 }).unwrap();
    let Modifier::FFD { dims, points, .. } = &s.scene.node(a).unwrap().modifiers[1].modifier else { panic!() };
    assert_eq!((*dims, points.len()), ([3, 2, 5], 30));
    // Lattice level: moving the whole lattice drags the object with it.
    execute(&mut s, Command::SwitchToLatticeLevel).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [3.0, 0.0, 0.0], system: None }).unwrap();
    let (lo, _) = s.scene.eval_mesh(a).value.mesh.aabb();
    assert!((lo.x - (-5.0 + 3.0)).abs() < 1e-3, "{lo:?}");
    // Set Volume level moves points and their rest positions: no deformation.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::SwitchToSetVolumeLevel).unwrap();
    execute(&mut s, Command::SelectControlPoints { op: loom_commands::SelectOp::Replace, indices: vec![0] }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [-3.0, 0.0, 0.0], system: None }).unwrap();
    assert_eq!(s.scene.eval_mesh(a).value.mesh.aabb(), before);
    // Cylindrical lattice and Conform To Shape run without error.
    execute(&mut s, Command::AddModifier { name: "FFD(cyl)".into() }).unwrap();
    execute(&mut s, Command::ConformToShape).unwrap();
    assert!(s.scene.eval_mesh(a).warnings.is_empty());
    let st = execute(&mut s, Command::State).unwrap();
    assert!(st.to_string().contains("FFD(cyl)"), "{st}");
}
