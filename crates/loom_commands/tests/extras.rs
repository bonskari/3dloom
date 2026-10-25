//! Transform tools: Snapshot, Clone and Align, Place Highlight.

use glam::Vec3;
use loom_commands::{execute, AppState, CloneKind, Command, PickMode, PrimitiveKind, SelectOp};
use loom_scene::{NodeId, ObjectKind};

fn boxed(s: &mut AppState, at: [f32; 3]) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: at, size: [2.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn select(s: &mut AppState, ids: &[NodeId]) {
    execute(s, Command::SelectNodes { ids: ids.iter().map(|i| i.0).collect(), op: SelectOp::Replace }).unwrap();
}

fn pos(s: &AppState, id: NodeId) -> Vec3 {
    s.scene.world_matrix(id).w_axis.truncate()
}

/// A box keyed at (0,0,0) on frame 0, (50,20,0) on 50 and (100,0,10) on 100.
fn keyed(s: &mut AppState) -> NodeId {
    let b = boxed(s, [0.0; 3]);
    execute(s, Command::AutoKeyModeToggle).unwrap();
    execute(s, Command::SetFrame { frame: 50 }).unwrap();
    execute(s, Command::MoveSelection { delta: [50.0, 20.0, 0.0], system: None }).unwrap();
    execute(s, Command::SetFrame { frame: 100 }).unwrap();
    execute(s, Command::MoveSelection { delta: [50.0, -20.0, 10.0], system: None }).unwrap();
    execute(s, Command::AutoKeyModeToggle).unwrap();
    execute(s, Command::SetFrame { frame: 0 }).unwrap();
    b
}

fn snapshot(s: &mut AppState, range: bool, from: i32, to: i32, copies: u32, method: u32) -> Vec<NodeId> {
    let r = execute(s, Command::Snapshot { range, from, to, copies, method }).unwrap();
    r["created"].as_array().unwrap().iter().map(|v| NodeId(v.as_u64().unwrap())).collect()
}

#[test]
fn snapshot_single_clones_the_object_where_it_is_at_the_current_frame() {
    let mut s = AppState::new();
    let b = keyed(&mut s);
    execute(&mut s, Command::SetFrame { frame: 50 }).unwrap();
    let at50 = pos(&s, b);
    assert!(at50.distance(Vec3::ZERO) > 10.0);
    select(&mut s, &[b]);
    let made = snapshot(&mut s, false, 0, 0, 1, 0);
    assert_eq!(made.len(), 1);
    let c = made[0];
    assert_ne!(c, b);
    assert!(pos(&s, c).distance(at50) < 1e-3);
    // The clone stays put when time moves: no transform animation.
    execute(&mut s, Command::SetFrame { frame: 0 }).unwrap();
    assert!(pos(&s, c).distance(at50) < 1e-3, "{}", pos(&s, c));
    assert!(!s.scene.node(c).unwrap().transform.position.is_animated());
    assert_eq!(s.scene.node(c).unwrap().name, "Box002");
    // One undo removes the snapshot.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.node(c).is_none());
}

#[test]
fn snapshot_range_spaces_the_copies_evenly_in_time() {
    let mut s = AppState::new();
    let b = keyed(&mut s);
    select(&mut s, &[b]);
    let made = snapshot(&mut s, true, 0, 100, 3, 0);
    let at: Vec<Vec3> = made.iter().map(|c| pos(&s, *c)).collect();
    let want = [0.0, 50.0, 100.0].map(|t| s.scene.at_time(t, || pos(&s, b)));
    for (a, w) in at.iter().zip(want) {
        assert!(a.distance(w) < 1e-3, "{at:?}");
    }
    assert_eq!(s.scene.time(), 0.0, "the current frame is kept");
}

#[test]
fn snapshot_instance_and_mesh_methods() {
    let mut s = AppState::new();
    let b = boxed(&mut s, [0.0; 3]);
    execute(&mut s, Command::AddModifier { name: "Bend".into() }).unwrap();
    select(&mut s, &[b]);
    let inst = snapshot(&mut s, false, 0, 0, 1, 1)[0];
    assert_eq!(s.scene.node(inst).unwrap().instance_of, Some(b));
    select(&mut s, &[b]);
    let mesh = snapshot(&mut s, false, 0, 0, 1, 3)[0];
    let n = s.scene.node(mesh).unwrap();
    assert!(matches!(n.object, ObjectKind::EditablePoly(_)), "Mesh: a collapsed copy");
    assert!(n.modifiers.is_empty());
    assert_eq!(n.instance_of, None);
    assert_eq!(n.object.mesh().vertex_count(), s.scene.eval_mesh(b).value.mesh.vertex_count());
}

#[test]
fn clone_and_align_puts_a_clone_on_every_destination() {
    let mut s = AppState::new();
    let src = boxed(&mut s, [0.0; 3]);
    let d1 = boxed(&mut s, [100.0, 0.0, 0.0]);
    execute(&mut s, Command::SetTransform { id: d1.0, position: None, rotation_deg: Some([0.0, 0.0, 90.0]), scale: None }).unwrap();
    let d2 = boxed(&mut s, [0.0, 100.0, 0.0]);
    select(&mut s, &[src]);
    let r = execute(&mut s, Command::CloneAndAlign {
        destinations: vec![d1.0, d2.0],
        kind: CloneKind::Instance,
        link: true,
        position: [true; 3],
        position_offset: [10.0, 0.0, 0.0],
        orientation: [true; 3],
        orientation_offset: [0.0; 3],
        match_scale: [false; 3],
    })
    .unwrap();
    let made: Vec<NodeId> = r["created"].as_array().unwrap().iter().map(|v| NodeId(v.as_u64().unwrap())).collect();
    assert_eq!(made.len(), 2);
    // The offset is in the destination's local axes: d1 turned 90° puts +X along world +Y.
    assert!(pos(&s, made[0]).distance(Vec3::new(100.0, 10.0, 0.0)) < 1e-3, "{}", pos(&s, made[0]));
    assert!(pos(&s, made[1]).distance(Vec3::new(10.0, 100.0, 0.0)) < 1e-3, "{}", pos(&s, made[1]));
    let x = s.scene.world_matrix(made[0]).x_axis.truncate().normalize();
    assert!(x.distance(Vec3::Y) < 1e-4, "orientation follows d1: {x}");
    assert_eq!(s.scene.node(made[0]).unwrap().parent, Some(d1), "Link to Destination");
    assert_eq!(s.scene.node(made[0]).unwrap().instance_of, Some(src));
    assert!(pos(&s, src).distance(Vec3::ZERO) < 1e-4, "the source stays");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.node(made[0]).is_none() && s.scene.node(made[1]).is_none(), "one undo step");
}

#[test]
fn clone_and_align_only_on_chosen_axes() {
    let mut s = AppState::new();
    let src = boxed(&mut s, [5.0, 6.0, 7.0]);
    let d = boxed(&mut s, [100.0, 200.0, 300.0]);
    select(&mut s, &[src]);
    let r = execute(&mut s, Command::CloneAndAlign { destinations: vec![d.0], kind: CloneKind::Copy, link: false, position: [true, false, false], position_offset: [0.0; 3], orientation: [false; 3], orientation_offset: [0.0; 3], match_scale: [false; 3] }).unwrap();
    let c = NodeId(r["created"][0].as_u64().unwrap());
    assert!(pos(&s, c).distance(Vec3::new(100.0, 6.0, 7.0)) < 1e-3, "{}", pos(&s, c));
    assert_eq!(s.scene.node(c).unwrap().parent, None);
}

#[test]
fn clone_and_align_pick_adds_destinations() {
    let mut s = AppState::new();
    let src = boxed(&mut s, [0.0; 3]);
    let d = boxed(&mut s, [50.0, 0.0, 0.0]);
    select(&mut s, &[src]);
    execute(&mut s, Command::CloneAndAlignDialog).unwrap();
    execute(&mut s, Command::CloneAndAlignPick).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::CloneAlignDestination));
    execute(&mut s, Command::PickHit { id: d.0 }).unwrap();
    execute(&mut s, Command::PickHit { id: src.0 }).unwrap_err();
    assert_eq!(s.clone_align_destinations, vec![d], "a source is not a destination");
    assert_eq!(s.pick_mode, Some(PickMode::CloneAlignDestination), "Pick stays on until clicked again");
    assert_eq!(s.selection.iter().copied().collect::<Vec<_>>(), vec![src], "the sources stay selected");
}

#[test]
fn place_highlight_reflects_the_view_ray_keeping_the_distance() {
    let mut s = AppState::new();
    let light = boxed(&mut s, [0.0, 0.0, 100.0]);
    select(&mut s, &[light]);
    execute(&mut s, Command::PlaceHighlight).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::PlaceHighlight));
    let ray = Vec3::new(1.0, 0.0, -1.0).normalize();
    execute(&mut s, Command::PlaceHighlightAt { point: [0.0; 3], normal: [0.0, 0.0, 1.0], ray: ray.to_array() }).unwrap();
    let want = Vec3::new(1.0, 0.0, 1.0).normalize() * 100.0;
    assert!(pos(&s, light).distance(want) < 1e-3, "{}", pos(&s, light));
    // Its -Z looks at the highlight point.
    let fwd = -s.scene.world_matrix(light).z_axis.truncate().normalize();
    assert!(fwd.distance(-want.normalize()) < 1e-4, "{fwd}");
    // Dragging on: the distance is the one the object had before Place Highlight.
    execute(&mut s, Command::PlaceHighlightAt { point: [10.0, 0.0, 0.0], normal: [0.0, 0.0, 1.0], ray: [0.0, 0.0, -1.0] }).unwrap();
    let d = (Vec3::new(10.0, 0.0, 0.0) - Vec3::new(0.0, 0.0, 100.0)).length();
    assert!(pos(&s, light).distance(Vec3::new(10.0, 0.0, d)) < 1e-3, "{}", pos(&s, light));
}
