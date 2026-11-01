//! Transforming a large selection stays interactive.
use loom_commands::{execute, AppState, Axis, Command, PrimitiveKind};
use std::time::Instant;

fn scene(n: usize) -> AppState {
    let mut s = AppState::new();
    for i in 0..n {
        let p = [(i % 20) as f32 * 15.0, (i / 20) as f32 * 15.0, 0.0];
        execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: p, size: [10.0; 3] }).unwrap();
    }
    execute(&mut s, Command::SelectAll).unwrap();
    assert_eq!(s.selection.len(), n);
    s
}

fn time(s: &mut AppState, cmd: Command) -> f64 {
    let t = Instant::now();
    execute(s, cmd).unwrap();
    t.elapsed().as_secs_f64() * 1000.0
}

#[test]
fn moving_and_rotating_200_nodes() {
    let mut s = scene(200);
    let mv = time(&mut s, Command::MoveSelection { delta: [1.0, 0.0, 0.0], system: None });
    let rot = time(&mut s, Command::RotateSelection { axis: Axis::Z, degrees: 5.0, system: None });
    let undo = time(&mut s, Command::UndoSceneOperation);
    eprintln!("perf 200 nodes: move {mv:.1} ms, rotate {rot:.1} ms, undo {undo:.1} ms");
    if !cfg!(debug_assertions) {
        assert!(mv < 50.0 && rot < 50.0, "move {mv:.1} ms / rotate {rot:.1} ms");
    }
}

