use loom_commands::{execute, AppState, Command, CoordSystem, PrimitiveKind};

fn state() -> AppState {
    AppState::new()
}

fn create_box(s: &mut AppState) -> loom_scene::NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [1.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

#[test]
fn preferences_undo_levels_apply() {
    let mut s = state();
    assert_eq!(s.prefs.undo_levels, 20);
    execute(&mut s, Command::SetPreference { name: "Scene Undo Levels".into(), value: 2.0 }).unwrap();
    assert_eq!(s.prefs.undo_levels, 2);
    for _ in 0..5 {
        create_box(&mut s);
    }
    assert_eq!(s.undo.len(), 2, "the stack keeps only the last two operations");
    assert!(execute(&mut s, Command::SetPreference { name: "No Such Preference".into(), value: 1.0 }).is_err());
    execute(&mut s, Command::SetPreference { name: "Spinner Precision".into(), value: 1.0 }).unwrap();
    assert_eq!(s.prefs.spinner_precision, 1);
    execute(&mut s, Command::Preferences).unwrap();
    assert_eq!(s.dialog, Some(loom_commands::Dialog::Preferences));
}

#[test]
fn reference_coordinate_system_is_per_tool_unless_constant() {
    let mut s = state();
    execute(&mut s, Command::SelectAndMove).unwrap();
    execute(&mut s, Command::ReferenceCoordinateSystem { system: CoordSystem::Local }).unwrap();
    execute(&mut s, Command::SelectAndRotate).unwrap();
    assert_eq!(s.coord_system, CoordSystem::View, "Rotate keeps its own system");
    execute(&mut s, Command::SelectAndMove).unwrap();
    assert_eq!(s.coord_system, CoordSystem::Local);
    execute(&mut s, Command::SetPreference { name: "Reference Coordinate System Constant".into(), value: 1.0 }).unwrap();
    execute(&mut s, Command::ReferenceCoordinateSystem { system: CoordSystem::World }).unwrap();
    execute(&mut s, Command::SelectAndScale).unwrap();
    assert_eq!(s.coord_system, CoordSystem::World, "Constant: one system for every tool");
}

#[test]
fn units_display_formats_metres() {
    let mut s = state();
    assert_eq!(s.format_units(12.5), "12.5");
    execute(&mut s, Command::SetUnits { system_unit: None, system_scale: None, display: Some("Meters".into()), custom_name: None, custom_value: None, custom_unit: None, fraction: None, default_feet: None, respect_system_units: None }).unwrap();
    // System unit is inches by default: 100 units = 2.54 m.
    assert_eq!(s.format_units(100.0), "2.54m");
    execute(&mut s, Command::SetUnits { system_unit: Some("Centimeters".into()), system_scale: None, display: Some("Millimeters".into()), custom_name: None, custom_value: None, custom_unit: None, fraction: None, default_feet: None, respect_system_units: None }).unwrap();
    assert_eq!(s.format_units(2.5), "25.0mm");
    execute(&mut s, Command::SetUnits { system_unit: None, system_scale: None, display: Some("Decimal Feet".into()), custom_name: None, custom_value: None, custom_unit: None, fraction: None, default_feet: None, respect_system_units: None }).unwrap();
    assert_eq!(s.format_units(30.48), "1.0'");
    execute(&mut s, Command::SetUnits { system_unit: None, system_scale: None, display: Some("Custom".into()), custom_name: Some("FL".into()), custom_value: Some(2.0), custom_unit: Some("Centimeters".into()), fraction: None, default_feet: None, respect_system_units: None }).unwrap();
    assert_eq!(s.format_units(5.0), "2.5FL");
    assert!(execute(&mut s, Command::SetUnits { system_unit: None, system_scale: None, display: Some("Parsecs".into()), custom_name: None, custom_value: None, custom_unit: None, fraction: None, default_feet: None, respect_system_units: None }).is_err());
    execute(&mut s, Command::UnitsSetup).unwrap();
    assert_eq!(s.dialog, Some(loom_commands::Dialog::UnitsSetup));
}

#[test]
fn customize_ui_lists_every_keymap_binding() {
    let rows = loom_commands::keyboard_chart();
    let km = loom_commands::Keymap::main_ui();
    for (chord, action) in km.bindings() {
        assert!(rows.iter().any(|r| r.action == action && r.shortcut.split(", ").any(|c| c == chord.label())), "{action} {} missing from the chart", chord.label());
    }
    assert!(rows.iter().any(|r| r.action == "Create Primitive" && r.shortcut.is_empty() && r.group == "Loom"));
    let dir = std::env::temp_dir().join("loom-chart-test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("keyboard-chart.txt");
    loom_commands::write_keyboard_chart(&path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("Select and Move\tW"));
    let mut s = state();
    execute(&mut s, Command::CustomizeUserInterface).unwrap();
    assert_eq!(s.dialog, Some(loom_commands::Dialog::CustomizeUI));
}

#[test]
fn manipulate_mode_drag_changes_radius() {
    let mut s = state();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Sphere, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    assert!(!s.manipulate);
    execute(&mut s, Command::SelectAndManipulate).unwrap();
    assert!(s.manipulate);
    let radius = |s: &AppState| match s.scene.node(id).unwrap().object.parameters().into_iter().find(|(n, _)| *n == "Radius").unwrap().1 {
        loom_scene::ParamValue::Float(f) => f,
        _ => unreachable!(),
    };
    let before = radius(&s);
    execute(&mut s, Command::Manipulate { id: id.0, name: "Radius".into(), delta: 2.5 }).unwrap();
    assert!((radius(&s) - (before + 2.5)).abs() < 1e-5);
    execute(&mut s, Command::Manipulate { id: id.0, name: "Radius".into(), delta: -100.0 }).unwrap();
    assert!(radius(&s) >= 0.0, "clamped at zero");
    assert!(execute(&mut s, Command::Manipulate { id: id.0, name: "Segments".into(), delta: 1.0 }).is_err(), "only float parameters have manipulators");
    execute(&mut s, Command::ObjectProperties).unwrap();
    assert_eq!(s.dialog, Some(loom_commands::Dialog::ObjectProperties));
}
