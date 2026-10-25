//! Skin Morph, Skin Wrap and Skin Wrap Patch the way a person uses them:
//! the bone list, Pick Bone, Points clicks, the Selection tools, Edit with
//! the move gizmo, the falloff and soft selection graphs; Add by picking,
//! Control Vertices clicks, local spinners, display and bake buttons; Pick
//! Patch and Resample.

use super::*;
use loom_scene::{Modifier, NodeId};

fn morph_data(h: &Harness, id: NodeId) -> loom_scene::morph::SkinMorphData {
    let Modifier::SkinMorph(d) = &h.app.state.scene.node(id).unwrap().modifiers[0].modifier else { panic!() };
    (**d).clone()
}

fn wrap_data(h: &Harness, id: NodeId) -> loom_scene::morph::SkinWrapData {
    let Modifier::SkinWrap(d) = &h.app.state.scene.node(id).unwrap().modifiers[0].modifier else { panic!() };
    (**d).clone()
}

/// A 30-unit cylinder with a three-bone chain up to z 42; the Skin Morph
/// binds the middle bone (z 20–40, its top end above the mesh).
fn morph_arm(h: &mut Harness) -> (NodeId, Vec<u64>) {
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 2400.0));
    h.frames(2);
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Cylinder, position: [0.0; 3], size: [8.0, 8.0, 30.0] });
    let arm = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::SetParameter { id: arm.0, name: "Height Segments".into(), value: 6.0 });
    let r = h.app.dispatch(Command::CreateBoneChain { points: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 20.0], [0.0, 0.0, 40.0], [0.0, 0.0, 42.0]], name: None }).unwrap();
    let bones: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    h.cmd(Command::SelectNodes { ids: vec![arm.0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::SkinMorphModifier);
    h.cmd(Command::SkinMorphAddBones { ids: vec![bones[1]] });
    h.app.panel_tab = crate::Tab::Modify;
    framed(h);
    (arm, bones)
}

/// Clicks the rollout header `title` if its body (holding `inside`) is not shown.
fn open_rollout(h: &mut Harness, title: &str, inside: &str) {
    if h.widgets_labelled(inside).is_empty() {
        h.click(panel_text(h, title));
    }
    assert!(!h.widgets_labelled(inside).is_empty(), "{title} shows {inside}");
}

/// Skin Morph output vertices on screen, each at least 14 px from every other.
fn clear_points(h: &Harness) -> Vec<(u32, egui::Pos2)> {
    let (_, _, pts) = loom_commands::morph::skin_morph_points(&h.app.state).unwrap();
    let rect = h.app.viewport_rects[h.app.state.active_viewport].shrink(40.0);
    let cam = h.app.state.active().camera.eye_position();
    let screen: Vec<egui::Pos2> = pts.iter().map(|q| h.screen_of(*q)).collect();
    (0..pts.len())
        // The front of the cylinder (towards the camera, off its axis).
        .filter(|i| glam::Vec2::new(pts[*i].x, pts[*i].y).length() > 1.0 && glam::Vec2::new(pts[*i].x, pts[*i].y).dot(glam::Vec2::new(cam.x, cam.y)) > 0.0 && pts[*i].z > 1.0 && pts[*i].z < 29.0)
        .filter(|i| rect.contains(screen[*i]) && (0..pts.len()).all(|j| j == *i || screen[j].distance(screen[*i]) > 14.0))
        .map(|i| (i as u32, screen[i]))
        .collect()
}

#[test]
fn skin_morph_bone_list_points_clicks_and_edit_with_the_move_gizmo() {
    let mut h = Harness::new();
    let (arm, bones) = morph_arm(&mut h);
    h.cmd(Command::ModifierSubobjectLevel { index: 0, level: 1 });
    // A click on the bone's orange line (above the mesh) highlights it.
    h.click(h.screen_of(Vec3::new(0.0, 0.0, 37.0)));
    assert_eq!(h.app.state.skin_morph_bone, Some(NodeId(bones[1])), "a click on the bone line highlights the bone");
    h.click_widget("Create Morph");
    assert_eq!(morph_data(&h, arm).morphs.len(), 1);
    h.frames(1);
    assert_eq!(h.widgets_labelled("Morph 1 (100.0)").len(), 1, "the list shows the morph at 100%");
    // Points: click, Ctrl adds, Alt removes.
    let pts = clear_points(&h);
    assert!(pts.len() >= 2, "clickable points: {}", pts.len());
    h.click(pts[0].1);
    assert_eq!(morph_data(&h, arm).selection, vec![pts[0].0]);
    h.mods = egui::Modifiers::CTRL;
    h.click(pts[1].1);
    h.mods = egui::Modifiers::NONE;
    assert_eq!(morph_data(&h, arm).selection.len(), 2);
    h.mods = egui::Modifiers::ALT;
    h.click(pts[1].1);
    h.mods = egui::Modifiers::NONE;
    assert_eq!(morph_data(&h, arm).selection, vec![pts[0].0]);
    // Selection > Grow, then Shrink back.
    open_rollout(&mut h, "Selection", "Grow");
    h.click_widget("Grow");
    assert!(morph_data(&h, arm).selection.len() > 1);
    h.click_widget("Shrink");
    assert_eq!(morph_data(&h, arm).selection, vec![pts[0].0]);
    // Edit, then drag the move gizmo: the point moves into the morph.
    h.click_widget("Edit");
    assert_eq!(morph_data(&h, arm).editing, Some(0));
    h.key(egui::Key::W);
    let centre = loom_commands::morph::points_center(&h.app.state).expect("the gizmo sits on the selected point");
    let before = loom_commands::morph::skin_morph_points(&h.app.state).unwrap().2[pts[0].0 as usize];
    let at = h.screen_of(centre);
    h.drag(at, at + egui::vec2(60.0, 0.0));
    let after = loom_commands::morph::skin_morph_points(&h.app.state).unwrap().2[pts[0].0 as usize];
    assert!((after - before).length() > 0.5, "the dragged point moved: {:?}", after - before);
    assert_eq!(morph_data(&h, arm).morphs[0].deltas.len(), 1, "its delta is in the morph");
    assert!(h.app.state.scene.node(arm).unwrap().transform.position.get().length() < 1e-6, "the object did not move");
    h.click_widget("Edit");
    assert_eq!(morph_data(&h, arm).editing, None);
    // An empty click clears the points and keeps the level.
    let rect = h.app.viewport_rects[h.app.state.active_viewport];
    h.click(egui::pos2(rect.right() - 60.0, rect.bottom() - 40.0));
    assert!(morph_data(&h, arm).selection.is_empty());
    assert!(loom_commands::morph::points_level(&h.app.state).is_some(), "still at the Points level");
}

#[test]
fn skin_morph_pick_bone_custom_falloff_graph_and_soft_selection_graph() {
    let mut h = Harness::new();
    let (arm, bones) = morph_arm(&mut h);
    h.click_widget("Pick Bone");
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::SkinMorphBone));
    h.click(h.screen_of(Vec3::new(0.0, 0.0, 41.0)));
    assert!(morph_data(&h, arm).bones.iter().any(|b| b.node == NodeId(bones[2])), "the clicked bone joins the list");
    let away = h.screen_of(Vec3::new(30.0, 30.0, 0.0));
    right_click(&mut h, away);
    assert_eq!(h.app.state.pick_mode, None, "right-click ends Pick Bone");
    // Highlight the bone in the list, Create Morph, choose Custom Falloff and open its graph.
    let bone_name = h.app.state.scene.node(NodeId(bones[1])).unwrap().name.clone();
    h.click(panel_text(&h, &bone_name));
    h.click_widget("Create Morph");
    h.click_widget("Linear");
    h.click_text_anywhere("Custom Falloff");
    assert_eq!(morph_data(&h, arm).morphs[0].falloff, 4);
    h.click_widget("G");
    // Insert a point into the falloff graph.
    let inserts = h.widgets_labelled("Insert");
    assert_eq!(inserts.len(), 1, "the falloff graph's toolbar");
    let ins = inserts[0];
    h.click(ins.center());
    h.click(egui::pos2(ins.left() + 30.0, ins.bottom() + 40.0));
    assert_eq!(morph_data(&h, arm).morphs[0].curve.len(), 6, "a sixth point in the Custom Falloff graph");
    // The soft selection graph: Use Soft Selection, Reset Graph.
    open_rollout(&mut h, "Selection", "Reset Graph");
    h.click_widget("Use Soft Selection");
    assert!(morph_data(&h, arm).soft.on);
    h.click_widget("Reset Graph");
    assert_eq!(morph_data(&h, arm).soft.curve, loom_scene::morph::default_curve());
    // Options and Copy and Paste checkboxes.
    open_rollout(&mut h, "Options", "Beginner Mode");
    h.click_widget("Beginner Mode");
    assert!(!morph_data(&h, arm).options.beginner_mode);
    h.click_widget("Show Current Angle");
    h.click_widget("Show Morph Bone Matrix");
    h.click_widget("Show Driver Bone Matrix");
    open_rollout(&mut h, "Copy and Paste", "Show Mirror Plane");
    h.click_widget("Show Mirror Plane");
    h.click_widget("Preview Bone");
    h.click_widget("Preview Vertices");
    h.frames(2);
    let d = morph_data(&h, arm);
    assert!(d.options.show_current_angle && d.options.show_morph_matrix && d.options.show_driver_matrix && d.mirror.show_plane && d.mirror.preview_bone && d.mirror.preview_vertices);
    // Planar joint for the morph's bone.
    h.click_widget("Planar Joint");
    assert!(morph_data(&h, arm).bones.iter().find(|b| b.node == NodeId(bones[1])).unwrap().planar);
}

/// A 20×20 control plane and a finer 16×16 plane half a unit above it, wrapped.
fn wrapped(h: &mut Harness) -> (NodeId, NodeId) {
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 2400.0));
    h.frames(2);
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [20.0, 20.0, 0.0] });
    let control = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0, 0.0, 0.5], size: [16.0, 16.0, 0.0] });
    let hi = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::SetParameter { id: hi.0, name: "Length Segs".into(), value: 8.0 });
    h.cmd(Command::SetParameter { id: hi.0, name: "Width Segs".into(), value: 8.0 });
    h.cmd(Command::SkinWrapModifier);
    h.app.panel_tab = crate::Tab::Modify;
    framed(h);
    (control, hi)
}

#[test]
fn skin_wrap_add_by_picking_control_vertices_locals_display_and_bake() {
    let mut h = Harness::new();
    let (control, hi) = wrapped(&mut h);
    h.click_widget("Add");
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::SkinWrapControl));
    h.click(h.screen_of(Vec3::new(9.5, -9.5, 0.0)));
    assert_eq!(wrap_data(&h, hi).bases, vec![control], "the clicked plane is the control object");
    let away = h.screen_of(Vec3::new(40.0, 40.0, 0.0));
    right_click(&mut h, away);
    assert_eq!(h.app.state.pick_mode, None);
    h.cmd(Command::ModifierSubobjectLevel { index: 0, level: 1 });
    // Click the control vertex at the corner (10, -10).
    let cvs = loom_commands::morph::control_vertices(&h.app.state);
    let corner = cvs.iter().find(|(_, p)| (*p - Vec3::new(10.0, -10.0, 0.0)).length() < 1e-3).unwrap().0;
    h.click(h.screen_of(Vec3::new(10.0, -10.0, 0.0)));
    assert_eq!(wrap_data(&h, hi).selection, vec![corner], "a click selects the control vertex");
    // Local Str: drag its spinner.
    let s = h.spinner_after("Local Str:");
    h.drag(s.center(), s.center() + egui::vec2(40.0, 0.0));
    let l = wrap_data(&h, hi).local(corner.0, corner.1);
    assert!(l.strength != 1.0, "Local Str changed: {}", l.strength);
    // Display Parameters > Display Loops off.
    open_rollout(&mut h, "Display Parameters", "Display Loops");
    h.click_widget("Display Loops");
    assert!(!wrap_data(&h, hi).display.loops);
    h.click_widget("Display Unassigned Points");
    assert!(wrap_data(&h, hi).display.unassigned);
    // Advanced Parameters: Bake Control Verts stores them on the control object.
    open_rollout(&mut h, "Advanced Parameters", "Bake Control Verts");
    h.click_widget("Bake Control Verts");
    assert!(h.app.state.scene.node(control).unwrap().skin_wrap_bake.is_some());
    h.click_widget("Show Mirror Data");
    assert!(wrap_data(&h, hi).mirror.show);
    // Face Deformation turns Distance Infl. and Face Limit off; the engine radio sets it.
    h.click_widget("Face Deformation");
    assert_eq!(wrap_data(&h, hi).engine, 0);
    h.click_widget("Vertex Deformation");
    assert_eq!(wrap_data(&h, hi).engine, 1);
    h.click_widget("Blend To Base Mesh");
    assert!(wrap_data(&h, hi).blend_to_base);
}

#[test]
fn skin_wrap_patch_picks_a_patch_and_resamples() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 1600.0));
    h.cmd(Command::CreatePatchGrid { quad: true, position: [0.0; 3], length: 20.0, width: 20.0 });
    let patch = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0, 0.0, 1.0], size: [10.0, 10.0, 0.0] });
    let cloth = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::SkinWrapPatchModifier);
    h.app.panel_tab = crate::Tab::Modify;
    framed(&mut h);
    h.click_widget("None");
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::ModifierNode));
    h.click(h.screen_of(Vec3::new(8.0, -8.0, 0.0)));
    let Modifier::SkinWrapPatch(d) = &h.app.state.scene.node(cloth).unwrap().modifiers[0].modifier else { panic!() };
    assert_eq!(d.patch, Some(patch), "Pick Patch takes the clicked patch");
    assert_eq!(d.binds.len(), 25);
    h.frames(1);
    let name = h.app.state.scene.node(patch).unwrap().name.clone();
    assert_eq!(h.widgets_labelled(&name).len(), 1, "the button shows the patch's name");
    let s = h.spinner_after("Sample Rate:");
    h.drag(s.center(), s.center() + egui::vec2(30.0, 0.0));
    let Modifier::SkinWrapPatch(d) = &h.app.state.scene.node(cloth).unwrap().modifiers[0].modifier else { panic!() };
    assert!(d.sample_rate > 5, "Sample Rate raised: {}", d.sample_rate);
    h.click_widget("Resample");
}
