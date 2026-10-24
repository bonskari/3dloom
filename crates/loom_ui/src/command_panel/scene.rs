//! Scene tab: the object hierarchy as a tree. Click selects (Ctrl adds,
//! Alt removes); children sit under their parent behind an expander.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, SelectOp};
use loom_scene::{NodeId, Scene};

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
    let kids = tree.get(&Some(id)).cloned().unwrap_or_default();
    if kids.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            if ui.add(egui::SelectableLabel::new(selected, text)).clicked() {
                *clicked = Some(id);
            }
        });
        return;
    }
    let state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui.make_persistent_id(("scene_tree", id.0)), false);
    state
        .show_header(ui, |ui| {
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
    fn modifier_keys_pick_the_selection_op() {
        assert_eq!(click_op(egui::Modifiers::NONE), SelectOp::Replace);
        assert_eq!(click_op(egui::Modifiers::CTRL), SelectOp::Add);
        assert_eq!(click_op(egui::Modifiers::ALT), SelectOp::Subtract);
    }
}
