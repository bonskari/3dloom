//! Biped rollouts: Create Biped (Create panel) and the Motion panel's Biped
//! rollout (Figure Mode) with its Structure rollout. Every edit dispatches a
//! command; the Create Biped values for the next biped are UI state.

use super::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, SelectOp};
use loom_scene::biped::{BipedStructure, BodyType};
use loom_scene::NodeId;

fn run(app: &mut LoomApp, cmd: Command) {
    if let Err(e) = app.dispatch(cmd) {
        app.state.status_prompt = e;
    }
}

fn mem<T: Clone + Send + Sync + 'static>(ui: &egui::Ui, key: &str, init: T) -> T {
    ui.ctx().data_mut(|d| d.get_temp_mut_or_insert_with(egui::Id::new(key), || init).clone())
}

fn store<T: Clone + Send + Sync + 'static>(ui: &egui::Ui, key: &str, v: T) {
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(key), v));
}

/// A Structure check box; a click sets `out`.
fn check(ui: &mut egui::Ui, s: &BipedStructure, name: &'static str, label: &str, enabled: bool, out: &mut Option<(&'static str, f64)>) {
    let mut v = s.flag(name).unwrap_or(false);
    if ui.add_enabled(enabled, egui::Checkbox::new(&mut v, label)).changed() {
        *out = Some((name, v as u32 as f64));
    }
}

/// The Structure controls; returns the parameter the user changed.
fn structure_grid(ui: &mut egui::Ui, salt: &str, s: &BipedStructure, height: bool) -> Option<(&'static str, f64)> {
    let mut out = None;
    ui.horizontal(|ui| {
        ui.label("Body Type:");
        egui::ComboBox::from_id_salt((salt, "body_type")).selected_text(s.body_type.label()).show_ui(ui, |ui| {
            for b in BodyType::ALL {
                if ui.selectable_label(s.body_type == b, b.label()).clicked() && s.body_type != b {
                    out = Some(("Body Type", b.index() as f64));
                }
            }
        });
    });
    let mut changed = None;
    check(ui, s, "Arms", "Arms", true, &mut changed);
    egui::Grid::new(salt).num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        for (name, lo, hi) in BipedStructure::LINKS {
            ui.label(format!("{name}:"));
            let mut v = s.count(name).unwrap_or(lo);
            if ui.add(crate::spinner::spin(ui, &mut v, lo..=hi).speed(0.05)).changed() {
                changed = Some((name, v as f64));
            }
            ui.end_row();
        }
    });
    ui.horizontal(|ui| {
        ui.label("Props:");
        check(ui, s, "Prop1", "1", true, &mut changed);
        check(ui, s, "Prop2", "2", true, &mut changed);
        check(ui, s, "Prop3", "3", true, &mut changed);
    });
    egui::Grid::new((salt, "ankle")).num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label("Ankle Attach:");
        let mut a = s.ankle_attach.get();
        if ui.add(crate::spinner::spin(ui, &mut a, 0.0..=1.0).speed(0.01)).changed() {
            changed = Some(("Ankle Attach", a as f64));
        }
        ui.end_row();
        if height {
            ui.label("Height:");
            let mut h = s.height.get();
            if ui.add(crate::spinner::spin(ui, &mut h, loom_scene::biped::MIN_HEIGHT..=f32::MAX).speed(0.1)).changed() {
                changed = Some(("Height", h as f64));
            }
            ui.end_row();
        }
    });
    let knuckles = s.knuckles.get();
    check(ui, s, "Triangle Pelvis", "Triangle Pelvis", true, &mut changed);
    check(ui, s, "Triangle Neck", "Triangle Neck", true, &mut changed);
    check(ui, s, "ForeFeet", "ForeFeet", !knuckles, &mut changed);
    check(ui, s, "Knuckles", "Knuckles", !s.forefeet.get(), &mut changed);
    ui.indent((salt, "thumb"), |ui| check(ui, s, "Short Thumb", "Short Thumb", knuckles, &mut changed));
    ui.separator();
    ui.label("Twist Links");
    check(ui, s, "Twists", "Twists", true, &mut changed);
    egui::Grid::new((salt, "twists")).num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        for (name, lo, hi) in BipedStructure::TWISTS {
            let enabled = s.twists.get() && (name != "Horse Link" || s.leg_links.get() >= 4);
            ui.add_enabled(enabled, egui::Label::new(format!("{name}:")));
            let mut v = s.count(name).unwrap_or(lo);
            if ui.add_enabled(enabled, crate::spinner::spin(ui, &mut v, lo..=hi).speed(0.05)).changed() {
                changed = Some((name, v as f64));
            }
            ui.end_row();
        }
    });
    out.or(changed)
}

/// The nodes of the tail `name` (its links and nub).
fn xtra_nodes(app: &LoomApp, root: NodeId, name: &str) -> Vec<NodeId> {
    loom_commands::biped::parts(&app.state, root)
        .into_iter()
        .filter(|(suffix, _)| suffix.strip_prefix(name).is_some_and(|r| r.is_empty() || r == "Nub" || r.parse::<u32>().is_ok()))
        .map(|(_, id)| id)
        .collect()
}

/// Structure > Xtras group; `motion` adds the parent controls (Motion panel only).
fn xtras_group(ui: &mut egui::Ui, app: &mut LoomApp, root: NodeId, s: &BipedStructure, motion: bool) {
    ui.separator();
    ui.label("Xtras");
    let key = format!("biped_xtra_{}", root.0);
    let mut current: Option<String> = mem(ui, &key, None::<String>).filter(|n| s.xtras.iter().any(|x| &x.name == n));
    let mut synch: bool = mem(ui, "biped_xtra_synch", false);
    let mut symmetrical: bool = mem(ui, "biped_xtra_symmetrical", false);
    // Synch Selection: a selected tail in a viewport becomes the current one.
    if synch {
        let sel: Vec<NodeId> = app.state.selection.iter().copied().collect();
        if let Some(x) = s.xtras.iter().find(|x| xtra_nodes(app, root, &x.name).iter().any(|id| sel.contains(id))) {
            current = Some(x.name.clone());
        }
    }
    ui.horizontal_wrapped(|ui| {
        if ui.button("Create Xtra").clicked() {
            if let Ok(v) = app.dispatch(Command::BipedCreateXtra { id: root.0, name: None, links: None }) {
                current = v["name"].as_str().map(str::to_owned);
            }
        }
        if ui.add_enabled(current.is_some(), egui::Button::new("Delete Xtra")).clicked() {
            if let Some(n) = current.take() {
                run(app, Command::BipedDeleteXtra { id: root.0, name: n });
            }
        }
        let opposite_ok = current.as_ref().is_some_and(|n| s.xtras.iter().any(|x| &x.name == n && x.opposite.is_none()));
        if ui.add_enabled(opposite_ok, egui::Button::new("Create Opposite Xtra")).clicked() {
            if let Some(n) = current.clone() {
                if let Ok(v) = app.dispatch(Command::BipedCreateOppositeXtra { id: root.0, name: n }) {
                    current = v["name"].as_str().map(str::to_owned);
                }
            }
        }
    });
    ui.checkbox(&mut synch, "Synch Selection");
    ui.checkbox(&mut symmetrical, "Select Symmetrical");
    let mut picked = None;
    if let Some(name) = current.clone() {
        // The typed name lives in memory until the field loses focus.
        let edit_key = format!("{key}_edit");
        let mut text: String = mem(ui, &edit_key, None::<String>).unwrap_or_else(|| name.clone());
        let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(140.0).id_salt((&key, "name")));
        store(ui, &edit_key, r.has_focus().then(|| text.clone()));
        if r.lost_focus() && text.trim() != name {
            if let Ok(v) = app.dispatch(Command::BipedSetXtra { id: root.0, name: name.clone(), new_name: Some(text), links: None, parent: None, reorient: None }) {
                current = v["name"].as_str().map(str::to_owned);
            } else {
                app.state.status_prompt = "that name is taken".into();
            }
        }
    }
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_height(60.0);
        for x in &s.xtras {
            if ui.selectable_label(current.as_deref() == Some(x.name.as_str()), &x.name).clicked() {
                current = Some(x.name.clone());
                picked = Some(x.clone());
            }
        }
    });
    if let Some(x) = picked.filter(|_| synch) {
        let mut ids = xtra_nodes(app, root, &x.name);
        if let Some(o) = x.opposite.as_ref().filter(|_| symmetrical) {
            ids.extend(xtra_nodes(app, root, o));
        }
        run(app, Command::SelectNodes { ids: ids.into_iter().map(|i| i.0).collect(), op: SelectOp::Replace });
    }
    if let Some(x) = current.as_ref().and_then(|n| s.xtras.iter().find(|x| &x.name == n)).cloned() {
        ui.horizontal(|ui| {
            ui.label("Links:");
            let mut l = x.links;
            if ui.add(crate::spinner::spin(ui, &mut l, 1..=25).speed(0.05)).changed() {
                run(app, Command::BipedSetXtra { id: root.0, name: x.name.clone(), new_name: None, links: Some(l), parent: None, reorient: None });
            }
        });
        if motion {
            let root_name = app.state.scene.node(root).map(|n| n.name.clone()).unwrap_or_default();
            let parent = x.parent.as_ref().map_or(root_name.clone(), |p| format!("{root_name} {p}"));
            ui.label(format!("Parent: {parent}"));
            let mut reorient: bool = mem(ui, "biped_xtra_reorient", true);
            let picking = app.state.pick_mode == Some(loom_commands::PickMode::BipedXtraParent);
            if ui.add(egui::Button::new("Pick Parent").selected(picking)).clicked() {
                if picking {
                    run(app, Command::ExitPickMode);
                } else {
                    run(app, Command::BipedPickXtraParent { name: x.name.clone(), reorient: Some(reorient) });
                }
            }
            ui.checkbox(&mut reorient, "Reorient to Parent");
            store(ui, "biped_xtra_reorient", reorient);
        }
    }
    store(ui, &key, current);
    store(ui, "biped_xtra_synch", synch);
    store(ui, "biped_xtra_symmetrical", symmetrical);
}

fn biped_of(app: &LoomApp, id: NodeId) -> Option<(NodeId, BipedStructure)> {
    let root = loom_commands::biped::root_of(&app.state, id)?;
    let s = loom_commands::biped::structure(&app.state, root)?.clone();
    Some((root, s))
}

/// Create > Systems > Biped: Root Name and the structure of the next biped;
/// the biped just made stays editable here while it is selected.
pub fn create_rollout(ui: &mut egui::Ui, app: &mut LoomApp) {
    rollout(ui, "Create Biped", true, |ui| {
        ui.label("Drag in a viewport from the feet up to set the height.");
        let selected = app.state.selection.iter().next().copied();
        let current = app.last_biped.filter(|r| selected == Some(*r)).and_then(|r| biped_of(app, r));
        if let Some((root, s)) = &current {
            let mut name = app.state.scene.node(*root).map(|n| n.name.clone()).unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label("Root Name:");
                let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(120.0));
                if r.lost_focus() {
                    run(app, Command::BipedRootName { id: root.0, name: name.clone() });
                }
            });
            if let Some((name, value)) = structure_grid(ui, "create_biped_current", s, true) {
                let _ = app.biped_defaults.set(name, value);
                run(app, Command::SetParameter { id: root.0, name: name.into(), value });
            }
            xtras_group(ui, app, *root, s, false);
        } else {
            let d = app.biped_defaults.clone();
            if let Some((name, value)) = structure_grid(ui, "create_biped_defaults", &d, false) {
                if let Err(e) = app.biped_defaults.set(name, value) {
                    app.state.status_prompt = e;
                }
            }
        }
    });
}

/// Motion panel: the Biped rollout (Figure Mode) and, in Figure Mode, Structure.
pub fn motion_rollouts(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId) {
    let Some((root, s)) = biped_of(app, id) else { return };
    rollout(ui, "Biped", true, |ui| {
        let label = if s.figure_mode { "Figure Mode (on)" } else { "Figure Mode" };
        if ui.add(egui::Button::new(label).selected(s.figure_mode)).clicked() {
            run(app, Command::BipedFigureMode { on: Some(!s.figure_mode) });
        }
        // Figure files (.fig) load and save in Figure Mode.
        ui.horizontal(|ui| {
            if ui.add_enabled(s.figure_mode, egui::Button::new("Load File")).on_hover_text("Load a figure file (.fig)").clicked() {
                run(app, Command::BipedLoadFile { path: None });
            }
            if ui.add_enabled(s.figure_mode, egui::Button::new("Save File")).on_hover_text("Save the figure (.fig)").clicked() {
                run(app, Command::BipedSaveFile { path: None });
            }
        });
    });
    rollout(ui, "Structure", true, |ui| {
        ui.add_enabled_ui(s.figure_mode, |ui| {
            if let Some((name, value)) = structure_grid(ui, "motion_biped_structure", &s, true) {
                run(app, Command::SetParameter { id: root.0, name: name.into(), value });
            }
            xtras_group(ui, app, root, &s, true);
        });
        if !s.figure_mode {
            ui.label("Turn on Figure Mode to edit the structure.");
        }
    });
}
