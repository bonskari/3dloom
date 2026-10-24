//! Track View dialogs: Parameter Curve Out-of-Range Types, Reduce Keys,
//! Track Sets Editor, Assign Controller, Key Info, Filters, Track View
//! Utilities. Every control dispatches a command.

use crate::LoomApp;
use eframe::egui;
use loom_commands::track_view::{first_selected_key, highlighted_tracks};
use loom_commands::{Command, SelectOp};
use loom_scene::anim::{Interp, Ort, TangentType};
use loom_scene::tracks::{self, Role};

const ORTS: [(Ort, &str); 6] = [(Ort::Constant, "Constant"), (Ort::Cycle, "Cycle"), (Ort::Loop, "Loop"), (Ort::PingPong, "Ping Pong"), (Ort::Linear, "Linear"), (Ort::RelativeRepeat, "Relative Repeat")];
const TANGENTS: [(TangentType, &str); 7] = [(TangentType::Auto, "Auto"), (TangentType::Custom, "Custom"), (TangentType::Fast, "Fast"), (TangentType::Slow, "Slow"), (TangentType::Step, "Step"), (TangentType::Linear, "Linear"), (TangentType::Smooth, "Smooth")];

fn mem<T: Clone + Send + Sync + 'static>(ctx: &egui::Context, key: &str, init: T) -> T {
    ctx.data_mut(|d| d.get_temp_mut_or_insert_with(egui::Id::new(key), || init).clone())
}

fn store<T: Clone + Send + Sync + 'static>(ctx: &egui::Context, key: &str, v: T) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(key), v));
}

fn report(app: &mut LoomApp, r: Result<serde_json::Value, String>) {
    if let Err(e) = r {
        app.state.status_prompt = e;
    }
}

pub fn out_of_range_types(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let cur = highlighted_tracks(&app.state).into_iter().find_map(|t| tracks::read(&app.state.scene, &t).and_then(|p| p.keys().and_then(|k| k.tracks.first().map(|tr| (tr.before, tr.after)))));
    let (mut before, mut after) = mem(ctx, "tv_ort", cur.unwrap_or((Ort::Constant, Ort::Constant)));
    let mut close = false;
    egui::Window::new("Param Curve Out-of-Range Types").open(open).resizable(false).show(ctx, |ui| {
        ui.horizontal_wrapped(|ui| {
            for (o, label) in ORTS {
                ui.vertical(|ui| {
                    ui.label(label);
                    ui.horizontal(|ui| {
                        if ui.selectable_label(before == o, "<").on_hover_text("In (before the range)").clicked() {
                            before = o;
                        }
                        if ui.selectable_label(after == o, ">").on_hover_text("Out (after the range)").clicked() {
                            after = o;
                        }
                    });
                });
            }
        });
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                let r = app.dispatch(Command::TrackViewSetOrt { before, after });
                report(app, r);
                close = true;
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
        });
    });
    store(ctx, "tv_ort", (before, after));
    if close {
        *open = false;
    }
}

pub fn reduce_keys(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let mut threshold: f32 = mem(ctx, "tv_reduce", 0.5);
    let mut close = false;
    egui::Window::new("Reduce Keys").open(open).resizable(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Threshold:");
            ui.add(crate::spinner::spin(ui, &mut threshold, 0.0..=1000.0).speed(0.05));
        });
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                let r = app.dispatch(Command::TrackViewReduceKeys { threshold });
                report(app, r);
                close = true;
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
        });
    });
    store(ctx, "tv_reduce", threshold);
    if close {
        *open = false;
    }
}

pub fn track_sets(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let mut name: String = mem(ctx, "tv_set_name", String::new());
    let mut current: Option<String> = mem(ctx, "tv_set_current", None);
    let sets = app.state.scene.track_data.track_sets.clone();
    egui::Window::new("Track Sets Editor").open(open).resizable(true).default_width(320.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut name);
            if ui.button("Create").on_hover_text("Create a set from the highlighted tracks").clicked() && !name.trim().is_empty() {
                let r = app.dispatch(Command::TrackSetCreate { name: name.trim().to_owned() });
                report(app, r);
                current = Some(name.trim().to_owned());
            }
        });
        ui.horizontal(|ui| {
            let cur = current.clone();
            let enabled = cur.is_some();
            if ui.add_enabled(enabled, egui::Button::new("Delete")).clicked() {
                let r = app.dispatch(Command::TrackSetDelete { name: cur.clone().unwrap() });
                report(app, r);
                current = None;
            }
            if ui.add_enabled(enabled, egui::Button::new("Add Highlighted")).clicked() {
                let r = app.dispatch(Command::TrackSetAddHighlighted { name: cur.clone().unwrap() });
                report(app, r);
            }
            if ui.add_enabled(enabled, egui::Button::new("Remove Highlighted")).clicked() {
                let r = app.dispatch(Command::TrackSetRemoveHighlighted { name: cur.clone().unwrap() });
                report(app, r);
            }
            if ui.add_enabled(enabled, egui::Button::new("Highlight")).clicked() {
                let r = app.dispatch(Command::TrackSetSelect { name: cur.unwrap(), op: SelectOp::Replace });
                report(app, r);
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            for (n, members) in &sets {
                let sel = current.as_deref() == Some(n.as_str());
                if ui.selectable_label(sel, format!("{n} ({})", members.len())).clicked() {
                    current = Some(n.clone());
                }
                if sel {
                    for m in members {
                        let label = tracks::list_tracks(&app.state.scene, m.node).into_iter().find(|t| t.key() == *m).map(|t| format!("{} / {}", app.state.scene.node(m.node).map(|n| n.name.clone()).unwrap_or_default(), t.name)).unwrap_or_else(|| "(missing)".into());
                        ui.label(format!("    {label}"));
                    }
                }
            }
        });
    });
    store(ctx, "tv_set_name", name);
    store(ctx, "tv_set_current", current);
}

pub fn assign_controller(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let tracks_now = highlighted_tracks(&app.state);
    let Some(first) = tracks_now.first().cloned() else {
        *open = false;
        return;
    };
    let role = Role::of(&first);
    let choices = role.choices();
    let current = tracks::read(&app.state.scene, &first).map(|p| p.controller_name(role)).unwrap_or_default();
    let mut pick: String = mem(ctx, "tv_assign_pick", current.clone());
    if !choices.contains(&pick) {
        pick = choices[0].clone();
    }
    let mut close = false;
    egui::Window::new(format!("Assign {} Controller", match role { Role::Position => "Position", Role::Rotation => "Rotation", Role::Scale => "Scale", _ => "Float" })).open(open).resizable(false).show(ctx, |ui| {
        ui.label(format!("Track: {}", first.name));
        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
            for c in &choices {
                let label = if *c == current { format!("{c}  (current)") } else { c.clone() };
                if ui.selectable_label(pick == *c, label).clicked() {
                    pick = c.clone();
                }
            }
        });
        ui.label(format!("Default: {}", choices[0]));
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                for t in &tracks_now {
                    let r = app.dispatch(Command::AssignController { id: t.node.0, track: t.name.clone(), controller: pick.clone() });
                    report(app, r);
                }
                close = true;
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
        });
    });
    store(ctx, "tv_assign_pick", pick);
    if close {
        *open = false;
    }
}

fn tangent_combo(ui: &mut egui::Ui, id: &str, t: TangentType) -> Option<TangentType> {
    let mut out = None;
    egui::ComboBox::from_id_salt(id).width(80.0).selected_text(TANGENTS.iter().find(|(x, _)| *x == t).map(|(_, l)| *l).unwrap_or("Auto")).show_ui(ui, |ui| {
        for (x, l) in TANGENTS {
            if ui.selectable_label(x == t, l).clicked() {
                out = Some(x);
            }
        }
    });
    out
}

pub fn key_info(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let Some((track, comp, index, key, interp)) = first_selected_key(&app.state) else {
        egui::Window::new("Key Info").open(open).resizable(false).show(ctx, |ui| {
            ui.label("Select a key in the Track View.");
        });
        return;
    };
    let node = app.state.scene.node(track.node).map(|n| n.name.clone()).unwrap_or_default();
    egui::Window::new(format!("{node}\\{}", track.name)).open(open).resizable(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(format!("Key # {}", index + 1));
            if track.ty == loom_scene::param::ParamType::Vec3 {
                ui.label(["X", "Y", "Z"][comp as usize % 3]);
            }
        });
        let (mut t, mut v) = (key.time, key.value);
        ui.horizontal(|ui| {
            ui.label("Time:");
            if ui.add(crate::spinner::spin(ui, &mut t, ..).speed(1.0)).changed() {
                let r = app.dispatch(Command::TrackViewSetKeyValues { time: Some(t), value: None, relative: Some(false) });
                report(app, r);
            }
            ui.label("Value:");
            if ui.add(crate::spinner::spin(ui, &mut v, ..).speed(0.1)).changed() {
                let r = app.dispatch(Command::TrackViewSetKeyValues { time: None, value: Some(v), relative: Some(false) });
                report(app, r);
            }
        });
        let set = |app: &mut LoomApp, c: Command| {
            let r = app.dispatch(c);
            report(app, r);
        };
        let info = |in_type, out_type, in_tan, out_tan, tension, continuity, bias, ease_to, ease_from| Command::TrackViewSetKeyInfo { in_type, out_type, in_tan, out_tan, tension, continuity, bias, ease_to, ease_from };
        match interp {
            Interp::Bezier => {
                ui.horizontal(|ui| {
                    ui.label("In:");
                    if let Some(x) = tangent_combo(ui, "ki_in", key.in_type) {
                        set(app, info(Some(x), None, None, None, None, None, None, None, None));
                    }
                    ui.label("Out:");
                    if let Some(x) = tangent_combo(ui, "ki_out", key.out_type) {
                        set(app, info(None, Some(x), None, None, None, None, None, None, None));
                    }
                });
                ui.separator();
                ui.label(egui::RichText::new("Advanced").strong());
                let (mut i, mut o) = (key.in_tan, key.out_tan);
                ui.horizontal(|ui| {
                    ui.label("In:");
                    if ui.add_enabled(key.in_type == TangentType::Custom, crate::spinner::spin(ui, &mut i, ..).speed(0.01)).changed() {
                        set(app, info(None, None, Some(i), if key.broken { None } else { Some(i) }, None, None, None, None, None));
                    }
                    ui.label("Out:");
                    if ui.add_enabled(key.out_type == TangentType::Custom, crate::spinner::spin(ui, &mut o, ..).speed(0.01)).changed() {
                        set(app, info(None, None, if key.broken { None } else { Some(o) }, Some(o), None, None, None, None, None));
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Normalize Time").on_hover_text("Even out the selected keys' timing").clicked() {
                        set(app, Command::TrackViewNormalizeTime);
                    }
                    let mut cv = key.constant_velocity;
                    if ui.checkbox(&mut cv, "Constant Velocity").changed() {
                        set(app, Command::TrackViewSetConstantVelocity { on: cv });
                    }
                });
            }
            Interp::Tcb => {
                let mut vals = [key.tension, key.continuity, key.bias, key.ease_to, key.ease_from];
                for (i, l) in ["Tension:", "Continuity:", "Bias:", "Ease To:", "Ease From:"].iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(*l);
                        if ui.add(crate::spinner::spin(ui, &mut vals[i], 0.0..=50.0).speed(0.5)).changed() {
                            let mut o = [None; 5];
                            o[i] = Some(vals[i]);
                            set(app, info(None, None, None, None, o[0], o[1], o[2], o[3], o[4]));
                        }
                    });
                }
            }
            Interp::Linear => {
                ui.label("Linear keys have no tangents.");
            }
        }
    });
}

pub fn filters(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let f = app.state.track_view.filters.clone();
    egui::Window::new("Filters").open(open).resizable(false).show(ctx, |ui| {
        ui.columns(2, |cols| {
            cols[0].label(egui::RichText::new("Show").strong());
            for (name, on) in [("Transforms", f.transforms), ("Base Objects", f.base_objects), ("Modified Objects", f.modified_objects), ("Note Tracks", f.note_tracks), ("Visibility Tracks", f.visibility_tracks), ("Controller Types", f.controller_types)] {
                let mut v = on;
                if cols[0].checkbox(&mut v, name).changed() {
                    let r = app.dispatch(Command::TrackViewFilter { name: name.into(), on: v });
                    report(app, r);
                }
            }
            cols[1].label(egui::RichText::new("Show Only").strong());
            for (name, on) in [("Animated Tracks", f.animated_only), ("Selected Objects", f.selected_objects), ("Selected Tracks", f.selected_tracks), ("Visible Objects", f.visible_objects), ("Keyable Tracks", f.keyable_only), ("Unlocked Attributes", f.unlocked_only)] {
                let mut v = on;
                if cols[1].checkbox(&mut v, name).changed() {
                    let r = app.dispatch(Command::TrackViewFilter { name: name.into(), on: v });
                    report(app, r);
                }
            }
        });
    });
}

pub fn utilities(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    const UTILS: [&str; 6] = ["Randomize Keys", "Create Out of Range Keys", "Select Keys by Time", "Euler Filter", "Soft Selection Settings", "Current Value Editor"];
    let mut which: usize = mem(ctx, "tv_util", 0);
    let mut nums: [f32; 4] = mem(ctx, "tv_util_nums", [5.0, 5.0, 20.0, 1.0]);
    let mut flag: bool = mem(ctx, "tv_util_flag", true);
    egui::Window::new("Track View Utilities").open(open).resizable(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                for (i, u) in UTILS.iter().enumerate() {
                    if ui.selectable_label(which == i, *u).clicked() {
                        which = i;
                        nums = match i {
                            0 => [5.0, 5.0, 1.0, 0.0],
                            1 => [20.0, 20.0, 10.0, 0.0],
                            2 | 3 => [app.state.frame_range.0 as f32, app.state.frame_range.1 as f32, 0.0, 0.0],
                            4 => [app.state.track_view.soft_range, app.state.track_view.soft_falloff, 0.0, 0.0],
                            _ => [0.0; 4],
                        };
                    }
                }
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(UTILS[which]).strong());
                let mut spin = |ui: &mut egui::Ui, l: &str, v: &mut f32| {
                    ui.horizontal(|ui| {
                        ui.label(l);
                        ui.add(crate::spinner::spin(ui, v, ..).speed(0.5));
                    });
                };
                let mut run: Option<Command> = None;
                match which {
                    0 => {
                        spin(ui, "Time +/-:", &mut nums[0]);
                        spin(ui, "Value +/-:", &mut nums[1]);
                        spin(ui, "Seed:", &mut nums[2]);
                        if ui.button("Apply").clicked() {
                            run = Some(Command::RandomizeKeys { time: nums[0], value: nums[1], seed: nums[2].max(1.0) as u32 });
                        }
                    }
                    1 => {
                        spin(ui, "Before:", &mut nums[0]);
                        spin(ui, "After:", &mut nums[1]);
                        spin(ui, "Samples:", &mut nums[2]);
                        if ui.button("Apply").clicked() {
                            run = Some(Command::CreateOutOfRangeKeys { before: nums[0] as i32, after: nums[1] as i32, samples: nums[2].max(1.0) as u32 });
                        }
                    }
                    2 => {
                        spin(ui, "Start Time:", &mut nums[0]);
                        spin(ui, "End Time:", &mut nums[1]);
                        ui.checkbox(&mut flag, "Clear Previous Selection");
                        if ui.button("OK").clicked() {
                            run = Some(Command::SelectKeysByTime { start: nums[0], end: nums[1], clear: flag });
                        }
                    }
                    3 => {
                        spin(ui, "Start:", &mut nums[0]);
                        spin(ui, "End:", &mut nums[1]);
                        if ui.button("Filter").clicked() {
                            run = Some(Command::EulerFilter { start: Some(nums[0] as i32), end: Some(nums[1] as i32) });
                        }
                    }
                    4 => {
                        let mut soft = app.state.track_view.soft;
                        if ui.checkbox(&mut soft, "Soft").changed() {
                            run = Some(Command::TrackViewSoftSelection { soft: Some(soft), range: None, falloff: None });
                        }
                        spin(ui, "Range:", &mut nums[0]);
                        spin(ui, "Falloff:", &mut nums[1]);
                        if ui.button("Set").clicked() {
                            run = Some(Command::TrackViewSoftSelection { soft: None, range: Some(nums[0]), falloff: Some(nums[1]) });
                        }
                    }
                    _ => {
                        spin(ui, "Value:", &mut nums[0]);
                        ui.checkbox(&mut flag, "Relative");
                        if ui.button("Set").clicked() {
                            run = Some(Command::TrackViewSetKeyValues { time: None, value: Some(nums[0]), relative: Some(flag) });
                        }
                    }
                }
                if let Some(c) = run {
                    let r = app.dispatch(c);
                    report(app, r);
                }
            });
        });
    });
    store(ctx, "tv_util", which);
    store(ctx, "tv_util_nums", nums);
    store(ctx, "tv_util_flag", flag);
}
