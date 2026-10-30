//! Track View model: key selection and edits, tangents, out-of-range types,
//! Simplify Curve, Dope Sheet time tools, note tracks, track sets, filters,
//! controller clipboard (phase 8).

use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp};
use loom_commands::track_view::{tree, KeyArg};
use loom_scene::anim::{Ort, TangentType};
use loom_scene::NodeId;

fn keyed_box(s: &mut AppState) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [2.0; 3] }).unwrap();
    let b = *s.selection.iter().next().unwrap();
    execute(s, Command::AutoKeyModeToggle).unwrap();
    for (f, x) in [(10, 10.0), (20, 0.0), (30, 10.0)] {
        execute(s, Command::SetFrame { frame: f }).unwrap();
        let cur = s.scene.node(b).unwrap().transform.position.get_at(f as f32).x;
        execute(s, Command::MoveSelection { delta: [x - cur, 0.0, 0.0], system: None }).unwrap();
    }
    execute(s, Command::AutoKeyModeToggle).unwrap();
    execute(s, Command::SetFrame { frame: 0 }).unwrap();
    execute(s, Command::OpenCurveEditor).unwrap();
    b
}

fn xkeys(s: &AppState, b: NodeId) -> Vec<(f32, f32)> {
    s.scene.node(b).unwrap().transform.position.keys().unwrap().tracks[0].keys.iter().map(|k| (k.time, k.value)).collect()
}

fn key(b: NodeId, time: f32) -> KeyArg {
    KeyArg { id: b.0, track: "Position".into(), comp: 0, time }
}

fn select(s: &mut AppState, b: NodeId, times: &[f32]) {
    execute(s, Command::TrackViewSelectKeys { keys: times.iter().map(|t| key(b, *t)).collect(), op: SelectOp::Replace }).unwrap();
}

#[test]
fn select_and_move_keys() {
    let mut s = AppState::new();
    let b = keyed_box(&mut s);
    select(&mut s, b, &[10.0]);
    execute(&mut s, Command::TrackViewMoveKeys { dt: 3.0, dv: 2.0 }).unwrap();
    assert_eq!(xkeys(&s, b)[1], (13.0, 12.0));
    // The selection follows the moved key, so Nudge Keys Right moves it again.
    execute(&mut s, Command::NudgeKeysRight).unwrap();
    assert_eq!(xkeys(&s, b)[1].0, 14.0);
    // Snap Frames (on) rounds a fractional move.
    execute(&mut s, Command::TrackViewMoveKeys { dt: 0.4, dv: 0.0 }).unwrap();
    assert_eq!(xkeys(&s, b)[1].0, 14.0);
    execute(&mut s, Command::SnapFrames).unwrap();
    execute(&mut s, Command::TrackViewMoveKeys { dt: 0.4, dv: 0.0 }).unwrap();
    assert!((xkeys(&s, b)[1].0 - 14.4).abs() < 1e-4);
    // Align Keys puts the leftmost selected key at the time slider.
    execute(&mut s, Command::SetFrame { frame: 5 }).unwrap();
    execute(&mut s, Command::AlignKeys).unwrap();
    assert_eq!(xkeys(&s, b)[1].0, 5.0);
    // Slide Keys moves the selection and everything after it.
    execute(&mut s, Command::TrackViewSlideKeys { dt: 2.0 }).unwrap();
    let k = xkeys(&s, b);
    assert_eq!((k[1].0, k[2].0, k[3].0), (7.0, 22.0, 32.0));
    // Lock Selection keeps the key selection.
    execute(&mut s, Command::LockSelection).unwrap();
    execute(&mut s, Command::TrackViewSelectKeys { keys: vec![], op: SelectOp::Replace }).unwrap();
    assert_eq!(s.track_view.selected_keys.len(), 1);
    // Delete removes the selected keys.
    execute(&mut s, Command::LockSelection).unwrap();
    execute(&mut s, Command::TrackViewDeleteKeys).unwrap();
    assert_eq!(xkeys(&s, b).len(), 3);
}

#[test]
fn scale_keys_and_values() {
    let mut s = AppState::new();
    let b = keyed_box(&mut s);
    select(&mut s, b, &[10.0, 20.0, 30.0]);
    execute(&mut s, Command::TrackViewScaleKeys { factor: 2.0, origin: Some(10.0) }).unwrap();
    let k = xkeys(&s, b);
    assert_eq!((k[1].0, k[2].0, k[3].0), (10.0, 30.0, 50.0));
    execute(&mut s, Command::TrackViewScaleValues { factor: 0.5, origin: 0.0 }).unwrap();
    let k = xkeys(&s, b);
    assert_eq!((k[1].1, k[2].1, k[3].1), (5.0, 0.0, 5.0));
}

#[test]
fn set_tangents_on_selection() {
    let mut s = AppState::new();
    let b = keyed_box(&mut s);
    select(&mut s, b, &[20.0]);
    execute(&mut s, Command::SetTangentsToStepped).unwrap();
    let ks = &s.scene.node(b).unwrap().transform.position.keys().unwrap().tracks[0].keys;
    assert_eq!((ks[2].in_type, ks[2].out_type), (TangentType::Step, TangentType::Step));
    assert_eq!(ks[1].out_type, TangentType::Auto, "only the selected key");
    execute(&mut s, Command::TrackViewSetTangents { tangent: TangentType::Fast, side: Some("in".into()) }).unwrap();
    let ks = &s.scene.node(b).unwrap().transform.position.keys().unwrap().tracks[0].keys;
    assert_eq!((ks[2].in_type, ks[2].out_type), (TangentType::Fast, TangentType::Step));
    execute(&mut s, Command::SetTangentsToSpline).unwrap();
    execute(&mut s, Command::BreakTangents).unwrap();
    assert!(s.scene.node(b).unwrap().transform.position.keys().unwrap().tracks[0].keys[2].broken);
    execute(&mut s, Command::UnifyTangents).unwrap();
    assert!(!s.scene.node(b).unwrap().transform.position.keys().unwrap().tracks[0].keys[2].broken);
}

#[test]
fn parameter_curve_out_of_range_types() {
    let mut s = AppState::new();
    let b = keyed_box(&mut s);
    execute(&mut s, Command::TrackViewHighlight { items: vec![format!("{}:Position", b.0)], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::TrackViewSetOrt { before: Ort::Constant, after: Ort::Cycle }).unwrap();
    let p = &s.scene.node(b).unwrap().transform.position;
    assert!((p.get_at(40.0).x - p.get_at(10.0).x).abs() < 1e-4, "cycles with period 30");
    execute(&mut s, Command::ParameterCurveOutOfRangeTypes).unwrap();
    assert!(s.dialog.is_some(), "the chart row opens the dialog");
}

#[test]
fn simplify_curve_reduces_keys_within_threshold() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [2.0; 3] }).unwrap();
    let b = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    // A straight ramp keyed every frame.
    for f in 1..=20 {
        execute(&mut s, Command::SetFrame { frame: f }).unwrap();
        execute(&mut s, Command::MoveSelection { delta: [1.0, 0.0, 0.0], system: None }).unwrap();
    }
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::OpenCurveEditor).unwrap();
    execute(&mut s, Command::TrackViewHighlight { items: vec![format!("{}:Position", b.0)], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::TrackViewReduceKeys { threshold: 0.5 }).unwrap();
    let k = xkeys(&s, b);
    assert!(k.len() < 6, "{} keys left", k.len());
    let p = &s.scene.node(b).unwrap().transform.position;
    for f in 0..=20 {
        assert!((p.get_at(f as f32).x - f as f32).abs() <= 0.5 + 1e-3, "frame {f}: {}", p.get_at(f as f32).x);
    }
}

#[test]
fn time_tools_scale_reverse_insert_cut_copy_paste() {
    let mut s = AppState::new();
    let b = keyed_box(&mut s);
    execute(&mut s, Command::OpenDopeSheet).unwrap();
    execute(&mut s, Command::TrackViewHighlight { items: vec![format!("{}:Position", b.0)], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::TrackViewInsertTime { at: 15.0, amount: 10.0 }).unwrap();
    let t: Vec<f32> = xkeys(&s, b).iter().map(|k| k.0).collect();
    assert_eq!(t, vec![0.0, 10.0, 30.0, 40.0]);
    execute(&mut s, Command::TrackViewSelectTime { start: 10.0, end: 30.0 }).unwrap();
    execute(&mut s, Command::ReverseTime).unwrap();
    assert_eq!(xkeys(&s, b)[1], (10.0, 0.0));
    assert_eq!(xkeys(&s, b)[2], (30.0, 10.0));
    execute(&mut s, Command::TrackViewScaleTime { factor: 0.5 }).unwrap();
    let t: Vec<f32> = xkeys(&s, b).iter().map(|k| k.0).collect();
    assert_eq!(t, vec![0.0, 10.0, 20.0, 30.0], "the block halves from its start, later keys follow");
    execute(&mut s, Command::TrackViewSelectTime { start: 10.0, end: 20.0 }).unwrap();
    execute(&mut s, Command::CopyTime).unwrap();
    execute(&mut s, Command::TrackViewPasteTime { at: Some(50.0), relative: Some(false) }).unwrap();
    let t: Vec<f32> = xkeys(&s, b).iter().map(|k| k.0).collect();
    assert_eq!(t, vec![0.0, 10.0, 20.0, 30.0, 50.0, 60.0]);
    // Exclude Right End Point leaves the key at the block's end out.
    execute(&mut s, Command::ExcludeRightEndPointToggle).unwrap();
    execute(&mut s, Command::CutTime).unwrap();
    let t: Vec<f32> = xkeys(&s, b).iter().map(|k| k.0).collect();
    assert_eq!(t, vec![0.0, 10.0, 20.0, 40.0, 50.0], "Cut removes 10..20 and pulls later keys back; the end key survives");
}

#[test]
fn note_tracks_and_track_sets() {
    let mut s = AppState::new();
    let b = keyed_box(&mut s);
    execute(&mut s, Command::TrackViewHighlight { items: vec![format!("{}", b.0)], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::AddNoteTrack).unwrap();
    assert_eq!(s.scene.track_data.note_tracks.len(), 1);
    execute(&mut s, Command::AddNoteKey { note: 0, time: 12.0, text: "foot plant".into() }).unwrap();
    assert_eq!(s.scene.track_data.note_tracks[0].keys[0].text, "foot plant");
    let items: Vec<String> = tree(&s).iter().map(|i| i.id.clone()).collect();
    assert!(items.iter().any(|i| i.ends_with(":note:0")), "{items:?}");
    execute(&mut s, Command::TrackViewHighlight { items: vec![format!("{}:note:0", b.0)], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::DeleteNoteTrack).unwrap();
    assert!(s.scene.track_data.note_tracks.is_empty());
    // Track sets store highlighted tracks by name.
    execute(&mut s, Command::TrackViewHighlight { items: vec![format!("{}:Position", b.0), format!("{}:Height", b.0)], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::TrackSetCreate { name: "Legs".into() }).unwrap();
    execute(&mut s, Command::TrackViewHighlight { items: vec![], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::TrackSetSelect { name: "Legs".into(), op: SelectOp::Replace }).unwrap();
    assert_eq!(s.track_view.highlighted.len(), 2);
    // Visibility track: added, keyed, removed.
    execute(&mut s, Command::TrackViewHighlight { items: vec![format!("{}", b.0)], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::AddVisibilityTrack).unwrap();
    assert!(s.scene.node(b).unwrap().visibility.is_some());
    execute(&mut s, Command::DeleteVisibilityTrack).unwrap();
    assert!(s.scene.node(b).unwrap().visibility.is_none());
    // Keyable Property Toggle: Set Keys skips a non-keyable track.
    execute(&mut s, Command::TrackViewHighlight { items: vec![format!("{}:Rotation", b.0)], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::KeyablePropertyToggle).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![b.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SetFrame { frame: 40 }).unwrap();
    execute(&mut s, Command::SetKeys).unwrap();
    assert!(!s.scene.node(b).unwrap().transform.rotation.is_animated());
}

#[test]
fn filters_animated_selected() {
    let mut s = AppState::new();
    let b = keyed_box(&mut s);
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Sphere, position: [9.0, 0.0, 0.0], size: [2.0; 3] }).unwrap();
    let still = *s.selection.iter().next().unwrap();
    let ids = |s: &AppState| tree(s).iter().map(|i| i.id.clone()).collect::<Vec<_>>();
    assert!(ids(&s).contains(&format!("{}", still.0)));
    execute(&mut s, Command::FilterAnimatedTracksToggle).unwrap();
    let only = ids(&s);
    assert!(!only.contains(&format!("{}", still.0)), "{only:?}");
    assert!(only.contains(&format!("{}:Position", b.0)));
    assert!(!only.contains(&format!("{}:Rotation", b.0)));
    execute(&mut s, Command::FilterAnimatedTracksToggle).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![still.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::FilterSelectedObjectsToggle).unwrap();
    let only = ids(&s);
    assert!(only.contains(&format!("{}", still.0)) && !only.contains(&format!("{}", b.0)));
    // Collapse / expand: a collapsed object hides its tracks.
    execute(&mut s, Command::FilterSelectedObjectsToggle).unwrap();
    execute(&mut s, Command::TrackViewHighlight { items: vec![format!("{}", b.0)], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::ExpandObjectToggle).unwrap();
    assert!(!ids(&s).contains(&format!("{}:Position", b.0)));
    execute(&mut s, Command::ExpandObjectToggle).unwrap();
    assert!(ids(&s).contains(&format!("{}:Position", b.0)));
}

#[test]
fn copy_paste_controller_and_make_unique() {
    let mut s = AppState::new();
    let b = keyed_box(&mut s);
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [5.0, 0.0, 0.0], size: [2.0; 3] }).unwrap();
    let c = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::TrackViewHighlight { items: vec![format!("{}:Position", b.0)], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::CopyController).unwrap();
    execute(&mut s, Command::TrackViewHighlight { items: vec![format!("{}:Position", c.0)], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::PasteController).unwrap();
    assert_eq!(s.scene.node(c).unwrap().transform.position, s.scene.node(b).unwrap().transform.position);
    // A float controller does not paste onto a position.
    execute(&mut s, Command::TrackViewHighlight { items: vec![format!("{}:Height", b.0)], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::CopyController).unwrap();
    execute(&mut s, Command::TrackViewHighlight { items: vec![format!("{}:Position", c.0)], op: SelectOp::Replace }).unwrap();
    assert!(execute(&mut s, Command::PasteController).is_err());
    // Make Controller Unique on a track that is not instanced says so.
    assert!(execute(&mut s, Command::MakeControllerUnique).is_err());
}

#[test]
fn every_track_view_chart_row_exists() {
    let reg: std::collections::HashSet<&str> = loom_commands::registry().iter().map(|i| i.name).collect();
    for (action, _) in loom_commands::keymap::track_view_chart_rows() {
        let deferred = loom_commands::command::DEFERRED_TRACK_VIEW.iter().any(|(n, _)| *n == action);
        assert!(reg.contains(action) || deferred, "{action}");
        if !deferred {
            assert_eq!(loom_commands::registry().iter().find(|i| i.name == action).unwrap().group, loom_commands::Group::TrackView, "{action}");
        }
    }
    assert_eq!(loom_commands::keymap::track_view_chart_rows().len(), 126);
    let k = loom_commands::keymap::Keymap::track_view();
    assert_eq!(k.lookup(loom_commands::keymap::parse_chord("M").unwrap()), Some("Move Keys"));
    assert_eq!(k.lookup(loom_commands::keymap::parse_chord("Space").unwrap()), Some("Lock Selection"));
}

#[test]
fn normalize_time_and_constant_velocity() {
    let mut s = AppState::new();
    let b = keyed_box(&mut s);
    // Bunch the middle keys up, then Normalize Time spaces them evenly again.
    select(&mut s, b, &[10.0]);
    execute(&mut s, Command::TrackViewMoveKeys { dt: 8.0, dv: 0.0 }).unwrap();
    select(&mut s, b, &[0.0, 18.0, 20.0, 30.0]);
    execute(&mut s, Command::TrackViewNormalizeTime).unwrap();
    let t: Vec<f32> = xkeys(&s, b).iter().map(|k| k.0).collect();
    assert_eq!(t, vec![0.0, 10.0, 20.0, 30.0]);
    // Constant Velocity: the segment after the key is linear.
    select(&mut s, b, &[10.0]);
    execute(&mut s, Command::TrackViewSetConstantVelocity { on: true }).unwrap();
    let p = s.scene.node(b).unwrap().transform.position.clone();
    assert!((p.get_at(15.0).x - 5.0).abs() < 1e-4, "{}", p.get_at(15.0).x);
    assert!(s.scene.node(b).unwrap().transform.position.keys().unwrap().tracks[0].keys[1].constant_velocity);
}

#[test]
fn ease_multiplier_limits_retime_and_range_modes() {
    assert!(loom_commands::command::DEFERRED_TRACK_VIEW.is_empty());
    let mut s = AppState::new();
    let b = keyed_box(&mut s);
    let px = format!("{}:Position", b.0);
    let py = format!("{}:Rotation", b.0);
    execute(&mut s, Command::TrackViewHighlight { items: vec![px.clone()], op: SelectOp::Replace }).unwrap();

    // Apply Ease & Multiplier curves, toggle enable, and delete.
    execute(&mut s, Command::ApplyEaseCurve).unwrap();
    execute(&mut s, Command::ApplyMultiplierCurve).unwrap();
    {
        let tr = &s.scene.node(b).unwrap().transform.position.keys().unwrap().tracks[0];
        assert!(tr.ease_curve.is_some());
        assert!(tr.multiplier_curve.is_some());
    }
    execute(&mut s, Command::EaseMultiplierCurveEnableToggle).unwrap();
    assert!(s.scene.node(b).unwrap().transform.position.keys().unwrap().tracks[0].ease_multiplier_disabled);
    execute(&mut s, Command::DeleteEaseMultiplierCurve).unwrap();
    {
        let tr = &s.scene.node(b).unwrap().transform.position.keys().unwrap().tracks[0];
        assert!(tr.ease_curve.is_none() && tr.multiplier_curve.is_none());
    }

    // Limits: set lower/upper, copy/paste limit only, remove limit.
    execute(&mut s, Command::SetFrame { frame: 2 }).unwrap();
    execute(&mut s, Command::SetLowerLimit).unwrap();
    execute(&mut s, Command::SetFrame { frame: 8 }).unwrap();
    execute(&mut s, Command::SetUpperLimit).unwrap();
    let lo = s.scene.node(b).unwrap().transform.position.keys().unwrap().tracks[0].lower_limit.unwrap();
    let hi = s.scene.node(b).unwrap().transform.position.keys().unwrap().tracks[0].upper_limit.unwrap();
    assert!(lo > 0.0 && hi < 10.0 && lo < hi);
    let pos_at_0 = s.scene.node(b).unwrap().transform.position.get_at(0.0).x;
    let pos_at_10 = s.scene.node(b).unwrap().transform.position.get_at(10.0).x;
    assert!((pos_at_0 - lo).abs() < 1e-4);
    assert!((pos_at_10 - hi).abs() < 1e-4);
    execute(&mut s, Command::CopyLimitOnly).unwrap();
    execute(&mut s, Command::TrackViewHighlight { items: vec![py.clone()], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::PasteLimitOnly).unwrap();
    assert_eq!(s.scene.node(b).unwrap().transform.rotation.keys().unwrap().tracks[0].lower_limit, Some(lo));
    assert_eq!(s.scene.node(b).unwrap().transform.rotation.keys().unwrap().tracks[0].upper_limit, Some(hi));
    execute(&mut s, Command::TrackViewHighlight { items: vec![px.clone()], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::RemoveLimit).unwrap();
    assert!(!s.scene.node(b).unwrap().transform.position.keys().unwrap().tracks[0].limit_enabled);

    // Retime Move Right / Left and EaseIn / EaseOut / Linear tangents.
    select(&mut s, b, &[10.0]);
    execute(&mut s, Command::RetimeMoveRight).unwrap();
    assert!((xkeys(&s, b)[1].0 - 11.0).abs() < 1e-4);
    execute(&mut s, Command::RetimeMoveLeft).unwrap();
    assert!((xkeys(&s, b)[1].0 - 10.0).abs() < 1e-4);
    execute(&mut s, Command::EaseIn).unwrap();
    execute(&mut s, Command::EaseOut).unwrap();
    execute(&mut s, Command::Linear).unwrap();

    // Respect / Ignore Animation Range and Position Ranges Mode.
    execute(&mut s, Command::TrackViewHighlight { items: vec![px], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::IgnoreAnimationRange).unwrap();
    assert!(!s.scene.node(b).unwrap().transform.position.keys().unwrap().tracks[0].respect_range);
    execute(&mut s, Command::RespectAnimationRange).unwrap();
    assert!(s.scene.node(b).unwrap().transform.position.keys().unwrap().tracks[0].respect_range);
    execute(&mut s, Command::PositionRangesMode).unwrap();
    assert!(s.track_view.position_ranges);
}

