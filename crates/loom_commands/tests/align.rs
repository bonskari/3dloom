use glam::Vec3;
use loom_commands::{execute, AlignPoint, AppState, Axis, CloneKind, Command, MirrorAxis, PickMode, PrimitiveKind, SelectOp};
use loom_scene::NodeId;

fn state() -> AppState {
    AppState::new()
}

fn make_box(s: &mut AppState, pos: [f32; 3], size: [f32; 3]) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: pos, size }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn bounds(s: &AppState, id: NodeId) -> (Vec3, Vec3) {
    s.object_bounds(id).unwrap()
}

fn select(s: &mut AppState, ids: &[NodeId]) {
    execute(s, Command::SelectNodes { ids: ids.iter().map(|i| i.0).collect(), op: SelectOp::Replace }).unwrap();
}

fn align(target: NodeId, axes: [bool; 3], current: AlignPoint, target_pt: AlignPoint) -> Command {
    Command::AlignObjects { target: target.0, x: axes[0], y: axes[1], z: axes[2], current, target_pt, orientation: [false; 3], match_scale: [false; 3] }
}

#[test]
fn align_position_center_to_center() {
    let mut s = state();
    let a = make_box(&mut s, [0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let b = make_box(&mut s, [10.0, 5.0, 3.0], [4.0, 4.0, 4.0]);
    select(&mut s, &[a]);
    execute(&mut s, Command::Align).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::AlignTarget));
    execute(&mut s, Command::PickHit { id: b.0 }).unwrap();
    assert_eq!(s.dialog, Some(loom_commands::Dialog::Align));
    assert_eq!(s.align_target, Some(b));
    execute(&mut s, align(b, [true, true, true], AlignPoint::Center, AlignPoint::Center)).unwrap();
    let (amin, amax) = bounds(&s, a);
    let (bmin, bmax) = bounds(&s, b);
    assert!(((amin + amax) * 0.5).abs_diff_eq((bmin + bmax) * 0.5, 1e-4));
}

#[test]
fn align_max_to_min_stacks_boxes() {
    let mut s = state();
    let a = make_box(&mut s, [0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let b = make_box(&mut s, [10.0, 5.0, 3.0], [4.0, 4.0, 4.0]);
    select(&mut s, &[a]);
    execute(&mut s, align(b, [false, false, true], AlignPoint::Maximum, AlignPoint::Minimum)).unwrap();
    let (_, amax) = bounds(&s, a);
    let (bmin, _) = bounds(&s, b);
    assert!((amax.z - bmin.z).abs() < 1e-4, "{} vs {}", amax.z, bmin.z);
    // X/Y untouched.
    assert_eq!(s.scene.node(a).unwrap().transform.position.get().x, 0.0);
    // Pivot to pivot, orientation and scale matched.
    execute(&mut s, Command::SetTransform { id: b.0, position: None, rotation_deg: Some([0.0, 0.0, 30.0]), scale: Some([1.0, 2.0, 1.0]) }).unwrap();
    execute(&mut s, Command::AlignObjects { target: b.0, x: true, y: true, z: true, current: AlignPoint::PivotPoint, target_pt: AlignPoint::PivotPoint, orientation: [true; 3], match_scale: [true; 3] }).unwrap();
    let ta = &s.scene.node(a).unwrap().transform;
    let tb = &s.scene.node(b).unwrap().transform;
    assert!(ta.position.get().abs_diff_eq(tb.position.get(), 1e-4));
    assert!((s.scene.node(a).unwrap().euler_deg().z - 30.0).abs() < 1e-3, "{:?}", s.scene.node(a).unwrap().euler_deg());
    assert!(ta.scale.get().abs_diff_eq(tb.scale.get(), 1e-5));
}

#[test]
fn quick_align_matches_pivot() {
    let mut s = state();
    let a = make_box(&mut s, [0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let b = make_box(&mut s, [10.0, 5.0, 3.0], [4.0, 4.0, 4.0]);
    select(&mut s, &[a]);
    execute(&mut s, Command::QuickAlign).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::QuickAlignTarget));
    execute(&mut s, Command::PickHit { id: b.0 }).unwrap();
    assert_eq!(s.pick_mode, None);
    assert!(s.scene.node(a).unwrap().transform.position.get().abs_diff_eq(Vec3::new(10.0, 5.0, 3.0), 1e-5));
}

#[test]
fn normal_align_opposes_normals() {
    let mut s = state();
    let a = make_box(&mut s, [0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let _b = make_box(&mut s, [10.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    select(&mut s, &[a]);
    // Source: centre of a's +X face; target: centre of b's -X face.
    let src_p = Vec3::new(1.0, 0.0, 1.0);
    let tgt_p = Vec3::new(9.0, 0.0, 1.0);
    execute(&mut s, Command::NormalAlignFaces { source_point: src_p.to_array(), source_normal: [1.0, 0.0, 0.0], target_point: tgt_p.to_array(), target_normal: [-1.0, 0.0, 0.0], offset: [0.0; 3], angle_deg: 0.0, flip: false }).unwrap();
    let w = s.scene.world_matrix(a);
    // The source point was world (1,0,1) = local (1,0,1); it now sits on the target point.
    assert!(w.transform_point3(src_p).abs_diff_eq(tgt_p, 1e-4), "{:?}", w.transform_point3(src_p));
    assert!(w.transform_vector3(Vec3::X).normalize().abs_diff_eq(Vec3::X, 1e-4), "normals oppose: source +X stays +X against target -X");
    // Flip: the source normal matches the target normal instead.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::NormalAlignFaces { source_point: src_p.to_array(), source_normal: [1.0, 0.0, 0.0], target_point: tgt_p.to_array(), target_normal: [-1.0, 0.0, 0.0], offset: [0.0; 3], angle_deg: 0.0, flip: true }).unwrap();
    let w = s.scene.world_matrix(a);
    assert!(w.transform_vector3(Vec3::X).normalize().abs_diff_eq(-Vec3::X, 1e-4));
    // Picking flow: two surface picks drive the same command and open the dialog.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::NormalAlign).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::NormalAlignSource));
    execute(&mut s, Command::PickSurface { id: a.0, point: src_p.to_array(), normal: [1.0, 0.0, 0.0] }).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::NormalAlignTarget));
    execute(&mut s, Command::PickSurface { id: _b.0, point: tgt_p.to_array(), normal: [-1.0, 0.0, 0.0] }).unwrap();
    assert_eq!(s.pick_mode, None);
    assert_eq!(s.dialog, Some(loom_commands::Dialog::NormalAlign));
    assert!(s.scene.world_matrix(a).transform_point3(src_p).abs_diff_eq(tgt_p, 1e-4));
}

#[test]
fn align_to_view_points_local_z_at_camera() {
    let mut s = state();
    let a = make_box(&mut s, [0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    select(&mut s, &[a]);
    execute(&mut s, Command::ActiveViewport { index: 1 }).unwrap(); // Front
    assert_eq!(s.active().kind, loom_commands::ViewKind::Front);
    execute(&mut s, Command::AlignToViewAxis { axis: Axis::Z, flip: false }).unwrap();
    let z = s.scene.world_matrix(a).z_axis.truncate().normalize();
    assert!(z.abs_diff_eq(Vec3::new(0.0, -1.0, 0.0), 1e-4), "{z}");
    execute(&mut s, Command::AlignToViewAxis { axis: Axis::X, flip: true }).unwrap();
    let x = s.scene.world_matrix(a).x_axis.truncate().normalize();
    assert!(x.abs_diff_eq(Vec3::new(0.0, 1.0, 0.0), 1e-4), "{x}");
    execute(&mut s, Command::AlignToView).unwrap();
    assert_eq!(s.dialog, Some(loom_commands::Dialog::AlignToView));
}

#[test]
fn align_camera_looks_down_the_picked_normal() {
    let mut s = state();
    execute(&mut s, Command::CreateCamera { target: false }).unwrap();
    let cam = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SetTransform { id: cam.0, position: Some([0.0, -20.0, 5.0]), rotation_deg: None, scale: None }).unwrap();
    let _b = make_box(&mut s, [0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    select(&mut s, &[cam]);
    execute(&mut s, Command::AlignCamera).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::AlignCamera));
    execute(&mut s, Command::PickSurface { id: _b.0, point: [1.0, 0.0, 1.0], normal: [1.0, 0.0, 0.0] }).unwrap();
    let w = s.scene.world_matrix(cam);
    let eye = w.w_axis.truncate();
    let forward = -w.z_axis.truncate().normalize();
    assert!(forward.abs_diff_eq(-Vec3::X, 1e-4), "{forward}");
    assert!(eye.y.abs() < 1e-4 && (eye.z - 1.0).abs() < 1e-4 && eye.x > 1.0, "{eye}");
}

#[test]
fn mirror_copy_flips_about_frame_axis() {
    let mut s = state();
    let a = make_box(&mut s, [5.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    execute(&mut s, Command::Mirror).unwrap();
    assert_eq!(s.dialog, Some(loom_commands::Dialog::Mirror));
    execute(&mut s, Command::MirrorObjects { axis: MirrorAxis::X, offset: 0.0, clone: Some(CloneKind::Copy) }).unwrap();
    assert_eq!(s.scene.nodes().count(), 2);
    let c = *s.selection.iter().next().unwrap();
    assert_ne!(c, a);
    // Original untouched; the copy's +X corner (6,1,2) lands at (4,1,2).
    let (amin, amax) = bounds(&s, a);
    assert!(amin.abs_diff_eq(Vec3::new(4.0, -1.0, 0.0), 1e-4) && amax.abs_diff_eq(Vec3::new(6.0, 1.0, 2.0), 1e-4));
    let wc = s.scene.world_matrix(c);
    assert!(wc.transform_point3(Vec3::new(1.0, 1.0, 2.0)).abs_diff_eq(Vec3::new(4.0, 1.0, 2.0), 1e-4), "{:?}", wc.transform_point3(Vec3::new(1.0, 1.0, 2.0)));
    assert!(wc.determinant() < 0.0, "a mirror is a reflection");
    // Offset moves the clone's pivot along the mirror axis.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    select(&mut s, &[a]);
    execute(&mut s, Command::MirrorObjects { axis: MirrorAxis::X, offset: 3.0, clone: Some(CloneKind::Instance) }).unwrap();
    let c = *s.selection.iter().next().unwrap();
    assert!((s.scene.world_matrix(c).w_axis.truncate().x - 2.0).abs() < 1e-4);
    assert_eq!(s.scene.node(c).unwrap().instance_of, Some(a));
}

#[test]
fn mirror_no_clone_mirrors_in_place() {
    let mut s = state();
    let a = make_box(&mut s, [5.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    execute(&mut s, Command::MirrorObjects { axis: MirrorAxis::XY, offset: 0.0, clone: None }).unwrap();
    assert_eq!(s.scene.nodes().count(), 1);
    let w = s.scene.world_matrix(a);
    // XY flips both X and Y about the pivot: (6,1,2) -> (4,-1,2).
    assert!(w.transform_point3(Vec3::new(1.0, 1.0, 2.0)).abs_diff_eq(Vec3::new(4.0, -1.0, 2.0), 1e-4), "{:?}", w.transform_point3(Vec3::new(1.0, 1.0, 2.0)));
    // Mirroring in the Local system of a rotated object flips along its own axis.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::SetTransform { id: a.0, position: None, rotation_deg: Some([0.0, 0.0, 90.0]), scale: None }).unwrap();
    execute(&mut s, Command::ReferenceCoordinateSystem { system: loom_commands::CoordSystem::Local }).unwrap();
    execute(&mut s, Command::MirrorObjects { axis: MirrorAxis::X, offset: 0.0, clone: None }).unwrap();
    let w = s.scene.world_matrix(a);
    // Local +X of a 90°-rotated box is world +Y; the local corner (1,0,0) was at (5,1,0) and flips to (5,-1,0).
    assert!(w.transform_point3(Vec3::new(1.0, 0.0, 0.0)).abs_diff_eq(Vec3::new(5.0, -1.0, 0.0), 1e-4), "{:?}", w.transform_point3(Vec3::new(1.0, 0.0, 0.0)));
}
