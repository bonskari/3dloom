//! Viewport Configuration and the Shading / POV label menus used as a
//! person does: rendering levels, Apply To, Bounding Box display, Adaptive
//! Degradation while orbiting, Select Camera.

use super::*;
use egui::accesskit::Role;

fn menu_entry(h: &Harness, label: &str) -> egui::Pos2 {
    let open = menus(h);
    let all = h.find_all(Role::Button, label.trim());
    all.iter().copied().find(|r| open.iter().any(|m| m.contains_rect(*r))).unwrap_or_else(|| panic!("no menu entry {label:?}")).center()
}

/// Line segments painted in `color`.
fn lines_in(h: &Harness, color: egui::Color32) -> usize {
    fn walk(s: &egui::Shape, c: egui::Color32, n: &mut usize) {
        match s {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, c, n)),
            egui::Shape::LineSegment { stroke, .. } if stroke.color == c => *n += 1,
            _ => {}
        }
    }
    let mut n = 0;
    h.shapes.iter().for_each(|s| walk(&s.shape, color, &mut n));
    n
}

#[test]
fn the_shading_menu_draws_objects_as_bounding_boxes() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [20.0; 3] });
    h.cmd(Command::SelectNone);
    let id = h.app.state.scene.nodes().next().unwrap().id;
    h.cmd(Command::SetWireColor { id: id.0, rgb: [10, 200, 30] });
    maximized(&mut h);
    h.cmd(Command::ZoomExtentsAll);
    let wire = egui::Color32::from_rgb(10, 200, 30);
    assert_eq!(lines_in(&h, wire), 0, "shaded on the GPU: no painted box");
    h.click(label_rect(&h, 2).center());
    h.run(vec![egui::Event::PointerMoved(menu_entry(&h, "Other"))]);
    h.frames(2);
    h.click(menu_entry(&h, "    Bounding Box"));
    h.frames(2);
    assert_eq!(h.app.state.active().shading, loom_commands::Shading::BoundingBox);
    assert_eq!(lines_in(&h, wire), 12, "the box's twelve edges in its wire colour");
}

#[test]
fn rendering_method_applies_a_level_to_all_viewports() {
    let mut h = Harness::new();
    h.cmd(Command::ConfigureViewports { panel: Some("Rendering Method".into()) });
    h.frames(2);
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("Viewport Configuration"))).expect("the dialog");
    h.click(h.text_rect("All Viewports", win).unwrap().center());
    h.click(h.text_rect("Facets", win).unwrap().center());
    assert!(h.app.state.viewports.iter().all(|v| v.shading == loom_commands::Shading::Facets), "every view: Facets");
    h.click(h.text_rect("Simple", win).unwrap().center());
    assert!(h.app.state.viewports.iter().all(|v| v.transparency == loom_commands::Transparency::Simple));
}

#[test]
fn adaptive_degradation_drops_the_active_view_to_wireframe_while_orbiting_too_slowly() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [20.0; 3] });
    maximized(&mut h);
    h.cmd(Command::SetAdaptiveDegradation { general: None, active: Some("Wireframe".into()), maintain_fps: Some(100.0), never_degrade_selected: None, degrade_to_default_lighting: None });
    // The status bar's button turns it on.
    h.click(h.find(Role::Button, "Adaptive Degradation Toggle").center());
    assert!(h.app.state.degradation.on);
    let i = h.app.state.active_viewport;
    let c = h.app.viewport_rects[i].center();
    // Alt + middle drag orbits; mid-drag (30 frames a second, under 100) the view degrades.
    h.mods = egui::Modifiers::ALT;
    h.run(vec![egui::Event::PointerMoved(c)]);
    h.run(vec![egui::Event::PointerButton { pos: c, button: egui::PointerButton::Middle, pressed: true, modifiers: h.mods }]);
    for k in 1..6 {
        h.run(vec![egui::Event::PointerMoved(c + egui::vec2(k as f32 * 10.0, 0.0))]);
    }
    assert!(h.app.degrading, "degrading while orbiting");
    assert_eq!(crate::viewport_widget::shown_shading(&h.app, i), loom_commands::Shading::Wireframe);
    assert_eq!(h.app.state.viewports[i].shading, loom_commands::Shading::SmoothHighlights, "the view keeps its own level");
    h.run(vec![egui::Event::PointerButton { pos: c + egui::vec2(50.0, 0.0), button: egui::PointerButton::Middle, pressed: false, modifiers: h.mods }]);
    h.mods = egui::Modifiers::NONE;
    h.frames(2);
    assert!(!h.app.degrading, "back to its own level after the drag");
}

#[test]
fn the_pov_menu_of_a_camera_view_selects_the_camera_and_its_target() {
    let mut h = Harness::new();
    h.cmd(Command::CreateCamera { target: true });
    let cam = h.app.state.scene.nodes().find(|n| matches!(n.object, loom_scene::ObjectKind::Camera { .. })).unwrap().id;
    maximized(&mut h);
    let i = h.app.state.active_viewport as u32;
    h.cmd(Command::ViewportView { index: i, kind: loom_commands::ViewKind::Camera(cam) });
    h.cmd(Command::SelectNone);
    h.click(label_rect(&h, 1).center());
    h.click(menu_entry(&h, "    Select Camera"));
    assert_eq!(h.app.state.selection.iter().copied().collect::<Vec<_>>(), [cam]);
    h.click(label_rect(&h, 1).center());
    h.click(menu_entry(&h, "    Select Camera Target"));
    let t = *h.app.state.selection.iter().next().unwrap();
    assert_ne!(t, cam, "the target is selected");
}

#[test]
fn grab_viewport_puts_the_active_view_into_the_rendered_frame_window() {
    let mut h = Harness::new();
    maximized(&mut h);
    // [+] > Create Preview > Still Image.
    h.click(label_rect(&h, 0).center());
    h.run(vec![egui::Event::PointerMoved(menu_entry(&h, "Create Preview"))]);
    h.frames(2);
    h.click(menu_entry(&h, "    Still Image (Grab Viewport)"));
    assert!(h.app.grab_pending.is_some(), "a screenshot is asked for");
    // The window answers with the screen's picture.
    let size = [h.screen.width() as usize, h.screen.height() as usize];
    let image = std::sync::Arc::new(egui::ColorImage::new(size, egui::Color32::from_rgb(10, 20, 30)));
    h.run(vec![egui::Event::Screenshot { viewport_id: egui::ViewportId::ROOT, user_data: egui::UserData::default(), image }]);
    h.frames(1);
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    let img = h.app.state.rendered.image.clone().expect("the picture is in the Rendered Frame Window");
    assert!((img.width() as f32 - r.width()).abs() <= 1.0 && (img.height() as f32 - r.height()).abs() <= 1.0, "cut to the viewport: {}x{} vs {r:?}", img.width(), img.height());
    assert_eq!(img.get_pixel(2, 2).0, [10, 20, 30, 255]);
    assert!(h.app.state.rendered.open);
}
