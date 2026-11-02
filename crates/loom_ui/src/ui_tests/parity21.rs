//! Settings and dialogs added by the parity21 round, clicked and typed as a
//! person does.

use super::*;
use egui::accesskit::Role;

/// Types `text` into the spinner after the label `label` (click, select all, type, Enter).
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

#[test]
fn bridge_polygons_dialog_taper_narrows_the_middle_ring() {
    let mut h = Harness::new();
    poly_box(&mut h);
    let id = *h.app.state.selection.iter().next().unwrap();
    h.key(egui::Key::Num4);
    let m = cage(&h, id);
    let cap = |up: bool| (0..m.face_count() as u32).find(|f| (m.face_normal(loom_mesh::FaceId(*f)).z > 0.9) == up && m.face_normal(loom_mesh::FaceId(*f)).z.abs() > 0.9).unwrap();
    h.cmd(Command::SelectSub { node: Some(id.0), ids: vec![cap(true), cap(false)], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::BridgeSettings);
    h.frames(2);
    assert!(h.has(Role::Label, "Taper:") && h.has(Role::Label, "Bias:"), "Bridge Polygons shows Taper and Bias");
    type_after(&mut h, "Segments:", "2");
    type_after(&mut h, "Taper:", "-0.5");
    h.click(h.find(Role::Button, "OK").center());
    h.frames(2);
    assert_eq!(h.app.state.poly_settings.bridge_taper, -0.5);
    assert_eq!(h.app.state.poly_settings.bridge_bias, 0.0, "Bias default 0.0");
    // The 30-unit box: the ring halfway along the bridge is half as wide.
    let m = cage(&h, id);
    let (lo, hi) = (0..m.vertex_count() as u32).filter_map(|v| m.vertex_position(loom_mesh::VertexId(v))).fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p.z), a.1.max(p.z)));
    let mid = (lo + hi) / 2.0;
    let cx = (0..m.vertex_count() as u32).filter_map(|v| m.vertex_position(loom_mesh::VertexId(v))).map(|p| p.x).sum::<f32>() / m.vertex_count() as f32;
    let ring: Vec<f32> = (0..m.vertex_count() as u32).filter_map(|v| m.vertex_position(loom_mesh::VertexId(v))).filter(|p| (p.z - mid).abs() < 1e-3).map(|p| (p.x - cx).abs()).collect();
    assert_eq!(ring.len(), 4, "one middle ring");
    assert!(ring.iter().all(|w| (w - 7.5).abs() < 0.05), "middle ring half of the 15-unit half width: {ring:?}");
}

/// The checkbox on the row of the label `label`.
fn checkbox_after(h: &Harness, label: &str) -> egui::Rect {
    let l = h.find(Role::Label, label);
    h.widgets
        .iter()
        .filter(|(r, _, w)| *r == Role::CheckBox && w.min.x >= l.max.x - 1.0 && (w.center().y - l.center().y).abs() < l.height())
        .min_by(|a, b| a.2.min.x.total_cmp(&b.2.min.x))
        .map(|w| w.2)
        .unwrap_or_else(|| panic!("no checkbox after \"{label}\""))
}

/// Edges the wireframe draws on `id`'s stack result.
fn drawn_edges(h: &Harness, id: loom_scene::NodeId) -> (usize, usize) {
    let m = &h.app.state.scene.evaluated(id, None).value.mesh;
    let all: Vec<loom_mesh::HalfEdgeId> = (0..m.half_edges().len() as u32).map(loom_mesh::HalfEdgeId).filter(|e| m.canonical_edge_id(*e) == Some(*e)).collect();
    (all.len(), all.iter().filter(|e| m.edge_visible(**e)).count())
}

#[test]
fn meshsmooth_isoline_display_starts_on_and_shows_only_the_original_edges() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] });
    let id = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::AddModifier { name: "MeshSmooth".into() });
    h.cmd(Command::SetModifierParameter { index: 0, name: "Iterations".into(), value: 1.0 });
    h.click(h.find(Role::Button, "Modify").center());
    h.frames(2);
    // A 1x1x1 box: 12 edges, smoothed once 48, of which the 24 halves of the box edges are drawn.
    assert_eq!(drawn_edges(&h, id), (48, 24), "Isoline Display default on");
    h.click(checkbox_after(&h, "Isoline Display").center());
    h.frames(2);
    assert_eq!(drawn_edges(&h, id), (48, 48), "off: every subdivided edge is drawn");
}

/// Smoothing groups of the rim (vertical) polygons of `id`'s stack result.
fn rim_groups(h: &Harness, id: loom_scene::NodeId) -> Vec<u32> {
    let m = &h.app.state.scene.evaluated(id, None).value.mesh;
    (0..m.face_count() as u32).map(loom_mesh::FaceId).filter(|f| m.face_normal(*f).z.abs() < 0.5).map(|f| m.faces()[f.0 as usize].smoothing_groups).collect()
}

#[test]
fn shell_smooth_group_edge_mapping_and_bevel_spline() {
    let mut h = Harness::new();
    // Tall, so the whole Shell rollout fits.
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 2000.0));
    h.cmd(Command::CreateShape { kind: loom_commands::ShapeKind::Arc, position: [200.0, 0.0, 0.0], size: [20.0, 20.0, 0.0] });
    let arc = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [30.0, 30.0, 0.0] });
    let id = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::AddModifier { name: "Shell".into() });
    h.click(h.find(Role::Button, "Modify").center());
    h.frames(3);
    for l in ["Bevel Edges", "Override Smooth Group", "Smooth Grp", "Edge Mapping", "TV Offset", "Select Edges", "Select Inner Faces", "Select Outer Faces"] {
        assert!(h.has(Role::Label, l), "Shell shows {l}");
    }
    let Some(loom_scene::Modifier::Shell { tv_offset, edge_mapping, override_smooth, smooth_group, .. }) = h.app.state.scene.node(id).map(|n| n.modifiers[0].modifier.clone()) else { panic!() };
    assert_eq!((tv_offset.get(), edge_mapping.get(), override_smooth.get(), smooth_group.get()), (0.05, 0, false, 0), "page defaults (Edge Mapping: Copy)");
    assert!(!rim_groups(&h, id).is_empty());
    // Auto Smooth Edge off, Override Smooth Group off: the rim gets group 31.
    h.click(checkbox_after(&h, "Auto Smooth Edge").center());
    h.frames(2);
    assert!(rim_groups(&h, id).iter().all(|g| *g == 1 << 30), "{:?}", rim_groups(&h, id));
    // Override Smooth Group with Smooth Grp 0: no group.
    h.click(checkbox_after(&h, "Override Smooth Group").center());
    h.frames(2);
    assert!(rim_groups(&h, id).iter().all(|g| *g == 0));
    type_after(&mut h, "Smooth Grp", "3");
    assert!(rim_groups(&h, id).iter().all(|g| *g == 1 << 2));
    // Pick Bevel Spline: the arc's points become the rim's rings.
    let rings_before = h.app.state.scene.evaluated(id, None).value.mesh.vertex_count();
    h.click_widget("Pick Bevel Spline");
    h.frames(1);
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::ModifierNode));
    h.cmd(Command::PickModifierNode { index: 0, node: arc.0 });
    h.frames(2);
    assert!(h.has(Role::Label, &format!("Bevel Spline: {}", h.app.state.scene.node(arc).unwrap().name)));
    let Some(loom_scene::Modifier::Shell { bevel_edges, .. }) = h.app.state.scene.node(id).map(|n| n.modifiers[0].modifier.clone()) else { panic!() };
    assert!(bevel_edges.get(), "picking the spline turns Bevel Edges on");
    assert!(h.app.state.scene.evaluated(id, None).value.mesh.vertex_count() > rings_before, "the arc profile adds rim rings");
    h.click(checkbox_after(&h, "Bevel Edges").center());
    h.frames(2);
    assert_eq!(h.app.state.scene.evaluated(id, None).value.mesh.vertex_count(), rings_before, "Bevel Edges off: Segments again");
}

#[test]
fn stitch_tool_align_and_scale_clusters_start_on_and_toggle() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] });
    h.cmd(Command::AddModifier { name: "Unwrap UVW".into() });
    h.cmd(Command::EditUvws);
    h.cmd(Command::StitchDialog);
    h.frames(3);
    assert!(h.app.state.uv_editor.stitch_align && h.app.state.uv_editor.stitch_scale, "both default on");
    h.click(h.find(Role::CheckBox, "Scale Clusters").center());
    h.frames(2);
    assert!(!h.app.state.uv_editor.stitch_scale);
    h.click(h.find(Role::CheckBox, "Scale Clusters").center());
    h.click(h.find(Role::CheckBox, "Align Clusters").center());
    h.frames(2);
    assert!(h.app.state.uv_editor.stitch_scale && !h.app.state.uv_editor.stitch_align);
    // Scale Clusters takes effect only with Align Clusters: greyed out now.
    h.click(h.find(Role::CheckBox, "Scale Clusters").center());
    h.frames(2);
    assert!(h.app.state.uv_editor.stitch_scale, "disabled while Align Clusters is off");
}

/// Widths of the blue seam lines painted last frame.
fn seam_widths(h: &Harness) -> Vec<f32> {
    fn walk(s: &egui::Shape, out: &mut Vec<f32>) {
        match s {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            egui::Shape::LineSegment { stroke, .. } if stroke.color == egui::Color32::from_rgb(60, 120, 255) => out.push(stroke.width),
            _ => {}
        }
    }
    let mut out = Vec::new();
    h.shapes.iter().for_each(|c| walk(&c.shape, &mut out));
    out
}

#[test]
fn thick_seams_widen_as_the_view_zooms_in_thin_seams_do_not() {
    let mut h = Harness::new();
    maximized(&mut h);
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] });
    let id = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::AddModifier { name: "Unwrap UVW".into() });
    h.cmd(Command::ZoomExtentsAll);
    if let Some(loom_scene::Modifier::UnwrapUVW(u)) = h.app.state.scene.node_mut(id).map(|n| &mut n.modifiers[0].modifier) {
        u.seams.insert((0, 1));
    }
    h.click(h.find(Role::Button, "Modify").center());
    h.frames(2);
    let far = seam_widths(&h);
    assert_eq!(far.len(), 1, "one seam drawn");
    // [ zooms in.
    h.key(egui::Key::OpenBracket);
    h.key(egui::Key::OpenBracket);
    h.frames(2);
    let near = seam_widths(&h);
    assert!(near[0] > far[0] + 0.5, "thick (default) seams widen as the view zooms in: {far:?} -> {near:?}");
    h.click(h.find(Role::RadioButton, "Thin Seam Display").center());
    h.frames(2);
    assert_eq!(seam_widths(&h), vec![1.5], "thin seams stay thin");
}
