//! Skin Morph through commands: the Points level, selection tools, Edit
//! mode with the transform tools, Beginner Mode, Clear / Remove Verts,
//! Joint Type, External Mesh, Paste Mirror, Pick Bone and the options.

use glam::Vec3;
use loom_commands::{execute, AppState, Axis, Command, PrimitiveKind, SelectOp};
use loom_scene::{Modifier, NodeId};

/// A 20-unit cylinder along Z, two bones up its middle, a Skin Morph bound to the forearm.
fn arm(s: &mut AppState) -> (NodeId, Vec<u64>) {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Cylinder, position: [0.0; 3], size: [2.0, 2.0, 20.0] }).unwrap();
    let arm = *s.selection.iter().next().unwrap();
    execute(s, Command::SetParameter { id: arm.0, name: "Height Segments".into(), value: 8.0 }).unwrap();
    let r = execute(s, Command::CreateBoneChain { points: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 10.0], [0.0, 0.0, 20.0], [0.0, 0.0, 21.0]], name: None }).unwrap();
    let bones: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    execute(s, Command::SelectNodes { ids: vec![arm.0], op: SelectOp::Replace }).unwrap();
    execute(s, Command::SkinMorphModifier).unwrap();
    execute(s, Command::SkinMorphAddBones { ids: vec![bones[1]] }).unwrap();
    (arm, bones)
}

fn data(s: &AppState, id: NodeId) -> loom_scene::morph::SkinMorphData {
    let Modifier::SkinMorph(d) = &s.scene.node(id).unwrap().modifiers[0].modifier else { panic!() };
    (**d).clone()
}

fn bend(s: &mut AppState, arm: NodeId, bone: u64, degrees: f32) {
    execute(s, Command::SelectNodes { ids: vec![bone], op: SelectOp::Replace }).unwrap();
    execute(s, Command::RotateSelection { axis: Axis::X, degrees, system: None }).unwrap();
    execute(s, Command::SelectNodes { ids: vec![arm.0], op: SelectOp::Replace }).unwrap();
}

fn vertex_at(s: &AppState, id: NodeId, v: usize) -> Vec3 {
    s.scene.object_matrix(id).transform_point3(s.scene.eval_mesh(id).value.mesh.vertices()[v].position)
}

#[test]
fn points_level_selects_and_the_selection_tools_grow_shrink_loop_and_ring() {
    let mut s = AppState::new();
    let (id, _) = arm(&mut s);
    assert_eq!(s.scene.node(id).unwrap().modifiers[0].modifier.sub_levels(), &["Points"]);
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    // A vertex on the side of the cylinder, away from the caps.
    let mesh = s.scene.evaluated(id, Some(0)).value.mesh.clone();
    let side = mesh.vertices().iter().position(|v| (v.position.z - 10.0).abs() < 1e-3).unwrap() as u32;
    execute(&mut s, Command::SkinMorphSelectPoints { ids: vec![side], op: SelectOp::Replace }).unwrap();
    assert_eq!(data(&s, id).selection, vec![side]);
    execute(&mut s, Command::SkinMorphSelectionTool { tool: "Grow".into() }).unwrap();
    let grown = data(&s, id).selection.len();
    assert!(grown >= 5, "Grow adds the neighbours: {grown}");
    execute(&mut s, Command::SkinMorphSelectionTool { tool: "Shrink".into() }).unwrap();
    assert_eq!(data(&s, id).selection, vec![side], "Shrink takes the outer ring back");
    // Two vertices of one ring edge: Loop runs round the cylinder, Ring along it.
    let next = loom_mesh::select_ops::vertex_neighbours(&mesh, loom_mesh::VertexId(side)).into_iter().find(|n| (mesh.vertices()[n.0 as usize].position.z - 10.0).abs() < 1e-3).unwrap().0;
    execute(&mut s, Command::SkinMorphSelectPoints { ids: vec![side, next], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinMorphSelectionTool { tool: "Loop".into() }).unwrap();
    let ring_z: Vec<f32> = data(&s, id).selection.iter().map(|v| mesh.vertices()[*v as usize].position.z).collect();
    assert_eq!(ring_z.len(), 18, "Loop: the whole circle of 18 sides");
    assert!(ring_z.iter().all(|z| (z - 10.0).abs() < 1e-3));
    execute(&mut s, Command::SkinMorphSelectPoints { ids: vec![side, next], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinMorphSelectionTool { tool: "Ring".into() }).unwrap();
    assert!(data(&s, id).selection.len() > 2, "Ring: the parallel edges' vertices along the cylinder");
    assert!(execute(&mut s, Command::SkinMorphSelectionTool { tool: "Spiral".into() }).is_err());
    execute(&mut s, Command::SkinMorphSelectPoints { ids: vec![side], op: SelectOp::Subtract }).unwrap();
    assert!(!data(&s, id).selection.contains(&side));
}

#[test]
fn edit_mode_poses_the_bone_and_moving_points_makes_the_morph() {
    let mut s = AppState::new();
    let (id, bones) = arm(&mut s);
    bend(&mut s, id, bones[1], 90.0);
    let r = execute(&mut s, Command::SkinMorphCreateMorph { bone: bones[1] }).unwrap();
    assert_eq!(r["morph"], 0);
    // Back to straight: the morph is out of range.
    bend(&mut s, id, bones[1], -90.0);
    let straight = s.scene.world_matrix(NodeId(bones[1]));
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    let top = s.scene.evaluated(id, Some(0)).value.mesh.vertices().iter().position(|v| (v.position.z - 15.0).abs() < 1e-3).unwrap() as u32;
    execute(&mut s, Command::SkinMorphSelectPoints { ids: vec![top], op: SelectOp::Replace }).unwrap();
    // Outside Edit (Beginner Mode on), the move tool changes nothing.
    let before = vertex_at(&s, id, top as usize);
    execute(&mut s, Command::MoveSelection { delta: [0.0, 3.0, 0.0], system: None }).unwrap();
    assert!((vertex_at(&s, id, top as usize) - before).length() < 1e-4, "points move only in Edit mode");
    assert!(data(&s, id).morphs[0].deltas.is_empty());
    // Edit: the bone goes to the morph's pose; a move makes the delta.
    execute(&mut s, Command::SkinMorphEdit { morph: 0 }).unwrap();
    let posed = s.scene.world_matrix(NodeId(bones[1]));
    assert!(!posed.abs_diff_eq(straight, 1e-3), "Edit poses the bone at the morph");
    assert_eq!(data(&s, id).editing, Some(0));
    let before = vertex_at(&s, id, top as usize);
    execute(&mut s, Command::MoveSelection { delta: [0.0, 3.0, 0.0], system: Some(loom_commands::CoordSystem::World) }).unwrap();
    let after = vertex_at(&s, id, top as usize);
    assert!(((after - before) - Vec3::new(0.0, 3.0, 0.0)).length() < 1e-3, "the point follows the move: {:?}", after - before);
    assert_eq!(data(&s, id).morphs[0].deltas.len(), 1);
    assert!(s.scene.node(id).unwrap().transform.position.get().length() < 1e-6, "the object itself stays");
    // Leaving Edit restores the bone; at the straight pose the morph is gone.
    execute(&mut s, Command::SkinMorphEdit { morph: 0 }).unwrap();
    assert!(s.scene.world_matrix(NodeId(bones[1])).abs_diff_eq(straight, 1e-3), "the bone gets its transform back");
    assert_eq!(data(&s, id).editing, None);
    bend(&mut s, id, bones[1], 90.0);
    let w = execute(&mut s, Command::State).unwrap()["skin_morph"]["morphs"][0]["weight"].as_f64().unwrap();
    assert!((w - 1.0).abs() < 1e-3, "full at its pose: {w}");
    // Clear Verts keeps the vertex with no delta; Remove Verts drops it.
    execute(&mut s, Command::SkinMorphClearVerts { morph: 0 }).unwrap();
    assert_eq!(data(&s, id).morphs[0].deltas, vec![(top, Vec3::ZERO)]);
    execute(&mut s, Command::SkinMorphRemoveVerts { morph: 0 }).unwrap();
    assert!(data(&s, id).morphs[0].deltas.is_empty());
}

#[test]
fn with_beginner_mode_off_moving_points_creates_a_morph_on_the_fly() {
    let mut s = AppState::new();
    let (id, bones) = arm(&mut s);
    assert!(data(&s, id).options.beginner_mode, "Beginner Mode starts on");
    execute(&mut s, Command::SkinMorphSetOption { name: "Beginner Mode".into(), value: 0.0 }).unwrap();
    bend(&mut s, id, bones[1], 45.0);
    execute(&mut s, Command::SkinMorphSelect { bone: Some(bones[1]), morph: None }).unwrap();
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    execute(&mut s, Command::SkinMorphSelectPoints { ids: vec![3], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [1.0, 0.0, 0.0], system: Some(loom_commands::CoordSystem::World) }).unwrap();
    let d = data(&s, id);
    assert_eq!(d.morphs.len(), 1, "a morph made on the fly");
    assert_eq!(d.morphs[0].deltas.len(), 1);
    // A second move goes into the same morph (it is at 100%).
    execute(&mut s, Command::MoveSelection { delta: [1.0, 0.0, 0.0], system: Some(loom_commands::CoordSystem::World) }).unwrap();
    assert_eq!(data(&s, id).morphs.len(), 1);
}

#[test]
fn soft_selection_spreads_a_move_over_nearby_points() {
    let mut s = AppState::new();
    let (id, bones) = arm(&mut s);
    execute(&mut s, Command::SkinMorphCreateMorph { bone: bones[1] }).unwrap();
    execute(&mut s, Command::SkinMorphSetOption { name: "Use Soft Selection".into(), value: 1.0 }).unwrap();
    execute(&mut s, Command::SkinMorphSetOption { name: "Radius".into(), value: 6.0 }).unwrap();
    execute(&mut s, Command::SkinMorphSetCurve { graph: "Soft Selection".into(), morph: None, points: vec![[0.0, 0.0], [1.0, 1.0]] }).unwrap();
    let d = data(&s, id);
    assert!(d.soft.on && d.soft.radius == 6.0 && d.soft.curve == vec![[0.0, 0.0], [1.0, 1.0]]);
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    let top = s.scene.evaluated(id, Some(0)).value.mesh.vertices().iter().position(|v| (v.position.z - 15.0).abs() < 1e-3).unwrap() as u32;
    execute(&mut s, Command::SkinMorphSelectPoints { ids: vec![top], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinMorphEdit { morph: 0 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 1.0], system: Some(loom_commands::CoordSystem::World) }).unwrap();
    assert!(data(&s, id).morphs[0].deltas.len() > 3, "soft-selected points join the morph");
    execute(&mut s, Command::SkinMorphResetGraph).unwrap();
    assert_eq!(data(&s, id).soft.curve, loom_scene::morph::default_curve());
}

#[test]
fn joint_type_external_mesh_and_reload_target() {
    let mut s = AppState::new();
    let (id, bones) = arm(&mut s);
    execute(&mut s, Command::SkinMorphSetJointType { bone: bones[1], planar: true }).unwrap();
    assert!(data(&s, id).bones[0].planar);
    execute(&mut s, Command::SkinMorphCreateMorph { bone: bones[1] }).unwrap();
    // A copy of the arm with one vertex pushed out is the external target.
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Cylinder, position: [30.0, 0.0, 0.0], size: [2.0, 2.0, 20.0] }).unwrap();
    let target = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SetParameter { id: target.0, name: "Height Segments".into(), value: 8.0 }).unwrap();
    execute(&mut s, Command::SetParameter { id: target.0, name: "Radius".into(), value: 3.0 }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![id.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinMorphSetExternal { morph: 0, node: Some(target.0) }).unwrap();
    assert_eq!(data(&s, id).morphs[0].external, Some(target));
    execute(&mut s, Command::SkinMorphReloadTarget { morph: 0 }).unwrap();
    let all = data(&s, id).morphs[0].deltas.len();
    assert!(all > 20, "every side vertex differs (radius 3 vs 2): {all}");
    // Reload Only Selected Verts: just the selection.
    execute(&mut s, Command::SkinMorphRemoveVerts { morph: 0 }).unwrap();
    execute(&mut s, Command::SkinMorphSetMorph { morph: 0, name: "Reload Only Selected Verts".into(), value: 1.0 }).unwrap();
    let mut d = data(&s, id);
    let side: Vec<u32> = d.morphs[0].deltas.iter().take(2).map(|(v, _)| *v).collect();
    execute(&mut s, Command::SkinMorphSelectPoints { ids: side.clone(), op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinMorphRemoveVerts { morph: 0 }).unwrap();
    d = data(&s, id);
    assert_eq!(d.morphs[0].deltas.len(), all - 2);
    execute(&mut s, Command::SkinMorphReloadTarget { morph: 0 }).unwrap();
    assert_eq!(data(&s, id).morphs[0].deltas.len(), all, "the two selected vertices come back");
    // The External Mesh button picks in the viewport.
    execute(&mut s, Command::SkinMorphSetExternal { morph: 0, node: None }).unwrap();
    execute(&mut s, Command::SkinMorphSelect { bone: None, morph: Some(0) }).unwrap();
    execute(&mut s, Command::SkinMorphPickExternal).unwrap();
    assert_eq!(s.pick_mode, Some(loom_commands::PickMode::SkinMorphExternal));
    execute(&mut s, Command::PickHit { id: target.0 }).unwrap();
    assert_eq!(data(&s, id).morphs[0].external, Some(target));
    assert_eq!(s.pick_mode, None);
    // Falloff 4 is Custom, with its own graph.
    execute(&mut s, Command::SkinMorphSetMorph { morph: 0, name: "Falloff".into(), value: 4.0 }).unwrap();
    execute(&mut s, Command::SkinMorphSetCurve { graph: "Falloff".into(), morph: Some(0), points: vec![[1.0, 1.0], [0.0, 0.2]] }).unwrap();
    let d = data(&s, id);
    assert_eq!(d.morphs[0].falloff, 4);
    assert_eq!(d.morphs[0].curve, vec![[0.0, 0.2], [1.0, 1.0]], "points are kept in x order");
}

#[test]
fn pick_bone_adds_bones_until_turned_off() {
    let mut s = AppState::new();
    let (id, bones) = arm(&mut s);
    execute(&mut s, Command::SkinMorphPickBone).unwrap();
    assert_eq!(s.pick_mode, Some(loom_commands::PickMode::SkinMorphBone));
    execute(&mut s, Command::PickHit { id: bones[0] }).unwrap();
    assert_eq!(s.pick_mode, Some(loom_commands::PickMode::SkinMorphBone), "Pick Bone stays on for more bones");
    assert_eq!(data(&s, id).bones.len(), 2);
    execute(&mut s, Command::SkinMorphPickBone).unwrap();
    assert_eq!(s.pick_mode, None);
}

#[test]
fn paste_mirror_copies_a_bones_morphs_to_its_mirror_bone() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [20.0, 4.0, 10.0] }).unwrap();
    let body = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SetParameter { id: body.0, name: "Width Segs".into(), value: 4.0 }).unwrap();
    let r = execute(&mut s, Command::CreateBoneChain { points: vec![[5.0, 0.0, 0.0], [5.0, 0.0, 8.0], [5.0, 0.0, 9.0]], name: None }).unwrap();
    let right: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    let r = execute(&mut s, Command::CreateBoneChain { points: vec![[-5.0, 0.0, 0.0], [-5.0, 0.0, 8.0], [-5.0, 0.0, 9.0]], name: None }).unwrap();
    let left: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![body.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinMorphModifier).unwrap();
    execute(&mut s, Command::SkinMorphAddBones { ids: vec![right[0], left[0]] }).unwrap();
    execute(&mut s, Command::SkinMorphCreateMorph { bone: right[0] }).unwrap();
    let mesh = s.scene.node(body).unwrap().object.display_output();
    let world = s.scene.object_matrix(body);
    let pos: Vec<Vec3> = mesh.vertices().iter().map(|v| world.transform_point3(v.position)).collect();
    let v = pos.iter().position(|p| p.x > 9.0).unwrap();
    let mirror_v = pos.iter().position(|p| (*p - Vec3::new(-pos[v].x, pos[v].y, pos[v].z)).length() < 1e-3).unwrap();
    // A delta pushing the right side outward (+X in world).
    let bw = s.scene.world_matrix(NodeId(right[0]));
    let local = bw.inverse().transform_vector3(Vec3::X * 2.0);
    execute(&mut s, Command::SkinMorphSetVertexDelta { morph: 0, vertex: v as u32, delta: local.to_array() }).unwrap();
    assert_eq!(data(&s, body).mirror.axis, 0, "Mirror Plane defaults to X");
    execute(&mut s, Command::SkinMorphPasteMirror { bone: right[0] }).unwrap();
    let d = data(&s, body);
    assert_eq!(d.morphs.len(), 2);
    let m = &d.morphs[1];
    assert_eq!(d.bones[m.bone].node, NodeId(left[0]));
    assert_eq!(m.deltas.len(), 1);
    assert_eq!(m.deltas[0].0 as usize, mirror_v, "the delta lands on the mirrored vertex");
    let world_delta = s.scene.world_matrix(NodeId(left[0])).transform_vector3(m.deltas[0].1);
    assert!((world_delta - Vec3::new(-2.0, 0.0, 0.0)).length() < 1e-3, "pushes the left side outward: {world_delta:?}");
    // No bone across the plane within the threshold: an error.
    execute(&mut s, Command::SkinMorphSetOption { name: "Mirror Offset".into(), value: 3.0 }).unwrap();
    assert!(execute(&mut s, Command::SkinMorphPasteMirror { bone: right[0] }).is_err());
    for name in ["Show Mirror Plane", "Preview Bone", "Preview Vertices", "Show Driver Bone Matrix", "Show Morph Bone Matrix", "Show Current Angle"] {
        execute(&mut s, Command::SkinMorphSetOption { name: name.into(), value: 1.0 }).unwrap();
    }
    execute(&mut s, Command::SkinMorphSetOption { name: "Show Edges".into(), value: 0.0 }).unwrap();
    let st = execute(&mut s, Command::State).unwrap();
    assert_eq!(st["skin_morph"]["mirror"]["show_plane"], true);
    assert_eq!(st["skin_morph"]["options"]["show_edges"], false);
    assert_eq!(st["skin_morph"]["options"]["show_current_angle"], true);
}
