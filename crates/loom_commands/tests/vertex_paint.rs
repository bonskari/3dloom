//! VertexPaint: face selection that restricts painting, Paint All, the
//! eyedropper, layers (New Layer, layer Opacity, blend Mode).

use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp};
use loom_mesh::FaceId;
use loom_scene::{Modifier, NodeId};

/// A 10-unit box with a VertexPaint modifier painting `rgb`.
fn painted_box(s: &mut AppState, rgb: [f64; 3]) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    execute(s, Command::AddModifier { name: "VertexPaint".into() }).unwrap();
    colour(s, rgb);
    id
}

/// The active VertexPaint entry's colour swatch.
fn colour(s: &mut AppState, rgb: [f64; 3]) {
    let index = active(s) as u32;
    for (n, v) in ["Color R", "Color G", "Color B"].iter().zip(rgb) {
        execute(s, Command::SetModifierParameter { index, name: (*n).into(), value: v }).unwrap();
    }
}

fn active(s: &AppState) -> usize {
    let id = *s.selection.iter().next().unwrap();
    s.scene.node(id).unwrap().active_entry.unwrap()
}

fn colours(s: &AppState, id: NodeId) -> Vec<[f32; 3]> {
    s.scene.eval_mesh(id).value.mesh.vertices().iter().map(|v| v.color).collect()
}

fn near(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3)
}

fn top_face(s: &AppState, id: NodeId) -> u32 {
    let m = s.scene.eval_mesh(id).value.mesh.clone();
    (0..m.face_count() as u32).find(|f| m.face_normal(FaceId(*f)).z > 0.9).unwrap()
}

fn top_vertices(s: &AppState, id: NodeId) -> Vec<bool> {
    s.scene.eval_mesh(id).value.mesh.vertices().iter().map(|v| v.position.z > 9.9).collect()
}

#[test]
fn face_selection_restricts_the_brush_and_paint_all_fills_it() {
    let mut s = AppState::new();
    let b = painted_box(&mut s, [1.0, 0.0, 0.0]);
    let top = top_face(&s, b);
    execute(&mut s, Command::VertexPaintSelectFaces { ids: vec![top], op: SelectOp::Replace }).unwrap();
    // A brush over the whole box paints only the selected face's corners.
    execute(&mut s, Command::VertexPaintStroke { node: b.0, points: vec![[0.0, 0.0, 5.0]], rgb: [255, 0, 0], size: 100.0, opacity: 1.0 }).unwrap();
    let up = top_vertices(&s, b);
    for (c, t) in colours(&s, b).into_iter().zip(&up) {
        assert!(if *t { c[0] > 0.9 && c[1] < 0.95 } else { near(c, [1.0; 3]) }, "{c:?} top {t}");
    }
    // Paint All: the selection fills with the swatch colour.
    colour(&mut s, [0.0, 0.0, 1.0]);
    execute(&mut s, Command::VertexPaintPaintAll).unwrap();
    for (c, t) in colours(&s, b).into_iter().zip(&up) {
        assert!(if *t { near(c, [0.0, 0.0, 1.0]) } else { near(c, [1.0; 3]) }, "{c:?} top {t}");
    }
    // No faces selected: Paint All fills every face. Undo takes it back.
    execute(&mut s, Command::VertexPaintSelectFaces { ids: vec![], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::VertexPaintPaintAll).unwrap();
    assert!(colours(&s, b).into_iter().all(|c| near(c, [0.0, 0.0, 1.0])));
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(colours(&s, b).into_iter().zip(&up).all(|(c, t)| *t == near(c, [0.0, 0.0, 1.0])));
}

#[test]
fn new_layer_opacity_and_blend_modes() {
    let mut s = AppState::new();
    let b = painted_box(&mut s, [1.0, 0.0, 0.0]);
    execute(&mut s, Command::VertexPaintPaintAll).unwrap();
    execute(&mut s, Command::VertexPaintNewLayer { name: "Yellow Brick".into(), mode: 0 }).unwrap();
    let n = s.scene.node(b).unwrap();
    assert_eq!(n.modifiers.len(), 2, "New Layer adds another VertexPaint");
    assert!(matches!(n.modifiers[1].modifier, Modifier::VertexPaint { .. }));
    assert_eq!(n.modifiers[1].name, "Yellow Brick");
    assert_eq!(n.active_entry, Some(1));
    colour(&mut s, [0.0, 0.0, 1.0]);
    execute(&mut s, Command::VertexPaintPaintAll).unwrap();
    assert!(colours(&s, b).into_iter().all(|c| near(c, [0.0, 0.0, 1.0])), "the new layer covers the old");
    let set = |s: &mut AppState, name: &str, v: f64| execute(s, Command::SetModifierParameter { index: 1, name: name.into(), value: v }).unwrap();
    set(&mut s, "Layer Opacity", 0.5);
    assert!(colours(&s, b).into_iter().all(|c| near(c, [0.5, 0.0, 0.5])), "{:?}", colours(&s, b)[0]);
    let mode = |name: &str| loom_scene::modifier::VERTEX_PAINT_MODES.iter().position(|m| *m == name).unwrap() as f64;
    set(&mut s, "Mode", mode("Multiply"));
    assert!(colours(&s, b).into_iter().all(|c| near(c, [0.5, 0.0, 0.0])), "{:?}", colours(&s, b)[0]);
    set(&mut s, "Mode", mode("Color Dodge"));
    assert!(colours(&s, b).into_iter().all(|c| near(c, [1.0, 0.0, 0.5])), "{:?}", colours(&s, b)[0]);
    set(&mut s, "Mode", mode("Screen"));
    assert!(colours(&s, b).into_iter().all(|c| near(c, [1.0, 0.0, 0.5])), "{:?}", colours(&s, b)[0]);
    set(&mut s, "Layer Opacity", 0.0);
    assert!(colours(&s, b).into_iter().all(|c| near(c, [1.0, 0.0, 0.0])), "opacity 0 hides the layer");
}

#[test]
fn soft_strokes_build_up_and_the_eyedropper_picks_the_painted_colour() {
    let mut s = AppState::new();
    let b = painted_box(&mut s, [1.0, 0.0, 0.0]);
    let corner = [5.0, 5.0, 10.0];
    execute(&mut s, Command::VertexPaintStroke { node: b.0, points: vec![corner], rgb: [255, 0, 0], size: 3.0, opacity: 0.5 }).unwrap();
    let m = s.scene.eval_mesh(b).value.mesh.clone();
    let v = m.vertices().iter().position(|v| v.position.distance(glam::Vec3::from(corner)) < 1e-3).unwrap();
    assert!(near(colours(&s, b)[v], [1.0, 0.5, 0.5]), "half way: {:?}", colours(&s, b)[v]);
    execute(&mut s, Command::VertexPaintStroke { node: b.0, points: vec![corner], rgb: [255, 0, 0], size: 3.0, opacity: 0.5 }).unwrap();
    assert!(near(colours(&s, b)[v], [1.0, 0.25, 0.25]), "a second pass covers more: {:?}", colours(&s, b)[v]);
    // The eyedropper sets the swatch from the surface under it.
    colour(&mut s, [0.0, 1.0, 0.0]);
    execute(&mut s, Command::VertexPaintPickColor { node: b.0, point: corner }).unwrap();
    let Modifier::VertexPaint { color, .. } = &s.scene.node(b).unwrap().modifiers[0].modifier else { panic!() };
    let picked = [color[0].get(), color[1].get(), color[2].get()];
    assert!(near(picked, [1.0, 0.25, 0.25]), "{picked:?}");
    // Picking in the middle of the top face blends its corners.
    execute(&mut s, Command::VertexPaintPickColor { node: b.0, point: [0.0, 0.0, 10.0] }).unwrap();
    let Modifier::VertexPaint { color, .. } = &s.scene.node(b).unwrap().modifiers[0].modifier else { panic!() };
    assert!(color[1].get() > 0.3 && color[1].get() < 0.99, "a blend of the corners: {}", color[1].get());
}

/// Vertex colours of `id`'s plane vertex nearest `at`.
fn colour_near(s: &AppState, id: NodeId, at: glam::Vec3) -> [f32; 3] {
    let m = s.scene.eval_mesh(id).value.mesh.clone();
    let world = s.scene.object_matrix(id);
    let v = (0..m.vertex_count()).min_by(|a, b| world.transform_point3(m.vertices()[*a].position).distance(at).total_cmp(&world.transform_point3(m.vertices()[*b].position).distance(at))).unwrap();
    m.vertices()[v].color
}

#[test]
fn assign_vertex_colors_bakes_the_lights_and_their_shadows() {
    let mut s = AppState::new();
    // An omni over a box over a plane: the box shades the middle of the plane.
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0, 0.0, 20.0], size: [40.0; 3] }).unwrap();
    let r = execute(&mut s, Command::CreateLight { kind: loom_commands::light::LightKind::Omni, position: [0.0, 0.0, 120.0], target: [0.0; 3] }).unwrap();
    let omni = r["id"].as_u64().unwrap();
    execute(&mut s, Command::SetParameter { id: omni, name: "Shadows On".into(), value: 1.0 }).unwrap();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [200.0, 200.0, 0.0] }).unwrap();
    let plane = *s.selection.iter().next().unwrap();
    for n in ["Length Segs", "Width Segs"] {
        execute(&mut s, Command::SetParameter { id: plane.0, name: n.into(), value: 4.0 }).unwrap();
    }
    let (mid, edge) = (glam::Vec3::ZERO, glam::Vec3::new(100.0, 0.0, 0.0));
    // Lighting Only, no shadows: the middle (under the light) is the brightest.
    execute(&mut s, Command::AssignVertexColors { light_model: 1, by_vertex: true, shadows: false, update: false }).unwrap();
    let n = s.scene.node(plane).unwrap();
    assert!(matches!(n.modifiers.last().map(|m| &m.modifier), Some(Modifier::VertexPaint { .. })), "Assign To Selected adds a VertexPaint");
    let (a, b) = (colour_near(&s, plane, mid), colour_near(&s, plane, edge));
    assert!(a[0] > b[0] && b[0] > 0.0, "lit: middle {a:?} edge {b:?}");
    // Lighting + Diffuse with Shadows, Assign on the entry: the box's shadow darkens the middle.
    execute(&mut s, Command::AssignVertexColors { light_model: 0, by_vertex: true, shadows: true, update: true }).unwrap();
    assert_eq!(s.scene.node(plane).unwrap().modifiers.len(), 1, "Assign refills the entry");
    let (a, b) = (colour_near(&s, plane, mid), colour_near(&s, plane, edge));
    assert!(a[0] < 1e-3 && b[0] > 0.01, "shadowed middle {a:?}, lit edge {b:?}");
    // Diffuse: the object's colour everywhere, no lighting.
    execute(&mut s, Command::AssignVertexColors { light_model: 2, by_vertex: false, shadows: false, update: true }).unwrap();
    let w = s.scene.node(plane).unwrap().wire_color;
    let want = [w[0] as f32 / 255.0, w[1] as f32 / 255.0, w[2] as f32 / 255.0];
    assert!(s.scene.eval_mesh(plane).value.mesh.vertices().iter().all(|v| near(v.color, want)), "diffuse {want:?}");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(colour_near(&s, plane, mid)[0] < 1e-3, "undo brings the lit colours back");
}
