//! Our FBX 7.4 writer read back by ufbx (strict) and by our own reader.

use glam::Vec3;
use loom_fbx::{FbxDocument, FbxKind, FbxNode, WriteOpts};

fn sample() -> FbxDocument {
    let mut mesh = loom_mesh::box_segmented(10.0, 20.0, 30.0, [1, 1, 1]);
    loom_mesh::uvmap::auto_map(&mut mesh, loom_mesh::uvmap::AutoMap::Box, 1);
    loom_mesh::uvmap::auto_map(&mut mesh, loom_mesh::uvmap::AutoMap::Planar, 2);
    for (i, f) in mesh.faces_mut().iter_mut().enumerate() {
        f.smoothing_groups = 1 << (i % 3);
        f.material_id = (i % 2) as u16 + 1;
    }
    mesh.vertices_mut()[0].color = [1.0, 0.0, 0.0];
    let parent = FbxNode { name: "Root".into(), parent: None, translation: Vec3::new(1.0, 2.0, 3.0), rotation_deg: Vec3::new(0.0, 0.0, 90.0), scale: Vec3::ONE, kind: FbxKind::Mesh { mesh: mesh.clone(), materials: vec!["Wood".into(), "Steel".into()] }, skin: None, blend_shapes: Vec::new(), user_props: Vec::new() };
    let child = FbxNode { name: "Child".into(), parent: Some(0), translation: Vec3::new(5.0, 0.0, 0.0), rotation_deg: Vec3::ZERO, scale: Vec3::new(2.0, 2.0, 2.0), kind: FbxKind::Mesh { mesh, materials: vec![] }, skin: None, blend_shapes: Vec::new(), user_props: Vec::new() };
    let cam = FbxNode { name: "Cam".into(), parent: None, translation: Vec3::new(0.0, -50.0, 10.0), rotation_deg: Vec3::new(90.0, 0.0, 0.0), scale: Vec3::ONE, kind: FbxKind::Camera { fov_deg: 45.0 }, skin: None, blend_shapes: Vec::new(), user_props: Vec::new() };
    FbxDocument { nodes: vec![parent, child, cam], unit_metres: 0.0254, anim: None, ..Default::default() }
}

fn root_uvs(d: &FbxDocument) -> Vec<Vec3> {
    let FbxKind::Mesh { mesh, .. } = &d.nodes[0].kind else { panic!() };
    (0..mesh.face_count()).flat_map(|f| mesh.face_corners(loom_mesh::FaceId(f as u32)).unwrap().into_iter().map(|he| mesh.corner_uv(1, he).unwrap()).collect::<Vec<_>>()).collect()
}

#[test]
fn written_file_reads_back_strictly() {
    let doc = sample();
    let bytes = loom_fbx::write(&doc, &WriteOpts { creator: "3dloom test".into(), ..Default::default() });
    assert!(bytes.starts_with(b"Kaydara FBX Binary  \0\x1a\0"));
    assert_eq!(u32::from_le_bytes(bytes[23..27].try_into().unwrap()), 7400);
    let scene = ufbx::load_memory(&bytes, ufbx::LoadOpts { strict: true, ..Default::default() }).expect("ufbx strict load");
    assert_eq!(scene.meshes.len(), 2);
    assert_eq!(scene.cameras.len(), 1);
    assert!(scene.metadata.warnings.is_empty(), "{:?}", scene.metadata.warnings.iter().map(|w| w.description.to_string()).collect::<Vec<_>>());
    let back = loom_fbx::read(&bytes, 0.0254).unwrap();
    assert_eq!(back.nodes.len(), 3);
    let root = back.nodes.iter().find(|n| n.name == "Root").unwrap();
    let FbxKind::Mesh { mesh, materials } = &root.kind else { panic!("Root is a mesh") };
    assert_eq!(materials, &["Wood".to_string(), "Steel".to_string()]);
    assert_eq!(mesh.face_count(), 6);
    assert_eq!(mesh.vertex_count(), 8);
    assert_eq!(mesh.maps().len(), 2, "two uv channels");
    assert!((root.translation - Vec3::new(1.0, 2.0, 3.0)).length() < 1e-4);
    assert!((root.rotation_deg.z - 90.0).abs() < 1e-3, "{:?}", root.rotation_deg);
    let child = back.nodes.iter().find(|n| n.name == "Child").unwrap();
    assert_eq!(child.parent.map(|p| back.nodes[p].name.as_str()), Some("Root"));
    assert!((child.scale - Vec3::splat(2.0)).length() < 1e-4);
    let FbxKind::Mesh { mesh: cm, .. } = &child.kind else { panic!() };
    let groups: Vec<u32> = cm.faces().iter().map(|f| f.smoothing_groups).collect();
    assert_eq!(groups, vec![1, 2, 4, 1, 2, 4]);
    let mats: Vec<u16> = cm.faces().iter().map(|f| f.material_id).collect();
    assert_eq!(mats, vec![1, 2, 1, 2, 1, 2]);
    assert_eq!(cm.vertices()[0].color, [1.0, 0.0, 0.0]);
    let cam = back.nodes.iter().find(|n| n.name == "Cam").unwrap();
    let FbxKind::Camera { fov_deg } = cam.kind else { panic!() };
    assert!((fov_deg - 45.0).abs() < 1e-3);
    assert!((cam.translation - Vec3::new(0.0, -50.0, 10.0)).length() < 1e-3, "{:?}", cam.translation);
    // Same uvs per corner on the way back (channel 1), same vertex positions.
    let (a, b) = (root_uvs(&doc), root_uvs(&back));
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert!((x.truncate() - y.truncate()).length() < 1e-4, "{x:?} {y:?} (FBX uvs are 2D)");
    }
    let FbxKind::Mesh { mesh: m0, .. } = &doc.nodes[0].kind else { panic!() };
    for (p, q) in m0.vertices().iter().zip(mesh.vertices()) {
        assert!((p.position - q.position).length() < 1e-4);
    }
}
