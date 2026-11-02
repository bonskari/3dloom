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

/// Rendering Method > Rendering Options > Shade Selected Faces (F2) toggles
/// from the dialog too.
#[test]
fn rendering_method_shade_selected_faces_checkbox() {
    let mut h = Harness::new();
    h.cmd(Command::ConfigureViewports { panel: Some("Rendering Method".into()) });
    h.frames(2);
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("Viewport Configuration"))).expect("the dialog");
    let before = h.app.state.shade_selected_faces;
    let c = h.text_rect("Shade Selected Faces", win).unwrap().center();
    // Hover first, as a person's pointer arrives before the press.
    h.run(vec![egui::Event::PointerMoved(c)]);
    h.click(c);
    assert_eq!(h.app.state.shade_selected_faces, !before);
    pause(&mut h);
    h.click(c);
    assert_eq!(h.app.state.shade_selected_faces, before);
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
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::CreateStillImage), "Create Still Image File asks for a label");
    h.frames(2);
    h.click_text_anywhere("Grab");
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

/// Dragging the vertical split line moves it; right-clicking a split line >
/// Reset Layout puts both lines back.
#[test]
fn right_click_a_splitter_resets_the_layout() {
    let mut h = Harness::new();
    let default = loom_commands::viewport::LayoutSplits::default();
    let [a, b] = [h.app.viewport_rects[0], h.app.viewport_rects[1]];
    let split = egui::pos2((a.max.x + b.min.x) * 0.5, a.center().y);
    h.drag(split, split + egui::vec2(-120.0, 0.0));
    assert!(h.app.state.splits.x < default.x - 0.05, "the drag moved the line: {:?}", h.app.state.splits.x);
    let [a, b] = [h.app.viewport_rects[0], h.app.viewport_rects[1]];
    let split = egui::pos2((a.max.x + b.min.x) * 0.5, a.center().y);
    right_click(&mut h, split);
    h.click(menu_entry(&h, "Reset Layout"));
    assert_eq!((h.app.state.splits.x, h.app.state.splits.y), (default.x, default.y));
}

/// Walkthrough keys: Q / Z change the step's speed, ] / [ / Alt+[ its size,
/// Space locks vertical rotation for drags, Shift+Space levels the view.
#[test]
fn walkthrough_speed_step_size_lock_and_level_keys() {
    let mut h = Harness::new();
    h.cmd(Command::WalkThroughViewMode);
    let eye = |h: &Harness| h.app.state.active().camera.eye_position();
    let step = |h: &mut Harness| {
        let a = eye(h);
        h.key(egui::Key::W);
        (eye(h) - a).length()
    };
    let d = step(&mut h);
    assert!(d > 0.0, "W steps forward");
    h.key(egui::Key::Q);
    assert!((step(&mut h) - 4.0 * d).abs() < 1e-3 * d.max(1.0), "Q: faster");
    h.key(egui::Key::Z);
    assert!((step(&mut h) - 0.25 * d).abs() < 1e-3 * d.max(1.0), "Z: slower (Q off)");
    h.key(egui::Key::Z);
    h.key(egui::Key::CloseBracket);
    assert!((step(&mut h) - 1.25 * d).abs() < 1e-3 * d.max(1.0), "]: a longer step");
    h.mods = egui::Modifiers::ALT;
    h.key(egui::Key::OpenBracket);
    h.mods = egui::Modifiers::NONE;
    assert!((step(&mut h) - d).abs() < 1e-3 * d.max(1.0), "Alt+[: the default step");
    // Shift+Up Arrow: straight up by a step.
    let z = eye(&h).z;
    h.mods = egui::Modifiers::SHIFT;
    h.key(egui::Key::ArrowUp);
    h.mods = egui::Modifiers::NONE;
    assert!((eye(&h).z - z - d).abs() < 1e-3 * d.max(1.0), "Shift+Up Arrow moves up");
    // Space: a drag turns the view sideways only.
    h.key(egui::Key::Space);
    assert!(h.app.state.walk.lock_vertical);
    let pitch = h.app.state.active().camera.pitch;
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    h.drag(r.center(), r.center() + egui::vec2(40.0, 40.0));
    assert_eq!(h.app.state.active().camera.pitch, pitch, "locked: no tilt");
    h.key(egui::Key::Space);
    h.drag(r.center(), r.center() + egui::vec2(0.0, 40.0));
    assert_ne!(h.app.state.active().camera.pitch, pitch, "unlocked: the drag tilts");
    // Shift+Space: level, from the same eye point.
    let before = eye(&h);
    h.mods = egui::Modifiers::SHIFT;
    h.key(egui::Key::Space);
    h.mods = egui::Modifiers::NONE;
    assert!(h.app.state.active().camera.pitch.abs() < 1e-6 && h.app.state.active().camera.roll == 0.0);
    assert!((eye(&h) - before).length() < 1e-3, "the eye stays");
}

/// Object Properties > Never Degrade: the box keeps its shading while the
/// view degrades to wireframe; the other box degrades.
#[test]
fn never_degrade_keeps_an_object_shaded_while_the_view_degrades() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [40.0, 0.0, 0.0], size: [20.0; 3] });
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [20.0; 3] });
    let id = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::ObjectProperties);
    h.click(h.find(Role::CheckBox, "Never Degrade").center());
    assert!(h.app.state.scene.node(id).unwrap().display.never_degrade);
    h.cmd(Command::CloseDialog);
    h.cmd(Command::SelectNone);
    maximized(&mut h);
    h.cmd(Command::SetAdaptiveDegradation { general: None, active: Some("Wireframe".into()), maintain_fps: Some(100.0), never_degrade_selected: None, degrade_to_default_lighting: None });
    h.cmd(Command::AdaptiveDegradationToggle);
    let i = h.app.state.active_viewport;
    let c = h.app.viewport_rects[i].center();
    h.mods = egui::Modifiers::ALT;
    h.run(vec![egui::Event::PointerMoved(c)]);
    h.run(vec![egui::Event::PointerButton { pos: c, button: egui::PointerButton::Middle, pressed: true, modifiers: h.mods }]);
    for k in 1..6 {
        h.run(vec![egui::Event::PointerMoved(c + egui::vec2(k as f32 * 10.0, 0.0))]);
    }
    assert!(h.app.degrading);
    let kept: Vec<loom_scene::NodeId> = crate::viewport_widget::shade_only(&h.app, i, &[]).into_iter().map(|k| h.app.world_meshes()[k].0).collect();
    assert_eq!(kept, [id], "only the Never Degrade box stays shaded");
    h.run(vec![egui::Event::PointerButton { pos: c + egui::vec2(50.0, 0.0), button: egui::PointerButton::Middle, pressed: false, modifiers: h.mods }]);
    h.mods = egui::Modifiers::NONE;
    h.frames(2);
    assert!(crate::viewport_widget::shade_only(&h.app, i, &[]).is_empty(), "not degrading: nothing special");
}

/// Shading > Lighting and Shadows > Shadows turns the view's shadows on: the
/// spot light casts them.
#[test]
fn the_lighting_and_shadows_menu_turns_on_viewport_shadows() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [20.0; 3] });
    h.cmd(Command::CreateLight { kind: loom_commands::light::LightKind::TargetSpot, position: [50.0, -50.0, 100.0], target: [0.0; 3] });
    maximized(&mut h);
    let i = h.app.state.active_viewport;
    h.click(label_rect(&h, 2).center());
    let p = menu_entry(&h, "Lighting and Shadows");
    h.run(vec![egui::Event::PointerMoved(p)]);
    h.frames(3);
    h.click(menu_entry(&h, "    Shadows"));
    assert!(h.app.state.viewports[i].shadows);
    let shading = crate::light_overlay::view_lighting(&h.app, &h.app.state.viewports[i].clone(), glam::Mat4::IDENTITY, loom_render::ViewShading::default());
    let set = crate::viewport_widget::shadow_set(&h.app, &h.app.state.viewports[i].clone(), &shading).expect("the spot light casts");
    assert_eq!(set.lights[0], (0, 1));
}

/// Viewport Configuration > Lighting and Shadows: Shadows, Hard Shadows and
/// Shadow Intensity; an omni light then casts through six shadow layers.
#[test]
fn lighting_and_shadows_panel_sets_hard_shadows_and_intensity_and_omni_lights_cast() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [20.0; 3] });
    h.cmd(Command::CreateLight { kind: loom_commands::light::LightKind::Omni, position: [30.0, -30.0, 60.0], target: [0.0; 3] });
    h.cmd(Command::ConfigureViewports { panel: Some("Lighting and Shadows".into()) });
    h.frames(2);
    let w = |h: &Harness| h.ctx.memory(|m| m.area_rect(egui::Id::new("Viewport Configuration"))).expect("the dialog");
    h.click(h.find(Role::CheckBox, "Shadows").center());
    assert!(h.app.state.active().shadows);
    h.click(h.text_rect("Hard Shadows", w(&h)).unwrap().center());
    assert!(!h.app.state.active().soft_shadows);
    let spin = h.spinner_after("Shadow Intensity:");
    h.drag(spin.center(), spin.center() + egui::vec2(0.0, 100.0));
    let v = h.app.state.active().shadow_intensity;
    assert!((0.0..1.0).contains(&v), "dragging down lowers it: {v}");
    let vp = h.app.state.active().clone();
    let shading = crate::light_overlay::view_lighting(&h.app, &vp, glam::Mat4::IDENTITY, loom_render::ViewShading::default());
    let set = crate::viewport_widget::shadow_set(&h.app, &vp, &shading).expect("the omni casts");
    assert_eq!((set.layer_count, set.soft, set.intensity), (6, false, v));
}

/// Viewport Configuration > Lighting and Shadows > Ambient Occlusion with its
/// Radius and Intensity/Fade; Shading > Lighting and Shadows toggles it too.
#[test]
fn ambient_occlusion_from_the_panel_and_the_shading_menu() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0));
    h.cmd(Command::ConfigureViewports { panel: Some("Lighting and Shadows".into()) });
    h.frames(2);
    h.click(h.find(Role::CheckBox, "Ambient Occlusion").center());
    assert!(h.app.state.active().ambient_occlusion);
    let spin = h.spinner_after("Intensity/Fade:");
    h.drag(spin.center(), spin.center() + egui::vec2(0.0, 100.0));
    assert!(h.app.state.active().ao_intensity < 1.0);
    let spin = h.spinner_after("Radius:");
    h.drag(spin.center(), spin.center() + egui::vec2(0.0, -100.0));
    assert!(h.app.state.active().ao_radius > 10.0);
    h.cmd(Command::CloseDialog);
    maximized(&mut h);
    h.click(label_rect(&h, 2).center());
    let p = menu_entry(&h, "Lighting and Shadows");
    h.run(vec![egui::Event::PointerMoved(p)]);
    h.frames(3);
    h.click(menu_entry(&h, "✔ Ambient Occlusion"));
    assert!(!h.app.state.active().ambient_occlusion, "the menu turns it off");
}

/// Shading > Other > Lit Wireframes: the view draws wireframe (no shaded
/// faces) with its edges lit.
#[test]
fn the_shading_menu_sets_lit_wireframes() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [20.0; 3] });
    maximized(&mut h);
    h.click(label_rect(&h, 2).center());
    h.run(vec![egui::Event::PointerMoved(menu_entry(&h, "Other"))]);
    h.frames(2);
    h.click(menu_entry(&h, "    Lit Wireframes"));
    assert_eq!(h.app.state.active().shading, loom_commands::Shading::LitWireframes);
    assert!(!loom_commands::Shading::LitWireframes.shaded(), "no shaded faces");
    // The box's twelve edges with corner normals: each end's normal is the
    // average of the three faces at that corner.
    let m = &h.app.world_meshes()[0].1;
    let v = loom_render::mesh_buffer::lit_edges(m, [1.0; 3]);
    assert_eq!(v.len(), 24);
    for g in &v {
        let n = glam::Vec3::from(g.normal);
        assert!((n.length() - 1.0).abs() < 1e-4 && n.x.abs() > 0.5 && n.y.abs() > 0.5 && n.z.abs() > 0.5, "{n}");
    }
}
