//! Phase 7 review fix pass: one test per finding.

use glam::{Mat4, Vec3};
use loom_commands::{execute, AppState, Axis, Command, PrimitiveKind, SelectOp};
use loom_scene::{Modifier, NodeId, ObjectKind};

const RIG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../loom_fbx/tests/fixtures/sitting_idle.fbx");

fn dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join("loom-review7").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn by_name(s: &AppState, name: &str) -> NodeId {
    s.scene.nodes().find(|n| n.name == name).unwrap_or_else(|| panic!("{name} missing")).id
}

fn origin(s: &AppState, id: NodeId) -> Vec3 {
    s.scene.world_matrix(id).transform_point3(Vec3::ZERO)
}

fn skinned_mesh(s: &AppState) -> NodeId {
    s.scene.nodes().find(|n| n.modifiers.iter().any(|m| matches!(m.modifier, Modifier::Skin(_)))).unwrap().id
}

/// Finding 1: imported skeletons point along +Y; bones draw and solve along their real child direction.
#[test]
fn ik_on_the_imported_mixamo_arm_reaches_its_goal() {
    let mut s = AppState::new();
    execute(&mut s, Command::ImportFile { path: Some(RIG.into()) }).unwrap();
    let (arm, fore, hand) = (by_name(&s, "mixamorig:LeftArm"), by_name(&s, "mixamorig:LeftForeArm"), by_name(&s, "mixamorig:LeftHand"));
    // The bone's tip is its child's joint.
    let n = s.scene.node(arm).unwrap();
    let tip = s.scene.world_matrix(arm).transform_point3(n.object.bone_tip_local().unwrap());
    assert!((tip - origin(&s, fore)).length() < 0.05 * (origin(&s, fore) - origin(&s, arm)).length(), "tip {tip:?} vs forearm {:?}", origin(&s, fore));
    let hand_before = origin(&s, hand);
    let r = execute(&mut s, Command::CreateIkChain { start: arm.0, end: hand.0, solver: "hi".into(), name: None }).unwrap();
    let goal = NodeId(r["goal"].as_u64().unwrap());
    let hand_now = origin(&s, hand);
    let reach = (origin(&s, hand) - origin(&s, arm)).length();
    assert!((hand_now - hand_before).length() < 0.02 * reach, "creating the chain does not move the hand: {hand_before:?} → {hand_now:?}");
    execute(&mut s, Command::SelectNodes { ids: vec![goal.0], op: SelectOp::Replace }).unwrap();
    // The T-pose arm is straight: pull the goal in toward the shoulder and down so it is reachable.
    let target = origin(&s, arm) + (hand_before - origin(&s, arm)) * 0.6 + Vec3::new(0.0, 0.0, -0.3 * reach);
    let d = target - hand_before;
    execute(&mut s, Command::MoveSelection { delta: d.to_array(), system: None }).unwrap();
    let miss = (origin(&s, hand) - origin(&s, goal)).length();
    assert!(miss < 0.02 * reach, "hand misses the goal by {miss} (reach {reach})");
}

/// Finding 2: a chain hanging under another chain solves against its parent's solved pose.
#[test]
fn nested_chains_solve_parent_first() {
    let mut s = AppState::new();
    let r = execute(&mut s, Command::CreateBoneChain { points: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 10.0], [0.0, 0.0, 20.0], [0.0, 0.0, 21.0]], name: Some("Spine".into()) }).unwrap();
    let spine: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    let r = execute(&mut s, Command::CreateBoneChain { points: vec![[0.0, 0.0, 20.0], [10.0, 0.0, 20.0], [20.0, 0.0, 20.0], [21.0, 0.0, 20.0]], name: Some("Arm".into()) }).unwrap();
    let arm: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    execute(&mut s, Command::SetParent { id: arm[0], parent: Some(spine[1]) }).unwrap();
    // The arm chain first, then the spine chain.
    let r = execute(&mut s, Command::CreateIkChain { start: arm[0], end: arm[2], solver: "hi".into(), name: None }).unwrap();
    let arm_goal = NodeId(r["goal"].as_u64().unwrap());
    let r = execute(&mut s, Command::CreateIkChain { start: spine[0], end: spine[2], solver: "hi".into(), name: None }).unwrap();
    let spine_goal = NodeId(r["goal"].as_u64().unwrap());
    execute(&mut s, Command::SelectNodes { ids: vec![spine_goal.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [5.0, 0.0, -3.0], system: None }).unwrap();
    let miss = (origin(&s, NodeId(arm[2])) - origin(&s, arm_goal)).length();
    assert!(miss < 0.2, "the arm end misses its goal by {miss}");
}

/// Finding 3: an FBX import is one undo entry.
#[test]
fn fbx_import_is_one_undo_entry() {
    let mut s = AppState::new();
    let before = s.undo.len();
    execute(&mut s, Command::ImportFile { path: Some(RIG.into()) }).unwrap();
    assert_eq!(s.undo.len(), before + 1);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.nodes().count(), 0);
}

/// Finding 4: moving a skinned node re-evaluates its Skin (the bones hold the mesh).
#[test]
fn moving_a_skinned_node_keeps_the_mesh_with_the_bones() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let body = *s.selection.iter().next().unwrap();
    let r = execute(&mut s, Command::CreateBoneChain { points: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 10.0], [0.0, 0.0, 11.0]], name: None }).unwrap();
    let bones: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![body.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinModifier).unwrap();
    execute(&mut s, Command::SkinAddBones { ids: vec![bones[0]] }).unwrap();
    let centroid = |s: &AppState| {
        let m = s.scene.eval_mesh(body).value.mesh.clone();
        let w = s.scene.object_matrix(body);
        m.vertices().iter().map(|v| w.transform_point3(v.position)).sum::<Vec3>() / m.vertex_count() as f32
    };
    let c0 = centroid(&s);
    execute(&mut s, Command::MoveSelection { delta: [50.0, 0.0, 0.0], system: None }).unwrap();
    let c1 = centroid(&s);
    assert!((c1 - c0).length() < 1e-3, "the skinned mesh stays with its bones: {c0:?} → {c1:?}");
}

/// Findings 5 and 6: an imported skin keeps the file's world bind, exports it back, and new bones bind in world space.
#[test]
fn imported_skin_keeps_its_world_bind() {
    let d = dir("bind");
    let bytes = std::fs::read(RIG).unwrap();
    let original = loom_fbx::read(&bytes, 0.0254).unwrap();
    let orig_bind = original.nodes.iter().find_map(|n| n.skin.as_ref()).unwrap().mesh_bind;
    let mut s = AppState::new();
    execute(&mut s, Command::ImportFile { path: Some(RIG.into()) }).unwrap();
    let mesh = skinned_mesh(&s);
    let path = d.join("out.fbx");
    execute(&mut s, Command::SelectNodes { ids: vec![mesh.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::ExportSelected { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let back = loom_fbx::read(&std::fs::read(&path).unwrap(), 0.0254).unwrap();
    let bind = back.nodes.iter().find_map(|n| n.skin.as_ref()).unwrap().mesh_bind;
    assert!(bind.abs_diff_eq(orig_bind, 1e-2 * orig_bind.x_axis.length()), "mesh bind\n{orig_bind}\n{bind}");
    // A new, unmoved bone given full weight puts the vertex at its bind position in world space
    // (the file's pose differs from its bind pose, so that is not where it is drawn now).
    let world_v0 = |s: &AppState| s.scene.object_matrix(mesh).transform_point3(s.scene.eval_mesh(mesh).value.mesh.vertices()[0].position);
    let p0 = orig_bind.transform_point3(s.scene.node(mesh).unwrap().object.display_output().vertices()[0].position);
    let r = execute(&mut s, Command::CreateBoneChain { points: vec![[50.0, 0.0, 0.0], [60.0, 0.0, 0.0]], name: Some("Extra".into()) }).unwrap();
    let extra: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![mesh.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinAddBones { ids: vec![extra[0]] }).unwrap();
    execute(&mut s, Command::SkinSelectBone { id: extra[0] }).unwrap();
    execute(&mut s, Command::SkinSelectVertices { ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SetWeightTo10Skin).unwrap();
    let p1 = world_v0(&s);
    assert!((p1 - p0).length() < 1e-2 * p0.length().max(1.0), "{p0:?} → {p1:?}");
}

/// Finding 7: a pivot (object) offset is not applied twice to the exported skin.
#[test]
fn exported_skin_with_a_pivot_offset_skins_in_place() {
    let d = dir("offset");
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let body = *s.selection.iter().next().unwrap();
    s.scene.node_mut(body).unwrap().object_offset.position.set(Vec3::new(5.0, 0.0, 0.0));
    let r = execute(&mut s, Command::CreateBoneChain { points: vec![[0.0, 0.0, -5.0], [0.0, 0.0, 20.0], [0.0, 0.0, 21.0]], name: None }).unwrap();
    let bones: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![body.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinModifier).unwrap();
    execute(&mut s, Command::SkinAddBones { ids: vec![bones[0]] }).unwrap();
    let app_world: Vec<Vec3> = {
        let m = s.scene.eval_mesh(body).value.mesh.clone();
        let w = s.scene.object_matrix(body);
        m.vertices().iter().map(|v| w.transform_point3(v.position)).collect()
    };
    let path = d.join("off.fbx");
    execute(&mut s, Command::ExportSelected { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let scene = ufbx::load_memory(&std::fs::read(&path).unwrap(), ufbx::LoadOpts::default()).unwrap();
    let mesh = &scene.meshes[0];
    let c = &scene.skin_deformers[0].clusters[0];
    for (i, v) in mesh.vertices.iter().enumerate().take(8) {
        let q = ufbx::transform_position(&c.geometry_to_world, *v);
        let file = Vec3::new(q.x as f32, q.y as f32, q.z as f32) / 2.54; // the file is in cm, the scene in inches
        assert!((file - app_world[i]).length() < 1e-2, "vertex {i}: app {:?} file {file:?}", app_world[i]);
    }
}

/// Finding 8: the Skin vertex selection belongs to one Skin; another Skin starts with none.
#[test]
fn skin_vertex_selection_does_not_leak_between_skins() {
    let mut s = AppState::new();
    let mut skin = |s: &mut AppState, segs: f64, x: f32| {
        execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Cylinder, position: [x, 0.0, 0.0], size: [2.0, 2.0, 20.0] }).unwrap();
        let id = *s.selection.iter().next().unwrap();
        execute(s, Command::SetParameter { id: id.0, name: "Height Segments".into(), value: segs }).unwrap();
        let r = execute(s, Command::CreateBoneChain { points: vec![[x, 0.0, 0.0], [x, 0.0, 20.0], [x, 0.0, 21.0]], name: None }).unwrap();
        let b: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
        execute(s, Command::SelectNodes { ids: vec![id.0], op: SelectOp::Replace }).unwrap();
        execute(s, Command::SkinModifier).unwrap();
        execute(s, Command::SkinAddBones { ids: vec![b[0]] }).unwrap();
        id
    };
    let a = skin(&mut s, 10.0, 0.0);
    let b = skin(&mut s, 1.0, 30.0);
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinSelectVertices { ids: vec![100], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![b.0], op: SelectOp::Replace }).unwrap();
    assert!(loom_commands::skin::selection(&s).is_empty(), "{:?}", loom_commands::skin::selection(&s));
    assert!(execute(&mut s, Command::SetWeightTo10Skin).is_err(), "nothing selected on this Skin");
    let st = execute(&mut s, Command::State).unwrap();
    assert_eq!(st["skin"]["selection"], serde_json::json!([]));
}

/// Finding 10: Skin Wrap binds to the mesh flowing into it, not to the base object.
#[test]
fn skin_wrap_binds_to_the_mesh_below_it() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [20.0; 3] }).unwrap();
    let base = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0, 0.0, 2.0], size: [10.0; 3] }).unwrap();
    let wrapped = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::AddModifier { name: "TurboSmooth".into() }).unwrap();
    execute(&mut s, Command::SkinWrapModifier).unwrap();
    execute(&mut s, Command::SkinWrapAddBase { id: base.0 }).unwrap();
    let e = s.scene.eval_mesh(wrapped);
    assert!(e.warnings.is_empty(), "{:?}", e.warnings);
    let st = execute(&mut s, Command::State).unwrap();
    assert_eq!(st["skin_wrap"]["vertices"].as_u64().unwrap() as usize, e.value.mesh.vertex_count());
}

/// Finding 11: imported blend shapes sit below the Skin (FBX order).
#[test]
fn imported_blend_shapes_sit_below_the_skin() {
    let d = dir("order");
    use loom_fbx::{FbxBlendShape, FbxDocument, FbxKind, FbxNode, FbxSkin, WriteOpts};
    let mesh = loom_mesh::box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
    let n = mesh.vertex_count();
    let bone = FbxNode::new("Bone001", FbxKind::Bone { length: 10.0, size: 2.0 });
    let mut m = FbxNode::new("Head", FbxKind::Mesh { mesh, materials: vec![] });
    m.skin = Some(FbxSkin { bones: vec![(0, Mat4::IDENTITY)], mesh_bind: Mat4::IDENTITY, weights: vec![vec![(0, 1.0)]; n] });
    m.blend_shapes.push(FbxBlendShape { name: "up".into(), offsets: vec![(0, Vec3::Z)], weight: 50.0 });
    let path = d.join("order.fbx");
    std::fs::write(&path, loom_fbx::write(&FbxDocument { nodes: vec![bone, m], unit_metres: 0.0254, anim: None, ..Default::default() }, &WriteOpts::default())).unwrap();
    let mut s = AppState::new();
    execute(&mut s, Command::ImportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let head = s.scene.nodes().find(|n| n.name == "Head").unwrap();
    let names: Vec<&str> = head.modifiers.iter().map(|m| m.modifier.name()).collect();
    assert_eq!(names, vec!["Morpher", "Skin"], "bottom first");
    let _ = (Axis::X, ObjectKind::dummy(1.0));
}
