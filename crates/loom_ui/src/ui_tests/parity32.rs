//! The parity32 round.

use super::*;
use egui::accesskit::Role;

/// Types `text` into the spinner after the label `label`.
fn type_after(h: &mut Harness, label: &str, text: &str) {
    let at = h.spinner_after(label).center();
    h.click(at);
    h.mods = egui::Modifiers::COMMAND;
    h.key(egui::Key::A);
    h.mods = egui::Modifiers::NONE;
    h.run(vec![egui::Event::Text(text.into())]);
    h.key(egui::Key::Enter);
    h.frames(2);
}

/// A chain of `n` bones along +Z; returns their ids (the last one is the nub).
fn chain(h: &mut Harness, n: usize) -> Vec<loom_scene::NodeId> {
    let points: Vec<[f32; 3]> = (0..=n).map(|i| [0.0, 0.0, i as f32 * 20.0]).collect();
    let r = h.app.dispatch(Command::CreateBoneChain { points, name: None }).unwrap();
    r["nodes"].as_array().unwrap().iter().map(|v| loom_scene::NodeId(v.as_u64().unwrap())).collect()
}

fn param(h: &Harness, id: loom_scene::NodeId, name: &str) -> f64 {
    let n = h.app.state.scene.node(id).unwrap();
    n.object.parameters().iter().find(|(p, _)| *p == name).map(|(_, v)| v.as_f64()).unwrap_or_else(|| panic!("no parameter {name}"))
}

/// Bone Tools > Fin Adjustment Tools: Relative adds the typed change to each
/// bone's own value where Absolute sets the same value on all of them; Copy
/// then Paste carries the bone and fin settings to another bone.
#[test]
fn bone_fin_relative_mode_and_copy_paste_of_the_settings() {
    let mut h = Harness::new();
    let ids = chain(&mut h, 3);
    h.cmd(Command::SetParameter { id: ids[0].0, name: "Width".into(), value: 10.0 });
    h.cmd(Command::SetParameter { id: ids[1].0, name: "Width".into(), value: 20.0 });
    h.cmd(Command::SelectNodes { ids: vec![ids[0].0, ids[1].0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::BoneTools);
    h.frames(2);
    assert!(h.app.state.bone_fin_absolute, "Absolute by default");
    h.click_text_anywhere("Relative");
    assert!(!h.app.state.bone_fin_absolute);
    // The spinner edits the first bone's Width 10 -> 12; Relative moves both by +2.
    type_after(&mut h, "Width:", "12");
    assert_eq!(param(&h, ids[0], "Width"), 12.0);
    assert_eq!(param(&h, ids[1], "Width"), 22.0, "Relative keeps the size relationship");
    h.click_text_anywhere("Absolute");
    type_after(&mut h, "Width:", "5");
    assert_eq!(param(&h, ids[0], "Width"), 5.0);
    assert_eq!(param(&h, ids[1], "Width"), 5.0, "Absolute sets the same value");

    // Copy the first bone's settings, then paste them onto the third.
    h.cmd(Command::SetParameter { id: ids[0].0, name: "Side Fins".into(), value: 1.0 });
    h.cmd(Command::SetParameter { id: ids[0].0, name: "Side Fin Size".into(), value: 7.0 });
    h.cmd(Command::SelectNodes { ids: vec![ids[0].0], op: loom_commands::SelectOp::Replace });
    h.frames(2);
    h.click_text_anywhere("Copy");
    assert!(h.app.state.bone_settings_clipboard.is_some());
    h.cmd(Command::SelectNodes { ids: vec![ids[2].0], op: loom_commands::SelectOp::Replace });
    h.frames(2);
    h.click_text_anywhere("Paste");
    assert_eq!(param(&h, ids[2], "Side Fins"), 1.0);
    assert_eq!(param(&h, ids[2], "Side Fin Size"), 7.0);
    assert_eq!(param(&h, ids[2], "Width"), 5.0);
    h.cmd(Command::Undo);
    assert_eq!(param(&h, ids[2], "Side Fins"), 0.0, "one undo step");
}

/// Bone Tools > Object Properties: Auto-Align off keeps Bone Edit Mode from
/// turning the parent after its child; Realign brings it back, Reset Stretch
/// stretches it to the child and Reset Scale puts the scale back to 100 %.
#[test]
fn bone_auto_align_realign_reset_stretch_and_reset_scale() {
    let mut h = Harness::new();
    let ids = chain(&mut h, 2);
    let (root, mid) = (ids[0], ids[1]);
    h.cmd(Command::SelectNodes { ids: vec![root.0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::BoneTools);
    h.frames(2);
    assert_eq!(param(&h, root, "Auto-Align"), 1.0, "Auto-Align on by default");
    assert_eq!(param(&h, root, "Correct Negative Stretch"), 1.0);
    h.click(h.find(Role::CheckBox, "Auto-Align").center());
    assert_eq!(param(&h, root, "Auto-Align"), 0.0);

    // Bone Edit Mode: with Auto-Align off the parent keeps its length.
    h.cmd(Command::BoneEditModeToggle);
    let before = param(&h, root, "Length");
    h.cmd(Command::SelectNodes { ids: vec![mid.0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::MoveSelection { delta: [5.0, 0.0, 20.0], system: None });
    assert!((param(&h, root, "Length") - before).abs() < 1e-3, "Auto-Align off: no stretch");

    // Realign turns the root at its child, Reset Stretch reaches it.
    h.cmd(Command::SelectNodes { ids: vec![root.0], op: loom_commands::SelectOp::Replace });
    h.frames(2);
    h.click_text_anywhere("Reset Stretch");
    let reached = param(&h, root, "Length");
    let base = h.app.state.scene.world_matrix(root).transform_point3(glam::Vec3::ZERO);
    let child = h.app.state.scene.world_matrix(mid).transform_point3(glam::Vec3::ZERO);
    assert!((reached - base.distance(child)).abs() < 1e-2, "Reset Stretch reaches the child");
    assert!(h.app.dispatch(Command::BoneRealign).is_ok(), "Realign runs");

    // Reset Scale: a scaled bone goes back to 100 % on each axis.
    let mut t = h.app.state.scene.node(root).unwrap().transform.clone();
    t.scale.set(glam::Vec3::new(2.0, -1.0, 1.0));
    h.app.state.scene.node_mut(root).unwrap().transform = t;
    h.frames(2);
    h.click_text_anywhere("Reset Scale");
    assert_eq!(h.app.state.scene.node(root).unwrap().transform.scale.get(), glam::Vec3::ONE);
}

/// Bone Tools > Object Properties: the Flip checkbox sets the bone's stretch
/// Flip, and the Stretch Factor readout names the selected bones in Bone Edit
/// Mode (undefined for more than one).
#[test]
fn bone_stretch_flip_and_the_stretch_factor_readout() {
    let mut h = Harness::new();
    let ids = chain(&mut h, 3);
    h.cmd(Command::SelectNodes { ids: vec![ids[0].0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::BoneEditModeToggle);
    h.cmd(Command::BoneTools);
    h.frames(2);
    assert_eq!(param(&h, ids[0], "Flip"), 0.0);
    h.click(h.find(Role::CheckBox, "Flip").center());
    assert_eq!(param(&h, ids[0], "Flip"), 1.0);
    assert!(!h.text_rects("1 Bone Selected   Stretch Factor: X: 1.000 Y: 1.000 Z: 1.000").is_empty(), "the one bone's factors");
    h.cmd(Command::SelectNodes { ids: vec![ids[0].0, ids[1].0], op: loom_commands::SelectOp::Replace });
    h.frames(2);
    assert!(!h.text_rects("2 Bones Selected   Stretch Factor: undefined").is_empty(), "undefined for more than one");
}

/// Bone Tools > Bone Coloring: Apply Gradient colours the selected chain from
/// the Start Color at its highest parent to the End Color at its last child.
#[test]
fn bone_coloring_apply_gradient_runs_down_the_chain() {
    let mut h = Harness::new();
    let ids = chain(&mut h, 3);
    h.cmd(Command::SelectNodes { ids: vec![ids[0].0, ids[1].0, ids[2].0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::BoneTools);
    h.frames(2);
    h.app.bone_gradient_start = [0, 0, 0];
    h.app.bone_gradient_end = [200, 0, 0];
    h.frames(2);
    h.click_text_anywhere("Apply Gradient");
    let c = |id: loom_scene::NodeId| h.app.state.scene.node(id).unwrap().wire_color;
    assert_eq!(c(ids[0]), [0, 0, 0], "Start Color on the highest parent");
    assert_eq!(c(ids[1]), [100, 0, 0], "an intermediate colour in between");
    assert_eq!(c(ids[2]), [200, 0, 0], "End Color on the last child");
    h.cmd(Command::Undo);
    assert_ne!(c(ids[1]), [100, 0, 0], "one undo step");
}
