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
