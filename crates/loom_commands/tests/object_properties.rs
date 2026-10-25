//! Object Properties: Rendering Control (Visibility, Renderable, Inherit
//! Visibility, Visible to Camera, Receive / Cast Shadows), User Defined text
//! and the new Display Properties, on the selection with one undo step.

use loom_commands::file::{read, write, Document, SceneSettings, FORMAT};
use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp};
use loom_scene::NodeId;

fn box_at(s: &mut AppState, x: f32) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [10.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn select(s: &mut AppState, ids: &[NodeId]) {
    execute(s, Command::SelectNodes { ids: ids.iter().map(|i| i.0).collect(), op: SelectOp::Replace }).unwrap();
}

#[test]
fn render_properties_default_on_and_apply_to_the_selection_in_one_undo_step() {
    let mut s = AppState::new();
    let a = box_at(&mut s, 0.0);
    let b = box_at(&mut s, 20.0);
    let r = &s.scene.node(a).unwrap().render;
    assert!(r.renderable && r.inherit_visibility && r.visible_to_camera && r.receive_shadows && r.cast_shadows, "the reference's defaults are all on");
    select(&mut s, &[a, b]);
    execute(&mut s, Command::SetRenderProperty { name: "Cast Shadows".into(), value: false }).unwrap();
    assert!(!s.scene.node(a).unwrap().render.cast_shadows);
    assert!(!s.scene.node(b).unwrap().render.cast_shadows);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.node(a).unwrap().render.cast_shadows, "one undo step restores both");
    assert!(s.scene.node(b).unwrap().render.cast_shadows);
    assert!(execute(&mut s, Command::SetRenderProperty { name: "Glow".into(), value: true }).is_err());
}

#[test]
fn visibility_spinner_sets_the_visibility_track_and_inherits_from_the_parent() {
    let mut s = AppState::new();
    let parent = box_at(&mut s, 0.0);
    let child = box_at(&mut s, 20.0);
    execute(&mut s, Command::SetParent { id: child.0, parent: Some(parent.0) }).unwrap();
    assert_eq!(s.scene.render_visibility(child), 1.0, "no track: fully visible");
    select(&mut s, &[parent]);
    execute(&mut s, Command::SetObjectVisibility { value: 0.5 }).unwrap();
    assert_eq!(s.scene.node(parent).unwrap().visibility.as_ref().map(|p| p.get()), Some(0.5));
    assert!((s.scene.render_visibility(child) - 0.5).abs() < 1e-6, "Inherit Visibility multiplies by the parent's");
    select(&mut s, &[child]);
    execute(&mut s, Command::SetRenderProperty { name: "Inherit Visibility".into(), value: false }).unwrap();
    assert_eq!(s.scene.render_visibility(child), 1.0);
    // Out of range values are clamped to 0..1.
    select(&mut s, &[parent]);
    execute(&mut s, Command::SetObjectVisibility { value: 3.0 }).unwrap();
    assert_eq!(s.scene.render_visibility(parent), 1.0);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!((s.scene.render_visibility(parent) - 0.5).abs() < 1e-6);
}

#[test]
fn user_defined_text_and_render_props_survive_save_and_load() {
    let mut s = AppState::new();
    let a = box_at(&mut s, 0.0);
    execute(&mut s, Command::SetUserProperties { id: a.0, text: "lod=2\ncollision=box".into() }).unwrap();
    execute(&mut s, Command::SetRenderProperty { name: "Visible to Camera".into(), value: false }).unwrap();
    execute(&mut s, Command::SetDisplayProperty { id: a.0, name: "Edges Only".into(), value: false }).unwrap();
    execute(&mut s, Command::SetDisplayProperty { id: a.0, name: "Display Links".into(), value: true }).unwrap();
    let path = std::env::temp_dir().join("loom-object-properties.loom");
    write(&path, &Document { format: FORMAT, app: "test".into(), scene: s.scene.clone(), settings: SceneSettings::from_state(&s) }).unwrap();
    let doc = read(&path).unwrap();
    let n = doc.scene.node(a).unwrap();
    assert_eq!(n.user_props, "lod=2\ncollision=box");
    assert!(!n.render.visible_to_camera);
    assert!(!n.display.edges_only);
    assert!(n.display.display_links);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.node(a).unwrap().user_props, "", "User Defined edits undo");
}

#[test]
fn new_display_properties_are_settable_by_name() {
    let mut s = AppState::new();
    let a = box_at(&mut s, 0.0);
    let d = &s.scene.node(a).unwrap().display;
    assert!(d.edges_only, "Edges Only defaults on");
    assert!(!d.trajectory && !d.vertex_colors_shaded && !d.display_links && !d.link_replaces_object);
    for name in ["Trajectory", "Vertex Color Shaded", "Link Replaces Object"] {
        execute(&mut s, Command::SetDisplayProperty { id: a.0, name: name.into(), value: true }).unwrap();
        assert_eq!(s.scene.node(a).unwrap().display.get(name), Some(true), "{name}");
    }
}
