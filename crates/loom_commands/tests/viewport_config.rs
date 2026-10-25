//! Viewport Configuration: Rendering Method with Apply To, the rendering
//! levels, Transparency and options, Adaptive Degradation, Lighting and
//! Shadows, Regions, Safe Frames Lock / Region / Default Settings, and Save
//! as Default.

use loom_commands::{execute, AppState, Command, Shading, Transparency};

fn rendering(s: &mut AppState, apply_to: &str, shading: Option<&str>, transparency: Option<&str>) {
    execute(s, Command::SetViewportRendering { apply_to: Some(apply_to.into()), shading: shading.map(str::to_owned), edged_faces: None, transparency: transparency.map(str::to_owned), disable_textures: None, force_two_sided: None }).unwrap();
}

#[test]
fn apply_to_picks_the_active_all_or_all_but_the_active_viewport() {
    let mut s = AppState::new();
    assert_eq!(s.viewports.len(), 4);
    s.active_viewport = 3;
    rendering(&mut s, "Active Only", Some("Facets"), None);
    assert_eq!(s.viewports.iter().map(|v| v.shading).collect::<Vec<_>>(), [Shading::Wireframe, Shading::Wireframe, Shading::Wireframe, Shading::Facets]);
    rendering(&mut s, "All But Active", Some("Bounding Box"), None);
    assert_eq!(s.viewports.iter().map(|v| v.shading).collect::<Vec<_>>(), [Shading::BoundingBox, Shading::BoundingBox, Shading::BoundingBox, Shading::Facets]);
    rendering(&mut s, "All Viewports", Some("Smooth"), Some("Simple"));
    assert!(s.viewports.iter().all(|v| v.shading == Shading::Smooth && v.transparency == Transparency::Simple));
    assert!(execute(&mut s, Command::SetViewportRendering { apply_to: Some("Some".into()), shading: None, edged_faces: None, transparency: None, disable_textures: None, force_two_sided: None }).is_err());
    assert!(execute(&mut s, Command::SetViewportRendering { apply_to: None, shading: Some("Toon".into()), edged_faces: None, transparency: None, disable_textures: None, force_two_sided: None }).is_err());
    execute(&mut s, Command::SetViewportRendering { apply_to: None, shading: Some("Facets + Highlights".into()), edged_faces: Some(true), transparency: Some("None".into()), disable_textures: Some(true), force_two_sided: Some(true) }).unwrap();
    let v = s.active();
    assert_eq!((v.shading, v.edged_faces, v.transparency, v.disable_textures, v.force_two_sided), (Shading::FacetsHighlights, true, Transparency::None, true, true));
}

#[test]
fn the_rendering_options_and_degradation_are_saved_with_the_scene() {
    let dir = std::env::temp_dir().join("loom-viewport-config-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("v.loom").to_string_lossy().into_owned();
    let mut s = AppState::new();
    rendering(&mut s, "All Viewports", Some("Facets"), Some("None"));
    execute(&mut s, Command::AdaptiveDegradationToggle).unwrap();
    execute(&mut s, Command::SetAdaptiveDegradation { general: Some("Wireframe".into()), active: Some("Facets".into()), maintain_fps: Some(24.0), never_degrade_selected: Some(true), degrade_to_default_lighting: Some(false) }).unwrap();
    assert!(execute(&mut s, Command::SetAdaptiveDegradation { general: Some("Fast".into()), active: None, maintain_fps: None, never_degrade_selected: None, degrade_to_default_lighting: None }).is_err());
    execute(&mut s, Command::SaveFileAs { path: Some(path.clone()) }).unwrap();
    let mut t = AppState::new();
    execute(&mut t, Command::OpenFile { path: Some(path) }).unwrap();
    assert!(t.viewports.iter().all(|v| v.shading == Shading::Facets && v.transparency == Transparency::None));
    assert_eq!(t.degradation, s.degradation);
    assert!(t.degradation.on && t.degradation.never_degrade_selected && t.degradation.maintain_fps == 24.0);
}

#[test]
fn hardware_shading_turns_the_views_realistic_and_back() {
    let mut s = AppState::new();
    execute(&mut s, Command::SetViewportLighting { apply_to: Some("All Viewports".into()), scene_lights: Some(false), default_lights: Some(2), hardware: Some(true) }).unwrap();
    assert!(s.viewports.iter().all(|v| v.shading == Shading::Realistic && !v.scene_lights && v.default_lights == 2));
    execute(&mut s, Command::SetViewportLighting { apply_to: None, scene_lights: None, default_lights: Some(7), hardware: Some(false) }).unwrap();
    assert_eq!(s.active().shading, Shading::SmoothHighlights);
    assert_eq!(s.active().default_lights, 2, "1 or 2 default lights");
}

#[test]
fn regions_set_the_sub_region_and_the_blowup_region() {
    let mut s = AppState::new();
    execute(&mut s, Command::SetRenderRegions { region: Some([0.5, 0.1, 0.2, 0.9]), blowup: Some([0.0, 0.0, 2.0, 0.5]) }).unwrap();
    assert_eq!(s.scene.render.region, [0.2, 0.1, 0.5, 0.9], "corners in order");
    assert_eq!(s.scene.render.blowup, [0.0, 0.0, 1.0, 0.5], "inside the frame");
}

#[test]
fn safe_frames_lock_region_and_default_settings() {
    let mut s = AppState::new();
    let set = |s: &mut AppState, action: Option<[f32; 2]>, lock: Option<Vec<bool>>, region: Option<bool>, default_settings: Option<bool>| {
        execute(s, Command::SetSafeFrames { live_area: None, action_safe: None, title_safe: None, user_safe: None, grid: None, action, title: None, user: None, grid_4x3: None, lock, region, default_settings }).unwrap();
    };
    set(&mut s, None, Some(vec![true, false, false]), Some(true), None);
    set(&mut s, Some([12.0, 30.0]), None, None, None);
    assert_eq!(s.display.safe_frames.action, [12.0, 12.0], "locked: one value for both");
    assert!(s.display.safe_frames.region);
    set(&mut s, None, None, None, Some(true));
    assert_eq!(s.display.safe_frames, loom_commands::display::SafeFrames::default());
}

#[test]
fn save_as_default_is_what_reset_scenes_start_with() {
    let dir = std::env::temp_dir().join("loom-viewport-defaults-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("LOOM_CONFIG_DIR", &dir);
    let mut s = AppState::new();
    execute(&mut s, Command::ViewportLayout { layout: loom_commands::ViewportLayout::ALL[0] }).unwrap();
    rendering(&mut s, "All Viewports", Some("Facets"), None);
    execute(&mut s, Command::SaveViewportDefaults).unwrap();
    let mut t = AppState::new();
    t.use_viewport_defaults = true;
    execute(&mut t, Command::ResetFile).unwrap();
    assert_eq!(t.layout, s.layout);
    assert!(t.viewports.iter().all(|v| v.shading == Shading::Facets));
    // Without the app's flag (tests, scripts) a reset keeps the program's views.
    let mut u = AppState::new();
    execute(&mut u, Command::ResetFile).unwrap();
    let shadings = |s: &AppState| s.viewports.iter().map(|v| v.shading).collect::<Vec<_>>();
    assert_eq!(shadings(&u), shadings(&AppState::new()));
}
