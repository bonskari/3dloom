//! The track bar as a person uses it: a drag on empty track bar selects the
//! keys it spans, Delete over it removes them, Ctrl+Alt drags move the active
//! time segment, and a right-click cancels a key drag.

use super::*;

/// The centres of the position keys on the track bar, left to right.
fn key_xs(h: &Harness) -> Vec<f32> {
    fn walk(s: &egui::Shape, c: egui::Color32, out: &mut Vec<egui::Rect>) {
        match s {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, c, out)),
            egui::Shape::Rect(r) if r.fill == c && r.rect.width() < 7.0 => out.push(r.rect),
            _ => {}
        }
    }
    let mut v = Vec::new();
    let c = h.app.color("Position Keys");
    h.shapes.iter().for_each(|s| walk(&s.shape, c, &mut v));
    let mut xs: Vec<f32> = v.iter().map(|r| r.center().x).collect();
    xs.sort_by(f32::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 1.0);
    xs
}

/// A box with position keys; returns (y of the track bar, x of frame f).
fn keyed(h: &mut Harness, frames: &[i32]) -> (f32, impl Fn(f32) -> f32) {
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    h.cmd(Command::AutoKeyModeToggle);
    for f in frames {
        h.cmd(Command::SetFrame { frame: *f });
        h.cmd(Command::MoveSelection { delta: [5.0, 0.0, 0.0], system: None });
    }
    h.cmd(Command::AutoKeyModeToggle);
    h.cmd(Command::SetFrame { frame: 0 });
    h.frames(2);
    let times = loom_commands::anim_cmds::selection_key_times(&h.app.state);
    let xs = key_xs(h);
    assert_eq!(xs.len(), times.len(), "a key mark per key time {times:?}");
    let (t0, t1, x0, x1) = (times[0], *times.last().unwrap(), xs[0], *xs.last().unwrap());
    let per = (x1 - x0) / (t1 - t0);
    // The keys' y: the centre of a key mark.
    let c = h.app.color("Position Keys");
    let y = h
        .shapes
        .iter()
        .find_map(|s| if let egui::Shape::Rect(r) = &s.shape { (r.fill == c).then(|| r.rect.center().y) } else { None })
        .expect("a key mark");
    (y, move |f: f32| x0 + (f - t0) * per)
}

#[test]
fn a_drag_on_empty_track_bar_selects_the_keys_it_spans_and_delete_removes_them() {
    let mut h = Harness::new();
    let (y, x) = keyed(&mut h, &[10, 20, 40]);
    h.drag(egui::pos2(x(15.0), y), egui::pos2(x(45.0), y));
    let mut sel = h.app.track_bar.selected.clone();
    sel.sort();
    assert_eq!(sel, [20, 40], "the keys between frames 15 and 45");
    let id = *h.app.state.selection.iter().next().unwrap();
    h.run(vec![egui::Event::PointerMoved(egui::pos2(x(30.0), y))]);
    h.frames(1);
    h.key(egui::Key::Delete);
    let times = loom_commands::anim_cmds::selection_key_times(&h.app.state);
    assert!(!times.contains(&20.0) && !times.contains(&40.0) && times.contains(&10.0), "the selected keys are gone: {times:?}");
    assert!(h.app.state.scene.node(id).is_some(), "the object stays");
}

#[test]
fn ctrl_alt_drags_move_the_active_time_segment() {
    let mut h = Harness::new();
    let (y, x) = keyed(&mut h, &[10]);
    h.mods = egui::Modifiers::CTRL | egui::Modifiers::ALT;
    h.drag(egui::pos2(x(50.0), y), egui::pos2(x(60.0), y));
    let (s, e) = h.app.state.frame_range;
    assert!((s - 10).abs() <= 1 && e == 100, "left drag: the start ({s}, {e})");
    h.drag_with(egui::PointerButton::Secondary, egui::pos2(x(50.0), y), egui::pos2(x(30.0), y));
    let (s2, e) = h.app.state.frame_range;
    assert_eq!(s2, s);
    assert!(e < 100, "right drag: the end comes in ({e})");
    h.mods = egui::Modifiers::NONE;
}

#[test]
fn a_right_click_cancels_a_key_drag() {
    let mut h = Harness::new();
    let (y, x) = keyed(&mut h, &[10, 20]);
    let (a, b) = (egui::pos2(x(10.0), y), egui::pos2(x(16.0), y));
    h.run(vec![egui::Event::PointerMoved(a)]);
    h.run(vec![egui::Event::PointerButton { pos: a, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE }]);
    for i in 1..=6 {
        h.run(vec![egui::Event::PointerMoved(a + (b - a) * (i as f32 / 6.0))]);
    }
    h.run(vec![egui::Event::PointerButton { pos: b, button: egui::PointerButton::Secondary, pressed: true, modifiers: egui::Modifiers::NONE }]);
    h.run(vec![egui::Event::PointerButton { pos: b, button: egui::PointerButton::Secondary, pressed: false, modifiers: egui::Modifiers::NONE }]);
    h.run(vec![egui::Event::PointerButton { pos: b, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE }]);
    h.frames(2);
    let times = loom_commands::anim_cmds::selection_key_times(&h.app.state);
    assert!(times.contains(&10.0) && !times.contains(&16.0), "the key stays at 10: {times:?}");
}
