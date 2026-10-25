use glam::Vec3;
use loom_commands::{execute, AppState, CloneKind, Command, PrimitiveKind, SpacingContext};
use loom_scene::NodeId;

fn state() -> AppState {
    AppState::new()
}

fn make_box(s: &mut AppState, pos: [f32; 3]) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: pos, size: [2.0, 2.0, 2.0] }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn array(count: [u32; 3], mv: [f32; 3], rot: [f32; 3]) -> Command {
    Command::ArrayObjects {
        mv,
        rotate: rot,
        scale: [100.0; 3],
        totals: [false; 3],
        re_orient: true,
        uniform: false,
        kind: CloneKind::Copy,
        count: count,
        row_2d: [0.0, 10.0, 0.0],
        row_3d: [0.0, 0.0, 10.0],
    }
}

fn xs(s: &AppState) -> Vec<f32> {
    let mut v: Vec<f32> = s.scene.nodes().map(|n| s.scene.world_matrix(n.id).w_axis.x).collect();
    v.sort_by(f32::total_cmp);
    v
}

#[test]
fn array_1d_count_3_move_x_creates_two_copies() {
    let mut s = state();
    let a = make_box(&mut s, [0.0, 0.0, 0.0]);
    execute(&mut s, Command::Array).unwrap();
    assert_eq!(s.dialog, Some(loom_commands::Dialog::Array));
    execute(&mut s, array([3, 1, 1], [25.0, 0.0, 0.0], [0.0; 3])).unwrap();
    assert_eq!(s.scene.nodes().count(), 3);
    assert_eq!(xs(&s), vec![0.0, 25.0, 50.0]);
    assert!(s.scene.node(a).is_some());
    assert_eq!(s.selection.len(), 1, "the selection moves to the last copy");
    assert_eq!(s.scene.world_matrix(*s.selection.iter().next().unwrap()).w_axis.x, 50.0);
    assert_eq!(s.undo.labels().last().map(String::as_str), Some("Array"));
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.nodes().count(), 1, "one undo removes the whole array");
}

#[test]
fn array_count_1_is_noop_no_undo() {
    let mut s = state();
    make_box(&mut s, [0.0, 0.0, 0.0]);
    let before = s.undo.labels();
    execute(&mut s, array([1, 1, 1], [25.0, 0.0, 0.0], [0.0; 3])).unwrap();
    assert_eq!(s.scene.nodes().count(), 1);
    assert_eq!(s.undo.labels(), before);
}

#[test]
fn array_count_0_rejected() {
    let mut s = state();
    make_box(&mut s, [0.0, 0.0, 0.0]);
    assert!(execute(&mut s, array([0, 1, 1], [25.0, 0.0, 0.0], [0.0; 3])).is_err());
    assert!(execute(&mut s, array([2, 0, 1], [25.0, 0.0, 0.0], [0.0; 3])).is_err());
    assert_eq!(s.scene.nodes().count(), 1);
}

#[test]
fn array_2d_grid_counts() {
    let mut s = state();
    make_box(&mut s, [0.0, 0.0, 0.0]);
    execute(&mut s, array([3, 2, 1], [25.0, 0.0, 0.0], [0.0; 3])).unwrap();
    assert_eq!(s.scene.nodes().count(), 6);
    let mut ys: Vec<f32> = s.scene.nodes().map(|n| s.scene.world_matrix(n.id).w_axis.y).collect();
    ys.sort_by(f32::total_cmp);
    assert_eq!(ys, vec![0.0, 0.0, 0.0, 10.0, 10.0, 10.0]);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::SelectAll).unwrap();
    execute(&mut s, array([2, 2, 2], [25.0, 0.0, 0.0], [0.0; 3])).unwrap();
    assert_eq!(s.scene.nodes().count(), 8);
}

#[test]
fn array_totals_mode_divides_increment() {
    let mut s = state();
    let a = make_box(&mut s, [0.0, 0.0, 0.0]);
    let mut c = array([3, 1, 1], [120.0, 0.0, 0.0], [0.0, 0.0, 90.0]);
    if let Command::ArrayObjects { totals, .. } = &mut c {
        *totals = [true, true, false];
    }
    execute(&mut s, c).unwrap();
    assert_eq!(xs(&s), vec![0.0, 40.0, 80.0]);
    // Total rotation 90 over three members: 0, 30, 60 with Re-Orient.
    let mut zs: Vec<f32> = s.scene.nodes().map(|n| n.euler_deg().z).collect();
    zs.sort_by(f32::total_cmp);
    assert!(zs.iter().zip([0.0, 30.0, 60.0]).all(|(a, b)| (a - b).abs() < 1e-3), "{zs:?}");
    // Rotation orbits around the transform centre: with Use Selection Center on a
    // single box it is the pivot, so positions stay on the X line above.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::SelectAll).unwrap();
    // Without Re-Orient the copies keep the original orientation.
    let mut c = array([2, 1, 1], [0.0; 3], [0.0, 0.0, 90.0]);
    if let Command::ArrayObjects { re_orient, .. } = &mut c {
        *re_orient = false;
    }
    execute(&mut s, Command::SetTransform { id: a.0, position: Some([10.0, 0.0, 0.0]), rotation_deg: None, scale: None }).unwrap();
    execute(&mut s, Command::UseCenter { mode: loom_commands::CenterMode::TransformCoordinate }).unwrap();
    execute(&mut s, c).unwrap();
    let copy = s.scene.nodes().find(|n| n.id != a).unwrap();
    assert!(s.scene.world_matrix(copy.id).w_axis.truncate().abs_diff_eq(Vec3::new(0.0, 10.0, 0.0), 1e-3), "orbits the world origin");
    assert_eq!(copy.euler_deg(), Vec3::ZERO);
}

#[test]
fn spacing_tool_distributes_evenly() {
    let mut s = state();
    let a = make_box(&mut s, [0.0, 0.0, 0.0]);
    execute(&mut s, Command::SpacingTool).unwrap();
    assert_eq!(s.dialog, Some(loom_commands::Dialog::SpacingTool));
    execute(&mut s, Command::SpaceObjects { start: [0.0, 0.0, 0.0], end: [40.0, 0.0, 0.0], count: 5, spacing: None, start_offset: 0.0, end_offset: 0.0, context: SpacingContext::Centers, follow: false, kind: CloneKind::Copy }).unwrap();
    // The original stays; five copies sit at 0, 10, 20, 30, 40.
    assert_eq!(s.scene.nodes().count(), 6);
    let copies: Vec<f32> = s.scene.nodes().filter(|n| n.id != a).map(|n| s.scene.world_matrix(n.id).w_axis.x).collect::<Vec<_>>();
    let mut copies = copies;
    copies.sort_by(f32::total_cmp);
    assert_eq!(copies, vec![0.0, 10.0, 20.0, 30.0, 40.0]);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.nodes().count(), 1);
    execute(&mut s, Command::SelectAll).unwrap();
    // Follow: local X along the path direction (+Y here).
    execute(&mut s, Command::SpaceObjects { start: [0.0, 0.0, 0.0], end: [0.0, 30.0, 0.0], count: 2, spacing: None, start_offset: 5.0, end_offset: 5.0, context: SpacingContext::Centers, follow: true, kind: CloneKind::Instance }).unwrap();
    let mut ys: Vec<f32> = s.scene.nodes().filter(|n| n.id != a).map(|n| s.scene.world_matrix(n.id).w_axis.y).collect();
    ys.sort_by(f32::total_cmp);
    assert_eq!(ys, vec![5.0, 25.0]);
    let c = s.scene.nodes().find(|n| n.id != a).unwrap();
    assert!(s.scene.world_matrix(c.id).x_axis.truncate().normalize().abs_diff_eq(Vec3::Y, 1e-4));
    assert_eq!(c.instance_of, Some(a));
    // Edges context: boxes 2 wide touch end to end over a 10-unit span.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::SelectAll).unwrap();
    execute(&mut s, Command::SpaceObjects { start: [0.0, 0.0, 0.0], end: [10.0, 0.0, 0.0], count: 5, spacing: None, start_offset: 0.0, end_offset: 0.0, context: SpacingContext::Edges, follow: false, kind: CloneKind::Copy }).unwrap();
    let mut xs: Vec<f32> = s.scene.nodes().filter(|n| n.id != a).map(|n| s.scene.world_matrix(n.id).w_axis.x).collect();
    xs.sort_by(f32::total_cmp);
    assert!(xs.iter().zip([1.0, 3.0, 5.0, 7.0, 9.0]).all(|(a, b)| (a - b).abs() < 1e-4), "{xs:?}");
}

#[test]
fn repeating_an_array_continues_it_from_the_last_copy() {
    let mut s = state();
    make_box(&mut s, [0.0, 0.0, 0.0]);
    execute(&mut s, array([3, 1, 1], [25.0, 0.0, 0.0], [0.0; 3])).unwrap();
    execute(&mut s, array([3, 1, 1], [25.0, 0.0, 0.0], [0.0; 3])).unwrap();
    assert_eq!(xs(&s), vec![0.0, 25.0, 50.0, 75.0, 100.0]);
    assert_eq!(s.undo.labels().iter().filter(|l| *l == "Array").count(), 2);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(xs(&s), vec![0.0, 25.0, 50.0], "undo leaves the first array");
}
