//! UVW Map / UVW XForm modifiers and the viewport texture display.

use glam::Vec3;
use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp};
use loom_scene::{Modifier, NodeId, TextureDisplay};

fn boxed(s: &mut AppState) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0, 20.0, 30.0] }).unwrap();
    *s.selection.iter().next().unwrap()
}

/// Bounds of channel `ch` over every corner of the node's stack result.
fn uv_bounds(s: &AppState, id: NodeId, ch: u32) -> (Vec3, Vec3) {
    let m = s.scene.eval_mesh(id).value.mesh.clone();
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for i in 0..m.half_edge_count() {
        if m.half_edges()[i].face.is_none() {
            continue;
        }
        let uv = m.corner_uv(ch, loom_mesh::HalfEdgeId(i as u32)).unwrap();
        lo = lo.min(uv);
        hi = hi.max(uv);
    }
    (lo, hi)
}

#[test]
fn uvw_map_planar_on_box_fits_bounds() {
    let mut s = AppState::new();
    let a = boxed(&mut s);
    execute(&mut s, Command::AddModifier { name: "UVW Map".into() }).unwrap();
    let Modifier::UVWMap { length, width, height, projection, channel, .. } = &s.scene.node(a).unwrap().modifiers[0].modifier else { panic!("not a UVW Map") };
    assert_eq!(projection.get(), 0, "Planar by default");
    assert_eq!(channel.get(), 1);
    // Fit on add: the gizmo matches the object's bounds.
    assert!((width.get() - 10.0).abs() < 1e-4 && (length.get() - 20.0).abs() < 1e-4 && (height.get() - 30.0).abs() < 1e-4, "{} {} {}", width.get(), length.get(), height.get());
    let (lo, hi) = uv_bounds(&s, a, 1);
    assert!(lo.x.abs() < 1e-4 && lo.y.abs() < 1e-4 && (hi.x - 1.0).abs() < 1e-4 && (hi.y - 1.0).abs() < 1e-4, "{lo:?} {hi:?}");
    // Mapping radios and tiling are rollout parameters.
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Mapping".into(), value: 4.0 }).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "U Tile".into(), value: 3.0 }).unwrap();
    let (_, hi) = uv_bounds(&s, a, 1);
    assert!((hi.x - 3.0).abs() < 1e-4, "{hi:?}");
    let m = &s.scene.node(a).unwrap().modifiers[0].modifier;
    assert_eq!(m.sub_levels(), &["Gizmo"]);
    assert!(m.parameters().iter().any(|(n, _)| *n == "Mapping") && m.choices("Mapping").unwrap().len() == 7);
}

#[test]
fn uvw_map_fit_center_reset_are_undoable() {
    let mut s = AppState::new();
    let a = boxed(&mut s);
    execute(&mut s, Command::AddModifier { name: "UVW Map".into() }).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Length".into(), value: 5.0 }).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Width".into(), value: 5.0 }).unwrap();
    let (_, hi) = uv_bounds(&s, a, 1);
    assert!((hi.x - 1.5).abs() < 1e-3, "a 5-wide map over a 10-wide box: {hi:?}");
    execute(&mut s, Command::UvwMapFit).unwrap();
    let (_, hi) = uv_bounds(&s, a, 1);
    assert!((hi.x - 1.0).abs() < 1e-4, "{hi:?}");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    let (_, hi) = uv_bounds(&s, a, 1);
    assert!((hi.x - 1.5).abs() < 1e-3, "undo restores the 5-wide gizmo: {hi:?}");
    // Move the gizmo, Center puts it back on the object's centre.
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [5.0, 0.0, 0.0], system: None }).unwrap();
    let (lo, _) = uv_bounds(&s, a, 1);
    assert!(lo.x < -0.4, "shifted: {lo:?}");
    execute(&mut s, Command::UvwMapCenter).unwrap();
    let (lo, _) = uv_bounds(&s, a, 1);
    assert!(lo.x > -0.6 && lo.x < -0.4, "centred on the 5-wide gizmo: {lo:?}");
    execute(&mut s, Command::UvwMapReset).unwrap();
    let (lo, hi) = uv_bounds(&s, a, 1);
    assert!(lo.x.abs() < 1e-4 && (hi.x - 1.0).abs() < 1e-4, "reset fits again: {lo:?} {hi:?}");
    // Alignment radios: Z (default), X, Y change which object axis the map faces.
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Alignment".into(), value: 0.0 }).unwrap();
    let (lo, hi) = uv_bounds(&s, a, 1);
    assert!(lo.x.abs() < 1e-3 && (hi.x - 1.0).abs() < 1e-3, "X alignment refits: {lo:?} {hi:?}");
    execute(&mut s, Command::UvwMapViewAlign).unwrap();
    execute(&mut s, Command::UvwMapNormalAlign).unwrap();
    assert_eq!(s.pick_mode, Some(loom_commands::PickMode::UvwMapNormal));
}

#[test]
fn uvw_map_channel_0_writes_vertex_colors() {
    let mut s = AppState::new();
    let a = boxed(&mut s);
    execute(&mut s, Command::AddModifier { name: "UVW Map".into() }).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Channel".into(), value: 0.0 }).unwrap();
    let m = s.scene.eval_mesh(a).value.mesh.clone();
    let colors: Vec<[f32; 3]> = m.vertices().iter().map(|v| v.color).collect();
    assert!(colors.iter().any(|c| c[0] < 0.1) && colors.iter().any(|c| c[0] > 0.9), "u maps to red: {colors:?}");
    assert!(m.map(1).is_some(), "channel 1 untouched");
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Channel".into(), value: 3.0 }).unwrap();
    let m = s.scene.eval_mesh(a).value.mesh.clone();
    assert!(m.map(3).is_some());
}

#[test]
fn uvw_xform_tiles_and_offsets() {
    let mut s = AppState::new();
    let a = boxed(&mut s);
    execute(&mut s, Command::AddModifier { name: "UVW XForm".into() }).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "U Tile".into(), value: 2.0 }).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "V Offset".into(), value: 0.5 }).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "W Flip".into(), value: 1.0 }).unwrap();
    let (lo, hi) = uv_bounds(&s, a, 1);
    assert!((hi.x - 2.0).abs() < 1e-4 && (lo.y - 0.5).abs() < 1e-4 && (hi.y - 1.5).abs() < 1e-4, "{lo:?} {hi:?}");
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Rotation".into(), value: 90.0 }).unwrap();
    let (lo, hi) = uv_bounds(&s, a, 1);
    assert!(hi.x - lo.x < 1.01 && hi.y - lo.y > 1.99, "rotated 90°: {lo:?} {hi:?}");
}

#[test]
fn texture_display_is_set_by_command_and_saved() {
    let mut s = AppState::new();
    let a = boxed(&mut s);
    assert_eq!(s.scene.node(a).unwrap().display.texture, TextureDisplay::None);
    execute(&mut s, Command::SetTextureDisplay { id: a.0, kind: "checker".into(), path: None }).unwrap();
    assert_eq!(s.scene.node(a).unwrap().display.texture, TextureDisplay::Checker);
    assert!(execute(&mut s, Command::SetTextureDisplay { id: a.0, kind: "bitmap".into(), path: None }).is_err(), "a bitmap needs a path");
    execute(&mut s, Command::SetTextureDisplay { id: a.0, kind: "bitmap".into(), path: Some("/tmp/x.png".into()) }).unwrap();
    assert_eq!(s.scene.node(a).unwrap().display.texture, TextureDisplay::Bitmap("/tmp/x.png".into()));
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.node(a).unwrap().display.texture, TextureDisplay::Checker);
    let d = std::env::temp_dir().join("loom-uvw-tests");
    std::fs::create_dir_all(&d).unwrap();
    let path = d.join("tex.loom");
    execute(&mut s, Command::SaveFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let doc = loom_commands::file::read(&path).unwrap();
    assert_eq!(doc.scene.node(a).unwrap().display.texture, TextureDisplay::Checker);
    // Chart rows add the modifiers.
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::UvwMapModifier).unwrap();
    assert!(matches!(s.scene.node(a).unwrap().modifiers.last().unwrap().modifier, Modifier::UVWMap { .. }));
}
