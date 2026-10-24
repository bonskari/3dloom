//! Review fix pass: Euler order, camera axes and FOV, header ids, centimetres, unused vertices, scanner safety.

use glam::{EulerRot, Quat, Vec3};
use loom_fbx::{FbxDocument, FbxKind, FbxNode, WriteOpts};

fn quat(deg: Vec3) -> Quat {
    Quat::from_euler(EulerRot::XYZ, deg.x.to_radians(), deg.y.to_radians(), deg.z.to_radians())
}

fn mesh_node(name: &str, rotation_deg: Vec3) -> FbxNode {
    FbxNode { name: name.into(), parent: None, translation: Vec3::ZERO, rotation_deg, scale: Vec3::ONE, kind: FbxKind::Mesh { mesh: loom_mesh::box_segmented(10.0, 10.0, 10.0, [1, 1, 1]), materials: vec![] }, skin: None, blend_shapes: Vec::new() }
}

fn plain(bytes: &[u8]) -> ufbx::SceneRoot {
    ufbx::load_memory(bytes, ufbx::LoadOpts::default()).unwrap()
}

fn node_quat(n: &ufbx::Node) -> Quat {
    let q = n.local_transform.rotation;
    Quat::from_xyzw(q.x as f32, q.y as f32, q.z as f32, q.w as f32)
}

#[test]
fn rotations_on_three_axes_survive() {
    let r = Vec3::new(30.0, 40.0, 50.0);
    let bytes = loom_fbx::write(&FbxDocument { nodes: vec![mesh_node("R", r)], unit_metres: 0.01, anim: None }, &WriteOpts::default());
    // What any FBX reader computes from Lcl Rotation (RotationOrder XYZ) must be loom's orientation.
    let s = plain(&bytes);
    let n = s.nodes.iter().find(|n| n.mesh.is_some()).unwrap();
    let dot = node_quat(n).dot(quat(r)).abs();
    assert!(dot > 0.9999, "orientation differs: dot {dot}");
    let back = loom_fbx::read(&bytes, 0.01).unwrap();
    let dot = quat(back.nodes[0].rotation_deg).dot(quat(r)).abs();
    assert!(dot > 0.9999, "round trip: {:?} vs {r:?}", back.nodes[0].rotation_deg);
}

#[test]
fn camera_forward_survives() {
    // A loom camera looks down its local -Z; rotated (90, 0, 0) it looks along +Y.
    let cam = FbxNode { name: "Cam".into(), parent: None, translation: Vec3::ZERO, rotation_deg: Vec3::new(90.0, 0.0, 0.0), scale: Vec3::ONE, kind: FbxKind::Camera { fov_deg: 45.0 }, skin: None, blend_shapes: Vec::new() };
    let bytes = loom_fbx::write(&FbxDocument { nodes: vec![cam], unit_metres: 0.01, anim: None }, &WriteOpts::default());
    // FBX cameras look along their local +X: that axis must be world +Y in the file.
    let s = plain(&bytes);
    let n = s.nodes.iter().find(|n| n.camera.is_some()).unwrap();
    let fwd = node_quat(n) * Vec3::X;
    assert!((fwd - Vec3::Y).length() < 1e-3, "file camera looks along {fwd:?}");
    let back = loom_fbx::read(&bytes, 0.01).unwrap();
    let fwd = quat(back.nodes[0].rotation_deg) * Vec3::NEG_Z;
    assert!((fwd - Vec3::Y).length() < 1e-3, "imported camera looks along {fwd:?}");
}

#[test]
fn camera_fov_is_vertical() {
    let cam = FbxNode { name: "Cam".into(), parent: None, translation: Vec3::ZERO, rotation_deg: Vec3::ZERO, scale: Vec3::ONE, kind: FbxKind::Camera { fov_deg: 45.0 }, skin: None, blend_shapes: Vec::new() };
    let bytes = loom_fbx::write(&FbxDocument { nodes: vec![cam], unit_metres: 0.01, anim: None }, &WriteOpts::default());
    let s = plain(&bytes);
    let c = &s.cameras[0];
    assert!(c.field_of_view_deg.x > c.field_of_view_deg.y + 1.0, "a 16:9 camera is wider than tall: {:?}", c.field_of_view_deg);
    assert!((c.field_of_view_deg.y - 45.0).abs() < 1e-2, "{:?}", c.field_of_view_deg);
    let back = loom_fbx::read(&bytes, 0.01).unwrap();
    let FbxKind::Camera { fov_deg } = back.nodes[0].kind else { panic!() };
    assert!((fov_deg - 45.0).abs() < 1e-2, "{fov_deg}");
}

#[test]
fn file_id_and_creation_time_are_the_known_good_pair() {
    let bytes = loom_fbx::write(&FbxDocument { nodes: vec![mesh_node("B", Vec3::ZERO)], unit_metres: 0.01, anim: None }, &WriteOpts::default());
    let id: [u8; 16] = [0x28, 0xb3, 0x2a, 0xeb, 0xb6, 0x24, 0xcc, 0xc2, 0xbf, 0xc8, 0xb0, 0x2a, 0xa9, 0x2b, 0xfc, 0xf1];
    assert!(bytes.windows(16).any(|w| w == id), "FileId");
    assert!(bytes.windows(23).any(|w| w == b"1970-01-01 10:00:00:000"), "CreationTime");
    // The footer pads 1..16 bytes after the footer id: the file never ends on the id itself.
    let fid: [u8; 16] = [0xfa, 0xbc, 0xab, 0x09, 0xd0, 0xc8, 0xd4, 0x66, 0xb1, 0x76, 0xfb, 0x83, 0x1c, 0xf7, 0x26, 0x7e];
    let pos = bytes.windows(16).position(|w| w == fid).unwrap();
    let after = &bytes[pos + 16..];
    let zeros = after.iter().take_while(|b| **b == 0).count();
    assert!((1..=16).contains(&zeros), "1..16 pad bytes then the version: {zeros}");
    assert_eq!(&after[zeros..zeros + 4], &7400u32.to_le_bytes());
}

#[test]
fn vertices_are_written_in_centimetres() {
    // A 10-unit box in an inch scene is 25.4 cm in the file, with UnitScaleFactor 1 (Unreal's default import ignores it).
    let bytes = loom_fbx::write(&FbxDocument { nodes: vec![mesh_node("B", Vec3::ZERO)], unit_metres: 0.0254, anim: None }, &WriteOpts::default());
    let s = plain(&bytes);
    assert!((s.settings.unit_meters - 0.01).abs() < 1e-9, "{}", s.settings.unit_meters);
    let m = &s.meshes[0];
    let (lo, hi) = m.vertices.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(v.z), hi.max(v.z)));
    assert!((hi - lo - 25.4).abs() < 1e-3, "{lo}..{hi}");
    let back = loom_fbx::read(&bytes, 0.0254).unwrap();
    let FbxKind::Mesh { mesh, .. } = &back.nodes[0].kind else { panic!() };
    let (lo, hi) = mesh.aabb();
    assert!((hi.z - lo.z - 10.0).abs() < 1e-3, "back in inches: {lo:?} {hi:?}");
}

#[test]
fn unused_vertex_keeps_hard_edges() {
    // A flat-shaded box whose vertex 0 is unused, normals split per face, no smoothing layer.
    let b = loom_mesh::box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
    let mut positions = vec![Vec3::new(99.0, 99.0, 99.0)];
    positions.extend(b.vertices().iter().map(|v| v.position));
    let mut faces = Vec::new();
    let mut normals = Vec::new();
    for f in 0..b.face_count() {
        let ids: Vec<u32> = b.face_vertex_ids(loom_mesh::FaceId(f as u32)).unwrap().iter().map(|v| v.0 + 1).collect();
        let n = b.face_normal(loom_mesh::FaceId(f as u32));
        normals.extend(std::iter::repeat_n(n, ids.len()));
        faces.push(ids);
    }
    let bytes = loom_fbx::write_raw_faces(&positions, &faces, Some(&normals), 0.01);
    let back = loom_fbx::read(&bytes, 0.01).unwrap();
    let FbxKind::Mesh { mesh, .. } = &back.nodes[0].kind else { panic!() };
    let groups: Vec<u32> = mesh.faces().iter().map(|f| f.smoothing_groups).collect();
    for i in 0..mesh.half_edge_count() {
        let he = &mesh.half_edges()[i];
        let (Some(a), Some(c)) = (he.face, mesh.half_edges()[he.twin.0 as usize].face) else { continue };
        assert_eq!(groups[a.0 as usize] & groups[c.0 as usize], 0, "box edges stay hard: {groups:?}");
    }
}

#[test]
fn truncated_or_corrupt_binary_does_not_panic() {
    let bytes = loom_fbx::write(&FbxDocument { nodes: vec![mesh_node("B", Vec3::ZERO)], unit_metres: 0.01, anim: None }, &WriteOpts::default());
    for n in (0..bytes.len()).step_by(7) {
        let _ = loom_fbx::binary::parse(&bytes[..n]);
    }
    // An end offset pointing backwards must not loop forever.
    let mut bad = bytes.clone();
    bad[27..31].copy_from_slice(&5u32.to_le_bytes());
    assert!(loom_fbx::binary::parse(&bad).is_none());
    let mut huge = bytes.clone();
    huge[31..35].copy_from_slice(&u32::MAX.to_le_bytes());
    let _ = loom_fbx::binary::parse(&huge);
}
