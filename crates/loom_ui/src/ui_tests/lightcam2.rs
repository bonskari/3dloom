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
