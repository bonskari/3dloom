//! Viewport side of the Editable Poly modes: Extrude / Chamfer drags,
//! Target Weld, Insert Vertex, Edit Triangulation and Create picks, and
//! their rubber bands.

use crate::gizmo::Projection;
use crate::viewport_widget::{creation_anchor, pick_edge, pick_face, pick_vertex, surface_hit_node, Pick};
use crate::LoomApp;
use eframe::egui;
use glam::{Mat4, Vec3};
use loom_commands::{Command, Dialog, PickMode, PolyDrag, PolySettingsKind, SelectionMode, SubObjectId};
use loom_scene::NodeId;

pub fn is_poly_pick(mode: Option<PickMode>) -> bool {
    matches!(
        mode,
        Some(PickMode::TargetWeldSource | PickMode::TargetWeldTarget | PickMode::InsertVertex | PickMode::EditTriangulationA | PickMode::EditTriangulationB | PickMode::CreatePolygon | PickMode::CreateVertex
            | PickMode::QuickSliceA | PickMode::QuickSliceB | PickMode::CutA | PickMode::CutB
            | PickMode::SwiftLoop | PickMode::BridgeEdge1 | PickMode::BridgeEdge2 | PickMode::HingeEdge)
    )
}

/// A click while a poly pick mode is on.
pub fn click(app: &mut LoomApp, index: usize, p: egui::Pos2, rect: egui::Rect, view: Mat4, proj: Mat4) {
    let view_proj = proj * view;
    match app.state.pick_mode {
        Some(PickMode::TargetWeldSource | PickMode::TargetWeldTarget | PickMode::EditTriangulationA | PickMode::EditTriangulationB | PickMode::CreatePolygon) => {
            if let Some(Pick::SubObject { mesh, id: SubObjectId::Vertex(v) }) = pick_vertex(app, p, rect, view_proj) {
                let node = app.world_meshes()[mesh].0;
                let _ = app.dispatch(Command::PickSub { node: node.0, id: v.0 });
            }
        }
        Some(PickMode::InsertVertex) => {
            if let Some(Pick::SubObject { mesh, id: SubObjectId::Edge(e) }) = pick_edge(app, p, rect, view_proj) {
                let (node, m) = &app.world_meshes()[mesh];
                let Some((a, b)) = m.edge_vertices(e) else { return };
                let (Some(pa), Some(pb)) = (m.vertex_position(a), m.vertex_position(b)) else { return };
                let pr = Projection::new(view, proj, rect);
                let t = match (pr.screen(pa), pr.screen(pb)) {
                    (Some(sa), Some(sb)) => {
                        let ab = sb - sa;
                        if ab.length_sq() < 1e-6 { 0.5 } else { ((p - sa).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) }
                    }
                    _ => 0.5,
                };
                let _ = app.dispatch(Command::InsertVertexOnEdge { node: node.0, edge: e.0, t });
            } else if let Some(Pick::SubObject { mesh, id: SubObjectId::Face(f) }) = pick_face(app, p, rect, view, proj) {
                let node = app.world_meshes()[mesh].0;
                if let Some((_, point, _)) = surface_hit_node(app, p, rect, view, proj) {
                    let _ = app.dispatch(Command::InsertVertexInFace { node: node.0, face: f.0, point: point.to_array() });
                }
            }
        }
        Some(PickMode::QuickSliceA | PickMode::QuickSliceB | PickMode::CutA | PickMode::CutB) => {
            if let Some(Pick::SubObject { mesh, id: SubObjectId::Face(f) }) = pick_face(app, p, rect, view, proj) {
                let node = app.world_meshes()[mesh].0;
                if let Some((_, point, _)) = surface_hit_node(app, p, rect, view, proj) {
                    let _ = app.dispatch(Command::PickFacePoint { node: node.0, face: f.0, point: point.to_array() });
                }
            } else if matches!(app.state.pick_mode, Some(PickMode::QuickSliceA | PickMode::QuickSliceB)) {
                // QuickSlice also takes points off the mesh (the construction plane).
                if let Some((pt, _)) = creation_anchor(app, index, p, rect, view, proj) {
                    let _ = app.dispatch(Command::PickFacePoint { node: app.state.selection.iter().next().map(|n| n.0).unwrap_or(0), face: u32::MAX, point: pt.to_array() });
                }
            }
        }
        Some(PickMode::BridgeEdge1 | PickMode::BridgeEdge2 | PickMode::HingeEdge) => {
            if let Some(Pick::SubObject { mesh, id: SubObjectId::Edge(e) }) = pick_edge(app, p, rect, view_proj) {
                let node = app.world_meshes()[mesh].0;
                let _ = app.dispatch(Command::PickSub { node: node.0, id: e.0 });
            }
        }
        Some(PickMode::SwiftLoop) => {
            if let Some((node, edge, t)) = swift_loop_target(app, p, &Projection::new(view, proj, rect)) {
                let _ = app.dispatch(Command::SwiftLoop { node: node.0, edge: edge.0, t });
            }
        }
        Some(PickMode::CreateVertex) => {
            let Some(node) = app.state.selection.iter().next().copied() else { return };
            let point = surface_hit_node(app, p, rect, view, proj).map(|h| h.1).or_else(|| creation_anchor(app, index, p, rect, view, proj).map(|(a, _)| a));
            if let Some(pt) = point {
                let _ = app.dispatch(Command::CreateVertex { node: node.0, point: pt.to_array() });
            }
        }
        _ => {}
    }
}

/// Swift Loop under the pointer: the edge of the polygon there nearest the
/// pointer on screen (or the nearest edge when no polygon is hit), and the
/// pointer's place along it from its canonical origin.
fn swift_loop_target(app: &LoomApp, p: egui::Pos2, pr: &Projection) -> Option<(NodeId, loom_mesh::HalfEdgeId, f32)> {
    let along = |sa: egui::Pos2, sb: egui::Pos2| -> (f32, f32) {
        let ab = sb - sa;
        let t = if ab.length_sq() < 1e-6 { 0.5 } else { ((p - sa).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) };
        ((sa + ab * t).distance(p), t)
    };
    let (mesh_i, edge) = match pick_face(app, p, pr.rect, pr.view, pr.proj) {
        Some(Pick::SubObject { mesh, id: SubObjectId::Face(f) }) => {
            let m = &app.world_meshes()[mesh].1;
            let best = m
                .face_corners(f)?
                .into_iter()
                .filter_map(|h| {
                    let (a, b) = m.edge_vertices(h)?;
                    let (sa, sb) = (pr.screen(m.vertex_position(a)?)?, pr.screen(m.vertex_position(b)?)?);
                    Some((along(sa, sb).0, h))
                })
                .min_by(|x, y| x.0.total_cmp(&y.0))?;
            (mesh, m.canonical_edge_id(best.1)?)
        }
        _ => match pick_edge(app, p, pr.rect, pr.proj * pr.view) {
            Some(Pick::SubObject { mesh, id: SubObjectId::Edge(e) }) => (mesh, e),
            _ => return None,
        },
    };
    let (node, m) = &app.world_meshes()[mesh_i];
    let (a, b) = m.edge_vertices(edge)?;
    let (sa, sb) = (pr.screen(m.vertex_position(a)?)?, pr.screen(m.vertex_position(b)?)?);
    Some((*node, edge, along(sa, sb).1))
}

#[derive(Clone, Copy)]
struct DragState {
    start: egui::Pos2,
    upp: f32,
    sel: usize,
}

/// Extrude / Chamfer mode drags: press on the viewport with a sub-object
/// selection, drag to set the value, release to keep it. Returns true while
/// the mode owns the pointer.
pub fn drag(app: &mut LoomApp, ctx: &egui::Context, pointer: &egui::PointerState, hovered: bool, p: egui::Pos2, projection: &Projection) -> bool {
    let Some(mode) = app.state.poly_drag else { return false };
    if app.state.selection_mode == SelectionMode::Object {
        return false;
    }
    let id = egui::Id::new("poly_drag_state");
    let sel_id = id.with("selection");
    let kind = match mode {
        PolyDrag::Extrude => PolySettingsKind::Extrude,
        PolyDrag::Chamfer => PolySettingsKind::Chamfer,
        PolyDrag::Inset => PolySettingsKind::Inset,
        PolyDrag::Bevel => PolySettingsKind::Bevel,
        PolyDrag::Outline => PolySettingsKind::Outline,
        PolyDrag::Hinge => PolySettingsKind::Hinge,
    };
    // Hinge From Edge: without a hinge the first click picks the edge.
    if pointer.button_released(egui::PointerButton::Primary) {
        app.poly_dragging = false;
    }
    if mode == PolyDrag::Hinge && app.state.poly_settings.hinge_edge.is_none() {
        if pointer.button_pressed(egui::PointerButton::Primary) && hovered {
            app.poly_dragging = true;
            if let Some(Pick::SubObject { mesh, id: SubObjectId::Edge(e) }) = pick_edge(app, p, projection.rect, projection.proj * projection.view) {
                let node = app.world_meshes()[mesh].0;
                app.state.poly_settings.hinge_edge = Some((node, e.0));
            }
        }
        return true;
    }
    let dialog = Dialog::PolySettings(kind);
    let mut st: Option<DragState> = ctx.data(|d| d.get_temp(id));
    if st.is_none() && pointer.button_pressed(egui::PointerButton::Primary) && hovered && !app.state.sub_selection.is_empty() {
        app.poly_dragging = true;
        let centre = selection_centre(app);
        let sel: Vec<(u64, SubObjectId)> = app.state.sub_selection.iter().map(|(n, s)| (n.0, s)).collect();
        st = Some(DragState { start: p, upp: projection.units_per_pixel(centre).max(1e-6), sel: sel.len() });
        ctx.data_mut(|d| {
            d.insert_temp(id, st.unwrap());
            d.insert_temp(sel_id, sel);
        });
    }
    let Some(s) = st else { return false };
    let sel: Vec<(u64, SubObjectId)> = ctx.data(|d| d.get_temp(sel_id)).unwrap_or_default();
    let delta = s.start - p;
    let ps = app.state.poly_settings;
    let cmd = match mode {
        // Up = positive height.
        PolyDrag::Extrude => Command::Extrude { height: delta.y * s.upp, width: ps.extrude_width, kind: ps.extrude_kind },
        PolyDrag::Chamfer => Command::Chamfer { amount: delta.length() * s.upp, segments: ps.chamfer_segments, open: ps.chamfer_open },
        PolyDrag::Inset => Command::Inset { amount: delta.length() * s.upp, by_polygon: ps.inset_by_polygon },
        // Bevel: up/down = height, sideways = outline (a two-stage drag folded into one).
        PolyDrag::Bevel => Command::Bevel { height: delta.y * s.upp, outline: -delta.x * s.upp, kind: ps.bevel_kind },
        PolyDrag::Outline => Command::Outline { amount: delta.y * s.upp },
        // Half a degree per pixel.
        PolyDrag::Hinge => Command::HingeFromEdge { angle: delta.y * 0.5, segments: ps.hinge_segments, edge: None },
    };
    if pointer.delta() != egui::Vec2::ZERO && s.sel > 0 {
        app.preview_undo();
        app.preview_apply(dialog, |app| {
            app.state.sub_selection.clear();
            for (n, so) in &sel {
                app.state.sub_selection.insert(NodeId(*n), *so);
            }
            let _ = app.dispatch(cmd.clone());
        });
        ctx.request_repaint();
    }
    if pointer.button_released(egui::PointerButton::Primary) || !pointer.button_down(egui::PointerButton::Primary) {
        app.preview_keep();
        ctx.data_mut(|d| {
            d.remove::<DragState>(id);
            d.remove::<Vec<(u64, SubObjectId)>>(sel_id);
        });
    }
    true
}

/// A Branches drag in progress: the polygons it grows from and the path so far.
#[derive(Clone)]
struct BranchDrag {
    node: u64,
    faces: Vec<u32>,
    /// World points on the plane facing the view through the polygons' centre.
    path: Vec<Vec3>,
    normal: Vec3,
    /// Where the last path point was taken, on screen.
    last: egui::Pos2,
}

/// Freeform > PolyDraw > Branches: a drag from a polygon (Shift: from the
/// selected polygons) draws the branch path on a plane facing the view, one
/// point per Minimum Distance pixels; the release grows the branch (one
/// undo). At Polygon level Ctrl+click selects a polygon, Ctrl+Alt+click adds
/// one. Returns true while Branches owns the pointer.
pub fn branches_drag(app: &mut LoomApp, ctx: &egui::Context, pointer: &egui::PointerState, hovered: bool, p: egui::Pos2, projection: &Projection) -> bool {
    if !app.state.branches {
        return false;
    }
    let id = egui::Id::new("branch_drag");
    let mods = ctx.input(|i| i.modifiers);
    if pointer.button_pressed(egui::PointerButton::Primary) && hovered {
        app.poly_dragging = true;
        let hit = pick_face(app, p, projection.rect, projection.view, projection.proj).and_then(|pick| match pick {
            Pick::SubObject { mesh, id: SubObjectId::Face(f) } => Some((app.world_meshes()[mesh].0, f.0)),
            _ => None,
        });
        if mods.ctrl || mods.command {
            if let (Some((node, f)), SelectionMode::Face) = (hit, app.state.selection_mode) {
                let op = if mods.alt { loom_commands::SelectOp::Add } else { loom_commands::SelectOp::Replace };
                let _ = app.dispatch(Command::SelectSub { node: Some(node.0), ids: vec![f], op });
            }
            return true;
        }
        let from = if mods.shift {
            app.state.sub_selection.by_node().into_iter().find_map(|(n, ids)| {
                let faces: Vec<u32> = ids.iter().filter_map(|s| if let SubObjectId::Face(f) = s { Some(f.0) } else { None }).collect();
                (!faces.is_empty()).then_some((n, faces))
            })
        } else {
            hit.map(|(n, f)| (n, vec![f]))
        };
        if let Some((node, faces)) = from {
            let Some((_, mesh)) = app.world_meshes().iter().find(|(n, _)| *n == node) else { return true };
            let pts: Vec<Vec3> = faces.iter().flat_map(|f| mesh.face_positions(loom_mesh::FaceId(*f)).unwrap_or_default()).collect();
            if !pts.is_empty() {
                let start = pts.iter().copied().sum::<Vec3>() / pts.len() as f32;
                let drag = BranchDrag { node: node.0, faces, path: vec![start], normal: projection.toward_viewer_at(start), last: p };
                ctx.data_mut(|d| d.insert_temp(id, drag));
            }
        }
        return true;
    }
    let Some(mut drag) = ctx.data(|d| d.get_temp::<BranchDrag>(id)) else {
        // The release of a press Branches took (a Ctrl+click) is its own too.
        let owned = app.poly_dragging;
        if pointer.button_released(egui::PointerButton::Primary) {
            app.poly_dragging = false;
        }
        return owned;
    };
    if pointer.button_down(egui::PointerButton::Primary) && p.distance(drag.last) >= app.state.poly_settings.branch_min_distance {
        if let Some(w) = projection.hit_plane(p, drag.path[0], drag.normal) {
            drag.path.push(w);
            drag.last = p;
            ctx.data_mut(|d| d.insert_temp(id, drag.clone()));
            ctx.request_repaint();
        }
    }
    if pointer.button_released(egui::PointerButton::Primary) || !pointer.button_down(egui::PointerButton::Primary) {
        ctx.data_mut(|d| d.remove::<BranchDrag>(id));
        app.poly_dragging = false;
        if drag.path.len() >= 2 {
            let cmd = Command::Branch { node: drag.node, faces: drag.faces, path: drag.path.iter().map(|v| v.to_array()).collect(), taper: app.state.poly_settings.branch_taper };
            if let Err(e) = app.dispatch(cmd) {
                app.last_error = Some(e);
            }
        }
    }
    true
}

/// The Branches path being drawn, as a polyline.
fn draw_branch_path(painter: &egui::Painter, projection: &Projection) {
    let Some(drag) = painter.ctx().data(|d| d.get_temp::<BranchDrag>(egui::Id::new("branch_drag"))) else { return };
    let pts: Vec<egui::Pos2> = drag.path.iter().filter_map(|w| projection.screen(*w)).collect();
    painter.add(egui::Shape::line(pts, egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(80, 220, 80))));
}

fn selection_centre(app: &LoomApp) -> Vec3 {
    let mut sum = Vec3::ZERO;
    let mut n = 0.0;
    for (node, ids) in app.state.sub_selection.by_node() {
        let Some((_, mesh)) = app.world_meshes().iter().find(|(id, _)| *id == node) else { continue };
        for so in ids {
            let vs: Vec<loom_mesh::VertexId> = match so {
                SubObjectId::Vertex(v) => vec![*v],
                SubObjectId::Edge(e) => mesh.edge_vertices(*e).map(|(a, b)| vec![a, b]).unwrap_or_default(),
                SubObjectId::Face(f) => mesh.face_vertex_ids(*f).unwrap_or_default(),
            };
            for v in vs {
                if let Some(p) = mesh.vertex_position(v) {
                    sum += p;
                    n += 1.0;
                }
            }
        }
    }
    if n > 0.0 { sum / n } else { Vec3::ZERO }
}

fn vertex_screen(app: &LoomApp, projection: &Projection, node: NodeId, v: u32) -> Option<egui::Pos2> {
    let (_, mesh) = app.world_meshes().iter().find(|(id, _)| *id == node)?;
    projection.screen(mesh.vertex_position(loom_mesh::VertexId(v))?)
}

/// Rubber bands: Target Weld / Edit Tri from the first pick to the pointer,
/// Create's polyline through the picked vertices.
pub fn draw(painter: &egui::Painter, app: &LoomApp, projection: &Projection, hover: Option<egui::Pos2>) {
    let stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(255, 255, 255));
    draw_branch_path(painter, projection);
    draw_skin(painter, app, projection);
    draw_normals(painter, app, projection);
    draw_hsds_cage(painter, app, projection);
    draw_vertex_paint_faces(painter, app, projection);
    // Modifier gizmo: the input's bounding box in the gizmo frame, orange.
    for id in app.state.selection.iter() {
        let Some(n) = app.state.scene.node(*id) else { continue };
        let Some(a) = n.active_entry else { continue };
        let Some(slot) = n.modifiers.get(a) else { continue };
        if let loom_scene::Modifier::FFD { kind, dims, points, gizmo, .. } = &slot.modifier {
            // FFD: the lattice in orange, control points green, selected ones red.
            let world = app.state.scene.object_matrix(*id) * gizmo.matrix();
            let pts: Vec<Option<egui::Pos2>> = points.iter().map(|p| projection.screen(world.transform_point3(*p))).collect();
            let col = egui::Color32::from_rgb(255, 160, 40);
            for (a, b) in loom_mesh::ffd::lattice_edges(*dims, *kind == 4) {
                if let (Some(p), Some(q)) = (pts[a], pts[b]) {
                    painter.line_segment([p, q], egui::Stroke::new(1.0_f32, col));
                }
            }
            if slot.sub_level == 1 || slot.sub_level == 3 {
                for (i, p) in pts.iter().enumerate() {
                    if let Some(p) = p {
                        let sel = app.state.ffd_selection.contains(&(i as u32));
                        let c = if sel { egui::Color32::from_rgb(255, 40, 40) } else { egui::Color32::from_rgb(60, 220, 60) };
                        painter.rect_filled(egui::Rect::from_center_size(*p, egui::vec2(6.0, 6.0)), 0.0, c);
                    }
                }
            }
            continue;
        }
        if let loom_scene::Modifier::UnwrapUVW(u) = &slot.modifier {
            // Unwrap UVW: seams in blue on the object, the map gizmo in yellow.
            let world = app.state.scene.object_matrix(*id);
            let input = match a.checked_sub(1) {
                Some(prev) => app.state.scene.evaluated(*id, Some(prev)).value.mesh.clone(),
                None => n.object.display_output(),
            };
            if u.show_seams.get() && !u.seams.is_empty() {
                let w: f32 = if u.thin_seams.get() { 1.5 } else { 3.0 };
                let blue = egui::Stroke::new(w, egui::Color32::from_rgb(60, 120, 255));
                for (va, vb) in &u.seams {
                    let (Some(pa), Some(pb)) = (input.vertex_position(loom_mesh::VertexId(*va)), input.vertex_position(loom_mesh::VertexId(*vb))) else { continue };
                    if let (Some(p), Some(q)) = (projection.screen(world.transform_point3(pa)), projection.screen(world.transform_point3(pb))) {
                        painter.line_segment([p, q], blue);
                    }
                }
            }
            if app.state.uv_editor.display_quick_map_gizmo || u.map_kind.get() > 0 {
                let g = world * u.map_gizmo.matrix();
                let (w, l) = (u.map_size.x * 0.5, u.map_size.y * 0.5);
                let col = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(255, 220, 60));
                let corners = [Vec3::new(-w, -l, 0.0), Vec3::new(w, -l, 0.0), Vec3::new(w, l, 0.0), Vec3::new(-w, l, 0.0)];
                for i in 0..4 {
                    if let (Some(p), Some(q)) = (projection.screen(g.transform_point3(corners[i])), projection.screen(g.transform_point3(corners[(i + 1) % 4]))) {
                        painter.line_segment([p, q], col);
                    }
                }
            }
            continue;
        }
        if let loom_scene::Modifier::UVWMap { projection: mapping, length, width, height, gizmo, .. } = &slot.modifier {
            // The mapping gizmo: a plane, cylinder, sphere or box of the map's size.
            let world = app.state.scene.object_matrix(*id) * gizmo.matrix();
            let (w, l, h) = (width.get() * 0.5, length.get() * 0.5, height.get() * 0.5);
            let col = egui::Color32::from_rgb(255, 200, 40);
            let stroke = egui::Stroke::new(1.0_f32, col);
            let seg = |a: Vec3, b: Vec3| {
                if let (Some(p), Some(q)) = (projection.screen(world.transform_point3(a)), projection.screen(world.transform_point3(b))) {
                    painter.line_segment([p, q], stroke);
                }
            };
            let circle = |z: f32, r: f32, axis: u8| {
                let n = 32;
                for k in 0..n {
                    let (a0, a1) = (k as f32 / n as f32 * std::f32::consts::TAU, (k + 1) as f32 / n as f32 * std::f32::consts::TAU);
                    let pt = |a: f32| match axis {
                        0 => Vec3::new(z, r * a.cos(), r * a.sin()),
                        1 => Vec3::new(r * a.cos(), z, r * a.sin()),
                        _ => Vec3::new(r * a.cos(), r * a.sin(), z),
                    };
                    seg(pt(a0), pt(a1));
                }
            };
            match mapping.get() {
                1 => {
                    let r = w.max(l);
                    circle(-h, r, 2);
                    circle(h, r, 2);
                    for k in 0..4 {
                        let a = k as f32 * std::f32::consts::FRAC_PI_2;
                        seg(Vec3::new(r * a.cos(), r * a.sin(), -h), Vec3::new(r * a.cos(), r * a.sin(), h));
                    }
                }
                2 | 3 => {
                    let r = w.max(l).max(h);
                    circle(0.0, r, 0);
                    circle(0.0, r, 1);
                    circle(0.0, r, 2);
                }
                4 | 6 => {
                    for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
                        let c = |i: u8| Vec3::new(if i & 1 == 0 { -w } else { w }, if i & 2 == 0 { -l } else { l }, if i & 4 == 0 { -h } else { h });
                        seg(c(a), c(b));
                    }
                }
                _ => {
                    seg(Vec3::new(-w, -l, 0.0), Vec3::new(w, -l, 0.0));
                    seg(Vec3::new(w, -l, 0.0), Vec3::new(w, l, 0.0));
                    seg(Vec3::new(w, l, 0.0), Vec3::new(-w, l, 0.0));
                    seg(Vec3::new(-w, l, 0.0), Vec3::new(-w, -l, 0.0));
                    // The "up" tick on the top edge.
                    seg(Vec3::new(0.0, l, 0.0), Vec3::new(0.0, l + l * 0.2, 0.0));
                }
            }
            continue;
        }
        if let loom_scene::Modifier::AffectRegion(d) = &slot.modifier {
            // Affect Region: the pull vector from the start point to the end point.
            let world = app.state.scene.object_matrix(*id);
            let col = egui::Color32::from_rgb(255, 160, 40);
            if let (Some(s), Some(e)) = (projection.screen(world.transform_point3(Vec3::ZERO)), projection.screen(world.transform_point3(d.gizmo.position.get()))) {
                painter.line_segment([s, e], egui::Stroke::new(1.5_f32, col));
                painter.circle_stroke(s, 4.0, egui::Stroke::new(1.5_f32, col));
                painter.circle_filled(e, 4.0, col);
            }
            continue;
        }
        let Some(g) = slot.modifier.gizmo() else { continue };
        let input = match a.checked_sub(1) {
            Some(prev) => app.state.scene.evaluated(*id, Some(prev)).value.mesh.clone(),
            None => n.object.display_output(),
        };
        let (lo, hi) = input.aabb();
        let world = app.state.scene.object_matrix(*id) * g.matrix();
        let ginv = g.matrix().inverse();
        // Box corners in gizmo space from the object-space bounds.
        let corners: Vec<Vec3> = (0..8).map(|i| {
            let c = Vec3::new(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z });
            ginv.transform_point3(c)
        }).collect();
        let (mut glo, mut ghi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        for c in &corners {
            glo = glo.min(*c);
            ghi = ghi.max(*c);
        }
        let col = egui::Color32::from_rgb(255, 160, 40);
        let pts: Vec<Option<egui::Pos2>> = (0..8).map(|i| {
            let c = Vec3::new(if i & 1 == 0 { glo.x } else { ghi.x }, if i & 2 == 0 { glo.y } else { ghi.y }, if i & 4 == 0 { glo.z } else { ghi.z });
            projection.screen(world.transform_point3(c))
        }).collect();
        for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
            if let (Some(p), Some(q)) = (pts[a], pts[b]) {
                painter.line_segment([p, q], egui::Stroke::new(1.0_f32, col));
            }
        }
        // The Center: a deformation modifier's own point, else the gizmo origin.
        let center = slot.modifier.deformer().map(|d| d.center.get()).unwrap_or(Vec3::ZERO);
        if let Some(c) = projection.screen(world.transform_point3(center)) {
            painter.circle_stroke(c, 4.0, egui::Stroke::new(1.5_f32, col));
        }
    }
    if app.state.slice_plane_mode {
        let sp = app.state.slice_plane;
        let (x, y) = (sp.rotation * Vec3::X * sp.size, sp.rotation * Vec3::Y * sp.size);
        let corners = [sp.origin - x - y, sp.origin + x - y, sp.origin + x + y, sp.origin - x + y];
        let pts: Vec<egui::Pos2> = corners.iter().filter_map(|c| projection.screen(*c)).collect();
        if pts.len() == 4 {
            painter.add(egui::Shape::convex_polygon(pts.clone(), egui::Color32::from_rgba_unmultiplied(255, 220, 60, 40), egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(255, 220, 60))));
        }
    }
    match app.state.pick_mode {
        Some(PickMode::QuickSliceB | PickMode::CutB) => {
            if let (Some((_, _, a)), Some(h)) = (app.state.pick_point_a, hover) {
                if let Some(sa) = projection.screen(a) {
                    painter.line_segment([sa, h], stroke);
                    painter.circle_stroke(sa, 4.0, stroke);
                }
            }
        }
        Some(PickMode::SwiftLoop) => {
            // The green loop a click would insert.
            let Some(h) = hover else { return };
            let Some((node, edge, t)) = swift_loop_target(app, h, projection) else { return };
            let Some((_, m)) = app.world_meshes().iter().find(|(n, _)| *n == node) else { return };
            let (pts, closed) = loom_mesh::topo_ops::swift_loop_points(m, edge, t);
            let sp: Vec<egui::Pos2> = pts.iter().filter_map(|q| projection.screen(*q)).collect();
            let green = egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(60, 230, 60));
            for w in sp.windows(2) {
                painter.line_segment([w[0], w[1]], green);
            }
            if closed && sp.len() > 2 {
                painter.line_segment([sp[sp.len() - 1], sp[0]], green);
            }
        }
        Some(PickMode::TargetWeldTarget | PickMode::EditTriangulationB) => {
            if let (Some((node, v)), Some(h)) = (app.state.pick_source, hover) {
                if let Some(sp) = vertex_screen(app, projection, node, v) {
                    painter.line_segment([sp, h], stroke);
                    painter.circle_stroke(sp, 4.0, stroke);
                }
            }
        }
        Some(PickMode::CreatePolygon) => {
            let pts: Vec<egui::Pos2> = app.state.create_verts.iter().filter_map(|(n, v)| vertex_screen(app, projection, *n, *v)).collect();
            for w in pts.windows(2) {
                painter.line_segment([w[0], w[1]], stroke);
            }
            if let (Some(last), Some(h)) = (pts.last(), hover) {
                painter.line_segment([*last, h], stroke);
            }
            for p in &pts {
                painter.circle_filled(*p, 3.0, egui::Color32::from_rgb(255, 255, 255));
            }
        }
        _ => {}
    }
}

/// Screen positions of the active FFD entry's control points, when its
/// Control Points or Set Volume level is on.
/// Skin > Edit Envelopes: the current bone's envelope capsule (inner red,
/// outer purple), cross-section rings and the vertices coloured by weight.
/// Edit Normals colours: unspecified (follow the smoothing groups) dark
/// blue, explicit (turned or set by hand) green, selected red.
pub const NORMAL_UNSPECIFIED: egui::Color32 = egui::Color32::from_rgb(40, 70, 220);
pub const NORMAL_EXPLICIT: egui::Color32 = egui::Color32::from_rgb(40, 220, 60);
pub const NORMAL_SELECTED: egui::Color32 = egui::Color32::from_rgb(255, 40, 40);

/// One normal of the Edit Normals entry at its Normal level, on screen.
struct ScreenNormal {
    id: u32,
    from: egui::Pos2,
    to: egui::Pos2,
    explicit: bool,
    selected: bool,
    /// It points away from the viewer (Ignore Backfacing skips it).
    back: bool,
}

/// Every normal of the Edit Normals entry at its Normal level as a screen
/// line from its corner's vertex, Display Length long.
fn normals_on_screen(app: &LoomApp, projection: &Projection) -> Option<Vec<ScreenNormal>> {
    let (id, index) = loom_commands::normals::normal_level(&app.state)?;
    let n = app.state.scene.node(id)?;
    let loom_scene::Modifier::EditNormals(d) = &n.modifiers[index].modifier else { return None };
    let input = loom_commands::normals::input_mesh(&app.state, id, index);
    let groups = d.groups_for(&input);
    let world = app.state.scene.object_matrix(id);
    let all = d.normals(&input, &groups);
    let mut drawn = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for (i, g) in groups.iter().enumerate() {
        if *g == loom_scene::edit_normals::NO_GROUP || !drawn.insert(*g) {
            continue;
        }
        let v = input.half_edges()[i].origin;
        let Some(p) = input.vertex_position(v) else { continue };
        let dir = all.get(g).copied().unwrap_or_default();
        let a = world.transform_point3(p);
        let b = world.transform_point3(p + dir * d.display_length);
        if let (Some(sa), Some(sb)) = (projection.screen(a), projection.screen(b)) {
            let back = (b - a).dot(projection.toward_viewer_at(a)) < 0.0;
            out.push(ScreenNormal { id: *g, from: sa, to: sb, explicit: d.explicit.contains_key(g), selected: d.selection.contains(g), back });
        }
    }
    Some(out)
}

fn draw_normals(painter: &egui::Painter, app: &LoomApp, projection: &Projection) {
    for n in normals_on_screen(app, projection).unwrap_or_default() {
        let c = if n.selected { NORMAL_SELECTED } else if n.explicit { NORMAL_EXPLICIT } else { NORMAL_UNSPECIFIED };
        painter.line_segment([n.from, n.to], egui::Stroke::new(1.5_f32, c));
    }
}

fn segment_distance(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let t = if ab.length_sq() < 1e-6 { 0.0 } else { ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) };
    p.distance(a + ab * t)
}

/// Edit Normals at the Normal level: a click (nearest within 8 px) or a
/// region selects normals, or the normals of the vertices / edges / faces
/// under Select By; Ctrl adds, Alt removes, a click on nothing clears.
/// False when the Normal level is not on.
pub fn normals_pick(app: &mut LoomApp, projection: &Projection, region: Option<&crate::region::Region>, p: egui::Pos2, mods: loom_input::Modifiers) -> bool {
    let Some(normals) = normals_on_screen(app, projection) else { return false };
    let Some((id, index)) = loom_commands::normals::normal_level(&app.state) else { return false };
    let Some(loom_scene::Modifier::EditNormals(d)) = app.state.scene.node(id).map(|n| &n.modifiers[index].modifier) else { return false };
    let (by, ignore_back) = (d.select_by, d.ignore_backfacing);
    let ids: Vec<u32> = if by == 0 {
        let shown = normals.iter().filter(|n| !(ignore_back && n.back));
        match region {
            // A region takes the normals whose line it touches (sampled along the line).
            Some(r) => shown
                .filter(|n| {
                    let steps = ((n.to - n.from).length() / 3.0).ceil().max(1.0) as usize;
                    (0..=steps).any(|k| r.contains(n.from + (n.to - n.from) * (k as f32 / steps as f32)))
                })
                .map(|n| n.id)
                .collect(),
            None => shown.map(|n| (n.id, segment_distance(p, n.from, n.to))).filter(|(_, d)| *d <= 8.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(g, _)| vec![g]).unwrap_or_default(),
        }
    } else {
        let input = loom_commands::normals::input_mesh(&app.state, id, index);
        mesh_ids_at(projection, &input, app.state.scene.object_matrix(id), by, ignore_back, region, p)
    };
    let op = if mods.ctrl { loom_commands::SelectOp::Add } else if mods.alt { loom_commands::SelectOp::Subtract } else { loom_commands::SelectOp::Replace };
    let _ = app.dispatch(Command::EditNormalsSelect { ids, op });
    true
}

/// Vertex (1), edge (2) or face (3) ids of `input` (placed by `world`)
/// under a click or in a region.
#[allow(clippy::too_many_arguments)]
fn mesh_ids_at(projection: &Projection, input: &loom_mesh::Mesh, world: Mat4, by: u32, ignore_back: bool, region: Option<&crate::region::Region>, p: egui::Pos2) -> Vec<u32> {
    let wp: Vec<Vec3> = input.vertices().iter().map(|v| world.transform_point3(v.position)).collect();
    let sp: Vec<Option<egui::Pos2>> = wp.iter().map(|q| projection.screen(*q)).collect();
    let face_front: Vec<bool> = (0..input.face_count() as u32)
        .map(|f| {
            let f = loom_mesh::FaceId(f);
            let vs = input.face_vertex_ids(f).unwrap_or_default();
            let c = vs.iter().map(|v| wp[v.0 as usize]).sum::<Vec3>() / vs.len().max(1) as f32;
            world.transform_vector3(input.face_normal(f)).dot(projection.toward_viewer_at(c)) >= 0.0
        })
        .collect();
    let he = input.half_edges();
    let front_he = |i: usize| -> bool { !ignore_back || [Some(i), Some(he[i].twin.0 as usize)].iter().flatten().any(|h| he.get(*h).and_then(|h| h.face).is_some_and(|f| face_front[f.0 as usize])) };
    match by {
        1 => {
            let visible: Vec<bool> = (0..wp.len()).map(|v| !ignore_back || he.iter().any(|h| h.origin.0 as usize == v && h.face.is_some_and(|f| face_front[f.0 as usize]))).collect();
            let cands = sp.iter().enumerate().filter_map(|(v, s)| s.filter(|_| visible[v]).map(|s| (v as u32, s)));
            match region {
                Some(r) => cands.filter(|(_, s)| r.contains(*s)).map(|(v, _)| v).collect(),
                None => cands.map(|(v, s)| (v, s.distance(p))).filter(|(_, d)| *d <= 8.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(v, _)| vec![v]).unwrap_or_default(),
            }
        }
        2 => {
            let edges = (0..he.len()).filter(|i| input.canonical_edge_id(loom_mesh::HalfEdgeId(*i as u32)).is_some_and(|c| c.0 as usize == *i) && front_he(*i)).filter_map(|i| {
                let (a, b) = (sp[he[i].origin.0 as usize]?, sp[he[he[i].next.0 as usize].origin.0 as usize]?);
                Some((i as u32, a, b))
            });
            match region {
                Some(r) => edges.filter(|(_, a, b)| r.contains(*a) && r.contains(*b)).map(|(e, _, _)| e).collect(),
                None => edges.map(|(e, a, b)| (e, segment_distance(p, a, b))).filter(|(_, d)| *d <= 6.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(e, _)| vec![e]).unwrap_or_default(),
            }
        }
        _ => {
            let eye = projection.view.inverse().w_axis.truncate();
            let faces = (0..input.face_count()).filter(|f| !ignore_back || face_front[*f]).filter_map(|f| {
                let vs = input.face_vertex_ids(loom_mesh::FaceId(f as u32)).unwrap_or_default();
                let poly: Option<Vec<egui::Pos2>> = vs.iter().map(|v| sp[v.0 as usize]).collect();
                let c = vs.iter().map(|v| wp[v.0 as usize]).sum::<Vec3>() / vs.len().max(1) as f32;
                Some((f as u32, poly?, c.distance(eye)))
            });
            match region {
                Some(r) => faces.filter(|(_, poly, _)| {
                    let c = poly.iter().fold(egui::Vec2::ZERO, |a, q| a + q.to_vec2()) / poly.len().max(1) as f32;
                    r.contains(c.to_pos2())
                }).map(|(f, _, _)| f).collect(),
                None => faces.filter(|(_, poly, _)| crate::region::Region::Polygon(poly.clone()).contains(p)).min_by(|a, b| a.2.total_cmp(&b.2)).map(|(f, _, _)| vec![f]).unwrap_or_default(),
            }
        }
    }
}

/// VertexPaint > Face Selection: a click or region selects faces of the
/// entry's input (Ctrl adds, Alt removes, a click on nothing clears).
/// False when Face Selection is off.
pub fn vertex_paint_face_pick(app: &mut LoomApp, projection: &Projection, region: Option<&crate::region::Region>, p: egui::Pos2, mods: loom_input::Modifiers) -> bool {
    if !app.state.vertex_paint_faces {
        return false;
    }
    let Some((id, index)) = loom_commands::vertex_paint::entry(&app.state) else { return false };
    let input = loom_commands::vertex_paint::input_mesh(&app.state, id, index);
    let ids = mesh_ids_at(projection, &input, app.state.scene.object_matrix(id), 3, true, region, p);
    let op = if mods.ctrl { loom_commands::SelectOp::Add } else if mods.alt { loom_commands::SelectOp::Subtract } else { loom_commands::SelectOp::Replace };
    let _ = app.dispatch(Command::VertexPaintSelectFaces { ids, op });
    true
}

/// VertexPaint's selected faces: red outline over a faint red fill.
pub const VERTEX_PAINT_FACE: egui::Color32 = egui::Color32::from_rgb(255, 30, 30);

fn draw_vertex_paint_faces(painter: &egui::Painter, app: &LoomApp, projection: &Projection) {
    let Some((id, index)) = loom_commands::vertex_paint::entry(&app.state) else { return };
    let Some(loom_scene::Modifier::VertexPaint { faces, .. }) = app.state.scene.node(id).map(|n| &n.modifiers[index].modifier) else { return };
    if faces.is_empty() {
        return;
    }
    let input = loom_commands::vertex_paint::input_mesh(&app.state, id, index);
    let world = app.state.scene.object_matrix(id);
    for f in faces {
        let Some(vs) = input.face_vertex_ids(loom_mesh::FaceId(*f)) else { continue };
        let pts: Option<Vec<egui::Pos2>> = vs.iter().map(|v| input.vertex_position(*v).and_then(|q| projection.screen(world.transform_point3(q)))).collect();
        let Some(pts) = pts else { continue };
        if pts.len() >= 3 {
            painter.add(egui::Shape::convex_polygon(pts.clone(), egui::Color32::from_rgba_unmultiplied(255, 0, 0, 60), egui::Stroke::NONE));
        }
        for i in 0..pts.len() {
            painter.line_segment([pts[i], pts[(i + 1) % pts.len()]], egui::Stroke::new(1.5_f32, VERTEX_PAINT_FACE));
        }
    }
}

/// HSDS at a sub-object level: the cage of the level being edited, yellow.
pub const HSDS_CAGE: egui::Color32 = egui::Color32::from_rgb(255, 230, 0);

fn draw_hsds_cage(painter: &egui::Painter, app: &LoomApp, projection: &Projection) {
    for (id, mesh) in app.world_meshes() {
        let Some(n) = app.state.scene.node(*id) else { continue };
        if !app.state.selection.contains(id) || !n.active_entry.and_then(|a| n.modifiers.get(a)).is_some_and(|s| s.sub_level > 0 && matches!(s.modifier, loom_scene::Modifier::Hsds(_))) {
            continue;
        }
        let pts: Vec<Option<egui::Pos2>> = mesh.vertices().iter().map(|v| projection.screen(v.position)).collect();
        for (i, he) in mesh.half_edges().iter().enumerate() {
            if (i as u32) > he.twin.0 {
                continue;
            }
            let b = mesh.half_edges()[he.next.0 as usize].origin;
            if let (Some(a), Some(b)) = (pts[he.origin.0 as usize], pts[b.0 as usize]) {
                painter.line_segment([a, b], egui::Stroke::new(1.0_f32, HSDS_CAGE));
            }
        }
    }
}

fn draw_skin(painter: &egui::Painter, app: &LoomApp, projection: &Projection) {
    if !app.state.skin_active() {
        return;
    }
    let Some((_, weights)) = loom_commands::skin::current_bone_weights(&app.state) else { return };
    let display = loom_commands::skin::entry(&app.state).map(|e| e.data.display).unwrap_or_default();
    if app.state.skin_filter_envelopes && !display.no_envelopes {
        for env in loom_commands::skin::shown_envelopes_world(&app.state) {
            draw_envelope(painter, app, projection, &env);
        }
    }
    let selection: std::collections::BTreeSet<u32> = loom_commands::skin::selection(&app.state).into_iter().collect();
    if app.state.skin_filter_vertices {
        let Some((_, positions)) = loom_commands::skin::vertex_world_positions(&app.state) else { return };
        for (i, (w, q)) in weights.iter().zip(&positions).enumerate() {
            let Some(p) = projection.screen(*q) else { continue };
            let selected = selection.contains(&(i as u32));
            // Show Colored Vertices: the current bone's weight from blue (0) to red (1).
            let colour = if display.colored_vertices {
                let c = loom_commands::skin::weight_color(*w);
                egui::Color32::from_rgb(c[0], c[1], c[2])
            } else {
                egui::Color32::from_rgb(40, 40, 40)
            };
            if selected {
                painter.rect_filled(egui::Rect::from_center_size(p, egui::vec2(7.0, 7.0)), 0.0, egui::Color32::WHITE);
            }
            painter.rect_filled(egui::Rect::from_center_size(p, egui::vec2(if selected { 5.0 } else { 4.0 }, if selected { 5.0 } else { 4.0 })), 0.0, colour);
        }
    }
}

/// One envelope capsule (inner red, outer purple); the current bone's with
/// its cross-section rings and end handles, the others dimmed.
fn draw_envelope(painter: &egui::Painter, app: &LoomApp, projection: &Projection, env: &loom_commands::skin::EnvelopeWorld) {
    let (start, end, inner, outer) = (env.start, env.end, env.inner, env.outer);
    let axis = (end - start).normalize_or_zero();
    let (u, v) = {
        let a = if axis.x.abs() < 0.9 { glam::Vec3::X } else { glam::Vec3::Y };
        let u = axis.cross(a).normalize_or_zero();
        (u, axis.cross(u))
    };
    let ring = |centre: glam::Vec3, r: f32, colour: egui::Color32| {
        let pts: Vec<Option<egui::Pos2>> = (0..24)
            .map(|i| {
                let t = i as f32 / 24.0 * std::f32::consts::TAU;
                projection.screen(centre + (u * t.cos() + v * t.sin()) * r)
            })
            .collect();
        for i in 0..24 {
            if let (Some(a), Some(b)) = (pts[i], pts[(i + 1) % 24]) {
                painter.line_segment([a, b], egui::Stroke::new(1.0_f32, colour));
            }
        }
    };
    let (red, purple) = if env.current { (egui::Color32::from_rgb(230, 60, 60), egui::Color32::from_rgb(170, 80, 220)) } else { (egui::Color32::from_rgb(140, 70, 70), egui::Color32::from_rgb(110, 70, 140)) };
    ring(start, inner[0], red);
    ring(end, inner[1], red);
    ring(start, outer[0], purple);
    ring(end, outer[1], purple);
    if env.current && app.state.skin_filter_cross_sections {
        for (t, i, o) in &env.cross_sections {
            let c = start.lerp(end, *t);
            ring(c, *i, red);
            ring(c, *o, purple);
        }
    }
    // Four lines along the capsule for each radius.
    for (r0, r1, colour) in [(inner[0], inner[1], red), (outer[0], outer[1], purple)] {
        for dir in [u, v, -u, -v] {
            if let (Some(a), Some(b)) = (projection.screen(start + dir * r0), projection.screen(end + dir * r1)) {
                painter.line_segment([a, b], egui::Stroke::new(1.0_f32, colour));
            }
        }
    }
    if env.current {
        if let (Some(a), Some(b)) = (projection.screen(start), projection.screen(end)) {
            let handle = app.state.skin_handle;
            painter.rect_filled(egui::Rect::from_center_size(a, egui::vec2(7.0, 7.0)), 0.0, if handle == Some(0) { egui::Color32::WHITE } else { red });
            painter.rect_filled(egui::Rect::from_center_size(b, egui::vec2(7.0, 7.0)), 0.0, if handle == Some(1) { egui::Color32::WHITE } else { red });
        }
    }
}

/// Click / region selection at the Envelope level: vertices (Select
/// Vertices on, Backface Cull Vertices honoured), else a click on a bone
/// makes it current; a click on nothing clears the vertex selection.
pub fn skin_pick(app: &mut LoomApp, projection: &Projection, region: Option<&crate::region::Region>, p: egui::Pos2, mods: loom_input::Modifiers) -> bool {
    if !loom_commands::skin::picks_vertices(&app.state) {
        return false;
    }
    let pts: Vec<Option<egui::Pos2>> = if app.state.skin_filter_vertices {
        let Some((_, positions)) = loom_commands::skin::vertex_world_positions(&app.state) else { return false };
        let normals = if app.state.skin_backface_cull { loom_commands::skin::vertex_world_normals(&app.state) } else { None };
        positions
            .iter()
            .enumerate()
            .map(|(i, q)| {
                let back = normals.as_ref().is_some_and(|n| n[i].dot(projection.toward_viewer_at(*q)) < 0.0);
                if back { None } else { projection.screen(*q) }
            })
            .collect()
    } else {
        Vec::new()
    };
    let hits: Vec<u32> = match region {
        Some(r) => pts.iter().enumerate().filter_map(|(i, sp)| sp.filter(|sp| r.contains(*sp)).map(|_| i as u32)).collect(),
        None => pts
            .iter()
            .enumerate()
            .filter_map(|(i, sp)| sp.map(|sp| (i as u32, sp.distance(p))))
            .filter(|(_, d)| *d <= 8.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| vec![i])
            .unwrap_or_default(),
    };
    if hits.is_empty() && region.is_none() {
        // A click on a bone makes it the current bone.
        if let Some(bone) = pick_bone(app, p, projection) {
            let _ = app.dispatch(Command::SkinSelectBone { id: bone.0 });
            return true;
        }
    }
    let op = if mods.ctrl { loom_commands::SelectOp::Add } else if mods.alt { loom_commands::SelectOp::Subtract } else { loom_commands::SelectOp::Replace };
    if hits.is_empty() && op != loom_commands::SelectOp::Replace {
        return true;
    }
    let _ = app.dispatch(Command::SkinSelectVertices { ids: hits, op });
    true
}

/// Paint Weights: LMB drags dab weight onto the current bone (one undo entry
/// per stroke), Alt erases, Ctrl+Shift drag sizes the brush, Alt+Shift drag
/// sets its strength; the brush ring follows the pointer on the surface.
/// Returns true while the button belongs to the brush.
#[allow(clippy::too_many_arguments)]
pub fn skin_paint(app: &mut LoomApp, ctx: &egui::Context, painter: &egui::Painter, pointer: &egui::PointerState, hovered: bool, p: egui::Pos2, rect: egui::Rect, view: Mat4, proj: Mat4, projection: &Projection) -> bool {
    if !loom_commands::skin::painting(&app.state) {
        return false;
    }
    let Some((id, _)) = app.state.skin_entry() else { return false };
    let mods = ctx.input(|i| i.modifiers);
    let down = pointer.button_down(egui::PointerButton::Primary) && (hovered || app.deform_painting);
    let resizing = mods.shift && (mods.ctrl || mods.alt);
    if pointer.button_pressed(egui::PointerButton::Primary) && hovered {
        app.deform_painting = true;
        app.poly_dragging = true;
        // One stroke = one undo entry (a resize drag commits nothing, so leaves none).
        app.state.undo.begin("Paint Weights");
    }
    let hit = |app: &LoomApp, at: egui::Pos2| surface_hit_node(app, at, rect, view, proj).filter(|(n, _, _)| *n == id);
    if down && app.deform_painting {
        if resizing {
            let dx = pointer.delta().x;
            if dx != 0.0 {
                let painter_opts = app.state.skin_painter.clone();
                let (name, value) = if mods.ctrl {
                    let upp = hit(app, p).map(|(_, at, _)| projection.units_per_pixel(at)).unwrap_or(0.1);
                    ("Max Size", (painter_opts.size + dx * upp).max(0.01) as f64)
                } else {
                    ("Max Strength", (painter_opts.strength + dx * 0.005).clamp(0.0, 1.0) as f64)
                };
                let _ = app.dispatch(Command::SkinSetPainterOption { name: name.into(), value });
            }
        } else if pointer.delta() != egui::Vec2::ZERO || pointer.button_pressed(egui::PointerButton::Primary) {
            if let Some((_, at, _)) = hit(app, p) {
                let _ = app.dispatch(Command::SkinPaintStroke { points: vec![at.to_array()], erase: mods.alt, pressure: None });
            }
        }
        ctx.request_repaint();
    }
    if down || app.deform_painting {
        app.drag_select_start = None;
        app.alt_click = None;
    }
    if pointer.button_released(egui::PointerButton::Primary) && app.deform_painting {
        app.deform_painting = false;
        app.poly_dragging = false;
        app.state.undo.end();
    }
    if app.state.skin_painter.draw_ring {
        if let Some(hp) = pointer.hover_pos().filter(|hp| rect.contains(*hp)) {
            if let Some((_, at, normal)) = hit(app, hp) {
                // The brush ring lies on the surface around the hit point.
                let r = app.state.skin_painter.size;
                let a = if normal.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
                let u = normal.cross(a).normalize_or_zero();
                let v = normal.cross(u);
                let pts: Vec<egui::Pos2> = (0..=32).filter_map(|i| projection.screen(at + (u * (i as f32 / 32.0 * std::f32::consts::TAU).cos() + v * (i as f32 / 32.0 * std::f32::consts::TAU).sin()) * r)).collect();
                painter.add(egui::Shape::line(pts, egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(255, 255, 255))));
                if let (Some(a), Some(b)) = (projection.screen(at), projection.screen(at + normal * r * 0.5)) {
                    painter.line_segment([a, b], egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(255, 255, 255)));
                }
            }
        }
    }
    true
}

/// The Skin bone whose projected segment is nearest the pointer (within 10 px).
fn pick_bone(app: &LoomApp, p: egui::Pos2, projection: &Projection) -> Option<NodeId> {
    let e = loom_commands::skin::entry(&app.state).ok()?;
    let mut best: Option<(f32, NodeId)> = None;
    for b in &e.data.bones {
        let w = app.state.scene.world_matrix(b.node);
        let len = app.state.scene.node(b.node).and_then(|n| n.object.bone_length()).unwrap_or(1.0);
        let (Some(a), Some(c)) = (projection.screen(w.transform_point3(glam::Vec3::ZERO)), projection.screen(w.transform_point3(glam::Vec3::X * len))) else { continue };
        let ab = c - a;
        let t = if ab.length_sq() < 1e-6 { 0.0 } else { ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) };
        let d = (a + ab * t).distance(p);
        if d <= 10.0 && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, b.node));
        }
    }
    best.map(|(_, id)| id)
}

pub fn ffd_points_on_screen(app: &LoomApp, projection: &Projection) -> Option<Vec<Option<egui::Pos2>>> {
    let (id, a) = app.state.ffd_active()?;
    let n = app.state.scene.node(id)?;
    let slot = n.modifiers.get(a)?;
    if slot.sub_level != 1 && slot.sub_level != 3 {
        return None;
    }
    let loom_scene::Modifier::FFD { points, gizmo, .. } = &slot.modifier else { return None };
    let world = app.state.scene.object_matrix(id) * gizmo.matrix();
    Some(points.iter().map(|p| projection.screen(world.transform_point3(*p))).collect())
}

/// Click / region selection of FFD control points. Returns true when the
/// active entry's control point level consumed the pick.
pub fn ffd_pick(app: &mut LoomApp, projection: &Projection, region: Option<&crate::region::Region>, p: egui::Pos2, mods: loom_input::Modifiers) -> bool {
    let Some(pts) = ffd_points_on_screen(app, projection) else { return false };
    let hits: Vec<u32> = match region {
        Some(r) => pts.iter().enumerate().filter_map(|(i, sp)| sp.filter(|sp| r.contains(*sp)).map(|_| i as u32)).collect(),
        None => pts
            .iter()
            .enumerate()
            .filter_map(|(i, sp)| sp.map(|sp| (i as u32, sp.distance(p))))
            .filter(|(_, d)| *d <= 8.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| vec![i])
            .unwrap_or_default(),
    };
    let op = if mods.ctrl { loom_commands::SelectOp::Add } else if mods.alt { loom_commands::SelectOp::Subtract } else { loom_commands::SelectOp::Replace };
    let _ = app.dispatch(Command::SelectControlPoints { op, indices: hits });
    true
}
