use super::*;
use crate::{Node, PolyObject, ModifierSlot};
use glam::Vec3;

fn fixture() -> (Scene, NodeId, NodeId) {
    let mut scene = Scene::new();
    let bone = scene.add_node(Node::new("bone", ObjectKind::box_object(1.0, 1.0, 1.0)));
    let mut poly = PolyObject::from_mesh(loom_mesh::box_primitive(Vec3::ONE));
    poly.passes_up = Some((loom_mesh::select_ops::SubLevel::Vertex, vec![0, 2]));
    let count = poly.mesh.vertex_count();
    let mut node = Node::new("skin", ObjectKind::EditablePoly(poly));
    let data = SkinData::from_file(&[(bone, Mat4::IDENTITY, crate::skin::Envelope::default())], Mat4::IDENTITY, vec![vec![(0, 1.0)]; count]);
    node.modifiers.push(ModifierSlot::new(Modifier::Skin(Box::new(data))));
    let skin = scene.add_node(node);
    anim::with(0.0, true, || scene.transform_mut(bone).unwrap().transform.position.set(Vec3::ZERO));
    anim::with(10.0, true, || scene.transform_mut(bone).unwrap().transform.position.set(Vec3::new(3.0, 2.0, 1.0)));
    (scene, skin, bone)
}

fn compare(actual: &Evaluated, expected: &Evaluated) {
    assert_eq!(actual.warnings, expected.warnings);
    let (a,b) = (&actual.value, &expected.value);
    assert_eq!(a.mesh, b.mesh);
    assert_eq!(a.selection, b.selection);
    assert_eq!(a.shape_selection, b.shape_selection);
    assert_eq!(a.warnings, b.warnings);
    assert_eq!(a.patch_steps, b.patch_steps);
    assert_eq!(format!("{:?}", a.shape), format!("{:?}", b.shape));
    assert_eq!(format!("{:?}", a.patch), format!("{:?}", b.patch));
    for (a,b) in a.mesh.vertices().iter().zip(b.mesh.vertices()) {
        assert_eq!(a.position.to_array().map(f32::to_bits), b.position.to_array().map(f32::to_bits));
        assert_eq!(a.color.map(f32::to_bits), b.color.map(f32::to_bits));
        assert_eq!(a.illum.map(f32::to_bits), b.illum.map(f32::to_bits));
        assert_eq!(a.alpha.to_bits(), b.alpha.to_bits());
    }
    for (channel,a) in a.mesh.maps() {
        let b = b.mesh.map(*channel).unwrap();
        for (a,b) in a.verts.iter().zip(&b.verts) { assert_eq!(a.to_array().map(f32::to_bits), b.to_array().map(f32::to_bits)); }
    }
}

fn parallel(jobs: Vec<SkinEvaluationJob<'_>>) -> Vec<SkinEvaluationResult> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = jobs.into_iter().map(|job| scope.spawn(move || {
            // A job must restore a worker's preexisting TLS state as well as
            // evaluate using its own frame/render mode.
            anim::with(123.0, true, || anim::with_rendering(|| {
                let result = job.run();
                assert_eq!(anim::time(), 123.0);
                assert!(anim::animate());
                assert!(anim::rendering());
                result
            }))
        })).collect();
        handles.into_iter().rev().map(|h| h.join().unwrap()).collect()
    })
}

#[test]
fn base_output_runs_in_executor_and_propagates_animated_reads() {
    let (mut scene, skin, bone) = fixture();
    scene.transform_mut(bone).unwrap().transform = crate::Transform::default();
    let ObjectKind::EditablePoly(poly) = &mut scene.node_mut(skin).unwrap().object else { unreachable!() };
    anim::with(0.0, true, || poly.subdiv.use_nurms.set(false));
    anim::with(10.0, true, || poly.subdiv.use_nurms.set(true));
    let serial = scene.clone();
    let source = serde_json::to_vec(&scene).unwrap();
    for frame in [0.0, 10.0, 3.25, 10.0, 0.0] {
        scene.set_time(frame);
        serial.set_time(frame);
        let expected = serial.evaluated(skin, None);
        let reads = anim::animated_reads();
        let actual = scene.evaluated_batch(&[skin], |jobs| {
            assert_eq!(anim::animated_reads(), reads, "base object evaluated before the executor");
            assert_eq!(jobs.len(), 1, "animated base output reused at another time");
            parallel(jobs)
        });
        compare(&actual[0], &expected);
        assert!(anim::animated_reads() > reads, "worker's animated base read was not published");
        let cached = scene.evaluated_batch(&[skin], |jobs| { assert!(jobs.is_empty()); vec![] });
        assert!(Arc::ptr_eq(&actual[0], &cached[0]));
    }
    assert_eq!(serde_json::to_vec(&scene).unwrap(), source);
}

#[test]
fn editable_mesh_input_does_not_apply_poly_subdivision_settings() {
    let (mut scene, skin, _) = fixture();
    let node = scene.node_mut(skin).unwrap();
    let ObjectKind::EditablePoly(mut poly) = node.object.clone() else { unreachable!() };
    poly.subdiv.use_nurms.set(true);
    let count = poly.mesh.vertex_count();
    node.object = ObjectKind::EditableMesh(poly);
    let serial = scene.clone();
    for frame in [0.0, 3.25, 10.0, 0.0] {
        scene.set_time(frame);
        serial.set_time(frame);
        let actual = scene.evaluated_batch(&[skin], parallel);
        compare(&actual[0], &serial.evaluated(skin, None));
        assert_eq!(actual[0].value.mesh.vertex_count(), count);
    }
}

#[test]
fn batch_jobs_preserve_full_output_cache_edits_and_worker_tls() {
    let (mut scene, skin, bone) = fixture();
    for step in 0..7 {
        if step == 1 { scene.transform_mut(bone).unwrap().transform.scale.set(Vec3::new(1.0, 2.0, 3.0)); }
        if step == 2 {
            let Modifier::Skin(data) = &mut scene.node_mut(skin).unwrap().modifiers[0].modifier else { unreachable!() };
            data.weights[0] = vec![(0, 0.25), (999, 0.75)];
            data.bones[0].envelope.relative = true;
        }
        if step == 3 {
            let Modifier::Skin(data) = &mut scene.node_mut(skin).unwrap().modifiers[0].modifier else { unreachable!() };
            data.weights.truncate(1); // Preserve serial topology warnings.
        }
        if step == 4 { scene.node_mut(skin).unwrap().modifiers[0].enabled = false; }
        if step == 5 { scene.node_mut(skin).unwrap().modifiers[0].enabled = true; }
        if step == 6 {
            let slot = scene.node(skin).unwrap().modifiers[0].clone();
            scene.node_mut(skin).unwrap().modifiers.push(slot); // General-stack fallback.
        }
        for frame in [0.0, 3.0, 39.25] {
            scene.set_time(frame);
            let serial = scene.clone();
            let expected = serial.evaluated(skin, None);
            let actual = scene.evaluated_batch(&[skin, bone, skin], parallel);
            compare(&actual[0], &expected);
            compare(&actual[1], &serial.evaluated(bone, None));
            assert!(Arc::ptr_eq(&actual[0], &actual[2]));
            let count = scene.eval_count();
            let reads = anim::animated_reads();
            let repeated = scene.evaluated_batch(&[skin], |jobs| { assert!(jobs.is_empty()); Vec::new() });
            assert!(Arc::ptr_eq(&actual[0], &repeated[0]));
            assert_eq!(scene.eval_count(), count);
            if step != 4 { assert!(anim::animated_reads() > reads); }
        }
    }
}

#[test]
fn batch_render_mode_and_changed_time_use_ordinary_cache_rules() {
    let (mut scene, skin, bone) = fixture();
    let second = scene.add_node(Node::new("second", ObjectKind::box_object(1.0, 1.0, 1.0)));
    scene.transform_mut(second).unwrap().transform.position.set(Vec3::Y * 4.0);
    let count = scene.node(skin).unwrap().object.mesh().vertex_count();
    let mut data = SkinData::from_file(&[(bone, Mat4::IDENTITY, crate::skin::Envelope::default()), (second, Mat4::IDENTITY, crate::skin::Envelope::default())], Mat4::IDENTITY, vec![vec![(0, 0.4), (1, 0.6)]; count]);
    data.fast_updates = true;
    scene.node_mut(skin).unwrap().modifiers[0].modifier = Modifier::Skin(Box::new(data));
    scene.set_time(3.0);
    let serial = scene.clone();
    let viewport = scene.evaluated_batch(&[skin], parallel);
    compare(&viewport[0], &serial.evaluated(skin, None));
    anim::with_rendering(|| {
        let rendered = scene.evaluated_batch(&[skin], parallel);
        compare(&rendered[0], &serial.evaluated(skin, None));
        assert_ne!(viewport[0].value.mesh, rendered[0].value.mesh, "Fast Updates must differ from full render skinning");
    });
    scene.set_time(6.0);
    let actual = scene.evaluated_batch(&[skin], |jobs| {
        let out = parallel(jobs);
        scene.set_time(7.0); // A caller cannot publish a job from another frame.
        out
    });
    serial.set_time(7.0);
    compare(&actual[0], &serial.evaluated(skin, None));
}

#[test]
#[ignore = "requires LOOM_PERF_SCENE real saved scene"]
fn actual_all_159_skin_poses_match_serial_full_stack() {
    let path = std::env::var("LOOM_PERF_SCENE").expect("provide the real saved scene");
    let mut value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let start = value["settings"]["frame_range"][0].as_i64().unwrap();
    let end = value["settings"]["frame_range"][1].as_i64().unwrap();
    let scene: Scene = serde_json::from_value(value["scene"].take()).unwrap();
    let serial = scene.clone();
    let ids: Vec<_> = scene.nodes().filter(|n| n.modifiers.iter().any(|m| matches!(&m.modifier, Modifier::Skin(_)))).map(|n| n.id).collect();
    assert_eq!(ids.len(), 5);
    for frame in (start..=end).map(|f| f as f32).chain([39.25]) {
        scene.set_time(frame);
        serial.set_time(frame);
        let actual = scene.evaluated_batch(&ids, |jobs| { assert_eq!(jobs.len(), 5); parallel(jobs) });
        for (id, actual) in ids.iter().zip(actual) { compare(&actual, &serial.evaluated(*id, None)); }
    }
}

#[test]
fn geometry_attachment_constraints_keep_serial_cycle_guards() {
    let (mut scene, skin, bone) = fixture();
    scene.node_mut(bone).unwrap().constraints.position = Some(crate::constraint::PositionConstraint::Attachment { target: skin, face: 0, a: 0.2, b: 0.3, align: true });
    scene.set_time(3.0);
    let serial = scene.clone();
    let expected = serial.evaluated(skin, None);
    let actual = scene.evaluated_batch(&[skin], |jobs| {
        assert!(jobs.is_empty(), "geometry-dependent transforms retain serial recursion guards");
        Vec::new()
    });
    compare(&actual[0], &expected);
}

#[test]
fn static_skin_keeps_timeless_cache_and_instanced_slots_use_serial() {
    let (mut scene, skin, bone) = fixture();
    scene.transform_mut(bone).unwrap().transform.position = crate::Param::new(Vec3::ZERO);
    scene.set_time(0.0);
    let first = scene.evaluated_batch(&[skin], parallel);
    scene.set_time(10.0);
    let count = scene.eval_count();
    let second = scene.evaluated_batch(&[skin], |jobs| { assert!(jobs.is_empty()); Vec::new() });
    assert!(Arc::ptr_eq(&first[0], &second[0]));
    assert_eq!(scene.eval_count(), count);
    scene.node_mut(skin).unwrap().modifiers[0].instance_of = Some((skin, 0));
    let serial = scene.clone();
    let actual = scene.evaluated_batch(&[skin], |jobs| { assert!(jobs.is_empty()); Vec::new() });
    compare(&actual[0], &serial.evaluated(skin, None));
}
