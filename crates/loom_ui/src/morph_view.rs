//! Viewport side of Skin Morph and Skin Wrap: the bones, morph vertices,
//! matrices, angle wedges and mirror previews of Skin Morph; the control
//! vertices, volumes of influence, influence colours, Face Limit, unassigned
//! points and mirror data of Skin Wrap; and their click / region picks.

use crate::gizmo::Projection;
use crate::LoomApp;
use eframe::egui;
use glam::{Mat4, Quat, Vec3};
use loom_commands::Command;
use loom_scene::morph::{SkinMorphData, SkinWrapData};
use loom_scene::{Modifier, NodeId};
use std::collections::BTreeSet;

const ORANGE: egui::Color32 = egui::Color32::from_rgb(255, 140, 20);
const YELLOW: egui::Color32 = egui::Color32::from_rgb(255, 235, 40);
const RED: egui::Color32 = egui::Color32::from_rgb(235, 30, 30);
const BLUE: egui::Color32 = egui::Color32::from_rgb(40, 90, 255);
const WHITE: egui::Color32 = egui::Color32::from_rgb(240, 240, 240);

/// The selected node's active stack entry when it is a Skin Morph: (node, index, data, level).
fn active_skin_morph(app: &LoomApp) -> Option<(NodeId, usize, SkinMorphData, u32)> {
    app.state.selection.iter().find_map(|id| {
        let n = app.state.scene.node(*id)?;
        let a = n.active_entry?;
        let slot = n.modifiers.get(a)?;
        match &slot.modifier {
            Modifier::SkinMorph(d) => Some((*id, a, (**d).clone(), slot.sub_level)),
            _ => None,
        }
    })
}

fn active_skin_wrap(app: &LoomApp) -> Option<(NodeId, usize, SkinWrapData, u32)> {
    app.state.selection.iter().find_map(|id| {
        let n = app.state.scene.node(*id)?;
        let a = n.active_entry?;
        let slot = n.modifiers.get(a)?;
        match &slot.modifier {
            Modifier::SkinWrap(d) => Some((*id, a, (**d).clone(), slot.sub_level)),
            _ => None,
        }
    })
}

fn rotation_of(m: Mat4) -> Quat {
    m.to_scale_rotation_translation().1.normalize()
}

fn segment(painter: &egui::Painter, pr: &Projection, a: Vec3, b: Vec3, stroke: egui::Stroke) {
    if let (Some(p), Some(q)) = (pr.screen(a), pr.screen(b)) {
        painter.line_segment([p, q], stroke);
    }
}

/// An axis tripod: X red, Y green, Z blue.
fn tripod(painter: &egui::Painter, pr: &Projection, origin: Vec3, rot: Quat, size: f32) {
    for (axis, c) in [(Vec3::X, egui::Color32::from_rgb(230, 40, 40)), (Vec3::Y, egui::Color32::from_rgb(40, 210, 40)), (Vec3::Z, egui::Color32::from_rgb(50, 90, 255))] {
        segment(painter, pr, origin, origin + rot * axis * size, egui::Stroke::new(1.5_f32, c));
    }
}

/// A circle (or an arc from 0 to `sweep` radians, closed to the centre as a wedge).
fn arc(painter: &egui::Painter, pr: &Projection, centre: Vec3, u: Vec3, v: Vec3, radius: f32, sweep: f32, wedge: bool, stroke: egui::Stroke) {
    let n = 32;
    let mut pts: Vec<egui::Pos2> = (0..=n).filter_map(|k| {
        let a = sweep * k as f32 / n as f32;
        pr.screen(centre + (u * a.cos() + v * a.sin()) * radius)
    }).collect();
    if wedge {
        if let Some(c) = pr.screen(centre) {
            pts.insert(0, c);
            pts.push(c);
        }
    }
    painter.add(egui::Shape::line(pts, stroke));
}

/// A square of half-size `size` around `centre`, perpendicular to world axis `axis`.
fn mirror_plane(painter: &egui::Painter, pr: &Projection, axis: u32, centre: Vec3, size: f32) {
    let a = axis.min(2) as usize;
    let (u, v) = match a {
        0 => (Vec3::Y, Vec3::Z),
        1 => (Vec3::X, Vec3::Z),
        _ => (Vec3::X, Vec3::Y),
    };
    let c = [centre + (u + v) * size, centre + (u - v) * size, centre - (u + v) * size, centre - (u - v) * size];
    for i in 0..4 {
        segment(painter, pr, c[i], c[(i + 1) % 4], egui::Stroke::new(1.5_f32, RED));
    }
}

/// The world bounds of points: (centre, half the largest extent).
fn bounds(points: &[Vec3]) -> (Vec3, f32) {
    if points.is_empty() {
        return (Vec3::ZERO, 10.0);
    }
    let (lo, hi) = points.iter().fold((Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
    ((lo + hi) * 0.5, ((hi - lo).max_element() * 0.6).max(1.0))
}

pub fn draw(painter: &egui::Painter, app: &LoomApp, pr: &Projection) {
    if let Some((id, index, d, level)) = active_skin_morph(app) {
        draw_skin_morph(painter, app, pr, id, index, &d, level);
    }
    if let Some((id, index, d, level)) = active_skin_wrap(app) {
        draw_skin_wrap(painter, app, pr, id, index, &d, level);
    }
}

fn draw_skin_morph(painter: &egui::Painter, app: &LoomApp, pr: &Projection, id: NodeId, index: usize, d: &SkinMorphData, level: u32) {
    let scene = &app.state.scene;
    let world = |n: NodeId| Some(scene.world_matrix(n));
    let current = app.state.skin_morph_current.and_then(|m| d.morphs.get(m).map(|x| (m, x)));
    let current_bone = current.map(|(_, m)| m.bone).or_else(|| app.state.skin_morph_bone.and_then(|b| d.bones.iter().position(|x| x.node == b)));
    // Each listed bone: an orange line through it; the highlighted one thicker, yellow.
    for (i, b) in d.bones.iter().enumerate() {
        let w = scene.world_matrix(b.node);
        let len = scene.node(b.node).and_then(|n| n.object.bone_length()).unwrap_or(10.0);
        let hi = current_bone == Some(i);
        let width = d.options.bone_size.max(1.0) + if hi { 2.0 } else { 0.0 };
        segment(painter, pr, w.transform_point3(Vec3::ZERO), w.transform_point3(Vec3::X * len), egui::Stroke::new(width, if hi { YELLOW } else { ORANGE }));
    }
    let Some((_, _, pts)) = loom_commands::morph::skin_morph_points(&app.state) else { return };
    let screen: Vec<Option<egui::Pos2>> = pts.iter().map(|p| pr.screen(*p)).collect();
    let morph_verts: BTreeSet<u32> = d.morphs.iter().flat_map(|m| m.deltas.iter().map(|(v, _)| *v)).collect();
    let current_verts: BTreeSet<u32> = current.map(|(_, m)| m.deltas.iter().map(|(v, _)| *v).collect()).unwrap_or_default();
    if level == 1 {
        if d.options.show_edges && !morph_verts.is_empty() {
            // Show Edges: the edges touching morphable vertices, orange.
            let mesh = scene.evaluated(id, Some(index)).value.mesh.clone();
            let he = mesh.half_edges();
            for h in he {
                let (a, b) = (h.origin.0, he[h.next.0 as usize].origin.0);
                if a < b && (morph_verts.contains(&a) || morph_verts.contains(&b)) {
                    if let (Some(Some(p)), Some(Some(q))) = (screen.get(a as usize), screen.get(b as usize)) {
                        painter.line_segment([*p, *q], egui::Stroke::new(1.0_f32, ORANGE));
                    }
                }
            }
        }
        let selected: BTreeSet<u32> = d.selection.iter().copied().collect();
        for (i, s) in screen.iter().enumerate() {
            let Some(s) = s else { continue };
            let v = i as u32;
            let (c, size) = if selected.contains(&v) {
                (RED, 5.0)
            } else if current_verts.contains(&v) {
                (YELLOW, 4.0)
            } else if morph_verts.contains(&v) {
                (ORANGE, 4.0)
            } else {
                (egui::Color32::from_rgb(30, 30, 60), 2.5)
            };
            painter.rect_filled(egui::Rect::from_center_size(*s, egui::vec2(size, size)), 0.0, c);
        }
    }
    let size = d.options.matrix_size.max(0.1);
    if let Some(bi) = current_bone {
        let bw = scene.world_matrix(d.bones[bi].node);
        let origin = bw.transform_point3(Vec3::ZERO);
        if d.options.show_driver_matrix {
            tripod(painter, pr, origin, rotation_of(bw), size);
        }
        if let Some((_, m)) = current {
            if let Some(pose) = m.pose {
                let parent = d.bones[bi].parent.map(|p| rotation_of(scene.world_matrix(p))).unwrap_or(Quat::IDENTITY);
                let morph_rot = parent * pose;
                if d.options.show_morph_matrix {
                    tripod(painter, pr, origin, morph_rot, size * 0.8);
                }
                if d.options.show_current_angle {
                    // Pie wedges of the angles between the driver and the morph matrix: X red, Y blue, Z green.
                    if let Some(cur) = d.local_rotation(bi, &world) {
                        let (ax, ay, az) = (pose.inverse() * cur).to_euler(glam::EulerRot::XYZ);
                        let r = size * 0.6;
                        let stroke = |c| egui::Stroke::new(1.5_f32, c);
                        arc(painter, pr, origin, morph_rot * Vec3::Y, morph_rot * Vec3::Z, r, ax, true, stroke(egui::Color32::from_rgb(230, 40, 40)));
                        arc(painter, pr, origin, morph_rot * Vec3::Z, morph_rot * Vec3::X, r, ay, true, stroke(egui::Color32::from_rgb(50, 90, 255)));
                        arc(painter, pr, origin, morph_rot * Vec3::X, morph_rot * Vec3::Y, r, az, true, stroke(egui::Color32::from_rgb(40, 210, 40)));
                    }
                }
            }
        }
        // Copy and Paste previews.
        if d.mirror.preview_bone {
            if let Some(t) = loom_commands::morph::mirror_bone(d, bi, &|n| scene.world_matrix(n)) {
                let w = scene.world_matrix(d.bones[t].node);
                let len = scene.node(d.bones[t].node).and_then(|n| n.object.bone_length()).unwrap_or(10.0);
                segment(painter, pr, w.transform_point3(Vec3::ZERO), w.transform_point3(Vec3::X * len), egui::Stroke::new(d.options.bone_size.max(1.0) + 2.0, RED));
            }
        }
        if d.mirror.preview_vertices {
            let base = scene.node(id).map(|n| n.object.display_output()).unwrap_or_default();
            let ow = scene.object_matrix(id);
            let bpos: Vec<Vec3> = base.vertices().iter().map(|v| ow.transform_point3(v.position)).collect();
            for m in d.morphs.iter().filter(|m| m.bone == bi) {
                for (v, _) in &m.deltas {
                    if let Some(Some(s)) = loom_commands::morph::mirror_vertex(&d.mirror, &bpos, *v).and_then(|t| screen.get(t as usize)) {
                        painter.circle_filled(*s, 3.0, RED);
                    }
                }
            }
        }
    }
    if d.mirror.show_plane {
        let (c, r) = bounds(&pts);
        let mut centre = c;
        centre[d.mirror.axis.min(2) as usize] = d.mirror.offset;
        mirror_plane(painter, pr, d.mirror.axis, centre, r);
    }
}

/// Blue (weakest) to orange (strongest).
fn influence_colour(t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t) as u8;
    egui::Color32::from_rgb(l(BLUE.r(), ORANGE.r()), l(BLUE.g(), ORANGE.g()), l(BLUE.b(), ORANGE.b()))
}

fn draw_skin_wrap(painter: &egui::Painter, app: &LoomApp, pr: &Projection, id: NodeId, index: usize, d: &SkinWrapData, level: u32) {
    let scene = &app.state.scene;
    let ow = scene.object_matrix(id);
    let points: Vec<Vec3> = scene.evaluated(id, Some(index)).value.mesh.vertices().iter().map(|v| ow.transform_point3(v.position)).collect();
    let screen: Vec<Option<egui::Pos2>> = points.iter().map(|p| pr.screen(*p)).collect();
    if d.display.unassigned {
        // A red circle: no closest face; a red box: a closest face but no weights.
        for (i, s) in screen.iter().enumerate() {
            let Some(s) = s else { continue };
            match d.closest.get(i) {
                Some(None) => {
                    painter.circle_stroke(*s, 5.0, egui::Stroke::new(1.5_f32, RED));
                }
                Some(Some(_)) if d.binds.get(i).is_some_and(|b| b.is_empty()) => {
                    painter.rect_stroke(egui::Rect::from_center_size(*s, egui::vec2(9.0, 9.0)), 0.0, egui::Stroke::new(1.5_f32, RED), egui::StrokeKind::Middle);
                }
                _ => {}
            }
        }
    }
    if level != 1 {
        return;
    }
    let geos = d.control_geometry(scene);
    let selected: BTreeSet<(u32, u32)> = d.selection.iter().copied().collect();
    if d.display.control_verts {
        for (b, g) in geos.iter().enumerate() {
            for (v, p) in g.pos.iter().enumerate() {
                let Some(s) = pr.screen(*p) else { continue };
                let sel = selected.contains(&(b as u32, v as u32));
                painter.rect_filled(egui::Rect::from_center_size(s, egui::vec2(if sel { 7.0 } else { 5.0 }, if sel { 7.0 } else { 5.0 })), 0.0, if sel { RED } else { ORANGE });
            }
        }
    }
    for (b, v) in &d.selection {
        let Some(g) = geos.get(*b as usize) else { continue };
        let Some(frame) = g.frames.get(*v as usize).copied().flatten() else { continue };
        let r = d.cv_frames.get(*b as usize).and_then(|f| f.get(*v as usize)).and_then(|x| x.as_ref()).map(|x| x.1).unwrap_or(1.0);
        let loc = d.local(*b, *v);
        let (o, x, y, z) = frame;
        if d.display.face_limit {
            for i in d.face_limit_points(g, *b, *v) {
                if let Some(Some(s)) = screen.get(i) {
                    painter.circle_stroke(*s, 3.5, egui::Stroke::new(1.0_f32, WHITE));
                }
            }
        }
        // Influence colours: orange strongest … blue weakest; red in range but not weighted.
        let infl = d.influence_of(*b, *v);
        let top = infl.iter().map(|x| x.1).fold(0.0f32, f32::max).max(1e-6);
        for (i, w) in infl {
            if let Some(Some(s)) = screen.get(i) {
                painter.circle_filled(*s, 3.0, influence_colour(w / top));
            }
        }
        for i in d.in_range_unweighted(*b, *v) {
            if let Some(Some(s)) = screen.get(i) {
                painter.circle_filled(*s, 3.0, RED);
            }
        }
        if d.display.loops {
            let stroke = egui::Stroke::new(1.0_f32, RED);
            // The volume of influence: an ellipsoid with radii along the vertex's axes.
            let (rx, ry, rz) = (r * loc.x, r * loc.y, r * loc.z);
            let ellipse = |u: Vec3, ru: f32, v: Vec3, rv: f32| {
                let pts: Vec<egui::Pos2> = (0..=48).filter_map(|k| {
                    let a = std::f32::consts::TAU * k as f32 / 48.0;
                    pr.screen(o + u * (ru * a.cos()) + v * (rv * a.sin()))
                }).collect();
                painter.add(egui::Shape::line(pts, stroke));
            };
            ellipse(x, rx, y, ry);
            ellipse(y, ry, z, rz);
            ellipse(x, rx, z, rz);
        }
        if d.display.axis {
            tripod(painter, pr, o, Quat::from_mat3(&glam::Mat3::from_cols(x, y, z)), r * 0.5);
        }
    }
    if d.mirror.show {
        let all: Vec<Vec3> = geos.iter().flat_map(|g| g.pos.iter().copied()).collect();
        let (c, size) = bounds(&all);
        let mut centre = c;
        centre[d.mirror.axis.min(2) as usize] = d.mirror.offset;
        mirror_plane(painter, pr, d.mirror.axis, centre, size);
        for (b, v) in &d.selection {
            if let Some(p) = geos.get(*b as usize).and_then(|g| g.pos.get(*v as usize)) {
                if let Some(s) = pr.screen(d.mirror.reflect_point(*p)) {
                    painter.circle_stroke(s, 4.0, egui::Stroke::new(1.5_f32, YELLOW));
                }
            }
        }
    }
}

/// Indices of `pts` under a click (the nearest within 8 px) or in a region.
fn hits(pts: &[Option<egui::Pos2>], region: Option<&crate::region::Region>, p: egui::Pos2) -> Vec<usize> {
    match region {
        Some(r) => pts.iter().enumerate().filter_map(|(i, s)| s.filter(|s| r.contains(*s)).map(|_| i)).collect(),
        None => pts.iter().enumerate().filter_map(|(i, s)| s.map(|s| (i, s.distance(p)))).filter(|(_, d)| *d <= 8.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| vec![i]).unwrap_or_default(),
    }
}

/// The Skin Morph bone whose line passes nearest `p` (within 10 px).
fn bone_at(app: &LoomApp, d: &SkinMorphData, pr: &Projection, p: egui::Pos2) -> Option<NodeId> {
    let mut best: Option<(f32, NodeId)> = None;
    for b in &d.bones {
        let w = app.state.scene.world_matrix(b.node);
        let len = app.state.scene.node(b.node).and_then(|n| n.object.bone_length()).unwrap_or(10.0);
        let (Some(a), Some(c)) = (pr.screen(w.transform_point3(Vec3::ZERO)), pr.screen(w.transform_point3(Vec3::X * len))) else { continue };
        let ab = c - a;
        let t = if ab.length_sq() < 1e-6 { 0.0 } else { ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) };
        let dist = (a + ab * t).distance(p);
        if dist <= 10.0 && best.is_none_or(|(bd, _)| dist < bd) {
            best = Some((dist, b.node));
        }
    }
    best.map(|(_, id)| id)
}

/// Clicks and regions at the Skin Morph Points level (vertices; a click on
/// a bone's line highlights the bone) and the Skin Wrap Control Vertices
/// level. Ctrl adds, Alt removes, a click on nothing clears. False when
/// neither level is on.
pub fn pick(app: &mut LoomApp, pr: &Projection, region: Option<&crate::region::Region>, p: egui::Pos2, mods: loom_input::Modifiers) -> bool {
    let op = if mods.ctrl { loom_commands::SelectOp::Add } else if mods.alt { loom_commands::SelectOp::Subtract } else { loom_commands::SelectOp::Replace };
    if loom_commands::morph::points_level(&app.state).is_some() {
        let Some((_, _, pts)) = loom_commands::morph::skin_morph_points(&app.state) else { return false };
        let screen: Vec<Option<egui::Pos2>> = pts.iter().map(|q| pr.screen(*q)).collect();
        let found = hits(&screen, region, p);
        if found.is_empty() && region.is_none() {
            if let Some(d) = active_skin_morph(app).map(|x| x.2) {
                if let Some(bone) = bone_at(app, &d, pr, p) {
                    let _ = app.dispatch(Command::SkinMorphSelect { bone: Some(bone.0), morph: None });
                    return true;
                }
            }
        }
        if found.is_empty() && op != loom_commands::SelectOp::Replace {
            return true;
        }
        let _ = app.dispatch(Command::SkinMorphSelectPoints { ids: found.into_iter().map(|i| i as u32).collect(), op });
        return true;
    }
    if loom_commands::morph::control_vertex_level(&app.state).is_some() {
        let cvs = loom_commands::morph::control_vertices(&app.state);
        let screen: Vec<Option<egui::Pos2>> = cvs.iter().map(|(_, q)| pr.screen(*q)).collect();
        let found = hits(&screen, region, p);
        if found.is_empty() && op != loom_commands::SelectOp::Replace {
            return true;
        }
        let _ = app.dispatch(Command::SkinWrapSelectControlVerts { ids: found.into_iter().map(|i| [cvs[i].0 .0, cvs[i].0 .1]).collect(), op });
        return true;
    }
    false
}
