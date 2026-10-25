//! Clone and Align (Tools > Align > Clone and Align): a modeless dialog.
//! The selection is the source; Pick / Pick List / Clear All set the
//! destinations; the clones preview live until Apply keeps them.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{CloneKind, Command, PickMode};

#[derive(Clone, Copy, PartialEq)]
struct Settings {
    link: bool,
    kind: CloneKind,
    position: [bool; 3],
    position_offset: [f32; 3],
    orientation: [bool; 3],
    orientation_offset: [f32; 3],
    match_scale: [bool; 3],
}

impl Default for Settings {
    fn default() -> Self {
        Self { link: false, kind: CloneKind::Copy, position: [true; 3], position_offset: [0.0; 3], orientation: [true; 3], orientation_offset: [0.0; 3], match_scale: [false; 3] }
    }
}

/// What the preview was built from: a change of any of it rebuilds it.
#[derive(Clone, PartialEq)]
struct Key {
    settings: Settings,
    sources: Vec<loom_scene::NodeId>,
    destinations: Vec<loom_scene::NodeId>,
    /// While Pick is on there is no preview (a click must hit the real objects).
    picking: bool,
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("clone_and_align_dialog");
    let key_id = egui::Id::new("clone_and_align_key");
    let list_id = egui::Id::new("clone_and_align_pick_list");
    let mut st: Settings = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
    let mut show_list: bool = ctx.data(|d| d.get_temp(list_id)).unwrap_or(false);
    let picking = app.state.pick_mode == Some(PickMode::CloneAlignDestination);
    let dests = app.state.clone_align_destinations.clone();
    let sources: Vec<loom_scene::NodeId> = app.state.selection.iter().copied().filter(|s| !dests.contains(s)).collect();
    let speed = app.spinner_speed(0.1);
    let mut apply = false;
    let mut cancel = false;
    let mut set_dests: Option<Vec<u64>> = None;
    egui::Window::new("Clone and Align").open(open).resizable(false).show(ctx, |ui| {
        ui.label(egui::RichText::new("Source and Destination Parameters").strong());
        ui.label(format!("{} Destination Objects", dests.len()));
        ui.horizontal(|ui| {
            if ui.selectable_label(picking, "Pick").clicked() {
                let _ = app.dispatch(Command::CloneAndAlignPick);
            }
            if ui.selectable_label(show_list, "Pick List").clicked() {
                show_list = !show_list;
            }
            if ui.add_enabled(!dests.is_empty(), egui::Button::new("Clear All")).clicked() {
                set_dests = Some(Vec::new());
            }
        });
        if show_list {
            // Pick Destination Objects: every other object by name.
            egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                let names: Vec<(loom_scene::NodeId, String)> = app.state.scene.nodes().filter(|n| !sources.contains(&n.id) && app.state.is_selectable(n)).map(|n| (n.id, n.name.clone())).collect();
                for (nid, name) in names {
                    let mut on = dests.contains(&nid);
                    if ui.checkbox(&mut on, name).changed() {
                        let mut d: Vec<u64> = dests.iter().map(|n| n.0).filter(|n| *n != nid.0).collect();
                        if on {
                            d.push(nid.0);
                        }
                        set_dests = Some(d);
                    }
                }
            });
        }
        ui.label(format!("{} Source Objects", sources.len()));
        ui.checkbox(&mut st.link, "Link to Destination");
        ui.separator();
        ui.label(egui::RichText::new("Clone Parameters").strong());
        ui.horizontal(|ui| {
            ui.radio_value(&mut st.kind, CloneKind::Copy, "Copy");
            ui.radio_value(&mut st.kind, CloneKind::Instance, "Instance");
            ui.radio_value(&mut st.kind, CloneKind::Reference, "Reference");
        });
        ui.separator();
        ui.label(egui::RichText::new("Align Parameters").strong());
        let axes = ["X", "Y", "Z"];
        ui.label("Align Position (World):");
        egui::Grid::new("clone_align_position").num_columns(3).spacing([6.0, 4.0]).show(ui, |ui| {
            for i in 0..3 {
                ui.checkbox(&mut st.position[i], format!("{} Position", axes[i]));
                ui.label("Offset:");
                ui.add(app.length_drag(ui, &mut st.position_offset[i], speed));
                ui.end_row();
            }
        });
        ui.label("Align Orientation (World):");
        egui::Grid::new("clone_align_orientation").num_columns(3).spacing([6.0, 4.0]).show(ui, |ui| {
            for i in 0..3 {
                ui.checkbox(&mut st.orientation[i], format!("{} Orientation", axes[i]));
                ui.label("Offset:");
                ui.add(crate::spinner::spin(ui, &mut st.orientation_offset[i], ..).speed(speed).fixed_decimals(app.state.prefs.spinner_precision).suffix("°"));
                ui.end_row();
            }
        });
        ui.label("Match Scale:");
        ui.horizontal(|ui| {
            for i in 0..3 {
                ui.checkbox(&mut st.match_scale[i], format!("{} Axis", axes[i]));
            }
        });
        if ui.button("Reset All Parameters").clicked() {
            let s = Settings::default();
            st.position = s.position;
            st.position_offset = s.position_offset;
            st.orientation = s.orientation;
            st.orientation_offset = s.orientation_offset;
            st.match_scale = s.match_scale;
        }
        ui.horizontal(|ui| {
            apply = ui.add_enabled(!dests.is_empty() && !sources.is_empty(), egui::Button::new("Apply")).clicked();
            cancel = ui.button("Cancel").clicked();
        });
    });
    if let Some(d) = set_dests {
        let _ = app.dispatch(Command::CloneAndAlignSetDestinations { ids: d });
    }
    let dests = app.state.clone_align_destinations.clone();
    let picking = app.state.pick_mode == Some(PickMode::CloneAlignDestination);
    let key = Key { settings: st, sources: sources.clone(), destinations: dests.clone(), picking };
    let last: Option<Key> = ctx.data(|d| d.get_temp(key_id));
    let run = |app: &mut LoomApp| {
        let _ = app.dispatch(Command::CloneAndAlign {
            destinations: dests.iter().map(|d| d.0).collect(),
            kind: st.kind,
            link: st.link,
            position: st.position,
            position_offset: st.position_offset,
            orientation: st.orientation,
            orientation_offset: st.orientation_offset,
            match_scale: st.match_scale,
        });
    };
    if apply {
        // The previewed clones stay; nothing previews until something changes.
        if app.preview_dialog() != Some(loom_commands::Dialog::CloneAndAlign) {
            run(app);
        }
        app.preview_keep();
        ctx.data_mut(|d| d.insert_temp(key_id, key.clone()));
    } else if last.as_ref() != Some(&key) && *open && !cancel {
        app.preview_undo();
        if !dests.is_empty() && !sources.is_empty() && !picking {
            // The clones are not selected: the sources stay the selection.
            app.preview_apply(loom_commands::Dialog::CloneAndAlign, run);
        }
        ctx.data_mut(|d| d.insert_temp(key_id, key.clone()));
    }
    if cancel || !*open {
        app.preview_undo();
        *open = false;
        if app.state.pick_mode == Some(PickMode::CloneAlignDestination) {
            let _ = app.dispatch(Command::CloneAndAlignPick);
        }
        ctx.data_mut(|d| d.remove::<Key>(key_id));
    }
    ctx.data_mut(|d| {
        d.insert_temp(id, st);
        d.insert_temp(list_id, show_list);
    });
}
