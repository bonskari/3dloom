//! Skin Wrap Patch through commands: the modifier, Pick Patch (also by a
//! viewport pick), Sample Rate, Resample, and a move of the patch
//! carrying the mesh.

use loom_commands::{execute, AppState, Command, PickMode, PrimitiveKind, SelectOp};
use loom_scene::{Modifier, NodeId};

fn data(s: &AppState, id: NodeId) -> loom_scene::morph::SkinWrapPatchData {
    let Modifier::SkinWrapPatch(d) = &s.scene.node(id).unwrap().modifiers[0].modifier else { panic!() };
    (**d).clone()
}

#[test]
fn a_picked_patch_deforms_the_mesh() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0, 0.0, 1.0], size: [16.0, 16.0, 0.0] }).unwrap();
    let cloth = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SkinWrapPatchModifier).unwrap();
    assert_eq!(s.scene.node(cloth).unwrap().modifiers[0].name, "Skin Wrap Patch");
    let st = execute(&mut s, Command::State).unwrap();
    assert_eq!(st["skin_wrap_patch"]["patch"], serde_json::Value::Null, "the button reads None");
    // A box is not a patch object.
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [40.0, 0.0, 0.0], size: [5.0; 3] }).unwrap();
    let boxed = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![cloth.0], op: SelectOp::Replace }).unwrap();
    assert!(execute(&mut s, Command::SkinWrapPatchPick { node: boxed.0 }).is_err());
    // A Quad Patch grid.
    execute(&mut s, Command::CreatePatchGrid { quad: true, position: [0.0; 3], length: 20.0, width: 20.0 }).unwrap();
    let patch = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![cloth.0], op: SelectOp::Replace }).unwrap();
    // Pick Patch by a viewport pick (the button turns the pick on).
    s.pick_mode = Some(PickMode::ModifierNode);
    execute(&mut s, Command::PickHit { id: patch.0 }).unwrap();
    let d = data(&s, cloth);
    assert_eq!(d.patch, Some(patch));
    assert_eq!(d.binds.len(), 25);
    let st = execute(&mut s, Command::State).unwrap();
    assert_eq!(st["skin_wrap_patch"]["bound"], 25);
    execute(&mut s, Command::SkinWrapPatchSetSampleRate { rate: 8 }).unwrap();
    assert_eq!(data(&s, cloth).sample_rate, 8);
    let rest = s.scene.eval_mesh(cloth).value.mesh.clone();
    // Moving the patch carries the mesh.
    execute(&mut s, Command::SelectNodes { ids: vec![patch.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 4.0], system: Some(loom_commands::CoordSystem::World) }).unwrap();
    let moved = s.scene.eval_mesh(cloth).value.mesh.clone();
    for (a, b) in rest.vertices().iter().zip(moved.vertices()) {
        assert!(((b.position - a.position).z - 4.0).abs() < 1e-3);
    }
    // Resample there: the mesh stays put.
    execute(&mut s, Command::SelectNodes { ids: vec![cloth.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinWrapPatchResample).unwrap();
    let again = s.scene.eval_mesh(cloth).value.mesh.clone();
    for (a, b) in rest.vertices().iter().zip(again.vertices()) {
        assert!((b.position - a.position).length() < 1e-3, "resampled at the new place: no deformation");
    }
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(loom_commands::registry().iter().any(|i| i.name == "Skin Wrap Patch Modifier"));
}
