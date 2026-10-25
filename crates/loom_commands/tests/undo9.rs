//! Undo after the phase 9 modifier edits: PathDeform, Point Cache, Spline
//! IK Control and Edit Normals Copy / Paste each come back exactly.

use glam::Vec3;
use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp, ShapeKind};
use loom_scene::{Modifier, NodeId};

fn prim(s: &mut AppState, kind: PrimitiveKind, size: [f32; 3]) -> NodeId {
    execute(s, Command::CreatePrimitive { kind, position: [0.0; 3], size }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn select(s: &mut AppState, id: NodeId) {
    execute(s, Command::SelectNodes { ids: vec![id.0], op: SelectOp::Replace }).unwrap();
}

fn add(s: &mut AppState, name: &str) {
    execute(s, Command::AddModifier { name: name.into() }).unwrap();
}

fn set(s: &mut AppState, index: u32, name: &str, value: f64) {
    execute(s, Command::SetModifierParameter { index, name: name.into(), value }).unwrap();
}

fn verts(s: &AppState, id: NodeId) -> Vec<Vec3> {
    s.scene.eval_mesh(id).value.mesh.vertices().iter().map(|v| v.position).collect()
}

fn stack(s: &AppState, id: NodeId) -> Vec<Modifier> {
    s.scene.node(id).unwrap().modifiers.iter().map(|m| m.modifier.clone()).collect()
}

fn same(a: &[Vec3], b: &[Vec3]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(p, q)| p.distance(*q) < 1e-4)
}

fn undo(s: &mut AppState) {
    execute(s, Command::UndoSceneOperation).unwrap();
}

#[test]
fn undo_pathdeform_edits() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Circle, position: [0.0; 3], size: [100.0, 100.0, 0.0] }).unwrap();
    let circle = *s.selection.iter().next().unwrap();
    let b = prim(&mut s, PrimitiveKind::Box, [2.0, 2.0, 10.0]);
    add(&mut s, "PathDeform");
    let plain = (stack(&s, b), verts(&s, b));
    execute(&mut s, Command::PickModifierNode { index: 0, node: circle.0 }).unwrap();
    let picked = (stack(&s, b), verts(&s, b));
    assert!(!same(&plain.1, &picked.1), "the pick deforms");
    let mut steps = vec![picked];
    for (name, value) in [("Path Deform Axis", 2.0), ("Percent", 25.0), ("Stretch", 2.0), ("Rotation", 90.0), ("Twist", 180.0), ("Flip", 1.0)] {
        set(&mut s, 0, name, value);
        let now = (stack(&s, b), verts(&s, b));
        assert!(!same(&now.1, &steps.last().unwrap().1), "{name} changes the mesh");
        steps.push(now);
    }
    // Each undo returns the previous parameters and the previous mesh.
    steps.pop();
    while let Some((st, v)) = steps.pop() {
        undo(&mut s);
        assert_eq!(stack(&s, b), st);
        assert!(same(&verts(&s, b), &v));
    }
    undo(&mut s);
    assert_eq!(stack(&s, b), plain.0, "the pick is undone");
    assert!(same(&verts(&s, b), &plain.1));
}

#[test]
fn undo_point_cache_load_and_unload() {
    let mut s = AppState::new();
    let b = prim(&mut s, PrimitiveKind::Box, [10.0; 3]);
    execute(&mut s, Command::SetParameter { id: b.0, name: "Height Segs".into(), value: 4.0 }).unwrap();
    add(&mut s, "Bend");
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 4 }).unwrap();
    set(&mut s, 0, "Angle", 90.0);
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 0 }).unwrap();
    add(&mut s, "Point Cache");
    let dir = std::env::temp_dir().join(format!("loom-undo9-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("bend.pc2").to_string_lossy().to_string();
    execute(&mut s, Command::PointCacheRecord { index: 1, path: file.clone(), start: 0, end: 4 }).unwrap();
    execute(&mut s, Command::ToggleModifier { index: 0, on: false }).unwrap();
    execute(&mut s, Command::SetFrame { frame: 4 }).unwrap();
    let playing = verts(&s, b);
    let path = |s: &AppState| match &s.scene.node(b).unwrap().modifiers[1].modifier {
        Modifier::PointCache(d) => d.path.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(path(&s), file);
    execute(&mut s, Command::PointCacheUnload { index: 1 }).unwrap();
    assert_eq!(path(&s), "");
    let still = verts(&s, b);
    assert!(!same(&still, &playing), "unloaded: the bend is off, nothing plays");
    // Undo Unload: the file plays again.
    undo(&mut s);
    assert_eq!(path(&s), file);
    assert!(same(&verts(&s, b), &playing));
    // Undo the Bend toggle and the recording: no file on the entry.
    undo(&mut s);
    undo(&mut s);
    assert_eq!(path(&s), "");
    // Redo brings the recording back.
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert_eq!(path(&s), file);
}

#[test]
fn undo_spline_ik_control_create_helpers() {
    let mut s = AppState::new();
    let r = execute(&mut s, Command::LineStart { point: [0.0, 0.0, 0.0] }).unwrap();
    let id = r["id"].as_u64().unwrap();
    for p in [[10.0, 0.0, 0.0], [20.0, 0.0, 0.0]] {
        execute(&mut s, Command::LineAddKnot { node: id, point: p, smooth: false }).unwrap();
    }
    execute(&mut s, Command::LineEnd { node: id }).unwrap();
    let line = NodeId(id);
    select(&mut s, line);
    add(&mut s, "Spline IK Control");
    let nodes = s.scene.nodes().count();
    let before = stack(&s, line);
    execute(&mut s, Command::SplineIkControlCreateHelpers).unwrap();
    assert_eq!(s.scene.nodes().count(), nodes + 3);
    let helpers: Vec<NodeId> = match &stack(&s, line)[0] {
        Modifier::SplineIkControl(d) => d.helpers.iter().map(|h| h.unwrap()).collect(),
        other => panic!("{other:?}"),
    };
    select(&mut s, helpers[2]);
    execute(&mut s, Command::MoveSelection { delta: [0.0, 5.0, 0.0], system: None }).unwrap();
    let knots = |s: &AppState| s.scene.eval_mesh(line).value.shape.clone().unwrap().knot_positions();
    let moved = knots(&s);
    // Undo the move: the knot is back; undo Create Helpers: no helpers, no links.
    undo(&mut s);
    assert!((knots(&s)[2] - moved[2]).length() > 4.0);
    // The helper selection is an undo step of its own.
    undo(&mut s);
    undo(&mut s);
    assert_eq!(s.scene.nodes().count(), nodes);
    assert!(helpers.iter().all(|h| s.scene.node(*h).is_none()));
    assert_eq!(stack(&s, line), before);
    // Redo: the helpers, the helper selection and the moved knot come back.
    for _ in 0..3 {
        execute(&mut s, Command::RedoSceneOperation).unwrap();
    }
    assert_eq!(s.scene.nodes().count(), nodes + 3);
    assert!(same(&knots(&s), &moved));
}

#[test]
fn undo_edit_normals_copy_paste() {
    let mut s = AppState::new();
    let b = prim(&mut s, PrimitiveKind::Box, [10.0; 3]);
    add(&mut s, "Edit Normals");
    let normals = |s: &AppState| loom_mesh::mesh_mods::corner_normals(&s.scene.eval_mesh(b).value.mesh);
    execute(&mut s, Command::EditNormalsSelectBy { level: 0 }).unwrap();
    execute(&mut s, Command::EditNormalsSelect { ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::CopyNormal).unwrap();
    execute(&mut s, Command::EditNormalsSelectBy { level: 1 }).unwrap();
    execute(&mut s, Command::EditNormalsSelect { ids: vec![6], op: SelectOp::Replace }).unwrap();
    let before = (stack(&s, b), normals(&s));
    execute(&mut s, Command::PasteNormal).unwrap();
    let pasted = (stack(&s, b), normals(&s));
    assert_ne!(pasted.1, before.1);
    undo(&mut s);
    assert_eq!(stack(&s, b), before.0, "the modifier data is back");
    assert!(same(&normals(&s), &before.1), "the normals are back");
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert_eq!(stack(&s, b), pasted.0);
    assert!(same(&normals(&s), &pasted.1));
}
