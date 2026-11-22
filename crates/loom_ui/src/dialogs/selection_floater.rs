//! Tools > Selection Floater: a modeless Select Objects list that stays open
//! after Select. As in the reference: the find field highlights the names
//! that begin with its text (Find Case Sensitive); click / Ctrl+click /
//! Shift+click highlight rows, a double-click selects one object; All / None
//! / Invert for the list and for List Types; Sort Alphabetical, By Type, By
//! Color or By Size (faces), alphabetical while Display Subtree indents the
//! hierarchy; Display Influences draws the highlighted objects' influences in
//! blue and Influences highlights them; Select Subtree / Select Dependents
//! bring a highlighted object's children / dependents along; the Selection
//! Sets list highlights a named set. Hidden and frozen objects are not listed.

use crate::dialogs::select_from_scene::{list_click, name_matches};
use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;
use loom_scene::{Node, NodeId, ObjectKind};
use std::collections::{BTreeMap, BTreeSet};

/// List Types, in the reference's order (By Type sorts in this order too).
pub const LIST_TYPES: [&str; 8] = ["Geometry", "Shapes", "Lights", "Cameras", "Helpers", "Groups", "XRefs", "Bone Objects"];

/// Sort group.
pub const SORTS: [&str; 4] = ["Alphabetical", "By Type", "By Color", "By Size"];

/// The influences' colour in the list (Display Influences).
pub const INFLUENCE_BLUE: egui::Color32 = egui::Color32::from_rgb(90, 150, 255);

#[derive(Clone)]
struct Floater {
    find: String,
    case_sensitive: bool,
    highlighted: BTreeSet<NodeId>,
    anchor: Option<NodeId>,
    sort: usize,
    types: [bool; LIST_TYPES.len()],
    display_subtree: bool,
    display_influences: bool,
    select_subtree: bool,
    select_dependents: bool,
    /// The scene selection last seen: a change outside the floater shows as the highlight.
    seen: BTreeSet<NodeId>,
}

impl Default for Floater {
    fn default() -> Self {
        Floater { find: String::new(), case_sensitive: false, highlighted: BTreeSet::new(), anchor: None, sort: 0, types: [true; LIST_TYPES.len()], display_subtree: false, display_influences: false, select_subtree: false, select_dependents: false, seen: BTreeSet::new() }
    }
}

fn id() -> egui::Id {
    egui::Id::new("selection_floater_state")
}

/// The List Types entry of a node (an XRef'd object counts as XRefs).
pub fn list_type(app: &LoomApp, n: &Node) -> usize {
    if app.state.scene.xrefs.iter().any(|x| x.nodes.iter().any(|(_, id)| *id == n.id)) {
        return 6;
    }
    match &n.object {
        k if k.is_group_head() => 5,
        ObjectKind::Bone { .. } => 7,
        ObjectKind::Light(_) => 2,
        ObjectKind::Camera { .. } => 3,
        k if k.is_shape() => 1,
        k if k.is_helper() => 4,
        _ => 0,
    }
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp) {
    if !app.state.selection_floater {
        ctx.data_mut(|d| d.remove::<Floater>(id()));
        return;
    }
    let mut f: Floater = ctx.data(|d| d.get_temp(id())).unwrap_or_default();
    if f.seen != app.state.selection {
        f.seen = app.state.selection.clone();
        f.highlighted = f.seen.clone();
    }
    let s = &app.state;
    let listed: Vec<&Node> = s
        .scene
        .nodes()
        .filter(|n| s.is_visible(n) && !(n.frozen || s.scene.layer(n.layer).is_some_and(|l| l.frozen)))
        .filter(|n| f.types[list_type(app, n)])
        .collect();
    let listed_ids: BTreeSet<NodeId> = listed.iter().map(|n| n.id).collect();
    let faces = |n: &Node| if loom_commands::explorer::category(&n.object) == loom_commands::explorer::Category::Geometry { s.scene.eval_mesh(n.id).value.mesh.face_count() } else { 0 };
    let name_key = |n: &Node| if f.case_sensitive { n.name.clone() } else { n.name.to_lowercase() };
    // Rows as (id, depth); Display Subtree indents each object under its nearest listed ancestor, alphabetically.
    let mut rows: Vec<(NodeId, usize)> = Vec::new();
    if f.display_subtree {
        let shown_parent = |n: &Node| {
            let mut cur = n.parent;
            while let Some(p) = cur {
                if listed_ids.contains(&p) {
                    return Some(p);
                }
                cur = s.scene.node(p).and_then(|pn| pn.parent);
            }
            None
        };
        let mut children: BTreeMap<Option<NodeId>, Vec<&Node>> = BTreeMap::new();
        for n in &listed {
            children.entry(shown_parent(n)).or_default().push(n);
        }
        for v in children.values_mut() {
            v.sort_by_key(|n| name_key(n));
        }
        fn walk(children: &BTreeMap<Option<NodeId>, Vec<&Node>>, parent: Option<NodeId>, depth: usize, rows: &mut Vec<(NodeId, usize)>) {
            for n in children.get(&parent).map(|v| v.as_slice()).unwrap_or(&[]) {
                rows.push((n.id, depth));
                walk(children, Some(n.id), depth + 1, rows);
            }
        }
        walk(&children, None, 0, &mut rows);
    } else {
        let mut v = listed.clone();
        match f.sort {
            1 => v.sort_by_key(|n| (list_type(app, n), name_key(n))),
            2 => v.sort_by_key(|n| (n.wire_color, name_key(n))),
            3 => v.sort_by_key(|n| (faces(n), name_key(n))),
            _ => v.sort_by_key(|n| name_key(n)),
        }
        rows = v.iter().map(|n| (n.id, 0)).collect();
    }
    let order: Vec<NodeId> = rows.iter().map(|r| r.0).collect();
    // Select Subtree / Select Dependents: what a highlighted object brings along (subtree first).
    let bring_along = |hl: &mut BTreeSet<NodeId>, f: &Floater| {
        if f.select_subtree {
            let add: Vec<NodeId> = listed.iter().filter(|n| s.scene.ancestors(n.id).iter().any(|a| hl.contains(a))).map(|n| n.id).collect();
            hl.extend(add);
        }
        if f.select_dependents {
            let add: Vec<NodeId> = hl.iter().flat_map(|k| loom_commands::stack::dependents(s, *k)).filter(|d| listed_ids.contains(d)).collect();
            hl.extend(add);
        }
    };
    let influences: BTreeSet<NodeId> = if f.display_influences { f.highlighted.iter().flat_map(|k| loom_commands::stack::influences(s, *k)).collect() } else { BTreeSet::new() };
    let sets: Vec<String> = s.named_sets.keys().cloned().collect();
    let mut select: Option<Vec<u64>> = None;
    let mut open = true;
    egui::Window::new("Selection Floater").open(&mut open).default_width(420.0).resizable(false).show(ctx, |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(220.0);
                let r = ui.add(egui::TextEdit::singleline(&mut f.find).desired_width(210.0).hint_text("Find"));
                crate::icons::named(&r, egui::WidgetType::TextEdit, "Selection Floater Find");
                if r.changed() {
                    f.highlighted = if f.find.is_empty() { BTreeSet::new() } else { order.iter().copied().filter(|k| s.scene.node(*k).is_some_and(|n| name_matches(&f.find, &n.name, f.case_sensitive))).collect() };
                    bring_along(&mut f.highlighted, &f);
                }
                egui::ScrollArea::vertical().id_salt("selection_floater_list").max_height(260.0).show(ui, |ui| {
                    ui.set_min_width(210.0);
                    for (k, depth) in &rows {
                        let Some(n) = s.scene.node(*k) else { continue };
                        let name = if n.object.is_group_head() { format!("[{}]", n.name) } else { n.name.clone() };
                        let text = egui::RichText::new(format!("{}{name}", "    ".repeat(*depth)));
                        let text = if influences.contains(k) && !f.highlighted.contains(k) { text.color(INFLUENCE_BLUE) } else { text };
                        let r = ui.selectable_label(f.highlighted.contains(k), text);
                        if r.double_clicked() {
                            f.highlighted = [*k].into();
                            bring_along(&mut f.highlighted, &f);
                            select = Some(f.highlighted.iter().map(|n| n.0).collect());
                        } else if r.clicked() {
                            let mods = ui.input(|i| i.modifiers);
                            list_click(&mut f.highlighted, &mut f.anchor, &order, *k, mods);
                            bring_along(&mut f.highlighted, &f);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("All").clicked() {
                        f.highlighted = order.iter().copied().collect();
                    }
                    if ui.button("None").clicked() {
                        f.highlighted.clear();
                    }
                    if ui.button("Invert").clicked() {
                        f.highlighted = order.iter().copied().filter(|k| !f.highlighted.contains(k)).collect();
                    }
                    if ui.button("Influences").clicked() {
                        let add: Vec<NodeId> = f.highlighted.iter().flat_map(|k| loom_commands::stack::influences(s, *k)).filter(|k| listed_ids.contains(k)).collect();
                        f.highlighted.extend(add);
                    }
                });
                ui.checkbox(&mut f.case_sensitive, "Find Case Sensitive");
                ui.checkbox(&mut f.display_subtree, "Display Subtree");
                ui.checkbox(&mut f.display_influences, "Display Influences");
                ui.checkbox(&mut f.select_subtree, "Select Subtree");
                ui.checkbox(&mut f.select_dependents, "Select Dependents");
            });
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Sort").strong());
                ui.add_enabled_ui(!f.display_subtree, |ui| {
                    for (i, label) in SORTS.iter().enumerate() {
                        if ui.radio(f.sort == i, *label).clicked() {
                            f.sort = i;
                        }
                    }
                });
                ui.separator();
                ui.label(egui::RichText::new("List Types").strong());
                for (i, label) in LIST_TYPES.iter().enumerate() {
                    ui.checkbox(&mut f.types[i], *label);
                }
                ui.horizontal(|ui| {
                    if ui.button("All").clicked() {
                        f.types = [true; LIST_TYPES.len()];
                    }
                    if ui.button("None").clicked() {
                        f.types = [false; LIST_TYPES.len()];
                    }
                    if ui.button("Invert").clicked() {
                        f.types = f.types.map(|t| !t);
                    }
                });
                ui.separator();
                ui.label(egui::RichText::new("Selection Sets").strong());
                egui::ComboBox::from_id_salt("selection_floater_sets").selected_text("").width(150.0).show_ui(ui, |ui| {
                    for name in &sets {
                        if ui.selectable_label(false, name).clicked() {
                            f.highlighted = s.named_sets.get(name).map(|v| v.iter().copied().filter(|k| listed_ids.contains(k)).collect()).unwrap_or_default();
                        }
                    }
                });
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Select").clicked() {
                select = Some(f.highlighted.iter().filter(|k| listed_ids.contains(k)).map(|n| n.0).collect());
            }
            if ui.button("Close").clicked() {
                open = false;
            }
        });
    });
    if let Some(ids) = select {
        let _ = app.dispatch(Command::SelectNodes { ids, op: loom_commands::SelectOp::Replace });
        // The floater stays open; its highlight is the new selection.
        f.seen = app.state.selection.clone();
        f.highlighted = f.seen.clone();
    }
    ctx.data_mut(|d| d.insert_temp(id(), f));
    if !open {
        let _ = app.dispatch(Command::SelectionFloater);
    }
}
