//! Skin > Edit Envelopes in the viewports: envelopes (capsules with their
//! cross sections and end points), the weighted vertices (by weight, by
//! bone colour or by mirror side), gizmo lattices and the mirror plane;
//! picking vertices, end points, cross sections, lattice points and bones;
//! dragging end points and cross-section radii.

use crate::gizmo::Projection;
use crate::LoomApp;
use eframe::egui;
use glam::Vec3;
use loom_commands::skin::EnvelopeWorld;
use loom_commands::{Command, SelectOp};
use loom_scene::NodeId;

const RED: egui::Color32 = egui::Color32::from_rgb(230, 60, 60);
const PURPLE: egui::Color32 = egui::Color32::from_rgb(170, 80, 220);
const DIM_RED: egui::Color32 = egui::Color32::from_rgb(140, 70, 70);
const DIM_PURPLE: egui::Color32 = egui::Color32::from_rgb(110, 70, 140);
const PINK: egui::Color32 = egui::Color32::from_rgb(255, 105, 180);
const YELLOW: egui::Color32 = egui::Color32::from_rgb(255, 230, 40);
/// Mirror Mode sides: green negative, blue positive, red unmatched.
const GREEN_SIDE: egui::Color32 = egui::Color32::from_rgb(40, 200, 60);
const BLUE_SIDE: egui::Color32 = egui::Color32::from_rgb(50, 90, 255);
const RED_SIDE: egui::Color32 = egui::Color32::from_rgb(230, 40, 40);

/// Whether a world point is hidden behind the scene's geometry (Draw On Top off).
fn occluded(app: &LoomApp, projection: &Projection, q: Vec3) -> bool {
    let dir = projection.toward_viewer_at(q);
    let origin = q + dir * 1e-3;
    for (_, mesh) in app.world_meshes() {
        for f in 0..mesh.face_count() {
            let Some(pts) = mesh.face_positions(loom_mesh::FaceId(f as u32)) else { continue };
            for k in 1..pts.len().saturating_sub(1) {
                if crate::viewport_widget::ray_triangle(origin, dir, pts[0], pts[k], pts[k + 1]).is_some_and(|t| t > 1e-3) {
                    return true;
                }
            }
        }
    }
    false
}

/// Two unit vectors across `axis`.
fn across(axis: Vec3) -> (Vec3, Vec3) {
    let a = if axis.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
    let u = axis.cross(a).normalize_or_zero();
    (u, axis.cross(u))
}

/// A line drawn only where it is not hidden, when `on_top` is off.
fn seg(painter: &egui::Painter, app: &LoomApp, projection: &Projection, a: Vec3, b: Vec3, colour: egui::Color32, on_top: bool) {
    if !on_top && occluded(app, projection, (a + b) * 0.5) {
        return;
    }
    if let (Some(sa), Some(sb)) = (projection.screen(a), projection.screen(b)) {
        painter.line_segment([sa, sb], egui::Stroke::new(1.0_f32, colour));
    }
}

/// The 4 control points of a cross-section ring (on the ring, across the axis).
pub fn ring_points(env: &EnvelopeWorld, u: f32, r: f32) -> [Vec3; 4] {
    let c = env.point(u);
    let (a, b) = across(env.direction(u));
    [c + a * r, c + b * r, c - a * r, c - b * r]
}

fn ring(painter: &egui::Painter, app: &LoomApp, projection: &Projection, env: &EnvelopeWorld, u: f32, r: f32, colour: egui::Color32, on_top: bool) {
    let c = env.point(u);
    let (a, b) = across(env.direction(u));
    let pts: Vec<Vec3> = (0..=24).map(|i| i as f32 / 24.0 * std::f32::consts::TAU).map(|t| c + (a * t.cos() + b * t.sin()) * r).collect();
    for w in pts.windows(2) {
        seg(painter, app, projection, w[0], w[1], colour, on_top);
    }
}

/// One envelope: inner (red) and outer (purple) rings at its cross sections
/// and lines along it; the current one with its end points and cross-section
/// control points, the selected ring in yellow.
fn draw_envelope(painter: &egui::Painter, app: &LoomApp, projection: &Projection, env: &EnvelopeWorld, display: &loom_scene::skin::SkinDisplay) {
    let (red, purple) = if env.current { (RED, PURPLE) } else { (DIM_RED, DIM_PURPLE) };
    let on_top = display.envelope_on_top;
    let cs_top = display.cross_sections_on_top;
    let sections = env.sections();
    let selected = app.state.skin_ui.cross_section.filter(|_| env.current);
    for (k, (u, i, o)) in sections.iter().enumerate() {
        let sel = |outer: bool| selected == Some((k, outer));
        ring(painter, app, projection, env, *u, *i, if sel(false) { YELLOW } else { red }, cs_top);
        ring(painter, app, projection, env, *u, *o, if sel(true) { YELLOW } else { purple }, cs_top);
        if env.current && app.state.skin_filter_cross_sections {
            for (r, outer) in [(*i, false), (*o, true)] {
                for q in ring_points(env, *u, r) {
                    if !cs_top && occluded(app, projection, q) {
                        continue;
                    }
                    if let Some(sp) = projection.screen(q) {
                        painter.circle_filled(sp, 2.5, if sel(outer) { YELLOW } else if outer { purple } else { red });
                    }
                }
            }
        }
    }
    // Lines along the capsule for each radius, following a curved axis.
    let steps = if env.path.len() >= 2 { 16 } else { 1 };
    for outer in [false, true] {
        for side in 0..4 {
            for s in 0..steps {
                let (u0, u1) = (s as f32 / steps as f32, (s + 1) as f32 / steps as f32);
                let at = |u: f32| {
                    let (i, o) = env.radii_at(u);
                    ring_points(env, u, if outer { o } else { i })[side]
                };
                seg(painter, app, projection, at(u0), at(u1), if outer { purple } else { red }, on_top);
            }
        }
    }
    if env.current && app.state.skin_filter_envelopes {
        for (k, q) in [env.start, env.end].into_iter().enumerate() {
            if let Some(sp) = projection.screen(q) {
                let h = app.state.skin_handle == Some(k as u8);
                painter.rect_filled(egui::Rect::from_center_size(sp, egui::vec2(7.0, 7.0)), 0.0, if h { egui::Color32::WHITE } else { red });
            }
        }
    }
}

/// The weighted points' screen positions (None when off screen, back-facing
/// with Backface Cull Vertices, or hidden without Show Hidden Vertices).
fn point_screens(app: &LoomApp, projection: &Projection, cull: bool) -> Option<Vec<Option<egui::Pos2>>> {
    let (_, positions) = loom_commands::skin::vertex_world_positions(&app.state)?;
    let normals = if cull { loom_commands::skin::vertex_world_normals(&app.state) } else { None };
    let e = loom_commands::skin::entry(&app.state).ok()?;
    let show_hidden = e.data.display.show_hidden_vertices;
    Some(
        positions
            .iter()
            .enumerate()
            .map(|(i, q)| {
                let hidden = !show_hidden && e.input.vertices().get(i).is_some_and(|v| v.hidden);
                let back = normals.as_ref().is_some_and(|n| n.get(i).is_some_and(|n| n.dot(projection.toward_viewer_at(*q)) < 0.0));
                if hidden || back { None } else { projection.screen(*q) }
            })
            .collect(),
    )
}

pub fn draw(painter: &egui::Painter, app: &LoomApp, projection: &Projection) {
    if !app.state.skin_active() {
        return;
    }
    let Ok(e) = loom_commands::skin::entry(&app.state) else { return };
    let display = e.data.display.clone();
    if app.state.skin_filter_envelopes && !display.no_envelopes {
        for env in loom_commands::skin::shown_envelopes_world(&app.state) {
            draw_envelope(painter, app, projection, &env, &display);
        }
    }
    draw_lattices(painter, app, projection, &e);
    draw_mirror_plane(painter, app, projection, &e);
    let Some(screens) = point_screens(app, projection, false) else { return };
    if display.all_vertices {
        // Show All Vertices: a tick at every vertex (and patch handle).
        for sp in screens.iter().flatten() {
            painter.line_segment([*sp + egui::vec2(-2.0, 0.0), *sp + egui::vec2(2.0, 0.0)], egui::Stroke::new(1.0_f32, egui::Color32::from_gray(200)));
        }
    }
    let selection: std::collections::BTreeSet<u32> = loom_commands::skin::selection(&app.state).into_iter().collect();
    let partners = loom_commands::skin::mirror_partners(&app.state);
    let sides = loom_commands::skin::mirror_sides(&app.state);
    if app.state.skin_filter_vertices || sides.is_some() {
        let colors = loom_commands::skin::point_colors(&app.state).map(|(_, c)| c).unwrap_or_default();
        for (i, sp) in screens.iter().enumerate() {
            let Some(p) = *sp else { continue };
            let selected = selection.contains(&(i as u32));
            let colour = match sides.as_ref().and_then(|s| s.get(i)) {
                // Mirror Mode: the side colours; the partner of a selected vertex brighter.
                Some(side) => {
                    let base = match side { -1 => GREEN_SIDE, 1 => BLUE_SIDE, _ => RED_SIDE };
                    if partners.contains(&(i as u32)) { egui::Color32::from_rgb(base.r().saturating_add(90), base.g().saturating_add(90), base.b().saturating_add(90)) } else { base }
                }
                None if display.colored_vertices => colors.get(i).map(|c| egui::Color32::from_rgb(c[0], c[1], c[2])).unwrap_or(egui::Color32::from_gray(40)),
                None => egui::Color32::from_rgb(40, 40, 40),
            };
            if selected {
                painter.rect_filled(egui::Rect::from_center_size(p, egui::vec2(7.0, 7.0)), 0.0, if sides.is_some() { YELLOW } else { egui::Color32::WHITE });
            }
            painter.rect_filled(egui::Rect::from_center_size(p, egui::vec2(if selected { 5.0 } else { 4.0 }, if selected { 5.0 } else { 4.0 })), 0.0, colour);
        }
    }
    // Weight Table: a double-clicked vertex ID shows in pink.
    if let Some(Some(p)) = app.state.skin_ui.highlight.and_then(|v| screens.get(v as usize).copied()) {
        painter.rect_filled(egui::Rect::from_center_size(p, egui::vec2(9.0, 9.0)), 0.0, PINK);
    }
}

/// The current gizmo's lattice (every gizmo's with Show All Gizmos): its
/// lines and points, the picked points yellow while Edit Lattice is on.
fn draw_lattices(painter: &egui::Painter, app: &LoomApp, projection: &Projection, e: &loom_commands::skin::Entry) {
    for (gi, g) in e.data.gizmos.iter().enumerate() {
        let current = app.state.skin_gizmo == Some(gi);
        if !current && !e.data.display.all_gizmos {
            continue;
        }
        let Some(lat) = g.lattice.as_ref() else { continue };
        let Some(pts) = loom_commands::skin::lattice_points_of(&app.state, &e.data, g) else { continue };
        let [nx, ny, nz] = lat.dims.map(|d| d.max(2) as usize);
        let colour = if current { egui::Color32::from_rgb(255, 160, 40) } else { egui::Color32::from_rgb(150, 110, 60) };
        let idx = |i: usize, j: usize, k: usize| loom_mesh::ffd::index(lat.dims, i, j, k);
        for k in 0..nz {
            for j in 0..ny {
                for i in 0..nx {
                    let a = pts[idx(i, j, k)];
                    for (di, dj, dk) in [(1, 0, 0), (0, 1, 0), (0, 0, 1)] {
                        let (ii, jj, kk) = (i + di, j + dj, k + dk);
                        if ii < nx && jj < ny && kk < nz {
                            seg(painter, app, projection, a, pts[idx(ii, jj, kk)], colour, true);
                        }
                    }
                }
            }
        }
        if current && app.state.skin_ui.edit_lattice {
            for (i, q) in pts.iter().enumerate() {
                if let Some(sp) = projection.screen(*q) {
                    let sel = app.state.skin_ui.lattice_selection.contains(&(i as u32));
                    painter.circle_filled(sp, if sel { 4.0 } else { 3.0 }, if sel { YELLOW } else { colour });
                }
            }
        }
    }
}

/// Mirror Mode: the mirror plane at the object's pivot (shifted by Mirror Offset), across its bounds.
fn draw_mirror_plane(painter: &egui::Painter, app: &LoomApp, projection: &Projection, e: &loom_commands::skin::Entry) {
    let m = &e.data.mirror;
    if !m.mode {
        return;
    }
    let (lo, hi) = e.input.aabb();
    let a = m.plane.min(2) as usize;
    let (b, c) = ((a + 1) % 3, (a + 2) % 3);
    let world = app.state.scene.object_matrix(e.id);
    let corner = |x: f32, y: f32| {
        let mut p = Vec3::ZERO;
        p[a] = m.offset;
        p[b] = x;
        p[c] = y;
        world.transform_point3(p)
    };
    let pad = (hi - lo).max_element() * 0.1;
    let q = [corner(lo[b] - pad, lo[c] - pad), corner(hi[b] + pad, lo[c] - pad), corner(hi[b] + pad, hi[c] + pad), corner(lo[b] - pad, hi[c] + pad)];
    for i in 0..4 {
        seg(painter, app, projection, q[i], q[(i + 1) % 4], egui::Color32::from_rgb(255, 210, 60), true);
    }
}

/// The current envelope.
fn current_envelope(app: &LoomApp) -> Option<EnvelopeWorld> {
    loom_commands::skin::shown_envelopes_world(&app.state).into_iter().find(|e| e.current)
}

/// The axis parameter (0..1) of the current envelope nearest the pointer, within 10 px.
fn axis_u_at(env: &EnvelopeWorld, projection: &Projection, p: egui::Pos2) -> Option<f32> {
    (0..=100).map(|i| i as f32 / 100.0).filter_map(|u| projection.screen(env.point(u)).map(|sp| (u, sp.distance(p)))).filter(|(_, d)| *d <= 10.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(u, _)| u)
}

/// The current envelope's end point (0 start, 1 end) under the pointer, with its distance in pixels.
fn end_point_at(app: &LoomApp, projection: &Projection, p: egui::Pos2) -> Option<(u8, f32)> {
    if !app.state.skin_filter_envelopes {
        return None;
    }
    let env = current_envelope(app)?;
    [env.start, env.end].iter().enumerate().filter_map(|(k, q)| projection.screen(*q).map(|sp| (k as u8, sp.distance(p)))).filter(|(_, d)| *d <= 7.0).min_by(|a, b| a.1.total_cmp(&b.1))
}

/// The nearest pickable vertex to the pointer within 8 px (Select Vertices on): (vertex, distance).
fn vertex_at(app: &LoomApp, projection: &Projection, p: egui::Pos2) -> Option<(u32, f32)> {
    if !app.state.skin_filter_vertices {
        return None;
    }
    let pts = point_screens(app, projection, app.state.skin_backface_cull)?;
    pts.iter().enumerate().filter_map(|(i, sp)| sp.map(|sp| (i as u32, sp.distance(p)))).filter(|(_, d)| *d <= 8.0).min_by(|a, b| a.1.total_cmp(&b.1))
}

/// What a click at `p` lands on: the nearest of a vertex, an end point and a cross-section point.
enum Hit {
    Vertex(u32),
    End(u8),
    Section(usize, bool, Vec3, Vec3),
}

fn hit_at(app: &LoomApp, projection: &Projection, p: egui::Pos2) -> Option<Hit> {
    let mut best: Option<(f32, Hit)> = vertex_at(app, projection, p).map(|(v, d)| (d, Hit::Vertex(v)));
    if let Some((k, d)) = end_point_at(app, projection, p) {
        if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
            best = Some((d, Hit::End(k)));
        }
    }
    if let Some((k, outer, c, a, d)) = section_at(app, projection, p) {
        if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
            best = Some((d, Hit::Section(k, outer, c, a)));
        }
    }
    best.map(|(_, h)| h)
}

/// The current envelope's cross-section control point under the pointer:
/// (section, outer ring, its centre, the axis there, distance in pixels).
fn section_at(app: &LoomApp, projection: &Projection, p: egui::Pos2) -> Option<(usize, bool, Vec3, Vec3, f32)> {
    if !app.state.skin_filter_cross_sections {
        return None;
    }
    let env = current_envelope(app)?;
    let mut best: Option<(f32, usize, bool)> = None;
    for (k, (u, i, o)) in env.sections().iter().enumerate() {
        for (r, outer) in [(*i, false), (*o, true)] {
            for q in ring_points(&env, *u, r) {
                if let Some(sp) = projection.screen(q) {
                    let d = sp.distance(p);
                    if d <= 7.0 && best.is_none_or(|(bd, _, _)| d < bd) {
                        best = Some((d, k, outer));
                    }
                }
            }
        }
    }
    let (d, k, outer) = best?;
    let u = env.sections()[k].0;
    Some((k, outer, env.point(u), env.direction(u), d))
}

/// Click / region selection at the Envelope level: Add cross section on
/// the bone, lattice points (Edit Lattice), the envelope's end points and
/// cross sections, vertices (Select Vertices on, Backface Cull Vertices and
/// hidden vertices honoured), else a bone; a click on nothing clears the
/// vertex selection.
pub fn pick(app: &mut LoomApp, projection: &Projection, region: Option<&crate::region::Region>, p: egui::Pos2, mods: loom_input::Modifiers) -> bool {
    if !loom_commands::skin::picks_vertices(&app.state) {
        return false;
    }
    let op = if mods.ctrl { SelectOp::Add } else if mods.alt { SelectOp::Subtract } else { SelectOp::Replace };
    if app.state.skin_ui.adding_cross_section {
        // Cross Sections > Add: the click on the bone places the section.
        if let Some(u) = current_envelope(app).and_then(|env| axis_u_at(&env, projection, p)) {
            let _ = app.dispatch(Command::SkinAddCrossSectionAt { u });
        }
        return true;
    }
    if app.state.skin_ui.edit_lattice {
        let pts = loom_commands::skin::lattice_world_points(&app.state).unwrap_or_default();
        let screens: Vec<Option<egui::Pos2>> = pts.iter().map(|q| projection.screen(*q)).collect();
        let hits: Vec<u32> = match region {
            Some(r) => screens.iter().enumerate().filter_map(|(i, sp)| sp.filter(|sp| r.contains(*sp)).map(|_| i as u32)).collect(),
            None => screens.iter().enumerate().filter_map(|(i, sp)| sp.map(|sp| (i as u32, sp.distance(p)))).filter(|(_, d)| *d <= 8.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| vec![i]).unwrap_or_default(),
        };
        if !hits.is_empty() || op == SelectOp::Replace {
            let _ = app.dispatch(Command::SkinSelectLatticePoints { indices: hits, op });
        }
        return true;
    }
    if region.is_none() {
        match hit_at(app, projection, p) {
            Some(Hit::End(k)) => {
                let _ = app.dispatch(if k == 0 { Command::SelectStartPointSkin } else { Command::SelectEndPointSkin });
                return true;
            }
            Some(Hit::Section(k, outer, _, _)) => {
                let _ = app.dispatch(Command::SkinSelectCrossSection { index: k as u32, outer });
                return true;
            }
            _ => {}
        }
    }
    let pts: Vec<Option<egui::Pos2>> = if app.state.skin_filter_vertices { point_screens(app, projection, app.state.skin_backface_cull).unwrap_or_default() } else { Vec::new() };
    let hits: Vec<u32> = match region {
        Some(r) => pts.iter().enumerate().filter_map(|(i, sp)| sp.filter(|sp| r.contains(*sp)).map(|_| i as u32)).collect(),
        None => pts.iter().enumerate().filter_map(|(i, sp)| sp.map(|sp| (i as u32, sp.distance(p)))).filter(|(_, d)| *d <= 8.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| vec![i]).unwrap_or_default(),
    };
    if hits.is_empty() && region.is_none() {
        // A click on a bone makes it the current bone.
        if let Some(bone) = pick_bone(app, p, projection) {
            let _ = app.dispatch(Command::SkinSelectBone { id: bone.0 });
            return true;
        }
    }
    if hits.is_empty() && op != SelectOp::Replace {
        return true;
    }
    let _ = app.dispatch(Command::SkinSelectVertices { ids: hits, op });
    true
}

/// The Skin bone whose projected axis is nearest the pointer (within 10 px).
fn pick_bone(app: &LoomApp, p: egui::Pos2, projection: &Projection) -> Option<NodeId> {
    let e = loom_commands::skin::entry(&app.state).ok()?;
    let mut best: Option<(f32, NodeId)> = None;
    for b in &e.data.bones {
        let w = app.state.scene.world_matrix(b.node);
        let len = app.state.scene.node(b.node).and_then(|n| n.object.bone_length()).unwrap_or(1.0);
        let (Some(a), Some(c)) = (projection.screen(w.transform_point3(Vec3::ZERO)), projection.screen(w.transform_point3(Vec3::X * len))) else { continue };
        let ab = c - a;
        let t = if ab.length_sq() < 1e-6 { 0.0 } else { ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) };
        let d = (a + ab * t).distance(p);
        if d <= 10.0 && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, b.node));
        }
    }
    best.map(|(_, id)| id)
}

/// A drag on an envelope end point or a cross-section control point.
#[derive(Debug, Clone, Copy)]
enum SkinDrag {
    End { end: u8, pending: Option<Vec3> },
    Section { centre: Vec3, axis: Vec3, pending: Option<f32> },
}

/// Dragging an envelope's end point moves it in the view plane; dragging a
/// cross-section control point sets that ring's radius (with Update on
/// mouse up the change applies on release). Returns true while it owns the button.
pub fn drag(app: &mut LoomApp, ctx: &egui::Context, pointer: &egui::PointerState, hovered: bool, p: egui::Pos2, projection: &Projection) -> bool {
    let id = egui::Id::new("skin_envelope_drag");
    let mut state: Option<SkinDrag> = ctx.data(|d| d.get_temp(id)).flatten();
    if state.is_none() {
        if !pointer.button_pressed(egui::PointerButton::Primary) || !hovered || !loom_commands::skin::picks_vertices(&app.state) {
            return false;
        }
        if loom_commands::skin::painting(&app.state) || app.state.skin_ui.adding_cross_section || app.state.skin_ui.edit_lattice || app.transform_drag.is_some() || app.state.nav_mode != loom_input::NavMode::None {
            return false;
        }
        match hit_at(app, projection, p) {
            Some(Hit::End(end)) => {
                let _ = app.dispatch(if end == 0 { Command::SelectStartPointSkin } else { Command::SelectEndPointSkin });
                app.state.undo.begin("Move Envelope");
                state = Some(SkinDrag::End { end, pending: None });
            }
            Some(Hit::Section(k, outer, centre, axis)) => {
                let _ = app.dispatch(Command::SkinSelectCrossSection { index: k as u32, outer });
                app.state.undo.begin("Cross Section Radius");
                state = Some(SkinDrag::Section { centre, axis, pending: None });
            }
            // A vertex (or nothing) under the pointer: the click picks it.
            _ => return false,
        }
        app.poly_dragging = true;
        app.drag_select_start = None;
        ctx.data_mut(|d| d.insert_temp(id, state));
        return true;
    }
    let mouse_up = loom_commands::skin::entry(&app.state).is_ok_and(|e| e.data.update_on_mouse_up);
    let mut s = state.expect("checked");
    if pointer.delta() != egui::Vec2::ZERO {
        match &mut s {
            SkinDrag::End { end, pending } => {
                let at = current_envelope(app).map(|env| if *end == 0 { env.start } else { env.end });
                if let Some(to) = at.and_then(|at| projection.hit_plane(p, at, projection.toward_viewer_at(at))) {
                    if mouse_up {
                        *pending = Some(to);
                    } else {
                        let _ = app.dispatch(Command::SkinMoveEnvelopePoint { end: *end as u32, position: to.to_array() });
                    }
                }
            }
            SkinDrag::Section { centre, axis, pending } => {
                let r = projection.hit_plane(p, *centre, *axis).map(|h| h.distance(*centre)).unwrap_or_else(|| projection.screen(*centre).map(|c| c.distance(p) * projection.units_per_pixel(*centre)).unwrap_or(0.0));
                if mouse_up {
                    *pending = Some(r);
                } else {
                    let _ = app.dispatch(Command::SkinSetCrossSectionRadius { radius: r });
                }
            }
        }
        ctx.request_repaint();
    }
    app.drag_select_start = None;
    if pointer.button_released(egui::PointerButton::Primary) || !pointer.button_down(egui::PointerButton::Primary) {
        match s {
            SkinDrag::End { end, pending: Some(to) } => {
                let _ = app.dispatch(Command::SkinMoveEnvelopePoint { end: end as u32, position: to.to_array() });
            }
            SkinDrag::Section { pending: Some(r), .. } => {
                let _ = app.dispatch(Command::SkinSetCrossSectionRadius { radius: r });
            }
            _ => {}
        }
        app.state.undo.end();
        app.poly_dragging = false;
        ctx.data_mut(|d| d.insert_temp::<Option<SkinDrag>>(id, None));
        return true;
    }
    ctx.data_mut(|d| d.insert_temp(id, Some(s)));
    true
}
