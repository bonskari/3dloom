//! Material Editor: shaders, Extended Parameters, the other material types,
//! the workflow buttons and material libraries, clicked as a person does.

use super::*;
use egui::accesskit::Role;
use loom_scene::material::Shader;

/// The editor open on a tall screen (long rollouts).
fn editor() -> Harness {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1600.0));
    material_editor(&mut h);
    h.frames(2);
    h
}

/// Types `text` into the spinner at `at` (click, select all, type, Enter).
fn type_into(h: &mut Harness, at: egui::Pos2, text: &str) {
    h.click(at);
    h.mods = egui::Modifiers::COMMAND;
    h.key(egui::Key::A);
    h.mods = egui::Modifiers::NONE;
    h.run(vec![egui::Event::Text(text.into())]);
    h.key(egui::Key::Enter);
    h.frames(2);
}

/// Scrolls the Material Editor down until the label `label` shows (a person scrolls the rollouts).
fn scroll_to(h: &mut Harness, label: &str) {
    for _ in 0..30 {
        let win = material_editor_rect(h);
        // Shown inside the window, not under its bottom edge.
        let inner = win.shrink2(egui::vec2(0.0, 12.0));
        if h.text_rects(label).into_iter().any(|r| inner.contains_rect(r)) {
            break;
        }
        let p = win.center();
        h.run(vec![egui::Event::PointerMoved(p)]);
        h.run(vec![egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, -120.0), modifiers: egui::Modifiers::NONE }]);
        h.frames(2);
    }
    // Let the scrolling come to rest before clicking.
    h.frames(20);
}

/// The spinner right of the label `label` inside the Material Editor.
fn editor_spinner(h: &mut Harness, label: &str) -> egui::Pos2 {
    scroll_to(h, label);
    let win = material_editor_rect(h);
    let l = h.find_all(Role::Label, label).into_iter().find(|r| win.contains_rect(*r)).unwrap_or_else(|| panic!("no {label:?} label in the editor"));
    h.widgets
        .iter()
        .filter(|(r, _, w)| *r == Role::SpinButton && w.min.x >= l.max.x - 1.0 && (w.center().y - l.center().y).abs() < l.height())
        .min_by(|a, b| a.2.min.x.total_cmp(&b.2.min.x))
        .map(|w| w.2.center())
        .unwrap_or_else(|| panic!("no spinner after {label:?}"))
}

/// Scrolls the Material Editor back to its top.
fn scroll_top(h: &mut Harness) {
    let p = material_editor_rect(h).center();
    h.run(vec![egui::Event::PointerMoved(p)]);
    h.run(vec![egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, 5000.0), modifiers: egui::Modifiers::NONE }]);
    h.frames(3);
}

/// Picks `shader` in the Shader Basic Parameters drop-down.
fn pick_shader(h: &mut Harness, current: &str, shader: &str) {
    scroll_top(h);
    click_in_editor(h, current, 0.0);
    h.frames(2);
    // The pop-up paints last.
    let item = h.text_centers(shader).into_iter().last().unwrap_or_else(|| panic!("{shader:?} is not in the drop-down"));
    h.click(item);
    h.frames(2);
}

#[test]
fn the_shader_drop_down_changes_the_basic_parameters() {
    let mut h = editor();
    let win = material_editor_rect(&h);
    assert!(h.text_rect("Blinn Basic Parameters", win).is_some());
    pick_shader(&mut h, "Blinn", "Oren-Nayar-Blinn");
    assert_eq!(editor_material(&h).shading.shader, Shader::OrenNayarBlinn);
    let win = material_editor_rect(&h);
    assert!(h.text_rect("Oren-Nayar-Blinn Basic Parameters", win).is_some(), "the rollout follows the shader");
    let p = editor_spinner(&mut h, "Roughness");
    type_into(&mut h, p, "40");
    assert_eq!(editor_material(&h).shading.roughness, 40.0);
    let p = editor_spinner(&mut h, "Diffuse Level");
    type_into(&mut h, p, "150");
    assert_eq!(editor_material(&h).shading.diffuse_level, 150.0);
    pick_shader(&mut h, "Oren-Nayar-Blinn", "Strauss");
    assert_eq!(editor_material(&h).shading.shader, Shader::Strauss);
    let win = material_editor_rect(&h);
    assert!(h.text_rect("Strauss Basic Parameters", win).is_some());
    assert!(h.text_rect("Specular Level", win).is_none(), "Strauss has no Specular Level");
    let p = editor_spinner(&mut h, "Metalness");
    type_into(&mut h, p, "80");
    assert_eq!(editor_material(&h).shading.metalness, 80.0);
    pick_shader(&mut h, "Strauss", "Multi-Layer");
    let win = material_editor_rect(&h);
    assert!(h.text_rect("Second Specular Layer", win).is_some());
    let p = editor_spinner(&mut h, "Level 2");
    type_into(&mut h, p, "30");
    assert_eq!(editor_material(&h).shading.level2, 30.0);
}

/// The Type button, then `kind` double-clicked in the Material/Map Browser
/// (and OK in Replace Material for a compound type).
fn choose_type(h: &mut Harness, current: &str, kind: &str) {
    scroll_top(h);
    click_in_editor(h, current, 0.0);
    // Past the double-click time of the Type button's click.
    h.frames(12);
    let p = window_text(h, "Material/Map Browser", kind);
    h.click(p);
    h.click(p);
    h.frames(3);
    if h.text_rects("Replace Material").len() == 1 {
        h.click_text_anywhere("OK");
        h.frames(2);
    }
}

#[test]
fn top_bottom_and_composite_are_chosen_and_set_in_their_rollouts() {
    let mut h = editor();
    choose_type(&mut h, "Standard", "Top/Bottom");
    assert_eq!(editor_material(&h).kind.label(), "Top/Bottom");
    let win = material_editor_rect(&h);
    assert!(h.text_rect("Top/Bottom Basic Parameters", win).is_some());
    let p = editor_spinner(&mut h, "Position");
    type_into(&mut h, p, "80");
    scroll_to(&mut h, "Local");
    click_in_editor(&mut h, "Local", 0.0);
    let loom_scene::material::MaterialKind::TopBottom(t) = editor_material(&h).kind else { panic!("Top/Bottom") };
    assert!(t.local && t.position == 80.0);
    choose_type(&mut h, "Top/Bottom", "Composite");
    assert_eq!(editor_material(&h).kind.label(), "Composite");
    // Mat 1's S button.
    let win = material_editor_rect(&h);
    let s_buttons: Vec<egui::Pos2> = h.text_centers("S").into_iter().filter(|p| win.contains(*p)).collect();
    assert!(!s_buttons.is_empty(), "an A / S / M row per Mat");
    let top = *s_buttons.iter().min_by(|a, b| a.y.total_cmp(&b.y)).unwrap();
    h.click(top);
    h.frames(2);
    let loom_scene::material::MaterialKind::Composite(c) = editor_material(&h).kind else { panic!("Composite") };
    assert_eq!(c.layers[0].mode, 1, "Mat 1 is subtractive");
}

#[test]
fn raytrace_and_architectural_rollouts() {
    let mut h = editor();
    choose_type(&mut h, "Standard", "Raytrace");
    let win = material_editor_rect(&h);
    assert!(h.text_rect("Raytrace Basic Parameters", win).is_some());
    let p = editor_spinner(&mut h, "Index of Refr.");
    type_into(&mut h, p, "1.8");
    let loom_scene::material::MaterialKind::Raytrace(r) = editor_material(&h).kind else { panic!("Raytrace") };
    assert!((r.ior - 1.8).abs() < 1e-4);
    choose_type(&mut h, "Raytrace", "Architectural");
    let win = material_editor_rect(&h);
    assert!(h.text_rect("Physical Qualities", win).is_some());
    // Templates drop-down: Glass - Clear (near the top of the list).
    scroll_to(&mut h, "User Defined");
    click_in_editor(&mut h, "User Defined", 0.0);
    h.frames(2);
    let glass = *h.text_centers("Glass - Clear").last().expect("Glass - Clear in the list");
    h.click(glass);
    h.frames(2);
    let loom_scene::material::MaterialKind::Architectural(a) = editor_material(&h).kind else { panic!("Architectural") };
    assert_eq!(loom_scene::material_types::ARCH_TEMPLATES[a.template as usize].0, "Glass - Clear");
}

#[test]
fn extended_parameters_rollout_sets_ior_and_the_transparency_type() {
    let mut h = editor();
    scroll_to(&mut h, "Extended Parameters");
    click_in_editor(&mut h, "Extended Parameters", 0.0);
    let p = editor_spinner(&mut h, "Index of Refraction");
    type_into(&mut h, p, "1.33");
    assert!((editor_material(&h).extended.ior - 1.33).abs() < 1e-4);
    scroll_to(&mut h, "Additive");
    click_in_editor(&mut h, "Additive", 0.0);
    assert_eq!(editor_material(&h).extended.transparency_type, 2);
    scroll_to(&mut h, "Out");
    click_in_editor(&mut h, "Out", 0.0);
    assert!(editor_material(&h).extended.falloff_out);
}

#[test]
fn put_to_library_then_get_it_from_the_mtl_library_in_the_browser() {
    let mut h = editor();
    h.cmd(Command::RenameMaterial { slot: None, name: "Shiny".into() });
    scroll_top(&mut h);
    click_in_editor(&mut h, "Put to Library", 0.0);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::PutToLibrary));
    h.click_text_anywhere("OK");
    h.frames(2);
    assert_eq!(h.app.state.material_library.entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), vec!["Shiny"]);
    // Another slot gets it through Get Material > Browse From Mtl Library.
    h.cmd(Command::MaterialEditorSlot { slot: 3 });
    click_in_editor(&mut h, "Get Material", 0.0);
    h.frames(12);
    let p = window_text(&h, "Material/Map Browser", "Mtl Library");
    h.click(p);
    h.frames(12);
    let row = window_text(&h, "Material/Map Browser", "Shiny [Standard]");
    h.click(row);
    h.click(row);
    h.frames(3);
    assert_eq!(h.app.state.scene.slot_material(3).name, "Shiny");
}

#[test]
fn utilities_clean_multi_material_removes_unused_sub_materials_on_ok() {
    let mut h = editor();
    h.cmd(Command::CreatePrimitive { kind: loom_commands::PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    h.cmd(Command::SetMaterialType { kind: "Multi/Sub-Object".into(), keep_old: Some(false) });
    h.cmd(Command::AssignMaterialToSelection);
    h.frames(2);
    let id = h.app.state.scene.material_editor.slots[h.app.state.scene.material_editor.active].unwrap();
    let count = |h: &Harness| match &h.app.state.scene.materials[&id].kind {
        loom_scene::material::MaterialKind::MultiSub(l) => l.len(),
        _ => 0,
    };
    assert_eq!(count(&h), 10);
    scroll_top(&mut h);
    click_in_editor(&mut h, "Utilities", 0.0);
    h.frames(2);
    h.click_text_anywhere("Clean MultiMaterial...");
    h.frames(2);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::CleanMultiMaterial));
    assert!(!h.text_rects("Clean Multi-Materials").is_empty(), "the dialog shows");
    h.click_text_anywhere("OK");
    h.frames(2);
    assert!(count(&h) < 10, "unused sub-materials removed: {}", count(&h));
    assert_eq!(h.app.state.dialog, None);
}

#[test]
fn slot_right_click_render_map_shows_the_map_in_the_frame_window() {
    let mut h = editor();
    h.cmd(Command::NewMap { map: "Diffuse Color".into(), kind: "Checker".into() });
    scroll_top(&mut h);
    h.frames(2);
    // The first sample slot: the top-left square widget in the editor.
    let win = material_editor_rect(&h);
    let slot = h
        .widgets
        .iter()
        .filter(|(_, _, r)| win.contains_rect(*r) && r.width() > 40.0 && (r.width() - r.height()).abs() < 1.0)
        .map(|w| w.2)
        .min_by(|a, b| (a.min.y, a.min.x).partial_cmp(&(b.min.y, b.min.x)).unwrap())
        .expect("the sample slots");
    right_click(&mut h, slot.center());
    h.frames(2);
    h.click_text_anywhere("Render Map...");
    h.frames(2);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::RenderMap));
    h.click_text_anywhere("Render");
    h.frames(2);
    let img = h.app.state.rendered.image.clone().expect("rendered into the Rendered Frame Window");
    assert_eq!(img.dimensions(), (320, 240), "the default Dimensions");
    let (a, b) = (img.get_pixel(40, 40).0, img.get_pixel(280, 40).0);
    assert!(a[0].abs_diff(b[0]) > 200, "the checker's two squares: {a:?} {b:?}");
}

#[test]
fn lock_ambient_and_diffuse_maps_check_box_unlocks_the_ambient_map() {
    let mut h = editor();
    scroll_top(&mut h);
    assert!(editor_material(&h).ambient_map_locked);
    click_in_editor(&mut h, "Lock Ambient and Diffuse Maps", 0.0);
    h.frames(2);
    assert!(!editor_material(&h).ambient_map_locked);
}

/// The colour swatch right of the Basic Parameters label `label` ("Diffuse:").
fn swatch_after(h: &Harness, label: &str) -> egui::Pos2 {
    let win = material_editor_rect(h);
    let l = h.text_rects(label).into_iter().find(|r| win.contains_rect(*r)).unwrap_or_else(|| panic!("no {label:?}"));
    h.widgets
        .iter()
        .filter(|(_, _, w)| w.min.x >= l.max.x - 1.0 && (w.center().y - l.center().y).abs() < l.height() && w.width() > 8.0)
        .min_by(|a, b| a.2.min.x.total_cmp(&b.2.min.x))
        .map(|w| w.2.center())
        .unwrap_or_else(|| panic!("no swatch after {label:?}"))
}

#[test]
fn dropping_a_swatch_on_another_asks_copy_or_swap() {
    let mut h = editor();
    h.cmd(Command::SetMaterialParameter { slot: None, name: "Ambient/Diffuse Lock".into(), value: 0.0 });
    h.cmd(Command::SetMaterialColor { slot: None, which: "Diffuse".into(), rgb: [255, 0, 0] });
    scroll_top(&mut h);
    h.frames(2);
    let (d, sp) = (swatch_after(&h, "Diffuse:"), swatch_after(&h, "Specular:"));
    h.drag(d, sp);
    h.frames(2);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::CopySwapColors));
    h.click_text_anywhere("Copy");
    h.frames(2);
    assert_eq!(editor_material(&h).specular.get(), glam::Vec3::X, "Specular got Diffuse's red");
    assert_eq!(h.app.state.dialog, None);
}

#[test]
fn options_menu_propagate_materials_to_instances_toggles() {
    let mut h = editor();
    scroll_top(&mut h);
    let win = material_editor_rect(&h);
    // The menu bar's Options: the topmost "Options" text in the editor.
    let menu = h.text_rects("Options").into_iter().filter(|r| win.contains_rect(*r)).min_by(|a, b| a.min.y.total_cmp(&b.min.y)).expect("Options menu");
    h.click(menu.center());
    h.frames(2);
    h.click_text_anywhere("Propagate Materials to Instances");
    h.frames(2);
    assert!(h.app.state.scene.material_editor.options.propagate_to_instances);
}

#[test]
fn show_end_result_shows_the_whole_material_below_the_top_level() {
    use crate::dialogs::material_editor::{slot_sample, Sample};
    let mut h = editor();
    h.cmd(Command::SetMaterialType { kind: "Multi/Sub-Object".into(), keep_old: Some(false) });
    h.cmd(Command::MaterialEditorGoToSub { index: 1 });
    h.frames(2);
    let multi = |s: &Sample| matches!(s, Sample::Surfaces(loom_scene::material::Surfaces::PerId(_)));
    assert!(multi(&slot_sample(&h.app, 0)), "on (the default): the whole Multi/Sub-Object material");
    scroll_top(&mut h);
    click_in_editor(&mut h, "Show End Result", 0.0);
    h.frames(2);
    assert!(!h.app.state.scene.material_editor.options.show_end_result);
    assert!(!multi(&slot_sample(&h.app, 0)), "off: only the sub-material");
}

#[test]
fn material_id_channel_flyout_sets_the_channel() {
    let mut h = editor();
    scroll_top(&mut h);
    let win = material_editor_rect(&h);
    // The flyout shows the current channel, 0.
    let end = h.text_rects("Show End Result").into_iter().find(|r| win.contains_rect(*r)).expect("toolbar");
    let combo = h.widgets.iter().filter(|(role, _, r)| *role == Role::ComboBox && win.contains_rect(*r) && (r.center().y - end.center().y).abs() < 14.0).map(|w| w.2).min_by(|a, b| (a.center().x - end.min.x).abs().total_cmp(&(b.center().x - end.min.x).abs())).unwrap_or_else(|| panic!("no flyout near {end:?}: {:?}", h.widgets.iter().filter(|w| w.0 == Role::ComboBox).collect::<Vec<_>>()));
    let nines = |h: &Harness| h.widgets.iter().filter(|w| w.1 == "9").map(|w| w.2).collect::<Vec<_>>();
    let before = nines(&h);
    h.click(combo.center());
    h.frames(2);
    // The "9" the opened list adds.
    let after = nines(&h);
    let nine = after.iter().copied().find(|r| !before.contains(r)).unwrap_or_else(|| panic!("the list did not open at {combo:?}: {before:?} -> {after:?}"));
    h.click(nine.center());
    h.frames(2);
    assert_eq!(editor_material(&h).effects_channel, 9);
}

#[test]
fn type_button_right_click_copy_then_paste_copy() {
    let mut h = editor();
    h.cmd(Command::NewMap { map: "Diffuse Color".into(), kind: "Checker".into() });
    scroll_top(&mut h);
    let win = material_editor_rect(&h);
    let ty = h.find_all(Role::Button, "Checker").into_iter().find(|r| win.contains_rect(*r)).expect("the Type button");
    right_click(&mut h, ty.center());
    h.click_text_anywhere("Copy");
    h.frames(2);
    assert!(matches!(h.app.state.material_clip, Some(loom_commands::material::MaterialClip::Map(_))));
    h.cmd(Command::MaterialEditorGoToParent);
    h.cmd(Command::NewMap { map: "Bump".into(), kind: "Noise".into() });
    h.frames(2);
    let win = material_editor_rect(&h);
    let ty = h.find_all(Role::Button, "Noise").into_iter().find(|r| win.contains_rect(*r)).expect("the Type button");
    right_click(&mut h, ty.center());
    h.click_text_anywhere("Paste (Copy)");
    h.frames(2);
    h.cmd(Command::MaterialEditorGoToParent);
    assert_eq!(editor_material(&h).map("Bump").map(|m| m.kind), Some(loom_scene::material::MapType::Checker));
}

#[test]
fn material_explorer_assigns_the_picked_material_to_the_selection() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: loom_commands::PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    h.cmd(Command::RenameMaterial { slot: None, name: "Brick".into() });
    h.cmd(Command::AssignMaterialToSelection);
    let brick = h.app.state.scene.material_editor.slots[0].unwrap();
    h.cmd(Command::CreatePrimitive { kind: loom_commands::PrimitiveKind::Box, position: [30.0, 0.0, 0.0], size: [10.0; 3] });
    let b = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::MaterialExplorer);
    h.frames(2);
    h.click_text_anywhere("Brick");
    h.frames(1);
    h.click_text_anywhere("Assign to Selection");
    h.frames(2);
    assert_eq!(h.app.state.scene.node(b).unwrap().material, Some(brick));
}

#[test]
fn bitmap_time_end_condition_radio() {
    let mut h = editor();
    h.cmd(Command::NewMap { map: "Diffuse Color".into(), kind: "Bitmap".into() });
    scroll_to(&mut h, "Ping Pong");
    click_in_editor(&mut h, "Ping Pong", 0.0);
    h.frames(2);
    let ed = &h.app.state.scene.material_editor;
    let m = &h.app.state.scene.materials[&ed.slots[ed.active].unwrap()];
    assert_eq!(m.map("Diffuse Color").unwrap().time.end, 1);
}

#[test]
fn use_real_world_scale_check_box() {
    let mut h = editor();
    h.cmd(Command::NewMap { map: "Diffuse Color".into(), kind: "Checker".into() });
    scroll_to(&mut h, "Use Real-World Scale");
    click_in_editor(&mut h, "Use Real-World Scale", 0.0);
    h.frames(2);
    let ed = &h.app.state.scene.material_editor;
    let m = &h.app.state.scene.materials[&ed.slots[ed.active].unwrap()];
    assert!(m.map("Diffuse Color").unwrap().coords.real_world);
    assert!(!h.text_rects("Size").is_empty(), "Tiling reads Size");
}

#[test]
fn missing_map_coordinates_dialog_lists_the_object_and_cancels() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: loom_commands::PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    h.cmd(Command::NewMap { map: "Diffuse Color".into(), kind: "Checker".into() });
    h.cmd(Command::SetMapParameter { name: "Map Channel".into(), value: 4.0 });
    h.cmd(Command::AssignMaterialToSelection);
    h.cmd(Command::Render { path: None, width: Some(8), height: Some(8) });
    h.frames(2);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::MissingMapCoordinates));
    let name = h.app.state.scene.nodes().next().unwrap().name.clone();
    assert!(!h.text_rects(&format!("(UVW 4): {name}")).is_empty(), "the channel and the object are listed");
    h.click_text_anywhere("Cancel");
    h.frames(2);
    assert_eq!(h.app.state.dialog, None);
    assert!(h.app.state.rendered.image.is_none(), "nothing rendered");
}

#[test]
fn video_color_check_toggle() {
    let mut h = editor();
    scroll_top(&mut h);
    click_in_editor(&mut h, "Video Color Check", 0.0);
    h.frames(2);
    assert!(h.app.state.scene.material_editor.options.video_color_check);
}

#[test]
fn make_preview_saves_the_slot_animating_over_the_frames() {
    let mut h = editor();
    scroll_top(&mut h);
    click_in_editor(&mut h, "Make Preview", 0.0);
    h.frames(2);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::MaterialPreview));
    // Diffuse red at 0, blue at 10 (Auto Key).
    h.cmd(Command::SetMaterialParameter { slot: None, name: "Self-Illumination".into(), value: 100.0 });
    h.cmd(Command::SetMaterialColor { slot: None, which: "Diffuse".into(), rgb: [255, 0, 0] });
    h.cmd(Command::AutoKeyModeToggle);
    h.cmd(Command::SetFrame { frame: 10 });
    h.cmd(Command::SetMaterialColor { slot: None, which: "Diffuse".into(), rgb: [0, 0, 255] });
    h.cmd(Command::AutoKeyModeToggle);
    let dir = std::env::temp_dir().join(format!("loom_mtl_preview_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.png").to_string_lossy().into_owned();
    h.cmd(Command::MakeMaterialPreview { start: 0, end: 10, every: Some(5), size: 32, path: path.clone() });
    h.frames(2);
    let px = |n: i32| image::open(loom_scene::render::numbered_path(&path, n)).unwrap().to_rgba8().get_pixel(16, 16).0;
    let (a, c) = (px(0), px(2));
    assert!(a[0] > a[2] + 100, "frame 0 red: {a:?}");
    assert!(c[2] > c[0] + 100, "frame 10 blue: {c:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
