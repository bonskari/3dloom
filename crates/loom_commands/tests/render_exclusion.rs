//! Bones, helpers, cameras, lights, targets and a biped never render
//! (Render, bake, the Realistic shading's lit surfaces); a bone renders only
//! with its Renderable turned on.

use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp};
use loom_scene::NodeId;

fn select(s: &mut AppState, ids: &[NodeId]) {
    execute(s, Command::SelectNodes { ids: ids.iter().map(|i| i.0).collect(), op: SelectOp::Replace }).unwrap();
}

#[test]
fn bones_are_not_renderable_by_default_but_render_when_turned_on() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreateBoneChain { points: vec![[0.0; 3], [0.0, 0.0, 20.0], [0.0, 0.0, 40.0]], name: None }).unwrap();
    let bones: Vec<NodeId> = s.scene.nodes().filter(|n| n.object.is_bone()).map(|n| n.id).collect();
    assert!(bones.len() >= 2);
    for b in &bones {
        let n = s.scene.node(*b).unwrap();
        assert!(!n.render.renderable, "Renderable is off for a new bone");
        assert!(!s.scene.renders(n));
        assert!(!loom_commands::bake::is_geometry(n), "bones never bake");
    }
    select(&mut s, &bones[..1]);
    execute(&mut s, Command::SetRenderProperty { name: "Renderable".into(), value: true }).unwrap();
    assert!(s.scene.renders(s.scene.node(bones[0]).unwrap()), "Renderable on: the bone renders");
    assert!(!s.scene.renders(s.scene.node(bones[1]).unwrap()));
}

#[test]
fn helpers_cameras_lights_targets_and_biped_never_render_even_when_renderable() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    for kind in ["dummy", "point", "protractor", "compass", "grid"] {
        execute(&mut s, Command::CreateHelper { kind: kind.into(), position: [30.0, 0.0, 0.0], size: 10.0 }).unwrap();
    }
    execute(&mut s, Command::CreateCameraAt { target: true, position: [0.0, -100.0, 50.0], look_at: [0.0; 3] }).unwrap();
    execute(&mut s, Command::CreateLight { kind: loom_commands::light::LightKind::TargetSpot, position: [50.0, -50.0, 80.0], target: [0.0; 3] }).unwrap();
    execute(&mut s, Command::CreateBiped { position: [80.0, 0.0, 0.0], height: Some(180.0), name: Some("Bip001".into()), arms: None, neck_links: None, spine_links: None, leg_links: None, tail_links: None, fingers: None, finger_links: None, toes: None, toe_links: None, body_type: None, ponytail1_links: None, ponytail2_links: None, prop1: None, prop2: None, prop3: None, ankle_attach: None, triangle_pelvis: None, triangle_neck: None, forefeet: None, knuckles: None, short_thumb: None, twists: None, upper_arm_twists: None, forearm_twists: None, thigh_twists: None, calf_twists: None, horse_link_twists: None }).unwrap();
    assert!(s.scene.nodes().any(|n| n.name.starts_with("Bip001") && n.object.is_bone()), "biped parts are bones");
    // Everything but the bones is made Renderable: still only the box renders.
    let all: Vec<NodeId> = s.scene.nodes().filter(|n| !n.object.is_bone()).map(|n| n.id).collect();
    select(&mut s, &all);
    execute(&mut s, Command::SetRenderProperty { name: "Renderable".into(), value: true }).unwrap();
    for n in s.scene.nodes() {
        let geometry = matches!(n.object, loom_scene::ObjectKind::Box { .. });
        assert_eq!(s.scene.renders(n), geometry, "{} renders only if it is the box", n.name);
        assert_eq!(loom_commands::bake::is_geometry(n), geometry, "{} bakes only if it is the box", n.name);
    }
}
