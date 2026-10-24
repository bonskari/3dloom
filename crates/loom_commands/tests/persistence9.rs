//! Every modifier phase 9 added survives a .loom save and open.

use loom_commands::{execute, AppState, Command, PrimitiveKind, ShapeKind};

const MESH: &[&str] = &[
    "Edit Normals", "Mesh Select", "Vol. Select", "Affect Region", "PathDeform", "PathDeform (WSM)", "Vertex Weld", "Substitute", "Flex",
    "Camera Map", "Camera Map (WSM)", "MapScaler", "MapScaler (WSM)", "UVW Mapping Add", "UVW Mapping Clear", "HSDS", "Subdivide (WSM)", "Point Cache", "Point Cache (WSM)",
];
const SHAPE: &[&str] = &["Spline Select", "Delete Spline", "Normalize Spline", "Renderable Spline", "Fillet/Chamfer", "Trim/Extend", "CrossSection", "Surface", "Bevel Profile", "Spline IK Control"];

#[test]
fn phase9_modifiers_round_trip_through_loom_files() {
    let dir = std::env::temp_dir().join(format!("loom-persist9-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = AppState::new();
    for (i, name) in MESH.iter().enumerate() {
        execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [i as f32 * 20.0, 0.0, 0.0], size: [10.0; 3] }).unwrap();
        execute(&mut s, Command::AddModifier { name: (*name).into() }).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    for (i, name) in SHAPE.iter().enumerate() {
        execute(&mut s, Command::CreateShape { kind: ShapeKind::Rectangle, position: [i as f32 * 20.0, 50.0, 0.0], size: [10.0, 10.0, 0.0] }).unwrap();
        execute(&mut s, Command::AddModifier { name: (*name).into() }).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    let path = dir.join("p9.loom");
    execute(&mut s, Command::SaveFileAs { path: Some(path.to_string_lossy().into()) }).unwrap();
    let mut t = AppState::new();
    execute(&mut t, Command::OpenFile { path: Some(path.to_string_lossy().into()) }).unwrap();
    let dump = |s: &AppState| -> Vec<serde_json::Value> { s.scene.nodes().map(|n| serde_json::to_value(&n.modifiers).unwrap()).collect() };
    let (a, b) = (dump(&s), dump(&t));
    assert_eq!(a.len(), MESH.len() + SHAPE.len());
    assert_eq!(a, b);
    let names: Vec<String> = t.scene.nodes().flat_map(|n| n.modifiers.iter().map(|m| m.modifier.name().to_string())).collect();
    for n in MESH.iter().chain(SHAPE) {
        assert!(names.iter().any(|x| x == n), "{n} after reopening");
    }
}
