//! Select From Scene dialog (H), also used for Hide/Unhide/Freeze/Unfreeze
//! by Name, Attach List, Select Parent (Select and Link) and Attach To Group
//! with the matching title, list and action.
//!
//! As in the reference: the list starts with the current selection
//! highlighted; a click highlights one row, Ctrl+click toggles a row,
//! Shift+click a range; a double-click picks that row and closes; Display >
//! Display Children shows hierarchies as collapsible branches (Expand All /
//! Collapse All); typing in Find highlights the matching names.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, DialogPurpose};
use loom_scene::{Node, NodeId, ObjectKind};
use std::collections::{BTreeMap, BTreeSet};

/// The Attach List's "Scene Root" row (the root of the hierarchy, not an object).
pub const SCENE_ROOT: u64 = u64::MAX;

/// Highlighting by a click on row `k` of `order` (the rows as displayed):
/// plain click highlights only `k`, Ctrl toggles it, Shift highlights the
/// range from the last clicked row.
pub fn list_click<K: Ord + Copy>(chosen: &mut BTreeSet<K>, anchor: &mut Option<K>, order: &[K], k: K, mods: egui::Modifiers) {
    if mods.shift {
        let a = anchor.and_then(|a| order.iter().position(|x| *x == a));
        let b = order.iter().position(|x| *x == k);
        if let (Some(a), Some(b)) = (a, b) {
            if !mods.ctrl {
                chosen.clear();
            }
            chosen.extend(order[a.min(b)..=a.max(b)].iter().copied());
            return;
        }
    }
    if mods.ctrl {
        if !chosen.remove(&k) {
            chosen.insert(k);
        }
    } else {
        chosen.clear();
        chosen.insert(k);
    }
    *anchor = Some(k);
}

/// Find field match: the start of the name, `*` any run, `?` one character, case-insensitive unless `case`.
pub fn name_matches(pattern: &str, name: &str, case: bool) -> bool {
    fn glob(p: &[char], n: &[char]) -> bool {
        match (p.first(), n.first()) {
            (None, _) => true, // a prefix match: the rest of the name is free
            (Some('*'), _) => glob(&p[1..], n) || (!n.is_empty() && glob(p, &n[1..])),
            (Some('?'), Some(_)) => glob(&p[1..], &n[1..]),
            (Some(a), Some(b)) => a == b && glob(&p[1..], &n[1..]),
            (Some(_), None) => false,
        }
    }
    let (p, n): (Vec<char>, Vec<char>) = if case { (pattern.chars().collect(), name.chars().collect()) } else { (pattern.to_lowercase().chars().collect(), name.to_lowercase().chars().collect()) };
    glob(&p, &n)
}

/// Toggles that outlast one opening of the dialog.
#[derive(Clone)]
struct Options {
    display_children: bool,
    case_sensitive: bool,
    geometry: bool,
    shapes: bool,
    cameras: bool,
    helpers: bool,
    show_hidden: bool,
    show_frozen: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { display_children: true, case_sensitive: false, geometry: true, shapes: true, cameras: true, helpers: true, show_hidden: false, show_frozen: false }
    }
}

/// One opening's state (cleared when the dialog closes).
#[derive(Default, Clone)]
struct Session {
    opened: bool,
    search: String,
    chosen: BTreeSet<u64>,
    anchor: Option<u64>,
    expanded: BTreeSet<u64>,
}

fn session_id() -> egui::Id {
    egui::Id::new("select_from_scene_state")
}

fn options_id() -> egui::Id {
    egui::Id::new("select_from_scene_options")
}

/// Forgets the highlighted rows (called when any dialog closes).
pub fn clear_state(d: &mut egui::util::IdTypeMap) {
    d.remove::<Session>(session_id());
}

pub fn title(purpose: DialogPurpose) -> &'static str {
    match purpose {
        DialogPurpose::Select => "Select From Scene",
        DialogPurpose::Hide => "Hide Objects",
        DialogPurpose::Unhide => "Unhide Objects",
        DialogPurpose::Freeze => "Freeze Objects",
        DialogPurpose::Unfreeze => "Unfreeze Objects",
        DialogPurpose::Attach => "Attach List",
        DialogPurpose::SkinAddBones => "Select Bones",
        DialogPurpose::LinkParent => "Select Parent",
        DialogPurpose::GroupAttach => "Attach To Group",
    }
}

fn verb(purpose: DialogPurpose) -> &'static str {
    match purpose {
        DialogPurpose::Select | DialogPurpose::SkinAddBones => "Select",
        DialogPurpose::Hide => "Hide",
        DialogPurpose::Unhide => "Unhide",
        DialogPurpose::Freeze => "Freeze",
        DialogPurpose::Unfreeze => "Unfreeze",
        DialogPurpose::Attach => "Attach",
        DialogPurpose::LinkParent => "Link",
        DialogPurpose::GroupAttach => "Attach",
    }
}

/// Whether the dialog picks one object only (Select Parent, Attach To Group).
fn single(purpose: DialogPurpose) -> bool {
    matches!(purpose, DialogPurpose::LinkParent | DialogPurpose::GroupAttach)
}

fn type_ok(o: &Options, n: &Node) -> bool {
    match n.object {
        ObjectKind::Camera { .. } => o.cameras,
        ref k if k.is_shape() => o.shapes,
        ref k if k.is_helper() => o.helpers,
        _ => o.geometry,
    }
}

/// The objects the dialog offers for `purpose`.
fn candidate(app: &LoomApp, purpose: DialogPurpose, o: &Options, n: &Node) -> bool {
    let s = &app.state;
    let hidden = !s.is_visible(n);
    let frozen = n.frozen || s.scene.layer(n.layer).is_some_and(|l| l.frozen);
    let shown = match purpose {
        DialogPurpose::Unhide => n.hidden,
        DialogPurpose::Unfreeze => n.frozen,
        DialogPurpose::Hide => !n.hidden,
        DialogPurpose::Freeze => !n.frozen && (o.show_hidden || !hidden),
        DialogPurpose::Attach => !hidden && !frozen && !s.selection.contains(&n.id) && !n.object.is_helper() && !n.object.is_shape(),
        DialogPurpose::LinkParent => !hidden && !frozen && !s.selection.iter().any(|c| *c == n.id || s.scene.is_descendant(n.id, *c)),
        DialogPurpose::GroupAttach => !hidden && matches!(n.object, ObjectKind::Group { .. }) && !s.selection.contains(&n.id),
        DialogPurpose::Select | DialogPurpose::SkinAddBones => (o.show_hidden || !hidden) && (o.show_frozen || !frozen),
    };
    shown && (matches!(purpose, DialogPurpose::Unhide | DialogPurpose::Unfreeze | DialogPurpose::GroupAttach) || type_ok(o, n))
}

/// A displayed row.
struct Row {
    id: u64,
    depth: usize,
    has_children: bool,
    name: String,
    kind: &'static str,
    state: &'static str,
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let purpose = app.state.dialog_purpose;
    let mut o: Options = ctx.data(|d| d.get_temp(options_id())).unwrap_or_default();
    let mut st: Session = ctx.data(|d| d.get_temp(session_id())).unwrap_or_default();
    if !st.opened {
        st.opened = true;
        // The current selection starts highlighted.
        if purpose == DialogPurpose::Select {
            st.chosen = app.state.selection.iter().map(|n| n.0).collect();
        }
    }
    // The rows the dialog offers, each under its nearest offered ancestor.
    let listed: Vec<&Node> = app.state.scene.nodes().filter(|n| candidate(app, purpose, &o, n)).collect();
    let listed_ids: BTreeSet<NodeId> = listed.iter().map(|n| n.id).collect();
    let shown_parent = |n: &Node| -> Option<NodeId> {
        let mut cur = n.parent;
        while let Some(p) = cur {
            if listed_ids.contains(&p) {
                return Some(p);
            }
            cur = app.state.scene.node(p).and_then(|pn| pn.parent);
        }
        None
    };
    let mut children: BTreeMap<Option<NodeId>, Vec<&Node>> = BTreeMap::new();
    for n in &listed {
        let parent = if o.display_children { shown_parent(n) } else { None };
        children.entry(parent).or_default().push(n);
    }
    let attach_root = purpose == DialogPurpose::Attach;
    let mut rows: Vec<Row> = Vec::new();
    if attach_root {
        rows.push(Row { id: SCENE_ROOT, depth: 0, has_children: !listed.is_empty(), name: "Scene Root".into(), kind: "", state: "" });
    }
    fn walk(app: &LoomApp, children: &BTreeMap<Option<NodeId>, Vec<&Node>>, expanded: &BTreeSet<u64>, tree: bool, parent: Option<NodeId>, depth: usize, rows: &mut Vec<Row>) {
        for n in children.get(&parent).map(|v| v.as_slice()).unwrap_or(&[]) {
            let has_children = children.get(&Some(n.id)).is_some_and(|c| !c.is_empty());
            let group = matches!(n.object, ObjectKind::Group { .. });
            let hidden = !app.state.is_visible(n);
            let frozen = n.frozen || app.state.scene.layer(n.layer).is_some_and(|l| l.frozen);
            rows.push(Row {
                id: n.id.0,
                depth,
                has_children: tree && has_children,
                // Group heads read [Name], as in every list of the reference.
                name: if group { format!("[{}]", n.name) } else { n.name.clone() },
                kind: n.object.base_name(),
                state: match (hidden, frozen) {
                    (true, true) => "hidden, frozen",
                    (true, false) => "hidden",
                    (false, true) => "frozen",
                    _ => "",
                },
            });
            if tree && expanded.contains(&n.id.0) {
                walk(app, children, expanded, tree, Some(n.id), depth + 1, rows);
            }
        }
    }
    walk(app, &children, &st.expanded, o.display_children, None, usize::from(attach_root), &mut rows);
    let order: Vec<u64> = rows.iter().map(|r| r.id).collect();
    let all_rows: BTreeSet<u64> = order.iter().copied().collect();
    let tree_ids: Vec<u64> = listed.iter().filter(|n| children.get(&Some(n.id)).is_some_and(|c| !c.is_empty())).map(|n| n.id.0).collect();

    let mut done = false;
    let mut run: Option<Vec<u64>> = None;
    egui::Window::new(title(purpose)).open(open).default_width(400.0).show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("Select", |ui| {
                if ui.button("Select All").clicked() {
                    st.chosen = all_rows.clone();
                    ui.close_menu();
                }
                if ui.button("Select None").clicked() {
                    st.chosen.clear();
                    ui.close_menu();
                }
                if ui.button("Select Invert").clicked() {
                    st.chosen = all_rows.difference(&st.chosen).copied().collect();
                    ui.close_menu();
                }
                ui.separator();
                ui.checkbox(&mut o.case_sensitive, "Find Case Sensitive");
            });
            ui.menu_button("Display", |ui| {
                ui.checkbox(&mut o.display_children, "Display Children");
                ui.separator();
                if ui.add_enabled(o.display_children, egui::Button::new("Expand All")).clicked() {
                    st.expanded = tree_ids.iter().copied().collect();
                    ui.close_menu();
                }
                if ui.add_enabled(o.display_children, egui::Button::new("Collapse All")).clicked() {
                    st.expanded.clear();
                    ui.close_menu();
                }
                ui.separator();
                ui.checkbox(&mut o.geometry, "Geometry");
                ui.checkbox(&mut o.shapes, "Shapes");
                ui.checkbox(&mut o.cameras, "Cameras");
                ui.checkbox(&mut o.helpers, "Helpers");
                if matches!(purpose, DialogPurpose::Select | DialogPurpose::Freeze | DialogPurpose::SkinAddBones) {
                    ui.separator();
                    ui.checkbox(&mut o.show_hidden, "Hidden");
                    if purpose != DialogPurpose::Freeze {
                        ui.checkbox(&mut o.show_frozen, "Frozen");
                    }
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label("Find:");
            if ui.text_edit_singleline(&mut st.search).changed() && !st.search.is_empty() {
                // As you type, the matching names are highlighted.
                st.chosen = rows.iter().filter(|r| r.id != SCENE_ROOT && name_matches(&st.search, r.name.trim_start_matches('['), o.case_sensitive)).map(|r| r.id).collect();
                if single(purpose) {
                    let first = st.chosen.iter().next().copied();
                    st.chosen = first.into_iter().collect();
                }
            }
            if !single(purpose) {
                if ui.button("Select All").clicked() {
                    st.chosen = all_rows.clone();
                }
                if ui.button("Select None").clicked() {
                    st.chosen.clear();
                }
                if ui.button("Select Invert").clicked() {
                    st.chosen = all_rows.difference(&st.chosen).copied().collect();
                }
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            egui::Grid::new("sfs_rows").num_columns(3).striped(true).show(ui, |ui| {
                ui.label(egui::RichText::new("Name").strong());
                ui.label(egui::RichText::new("Type").strong());
                ui.label(egui::RichText::new("State").strong());
                ui.end_row();
                for r in &rows {
                    ui.horizontal(|ui| {
                        ui.add_space(r.depth as f32 * 14.0);
                        if r.has_children && r.id != SCENE_ROOT {
                            let open_branch = st.expanded.contains(&r.id);
                            if ui.add_sized([16.0, 16.0], egui::Button::new(if open_branch { "−" } else { "+" }).small()).on_hover_text(if open_branch { "Collapse" } else { "Expand" }).clicked() {
                                if open_branch {
                                    st.expanded.remove(&r.id);
                                } else {
                                    st.expanded.insert(r.id);
                                }
                            }
                        } else if o.display_children && r.id != SCENE_ROOT {
                            // Leaves line up with their siblings' names.
                            ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                        }
                        let on = st.chosen.contains(&r.id);
                        let resp = ui.selectable_label(on, &r.name);
                        if resp.double_clicked() && r.id != SCENE_ROOT {
                            // Picks this one and closes.
                            run = Some(vec![r.id]);
                        } else if resp.clicked() {
                            let mut mods = ui.input(|i| i.modifiers);
                            if single(purpose) {
                                mods = egui::Modifiers::NONE;
                            }
                            list_click(&mut st.chosen, &mut st.anchor, &order, r.id, mods);
                        }
                    });
                    ui.label(r.kind);
                    ui.label(r.state);
                    ui.end_row();
                }
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            let picked: Vec<u64> = order.iter().copied().filter(|id| *id != SCENE_ROOT && st.chosen.contains(id)).collect();
            if ui.add_enabled(!single(purpose) || picked.len() == 1, egui::Button::new(verb(purpose))).clicked() {
                run = Some(picked);
            }
            if ui.button("Cancel").clicked() {
                done = true;
            }
        });
    });
    if let Some(ids) = run {
        match purpose {
            DialogPurpose::Attach => {
                for id in &ids {
                    let _ = app.dispatch(Command::AttachNode { id: *id });
                }
            }
            DialogPurpose::LinkParent | DialogPurpose::GroupAttach => {
                if let Some(id) = ids.first() {
                    if let Err(e) = app.dispatch(Command::PickHit { id: *id }) {
                        app.state.status_prompt = e;
                    }
                }
            }
            DialogPurpose::SkinAddBones => {
                let _ = app.dispatch(Command::SkinAddBones { ids });
            }
            DialogPurpose::Select => {
                let _ = app.dispatch(Command::SelectNodes { ids, op: loom_commands::SelectOp::Replace });
            }
            DialogPurpose::Hide => {
                let _ = app.dispatch(Command::HideNodes { ids });
            }
            DialogPurpose::Unhide => {
                let _ = app.dispatch(Command::UnhideNodes { ids });
            }
            DialogPurpose::Freeze => {
                let _ = app.dispatch(Command::FreezeNodes { ids });
            }
            DialogPurpose::Unfreeze => {
                let _ = app.dispatch(Command::UnfreezeNodes { ids });
            }
        }
        done = true;
    }
    if done {
        *open = false;
        ctx.data_mut(|d| d.remove::<Session>(session_id()));
    } else {
        ctx.data_mut(|d| d.insert_temp(session_id(), st));
    }
    ctx.data_mut(|d| d.insert_temp(options_id(), o));
}
