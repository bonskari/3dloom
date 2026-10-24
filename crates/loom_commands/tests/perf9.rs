//! Phase 9 modifiers stay usable on dense meshes: one evaluation of a
//! 10,201-vertex plane is linear work, not a half-edge walk per vertex.

use loom_commands::{execute, AppState, Command, PrimitiveKind, ShapeKind};
use std::time::{Duration, Instant};

fn dense_plane(s: &mut AppState) -> loom_scene::NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [100.0, 100.0, 0.0] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    execute(s, Command::SetParameter { id: id.0, name: "Length Segs".into(), value: 100.0 }).unwrap();
    execute(s, Command::SetParameter { id: id.0, name: "Width Segs".into(), value: 100.0 }).unwrap();
    id
}

/// Debug builds are slow; the quadratic versions took many seconds here.
const BUDGET: Duration = Duration::from_millis(2500);

fn timed(name: &str, setup: impl FnOnce(&mut AppState, loom_scene::NodeId)) {
    let mut s = AppState::new();
    let id = dense_plane(&mut s);
    let n = s.scene.eval_mesh(id).value.mesh.vertex_count();
    assert_eq!(n, 101 * 101);
    setup(&mut s, id);
    execute(&mut s, Command::SetFrame { frame: 3 }).unwrap();
    let t = Instant::now();
    let e = s.scene.eval_mesh(id);
    let dt = t.elapsed();
    assert_eq!(e.value.mesh.vertex_count(), n, "{name}: {:?}", e.warnings);
    assert!(dt < BUDGET, "{name}: one evaluation took {dt:?}");
}

#[test]
fn affect_region_is_linear() {
    timed("Affect Region", |s, _| {
        execute(s, Command::AddModifier { name: "Affect Region".into() }).unwrap();
        execute(s, Command::SetModifierParameter { index: 0, name: "Falloff".into(), value: 200.0 }).unwrap();
    });
}

#[test]
fn path_deform_is_linear() {
    timed("PathDeform", |s, id| {
        execute(s, Command::CreateShape { kind: ShapeKind::Circle, position: [0.0; 3], size: [300.0, 300.0, 0.0] }).unwrap();
        let c = *s.selection.iter().next().unwrap();
        execute(s, Command::SelectNodes { ids: vec![id.0], op: loom_commands::SelectOp::Replace }).unwrap();
        execute(s, Command::AddModifier { name: "PathDeform".into() }).unwrap();
        execute(s, Command::PickModifierNode { index: 0, node: c.0 }).unwrap();
    });
}

#[test]
fn flex_is_linear() {
    timed("Flex", |s, _| {
        execute(s, Command::AddModifier { name: "Flex".into() }).unwrap();
        execute(s, Command::SetModifierParameter { index: 0, name: "Samples".into(), value: 1.0 }).unwrap();
    });
}

#[test]
fn point_cache_is_linear() {
    timed("Point Cache", |s, _| {
        execute(s, Command::AddModifier { name: "Point Cache".into() }).unwrap();
        let dir = std::env::temp_dir().join(format!("loom-perf9-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("plane.pc2");
        execute(s, Command::PointCacheRecord { index: 0, path: f.to_string_lossy().into(), start: 0, end: 4 }).unwrap();
    });
}

#[test]
fn edit_normals_is_linear() {
    timed("Edit Normals", |s, _| {
        execute(s, Command::AddModifier { name: "Edit Normals".into() }).unwrap();
    });
}

#[test]
fn edited_normals_evaluate_in_linear_time() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [100.0, 100.0, 0.0] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SetParameter { id: id.0, name: "Length Segs".into(), value: 100.0 }).unwrap();
    execute(&mut s, Command::SetParameter { id: id.0, name: "Width Segs".into(), value: 100.0 }).unwrap();
    execute(&mut s, Command::AddModifier { name: "Edit Normals".into() }).unwrap();
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    execute(&mut s, Command::EditNormalsSelectBy { level: 1 }).unwrap();
    execute(&mut s, Command::EditNormalsSelect { ids: (0..101 * 101).collect(), op: loom_commands::SelectOp::Replace }).unwrap();
    let t = Instant::now();
    execute(&mut s, Command::BreakNormals).unwrap();
    execute(&mut s, Command::MakeExplicit).unwrap();
    let e = s.scene.eval_mesh(id);
    let dt = t.elapsed();
    assert!(e.value.mesh.map(loom_mesh::NORMALS_CHANNEL).is_some(), "{:?}", e.warnings);
    assert!(dt < BUDGET, "Break + evaluation took {dt:?}");
}
