//! Morpher rollouts (Global Parameters, Channel Color Legend, Channel List,
//! Channel Parameters, Advanced Parameters) and Skin Morph rollouts
//! (Parameters, Local Properties).

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
            if ui.add(egui::DragValue::new(&mut mn).speed(1.0)).changed() {
                let _ = app.dispatch(Command::MorpherSetGlobal { name: "Spinner Min".into(), value: mn as f64 });
            }
            ui.label("Max:");
            let mut mx = d.spinner_max;
            if ui.add(egui::DragValue::new(&mut mx).speed(1.0)).changed() {
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
                if ui.add(egui::DragValue::new(&mut v).speed(0.5).range(d.spinner_min.min(d.spinner_max)..=d.spinner_max.max(d.spinner_min)).fixed_decimals(1)).changed() {
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
                if ui.add(egui::DragValue::new(&mut v).speed(1.0)).changed() {
                    let _ = app.dispatch(Command::MorpherSetChannel { channel: i as u32, name: name.into(), value: v as f64 });
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Tension:");
            let mut t = ch.tension;
            if ui.add(egui::DragValue::new(&mut t).speed(0.01).range(0.0..=1.0)).changed() {
                let _ = app.dispatch(Command::MorpherSetChannel { channel: i as u32, name: "Tension".into(), value: t as f64 });
            }
        });
        ui.label("Progressive Morph");
        for (k, t) in ch.targets.iter().enumerate() {
            ui.horizontal(|ui| {
                let name = t.node.and_then(|n| app.state.scene.node(n)).map(|n| n.name.clone()).unwrap_or_else(|| "captured".into());
                ui.label(format!("{k}: {name}"));
                let mut p = t.percent;
                if ui.add(egui::DragValue::new(&mut p).speed(1.0).range(0.0..=100.0).suffix(" %")).changed() {
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

pub fn skin_morph(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, index: usize) {
    let Some(node) = app.state.scene.node(id) else { return };
    let Some(slot) = node.modifiers.get(index) else { return };
    let Modifier::SkinMorph(d) = &slot.modifier else { return };
    let d = (**d).clone();
    rollout(ui, "Parameters", true, |ui| {
        ui.label("Bones:");
        let bones: Vec<(NodeId, String)> = app.state.scene.nodes().filter(|n| n.object.is_bone() && !d.bones.iter().any(|b| b.node == n.id)).map(|n| (n.id, n.name.clone())).collect();
        egui::ComboBox::from_id_salt("skin_morph_add_bone").selected_text("Add Bone").show_ui(ui, |ui| {
            for (bid, name) in &bones {
                if ui.selectable_label(false, name).clicked() {
                    let _ = app.dispatch(Command::SkinMorphAddBones { ids: vec![bid.0] });
                }
            }
        });
        let current_bone = app.state.skin_bone;
        for b in &d.bones {
            ui.horizontal(|ui| {
                let name = app.state.scene.node(b.node).map(|n| n.name.clone()).unwrap_or_default();
                if ui.selectable_label(current_bone == Some(b.node), &name).clicked() {
                    app.state.skin_bone = Some(b.node);
                }
                if ui.small_button("x").clicked() {
                    let _ = app.dispatch(Command::SkinMorphRemoveBone { id: b.node.0 });
                }
            });
        }
        ui.separator();
        ui.label("Morphs:");
        for (i, m) in d.morphs.iter().enumerate() {
            let bone = d.bones.get(m.bone).and_then(|b| app.state.scene.node(b.node)).map(|n| n.name.clone()).unwrap_or_default();
            if ui.selectable_label(app.state.skin_morph_current == Some(i), format!("{} ({bone} @ {:.0}°)", m.name, m.angle_deg)).clicked() {
                app.state.skin_morph_current = Some(i);
            }
        }
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(current_bone.is_some(), egui::Button::new("Create Morph")).clicked() {
                if let Some(b) = current_bone {
                    let _ = app.dispatch(Command::SkinMorphCreateMorph { bone: b.0 });
                }
            }
            if let Some(cur) = app.state.skin_morph_current {
                if ui.button("Delete Morph").clicked() {
                    let _ = app.dispatch(Command::SkinMorphDeleteMorph { morph: cur as u32 });
                }
                if ui.button("Reload").clicked() {
                    let _ = app.dispatch(Command::SkinMorphReload { morph: cur as u32 });
                }
            }
        });
    });
    if let Some((cur, m)) = app.state.skin_morph_current.and_then(|i| d.morphs.get(i).map(|m| (i, m.clone()))) {
        rollout(ui, "Local Properties", true, |ui| {
            let mut name = m.name.clone();
            let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(160.0));
            if r.lost_focus() && name != m.name {
                let _ = app.dispatch(Command::SkinMorphRenameMorph { morph: cur as u32, name });
            }
            let mut on = m.enabled;
            if ui.checkbox(&mut on, "Enabled").changed() {
                let _ = app.dispatch(Command::SkinMorphSetMorph { morph: cur as u32, name: "Enabled".into(), value: if on { 1.0 } else { 0.0 } });
            }
            egui::Grid::new("skin_morph_props").num_columns(2).show(ui, |ui| {
                for (label, pname, value, range) in [("Angle:", "Angle", m.angle_deg, -360.0..=360.0), ("Influence Angle:", "Influence Angle", m.influence_deg, 0.1..=360.0)] {
                    ui.label(label);
                    let mut v = value;
                    if ui.add(egui::DragValue::new(&mut v).speed(0.5).range(range)).changed() {
                        let _ = app.dispatch(Command::SkinMorphSetMorph { morph: cur as u32, name: pname.into(), value: v as f64 });
                    }
                    ui.end_row();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Falloff:");
                for (i, l) in ["Linear", "Sinual", "Fast", "Slow"].iter().enumerate() {
                    if ui.selectable_label(m.falloff == i as u32, *l).clicked() {
                        let _ = app.dispatch(Command::SkinMorphSetMorph { morph: cur as u32, name: "Falloff".into(), value: i as f64 });
                    }
                }
            });
            ui.label(format!("{} vertex deltas (Skin Morph Set Vertex Delta)", m.deltas.len()));
        });
    }
}

pub fn skin_wrap(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, index: usize) {
    let Some(node) = app.state.scene.node(id) else { return };
    let Some(slot) = node.modifiers.get(index) else { return };
    let Modifier::SkinWrap(d) = &slot.modifier else { return };
    let d = (**d).clone();
    rollout(ui, "Parameters", true, |ui| {
        ui.label("Base objects:");
        let candidates: Vec<(NodeId, String)> = app.state.scene.nodes().filter(|n| n.id != id && !n.object.is_helper() && !n.object.is_bone() && !d.bases.contains(&n.id)).map(|n| (n.id, n.name.clone())).collect();
        egui::ComboBox::from_id_salt("skin_wrap_add").selected_text("Add").show_ui(ui, |ui| {
            for (nid, name) in &candidates {
                if ui.selectable_label(false, name).clicked() {
                    let _ = app.dispatch(Command::SkinWrapAddBase { id: nid.0 });
                }
            }
        });
        for b in &d.bases {
            ui.horizontal(|ui| {
                ui.label(app.state.scene.node(*b).map(|n| n.name.clone()).unwrap_or_default());
                if ui.small_button("Remove").clicked() {
                    let _ = app.dispatch(Command::SkinWrapRemoveBase { id: b.0 });
                }
            });
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Deformation Engine:");
            for (i, l) in ["Face Deformation", "Vertex Deformation"].iter().enumerate() {
                if ui.selectable_label(d.engine == i as u32, *l).clicked() {
                    let _ = app.dispatch(Command::SkinWrapSetOption { name: "Deformation Engine".into(), value: i as f64 });
                }
            }
        });
        let speed = app.spinner_speed(0.1);
        egui::Grid::new("skin_wrap_opts").num_columns(2).show(ui, |ui| {
            for (label, name, value, range) in [("Falloff:", "Falloff", d.falloff as f64, 0.0..=10.0), ("Distance Infl.:", "Distance Influence", d.distance_influence as f64, 0.001..=100.0), ("Face Limit:", "Face Limit", d.face_limit as f64, 1.0..=50.0), ("Threshold:", "Threshold", d.threshold as f64, 0.0..=1e6)] {
                ui.label(label);
                let mut v = value;
                if ui.add(egui::DragValue::new(&mut v).speed(speed).range(range)).changed() {
                    let _ = app.dispatch(Command::SkinWrapSetOption { name: name.into(), value: if name == "Face Limit" { v.round() } else { v } });
                }
                ui.end_row();
            }
        });
        let mut all = d.weight_all_points;
        if ui.checkbox(&mut all, "Weight All Points").changed() {
            let _ = app.dispatch(Command::SkinWrapSetOption { name: "Weight All Points".into(), value: if all { 1.0 } else { 0.0 } });
        }
        ui.horizontal(|ui| {
            if ui.button("Reset").clicked() {
                let _ = app.dispatch(Command::SkinWrapReset);
            }
            if ui.add_enabled(!d.bases.is_empty(), egui::Button::new("Convert To Skin")).clicked() {
                let _ = app.dispatch(Command::SkinWrapConvertToSkin);
            }
        });
        ui.label(format!("{} of {} vertices bound", d.binds.iter().filter(|b| !b.is_empty()).count(), d.binds.len()));
    });
}
