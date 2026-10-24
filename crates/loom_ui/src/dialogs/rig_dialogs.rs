//! Animation > Bone Tools...: Bone Editing Tools, Fin
//! Adjustment Tools and Object Properties for the selected bones.

use crate::command_panel::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;
use loom_scene::{NodeId, ObjectKind};

fn selected_bones(app: &LoomApp) -> Vec<NodeId> {
    app.state.selection.iter().copied().filter(|id| app.state.scene.node(*id).is_some_and(|n| n.object.is_bone())).collect()
}

fn set_all(app: &mut LoomApp, bones: &[NodeId], name: &str, value: f64) {
    for id in bones {
        let _ = app.dispatch(Command::SetParameter { id: id.0, name: name.into(), value });
    }
}

pub fn bone_tools(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    egui::Window::new("Bone Tools").open(open).resizable(false).default_width(300.0).show(ctx, |ui| {
        let bones = selected_bones(app);
        let first = bones.first().and_then(|id| app.state.scene.node(*id)).cloned();
        rollout(ui, "Bone Editing Tools", true, |ui| {
            ui.label("Bone Pivot Position");
            let w = ui.available_width();
            if ui.add_sized([w, 22.0], egui::SelectableLabel::new(app.state.bone_edit_mode, "Bone Edit Mode")).clicked() {
                let _ = app.dispatch(Command::BoneEditModeToggle);
            }
            ui.label("Bone Tools");
            let half = (w - 8.0) / 2.0;
            ui.horizontal(|ui| {
                let on = app.create_kind == Some(crate::CreateKind::Bones);
                if ui.add_sized([half, 22.0], egui::SelectableLabel::new(on, "Create Bones")).clicked() {
                    app.create_kind = if on { None } else { Some(crate::CreateKind::Bones) };
                    app.bone_points.clear();
                }
                if ui.add_enabled(bones.len() == 1, egui::Button::new("Create End").min_size(egui::vec2(half, 22.0))).clicked() {
                    let _ = app.dispatch(Command::CreateEndBone { id: bones[0].0 });
                }
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(!bones.is_empty(), egui::Button::new("Remove").min_size(egui::vec2(half, 22.0))).clicked() {
                    for id in &bones {
                        let _ = app.dispatch(Command::DeleteBone { id: id.0 });
                    }
                }
                if ui.add_enabled(bones.len() == 2, egui::Button::new("Connect").min_size(egui::vec2(half, 22.0))).clicked() {
                    let _ = app.dispatch(Command::ConnectBones { id: bones[0].0, to: bones[1].0 });
                }
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(!bones.is_empty(), egui::Button::new("Delete").min_size(egui::vec2(half, 22.0))).clicked() {
                    let _ = app.dispatch(Command::DeleteObjects);
                }
                if ui.add_enabled(bones.len() == 1, egui::Button::new("Reassign Root").min_size(egui::vec2(half, 22.0))).clicked() {
                    let _ = app.dispatch(Command::ReassignRoot { id: bones[0].0 });
                }
            });
            if ui.add_enabled(bones.len() == 1, egui::Button::new("Refine").min_size(egui::vec2(w, 22.0))).clicked() {
                let _ = app.dispatch(Command::RefineBone { id: bones[0].0, t: 0.5 });
            }
            ui.label("Mirror");
            ui.horizontal(|ui| {
                ui.label("Plane:");
                for (i, l) in ["X", "Y", "Z"].iter().enumerate() {
                    if ui.selectable_label(app.bone_mirror_axis == i as u32, *l).clicked() {
                        app.bone_mirror_axis = i as u32;
                    }
                }
                ui.label("Offset:");
                let speed = app.spinner_speed(0.1);
                ui.add(egui::DragValue::new(&mut app.bone_mirror_offset).speed(speed));
            });
            if ui.add_enabled(!bones.is_empty(), egui::Button::new("Mirror").min_size(egui::vec2(w, 22.0))).clicked() {
                let _ = app.dispatch(Command::MirrorBones { axis: app.bone_mirror_axis, offset: app.bone_mirror_offset, flip: 0 });
            }
        });
        rollout(ui, "Fin Adjustment Tools", true, |ui| {
            let Some(n) = first.as_ref() else {
                ui.label("Select a bone.");
                return;
            };
            let ObjectKind::Bone { width, height, taper, fins, .. } = &n.object else { return };
            let speed = app.spinner_speed(0.1);
            egui::Grid::new("bone_size").num_columns(2).show(ui, |ui| {
                for (label, name, value, range) in [("Width:", "Width", width.get(), 0.0..=1e6), ("Height:", "Height", height.get(), 0.0..=1e6), ("Taper:", "Taper", taper.get() * 100.0, 0.0..=100.0)] {
                    ui.label(label);
                    let mut v = value;
                    if ui.add(egui::DragValue::new(&mut v).speed(speed).range(range)).changed() {
                        set_all(app, &bones, name, if name == "Taper" { (v / 100.0) as f64 } else { v as f64 });
                    }
                    ui.end_row();
                }
            });
            for (group, on, size, start, end) in [
                ("Side Fins", fins.side.get(), fins.side_size.get(), fins.side_start_taper.get(), fins.side_end_taper.get()),
                ("Front Fins", fins.front.get(), fins.front_size.get(), fins.front_start_taper.get(), fins.front_end_taper.get()),
                ("Back Fins", fins.back.get(), fins.back_size.get(), fins.back_start_taper.get(), fins.back_end_taper.get()),
            ] {
                let mut v = on;
                if ui.checkbox(&mut v, group).changed() {
                    set_all(app, &bones, group, if v { 1.0 } else { 0.0 });
                }
                let stem = group.trim_end_matches('s');
                egui::Grid::new(group).num_columns(2).show(ui, |ui| {
                    for (label, suffix, value, scale) in [("Size:", " Size", size, 1.0), ("Start Taper:", " Start Taper", start * 100.0, 0.01), ("End Taper:", " End Taper", end * 100.0, 0.01)] {
                        ui.label(label);
                        let mut x = value;
                        if ui.add_enabled(on, egui::DragValue::new(&mut x).speed(speed).range(0.0..=if scale < 1.0 { 100.0 } else { 1e6 })).changed() {
                            set_all(app, &bones, &format!("{stem}{suffix}"), (x * scale) as f64);
                        }
                        ui.end_row();
                    }
                });
            }
        });
        rollout(ui, "Object Properties", true, |ui| {
            let Some(n) = first.as_ref() else { return };
            let ObjectKind::Bone { bone_on, freeze_length, stretch, stretch_axis, .. } = &n.object else { return };
            for (label, name, on) in [("Bone On", "Bone On", bone_on.get()), ("Freeze Length", "Freeze Length", freeze_length.get())] {
                let mut v = on;
                if ui.checkbox(&mut v, label).changed() {
                    set_all(app, &bones, name, if v { 1.0 } else { 0.0 });
                }
            }
            ui.horizontal(|ui| {
                ui.label("Stretch:");
                for (i, l) in ["None", "Scale", "Squash"].iter().enumerate() {
                    if ui.selectable_label(stretch.get() == i as u32, *l).clicked() {
                        set_all(app, &bones, "Stretch", i as f64);
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Axis:");
                for (i, l) in ["X", "Y", "Z"].iter().enumerate() {
                    if ui.selectable_label(stretch_axis.get() == i as u32, *l).clicked() {
                        set_all(app, &bones, "Stretch Axis", i as f64);
                    }
                }
            });
        });
    });
}

/// Skin > Weight Table: vertices down the left, the bones across the top.
/// Drag or type a weight; with several rows selected an edit on a selected
/// row sets them all; right-click enters 0, Ctrl+right-click 1.
pub fn weight_table(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let Ok(e) = loom_commands::skin::entry(&app.state) else {
        *open = false;
        return;
    };
    let id = egui::Id::new("weight_table");
    // Vertex selection drop-down: 0 All Vertices, 1 Selected Vertices, 2 Selected Bone; then Show Affected Bones, Show Attributes.
    let (mut shown, mut affected_only, mut attributes) = ctx.data(|d| d.get_temp::<(u8, bool, bool)>(id)).unwrap_or((0, false, true));
    let sel = loom_commands::skin::selection(&app.state);
    let sel_set: std::collections::BTreeSet<u32> = sel.iter().copied().collect();
    let current = app.state.skin_bone.or_else(|| e.data.bones.first().map(|b| b.node));
    let weights: Vec<Vec<(u32, f32)>> = (0..e.input.vertex_count()).map(|v| e.data.vertex_weights(v, e.data.mesh_bind.transform_point3(e.input.vertices()[v].position))).collect();
    let cur_index = e.data.bones.iter().position(|b| Some(b.node) == current).map(|i| i as u32);
    let rows: Vec<usize> = match shown {
        1 => sel.iter().map(|v| *v as usize).collect(),
        2 => (0..weights.len()).filter(|v| cur_index.is_some_and(|b| weights[*v].iter().any(|(x, w)| *x == b && *w > 0.0))).collect(),
        _ => (0..weights.len()).collect(),
    };
    let bones: Vec<(usize, String)> = e
        .data
        .bones
        .iter()
        .enumerate()
        .filter(|(i, _)| !affected_only || rows.iter().any(|v| weights[*v].iter().any(|(x, w)| *x as usize == *i && *w > 0.0)))
        .map(|(i, b)| (i, app.state.scene.node(b.node).map(|n| n.name.clone()).unwrap_or_default()))
        .collect();
    egui::Window::new("Weight Table").open(open).resizable(true).default_width(560.0).show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("Edit", |ui| {
                for (label, cmd) in [("Copy", Command::CopyWeightsSkin), ("Paste", Command::PasteWeightsSkin), ("Remove Zero Weights", Command::RemoveZeroWeightsSkin)] {
                    if ui.button(label).clicked() {
                        let _ = app.dispatch(cmd);
                        ui.close_menu();
                    }
                }
                ui.separator();
                let all: Vec<u32> = (0..weights.len() as u32).collect();
                let invert: Vec<u32> = all.iter().copied().filter(|v| !sel_set.contains(v)).collect();
                for (label, ids) in [("Select All", all), ("Select None", Vec::new()), ("Select Invert", invert)] {
                    if ui.button(label).clicked() {
                        let _ = app.dispatch(Command::SkinSelectVertices { ids, op: loom_commands::SelectOp::Replace });
                        ui.close_menu();
                    }
                }
            });
            ui.menu_button("Options", |ui| {
                ui.checkbox(&mut affected_only, "Show Affected Bones");
                ui.checkbox(&mut attributes, "Show Attributes");
            });
        });
        const CELL: f32 = 64.0;
        const ATTR: f32 = 16.0;
        let row_h = ui.spacing().interact_size.y + 2.0;
        egui::ScrollArea::horizontal().id_salt("weight_table_h").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_sized([56.0, row_h], egui::Label::new("Vertex ID"));
                if attributes {
                    for a in ["S", "M", "R"] {
                        ui.add_sized([ATTR, row_h], egui::Label::new(a));
                    }
                }
                for (i, name) in &bones {
                    let text = if Some(e.data.bones[*i].node) == current { egui::RichText::new(name.as_str()).strong() } else { egui::RichText::new(name.as_str()) };
                    // A click on a bone name makes it the current bone.
                    if ui.add_sized([CELL, row_h], egui::Label::new(text).truncate().sense(egui::Sense::click())).clicked() {
                        let _ = app.dispatch(Command::SkinSelectBone { id: e.data.bones[*i].node.0 });
                    }
                }
            });
            // Only the rows in view are laid out (a 6k-vertex mesh × 50 bones stays interactive).
            egui::ScrollArea::vertical().id_salt("weight_table_v").max_height(360.0).show_rows(ui, row_h, rows.len(), |ui, range| {
                for v in rows[range].iter().copied() {
                    let w = &weights[v];
                    let selected = sel_set.contains(&(v as u32));
                    ui.horizontal(|ui| {
                        // Vertex ID: click selects (Ctrl adds, Alt removes).
                        if ui.add_sized([56.0, row_h], egui::SelectableLabel::new(selected, format!("#{v}"))).clicked() {
                            let m = ui.input(|i| i.modifiers);
                            let op = if m.ctrl { loom_commands::SelectOp::Add } else if m.alt { loom_commands::SelectOp::Subtract } else { loom_commands::SelectOp::Replace };
                            let _ = app.dispatch(Command::SkinSelectVertices { ids: vec![v as u32], op });
                        }
                        if attributes {
                            let flags = [selected, e.data.explicit.get(v).copied().unwrap_or(false), e.data.rigid.get(v).copied().unwrap_or(false)];
                            for f in flags {
                                ui.add_sized([ATTR, row_h], egui::Label::new(if f { "x" } else { "" }));
                            }
                        }
                        for (i, _) in &bones {
                            let cur = w.iter().find(|(b, _)| *b as usize == *i).map(|x| x.1).unwrap_or(0.0);
                            let mut x = cur;
                            let r = ui.add_sized([CELL, row_h], egui::DragValue::new(&mut x).speed(0.01).range(0.0..=1.0).fixed_decimals(3));
                            // An edit on a selected row sets every selected vertex.
                            let targets: Vec<u32> = if selected { sel.clone() } else { vec![v as u32] };
                            let bone = e.data.bones[*i].node.0;
                            // A drag is one undo entry.
                            if r.drag_started() {
                                app.state.undo.begin("Weight Table");
                            }
                            if r.changed() {
                                let _ = app.dispatch(Command::SkinSetVertexWeights { vertices: targets.clone(), bone, weight: x });
                            }
                            if r.drag_stopped() {
                                app.state.undo.end();
                            }
                            if r.secondary_clicked() {
                                let one = ui.input(|i| i.modifiers.ctrl);
                                let _ = app.dispatch(Command::SkinSetVertexWeights { vertices: targets, bone, weight: if one { 1.0 } else { 0.0 } });
                            }
                        }
                    });
                }
            });
        });
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("weight_table_rows").selected_text(["All Vertices", "Selected Vertices", "Selected Bone"][shown as usize % 3]).show_ui(ui, |ui| {
                for (k, label) in ["All Vertices", "Selected Vertices", "Selected Bone"].iter().enumerate() {
                    ui.selectable_value(&mut shown, k as u8, *label);
                }
            });
            ui.label(format!("{} of {} vertices", rows.len(), weights.len()));
        });
    });
    ctx.data_mut(|d| d.insert_temp(id, (shown, affected_only, attributes)));
}

/// Skin > Weight Tool: selection tools, the weight buttons and the first
/// selected vertex's weights.
pub fn weight_tool(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    if loom_commands::skin::entry(&app.state).is_err() {
        *open = false;
        return;
    }
    egui::Window::new("Weight Tool").open(open).resizable(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            for (label, cmd) in [("Shrink", Command::SkinShrink), ("Grow", Command::SkinGrow), ("Ring", Command::SkinRing), ("Loop", Command::SkinLoop)] {
                if ui.button(label).clicked() {
                    let _ = app.dispatch(cmd);
                }
            }
        });
        ui.horizontal(|ui| {
            for (label, cmd) in [("0", Command::SetWeightTo00Skin), (".1", Command::SetWeightTo010Skin), (".25", Command::SetWeightTo025Skin), (".5", Command::SetWeightTo05Skin), (".75", Command::SetWeightTo075Skin), (".9", Command::SetWeightTo090Skin), ("1", Command::SetWeightTo10Skin)] {
                if ui.button(label).clicked() {
                    let _ = app.dispatch(cmd);
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Set Weight").clicked() {
                let _ = app.dispatch(Command::SetWeightCustomSkin);
            }
            let mut v = app.state.skin_custom_weight;
            if ui.add(egui::DragValue::new(&mut v).speed(0.05).range(0.0..=1.0).fixed_decimals(3)).changed() {
                let _ = app.dispatch(Command::SkinSetCustomWeight { value: v });
            }
            if ui.button("+").clicked() {
                let _ = app.dispatch(Command::AddWeightSkin);
            }
            if ui.button("-").clicked() {
                let _ = app.dispatch(Command::SubtractWeightSkin);
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Scale Weight").clicked() {
                let _ = app.dispatch(Command::ScaleWeightCustomSkin);
            }
            let mut k = app.state.skin_scale_weight;
            if ui.add(egui::DragValue::new(&mut k).speed(0.05).range(0.0..=10.0).fixed_decimals(3)).changed() {
                let _ = app.dispatch(Command::SkinSetScaleWeight { value: k });
            }
            if ui.button("+").clicked() {
                let _ = app.dispatch(Command::ScaleWeightUpSkin);
            }
            if ui.button("-").clicked() {
                let _ = app.dispatch(Command::ScaleWeightDownSkin);
            }
        });
        ui.horizontal(|ui| {
            for (label, cmd) in [("Copy", Command::CopyWeightsSkin), ("Paste", Command::PasteWeightsSkin), ("Paste-Pos", Command::PasteWeightsByPosSkin), ("Blend", Command::BlendWeightsSkin)] {
                if ui.button(label).clicked() {
                    let _ = app.dispatch(cmd);
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Paste-Pos Tolerance:");
            let mut t = app.state.skin_paste_tolerance;
            if ui.add(egui::DragValue::new(&mut t).speed(0.01).range(0.0..=1e6).fixed_decimals(3)).changed() {
                let _ = app.dispatch(Command::SkinSetPasteTolerance { value: t });
            }
        });
        if let Ok(e) = loom_commands::skin::entry(&app.state) {
            let sel: Vec<u32> = loom_commands::skin::selection(&app.state);
            ui.label(format!("{} Copied   {} Selected", app.state.skin_clipboard_weights.len(), sel.len()));
            if let Some(v) = sel.first() {
                // The first selected vertex's bones: a click makes one the current bone.
                let p = e.data.mesh_bind.transform_point3(e.input.vertices()[*v as usize].position);
                for (b, w) in e.data.vertex_weights(*v as usize, p) {
                    let Some(bone) = e.data.bones.get(b as usize) else { continue };
                    let name = app.state.scene.node(bone.node).map(|n| n.name.clone()).unwrap_or_default();
                    if ui.selectable_label(app.state.skin_bone == Some(bone.node), format!("{w:.3}  {name}")).clicked() {
                        let _ = app.dispatch(Command::SkinSelectBone { id: bone.node.0 });
                    }
                }
            }
        }
    });
}

/// Skin > Weight Properties > Painter Options: the brush for Paint Weights.
pub fn painter_options(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    if loom_commands::skin::entry(&app.state).is_err() {
        *open = false;
        return;
    }
    let p = app.state.skin_painter.clone();
    let set = |app: &mut LoomApp, name: &str, value: f64| {
        let _ = app.dispatch(Command::SkinSetPainterOption { name: name.into(), value });
    };
    egui::Window::new("Painter Options").open(open).resizable(false).show(ctx, |ui| {
        ui.label("Brush Properties");
        egui::Grid::new("painter_brush").num_columns(2).show(ui, |ui| {
            ui.label("Max. Strength:");
            let mut s = p.strength;
            if ui.add(egui::DragValue::new(&mut s).speed(0.01).range(0.0..=1.0).fixed_decimals(2)).changed() {
                set(app, "Max Strength", s as f64);
            }
            ui.end_row();
            ui.label("Max. Size:");
            let mut z = p.size;
            if ui.add(egui::DragValue::new(&mut z).speed(app.spinner_speed(0.1)).range(0.01..=1e6).fixed_decimals(2)).changed() {
                set(app, "Max Size", z as f64);
            }
            ui.end_row();
        });
        let mut additive = p.additive;
        if ui.checkbox(&mut additive, "Additive").changed() {
            set(app, "Additive", if additive { 1.0 } else { 0.0 });
        }
        ui.horizontal(|ui| {
            ui.label("Falloff:");
            for (i, l) in ["Linear", "Smooth", "Slow", "Fast", "Flat"].iter().enumerate() {
                if ui.selectable_label(p.falloff == i as u32, *l).clicked() {
                    set(app, "Falloff", i as f64);
                }
            }
        });
        ui.separator();
        ui.label("Display Options");
        let mut ring = p.draw_ring;
        if ui.checkbox(&mut ring, "Draw Ring").changed() {
            set(app, "Draw Ring", if ring { 1.0 } else { 0.0 });
        }
        ui.separator();
        ui.horizontal(|ui| {
            let mut m = p.mirror;
            if ui.checkbox(&mut m, "Mirror").changed() {
                set(app, "Mirror", if m { 1.0 } else { 0.0 });
            }
            for (i, l) in ["X", "Y", "Z"].iter().enumerate() {
                if ui.selectable_label(p.mirror_axis == i as u32, *l).clicked() {
                    set(app, "Mirror Axis", i as f64);
                }
            }
            ui.label("Offset:");
            let mut o = p.mirror_offset;
            if ui.add(egui::DragValue::new(&mut o).speed(app.spinner_speed(0.1))).changed() {
                set(app, "Mirror Offset", o as f64);
            }
        });
    });
}
