//! Unwrap UVW on the Vertex Color Channel (channel 0) and the State report.

use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp};

#[test]
fn unwrap_channel_0_edits_vertex_colors() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let a = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::AddModifier { name: "Unwrap UVW".into() }).unwrap();
    execute(&mut s, Command::UnwrapMapChannel { channel: 0 }).unwrap();
    // The working channel 0 is the vertex colour per vertex: all white at first.
    let r = execute(&mut s, Command::State).unwrap();
    assert_eq!(r["unwrap"]["channel"], 0);
    assert_eq!(r["unwrap"]["tvs"], 8, "one colour tv per vertex");
    // Move every colour-tv to (0.2, 0.6): r = 0.2, g = 0.6 on the result.
    execute(&mut s, Command::SelectAllTv).unwrap();
    execute(&mut s, Command::SetTvPosition { u: Some(0.2), v: Some(0.6), w: None }).unwrap();
    let m = s.scene.eval_mesh(a).value.mesh.clone();
    for v in m.vertices() {
        assert!((v.color[0] - 0.2).abs() < 1e-5 && (v.color[1] - 0.6).abs() < 1e-5 && (v.color[2] - 1.0).abs() < 1e-5, "{:?}", v.color);
    }
    assert!(m.map(1).is_some(), "channel 1 is untouched");
    // Back to channel 1: the edits of channel 0 are dropped, colours return.
    execute(&mut s, Command::UnwrapMapChannel { channel: 1 }).unwrap();
    let m = s.scene.eval_mesh(a).value.mesh.clone();
    assert!(m.vertices().iter().all(|v| v.color == [1.0; 3]));
}

#[test]
fn state_reports_map_channels() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let a = *s.selection.iter().next().unwrap();
    let r = execute(&mut s, Command::State).unwrap();
    assert_eq!(r["nodes"][0]["maps"], serde_json::json!([1]));
    execute(&mut s, Command::AddModifier { name: "UVW Map".into() }).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Channel".into(), value: 2.0 }).unwrap();
    let r = execute(&mut s, Command::State).unwrap();
    assert_eq!(r["nodes"][0]["maps"], serde_json::json!([1, 2]));
    // Set Vertex Color / Select by Vertex Color are the Editable Poly chart rows.
    for name in ["Set Vertex Color", "Select by Vertex Color"] {
        assert!(loom_commands::registry().iter().any(|i| i.name == name), "{name} missing");
    }
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
}
