//! Opt-in CPU triangulation benchmark using a saved production scene.
use std::time::Instant;

#[test]
#[ignore = "release benchmark requires LOOM_PERF_SCENE"]
fn saved_scene_triangulation_cost() {
    let path = std::env::var("LOOM_PERF_SCENE").expect("provide an actual saved scene");
    let doc = loom_commands::file::read(std::path::Path::new(&path)).unwrap();
    let meshes: Vec<_> = doc.scene.nodes().filter(|n| n.modifiers.iter().any(|m| matches!(m.modifier, loom_scene::Modifier::Skin(_))))
        .map(|n| doc.scene.eval_mesh(n.id).value.mesh.clone()).collect();
    assert!(!meshes.is_empty());
    eprintln!("BENCH skins={} vertices={} faces={}", meshes.len(), meshes.iter().map(|m| m.vertex_count()).sum::<usize>(), meshes.iter().map(|m| m.face_count()).sum::<usize>());
    // Compare every production-scene corner with the uncached normal path.
    for mesh in &meshes {
        let (vertices, _) = loom_render::triangulate_with_face_normals(mesh, [1.0; 4], false);
        let mut corner = 0;
        for (i, face) in mesh.faces().iter().enumerate().filter(|(_, f)| !f.hidden) {
            let mut he_id = face.he;
            loop {
                let he = &mesh.half_edges()[he_id.0 as usize];
                let expected = mesh.explicit_normal(he_id).unwrap_or_else(|| mesh.vertex_normal(he.origin, loom_mesh::FaceId(i as u32)));
                assert_eq!(vertices[corner].normal, expected.to_array());
                corner += 1;
                he_id = he.next;
                if he_id == face.he { break; }
            }
        }
        assert_eq!(corner, vertices.len());
    }
    let started = Instant::now();
    for _ in 0..5 {
        for mesh in &meshes {
            std::hint::black_box(loom_render::triangulate_with_face_normals(mesh, [1.0; 4], false));
        }
    }
    eprintln!("BENCH triangulate_frame_ms={:.3}", started.elapsed().as_secs_f64() * 1000.0 / 5.0);
}
