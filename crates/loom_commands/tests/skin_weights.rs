//! Skin weighting commands beyond the envelopes: Weight Tool scale / paste
//! tolerance, Weight Table cells, bone name field, Envelope Visibility,
//! Paint Weights.

use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp};
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

/// The weight bone `b` (list index) gives vertex `v` now.
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
fn weight_tool_scale_weight_uses_its_own_field_and_five_percent_steps() {
    let mut s = AppState::new();
    let (arm, bones) = skinned_arm(&mut s);
    // The joint ring: both bones weight it, so one can give weight to the other.
    let ring = verts_at_z(&s, arm, 10.0);
    execute(&mut s, Command::SkinSelectBone { id: bones[1] }).unwrap();
    execute(&mut s, Command::SkinSelectVertices { ids: ring.clone(), op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SetWeightTo05Skin).unwrap();
    let v = ring[0] as usize;
    assert!((weight(&s, arm, v, 1) - 0.5).abs() < 1e-4);
    // Scale Weight field defaults to 0.95, apart from the Set Weight field (0.5).
    assert_eq!(s.skin_scale_weight, 0.95);
    execute(&mut s, Command::ScaleWeightCustomSkin).unwrap();
    assert!((weight(&s, arm, v, 1) - 0.475).abs() < 1e-4, "{}", weight(&s, arm, v, 1));
    execute(&mut s, Command::ScaleWeightUpSkin).unwrap();
    assert!((weight(&s, arm, v, 1) - 0.475 * 1.05).abs() < 1e-4);
    execute(&mut s, Command::ScaleWeightDownSkin).unwrap();
    assert!((weight(&s, arm, v, 1) - 0.475 * 1.05 * 0.95).abs() < 1e-4);
    execute(&mut s, Command::SkinSetScaleWeight { value: 2.0 }).unwrap();
    execute(&mut s, Command::ScaleWeightCustomSkin).unwrap();
    assert!((weight(&s, arm, v, 1) - 0.475 * 1.05 * 0.95 * 2.0).abs() < 1e-4);
    // The other bone takes the rest.
    assert!((weight(&s, arm, v, 0) + weight(&s, arm, v, 1) - 1.0).abs() < 1e-4);
}

#[test]
fn preset_weight_buttons_set_the_current_bone_and_undo_restores() {
    let mut s = AppState::new();
    let (arm, bones) = skinned_arm(&mut s);
    let ring = verts_at_z(&s, arm, 0.0);
    execute(&mut s, Command::SkinSelectBone { id: bones[1] }).unwrap();
    execute(&mut s, Command::SkinSelectVertices { ids: ring.clone(), op: SelectOp::Replace }).unwrap();
    for (cmd, w) in [(Command::SetWeightTo010Skin, 0.1), (Command::SetWeightTo025Skin, 0.25), (Command::SetWeightTo075Skin, 0.75), (Command::SetWeightTo090Skin, 0.9), (Command::SetWeightTo10Skin, 1.0), (Command::SetWeightTo00Skin, 0.0)] {
        execute(&mut s, cmd).unwrap();
        for v in &ring {
            assert!((weight(&s, arm, *v as usize, 1) - w).abs() < 1e-4, "vertex {v} weight {w}");
        }
    }
    for _ in 0..6 {
        execute(&mut s, Command::UndoSceneOperation).unwrap();
    }
    assert!(!skin_data(&s, arm).explicit[ring[0] as usize], "six undos: back to the envelopes");
}

#[test]
fn paste_pos_fills_only_vertices_within_the_tolerance() {
    let mut s = AppState::new();
    let (arm, bones) = skinned_arm(&mut s);
    let top = verts_at_z(&s, arm, 10.0);
    let bottom = verts_at_z(&s, arm, 0.0);
    execute(&mut s, Command::SkinSelectBone { id: bones[1] }).unwrap();
    execute(&mut s, Command::SkinSelectVertices { ids: top.clone(), op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SetWeightTo025Skin).unwrap();
    execute(&mut s, Command::CopyWeightsSkin).unwrap();
    // The bottom ring is 10 units away: outside the 0.1 tolerance, nothing changes.
    execute(&mut s, Command::SkinSelectVertices { ids: bottom.clone(), op: SelectOp::Replace }).unwrap();
    let before = weight(&s, arm, bottom[0] as usize, 1);
    execute(&mut s, Command::PasteWeightsByPosSkin).unwrap();
    assert_eq!(weight(&s, arm, bottom[0] as usize, 1), before);
    execute(&mut s, Command::SkinSetPasteTolerance { value: 25.0 }).unwrap();
    execute(&mut s, Command::PasteWeightsByPosSkin).unwrap();
    assert!((weight(&s, arm, bottom[0] as usize, 1) - 0.25).abs() < 1e-4);
}

#[test]
fn weight_table_cells_set_one_bone_on_given_vertices_without_touching_the_selection() {
    let mut s = AppState::new();
    let (arm, bones) = skinned_arm(&mut s);
    let ring = verts_at_z(&s, arm, 10.0);
    execute(&mut s, Command::SkinSelectVertices { ids: vec![ring[0]], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinSelectBone { id: bones[0] }).unwrap();
    execute(&mut s, Command::SkinSetVertexWeights { vertices: ring[1..3].to_vec(), bone: bones[1], weight: 0.8 }).unwrap();
    for v in &ring[1..3] {
        assert!((weight(&s, arm, *v as usize, 1) - 0.8).abs() < 1e-4);
        assert!((weight(&s, arm, *v as usize, 0) - 0.2).abs() < 1e-4);
    }
    assert_eq!(s.skin_selection.iter().copied().collect::<Vec<_>>(), vec![ring[0]], "the selection stays");
    assert_eq!(s.skin_bone, Some(NodeId(bones[0])), "the current bone stays");
    assert!(execute(&mut s, Command::SkinSetVertexWeights { vertices: vec![9999], bone: bones[1], weight: 1.0 }).is_err());
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(!skin_data(&s, arm).explicit[ring[1] as usize]);
}

#[test]
fn bone_name_field_and_envelope_visibility() {
    let mut s = AppState::new();
    let (arm, bones) = skinned_arm(&mut s);
    let names: Vec<String> = bones[..2].iter().map(|b| s.scene.node(NodeId(*b)).unwrap().name.clone()).collect();
    execute(&mut s, Command::SkinSelectBoneByName { name: names[1].to_uppercase() }).unwrap();
    assert_eq!(s.skin_bone, Some(NodeId(bones[1])), "{names:?}");
    execute(&mut s, Command::SkinSelectBoneByName { name: format!("*{}", &names[0][names[0].len() - 1..]) }).unwrap();
    assert_eq!(s.skin_bone, Some(NodeId(bones[0])), "wildcard: {names:?}");
    assert!(execute(&mut s, Command::SkinSelectBoneByName { name: "nothing like it".into() }).is_err());
    execute(&mut s, Command::SkinEnvelopeVisibility).unwrap();
    assert!(skin_data(&s, arm).bones[0].envelope_visible);
    execute(&mut s, Command::SkinSelectBone { id: bones[1] }).unwrap();
    let shown = loom_commands::skin::shown_envelopes_world(&s);
    assert_eq!(shown.len(), 2, "the current bone's and the visible one");
    assert_eq!(shown.iter().filter(|e| e.current).count(), 1);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(!skin_data(&s, arm).bones[0].envelope_visible);
    assert_eq!(loom_commands::skin::shown_envelopes_world(&s).len(), 1);
}

#[test]
fn paint_weights_brushes_the_current_bone_near_the_stroke_and_alt_erases() {
    let mut s = AppState::new();
    let (arm, bones) = skinned_arm(&mut s);
    execute(&mut s, Command::SkinSelectBone { id: bones[1] }).unwrap();
    execute(&mut s, Command::SkinPaintWeightsToggle).unwrap();
    assert!(loom_commands::skin::painting(&s));
    for (name, value) in [("Max Size", 3.0), ("Max Strength", 0.5), ("Paint Blend Weights", 0.0), ("Falloff", 4.0)] {
        execute(&mut s, Command::SkinSetPainterOption { name: name.into(), value }).unwrap();
    }
    let near = verts_at_z(&s, arm, 2.5);
    let far = verts_at_z(&s, arm, 20.0);
    let mesh = s.scene.node(arm).unwrap().object.display_output();
    let at = mesh.vertex_position(loom_mesh::VertexId(near[0])).unwrap();
    let w0 = weight(&s, arm, near[0] as usize, 1);
    let far0 = weight(&s, arm, far[0] as usize, 1);
    let r = execute(&mut s, Command::SkinPaintStroke { points: vec![at.to_array()], erase: false, pressure: None }).unwrap();
    assert!(r["painted"].as_u64().unwrap() > 0);
    let w1 = weight(&s, arm, near[0] as usize, 1);
    assert!((w1 - (w0 + 0.5).min(1.0)).abs() < 1e-4, "additive flat brush adds Max Strength: {w0} -> {w1}");
    assert_eq!(weight(&s, arm, far[0] as usize, 1), far0, "outside the brush nothing changes");
    execute(&mut s, Command::SkinPaintStroke { points: vec![at.to_array()], erase: true, pressure: None }).unwrap();
    assert!((weight(&s, arm, near[0] as usize, 1) - (w1 - 0.5).max(0.0)).abs() < 1e-4, "erase takes it away");
    // Each stroke is one undo entry.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!((weight(&s, arm, near[0] as usize, 1) - w1).abs() < 1e-4);
    // Mirror (Z about 10): a dab for bone 1 at z = 2.5 also lands at z = 17.5, where bone 2 holds the vertex.
    execute(&mut s, Command::SkinSelectBone { id: bones[0] }).unwrap();
    execute(&mut s, Command::SkinSetPainterOption { name: "Mirror".into(), value: 1.0 }).unwrap();
    execute(&mut s, Command::SkinSetPainterOption { name: "Mirror Axis".into(), value: 2.0 }).unwrap();
    execute(&mut s, Command::SkinSetPainterOption { name: "Mirror Offset".into(), value: 10.0 }).unwrap();
    let twin = verts_at_z(&s, arm, 17.5).into_iter().find(|v| { let p = mesh.vertex_position(loom_mesh::VertexId(*v)).unwrap(); (p.x - at.x).abs() < 1e-3 && (p.y - at.y).abs() < 1e-3 }).unwrap();
    let before = weight(&s, arm, twin as usize, 0);
    assert!(before < 0.1, "{before}");
    execute(&mut s, Command::SkinPaintStroke { points: vec![at.to_array()], erase: false, pressure: None }).unwrap();
    assert!((weight(&s, arm, twin as usize, 0) - (before + 0.5)).abs() < 1e-4);
    let state = execute(&mut s, Command::State).unwrap();
    assert_eq!(state["skin"]["painter"]["max_size"], 3.0);
    assert!(execute(&mut s, Command::SkinSetPainterOption { name: "Bogus".into(), value: 1.0 }).is_err());
    execute(&mut s, Command::SkinPaintWeightsToggle).unwrap();
    assert!(!loom_commands::skin::painting(&s));
}

#[test]
fn paint_blend_weights_smooths_towards_the_neighbours() {
    let mut s = AppState::new();
    let (arm, bones) = skinned_arm(&mut s);
    execute(&mut s, Command::SkinSelectBone { id: bones[1] }).unwrap();
    let ring = verts_at_z(&s, arm, 2.5);
    let mesh = s.scene.node(arm).unwrap().object.display_output();
    let at = mesh.vertex_position(loom_mesh::VertexId(ring[0])).unwrap();
    execute(&mut s, Command::SkinSetPainterOption { name: "Max Size".into(), value: 0.01 }).unwrap();
    execute(&mut s, Command::SkinSetPainterOption { name: "Paint Blend Weights".into(), value: 0.0 }).unwrap();
    execute(&mut s, Command::SkinPaintStroke { points: vec![at.to_array()], erase: false, pressure: None }).unwrap();
    let sharp = weight(&s, arm, ring[0] as usize, 1);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::SkinSetPainterOption { name: "Paint Blend Weights".into(), value: 1.0 }).unwrap();
    execute(&mut s, Command::SkinPaintStroke { points: vec![at.to_array()], erase: false, pressure: None }).unwrap();
    let blended = weight(&s, arm, ring[0] as usize, 1);
    assert!(blended < sharp, "a lone painted vertex blends down towards its unpainted neighbours: {blended} vs {sharp}");
    for name in ["Skin Paint Weights", "Skin Paint Stroke", "Skin Set Painter Option", "Skin Painter Options", "Skin Set Vertex Weights", "Skin Select Bone By Name", "Skin Envelope Visibility", "Skin Set Scale Weight", "Skin Set Paste Tolerance"] {
        assert!(loom_commands::registry().iter().any(|i| i.name == name), "{name} missing");
    }
}
