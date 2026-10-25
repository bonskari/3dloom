//! Display panel settings that are not per object, the Object Color dialog
//! commands and the statistics / safe frame options, as the reference has them.

use loom_commands::display::{ColorSource, StatisticsScope};
use loom_commands::file::{read, write, Document, SceneSettings, FORMAT};
use loom_commands::{execute, AppState, Command, PrimitiveKind};
use loom_scene::NodeId;

fn box_at(s: &mut AppState, x: f32) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [10.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

#[test]
fn display_color_defaults_to_object_wire_and_material_shading() {
    let mut s = AppState::new();
    assert_eq!(s.display.wireframe_color, ColorSource::Object);
    assert_eq!(s.display.shaded_color, ColorSource::Material);
    execute(&mut s, Command::SetDisplayColor { wireframe: Some("Material Color".into()), shaded: Some("object".into()) }).unwrap();
    assert_eq!(s.display.wireframe_color, ColorSource::Material);
    assert_eq!(s.display.shaded_color, ColorSource::Object);
    assert!(execute(&mut s, Command::SetDisplayColor { wireframe: Some("Layer".into()), shaded: None }).is_err());
}

#[test]
fn hide_frozen_objects_and_bone_objects_leave_the_viewports() {
    let mut s = AppState::new();
    let a = box_at(&mut s, 0.0);
    execute(&mut s, Command::FreezeNodes { ids: vec![a.0] }).unwrap();
    assert!(s.is_visible(s.scene.node(a).unwrap()));
    execute(&mut s, Command::HideFrozenObjectsToggle).unwrap();
    assert!(!s.is_visible(s.scene.node(a).unwrap()), "Hide Frozen Objects hides the frozen box");
    execute(&mut s, Command::UnfreezeNodes { ids: vec![a.0] }).unwrap();
    assert!(s.is_visible(s.scene.node(a).unwrap()), "unfrozen, it shows again");
    execute(&mut s, Command::CreateBoneChain { points: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 10.0]], name: None }).unwrap();
    let bone = s.scene.nodes().find(|n| n.object.is_bone()).unwrap().id;
    execute(&mut s, Command::HideBoneObjectsToggle).unwrap();
    assert!(!s.is_visible(s.scene.node(bone).unwrap()), "Bone Objects category hides bones");
    assert!(s.is_visible(s.scene.node(a).unwrap()));
}

#[test]
fn object_color_dialog_colours_the_selection_and_sets_the_colour_for_new_objects() {
    let mut s = AppState::new();
    let a = box_at(&mut s, 0.0);
    let b = box_at(&mut s, 20.0);
    execute(&mut s, Command::SelectNodes { ids: vec![a.0, b.0], op: loom_commands::SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SetObjectColor { rgb: [10, 20, 30] }).unwrap();
    assert_eq!(s.scene.node(a).unwrap().wire_color, [10, 20, 30]);
    assert_eq!(s.scene.node(b).unwrap().wire_color, [10, 20, 30]);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_ne!(s.scene.node(a).unwrap().wire_color, [10, 20, 30], "one undo step for the whole selection");
    assert_ne!(s.scene.node(b).unwrap().wire_color, [10, 20, 30]);
    // Assign Random Colors off: new objects take the current colour.
    execute(&mut s, Command::SetAssignRandomColors { on: false }).unwrap();
    let c = box_at(&mut s, 40.0);
    let d = box_at(&mut s, 60.0);
    assert_eq!(s.scene.node(c).unwrap().wire_color, [10, 20, 30]);
    assert_eq!(s.scene.node(d).unwrap().wire_color, [10, 20, 30]);
    execute(&mut s, Command::SetAssignRandomColors { on: true }).unwrap();
    let e = box_at(&mut s, 80.0);
    let f = box_at(&mut s, 100.0);
    assert_ne!(s.scene.node(e).unwrap().wire_color, s.scene.node(f).unwrap().wire_color, "random colours differ by object");
    // Select By Color picks the objects of that colour.
    let r = execute(&mut s, Command::SelectByColor { rgb: [10, 20, 30] }).unwrap();
    assert_eq!(r["selected"].as_array().unwrap().len(), 2);
    assert!(s.selection.contains(&c) && s.selection.contains(&d));
    execute(&mut s, Command::SetCustomColor { index: 3, rgb: [1, 2, 3] }).unwrap();
    assert_eq!(s.display.custom_colors[3], [1, 2, 3]);
    assert!(execute(&mut s, Command::SetCustomColor { index: 16, rgb: [1, 2, 3] }).is_err());
}

#[test]
fn statistics_and_safe_frame_options_are_kept_in_the_scene_file() {
    let mut s = AppState::new();
    execute(&mut s, Command::SetStatistics { polygons: Some(false), triangles: None, edges: None, vertices: None, fps: Some(false), scope: Some("Total + Selection".into()), default_settings: None }).unwrap();
    execute(&mut s, Command::SetSafeFrames { live_area: None, action_safe: None, title_safe: None, user_safe: Some(true), grid: Some(true), action: Some([5.0, 6.0]), title: None, user: Some([120.0, 30.0]), grid_4x3: Some(false) }).unwrap();
    assert_eq!(s.display.safe_frames.user, [99.0, 30.0], "a frame keeps at least 1 % of the live area");
    execute(&mut s, Command::DisplaySelectedEdgedFacesToggle).unwrap();
    let path = std::env::temp_dir().join("loom-display-settings.loom");
    write(&path, &Document { format: FORMAT, app: "test".into(), scene: s.scene.clone(), settings: SceneSettings::from_state(&s) }).unwrap();
    let mut t = AppState::new();
    read(&path).unwrap().settings.apply_to(&mut t);
    assert!(!t.display.statistics.polygons && !t.display.statistics.fps && t.display.statistics.vertices);
    assert_eq!(t.display.statistics.scope, StatisticsScope::TotalAndSelection);
    assert_eq!(t.display.safe_frames.action, [5.0, 6.0]);
    assert!(t.display.safe_frames.user_safe && t.display.safe_frames.grid && !t.display.safe_frames.grid_4x3);
    assert!(t.display.display_selected_edged);
    execute(&mut t, Command::SetStatistics { polygons: None, triangles: None, edges: None, vertices: None, fps: None, scope: None, default_settings: Some(true) }).unwrap();
    assert!(t.display.statistics.polygons && t.display.statistics.fps);
    assert_eq!(t.display.statistics.scope, StatisticsScope::Total);
}
