//! Opt-in verification against real production exports, never fixture geometry.
use glam::Vec3;
use loom_commands::{execute, AppState, Command};
use loom_scene::{Modifier, NodeId};
use std::{
    hash::{Hash, Hasher},
    io::Write,
    path::Path,
};
#[derive(Default)]
struct Fingerprint(std::collections::hash_map::DefaultHasher);
impl Write for Fingerprint {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.write(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn skin_fingerprint(s: &AppState, ids: &[NodeId]) -> u64 {
    let mut out = Fingerprint::default();
    for id in ids {
        let n = s.scene.node(*id).unwrap();
        id.hash(&mut out.0);
        n.parent.hash(&mut out.0);
        for slot in &n.modifiers {
            if matches!(slot.modifier, Modifier::Skin(_)) {
                serde_json::to_writer(&mut out, &slot.modifier).unwrap();
            }
        }
    }
    out.0.finish()
}
#[test]
#[ignore = "requires LOOM_RETARGET_FBX and LOOM_RETARGET_BVH production assets"]
fn real_body342_controls_retarget_skin_and_plant_preserve_native_data() {
    let fbx = std::env::var("LOOM_RETARGET_FBX").unwrap();
    let bvh = std::env::var("LOOM_RETARGET_BVH").unwrap();
    let fbx_bytes = std::fs::read(&fbx).unwrap();
    let mut s = AppState::new();
    s.exchange.fbx_import.metahuman_body_only = true;
    let ids = loom_commands::fbx_io::import(&mut s, Path::new(&fbx)).unwrap();
    let bone_count = ids
        .iter()
        .filter(|id| s.scene.node(**id).unwrap().object.is_bone())
        .count();
    assert_eq!(bone_count, 342);
    let root = *ids
        .iter()
        .find(|id| {
            let n = s.scene.node(**id).unwrap();
            n.object.is_bone() && n.name == "root"
        })
        .unwrap();
    let original_skin = skin_fingerprint(&s, &ids);
    s.fps = 24.;
    loom_commands::bvh_io::import_animation(&mut s, Path::new(&bvh)).unwrap();
    let source = s.scene.animation_clips.last().unwrap().id;
    let retarget = execute(
        &mut s,
        Command::RetargetAnimationClip {
            source_clip: source,
            target_root: root.0,
            mapping: None,
        },
    )
    .unwrap();
    let clip = s
        .scene
        .animation_clips
        .iter()
        .find(|c| Some(c.id) == retarget["clip"].as_u64())
        .unwrap()
        .clone();
    let native: Vec<_> = ids
        .iter()
        .map(|id| (*id, s.scene.node(*id).unwrap().transform.clone()))
        .collect();
    let samples: Vec<_> = [clip.start, clip.start + 2.]
        .into_iter()
        .map(|t| {
            s.scene.at_time(t, || {
                ids.iter()
                    .map(|id| s.scene.world_matrix(*id))
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    execute(
        &mut s,
        Command::CreateLoomRig {
            root: root.0,
            mapping: None,
        },
    )
    .unwrap();
    assert_eq!(s.scene.loom_rigs[0].limbs.len(), 4);
    for (t, expected) in [clip.start, clip.start + 2.].into_iter().zip(samples) {
        for (id, world) in ids.iter().zip(expected) {
            assert!(
                s.scene
                    .at_time(t, || s.scene.world_matrix(*id))
                    .abs_diff_eq(world, 0.001),
                "neutral changed {} at {t}",
                id.0
            );
        }
    }
    execute(
        &mut s,
        Command::SetLoomRigLimb {
            rig: 1,
            limb: "Right Leg".into(),
            solver: "limb".into(),
            blend: 1.,
        },
    )
    .unwrap();
    let right = s.scene.loom_rigs[0]
        .limbs
        .iter()
        .find(|l| l.mapping.name == "Right Leg")
        .unwrap()
        .clone();
    let right_planted = s
        .scene
        .world_matrix(right.mapping.end)
        .transform_point3(Vec3::ZERO);
    let right_next = s.scene.at_time(s.current_frame as f32 + 1., || {
        s.scene
            .world_matrix(right.mapping.end)
            .transform_point3(Vec3::ZERO)
    });
    assert!(
        right_planted.distance(right_next) < 0.02,
        "right-foot drift {right_planted:?}->{right_next:?}"
    );
    eprintln!("Right-foot drift {}", right_planted.distance(right_next));
    let leg = s.scene.loom_rigs[0]
        .limbs
        .iter()
        .find(|l| l.mapping.name == "Left Leg")
        .unwrap()
        .mapping
        .clone();
    let influenced = [leg.start, leg.middle, leg.end];
    let mesh = *ids
        .iter()
        .filter(|id| {
            s.scene.node(**id).unwrap().modifiers.iter().any(|slot| {
                let Modifier::Skin(skin) = &slot.modifier else {
                    return false;
                };
                let indices: Vec<_> = skin
                    .bones
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| influenced.contains(&b.node))
                    .map(|(i, _)| i as u32)
                    .collect();
                skin.weights.iter().any(|weights| {
                    weights
                        .iter()
                        .any(|(bone, weight)| indices.contains(bone) && *weight > 1e-6)
                })
            })
        })
        .max_by_key(|id| s.scene.node(**id).unwrap().object.mesh().vertex_count())
        .expect("real mesh weighted to controlled leg");
    eprintln!(
        "Testing leg-weighted mesh {}",
        s.scene.node(mesh).unwrap().name
    );
    s.current_frame = (clip.start + 2.).round() as i32;
    let baseline = s.scene.at_time(s.current_frame as f32, || {
        s.scene
            .eval_mesh(mesh)
            .value
            .mesh
            .vertices()
            .iter()
            .map(|v| v.position)
            .collect::<Vec<_>>()
    });
    s.auto_key = true;
    execute(
        &mut s,
        Command::SetLoomRigLimb {
            rig: 1,
            limb: "Left Leg".into(),
            solver: "limb".into(),
            blend: 1.,
        },
    )
    .unwrap();
    let leg = s.scene.loom_rigs[0]
        .limbs
        .iter()
        .find(|l| l.mapping.name == "Left Leg")
        .unwrap()
        .clone();
    let hip = s
        .scene
        .world_matrix(leg.mapping.start)
        .transform_point3(Vec3::ZERO);
    let foot = s
        .scene
        .world_matrix(leg.mapping.end)
        .transform_point3(Vec3::ZERO);
    let delta = (hip - foot) * 0.05;
    s.selection = [leg.goal].into();
    execute(
        &mut s,
        Command::MoveSelection {
            delta: delta.to_array(),
            system: None,
        },
    )
    .unwrap();
    let planted = s
        .scene
        .world_matrix(leg.mapping.end)
        .transform_point3(Vec3::ZERO);
    let target = s.scene.world_matrix(leg.goal).transform_point3(Vec3::ZERO);
    assert!(
        planted.distance(target) < 0.01,
        "foot misses goal: {planted:?} != {target:?}"
    );
    let next = s.scene.at_time(s.current_frame as f32 + 1., || {
        s.scene
            .world_matrix(leg.mapping.end)
            .transform_point3(Vec3::ZERO)
    });
    assert!(
        planted.distance(next) < 0.02,
        "plant drift {planted:?}->{next:?}"
    );
    eprintln!(
        "Left-foot drift {}, goal error {}",
        planted.distance(next),
        planted.distance(target)
    );
    let changed = s.scene.eval_mesh(mesh);
    let moved = baseline
        .iter()
        .zip(changed.value.mesh.vertices())
        .filter(|(before, after)| before.distance(after.position) > 1e-4)
        .count();
    assert!(
        moved > 0,
        "control failed to deform actual skinned vertices"
    );
    assert!(changed
        .value
        .mesh
        .vertices()
        .iter()
        .all(|v| v.position.is_finite()));
    for (id, transform) in native {
        assert_eq!(
            s.scene.node(id).unwrap().transform,
            transform,
            "changed native keys {}",
            id.0
        );
    }
    assert_eq!(skin_fingerprint(&s, &ids), original_skin);
    assert_eq!(std::fs::read(&fbx).unwrap(), fbx_bytes);
    eprintln!(
        "Body {bone_count} bones; {} vertices; {moved} moved under planted leg control",
        baseline.len()
    );
}
