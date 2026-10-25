//! Edit menu selection commands: Select Instances, Select By > Color.
use loom_commands::{execute, AppState, CloneKind, Command, PickMode, PrimitiveKind, SelectOp};
use loom_scene::NodeId;

fn boxed(s: &mut AppState, x: f32) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [10.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn select(s: &mut AppState, ids: &[NodeId]) {
    execute(s, Command::SelectNodes { ids: ids.iter().map(|i| i.0).collect(), op: SelectOp::Replace }).unwrap();
}

fn selected(s: &AppState) -> Vec<NodeId> {
    let mut v: Vec<NodeId> = s.selection.iter().copied().collect();
    v.sort();
    v
}

#[test]
fn select_instances_selects_the_whole_instance_group() {
    let mut s = AppState::new();
    let a = boxed(&mut s, 0.0);
    execute(&mut s, Command::CloneOptions { kind: CloneKind::Instance, copies: 2, name: None, offset: Some([20.0, 0.0, 0.0]), rotate_deg: None, scale: None }).unwrap();
    let copies = selected(&s);
    let other = boxed(&mut s, 100.0);
    execute(&mut s, Command::CloneOptions { kind: CloneKind::Copy, copies: 1, name: None, offset: Some([20.0, 0.0, 0.0]), rotate_deg: None, scale: None }).unwrap();
    // From an instance: the source and every other instance, not the copies.
    select(&mut s, &[copies[1]]);
    execute(&mut s, Command::SelectInstances).unwrap();
    let mut group = vec![a, copies[0], copies[1]];
    group.sort();
    assert_eq!(selected(&s), group);
    // From the source too.
    select(&mut s, &[a]);
    execute(&mut s, Command::SelectInstances).unwrap();
    assert_eq!(selected(&s), group);
    // Unavailable without instances or with several objects selected.
    select(&mut s, &[other]);
    assert!(execute(&mut s, Command::SelectInstances).is_err());
    assert_eq!(selected(&s), vec![other], "the selection stays");
    select(&mut s, &[a, other]);
    assert!(execute(&mut s, Command::SelectInstances).is_err());
}

#[test]
fn select_by_color_pick_selects_the_picked_objects_colour() {
    let mut s = AppState::new();
    let a = boxed(&mut s, 0.0);
    let b = boxed(&mut s, 20.0);
    let c = boxed(&mut s, 40.0);
    for (id, rgb) in [(a, [200, 10, 10]), (b, [10, 200, 10]), (c, [200, 10, 10])] {
        select(&mut s, &[id]);
        execute(&mut s, Command::SetObjectColor { rgb }).unwrap();
    }
    select(&mut s, &[b]);
    execute(&mut s, Command::SelectByColorPick).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::SelectByColor));
    execute(&mut s, Command::PickHit { id: c.0 }).unwrap();
    let mut red = vec![a, c];
    red.sort();
    assert_eq!(selected(&s), red, "every object of the picked object's colour");
    assert_eq!(s.pick_mode, None, "one pick ends the mode");
}
