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

/// Set a bone parameter on every selected bone: Absolute sets `value` on each,
/// Relative adds the first bone's change to each bone's own value.
fn set_all(app: &mut LoomApp, bones: &[NodeId], name: &str, value: f64) {
    let delta = if app.state.bone_fin_absolute {
        None
    } else {
        bones.first().and_then(|id| app.state.scene.node(*id)).and_then(|n| n.object.parameters().iter().find(|(p, _)| *p == name).map(|(_, v)| value - v.as_f64()))
    };
    for id in bones {
        let value = match delta {
            Some(d) => app.state.scene.node(*id).and_then(|n| n.object.parameters().iter().find(|(p, _)| *p == name).map(|(_, v)| v.as_f64() + d)).unwrap_or(value),
            None => value,
        };
        let _ = app.dispatch(Command::SetParameter { id: id.0, name: name.into(), value });
    }
}

/// A colour swatch that opens egui's colour picker (Bone Coloring).
fn swatch(ui: &mut egui::Ui, rgb: &mut [u8; 3], label: &str) -> egui::Response {
    let r = ui.color_edit_button_srgb(rgb);
    let owned = label.to_owned();
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, owned.clone()));
    r
}

pub fn bone_tools(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    egui::Window::new("Bone Tools").open(open).resizable(false).default_width(300.0).show(ctx, |ui| {
        // The floater is taller than the screen with every rollout open, as in
        // the reference, so its rollouts scroll.
        egui::ScrollArea::vertical().max_height(ctx.screen_rect().height() - 120.0).show(ui, |ui| {
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
                    for id in &bones {
                        let _ = app.dispatch(Command::BoneToolsDelete { id: id.0 });
                    }
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
                ui.add(crate::spinner::spin(&mut app.bone_mirror_offset, ..).speed(speed));
            });
            if ui.add_enabled(!bones.is_empty(), egui::Button::new("Mirror").min_size(egui::vec2(w, 22.0))).clicked() {
                let _ = app.dispatch(Command::MirrorBones { axis: app.bone_mirror_axis, offset: app.bone_mirror_offset, flip: 0 });
            }
            ui.label("Bone Coloring");
            let current = first.as_ref().map(|n| n.wire_color).unwrap_or(app.state.display.current_color);
            ui.horizontal(|ui| {
                ui.label("Selected Bone Color:");
                let mut c = current;
                if swatch(ui, &mut c, "Selected Bone Color").changed() {
                    let _ = app.dispatch(Command::SetObjectColor { rgb: c });
                }
            });
            ui.horizontal(|ui| {
                ui.label("Start Color:");
                let mut a = app.bone_gradient_start;
                if swatch(ui, &mut a, "Start Color").changed() {
                    app.bone_gradient_start = a;
                }
                ui.label("End Color:");
                let mut b = app.bone_gradient_end;
                if swatch(ui, &mut b, "End Color").changed() {
                    app.bone_gradient_end = b;
                }
            });
            if ui.add_enabled(bones.len() >= 2, egui::Button::new("Apply Gradient").min_size(egui::vec2(w, 22.0))).clicked() {
                let _ = app.dispatch(Command::BoneApplyGradient { start: app.bone_gradient_start, end: app.bone_gradient_end });
            }
        });
        rollout(ui, "Fin Adjustment Tools", true, |ui| {
            ui.horizontal(|ui| {
                for (label, absolute) in [("Absolute", true), ("Relative", false)] {
                    if ui.selectable_label(app.state.bone_fin_absolute == absolute, label).clicked() {
                        let _ = app.dispatch(Command::BoneFinMode { absolute });
                    }
                }
                if ui.add_enabled(!bones.is_empty(), egui::Button::new("Copy")).clicked() {
                    let _ = app.dispatch(Command::BoneCopySettings);
                }
                if ui.add_enabled(!bones.is_empty() && app.state.bone_settings_clipboard.is_some(), egui::Button::new("Paste")).clicked() {
                    let _ = app.dispatch(Command::BonePasteSettings);
                }
            });
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
                    if ui.add(crate::spinner::spin(&mut v, range).speed(speed)).changed() {
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
                        if ui.add_enabled(on, crate::spinner::spin(&mut x, 0.0..=if scale < 1.0 { 100.0 } else { 1e6 }).speed(speed)).changed() {
                            set_all(app, &bones, &format!("{stem}{suffix}"), (x * scale) as f64);
                        }
                        ui.end_row();
                    }
                });
            }
        });
        rollout(ui, "Object Properties", true, |ui| {
            let Some(n) = first.as_ref() else { return };
            let ObjectKind::Bone { bone_on, freeze_length, stretch, stretch_axis, stretch_flip, auto_align, correct_negative_stretch, .. } = &n.object else { return };
            let on = bone_on.get();
            let mut v = on;
            if ui.checkbox(&mut v, "Bone On").changed() {
                set_all(app, &bones, "Bone On", if v { 1.0 } else { 0.0 });
            }
            for (label, name, value) in [
                ("Freeze Length", "Freeze Length", freeze_length.get()),
                ("Auto-Align", "Auto-Align", auto_align.get()),
                ("Correct Negative Stretch", "Correct Negative Stretch", correct_negative_stretch.get()),
            ] {
                let mut v = value;
                if ui.add_enabled(on, egui::Checkbox::new(&mut v, label)).changed() {
                    set_all(app, &bones, name, if v { 1.0 } else { 0.0 });
                }
            }
            let w = ui.available_width();
            let third = (w - 16.0) / 3.0;
            ui.horizontal(|ui| {
                for (label, cmd) in [("Realign", Command::BoneRealign), ("Reset Stretch", Command::BoneResetStretch), ("Reset Scale", Command::BoneResetScale)] {
                    if ui.add_enabled(on, egui::Button::new(label).min_size(egui::vec2(third, 22.0))).clicked() {
                        let _ = app.dispatch(cmd);
                    }
                }
            });
            // Stretch Factor Information, read in Bone Edit Mode as in the reference.
            ui.label(if !app.state.bone_edit_mode {
                format!("{} Bone(s) Selected   Stretch Factor: (Bone Edit Mode off)", bones.len())
            } else if bones.len() == 1 {
                let s = n.transform.scale.get();
                format!("1 Bone Selected   Stretch Factor: X: {:.3} Y: {:.3} Z: {:.3}", s.x, s.y, s.z)
            } else {
                format!("{} Bones Selected   Stretch Factor: undefined", bones.len())
            });
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
                let mut f = stretch_flip.get();
                if ui.checkbox(&mut f, "Flip").changed() {
                    set_all(app, &bones, "Flip", if f { 1.0 } else { 0.0 });
                }
            });
        });
        });
    });
}

/// Skin > Weight Tool: selection tools, the weight buttons and the first
/// selected vertex's weights.
pub fn weight_tool(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    if loom_commands::skin::entry(&app.state).is_err() {
        *open = false;
        return;
    }
    let window = egui::Window::new("Weight Tool").resizable(false);
    crate::os_window::OsWindow::new("Weight Tool", "Weight Tool", [260.0, 320.0]).fixed().show(ctx, open, window, |ui| {
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
            if ui.add(crate::spinner::spin(&mut v, 0.0..=1.0).speed(0.05).fixed_decimals(3)).changed() {
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
            if ui.add(crate::spinner::spin(&mut k, 0.0..=10.0).speed(0.05).fixed_decimals(3)).changed() {
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
            if ui.add(crate::spinner::spin(&mut t, 0.0..=1e6).speed(0.01).fixed_decimals(3)).changed() {
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
