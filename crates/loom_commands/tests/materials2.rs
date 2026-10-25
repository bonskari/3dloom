//! Standard material shaders (Shader Basic Parameters drop-down and each
//! shader's parameters), Extended Parameters, the other material types,
//! the editor's workflow commands and material libraries.

use loom_commands::{execute, AppState, Command};
use loom_scene::material::{surfaces, Shader, Surfaces};

fn set(s: &mut AppState, name: &str, value: f64) {
    execute(s, Command::SetMaterialParameter { slot: None, name: name.into(), value }).unwrap();
}

fn top_surface(s: &AppState) -> loom_scene::material::Surface {
    let id = s.scene.material_editor.slots[s.scene.material_editor.active].unwrap();
    match surfaces(&s.scene.materials, id).unwrap() {
        Surfaces::Single(x) => *x,
        Surfaces::PerId(_) => panic!("single"),
    }
}

#[test]
fn the_shader_drop_down_picks_each_shader_and_undo_restores_blinn() {
    let mut s = AppState::new();
    assert_eq!(s.scene.slot_material(0).shading.shader, Shader::Blinn, "a new material is Blinn");
    for (label, shader) in [
        ("Anisotropic", Shader::Anisotropic),
        ("Metal", Shader::Metal),
        ("Multi-Layer", Shader::MultiLayer),
        ("Oren-Nayar-Blinn", Shader::OrenNayarBlinn),
        ("Phong", Shader::Phong),
        ("Strauss", Shader::Strauss),
        ("Translucent Shader", Shader::Translucent),
        ("Blinn", Shader::Blinn),
    ] {
        execute(&mut s, Command::SetShader { shader: label.into() }).unwrap();
        assert_eq!(s.scene.slot_material(0).shading.shader, shader, "{label}");
    }
    execute(&mut s, Command::SetShader { shader: "Metal".into() }).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.slot_material(0).shading.shader, Shader::Blinn, "one undo step");
    assert!(execute(&mut s, Command::SetShader { shader: "Lambert".into() }).is_err());
    // A compound material has no shader.
    execute(&mut s, Command::SetMaterialType { kind: "Blend".into(), keep_old: Some(false) }).unwrap();
    assert!(execute(&mut s, Command::SetShader { shader: "Metal".into() }).is_err());
}

#[test]
fn shader_parameters_are_stored_clamped_and_reach_the_surface() {
    let mut s = AppState::new();
    execute(&mut s, Command::SetShader { shader: "Oren-Nayar-Blinn".into() }).unwrap();
    set(&mut s, "Diffuse Level", 150.0);
    set(&mut s, "Roughness", 500.0);
    let m = s.scene.slot_material(0);
    assert_eq!((m.shading.diffuse_level, m.shading.roughness), (150.0, 100.0), "Diffuse Level 0-400, Roughness 0-100");
    set(&mut s, "Diffuse Level", 900.0);
    assert_eq!(s.scene.slot_material(0).shading.diffuse_level, 400.0);

    execute(&mut s, Command::SetShader { shader: "Anisotropic".into() }).unwrap();
    set(&mut s, "Anisotropy", 80.0);
    set(&mut s, "Orientation", 45.0);
    set(&mut s, "Orientation", 20000.0);
    let m = s.scene.slot_material(0);
    assert_eq!((m.shading.anisotropy, m.shading.orientation), (80.0, 9999.0), "Orientation 0-9999");

    execute(&mut s, Command::SetShader { shader: "Strauss".into() }).unwrap();
    set(&mut s, "Metalness", 70.0);
    execute(&mut s, Command::SetShader { shader: "Multi-Layer".into() }).unwrap();
    set(&mut s, "Level 2", 40.0);
    set(&mut s, "Glossiness 2", 60.0);
    set(&mut s, "Anisotropy 2", 30.0);
    set(&mut s, "Orientation 2", 90.0);
    execute(&mut s, Command::SetMaterialColor { slot: None, which: "Specular 2".into(), rgb: [255, 0, 0] }).unwrap();
    execute(&mut s, Command::SetShader { shader: "Translucent Shader".into() }).unwrap();
    execute(&mut s, Command::SetMaterialColor { slot: None, which: "Translucent Color".into(), rgb: [0, 255, 0] }).unwrap();
    set(&mut s, "Backside Specular", 0.0);
    let m = s.scene.slot_material(0);
    let sh = &m.shading;
    assert_eq!(sh.metalness, 70.0);
    assert_eq!((sh.level2, sh.glossiness2, sh.anisotropy2, sh.orientation2), (40.0, 60.0, 30.0, 90.0));
    assert_eq!(sh.specular2, glam::Vec3::X);
    assert_eq!(sh.translucent_color, glam::Vec3::Y);
    assert!(!sh.backside_specular);
    let surf = top_surface(&s);
    assert_eq!(surf.shading, m.shading, "the resolved surface carries the shader");
}

#[test]
fn self_illumination_colour_and_the_diffuse_specular_lock() {
    let mut s = AppState::new();
    set(&mut s, "Self-Illum Color On", 1.0);
    execute(&mut s, Command::SetMaterialColor { slot: None, which: "Self-Illum Color".into(), rgb: [255, 128, 0] }).unwrap();
    let m = s.scene.slot_material(0);
    assert!(m.shading.self_illum_color_on);
    let surf = top_surface(&s);
    assert!((surf.self_illum_color - glam::Vec3::new(1.0, 128.0 / 255.0, 0.0)).length() < 1e-5, "the colour glows: {:?}", surf.self_illum_color);
    // Diffuse/Specular lock: Specular follows Diffuse.
    set(&mut s, "Diffuse/Specular Lock", 1.0);
    execute(&mut s, Command::SetMaterialColor { slot: None, which: "Diffuse".into(), rgb: [10, 20, 30] }).unwrap();
    let m = s.scene.slot_material(0);
    assert_eq!(m.specular.get(), m.diffuse.get());
    execute(&mut s, Command::SetMaterialColor { slot: None, which: "Specular".into(), rgb: [200, 0, 0] }).unwrap();
    let m = s.scene.slot_material(0);
    assert_eq!(m.diffuse.get(), m.specular.get(), "both ways");
}

#[test]
fn extended_parameters() {
    let mut s = AppState::new();
    set(&mut s, "Index of Refraction", 2.418);
    set(&mut s, "Falloff Out", 1.0);
    set(&mut s, "Falloff Amount", 60.0);
    set(&mut s, "Transparency Type", 2.0);
    set(&mut s, "Wire Size", 3.0);
    set(&mut s, "Wire Units", 1.0);
    set(&mut s, "Reflection Dimming", 1.0);
    set(&mut s, "Dim Level", 0.5);
    set(&mut s, "Refl. Level", 2.0);
    execute(&mut s, Command::SetMaterialColor { slot: None, which: "Filter Color".into(), rgb: [255, 0, 0] }).unwrap();
    let e = s.scene.slot_material(0).extended;
    assert!((e.ior - 2.418).abs() < 1e-5);
    assert!(e.falloff_out);
    assert_eq!((e.falloff_amount, e.transparency_type, e.wire_size, e.wire_units), (60.0, 2, 3.0, true));
    assert_eq!((e.reflection_dimming, e.dim_level, e.refl_level), (true, 0.5, 2.0));
    assert_eq!(e.filter_color, glam::Vec3::X);
    set(&mut s, "Index of Refraction", 50.0);
    assert_eq!(s.scene.slot_material(0).extended.ior, 10.0, "IOR 0.01-10");
    assert_eq!(top_surface(&s).extended, s.scene.slot_material(0).extended);
}

fn kind(s: &AppState) -> loom_scene::material::MaterialKind {
    let id = s.scene.material_editor.slots[s.scene.material_editor.active].unwrap();
    s.scene.materials[&id].kind.clone()
}

#[test]
fn every_material_type_can_be_chosen_with_its_sub_materials() {
    use loom_scene::material::MaterialKind as K;
    let mut s = AppState::new();
    for t in loom_scene::material::MaterialKind::TYPES {
        execute(&mut s, Command::SetMaterialType { kind: t.into(), keep_old: Some(false) }).unwrap_or_else(|e| panic!("{t}: {e}"));
        assert_eq!(kind(&s).label(), t);
        // Back to Standard before the next one (Shell keeps its own children).
        execute(&mut s, Command::SetMaterialType { kind: "Standard".into(), keep_old: None }).unwrap();
    }
    execute(&mut s, Command::SetMaterialType { kind: "Raytrace".into(), keep_old: None }).unwrap();
    let m = s.scene.slot_material(0);
    assert_eq!((m.shading.shader, m.specular_level.get(), m.glossiness.get()), (Shader::Phong, 50.0, 40.0), "a new Raytrace material: Phong, 50, 40");
    execute(&mut s, Command::SetMaterialColor { slot: None, which: "Reflect".into(), rgb: [255, 255, 255] }).unwrap();
    set(&mut s, "Index of Refr.", 2.0);
    let K::Raytrace(r) = kind(&s) else { panic!() };
    assert_eq!((r.reflect, r.ior), (glam::Vec3::ONE, 2.0));
    assert!(execute(&mut s, Command::SetShader { shader: "Strauss".into() }).is_err(), "Raytrace has no Strauss shading");
    execute(&mut s, Command::SetShader { shader: "Metal".into() }).unwrap();

    // Double Sided keeps the old material as the facing one.
    execute(&mut s, Command::SetMaterialType { kind: "Standard".into(), keep_old: None }).unwrap();
    execute(&mut s, Command::SetMaterialColor { slot: None, which: "Diffuse".into(), rgb: [255, 0, 0] }).unwrap();
    execute(&mut s, Command::SetMaterialType { kind: "Double Sided".into(), keep_old: Some(true) }).unwrap();
    let K::DoubleSided(d) = kind(&s) else { panic!() };
    assert_eq!(s.scene.materials[&d.materials[0].unwrap()].diffuse.get(), glam::Vec3::X);
    set(&mut s, "Translucency", 30.0);
    set(&mut s, "Back Material On", 0.0);
    let K::DoubleSided(d) = kind(&s) else { panic!() };
    assert_eq!((d.translucency, d.enabled), (30.0, [true, false]));

    // Top/Bottom: Swap, Local, Blend, Position.
    execute(&mut s, Command::SetMaterialType { kind: "Top/Bottom".into(), keep_old: Some(false) }).unwrap();
    let K::TopBottom(t0) = kind(&s) else { panic!() };
    set(&mut s, "Swap", 1.0);
    set(&mut s, "Local", 1.0);
    set(&mut s, "Blend", 20.0);
    set(&mut s, "Position", 70.0);
    let K::TopBottom(t) = kind(&s) else { panic!() };
    assert_eq!(t.materials, [t0.materials[1], t0.materials[0]]);
    assert!(t.local && t.blend == 20.0 && t.position == 70.0);

    // Composite: Base + 9; an empty Mat button makes a new material.
    execute(&mut s, Command::SetMaterialType { kind: "Composite".into(), keep_old: Some(false) }).unwrap();
    let K::Composite(c) = kind(&s) else { panic!() };
    assert_eq!((c.layers.len(), c.layers[2].material), (9, None));
    execute(&mut s, Command::MaterialEditorGoToSub { index: 3 }).unwrap();
    execute(&mut s, Command::MaterialEditorGoToParent).unwrap();
    set(&mut s, "Mat 3 Mode", 2.0);
    set(&mut s, "Mat 3 Amount", 40.0);
    let K::Composite(c) = kind(&s) else { panic!() };
    assert!(c.layers[2].material.is_some());
    assert_eq!((c.layers[2].mode, c.layers[2].amount), (2, 40.0));

    // Shellac, Matte/Shadow, Ink 'n Paint, Architectural.
    execute(&mut s, Command::SetMaterialType { kind: "Shellac".into(), keep_old: Some(false) }).unwrap();
    set(&mut s, "Shellac Color Blend", 60.0);
    let K::Shellac(sh) = kind(&s) else { panic!() };
    assert_eq!(sh.blend, 60.0);
    execute(&mut s, Command::SetMaterialType { kind: "Matte/Shadow".into(), keep_old: None }).unwrap();
    set(&mut s, "Receive Shadows", 0.0);
    execute(&mut s, Command::SetMaterialColor { slot: None, which: "Color".into(), rgb: [0, 0, 255] }).unwrap();
    let K::MatteShadow(ms) = kind(&s) else { panic!() };
    assert!(!ms.receive_shadows && ms.color == glam::Vec3::Z);
    execute(&mut s, Command::SetMaterialType { kind: "Ink 'n Paint".into(), keep_old: None }).unwrap();
    set(&mut s, "Paint Levels", 5.0);
    execute(&mut s, Command::SetMaterialColor { slot: None, which: "Ink Color".into(), rgb: [255, 0, 0] }).unwrap();
    let K::InkPaint(ink) = kind(&s) else { panic!() };
    assert_eq!((ink.paint_levels, ink.ink_color), (5, glam::Vec3::X));
    execute(&mut s, Command::SetMaterialType { kind: "Architectural".into(), keep_old: None }).unwrap();
    set(&mut s, "Template", 3.0);
    let K::Architectural(a) = kind(&s) else { panic!() };
    assert_eq!((a.template, a.transparency), (3, 95.0), "Glass - Clear sets its values");
    set(&mut s, "Shininess", 20.0);
    let K::Architectural(a) = kind(&s) else { panic!() };
    assert_eq!(a.shininess, 20.0);
    // One undo step per change.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    let K::Architectural(a) = kind(&s) else { panic!() };
    assert_eq!(a.shininess, 100.0);
}

#[test]
fn a_dragged_copy_of_a_composite_copies_its_layers() {
    let mut s = AppState::new();
    execute(&mut s, Command::SetMaterialType { kind: "Composite".into(), keep_old: Some(false) }).unwrap();
    execute(&mut s, Command::MaterialEditorGoToSub { index: 1 }).unwrap();
    execute(&mut s, Command::MaterialEditorGoToParent).unwrap();
    execute(&mut s, Command::CopyMaterialToSlot { from: 0, to: 1 }).unwrap();
    let a = s.scene.slot_material(0);
    let b = s.scene.slot_material(1);
    assert_ne!(a.children()[1], b.children()[1], "the copy's layer is a material of its own");
    assert!(b.children()[1].is_some());
}

#[test]
fn shader_and_extended_values_survive_a_loom_file() {
    use loom_commands::file::{read, write, Document, SceneSettings, FORMAT};
    let mut s = AppState::new();
    execute(&mut s, Command::SetShader { shader: "Strauss".into() }).unwrap();
    set(&mut s, "Metalness", 55.0);
    set(&mut s, "Index of Refraction", 1.33);
    let doc = Document { format: FORMAT, app: "test".into(), scene: s.scene.clone(), settings: SceneSettings::from_state(&s) };
    let dir = std::env::temp_dir().join(format!("loom-shader-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("shader.loom");
    write(&p, &doc).unwrap();
    let back = read(&p).unwrap();
    let m = back.scene.slot_material(0);
    assert_eq!((m.shading.shader, m.shading.metalness), (Shader::Strauss, 55.0));
    assert!((m.extended.ior - 1.33).abs() < 1e-5);
}
