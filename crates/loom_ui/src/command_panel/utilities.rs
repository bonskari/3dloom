//! Utilities tab: Measure, Rescale World Units, Collapse, Reset XForm.

use super::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Utility {
    #[default]
    None,
    Measure,
    RescaleWorldUnits,
    Collapse,
    ResetXForm,
    AssignVertexColors,
    LinkInheritance,
    ChannelInfo,
    UvwRemove,
    BitmapPaths,
    PanoramaExporter,
}

/// A utility by its name (the button label).
pub fn utility_named(name: &str) -> Utility {
    match name {
        "Measure" => Utility::Measure,
        "Rescale World Units" => Utility::RescaleWorldUnits,
        "Collapse" => Utility::Collapse,
        "Reset XForm" => Utility::ResetXForm,
        "Assign Vertex Colors" => Utility::AssignVertexColors,
        "Link Inheritance (Selected)" => Utility::LinkInheritance,
        "Channel Info" => Utility::ChannelInfo,
        "UVW Remove" => Utility::UvwRemove,
        "Bitmap/Photometric Paths" => Utility::BitmapPaths,
        "Panorama Exporter" => Utility::PanoramaExporter,
        _ => Utility::None,
    }
}

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    rollout(ui, "Utilities", true, |ui| {
        let w = ui.available_width();
        ui.horizontal(|ui| {
            if ui.button("More...").clicked() {
                app.utilities_more = true;
            }
            if ui.button("Configure Button Sets").clicked() {
                app.configure_utility_buttons = Some(app.state.utility_buttons.clone());
            }
        });
        for label in app.state.utility_buttons.clone() {
            let u = utility_named(&label);
            if ui.add_sized([w, 22.0], egui::SelectableLabel::new(app.utility == u, label.as_str())).clicked() {
                app.utility = if app.utility == u { Utility::None } else { u };
            }
        }
    });
    more_dialog(ui.ctx(), app);
    configure_dialog(ui.ctx(), app);
    let has = !app.state.selection.is_empty();
    match app.utility {
        Utility::None => {}
        Utility::PanoramaExporter => rollout(ui, "Panorama Exporter", true, |ui| {
            let cams: Vec<(u64, String)> = app.state.scene.nodes().filter(|n| matches!(n.object, loom_scene::ObjectKind::Camera { .. })).map(|n| (n.id.0, n.name.clone())).collect();
            let p = &mut app.state.panorama;
            if p.size == 0 {
                p.size = 512;
            }
            if p.camera.is_none_or(|c| !cams.iter().any(|x| x.0 == c)) {
                p.camera = cams.first().map(|c| c.0);
            }
            ui.horizontal(|ui| {
                ui.label("Camera:");
                let cur = p.camera.and_then(|c| cams.iter().find(|x| x.0 == c)).map_or("(no camera)".to_owned(), |x| x.1.clone());
                egui::ComboBox::from_id_salt("panorama_camera").selected_text(cur).show_ui(ui, |ui| {
                    for (id, name) in &cams {
                        if ui.selectable_label(p.camera == Some(*id), name).clicked() {
                            p.camera = Some(*id);
                        }
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.label("Face Size:");
                ui.add(egui::DragValue::new(&mut p.size).range(64..=4096));
            });
            ui.horizontal(|ui| {
                ui.label("Output:");
                ui.text_edit_singleline(&mut p.output);
            });
            let (camera, size, output) = (p.camera, p.size, p.output.clone());
            ui.horizontal(|ui| {
                if ui.add_enabled(camera.is_some(), egui::Button::new("Render...")).clicked() {
                    let r = app.dispatch(Command::PanoramaExport { camera: camera.unwrap_or(0), size: Some(size), output: Some(output) });
                    if let Err(e) = r {
                        app.last_error = Some(format!("Panorama Exporter: {e}"));
                    }
                }
                if ui.add_enabled(app.state.panorama.image.is_some(), egui::Button::new("Viewer...")).clicked() {
                    let _ = app.dispatch(Command::PanoramaViewer { path: None, yaw: None, pitch: None, fov: None });
                }
            });
        }),
        Utility::Measure => rollout(ui, "Measure", true, |ui| {
            let r = measured(app);
            ui.label(r["name"].as_str().unwrap_or("").to_owned());
            let mut lock = app.measure_lock.is_some();
            if ui.checkbox(&mut lock, "Lock Selected").changed() {
                app.measure_lock = lock.then(|| app.state.selection.iter().map(|n| n.0).collect());
            }
            measure_groups(ui, &r);
            ui.add_enabled_ui(!r["center_of_mass"].is_null() && app.measure_lock.is_none(), |ui| {
                if ui.button("Create Center Point").clicked() {
                    let _ = app.dispatch(Command::MeasureCreateCenterPoint);
                }
            });
            ui.horizontal(|ui| {
                if ui.button("New Floater").clicked() {
                    app.measure_floater = true;
                }
                if ui.button("Close").clicked() {
                    app.utility = Utility::None;
                }
            });
        }),
        Utility::RescaleWorldUnits => rollout(ui, "Rescale World Units", true, |ui| {
            ui.horizontal(|ui| {
                ui.label("Scale Factor:");
                ui.add(crate::spinner::spin(ui, &mut app.rescale_factor, 0.0001..=10_000.0).speed(0.01));
            });
            if ui.button("Rescale").clicked() {
                let _ = app.dispatch(Command::RescaleWorldUnits { factor: app.rescale_factor });
            }
        }),
        Utility::Collapse => rollout(ui, "Collapse", true, |ui| {
            ui.add_enabled_ui(has, |ui| {
                if ui.button("Collapse Selected").clicked() {
                    let _ = app.dispatch(Command::ConvertToEditablePoly);
                }
            });
        }),
        Utility::AssignVertexColors => rollout(ui, "Assign Vertex Colors", true, |ui| assign_vertex_colors(ui, app, false)),
        Utility::LinkInheritance => rollout(ui, "Link Inheritance (Selected)", true, |ui| link_inheritance(ui, app)),
        Utility::ChannelInfo => rollout(ui, "Channel Info", true, |ui| channel_info(ui, app)),
        Utility::UvwRemove => rollout(ui, "UVW Remove", true, |ui| uvw_remove(ui, app)),
        Utility::BitmapPaths => rollout(ui, "Path Editor", true, |ui| {
            if ui.button("Edit Resources...").clicked() && !app.state.bitmap_path_editor_open {
                let _ = app.dispatch(Command::BitmapPathEditor);
            }
        }),
        Utility::ResetXForm => rollout(ui, "Reset Transform", true, |ui| {
            ui.add_enabled_ui(has, |ui| {
                if ui.button("Reset Selected").clicked() {
                    let _ = app.dispatch(Command::ResetXForm);
                }
            });
        }),
    }
}

/// What Measure shows: the locked nodes, or the selection.
fn measured(app: &mut LoomApp) -> serde_json::Value {
    let r = match app.measure_lock.clone() {
        Some(ids) => app.dispatch(Command::MeasureNodes { ids }),
        None => app.dispatch(Command::Measure),
    };
    r.unwrap_or_default()
}

/// The Objects, Shapes and Dimensions groups of Measure (panel and floater).
fn measure_groups(ui: &mut egui::Ui, r: &serde_json::Value) {
    let f = |v: &serde_json::Value| v.as_f64().unwrap_or(0.0);
    let xyz = |v: &serde_json::Value| format!("X: {:.3}  Y: {:.3}  Z: {:.3}", f(&v[0]), f(&v[1]), f(&v[2]));
    ui.group(|ui| {
        ui.label("Objects");
        ui.label(format!("Surface Area: {:.3}", f(&r["surface_area"])));
        // An asterisk: the mesh has holes, so the volume is not exact.
        let star = if r["volume_open"].as_bool().unwrap_or(false) { "*" } else { "" };
        ui.label(format!("Volume: {:.3}{star}", f(&r["volume"])));
        ui.label("Center of Mass:");
        ui.label(if r["center_of_mass"].is_null() { "-".to_owned() } else { xyz(&r["center_of_mass"]) });
    });
    ui.group(|ui| {
        ui.label("Shapes");
        ui.label(format!("Length: {:.3}", f(&r["shape_length"])));
    });
    ui.group(|ui| {
        ui.label("Dimensions");
        ui.label(if r["dimensions"].is_null() { "-".to_owned() } else { xyz(&r["dimensions"]) });
    });
}

/// Measure > New Floater: a modeless Measure window with the same groups.
pub fn measure_floater(ctx: &egui::Context, app: &mut LoomApp) {
    if !app.measure_floater {
        return;
    }
    let r = measured(app);
    let mut open = true;
    egui::Window::new("Measure").open(&mut open).resizable(true).default_width(240.0).show(ctx, |ui| {
        ui.label(r["name"].as_str().unwrap_or("").to_owned());
        measure_groups(ui, &r);
    });
    app.measure_floater = open;
}

/// Assign Vertex Colors: Light Model, Color Assignment and Shadows, then
/// Assign To Selected (the utility) or Assign (a VertexPaint entry's rollout).
pub fn assign_vertex_colors(ui: &mut egui::Ui, app: &mut LoomApp, update: bool) {
    let id = egui::Id::new("assign_vertex_colors");
    let (mut model, mut by_vertex, mut shadows): (u32, bool, bool) = ui.ctx().data(|d| d.get_temp(id)).unwrap_or((0, false, false));
    ui.label("Light Model:");
    for (i, l) in loom_commands::assign_vertex_colors::LIGHT_MODELS.iter().enumerate() {
        ui.radio_value(&mut model, i as u32, *l);
    }
    ui.label("Color Assignment:");
    ui.radio_value(&mut by_vertex, false, "Color by Face");
    ui.radio_value(&mut by_vertex, true, "Color by Vertex");
    // Lighting Only turns Shadows off, as in the reference.
    ui.add_enabled(model != 1, egui::Checkbox::new(&mut shadows, "Shadows"));
    ui.ctx().data_mut(|d| d.insert_temp(id, (model, by_vertex, shadows)));
    let label = if update { "Assign" } else { "Assign To Selected" };
    if ui.add_enabled(!app.state.selection.is_empty(), egui::Button::new(label)).clicked() {
        if let Err(e) = app.dispatch(Command::AssignVertexColors { light_model: model, by_vertex, shadows, update }) {
            app.state.status_prompt = e;
        }
    }
}

/// Link Inheritance (Selected): the Inherit checkboxes of the Hierarchy
/// panel for every selected object at once (showing the first one's).
fn link_inheritance(ui: &mut egui::Ui, app: &mut LoomApp) {
    let Some(first) = app.state.selection.iter().next().and_then(|id| app.state.scene.node(*id)).map(|n| n.inherit) else {
        ui.label("Select the objects to change.");
        return;
    };
    let mut change = None;
    for (group, current) in [("Move", first.move_axes), ("Rotate", first.rotate_axes), ("Scale", first.scale_axes)] {
        ui.label(format!("{group}:"));
        ui.horizontal(|ui| {
            let mut axes = current;
            for (i, axis) in ["X", "Y", "Z"].iter().enumerate() {
                ui.checkbox(&mut axes[i], *axis);
            }
            if axes != current {
                change = Some((group, axes));
            }
        });
    }
    if let Some((group, axes)) = change {
        let pick = |g: &str| (g == group).then_some(axes);
        let cmd = Command::LinkInheritanceSelected { move_axes: pick("Move"), rotate_axes: pick("Rotate"), scale_axes: pick("Scale") };
        if let Err(e) = app.dispatch(cmd) {
            app.state.status_prompt = e;
        }
    }
}

fn channel_info(ui: &mut egui::Ui, app: &mut LoomApp) {
    let sel_id = egui::Id::new("channel_info_selected");
    let mut picked: Option<(u64, u32)> = ui.ctx().data(|d| d.get_temp(sel_id));
    let info = app.dispatch(Command::ChannelInfo).unwrap_or_default();
    let channels = info["channels"].as_array().cloned().unwrap_or_default();
    if channels.is_empty() {
        ui.label("Select an object to inspect map channels.");
        return;
    }
    egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
        egui::Grid::new("channel_info_grid").striped(true).num_columns(5).show(ui, |ui| {
            ui.label("Object");
            ui.label("ID");
            ui.label("Channel");
            ui.label("Verts");
            ui.label("Faces");
            ui.end_row();
            for row in &channels {
                let id = row["id"].as_u64().unwrap_or(0);
                let ch = row["channel"].as_u64().map(|c| c as u32);
                let name = row["object"].as_str().unwrap_or("");
                let id_str = row["channel_id"].as_str().unwrap_or("");
                let ch_name = row["name"].as_str().unwrap_or("");
                let verts = row["num_verts"].as_u64().unwrap_or(0);
                let faces = row["num_faces"].as_u64().unwrap_or(0);
                let is_sel = picked.is_some_and(|p| Some(p) == ch.map(|c| (id, c)));
                if ui.selectable_label(is_sel, name).clicked() {
                    if let Some(c) = ch {
                        picked = Some((id, c));
                    }
                }
                ui.label(id_str);
                ui.label(ch_name);
                ui.label(verts.to_string());
                ui.label(faces.to_string());
                ui.end_row();
            }
        });
    });
    ui.ctx().data_mut(|d| {
        if let Some(p) = picked {
            d.insert_temp(sel_id, p);
        }
    });
    let first_sel = app.state.selection.iter().next().map(|n| n.0);
    ui.horizontal(|ui| {
        let can_edit = picked.is_some_and(|(_, c)| c >= 1);
        if ui.add_enabled(can_edit, egui::Button::new("Copy")).clicked() {
            if let Some((id, channel)) = picked {
                let _ = app.dispatch(Command::ChannelInfoCopy { id, channel });
            }
        }
        if ui.add_enabled(can_edit && app.state.channel_clipboard.is_some(), egui::Button::new("Paste")).clicked() {
            if let Some((id, channel)) = picked {
                let _ = app.dispatch(Command::ChannelInfoPaste { id, channel });
            }
        }
        if ui.add_enabled(can_edit, egui::Button::new("Clear")).clicked() {
            if let Some((id, channel)) = picked {
                let _ = app.dispatch(Command::ChannelInfoClear { id, channel });
            }
        }
        if ui.add_enabled(first_sel.is_some(), egui::Button::new("Add")).clicked() {
            if let Some(id) = picked.map(|(n, _)| n).or(first_sel) {
                let _ = app.dispatch(Command::ChannelInfoAdd { id });
            }
        }
    });
}

fn uvw_remove(ui: &mut egui::Ui, app: &mut LoomApp) {
    let has = !app.state.selection.is_empty();
    let grey_id = egui::Id::new("uvw_remove_set_grey");
    let mut set_gray: bool = ui.ctx().data(|d| d.get_temp(grey_id)).unwrap_or(false);
    ui.add_enabled_ui(has, |ui| {
        if ui.button("UVW").clicked() {
            let _ = app.dispatch(Command::UvwRemove { mode: "uvw".into(), set_gray });
        }
        if ui.button("Materials").clicked() {
            let _ = app.dispatch(Command::UvwRemove { mode: "materials".into(), set_gray });
        }
        ui.checkbox(&mut set_gray, "Set Gray");
    });
    ui.ctx().data_mut(|d| d.insert_temp(grey_id, set_gray));
}

/// Utilities > More...: every utility in a list; OK (or a double-click) opens the chosen one.
fn more_dialog(ctx: &egui::Context, app: &mut LoomApp) {
    if !app.utilities_more {
        return;
    }
    let id = egui::Id::new("utilities_more_choice");
    let mut choice: Option<String> = ctx.data(|d| d.get_temp(id));
    let mut open = true;
    let mut done: Option<bool> = None;
    egui::Window::new("Utilities").id(egui::Id::new("utilities_more")).open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
        for name in loom_commands::state::UTILITIES {
            let r = ui.selectable_label(choice.as_deref() == Some(name), name);
            if r.clicked() {
                choice = Some(name.to_owned());
            }
            if r.double_clicked() {
                done = Some(true);
            }
        }
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                done = Some(true);
            }
            if ui.button("Cancel").clicked() {
                done = Some(false);
            }
        });
    });
    ctx.data_mut(|d| d.insert_temp(id, choice.clone()));
    if done == Some(true) {
        if let Some(c) = &choice {
            app.utility = utility_named(c);
        }
    }
    if done.is_some() || !open {
        app.utilities_more = false;
        ctx.data_mut(|d| d.remove::<Option<String>>(id));
    }
}

/// Configure Button Sets: the utilities on the left, the panel's buttons on
/// the right; click a utility, then a button to put it there; Total Buttons
/// sets how many there are.
fn configure_dialog(ctx: &egui::Context, app: &mut LoomApp) {
    let Some(mut buttons) = app.configure_utility_buttons.take() else { return };
    let id = egui::Id::new("configure_utility_pick");
    let mut pick: Option<String> = ctx.data(|d| d.get_temp(id)).flatten();
    let mut open = true;
    let mut done: Option<bool> = None;
    egui::Window::new("Configure Button Sets").open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.label("Utilities:");
                for name in loom_commands::state::UTILITIES {
                    if ui.selectable_label(pick.as_deref() == Some(name), name).clicked() {
                        pick = Some(name.to_owned());
                    }
                }
            });
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Total Buttons:");
                    let mut n = buttons.len() as u32;
                    if ui.add(crate::spinner::spin(ui, &mut n, 1..=32)).changed() {
                        buttons.resize(n as usize, String::new());
                    }
                });
                for (k, b) in buttons.iter_mut().enumerate() {
                    let text = if b.is_empty() { format!("(button {})", k + 1) } else { b.clone() };
                    if ui.add_sized([180.0, 20.0], egui::Button::new(text)).clicked() {
                        if let Some(p) = &pick {
                            *b = p.clone();
                        }
                    }
                }
            });
        });
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                done = Some(true);
            }
            if ui.button("Cancel").clicked() {
                done = Some(false);
            }
        });
    });
    match (done, open) {
        (Some(true), _) => {
            let _ = app.dispatch(Command::SetUtilityButtons { names: buttons.into_iter().filter(|b| !b.is_empty()).collect() });
            ctx.data_mut(|d| d.remove::<Option<String>>(id));
        }
        (Some(false), _) | (None, false) => ctx.data_mut(|d| d.remove::<Option<String>>(id)),
        (None, true) => {
            ctx.data_mut(|d| d.insert_temp(id, pick));
            app.configure_utility_buttons = Some(buttons);
        }
    }
}
