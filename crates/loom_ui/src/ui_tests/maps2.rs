//! The 2D, 3D, compositor and Falloff maps in the Material Editor, clicked
//! as a person does: picked by double-click in the Material/Map Browser,
//! changed through their rollouts; the sample slot shades 3D and
//! view-dependent maps at the sample object's points.

use super::*;
use egui::accesskit::Role;
use loom_scene::material::MapType;

/// The Material Editor on a box, a new map of `kind` in Diffuse Color picked
/// from the Browser by double-click.
fn pick_diffuse_map(h: &mut Harness, kind: &str) {
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1400.0));
    material_editor(h);
    let win = material_editor_rect(h);
    click_text(h, "Blinn Basic Parameters", win, 0.0);
    click_in_editor(h, "Maps", 0.0);
    h.click(row_text(h, "Diffuse Color", "None", material_editor_rect(h)));
    h.frames(12);
    double_click_in_browser(h, kind);
    assert_eq!(h.app.state.dialog, None, "a double-click picks {kind}");
    // The editor's rollouts from their top (the Browser's wheel may have reached them).
    let win = material_editor_rect(h);
    // Over the rollouts (the lower part of the editor, below the sample slots).
    h.run(vec![egui::Event::PointerMoved(win.center_bottom() - egui::vec2(0.0, 60.0))]);
    for _ in 0..5 {
        h.run(vec![egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, 2000.0), modifiers: egui::Modifiers::NONE }]);
    }
    h.frames(3);
}

/// Scrolls the Material/Map Browser's list until `kind` shows, then double-clicks it.
fn double_click_in_browser(h: &mut Harness, kind: &str) {
    let area = h.ctx.memory(|m| m.area_rect(egui::Id::new("Material/Map Browser"))).expect("the Browser");
    for _ in 0..40 {
        if let Some(r) = h.text_rects(kind).into_iter().find(|r| area.contains_rect(*r)) {
            h.click(r.center());
            h.click(r.center());
            h.frames(3);
            return;
        }
        // The wheel over the list, a few lines down.
        let list = h.text_rects("Bitmap").into_iter().chain(h.text_rects("Mix")).find(|r| area.contains_rect(*r)).map_or(area.center(), |r| r.center());
        h.run(vec![egui::Event::PointerMoved(list)]);
        h.run(vec![egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, -60.0), modifiers: egui::Modifiers::NONE }]);
        // Until the smooth scroll settles, so the rows stay where they are found.
        h.frames(30);
    }
    panic!("{kind:?} never shows in the Browser");
}

fn diffuse(h: &Harness) -> loom_scene::material::Map {
    editor_material(h).diffuse_map.expect("a diffuse map")
}

/// Clicks the spinner after `label` and types `value`.
fn type_into(h: &mut Harness, label: &str, value: &str) {
    let s = h.spinner_after(label);
    h.click(s.center());
    h.run(vec![egui::Event::Text(value.to_owned())]);
    h.key(egui::Key::Enter);
    h.frames(2);
}

/// Opens the drop-down list showing `current` on the row `row` and picks `item`.
fn pick_in_list(h: &mut Harness, row: &str, current: &str, item: &str) {
    h.click(row_text(h, row, current, material_editor_rect(h)));
    h.frames(2);
    h.click_text_anywhere(item);
    h.frames(2);
}

/// The material's surface at full Self-Illumination with no highlight: the slot shows the diffuse map as it is.
fn lit_by_itself(m: &loom_scene::material::StandardMaterial) -> loom_scene::material::Surface {
    let mut s = loom_scene::material::Surface::from_standard(m);
    s.self_illum = 1.0;
    s.specular = glam::Vec3::ZERO;
    s
}

#[test]
fn wood_from_the_browser_has_3d_coordinates_and_its_parameters() {
    let mut h = Harness::new();
    pick_diffuse_map(&mut h, "Wood");
    assert_eq!(diffuse(&h).kind, MapType::Wood);
    let win = material_editor_rect(&h);
    for label in ["Wood Parameters", "Coordinates", "Source:", "Grain Thickness:", "Radial Noise:", "Axial Noise:", "Color #1", "Color #2", "Blur Offset:"] {
        assert!(h.text_rect(label, win).is_some(), "{label:?} at the Wood level");
    }
    assert!(h.text_rect("Mirror", win).is_none(), "no 2D Mirror / Tile for a 3D map");
    type_into(&mut h, "Grain Thickness:", "3");
    assert_eq!(diffuse(&h).extra.grain_thickness, 3.0);
    type_into(&mut h, "Radial Noise:", "2.5");
    assert_eq!(diffuse(&h).extra.radial_noise, 2.5);
    // Source: World XYZ from its list.
    pick_in_list(&mut h, "Source:", "Object XYZ", "World XYZ");
    assert_eq!(diffuse(&h).coords3.source, 1);
    // Swap trades the colours.
    let before = diffuse(&h).colors;
    click_in_editor(&mut h, "Swap", 0.0);
    assert_eq!(diffuse(&h).colors[0], before[1]);
}

#[test]
fn tiles_preset_and_gaps_from_its_rollouts() {
    let mut h = Harness::new();
    pick_diffuse_map(&mut h, "Tiles");
    let win = material_editor_rect(&h);
    for label in ["Standard Controls", "Advanced Controls", "Preset Type:", "Horiz. Count:", "Grout Setup", "Mirror"] {
        assert!(h.text_rect(label, win).is_some(), "{label:?} at the Tiles level");
    }
    pick_in_list(&mut h, "Preset Type:", "Stack Bond", "Running Bond");
    assert_eq!(diffuse(&h).extra.preset, 1);
    type_into(&mut h, "Horizontal Gap:", "2");
    assert_eq!(diffuse(&h).extra.gap, [2.0, 2.0], "the gaps are locked");
    type_into(&mut h, "Horiz. Count:", "6");
    assert_eq!(diffuse(&h).extra.horiz_count, 6.0);
}

#[test]
fn composite_layers_are_added_and_filled_from_their_rollouts() {
    let mut h = Harness::new();
    pick_diffuse_map(&mut h, "Composite");
    assert!(h.text_rect("Total Layers: 1", material_editor_rect(&h)).is_some());
    click_in_editor(&mut h, "Add a New Layer", 0.0);
    assert_eq!(diffuse(&h).layers.len(), 2);
    assert!(h.text_rect("Layer 2", material_editor_rect(&h)).is_some(), "layer 2's rollout");
    // Layer 2 is drawn first (the top of the stack).
    type_into(&mut h, "Opacity:", "40");
    assert_eq!(diffuse(&h).layers[1].opacity, 40.0);
    pick_in_list(&mut h, "Blend Mode:", "Normal", "Multiply");
    assert_eq!(diffuse(&h).layers[1].blend, 5);
    // Layer 2's map button goes through the Browser to a new Checker below it.
    let win = material_editor_rect(&h);
    let none = h.text_rects("None").into_iter().filter(|r| win.contains_rect(*r)).min_by(|a, b| a.min.y.total_cmp(&b.min.y).then(a.min.x.total_cmp(&b.min.x))).expect("layer 2's map button");
    h.click(none.center());
    h.frames(12);
    double_click_in_browser(&mut h, "Checker");
    assert_eq!(h.app.state.scene.material_editor.path, vec![loom_scene::material::NavStep::Map("Diffuse Color".into()), loom_scene::material::NavStep::Map("Layer 2".into())]);
    assert_eq!(diffuse(&h).sub_map(2).map(|m| m.kind), Some(MapType::Checker));
    click_in_editor(&mut h, "Go to Parent", 0.0);
    assert!(h.text_rect("Composite Layers", material_editor_rect(&h)).is_some());
}

#[test]
fn gradient_ramp_type_and_flags_from_its_rollout() {
    let mut h = Harness::new();
    pick_diffuse_map(&mut h, "Gradient Ramp");
    let win = material_editor_rect(&h);
    for label in ["Gradient Ramp Parameters", "Flag #1", "Flag #2", "Flag #3", "Gradient Type:", "Interpolation:"] {
        assert!(h.text_rect(label, win).is_some(), "{label:?} at the Gradient Ramp level");
    }
    pick_in_list(&mut h, "Gradient Type:", "Linear", "Radial");
    assert_eq!(diffuse(&h).extra.ramp_type, 8);
    // Flag #3's Delete (Flag #1 and #2 are the ends and stay).
    let delete = h.find_all(Role::Button, "Delete").into_iter().last().expect("Flag #3's Delete");
    h.click(delete.center());
    h.frames(2);
    assert_eq!(diffuse(&h).flags.len(), 2);
}

#[test]
fn falloff_type_and_ior_from_its_rollout_and_the_sample_slot_shades_it() {
    let mut h = Harness::new();
    pick_diffuse_map(&mut h, "Falloff");
    let win = material_editor_rect(&h);
    for label in ["Falloff Parameters", "Front : Side", "Falloff Type:", "Falloff Direction:", "Mode Specific Parameters"] {
        assert!(h.text_rect(label, win).is_some(), "{label:?} at the Falloff level");
    }
    pick_in_list(&mut h, "Falloff Type:", "Perpendicular / Parallel", "Fresnel");
    assert_eq!(diffuse(&h).extra.falloff_type, 2);
    click_in_editor(&mut h, "Override Material IOR", 0.0);
    assert!(diffuse(&h).extra.override_ior);
    type_into(&mut h, "Index of Refraction:", "2.4");
    assert!((diffuse(&h).extra.ior - 2.4).abs() < 1e-5);

    // The sample sphere: Front (black) where it faces the view, Side (white) at its rim.
    let mut m = editor_material(&h);
    m.diffuse_map.as_mut().unwrap().extra.falloff_type = 1;
    let surfaces = loom_scene::material::Surfaces::Single(Box::new(lit_by_itself(&m)));
    let setup = crate::dialogs::material_editor::PreviewSetup::from_options(&loom_scene::material::EditorOptions::default(), loom_scene::material::SampleType::Sphere, (0.0, 0.0));
    let n = 64;
    let px = crate::dialogs::material_editor::preview_pixels(&crate::dialogs::material_editor::Sample::Surfaces(surfaces), n, &setup, &loom_scene::maps::Images::new());
    let centre = px[(n / 2) * n + n / 2];
    let rim = px[(n / 2) * n + 4];
    assert!(centre.r() < 40, "the facing middle is Front: {centre:?}");
    assert!(rim.r() > 150, "the rim is Side: {rim:?}");
}

#[test]
fn a_3d_map_in_the_sample_slot_follows_the_object_not_the_uvs() {
    // Noise at Object XYZ on the sphere: the slot image differs from the
    // same map read through the sphere's UVs (Explicit Map Channel).
    let mut m = loom_scene::material::StandardMaterial::new("n");
    let mut noise = loom_scene::material::Map::new(MapType::Noise, 100.0);
    noise.set_parameter("Size", 10.0).unwrap();
    m.diffuse_map = Some(noise.clone());
    let setup = crate::dialogs::material_editor::PreviewSetup::from_options(&loom_scene::material::EditorOptions::default(), loom_scene::material::SampleType::Sphere, (0.0, 0.0));
    let shot = |m: &loom_scene::material::StandardMaterial| {
        let s = loom_scene::material::Surfaces::Single(Box::new(lit_by_itself(m)));
        crate::dialogs::material_editor::preview_pixels(&crate::dialogs::material_editor::Sample::Surfaces(s), 48, &setup, &loom_scene::maps::Images::new())
    };
    let object = shot(&m);
    noise.set_parameter("Source", 2.0).unwrap();
    m.diffuse_map = Some(noise);
    let uvw = shot(&m);
    let differ = object.iter().zip(&uvw).filter(|(a, b)| (a.r() as i32 - b.r() as i32).abs() > 8).count();
    assert!(differ > 100, "{differ} pixels differ");
}
