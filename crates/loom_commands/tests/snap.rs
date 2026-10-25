use glam::{Vec2, Vec3};
use loom_commands::snap::{self, SnapHit, SnapKind, SnapMode, SnapQuery};
use loom_commands::{execute, AppState, Command, CoordSystem, PrimitiveKind};

fn state() -> AppState {
    AppState::new()
}

/// Top view: world X/Y map to screen pixels 1:1 (y up), origin at (0, 0).
fn top_project(w: Vec3) -> Option<Vec2> {
    Some(Vec2::new(w.x, -w.y))
}

fn query<'a>(pointer: Vec2, project: &'a dyn Fn(Vec3) -> Option<Vec2>) -> SnapQuery<'a> {
    SnapQuery {
        pointer,
        ray: (Vec3::new(pointer.x, -pointer.y, 100.0), -Vec3::Z),
        project,
        plane: (Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z),
        start: None,
        exclude: &[],
        exclude_vertices: &[],
    }
}

#[test]
fn grid_points_snap_rounds_to_spacing() {
    let p = snap::grid_point(Vec3::new(13.0, 27.0, 0.0), (Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z), 10.0);
    assert_eq!(p, Vec3::new(10.0, 30.0, 0.0));
    let l = snap::grid_line(Vec3::new(13.0, 28.0, 0.0), (Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z), 10.0);
    assert_eq!(l, Vec3::new(13.0, 30.0, 0.0));
}

#[test]
fn vertex_snap_picks_nearest_within_radius() {
    let mut s = state();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [100.0, 0.0, 0.0], size: [10.0, 10.0, 10.0] }).unwrap();
    s.snaps_on = true;
    s.snaps.targets.vertex = true;
    // Box corners at x = 95 / 105, y = -5 / 5; pointer 6 px from (95, 5).
    let hit = snap::resolve(&s, &query(Vec2::new(91.0, -1.0), &top_project)).expect("snap");
    assert_eq!(hit.kind, SnapKind::Vertex);
    assert!(hit.point.abs_diff_eq(Vec3::new(95.0, 5.0, 0.0), 1e-4), "{:?}", hit.point);
    // Grid points are secondary: a grid point equally close loses to the vertex.
    s.snaps.targets.grid_points = true;
    let hit = snap::resolve(&s, &query(Vec2::new(95.0, -5.0), &top_project)).expect("snap");
    assert_eq!(hit.kind, SnapKind::Vertex);
}

#[test]
fn no_candidate_in_radius_leaves_point_free() {
    let mut s = state();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [100.0, 0.0, 0.0], size: [10.0, 10.0, 10.0] }).unwrap();
    s.snaps_on = true;
    s.snaps.targets = snap::SnapTargets::none();
    s.snaps.targets.vertex = true;
    assert!(snap::resolve(&s, &query(Vec2::new(60.0, -1.0), &top_project)).is_none());
    s.snaps_on = false;
    s.snaps.targets.grid_points = true;
    assert!(snap::resolve(&s, &query(Vec2::new(1.0, -1.0), &top_project)).is_none());
}

#[test]
fn sequence_of_snapped_moves_lands_exactly() {
    let mut s = state();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.3, 0.7, 0.0], size: [1.0, 1.0, 1.0] }).unwrap();
    s.snaps_on = true;
    s.snaps.targets = snap::SnapTargets::none();
    s.snaps.targets.grid_points = true;
    s.home_grid.spacing = 10.0;
    for cursor in [Vec2::new(11.0, -19.0), Vec2::new(27.0, -33.0), Vec2::new(42.0, -8.0)] {
        let hit: SnapHit = snap::resolve(&s, &query(cursor, &top_project)).expect("grid point in radius");
        let delta = hit.point - s.selection_pivot();
        execute(&mut s, Command::MoveSelection { delta: delta.to_array(), system: Some(CoordSystem::World) }).unwrap();
    }
    assert_eq!(s.selection_pivot(), Vec3::new(40.0, 10.0, 0.0));
}

#[test]
fn angle_snap_rounds_to_increment() {
    let mut s = state();
    assert_eq!(s.snap_angle_deg(23.0), 23.0);
    s.angle_snap_on = true;
    assert_eq!(s.snap_angle_deg(23.0), 25.0);
    assert_eq!(s.snap_angle_deg(-12.4), -10.0);
    s.snaps.angle_deg = 15.0;
    assert_eq!(s.snap_angle_deg(23.0), 30.0);
}

#[test]
fn percent_snap_rounds_scale() {
    let mut s = state();
    assert_eq!(s.snap_scale_factor(1.23), 1.23);
    s.percent_snap_on = true;
    assert!((s.snap_scale_factor(1.23) - 1.2).abs() < 1e-6);
    assert!((s.snap_scale_factor(0.44) - 0.4).abs() < 1e-6);
}

#[test]
fn cycle_snap_type_2d_25d_3d() {
    let mut s = state();
    assert_eq!(s.snaps.mode, SnapMode::Snap3D);
    execute(&mut s, Command::CycleActiveSnapType).unwrap();
    assert_eq!(s.snaps.mode, SnapMode::Snap2D);
    execute(&mut s, Command::CycleActiveSnapType).unwrap();
    assert_eq!(s.snaps.mode, SnapMode::Snap25D);
    execute(&mut s, Command::CycleActiveSnapType).unwrap();
    assert_eq!(s.snaps.mode, SnapMode::Snap3D);
    execute(&mut s, Command::SnapToFrozenObjectsToggle).unwrap();
    assert!(s.snaps.snap_to_frozen);
    execute(&mut s, Command::EnableAxisConstraintsInSnapsToggle).unwrap();
    assert!(s.snaps.use_axis_constraints);
    execute(&mut s, Command::SnapTarget { name: "Midpoint".into(), on: true }).unwrap();
    assert!(s.snaps.targets.midpoint);
    execute(&mut s, Command::HomeGrid { spacing: Some(2.5), major_every: None, perspective_extent: None, inhibit_subdivision: None, inhibit_resize: None, dynamic_update_all: None }).unwrap();
    assert_eq!(s.home_grid.spacing, 2.5);
    execute(&mut s, Command::GridAndSnapSettings).unwrap();
    assert_eq!(s.dialog, Some(loom_commands::Dialog::GridAndSnapSettings));
}

#[test]
fn snap_modes_restrict_to_the_construction_plane() {
    let mut s = state();
    // A box floating above the plane: its corners are off the XY grid.
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [100.0, 0.0, 20.0], size: [10.0, 10.0, 10.0] }).unwrap();
    s.snaps_on = true;
    s.snaps.targets = snap::SnapTargets::none();
    s.snaps.targets.vertex = true;
    let q = query(Vec2::new(95.0, -5.0), &top_project);
    s.snaps.mode = SnapMode::Snap3D;
    assert_eq!(snap::resolve(&s, &q).unwrap().point.z, 20.0);
    s.snaps.mode = SnapMode::Snap25D;
    assert_eq!(snap::resolve(&s, &q).unwrap().point, Vec3::new(95.0, 5.0, 0.0));
    s.snaps.mode = SnapMode::Snap2D;
    assert!(snap::resolve(&s, &q).is_none());
}

fn snap_settings(preview: Option<f32>, radius: Option<f32>) -> Command {
    Command::SnapSettings { mode: None, marker_display: None, marker_size: None, preview_radius_px: preview, radius_px: radius, angle_deg: None, percent: None, use_axis_constraints: None, show_rubber_band: None }
}

#[test]
fn snap_preview_radius_and_snap_radius_push_each_other() {
    let mut s = state();
    assert_eq!((s.snaps.preview_radius_px, s.snaps.radius_px), (30.0, 20.0));
    // Preview below the snap radius lowers the snap radius to it.
    execute(&mut s, snap_settings(Some(15.0), None)).unwrap();
    assert_eq!((s.snaps.preview_radius_px, s.snaps.radius_px), (15.0, 15.0));
    // Snap radius above the preview raises the preview to it.
    execute(&mut s, snap_settings(None, Some(40.0))).unwrap();
    assert_eq!((s.snaps.preview_radius_px, s.snaps.radius_px), (40.0, 40.0));
}

#[test]
fn snap_marker_colour_and_major_lines_minimum() {
    let mut s = state();
    assert_eq!(s.snaps.marker_color, [0, 220, 255]);
    execute(&mut s, Command::SnapMarkerColor { rgb: [255, 0, 0] }).unwrap();
    assert_eq!(s.snaps.marker_color, [255, 0, 0]);
    execute(&mut s, Command::HomeGrid { spacing: None, major_every: Some(1), perspective_extent: None, inhibit_subdivision: None, inhibit_resize: None, dynamic_update_all: None }).unwrap();
    assert_eq!(s.home_grid.major_every, 2, "major lines every 2nd line at least");
}

#[test]
fn user_grids_activate_grids_when_creating() {
    let mut s = state();
    execute(&mut s, Command::CreateHelper { kind: "grid".into(), position: [0.0; 3], size: 50.0 }).unwrap();
    assert_eq!(s.active_grid(), None, "off by default");
    execute(&mut s, Command::UserGridSettings { activate_when_created: Some(true), autogrid_object_space: None }).unwrap();
    let r = execute(&mut s, Command::CreateHelper { kind: "grid".into(), position: [0.0; 3], size: 50.0 }).unwrap();
    assert_eq!(s.active_grid().map(|g| g.0), r["node"].as_u64());
    execute(&mut s, Command::UserGridSettings { activate_when_created: None, autogrid_object_space: Some(true) }).unwrap();
    assert!(s.user_grids.autogrid_object_space && s.user_grids.activate_when_created);
}
