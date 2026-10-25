//! Motion tab: Parameters (Assign Controller, PRS
//! Parameters, Key Info Basic / Advanced, constraint rollouts) and
//! Trajectories (Sub-Object Keys, Sample Range, Convert To / From,
//! Collapse). Every control dispatches a command.

use super::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::track_view::KeyArg;
use loom_commands::{Command, SelectOp};
use loom_scene::anim::TangentType;
use loom_scene::constraint::{PositionConstraint, RotationConstraint};
use loom_scene::tracks::{self, Role};
use loom_scene::NodeId;

fn run(app: &mut LoomApp, cmd: Command) {
    if let Err(e) = app.dispatch(cmd) {
        app.state.status_prompt = e;
    }
}

fn mem<T: Clone + Send + Sync + 'static>(ui: &egui::Ui, key: &str, init: T) -> T {
    ui.ctx().data_mut(|d| d.get_temp_mut_or_insert_with(egui::Id::new(key), || init).clone())
}

fn store<T: Clone + Send + Sync + 'static>(ui: &egui::Ui, key: &str, v: T) {
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(key), v));
}

const TRACKS: [&str; 3] = ["Position", "Rotation", "Scale"];

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    if ui.button("Import Animation…").clicked() {
        run(app, Command::ImportAnimationClip { path: None });
    }
    ui.separator();
    crate::kimodo::show(ui, app);
    let Some(id) = app.state.selection.iter().next().copied() else {
        ui.label("Select an object.");
        return;
    };
    if app.state.scene.node(id).is_none() {
        return;
    }
    let mut page: u8 = mem(ui, "motion_page", 0);
    ui.horizontal(|ui| {
        let w = (ui.available_width() - 8.0) / 2.0;
        if ui.add_sized([w, 22.0], egui::Button::new("Parameters").selected(page == 0)).clicked() {
            page = 0;
        }
        if ui.add_sized([w, 22.0], egui::Button::new("Trajectories").selected(page == 1)).clicked() {
            page = 1;
        }
    });
    store(ui, "motion_page", page);
    app.motion_trajectories = page == 1;
    if page == 0 {
        super::biped::motion_rollouts(ui, app, id);
        parameters(ui, app, id);
    } else {
        trajectories(ui, app);
    }
}

fn parameters(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId) {
    let mut track: usize = mem(ui, "motion_track", 0);
    let node = app.state.scene.node(id).unwrap().clone();
    rollout(ui, "Assign Controller", true, |ui| {
        ui.label(format!("{} \\ Transform : Position/Rotation/Scale", node.name));
        for (i, t) in TRACKS.iter().enumerate() {
            let name = match i {
                0 => node.constraints.position_name().map(str::to_owned).unwrap_or_else(|| tracks::AnyParam::Vec3(node.transform.position.clone()).controller_name(Role::Position)),
                1 => node.constraints.rotation_name().map(str::to_owned).unwrap_or_else(|| tracks::AnyParam::Vec3(node.transform.rotation.clone()).controller_name(Role::Rotation)),
                _ => tracks::AnyParam::Vec3(node.transform.scale.clone()).controller_name(Role::Scale),
            };
            if ui.selectable_label(track == i, format!("    {t} : {name}")).clicked() {
                track = i;
            }
        }
        if ui.button("Assign Controller").clicked() {
            app.state.track_view.highlighted = vec![format!("{}:{}", id.0, TRACKS[track])];
            run(app, Command::AssignControllerTrackView);
        }
    });
    constraint_rollouts(ui, app, id);
    rollout(ui, "PRS Parameters", true, |ui| {
        ui.columns(2, |c| {
            c[0].label("Create Key");
            c[1].label("Delete Key");
            for (i, t) in TRACKS.iter().enumerate() {
                if c[0].button(*t).clicked() {
                    run(app, [Command::KeyPosition, Command::KeyRotation, Command::KeyScale][i].clone());
                }
                if c[1].button(*t).clicked() {
                    let f = app.state.current_frame as f32;
                    let keys = (0..3).map(|comp| KeyArg { id: id.0, track: t.to_string(), comp, time: f }).collect();
                    run(app, Command::TrackViewSelectKeys { keys, op: SelectOp::Replace });
                    run(app, Command::TrackViewDeleteKeys);
                }
            }
        });
        ui.horizontal(|ui| {
            for (i, t) in TRACKS.iter().enumerate() {
                if ui.selectable_label(track == i, *t).clicked() {
                    track = i;
                }
            }
        });
    });
    store(ui, "motion_track", track);
    key_info(ui, app, id, TRACKS[track]);
}

/// Key Info for the chosen PRS track: the keys are the frames any axis is keyed at.
fn key_info(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, track: &str) {
    let Some(t) = tracks::list_tracks(&app.state.scene, id).into_iter().find(|x| x.name == track) else { return };
    let Some(p) = tracks::read(&app.state.scene, &t) else { return };
    let mut times: Vec<f32> = p.keys().map(|k| k.tracks.iter().flat_map(|tr| tr.keys.iter().map(|k| k.time)).collect()).unwrap_or_default();
    times.sort_by(f32::total_cmp);
    times.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
    let mut index: usize = mem(ui, "motion_key", 0);
    rollout(ui, "Key Info (Basic)", true, |ui| {
        if times.is_empty() {
            ui.label("No keys on this track.");
            return;
        }
        index = index.min(times.len() - 1);
        ui.horizontal(|ui| {
            if crate::icons::small(ui, crate::icons::PREVIOUS_FRAME, "Previous Key").clicked() {
                index = index.saturating_sub(1);
            }
            ui.label(format!("{}", index + 1));
            if crate::icons::small(ui, crate::icons::NEXT_FRAME, "Next Key").clicked() {
                index = (index + 1).min(times.len() - 1);
            }
        });
        let time = times[index];
        let k = p.keys().unwrap();
        let select_comp = |app: &mut LoomApp, comp: u32| run(app, Command::TrackViewSelectKeys { keys: vec![KeyArg { id: id.0, track: track.into(), comp, time }], op: SelectOp::Replace });
        let select_all = |app: &mut LoomApp| {
            let keys = (0..k.tracks.len() as u32).filter(|c| k.tracks[*c as usize].key_at(time).is_some()).map(|comp| KeyArg { id: id.0, track: track.into(), comp, time }).collect();
            run(app, Command::TrackViewSelectKeys { keys, op: SelectOp::Replace });
        };
        let mut tm = time;
        ui.horizontal(|ui| {
            ui.label("Time:");
            if ui.add(crate::spinner::spin(ui, &mut tm, ..).speed(1.0)).changed() {
                select_all(app);
                run(app, Command::TrackViewSetKeyValues { time: Some(tm), value: None, relative: Some(false) });
            }
        });
        for (c, axis) in ["X", "Y", "Z"].iter().enumerate() {
            let Some(i) = k.tracks.get(c).and_then(|tr| tr.key_at(time)) else {
                ui.label(format!("{axis} Value: (no key)"));
                continue;
            };
            let key = k.tracks[c].keys[i].clone();
            let mut v = key.value;
            ui.horizontal(|ui| {
                ui.label(format!("{axis} Value:"));
                if ui.add(crate::spinner::spin(ui, &mut v, ..).speed(0.1)).changed() {
                    select_comp(app, c as u32);
                    run(app, Command::TrackViewSetKeyValues { time: None, value: Some(v), relative: Some(false) });
                }
            });
        }
        let first = (0..k.tracks.len()).find_map(|c| k.tracks[c].key_at(time).map(|i| k.tracks[c].keys[i].clone()));
        if let Some(key) = first {
            ui.horizontal(|ui| {
                for (label, ty, out) in [("In:", key.in_type, false), ("Out:", key.out_type, true)] {
                    ui.label(label);
                    egui::ComboBox::from_id_salt(format!("motion_tan_{out}")).width(70.0).selected_text(format!("{ty:?}")).show_ui(ui, |ui| {
                        for t in [TangentType::Auto, TangentType::Custom, TangentType::Fast, TangentType::Slow, TangentType::Step, TangentType::Linear, TangentType::Smooth] {
                            if ui.selectable_label(t == ty, format!("{t:?}")).clicked() {
                                select_all(app);
                                run(app, Command::TrackViewSetKeyInfo { in_type: (!out).then_some(t), out_type: out.then_some(t), in_tan: None, out_tan: None, tension: None, continuity: None, bias: None, ease_to: None, ease_from: None });
                            }
                        }
                    });
                }
            });
            rollout(ui, "Key Info (Advanced)", false, |ui| {
                let (mut i, mut o) = (key.in_tan, key.out_tan);
                ui.horizontal(|ui| {
                    ui.label("In:");
                    if ui.add_enabled(key.in_type == TangentType::Custom, crate::spinner::spin(ui, &mut i, ..).speed(0.01)).changed() {
                        select_all(app);
                        run(app, Command::TrackViewSetKeyInfo { in_type: None, out_type: None, in_tan: Some(i), out_tan: None, tension: None, continuity: None, bias: None, ease_to: None, ease_from: None });
                    }
                    ui.label("Out:");
                    if ui.add_enabled(key.out_type == TangentType::Custom, crate::spinner::spin(ui, &mut o, ..).speed(0.01)).changed() {
                        select_all(app);
                        run(app, Command::TrackViewSetKeyInfo { in_type: None, out_type: None, in_tan: None, out_tan: Some(o), tension: None, continuity: None, bias: None, ease_to: None, ease_from: None });
                    }
                });
                if ui.button("Normalize Time").clicked() {
                    let keys = times.iter().flat_map(|tm| (0..3u32).filter(|c| k.tracks.get(*c as usize).is_some_and(|tr| tr.key_at(*tm).is_some())).map(move |comp| KeyArg { id: id.0, track: track.into(), comp, time: *tm })).collect();
                    run(app, Command::TrackViewSelectKeys { keys, op: SelectOp::Replace });
                    run(app, Command::TrackViewNormalizeTime);
                }
                let mut cv = key.constant_velocity;
                if ui.checkbox(&mut cv, "Constant Velocity").changed() {
                    select_all(app);
                    run(app, Command::TrackViewSetConstantVelocity { on: cv });
                }
            });
        }
    });
    store(ui, "motion_key", index);
}

fn target_list(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, targets: &[loom_scene::constraint::Target], rotation: bool) {
    for (i, t) in targets.iter().enumerate() {
        let name = app.state.scene.node(t.node).map(|n| n.name.clone()).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(name);
            let mut w = t.weight.get();
            if ui.add(crate::spinner::spin(ui, &mut w, 0.0..=100.0)).changed() {
                run(app, Command::SetConstraintWeight { id: id.0, rotation, index: i as u32, weight: w });
            }
            if ui.small_button("Delete").clicked() {
                run(app, Command::DeleteConstraintTarget { id: id.0, rotation, index: i as u32 });
            }
        });
    }
}

fn param_check(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, name: &str, on: bool) {
    let mut v = on;
    if ui.checkbox(&mut v, name).changed() {
        run(app, Command::SetConstraintParameter { id: id.0, name: name.into(), value: if v { 1.0 } else { 0.0 } });
    }
}

fn param_value(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, name: &str, value: f32, speed: f64) {
    ui.horizontal(|ui| {
        ui.label(format!("{name}:"));
        let mut v = value;
        if ui.add(crate::spinner::spin(ui, &mut v, ..).speed(speed)).changed() {
            run(app, Command::SetConstraintParameter { id: id.0, name: name.into(), value: v as f64 });
        }
    });
}

fn axis_row(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, axis: u32, flip: bool) {
    ui.horizontal(|ui| {
        for (i, a) in ["X", "Y", "Z"].iter().enumerate() {
            if ui.radio(axis == i as u32, *a).clicked() {
                run(app, Command::SetConstraintParameter { id: id.0, name: "Axis".into(), value: i as f64 });
            }
        }
        param_check(ui, app, id, "Flip", flip);
    });
}

fn pick_button(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, kind: &str) {
    if ui.button(label).clicked() {
        run(app, Command::StartConstraintPick { kind: kind.into() });
    }
}

fn constraint_rollouts(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId) {
    let c = app.state.scene.node(id).unwrap().constraints.clone();
    match &c.position {
        Some(PositionConstraint::Path(p)) => rollout(ui, "Path Parameters", true, |ui| {
            pick_button(ui, app, "Add Path", "Path Constraint");
            target_list(ui, app, id, &p.targets, false);
            param_value(ui, app, id, "% Along Path", p.percent.get(), 0.5);
            for (n, v) in [("Follow", p.follow), ("Bank", p.bank)] {
                param_check(ui, app, id, n, v);
            }
            param_value(ui, app, id, "Bank Amount", p.bank_amount, 0.05);
            param_value(ui, app, id, "Smoothness", p.smoothness, 0.05);
            for (n, v) in [("Allow Upside Down", p.allow_upside_down), ("Constant Velocity", p.constant_velocity), ("Loop", p.loop_path), ("Relative", p.relative)] {
                param_check(ui, app, id, n, v);
            }
            axis_row(ui, app, id, p.axis, p.flip);
        }),
        Some(PositionConstraint::Position { targets, keep_offset, .. }) => rollout(ui, "Position Constraint", true, |ui| {
            pick_button(ui, app, "Add Position Target", "Position Constraint");
            target_list(ui, app, id, targets, false);
            param_check(ui, app, id, "Keep Initial Offset", *keep_offset);
        }),
        Some(PositionConstraint::Attachment { target, face, a, b, align }) => rollout(ui, "Attachment Parameters", true, |ui| {
            ui.label(format!("Object: {}", app.state.scene.node(*target).map(|n| n.name.clone()).unwrap_or_default()));
            pick_button(ui, app, "Pick Object", "Attachment Constraint");
            param_check(ui, app, id, "Align to Surface", *align);
            param_value(ui, app, id, "Face", *face as f32, 1.0);
            param_value(ui, app, id, "A", *a, 0.01);
            param_value(ui, app, id, "B", *b, 0.01);
        }),
        None => {}
    }
    match &c.rotation {
        Some(RotationConstraint::LookAt { targets, axis, flip, keep_offset, .. }) => rollout(ui, "LookAt Constraint", true, |ui| {
            pick_button(ui, app, "Add LookAt Target", "LookAt Constraint");
            target_list(ui, app, id, targets, true);
            param_check(ui, app, id, "Keep Initial Offset", *keep_offset);
            ui.label("Select LookAt Axis:");
            axis_row(ui, app, id, *axis, *flip);
        }),
        Some(RotationConstraint::Orientation { targets, keep_offset, .. }) => rollout(ui, "Orientation Constraint", true, |ui| {
            pick_button(ui, app, "Add Orientation Target", "Orientation Constraint");
            target_list(ui, app, id, targets, true);
            param_check(ui, app, id, "Keep Initial Offset", *keep_offset);
        }),
        None => {}
    }
    if let Some(l) = &c.link {
        rollout(ui, "Link Params", true, |ui| {
            pick_button(ui, app, "Add Link", "Link Constraint");
            if ui.button("Link to World").clicked() {
                run(app, Command::LinkConstraint { target: None });
            }
            for (i, (f, t)) in l.links.iter().enumerate() {
                let name = t.and_then(|t| app.state.scene.node(t).map(|n| n.name.clone())).unwrap_or_else(|| "World".into());
                ui.horizontal(|ui| {
                    ui.label(format!("{name}  {f}"));
                    if ui.small_button("Delete Link").clicked() {
                        run(app, Command::DeleteLink { id: id.0, index: i as u32 });
                    }
                });
            }
        });
    }
}

fn trajectories(ui: &mut egui::Ui, app: &mut LoomApp) {
    let (s0, s1) = app.state.frame_range;
    let mut range: (i32, i32, u32) = mem(ui, "motion_sample", (s0, s1, 10));
    let mut collapse: (bool, bool, bool) = mem(ui, "motion_collapse", (true, true, true));
    rollout(ui, "Trajectories", true, |ui| {
        ui.horizontal(|ui| {
            if ui.button("Delete Key").clicked() {
                let frames: Vec<i32> = app.state.trajectory_selected.iter().copied().collect();
                let ids: Vec<NodeId> = app.state.selection.iter().copied().collect();
                let keys: Vec<KeyArg> = ids.iter().flat_map(|id| frames.iter().flat_map(move |f| (0..3).map(move |comp| KeyArg { id: id.0, track: "Position".into(), comp, time: *f as f32 }))).collect();
                if !keys.is_empty() {
                    run(app, Command::TrackViewSelectKeys { keys, op: SelectOp::Replace });
                    run(app, Command::TrackViewDeleteKeys);
                    run(app, Command::TrajectorySelectKeys { frames: Vec::new(), op: SelectOp::Replace });
                }
            }
            if ui.button("Add Key").clicked() {
                run(app, Command::KeyPosition);
            }
        });
        if ui.add(egui::Button::new("Sub-Object: Keys").selected(app.state.trajectory_keys)).clicked() {
            run(app, Command::TrajectorySubObjectKeys);
        }
        ui.separator();
        ui.label("Sample Range");
        ui.horizontal(|ui| {
            ui.label("Start Time:");
            ui.add(crate::spinner::spin(ui, &mut range.0, ..));
        });
        ui.horizontal(|ui| {
            ui.label("End Time:");
            ui.add(crate::spinner::spin(ui, &mut range.1, ..));
        });
        ui.horizontal(|ui| {
            ui.label("Samples:");
            ui.add(crate::spinner::spin(ui, &mut range.2, 1..=10000));
        });
        ui.label("Spline Conversion");
        ui.horizontal(|ui| {
            if ui.button("Convert To").clicked() {
                run(app, Command::TrajectoryConvertTo { start: Some(range.0), end: Some(range.1), samples: Some(range.2) });
            }
            let shapes: Vec<(u64, String)> = app.state.scene.nodes().filter(|n| n.object.shape().is_some()).map(|n| (n.id.0, n.name.clone())).collect();
            ui.menu_button("Convert From", |ui| {
                if shapes.is_empty() {
                    ui.label("No shapes in the scene");
                }
                for (sid, name) in shapes {
                    if ui.button(name).clicked() {
                        run(app, Command::TrajectoryConvertFrom { shape: sid, start: Some(range.0), end: Some(range.1), samples: Some(range.2) });
                        ui.close_menu();
                    }
                }
            });
        });
        ui.label("Collapse Transform");
        if ui.button("Collapse").clicked() {
            run(app, Command::TrajectoryCollapse { start: Some(range.0), end: Some(range.1), samples: Some(range.2), position: Some(collapse.0), rotation: Some(collapse.1), scale: Some(collapse.2) });
        }
        ui.horizontal(|ui| {
            ui.checkbox(&mut collapse.0, "Position");
            ui.checkbox(&mut collapse.1, "Rotation");
            ui.checkbox(&mut collapse.2, "Scale");
        });
    });
    store(ui, "motion_sample", range);
    store(ui, "motion_collapse", collapse);
}
