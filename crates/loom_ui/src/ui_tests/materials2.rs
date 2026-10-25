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

/// The spinner right of the label `label` inside the Material Editor.
fn editor_spinner(h: &Harness, label: &str) -> egui::Pos2 {
    let win = material_editor_rect(h);
    let l = h.find_all(Role::Label, label).into_iter().find(|r| win.contains_rect(*r)).unwrap_or_else(|| panic!("no {label:?} label in the editor"));
    h.widgets
        .iter()
        .filter(|(r, _, w)| *r == Role::SpinButton && w.min.x >= l.max.x - 1.0 && (w.center().y - l.center().y).abs() < l.height())
        .min_by(|a, b| a.2.min.x.total_cmp(&b.2.min.x))
        .map(|w| w.2.center())
        .unwrap_or_else(|| panic!("no spinner after {label:?}"))
}

/// Picks `shader` in the Shader Basic Parameters drop-down.
fn pick_shader(h: &mut Harness, current: &str, shader: &str) {
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
    let p = editor_spinner(&h, "Roughness");
    type_into(&mut h, p, "40");
    assert_eq!(editor_material(&h).shading.roughness, 40.0);
    let p = editor_spinner(&h, "Diffuse Level");
    type_into(&mut h, p, "150");
    assert_eq!(editor_material(&h).shading.diffuse_level, 150.0);
    pick_shader(&mut h, "Oren-Nayar-Blinn", "Strauss");
    assert_eq!(editor_material(&h).shading.shader, Shader::Strauss);
    let win = material_editor_rect(&h);
    assert!(h.text_rect("Strauss Basic Parameters", win).is_some());
    assert!(h.text_rect("Specular Level", win).is_none(), "Strauss has no Specular Level");
    let p = editor_spinner(&h, "Metalness");
    type_into(&mut h, p, "80");
    assert_eq!(editor_material(&h).shading.metalness, 80.0);
    pick_shader(&mut h, "Strauss", "Multi-Layer");
    let win = material_editor_rect(&h);
    assert!(h.text_rect("Second Specular Layer", win).is_some());
    let p = editor_spinner(&h, "Level 2");
    type_into(&mut h, p, "30");
    assert_eq!(editor_material(&h).shading.level2, 30.0);
}

#[test]
fn extended_parameters_rollout_sets_ior_and_the_transparency_type() {
    let mut h = editor();
    click_in_editor(&mut h, "Extended Parameters", 0.0);
    let p = editor_spinner(&h, "Index of Refraction");
    type_into(&mut h, p, "1.33");
    assert!((editor_material(&h).extended.ior - 1.33).abs() < 1e-4);
    click_in_editor(&mut h, "Additive", 0.0);
    assert_eq!(editor_material(&h).extended.transparency_type, 2);
    click_in_editor(&mut h, "Out", 0.0);
    assert!(editor_material(&h).extended.falloff_out);
}
