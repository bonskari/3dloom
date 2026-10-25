//! FBX bones, skin clusters and blend shapes both ways; the Mixamo rig round trip.

use glam::Vec3;
use loom_fbx::{FbxBlendShape, FbxDocument, FbxKind, FbxNode, WriteOpts};

#[test]
fn mixamo_skin_round_trip_is_lossless() {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sitting_idle.fbx")).unwrap();
    let doc = loom_fbx::read(&bytes, 0.01).unwrap();
    let bones: Vec<&FbxNode> = doc.nodes.iter().filter(|n| matches!(n.kind, FbxKind::Bone { .. })).collect();
    assert_eq!(bones.len(), 65);
    let hips = bones.iter().find(|b| b.name == "mixamorig:Hips").expect("Hips");
    assert!(hips.parent.is_none_or(|p| !matches!(doc.nodes[p].kind, FbxKind::Bone { .. })), "Hips is the root bone");
    let mesh_node = doc.nodes.iter().find(|n| matches!(n.kind, FbxKind::Mesh { .. })).unwrap();
    let skin = mesh_node.skin.as_ref().expect("skin");
    assert_eq!(skin.bones.len(), 52);
    let FbxKind::Mesh { mesh, .. } = &mesh_node.kind else { unreachable!() };
    assert_eq!(skin.weights.len(), mesh.vertex_count());
    assert!(skin.weights.iter().any(|w| w.len() >= 5), "up to 8 influences kept");
    assert!(skin.weights.iter().all(|w| (w.iter().map(|x| x.1).sum::<f32>() - 1.0).abs() < 1e-3 || w.is_empty()), "normalised");
    let out = loom_fbx::write(&doc, &WriteOpts::default());
    let back = loom_fbx::read(&out, 0.01).unwrap();
    let bones2: Vec<&FbxNode> = back.nodes.iter().filter(|n| matches!(n.kind, FbxKind::Bone { .. })).collect();
    assert_eq!(bones2.len(), 65);
    for (a, b) in bones.iter().zip(&bones2) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.parent.map(|p| &doc.nodes[p].name), b.parent.map(|p| &back.nodes[p].name), "{}", a.name);
        assert!((a.translation - b.translation).length() < 1e-2, "{}: {:?} vs {:?}", a.name, a.translation, b.translation);
    }
    let mesh2 = back.nodes.iter().find(|n| matches!(n.kind, FbxKind::Mesh { .. })).unwrap();
    let skin2 = mesh2.skin.as_ref().unwrap();
    assert_eq!(skin2.bones.len(), 52);
    assert_eq!(skin.weights.len(), skin2.weights.len());
    for (i, (w1, w2)) in skin.weights.iter().zip(&skin2.weights).enumerate() {
        assert_eq!(w1.len(), w2.len(), "vertex {i}: {w1:?} vs {w2:?}");
        for (a, b) in w1.iter().zip(w2) {
            assert_eq!(doc.nodes[skin.bones[a.0 as usize].0].name, back.nodes[skin2.bones[b.0 as usize].0].name, "vertex {i} bone");
            assert!((a.1 - b.1).abs() < 1e-5, "vertex {i} weight {} vs {}", a.1, b.1);
        }
    }
    for ((_, m1), (_, m2)) in skin.bones.iter().zip(&skin2.bones) {
        assert!(m1.abs_diff_eq(*m2, 1e-2), "bind matrix\n{m1}\n{m2}");
    }
    assert!(skin.mesh_bind.abs_diff_eq(skin2.mesh_bind, 1e-2));
    // ufbx strict accepts the file and sees the skin and its bind pose.
    let s = ufbx::load_memory(&out, ufbx::LoadOpts { strict: true, ..Default::default() }).unwrap();
    assert_eq!(s.skin_deformers.len(), 1);
    assert_eq!(s.skin_deformers[0].clusters.len(), 52);
    assert_eq!(s.bones.len(), 65);
    assert_eq!(s.poses.len(), 1);
}

#[test]
fn blend_shapes_round_trip() {
    let mesh = loom_mesh::box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
    let mut node = FbxNode::new("Head", FbxKind::Mesh { mesh, materials: vec![] });
    node.blend_shapes.push(FbxBlendShape { name: "smile".into(), offsets: vec![(0, Vec3::Z * 2.0), (3, Vec3::X)], weight: 30.0 });
    let bytes = loom_fbx::write(&FbxDocument { nodes: vec![node], unit_metres: 0.01, anim: None, ..Default::default() }, &WriteOpts::default());
    let s = ufbx::load_memory(&bytes, ufbx::LoadOpts { strict: true, ..Default::default() }).unwrap();
    assert_eq!(s.blend_deformers.len(), 1);
    let back = loom_fbx::read(&bytes, 0.01).unwrap();
    let bs = &back.nodes[0].blend_shapes;
    assert_eq!(bs.len(), 1);
    assert_eq!(bs[0].name, "smile");
    assert_eq!(bs[0].offsets.len(), 2);
    assert!((bs[0].offsets[0].1 - Vec3::Z * 2.0).length() < 1e-4, "{:?}", bs[0].offsets[0]);
    assert!((bs[0].weight - 30.0).abs() < 1e-3);
}

#[test]
fn bones_write_as_limb_nodes_with_their_size() {
    let mut root = FbxNode::new("Bone001", FbxKind::Bone { length: 10.0, size: 4.0 });
    root.translation = Vec3::new(1.0, 2.0, 3.0);
    let mut child = FbxNode::new("Bone002", FbxKind::Bone { length: 5.0, size: 4.0 });
    child.parent = Some(0);
    child.translation = Vec3::X * 10.0;
    let bytes = loom_fbx::write(&FbxDocument { nodes: vec![root, child], unit_metres: 0.01, anim: None, ..Default::default() }, &WriteOpts::default());
    let s = ufbx::load_memory(&bytes, ufbx::LoadOpts { strict: true, ..Default::default() }).unwrap();
    assert_eq!(s.bones.len(), 2);
    let back = loom_fbx::read(&bytes, 0.01).unwrap();
    let FbxKind::Bone { length, .. } = back.nodes[0].kind else { panic!("{:?}", back.nodes[0].kind) };
    assert!((length - 10.0).abs() < 1e-3, "the root's length is the distance to its child: {length}");
    assert_eq!(back.nodes[1].parent, Some(0));
}
