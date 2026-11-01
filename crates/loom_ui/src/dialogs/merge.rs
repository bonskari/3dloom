//! File > Merge: the objects of the chosen file, highlighted like Select
//! From Scene (click, Ctrl+click, Shift+click, All / None / Invert); nothing
//! starts highlighted. A highlighted name the scene already has asks the
//! Duplicate Name question (Merge, Skip, Delete Old, Rename, Auto-Rename,
//! Apply to all duplicates) before the merge runs as one undo step. Sort
//! (Alphabetical, By Type, By Color), List Types filters and Display
//! Subtree (children indented under their parents) arrange the list;
//! Select Subtree highlights a parent's children with it.

use super::select_from_scene::{list_click, name_matches};
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, MergeDuplicate};
use std::collections::BTreeSet;

/// List Types group of a file's object.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Geometry,
    Shapes,
    Lights,
    Cameras,
    Helpers,
}

const KINDS: [(Kind, &str); 5] = [(Kind::Geometry, "Geometry"), (Kind::Shapes, "Shapes"), (Kind::Lights, "Lights"), (Kind::Cameras, "Cameras"), (Kind::Helpers, "Helpers")];

/// A file object's type, wire colour and parent (index into the names).
#[derive(Clone)]
struct Info {
    kind: Kind,
    color: [u8; 3],
    parent: Option<usize>,
}

#[derive(Clone, Copy, PartialEq, Default)]
enum Sort {
    #[default]
    Alphabetical,
    ByType,
    ByColor,
}

#[derive(Default, Clone)]
struct St {
    chosen: BTreeSet<usize>,
    anchor: Option<usize>,
    find: String,
    /// Per name, read from the file once (empty when it cannot be read).
    info: Option<Vec<Info>>,
    sort: Sort,
    /// List Types turned off.
    hidden: BTreeSet<Kind>,
    subtree: bool,
    select_subtree: bool,
    /// After OK: the chosen names, the duplicates still to answer and the answers so far.
    asking: Option<Asking>,
}

#[derive(Clone)]
struct Asking {
    names: Vec<String>,
    pending: Vec<String>,
    answers: Vec<MergeDuplicate>,
    rename: String,
    apply_to_all: bool,
}

fn state_id() -> egui::Id {
    egui::Id::new("merge_dialog").with("state")
}

/// Forgets the highlight (called when any dialog closes).
pub fn clear_state(d: &mut egui::util::IdTypeMap) {
    d.remove::<St>(state_id());
}

/// The file's objects' types, colours and parents, by name order.
fn file_info(path: &std::path::Path, names: &[String]) -> Vec<Info> {
    let Ok((doc, _)) = loom_commands::file::read_any(path) else { return Vec::new() };
    let nodes: Vec<&loom_scene::Node> = doc.scene.nodes().collect();
    if nodes.len() != names.len() {
        return Vec::new();
    }
    nodes
        .iter()
        .map(|n| {
            use loom_scene::ObjectKind as O;
            let kind = match &n.object {
                O::Light(_) => Kind::Lights,
                O::Camera { .. } => Kind::Cameras,
                k if k.is_shape() => Kind::Shapes,
                k if k.is_helper() => Kind::Helpers,
                _ => Kind::Geometry,
            };
            let parent = n.parent.and_then(|p| nodes.iter().position(|m| m.id == p));
            Info { kind, color: doc.scene.display_color(n), parent }
        })
        .collect()
}

fn depth_of(info: &[Info], i: usize) -> usize {
    let mut d = 0;
    let mut at = info.get(i).and_then(|x| x.parent);
    while let Some(p) = at.filter(|_| d < info.len()) {
        d += 1;
        at = info[p].parent;
    }
    d
}

/// Whether `j` is a child (at any depth) of `i`.
fn descends(info: &[Info], j: usize, i: usize) -> bool {
    let mut at = info.get(j).and_then(|x| x.parent);
    for _ in 0..info.len() {
        match at {
            Some(p) if p == i => return true,
            Some(p) => at = info[p].parent,
            None => return false,
        }
    }
    false
}

/// The listed names (indexes): the List Types shown, sorted; with Display
/// Subtree each parent is followed by its children.
fn list_order(names: &[String], info: &[Info], st: &St) -> Vec<usize> {
    let key = |i: usize| {
        let x = info.get(i);
        let kind = x.map_or(Kind::Geometry, |x| x.kind);
        let color = x.map_or([0; 3], |x| x.color);
        match st.sort {
            Sort::Alphabetical => (Kind::Geometry, [0; 3], names[i].to_lowercase()),
            Sort::ByType => (kind, [0; 3], names[i].to_lowercase()),
            Sort::ByColor => (Kind::Geometry, color, names[i].to_lowercase()),
        }
    };
    let shown = |i: usize| !st.hidden.contains(&info.get(i).map_or(Kind::Geometry, |x| x.kind));
    // Without the file's parents (unreadable) the list stays flat.
    let subtree = st.subtree && info.len() == names.len();
    let mut roots: Vec<usize> = (0..names.len()).filter(|&i| shown(i) && (!subtree || info[i].parent.is_none_or(|p| !shown(p)))).collect();
    roots.sort_by_key(|&i| key(i));
    if !subtree {
        return roots;
    }
    fn add(i: usize, names: &[String], info: &[Info], out: &mut Vec<usize>, sorted: &dyn Fn(Vec<usize>) -> Vec<usize>) {
        out.push(i);
        let kids: Vec<usize> = (0..names.len()).filter(|&j| info[j].parent == Some(i)).collect();
        for k in sorted(kids) {
            add(k, names, info, out, sorted);
        }
    }
    let sorted = |mut v: Vec<usize>| {
        v.retain(|&j| shown(j));
        v.sort_by_key(|&j| key(j));
        v
    };
    let mut out = Vec::new();
    for r in roots {
        add(r, names, info, &mut out, &sorted);
    }
    out
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let Some((path, names)) = app.state.merge_pending.clone() else {
        *open = false;
        return;
    };
    let mut st: St = ctx.data(|d| d.get_temp(state_id())).unwrap_or_default();
    let mut done = false;
    let mut run: Option<(Vec<String>, Vec<MergeDuplicate>)> = None;
    let info = st.info.get_or_insert_with(|| file_info(&path, &names)).clone();
    let order = list_order(&names, &info, &st);
    let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if let Some(ask) = st.asking.as_mut() {
        // Duplicate Name: one question per name the scene already has.
        let name = ask.pending[0].clone();
        let mut answer: Option<MergeDuplicate> = None;
        let mut cancel = false;
        egui::Window::new("Duplicate Name").collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label(format!("An object named {name:?} already exists in the scene."));
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut ask.rename);
                let free = !ask.rename.trim().is_empty() && app.state.scene.nodes().all(|n| n.name != ask.rename.trim());
                if ui.add_enabled(free, egui::Button::new("Rename")).clicked() {
                    answer = Some(MergeDuplicate { name: name.clone(), action: "rename".into(), new_name: Some(ask.rename.trim().to_owned()) });
                }
            });
            ui.horizontal(|ui| {
                for (label, action) in [("Merge", "merge"), ("Skip", "skip"), ("Delete Old", "delete_old"), ("Auto-Rename", "auto_rename")] {
                    if ui.button(label).clicked() {
                        answer = Some(MergeDuplicate { name: name.clone(), action: action.into(), new_name: None });
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut ask.apply_to_all, "Apply to all duplicates");
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        if cancel {
            st.asking = None;
        } else if let Some(a) = answer {
            ask.pending.remove(0);
            if ask.apply_to_all && a.action != "rename" {
                for rest in ask.pending.drain(..) {
                    ask.answers.push(MergeDuplicate { name: rest, action: a.action.clone(), new_name: None });
                }
            }
            ask.answers.push(a);
            match ask.pending.first() {
                Some(next) => ask.rename = next.clone(),
                None => run = Some((ask.names.clone(), ask.answers.clone())),
            }
        }
    } else {
        let title = if app.state.merge_as_xref { "XRef Merge" } else { "Merge" };
        egui::Window::new(format!("{title} - {file_name}")).open(open).resizable(true).default_width(320.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Find:");
                if ui.text_edit_singleline(&mut st.find).changed() && !st.find.is_empty() {
                    st.chosen = order.iter().copied().filter(|i| name_matches(&st.find, &names[*i], false)).collect();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Sort:");
                ui.radio_value(&mut st.sort, Sort::Alphabetical, "Alphabetical");
                ui.radio_value(&mut st.sort, Sort::ByType, "By Type");
                ui.radio_value(&mut st.sort, Sort::ByColor, "By Color");
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("List Types:");
                for (k, label) in KINDS {
                    let mut on = !st.hidden.contains(&k);
                    if ui.checkbox(&mut on, label).changed() {
                        if on {
                            st.hidden.remove(&k);
                        } else {
                            st.hidden.insert(k);
                        }
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.small_button("Types All").clicked() {
                    st.hidden.clear();
                }
                if ui.small_button("Types None").clicked() {
                    st.hidden = KINDS.iter().map(|k| k.0).collect();
                }
                if ui.small_button("Types Invert").clicked() {
                    st.hidden = KINDS.iter().map(|k| k.0).filter(|k| !st.hidden.contains(k)).collect();
                }
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut st.subtree, "Display Subtree");
                ui.checkbox(&mut st.select_subtree, "Select Subtree");
            });
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for &i in &order {
                    let depth = if st.subtree { depth_of(&info, i) } else { 0 };
                    let label = format!("{}{}", "    ".repeat(depth), names[i]);
                    if ui.selectable_label(st.chosen.contains(&i), label).clicked() {
                        let mods = ui.input(|inp| inp.modifiers);
                        list_click(&mut st.chosen, &mut st.anchor, &order, i, mods);
                        if st.select_subtree && st.chosen.contains(&i) {
                            st.chosen.extend(order.iter().copied().filter(|&j| descends(&info, j, i)));
                        }
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button("All").clicked() {
                    // The listed objects (the types shown).
                    st.chosen = order.iter().copied().collect();
                }
                if ui.button("None").clicked() {
                    st.chosen.clear();
                }
                if ui.button("Invert").clicked() {
                    st.chosen = order.iter().copied().filter(|i| !st.chosen.contains(i)).collect();
                }
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(!st.chosen.is_empty(), egui::Button::new("OK")).clicked() {
                    let chosen: Vec<String> = st.chosen.iter().map(|i| names[*i].clone()).collect();
                    let mut pending: Vec<String> = chosen.iter().filter(|n| app.state.scene.nodes().any(|x| &x.name == *n)).cloned().collect();
                    pending.dedup();
                    // XRef objects keep their file names (renamed here when taken), no question.
                    if pending.is_empty() || app.state.merge_as_xref {
                        run = Some((chosen, Vec::new()));
                    } else {
                        st.asking = Some(Asking { names: chosen, rename: pending[0].clone(), pending, answers: Vec::new(), apply_to_all: false });
                    }
                }
                if ui.button("Cancel").clicked() {
                    done = true;
                }
            });
        });
    }
    if let Some((chosen, answers)) = run {
        let p = path.to_string_lossy().into_owned();
        let cmd = if app.state.merge_as_xref { Command::XrefObjects { path: Some(p), names: Some(chosen), automatic: None } } else { Command::MergeFile { path: Some(p), names: Some(chosen), duplicates: Some(answers) } };
        match app.dispatch(cmd) {
            Ok(_) => done = true,
            Err(e) => {
                app.state.status_prompt = e;
                st.asking = None;
            }
        }
    }
    if done {
        *open = false;
        app.state.merge_pending = None;
        app.state.merge_as_xref = false;
        ctx.data_mut(|d| d.remove::<St>(state_id()));
    } else {
        ctx.data_mut(|d| d.insert_temp(state_id(), st));
    }
}
