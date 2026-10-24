//! Modify tab: name and colour, Modifier List, the modifier
//! button set, the stack list with lightbulbs and its five buttons, then
//! the rollouts of the active entry (base object parameters, Editable Poly
//! rollouts, or the modifier's parameters and sub-object levels).

use super::{create, rollout};
use crate::shortcuts::HoverShortcut;
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, SelectionMode};
use loom_scene::Modifier;

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    create::name_and_color(ui, app);
    let Some(id) = app.state.selection.iter().next().copied() else {
        return;
    };
    let Some(node) = app.state.scene.node(id) else { return };
    let stack_name = node.object.stack_name().to_owned();
    let is_poly = node.object.poly().is_some();
    let is_spline = node.object.spline().is_some();
    let is_group = matches!(node.object, loom_scene::ObjectKind::Group { .. });
    let node_instanced = node.instance_of.is_some();
    let entries: Vec<(String, bool, bool, Option<(loom_scene::NodeId, usize)>, &'static [&'static str], u32)> =
        node.modifiers.iter().map(|s| (s.name.clone(), s.enabled, s.enabled_in_viewport, s.instance_of, s.modifier.sub_levels(), s.sub_level)).collect();
    let base_levels = loom_commands::base_sub_levels(&node.object);
    let base_object = node.object.clone();
    let active = node.active_entry;
    let warnings = app.state.scene.eval_mesh(id).warnings.clone();

    // Modifier List.
    ui.horizontal(|ui| {
        // `width` is the combo's text area: its padding and arrow come on top,
        // so they are reserved too (else the row outgrows the panel every frame).
        let sp = ui.spacing();
        let chrome = sp.button_padding.x * 2.0 + sp.icon_width + sp.icon_spacing + sp.item_spacing.x;
        let w = (ui.available_width() - 28.0 - chrome).max(40.0);
        let combo = egui::ComboBox::from_id_salt("modifier_list").selected_text("Modifier List").width(w).height(480.0).show_ui(ui, |ui| {
            // Search: typed text filters the list; Enter adds the first match.
            let query_id = egui::Id::new("modifier_list_query");
            let mut query: String = ui.ctx().data(|d| d.get_temp(query_id)).unwrap_or_default();
            let r = ui.add(egui::TextEdit::singleline(&mut query).hint_text("Search modifiers").desired_width(f32::INFINITY));
            if !r.has_focus() && !r.lost_focus() {
                r.request_focus();
            }
            ui.ctx().data_mut(|d| d.insert_temp(query_id, query.clone()));
            let mut add: Option<String> = None;
            if !query.trim().is_empty() {
                let hits = loom_commands::modifier_list::search(&query);
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    add = hits.first().cloned();
                }
                for name in &hits {
                    if ui.add_enabled(!is_group, egui::Button::new(name.as_str())).clicked() {
                        add = Some(name.clone());
                    }
                }
                if hits.is_empty() {
                    ui.weak("No modifier matches");
                }
            } else {
                ui.add_enabled(false, egui::Button::new(egui::RichText::new("MOST USED").small()));
                for name in loom_commands::modifier_list::most_used(&app.state.modifier_usage, 8) {
                    if ui.add_enabled(!is_group, egui::Button::new(name.as_str())).clicked() {
                        add = Some(name);
                    }
                }
                ui.add_enabled(false, egui::Button::new(egui::RichText::new("WORLD-SPACE MODIFIERS").small()));
                for name in Modifier::WSM_LIST {
                    if ui.add_enabled(!is_group, egui::Button::new(*name)).clicked() {
                        add = Some((*name).to_owned());
                    }
                }
                ui.add_enabled(false, egui::Button::new(egui::RichText::new("OBJECT-SPACE MODIFIERS").small()));
                for name in Modifier::LIST {
                    if ui.add_enabled(!is_group, egui::Button::new(*name)).clicked() {
                        add = Some((*name).to_owned());
                    }
                }
            }
            if let Some(name) = add {
                let _ = app.dispatch(Command::AddModifier { name });
                ui.ctx().data_mut(|d| d.remove::<String>(query_id));
                ui.close_menu();
            }
        });
        if combo.inner.is_none() {
            // Closed: the next opening starts with an empty search.
            ui.ctx().data_mut(|d| d.remove::<String>(egui::Id::new("modifier_list_query")));
        }
        if ui.small_button(if app.state.show_modifier_set { "▲" } else { "▼" }).on_hover_text("Show / hide the modifier button set").clicked() {
            let _ = app.dispatch(Command::ModifierSetButtonsToggle);
        }
    });
    if app.state.show_modifier_set {
        let set = app.state.modifier_set.clone();
        egui::Grid::new("modifier_set").num_columns(2).spacing([4.0, 2.0]).show(ui, |ui| {
            for (i, name) in set.iter().enumerate() {
                if ui.add_sized([ui.available_width().max(100.0), 18.0], egui::Button::new(name)).clicked() {
                    let _ = app.dispatch(Command::AddModifier { name: name.clone() });
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
    }

    // The stack: top entry first, the base object last.
    let mut select: Option<Option<usize>> = None;
    let mut toggle: Option<(usize, bool)> = None;
    let mut delete: Option<usize> = None;
    let mut collapse_to: Option<usize> = None;
    let mut move_to: Option<(usize, usize)> = None;
    let mut rename: Option<(usize, String)> = None;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_height(90.0);
        ui.set_min_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 1.0;
        // `+` / `−` in front of entries with sub-object levels. Entering a level
        // opens the entry; `−` closes it and, if one of its levels is active,
        // returns to the top level. Returns (open, closed while in a level).
        let expander = |ui: &mut egui::Ui, key: (u64, Option<usize>), has_levels: bool, in_level: bool| -> (bool, bool) {
            if !has_levels {
                ui.add_space(14.0);
                return (false, false);
            }
            let id = egui::Id::new(("stack_open", key));
            let stored = ui.ctx().data(|d| d.get_temp::<bool>(id));
            // A level entered elsewhere (hotkey, quad menu) opens the entry.
            let mut open = stored.unwrap_or(false) || (in_level && stored.is_none());
            if in_level && !open && stored == Some(false) {
                // Closed earlier, then a level was entered again: open it.
                open = ui.ctx().data(|d| d.get_temp::<bool>(id.with("was_in_level"))) == Some(false);
            }
            let mut leave = false;
            if ui.add(egui::Button::new(if open { "−" } else { "+" }).frame(false).min_size(egui::vec2(10.0, 0.0))).clicked() {
                open = !open;
                leave = !open && in_level;
            }
            ui.ctx().data_mut(|d| {
                d.insert_temp(id, open);
                d.insert_temp(id.with("was_in_level"), in_level && !leave);
            });
            (open, leave)
        };
        let level_row = |ui: &mut egui::Ui, label: &str, current: bool, chord: Option<String>| -> bool {
            ui.horizontal(|ui| {
                ui.add_space(40.0);
                let r = ui.add(egui::SelectableLabel::new(current, label));
                match chord {
                    Some(c) => r.on_hover_text(format!("{label} ({c})")),
                    None => r,
                }
                .clicked()
            })
            .inner
        };
        for (i, (name, enabled, _in_vp, instance, levels, lvl)) in entries.iter().enumerate().rev() {
            let mut open = false;
            ui.horizontal(|ui| {
                let (o, leave) = expander(ui, (id.0, Some(i)), !levels.is_empty(), active == Some(i) && *lvl > 0);
                open = o;
                if leave {
                    let _ = app.dispatch(Command::ModifierSubobjectLevel { index: i as u32, level: 0 });
                }
                let bulb = if *enabled { "💡" } else { "○" };
                if ui.add(egui::Button::new(bulb).frame(false)).on_hover_text("Modifier on / off").clicked() {
                    toggle = Some((i, !*enabled));
                }
                let label = if instance.is_some() { format!("{name}  (instanced)") } else { name.clone() };
                let text = if active == Some(i) { egui::RichText::new(label).strong() } else { egui::RichText::new(label) };
                let r = ui.add(egui::SelectableLabel::new(active == Some(i), text));
                if r.clicked() {
                    select = Some(Some(i));
                }
                // Drag to reorder: dropping on another row moves the entry there.
                let drag_id = egui::Id::new(("stack_drag", id.0));
                if r.drag_started() {
                    ui.ctx().data_mut(|d| d.insert_temp(drag_id, i));
                }
                if r.hovered() && ui.input(|inp| inp.pointer.any_released()) {
                    if let Some(from) = ui.ctx().data(|d| d.get_temp::<usize>(drag_id)) {
                        if from != i {
                            move_to = Some((from, i));
                        }
                        ui.ctx().data_mut(|d| d.remove::<usize>(drag_id));
                    }
                }
                r.context_menu(|ui| {
                    if ui.button("Rename").clicked() {
                        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("stack_rename", id.0)), (i, name.clone())));
                        ui.close_menu();
                    }
                    if ui.button("Delete").clicked() {
                        delete = Some(i);
                        ui.close_menu();
                    }
                    if ui.button(if *enabled { "Off" } else { "On" }).clicked() {
                        toggle = Some((i, !*enabled));
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Collapse To").clicked() {
                        collapse_to = Some(i);
                        ui.close_menu();
                    }
                    if ui.button("Collapse All").clicked() {
                        let _ = app.dispatch(Command::CollapseAll);
                        ui.close_menu();
                    }
                    if ui.add_enabled(instance.is_some(), egui::Button::new("Make Unique")).clicked() {
                        let _ = app.dispatch(Command::SelectStackEntry { index: Some(i as u32) });
                        let _ = app.dispatch(Command::MakeUnique);
                        ui.close_menu();
                    }
                });
            });
            if open {
                for (j, l) in levels.iter().enumerate() {
                    let level = j as u32 + 1;
                    let current = active == Some(i) && *lvl == level;
                    if level_row(ui, l, current, None) {
                        let _ = app.dispatch(Command::ModifierSubobjectLevel { index: i as u32, level: if current { 0 } else { level } });
                    }
                }
            }
        }
        let mode = app.state.selection_mode;
        let base_in_level = active.is_none() && mode != SelectionMode::Object;
        let mut base_open = false;
        ui.horizontal(|ui| {
            let (o, leave) = expander(ui, (id.0, None), !base_levels.is_empty(), base_in_level);
            base_open = o;
            if leave {
                let _ = app.dispatch(Command::SelectionMode { mode: SelectionMode::Object });
            }
            let base = if active.is_none() { egui::RichText::new(&stack_name).strong() } else { egui::RichText::new(&stack_name) };
            let r = ui.add(egui::SelectableLabel::new(active.is_none(), base));
            if r.clicked() {
                select = Some(None);
            }
            // Right-click on the base object: Convert To, Collapse All.
            r.context_menu(|ui| {
                ui.menu_button("Convert To", |ui| {
                    for (label, cmd) in base_convert_entries(&base_object) {
                        if ui.button(label).clicked() {
                            let _ = app.dispatch(cmd);
                            ui.close_menu();
                        }
                    }
                });
                if ui.add_enabled(!entries.is_empty(), egui::Button::new("Collapse All")).clicked() {
                    let _ = app.dispatch(Command::CollapseAll);
                    ui.close_menu();
                }
            });
        });
        if base_open {
            for (l, m) in &base_levels {
                let current = base_in_level && mode == *m;
                if level_row(ui, l, current, crate::shortcuts::chord(&app.state, crate::shortcuts::level_action(*m))) {
                    if active.is_some() {
                        let _ = app.dispatch(Command::SelectStackEntry { index: None });
                    }
                    let _ = app.dispatch(Command::SelectionMode { mode: if current { SelectionMode::Object } else { *m } });
                }
            }
        }
    });
    // Inline rename box.
    let rename_id = egui::Id::new(("stack_rename", id.0));
    if let Some((i, mut name)) = ui.ctx().data(|d| d.get_temp::<(usize, String)>(rename_id)) {
        ui.horizontal(|ui| {
            let r = ui.text_edit_singleline(&mut name);
            if r.lost_focus() && ui.input(|inp| inp.key_pressed(egui::Key::Enter)) {
                rename = Some((i, name.clone()));
                ui.ctx().data_mut(|d| d.remove::<(usize, String)>(rename_id));
            } else if r.changed() {
                ui.ctx().data_mut(|d| d.insert_temp(rename_id, (i, name.clone())));
            }
            if ui.button("Cancel").clicked() {
                ui.ctx().data_mut(|d| d.remove::<(usize, String)>(rename_id));
            }
        });
    }
    // The five buttons under the stack.
    ui.horizontal(|ui| {
        if ui.selectable_label(app.state.pin_stack, "📌").on_hover_text("Pin Stack").clicked() {
            let _ = app.dispatch(Command::PinStackToggle);
        }
        if ui.selectable_label(app.state.show_end_result, "⏚").on_hover_text("Show End Result On/Off Toggle").clicked() {
            let _ = app.dispatch(Command::ShowEndResultToggle);
        }
        let instanced = active.and_then(|a| entries.get(a)).is_some_and(|e| e.3.is_some()) || node_instanced;
        if ui.add_enabled(instanced, egui::Button::new("⧉")).on_hover_text("Make Unique").clicked() {
            let _ = app.dispatch(Command::MakeUnique);
        }
        if ui.add_enabled(active.is_some(), egui::Button::new("🗑")).on_hover_text("Remove modifier from the stack").clicked() {
            if let Some(a) = active {
                delete = Some(a);
            }
        }
        if ui.button("⚙").on_hover_text("Configure Modifier Sets").clicked() {
            let _ = app.dispatch(Command::ModifierSetButtonsToggle);
        }
    });
    if let Some(sel) = select {
        let _ = app.dispatch(Command::SelectStackEntry { index: sel.map(|i| i as u32) });
    }
    if let Some((i, on)) = toggle {
        let _ = app.dispatch(Command::ToggleModifier { index: i as u32, on });
    }
    if let Some(i) = delete {
        let _ = app.dispatch(Command::DeleteModifier { index: i as u32 });
    }
    if let Some(i) = collapse_to {
        let _ = app.dispatch(Command::CollapseTo { index: i as u32 });
    }
    if let Some((from, to)) = move_to {
        let _ = app.dispatch(Command::MoveModifier { from: from as u32, to: to as u32 });
    }
    if let Some((i, name)) = rename {
        let _ = app.dispatch(Command::RenameModifier { index: i as u32, name });
    }
    for w in &warnings {
        ui.colored_label(egui::Color32::from_rgb(255, 180, 60), w);
    }

    // Rollouts of the active entry.
    match active {
        None => {
            create::parameters(ui, app);
            if is_spline {
                super::modify_spline::selection::show(ui, app);
                super::modify_poly::soft_selection::show(ui, app);
                super::modify_spline::geometry::show(ui, app);
                return;
            }
            if !is_poly {
                return;
            }
            poly_rollouts(ui, app);
        }
        Some(a) => modifier_rollouts(ui, app, id, a),
    }
}

/// The Editable Poly rollouts, for the base object or an active Edit Poly entry.
fn poly_rollouts(ui: &mut egui::Ui, app: &mut LoomApp) {
    super::modify_poly::selection::show(ui, app);
    super::modify_poly::soft_selection::show(ui, app);
    super::modify_poly::edit_vertices::show(ui, app);
    super::modify_poly::edit_edges::show(ui, app);
    super::modify_poly::edit_borders::show(ui, app);
    super::modify_poly::edit_polygons::show(ui, app);
    super::modify_poly::edit_elements::show(ui, app);
    super::modify_poly::edit_geometry::show(ui, app);
    super::modify_poly::vertex_properties::show(ui, app);
    super::modify_poly::polygon_properties::show(ui, app);
    super::modify_poly::subdivision_surface::show(ui, app);
    super::modify_poly::paint_deformation::show(ui, app);
}

/// A modifier's rollouts: its sub-object levels and its parameters.
fn modifier_rollouts(ui: &mut egui::Ui, app: &mut LoomApp, id: loom_scene::NodeId, index: usize) {
    let Some(node) = app.state.scene.node(id) else { return };
    let Some(slot) = node.modifiers.get(index) else { return };
    let modifier = app.state.scene.resolved_modifier(id, index).unwrap_or_else(|| slot.modifier.clone());
    let levels = modifier.sub_levels();
    let name = slot.name.clone();
    let sub_level = slot.sub_level;
    if matches!(modifier, Modifier::EditSpline { .. }) {
        // Edit Spline: the Editable Spline rollouts work on the entry's data.
        super::modify_spline::selection::show(ui, app);
        super::modify_poly::soft_selection::show(ui, app);
        super::modify_spline::geometry::show(ui, app);
        return;
    }
    if matches!(modifier, Modifier::EditPoly { .. }) {
        // Edit Poly: the Editable Poly rollouts work on the entry's data.
        if name == "Edit Mesh" {
            ui.label(egui::RichText::new("Edit Mesh is edited as Edit Poly.").small());
        }
        poly_rollouts(ui, app);
        return;
    }
    if !levels.is_empty() {
        rollout(ui, &format!("{name}: Sub-object"), true, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (i, l) in levels.iter().enumerate() {
                    let lvl = i as u32 + 1;
                    if ui.selectable_label(sub_level == lvl, *l).clicked() {
                        let _ = app.dispatch(Command::ModifierSubobjectLevel { index: index as u32, level: if sub_level == lvl { 0 } else { lvl } });
                    }
                }
            });
        });
    }
    let sweep_custom = matches!(&modifier, Modifier::Sweep { section, .. } if section.get() == 10);
    let vol_mesh = matches!(&modifier, Modifier::VolSelect(d) if d.volume == 3);
    if modifier.picked_node().is_some() || matches!(modifier, Modifier::LinkedXForm { .. } | Modifier::Preserve { .. }) || sweep_custom || vol_mesh {
        rollout(ui, "Parameters", true, |ui| {
            let label = match modifier.picked_node().and_then(|n| app.state.scene.node(n)) {
                Some(n) => n.name.clone(),
                None => "None".to_owned(),
            };
            let what = if matches!(modifier, Modifier::LinkedXForm { .. }) { "Control Object" } else if sweep_custom { "Section Shape" } else if matches!(modifier, Modifier::VolSelect(_)) { "Mesh Object" } else if matches!(modifier, Modifier::BevelProfile { .. }) { "Profile" } else if matches!(modifier, Modifier::PathDeform(_)) { "Path" } else if matches!(modifier, Modifier::Substitute { .. }) { "Scene Object" } else if matches!(modifier, Modifier::CameraMap(_)) { "Camera" } else { "Original" };
            ui.label(format!("{what}: {label}"));
            let picking = app.state.pick_mode == Some(loom_commands::PickMode::ModifierNode);
            if ui.selectable_label(picking, format!("Pick {what}")).clicked() {
                app.state.pick_mode = if picking { None } else { Some(loom_commands::PickMode::ModifierNode) };
            }
        });
    }
    let unpicked = match &modifier {
        Modifier::BevelProfile { profile: None, .. } => Some("Profile"),
        Modifier::PathDeform(d) if d.path.is_none() => Some("Path"),
        Modifier::Substitute { node: None } => Some("Scene Object"),
        Modifier::CameraMap(d) if d.camera.is_none() => Some("Camera"),
        _ => None,
    };
    if let Some(what) = unpicked {
        rollout(ui, "Parameters", true, |ui| {
            ui.label(format!("{what}: None"));
            let picking = app.state.pick_mode == Some(loom_commands::PickMode::ModifierNode);
            if ui.selectable_label(picking, format!("Pick {what}")).clicked() {
                app.state.pick_mode = if picking { None } else { Some(loom_commands::PickMode::ModifierNode) };
            }
        });
    }
    if let Modifier::Hsds(d) = &modifier {
        let levels = d.levels.len();
        rollout(ui, "HSDS Parameters", true, |ui| {
            ui.label(format!("Subdivision Stack: base + {levels} level(s)"));
            if ui.button("Subdivide").on_hover_text("Split the selected polygons of the finest level into a new level").clicked() {
                let _ = app.dispatch(Command::HsdsSubdivide { faces: None });
            }
            let id = egui::Id::new("hsds_adaptive");
            let (mut lvl, mut ang): (u32, f32) = ui.ctx().data(|m| m.get_temp(id)).unwrap_or((2, 30.0));
            ui.horizontal(|ui| {
                if ui.button("Adaptive Subdivision").on_hover_text("Split polygons with an edge sharper than Max Angle, up to Max Level").clicked() {
                    let _ = app.dispatch(Command::HsdsAdaptiveSubdivide { max_level: lvl, max_angle: ang });
                }
            });
            egui::Grid::new("hsds_adaptive_grid").num_columns(2).show(ui, |ui| {
                ui.label("Max Level:");
                ui.add(crate::spinner::spin(ui, &mut lvl, 1..=8));
                ui.end_row();
                ui.label("Max Angle:");
                ui.add(crate::spinner::spin(ui, &mut ang, 0.0..=180.0).speed(0.5));
                ui.end_row();
            });
            ui.ctx().data_mut(|m| m.insert_temp(id, (lvl, ang)));
        });
    }
    if let Modifier::PointCache(d) = &modifier {
        let file = if d.path.is_empty() { "None".to_string() } else { std::path::Path::new(&d.path).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default() };
        let loaded = !d.path.is_empty();
        rollout(ui, "Point Cache Parameters", true, |ui| {
            ui.label(format!("Cache File: {file}"));
            ui.horizontal(|ui| {
                if ui.button("Record").on_hover_text("Record the stack below over the active time segment into a .pc2 file").clicked() {
                    let (start, end) = app.state.frame_range;
                    app.state.dialog = Some(loom_commands::Dialog::PointCacheRecord { index: index as u32, start, end });
                }
                if ui.button("Load").clicked() {
                    app.state.dialog = Some(loom_commands::Dialog::PointCacheLoad { index: index as u32 });
                }
                if ui.add_enabled(loaded, egui::Button::new("Unload")).clicked() {
                    let _ = app.dispatch(Command::PointCacheUnload { index: index as u32 });
                }
            });
        });
    }
    if let Modifier::SplineIkControl(d) = &modifier {
        let made = d.helpers.iter().filter(|h| h.is_some()).count();
        rollout(ui, "Spline IK Control Parameters", true, |ui| {
            if ui.add_enabled(made == 0, egui::Button::new("Create Helpers")).on_hover_text("A Point helper at every knot; the knots follow them").clicked() {
                let _ = app.dispatch(Command::SplineIkControlCreateHelpers);
            }
            ui.label(format!("Helper Count: {made}"));
        });
    }
    if let Modifier::FilletChamfer(d) = &modifier {
        let has = !d.ids.is_empty();
        rollout(ui, "Edit Vertex", true, |ui| {
            ui.label(format!("{} vertices selected, {} operations", d.ids.len(), d.ops.len()));
            ui.horizontal(|ui| {
                if ui.add_enabled(has && d.radius > 0.0, egui::Button::new("Apply Fillet")).on_hover_text("Fillet the selected vertices by Radius").clicked() {
                    let _ = app.dispatch(Command::FilletChamferApply { fillet: true });
                }
                if ui.add_enabled(has && d.distance > 0.0, egui::Button::new("Apply Chamfer")).on_hover_text("Chamfer the selected vertices by Distance").clicked() {
                    let _ = app.dispatch(Command::FilletChamferApply { fillet: false });
                }
            });
        });
    }
    if let Modifier::TrimExtend { picks } = &modifier {
        let n = picks.len();
        rollout(ui, "Pick Locations", true, |ui| {
            let picking = app.state.pick_mode == Some(loom_commands::PickMode::TrimExtendModifier);
            if ui.selectable_label(picking, "Pick Locations").on_hover_text("Click the spline where it should be trimmed or extended").clicked() {
                app.state.pick_mode = if picking { None } else { Some(loom_commands::PickMode::TrimExtendModifier) };
            }
            ui.label(format!("{n} locations"));
            ui.label("Operation:");
            let op = app.state.spline_settings.trim_extend_op;
            for (i, l) in ["Auto", "Trim Only", "Extend Only"].iter().enumerate() {
                if ui.radio(op == i as u32, *l).clicked() {
                    let _ = app.dispatch(Command::SplineSettings { name: "Trim/Extend Operation".into(), value: i as f64 });
                }
            }
        });
    }
    if let Modifier::STLCheck { check, test, .. } = &modifier {
        let input = match index.checked_sub(1) {
            Some(prev) => app.state.scene.evaluated(id, Some(prev)).value.mesh.clone(),
            None => app.state.scene.node(id).map(|n| n.object.display_output()).unwrap_or_default(),
        };
        let (check, test) = (check.get(), test.get());
        rollout(ui, "STL Check Status", true, |ui| {
            if !check {
                ui.label("Check is off.");
                return;
            }
            // The report is memoised per node version (the check is a full mesh walk).
            let key = egui::Id::new(("stl_report", id.0, index));
            let version = app.state.scene.node(id).map(|n| n.geom_version).unwrap_or(0);
            let cached: Option<(u64, loom_mesh::mesh_mods::StlReport)> = ui.ctx().data(|d| d.get_temp(key));
            let r = match cached {
                Some((v, r)) if v == version => r,
                _ => {
                    let r = loom_mesh::mesh_mods::stl_check(&input);
                    ui.ctx().data_mut(|d| d.insert_temp(key, (version, r.clone())));
                    r
                }
            };
            let rows = [("Open Edges", r.open_edges), ("Double Faces", r.double_faces), ("Spikes", r.spikes), ("Multiple Edges", r.multiple_edges)];
            for (i, (label, n)) in rows.iter().enumerate() {
                if test == 4 || test as usize == i {
                    ui.label(format!("{label}: {n}"));
                }
            }
        });
    }
    if let Modifier::ProOptimizer { calculated, .. } | Modifier::MultiRes { generated: calculated, .. } = &modifier {
        let (button, pname) = if matches!(modifier, Modifier::ProOptimizer { .. }) { ("Calculate", "Calculate") } else { ("Generate", "Generate") };
        let done = calculated.get();
        rollout(ui, "Optimization Level", true, |ui| {
            if ui.add_enabled(!done, egui::Button::new(button)).clicked() {
                let _ = app.dispatch(Command::SetModifierParameter { index: index as u32, name: pname.into(), value: 1.0 });
            }
            let stats = app.state.scene.evaluated(id, Some(index)).value.mesh.clone();
            ui.label(format!("Vertices: {}  Faces: {}", loom_mesh::optimize::triangle_vertex_count(&stats), stats.face_count()));
        });
    }
    if let Modifier::VertexPaint { color, .. } = &modifier {
        let mut rgb = [color[0].get(), color[1].get(), color[2].get()];
        rollout(ui, "VertexPaint", true, |ui| {
            ui.horizontal(|ui| {
                let painting = app.state.vertex_paint_brush;
                if ui.selectable_label(painting, "Paint").on_hover_text("Paint vertex colours in the viewports").clicked() {
                    let _ = app.dispatch(Command::VertexPaintBrushToggle);
                }
                if ui.color_edit_button_rgb(&mut rgb).changed() {
                    for (i, n) in ["Color R", "Color G", "Color B"].iter().enumerate() {
                        let _ = app.dispatch(Command::SetModifierParameter { index: index as u32, name: (*n).into(), value: rgb[i] as f64 });
                    }
                }
            });
        });
    }
    if matches!(modifier, Modifier::Skin(_)) {
        super::skin_rollouts::show(ui, app, id, index);
    }
    if matches!(modifier, Modifier::Morpher(_)) {
        super::morph_rollouts::morpher(ui, app, id, index);
    }
    if matches!(modifier, Modifier::SkinMorph(_)) {
        super::morph_rollouts::skin_morph(ui, app, id, index);
    }
    if matches!(modifier, Modifier::SkinWrap(_)) {
        super::morph_rollouts::skin_wrap(ui, app, id, index);
    }
    if matches!(modifier, Modifier::EditNormals(_)) {
        super::normals_rollout::show(ui, app);
    }
    if let Modifier::UnwrapUVW(u) = &modifier {
        let u = (**u).clone();
        let set = |app: &mut LoomApp, name: &str, value: f64| {
            let _ = app.dispatch(Command::SetModifierParameter { index: index as u32, name: name.into(), value });
        };
        rollout(ui, "Selection Parameters", true, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("+").on_hover_text("Expand Geom. Polygons").clicked() {
                    let _ = app.dispatch(Command::ExpandGeomPolygons);
                }
                if ui.button("-").on_hover_text("Contract Geom. Polygons").clicked() {
                    let _ = app.dispatch(Command::ContractGeomPolygons);
                }
                if ui.button("Ring").clicked() {
                    let _ = app.dispatch(Command::GeomEdgeRingSelection);
                }
                if ui.button("Loop").clicked() {
                    let _ = app.dispatch(Command::GeomEdgeLoopSelection);
                }
            });
            let mut ib = u.ignore_backfacing.get();
            if ui.checkbox(&mut ib, "Ignore Backfacing").changed() {
                let _ = app.dispatch(Command::IgnoreBackPolygons);
            }
            let mut se = u.select_by_element.get();
            if ui.checkbox(&mut se, "Select By Element").changed() {
                let _ = app.dispatch(Command::GeomElementSelectMode);
            }
            ui.horizontal(|ui| {
                let mut a = u.planar_angle.get();
                if ui.button("Planar Angle").clicked() {
                    let _ = app.dispatch(Command::PlanarThreshold);
                }
                if ui.add(crate::spinner::spin(ui, &mut a, 0.0..=180.0).speed(0.5).fixed_decimals(1)).changed() {
                    set(app, "Planar Angle", a as f64);
                }
            });
            ui.horizontal(|ui| {
                let mut m = u.select_matid.get();
                if ui.button("Select MatID").clicked() {
                    let _ = app.dispatch(Command::SelectByMaterialIdUnwrap);
                }
                if ui.add(crate::spinner::spin(ui, &mut m, 1..=65535)).changed() {
                    set(app, "Select MatID", m as f64);
                }
            });
            ui.horizontal(|ui| {
                let mut g = u.select_sg.get();
                if ui.button("Select SG").clicked() {
                    let _ = app.dispatch(Command::SelectBySmoothingGroupUnwrap);
                }
                if ui.add(crate::spinner::spin(ui, &mut g, 1..=32)).changed() {
                    set(app, "Select SG", g as f64);
                }
            });
        });
        rollout(ui, "Parameters", true, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Edit...").on_hover_cmd(&app.state, "Open the Edit UVWs window", Command::EditUvws.name()).clicked() {
                    let _ = app.dispatch(Command::EditUvws);
                }
                if ui.button("Reset UVWs").clicked() {
                    let _ = app.dispatch(Command::ResetUvws);
                }
                if ui.button("Save...").clicked() {
                    let _ = app.dispatch(Command::SaveUvw { path: None });
                }
                if ui.button("Load...").with_shortcut(&app.state, Command::LoadUvw { path: None }.name()).clicked() {
                    let _ = app.dispatch(Command::LoadUvw { path: None });
                }
            });
            ui.label(egui::RichText::new("Channel:").strong());
            ui.horizontal(|ui| {
                let ch = u.channel.get();
                if ui.radio(ch > 0, "Map Channel").clicked() && ch == 0 {
                    let _ = app.dispatch(Command::UnwrapMapChannel { channel: 1 });
                }
                let mut c = ch.max(1);
                if ui.add_enabled(ch > 0, crate::spinner::spin(ui, &mut c, 1..=99)).changed() {
                    let _ = app.dispatch(Command::UnwrapMapChannel { channel: c });
                }
                if ui.radio(ch == 0, "Vertex Color Channel").clicked() && ch != 0 {
                    let _ = app.dispatch(Command::UnwrapMapChannel { channel: 0 });
                }
            });
            ui.label(egui::RichText::new("Display:").strong());
            let mut ss = u.show_seams.get();
            if ui.checkbox(&mut ss, "Show Seams").with_shortcut(&app.state, Command::ShowSeamsInViewport.name()).changed() {
                let _ = app.dispatch(Command::ShowSeamsInViewport);
            }
            ui.horizontal(|ui| {
                let thin = u.thin_seams.get();
                if ui.radio(thin, "Thin Seam Display").clicked() && !thin {
                    set(app, "Thin Seam Display", 1.0);
                }
                if ui.radio(!thin, "Thick Seam Display").clicked() && thin {
                    set(app, "Thin Seam Display", 0.0);
                }
            });
            let mut ap = u.always_show_pelt_seam.get();
            if ui.checkbox(&mut ap, "Always Show Pelt Seam").changed() {
                let _ = app.dispatch(Command::PeltAlwaysShowSeams);
            }
            let mut pr = u.prevent_reflattening.get();
            if ui.checkbox(&mut pr, "Prevent Reflattening").changed() {
                let _ = app.dispatch(Command::PreventReflattening);
            }
        });
        rollout(ui, "Map Parameters", true, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (label, cmd) in [("Planar", Command::PlanarMapFaces), ("Pelt", Command::PeltDialog), ("Cylindrical", Command::CylindricalMap), ("Spherical", Command::SphericalMap), ("Box", Command::BoxMap)] {
                    if ui.button(label).with_shortcut(&app.state, cmd.name()).clicked() {
                        let _ = app.dispatch(cmd);
                    }
                }
                let picking = app.state.pick_mode == Some(loom_commands::PickMode::UnwrapSplineMap);
                if ui.selectable_label(picking, "Spline").on_hover_text("Spline Map: pick a shape; the selected faces map along it").clicked() {
                    let _ = app.dispatch(Command::SplineMap);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Spline:");
                let m = app.state.uv_editor.spline_map_mapping;
                for (i, l) in ["Circular", "Planar"].iter().enumerate() {
                    if ui.radio(m == i as u32, *l).clicked() {
                        let _ = app.dispatch(Command::SetUnwrapOption { name: "Spline Map Mapping".into(), value: i as f64 });
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                for (label, cmd) in [("Align X", Command::MappingAlignX), ("Align Y", Command::MappingAlignY), ("Align Z", Command::MappingAlignZ), ("Best Align", Command::MappingAlignNormal), ("Fit", Command::MappingFit), ("Align To View", Command::MappingAlignToView), ("Center", Command::MappingCenter), ("Reset", Command::MappingReset)] {
                    if ui.button(label).clicked() {
                        let _ = app.dispatch(cmd);
                    }
                }
            });
            let mut nm = u.normalize_map.get();
            if ui.checkbox(&mut nm, "Normalize Map").changed() {
                set(app, "Normalize Map", if nm { 1.0 } else { 0.0 });
            }
            ui.horizontal_wrapped(|ui| {
                let es = app.state.uv_editor.edit_seams;
                if ui.selectable_label(es, "Edit Seams").clicked() {
                    let _ = app.dispatch(Command::PeltEditSeams);
                }
                let p2p = matches!(app.state.pick_mode, Some(loom_commands::PickMode::SeamPointA | loom_commands::PickMode::SeamPointB));
                if ui.selectable_label(p2p, "Point To Point Seam").clicked() {
                    if p2p {
                        app.state.pick_mode = None;
                    } else {
                        let _ = app.dispatch(Command::PointToPointEdgeSelection);
                    }
                }
                if ui.button("Edge Sel To Seams").clicked() {
                    let _ = app.dispatch(Command::EdgeSelToPeltSeamAdd);
                }
                if ui.button("Exp. Face Sel To Seams").clicked() {
                    let _ = app.dispatch(Command::PeltExpandSelectionToSeams);
                }
                if ui.button("Clear Seams").clicked() {
                    let _ = app.dispatch(Command::ClearSeams);
                }
            });
            if ui.button("Quick Planar Map").clicked() {
                let _ = app.dispatch(Command::QuickPlanarMap);
            }
        });
        return;
    }
    if let Modifier::UVWMap { .. } = &modifier {
        rollout(ui, "Alignment", true, |ui| {
            let pick = app.state.pick_mode;
            ui.horizontal_wrapped(|ui| {
                if ui.button("Fit").clicked() {
                    let _ = app.dispatch(Command::UvwMapFit);
                }
                if ui.button("Center").clicked() {
                    let _ = app.dispatch(Command::UvwMapCenter);
                }
                if ui.button("Bitmap Fit...").clicked() {
                    app.state.dialog = Some(loom_commands::Dialog::BitmapFit);
                }
                if ui.selectable_label(pick == Some(loom_commands::PickMode::UvwMapNormal), "Normal Align").clicked() {
                    if pick == Some(loom_commands::PickMode::UvwMapNormal) {
                        app.state.pick_mode = None;
                    } else {
                        let _ = app.dispatch(Command::UvwMapNormalAlign);
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.button("View Align").clicked() {
                    let _ = app.dispatch(Command::UvwMapViewAlign);
                }
                let region = matches!(pick, Some(loom_commands::PickMode::UvwMapRegionA | loom_commands::PickMode::UvwMapRegionB));
                if ui.selectable_label(region, "Region Fit").on_hover_text("Click two corners in a viewport").clicked() {
                    app.state.pick_mode = if region { None } else { Some(loom_commands::PickMode::UvwMapRegionA) };
                }
                if ui.button("Reset").clicked() {
                    let _ = app.dispatch(Command::UvwMapReset);
                }
                let acquire = pick == Some(loom_commands::PickMode::UvwMapAcquire);
                if ui.selectable_label(acquire, "Acquire").on_hover_text("Click the object whose UVW Map to copy").clicked() {
                    app.state.pick_mode = if acquire { None } else { Some(loom_commands::PickMode::UvwMapAcquire) };
                }
            });
        });
    }
    if let Modifier::FFD { kind, dims, .. } = &modifier {
        let (kind, dims) = (*kind, *dims);
        rollout(ui, "FFD Parameters", true, |ui| {
            ui.label(format!("Lattice: {} x {} x {} points", dims[0], dims[1], dims[2]));
            if kind >= 3 {
                let key = egui::Id::new(("ffd_dims", id.0, index));
                let mut d: [u32; 3] = ui.ctx().data(|m| m.get_temp(key)).unwrap_or(dims);
                ui.horizontal(|ui| {
                    for (i, label) in (if kind == 4 { ["Side", "Radial", "Height"] } else { ["Length", "Width", "Height"] }).iter().enumerate() {
                        ui.label(*label);
                        ui.add(crate::spinner::spin(ui, &mut d[i], 2..=32));
                    }
                });
                ui.ctx().data_mut(|m| m.insert_temp(key, d));
                if ui.button("Set Number of Points").clicked() {
                    let _ = app.dispatch(Command::SetNumberOfPoints { length: d[0], width: d[1], height: d[2] });
                }
            }
            ui.horizontal(|ui| {
                if ui.button("Reset Lattice").clicked() {
                    let _ = app.dispatch(Command::ResetLattice);
                }
                if ui.button("Conform To Shape").clicked() {
                    let _ = app.dispatch(Command::ConformToShape);
                }
            });
        });
    }
    let params = modifier.parameters();
    // Skin, Morpher and Skin Morph draw their own rollouts.
    if params.is_empty() || matches!(modifier, Modifier::Skin(_) | Modifier::Morpher(_) | Modifier::SkinMorph(_) | Modifier::SkinWrap(_) | Modifier::EditNormals(_)) {
        return;
    }
    rollout(ui, "Parameters", true, |ui| {
        egui::Grid::new(("modifier_params", id.0, index)).num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            for (pname, value) in params {
                ui.label(pname);
                match value {
                    loom_scene::ParamValue::Float(f) => {
                        let mut v = f;
                        let r = ui.add(crate::spinner::spin(ui, &mut v, ..).speed(app.spinner_speed(0.05)).fixed_decimals(app.state.prefs.spinner_precision));
                        if r.drag_started() {
                            app.state.undo.begin(pname);
                        }
                        if r.changed() && (v - f).abs() > 1e-9 {
                            let _ = app.dispatch(Command::SetModifierParameter { index: index as u32, name: pname.into(), value: v as f64 });
                        }
                        if r.drag_stopped() {
                            app.state.undo.end();
                        }
                    }
                    loom_scene::ParamValue::Int(i) if modifier.choices(pname).is_some() => {
                        let choices = modifier.choices(pname).unwrap();
                        ui.horizontal_wrapped(|ui| {
                            for (ci, c) in choices.iter().enumerate() {
                                if ui.radio(i as usize == ci, *c).clicked() && i as usize != ci {
                                    let _ = app.dispatch(Command::SetModifierParameter { index: index as u32, name: pname.into(), value: ci as f64 });
                                }
                            }
                        });
                    }
                    loom_scene::ParamValue::Int(i) => {
                        let mut v = i;
                        let r = ui.add(crate::spinner::spin(ui, &mut v, ..).speed(app.spinner_speed(0.1)));
                        if r.drag_started() {
                            app.state.undo.begin(pname);
                        }
                        if r.changed() && v != i {
                            let _ = app.dispatch(Command::SetModifierParameter { index: index as u32, name: pname.into(), value: v as f64 });
                        }
                        if r.drag_stopped() {
                            app.state.undo.end();
                        }
                    }
                    loom_scene::ParamValue::Bool(b) => {
                        let mut v = b;
                        if ui.checkbox(&mut v, "").changed() {
                            let _ = app.dispatch(Command::SetModifierParameter { index: index as u32, name: pname.into(), value: if v { 1.0 } else { 0.0 } });
                        }
                    }
                }
                ui.end_row();
            }
        });
    });
}

/// The stack's base-object Convert To entries for this object.
pub(crate) fn base_convert_entries(object: &loom_scene::ObjectKind) -> Vec<(&'static str, Command)> {
    let mut v = Vec::new();
    if object.poly().is_none() {
        v.push(("Editable Poly", Command::ConvertToEditablePoly));
    }
    if object.shape().is_some() && object.spline().is_none() {
        v.push(("Editable Spline", Command::ConvertToEditableSpline));
    }
    v
}

#[cfg(test)]
mod convert_tests {
    use super::*;

    #[test]
    fn a_primitive_converts_to_editable_poly_from_the_stack() {
        let mut s = loom_commands::AppState::new();
        loom_commands::execute(&mut s, Command::CreatePrimitive { kind: loom_commands::PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
        let id = *s.selection.iter().next().unwrap();
        let entries = base_convert_entries(&s.scene.node(id).unwrap().object);
        assert_eq!(entries.iter().map(|(l, _)| *l).collect::<Vec<_>>(), ["Editable Poly"]);
        loom_commands::execute(&mut s, entries[0].1.clone()).unwrap();
        assert!(s.scene.node(id).unwrap().object.poly().is_some());
        assert!(base_convert_entries(&s.scene.node(id).unwrap().object).is_empty(), "already Editable Poly");
    }
}
