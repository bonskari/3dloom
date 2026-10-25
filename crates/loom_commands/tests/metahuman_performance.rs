//! Opt-in, full-source CPU benchmark. Run with the real FBX path in LOOM_METAHUMAN_FBX.
use loom_commands::{execute, AppState, Command};
use loom_scene::Modifier;
use std::time::Instant;

#[test]
#[ignore = "release benchmark requires LOOM_METAHUMAN_FBX"]
fn external_metahuman_native_evaluation_cost() {
    let path = std::env::var("LOOM_METAHUMAN_FBX").expect("provide the real MetaHuman FBX");
    let mut s = AppState::new();
    s.exchange.fbx_import.metahuman_body_only = true;
    let started = Instant::now();
    execute(&mut s, Command::ImportFile { path: Some(path) }).unwrap();
    eprintln!("BENCH import_ms={:.3}", started.elapsed().as_secs_f64() * 1000.0);
    let root = s.scene.nodes().find(|n| n.object.is_bone() && n.name == "root").unwrap().id;
    execute(&mut s, Command::CreateLoomRig { root: root.0, mapping: None }).unwrap();
    let meshes: Vec<_> = s.scene.nodes().filter(|n| n.modifiers.iter().any(|m| matches!(m.modifier, Modifier::Skin(_)))).map(|n| n.id).collect();
    let mut vertices = 0;
    for id in &meshes {
        let started = Instant::now();
        let evaluated = s.scene.evaluated(*id, None);
        vertices += evaluated.value.mesh.vertex_count();
        eprintln!("BENCH cold name={} vertices={} ms={:.3}", s.scene.node(*id).unwrap().name, evaluated.value.mesh.vertex_count(), started.elapsed().as_secs_f64() * 1000.0);
        assert!(evaluated.warnings.is_empty(), "{:?}", evaluated.warnings);
    }
    eprintln!("BENCH nodes={} skins={} vertices={vertices}", s.scene.nodes().count(), meshes.len());
    let started = Instant::now();
    for _ in 0..20 { for id in &meshes { std::hint::black_box(s.scene.evaluated(*id, None)); } }
    eprintln!("BENCH idle_cached_eval_frame_ms={:.3}", started.elapsed().as_secs_f64() * 1000.0 / 20.0);
    let started = Instant::now();
    for _ in 0..20 { for n in s.scene.nodes() { std::hint::black_box(s.scene.world_matrix(n.id)); } }
    eprintln!("BENCH idle_all_worlds_ms={:.3}", started.elapsed().as_secs_f64() * 1000.0 / 20.0);
    let started = Instant::now();
    for id in &meshes {
        let node = s.scene.node(*id).unwrap();
        for slot in &node.modifiers {
            if let Modifier::Skin(data) = &slot.modifier { std::hint::black_box(data.clone()); }
        }
    }
    eprintln!("BENCH skin_data_clone_frame_ms={:.3}", started.elapsed().as_secs_f64() * 1000.0);
    let started = Instant::now();
    for id in &meshes {
        let node = s.scene.node(*id).unwrap();
        for slot in &node.modifiers {
            if let Modifier::Skin(data) = &slot.modifier {
                for (i, v) in node.object.mesh().vertices().iter().enumerate() {
                    std::hint::black_box(data.point_weights(i, data.mesh_bind.transform_point3(v.position), None));
                }
            }
        }
    }
    eprintln!("BENCH skin_weight_resolution_frame_ms={:.3}", started.elapsed().as_secs_f64() * 1000.0);
    let goal = s.scene.loom_rigs[0].limbs[0].goal;
    execute(&mut s, Command::SetLoomRigLimb { rig: 1, limb: "Left Arm".into(), solver: "limb".into(), blend: 1.0 }).unwrap();
    let x = s.scene.node(goal).unwrap().transform.position.get().x;
    let linear = (loom_scene::anim::TangentType::Linear, loom_scene::anim::TangentType::Linear);
    let keys = s.scene.node_mut(goal).unwrap().transform.position.ensure_keys().unwrap();
    keys.tracks[0].set_key(0.0, x, linear);
    keys.tracks[0].set_key(30.0, x + 1.0, linear);
    let count = s.scene.eval_count();
    let started = Instant::now();
    for frame in 1..=10 {
        s.scene.set_time(frame as f32);
        for id in &meshes { std::hint::black_box(s.scene.evaluated(*id, None)); }
    }
    eprintln!("BENCH playback_eval_frame_ms={:.3} evaluated_stacks={}", started.elapsed().as_secs_f64() * 1000.0 / 10.0, s.scene.eval_count() - count);
    // Same full-resolution world mesh copy/transform the viewport prepares.
    let started = Instant::now();
    for _ in 0..5 {
        for id in &meshes {
            let mut mesh = s.scene.evaluated(*id, None).value.mesh.clone();
            let world = s.scene.object_matrix(*id);
            loom_mesh::edit_ops::transform_mesh(&mut mesh, world).unwrap();
            std::hint::black_box(mesh);
        }
    }
    eprintln!("BENCH idle_viewport_copy_transform_frame_ms={:.3}", started.elapsed().as_secs_f64() * 1000.0 / 5.0);
}
