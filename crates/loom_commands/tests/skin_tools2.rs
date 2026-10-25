//! Skin commands: Absolute / Relative, Paste to Multiple Bones, Painter
//! pressure and falloff graph, Weight Table exclusions and vertex sets,
//! Reset, Save / Load envelopes, cross sections, envelope end points,
//! Select Element, Display Projection, mirror envelopes, gizmos (Morph
//! Angle targets, lattice, copy / paste), Rigid Handles, Always Deform.

use loom_commands::{execute, AppState, Axis, Command, PrimitiveKind, SelectOp};
use loom_scene::{Modifier, NodeId};

/// A 20-unit cylinder along Z skinned to two 10-unit bones, at the Envelope level.
fn skinned_arm(s: &mut AppState) -> (NodeId, Vec<u64>) {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Cylinder, position: [0.0; 3], size: [2.0, 2.0, 20.0] }).unwrap();
    let arm = *s.selection.iter().next().unwrap();
    execute(s, Command::SetParameter { id: arm.0, name: "Height Segments".into(), value: 8.0 }).unwrap();
    let r = execute(s, Command::CreateBoneChain { points: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 10.0], [0.0, 0.0, 20.0], [0.0, 0.0, 21.0]], name: Some("Arm".into()) }).unwrap();
    let bones: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    execute(s, Command::SelectNodes { ids: vec![arm.0], op: SelectOp::Replace }).unwrap();
    execute(s, Command::SkinModifier).unwrap();
    execute(s, Command::SkinAddBones { ids: bones[..2].to_vec() }).unwrap();
    execute(s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    (arm, bones)
}

fn skin_data(s: &AppState, arm: NodeId) -> loom_scene::skin::SkinData {
    let n = s.scene.node(arm).unwrap();
    let Modifier::Skin(d) = &n.modifiers[0].modifier else { panic!("no Skin on the arm") };
    (**d).clone()
}

fn weight(s: &AppState, arm: NodeId, v: usize, b: u32) -> f32 {
    let d = skin_data(s, arm);
    let mesh = s.scene.node(arm).unwrap().object.display_output();
    let p = d.mesh_bind.transform_point3(mesh.vertices()[v].position);
    d.vertex_weights(v, p).into_iter().find(|(x, _)| *x == b).map(|x| x.1).unwrap_or(0.0)
}

fn verts_at_z(s: &AppState, arm: NodeId, z: f32) -> Vec<u32> {
    let mesh = s.scene.node(arm).unwrap().object.display_output();
    (0..mesh.vertex_count() as u32).filter(|v| (mesh.vertex_position(loom_mesh::VertexId(*v)).unwrap().z - z).abs() < 0.01).collect()
}

#[test]
fn the_vertices_filter_starts_off_and_envelope_mode_toggles_relative() {
    let mut s = AppState::new();
    assert!(!s.skin_filter_vertices, "Select > Vertices is off by default");
    assert!(s.skin_filter_envelopes && s.skin_filter_cross_sections);
    let (arm, bones) = skinned_arm(&mut s);
    execute(&mut s, Command::SkinSelectBone { id: bones[0] }).unwrap();
    execute(&mut s, Command::SkinSetEnvelopeMode { relative: true }).unwrap();
    assert!(skin_data(&s, arm).bones[0].envelope.relative);
    assert!(!skin_data(&s, arm).bones[1].envelope.relative);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(!skin_data(&s, arm).bones[0].envelope.relative);
}

#[test]
fn paste_to_multiple_bones_pastes_the_copied_envelope_onto_the_picked_bones() {
    let mut s = AppState::new();
    let (arm, bones) = skinned_arm(&mut s);
    assert!(execute(&mut s, Command::PasteToMultipleBonesSkin).is_err(), "nothing copied yet");
    execute(&mut s, Command::SkinSelectBone { id: bones[0] }).unwrap();
    execute(&mut s, Command::SkinSetEnvelope { inner_start: Some(4.0), inner_end: Some(4.0), outer_start: Some(7.0), outer_end: Some(7.0), falloff: Some(2), abs_effect: None, squash: None }).unwrap();
    execute(&mut s, Command::CopyEnvelopeSkin).unwrap();
    execute(&mut s, Command::PasteToMultipleBonesSkin).unwrap();
    assert_eq!(s.dialog, Some(loom_commands::Dialog::PasteToMultipleBones));
    execute(&mut s, Command::SkinPasteToBones { ids: vec![bones[1]] }).unwrap();
    let d = skin_data(&s, arm);
    assert_eq!(d.bones[1].envelope.inner, [4.0, 4.0]);
    assert_eq!(d.bones[1].envelope.outer, [7.0, 7.0]);
    assert_eq!(d.bones[1].envelope.falloff, 2);
    assert!(execute(&mut s, Command::SkinPasteToBones { ids: vec![9999] }).is_err());
}

#[test]
fn pen_pressure_runs_strength_from_min_to_max_and_the_graph_shapes_the_brush() {
    let mut s = AppState::new();
    let (arm, bones) = skinned_arm(&mut s);
    execute(&mut s, Command::SkinSelectBone { id: bones[1] }).unwrap();
    for (name, value) in [("Max Size", 3.0), ("Max Strength", 0.8), ("Min Strength", 0.2), ("Paint Blend Weights", 0.0), ("Falloff", 4.0), ("Enable Pressure Sensitivity", 1.0), ("Pressure Affects", 1.0)] {
        execute(&mut s, Command::SkinSetPainterOption { name: name.into(), value }).unwrap();
    }
    let near = verts_at_z(&s, arm, 2.5);
    let mesh = s.scene.node(arm).unwrap().object.display_output();
    let at = mesh.vertex_position(loom_mesh::VertexId(near[0])).unwrap();
    let w0 = weight(&s, arm, near[0] as usize, 1);
    execute(&mut s, Command::SkinPaintStroke { points: vec![at.to_array()], erase: false, pressure: Some(vec![0.0]) }).unwrap();
    let w1 = weight(&s, arm, near[0] as usize, 1);
    assert!((w1 - w0 - 0.2).abs() < 1e-4, "no pressure gives Min Strength: {w0} -> {w1}");
    execute(&mut s, Command::SkinPaintStroke { points: vec![at.to_array()], erase: false, pressure: Some(vec![0.5]) }).unwrap();
    let w2 = weight(&s, arm, near[0] as usize, 1);
    assert!((w2 - w1 - 0.5).abs() < 1e-4, "half pressure halfway: {w1} -> {w2}");
    // Pressure Affects Size: no pressure = Min Size (0) paints nothing.
    execute(&mut s, Command::SkinSetPainterOption { name: "Pressure Affects".into(), value: 2.0 }).unwrap();
    let r = execute(&mut s, Command::SkinPaintStroke { points: vec![(at + glam::Vec3::Z * 0.5).to_array()], erase: false, pressure: Some(vec![0.0]) }).unwrap();
    assert_eq!(r["painted"], 0);
    // The graph: zero strength at the centre and full at the rim.
    execute(&mut s, Command::SkinSetPainterCurve { points: vec![[0.0, 0.0], [1.0, 1.0]] }).unwrap();
    assert!((s.skin_painter.falloff_at(0.25) - 0.25).abs() < 1e-5);
    assert_eq!(s.skin_painter.falloff, loom_commands::skin::Painter::CUSTOM);
    // A falloff button puts its own curve back.
    execute(&mut s, Command::SkinSetPainterOption { name: "Falloff".into(), value: 0.0 }).unwrap();
    assert!((s.skin_painter.falloff_at(0.25) - 0.75).abs() < 1e-5);
    assert_eq!(s.skin_painter.curve.len(), 9);
    assert!(execute(&mut s, Command::SkinSetPainterCurve { points: vec![[0.0, 1.0]] }).is_err());
}

#[test]
fn exclusion_cells_and_vertex_sets() {
    let mut s = AppState::new();
    let (arm, bones) = skinned_arm(&mut s);
    let ring = verts_at_z(&s, arm, 10.0);
    let v = ring[0];
    assert!(weight(&s, arm, v as usize, 0) > 0.0);
    execute(&mut s, Command::SkinSetExclusion { vertex: v, bone: bones[0], excluded: true }).unwrap();
    assert!(skin_data(&s, arm).excluded[0].contains(&v));
    assert_eq!(weight(&s, arm, v as usize, 0), 0.0, "excluded from bone 0");
    execute(&mut s, Command::SkinSetExclusion { vertex: v, bone: bones[0], excluded: false }).unwrap();
    assert!(weight(&s, arm, v as usize, 0) > 0.0);
    // Vertex sets: named, picked back, deleted.
    assert!(execute(&mut s, Command::SkinAddVertexSet { name: "joint".into() }).is_err(), "needs a selection");
    execute(&mut s, Command::SkinSelectVertices { ids: ring.clone(), op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinAddVertexSet { name: "joint".into() }).unwrap();
    execute(&mut s, Command::SkinSelectVertices { ids: vec![], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinSelectVertexSet { name: "joint".into() }).unwrap();
    assert_eq!(loom_commands::skin::selection(&s), ring);
    execute(&mut s, Command::SkinRemoveVertexSet { name: "joint".into() }).unwrap();
    assert!(skin_data(&s, arm).vertex_sets.is_empty());
    assert!(execute(&mut s, Command::SkinSelectVertexSet { name: "joint".into() }).is_err());
}

#[test]
fn reset_selected_verts_bone_and_all_go_back_to_the_envelopes() {
    let mut s = AppState::new();
    let (arm, bones) = skinned_arm(&mut s);
    let ring = verts_at_z(&s, arm, 2.5);
    let top = verts_at_z(&s, arm, 17.5);
    execute(&mut s, Command::SkinSelectBone { id: bones[1] }).unwrap();
    let env = weight(&s, arm, ring[0] as usize, 1);
    execute(&mut s, Command::SkinSelectVertices { ids: ring.clone(), op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SetWeightTo075Skin).unwrap();
    assert!((weight(&s, arm, ring[0] as usize, 1) - 0.75).abs() < 1e-4);
    execute(&mut s, Command::SkinResetSelectedVerts).unwrap();
    assert!((weight(&s, arm, ring[0] as usize, 1) - env).abs() < 1e-4);
    // Reset Selected Bone: the vertices bone 1 weights.
    execute(&mut s, Command::SetWeightTo075Skin).unwrap();
    execute(&mut s, Command::SkinResetSelectedBone).unwrap();
    assert!(!skin_data(&s, arm).explicit[ring[0] as usize]);
    // Reset All Bones.
    execute(&mut s, Command::SkinSelectVertices { ids: top.clone(), op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SetWeightTo025Skin).unwrap();
    execute(&mut s, Command::SkinResetAllBones).unwrap();
    assert!(skin_data(&s, arm).explicit.iter().all(|x| !x));
}

#[test]
fn save_and_load_envelopes_match_bones_and_vertex_data() {
    let mut s = AppState::new();
    let (arm, bones) = skinned_arm(&mut s);
    execute(&mut s, Command::SkinSelectBone { id: bones[0] }).unwrap();
    execute(&mut s, Command::SkinSetEnvelope { inner_start: Some(3.5), inner_end: None, outer_start: None, outer_end: None, falloff: Some(1), abs_effect: None, squash: None }).unwrap();
    let ring = verts_at_z(&s, arm, 10.0);
    execute(&mut s, Command::SkinSelectVertices { ids: ring.clone(), op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SetWeightTo090Skin).unwrap();
    execute(&mut s, Command::SkinSelectVertices { ids: vec![ring[1]], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::ExcludeVertsSkin).unwrap();
    let dir = std::env::temp_dir().join(format!("loom-skin-env-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("arm.env").to_string_lossy().into_owned();
    let r = execute(&mut s, Command::SkinSaveEnvelopes { path: Some(path.clone()) }).unwrap();
    assert_eq!(r["bones"], 2);
    let saved = skin_data(&s, arm);
    // Change everything, then load back with bones swapped by hand and then by order.
    execute(&mut s, Command::SkinResetAllBones).unwrap();
    execute(&mut s, Command::SkinSetEnvelope { inner_start: Some(1.0), inner_end: None, outer_start: None, outer_end: None, falloff: Some(0), abs_effect: None, squash: None }).unwrap();
    let r = execute(&mut s, Command::SkinLoadEnvelopes { path: Some(path.clone()) }).unwrap();
    assert_eq!(s.dialog, Some(loom_commands::Dialog::LoadEnvelopes));
    assert_eq!(r["incoming"].as_array().unwrap().len(), 2);
    execute(&mut s, Command::SkinApplyLoadedEnvelopes { incoming: vec![0, 1], end_points: true, cross_sections: true, vertex_data: true, exclusions: true, by_index: true }).unwrap();
    let d = skin_data(&s, arm);
    assert_eq!(d.bones[0].envelope.inner[0], 3.5);
    assert_eq!(d.bones[0].envelope.falloff, 1);
    assert_eq!(d.excluded, saved.excluded);
    assert!((weight(&s, arm, ring[0] as usize, 0) - 0.9).abs() < 1e-4, "vertex data loaded");
    assert!(s.skin_ui.load.is_none());
    // By position, and onto swapped bones: bone 1 takes the file's bone 0.
    execute(&mut s, Command::SkinLoadEnvelopes { path: Some(path.clone()) }).unwrap();
    execute(&mut s, Command::SkinApplyLoadedEnvelopes { incoming: vec![-1, 0], end_points: false, cross_sections: false, vertex_data: false, exclusions: false, by_index: false }).unwrap();
    assert_eq!(skin_data(&s, arm).bones[1].envelope.inner[0], 3.5);
    assert!(execute(&mut s, Command::SkinLoadEnvelopes { path: Some(dir.join("missing.env").to_string_lossy().into_owned()) }).is_err());
    assert!(execute(&mut s, Command::SkinSaveEnvelopes { path: None }).is_ok());
    assert_eq!(s.dialog, Some(loom_commands::Dialog::SkinSaveEnvelopes));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cross_sections_are_added_on_the_bone_selected_resized_and_removed() {
    let mut s = AppState::new();
    let (arm, bones) = skinned_arm(&mut s);
    execute(&mut s, Command::SkinSelectBone { id: bones[0] }).unwrap();
    execute(&mut s, Command::AddCrossSectionSkin).unwrap();
    assert!(s.skin_ui.adding_cross_section, "Add waits for a click on the bone");
    execute(&mut s, Command::SkinAddCrossSectionAt { u: 0.25 }).unwrap();
    assert!(!s.skin_ui.adding_cross_section);
    assert_eq!(s.skin_ui.cross_section, Some((1, false)), "the new section is selected");
    assert_eq!(skin_data(&s, arm).bones[0].cross_sections.len(), 1);
    execute(&mut s, Command::SkinSelectCrossSection { index: 1, outer: true }).unwrap();
    execute(&mut s, Command::SkinSetCrossSectionRadius { radius: 9.0 }).unwrap();
    assert_eq!(skin_data(&s, arm).bones[0].cross_sections[0].2, 9.0);
    // The end sections are the envelope's own radii; they cannot be removed.
    execute(&mut s, Command::SkinSelectCrossSection { index: 2, outer: false }).unwrap();
    execute(&mut s, Command::SkinSetCrossSectionRadius { radius: 1.5 }).unwrap();
    assert_eq!(skin_data(&s, arm).bones[0].envelope.inner[1], 1.5);
    assert!(execute(&mut s, Command::RemoveCrossSectionSkin).is_err());
    execute(&mut s, Command::SkinSelectCrossSection { index: 1, outer: false }).unwrap();
    execute(&mut s, Command::RemoveCrossSectionSkin).unwrap();
    assert!(skin_data(&s, arm).bones[0].cross_sections.is_empty());
    assert!(execute(&mut s, Command::SkinSelectCrossSection { index: 5, outer: false }).is_err());
}

#[test]
fn envelope_end_points_move_with_the_move_tool_and_not_the_object() {
    let mut s = AppState::new();
    let (arm, bones) = skinned_arm(&mut s);
    execute(&mut s, Command::SkinSelectBone { id: bones[0] }).unwrap();
    let before = s.scene.node(arm).unwrap().transform.clone();
    execute(&mut s, Command::SelectEndPointSkin).unwrap();
    let pivot = s.selection_pivot();
    assert!((pivot - glam::Vec3::new(0.0, 0.0, 10.0)).length() < 1e-3, "the gizmo sits on the end point: {pivot:?}");
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 2.0], system: None }).unwrap();
    let e = &skin_data(&s, arm).bones[0].envelope;
    assert!((e.end - glam::Vec3::new(12.0, 0.0, 0.0)).length() < 1e-3 || (e.end.length() - 12.0).abs() < 1e-3, "{:?}", e.end);
    assert_eq!(s.scene.node(arm).unwrap().transform, before, "the object stays put");
    execute(&mut s, Command::SkinMoveEnvelopePoint { end: 0, position: [0.0, 0.0, 1.0] }).unwrap();
    assert!((skin_data(&s, arm).bones[0].envelope.start.length() - 1.0).abs() < 1e-3);
}

#[test]
fn select_element_brings_the_whole_element_and_projection_limits_sides() {
    let mut s = AppState::new();
    let (arm, _) = skinned_arm(&mut s);
    let n = s.scene.node(arm).unwrap().object.display_output().vertex_count() as u32;
    execute(&mut s, Command::SkinSelectElementToggle).unwrap();
    execute(&mut s, Command::SkinSelectVertices { ids: vec![0], op: SelectOp::Replace }).unwrap();
    assert_eq!(loom_commands::skin::selection(&s).len() as u32, n, "the cylinder is one element");
    execute(&mut s, Command::SkinSelectElementToggle).unwrap();
    // Mirror Mode on X with Display Projection Positive: only +X vertices select.
    execute(&mut s, Command::MirrorModeSkin).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Display Projection".into(), value: 1.0 }).unwrap();
    execute(&mut s, Command::SkinSelectVertices { ids: (0..n).collect(), op: SelectOp::Replace }).unwrap();
    let mesh = s.scene.node(arm).unwrap().object.display_output();
    assert!(loom_commands::skin::selection(&s).iter().all(|v| mesh.vertices()[*v as usize].position.x > 0.0));
    assert!(!loom_commands::skin::selection(&s).is_empty());
}

#[test]
fn mirror_sides_and_paste_green_to_blue_bones() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [20.0, 20.0, 20.0] }).unwrap();
    let body = *s.selection.iter().next().unwrap();
    let l = execute(&mut s, Command::CreateBoneChain { points: vec![[-8.0, 0.0, 5.0], [-8.0, 0.0, 15.0]], name: Some("Bone_L".into()) }).unwrap();
    let r = execute(&mut s, Command::CreateBoneChain { points: vec![[8.0, 0.0, 5.0], [8.0, 0.0, 15.0]], name: Some("Bone_R".into()) }).unwrap();
    let l: Vec<u64> = serde_json::from_value(l["nodes"].clone()).unwrap();
    let r: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![body.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinModifier).unwrap();
    execute(&mut s, Command::SkinAddBones { ids: vec![l[0], r[0]] }).unwrap();
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    execute(&mut s, Command::MirrorModeSkin).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Mirror Threshold".into(), value: 1.0 }).unwrap();
    let sides = loom_commands::skin::mirror_sides(&s).unwrap();
    assert!(sides.contains(&-1) && sides.contains(&1), "{sides:?}");
    execute(&mut s, Command::SkinSelectBone { id: l[0] }).unwrap();
    execute(&mut s, Command::SkinSetEnvelope { inner_start: Some(6.0), inner_end: Some(6.0), outer_start: Some(9.0), outer_end: Some(9.0), falloff: None, abs_effect: None, squash: None }).unwrap();
    execute(&mut s, Command::PasteGreenToBlueBones).unwrap();
    let d = skin_data(&s, body);
    assert_eq!(d.bones[1].envelope.inner, [6.0, 6.0]);
    assert_eq!(d.bones[1].envelope.outer, [9.0, 9.0]);
    // Manual Update: the sides stay as of the last Update.
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Manual Update".into(), value: 1.0 }).unwrap();
    execute(&mut s, Command::SkinMirrorUpdate).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Mirror Offset".into(), value: 100.0 }).unwrap();
    assert_eq!(loom_commands::skin::mirror_sides(&s).unwrap(), sides, "not updated until Update");
    execute(&mut s, Command::SkinMirrorUpdate).unwrap();
    assert_ne!(loom_commands::skin::mirror_sides(&s).unwrap(), sides);
}

#[test]
fn morph_angle_gizmo_takes_a_target_from_the_stack_at_the_bones_angle() {
    let mut s = AppState::new();
    let (arm, bones) = skinned_arm(&mut s);
    execute(&mut s, Command::SkinSelectBone { id: bones[1] }).unwrap();
    let joint = verts_at_z(&s, arm, 10.0);
    execute(&mut s, Command::SkinSelectVertices { ids: joint.clone(), op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinAddGizmo { kind: "morph".into() }).unwrap();
    let g = skin_data(&s, arm).gizmos[0].clone();
    assert_eq!(g.kind, loom_scene::skin::GizmoKind::MorphAngle);
    assert_eq!(g.keys.len(), 1, "the base target");
    assert_eq!(g.keys[0].name, "Base");
    assert_eq!(g.vertices.len(), joint.len());
    // Bend bone 2 by 90° and push the joint out with an XForm above the Skin.
    execute(&mut s, Command::SelectNodes { ids: vec![bones[1]], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: Axis::Y, degrees: 90.0, system: None }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![arm.0], op: SelectOp::Replace }).unwrap();
    let skinned: Vec<glam::Vec3> = s.scene.eval_mesh(arm).value.mesh.vertices().iter().map(|v| v.position).collect();
    // Target from another node: a copy of the skinned result with the joint pushed 1 along X.
    let mut target_mesh = s.scene.eval_mesh(arm).value.mesh.clone();
    for v in &joint {
        target_mesh.vertices_mut()[*v as usize].position.x += 1.0;
    }
    let mut node = loom_scene::Node::new("Target", loom_scene::ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(target_mesh)));
    node.transform = s.scene.node(arm).unwrap().transform.clone();
    let target = s.scene.add_node(node);
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    execute(&mut s, Command::SkinSelectGizmo { index: 0 }).unwrap();
    let r = execute(&mut s, Command::SkinGizmoAddFromNode { id: target.0 }).unwrap();
    assert_eq!(r["vertices"].as_u64().unwrap() as usize, joint.len());
    let g = skin_data(&s, arm).gizmos[0].clone();
    assert_eq!(g.keys.len(), 2);
    let morphed: Vec<glam::Vec3> = s.scene.eval_mesh(arm).value.mesh.vertices().iter().map(|v| v.position).collect();
    for v in &joint {
        let d = morphed[*v as usize] - skinned[*v as usize];
        assert!((d - glam::Vec3::X).length() < 1e-3, "vertex {v} moved {d:?}");
    }
    execute(&mut s, Command::SkinGizmoRenameTarget { key: 1, name: "Bent".into() }).unwrap();
    assert_eq!(skin_data(&s, arm).gizmos[0].keys[1].name, "Bent");
    execute(&mut s, Command::SkinGizmoDeleteTarget { key: 1 }).unwrap();
    assert!(execute(&mut s, Command::SkinGizmoDeleteTarget { key: 0 }).is_err(), "the last target stays");
    assert!(execute(&mut s, Command::SkinGizmoAddFromStack).is_ok());
}

#[test]
fn joint_gizmo_lattice_points_move_and_key_at_the_angle_copy_paste_and_options() {
    let mut s = AppState::new();
    let (arm, bones) = skinned_arm(&mut s);
    execute(&mut s, Command::SkinSelectBone { id: bones[1] }).unwrap();
    execute(&mut s, Command::SkinSelectVertices { ids: verts_at_z(&s, arm, 10.0), op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinAddGizmo { kind: "joint".into() }).unwrap();
    let g = skin_data(&s, arm).gizmos[0].clone();
    assert_eq!(g.lattice.as_ref().map(|l| l.count()), Some(27));
    assert!(g.name.starts_with("Joint Angle Deformer"));
    execute(&mut s, Command::SkinRenameGizmo { name: "Elbow".into() }).unwrap();
    assert_eq!(skin_data(&s, arm).gizmos[0].name, "Elbow");
    for (name, v) in [("Twist", 30.0), ("Use Bounding Volume", 1.0)] {
        execute(&mut s, Command::SkinSetGizmoOption { name: name.into(), value: v }).unwrap();
    }
    let g = skin_data(&s, arm).gizmos[0].clone();
    assert_eq!(g.twist, 30.0);
    assert!(g.bounding_volume);
    execute(&mut s, Command::SkinEditLatticeToggle).unwrap();
    assert!(s.skin_ui.edit_lattice);
    let rest: Vec<glam::Vec3> = s.scene.eval_mesh(arm).value.mesh.vertices().iter().map(|v| v.position).collect();
    // Move the whole lattice 1 along X: every vertex inside it follows.
    execute(&mut s, Command::SkinSelectLatticePoints { indices: (0..27).collect(), op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [1.0, 0.0, 0.0], system: None }).unwrap();
    let after: Vec<glam::Vec3> = s.scene.eval_mesh(arm).value.mesh.vertices().iter().map(|v| v.position).collect();
    let moved = (0..rest.len()).filter(|i| (after[*i] - rest[*i] - glam::Vec3::X).length() < 1e-2).count();
    assert!(moved >= 8, "{moved} vertices moved with the lattice");
    assert_eq!(s.scene.node(arm).unwrap().transform.position.get(), glam::Vec3::ZERO);
    execute(&mut s, Command::SkinSetLatticeOffset { key: 0, point: 3, offset: [0.0, 2.0, 0.0] }).unwrap();
    assert_eq!(skin_data(&s, arm).gizmos[0].keys[0].lattice[3], glam::Vec3::Y * 2.0);
    // Copy / paste: only onto a like gizmo.
    execute(&mut s, Command::SkinCopyGizmo).unwrap();
    execute(&mut s, Command::SkinAddGizmo { kind: "bulge".into() }).unwrap();
    assert!(execute(&mut s, Command::SkinPasteGizmo).is_err(), "joint onto bulge");
    execute(&mut s, Command::SkinAddGizmo { kind: "joint".into() }).unwrap();
    execute(&mut s, Command::SkinPasteGizmo).unwrap();
    assert_eq!(skin_data(&s, arm).gizmos[2].keys, skin_data(&s, arm).gizmos[0].keys);
    execute(&mut s, Command::SkinAngleKeysCurvesDialog).unwrap();
    assert_eq!(s.dialog, Some(loom_commands::Dialog::AngleKeysCurves));
}

#[test]
fn always_deform_back_on_binds_at_the_ref_frame_pose() {
    let mut s = AppState::new();
    let (arm, bones) = skinned_arm(&mut s);
    let rest: Vec<glam::Vec3> = s.scene.eval_mesh(arm).value.mesh.vertices().iter().map(|v| v.position).collect();
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Always Deform".into(), value: 0.0 }).unwrap();
    // Move the bones while off: the mesh does not follow.
    execute(&mut s, Command::SelectNodes { ids: vec![bones[0]], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [5.0, 0.0, 0.0], system: None }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![arm.0], op: SelectOp::Replace }).unwrap();
    let off: Vec<glam::Vec3> = s.scene.eval_mesh(arm).value.mesh.vertices().iter().map(|v| v.position).collect();
    assert!(rest.iter().zip(&off).all(|(a, b)| (*a - *b).length() < 1e-4));
    // Back on: the new place is the bind pose, so the mesh still sits where it was.
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Always Deform".into(), value: 1.0 }).unwrap();
    let on: Vec<glam::Vec3> = s.scene.eval_mesh(arm).value.mesh.vertices().iter().map(|v| v.position).collect();
    assert!(rest.iter().zip(&on).all(|(a, b)| (*a - *b).length() < 1e-3));
    // And from there it follows the bones again.
    execute(&mut s, Command::SelectNodes { ids: vec![bones[0]], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 3.0, 0.0], system: None }).unwrap();
    let moved: Vec<glam::Vec3> = s.scene.eval_mesh(arm).value.mesh.vertices().iter().map(|v| v.position).collect();
    assert!(rest.iter().zip(&moved).any(|(a, b)| (b.y - a.y - 3.0).abs() < 1e-3));
}

#[test]
fn rigid_handles_need_a_patch_and_remove_zero_weights_uses_the_limit() {
    let mut s = AppState::new();
    let (arm, bones) = skinned_arm(&mut s);
    execute(&mut s, Command::SkinSelectVertices { ids: vec![0], op: SelectOp::Replace }).unwrap();
    assert!(execute(&mut s, Command::SkinRigidHandlesToggle).is_err(), "a mesh has no handles");
    let ring = verts_at_z(&s, arm, 7.5);
    execute(&mut s, Command::SkinSelectBone { id: bones[1] }).unwrap();
    execute(&mut s, Command::SkinSelectVertices { ids: ring.clone(), op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SetWeightTo010Skin).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Remove Zero Limit".into(), value: 0.2 }).unwrap();
    execute(&mut s, Command::RemoveZeroWeightsSkin).unwrap();
    assert_eq!(weight(&s, arm, ring[0] as usize, 1), 0.0, "0.1 is below the 0.2 limit");
    assert!((weight(&s, arm, ring[0] as usize, 0) - 1.0).abs() < 1e-4);
    // The new Advanced Parameters set and read back.
    for name in ["Rigid Vertices (All)", "Rigid Patch Handles (All)", "Back Transform Vertices", "Ignore Bone Scale", "Fast Updates", "Update On Mouse Up", "Animatable Envelopes", "Show All Vertices", "Show Hidden Vertices"] {
        execute(&mut s, Command::SetModifierParameter { index: 0, name: name.into(), value: 1.0 }).unwrap();
    }
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Ref. Frame".into(), value: 12.0 }).unwrap();
    let d = skin_data(&s, arm);
    assert!(d.rigid_all && d.rigid_patch_handles_all && d.back_transform && d.ignore_bone_scale && d.fast_updates && d.update_on_mouse_up && d.animatable_envelopes && d.display.all_vertices && d.display.show_hidden_vertices);
    assert_eq!(d.ref_frame, 12.0);
}

#[test]
fn rigid_handles_on_an_editable_patch() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0, 10.0, 10.0] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::ConvertToEditablePatch).unwrap();
    let r = execute(&mut s, Command::CreateBoneChain { points: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 10.0], [0.0, 0.0, 11.0]], name: None }).unwrap();
    let bones: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![id.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinModifier).unwrap();
    execute(&mut s, Command::SkinAddBones { ids: vec![bones[0]] }).unwrap();
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    let patch = loom_commands::skin::input_patch(&s).expect("the Skin sees the patch");
    let d = skin_data(&s, id);
    assert_eq!(d.weights.len(), patch.verts.len() + patch.vecs.len());
    execute(&mut s, Command::SkinSelectVertices { ids: vec![0, 1], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinRigidHandlesToggle).unwrap();
    assert!(skin_data(&s, id).rigid_handles[0] && skin_data(&s, id).rigid_handles[1]);
    let (_, world) = loom_commands::skin::vertex_world_positions(&s).unwrap();
    assert_eq!(world.len(), patch.verts.len() + patch.vecs.len(), "the viewport shows vertices and handles");
}

#[test]
fn zoom_extents_selected_frames_the_skin_selection_and_the_highlight() {
    let mut s = AppState::new();
    let (arm, _) = skinned_arm(&mut s);
    let top = verts_at_z(&s, arm, 20.0);
    execute(&mut s, Command::SkinSelectVertices { ids: top, op: SelectOp::Replace }).unwrap();
    let (lo, hi) = loom_commands::skin::focus_bounds(&s).unwrap();
    assert!(lo.z > 19.9 && hi.z < 20.1, "{lo:?} {hi:?}");
    execute(&mut s, Command::ZoomExtentsSelected).unwrap();
    assert!((s.active().camera.target.z - 20.0).abs() < 0.5, "{:?}", s.active().camera.target);
    execute(&mut s, Command::SkinHighlightVertex { vertex: Some(3) }).unwrap();
    assert_eq!(s.skin_ui.highlight, Some(3));
}
