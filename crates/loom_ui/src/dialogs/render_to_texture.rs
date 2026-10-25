//! Rendering > Render To Texture (0): the modeless dialog with its General
//! Settings, Objects to Bake, Output, Baked Material and Automatic Mapping
//! rollouts, the Render / Unwrap Only / Close buttons and Original / Baked,
//! plus the Projection Options dialog. Every change is a command.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;
use loom_scene::bake::{BakeObjects, ElementKind, NormalSpace};
use loom_scene::material::MAP_CHANNELS;
use serde_json::{json, Value};

const SELECTED: &str = "rtt_selected_element";
const OPTIONS_OPEN: &str = "rtt_projection_options";

fn run(app: &mut LoomApp, cmd: &str, args: Value) {
    let r = app.dispatch_json(&json!({ "cmd": cmd, "args": args }));
    if r["ok"] != true {
        app.state.status_prompt = r["error"].as_str().unwrap_or("failed").to_owned();
    }
}

fn settings(app: &mut LoomApp, args: Value) {
    run(app, "Render To Texture Settings", args);
}

fn command(app: &mut LoomApp, c: Command) {
    if let Err(e) = app.dispatch(c) {
        app.state.status_prompt = e;
    }
}

/// A spinner drag is one undo step.
fn merge_drag(app: &mut LoomApp, r: &egui::Response) {
    if r.drag_started() {
        app.state.undo.begin("Render To Texture");
    }
    if r.drag_stopped() {
        app.state.undo.end();
    }
}

fn to_u8(c: [f32; 3]) -> [u8; 3] {
    c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn to_f(c: [u8; 3]) -> [f32; 3] {
    c.map(|v| v as f32 / 255.0)
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp) {
    if !app.state.render_to_texture_open {
        return;
    }
    let mut open = true;
    egui::Window::new("Render To Texture").open(&mut open).resizable(true).default_width(440.0).default_height(620.0).show(ctx, |ui| {
        egui::ScrollArea::vertical().id_salt("rtt_scroll").max_height(ui.ctx().screen_rect().height() - 160.0).show(ui, |ui| {
            general(ui, app);
            objects(ui, app);
            output(ui, app);
            baked_material(ui, app);
            automatic_mapping(ui, app);
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Render").clicked() {
                command(app, Command::BakeTextures);
            }
            if ui.button("Unwrap Only").clicked() {
                command(app, Command::UnwrapOnly);
            }
            if ui.button("Close").clicked() {
                app.state.render_to_texture_open = false;
            }
        });
        let s = app.state.scene.render.bake.clone();
        egui::Grid::new("rtt_original_baked").num_columns(3).show(ui, |ui| {
            ui.label("Views:");
            if ui.radio(!s.views_baked, "Original").clicked() {
                settings(app, json!({ "views_baked": false }));
            }
            if ui.radio(s.views_baked, "Baked").clicked() {
                settings(app, json!({ "views_baked": true }));
            }
            ui.end_row();
            ui.label("Render:");
            if ui.radio(!s.render_baked, "Original").clicked() {
                settings(app, json!({ "render_baked": false }));
            }
            if ui.radio(s.render_baked, "Baked").clicked() {
                settings(app, json!({ "render_baked": true }));
            }
            ui.end_row();
        });
    });
    if !open {
        app.state.render_to_texture_open = false;
    }
    let id = egui::Id::new(OPTIONS_OPEN);
    let mut options_open = ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    if options_open {
        egui::Window::new("Projection Options").open(&mut options_open).resizable(false).show(ctx, |ui| projection_options(ui, app));
        ctx.data_mut(|d| d.insert_temp(id, options_open));
    }
}

fn general(ui: &mut egui::Ui, app: &mut LoomApp) {
    let s = app.state.scene.render.bake.clone();
    crate::command_panel::rollout(ui, "General Settings", true, |ui| {
        ui.label(egui::RichText::new("Output").strong());
        ui.horizontal(|ui| {
            ui.label("Path:");
            let id = egui::Id::new("rtt_output_path");
            let mut text = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| s.output_dir.clone());
            let r = ui.add(egui::TextEdit::singleline(&mut text).hint_text(loom_commands::bake::output_dir(&s).to_string_lossy()).desired_width(300.0));
            if r.lost_focus() && text != s.output_dir {
                settings(app, json!({ "output_dir": text }));
            }
            if r.has_focus() {
                ui.data_mut(|d| d.insert_temp(id, text));
            } else {
                ui.data_mut(|d| d.remove::<String>(id));
            }
        });
        ui.horizontal(|ui| {
            let mut skip = s.skip_existing;
            if ui.checkbox(&mut skip, "Skip Existing Files").changed() {
                settings(app, json!({ "skip_existing": skip }));
            }
            let mut rfw = s.rendered_frame_window;
            if ui.checkbox(&mut rfw, "Rendered Frame Window").changed() {
                settings(app, json!({ "rendered_frame_window": rfw }));
            }
        });
        ui.label(egui::RichText::new("Render Settings").strong());
        ui.horizontal(|ui| {
            ui.label("Bevy renderer, the Render Setup values");
            if ui.button("Setup...").clicked() {
                command(app, Command::RenderSetup { width: None, height: None, image_aspect: None, pixel_aspect: None, lock_image_aspect: None, lock_pixel_aspect: None, render_hidden: None, force_two_sided: None, save_file: None, output: None });
            }
        });
    });
}

fn objects(ui: &mut egui::Ui, app: &mut LoomApp) {
    let s = app.state.scene.render.bake.clone();
    let ids = loom_commands::bake::bake_objects(&app.state, &s);
    crate::command_panel::rollout(ui, "Objects to Bake", true, |ui| {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("rtt_objects").max_height(90.0).auto_shrink([false, true]).show(ui, |ui| {
                egui::Grid::new("rtt_object_list").num_columns(3).striped(true).show(ui, |ui| {
                    ui.label(egui::RichText::new("Name").strong());
                    ui.label(egui::RichText::new("Map Channel").strong());
                    ui.label(egui::RichText::new("Edge Padding").strong());
                    ui.end_row();
                    for id in &ids {
                        let Some(n) = app.state.scene.node(*id) else { continue };
                        ui.label(n.name.clone());
                        ui.label(s.channel().to_string());
                        ui.label(s.padding.to_string());
                        ui.end_row();
                    }
                });
            });
        });
        ui.label(egui::RichText::new("Selected Object Settings").strong());
        ui.horizontal(|ui| {
            let mut enable = s.enable;
            if ui.checkbox(&mut enable, "Enable").changed() {
                settings(app, json!({ "enable": enable }));
            }
            ui.label("Padding:");
            let mut padding = s.padding;
            let r = ui.add(egui::DragValue::new(&mut padding).range(0..=64));
            merge_drag(app, &r);
            if r.changed() {
                settings(app, json!({ "padding": padding }));
            }
        });
        ui.label(egui::RichText::new("Projection Mapping").strong());
        ui.horizontal(|ui| {
            let mut on = s.projection;
            if ui.checkbox(&mut on, "Enabled").changed() {
                settings(app, json!({ "projection": on }));
            }
            let modifier = ids.first().and_then(|id| app.state.scene.node(*id)).and_then(|n| n.modifiers.iter().find(|m| matches!(m.modifier, loom_scene::Modifier::Projection(_))).map(|m| m.name.clone()));
            ui.label(modifier.unwrap_or_else(|| "(no Projection modifier)".into()));
        });
        ui.horizontal(|ui| {
            let others: Vec<(u64, String)> = app.state.scene.nodes().filter(|n| loom_commands::bake::is_geometry(n) && !ids.contains(&n.id) && app.state.is_visible(n)).map(|n| (n.id.0, n.name.clone())).collect();
            ui.add_enabled_ui(!ids.is_empty(), |ui| {
                ui.menu_button("Pick...", |ui| {
                    if others.is_empty() {
                        ui.weak("(no other objects)");
                    }
                    for (id, name) in &others {
                        if ui.button(name).clicked() {
                            command(app, Command::PickProjectionTarget { target: *id });
                            ui.close_menu();
                        }
                    }
                });
            });
            if ui.button("Options...").clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(OPTIONS_OPEN), true));
            }
        });
        ui.label(egui::RichText::new("Mapping Coordinates").strong());
        ui.horizontal(|ui| {
            if ui.radio(!s.auto_unwrap, "Use Existing Channel").clicked() {
                settings(app, json!({ "auto_unwrap": false }));
            }
            if ui.radio(s.auto_unwrap, "Use Automatic Unwrap").clicked() {
                settings(app, json!({ "auto_unwrap": true }));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Channel:");
            let mut ch = s.channel();
            let r = ui.add(egui::DragValue::new(&mut ch).range(1..=99));
            merge_drag(app, &r);
            if r.changed() {
                settings(app, json!({ "channel": ch }));
            }
            if ui.button("Clear Unwrappers").clicked() {
                command(app, Command::ClearUnwrappers);
            }
        });
        ui.horizontal(|ui| {
            for o in [BakeObjects::Individual, BakeObjects::AllSelected, BakeObjects::AllPrepared] {
                if ui.radio(s.objects == o, o.label()).clicked() {
                    settings(app, json!({ "objects": o.label() }));
                }
            }
        });
    });
}

fn output(ui: &mut egui::Ui, app: &mut LoomApp) {
    let s = app.state.scene.render.bake.clone();
    let sel_id = egui::Id::new(SELECTED);
    let mut selected: usize = ui.data(|d| d.get_temp(sel_id)).unwrap_or(0);
    let first_object = loom_commands::bake::bake_objects(&app.state, &s).first().and_then(|id| app.state.scene.node(*id)).map(|n| n.name.clone()).unwrap_or_default();
    let dir = loom_commands::bake::output_dir(&s);
    crate::command_panel::rollout(ui, "Output", true, |ui| {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("rtt_elements").max_height(110.0).auto_shrink([false, true]).show(ui, |ui| {
                egui::Grid::new("rtt_element_list").num_columns(4).striped(true).show(ui, |ui| {
                    for h in ["File Name", "Element Name", "Size", "Target Map Slot"] {
                        ui.label(egui::RichText::new(h).strong());
                    }
                    ui.end_row();
                    for (i, e) in s.elements.iter().enumerate() {
                        let file = e.path(&dir, &first_object).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                        let text = if e.enabled { egui::RichText::new(file) } else { egui::RichText::new(file).weak() };
                        if ui.selectable_label(i == selected, text).clicked() {
                            selected = i;
                        }
                        ui.label(&e.name);
                        ui.label(format!("{}x{}", e.width, e.height));
                        ui.label(if e.target_slot.is_empty() { "-" } else { e.target_slot.as_str() });
                        ui.end_row();
                    }
                });
            });
        });
        ui.horizontal(|ui| {
            ui.menu_button("Add...", |ui| {
                for k in ElementKind::ALL {
                    if ui.button(k.label()).clicked() {
                        run(app, "Add Texture Element", json!({ "element": k.label() }));
                        selected = app.state.scene.render.bake.elements.len().saturating_sub(1);
                        ui.close_menu();
                    }
                }
            });
            if ui.add_enabled(selected < s.elements.len(), egui::Button::new("Delete")).clicked() {
                command(app, Command::DeleteTextureElement { index: selected as u32 });
            }
        });
        let Some(e) = s.elements.get(selected).cloned() else {
            ui.weak("Add a texture element to render.");
            return;
        };
        let set = |app: &mut LoomApp, mut args: Value| {
            args["index"] = json!(selected);
            run(app, "Texture Element Settings", args);
        };
        ui.label(egui::RichText::new("Selected Element Common Settings").strong());
        egui::Grid::new("rtt_element_common").num_columns(2).show(ui, |ui| {
            let mut on = e.enabled;
            if ui.checkbox(&mut on, "Enable").changed() {
                set(app, json!({ "enabled": on }));
            }
            ui.end_row();
            ui.label("Name:");
            let mut name = e.name.clone();
            if ui.add(egui::TextEdit::singleline(&mut name).id_salt(("rtt_name", selected))).lost_focus() && name != e.name {
                set(app, json!({ "name": name }));
            }
            ui.end_row();
            ui.label("File Name and Type:");
            let mut file = e.file.clone();
            let hint = e.path(&dir, &first_object).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
            if ui.add(egui::TextEdit::singleline(&mut file).id_salt(("rtt_file", selected)).hint_text(hint)).lost_focus() && file != e.file {
                set(app, json!({ "file": file }));
            }
            ui.end_row();
            ui.label("Type:");
            egui::ComboBox::from_id_salt("rtt_format").selected_text(e.format.clone()).show_ui(ui, |ui| {
                for f in ["tga", "png", "jpg", "bmp", "tif", "exr"] {
                    if ui.selectable_label(e.format == f, f).clicked() {
                        set(app, json!({ "format": f }));
                    }
                }
            });
            ui.end_row();
            ui.label("Target Map Slot:");
            egui::ComboBox::from_id_salt("rtt_target_slot").selected_text(if e.target_slot.is_empty() { "(none)".to_owned() } else { e.target_slot.clone() }).show_ui(ui, |ui| {
                if ui.selectable_label(e.target_slot.is_empty(), "(none)").clicked() {
                    set(app, json!({ "target_slot": "" }));
                }
                for (slot, _) in MAP_CHANNELS {
                    if ui.selectable_label(e.target_slot == slot, slot).clicked() {
                        set(app, json!({ "target_slot": slot }));
                    }
                }
            });
            ui.end_row();
            ui.label("Element Type:");
            ui.label(e.kind.label());
            ui.end_row();
            ui.label("Element Background:");
            let mut bg = to_u8(e.background);
            if ui.color_edit_button_srgb(&mut bg).changed() {
                set(app, json!({ "background": to_f(bg) }));
            }
            ui.end_row();
            ui.label("Width:");
            let mut w = e.width;
            let r = ui.add(egui::DragValue::new(&mut w).range(1..=loom_commands::bake::MAX_MAP_SIZE));
            merge_drag(app, &r);
            if r.changed() {
                set(app, json!({ "width": w }));
            }
            ui.end_row();
            ui.label("Height:");
            let mut h = e.height;
            let r = ui.add(egui::DragValue::new(&mut h).range(1..=loom_commands::bake::MAX_MAP_SIZE));
            merge_drag(app, &r);
            if r.changed() {
                set(app, json!({ "height": h }));
            }
            ui.end_row();
        });
        ui.horizontal_wrapped(|ui| {
            for n in loom_commands::bake::SIZE_PRESETS.into_iter().chain([2048]) {
                if ui.selectable_label(e.width == n && e.height == n, format!("{n}x{n}")).clicked() {
                    set(app, json!({ "width": n, "height": n }));
                }
            }
        });
        if !e.kind.components().is_empty() {
            ui.label(egui::RichText::new("Selected Element Unique Settings").strong());
            ui.horizontal_wrapped(|ui| {
                for c in e.kind.components() {
                    let mut on = e.component(c).unwrap_or(false);
                    if ui.checkbox(&mut on, *c).changed() {
                        set(app, json!({ "component": c, "on": on }));
                    }
                }
            });
        }
    });
    ui.data_mut(|d| d.insert_temp(sel_id, selected.min(app.state.scene.render.bake.elements.len().saturating_sub(1))));
}

fn baked_material(ui: &mut egui::Ui, app: &mut LoomApp) {
    let s = app.state.scene.render.bake.clone();
    crate::command_panel::rollout(ui, "Baked Material", true, |ui| {
        ui.label(egui::RichText::new("Baked Material Settings").strong());
        if ui.radio(s.output_into_source, "Output Into Source").clicked() {
            settings(app, json!({ "output_into_source": true }));
        }
        if ui.radio(!s.output_into_source, "Save Source (Create Shell)").clicked() {
            settings(app, json!({ "output_into_source": false }));
        }
        ui.indent("rtt_shell", |ui| {
            ui.add_enabled_ui(!s.output_into_source, |ui| {
                if ui.radio(!s.create_new_baked, "Duplicate Source to Baked").clicked() {
                    settings(app, json!({ "create_new_baked": false }));
                }
                if ui.radio(s.create_new_baked, "Create New Baked").clicked() {
                    settings(app, json!({ "create_new_baked": true }));
                }
                ui.add_enabled(false, egui::Label::new("Standard: Blinn"));
            });
        });
        if ui.button("Update Baked Materials").clicked() {
            command(app, Command::UpdateBakedMaterials);
        }
        ui.horizontal(|ui| {
            if ui.button("Clear Shell Materials").clicked() {
                command(app, Command::ClearShellMaterials);
            }
            if ui.radio(!s.keep_baked, "Keep Source Materials").clicked() {
                settings(app, json!({ "keep_baked": false }));
            }
            if ui.radio(s.keep_baked, "Keep Baked Materials").clicked() {
                settings(app, json!({ "keep_baked": true }));
            }
        });
        let mut files = s.render_to_files_only;
        if ui.checkbox(&mut files, "Render to Files Only").changed() {
            settings(app, json!({ "render_to_files_only": files }));
        }
    });
}

fn automatic_mapping(ui: &mut egui::Ui, app: &mut LoomApp) {
    let s = app.state.scene.render.bake.clone();
    crate::command_panel::rollout(ui, "Automatic Mapping", false, |ui| {
        ui.label(egui::RichText::new("Automatic Unwrap Mapping").strong());
        egui::Grid::new("rtt_auto_mapping").num_columns(2).show(ui, |ui| {
            let mut rotate = s.rotate_clusters;
            if ui.checkbox(&mut rotate, "Rotate Clusters").changed() {
                settings(app, json!({ "rotate_clusters": rotate }));
            }
            let mut fill = s.fill_holes;
            if ui.checkbox(&mut fill, "Fill Holes").changed() {
                settings(app, json!({ "fill_holes": fill }));
            }
            ui.end_row();
            ui.label("Threshold Angle:");
            let mut a = s.threshold_angle;
            let r = ui.add(egui::DragValue::new(&mut a).range(0.0..=180.0).speed(0.5));
            merge_drag(app, &r);
            if r.changed() {
                settings(app, json!({ "threshold_angle": a }));
            }
            ui.end_row();
            ui.label("Spacing:");
            let mut sp = s.spacing;
            let r = ui.add(egui::DragValue::new(&mut sp).range(0.0..=1.0).speed(0.001).max_decimals(3));
            merge_drag(app, &r);
            if r.changed() {
                settings(app, json!({ "spacing": sp }));
            }
            ui.end_row();
        });
    });
}

fn projection_options(ui: &mut egui::Ui, app: &mut LoomApp) {
    let o = app.state.scene.render.bake.options.clone();
    let set = |app: &mut LoomApp, args: Value| run(app, "Projection Options", args);
    ui.label(egui::RichText::new("Method").strong());
    ui.horizontal(|ui| {
        let mut cage = o.use_cage;
        if ui.checkbox(&mut cage, "Use Cage").changed() {
            set(app, json!({ "use_cage": cage }));
        }
        ui.label("Offset:");
        let mut off = o.offset;
        let r = ui.add_enabled(!o.use_cage, egui::DragValue::new(&mut off).range(0.0..=100000.0).speed(0.1));
        merge_drag(app, &r);
        if r.changed() {
            set(app, json!({ "offset": off }));
        }
    });
    ui.label(egui::RichText::new("Resolve Hit").strong());
    ui.horizontal(|ui| {
        if ui.radio(o.closest, "Closest").clicked() {
            set(app, json!({ "closest": true }));
        }
        if ui.radio(!o.closest, "Furthest").clicked() {
            set(app, json!({ "closest": false }));
        }
    });
    let mut m = o.match_material_id;
    if ui.checkbox(&mut m, "Hit Only Matching Material ID").changed() {
        set(app, json!({ "match_material_id": m }));
    }
    let mut w = o.include_working_model;
    if ui.checkbox(&mut w, "Include Working Model").changed() {
        set(app, json!({ "include_working_model": w }));
    }
    ui.horizontal(|ui| {
        let mut miss = o.ray_miss_check;
        if ui.checkbox(&mut miss, "Ray miss check").changed() {
            set(app, json!({ "ray_miss_check": miss }));
        }
        let mut c = to_u8(o.ray_miss_color);
        if ui.color_edit_button_srgb(&mut c).changed() {
            set(app, json!({ "ray_miss_color": to_f(c) }));
        }
    });
    ui.label(egui::RichText::new("Normal Map Space").strong());
    ui.horizontal(|ui| {
        for n in NormalSpace::ALL {
            if ui.radio(o.space == n, n.label()).clicked() {
                set(app, json!({ "normal_space": n.label() }));
            }
        }
    });
    let tangent = o.space == NormalSpace::Tangent;
    ui.horizontal(|ui| {
        ui.label("Orientation: Red");
        let (a, b) = if tangent { ("Left", "Right") } else { ("-X", "+X") };
        egui::ComboBox::from_id_salt("rtt_red").selected_text(if o.red_flip { a } else { b }).show_ui(ui, |ui| {
            if ui.selectable_label(o.red_flip, a).clicked() {
                set(app, json!({ "red_flip": true }));
            }
            if ui.selectable_label(!o.red_flip, b).clicked() {
                set(app, json!({ "red_flip": false }));
            }
        });
        ui.label("Green");
        // Tangent: flip = Up (Down by default); the others: flip = -Y.
        let (on, off) = if tangent { ("Up", "Down") } else { ("-Y", "+Y") };
        egui::ComboBox::from_id_salt("rtt_green").selected_text(if o.green_flip { on } else { off }).show_ui(ui, |ui| {
            if ui.selectable_label(o.green_flip, on).clicked() {
                set(app, json!({ "green_flip": true }));
            }
            if ui.selectable_label(!o.green_flip, off).clicked() {
                set(app, json!({ "green_flip": false }));
            }
        });
    });
    ui.label(egui::RichText::new("Height Map").strong());
    egui::Grid::new("rtt_height").num_columns(4).show(ui, |ui| {
        ui.label("Min Height:");
        let mut lo = o.min_height;
        let r = ui.add(egui::DragValue::new(&mut lo).speed(0.1));
        merge_drag(app, &r);
        if r.changed() {
            set(app, json!({ "min_height": lo }));
        }
        ui.label("Max Height:");
        let mut hi = o.max_height;
        let r = ui.add(egui::DragValue::new(&mut hi).speed(0.1));
        merge_drag(app, &r);
        if r.changed() {
            set(app, json!({ "max_height": hi }));
        }
        ui.end_row();
        ui.label("Buffer min Height:");
        ui.label(format!("{:.3}", o.buffer_min));
        ui.label("Buffer max Height:");
        ui.label(format!("{:.3}", o.buffer_max));
        ui.end_row();
    });
}
