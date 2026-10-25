//! Skin modifier rollouts: Parameters, Mirror Parameters,
//! Display, Advanced Parameters, Gizmos and the deformer parameters.

use super::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;
use loom_scene::skin::GizmoKind;
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

fn run(app: &mut LoomApp, cmd: Command) {
    if let Err(e) = app.dispatch(cmd) {
        app.state.status_prompt = e;
    }
}

fn envelope(app: &mut LoomApp, inner: Option<f32>, outer: Option<f32>, falloff: Option<u32>, abs_effect: Option<f32>, squash: Option<f32>) {
    let _ = app.dispatch(Command::SkinSetEnvelope { inner_start: inner, inner_end: inner, outer_start: outer, outer_end: outer, falloff, abs_effect, squash });
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
        ui.horizontal_wrapped(|ui| {
            let mut el = app.state.skin_ui.select_element;
            if ui.checkbox(&mut el, "Select Element").changed() {
                let _ = app.dispatch(Command::SkinSelectElementToggle);
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
            // Add waits for a click on the bone in a viewport.
            if ui.selectable_label(app.state.skin_ui.adding_cross_section, "Add").clicked() {
                run(app, Command::AddCrossSectionSkin);
            }
            if ui.add_enabled(app.state.skin_ui.cross_section.is_some(), egui::Button::new("Remove")).clicked() {
                run(app, Command::RemoveCrossSectionSkin);
            }
        });
        if let Some(b) = current.and_then(|c| d.bones.iter().find(|b| b.node == c)) {
            ui.label("Envelope Properties");
            let e = b.envelope.clone();
            let (inner, outer) = e.radii();
            let speed = app.spinner_speed(0.1);
            egui::Grid::new("skin_env").num_columns(4).show(ui, |ui| {
                ui.label("Radius:");
                match app.state.skin_ui.cross_section {
                    // The selected cross section's ring.
                    Some((k, ring_outer)) => {
                        let sections = b.sections();
                        let mut r = sections.get(k).map(|s| if ring_outer { s.2 } else { s.1 }).unwrap_or(0.0);
                        if ui.add(crate::spinner::spin(ui, &mut r, 0.0..=1e6).speed(speed)).changed() {
                            run(app, Command::SkinSetCrossSectionRadius { radius: r });
                        }
                        ui.label(if ring_outer { "outer" } else { "inner" });
                    }
                    None => {
                        let mut i0 = inner[0];
                        if ui.add(crate::spinner::spin(ui, &mut i0, 0.0..=1e6).speed(speed)).changed() {
                            envelope(app, Some(i0), None, None, None, None);
                        }
                        let mut o0 = outer[0];
                        if ui.add(crate::spinner::spin(ui, &mut o0, 0.0..=1e6).speed(speed)).changed() {
                            envelope(app, None, Some(o0), None, None, None);
                        }
                        ui.label("inner / outer");
                    }
                }
                ui.end_row();
                ui.label("Squash:");
                let mut sq = e.squash;
                if ui.add(crate::spinner::spin(ui, &mut sq, 0.0..=10.0).speed(speed)).changed() {
                    envelope(app, None, None, None, None, Some(sq));
                }
                ui.end_row();
            });
            ui.horizontal(|ui| {
                // Absolute / Relative.
                if ui.selectable_label(!e.relative, "A").on_hover_text("Absolute").clicked() {
                    run(app, Command::SkinSetEnvelopeMode { relative: false });
                }
                if ui.selectable_label(e.relative, "R").on_hover_text("Relative").clicked() {
                    run(app, Command::SkinSetEnvelopeMode { relative: true });
                }
                ui.separator();
                ui.label("Falloff:");
                for (i, l) in ["Linear", "Sinual", "Fast Out", "Slow Out"].iter().enumerate() {
                    if ui.selectable_label(e.falloff == i as u32, *l).clicked() {
                        envelope(app, None, None, Some(i as u32), None, None);
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
                // The Paste flyout: Paste, Paste to All Bones, Paste to Multiple Bones.
                if ui.button("Paste").clicked() {
                    run(app, Command::PasteEnvelopeSkin);
                }
                if ui.button("Paste to All Bones").clicked() {
                    run(app, Command::PasteToAllEnvelopesSkin);
                }
                if ui.button("Paste to Multiple Bones").clicked() {
                    run(app, Command::PasteToMultipleBonesSkin);
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
                        envelope(app, None, None, None, Some(ab), None);
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                let rigid = sel.iter().all(|v| d.rigid.get(*v as usize).copied().unwrap_or(false)) && !sel.is_empty();
                if ui.selectable_label(rigid, "Rigid").clicked() {
                    let _ = app.dispatch(Command::SkinRigidToggle);
                }
                // Rigid Handles: patch vertices only.
                let patch = loom_commands::skin::input_patch(&app.state);
                let nv = patch.as_ref().map_or(0, |p| p.verts.len());
                let rigid_h = !sel.is_empty() && sel.iter().all(|v| (*v as usize) < nv && d.rigid_handles.get(*v as usize).copied().unwrap_or(false));
                if ui.add_enabled(patch.is_some(), egui::SelectableLabel::new(rigid_h, "Rigid Handles")).clicked() {
                    run(app, Command::SkinRigidHandlesToggle);
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
        ui.horizontal_wrapped(|ui| {
            for (label, cmd) in [("Mirror Paste", Command::MirrorPaste), ("Paste Green to Blue Bones", Command::PasteGreenToBlueBones), ("Paste Blue to Green Bones", Command::PasteBlueToGreenBones), ("Paste Green to Blue Verts", Command::PasteGreenToBlue), ("Paste Blue to Green Verts", Command::PasteBlueToGreen)] {
                if ui.add_enabled(d.mirror.mode, egui::Button::new(label)).clicked() {
                    run(app, cmd);
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Mirror Plane:");
            for (i, l) in ["X", "Y", "Z"].iter().enumerate() {
                if ui.selectable_label(d.mirror.plane == i as u32, *l).clicked() {
                    set(app, index, "Mirror Plane", i as f64);
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
        ui.horizontal(|ui| {
            ui.label("Display Projection:");
            const PROJ: [&str; 4] = ["Default Display", "Positive", "Negative", "None"];
            let mut p = d.mirror.projection as usize;
            egui::ComboBox::from_id_salt("skin_display_projection").selected_text(PROJ[p.min(3)]).show_ui(ui, |ui| {
                for (k, l) in PROJ.iter().enumerate() {
                    ui.selectable_value(&mut p, k, *l);
                }
            });
            if p as u32 != d.mirror.projection {
                set(app, index, "Display Projection", p as f64);
            }
        });
        ui.horizontal(|ui| {
            check(ui, app, index, "Manual Update", "Manual Update", d.mirror.manual_update);
            if ui.add_enabled(d.mirror.manual_update, egui::Button::new("Update")).clicked() {
                run(app, Command::SkinMirrorUpdate);
            }
        });
    });
    rollout(ui, "Display", false, |ui| {
        check(ui, app, index, "Show Colored Vertices", "Show Colored Vertices", d.display.colored_vertices);
        check(ui, app, index, "Show Colored Faces", "Show Colored Faces", d.display.colored_faces);
        check(ui, app, index, "Color All Weights", "Color All Weights", d.display.color_all_weights);
        check(ui, app, index, "Show All Envelopes", "Show All Envelopes", d.display.all_envelopes);
        check(ui, app, index, "Show All Vertices", "Show All Vertices", d.display.all_vertices);
        check(ui, app, index, "Show All Gizmos", "Show All Gizmos", d.display.all_gizmos);
        check(ui, app, index, "Show No Envelopes", "Show No Envelopes", d.display.no_envelopes);
        check(ui, app, index, "Show Hidden Vertices", "Show Hidden Vertices", d.display.show_hidden_vertices);
        ui.label("Draw On Top");
        check(ui, app, index, "Draw CrossSections On Top", "Cross Sections", d.display.cross_sections_on_top);
        check(ui, app, index, "Draw Envelope On Top", "Envelopes", d.display.envelope_on_top);
    });
    rollout(ui, "Advanced Parameters", false, |ui| {
        check(ui, app, index, "Always Deform", "Always Deform", d.always_deform);
        ui.horizontal(|ui| {
            ui.label("Ref. Frame:");
            let mut rf = d.ref_frame;
            if ui.add(crate::spinner::spin(ui, &mut rf, -100000.0..=100000.0).speed(1.0)).changed() {
                set(app, index, "Ref. Frame", rf.round() as f64);
            }
        });
        check(ui, app, index, "Back Transform Vertices", "Back Transform Vertices", d.back_transform);
        check(ui, app, index, "Rigid Vertices (All)", "Rigid Vertices (All)", d.rigid_all);
        check(ui, app, index, "Rigid Patch Handles (All)", "Rigid Patch Handles (All)", d.rigid_patch_handles_all);
        ui.horizontal(|ui| {
            ui.label("Bone Affect Limit:");
            let mut lim = d.bone_affect_limit as f64;
            if ui.add(crate::spinner::spin(ui, &mut lim, 1.0..=100.0).speed(1.0)).changed() {
                set(app, index, "Bone Affect Limit", lim.round());
            }
        });
        ui.label("Reset");
        ui.horizontal_wrapped(|ui| {
            for (label, cmd) in [("Reset Selected Verts", Command::SkinResetSelectedVerts), ("Reset Selected Bone", Command::SkinResetSelectedBone), ("Reset All Bones", Command::SkinResetAllBones)] {
                if ui.button(label).clicked() {
                    run(app, cmd);
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                run(app, Command::SkinSaveEnvelopes { path: None });
            }
            if ui.button("Load").clicked() {
                run(app, Command::SkinLoadEnvelopes { path: None });
            }
        });
        check(ui, app, index, "Update On Mouse Up", "Update on mouse up", d.update_on_mouse_up);
        check(ui, app, index, "Fast Updates", "Fast Updates", d.fast_updates);
        check(ui, app, index, "Ignore Bone Scale", "Ignore Bone Scale", d.ignore_bone_scale);
        check(ui, app, index, "Animatable Envelopes", "Animatable Envelopes", d.animatable_envelopes);
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
        ui.horizontal(|ui| {
            if ui.button("Remove Zero Weights").clicked() {
                let _ = app.dispatch(Command::RemoveZeroWeightsSkin);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Remove Zero Limit:");
            let mut z = d.remove_zero_limit;
            if ui.add(crate::spinner::spin(ui, &mut z, 0.0..=1.0).speed(0.001).fixed_decimals(3)).changed() {
                set(app, index, "Remove Zero Limit", z as f64);
            }
        });
    });
    gizmos(ui, app, &d);
}

/// The Gizmos rollout (list, deformer drop-down, Add / Remove / Copy /
/// Paste) and the current gizmo's parameters.
fn gizmos(ui: &mut egui::Ui, app: &mut LoomApp, d: &loom_scene::skin::SkinData) {
    let kind_id = egui::Id::new("skin_gizmo_kind");
    rollout(ui, "Gizmos", false, |ui| {
        for (i, g) in d.gizmos.iter().enumerate() {
            let bone = d.bones.get(g.bone).and_then(|b| app.state.scene.node(b.node)).map(|n| n.name.clone()).unwrap_or_default();
            let label = if g.name.is_empty() { format!("{} {}: {}", g.kind.label(), i, bone) } else { format!("{}: {}", g.name, bone) };
            if ui.selectable_label(app.state.skin_gizmo == Some(i), label).clicked() {
                let _ = app.dispatch(Command::SkinSelectGizmo { index: i as u32 });
            }
        }
        let mut kind: u8 = ui.data(|x| x.get_temp(kind_id)).unwrap_or(0);
        const KINDS: [(GizmoKind, &str); 3] = [(GizmoKind::JointAngle, "joint"), (GizmoKind::BulgeAngle, "bulge"), (GizmoKind::MorphAngle, "morph")];
        egui::ComboBox::from_id_salt("skin_gizmo_kinds").selected_text(KINDS[kind as usize % 3].0.label()).show_ui(ui, |ui| {
            for (k, (g, _)) in KINDS.iter().enumerate() {
                ui.selectable_value(&mut kind, k as u8, g.label());
            }
        });
        ui.data_mut(|x| x.insert_temp(kind_id, kind));
        ui.horizontal(|ui| {
            if ui.button("Add Gizmo").clicked() {
                run(app, Command::SkinAddGizmo { kind: KINDS[kind as usize % 3].1.into() });
            }
            let on = app.state.skin_gizmo.is_some();
            if ui.add_enabled(on, egui::Button::new("Remove Gizmo")).clicked() {
                run(app, Command::SkinRemoveGizmo);
            }
            if ui.add_enabled(on, egui::Button::new("Copy Gizmo")).clicked() {
                run(app, Command::SkinCopyGizmo);
            }
            if ui.add_enabled(on && app.state.skin_ui.clipboard_gizmo.is_some(), egui::Button::new("Paste Gizmo")).clicked() {
                run(app, Command::SkinPasteGizmo);
            }
        });
    });
    let Some(gi) = app.state.skin_gizmo else { return };
    let Some(g) = d.gizmos.get(gi).cloned() else { return };
    if g.kind == GizmoKind::MorphAngle {
        rollout(ui, "Deformer Parameters", true, |ui| {
            let bone = d.bones.get(g.bone).and_then(|b| app.state.scene.node(b.node)).map(|n| n.name.clone()).unwrap_or_default();
            ui.label(format!("{}: {bone}", g.kind.label()));
            let sel_id = egui::Id::new(("skin_morph_target", gi));
            let mut sel: usize = ui.data(|x| x.get_temp(sel_id)).unwrap_or(0).min(g.keys.len().saturating_sub(1));
            egui::ScrollArea::vertical().max_height(100.0).id_salt("skin_morph_targets").show(ui, |ui| {
                for (k, key) in g.keys.iter().enumerate() {
                    if ui.selectable_label(sel == k, format!("{}  {:.1}°", key.name, key.angle_deg)).clicked() {
                        sel = k;
                    }
                }
            });
            ui.data_mut(|x| x.insert_temp(sel_id, sel));
            if let Some(key) = g.keys.get(sel) {
                let mut name = key.name.clone();
                if ui.add(egui::TextEdit::singleline(&mut name).desired_width(ui.available_width())).changed() {
                    run(app, Command::SkinGizmoRenameTarget { key: sel as u32, name });
                }
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button("Add From Stack").clicked() {
                    run(app, Command::SkinGizmoAddFromStack);
                }
                // Add From Node: another object of the same vertex count is the target.
                let me = app.state.skin_entry().map(|(id, _)| id);
                let nodes: Vec<(u64, String)> = app.state.scene.nodes().filter(|n| Some(n.id) != me && !n.object.is_bone() && n.object.shape().is_none()).map(|n| (n.id.0, n.name.clone())).collect();
                egui::ComboBox::from_id_salt("skin_morph_add_from_node").selected_text("Add From Node").show_ui(ui, |ui| {
                    for (nid, name) in nodes {
                        if ui.selectable_label(false, name).clicked() {
                            run(app, Command::SkinGizmoAddFromNode { id: nid });
                        }
                    }
                });
                if ui.add_enabled(g.keys.len() > 1, egui::Button::new("Delete")).clicked() {
                    run(app, Command::SkinGizmoDeleteTarget { key: sel as u32 });
                }
            });
            let mut on = g.enabled;
            if ui.checkbox(&mut on, "Enable Gizmo").changed() {
                let _ = app.dispatch(Command::SkinSetGizmoOption { name: "Enabled".into(), value: if on { 1.0 } else { 0.0 } });
            }
        });
        return;
    }
    rollout(ui, "Gizmo Parameters", true, |ui| {
        let mut name = g.name.clone();
        if ui.add(egui::TextEdit::singleline(&mut name).desired_width(ui.available_width())).changed() {
            run(app, Command::SkinRenameGizmo { name });
        }
        ui.horizontal(|ui| {
            ui.label("Twist:");
            let mut t = g.twist;
            if ui.add(crate::spinner::spin(ui, &mut t, -360.0..=360.0).speed(0.5)).changed() {
                let _ = app.dispatch(Command::SkinSetGizmoOption { name: "Twist".into(), value: t as f64 });
            }
        });
        let mut bv = g.bounding_volume;
        if ui.checkbox(&mut bv, "Use Bounding Volume").changed() {
            let _ = app.dispatch(Command::SkinSetGizmoOption { name: "Use Bounding Volume".into(), value: if bv { 1.0 } else { 0.0 } });
        }
        let mut on = g.enabled;
        if ui.checkbox(&mut on, "Enable Gizmo").changed() {
            let _ = app.dispatch(Command::SkinSetGizmoOption { name: "Enabled".into(), value: if on { 1.0 } else { 0.0 } });
        }
        ui.horizontal(|ui| {
            ui.label("Influence:");
            let mut inf = g.influence;
            if ui.add(crate::spinner::spin(ui, &mut inf, 0.0..=10.0).speed(0.01)).changed() {
                let _ = app.dispatch(Command::SkinSetGizmoOption { name: "Influence".into(), value: inf as f64 });
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(g.lattice.is_some(), egui::SelectableLabel::new(app.state.skin_ui.edit_lattice, "Edit Lattice")).clicked() {
                run(app, Command::SkinEditLatticeToggle);
            }
            if ui.add_enabled(g.lattice.is_some(), egui::Button::new("Edit Angle Keys Curves")).clicked() {
                run(app, Command::SkinAngleKeysCurvesDialog);
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
            ui.label(if g.kind == GizmoKind::BulgeAngle { "Bulge" } else { "Deltas" });
            ui.end_row();
            for (k, key) in g.keys.iter().enumerate() {
                ui.label(format!("{k}"));
                let mut a = key.angle_deg;
                if ui.add(crate::spinner::spin(ui, &mut a, -360.0..=360.0).speed(0.5)).changed() {
                    let _ = app.dispatch(Command::SkinSetGizmoKey { key: k as u32, bulge: None, angle: Some(a) });
                }
                if g.kind == GizmoKind::BulgeAngle {
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
    });
}
