//! Smoothing from real exporter output: Blender 5.2 files written with each
//! "Smoothing" option (Normals Only, Face, Edge, Smoothing Groups) shade the
//! same after import, and 3dloom's own files keep their groups.
//!
//! The fixtures hold a cylinder (smooth sides, hard caps: Smooth by Angle
//! 30°), a pyramid (every edge hard, opposite sides meet only at the apex),
//! a flat cube and a smooth sphere (built by `blender_smoothing.py`).

use glam::Vec3;
use loom_fbx::{FbxDocument, FbxKind, FbxNode, WriteOpts};
use loom_mesh::{mesh_mods, FaceId, Mesh};
use std::collections::BTreeMap;

fn fixture(name: &str) -> FbxDocument {
    let bytes = std::fs::read(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap();
    loom_fbx::read(&bytes, 0.01).unwrap()
}

/// Whether the edge between faces `f` and `g` of object `object` is smooth in the source scene.
fn smooth_edge(object: &str, mesh: &Mesh, f: FaceId, g: FaceId) -> bool {
    match object {
        "Sphere" => true,
        "FlatCube" => false,
        _ => mesh.face_normal(f).dot(mesh.face_normal(g)) > 30f32.to_radians().cos(),
    }
}

/// Every vertex: the faces around it joined across smooth edges form fans;
/// the imported corner normals must agree inside a fan and differ between fans.
fn assert_shading_matches(object: &str, mesh: &Mesh, file: &str) {
    let normals = mesh_mods::corner_normals(mesh);
    let mut corner: BTreeMap<(u32, u32), Vec3> = BTreeMap::new();
    for (i, he) in mesh.half_edges().iter().enumerate() {
        if let Some(f) = he.face {
            corner.insert((f.0, he.origin.0), normals[i]);
        }
    }
    for v in 0..mesh.vertex_count() as u32 {
        let around = mesh.faces_around_vertex(loom_mesh::VertexId(v));
        let mut fan: BTreeMap<u32, u32> = around.iter().map(|f| (f.0, f.0)).collect();
        fn root(fan: &BTreeMap<u32, u32>, mut f: u32) -> u32 {
            while fan[&f] != f {
                f = fan[&f];
            }
            f
        }
        for he in mesh.half_edges().iter().filter(|he| he.origin.0 == v) {
            let (Some(f), Some(g)) = (he.face, mesh.half_edges()[he.twin.0 as usize].face) else { continue };
            if smooth_edge(object, mesh, f, g) {
                let (a, b) = (root(&fan, f.0), root(&fan, g.0));
                fan.insert(a, b);
            }
        }
        for (i, f) in around.iter().enumerate() {
            for g in &around[i + 1..] {
                let same = root(&fan, f.0) == root(&fan, g.0);
                let (nf, ng) = (corner[&(f.0, v)], corner[&(g.0, v)]);
                let equal = nf.dot(ng) > 0.9999;
                assert_eq!(equal, same, "{file} {object}: vertex {v}, faces {} and {} should {}share a normal ({nf:?} vs {ng:?}; groups {} {})", f.0, g.0, if same { "" } else { "not " }, mesh.faces()[f.0 as usize].smoothing_groups, mesh.faces()[g.0 as usize].smoothing_groups);
            }
        }
    }
}

fn check_file(file: &str) {
    let doc = fixture(file);
    let mut seen = Vec::new();
    for n in &doc.nodes {
        if let FbxKind::Mesh { mesh, .. } = &n.kind {
            assert_shading_matches(&n.name, mesh, file);
            seen.push(n.name.clone());
        }
    }
    seen.sort();
    assert_eq!(seen, ["Cylinder", "FlatCube", "Pyramid", "Sphere"], "{file}");
}

#[test]
fn blender_normals_only_shades_like_blender() {
    check_file("blender_smoothing_off.fbx");
}

#[test]
fn blender_face_smoothing_shades_like_blender() {
    // ByPolygon 0/1: the hard edges between smooth faces live only in the normals.
    check_file("blender_smoothing_face.fbx");
}

#[test]
fn blender_edge_smoothing_shades_like_blender() {
    // ByEdge: per-edge smooth flags.
    check_file("blender_smoothing_edge.fbx");
}

#[test]
fn blender_smoothing_groups_shade_like_blender() {
    // ByPolygon bitmasks, as 3ds Max writes them.
    check_file("blender_smoothing_smooth_group.fbx");
}

fn node(name: &str, mesh: Mesh) -> FbxNode {
    FbxNode { name: name.into(), parent: None, translation: Vec3::ZERO, rotation_deg: Vec3::ZERO, scale: Vec3::ONE, kind: FbxKind::Mesh { mesh, materials: vec![] }, skin: None, blend_shapes: Vec::new() }
}

#[test]
fn export_writes_the_group_bits_by_polygon() {
    // Bits as 3ds Max reads them: LayerElementSmoothing ByPolygon, one int per face.
    let mut mesh = loom_mesh::box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
    for (i, f) in mesh.faces_mut().iter_mut().enumerate() {
        f.smoothing_groups = [0, 1, 2, 1 << 31, 6, 1 << 20][i];
    }
    let bytes = loom_fbx::write(&FbxDocument { nodes: vec![node("Box", mesh.clone())], unit_metres: 0.01, anim: None, ..Default::default() }, &WriteOpts::default());
    let written: Vec<Vec<u32>> = loom_fbx::binary::smoothing_by_geometry(&bytes).into_values().collect();
    assert_eq!(written, [mesh.faces().iter().map(|f| f.smoothing_groups).collect::<Vec<u32>>()]);
    let back = loom_fbx::read(&bytes, 0.01).unwrap();
    let FbxKind::Mesh { mesh: b, .. } = &back.nodes[0].kind else { panic!() };
    assert_eq!(b.faces().iter().map(|f| f.smoothing_groups).collect::<Vec<u32>>(), [0, 1, 2, 1 << 31, 6, 1 << 20]);
}

#[test]
fn imported_groups_survive_a_round_trip() {
    for file in ["blender_smoothing_off.fbx", "blender_smoothing_face.fbx", "blender_smoothing_edge.fbx", "blender_smoothing_smooth_group.fbx"] {
        let doc = fixture(file);
        let bytes = loom_fbx::write(&doc, &WriteOpts::default());
        let back = loom_fbx::read(&bytes, 0.01).unwrap();
        for (a, b) in doc.nodes.iter().zip(&back.nodes) {
            let (FbxKind::Mesh { mesh: ma, .. }, FbxKind::Mesh { mesh: mb, .. }) = (&a.kind, &b.kind) else { continue };
            let ga: Vec<u32> = ma.faces().iter().map(|f| f.smoothing_groups).collect();
            let gb: Vec<u32> = mb.faces().iter().map(|f| f.smoothing_groups).collect();
            assert_eq!(ga, gb, "{file} {}", a.name);
            assert_shading_matches(&b.name, mb, file);
        }
    }
}

#[test]
fn ascii_by_polygon_and_by_edge() {
    // Two quads sharing the edge 1-2, the second tilted 45° up: an ASCII
    // file, whose `Smoothing` ufbx reads as bools (the bits are lost).
    let ascii = |layers: &str| {
        format!(
            "; FBX 7.4.0 project file\nFBXHeaderExtension:  {{\n FBXHeaderVersion: 1003\n FBXVersion: 7400\n}}\n\
             Objects:  {{\n Geometry: 1, \"Geometry::G\", \"Mesh\" {{\n  Vertices: *18 {{\n   a: 0,0,0,1,0,0,1,1,0,0,1,0,2,0,1,2,1,1\n  }}\n\
             PolygonVertexIndex: *8 {{\n   a: 0,1,2,-4,1,4,5,-3\n  }}\n  Edges: *7 {{\n   a: 0,1,2,3,4,5,6\n  }}\n  GeometryVersion: 124\n{layers}\
             Layer: 0 {{\n   Version: 100\n   LayerElement:  {{\n    Type: \"LayerElementSmoothing\"\n    TypedIndex: 0\n   }}\n   LayerElement:  {{\n    Type: \"LayerElementNormal\"\n    TypedIndex: 0\n   }}\n  }}\n }}\n\
             Model: 2, \"Model::Two\", \"Mesh\" {{\n  Version: 232\n }}\n}}\n\
             Connections:  {{\n C: \"OO\",2,0\n C: \"OO\",1,2\n}}\n"
        )
    };
    let read_groups = |layers: &str| {
        let doc = loom_fbx::read(ascii(layers).as_bytes(), 1.0).unwrap();
        let FbxKind::Mesh { mesh, .. } = &doc.nodes[0].kind else { panic!("{:?}", doc.nodes[0].kind) };
        mesh.faces().iter().map(|f| f.smoothing_groups).collect::<Vec<u32>>()
    };
    let smoothing = |mapping: &str, values: &str, n: usize| {
        format!("  LayerElementSmoothing: 0 {{\n   Version: 102\n   Name: \"\"\n   MappingInformationType: \"{mapping}\"\n   ReferenceInformationType: \"Direct\"\n   Smoothing: *{n} {{\n    a: {values}\n   }}\n  }}\n")
    };
    let normals = |corners: [[f32; 3]; 8]| {
        let a: Vec<String> = corners.iter().flatten().map(|x| x.to_string()).collect();
        format!("  LayerElementNormal: 0 {{\n   Version: 101\n   Name: \"\"\n   MappingInformationType: \"ByPolygonVertex\"\n   ReferenceInformationType: \"Direct\"\n   Normals: *24 {{\n    a: {}\n   }}\n  }}\n", a.join(","))
    };
    let (up, tilt) = ([0.0, 0.0, 1.0], [-std::f32::consts::FRAC_1_SQRT_2, 0.0, std::f32::consts::FRAC_1_SQRT_2]);
    let mid = { let m = (Vec3::from(up) + Vec3::from(tilt)).normalize(); [m.x, m.y, m.z] };
    // 3ds Max groups 2 and 4 (no shared bit): the normals split at the shared edge.
    let g = read_groups(&(smoothing("ByPolygon", "2,4", 2) + &normals([up, up, up, up, tilt, tilt, tilt, tilt])));
    assert_eq!(g[0] & g[1], 0, "{g:?}");
    assert!(g.iter().all(|x| *x != 0), "{g:?}");
    // Groups 3 and 6 (bit 2 shared): one normal per shared vertex.
    let g = read_groups(&(smoothing("ByPolygon", "3,6", 2) + &normals([up, mid, mid, up, mid, tilt, tilt, mid])));
    assert_ne!(g[0] & g[1], 0, "{g:?}");
    // A flat face (0) stays flat.
    let g = read_groups(&(smoothing("ByPolygon", "0,1", 2) + &normals([up, up, up, up, tilt, tilt, tilt, tilt])));
    assert_eq!(g[0], 0, "{g:?}");
    assert_ne!(g[1], 0, "{g:?}");
    // ByEdge over the `Edges` list (0-1, 1-2, 2-3, 3-0, 1-4, 4-5, 5-2): the shared edge 1-2 hard.
    let g = read_groups(&smoothing("ByEdge", "1,0,1,1,1,1,1", 7));
    assert_eq!(g[0] & g[1], 0, "{g:?}");
    assert!(g.iter().all(|x| *x != 0), "{g:?}");
    let g = read_groups(&smoothing("ByEdge", "1,1,1,1,1,1,1", 7));
    assert_ne!(g[0] & g[1], 0, "{g:?}");
}
