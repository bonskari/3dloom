//! Editable Spline viewport behaviour: segment / spline picking (the knots
//! are the cage vertices, so vertex picking is the mesh path), selection
//! overlays, Bezier handle gizmos and the Refine / Insert clicks.

use crate::gizmo::Projection;
use crate::viewport_widget::Pick;
use crate::LoomApp;
use eframe::egui;
use glam::{Mat4, Vec3};
use loom_commands::{Command, PickMode, SelectionMode, SubObjectId};
use loom_mesh::{FaceId, HalfEdgeId, VertexId};
use loom_scene::NodeId;
use loom_shape::Shape;

/// Screen polyline of one segment of a shape (object → world → screen).
fn segment_screen(shape: &Shape, si: usize, seg: usize, world: Mat4, screen: &dyn Fn(Vec3) -> Option<egui::Pos2>) -> Vec<egui::Pos2> {
    let Some(b) = shape.splines[si].segment(seg) else { return Vec::new() };
    let n = if shape.optimize && loom_shape::interp::is_straight(&b) { 1 } else { shape.steps + 1 };
    (0..=n).filter_map(|i| screen(world.transform_point3(loom_shape::interp::point_at(&b, i as f32 / n as f32)))).collect()
}

fn dist_to_polyline(p: egui::Pos2, pts: &[egui::Pos2]) -> f32 {
    let mut best = f32::INFINITY;
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let ab = b - a;
        let t = if ab.length_sq() < 1e-6 { 0.0 } else { ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) };
        best = best.min((a + ab * t).distance(p));
    }
    best
}

fn shape_nodes(app: &LoomApp) -> Vec<(usize, NodeId, Shape, Mat4, std::collections::BTreeSet<usize>)> {
    app.world_meshes()
        .iter()
        .enumerate()
        .filter_map(|(i, (id, _))| {
            let n = app.state.scene.node(*id)?;
            if !app.state.selection.contains(id) {
                return None;
            }
            let so = n.edit_spline()?;
            Some((i, *id, so.shape.clone(), app.state.scene.object_matrix(*id), so.hidden.clone()))
        })
        .collect()
}

/// Nearest segment (Segment level) or its spline (Spline level) within 8 px.
pub(crate) fn pick_sub(app: &LoomApp, p: egui::Pos2, rect: egui::Rect, view_proj: Mat4) -> Option<Pick> {
    let mode = app.state.selection_mode;
    let screen = |w: Vec3| crate::viewport_widget::project_world_to_screen(w, view_proj, rect);
    let mut best: Option<(usize, SubObjectId, f32)> = None;
    for (mesh_i, _, shape, world, hidden) in shape_nodes(app) {
        for si in 0..shape.splines.len() {
            if hidden.contains(&si) {
                continue;
            }
            for seg in 0..shape.splines[si].segment_count() {
                let d = dist_to_polyline(p, &segment_screen(&shape, si, seg, world, &screen));
                if d <= 8.0 && best.as_ref().is_none_or(|(_, _, bd)| d < *bd) {
                    let id = match mode {
                        SelectionMode::Edge | SelectionMode::Border => SubObjectId::Edge(HalfEdgeId(shape.segment_index(si, seg) as u32)),
                        _ => SubObjectId::Face(FaceId(si as u32)),
                    };
                    best = Some((mesh_i, id, d));
                }
            }
        }
    }
    best.map(|(mesh, id, _)| Pick::SubObject { mesh, id })
}

/// Region selection at the Segment / Spline levels: a segment counts by its
/// polyline points, a spline by all of its segments.
pub(crate) fn region_subs(app: &LoomApp, region: &crate::region::Region, rect: egui::Rect, view_proj: Mat4, window: bool) -> Vec<Pick> {
    let mode = app.state.selection_mode;
    let screen = |w: Vec3| crate::viewport_widget::project_world_to_screen(w, view_proj, rect);
    let mut out = Vec::new();
    for (mesh_i, _, shape, world, hidden) in shape_nodes(app) {
        for si in 0..shape.splines.len() {
            if hidden.contains(&si) {
                continue;
            }
            let mut all = Vec::new();
            let mut any = false;
            for seg in 0..shape.splines[si].segment_count() {
                let pts = segment_screen(&shape, si, seg, world, &screen);
                let hit = region.selects(&pts, window);
                any |= hit;
                if hit && matches!(mode, SelectionMode::Edge | SelectionMode::Border) {
                    out.push(Pick::SubObject { mesh: mesh_i, id: SubObjectId::Edge(HalfEdgeId(shape.segment_index(si, seg) as u32)) });
                }
                all.extend(pts);
            }
            if matches!(mode, SelectionMode::Face | SelectionMode::Element) && (if window { region.selects(&all, true) } else { any }) {
                out.push(Pick::SubObject { mesh: mesh_i, id: SubObjectId::Face(FaceId(si as u32)) });
            }
        }
    }
    out
}

/// Selected segments / splines in orange, Bezier handles of selected knots
/// (vertex level) and the optional vertex numbers.
pub fn draw(painter: &egui::Painter, app: &LoomApp, projection: &Projection) {
    let orange = egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(255, 150, 40));
    let screen = |w: Vec3| projection.screen(w);
    let ss = app.state.spline_settings;
    for (_, id, shape, world, hidden) in shape_nodes(app) {
        let selected: Vec<SubObjectId> = app.state.sub_selection.iter().filter(|(n, _)| *n == id).map(|(_, s)| s).collect();
        for s in selected.iter().filter(|_| ss.show_selected_segs) {
            match s {
                SubObjectId::Edge(e) => {
                    if let Some((si, seg)) = shape.segment_at(e.0 as usize) {
                        let pts = segment_screen(&shape, si, seg, world, &screen);
                        for w in pts.windows(2) {
                            painter.line_segment([w[0], w[1]], orange);
                        }
                    }
                }
                SubObjectId::Face(f) => {
                    let si = f.0 as usize;
                    if si < shape.splines.len() {
                        for seg in 0..shape.splines[si].segment_count() {
                            let pts = segment_screen(&shape, si, seg, world, &screen);
                            for w in pts.windows(2) {
                                painter.line_segment([w[0], w[1]], orange);
                            }
                        }
                    }
                }
                SubObjectId::Vertex(_) => {}
            }
        }
        if app.state.selection_mode == SelectionMode::Vertex {
            // Knot ticks (white), Bezier handles of the selected knots (green with squares).
            let green = egui::Color32::from_rgb(90, 230, 90);
            for (si, s) in shape.splines.iter().enumerate() {
                if hidden.contains(&si) {
                    continue;
                }
                for (k, knot) in s.knots.iter().enumerate() {
                    let flat = shape.knot_index(si, k);
                    let Some(sp) = screen(world.transform_point3(knot.pos)) else { continue };
                    if ss.show_vertex_numbers {
                        painter.text(sp + egui::vec2(6.0, -6.0), egui::Align2::LEFT_BOTTOM, format!("{flat}"), egui::FontId::proportional(11.0), egui::Color32::WHITE);
                    }
                    let is_sel = selected.contains(&SubObjectId::Vertex(VertexId(flat as u32)));
                    if is_sel && matches!(knot.kind, loom_shape::KnotKind::Bezier | loom_shape::KnotKind::BezierCorner) {
                        for h in [knot.pos + knot.in_vec, knot.pos + knot.out_vec] {
                            if let Some(hp) = screen(world.transform_point3(h)) {
                                painter.line_segment([sp, hp], egui::Stroke::new(1.0_f32, green));
                                painter.rect_filled(egui::Rect::from_center_size(hp, egui::vec2(7.0, 7.0)), 0.0, green);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct HandleDrag {
    node: u64,
    knot: u32,
    side_in: bool,
}

/// Dragging a Bezier handle square moves that handle (Set Knot Handle).
/// Returns true while a handle drag owns the pointer.
pub fn handle_drag(app: &mut LoomApp, ctx: &egui::Context, pointer: &egui::PointerState, hovered: bool, p: egui::Pos2, projection: &Projection) -> bool {
    if app.state.selection_mode != SelectionMode::Vertex {
        return false;
    }
    let id = egui::Id::new("spline_handle_drag");
    let mut st: Option<HandleDrag> = ctx.data(|d| d.get_temp(id));
    if st.is_none() && pointer.button_pressed(egui::PointerButton::Primary) && hovered {
        // Which handle square is under the pointer?
        let mut hit: Option<HandleDrag> = None;
        for (_, nid, shape, world, _) in shape_nodes(app) {
            let selected: Vec<u32> = app.state.sub_selection.iter().filter(|(n, _)| *n == nid).filter_map(|(_, s)| match s { SubObjectId::Vertex(v) => Some(v.0), _ => None }).collect();
            for flat in selected {
                let Some((si, k)) = shape.knot_at(flat as usize) else { continue };
                let knot = &shape.splines[si].knots[k];
                if !matches!(knot.kind, loom_shape::KnotKind::Bezier | loom_shape::KnotKind::BezierCorner) {
                    continue;
                }
                for (side_in, h) in [(true, knot.pos + knot.in_vec), (false, knot.pos + knot.out_vec)] {
                    if projection.screen(world.transform_point3(h)).is_some_and(|hp| hp.distance(p) <= 6.0) {
                        hit = Some(HandleDrag { node: nid.0, knot: flat, side_in });
                    }
                }
            }
        }
        if let Some(h) = hit {
            app.state.undo.begin("Move Handle");
            app.poly_dragging = true;
            ctx.data_mut(|d| d.insert_temp(id, h));
            st = Some(h);
        }
    }
    let Some(h) = st else { return false };
    if pointer.is_moving() && pointer.button_down(egui::PointerButton::Primary) {
        // The handle moves on the view plane through the knot.
        let node = NodeId(h.node);
        let Some(shape) = app.state.scene.node(node).and_then(|n| n.edit_shape()) else { return true };
        let Some((si, k)) = shape.knot_at(h.knot as usize) else { return true };
        let world = app.state.scene.object_matrix(node);
        let knot_world = world.transform_point3(shape.splines[si].knots[k].pos);
        let (o, d) = projection.ray(p);
        let n = projection.view.inverse().z_axis.truncate().normalize_or_zero();
        let denom = d.dot(n);
        if denom.abs() > 1e-6 {
            let t = (knot_world - o).dot(n) / denom;
            let target = o + d * t;
            let _ = app.dispatch(Command::SetKnotHandle { node: h.node, knot: h.knot, side_in: h.side_in, point: target.to_array() });
            ctx.request_repaint();
        }
    }
    if pointer.button_released(egui::PointerButton::Primary) {
        app.state.undo.end();
        app.poly_dragging = false;
        ctx.data_mut(|d| d.remove::<HandleDrag>(id));
    }
    true
}

#[derive(Clone)]
struct FilletDrag {
    start: egui::Pos2,
    upp: f32,
    sel: Vec<(u64, SubObjectId)>,
    /// Undo version right after the last applied step (to take it back on the next move).
    applied: Option<u64>,
}

/// Fillet / Chamfer mode: pressing on a selected vertex and dragging up
/// rounds (cuts) every selected vertex by the drag, live. Returns true while the mode owns the pointer.
pub fn fillet_drag(app: &mut LoomApp, ctx: &egui::Context, pointer: &egui::PointerState, hovered: bool, p: egui::Pos2, projection: &Projection) -> bool {
    let chamfer = match app.state.pick_mode {
        Some(PickMode::SplineFillet) => false,
        Some(PickMode::SplineChamfer) => true,
        _ => return false,
    };
    if app.state.selection_mode != SelectionMode::Vertex {
        return false;
    }
    let id = egui::Id::new("spline_fillet_drag");
    let mut st: Option<FilletDrag> = ctx.data(|d| d.get_temp(id));
    if st.is_none() && pointer.button_pressed(egui::PointerButton::Primary) && hovered {
        // The press must land on a selected vertex.
        let mut at: Option<Vec3> = None;
        for (_, nid, shape, world, _) in shape_nodes(app) {
            for (n, so) in app.state.sub_selection.iter() {
                let SubObjectId::Vertex(v) = so else { continue };
                let Some((si, k)) = shape.knot_at(v.0 as usize).filter(|_| n == nid) else { continue };
                let w = world.transform_point3(shape.splines[si].knots[k].pos);
                if projection.screen(w).is_some_and(|sp| sp.distance(p) <= 8.0) {
                    at = Some(w);
                }
            }
        }
        if let Some(w) = at {
            let sel = app.state.sub_selection.iter().map(|(n, s)| (n.0, s)).collect();
            st = Some(FilletDrag { start: p, upp: projection.units_per_pixel(w).max(1e-6), sel, applied: None });
            app.poly_dragging = true;
        }
    }
    // Elsewhere a click selects vertices as usual.
    let Some(mut s) = st else { return false };
    if pointer.is_moving() && pointer.button_down(egui::PointerButton::Primary) {
        // Each move redoes the operation from the original vertices, so the
        // drag leaves one undo entry.
        if s.applied.is_some_and(|v| v == app.state.undo.version()) {
            let _ = app.dispatch(Command::UndoSceneOperation);
        }
        app.state.sub_selection.clear();
        for (n, so) in &s.sel {
            app.state.sub_selection.insert(NodeId(*n), *so);
        }
        s.applied = None;
        let amount = (s.start.y - p.y) * s.upp;
        if amount > 1e-4 {
            let before = app.state.undo.version();
            let _ = app.dispatch(if chamfer { Command::SplineChamfer { amount } } else { Command::SplineFillet { amount } });
            s.applied = (app.state.undo.version() != before).then(|| app.state.undo.version());
        }
        ctx.request_repaint();
    }
    if pointer.button_released(egui::PointerButton::Primary) || !pointer.button_down(egui::PointerButton::Primary) {
        app.poly_dragging = false;
        ctx.data_mut(|d| d.remove::<FilletDrag>(id));
    } else {
        ctx.data_mut(|d| d.insert_temp(id, s));
    }
    true
}

pub fn is_spline_pick(mode: Option<PickMode>) -> bool {
    matches!(mode, Some(PickMode::SplineRefine | PickMode::SplineInsert | PickMode::SplineTrim | PickMode::SplineExtend | PickMode::SplineCrossInsert | PickMode::SplineBind | PickMode::TrimExtendModifier))
}

/// Refine / Insert click: the nearest segment gets a knot at the nearest
/// parameter; Insert leaves the new knot selected at the Vertex level.
pub fn click(app: &mut LoomApp, p: egui::Pos2, rect: egui::Rect, view_proj: Mat4) {
    if app.state.pick_mode == Some(PickMode::TrimExtendModifier) {
        // The modifier's shape is the stack's, not an Editable Spline: the click
        // lands on the shape's own plane (its local XY).
        let Some(id) = app.state.selection.iter().next().copied() else { return };
        let m = app.state.scene.object_matrix(id);
        let ndc = |z: f32| Vec3::new((p.x - rect.left()) / rect.width() * 2.0 - 1.0, 1.0 - (p.y - rect.top()) / rect.height() * 2.0, z);
        let inv = view_proj.inverse();
        let (a, b) = (inv.project_point3(ndc(0.0)), inv.project_point3(ndc(1.0)));
        let (o, n) = (m.w_axis.truncate(), m.z_axis.truncate().normalize_or_zero());
        let d = b - a;
        let den = d.dot(n);
        if den.abs() > 1e-9 {
            let hit = a + d * ((o - a).dot(n) / den);
            let _ = app.dispatch(Command::PickPoint { point: hit.to_array() });
        }
        return;
    }
    let screen = |w: Vec3| crate::viewport_widget::project_world_to_screen(w, view_proj, rect);
    let mut best: Option<(NodeId, usize, f32, f32, Vec3)> = None;
    for (_, id, shape, world, hidden) in shape_nodes(app) {
        for si in 0..shape.splines.len() {
            if hidden.contains(&si) {
                continue;
            }
            for seg in 0..shape.splines[si].segment_count() {
                let pts = segment_screen(&shape, si, seg, world, &screen);
                let d = dist_to_polyline(p, &pts);
                if d <= 8.0 && best.as_ref().is_none_or(|(_, _, _, bd, _)| d < *bd) {
                    // Nearest parameter along the segment, by its sampled points.
                    let n = (pts.len() - 1).max(1);
                    let (mut bt, mut bdist) = (0.0, f32::INFINITY);
                    for (i, w) in pts.windows(2).enumerate() {
                        let (a, b) = (w[0], w[1]);
                        let ab = b - a;
                        let t = if ab.length_sq() < 1e-6 { 0.0 } else { ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) };
                        let dd = (a + ab * t).distance(p);
                        if dd < bdist {
                            bdist = dd;
                            bt = (i as f32 + t) / n as f32;
                        }
                    }
                    let on = shape.splines[si].segment(seg).map(|b| world.transform_point3(loom_shape::interp::point_at(&b, bt))).unwrap_or_default();
                    best = Some((id, shape.segment_index(si, seg), bt, d, on));
                }
            }
        }
    }
    let Some((id, seg, t, _, on)) = best else { return };
    if matches!(app.state.pick_mode, Some(PickMode::SplineTrim | PickMode::SplineExtend | PickMode::SplineCrossInsert | PickMode::SplineBind)) {
        // These tools take the clicked point on the spline; they stay on for more clicks.
        let _ = app.dispatch(Command::PickPoint { point: on.to_array() });
        return;
    }
    if app.state.pick_mode == Some(PickMode::SplineRefine) && app.state.spline_settings.refine_connect {
        // Refine with Connect on an existing vertex asks Refine / Connect Only.
        let knot = shape_nodes(app).into_iter().filter(|(_, n, ..)| *n == id).find_map(|(_, _, shape, world, hidden)| {
            (0..shape.splines.len()).filter(|si| !hidden.contains(si)).flat_map(|si| (0..shape.splines[si].knots.len()).map(move |k| (si, k))).find_map(|(si, k)| {
                let sp = screen(world.transform_point3(shape.splines[si].knots[k].pos))?;
                (sp.distance(p) <= 6.0).then(|| shape.knot_index(si, k))
            })
        });
        if let Some(knot) = knot {
            let _ = app.dispatch(Command::SplineRefineAtVertex { node: id.0, knot: knot as u32, segment: seg as u32, t });
            return;
        }
    }
    let insert = app.state.pick_mode == Some(PickMode::SplineInsert);
    if insert && app.state.selection_mode != SelectionMode::Vertex {
        let _ = app.dispatch(Command::SelectionMode { mode: SelectionMode::Vertex });
    }
    let _ = app.dispatch(Command::SplineRefine { node: id.0, segment: seg as u32, t });
    if !insert {
        // Refine stays on for more clicks; Insert hands the knot to the Move tool.
    } else {
        let _ = app.dispatch(Command::SelectAndMove);
    }
}
