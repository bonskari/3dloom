//! Scene tab: the object hierarchy as a tree. Click selects (Ctrl adds,
//! Alt removes); children sit under their parent behind an expander.
//! A small type glyph precedes each name.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, SelectOp};
use loom_scene::{NodeId, ObjectKind, Scene};

/// The type glyph a Scene row draws before the object's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconKind {
    /// Editable Poly and the primitives.
    Geometry,
    /// Shapes and Editable Splines.
    Shape,
    Camera,
    /// Dummy, Point and IK goals.
    Helper,
    Bone,
    Group,
    /// Boolean, ProBoolean, Loft.
    Compound,
}

pub fn icon_kind(object: &ObjectKind) -> IconKind {
    use ObjectKind as O;
    match object {
        o if o.is_shape() => IconKind::Shape,
        O::Camera { .. } => IconKind::Camera,
        O::Dummy { .. } | O::Point { .. } | O::IkGoal { .. } => IconKind::Helper,
        O::Bone { .. } => IconKind::Bone,
        O::Group { .. } => IconKind::Group,
        O::Boolean { .. } | O::ProBoolean { .. } | O::Loft { .. } => IconKind::Compound,
        _ => IconKind::Geometry,
    }
}

impl IconKind {
    pub fn color(self) -> egui::Color32 {
        match self {
            IconKind::Geometry => egui::Color32::from_rgb(110, 170, 235),
            IconKind::Shape => egui::Color32::from_rgb(235, 190, 80),
            IconKind::Camera => egui::Color32::from_rgb(120, 200, 210),
            IconKind::Helper => egui::Color32::from_rgb(120, 200, 110),
            IconKind::Bone => egui::Color32::from_rgb(215, 180, 140),
            IconKind::Group => egui::Color32::from_rgb(200, 200, 200),
            IconKind::Compound => egui::Color32::from_rgb(190, 140, 230),
        }
    }
}

/// Draws the glyph in a 14 px square at the cursor.
fn icon(ui: &mut egui::Ui, kind: IconKind) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    let c = kind.color();
    let stroke = egui::Stroke::new(1.4, c);
    let r = rect.shrink(2.0);
    let at = |x: f32, y: f32| egui::pos2(r.left() + x * r.width(), r.top() + y * r.height());
    match kind {
        IconKind::Geometry => {
            // An isometric cube: lit top, two shaded sides.
            let (top, left, right) = (at(0.5, 0.0), at(0.0, 0.25), at(1.0, 0.25));
            let (mid, bl, br, bottom) = (at(0.5, 0.5), at(0.0, 0.75), at(1.0, 0.75), at(0.5, 1.0));
            p.add(egui::Shape::convex_polygon(vec![top, right, mid, left], c, egui::Stroke::NONE));
            p.add(egui::Shape::convex_polygon(vec![left, mid, bottom, bl], c.gamma_multiply(0.7), egui::Stroke::NONE));
            p.add(egui::Shape::convex_polygon(vec![mid, right, br, bottom], c.gamma_multiply(0.5), egui::Stroke::NONE));
        }
        IconKind::Shape => {
            let pts: Vec<egui::Pos2> = (0..=12).map(|i| i as f32 / 12.0).map(|t| at(t, 0.5 - 0.4 * (t * std::f32::consts::TAU).sin())).collect();
            p.line(pts, stroke);
        }
        IconKind::Camera => {
            p.rect_filled(egui::Rect::from_min_max(at(0.0, 0.25), at(0.62, 0.8)), 1.0, c);
            p.add(egui::Shape::convex_polygon(vec![at(0.62, 0.52), at(1.0, 0.25), at(1.0, 0.8)], c, egui::Stroke::NONE));
        }
        IconKind::Helper => {
            p.rect_stroke(egui::Rect::from_min_max(at(0.15, 0.15), at(0.85, 0.85)), 0.0, stroke);
            p.line_segment([at(0.5, 0.0), at(0.5, 1.0)], stroke);
            p.line_segment([at(0.0, 0.5), at(1.0, 0.5)], stroke);
        }
        IconKind::Bone => {
            p.add(egui::Shape::convex_polygon(vec![at(0.0, 0.5), at(0.3, 0.2), at(1.0, 0.5), at(0.3, 0.8)], c, egui::Stroke::NONE));
        }
        IconKind::Group => {
            // Brackets: [ ]
            for (x0, x1) in [(0.3, 0.0), (0.7, 1.0)] {
                p.line(vec![at(x0, 0.0), at(x1, 0.0), at(x1, 1.0), at(x0, 1.0)], stroke);
            }
        }
        IconKind::Compound => {
            p.circle_stroke(at(0.36, 0.5), r.width() * 0.34, stroke);
            p.circle_stroke(at(0.64, 0.5), r.width() * 0.34, stroke);
        }
    }
}

/// The scene's parent → children table, children in scene order.
pub fn children_of(scene: &Scene) -> std::collections::BTreeMap<Option<NodeId>, Vec<NodeId>> {
    let mut map: std::collections::BTreeMap<Option<NodeId>, Vec<NodeId>> = Default::default();
    for n in scene.nodes() {
        // A parent that no longer exists puts the node at the top level.
        let parent = n.parent.filter(|p| scene.node(*p).is_some());
        map.entry(parent).or_default().push(n.id);
    }
    map
}

/// The selection a click on a row makes: Ctrl adds, Alt removes, plain replaces.
pub fn click_op(mods: egui::Modifiers) -> SelectOp {
    if mods.ctrl || mods.command {
        SelectOp::Add
    } else if mods.alt {
        SelectOp::Subtract
    } else {
        SelectOp::Replace
    }
}

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    let tree = children_of(&app.state.scene);
    let roots = tree.get(&None).cloned().unwrap_or_default();
    if roots.is_empty() {
        ui.weak("The scene is empty.");
        return;
    }
    let mut clicked: Option<NodeId> = None;
    for id in roots {
        row(ui, app, &tree, id, &mut clicked);
    }
    if let Some(id) = clicked {
        let op = click_op(ui.input(|i| i.modifiers));
        let _ = app.dispatch(Command::SelectNodes { ids: vec![id.0], op });
    }
}

fn row(ui: &mut egui::Ui, app: &LoomApp, tree: &std::collections::BTreeMap<Option<NodeId>, Vec<NodeId>>, id: NodeId, clicked: &mut Option<NodeId>) {
    let Some(n) = app.state.scene.node(id) else { return };
    let selected = app.state.selection.contains(&id);
    let mut text = egui::RichText::new(&n.name);
    if n.hidden {
        text = text.weak().italics();
    }
    let kind = icon_kind(&n.object);
    let kids = tree.get(&Some(id)).cloned().unwrap_or_default();
    if kids.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            icon(ui, kind);
            if ui.add(egui::SelectableLabel::new(selected, text)).clicked() {
                *clicked = Some(id);
            }
        });
        return;
    }
    let state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui.make_persistent_id(("scene_tree", id.0)), false);
    state
        .show_header(ui, |ui| {
            icon(ui, kind);
            if ui.add(egui::SelectableLabel::new(selected, text)).clicked() {
                *clicked = Some(id);
            }
        })
        .body(|ui| {
            for k in kids {
                row(ui, app, tree, k, clicked);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_commands::{execute, AppState, PrimitiveKind};

    #[test]
    fn children_hang_under_their_parent() {
        let mut s = AppState::new();
        let mut ids = Vec::new();
        for _ in 0..3 {
            execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [1.0; 3] }).unwrap();
            ids.push(*s.selection.iter().next().unwrap());
        }
        execute(&mut s, Command::SetParent { id: ids[1].0, parent: Some(ids[0].0) }).unwrap();
        let t = children_of(&s.scene);
        assert_eq!(t[&None], vec![ids[0], ids[2]]);
        assert_eq!(t[&Some(ids[0])], vec![ids[1]]);
    }

    #[test]
    fn icon_kind_follows_the_object_kind() {
        let mut s = AppState::new();
        let run = |s: &mut AppState, c: Command| {
            execute(s, c).unwrap();
            let id = *s.selection.iter().next().unwrap();
            icon_kind(&s.scene.node(id).unwrap().object)
        };
        assert_eq!(run(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [1.0; 3] }), IconKind::Geometry);
        assert_eq!(run(&mut s, Command::ConvertToEditablePoly), IconKind::Geometry);
        assert_eq!(run(&mut s, Command::Group { name: None }), IconKind::Group);
        assert_eq!(run(&mut s, Command::CreateShape { kind: loom_commands::ShapeKind::Circle, position: [0.0; 3], size: [1.0; 3] }), IconKind::Shape);
        assert_eq!(run(&mut s, Command::ConvertToEditableSpline), IconKind::Shape);
        assert_eq!(run(&mut s, Command::CreateCameraAt { target: false, position: [0.0, -5.0, 0.0], look_at: [0.0; 3] }), IconKind::Camera);
        assert_eq!(run(&mut s, Command::CreateHelper { kind: "dummy".into(), position: [0.0; 3], size: 1.0 }), IconKind::Helper);
        assert_eq!(run(&mut s, Command::CreateHelper { kind: "point".into(), position: [0.0; 3], size: 1.0 }), IconKind::Helper);
        assert_eq!(run(&mut s, Command::CreateBoneChain { points: vec![[0.0; 3], [1.0, 0.0, 0.0]], name: None }), IconKind::Bone);
        assert_eq!(run(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Sphere, position: [0.0; 3], size: [1.0; 3] }), IconKind::Geometry);
        assert_eq!(run(&mut s, Command::CreateProBoolean), IconKind::Compound);
    }

    #[test]
    fn modifier_keys_pick_the_selection_op() {
        assert_eq!(click_op(egui::Modifiers::NONE), SelectOp::Replace);
        assert_eq!(click_op(egui::Modifiers::CTRL), SelectOp::Add);
        assert_eq!(click_op(egui::Modifiers::ALT), SelectOp::Subtract);
    }
}
