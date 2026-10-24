//! Skin modifier rollouts: Parameters, Mirror Parameters,
//! Display, Advanced Parameters, Gizmos.

use super::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;
use loom_scene::{Modifier, NodeId};

fn set(app: &mut LoomApp, index: usize, name: &str, value: f64) {
    let _ = app.dispatch(Command::SetModifierParameter { index: index as u32, name: name.into(), value });
}

fn check(ui: &mut egui::Ui, app: &mut LoomApp, index: usize, name: &str, label: &str, on: bool) {
    let mut v = on;
    if ui.checkbox(&mut v, label).changed() {
        set(app, index, name, if v { 1.0 } else { 0.0 });
    }
}

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, index: usize) {
    let Some(node) = app.state.scene.node(id) else { return };
    let Some(slot) = node.modifiers.get(index) else { return };
    let Modifier::Skin(d) = &slot.modifier else { return };
    let d = (**d).clone();
    let editing = node.active_entry == Some(index) && slot.sub_level == 1;
    rollout(ui, "Parameters", true, |ui| {
        let w = ui.available_width();
        if ui.add_sized([w, 22.0], egui::SelectableLabel::new(editing, "Edit Envelopes")).clicked() {
            let _ = app.dispatch(Command::ModifierSubobjectLevel { index: index as u32, level: if editing { 0 } else { 1 } });
        }
        ui.label("Select");
        ui.horizontal_wrapped(|ui| {
            for (label, on, cmd) in [("Vertices", app.state.skin_filter_vertices, Command::FilterVerticesSkin), ("Envelopes", app.state.skin_filter_envelopes, Command::FilterEnvelopesSkin), ("Cross Sections", app.state.skin_filter_cross_sections, Command::FilterCrossSectionsSkin)] {
                let mut v = on;
                if ui.checkbox(&mut v, label).changed() {
                    let _ = app.dispatch(cmd);
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (label, cmd) in [("Shrink", Command::SkinShrink), ("Grow", Command::SkinGrow), ("Ring", Command::SkinRing), ("Loop", Command::SkinLoop)] {
                if ui.add_enabled(editing, egui::Button::new(label)).clicked() {
                    let _ = app.dispatch(cmd);
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(editing, egui::Button::new("Select Element")).clicked() {
                let _ = app.dispatch(Command::SkinElement);
            }
            let mut bf = app.state.skin_backface_cull;
            if ui.checkbox(&mut bf, "Backface Cull Vertices").changed() {
                let _ = app.dispatch(Command::BackfaceCullVerticesSkin);
            }
        });
        ui.separator();
        ui.label("Bones:");
        ui.horizontal(|ui| {
            if ui.button("Add").clicked() {
                let _ = app.dispatch(Command::AddBonesSkin);
            }
            if ui.add_enabled(app.state.skin_bone.is_some(), egui::Button::new("Remove")).clicked() {
                let _ = app.dispatch(Command::RemoveMultipleBonesSkin);
            }
        });
        let current = app.state.skin_bone.or_else(|| d.bones.first().map(|b| b.node));
        egui::ScrollArea::vertical().max_height(140.0).id_salt("skin_bones").show(ui, |ui| {
            for b in &d.bones {
                let name = app.state.scene.node(b.node).map(|n| n.name.clone()).unwrap_or_else(|| format!("#{}", b.node.0));
                if ui.selectable_label(current == Some(b.node), name).clicked() {
                    let _ = app.dispatch(Command::SkinSelectBone { id: b.node.0 });
                }
            }
        });
        // Bone name field: typing highlights the first matching bone (* wildcards).
        let name_id = egui::Id::new(("skin_bone_name", id, index));
        let mut typed = ui.data(|d| d.get_temp::<String>(name_id)).unwrap_or_default();
        if ui.add(egui::TextEdit::singleline(&mut typed).desired_width(ui.available_width())).changed() {
            if !typed.is_empty() {
                let _ = app.dispatch(Command::SkinSelectBoneByName { name: typed.clone() });
            }
            ui.data_mut(|d| d.insert_temp(name_id, typed));
        }
        ui.separator();
        ui.label("Cross Sections");
        ui.horizontal(|ui| {
            if ui.button("Add").clicked() {
                let _ = app.dispatch(Command::AddCrossSectionSkin);
            }
            if ui.button("Remove").clicked() {
                let _ = app.dispatch(Command::RemoveCrossSectionSkin);
            }
        });
        if let Some(b) = current.and_then(|c| d.bones.iter().find(|b| b.node == c)) {
            ui.label("Envelope Properties");
            let e = b.envelope.clone();
            let speed = app.spinner_speed(0.1);
            egui::Grid::new("skin_env").num_columns(4).show(ui, |ui| {
                ui.label("Radius:");
                let mut i0 = e.inner[0];
                if ui.add(crate::spinner::spin(ui, &mut i0, 0.0..=1e6).speed(speed)).changed() {
                    let _ = app.dispatch(Command::SkinSetEnvelope { inner_start: Some(i0), inner_end: Some(i0), outer_start: None, outer_end: None, falloff: None, abs_effect: None, squash: None });
                }
                let mut o0 = e.outer[0];
                if ui.add(crate::spinner::spin(ui, &mut o0, 0.0..=1e6).speed(speed)).changed() {
                    let _ = app.dispatch(Command::SkinSetEnvelope { inner_start: None, inner_end: None, outer_start: Some(o0), outer_end: Some(o0), falloff: None, abs_effect: None, squash: None });
                }
                ui.label("inner / outer");
                ui.end_row();
                ui.label("Squash:");
                let mut sq = e.squash;
                if ui.add(crate::spinner::spin(ui, &mut sq, 0.0..=10.0).speed(speed)).changed() {
                    let _ = app.dispatch(Command::SkinSetEnvelope { inner_start: None, inner_end: None, outer_start: None, outer_end: None, falloff: None, abs_effect: None, squash: Some(sq) });
                }
                ui.end_row();
            });
            ui.horizontal(|ui| {
                ui.label("Falloff:");
                for (i, l) in ["Linear", "Sinual", "Fast Out", "Slow Out"].iter().enumerate() {
                    if ui.selectable_label(e.falloff == i as u32, *l).clicked() {
                        let _ = app.dispatch(Command::SkinSetEnvelope { inner_start: None, inner_end: None, outer_start: None, outer_end: None, falloff: Some(i as u32), abs_effect: None, squash: None });
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.selectable_label(b.envelope_visible, "Envelope Visibility").clicked() {
                    let _ = app.dispatch(Command::SkinEnvelopeVisibility);
                }
                if ui.button("Copy").clicked() {
                    let _ = app.dispatch(Command::CopyEnvelopeSkin);
                }
                if ui.button("Paste").clicked() {
                    let _ = app.dispatch(Command::PasteEnvelopeSkin);
                }
                if ui.button("Paste to All Bones").clicked() {
                    let _ = app.dispatch(Command::PasteToAllEnvelopesSkin);
                }
            });
            ui.separator();
            ui.label("Weight Properties");
            let sel = loom_commands::skin::selection(&app.state);
            ui.horizontal(|ui| {
                ui.label("Abs. Effect:");
                if let Some(first) = sel.first() {
                    // With vertices selected: the current bone's weight on them (the first one's shows).
                    let bi = d.bones.iter().position(|x| x.node == b.node).unwrap_or(0) as u32;
                    let w = loom_commands::skin::entry(&app.state).ok().map(|en| {
                        let p = en.data.mesh_bind.transform_point3(en.input.vertices()[*first as usize].position);
                        en.data.vertex_weights(*first as usize, p).into_iter().find(|(x, _)| *x == bi).map(|x| x.1).unwrap_or(0.0)
                    });
                    let mut ab = w.unwrap_or(0.0);
                    let r = ui.add(crate::spinner::spin(ui, &mut ab, 0.0..=1.0).speed(0.01).fixed_decimals(3));
                    // A drag is one undo entry.
                    if r.drag_started() {
                        app.state.undo.begin("Abs. Effect");
                    }
                    if r.changed() {
                        let _ = app.dispatch(Command::SkinSetVertexWeight { weight: ab, absolute: true });
                    }
                    if r.drag_stopped() {
                        app.state.undo.end();
                    }
                } else {
                    let mut ab = e.abs_effect;
                    if ui.add(crate::spinner::spin(ui, &mut ab, 0.0..=1.0).speed(0.01)).changed() {
                        let _ = app.dispatch(Command::SkinSetEnvelope { inner_start: None, inner_end: None, outer_start: None, outer_end: None, falloff: None, abs_effect: Some(ab), squash: None });
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                let rigid = sel.iter().all(|v| d.rigid.get(*v as usize).copied().unwrap_or(false)) && !sel.is_empty();
                if ui.selectable_label(rigid, "Rigid").clicked() {
                    let _ = app.dispatch(Command::SkinRigidToggle);
                }
                if ui.button("Normalize").clicked() {
                    let _ = app.dispatch(Command::SkinNormalize);
                }
            });
            ui.horizontal_wrapped(|ui| {
                for (label, cmd) in [("Exclude Selected Verts", Command::ExcludeVertsSkin), ("Include Selected Verts", Command::IncludeVertsSkin), ("Select Excluded Verts", Command::SelectExcludedVertsSkin)] {
                    if ui.button(label).clicked() {
                        let _ = app.dispatch(cmd);
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.button("Bake Selected Verts").clicked() {
                    let _ = app.dispatch(Command::SkinBakeSelectedVerts);
                }
                if ui.button("Weight Tool").clicked() {
                    let _ = app.dispatch(Command::WeightToolDialogSkin);
                }
                if ui.button("Weight Table").clicked() {
                    let _ = app.dispatch(Command::WeightTableDialogSkin);
                }
            });
            ui.horizontal(|ui| {
                if ui.selectable_label(loom_commands::skin::painting(&app.state), "Paint Weights").clicked() {
                    let _ = app.dispatch(Command::SkinPaintWeightsToggle);
                }
                if ui.button("...").on_hover_text("Painter Options").clicked() {
                    let _ = app.dispatch(Command::SkinPainterOptionsDialog);
                }
            });
            let mut blend = app.state.skin_painter.blend;
            if ui.checkbox(&mut blend, "Paint Blend Weights").changed() {
                let _ = app.dispatch(Command::SkinSetPainterOption { name: "Paint Blend Weights".into(), value: if blend { 1.0 } else { 0.0 } });
            }
        }
    });
    rollout(ui, "Mirror Parameters", false, |ui| {
        check(ui, app, index, "Mirror Mode", "Mirror Mode", d.mirror.mode);
        ui.horizontal(|ui| {
            ui.label("Mirror Plane:");
            for (i, l) in ["X", "Y", "Z"].iter().enumerate() {
                if ui.selectable_label(d.mirror.plane == i as u32, *l).clicked() {
                    set(app, index, "Mirror Plane", i as f64);
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (label, cmd) in [("Mirror Paste", Command::MirrorPaste), ("Paste Green to Blue", Command::PasteGreenToBlue), ("Paste Blue to Green", Command::PasteBlueToGreen)] {
                if ui.add_enabled(d.mirror.mode, egui::Button::new(label)).clicked() {
                    let _ = app.dispatch(cmd);
                }
            }
        });
        let speed = app.spinner_speed(0.1);
        egui::Grid::new("skin_mirror").num_columns(2).show(ui, |ui| {
            ui.label("Mirror Offset:");
            let mut o = d.mirror.offset;
            if ui.add(crate::spinner::spin(ui, &mut o, ..).speed(speed)).changed() {
                set(app, index, "Mirror Offset", o as f64);
            }
            ui.end_row();
            ui.label("Mirror Thresh.:");
            let mut th = d.mirror.threshold;
            if ui.add(crate::spinner::spin(ui, &mut th, 0.0..=1e6).speed(speed)).changed() {
                set(app, index, "Mirror Threshold", th as f64);
            }
            ui.end_row();
        });
    });
    rollout(ui, "Display", false, |ui| {
        check(ui, app, index, "Show Colored Vertices", "Show Colored Vertices", d.display.colored_vertices);
        check(ui, app, index, "Show Colored Faces", "Show Colored Faces", d.display.colored_faces);
        check(ui, app, index, "Color All Weights", "Color All Weights", d.display.color_all_weights);
        check(ui, app, index, "Show All Envelopes", "Show All Envelopes", d.display.all_envelopes);
        check(ui, app, index, "Show All Gizmos", "Show All Gizmos", d.display.all_gizmos);
        check(ui, app, index, "Show No Envelopes", "Show No Envelopes", d.display.no_envelopes);
        check(ui, app, index, "Draw CrossSections On Top", "Draw Cross Sections On Top", d.display.cross_sections_on_top);
        check(ui, app, index, "Draw Envelope On Top", "Draw Envelope On Top", d.display.envelope_on_top);
    });
    rollout(ui, "Advanced Parameters", false, |ui| {
        check(ui, app, index, "Always Deform", "Always Deform", d.always_deform);
        ui.horizontal(|ui| {
            ui.label("Bone Affect Limit:");
            let mut lim = d.bone_affect_limit as f64;
            if ui.add(crate::spinner::spin(ui, &mut lim, 1.0..=100.0).speed(1.0)).changed() {
                set(app, index, "Bone Affect Limit", lim.round());
            }
        });
        check(ui, app, index, "Weight All Vertices", "Weight All Vertices", d.weight_all_vertices);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Set Skin Pose").clicked() {
                let _ = app.dispatch(Command::SetSkinPose);
            }
            if ui.add_enabled(d.skin_pose.is_some(), egui::Button::new("Assume Skin Pose")).clicked() {
                let _ = app.dispatch(Command::AssumeSkinPose);
            }
            if ui.add_enabled(d.skin_pose.is_some(), egui::SelectableLabel::new(app.state.skin_pose_live.is_some(), "Skin Pose Mode")).clicked() {
                let _ = app.dispatch(Command::SkinPoseModeOnOffToggle);
            }
        });
        if ui.button("Remove Zero Weights").clicked() {
            let _ = app.dispatch(Command::RemoveZeroWeightsSkin);
        }
    });
    rollout(ui, "Gizmos", false, |ui| {
        ui.horizontal(|ui| {
            if ui.button("Add Joint Angle").clicked() {
                let _ = app.dispatch(Command::SkinAddGizmo { kind: "joint".into() });
            }
            if ui.button("Add Bulge Angle").clicked() {
                let _ = app.dispatch(Command::SkinAddGizmo { kind: "bulge".into() });
            }
            if ui.add_enabled(app.state.skin_gizmo.is_some(), egui::Button::new("Remove")).clicked() {
                let _ = app.dispatch(Command::SkinRemoveGizmo);
            }
        });
        for (i, g) in d.gizmos.iter().enumerate() {
            let label = format!("{} {}: {} ({} keys)", match g.kind { loom_scene::skin::GizmoKind::JointAngle => "Joint Angle", loom_scene::skin::GizmoKind::BulgeAngle => "Bulge Angle" }, i, d.bones.get(g.bone).and_then(|b| app.state.scene.node(b.node)).map(|n| n.name.clone()).unwrap_or_default(), g.keys.len());
            if ui.selectable_label(app.state.skin_gizmo == Some(i), label).clicked() {
                let _ = app.dispatch(Command::SkinSelectGizmo { index: i as u32 });
            }
        }
        if let Some(g) = app.state.skin_gizmo.and_then(|i| d.gizmos.get(i)) {
            let mut on = g.enabled;
            if ui.checkbox(&mut on, "Enabled").changed() {
                let _ = app.dispatch(Command::SkinSetGizmoOption { name: "Enabled".into(), value: if on { 1.0 } else { 0.0 } });
            }
            ui.horizontal(|ui| {
                ui.label("Influence:");
                let mut inf = g.influence;
                if ui.add(crate::spinner::spin(ui, &mut inf, 0.0..=10.0).speed(0.01)).changed() {
                    let _ = app.dispatch(Command::SkinSetGizmoOption { name: "Influence".into(), value: inf as f64 });
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Add Key").clicked() {
                    let _ = app.dispatch(Command::SkinAddGizmoKey);
                }
                if ui.button("Reset").clicked() {
                    let _ = app.dispatch(Command::GizmoResetResetRotationPlaneSkin);
                }
            });
            egui::Grid::new("gizmo_keys").num_columns(3).show(ui, |ui| {
                ui.label("Key");
                ui.label("Angle");
                ui.label(if g.kind == loom_scene::skin::GizmoKind::BulgeAngle { "Bulge" } else { "Deltas" });
                ui.end_row();
                for (k, key) in g.keys.iter().enumerate() {
                    ui.label(format!("{k}"));
                    let mut a = key.angle_deg;
                    if ui.add(crate::spinner::spin(ui, &mut a, -360.0..=360.0).speed(0.5)).changed() {
                        let _ = app.dispatch(Command::SkinSetGizmoKey { key: k as u32, bulge: None, angle: Some(a) });
                    }
                    if g.kind == loom_scene::skin::GizmoKind::BulgeAngle {
                        let mut b = key.bulge;
                        if ui.add(crate::spinner::spin(ui, &mut b, -2.0..=5.0).speed(0.01)).changed() {
                            let _ = app.dispatch(Command::SkinSetGizmoKey { key: k as u32, bulge: Some(b), angle: None });
                        }
                    } else {
                        ui.label(format!("{}", key.deltas.len()));
                    }
                    ui.end_row();
                }
            });
        }
    });
}
