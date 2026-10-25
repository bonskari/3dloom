//! Skin Wrap through commands: Add by picking, the Control Vertices level
//! and its local parameters, Blend To Base Mesh, the mirror tools, Bake /
//! Retrieve, Reset and the display settings.

use loom_commands::{execute, AppState, Command, PickMode, PrimitiveKind, SelectOp};
use loom_scene::{Modifier, NodeId};

/// A 20×20 control plane (4×4 segments) and a finer plane half a unit above it, wrapped.
fn wrapped(s: &mut AppState) -> (NodeId, NodeId) {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [20.0, 20.0, 0.0] }).unwrap();
    let control = *s.selection.iter().next().unwrap();
    execute(s, Command::SetParameter { id: control.0, name: "Length Segs".into(), value: 4.0 }).unwrap();
    execute(s, Command::SetParameter { id: control.0, name: "Width Segs".into(), value: 4.0 }).unwrap();
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0, 0.0, 0.5], size: [16.0, 16.0, 0.0] }).unwrap();
    let hi = *s.selection.iter().next().unwrap();
    execute(s, Command::SetParameter { id: hi.0, name: "Length Segs".into(), value: 8.0 }).unwrap();
    execute(s, Command::SetParameter { id: hi.0, name: "Width Segs".into(), value: 8.0 }).unwrap();
    execute(s, Command::SkinWrapModifier).unwrap();
    (control, hi)
}

fn data(s: &AppState, id: NodeId) -> loom_scene::morph::SkinWrapData {
    let Modifier::SkinWrap(d) = &s.scene.node(id).unwrap().modifiers[0].modifier else { panic!() };
    (**d).clone()
}

/// Control vertex index nearest a world point.
fn cv_near(s: &AppState, x: f32, y: f32) -> u32 {
    loom_commands::morph::control_vertices(s).into_iter().min_by(|a, b| a.1.distance(glam::Vec3::new(x, y, 0.0)).total_cmp(&b.1.distance(glam::Vec3::new(x, y, 0.0)))).unwrap().0 .1
}

#[test]
fn add_picks_control_objects_and_the_control_vertex_level_sets_locals() {
    let mut s = AppState::new();
    let (control, hi) = wrapped(&mut s);
    execute(&mut s, Command::SkinWrapPickControl).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::SkinWrapControl));
    execute(&mut s, Command::PickHit { id: control.0 }).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::SkinWrapControl), "Add stays on until right-click or Add again");
    execute(&mut s, Command::SkinWrapPickControl).unwrap();
    assert_eq!(s.pick_mode, None);
    let d = data(&s, hi);
    assert_eq!(d.bases, vec![control]);
    assert_eq!(d.engine, 1, "Vertex Deformation");
    assert_eq!(d.locals[0].len(), 25);
    assert_eq!(s.scene.node(hi).unwrap().modifiers[0].modifier.sub_levels(), &["Control Vertices"]);
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    let centre = cv_near(&s, 0.0, 0.0);
    execute(&mut s, Command::SkinWrapSelectControlVerts { ids: vec![[0, centre]], op: SelectOp::Replace }).unwrap();
    assert_eq!(data(&s, hi).selection, vec![(0, centre)]);
    let before = data(&s, hi).influence_of(0, centre).len();
    assert!(before > 0);
    execute(&mut s, Command::SkinWrapSetLocal { name: "Local Scale Mult".into(), value: 2.0 }).unwrap();
    let after = data(&s, hi).influence_of(0, centre).len();
    assert!(after > before, "a larger volume reaches more points: {before} -> {after}");
    execute(&mut s, Command::SkinWrapSetLocal { name: "Local Strength".into(), value: -1.0 }).unwrap();
    let l = data(&s, hi).local(0, centre);
    assert_eq!((l.strength, l.scale), (-1.0, 2.0));
    for (n, v) in [("Local X", 0.5), ("Local Y", 0.6), ("Local Z", 0.7)] {
        execute(&mut s, Command::SkinWrapSetLocal { name: n.into(), value: v }).unwrap();
    }
    let l = data(&s, hi).local(0, centre);
    assert_eq!((l.x, l.y, l.z), (0.5, 0.6, 0.7));
    let st = execute(&mut s, Command::State).unwrap();
    assert_eq!(st["skin_wrap"]["selected_locals"][0]["strength"], -1.0);
    // At the Control Vertices level the move tool leaves the wrapped object where it is.
    execute(&mut s, Command::MoveSelection { delta: [5.0, 0.0, 0.0], system: None }).unwrap();
    assert!(s.scene.node(hi).unwrap().transform.position.get().x.abs() < 1e-6);
    // Reset: every local back to 1.0.
    execute(&mut s, Command::SkinWrapReset).unwrap();
    assert_eq!(data(&s, hi).local(0, centre), Default::default());
    assert!(execute(&mut s, Command::SkinWrapSetLocal { name: "Local W".into(), value: 1.0 }).is_err());
}

#[test]
fn options_rebind_with_the_reference_ranges() {
    let mut s = AppState::new();
    let (control, hi) = wrapped(&mut s);
    execute(&mut s, Command::SkinWrapAddBase { id: control.0 }).unwrap();
    execute(&mut s, Command::SkinWrapSetOption { name: "Falloff".into(), value: 0.0 }).unwrap();
    assert_eq!(data(&s, hi).falloff, 0.001, "Falloff bottoms out at 0.001 (rigid)");
    execute(&mut s, Command::SkinWrapSetOption { name: "Distance Influence".into(), value: 50.0 }).unwrap();
    assert_eq!(data(&s, hi).distance_influence, 10.0);
    execute(&mut s, Command::SkinWrapSetOption { name: "Face Limit".into(), value: 0.0 }).unwrap();
    assert_eq!(data(&s, hi).face_limit, 0, "Face Limit can be 0");
    execute(&mut s, Command::SkinWrapSetOption { name: "Blend To Base Mesh".into(), value: 1.0 }).unwrap();
    execute(&mut s, Command::SkinWrapSetOption { name: "Blend Distance".into(), value: 2.0 }).unwrap();
    let d = data(&s, hi);
    assert!(d.blend_to_base && d.blend_distance == 2.0);
    assert!(d.blend.iter().all(|b| *b > 0.0 && *b < 1.0), "half a unit off the control faces: partly blended");
    for (n, v) in [("Display Loops", 0.0), ("Display Axis", 0.0), ("Display Face Limit", 0.0), ("Display Unassigned Points", 1.0), ("Display Control Verts", 0.0)] {
        execute(&mut s, Command::SkinWrapSetOption { name: n.into(), value: v }).unwrap();
    }
    let d = data(&s, hi);
    assert!(!d.display.loops && !d.display.axis && !d.display.face_limit && d.display.unassigned && !d.display.control_verts);
}

#[test]
fn mirror_selected_copies_local_settings_across_the_plane() {
    let mut s = AppState::new();
    let (control, hi) = wrapped(&mut s);
    execute(&mut s, Command::SkinWrapAddBase { id: control.0 }).unwrap();
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    let right = cv_near(&s, 5.0, 5.0);
    let left = cv_near(&s, -5.0, 5.0);
    execute(&mut s, Command::SkinWrapSelectControlVerts { ids: vec![[0, right]], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinWrapSetLocal { name: "Local Strength".into(), value: 0.5 }).unwrap();
    execute(&mut s, Command::SkinWrapSetOption { name: "Show Mirror Data".into(), value: 1.0 }).unwrap();
    execute(&mut s, Command::SkinWrapSetOption { name: "Mirror Plane".into(), value: 0.0 }).unwrap();
    execute(&mut s, Command::SkinWrapSetOption { name: "Mirror Threshold".into(), value: 0.5 }).unwrap();
    execute(&mut s, Command::SkinWrapMirrorSelected).unwrap();
    assert_eq!(data(&s, hi).local(0, left).strength, 0.5, "the mirrored vertex gets the setting");
    assert_eq!(data(&s, hi).local(0, cv_near(&s, 5.0, -5.0)).strength, 1.0, "others keep theirs");
    // An offset plane finds nothing within the threshold.
    execute(&mut s, Command::SkinWrapSetOption { name: "Mirror Offset".into(), value: 1.0 }).unwrap();
    assert!(execute(&mut s, Command::SkinWrapMirrorSelected).is_err());
}

#[test]
fn bake_and_retrieve_control_verts_round_trip_through_the_control_object() {
    let mut s = AppState::new();
    let (control, hi) = wrapped(&mut s);
    execute(&mut s, Command::SkinWrapAddBase { id: control.0 }).unwrap();
    assert!(execute(&mut s, Command::SkinWrapRetrieveControlVerts).is_err(), "nothing baked yet");
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    let cv = cv_near(&s, 0.0, 0.0);
    execute(&mut s, Command::SkinWrapSelectControlVerts { ids: vec![[0, cv]], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinWrapSetLocal { name: "Local Strength".into(), value: 3.0 }).unwrap();
    execute(&mut s, Command::SkinWrapSetOption { name: "Falloff".into(), value: 2.0 }).unwrap();
    execute(&mut s, Command::SkinWrapBakeControlVerts).unwrap();
    let bake = s.scene.node(control).unwrap().skin_wrap_bake.clone().expect("baked onto the control object");
    assert_eq!(bake.locals[cv as usize].strength, 3.0);
    assert_eq!(bake.falloff, 2.0);
    execute(&mut s, Command::SkinWrapReset).unwrap();
    execute(&mut s, Command::SkinWrapSetOption { name: "Falloff".into(), value: 1.0 }).unwrap();
    assert_eq!(data(&s, hi).local(0, cv).strength, 1.0);
    execute(&mut s, Command::SkinWrapRetrieveControlVerts).unwrap();
    let d = data(&s, hi);
    assert_eq!(d.local(0, cv).strength, 3.0);
    assert_eq!(d.falloff, 2.0);
    // Undo takes the retrieve back.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(data(&s, hi).local(0, cv).strength, 1.0);
}

#[test]
fn unassigned_points_are_counted_for_the_display() {
    let mut s = AppState::new();
    let (control, _) = wrapped(&mut s);
    execute(&mut s, Command::SkinWrapAddBase { id: control.0 }).unwrap();
    execute(&mut s, Command::SkinWrapSetOption { name: "Threshold".into(), value: 0.1 }).unwrap();
    let st = execute(&mut s, Command::State).unwrap();
    assert_eq!(st["skin_wrap"]["unassigned"], 81, "no closest face within 0.1 of points half a unit up");
    execute(&mut s, Command::SkinWrapSetOption { name: "Threshold".into(), value: 5.0 }).unwrap();
    execute(&mut s, Command::SkinWrapSetOption { name: "Distance Influence".into(), value: 0.05 }).unwrap();
    let st = execute(&mut s, Command::State).unwrap();
    assert_eq!(st["skin_wrap"]["unassigned"], 0);
    assert!(st["skin_wrap"]["unweighted"].as_u64().unwrap() > 0, "closest face found, no control vertex reaches");
}
