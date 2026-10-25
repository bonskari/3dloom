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
