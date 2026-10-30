//! Helpers > Dummy, clicked as a person does: a line-drawn Dummy picks by
//! its lines (not by the empty space inside them), and the Modify panel's
//! Display Type radios and Color swatch drive its options.

use super::*;
use loom_scene::ObjectKind;

fn circle_dummy(h: &mut Harness) -> loom_scene::NodeId {
    h.cmd(Command::CreateHelper { kind: "dummy".into(), position: [0.0; 3], size: 80.0 });
    let d = *h.app.state.selection.iter().next().unwrap();
    let circle = loom_scene::node::DUMMY_DISPLAY_TYPES.iter().position(|t| *t == "Circle").unwrap();
    h.cmd(Command::SetParameter { id: d.0, name: "Display Type".into(), value: circle as f64 });
    d
}

#[test]
fn a_line_drawn_dummy_is_picked_by_its_lines_only() {
    let mut h = Harness::new();
    let d = circle_dummy(&mut h);
    wide_view(&mut h);
    h.cmd(Command::SelectNone);
    h.click(h.screen_of(Vec3::ZERO));
    assert!(h.app.state.selection.is_empty(), "inside the circle is empty space");
    h.click(h.screen_of(Vec3::new(40.0, 0.0, 0.0)));
    assert_eq!(h.app.state.selection.iter().copied().collect::<Vec<_>>(), vec![d], "the circle's line picks it");
}

#[test]
fn modify_panel_switches_the_display_type_and_shows_the_options() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 1400.0));
    h.cmd(Command::CreateHelper { kind: "dummy".into(), position: [0.0; 3], size: 10.0 });
    let d = *h.app.state.selection.iter().next().unwrap();
    h.app.panel_tab = crate::Tab::Modify;
    h.frames(2);
    for label in ["Display Type", "Length X", "Use Color", "Constant Screen Size", "Draw On Top", "Show Name", "Axis Tripod"] {
        assert!(!h.text_rects(label).is_empty(), "{label} shows");
    }
    h.click(panel_text(&h, "Diamond"));
    assert!(matches!(&h.app.state.scene.node(d).unwrap().object, ObjectKind::Dummy { display, .. } if display.get() == 5));
    assert_eq!(h.app.state.scene.eval_mesh(d).value.mesh.face_count(), 0, "drawn as lines");
}
