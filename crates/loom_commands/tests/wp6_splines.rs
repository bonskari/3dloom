//! Splines as the reference's chess-set and helmet tutorials use them:
//! Refine with Connect, the Refine / Connect Only prompt, the "Close
//! spline?" prompt, Area Selection, the Line pivot, Lathe about the pivot
//! and Extrude Along Spline's Taper Curve / Rotation / preview pick.

use glam::Vec3;
use loom_commands::{execute, AppState, Command, Dialog, PickMode, PolySettingsKind, PrimitiveKind, SelectOp, SelectionMode, ShapeKind, SplineSettings};
use loom_scene::{Modifier, NodeId};

fn square(s: &mut AppState) -> NodeId {
    execute(s, Command::CreateShape { kind: ShapeKind::Rectangle, position: [0.0; 3], size: [10.0, 10.0, 0.0] }).unwrap();
    execute(s, Command::ConvertToEditableSpline).unwrap();
    *s.selection.iter().next().unwrap()
}

fn shape(s: &AppState, id: NodeId) -> loom_shape::Shape {
    s.scene.node(id).unwrap().object.shape().unwrap()
}

fn setting(s: &mut AppState, name: &str, value: f64) {
    execute(s, Command::SplineSettings { name: name.into(), value }).unwrap();
}

fn refine(s: &mut AppState, id: NodeId, segment: u32, t: f32) -> Vec3 {
    let r = execute(s, Command::SplineRefine { node: id.0, segment, t }).unwrap();
    let k = r["knot"].as_u64().unwrap() as usize;
    let sh = shape(s, id);
    let (si, ki) = sh.knot_at(k).unwrap();
    sh.splines[si].knots[ki].pos
}

fn near(a: Vec3, b: Vec3) -> bool {
    a.distance(b) < 1e-3
}

#[test]
fn refine_with_connect_joins_the_inserted_vertices_in_order() {
    let mut s = AppState::new();
    let id = square(&mut s);
    // Connect off: Refine only adds vertices.
    execute(&mut s, Command::RefineSpline).unwrap();
    refine(&mut s, id, 0, 0.5);
    assert_eq!(shape(&s, id).splines.len(), 1);
    execute(&mut s, Command::ExitPickMode).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    // Connect on: each new vertex joins the previous one by a segment.
    setting(&mut s, "Refine Connect", 1.0);
    execute(&mut s, Command::RefineSpline).unwrap();
    let a = refine(&mut s, id, 0, 0.5);
    assert_eq!(shape(&s, id).splines.len(), 1, "one vertex connects nothing yet");
    let b = refine(&mut s, id, 3, 0.5);
    let sh = shape(&s, id);
    assert_eq!(sh.splines.len(), 2, "the second vertex adds the connecting spline");
    assert!(!sh.splines[1].closed);
    assert_eq!(sh.splines[1].knots.len(), 2);
    assert!(near(sh.splines[1].knots[0].pos, a) && near(sh.splines[1].knots[1].pos, b), "{:?}", sh.splines[1].knots);
    let c = refine(&mut s, id, 2, 0.5);
    let sh = shape(&s, id);
    assert_eq!(sh.splines.len(), 2, "the same connecting spline grows");
    assert_eq!(sh.splines[1].knots.len(), 3);
    assert!(near(sh.splines[1].knots[2].pos, c));
    assert_eq!(sh.splines[0].knots.len(), 7, "the refined vertices stay on the square");
    // Right-click (Exit Pick Mode) ends the chain: the next Refine starts a new one.
    execute(&mut s, Command::ExitPickMode).unwrap();
    execute(&mut s, Command::RefineSpline).unwrap();
    refine(&mut s, id, 0, 0.5);
    let sh = shape(&s, id);
    assert_eq!(sh.splines.len(), 2);
    assert_eq!(sh.splines[1].knots.len(), 3, "a new chain does not continue the old one");
}

#[test]
fn refine_on_an_existing_vertex_asks_refine_or_connect_only() {
    let mut s = AppState::new();
    let id = square(&mut s);
    setting(&mut s, "Refine Connect", 1.0);
    execute(&mut s, Command::RefineSpline).unwrap();
    execute(&mut s, Command::SplineRefineAtVertex { node: id.0, knot: 0, segment: 0, t: 0.0 }).unwrap();
    assert_eq!(s.dialog, Some(Dialog::RefineVertex), "the warning opens");
    assert_eq!(shape(&s, id).knot_count(), 4, "nothing happens before the answer");
    // Connect Only: the existing vertex starts the chain, no vertex is added.
    execute(&mut s, Command::SplineRefineReply { connect_only: true, dont_show: false }).unwrap();
    assert_eq!(s.dialog, None);
    assert_eq!(shape(&s, id).knot_count(), 4);
    let b = refine(&mut s, id, 1, 0.5);
    let sh = shape(&s, id);
    assert_eq!(sh.splines.len(), 2);
    assert!(near(sh.splines[1].knots[0].pos, Vec3::new(-5.0, -5.0, 0.0)) && near(sh.splines[1].knots[1].pos, b), "{:?}", sh.splines[1].knots);
    // Refine with "Do not show this message again": the vertex is refined and the choice sticks.
    execute(&mut s, Command::SplineRefineAtVertex { node: id.0, knot: 3, segment: 2, t: 0.99 }).unwrap();
    assert_eq!(s.dialog, Some(Dialog::RefineVertex));
    execute(&mut s, Command::SplineRefineReply { connect_only: false, dont_show: true }).unwrap();
    let sh = shape(&s, id);
    assert_eq!(sh.splines[0].knots.len(), 6, "Refine adds a vertex next to the clicked one");
    assert_eq!(sh.splines[1].knots.len(), 3);
    assert!(near(sh.splines[1].knots[2].pos, sh.splines[0].knots[3].pos));
    execute(&mut s, Command::SplineRefineAtVertex { node: id.0, knot: 0, segment: 0, t: 0.01 }).unwrap();
    assert_eq!(s.dialog, None, "not asked again");
    assert_eq!(shape(&s, id).splines[0].knots.len(), 7, "the remembered choice runs");
}

#[test]
fn area_selection_threshold_defaults_to_a_tenth() {
    assert_eq!(SplineSettings::default().area_threshold, 0.1);
    assert_eq!(AppState::new().spline_settings.area_threshold, 0.1);
}

#[test]
fn clicking_the_first_vertex_asks_to_close_the_spline() {
    let mut s = AppState::new();
    execute(&mut s, Command::LineStart { point: [0.0, 0.0, 0.0] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    for p in [[10.0, 0.0, 0.0], [10.0, 10.0, 0.0]] {
        execute(&mut s, Command::LineAddKnot { node: id.0, point: p, smooth: false }).unwrap();
    }
    execute(&mut s, Command::LineCloseAsk { node: id.0, point: [0.0, 0.0, 0.0], smooth: false }).unwrap();
    assert_eq!(s.dialog, Some(Dialog::CloseSpline));
    assert!(!shape(&s, id).splines[0].closed, "not closed before Yes");
    // No: the click is an ordinary vertex on the first one, the line goes on.
    execute(&mut s, Command::LineCloseReply { close: false }).unwrap();
    assert_eq!(s.dialog, None);
    let sh = shape(&s, id);
    assert!(!sh.splines[0].closed);
    assert_eq!(sh.splines[0].knots.len(), 4);
    execute(&mut s, Command::LineRemoveLastKnot { node: id.0 }).unwrap();
    // Yes closes it.
    execute(&mut s, Command::LineCloseAsk { node: id.0, point: [0.0, 0.0, 0.0], smooth: false }).unwrap();
    execute(&mut s, Command::LineCloseReply { close: true }).unwrap();
    let sh = shape(&s, id);
    assert!(sh.splines[0].closed);
    assert_eq!(sh.splines[0].knots.len(), 3);
}

#[test]
fn a_finished_line_has_its_pivot_at_the_centre_of_its_bounds() {
    let mut s = AppState::new();
    execute(&mut s, Command::LineStart { point: [5.0, 0.0, 0.0] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    for p in [[10.0, 0.0, 0.0], [10.0, 10.0, 0.0], [5.0, 10.0, 0.0]] {
        execute(&mut s, Command::LineAddKnot { node: id.0, point: p, smooth: false }).unwrap();
    }
    execute(&mut s, Command::LineEnd { node: id.0 }).unwrap();
    let pos = s.scene.node(id).unwrap().transform.position.get();
    assert!(near(pos, Vec3::new(7.5, 5.0, 0.0)), "{pos:?}");
    let m = s.scene.object_matrix(id);
    let world: Vec<Vec3> = shape(&s, id).splines[0].knots.iter().map(|k| m.transform_point3(k.pos)).collect();
    assert!(near(world[0], Vec3::new(5.0, 0.0, 0.0)) && near(world[2], Vec3::new(10.0, 10.0, 0.0)), "the knots stay where they were drawn: {world:?}");
}

#[test]
fn lathe_revolves_about_the_pivot_until_an_align_button_is_pressed() {
    let mut s = AppState::new();
    execute(&mut s, Command::LineStart { point: [5.0, 0.0, 0.0] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    for p in [[10.0, 0.0, 0.0], [10.0, 10.0, 0.0], [5.0, 10.0, 0.0]] {
        execute(&mut s, Command::LineAddKnot { node: id.0, point: p, smooth: false }).unwrap();
    }
    execute(&mut s, Command::LineEnd { node: id.0 }).unwrap();
    execute(&mut s, Command::AddModifier { name: "Lathe".into() }).unwrap();
    let Modifier::Lathe { align, .. } = &s.scene.node(id).unwrap().modifiers[0].modifier else { panic!() };
    assert_eq!(align.get(), 3, "no Align button is down");
    // The pivot is the profile's centre (local origin): the axis runs through it.
    let (lo, hi) = s.scene.eval_mesh(id).value.mesh.aabb();
    assert!((lo.x + 2.5).abs() < 1e-3 && (hi.x - 2.5).abs() < 1e-3, "{lo:?} {hi:?}");
    // Min moves the axis to the profile's left side.
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Align".into(), value: 0.0 }).unwrap();
    let (lo, hi) = s.scene.eval_mesh(id).value.mesh.aabb();
    assert!((lo.x + 7.5).abs() < 1e-3 && (hi.x - 2.5).abs() < 1e-3, "{lo:?} {hi:?}");
}

/// A 30-unit poly box with its top face selected and a line path; returns (box, line).
fn box_and_path(s: &mut AppState, path: &[[f32; 3]]) -> (NodeId, NodeId) {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] }).unwrap();
    execute(s, Command::ConvertToEditablePoly).unwrap();
    let b = *s.selection.iter().next().unwrap();
    execute(s, Command::LineStart { point: path[0] }).unwrap();
    let line = *s.selection.iter().next().unwrap();
    for p in &path[1..] {
        execute(s, Command::LineAddKnot { node: line.0, point: *p, smooth: false }).unwrap();
    }
    execute(s, Command::LineEnd { node: line.0 }).unwrap();
    execute(s, Command::SelectNodes { ids: vec![b.0], op: SelectOp::Replace }).unwrap();
    execute(s, Command::SelectionMode { mode: SelectionMode::Face }).unwrap();
    select_top(s, b);
    (b, line)
}

/// The box's top face is the polygon selection (an undone extrusion leaves its caps' ids selected).
fn select_top(s: &mut AppState, b: NodeId) {
    let mesh = s.scene.node(b).unwrap().object.mesh().into_owned();
    let top = (0..mesh.face_count() as u32).find(|f| mesh.face_normal(loom_mesh::FaceId(*f)).z > 0.9).unwrap();
    execute(s, Command::SelectSub { node: None, ids: vec![top], op: SelectOp::Replace }).unwrap();
}

/// Largest |x| / |y| of the box's vertices at height `z`.
fn ring_extent(s: &AppState, b: NodeId, z: f32) -> (f32, f32) {
    let m = s.scene.node(b).unwrap().object.mesh().into_owned();
    let mut e = (0.0f32, 0.0f32);
    for v in m.vertices() {
        let p = v.position;
        if (p.z - z).abs() < 1e-2 {
            e = (e.0.max(p.x.abs()), e.1.max(p.y.abs()));
        }
    }
    e
}

#[test]
fn extrude_along_spline_taper_amount_and_taper_curve() {
    let mut s = AppState::new();
    let (b, line) = box_and_path(&mut s, &[[0.0, 0.0, 0.0], [0.0, 0.0, 40.0]]);
    // A negative Taper Amount narrows the end (the reference's horns use -0.56).
    execute(&mut s, Command::ExtrudeAlongSpline { spline: line.0, segments: 4, taper: -0.5, taper_curve: 0.0, twist: 0.0, rotation: 0.0, align: true }).unwrap();
    let (x, _) = ring_extent(&s, b, 70.0);
    assert!((x - 7.5).abs() < 1e-2, "end ring half width {x}");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    select_top(&mut s, b);
    // Taper Curve bulges the middle and leaves the ends alone.
    execute(&mut s, Command::ExtrudeAlongSpline { spline: line.0, segments: 4, taper: 0.0, taper_curve: 1.0, twist: 0.0, rotation: 0.0, align: true }).unwrap();
    let (mid, _) = ring_extent(&s, b, 50.0);
    let (end, _) = ring_extent(&s, b, 70.0);
    assert!((mid - 18.75).abs() < 1e-2, "middle ring half width {mid}");
    assert!((end - 15.0).abs() < 1e-2, "end ring half width {end}");
}

#[test]
fn extrude_along_spline_rotation_turns_the_path_about_the_face_normal() {
    let mut s = AppState::new();
    let (b, line) = box_and_path(&mut s, &[[0.0, 0.0, 0.0], [0.0, 0.0, 20.0], [20.0, 0.0, 40.0]]);
    execute(&mut s, Command::ExtrudeAlongSpline { spline: line.0, segments: 4, taper: 0.0, taper_curve: 0.0, twist: 0.0, rotation: 0.0, align: true }).unwrap();
    let (_, hi) = s.scene.node(b).unwrap().object.mesh().aabb();
    assert!(hi.x > 25.0 && hi.y < 15.5, "bends towards +X: {hi:?}");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    select_top(&mut s, b);
    execute(&mut s, Command::ExtrudeAlongSpline { spline: line.0, segments: 4, taper: 0.0, taper_curve: 0.0, twist: 0.0, rotation: 90.0, align: true }).unwrap();
    let (_, hi) = s.scene.node(b).unwrap().object.mesh().aabb();
    assert!(hi.y > 25.0 && hi.x < 15.5, "Rotation 90 turns the bend towards +Y: {hi:?}");
}

#[test]
fn pick_spline_in_the_open_dialog_keeps_the_path_for_the_preview() {
    let mut s = AppState::new();
    let (b, line) = box_and_path(&mut s, &[[0.0, 0.0, 0.0], [0.0, 0.0, 40.0]]);
    let faces = s.scene.node(b).unwrap().object.mesh().face_count();
    execute(&mut s, Command::ExtrudeAlongSplineSettings).unwrap();
    assert_eq!(s.dialog, Some(Dialog::PolySettings(PolySettingsKind::ExtrudeAlongSpline)));
    execute(&mut s, Command::ExtrudeAlongSplineMode).unwrap();
    execute(&mut s, Command::PickHit { id: line.0 }).unwrap();
    assert_eq!(s.spline_settings.extrude_path, Some(line), "the dialog holds the picked spline");
    assert_eq!(s.pick_mode, None);
    assert_eq!(s.scene.node(b).unwrap().object.mesh().face_count(), faces, "the dialog's preview runs the extrusion, not the pick");
    for (name, v) in [("Extrude Taper Curve", 0.5), ("Extrude Rotation", 45.0)] {
        setting(&mut s, name, v);
    }
    assert_eq!(s.spline_settings.extrude_taper_curve, 0.5);
    assert_eq!(s.spline_settings.extrude_rotation, 45.0);
    // Without the dialog the pick runs the extrusion straight away.
    execute(&mut s, Command::CloseDialog).unwrap();
    execute(&mut s, Command::ExtrudeAlongSplineMode).unwrap();
    execute(&mut s, Command::PickHit { id: line.0 }).unwrap();
    assert!(s.scene.node(b).unwrap().object.mesh().face_count() > faces);
}

#[test]
fn fillet_and_chamfer_modes_toggle() {
    let mut s = AppState::new();
    square(&mut s);
    execute(&mut s, Command::SplineFilletMode { chamfer: false }).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::SplineFillet));
    execute(&mut s, Command::SplineFilletMode { chamfer: true }).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::SplineChamfer));
    execute(&mut s, Command::SplineFilletMode { chamfer: true }).unwrap();
    assert_eq!(s.pick_mode, None);
}
