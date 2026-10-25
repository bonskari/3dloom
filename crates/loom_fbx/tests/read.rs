//! Reader completeness: units/axes, smoothing from normals, broken input, a real file.

use glam::Vec3;
use loom_fbx::{FbxDocument, FbxKind, FbxNode, WriteAxes, WriteOpts};

fn node(name: &str, mesh: loom_mesh::Mesh, translation: Vec3) -> FbxNode {
    FbxNode { name: name.into(), parent: None, translation, rotation_deg: Vec3::ZERO, scale: Vec3::ONE, kind: FbxKind::Mesh { mesh, materials: vec![] }, skin: None, blend_shapes: Vec::new() }
}

#[test]
fn y_up_centimetre_file_converts_to_scene_units_and_z_up() {
    let mesh = loom_mesh::box_segmented(100.0, 100.0, 100.0, [1, 1, 1]); // 100 cm cube in a cm document
    let bytes = loom_fbx::write(&FbxDocument { nodes: vec![node("Cube", mesh, Vec3::new(0.0, 0.0, 50.0))], unit_metres: 0.01, anim: None, layers: Vec::new() }, &WriteOpts { axes: WriteAxes::YUp, ..Default::default() });
    let back = loom_fbx::read(&bytes, 0.0254).unwrap(); // an inch scene
    let n = &back.nodes[0];
    let FbxKind::Mesh { mesh, .. } = &n.kind else { panic!() };
    let (lo, hi) = mesh.aabb();
    assert!((hi.z - lo.z - 39.3701).abs() < 1e-2, "100 cm = 39.37 in along Z: {lo:?} {hi:?}");
    assert!((hi.x - lo.x - 39.3701).abs() < 1e-2 && (hi.y - lo.y - 39.3701).abs() < 1e-2);
    assert!((n.translation - Vec3::new(0.0, 0.0, 50.0 / 2.54)).length() < 1e-2, "{:?}", n.translation);
}

#[test]
fn normals_without_smoothing_become_groups() {
    let mut mesh = loom_mesh::box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
    for f in mesh.faces_mut() {
        f.smoothing_groups = 0;
    }
    let bytes = loom_fbx::write(&FbxDocument { nodes: vec![node("Box", mesh, Vec3::ZERO)], unit_metres: 0.0254, anim: None, layers: Vec::new() }, &WriteOpts { smoothing_layer: false, ..Default::default() });
    let back = loom_fbx::read(&bytes, 0.0254).unwrap();
    let FbxKind::Mesh { mesh, .. } = &back.nodes[0].kind else { panic!() };
    let groups: Vec<u32> = mesh.faces().iter().map(|f| f.smoothing_groups).collect();
    assert!(groups.iter().all(|g| g.count_ones() == 1), "{groups:?}");
    for i in 0..mesh.half_edge_count() {
        let he = &mesh.half_edges()[i];
        let (Some(a), Some(b)) = (he.face, mesh.half_edges()[he.twin.0 as usize].face) else { continue };
        assert_eq!(groups[a.0 as usize] & groups[b.0 as usize], 0, "box edges are hard");
    }
    // A smooth sphere written the same way stays one group.
    let sphere = loom_mesh::sphere_segmented(10.0, 12, 1.0);
    let bytes = loom_fbx::write(&FbxDocument { nodes: vec![node("S", sphere, Vec3::ZERO)], unit_metres: 0.0254, anim: None, layers: Vec::new() }, &WriteOpts { smoothing_layer: false, ..Default::default() });
    let back = loom_fbx::read(&bytes, 0.0254).unwrap();
    let FbxKind::Mesh { mesh, .. } = &back.nodes[0].kind else { panic!() };
    assert!(mesh.faces().iter().all(|f| f.smoothing_groups == 1), "{:?}", mesh.faces().iter().map(|f| f.smoothing_groups).collect::<Vec<_>>());
}

#[test]
fn broken_faces_import_without_panic() {
    let bytes = loom_fbx::write_raw_faces(&[Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z], &[vec![0, 1, 2], vec![0, 1, 2], vec![0, 1, 1, 3], vec![1, 2]], None, 0.0254);
    let back = loom_fbx::read(&bytes, 0.0254).unwrap();
    let FbxKind::Mesh { mesh, .. } = &back.nodes[0].kind else { panic!("{:?}", back.nodes[0].kind) };
    mesh.check_invariants().unwrap();
    assert!(mesh.face_count() >= 1 && mesh.face_count() <= 3, "{}", mesh.face_count());
}

#[test]
fn reads_the_mixamo_character() {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/character_for_mixamo.fbx")).unwrap();
    let doc = loom_fbx::read(&bytes, 0.01).unwrap();
    let meshes: Vec<&FbxNode> = doc.nodes.iter().filter(|n| matches!(n.kind, FbxKind::Mesh { .. })).collect();
    assert!(!meshes.is_empty());
    for n in meshes {
        let FbxKind::Mesh { mesh, materials } = &n.kind else { unreachable!() };
        mesh.check_invariants().unwrap();
        assert!(mesh.face_count() > 0);
        assert!(mesh.map(1).is_some(), "{}: no uv channel", n.name);
        assert!(!materials.is_empty(), "{}: no material names", n.name);
        let (lo, hi) = mesh.aabb();
        assert!(hi.z - lo.z > hi.y - lo.y, "{}: a standing character is tallest along Z after conversion: {lo:?} {hi:?}", n.name);
    }
}
