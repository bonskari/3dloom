//! Body-only conversion preserves the original weighted surface, not a rigid head substitute.
use glam::{EulerRot, Mat4, Quat, Vec3};
use loom_fbx::{body_only_profile, FbxBlendShape, FbxDocument, FbxKind, FbxNode, FbxSkin};

fn worlds(doc: &FbxDocument) -> Vec<Mat4> {
    let mut out = Vec::new();
    for n in &doc.nodes {
        let r = n.rotation_deg * std::f32::consts::PI / 180.0;
        let local = Mat4::from_scale_rotation_translation(n.scale, Quat::from_euler(EulerRot::XYZ, r.x, r.y, r.z), n.translation);
        out.push(n.parent.map_or(local, |p| out[p] * local));
    }
    out
}
fn add(doc: &mut FbxDocument, name: &str, parent: Option<usize>, bone: bool, offset: Vec3) -> usize {
    let mut node = FbxNode::new(name, if bone { FbxKind::Bone { length: 1.0, size: 0.1 } } else { FbxKind::Empty });
    node.parent = parent;
    node.translation = offset;
    let id = doc.nodes.len();
    doc.nodes.push(node);
    id
}
fn mesh(doc: &mut FbxDocument, name: &str, parent: usize, bones: &[usize], weights: Vec<(u32, f32)>) -> usize {
    let id = add(doc, name, Some(parent), false, Vec3::new(0.3, -0.7, 0.2));
    doc.nodes[id].rotation_deg = Vec3::new(7.0, 13.0, -11.0);
    let geometry = loom_mesh::box_segmented(1.0, 1.0, 1.0, [1, 1, 1]);
    let count = geometry.vertex_count();
    doc.nodes[id].kind = FbxKind::Mesh { mesh: geometry, materials: vec!["skin".into()] };
    let world = worlds(doc);
    doc.nodes[id].skin = Some(FbxSkin {
        bones: bones.iter().map(|b| (*b, world[*b].inverse() * world[id])).collect(),
        mesh_bind: world[id], weights: vec![weights; count],
    });
    id
}
fn fixture() -> FbxDocument {
    let mut d = FbxDocument { unit_metres: 0.01, ..Default::default() };
    let actor = add(&mut d, "Ray", None, false, Vec3::new(12.0, -4.0, 3.0));
    d.nodes[actor].rotation_deg = Vec3::new(0.0, 0.0, 17.0);
    let body = add(&mut d, "Body", Some(actor), false, Vec3::X);
    let root = add(&mut d, "root", Some(body), true, Vec3::ZERO);
    let neck = add(&mut d, "neck_01", Some(root), true, Vec3::Z);
    let upper = add(&mut d, "neck_02", Some(neck), true, Vec3::Z);
    let head = add(&mut d, "head", Some(upper), true, Vec3::Z);
    mesh(&mut d, "Body", body, &[root, neck, upper, head], vec![(0, 0.4), (1, 0.6)]);
    let face = add(&mut d, "Face", Some(actor), false, Vec3::new(4.0, 0.0, 0.0));
    let fr = add(&mut d, "root", Some(face), true, Vec3::ZERO);
    let fn1 = add(&mut d, "neck_01", Some(fr), true, Vec3::new(-3.0, 0.0, 1.0));
    let fn2 = add(&mut d, "neck_02", Some(fn1), true, Vec3::Z);
    let fh = add(&mut d, "head", Some(fn2), true, Vec3::Z);
    let facial_neck = add(&mut d, "FACIAL_Neck", Some(fn1), true, Vec3::new(0.2, -0.1, 0.4));
    let facial_head = add(&mut d, "FACIAL_Head", Some(fh), true, Vec3::new(0.5, 0.3, 0.2));
    d.nodes[facial_head].rotation_deg = Vec3::new(10.0, 20.0, 30.0);
    let facial_lip = add(&mut d, "FACIAL_Lip", Some(facial_head), true, Vec3::new(-0.3, 0.1, 0.2));
    let fm = mesh(&mut d, "Face", face, &[facial_neck, facial_head, fn2, facial_lip], vec![(0, 0.25), (1, 0.25), (2, 0.25), (3, 0.25)]);
    d.nodes[fm].blend_shapes.push(FbxBlendShape { name: "smile".into(), offsets: vec![(0, Vec3::X)], weight: 0.0 });
    let clothes = add(&mut d, "Coat", Some(actor), false, Vec3::new(-2.0, 0.0, 0.0));
    let cr = add(&mut d, "Root", Some(clothes), true, Vec3::new(3.0, 0.0, 0.0));
    let cn = add(&mut d, "neck_01", Some(cr), true, Vec3::Z);
    let cu = add(&mut d, "neck_02", Some(cn), true, Vec3::Z);
    let ch = add(&mut d, "head", Some(cu), true, Vec3::Z);
    mesh(&mut d, "Coat", clothes, &[cr, cn, cu, ch], vec![(0, 0.2), (1, 0.3), (3, 0.5)]);
    d
}
fn surfaces(d: &FbxDocument) -> Vec<(String, Vec<Vec3>)> {
    let world = worlds(d);
    d.nodes.iter().enumerate().filter_map(|(i, n)| {
        let FbxKind::Mesh { mesh, .. } = &n.kind else { return None };
        let points = mesh.vertices().iter().enumerate().map(|(vi, v)| {
            match &n.skin {
                Some(s) => s.weights[vi].iter().map(|(bi, w)| {
                    let (bone, g2b) = &s.bones[*bi as usize];
                    (world[*bone] * *g2b).transform_point3(v.position) * *w
                }).sum(),
                None => world[i].transform_point3(v.position),
            }
        }).collect();
        Some((n.name.clone(), points))
    }).collect()
}
fn assert_surfaces(a: &[(String, Vec<Vec3>)], b: &[(String, Vec<Vec3>)], epsilon: f32) {
    assert_eq!(a.len(), b.len());
    for ((an, av), (bn, bv)) in a.iter().zip(b) {
        assert_eq!(an, bn);
        assert_eq!(av.len(), bv.len());
        for (i, (a, b)) in av.iter().zip(bv).enumerate() {
            assert!((*a - *b).length() <= epsilon, "{an} vertex {i}: {a:?} != {b:?}");
        }
    }
}
#[test]
fn neutral_surface_weights_and_mesh_frames_are_preserved() {
    let source = fixture();
    let result = body_only_profile(&source).unwrap();
    assert_surfaces(&surfaces(&source), &surfaces(&result), 0.0001);
    assert_eq!(result.nodes.iter().filter(|n| matches!(n.kind, FbxKind::Bone { .. })).count(), 4);
    assert!(!result.nodes.iter().any(|n| n.name.starts_with("FACIAL_")));
    for (old, new) in source.nodes.iter().filter(|n| n.skin.is_some()).zip(result.nodes.iter().filter(|n| n.skin.is_some())) {
        match (&old.kind, &new.kind) {
            (FbxKind::Mesh { mesh: a, materials: am }, FbxKind::Mesh { mesh: b, materials: bm }) => { assert_eq!(a, b); assert_eq!(am, bm); }
            _ => panic!("expected mesh"),
        }
        assert_eq!(old.translation, new.translation);
        assert_eq!(old.rotation_deg, new.rotation_deg);
        assert_eq!(old.scale, new.scale);
        let (old, new) = (old.skin.as_ref().unwrap(), new.skin.as_ref().unwrap());
        assert_eq!(old.weights, new.weights);
        assert_eq!(old.mesh_bind, new.mesh_bind);
        assert_eq!(old.bones.len(), new.bones.len());
        assert!(new.weights.iter().all(|w| (w.iter().map(|x| x.1).sum::<f32>() - 1.0).abs() < 1e-6));
    }
    let face = result.nodes.iter().find(|n| n.skin.is_some() && n.name == "Face").unwrap();
    assert!(face.blend_shapes.is_empty());
    let skin = face.skin.as_ref().unwrap();
    assert_eq!(skin.bones[1].0, skin.bones[3].0, "distinct entries retain the same canonical head target");
}
#[test]
fn independent_head_and_neck_motion_reaches_face_and_clothes() {
    let source = fixture();
    for (bone, rotation) in [("neck_01", Vec3::new(0.0, 0.0, 30.0)), ("neck_02", Vec3::new(15.0, 0.0, 0.0)), ("head", Vec3::new(0.0, 40.0, 0.0))] {
        let mut result = body_only_profile(&source).unwrap();
        // The independent reference rotates each original component's corresponding bone.
        let mut reference = source.clone();
        for n in &mut reference.nodes { if n.name == bone { n.rotation_deg += rotation; } }
        result.nodes.iter_mut().find(|n| n.name == bone).unwrap().rotation_deg += rotation;
        assert_surfaces(&surfaces(&reference), &surfaces(&result), 0.0001);
        assert_ne!(surfaces(&source), surfaces(&result));
    }
}
#[test]
fn rejects_active_or_nonfinite_face_morphs_and_omits_animation() {
    for weight in [1.0, f32::NAN] {
        let mut source = fixture();
        source.nodes.iter_mut().find(|n| !n.blend_shapes.is_empty()).unwrap().blend_shapes[0].weight = weight;
        assert!(body_only_profile(&source).unwrap_err().contains("morph"));
    }
    let mut source = fixture();
    source.anim = Some(loom_fbx::FbxTake { tracks: vec![(2, Default::default())], ..Default::default() });
    assert!(body_only_profile(&source).unwrap().anim.is_none());
}
#[test]
fn remaps_metadata_references_and_validates_canonical_bones() {
    let mut source = fixture();
    let duplicate_head = source.nodes.iter().enumerate().rfind(|(_, n)| n.name == "head").unwrap().0;
    let coat = source.nodes.iter().enumerate().find(|(_, n)| n.name == "Coat" && n.skin.is_some()).unwrap().0;
    source.selection_sets.push(("bones".into(), vec![duplicate_head, coat]));
    source.layers.push(loom_fbx::FbxLayer { name: "body".into(), color: Vec3::ONE, visible: true, frozen: false, nodes: vec![duplicate_head, coat] });
    source.instances.push((coat, coat));
    source.subdivision.push((coat, 1, 2));
    let result = body_only_profile(&source).unwrap();
    let head = result.nodes.iter().position(|n| n.name == "head").unwrap();
    let coat = result.nodes.iter().position(|n| n.name == "Coat" && n.skin.is_some()).unwrap();
    assert_eq!(result.selection_sets[0].1, vec![head, coat]);
    assert_eq!(result.layers[0].nodes, vec![head, coat]);
    assert_eq!(result.instances, vec![(coat, coat)]);
    assert_eq!(result.subdivision, vec![(coat, 1, 2)]);
    source.nodes[3].name = "head".into();
    assert!(body_only_profile(&source).is_err());
}
#[test]
fn rejects_invalid_skin_references_unknown_bones_and_ambiguous_body() {
    let mut source = fixture();
    let skin = source.nodes.iter_mut().find_map(|n| n.skin.as_mut()).unwrap();
    skin.bones[0].0 = usize::MAX;
    assert!(body_only_profile(&source).unwrap_err().contains("reference"));
    let mut source = fixture();
    source.nodes.iter_mut().find(|n| n.name == "FACIAL_Lip").unwrap().name = "unknown_joint".into();
    assert!(body_only_profile(&source).unwrap_err().contains("no canonical"));
    let mut source = fixture();
    add(&mut source, "root", Some(1), true, Vec3::ZERO);
    assert!(body_only_profile(&source).unwrap_err().contains("one Body"));
}
#[test]
fn rejects_invalid_weights_without_silently_renormalizing() {
    for weight in [-0.2, f32::NAN, 0.0] {
        let mut source = fixture();
        source.nodes.iter_mut().find_map(|n| n.skin.as_mut()).unwrap().weights[0][0].1 = weight;
        assert!(body_only_profile(&source).unwrap_err().contains("weights"));
    }
}
#[test]
fn external_metahuman_preserves_every_neutral_vertex() {
    let Some(path) = std::env::var_os("LOOM_METAHUMAN_FBX") else { return };
    let source = loom_fbx::read(&std::fs::read(path).unwrap(), 0.01).unwrap();
    let result = body_only_profile(&source).unwrap();
    assert_surfaces(&surfaces(&source), &surfaces(&result), 0.001);
    let before = surfaces(&source);
    let after = surfaces(&result);
    let max_error = before.iter().zip(&after).flat_map(|((_, a), (_, b))| a.iter().zip(b).map(|(a, b)| (*a - *b).length())).fold(0.0f32, f32::max);
    eprintln!("external source: {} -> {} nodes; {} vertices compared; max world error {} cm", source.nodes.len(), result.nodes.len(), before.iter().map(|(_, v)| v.len()).sum::<usize>(), max_error);
    assert!(source.nodes.iter().flat_map(|n| &n.blend_shapes).all(|m| m.weight == 0.0));
    assert!(result.anim.is_none());
    assert!(result.nodes.iter().all(|n| n.blend_shapes.is_empty()));
    for (old, new) in source.nodes.iter().filter_map(|n| n.skin.as_ref()).zip(result.nodes.iter().filter_map(|n| n.skin.as_ref())) {
        assert_eq!(old.weights, new.weights);
        assert_eq!(old.bones.len(), new.bones.len());
        assert_eq!(old.mesh_bind, new.mesh_bind);
    }
    assert_eq!(result.nodes.iter().filter(|n| matches!(n.kind, FbxKind::Bone { .. })).count(), 342);
    assert!(!result.nodes.iter().any(|n| n.name.starts_with("FACIAL_")));
    for n in &result.nodes {
        assert!(n.parent.is_none_or(|p| p < result.nodes.len()));
        if let Some(s) = &n.skin {
            assert!(s.bones.iter().all(|(i, _)| *i < result.nodes.len() && matches!(result.nodes[*i].kind, FbxKind::Bone { .. })));
        }
    }
}
