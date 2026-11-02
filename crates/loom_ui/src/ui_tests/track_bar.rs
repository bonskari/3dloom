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

/// Right-click the track bar > Configure > Show Frame Numbers turns its
/// frame numbers off and on.
#[test]
fn track_bar_configure_show_frame_numbers() {
    let mut h = Harness::new();
    let (y, x) = keyed(&mut h, &[10, 20]);
    let band = egui::Rect::from_x_y_ranges(0.0..=SCREEN.max.x, (y - 14.0)..=(y + 9.0));
    assert!(h.text_rects("0").iter().any(|r| band.contains_rect(*r)), "frame numbers along the track bar");
    let p = egui::pos2(x(30.0), y);
    h.run(vec![egui::Event::PointerMoved(p)]);
    h.run(vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed: true, modifiers: egui::Modifiers::NONE }]);
    h.run(vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed: false, modifiers: egui::Modifiers::NONE }]);
    h.frames(1);
    let configure = h.find(egui::accesskit::Role::Button, "Configure").center();
    h.run(vec![egui::Event::PointerMoved(configure)]);
    h.frames(3);
    h.click(h.find(egui::accesskit::Role::CheckBox, "Show Frame Numbers").center());
    assert!(!h.app.state.track_bar_frame_numbers);
    h.frames(1);
    assert!(!h.text_rects("0").iter().any(|r| band.contains_rect(*r)), "no frame numbers");
}

/// Right-click a key > Delete Key > one track, or All; Controller
/// Properties lists the selection's procedural controllers.
#[test]
fn track_bar_delete_key_submenu_and_controller_properties() {
    let mut h = Harness::new();
    let (y, x) = keyed(&mut h, &[10, 20]);
    let id = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::AssignController { id: id.0, track: "Scale".into(), controller: "Noise Scale".into() });
    h.app.state.dialog = None;
    let open_menu = |h: &mut Harness, f: f32| {
        let p = egui::pos2(x(f), y);
        h.run(vec![egui::Event::PointerMoved(p)]);
        h.run(vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed: true, modifiers: egui::Modifiers::NONE }]);
        h.run(vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed: false, modifiers: egui::Modifiers::NONE }]);
        h.frames(1);
    };
    let hover = |h: &mut Harness, label: &str| {
        let at = h.find(egui::accesskit::Role::Button, label).center();
        h.run(vec![egui::Event::PointerMoved(at)]);
        h.frames(3);
    };
    // Delete Key > the X position key at frame 10 only.
    open_menu(&mut h, 10.0);
    assert!(h.has(egui::accesskit::Role::Button, "Delete Selected Keys"));
    hover(&mut h, "Delete Key");
    let name = h.app.state.scene.node(id).unwrap().name.clone();
    h.click(h.find(egui::accesskit::Role::Button, &format!("{name}: Position X")).center());
    let k = h.app.state.scene.node(id).unwrap().transform.position.keys().unwrap().clone();
    assert!(k.tracks[0].key_at(10.0).is_none(), "X at 10 deleted");
    assert!(k.tracks[0].key_at(20.0).is_some(), "the other frame keeps its key");
    // Delete Key > All at frame 20.
    open_menu(&mut h, 20.0);
    hover(&mut h, "Delete Key");
    // The submenu's All: the one next to its entries.
    let entry = h.find(egui::accesskit::Role::Button, &format!("{name}: Position X")).center();
    let all = *h.find_all(egui::accesskit::Role::Button, "All").iter().min_by(|a, b| a.center().distance(entry).total_cmp(&b.center().distance(entry))).unwrap();
    h.click(all.center());
    assert!(!loom_commands::anim_cmds::selection_key_times(&h.app.state).contains(&20.0));
    // Controller Properties > the Noise Scale opens its dialog.
    open_menu(&mut h, 30.0);
    hover(&mut h, "Controller Properties");
    h.click(h.find(egui::accesskit::Role::Button, &format!("{name}: Scale (Noise Scale)")).center());
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::NoiseController));
}
