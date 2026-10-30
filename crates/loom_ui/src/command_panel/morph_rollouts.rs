//! Morpher rollouts (Global Parameters, Channel Color Legend, Channel List,
//! Channel Parameters, Advanced Parameters), Skin Morph rollouts
//! (Parameters, Selection, Local Properties, Copy and Paste, Options), Skin
//! Wrap rollouts (Parameters, Advanced Parameters, Display Parameters) and
//! the Skin Wrap Patch rollout.

use super::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;
use loom_scene::{Modifier, NodeId};

pub fn morpher(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, index: usize) {
    let Some(node) = app.state.scene.node(id) else { return };
    let Some(slot) = node.modifiers.get(index) else { return };
    let Modifier::Morpher(d) = &slot.modifier else { return };
    let d = (**d).clone();
    rollout(ui, "Global Parameters", false, |ui| {
        let mut uvs = d.use_vertex_selection;
        if ui.checkbox(&mut uvs, "Use Vertex Selection").changed() {
            let _ = app.dispatch(Command::MorpherSetGlobal { name: "Use Vertex Selection".into(), value: if uvs { 1.0 } else { 0.0 } });
        }
        ui.horizontal(|ui| {
            ui.label("Spinner Min:");
            let mut mn = d.spinner_min;
            if ui.add(crate::spinner::spin(ui, &mut mn, ..).speed(1.0)).changed() {
                let _ = app.dispatch(Command::MorpherSetGlobal { name: "Spinner Min".into(), value: mn as f64 });
            }
            ui.label("Max:");
            let mut mx = d.spinner_max;
            if ui.add(crate::spinner::spin(ui, &mut mx, ..).speed(1.0)).changed() {
                let _ = app.dispatch(Command::MorpherSetGlobal { name: "Spinner Max".into(), value: mx as f64 });
            }
        });
    });
    rollout(ui, "Channel Color Legend", false, |ui| {
        for (colour, text) in [(egui::Color32::from_gray(150), "Empty channel"), (egui::Color32::from_rgb(90, 160, 255), "Channel with target"), (egui::Color32::from_rgb(220, 120, 60), "Channel without target (captured)"), (egui::Color32::from_rgb(220, 60, 60), "Invalid target (vertex count)")] {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2.0, colour);
                ui.label(text);
            });
        }
    });
    rollout(ui, "Channel List", true, |ui| {
        let page = app.state.morph_channel / 10;
        ui.horizontal(|ui| {
            if ui.button("<").clicked() && page > 0 {
                app.state.morph_channel = (page - 1) * 10;
            }
            ui.label(format!("Channels {}–{}", page * 10 + 1, page * 10 + 10));
            if ui.button(">").clicked() && page < 9 {
                app.state.morph_channel = (page + 1) * 10;
            }
        });
        for i in page * 10..page * 10 + 10 {
            let ch = d.channel_ref(i).cloned().unwrap_or_default();
            let colour = if ch.targets.is_empty() {
                egui::Color32::from_gray(150)
            } else if !ch.valid {
                egui::Color32::from_rgb(220, 60, 60)
            } else if ch.targets.iter().any(|t| t.node.is_some()) {
                egui::Color32::from_rgb(90, 160, 255)
            } else {
                egui::Color32::from_rgb(220, 120, 60)
            };
            ui.horizontal(|ui| {
                let current = app.state.morph_channel == i;
                if ui.add(egui::SelectableLabel::new(current, egui::RichText::new(format!("{:>3} {}", i + 1, ch.name)).color(colour))).clicked() {
                    app.state.morph_channel = i;
                }
                let mut v = ch.value.get();
                if ui.add(crate::spinner::spin(ui, &mut v, d.spinner_min.min(d.spinner_max)..=d.spinner_max.max(d.spinner_min)).speed(0.5).fixed_decimals(1)).changed() {
                    let _ = app.dispatch(Command::MorpherSetChannel { channel: i as u32, name: "Value".into(), value: v as f64 });
                }
            });
        }
    });
    rollout(ui, "Channel Parameters", true, |ui| {
        let i = app.state.morph_channel;
        let ch = d.channel_ref(i).cloned().unwrap_or_default();
        ui.label(format!("Channel {}: {}", i + 1, ch.name));
        let mut name = ch.name.clone();
        let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(160.0));
        if r.lost_focus() && name != ch.name {
            let _ = app.dispatch(Command::MorpherRenameChannel { channel: i as u32, name });
        }
        let candidates: Vec<(NodeId, String)> = app.state.scene.nodes().filter(|n| n.id != id && !n.object.is_helper() && !n.object.is_bone()).map(|n| (n.id, n.name.clone())).collect();
        egui::ComboBox::from_id_salt("morph_pick").selected_text("Pick Object from Scene").show_ui(ui, |ui| {
            for (nid, name) in &candidates {
                if ui.selectable_label(false, name).clicked() {
                    let _ = app.dispatch(Command::MorpherPickTarget { channel: i as u32, node: nid.0 });
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("Capture Current State").clicked() {
                let _ = app.dispatch(Command::MorpherCaptureCurrentState { channel: i as u32 });
            }
            if ui.button("Delete").clicked() {
                let _ = app.dispatch(Command::MorpherDeleteChannel { channel: i as u32 });
            }
            if ui.add_enabled(!ch.targets.is_empty(), egui::Button::new("Extract")).clicked() {
                let _ = app.dispatch(Command::MorpherExtractTarget { channel: i as u32 });
            }
        });
        let mut active = ch.active;
        if ui.checkbox(&mut active, "Channel is Active").changed() {
            let _ = app.dispatch(Command::MorpherSetChannel { channel: i as u32, name: "Active".into(), value: if active { 1.0 } else { 0.0 } });
        }
        let mut lim = ch.use_limits;
        if ui.checkbox(&mut lim, "Use Limits").changed() {
            let _ = app.dispatch(Command::MorpherSetChannel { channel: i as u32, name: "Use Limits".into(), value: if lim { 1.0 } else { 0.0 } });
        }
        ui.horizontal(|ui| {
            for (label, name, value) in [("Min:", "Min", ch.min), ("Max:", "Max", ch.max)] {
                ui.label(label);
                let mut v = value;
                if ui.add(crate::spinner::spin(ui, &mut v, ..).speed(1.0)).changed() {
                    let _ = app.dispatch(Command::MorpherSetChannel { channel: i as u32, name: name.into(), value: v as f64 });
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Tension:");
            let mut t = ch.tension;
            if ui.add(crate::spinner::spin(ui, &mut t, 0.0..=1.0).speed(0.01)).changed() {
                let _ = app.dispatch(Command::MorpherSetChannel { channel: i as u32, name: "Tension".into(), value: t as f64 });
            }
        });
        ui.label("Progressive Morph");
        for (k, t) in ch.targets.iter().enumerate() {
            ui.horizontal(|ui| {
                let name = t.node.and_then(|n| app.state.scene.node(n)).map(|n| n.name.clone()).unwrap_or_else(|| "captured".into());
                ui.label(format!("{k}: {name}"));
                let mut p = t.percent;
                if ui.add(crate::spinner::spin(ui, &mut p, 0.0..=100.0).speed(1.0).suffix(" %")).changed() {
                    let _ = app.dispatch(Command::MorpherSetProgressivePercent { channel: i as u32, target: k as u32, percent: p });
                }
            });
        }
        egui::ComboBox::from_id_salt("morph_pick_prog").selected_text("Pick Progressive Target").show_ui(ui, |ui| {
            for (nid, name) in &candidates {
                if ui.selectable_label(false, name).clicked() {
                    let _ = app.dispatch(Command::MorpherAddProgressiveTarget { channel: i as u32, node: nid.0 });
                }
            }
        });
        if ui.button("Reload").clicked() {
            let _ = app.dispatch(Command::MorpherReload);
        }
    });
    rollout(ui, "Advanced Parameters", false, |ui| {
        let bytes: usize = d.channels.iter().flat_map(|c| c.targets.iter()).map(|t| t.offsets.len() * 12).sum();
        ui.label(format!("Approximate Memory Usage: {} KB", bytes / 1024));
    });
}

fn check(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, on: bool, command: impl FnOnce(f64) -> Command) {
    let mut v = on;
    if ui.checkbox(&mut v, label).changed() {
        let _ = app.dispatch(command(if v { 1.0 } else { 0.0 }));
    }
}

/// A labelled spinner row in a two-column grid.
fn spin_row(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, value: f64, range: std::ops::RangeInclusive<f64>, speed: f64, command: impl FnOnce(f64) -> Command) {
    ui.label(label);
    let mut v = value;
    if ui.add(crate::spinner::spin(ui, &mut v, range).speed(speed)).changed() {
        let _ = app.dispatch(command(v));
    }
    ui.end_row();
}

pub fn skin_morph(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, index: usize) {
    let Some(node) = app.state.scene.node(id) else { return };
    let Some(slot) = node.modifiers.get(index) else { return };
    let Modifier::SkinMorph(d) = &slot.modifier else { return };
    let d = (**d).clone();
    let world = |n: NodeId| Some(app.state.scene.world_matrix(n));
    let weights: Vec<f32> = (0..d.morphs.len()).map(|i| d.morph_weight(i, &world)).collect();
    let current = app.state.skin_morph_current.filter(|m| *m < d.morphs.len());
    let current_bone = current.and_then(|m| d.bones.get(d.morphs[m].bone)).map(|b| b.node).or(app.state.skin_morph_bone.filter(|b| d.bones.iter().any(|x| x.node == *b)));
    let set = |name: &str| {
        let name = name.to_string();
        move |v: f64| Command::SkinMorphSetOption { name, value: v }
    };
    rollout(ui, "Parameters", true, |ui| {
        // The list: each bone with its morphs (+ / - folds them), each morph with its influence now.
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            if d.bones.is_empty() {
                ui.weak("(no bones)");
            }
            for (bi, b) in d.bones.iter().enumerate() {
                let open_id = egui::Id::new(("skin_morph_open", id.0, b.node.0));
                let open: bool = ui.ctx().data(|m| m.get_temp(open_id)).unwrap_or(true);
                let name = app.state.scene.node(b.node).map(|n| n.name.clone()).unwrap_or_default();
                ui.horizontal(|ui| {
                    let has = d.morphs.iter().any(|m| m.bone == bi);
                    if ui.add_enabled(has, egui::Button::new(if open { "-" } else { "+" }).small()).clicked() {
                        ui.ctx().data_mut(|m| m.insert_temp(open_id, !open));
                    }
                    if ui.selectable_label(current.is_none() && current_bone == Some(b.node), &name).clicked() {
                        let _ = app.dispatch(Command::SkinMorphSelect { bone: Some(b.node.0), morph: None });
                    }
                });
                if open {
                    for (mi, m) in d.morphs.iter().enumerate().filter(|(_, m)| m.bone == bi) {
                        ui.horizontal(|ui| {
                            ui.add_space(22.0);
                            let text = if m.enabled { format!("{} ({:.1})", m.name, weights[mi] * 100.0) } else { format!("{} (Disabled)", m.name) };
                            if ui.selectable_label(current == Some(mi), text).clicked() {
                                let _ = app.dispatch(Command::SkinMorphSelect { bone: None, morph: Some(mi as u32) });
                            }
                        });
                    }
                }
            }
        });
        let bones: Vec<(NodeId, String)> = app.state.scene.nodes().filter(|n| n.object.is_bone() && !d.bones.iter().any(|b| b.node == n.id)).map(|n| (n.id, n.name.clone())).collect();
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("skin_morph_add_bone").selected_text("Add Bone").show_ui(ui, |ui| {
                for (bid, name) in &bones {
                    if ui.selectable_label(false, name).clicked() {
                        let _ = app.dispatch(Command::SkinMorphAddBones { ids: vec![bid.0] });
                    }
                }
            });
            let picking = app.state.pick_mode == Some(loom_commands::PickMode::SkinMorphBone);
            if ui.selectable_label(picking, "Pick Bone").on_hover_text("Click bones in the viewports; right-click ends").clicked() {
                let _ = app.dispatch(Command::SkinMorphPickBone);
            }
            if ui.add_enabled(current_bone.is_some(), egui::Button::new("Remove Bone")).clicked() {
                if let Some(b) = current_bone {
                    let _ = app.dispatch(Command::SkinMorphRemoveBone { id: b.0 });
                }
            }
        });
    });
    rollout(ui, "Selection", false, |ui| {
        check(ui, app, "Use Soft Selection", d.soft.on, set("Use Soft Selection"));
        egui::Grid::new("skin_morph_soft").num_columns(2).show(ui, |ui| {
            spin_row(ui, app, "Radius:", d.soft.radius as f64, 0.0..=1e6, 0.1, set("Radius"));
        });
        ui.horizontal(|ui| {
            check(ui, app, "Edge Limit", d.soft.edge_limit_on, set("Use Edge Limit"));
            let mut n = d.soft.edge_limit;
            if ui.add_enabled(d.soft.edge_limit_on, crate::spinner::spin(ui, &mut n, 0..=1000).speed(0.2)).changed() {
                let _ = app.dispatch(Command::SkinMorphSetOption { name: "Edge Limit".into(), value: n as f64 });
            }
        });
        if ui.button("Reset Graph").clicked() {
            let _ = app.dispatch(Command::SkinMorphResetGraph);
        }
        if let Some(points) = crate::curve_graph::show(ui, "skin_morph_soft", &d.soft.curve) {
            let _ = app.dispatch(Command::SkinMorphSetCurve { graph: "Soft Selection".into(), morph: None, points });
        }
        ui.horizontal(|ui| {
            for tool in ["Ring", "Loop", "Shrink", "Grow"] {
                if ui.button(tool).clicked() {
                    let _ = app.dispatch(Command::SkinMorphSelectionTool { tool: tool.into() });
                }
            }
        });
    });
    rollout(ui, "Local Properties", true, |ui| {
        let m = current.map(|i| (i as u32, d.morphs[i].clone()));
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(current_bone.is_some(), egui::Button::new("Create Morph")).clicked() {
                if let Some(b) = current_bone {
                    let _ = app.dispatch(Command::SkinMorphCreateMorph { bone: b.0 });
                }
            }
            if ui.add_enabled(m.is_some(), egui::Button::new("Delete Morph")).clicked() {
                if let Some((i, _)) = &m {
                    let _ = app.dispatch(Command::SkinMorphDeleteMorph { morph: *i });
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            let editing = m.as_ref().is_some_and(|(i, _)| d.editing == Some(*i as usize));
            if ui.add_enabled(m.is_some(), egui::Button::new("Edit").selected(editing)).clicked() {
                if let Some((i, _)) = &m {
                    let _ = app.dispatch(Command::SkinMorphEdit { morph: *i });
                }
            }
            for (label, clear) in [("Clear Verts", true), ("Remove Verts", false)] {
                if ui.add_enabled(m.is_some(), egui::Button::new(label)).clicked() {
                    if let Some((i, _)) = &m {
                        let _ = app.dispatch(if clear { Command::SkinMorphClearVerts { morph: *i } } else { Command::SkinMorphRemoveVerts { morph: *i } });
                    }
                }
            }
            if ui.add_enabled(m.is_some(), egui::Button::new("Reset Orient")).on_hover_text("The morph's pose becomes the bone's current orientation").clicked() {
                if let Some((i, _)) = &m {
                    let _ = app.dispatch(Command::SkinMorphReload { morph: *i });
                }
            }
        });
        let Some((cur, m)) = m else { return };
        let mut on = m.enabled;
        if ui.checkbox(&mut on, "Enabled").changed() {
            let _ = app.dispatch(Command::SkinMorphSetMorph { morph: cur, name: "Enabled".into(), value: if on { 1.0 } else { 0.0 } });
        }
        ui.horizontal(|ui| {
            ui.label("Morph Name:");
            let mut name = m.name.clone();
            let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(120.0));
            if r.lost_focus() && name != m.name {
                let _ = app.dispatch(Command::SkinMorphRenameMorph { morph: cur, name });
            }
        });
        egui::Grid::new("skin_morph_props").num_columns(2).show(ui, |ui| {
            spin_row(ui, app, "Influence Angle:", m.influence_deg as f64, 0.1..=360.0, 0.5, |v| Command::SkinMorphSetMorph { morph: cur, name: "Influence Angle".into(), value: v });
        });
        let graph_id = egui::Id::new(("skin_morph_falloff_graph", id.0, cur));
        let mut graph: bool = ui.ctx().data(|x| x.get_temp(graph_id)).unwrap_or(false);
        ui.horizontal(|ui| {
            ui.label("Falloff:");
            let names = ["Linear", "Sinual", "Fast", "Slow", "Custom Falloff"];
            egui::ComboBox::from_id_salt("skin_morph_falloff").selected_text(names[(m.falloff as usize).min(4)]).show_ui(ui, |ui| {
                for (i, l) in names.iter().enumerate() {
                    if ui.selectable_label(m.falloff == i as u32, *l).clicked() {
                        let _ = app.dispatch(Command::SkinMorphSetMorph { morph: cur, name: "Falloff".into(), value: i as f64 });
                    }
                }
            });
            if ui.add_enabled(m.falloff == loom_scene::morph::FALLOFF_CUSTOM, egui::Button::new("G").selected(graph)).on_hover_text("Edit the Custom Falloff graph").clicked() {
                graph = !graph;
            }
        });
        ui.ctx().data_mut(|x| x.insert_temp(graph_id, graph));
        if graph && m.falloff == loom_scene::morph::FALLOFF_CUSTOM {
            if let Some(points) = crate::curve_graph::show(ui, "skin_morph_falloff", &m.curve) {
                let _ = app.dispatch(Command::SkinMorphSetCurve { graph: "Falloff".into(), morph: Some(cur), points });
            }
        }
        if let Some(b) = d.bones.get(m.bone) {
            ui.label("Joint Type:");
            ui.horizontal(|ui| {
                for (label, planar) in [("Ball Joint", false), ("Planar Joint", true)] {
                    if ui.radio(b.planar == planar, label).clicked() && b.planar != planar {
                        let _ = app.dispatch(Command::SkinMorphSetJointType { bone: b.node.0, planar });
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            ui.label("External Mesh:");
            let label = m.external.and_then(|n| app.state.scene.node(n)).map(|n| n.name.clone()).unwrap_or_else(|| "-none-".into());
            let picking = app.state.pick_mode == Some(loom_commands::PickMode::SkinMorphExternal);
            if ui.selectable_label(picking, label).on_hover_text("Click, then the target object").clicked() {
                let _ = app.dispatch(Command::SkinMorphPickExternal);
            }
        });
        if ui.add_enabled(m.external.is_some(), egui::Button::new("Reload Target")).clicked() {
            let _ = app.dispatch(Command::SkinMorphReloadTarget { morph: cur });
        }
        let mut only = m.reload_selected;
        if ui.checkbox(&mut only, "Reload only selected verts").changed() {
            let _ = app.dispatch(Command::SkinMorphSetMorph { morph: cur, name: "Reload Only Selected Verts".into(), value: if only { 1.0 } else { 0.0 } });
        }
        ui.weak(format!("{} vertices in the morph", m.deltas.len()));
    });
    rollout(ui, "Copy and Paste", false, |ui| {
        if ui.add_enabled(current_bone.is_some(), egui::Button::new("Paste Mirror")).clicked() {
            if let Some(b) = current_bone {
                let _ = app.dispatch(Command::SkinMorphPasteMirror { bone: b.0 });
            }
        }
        check(ui, app, "Show Mirror Plane", d.mirror.show_plane, set("Show Mirror Plane"));
        check(ui, app, "Preview Bone", d.mirror.preview_bone, set("Preview Bone"));
        check(ui, app, "Preview Vertices", d.mirror.preview_vertices, set("Preview Vertices"));
        ui.horizontal(|ui| {
            ui.label("Mirror Plane:");
            egui::ComboBox::from_id_salt("skin_morph_mirror_plane").selected_text(["X", "Y", "Z"][d.mirror.axis.min(2) as usize]).show_ui(ui, |ui| {
                for (i, a) in ["X", "Y", "Z"].iter().enumerate() {
                    if ui.selectable_label(d.mirror.axis == i as u32, *a).clicked() {
                        let _ = app.dispatch(Command::SkinMorphSetOption { name: "Mirror Plane".into(), value: i as f64 });
                    }
                }
            });
        });
        egui::Grid::new("skin_morph_mirror").num_columns(2).show(ui, |ui| {
            spin_row(ui, app, "Mirror Offset:", d.mirror.offset as f64, -1e6..=1e6, 0.1, set("Mirror Offset"));
            spin_row(ui, app, "Mirror Threshold:", d.mirror.threshold as f64, 0.0..=1e6, 0.01, set("Mirror Threshold"));
        });
    });
    rollout(ui, "Options", false, |ui| {
        check(ui, app, "Beginner Mode", d.options.beginner_mode, set("Beginner Mode"));
        check(ui, app, "Show Driver Bone Matrix", d.options.show_driver_matrix, set("Show Driver Bone Matrix"));
        check(ui, app, "Show Morph Bone Matrix", d.options.show_morph_matrix, set("Show Morph Bone Matrix"));
        check(ui, app, "Show Current Angle", d.options.show_current_angle, set("Show Current Angle"));
        check(ui, app, "Show Edges", d.options.show_edges, set("Show Edges"));
        egui::Grid::new("skin_morph_options").num_columns(2).show(ui, |ui| {
            spin_row(ui, app, "Matrix Size:", d.options.matrix_size as f64, 0.0..=1e5, 0.1, set("Matrix Size"));
            spin_row(ui, app, "Bone Size:", d.options.bone_size as f64, 0.0..=100.0, 0.1, set("Bone Size"));
        });
    });
}

/// A local parameter row: label, command name, getter.
type LocalField = (&'static str, &'static str, fn(&loom_scene::morph::CvLocal) -> f32);

pub fn skin_wrap(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, index: usize) {
    let Some(node) = app.state.scene.node(id) else { return };
    let Some(slot) = node.modifiers.get(index) else { return };
    let Modifier::SkinWrap(d) = &slot.modifier else { return };
    let cv_level = slot.sub_level == 1 && node.active_entry == Some(index);
    let d = (**d).clone();
    let set = |name: &str| {
        let name = name.to_string();
        move |v: f64| Command::SkinWrapSetOption { name, value: v }
    };
    let speed = app.spinner_speed(0.1);
    rollout(ui, "Parameters", true, |ui| {
        let list_id = egui::Id::new(("skin_wrap_list", id.0));
        let mut highlighted: Option<NodeId> = ui.ctx().data(|m| m.get_temp(list_id)).filter(|n| d.bases.contains(n));
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            if d.bases.is_empty() {
                ui.weak("(no control objects)");
            }
            for b in &d.bases {
                let name = app.state.scene.node(*b).map(|n| n.name.clone()).unwrap_or_default();
                if ui.selectable_label(highlighted == Some(*b), name).clicked() {
                    highlighted = Some(*b);
                }
            }
        });
        ui.ctx().data_mut(|m| match highlighted {
            Some(h) => m.insert_temp(list_id, h),
            None => m.remove::<NodeId>(list_id),
        });
        ui.horizontal(|ui| {
            let picking = app.state.pick_mode == Some(loom_commands::PickMode::SkinWrapControl);
            if ui.selectable_label(picking, "Add").on_hover_text("Click control objects in the viewports; right-click ends").clicked() {
                let _ = app.dispatch(Command::SkinWrapPickControl);
            }
            if ui.add_enabled(highlighted.is_some(), egui::Button::new("Remove")).clicked() {
                if let Some(b) = highlighted {
                    let _ = app.dispatch(Command::SkinWrapRemoveBase { id: b.0 });
                }
            }
        });
        ui.label("Deformation Engine:");
        for (label, engine) in [("Vertex Deformation", 1u32), ("Face Deformation", 0)] {
            if ui.radio(d.engine == engine, label).clicked() && d.engine != engine {
                let _ = app.dispatch(Command::SkinWrapSetOption { name: "Deformation Engine".into(), value: engine as f64 });
            }
        }
        let vertex = d.engine == 1;
        egui::Grid::new("skin_wrap_opts").num_columns(2).show(ui, |ui| {
            spin_row(ui, app, "Falloff:", d.falloff as f64, 0.001..=10.0, speed, set("Falloff"));
            ui.add_enabled_ui(vertex, |ui| ui.label("Distance Infl.:"));
            let mut v = d.distance_influence as f64;
            if ui.add_enabled(vertex, crate::spinner::spin(ui, &mut v, 0.001..=10.0).speed(speed)).changed() {
                let _ = app.dispatch(Command::SkinWrapSetOption { name: "Distance Influence".into(), value: v });
            }
            ui.end_row();
            ui.add_enabled_ui(vertex, |ui| ui.label("Face Limit:"));
            let mut f = d.face_limit;
            if ui.add_enabled(vertex, crate::spinner::spin(ui, &mut f, 0..=30).speed(0.2)).changed() {
                let _ = app.dispatch(Command::SkinWrapSetOption { name: "Face Limit".into(), value: f as f64 });
            }
            ui.end_row();
        });
        check(ui, app, "Blend To Base Mesh", d.blend_to_base, set("Blend To Base Mesh"));
        egui::Grid::new("skin_wrap_blend").num_columns(2).show(ui, |ui| {
            ui.add_enabled_ui(d.blend_to_base, |ui| ui.label("Blend Distance:"));
            let mut v = d.blend_distance as f64;
            if ui.add_enabled(d.blend_to_base, crate::spinner::spin(ui, &mut v, 0.001..=1e6).speed(speed)).changed() {
                let _ = app.dispatch(Command::SkinWrapSetOption { name: "Blend Distance".into(), value: v });
            }
            ui.end_row();
        });
        ui.separator();
        // Local parameters: the selected control vertices' (blank where they differ).
        let locals: Vec<loom_scene::morph::CvLocal> = d.selection.iter().map(|(b, v)| d.local(*b, *v)).collect();
        let enabled = cv_level && !locals.is_empty();
        egui::Grid::new("skin_wrap_locals").num_columns(2).show(ui, |ui| {
            let fields: [LocalField; 5] = [("Local Str:", "Local Strength", |l| l.strength), ("Local Scale Mult:", "Local Scale Mult", |l| l.scale), ("Local X:", "Local X", |l| l.x), ("Local Y:", "Local Y", |l| l.y), ("Local Z:", "Local Z", |l| l.z)];
            for (label, name, get) in fields {
                let first = locals.first().map(get).unwrap_or(1.0);
                let same = locals.iter().all(|l| (get(l) - first).abs() < 1e-6);
                ui.add_enabled_ui(enabled, |ui| ui.label(label));
                let mut v = first as f64;
                let range = if name == "Local Strength" { -100.0..=100.0 } else { 0.001..=100.0 };
                let mut w = crate::spinner::spin(ui, &mut v, range).speed(0.01);
                if !same || !enabled {
                    w = w.custom_formatter(|_, _| String::new());
                }
                if ui.add_enabled(enabled, w).changed() {
                    let _ = app.dispatch(Command::SkinWrapSetLocal { name: name.into(), value: v });
                }
                ui.end_row();
            }
        });
        ui.separator();
        if ui.button("Reset").on_hover_text("Local values back to 1.0; rebind at the current pose").clicked() {
            let _ = app.dispatch(Command::SkinWrapReset);
        }
        egui::Grid::new("skin_wrap_threshold").num_columns(2).show(ui, |ui| {
            spin_row(ui, app, "Threshold:", d.threshold as f64, 0.0..=1e6, speed, set("Threshold"));
        });
        check(ui, app, "Weight All Points", d.weight_all_points, set("Weight All Points"));
        if ui.add_enabled(!d.bases.is_empty(), egui::Button::new("Convert To Skin")).clicked() {
            let _ = app.dispatch(Command::SkinWrapConvertToSkin);
        }
        ui.weak(format!("{} of {} points weighted", d.binds.iter().filter(|b| !b.is_empty()).count(), d.binds.len()));
    });
    rollout(ui, "Advanced Parameters", false, |ui| {
        check(ui, app, "Show Mirror Data", d.mirror.show, set("Show Mirror Data"));
        ui.horizontal(|ui| {
            ui.label("Mirror Plane:");
            egui::ComboBox::from_id_salt("skin_wrap_mirror_plane").selected_text(["X", "Y", "Z"][d.mirror.axis.min(2) as usize]).show_ui(ui, |ui| {
                for (i, a) in ["X", "Y", "Z"].iter().enumerate() {
                    if ui.selectable_label(d.mirror.axis == i as u32, *a).clicked() {
                        let _ = app.dispatch(Command::SkinWrapSetOption { name: "Mirror Plane".into(), value: i as f64 });
                    }
                }
            });
        });
        egui::Grid::new("skin_wrap_mirror").num_columns(2).show(ui, |ui| {
            spin_row(ui, app, "Mirror Offset:", d.mirror.offset as f64, -1e6..=1e6, speed, set("Mirror Offset"));
            spin_row(ui, app, "Mirror Threshold:", d.mirror.threshold as f64, 0.0..=1e6, 0.01, set("Mirror Threshold"));
        });
        if ui.add_enabled(cv_level && !d.selection.is_empty(), egui::Button::new("Mirror Selected")).clicked() {
            let _ = app.dispatch(Command::SkinWrapMirrorSelected);
        }
        if ui.add_enabled(!d.bases.is_empty(), egui::Button::new("Bake Control Verts")).clicked() {
            let _ = app.dispatch(Command::SkinWrapBakeControlVerts);
        }
        if ui.add_enabled(!d.bases.is_empty(), egui::Button::new("Retrieve Control Verts")).clicked() {
            let _ = app.dispatch(Command::SkinWrapRetrieveControlVerts);
        }
    });
    rollout(ui, "Display Parameters", false, |ui| {
        check(ui, app, "Display Loops", d.display.loops, set("Display Loops"));
        check(ui, app, "Display Axis", d.display.axis, set("Display Axis"));
        check(ui, app, "Display Face Limit", d.display.face_limit, set("Display Face Limit"));
        check(ui, app, "Display Unassigned Points", d.display.unassigned, set("Display Unassigned Points"));
        check(ui, app, "Display Control Verts", d.display.control_verts, set("Display Control Verts"));
    });
}

pub fn skin_wrap_patch(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, index: usize) {
    let Some(node) = app.state.scene.node(id) else { return };
    let Some(slot) = node.modifiers.get(index) else { return };
    let Modifier::SkinWrapPatch(d) = &slot.modifier else { return };
    let d = (**d).clone();
    rollout(ui, "Parameters", true, |ui| {
        let label = d.patch.and_then(|p| app.state.scene.node(p)).map(|n| n.name.clone()).unwrap_or_else(|| "None".into());
        let picking = app.state.pick_mode == Some(loom_commands::PickMode::ModifierNode);
        if ui.selectable_label(picking, label).on_hover_text("Pick Patch: click, then the patch object").clicked() {
            app.state.pick_mode = if picking { None } else { Some(loom_commands::PickMode::ModifierNode) };
        }
        egui::Grid::new("skin_wrap_patch").num_columns(2).show(ui, |ui| {
            ui.label("Sample Rate:");
            let mut r = d.sample_rate;
            if ui.add(crate::spinner::spin(ui, &mut r, 1..=100).speed(0.2)).changed() {
                let _ = app.dispatch(Command::SkinWrapPatchSetSampleRate { rate: r });
            }
            ui.end_row();
        });
        if ui.add_enabled(d.patch.is_some(), egui::Button::new("Resample")).clicked() {
            let _ = app.dispatch(Command::SkinWrapPatchResample);
        }
    });
}
