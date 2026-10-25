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
