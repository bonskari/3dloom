//! The 2D, 3D, compositor, colour modifier and Falloff maps through the
//! commands: New Map of every type, parameters by label with their ranges,
//! colour components, Composite layers and Gradient Ramp flags, the editor's
//! levels below them, undo and the .loom file.

use loom_commands::file::{read, write, Document, SceneSettings, FORMAT};
use loom_commands::{execute, AppState, Command};
use loom_scene::material::{Map, MapType, NavStep};

fn param(s: &mut AppState, name: &str, value: f64) {
    execute(s, Command::SetMapParameter { name: name.into(), value }).unwrap_or_else(|e| panic!("{name}: {e}"));
}

fn diffuse(s: &AppState) -> Map {
    s.scene.slot_material(s.scene.material_editor.active).diffuse_map.expect("a diffuse map")
}

#[test]
fn every_map_type_comes_from_new_map_and_undoes() {
    for ty in MapType::ALL {
        let mut s = AppState::new();
        let undo = s.undo.len();
        execute(&mut s, Command::NewMap { map: "Diffuse Color".into(), kind: ty.label().into() }).unwrap_or_else(|e| panic!("{}: {e}", ty.label()));
        let m = diffuse(&s);
        assert_eq!(m.kind, ty);
        assert_eq!(m.name, "Map #1");
        assert_eq!(s.scene.material_editor.path, vec![NavStep::Map("Diffuse Color".into())]);
        assert_eq!(s.undo.len(), undo + 1, "{}: one undo step", ty.label());
        execute(&mut s, Command::UndoSceneOperation).unwrap();
        assert!(s.scene.slot_material(0).diffuse_map.is_none(), "{} undone", ty.label());
    }
    assert!(execute(&mut AppState::new(), Command::NewMap { map: "Diffuse Color".into(), kind: "Sky".into() }).is_err());
}

#[test]
fn parameters_by_label_are_clamped_and_undo() {
    let cases: &[(&str, &[(&str, f64, f64)])] = &[
        ("Swirl", &[("Color Contrast", 9.0, 4.0), ("Swirl Intensity", -3.0, -3.0), ("Twist", 30.0, 20.0), ("Constant Detail", 3.4, 3.0), ("Random Seed", 70000.0, 65535.0)]),
        ("Tiles", &[("Preset Type", 3.0, 3.0), ("Horiz. Count", 6.0, 6.0), ("% Holes", 150.0, 100.0), ("Rough", 1.5, 1.5), ("Per Row", 3.0, 3.0)]),
        ("Gradient Ramp", &[("Gradient Type", 99.0, 11.0), ("Interpolation", 2.0, 2.0), ("Noise Amount", 3.0, 1.0), ("Smooth", 0.4, 0.4)]),
        ("Cellular", &[("Variation", 55.0, 55.0), ("Spread", 0.43, 0.43), ("Chips", 1.0, 1.0), ("Mid", 0.65, 0.65), ("Iterations", 40.0, 10.0)]),
        ("Dent", &[("Size", 500.0, 500.0), ("Strength", 60.0, 60.0), ("Iterations", 0.0, 1.0)]),
        ("Marble", &[("Vein Width", 0.05, 0.05)]),
        ("Perlin Marble", &[("Levels", 20.0, 10.0), ("Color 1 Saturation", 0.0, 1.0)]),
        ("Planet", &[("Continent Size", 10.0, 10.0), ("Ocean %", 120.0, 100.0), ("Blend Water/Land", 0.0, 0.0)]),
        ("Smoke", &[("Exponent", 2.0, 2.0), ("Iterations", 30.0, 20.0), ("Phase", 1.5, 1.5)]),
        ("Speckle", &[("Size", 0.5, 0.5)]),
        ("Splat", &[("Threshold", 2.0, 1.0)]),
        ("Stucco", &[("Thickness", 0.3, 0.3), ("Threshold", -1.0, 0.0)]),
        ("Waves", &[("Num Wave Sets", 80.0, 50.0), ("Distribution 2D", 1.0, 1.0), ("Amplitude", 2.0, 2.0)]),
        ("Wood", &[("Grain Thickness", 3.0, 3.0), ("Radial Noise", 2.0, 2.0), ("Axial Noise", 0.0, 0.0)]),
        ("Noise", &[("Source", 1.0, 1.0), ("Offset Z", 5.0, 5.0), ("Tiling X", 2.0, 2.0), ("Angle Y", 90.0, 90.0), ("Invert", 1.0, 1.0), ("RGB Level", 2.0, 2.0)]),
        ("Mask", &[("Invert Mask", 1.0, 1.0)]),
        ("RGB Multiply", &[("Alpha From", 0.0, 0.0)]),
        ("Color Correction", &[("Channels", 3.0, 3.0), ("Blue", 0.0, 0.0), ("Hue Shift", 400.0, 180.0), ("Brightness", -20.0, -20.0)]),
        ("Output", &[("Output Amount", 0.5, 0.5), ("Clamp", 1.0, 1.0), ("Bump Amount", 2.0, 2.0)]),
        ("Vertex Color", &[("Map Channel", 0.0, 0.0), ("Sub Channel", 2.0, 2.0)]),
        ("Falloff", &[("Falloff Type", 2.0, 2.0), ("Falloff Direction", 8.0, 8.0), ("Index of Refraction", 0.5, 1.0), ("Side Amount", 50.0, 50.0), ("Far Distance", 300.0, 300.0)]),
        ("Mix", &[("Use Curve", 1.0, 1.0), ("Upper", 0.8, 0.8), ("Lower", -1.0, 0.0)]),
    ];
    // The stored value behind each label.
    fn stored(m: &Map, name: &str) -> f64 {
        let e = &m.extra;
        let v: f32 = match name {
            "Color Contrast" => e.color_contrast,
            "Swirl Intensity" => e.swirl_intensity,
            "Twist" => e.twist,
            "Constant Detail" => e.constant_detail,
            "Random Seed" => e.seed as f32,
            "Preset Type" => e.preset as f32,
            "Horiz. Count" => e.horiz_count,
            "% Holes" => e.holes,
            "Rough" => e.rough,
            "Per Row" => e.per_row as f32,
            "Gradient Type" => e.ramp_type as f32,
            "Interpolation" => e.interpolation as f32,
            "Noise Amount" => e.noise_amount,
            "Smooth" => e.smooth,
            "Variation" => e.variation,
            "Spread" => e.spread,
            "Chips" => e.chips as u8 as f32,
            "Mid" => e.mid,
            "Iterations" => e.iterations,
            "Size" => m.params.size,
            "Strength" => e.strength,
            "Vein Width" => e.vein_width,
            "Levels" => m.params.levels,
            "Color 1 Saturation" => e.saturation[0],
            "Continent Size" => e.continent_size,
            "Ocean %" => e.ocean,
            "Blend Water/Land" => e.blend_water_land as u8 as f32,
            "Exponent" => e.exponent,
            "Phase" => m.params.phase,
            "Threshold" => e.threshold,
            "Thickness" => e.thickness,
            "Num Wave Sets" => e.wave_sets as f32,
            "Distribution 2D" => e.distribution_2d as u8 as f32,
            "Amplitude" => e.amplitude,
            "Grain Thickness" => e.grain_thickness,
            "Radial Noise" => e.radial_noise,
            "Axial Noise" => e.axial_noise,
            "Source" => m.coords3.source as f32,
            "Offset Z" => m.coords3.offset[2],
            "Tiling X" => m.coords3.tiling[0],
            "Angle Y" => m.coords3.angle[1],
            "Invert" => m.output.invert as u8 as f32,
            "RGB Level" => m.output.rgb_level,
            "Invert Mask" => e.invert_mask as u8 as f32,
            "Alpha From" => e.alpha_from as f32,
            "Channels" => e.channels as f32,
            "Blue" => e.custom_channels[2] as f32,
            "Hue Shift" => e.hue_shift,
            "Brightness" => e.brightness,
            "Output Amount" => m.output.amount,
            "Clamp" => m.output.clamp as u8 as f32,
            "Bump Amount" => m.output.bump_amount,
            "Map Channel" => m.coords.map_channel as f32,
            "Sub Channel" => e.sub_channel as f32,
            "Falloff Type" => e.falloff_type as f32,
            "Falloff Direction" => e.falloff_direction as f32,
            "Index of Refraction" => e.ior,
            "Side Amount" => e.side_amount,
            "Far Distance" => e.far_distance,
            "Use Curve" => e.use_curve as u8 as f32,
            "Upper" => e.upper,
            "Lower" => e.lower,
            other => panic!("no field for {other}"),
        };
        v as f64
    }
    for (kind, params) in cases {
        let mut s = AppState::new();
        execute(&mut s, Command::NewMap { map: "Diffuse Color".into(), kind: (*kind).into() }).unwrap();
        let fresh = diffuse(&s);
        for (name, set, want) in params.iter() {
            param(&mut s, name, *set);
            let got = stored(&diffuse(&s), name);
            assert!((got - want).abs() < 1e-4, "{kind} {name}: set {set}, got {got}, want {want}");
        }
        for _ in 0..params.len() {
            execute(&mut s, Command::UndoSceneOperation).unwrap();
        }
        assert_eq!(diffuse(&s), fresh, "{kind}: every change undone");
        assert!(execute(&mut s, Command::SetMapParameter { name: "Sheen".into(), value: 1.0 }).is_err());
    }
}

#[test]
fn colour_components_and_their_maps() {
    let mut s = AppState::new();
    execute(&mut s, Command::NewMap { map: "Diffuse Color".into(), kind: "Planet".into() }).unwrap();
    execute(&mut s, Command::SetMapColor { which: "Color #7".into(), rgb: [255, 0, 0] }).unwrap();
    assert_eq!(diffuse(&s).color(6), glam::Vec3::X, "Planet's seventh colour");
    assert!(execute(&mut s, Command::NewMap { map: "Color #1".into(), kind: "Checker".into() }).is_err(), "Planet's colours take no maps");
    execute(&mut s, Command::SetMapType { kind: "Tiles".into(), keep_old: None }).unwrap();
    execute(&mut s, Command::SetMapColor { which: "Grout".into(), rgb: [0, 0, 255] }).unwrap();
    assert!(execute(&mut s, Command::SetMapColor { which: "Color #1".into(), rgb: [0, 0, 255] }).is_err());
    execute(&mut s, Command::NewMap { map: "Tiles".into(), kind: "Noise".into() }).unwrap();
    assert_eq!(s.scene.material_editor.path, vec![NavStep::Map("Diffuse Color".into()), NavStep::Map("Tiles".into())]);
    execute(&mut s, Command::MaterialEditorGoToParent).unwrap();
    param(&mut s, "Tiles Map On", 0.0);
    let m = diffuse(&s);
    assert_eq!((m.colors[1], m.sub_map(0).map(|x| (x.kind, x.enabled))), (glam::Vec3::Z, Some((MapType::Noise, false))));
    // Replace Map keeping the old map: RGB Tint keeps it in Map, a Gradient Ramp under Flag #1.
    execute(&mut s, Command::SetMapType { kind: "RGB Tint".into(), keep_old: Some(true) }).unwrap();
    let m = diffuse(&s);
    assert_eq!(m.sub_map(3).map(|x| x.kind), Some(MapType::Tiles));
    execute(&mut s, Command::SetMapType { kind: "Gradient Ramp".into(), keep_old: Some(true) }).unwrap();
    let m = diffuse(&s);
    assert_eq!((m.sub_map(0).map(|x| x.kind), m.sub_map(1).map(|x| x.kind)), (None, Some(MapType::RgbTint)));
}

#[test]
fn composite_layers_and_ramp_flags_through_the_editor_levels() {
    let mut s = AppState::new();
    execute(&mut s, Command::NewMap { map: "Diffuse Color".into(), kind: "Composite".into() }).unwrap();
    execute(&mut s, Command::NewMap { map: "Layer 1".into(), kind: "Checker".into() }).unwrap();
    execute(&mut s, Command::MaterialEditorGoToParent).unwrap();
    param(&mut s, "Add Layer", 1.0);
    param(&mut s, "Layer 2 Blend Mode", 5.0);
    param(&mut s, "Layer 2 Opacity", 60.0);
    execute(&mut s, Command::NewMap { map: "Mask 2".into(), kind: "Gradient Ramp".into() }).unwrap();
    param(&mut s, "Add Flag", 25.0);
    execute(&mut s, Command::SetMapColor { which: "Flag #4".into(), rgb: [255, 255, 0] }).unwrap();
    execute(&mut s, Command::NewMap { map: "Flag #4".into(), kind: "Wood".into() }).unwrap();
    let path = vec![NavStep::Map("Diffuse Color".into()), NavStep::Map("Mask 2".into()), NavStep::Map("Flag #4".into())];
    assert_eq!(s.scene.material_editor.path, path, "three levels down");
    // The Navigator reaches the same level by its names; siblings cycle.
    execute(&mut s, Command::MaterialEditorNavigate { path: vec!["Diffuse Color".into(), "Layer 1".into()] }).unwrap();
    execute(&mut s, Command::MaterialEditorGoForwardToSibling).unwrap();
    assert_eq!(s.scene.material_editor.path.last(), Some(&NavStep::Map("Mask 2".into())));
    execute(&mut s, Command::MaterialEditorGoToParent).unwrap();
    let m = diffuse(&s);
    assert_eq!(m.layers.len(), 2);
    assert_eq!((m.layers[1].blend, m.layers[1].opacity), (5, 60.0));
    let ramp = m.sub_map(3).expect("layer 2's mask");
    assert_eq!(ramp.flags.len(), 4);
    assert_eq!(ramp.sub_map(4).map(|x| x.kind), Some(MapType::Wood));
    // Deleting layer 1 moves layer 2 down with its mask.
    param(&mut s, "Delete Layer", 1.0);
    let m = diffuse(&s);
    assert_eq!((m.layers.len(), m.sub_map(0).map(|x| x.kind), m.sub_map(1).map(|x| x.kind)), (1, None, Some(MapType::GradientRamp)));
    assert!(execute(&mut s, Command::SetMapParameter { name: "Delete Layer".into(), value: 1.0 }).is_err(), "the last layer stays");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(diffuse(&s).layers.len(), 2);

    // Saved and opened: every level and parameter comes back.
    let file = std::env::temp_dir().join(format!("loom-maps2-{}.loom", std::process::id()));
    write(&file, &Document { format: FORMAT, app: "test".into(), scene: s.scene.clone(), settings: SceneSettings::from_state(&s) }).unwrap();
    let back = read(&file).unwrap().scene;
    let _ = std::fs::remove_file(&file);
    assert_eq!(back.materials, s.scene.materials);
    assert_eq!(back.material_editor, s.scene.material_editor);
}

#[test]
fn every_map_type_round_trips_through_the_loom_file() {
    let mut s = AppState::new();
    let channels = ["Ambient Color", "Diffuse Color", "Specular Color", "Specular Level", "Glossiness", "Self-Illumination", "Opacity", "Filter Color", "Bump", "Reflection", "Refraction", "Displacement"];
    let mut slot = 0;
    for (i, ty) in MapType::ALL.iter().enumerate() {
        if i % channels.len() == 0 && i > 0 {
            slot += 1;
            execute(&mut s, Command::MaterialEditorSlot { slot }).unwrap();
        }
        execute(&mut s, Command::NewMap { map: channels[i % channels.len()].into(), kind: ty.label().into() }).unwrap();
        // One non-default parameter each where the type has one.
        let _ = execute(&mut s, Command::SetMapParameter { name: "Random Seed".into(), value: 7.0 });
        let _ = execute(&mut s, Command::SetMapParameter { name: "Invert".into(), value: 1.0 });
        execute(&mut s, Command::MaterialEditorGoToParent).unwrap();
    }
    let file = std::env::temp_dir().join(format!("loom-maps2-all-{}.loom", std::process::id()));
    write(&file, &Document { format: FORMAT, app: "test".into(), scene: s.scene.clone(), settings: SceneSettings::from_state(&s) }).unwrap();
    let back = read(&file).unwrap().scene;
    let _ = std::fs::remove_file(&file);
    assert_eq!(back.materials, s.scene.materials);
    let kinds: Vec<MapType> = back.materials.values().flat_map(|m| m.map_slots().into_iter().filter_map(|c| m.map(c).map(|x| x.kind)).collect::<Vec<_>>()).collect();
    for ty in MapType::ALL {
        assert!(kinds.contains(&ty), "{} saved", ty.label());
    }
}
