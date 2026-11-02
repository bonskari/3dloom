//! Photometric lights, Sunlight systems, light viewport controls, Add
//! Default Lights to Scene, Select By Layer, File > Replace, Asset Tracking,
//! Select Target / pose quadrants and Link Inheritance, driven like a person.

use super::*;
use egui::accesskit::Role;
use loom_scene::NodeId;

fn point_at(h: &mut Harness, p: egui::Pos2) {
    h.run(vec![egui::Event::PointerMoved(p)]);
    h.frames(2);
}

/// Opens a menu bar menu and its submenus, then clicks the item.
fn menu_path(h: &mut Harness, path: &[&str]) {
    h.click(h.find(Role::Button, path[0]).center());
    let in_menu = |h: &Harness, label: &str| {
        let cp = egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new("command_panel")).map(|p| p.rect);
        let all = h.find_all(Role::Button, label);
        all.iter().copied().find(|r| !cp.is_some_and(|c| c.contains(r.center()))).unwrap_or_else(|| panic!("no menu entry {label:?}: {all:?}")).center()
    };
    for sub in &path[1..path.len() - 1] {
        let p = in_menu(h, sub);
        point_at(h, p);
    }
    let p = in_menu(h, path[path.len() - 1]);
    h.click(p);
}

/// The texts reading `text` inside the window with area id `window`.
fn in_window(h: &Harness, window: &str, text: &str) -> Vec<egui::Pos2> {
    let area = h.ctx.memory(|m| m.area_rect(egui::Id::new(window))).unwrap_or_else(|| panic!("no {window} window"));
    h.text_centers(text).into_iter().filter(|p| area.contains(*p)).collect()
}

fn lowest(h: &Harness, window: &str, text: &str) -> egui::Pos2 {
    *in_window(h, window, text).iter().max_by(|a, b| a.y.total_cmp(&b.y)).unwrap_or_else(|| panic!("no {text:?} in {window}"))
}

fn light(h: &Harness, id: NodeId) -> loom_scene::light::LightObject {
    h.app.state.scene.node(id).unwrap().object.light().expect("a light").clone()
}

fn sel(h: &Harness) -> Vec<NodeId> {
    h.app.state.selection.iter().copied().collect()
}

fn panel_width(h: &Harness) -> f32 {
    egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new("command_panel")).expect("panel state").rect.width()
}

/// The texts right of `x` on a tall screen (what widens the panel).
fn widest_texts(h: &mut Harness) -> Vec<(String, f32)> {
    fn walk(shape: &egui::Shape, out: &mut Vec<(String, f32)>) {
        match shape {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            egui::Shape::Text(t) => out.push((t.galley.text().to_string(), t.galley.rect.translate(t.pos.to_vec2()).max.x)),
            _ => {}
        }
    }
    h.time += 0.03;
    let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 4000.0))), time: Some(h.time), ..Default::default() };
    let app = &mut h.app;
    let shapes = h.ctx.run(input, |ctx| app.frame(ctx)).shapes;
    let mut all = Vec::new();
    for s in &shapes {
        walk(&s.shape, &mut all);
    }
    all.sort_by(|a, b| b.1.total_cmp(&a.1));
    all.truncate(10);
    all
}

/// Creating a light ends on the Modify panel: its rollouts must not widen
/// the command panel, or every viewport shifts after a creation.
#[test]
fn light_and_sun_rollouts_keep_the_command_panel_width() {
    use loom_commands::light::LightKind;
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 4000.0));
    h.frames(3);
    let start = panel_width(&h);
    let mut cases: Vec<(String, Command)> = LightKind::ALL.iter().chain(LightKind::PHOTOMETRIC.iter()).map(|k| (k.label().to_owned(), Command::CreateLight { kind: *k, position: [0.0, -80.0, 60.0], target: [0.0; 3] })).collect();
    cases.push(("Sunlight".into(), Command::CreateSunlight { daylight: false, position: [0.0; 3], radius: 30.0, orbital_scale: None }));
    for (label, cmd) in cases {
        h.cmd(Command::ResetFile);
        h.cmd(cmd);
        h.app.panel_tab = crate::Tab::Modify;
        h.frames(3);
        let w = panel_width(&h);
        let wide = if (w - start).abs() > 1.0 {
            let mut v: Vec<(String, f32, f32)> = h.widgets.iter().filter(|w| w.2.min.x > 1000.0).map(|w| (format!("{:?} {}", w.0, w.1), w.2.min.x, w.2.max.x)).collect();
            v.sort_by(|a, b| b.2.total_cmp(&a.2));
            v.truncate(12);
            format!("{v:?} {:?}", widest_texts(&mut h))
        } else {
            String::new()
        };
        assert!((w - start).abs() <= 1.0, "{label}: the panel grew from {start} to {w}; rightmost widgets {wide}");
    }
}

/// The view does not move when a creation drag ends (the panel keeps its width).
#[test]
fn creating_a_target_light_does_not_move_the_viewports() {
    let mut h = Harness::new();
    wide_view(&mut h);
    h.app.panel_tab = crate::Tab::Create;
    h.frames(2);
    let before = h.app.viewport_rects.clone();
    h.app.create_kind = Some(crate::CreateKind::Light(loom_commands::light::LightKind::TargetLight));
    let (a, b) = (h.screen_of(Vec3::new(-40.0, -40.0, 0.0)), h.screen_of(Vec3::new(30.0, 20.0, 0.0)));
    h.drag(a, b);
    h.frames(2);
    assert_eq!(h.app.panel_tab, crate::Tab::Modify, "creation ends on the Modify panel");
    assert_eq!(h.app.viewport_rects, before);
}

#[test]
fn the_lights_category_starts_photometric_and_switches_to_standard() {
    let mut h = Harness::new();
    h.app.panel_tab = crate::Tab::Create;
    h.frames(2);
    let cp = panel(&h, "command_panel");
    h.click(h.find(Role::Button, "Lights").center());
    assert!(h.text_rect("Photometric", cp).is_some(), "photometric lights are the default");
    click_text(&mut h, "Free Light", cp, 0.0);
    assert_eq!(h.app.create_kind, Some(crate::CreateKind::Light(loom_commands::light::LightKind::FreeLight)));
    click_text(&mut h, "Photometric", cp, 0.0);
    let p = *h.text_centers("Standard").last().expect("the Standard entry");
    h.click(p);
    h.frames(2);
    assert!(!h.app.photometric_lights);
    assert_eq!(h.app.create_kind, None, "switching the class ends the create mode");
    click_text(&mut h, "Omni", cp, 0.0);
    assert_eq!(h.app.create_kind, Some(crate::CreateKind::Light(loom_commands::light::LightKind::Omni)));
}

#[test]
fn dragging_a_target_light_puts_the_light_at_the_press_and_its_target_at_the_release() {
    let mut h = Harness::new();
    wide_view(&mut h);
    h.app.create_kind = Some(crate::CreateKind::Light(loom_commands::light::LightKind::TargetLight));
    let (from, to) = (Vec3::new(-40.0, -40.0, 0.0), Vec3::new(30.0, 20.0, 0.0));
    let (a, b) = (h.screen_of(from), h.screen_of(to));
    h.drag(a, b);
    let id = *h.app.state.selection.iter().next().expect("the new light is selected");
    let l = light(&h, id);
    assert!(l.photometric.is_some());
    assert_eq!(h.app.state.scene.node(id).unwrap().name, "TPhotometricLight001");
    let t = l.target.expect("a target light");
    let at = h.app.state.scene.world_matrix(t).w_axis.truncate();
    let eye = h.app.state.scene.world_matrix(id).w_axis.truncate();
    assert!(at.distance(to) < 0.5, "target at the release: {at}, light at {eye}");
}

#[test]
fn the_templates_rollout_and_distribution_list_edit_a_photometric_light() {
    let mut h = Harness::new();
    let r = h.app.dispatch(Command::CreateLight { kind: loom_commands::light::LightKind::FreeLight, position: [0.0, 0.0, 80.0], target: [0.0; 3] }).unwrap();
    let id = NodeId(r["id"].as_u64().unwrap());
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 2400.0));
    h.app.panel_tab = crate::Tab::Modify;
    h.frames(3);
    let cp = panel(&h, "command_panel");
    assert!(h.text_rect("Free Light", cp).is_some(), "the stack names the photometric light");
    click_text(&mut h, "(Select a template)", cp, 0.0);
    let p = *h.text_centers("Halogen Spotlight").last().expect("the template entry");
    h.click(p);
    h.frames(2);
    let l = light(&h, id);
    assert_eq!(l.photometric.as_ref().unwrap().candela.get(), 3000.0);
    assert_eq!(l.light_type, loom_scene::light::LightType::Spot);
    let cp = panel(&h, "command_panel");
    assert!(h.text_rect("Distribution (Spotlight)", cp).is_some(), "the spotlight rollout appears");
    // Light Distribution (Type): back to Uniform Spherical.
    click_text(&mut h, "Spotlight", cp, 0.0);
    let p = *h.text_centers("Uniform Spherical").last().expect("the distribution entry");
    h.click(p);
    h.frames(2);
    assert_eq!(light(&h, id).photometric.unwrap().distribution.get(), loom_scene::light::DIST_SPHERICAL);
}

#[test]
fn dragging_a_sunlight_sizes_its_compass_and_places_the_sun() {
    let mut h = Harness::new();
    wide_view(&mut h);
    h.app.panel_tab = crate::Tab::Create;
    h.app.create_category = crate::Category::Systems;
    h.frames(2);
    let cp = panel(&h, "command_panel");
    click_text(&mut h, "Sunlight", cp, 0.0);
    assert_eq!(h.app.create_kind, Some(crate::CreateKind::SunSystem { daylight: false }));
    let (from, to) = (Vec3::ZERO, Vec3::new(40.0, 0.0, 0.0));
    let (a, b) = (h.screen_of(from), h.screen_of(to));
    h.drag(a, b);
    let compass = h.app.state.scene.nodes().find(|n| matches!(n.object, loom_scene::ObjectKind::Compass { .. })).expect("a compass").id;
    let radius = match &h.app.state.scene.node(compass).unwrap().object {
        loom_scene::ObjectKind::Compass { radius, .. } => radius.get(),
        _ => 0.0,
    };
    let at = h.app.state.scene.world_matrix(compass).w_axis.truncate();
    assert!((radius - 40.0).abs() < 0.5, "the drag sets the compass radius: {radius}, compass at {at}");
    let sun = h.app.state.scene.nodes().find(|n| matches!(&n.object, loom_scene::ObjectKind::Light(l) if l.sun.is_some())).expect("a sun").id;
    assert_eq!(h.app.state.scene.node(sun).unwrap().parent, Some(compass));
    let d = h.app.state.scene.world_matrix(sun).w_axis.truncate().length();
    assert!((d - radius * 4.0).abs() < 1.0, "orbital scale follows the radius: {d}");
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.len(), 0, "one undo step");
}

#[test]
fn a_light_view_shows_the_light_controls() {
    let mut h = Harness::new();
    let r = h.app.dispatch(Command::CreateLight { kind: loom_commands::light::LightKind::TargetSpot, position: [0.0, -100.0, 50.0], target: [0.0; 3] }).unwrap();
    let spot = NodeId(r["id"].as_u64().unwrap());
    h.cmd(Command::SpotDirectionalLightView);
    maximized(&mut h);
    h.cmd(Command::SelectNone);
    assert_eq!(h.app.state.active().kind, loom_commands::ViewKind::Camera(spot), "the light view");
    let names: Vec<&str> = h.widgets.iter().filter(|w| w.0 == Role::Button).map(|w| w.1.as_str()).collect();
    assert!(h.has(Role::Button, "Light Hotspot") && h.has(Role::Button, "Light Falloff"), "{names:?}");
    assert!(!h.has(Role::Button, "Perspective"), "camera controls are swapped out");
    h.click(h.find(Role::Button, "Light Falloff").center());
    assert_eq!(h.app.state.nav_mode, loom_input::NavMode::LightFalloff);
    // A drag in the view widens or narrows the falloff.
    let before = light(&h, spot).falloff.get();
    let c = h.app.viewport_rects[h.app.state.active_viewport].center() + egui::vec2(0.0, 80.0);
    h.drag(c, c + egui::vec2(0.0, -80.0));
    assert!((light(&h, spot).falloff.get() - before).abs() > 0.5, "the falloff changed");
}

#[test]
fn views_add_default_lights_to_scene_dialog_adds_the_omnis() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [20.0; 3] });
    menu_path(&mut h, &["Views", "Add Default Lights to Scene..."]);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::AddDefaultLights));
    let p = lowest(&h, "Add Default Lights to Scene", "OK");
    h.click(p);
    h.frames(2);
    assert!(h.app.state.scene.nodes().any(|n| n.name == "DefaultKeyLight") && h.app.state.scene.nodes().any(|n| n.name == "DefaultFillLight"));
    assert_eq!(h.app.state.dialog, None);
}

#[test]
fn edit_select_by_layer_picks_layers_and_selects_their_objects() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    h.cmd(Command::CreateLayer { name: Some("Props".into()) });
    let on_props = sel(&h);
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [30.0, 0.0, 0.0], size: [10.0; 3] });
    h.cmd(Command::SelectNone);
    menu_path(&mut h, &["Edit", "Select By", "Layer..."]);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::SelectByLayer));
    let p = in_window(&h, "Select By Layer", "Props")[0];
    h.click(p);
    let ok = lowest(&h, "Select By Layer", "OK");
    h.click(ok);
    h.frames(2);
    let props = h.app.state.scene.layers.iter().find(|l| l.name == "Props").unwrap().id;
    let want: Vec<NodeId> = h.app.state.scene.nodes().filter(|n| n.layer == props).map(|n| n.id).collect();
    assert_eq!(sel(&h), want);
    assert!(!on_props.is_empty());
}

#[test]
fn file_replace_lists_the_matching_objects_and_ok_replaces_them() {
    let dir = std::env::temp_dir().join(format!("loom-ui-replace-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("parts.loom");
    let mut f = loom_commands::AppState::new();
    loom_commands::execute(&mut f, Command::CreatePrimitive { kind: PrimitiveKind::Sphere, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let s = *f.selection.iter().next().unwrap();
    loom_commands::execute(&mut f, Command::Rename { id: s.0, name: "Box001".into() }).unwrap();
    loom_commands::execute(&mut f, Command::SaveFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [30.0, 0.0, 0.0], size: [10.0; 3] });
    let b = sel(&h)[0];
    h.cmd(Command::ReplaceFile { path: Some(path.to_string_lossy().into_owned()), names: None, materials: None });
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::ReplaceObjects));
    assert_eq!(in_window(&h, "replace_objects", "Box001").len(), 1, "the matching object is listed");
    let ok = lowest(&h, "replace_objects", "OK");
    h.click(ok);
    h.frames(2);
    assert!(matches!(h.app.state.scene.node(b).unwrap().object, loom_scene::ObjectKind::Sphere { .. }));
    assert_eq!(h.app.state.dialog, None);
}

#[test]
fn asset_tracking_shows_missing_files_and_strip_path_repaths() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let b = sel(&h)[0];
    h.cmd(Command::SetTextureDisplay { id: b.0, kind: "bitmap".into(), path: Some("/no/such/dir/brick.png".into()) });
    menu_path(&mut h, &["File", "Asset Tracking..."]);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::AssetTracking));
    assert_eq!(in_window(&h, "Asset Tracking", "Missing").len(), 1);
    let p = in_window(&h, "Asset Tracking", "brick.png")[0];
    h.click(p);
    let strip = lowest(&h, "Asset Tracking", "Strip Path");
    h.click(strip);
    h.frames(2);
    assert_eq!(loom_commands::assets::scene_assets(&h.app.state), vec!["brick.png".to_owned()]);
}

#[test]
fn a_target_lights_quad_menu_selects_its_target_and_the_animation_quad_freezes() {
    let mut h = Harness::new();
    let r = h.app.dispatch(Command::CreateLight { kind: loom_commands::light::LightKind::TargetSpot, position: [0.0, -100.0, 50.0], target: [0.0; 3] }).unwrap();
    let t = NodeId(r["target"].as_u64().unwrap());
    let q = crate::quad_menu::create_quadrants(&h.app);
    let tools1 = q.tools1.expect("tools 1 for a target light");
    let item = tools1.items.iter().find(|i| i.label == "Select Target").expect("Select Target");
    let crate::quad_menu::Action::Command(c) = &item.action else { panic!("a command") };
    let _ = h.app.dispatch(c.clone());
    assert_eq!(sel(&h), vec![t]);
    let q = crate::quad_menu::animation_quadrants(&h.app);
    let pose = q.tools1.expect("the pose quadrant with a selection");
    let labels: Vec<&str> = pose.items.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&"Freeze Transform") && labels.contains(&"Transform to Zero"), "{labels:?}");
}

/// Tools 1 > Cast Shadows turns the selected light's shadows on and off.
#[test]
fn a_lights_quad_menu_cast_shadows_toggles_its_shadows() {
    let mut h = Harness::new();
    let r = h.app.dispatch(Command::CreateLight { kind: loom_commands::light::LightKind::Omni, position: [0.0, 0.0, 50.0], target: [0.0; 3] }).unwrap();
    let id = NodeId(r["id"].as_u64().unwrap());
    for _ in 0..2 {
        let before = light(&h, id).shadows_on.get();
        let tools1 = crate::quad_menu::create_quadrants(&h.app).tools1.expect("tools 1 for a light");
        let item = tools1.items.iter().find(|i| i.label.ends_with("Cast Shadows")).expect("Cast Shadows");
        assert_eq!(item.label.starts_with('✔'), before, "the check mark shows Shadows On");
        let crate::quad_menu::Action::Command(c) = &item.action else { panic!("a command") };
        let _ = h.app.dispatch(c.clone());
        assert_eq!(light(&h, id).shadows_on.get(), !before);
    }
}

#[test]
fn a_model_file_dropped_on_the_window_is_imported() {
    let dir = std::env::temp_dir().join(format!("loom-ui-drop-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("part.stl");
    let mesh = loom_mesh::box_primitive(Vec3::splat(5.0));
    std::fs::write(&path, loom_mesh::stl::write_binary("part", &loom_mesh::stl::triangles(&mesh, glam::Mat4::IDENTITY))).unwrap();
    let mut h = Harness::new();
    h.time += 1.0 / 30.0;
    let input = egui::RawInput { screen_rect: Some(h.screen), time: Some(h.time), dropped_files: vec![egui::DroppedFile { path: Some(path.clone()), ..Default::default() }], ..Default::default() };
    let app = &mut h.app;
    h.shapes = h.ctx.run(input, |ctx| app.frame(ctx)).shapes;
    h.frames(2);
    assert_eq!(h.app.state.scene.len(), 1, "the STL came in");
    assert!(matches!(h.app.state.scene.nodes().next().unwrap().object, loom_scene::ObjectKind::EditableMesh(_)));
}

#[test]
fn views_update_during_spinner_drag_holds_the_viewports_until_release() {
    let mut h = Harness::new();
    menu_path(&mut h, &["Views", "✔ Update During Spinner Drag"]);
    assert!(!h.app.state.update_during_spinner_drag, "the menu item turned it off");
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let id = sel(&h)[0];
    h.app.panel_tab = crate::Tab::Modify;
    h.frames(3);
    let before = h.app.world_meshes()[0].1.aabb();
    let length = h.spinner_after("Length");
    let m = h.mods;
    let start = length.center();
    h.run(vec![egui::Event::PointerMoved(start)]);
    h.run(vec![egui::Event::PointerButton { pos: start, button: egui::PointerButton::Primary, pressed: true, modifiers: m }]);
    for i in 1..=6 {
        h.run(vec![egui::Event::PointerMoved(start + egui::vec2(i as f32 * 8.0, 0.0))]);
    }
    let changed = match &h.app.state.scene.node(id).unwrap().object {
        loom_scene::ObjectKind::Box { length, .. } => length.get(),
        _ => 0.0,
    };
    assert!(changed != 10.0, "the spinner changed the box: {changed}");
    assert_eq!(h.app.world_meshes()[0].1.aabb(), before, "the viewport waits for the release");
    h.run(vec![egui::Event::PointerButton { pos: start + egui::vec2(48.0, 0.0), button: egui::PointerButton::Primary, pressed: false, modifiers: m }]);
    h.frames(2);
    assert_ne!(h.app.world_meshes()[0].1.aabb(), before, "released: the viewport catches up");
}

#[test]
fn link_inheritance_utility_unchecks_an_axis_on_every_selected_object() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let a = sel(&h)[0];
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [30.0, 0.0, 0.0], size: [10.0; 3] });
    let b = sel(&h)[0];
    h.cmd(Command::SelectNodes { ids: vec![a.0, b.0], op: loom_commands::SelectOp::Replace });
    h.app.panel_tab = crate::Tab::Utilities;
    h.frames(2);
    let cp = panel(&h, "command_panel");
    click_text(&mut h, "Link Inheritance (Selected)", cp, 0.0);
    let cp = panel(&h, "command_panel");
    let rotate = h.text_rect("Rotate:", cp).expect("the Rotate row");
    // The X checkbox under "Rotate:".
    let x = h.text_centers("X").into_iter().filter(|p| cp.contains(*p) && p.y > rotate.max.y).min_by(|p, q| p.y.total_cmp(&q.y)).expect("Rotate X");
    h.click(x);
    h.frames(2);
    for id in [a, b] {
        assert_eq!(h.app.state.scene.node(id).unwrap().inherit.rotate_axes, [false, true, true]);
    }
}

/// Tools 1 > Apply Camera Correction Modifier puts Camera Correction on the
/// selected camera; its Guess button sets the Amount that levels the camera,
/// and a view through the camera then uses it.
#[test]
fn camera_correction_from_the_quad_menu_and_its_guess_button() {
    let mut h = Harness::new();
    let r = h.app.dispatch(Command::CreateCamera { target: true }).unwrap();
    let id = NodeId(r["id"].as_u64().unwrap());
    let tools1 = crate::quad_menu::create_quadrants(&h.app).tools1.expect("tools 1 for a camera");
    let item = tools1.items.iter().find(|i| i.label == "Apply Camera Correction Modifier").expect("Apply Camera Correction Modifier");
    let crate::quad_menu::Action::Command(c) = &item.action else { panic!("a command") };
    h.app.dispatch(c.clone()).unwrap();
    let node = h.app.state.scene.node(id).unwrap();
    assert_eq!(node.modifiers.len(), 1);
    assert_eq!(loom_scene::camera::correction(node), Some((0.0, 90.0)), "Amount 0, Direction 90");
    h.app.panel_tab = crate::Tab::Modify;
    h.frames(3);
    let before = loom_commands::camera::overlay_lines(&h.app.state.scene, id, true);
    h.click(h.find(Role::Button, "Guess").center());
    h.frames(2);
    // The viewports' cone follows the corrected view.
    let after = loom_commands::camera::overlay_lines(&h.app.state.scene, id, true);
    assert_eq!(before.len(), after.len());
    assert!(before.iter().zip(&after).any(|(a, b)| a.1.distance(b.1) > 1.0), "the cone tilts with the correction");
    let want = loom_scene::camera::correction_guess(h.app.state.scene.world_matrix(id));
    assert!(want.abs() > 1.0, "the default view looks down: {want}");
    let (amount, _) = loom_scene::camera::correction(h.app.state.scene.node(id).unwrap()).unwrap();
    assert!((amount - want).abs() < 1e-3, "Guess set Amount {amount}, want {want}");
    let mut vp = h.app.state.viewports[0].clone();
    vp.kind = loom_commands::ViewKind::Camera(id);
    let cam = h.app.camera_for_view(&vp);
    assert!(cam.correction.is_some_and(|(a, d)| (a - want.to_radians()).abs() < 1e-4 && (d - 90f32.to_radians()).abs() < 1e-4));
}

/// Camera Correction is refused on anything but a camera.
#[test]
fn camera_correction_only_goes_on_cameras() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    assert!(h.app.dispatch(Command::AddModifier { name: "Camera Correction".into() }).is_err());
}

/// A light's Name and Color rollout: renaming a target light renames its
/// target, and the colour swatch colours the light's geometry only.
#[test]
fn a_lights_name_and_color_rollout_renames_its_target_and_colours_its_geometry() {
    let mut h = Harness::new();
    let r = h.app.dispatch(Command::CreateLight { kind: loom_commands::light::LightKind::TargetSpot, position: [0.0, -100.0, 50.0], target: [0.0; 3] }).unwrap();
    let (id, t) = (NodeId(r["id"].as_u64().unwrap()), NodeId(r["target"].as_u64().unwrap()));
    h.app.panel_tab = crate::Tab::Modify;
    h.frames(3);
    let cp = panel(&h, "command_panel");
    assert!(h.text_rect("Name and Color", cp).is_some(), "the Modify panel shows Name and Color for a light");
    h.cmd(Command::Rename { id: id.0, name: "Key".into() });
    assert_eq!(h.app.state.scene.node(t).unwrap().name, "Key.Target");
    let color = light(&h, id).color.get();
    h.cmd(Command::SetWireColor { id: id.0, rgb: [200, 10, 10] });
    let n = h.app.state.scene.node(id).unwrap();
    assert_eq!(h.app.state.scene.display_color(n), [200, 10, 10], "the geometry shows the colour");
    assert_eq!(light(&h, id).color.get(), color, "the emitted colour stays");
}

/// The Light Include/Exclude tool: Exclude + Assign to Light puts the
/// selected object on the picked light's exclude list, the list shows it, and
/// Clear Light empties it.
#[test]
fn the_light_include_exclude_tool_excludes_the_selection_from_a_picked_light() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let b = sel(&h)[0];
    let r = h.app.dispatch(Command::CreateLight { kind: loom_commands::light::LightKind::Omni, position: [0.0, 0.0, 50.0], target: [0.0; 3] }).unwrap();
    let l = NodeId(r["id"].as_u64().unwrap());
    h.app.state.selection = [b].into_iter().collect();
    h.cmd(Command::LightIncludeExcludeTool);
    h.frames(3);
    let exclude = *in_window(&h, "Light Incl/Excl", "Exclude").iter().min_by(|a, b| a.y.total_cmp(&b.y)).expect("the Geometry group's Exclude");
    h.click(exclude);
    assert!(h.app.state.incl_excl_exclude);
    h.click(in_window(&h, "Light Incl/Excl", "Assign to Light")[0]);
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::LightInclExclAssign));
    h.cmd(Command::PickHit { id: l.0 });
    h.frames(2);
    let ex = light(&h, l).exclusion;
    assert!(!ex.include && ex.nodes == vec![b] && ex.scope == 2, "{ex:?}");
    let name = h.app.state.scene.node(b).unwrap().name.clone();
    assert!(!in_window(&h, "Light Incl/Excl", &name).is_empty(), "the Objects list shows {name}");
    // The shaded viewports leave the box unlit by the light (its first scene light).
    let off = |h: &Harness| h.app.world_meshes().iter().position(|(id, _)| *id == b).map(|i| h.app.world_styles()[i].lights_off);
    assert_eq!(off(&h), Some(1), "the box is excluded from light 0");
    let mut vp = h.app.state.viewports[0].clone();
    vp.scene_lights = true;
    let shading = crate::light_overlay::view_lighting(&h.app, &vp, glam::Mat4::IDENTITY, loom_render::ViewShading::default());
    assert!(shading.flags & loom_render::view_flags::SCENE_LIGHTS != 0, "a view lit by the scene lights applies the exclusion");
    // Both: the light's viewport shadow map leaves the box out too; Illumination only keeps it casting.
    let shadows = |h: &Harness| h.app.world_meshes().iter().position(|(id, _)| *id == b).map(|i| h.app.world_styles()[i].shadows_off);
    assert_eq!(shadows(&h), Some(1), "the box casts no shadow from light 0");
    h.cmd(Command::SetLightExclusion { id: l.0, include: false, scope: 0, nodes: vec![b.0] });
    h.frames(1);
    assert_eq!((off(&h), shadows(&h)), (Some(1), Some(0)), "Illumination only: unlit but casting");
    h.cmd(Command::SetLightExclusion { id: l.0, include: false, scope: 1, nodes: vec![b.0] });
    h.frames(1);
    assert_eq!((off(&h), shadows(&h)), (Some(0), Some(1)), "Shadow Casting only: lit, no shadow");
    h.click(in_window(&h, "Light Incl/Excl", "Clear Light")[0]);
    assert!(light(&h, l).exclusion.nodes.is_empty());
    h.frames(1);
    assert_eq!(off(&h), Some(0), "cleared: lit again");
}

/// Area Shadows > Shadow Quality: the traced shadows' rays, so the spinner works.
#[test]
fn area_shadows_shadow_quality_spinner_sets_the_rays() {
    let mut h = Harness::new();
    let r = h.app.dispatch(Command::CreateLight { kind: loom_commands::light::LightKind::Omni, position: [0.0, 0.0, 50.0], target: [0.0; 3] }).unwrap();
    let l = NodeId(r["id"].as_u64().unwrap());
    h.cmd(Command::SetParameter { id: l.0, name: "Shadow Type".into(), value: 3.0 });
    h.app.panel_tab = crate::Tab::Modify;
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 2400.0));
    h.frames(3);
    // The rollout header (below the Shadow Type list showing the same name).
    let header = *h.widgets_labelled("Area Shadows").last().expect("the Area Shadows rollout");
    h.click(header.center());
    h.frames(2);
    let before = light(&h, l).shadow_quality.get();
    let spin = h.spinner_after("Shadow Quality:");
    h.drag(spin.center(), spin.center() + egui::vec2(0.0, -40.0));
    assert!(light(&h, l).shadow_quality.get() > before, "Shadow Quality went up from {before}");
}

/// Adv. Ray Traced > Optimizations rollout: Transparent Shadows and Skip
/// Coplanar Faces checkboxes and the Threshold spinner set the light, and
/// its traced shadow carries them; undoable.
#[test]
fn optimizations_rollout_sets_transparent_and_coplanar_shadows() {
    let mut h = Harness::new();
    let r = h.app.dispatch(Command::CreateLight { kind: loom_commands::light::LightKind::Omni, position: [0.0, 0.0, 50.0], target: [0.0; 3] }).unwrap();
    let l = NodeId(r["id"].as_u64().unwrap());
    h.cmd(Command::SetParameter { id: l.0, name: "Shadow Type".into(), value: 2.0 });
    h.app.panel_tab = crate::Tab::Modify;
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 2400.0));
    h.frames(3);
    let header = *h.widgets_labelled("Optimizations").last().expect("the Optimizations rollout");
    h.click(header.center());
    h.frames(2);
    assert_eq!(light(&h, l).traced_shadow().map(|t| (t.transparent, t.coplanar)), Some((false, None)), "off by default");
    let on = *h.widgets_labelled("On").iter().find(|r| r.top() > header.top()).expect("Transparent Shadows On");
    h.click(on.center());
    h.click(h.widget("Skip Coplanar Faces").center());
    assert!(light(&h, l).transparent_shadows.get() && light(&h, l).skip_coplanar.get());
    let spin = h.spinner_after("Threshold:");
    h.drag(spin.center(), spin.center() + egui::vec2(0.0, 40.0));
    let t = light(&h, l).coplanar_threshold.get();
    assert!(t < 0.93, "the Threshold went down: {t}");
    let traced = light(&h, l).traced_shadow().unwrap();
    assert!(traced.transparent && traced.coplanar == Some(t), "the traced shadow uses them");
    assert!(h.find_all(egui::accesskit::Role::CheckBox, "Supersampled Material").len() == 1, "the antialias suppression shown (disabled)");
    h.cmd(Command::UndoSceneOperation);
    h.cmd(Command::UndoSceneOperation);
    h.cmd(Command::UndoSceneOperation);
    assert!(!light(&h, l).transparent_shadows.get(), "undoable");
}

/// Photometric Shape/Area Shadows > Light Shape Visible in Rendering: the
/// checkbox works, and a render then shows the emitter (here a Disc of the
/// Radius spinner's size under the light) self-illuminated in its colour;
/// off, or a Point shape, nothing.
#[test]
fn light_shape_visible_in_rendering_adds_the_glowing_emitter() {
    let mut h = Harness::new();
    let r = h.app.dispatch(Command::CreateLight { kind: loom_commands::light::LightKind::FreeLight, position: [10.0, 20.0, 50.0], target: [10.0, 20.0, 0.0] }).unwrap();
    let l = NodeId(r["id"].as_u64().unwrap());
    let shapes = |h: &Harness| {
        let lights = loom_scene::light::scene_lights(&h.app.state.scene, |_| true);
        crate::realistic::light_shapes(&h.app.state.scene, &lights)
    };
    h.cmd(Command::SetParameter { id: l.0, name: "Light Shape".into(), value: 3.0 });
    h.cmd(Command::SetParameter { id: l.0, name: "Shape Radius".into(), value: 4.0 });
    assert!(shapes(&h).is_empty(), "off by default");
    h.app.panel_tab = crate::Tab::Modify;
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 2400.0));
    h.frames(3);
    let header = *h.widgets_labelled("Shape/Area Shadows").last().expect("the rollout");
    h.click(header.center());
    h.frames(2);
    h.click(h.widget("Light Shape Visible in Rendering").center());
    let ph = |h: &Harness| light(h, l).photometric.clone().unwrap();
    assert!(ph(&h).render_shape.get(), "the checkbox is enabled and sets it");
    let s = shapes(&h);
    assert_eq!(s.len(), 1);
    let (lo, hi) = s[0].mesh.vertices().iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(a, b), v| (a.min(v.position), b.max(v.position)));
    assert!(((hi.x - lo.x) - 8.0).abs() < 0.1 && ((hi.y - lo.y) - 8.0).abs() < 0.1 && (hi.z - lo.z) < 0.5, "a disc of radius 4 in the light's plane: {lo} {hi}");
    assert!((((lo + hi) * 0.5) - Vec3::new(10.0, 20.0, 50.0)).length() < 0.2, "at the light");
    let look = s[0].surfaces.for_face(0);
    assert!(look.self_illum >= 1.0 && (look.diffuse.max_element() - 1.0).abs() < 1e-3, "self-illuminated in the light's colour: {:?}", look.diffuse);
    h.cmd(Command::SetParameter { id: l.0, name: "Light Shape".into(), value: 0.0 });
    assert!(shapes(&h).is_empty(), "a Point has no shape");
    h.cmd(Command::UndoSceneOperation);
    h.cmd(Command::UndoSceneOperation);
    assert!(!ph(&h).render_shape.get(), "undoable");
}

/// A light's Atmospheres & Effects rollout: Add > New Volume Light > OK
/// makes a Volume Light the light lights (one undo step), the list shows it,
/// Setup opens it in Environment and Effects, Delete takes the light out.
#[test]
fn light_atmospheres_and_effects_add_setup_delete() {
    use loom_scene::environment::AtmosphereKind;
    let mut h = Harness::new();
    let r = h.app.dispatch(Command::CreateLight { kind: loom_commands::light::LightKind::Omni, position: [0.0, 0.0, 50.0], target: [0.0; 3] }).unwrap();
    let l = NodeId(r["id"].as_u64().unwrap());
    h.app.panel_tab = crate::Tab::Modify;
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 2400.0));
    h.frames(3);
    let header = *h.widgets_labelled("Atmospheres & Effects").last().expect("the rollout");
    h.click(header.center());
    h.frames(2);
    let add = *h.widgets_labelled("Add").iter().find(|r| r.top() > header.top()).expect("Add");
    h.click(add.center());
    h.frames(2);
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("Add Atmosphere or Effect"))).expect("the Add Atmosphere or Effect window");
    let ok = *h.text_rects("OK").iter().find(|r| win.contains_rect(**r)).expect("its OK");
    h.click(ok.center());
    let lit = |h: &Harness| h.app.state.scene.environment.atmospheres.iter().filter(|a| matches!(&a.kind, AtmosphereKind::VolumeLight(v) if v.lights.contains(&l))).count();
    assert_eq!(lit(&h), 1, "a new Volume Light picks the light");
    h.frames(2);
    let entry = *h.widgets_labelled("Volume Light").iter().find(|r| r.top() > header.top()).expect("listed");
    h.click(entry.center());
    h.frames(1);
    let setup = *h.widgets_labelled("Setup").last().unwrap();
    h.click(setup.center());
    assert_eq!(h.app.state.environment_ui.open, Some(loom_commands::environment::EnvironmentTab::Environment), "Setup opens it");
    h.app.state.environment_ui.open = None;
    h.frames(2);
    let delete = *h.widgets_labelled("Delete").iter().find(|r| r.top() > header.top()).unwrap();
    h.click(delete.center());
    assert_eq!(lit(&h), 0, "Delete takes the light out");
    assert_eq!(h.app.state.scene.environment.atmospheres.len(), 1, "the atmosphere stays");
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(lit(&h), 1);
    h.cmd(Command::UndoSceneOperation);
    assert!(h.app.state.scene.environment.atmospheres.is_empty(), "the Add is one undo step");
}

/// Light Distribution (Type) > Photometric Web shows the Distribution
/// (Photometric Web) rollout; its file button opens the web file dialog,
/// the chosen file lights the light, and X Rotation turns the web.
#[test]
fn photometric_web_rollout_chooses_a_web_file_and_rotates_it() {
    let dir = std::env::temp_dir().join(format!("loom-ui-ies-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("downlight.ies");
    std::fs::write(&path, "IESNA:LM-63-2002\nTILT=NONE\n1 -1 1.0 4 1 1 2 0.1 0.1 0.0\n1.0 1.0 20\n0 30 60 90\n0\n1000 500 0 0\n").unwrap();
    let mut h = Harness::new();
    h.app.native_file_dialogs = false;
    let r = h.app.dispatch(Command::CreateLight { kind: loom_commands::light::LightKind::FreeLight, position: [0.0, 0.0, 80.0], target: [0.0; 3] }).unwrap();
    let id = NodeId(r["id"].as_u64().unwrap());
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 2400.0));
    h.app.panel_tab = crate::Tab::Modify;
    h.frames(3);
    let cp = panel(&h, "command_panel");
    assert!(h.text_rect("Distribution (Photometric Web)", cp).is_none());
    click_text(&mut h, "Uniform Spherical", cp, 0.0);
    let p = *h.text_centers("Photometric Web").last().expect("the distribution entry");
    h.click(p);
    h.frames(2);
    let cp = panel(&h, "command_panel");
    assert!(h.text_rect("Distribution (Photometric Web)", cp).is_some(), "the web rollout appears");
    click_text(&mut h, "< Choose photometric file >", cp, 0.0);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::PhotometricWeb { light: id.0 }));
    super::choose_file(&mut h, &path, "Open");
    h.frames(2);
    let ph = light(&h, id).photometric.unwrap();
    assert_eq!((ph.candela.get(), ph.web.as_ref().map(|w| w.peak)), (1000.0, Some(1000.0)), "{}", h.app.state.status_prompt);
    let cp = panel(&h, "command_panel");
    assert!(h.text_rect("downlight", cp).is_some(), "the button names the file");
    // The icon is the web's shape: nothing above the light.
    let icon = light(&h, id).icon_mesh();
    let top = icon.vertices().iter().map(|v| v.position.z).fold(f32::MIN, f32::max);
    let bottom = icon.vertices().iter().map(|v| v.position.z).fold(f32::MAX, f32::min);
    assert!(top < 0.5 && bottom < -7.9, "{top} {bottom}");
    h.cmd(Command::SetParameter { id: id.0, name: "X Rotation".into(), value: 90.0 });
    assert_eq!(light(&h, id).photometric.unwrap().web_rotation_x.get(), 90.0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Create > Helpers > Assembly Heads > Luminaire: a click in a viewport
/// makes the head; its Dimmer spinner dims a light linked below it.
#[test]
fn assembly_heads_luminaire_is_created_and_its_dimmer_dims_its_lights() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0));
    h.app.panel_tab = crate::Tab::Create;
    h.app.create_category = crate::Category::Helpers;
    h.frames(2);
    let cp = panel(&h, "command_panel");
    click_text(&mut h, "Standard", cp, 0.0);
    let p = *h.text_centers("Assembly Heads").last().expect("the helper class");
    h.click(p);
    h.frames(2);
    let cp = panel(&h, "command_panel");
    click_text(&mut h, "Luminaire", cp, 0.0);
    assert_eq!(h.app.create_kind, Some(crate::CreateKind::Helper(crate::HelperKind::Luminaire)));
    let p = h.screen_of(Vec3::new(0.0, 0.0, 0.0));
    h.click(p);
    let head = *h.app.state.selection.iter().next().expect("the new head is selected");
    assert!(matches!(h.app.state.scene.node(head).unwrap().object, loom_scene::ObjectKind::Luminaire { .. }));
    let r = h.app.dispatch(Command::CreateLight { kind: loom_commands::light::LightKind::Omni, position: [0.0, 0.0, 10.0], target: [0.0; 3] }).unwrap();
    let light = NodeId(r["id"].as_u64().unwrap());
    h.cmd(Command::SetParent { id: light.0, parent: Some(head.0) });
    h.cmd(Command::SelectNodes { ids: vec![head.0], op: loom_commands::SelectOp::Replace });
    h.app.start_create(None);
    h.app.panel_tab = crate::Tab::Modify;
    h.frames(3);
    let cp = panel(&h, "command_panel");
    assert!(h.text_rect("Luminaire Parameters", cp).is_some());
    assert!(h.text_rect("Filter Color:", cp).is_some(), "the filter is a colour swatch");
    let lit = |h: &Harness| loom_scene::light::scene_lights(&h.app.state.scene, |_| true).into_iter().find(|l| l.id == light).unwrap().color.x;
    let full = lit(&h);
    // Type 25 into the Dimmer spinner.
    let row = h.text_rect("Dimmer", cp).expect("the Dimmer row");
    h.click(egui::pos2(row.right() + 60.0, row.center().y));
    h.run(vec![egui::Event::Key { key: egui::Key::A, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND }]);
    h.run(vec![egui::Event::Text("25".into())]);
    h.key(egui::Key::Enter);
    h.frames(2);
    assert!((lit(&h) - full * 0.25).abs() < 1e-4, "{} vs {}", lit(&h), full);
}

/// Utilities > More... > Resource Collector: an output path, Update
/// Materials and Begin copy the scene's bitmaps there and repath them.
#[test]
fn resource_collector_utility_collects_into_the_typed_folder() {
    let dir = std::env::temp_dir().join(format!("loom-ui-rc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("wood.png");
    std::fs::write(&src, b"bitmap bytes").unwrap();
    let mut h = Harness::new();
    h.app.native_file_dialogs = false;
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    h.cmd(Command::SetMaterialMap { slot: Some(0), map: "Diffuse Color".into(), path: Some(src.to_string_lossy().into_owned()), amount: None });
    h.cmd(Command::AssignMaterialToSelection);
    h.app.panel_tab = crate::Tab::Utilities;
    h.frames(2);
    h.click(h.find(Role::Button, "More...").center());
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("utilities_more"))).expect("the Utilities list");
    // A pause, so this click is not a double-click with the last one.
    h.frames(30);
    h.click(h.text_rect("Resource Collector", win).expect("listed under More...").center());
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("utilities_more"))).unwrap();
    h.click(h.text_rect("OK", win).expect("OK").center());
    h.frames(2);
    let cp = panel(&h, "command_panel");
    let label = h.text_rect("Output Path:", cp).expect("the output field");
    h.click(egui::pos2(label.center().x, label.bottom() + 12.0));
    let out = dir.join("out");
    h.run(vec![egui::Event::Text(out.to_string_lossy().into_owned())]);
    click_text(&mut h, "Update Materials", cp, 0.0);
    click_text(&mut h, "Begin", cp, 0.0);
    h.frames(2);
    assert!(out.join("wood.png").is_file(), "{:?} {}", h.app.last_error, h.app.state.status_prompt);
    assert!(loom_commands::assets::scene_assets(&h.app.state).iter().all(|p| p.starts_with(out.to_str().unwrap())));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Light Lister > General Settings: a change applies to every light (or the
/// selected ones), and Global Level is the Environment's.
#[test]
fn light_lister_general_settings_apply_to_every_light() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0));
    let mut lights = Vec::new();
    for x in [0.0, 60.0] {
        h.cmd(Command::CreateLight { kind: loom_commands::light::LightKind::Omni, position: [x, 0.0, 50.0], target: [0.0; 3] });
        lights.push(*h.app.state.selection.iter().next().unwrap());
    }
    h.cmd(Command::LightLister);
    h.frames(2);
    h.click(h.find(egui::accesskit::Role::RadioButton, "General Settings").center());
    let light = |h: &Harness, i: usize| h.app.state.scene.node(lights[i]).unwrap().object.light().cloned().unwrap();
    // Shadows on for both.
    let general = h.find(egui::accesskit::Role::RadioButton, "General Settings");
    // A header of the General Settings row (the Modify panel has labels of the same names).
    let header_of = |h: &Harness, name: &str| h.find_all(egui::accesskit::Role::Label, name).into_iter().filter(|r| r.min.y > general.max.y && r.min.x > general.min.x - 60.0 && r.min.x < general.min.x + 700.0).min_by(|a, b| (a.min.y - general.max.y).total_cmp(&(b.min.y - general.max.y))).unwrap_or_else(|| panic!("the {name} header: {:?} below {general:?}", h.find_all(egui::accesskit::Role::Label, name)));
    let header = header_of(&h, "Shadows");
    let check = h.widgets.iter().filter(|(r, _, w)| *r == egui::accesskit::Role::CheckBox && w.min.y > header.max.y && (w.center().x - header.center().x).abs() < 30.0).min_by(|a, b| a.2.min.y.total_cmp(&b.2.min.y)).map(|w| w.2).expect("the Shadows check box");
    h.click(check.center());
    assert!(light(&h, 0).shadows_on.get() && light(&h, 1).shadows_on.get(), "both lights cast shadows");
    // The Multiplier spinner's up arrow: both lights 1.01.
    let header = header_of(&h, "Multiplier");
    let spin = h.widgets.iter().filter(|(r, _, w)| *r == egui::accesskit::Role::SpinButton && w.min.y > header.max.y && w.min.x >= header.min.x - 20.0).min_by(|a, b| a.2.min.y.total_cmp(&b.2.min.y).then(a.2.min.x.total_cmp(&b.2.min.x))).map(|w| w.2).expect("the Multiplier spinner");
    h.click(egui::pos2(spin.max.x + crate::spinner::ARROW_WIDTH / 2.0, spin.center().y - spin.height() / 4.0));
    assert!((light(&h, 0).multiplier.get() - 1.01).abs() < 1e-4 && (light(&h, 1).multiplier.get() - 1.01).abs() < 1e-4, "{} {}", light(&h, 0).multiplier.get(), light(&h, 1).multiplier.get());
    // Global Level.
    let level = h.spinner_after("Global Level:");
    h.click(egui::pos2(level.max.x + crate::spinner::ARROW_WIDTH / 2.0, level.center().y - level.height() / 4.0));
    assert!((h.app.state.scene.environment.level.get() - 1.01).abs() < 1e-4, "{}", h.app.state.scene.environment.level.get());
}
