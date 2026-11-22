//! Units Setup: the fractional US units and their fractions, Default Units
//! of Feet w/ Inches, and the Units Mismatch dialog of a file in other system units.

use loom_commands::{execute, AppState, Command, Dialog, PrimitiveKind};

fn units(s: &mut AppState, system: Option<&str>, display: Option<&str>, fraction: Option<u32>, default_feet: Option<bool>, respect: Option<bool>) {
    execute(s, Command::SetUnits { system_unit: system.map(str::to_owned), system_scale: None, display: display.map(str::to_owned), custom_name: None, custom_value: None, custom_unit: None, fraction, default_feet, respect_system_units: respect }).unwrap();
}

#[test]
fn fractional_inches_and_feet_show_fractions_of_the_chosen_size() {
    let mut s = AppState::new(); // 1 unit = 1 inch
    units(&mut s, None, Some("Fractional Inches"), Some(8), None, None);
    assert_eq!(s.format_units(3.125), "3 1/8\"");
    assert_eq!(s.format_units(0.5), "1/2\"", "reduced");
    assert_eq!(s.format_units(2.0), "2\"");
    units(&mut s, None, None, Some(4), None, None);
    assert_eq!(s.format_units(3.125), "3 1/4\"", "to the nearest quarter");
    units(&mut s, None, Some("Feet w/ Fractional Inches"), Some(16), None, None);
    assert_eq!(s.format_units(67.25), "5'7 1/4\"");
    units(&mut s, None, Some("Fractional Feet"), Some(2), None, None);
    assert_eq!(s.format_units(30.0), "2 1/2'");
    assert!(execute(&mut s, Command::SetUnits { system_unit: None, system_scale: None, display: None, custom_name: None, custom_value: None, custom_unit: None, fraction: Some(3), default_feet: None, respect_system_units: None }).is_err(), "1/3 is not offered");
}

#[test]
fn typed_fractions_parse_and_default_units_pick_feet_or_inches() {
    let mut s = AppState::new();
    let near = |v: Option<f64>, want: f64| v.is_some_and(|v| (v - want).abs() < 1e-9);
    units(&mut s, None, Some("Feet w/ Fractional Inches"), None, Some(true), None);
    assert!(near(s.units.parse("3 1/2\""), 3.5));
    assert!(near(s.units.parse("5'7 1/4\""), 67.25));
    assert!(near(s.units.parse("1/4'"), 3.0));
    assert!(near(s.units.parse("5"), 60.0), "Default Units Feet: 5 is five feet");
    units(&mut s, None, None, None, Some(false), None);
    assert_eq!(s.units.parse("5"), Some(5.0), "Default Units Inches: 5 is five inches");
    assert!(near(s.units.parse("5'"), 60.0));
}

fn saved_in(system: &str, name: &str) -> String {
    let dir = std::env::temp_dir().join("loom-units-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name).to_string_lossy().into_owned();
    let mut s = AppState::new();
    units(&mut s, Some(system), None, None, None, None);
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [10.0, 0.0, 0.0], size: [10.0; 3] }).unwrap();
    execute(&mut s, Command::SaveFileAs { path: Some(path.clone()) }).unwrap();
    path
}

fn box_x(s: &AppState) -> f32 {
    s.scene.nodes().next().unwrap().transform.position.get().x
}

#[test]
fn a_file_in_other_system_units_asks_to_rescale_or_adopt() {
    let path = saved_in("Feet", "feet.loom");
    // Rescale: 10 feet become 120 inches.
    let mut s = AppState::new();
    let r = execute(&mut s, Command::OpenFile { path: Some(path.clone()) }).unwrap();
    assert_eq!(s.dialog, Some(Dialog::UnitsMismatch), "{r}");
    assert_eq!(s.scene.len(), 0, "nothing loaded yet");
    execute(&mut s, Command::UnitsMismatchReply { rescale: true }).unwrap();
    assert!((box_x(&s) - 120.0).abs() < 1e-3, "{}", box_x(&s));
    assert_eq!(s.units.system_unit, loom_commands::SystemUnit::Inches);
    // Adopt: the scene keeps feet.
    let mut t = AppState::new();
    execute(&mut t, Command::OpenFile { path: Some(path.clone()) }).unwrap();
    execute(&mut t, Command::UnitsMismatchReply { rescale: false }).unwrap();
    assert_eq!(box_x(&t), 10.0);
    assert_eq!(t.units.system_unit, loom_commands::SystemUnit::Feet);
    // Respect System Units in Files off: no question, the numbers in our units.
    let mut u = AppState::new();
    units(&mut u, None, None, None, None, Some(false));
    execute(&mut u, Command::OpenFile { path: Some(path) }).unwrap();
    assert_eq!(u.dialog, None);
    assert_eq!((box_x(&u), u.units.system_unit), (10.0, loom_commands::SystemUnit::Inches));
    assert!(execute(&mut u, Command::UnitsMismatchReply { rescale: true }).is_err(), "nothing waits for an answer");
}

fn float_param(s: &AppState, id: loom_scene::NodeId, name: &str) -> f32 {
    match s.scene.node(id).unwrap().object.parameters().into_iter().find(|(n, _)| *n == name) {
        Some((_, loom_scene::ParamValue::Float(f))) => f,
        other => panic!("{name}: {other:?}"),
    }
}

/// Rescale World Units scales lengths only: a light's Multiplier, Hotspot
/// and Falloff, a camera's FOV and Lens and a torus's Rotation stay.
#[test]
fn rescale_world_units_scales_only_length_parameters() {
    let mut s = AppState::new();
    let new_id = |s: &AppState| *s.selection.iter().next().unwrap();
    execute(&mut s, Command::CreateLight { kind: loom_commands::light::LightKind::FreeSpot, position: [0.0, 0.0, 50.0], target: [0.0; 3] }).unwrap();
    let spot = new_id(&s);
    execute(&mut s, Command::CreateCameraAt { target: false, position: [0.0, -100.0, 0.0], look_at: [0.0; 3] }).unwrap();
    let cam = new_id(&s);
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Torus, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let torus = new_id(&s);
    execute(&mut s, Command::SetParameter { id: torus.0, name: "Rotation".into(), value: 30.0 }).unwrap();
    let floats = |s: &AppState, id: loom_scene::NodeId| -> Vec<(&'static str, f32)> {
        s.scene.node(id).unwrap().object.parameters().into_iter().filter_map(|(n, v)| if let loom_scene::ParamValue::Float(f) = v { Some((n, f)) } else { None }).collect()
    };
    let before: Vec<(loom_scene::NodeId, &str, f32)> = [spot, cam, torus].into_iter().flat_map(|id| floats(&s, id).into_iter().map(move |(n, f)| (id, n, f))).collect();
    execute(&mut s, Command::RescaleWorldUnits { factor: 10.0, selection: None }).unwrap();
    let scaled = ["Target Distance", "Decay Start", "Near Start", "Near End", "Far Start", "Far End", "Near Range", "Far Range", "Near Clip", "Far Clip", "Radius 1", "Radius 2"];
    let kept = ["Multiplier", "Hotspot", "Falloff", "Aspect", "Contrast", "FOV", "Lens", "Rotation", "Twist", "Slice From", "Slice To"];
    for (id, name, was) in before {
        let now = float_param(&s, id, name);
        if scaled.contains(&name) {
            assert!((now - was * 10.0).abs() <= 1e-3 * was.abs().max(1.0), "{name} scales: {was} -> {now}");
        } else if kept.contains(&name) {
            assert!((now - was).abs() <= 1e-4 * was.abs().max(1.0), "{name} stays: {was} -> {now}");
        }
    }
    assert_eq!(float_param(&s, spot, "Multiplier"), 1.0);
    assert_eq!(float_param(&s, torus, "Rotation"), 30.0);
}

fn mod_param(s: &AppState, id: loom_scene::NodeId, slot: usize, name: &str) -> f32 {
    match s.scene.node(id).unwrap().modifiers[slot].modifier.parameters().into_iter().find(|(n, _)| *n == name) {
        Some((_, loom_scene::ParamValue::Float(f))) => f,
        other => panic!("{name}: {other:?}"),
    }
}

/// Rescale World Units scales the modifiers' lengths too: limits, amounts,
/// gizmo extents, gizmo positions and Centers; angles stay.
#[test]
fn rescale_world_units_scales_modifier_lengths_and_gizmos() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    for (name, param, value) in [("Bend", "Upper Limit", 4.0), ("Push", "Push Value", 2.0), ("Shell", "Outer Amount", 3.0), ("UVW Map", "Length", 12.0)] {
        execute(&mut s, Command::AddModifier { name: name.into() }).unwrap();
        let slot = s.scene.node(id).unwrap().modifiers.iter().position(|m| m.name == name).unwrap();
        execute(&mut s, Command::SetModifierParameter { index: slot as u32, name: param.into(), value }).unwrap();
    }
    let bend = s.scene.node(id).unwrap().modifiers.iter().position(|m| m.name == "Bend").unwrap();
    execute(&mut s, Command::SetModifierParameter { index: bend as u32, name: "Angle".into(), value: 45.0 }).unwrap();
    {
        let m = &mut s.scene.node_mut(id).unwrap().modifiers[bend].modifier;
        m.gizmo_mut().unwrap().position.set(glam::Vec3::new(1.0, 2.0, 3.0));
        if let loom_scene::modifier::Modifier::Bend { d, .. } = m {
            d.center.set(glam::Vec3::new(0.0, 0.0, 5.0));
        }
    }
    let slots: Vec<(String, usize)> = s.scene.node(id).unwrap().modifiers.iter().enumerate().map(|(i, m)| (m.name.clone(), i)).collect();
    let at = |n: &str| slots.iter().find(|(m, _)| m == n).unwrap().1;
    execute(&mut s, Command::RescaleWorldUnits { factor: 10.0, selection: None }).unwrap();
    assert_eq!(mod_param(&s, id, at("Bend"), "Upper Limit"), 40.0);
    assert_eq!(mod_param(&s, id, at("Bend"), "Angle"), 45.0, "angles stay");
    assert_eq!(mod_param(&s, id, at("Push"), "Push Value"), 20.0);
    assert_eq!(mod_param(&s, id, at("Shell"), "Outer Amount"), 30.0);
    assert_eq!(mod_param(&s, id, at("UVW Map"), "Length"), 120.0);
    let m = &s.scene.node(id).unwrap().modifiers[at("Bend")].modifier;
    assert_eq!(m.gizmo().unwrap().position.get(), glam::Vec3::new(10.0, 20.0, 30.0));
    assert_eq!(m.deformer().unwrap().center.get(), glam::Vec3::new(0.0, 0.0, 50.0));
}

/// Affect: Selection rescales only the selected objects, about the world origin.
#[test]
fn rescale_world_units_selection_only_touches_the_selection() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [5.0, 0.0, 0.0], size: [10.0; 3] }).unwrap();
    let a = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [-5.0, 0.0, 0.0], size: [10.0; 3] }).unwrap();
    let b = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::RescaleWorldUnits { factor: 2.0, selection: Some(true) }).unwrap();
    assert_eq!(float_param(&s, b, "Length"), 20.0);
    assert_eq!(s.scene.node(b).unwrap().transform.position.get(), glam::Vec3::new(-10.0, 0.0, 0.0));
    assert_eq!(float_param(&s, a, "Length"), 10.0, "not selected");
    assert_eq!(s.scene.node(a).unwrap().transform.position.get(), glam::Vec3::new(5.0, 0.0, 0.0));
}

/// An Edit Poly modifier's moved vertex keeps its place relative to the
/// object after Rescale World Units: the whole result is 10 times larger.
#[test]
fn rescale_world_units_scales_edit_poly_modifier_geometry() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::AddModifier { name: "Edit Poly".into() }).unwrap();
    {
        let m = &mut s.scene.node_mut(id).unwrap().modifiers[0].modifier;
        if let loom_scene::modifier::Modifier::EditPoly { poly, .. } = m {
            poly.mesh.vertices_mut()[0].position += glam::Vec3::new(0.0, 0.0, 3.0);
        }
    }
    let before: Vec<glam::Vec3> = s.scene.eval_mesh(id).value.mesh.vertices().iter().map(|v| v.position).collect();
    execute(&mut s, Command::RescaleWorldUnits { factor: 10.0, selection: None }).unwrap();
    let after: Vec<glam::Vec3> = s.scene.eval_mesh(id).value.mesh.vertices().iter().map(|v| v.position).collect();
    assert_eq!(before.len(), after.len());
    for (b, a) in before.iter().zip(&after) {
        assert!((*b * 10.0 - *a).length() < 1e-3, "{b} -> {a}");
    }
}

/// Type-in entry as the reference's examples: a series is summed, each
/// number in its own unit, a plain number after feet is inches in US
/// Standard and fractions take a unit (system unit inches).
#[test]
fn typed_series_sum_with_their_own_units() {
    let near = |a: Option<f64>, b: f64| a.is_some_and(|a| (a - b).abs() < 1e-3);
    let mut s = AppState::new();
    units(&mut s, Some("Inches"), Some("Centimeters"), None, None, None);
    let cm = 1.0 / 2.54;
    assert!(near(s.units.parse("1'"), 12.0), "1' is 30.48cm");
    assert!(near(s.units.parse("14 286 175"), 475.0 * cm), "the series totals 475cm");
    assert!(near(s.units.parse("1' 1"), 12.0 + cm), "a foot and a centimetre");
    units(&mut s, None, Some("Feet w/ Fractional Inches"), None, Some(true), None);
    assert!(near(s.units.parse("12"), 144.0), "Default Units Feet: 12'");
    assert!(near(s.units.parse("1' 2"), 14.0), "after feet, inches");
    assert!(near(s.units.parse("18/3"), 72.0), "6'");
    assert!(near(s.units.parse("18/3\""), 6.0), "0'6\"");
    assert!(near(s.units.parse("18/3cm"), 6.0 * cm), "converted on the fly");
    assert!(near(s.units.parse("-1' 2"), -14.0));
    assert_eq!(s.units.parse("2 furlongs"), None);
}
