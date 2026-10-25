//! Spacing Tool: Distribution modes, Pick Path along a shape's splines.

use glam::Vec3;
use loom_commands::spacing::{distances, DISTRIBUTIONS};
use loom_commands::{execute, AppState, CloneKind, Command, PickMode, PrimitiveKind, ShapeKind, SpacingContext};
use loom_scene::NodeId;

fn close(a: &[f32], b: &[f32]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3)
}

#[test]
fn distribution_modes_place_objects_as_the_reference_describes() {
    assert_eq!(DISTRIBUTIONS.len(), 19);
    let d = |mode, count, spacing, so, eo| distances(mode, 40.0, count, spacing, so, eo, 0.0);
    // Free Center: from the start, Spacing apart, Count objects.
    assert!(close(&d(0, 3, 10.0, 0.0, 0.0), &[0.0, 10.0, 20.0]));
    // Divide Evenly, Objects at Ends.
    assert!(close(&d(1, 5, 0.0, 0.0, 0.0), &[0.0, 10.0, 20.0, 30.0, 40.0]));
    assert!(close(&d(1, 1, 0.0, 0.0, 0.0), &[20.0]), "one object sits in the middle");
    // Centered, Specify Spacing: as many as fit, centred (45 long fits 5 at 10 apart).
    assert!(close(&distances(2, 45.0, 1, 10.0, 0.0, 0.0, 0.0), &[2.5, 12.5, 22.5, 32.5, 42.5]));
    // End Offset: Count objects toward the start from the end offset.
    assert!(close(&d(3, 3, 5.0, 0.0, 10.0), &[30.0, 25.0, 20.0]));
    // End Offset, Divide Evenly: from the start to the end offset.
    assert!(close(&d(4, 3, 0.0, 0.0, 10.0), &[0.0, 15.0, 30.0]));
    // End Offset, Specify Spacing: from the end offset toward the start, as many as fit.
    assert!(close(&d(5, 1, 12.0, 0.0, 10.0), &[30.0, 18.0, 6.0]));
    // Start Offset modes mirror them.
    assert!(close(&d(6, 3, 5.0, 10.0, 0.0), &[10.0, 15.0, 20.0]));
    assert!(close(&d(7, 3, 0.0, 10.0, 0.0), &[10.0, 25.0, 40.0]));
    assert!(close(&d(8, 1, 12.0, 10.0, 0.0), &[10.0, 22.0, 34.0]));
    // Specify Offsets and Spacing: between the offsets, as many as fit.
    assert!(close(&d(9, 1, 10.0, 5.0, 5.0), &[5.0, 15.0, 25.0, 35.0]));
    // Specify Offsets, Divide Evenly.
    assert!(close(&d(10, 3, 0.0, 5.0, 5.0), &[5.0, 20.0, 35.0]));
    assert!(close(&d(10, 1, 0.0, 5.0, 15.0), &[15.0]), "one object at the centre between the offsets");
    // Space from End: the end offset is locked to the spacing.
    assert!(close(&d(11, 3, 10.0, 0.0, 0.0), &[30.0, 20.0, 10.0]));
    assert!(close(&d(12, 4, 0.0, 0.0, 0.0), &[30.0, 20.0, 10.0, 0.0]));
    assert!(close(&d(13, 1, 15.0, 0.0, 0.0), &[25.0, 10.0]));
    // Space from Start.
    assert!(close(&d(14, 3, 10.0, 0.0, 0.0), &[10.0, 20.0, 30.0]));
    assert!(close(&d(15, 4, 0.0, 0.0, 0.0), &[10.0, 20.0, 30.0, 40.0]));
    assert!(close(&d(16, 1, 15.0, 0.0, 0.0), &[15.0, 30.0]));
    // Specify Spacing, Matching Offsets.
    assert!(close(&d(17, 1, 10.0, 0.0, 0.0), &[10.0, 20.0, 30.0]));
    // Divide Evenly, No Objects at Ends.
    assert!(close(&d(18, 3, 0.0, 0.0, 0.0), &[10.0, 20.0, 30.0]));
}

#[test]
fn edges_context_keeps_end_objects_inside_and_spaces_between_edges() {
    // Objects 4 long: Objects at Ends sit 2 in from each end.
    assert!(close(&distances(1, 40.0, 3, 0.0, 0.0, 0.0, 4.0), &[2.0, 20.0, 38.0]));
    // Start Offset, Specify Spacing with 6 between edges: centres 10 apart.
    assert!(close(&distances(8, 40.0, 1, 6.0, 0.0, 0.0, 4.0), &[2.0, 12.0, 22.0, 32.0]));
}

fn boxed(s: &mut AppState) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0, 0.0, -50.0], size: [2.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

#[test]
fn pick_path_spaces_objects_along_a_curved_shape() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Arc, position: [0.0; 3], size: [20.0, 20.0, 0.0] }).unwrap();
    let arc = *s.selection.iter().next().unwrap();
    for (name, v) in [("Radius", 10.0), ("From", 0.0), ("To", 180.0)] {
        execute(&mut s, Command::SetParameter { id: arc.0, name: name.into(), value: v }).unwrap();
    }
    let b = boxed(&mut s);
    execute(&mut s, Command::SpacingTool).unwrap();
    execute(&mut s, Command::SpacingPickPath).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::SpacingPath));
    execute(&mut s, Command::PickHit { id: b.0 }).unwrap_err();
    execute(&mut s, Command::PickHit { id: arc.0 }).unwrap();
    assert_eq!(s.spacing_path, Some(arc));
    assert_eq!(s.pick_mode, None);
    assert_eq!(s.selection.iter().copied().collect::<Vec<_>>(), vec![b], "the box stays selected");
    let r = execute(&mut s, Command::SpaceAlongPath { path: Some(arc.0), start: [0.0; 3], end: [0.0; 3], distribution: 1, count: 3, spacing: 0.0, start_offset: 0.0, end_offset: 0.0, context: SpacingContext::Centers, follow: true, kind: CloneKind::Copy }).unwrap();
    assert_eq!(r["created"], 3);
    let mut at: Vec<(Vec3, Vec3)> = s.selection.iter().map(|id| (s.scene.world_matrix(*id).w_axis.truncate(), s.scene.world_matrix(*id).x_axis.truncate().normalize())).collect();
    at.sort_by(|a, b| b.0.x.total_cmp(&a.0.x));
    // The ends and the middle of the half circle, local X along its tangent.
    assert!(at[0].0.distance(Vec3::new(10.0, 0.0, 0.0)) < 0.05, "{at:?}");
    assert!(at[1].0.distance(Vec3::new(0.0, 10.0, 0.0)) < 0.2, "{at:?}");
    assert!(at[2].0.distance(Vec3::new(-10.0, 0.0, 0.0)) < 0.05, "{at:?}");
    assert!(at[1].1.distance(-Vec3::X) < 0.06, "Follow: tangent at the top points -X: {at:?}");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.len(), 2, "one undo step");
}

#[test]
fn space_along_points_and_free_center_runs_straight() {
    let mut s = AppState::new();
    boxed(&mut s);
    execute(&mut s, Command::SpaceAlongPath { path: None, start: [0.0; 3], end: [10.0, 0.0, 0.0], distribution: 0, count: 3, spacing: 20.0, start_offset: 0.0, end_offset: 0.0, context: SpacingContext::Centers, follow: false, kind: CloneKind::Copy }).unwrap();
    let mut xs: Vec<f32> = s.selection.iter().map(|id| s.scene.world_matrix(*id).w_axis.x).collect();
    xs.sort_by(f32::total_cmp);
    assert!(close(&xs, &[0.0, 20.0, 40.0]), "Free Center runs on past the end: {xs:?}");
}
