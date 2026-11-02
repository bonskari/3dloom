//! Controller property dialogs: Expression Controller (with its Track View
//! Pick, constant, Function List and Debug windows), Noise Controller,
//! Spring and Audio Controller. Every control dispatches a command on the
//! track `AppState::ctl_track` names.

use crate::LoomApp;
use eframe::egui;
use loom_commands::ctl_props::read_ctl;
use loom_commands::Command;
use loom_scene::expr::{ExprCtrl, Val};
use loom_scene::tracks::{self, TrackPath};
use loom_scene::NodeId;

fn mem<T: Clone + Send + Sync + 'static>(ctx: &egui::Context, key: &str, init: impl FnOnce() -> T) -> T {
    ctx.data_mut(|d| d.get_temp_mut_or_insert_with(egui::Id::new(key), init).clone())
}

fn store<T: Clone + Send + Sync + 'static>(ctx: &egui::Context, key: &str, v: T) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(key), v));
}

fn run(app: &mut LoomApp, c: Command) -> bool {
    match app.dispatch(c) {
        Ok(_) => true,
        Err(e) => {
            app.state.status_prompt = e;
            false
        }
    }
}

/// The dialog's node, track name and the window title "Node \ Track".
fn target(app: &LoomApp) -> Option<(u64, String, String)> {
    let (id, track) = app.state.ctl_track.clone()?;
    let node = app.state.scene.node(NodeId(id))?.name.clone();
    Some((id, track.clone(), format!("{node} \\ {track}")))
}

fn fmt(v: Val) -> String {
    match v {
        Val::S(x) => format!("{x:.3}"),
        Val::V(v) => format!("[{:.3}, {:.3}, {:.3}]", v.x, v.y, v.z),
    }
}

/// Dialog-local state of the Expression Controller dialog.
#[derive(Clone, Default)]
struct ExprUi {
    /// The track it was filled from (a new track refills it).
    for_track: Option<(u64, String)>,
    name: String,
    vector: bool,
    selected: Option<String>,
    expression: String,
    description: String,
    error: String,
    constant: Option<[f32; 3]>,
    picking: bool,
    pick_node: Option<u64>,
    pick_track: Option<String>,
    functions: bool,
    debug: bool,
    /// The controller's expression when last shown: a change from outside
    /// (Load, undo) refills the field.
    seen: String,
    seen_description: String,
}

pub fn expression(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let Some((id, track, title)) = target(app) else {
        *open = false;
        return;
    };
    let Ok((tp, e)) = read_ctl::<ExprCtrl>(&app.state, NodeId(id), &track, "Expression") else {
        *open = false;
        return;
    };
    let mut st: ExprUi = mem(ctx, "expr_ui", ExprUi::default);
    if st.for_track.as_ref() != Some(&(id, track.clone())) {
        st = ExprUi { for_track: Some((id, track.clone())), expression: e.expression.clone(), description: e.description.clone(), ..Default::default() };
    }
    if st.seen_description != e.description {
        st.description = e.description.clone();
        st.seen_description = e.description.clone();
    }
    if st.seen != e.expression {
        st.expression = e.expression.clone();
        st.seen = e.expression.clone();
        st.error.clear();
    }
    let mut close = false;
    egui::Window::new(format!("Expression Controller : {title}")).id(egui::Id::new("Expression Controller")).open(open).resizable(true).default_width(560.0).show(ctx, |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(230.0);
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Create Variables").strong());
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.add(egui::TextEdit::singleline(&mut st.name).desired_width(140.0).id(egui::Id::new("expr_var_name")));
                    });
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut st.vector, false, "Scalar");
                        ui.radio_value(&mut st.vector, true, "Vector");
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Create").clicked() && run(app, Command::ExpressionCreateVariable { id, track: track.clone(), name: st.name.clone(), vector: st.vector }) {
                            st.selected = Some(st.name.clone());
                        }
                        if ui.add_enabled(st.selected.is_some(), egui::Button::new("Delete")).clicked() && run(app, Command::ExpressionDeleteVariable { id, track: track.clone(), name: st.selected.clone().unwrap_or_default() }) {
                            st.selected = None;
                        }
                        if ui.add_enabled(st.selected.is_some(), egui::Button::new("Rename")).clicked() && run(app, Command::ExpressionRenameVariable { id, track: track.clone(), name: st.selected.clone().unwrap_or_default(), new_name: st.name.clone() }) {
                            st.selected = Some(st.name.clone());
                        }
                    });
                });
                ui.columns(2, |c| {
                    for (col, vector, label) in [(0, false, "Scalars"), (1, true, "Vectors")] {
                        c[col].label(label);
                        egui::Frame::group(c[col].style()).show(&mut c[col], |ui| {
                            ui.set_min_height(90.0);
                            for v in e.vars.iter().filter(|v| v.vector == vector) {
                                if ui.selectable_label(st.selected.as_deref() == Some(v.name.as_str()), &v.name).clicked() {
                                    st.selected = Some(v.name.clone());
                                    st.name = v.name.clone();
                                }
                            }
                        });
                    }
                });
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Variable Parameters").strong());
                    let var = st.selected.as_deref().and_then(|n| e.var(n)).cloned();
                    let mut ticks = var.as_ref().map(|v| v.tick_offset).unwrap_or(0);
                    ui.horizontal(|ui| {
                        ui.label("Tick Offset:");
                        if ui.add_enabled(var.is_some(), crate::spinner::spin(&mut ticks, ..).speed(10.0)).changed() {
                            run(app, Command::ExpressionSetTickOffset { id, track: track.clone(), name: st.selected.clone().unwrap_or_default(), ticks });
                        }
                    });
                    let src = var.as_ref().map(|v| match &v.source {
                        Some(s) => s.label.clone(),
                        None if v.vector => format!("Constant [{}, {}, {}]", v.constant[0], v.constant[1], v.constant[2]),
                        None => format!("Constant {}", v.constant[0]),
                    });
                    ui.label(src.unwrap_or_default());
                    ui.horizontal(|ui| {
                        if ui.add_enabled(var.is_some(), egui::Button::new("Assign to Constant")).clicked() {
                            st.constant = var.as_ref().map(|v| v.constant);
                        }
                        if ui.add_enabled(var.is_some(), egui::Button::new("Assign to Controller")).clicked() {
                            st.picking = true;
                        }
                    });
                });
            });
            ui.vertical(|ui| {
                ui.label("Expression");
                ui.add(egui::TextEdit::multiline(&mut st.expression).desired_rows(6).desired_width(300.0).code_editor().id(egui::Id::new("expr_text")));
                if !st.error.is_empty() {
                    ui.colored_label(ui.visuals().error_fg_color, format!("Error: {}", st.error));
                }
                ui.label("Description");
                let d = ui.add(egui::TextEdit::multiline(&mut st.description).desired_rows(3).desired_width(300.0).id(egui::Id::new("expr_desc")));
                if d.lost_focus() && st.description != e.description {
                    run(app, Command::ExpressionSet { id, track: track.clone(), expression: None, description: Some(st.description.clone()) });
                }
                ui.horizontal(|ui| {
                    if ui.button("Function List").clicked() {
                        st.functions = !st.functions;
                    }
                    if ui.button("Save").clicked() {
                        run(app, Command::ExpressionFileDialog { save: true });
                    }
                    if ui.button("Load").clicked() {
                        run(app, Command::ExpressionFileDialog { save: false });
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Debug").clicked() {
                        st.debug = !st.debug;
                    }
                    if ui.button("Evaluate").clicked() {
                        match app.dispatch(Command::ExpressionSet { id, track: track.clone(), expression: Some(st.expression.clone()), description: Some(st.description.clone()) }) {
                            Ok(_) => st.error.clear(),
                            Err(err) => st.error = err,
                        }
                    }
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
            });
        });
    });
    // Assign to Constant: the variable's value.
    if let Some(mut c) = st.constant {
        let name = st.selected.clone().unwrap_or_default();
        let vector = e.var(&name).is_some_and(|v| v.vector);
        let (mut ok, mut cancel) = (false, false);
        egui::Window::new(name.clone()).id(egui::Id::new("expr_constant")).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if vector {
                    for (i, l) in ["X:", "Y:", "Z:"].iter().enumerate() {
                        ui.label(*l);
                        ui.add(crate::spinner::spin(&mut c[i], ..).speed(0.1));
                    }
                } else {
                    ui.label("Value:");
                    ui.add(crate::spinner::spin(&mut c[0], ..).speed(0.1));
                }
            });
            ui.horizontal(|ui| {
                ok = ui.button("OK").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        st.constant = Some(c);
        if ok {
            run(app, Command::ExpressionAssignConstant { id, track: track.clone(), name, value: c });
        }
        if ok || cancel {
            st.constant = None;
        }
    }
    if st.picking {
        track_view_pick(ctx, app, &mut st, id, &track, &tp);
    }
    if st.functions {
        egui::Window::new("Function List").id(egui::Id::new("expr_functions")).open(&mut st.functions).default_width(300.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                egui::Grid::new("expr_fn_grid").striped(true).show(ui, |ui| {
                    for (f, m) in loom_scene::expr::FUNCTIONS {
                        ui.monospace(*f);
                        ui.label(*m);
                        ui.end_row();
                    }
                });
            });
        });
    }
    if st.debug {
        let t = app.state.current_frame as f32;
        egui::Window::new("Expression Debug Window").id(egui::Id::new("expr_debug")).open(&mut st.debug).show(ctx, |ui| {
            egui::Grid::new("expr_debug_grid").show(ui, |ui| {
                for (n, v) in ExprCtrl::time_vars(t) {
                    ui.label(n);
                    ui.label(format!("{v:.3}"));
                    ui.end_row();
                }
                for v in &e.vars {
                    ui.label(&v.name);
                    ui.label(fmt(e.var_value(v, t)));
                    ui.end_row();
                }
                ui.label("Value");
                ui.label(match e.value(t) {
                    Ok(v) => fmt(v),
                    Err(err) => format!("error at: {err}"),
                });
                ui.end_row();
            });
        });
    }
    store(ctx, "expr_ui", st);
    if close {
        *open = false;
    }
}

/// Track View Pick: the scene's objects and their float / vector tracks.
fn track_view_pick(ctx: &egui::Context, app: &mut LoomApp, st: &mut ExprUi, id: u64, track: &str, me: &TrackPath) {
    let name = st.selected.clone().unwrap_or_default();
    let (mut ok, mut cancel) = (false, false);
    let mut nodes: Vec<(u64, String)> = app.state.scene.nodes().map(|n| (n.id.0, n.name.clone())).collect();
    nodes.sort_by(|a, b| a.1.cmp(&b.1));
    egui::Window::new("Track View Pick").id(egui::Id::new("expr_pick")).collapsible(false).default_width(280.0).show(ctx, |ui| {
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for (nid, nname) in &nodes {
                let open = st.pick_node == Some(*nid);
                if ui.selectable_label(open, format!("{} {nname}", if open { "-" } else { "+" })).clicked() {
                    st.pick_node = if open { None } else { Some(*nid) };
                    st.pick_track = None;
                }
                if open {
                    for t in tracks::list_tracks(&app.state.scene, NodeId(*nid)) {
                        if !matches!(t.ty, loom_scene::param::ParamType::F32 | loom_scene::param::ParamType::Vec3) || t == *me {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            ui.add_space(16.0);
                            if ui.selectable_label(st.pick_track.as_deref() == Some(t.name.as_str()), &t.name).clicked() {
                                st.pick_track = Some(t.name.clone());
                            }
                        });
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            ok = ui.add_enabled(st.pick_track.is_some(), egui::Button::new("OK")).clicked();
            cancel = ui.button("Cancel").clicked();
        });
    });
    if ok {
        if let (Some(n), Some(t)) = (st.pick_node, st.pick_track.clone()) {
            if run(app, Command::ExpressionAssignController { id, track: track.to_owned(), name, source_id: n, source_track: t }) {
                st.picking = false;
            }
        }
    }
    if cancel {
        st.picking = false;
    }
}

/// A labelled spinner row; returns the new value when it changed.
fn row(ui: &mut egui::Ui, label: &str, v: f32, range: std::ops::RangeInclusive<f32>, speed: f64) -> Option<f32> {
    let mut x = v;
    let mut out = None;
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.add(crate::spinner::spin(&mut x, range).speed(speed)).changed() {
            out = Some(x);
        }
    });
    out
}

pub fn noise(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let Some((id, track, title)) = target(app) else {
        *open = false;
        return;
    };
    let Ok((tp, n)) = read_ctl::<loom_scene::param::NoiseCtrl>(&app.state, NodeId(id), &track, "Noise") else {
        *open = false;
        return;
    };
    let dims = if tp.ty == loom_scene::param::ParamType::Vec3 { 3 } else { 1 };
    let scale = tracks::Role::of(&tp) == tracks::Role::Scale;
    let set = |app: &mut LoomApp, name: &str, value: f64, index: Option<u32>| {
        run(app, Command::NoiseSetParameter { id, track: track.clone(), name: name.into(), value, index });
    };
    egui::Window::new(format!("Noise Controller : {title}")).id(egui::Id::new("Noise Controller")).open(open).resizable(false).show(ctx, |ui| {
        let mut seed = n.seed;
        ui.horizontal(|ui| {
            ui.label("Seed:");
            if ui.add(crate::spinner::spin(&mut seed, 0..=u32::MAX)).changed() {
                set(app, "Seed", seed as f64, None);
            }
        });
        if let Some(v) = row(ui, "Frequency:", n.frequency, 0.001..=100.0, 0.01) {
            set(app, "Frequency", v as f64, None);
        }
        for i in 0..dims {
            let label = match (dims, i) {
                (1, _) => "Strength:".to_owned(),
                _ => format!("{} Strength:", ["X", "Y", "Z"][i]),
            };
            let s = n.strength.get(i).or(n.strength.last()).copied().unwrap_or(0.0);
            // Scale strengths are shown in percent.
            let k = if scale { 100.0 } else { 1.0 };
            ui.horizontal(|ui| {
                ui.label(label);
                let mut x = s * k;
                if ui.add(crate::spinner::spin(&mut x, ..).speed(0.5)).changed() {
                    set(app, "Strength", (x / k) as f64, Some(i as u32));
                }
                let mut pos = n.positive.get(i).copied().unwrap_or(false);
                if ui.checkbox(&mut pos, ">0").changed() {
                    set(app, ">0", if pos { 1.0 } else { 0.0 }, Some(i as u32));
                }
            });
        }
        if let Some(v) = row(ui, "Ramp In:", n.ramp_in, 0.0..=100_000.0, 1.0) {
            set(app, "Ramp In", v as f64, None);
        }
        if let Some(v) = row(ui, "Ramp Out:", n.ramp_out, 0.0..=100_000.0, 1.0) {
            set(app, "Ramp Out", v as f64, None);
        }
        let mut fractal = n.fractal;
        if ui.checkbox(&mut fractal, "Fractal Noise").changed() {
            set(app, "Fractal Noise", if fractal { 1.0 } else { 0.0 }, None);
        }
        ui.add_enabled_ui(n.fractal, |ui| {
            if let Some(v) = row(ui, "Roughness:", n.roughness, 0.0..=1.0, 0.01) {
                set(app, "Roughness", v as f64, None);
            }
        });
        // Characteristic Graph: the first component over the controller's range.
        let (rect, _) = ui.allocate_exact_size(egui::vec2(260.0, 60.0), egui::Sense::hover());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
        let end = n.end.max(1.0);
        let amp = n.strength.iter().fold(1e-6f32, |m, s| m.max(s.abs()));
        let pts: Vec<egui::Pos2> = (0..=120)
            .map(|i| {
                let t = end * i as f32 / 120.0;
                let v = n.eval(t, 1)[0] / amp;
                egui::pos2(rect.left() + rect.width() * i as f32 / 120.0, rect.center().y - v * rect.height() * 0.45)
            })
            .collect();
        p.add(egui::Shape::line(pts, egui::Stroke::new(1.0_f32, ui.visuals().text_color())));
    });
}

pub fn spring(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let Some((id, track, title)) = target(app) else {
        *open = false;
        return;
    };
    let Ok((_, s)) = read_ctl::<loom_scene::param::SpringCtrl<glam::Vec3>>(&app.state, NodeId(id), &track, "Spring") else {
        *open = false;
        return;
    };
    let mut sel: usize = mem(ctx, "spring_sel", || 0);
    sel = sel.min(s.springs.len());
    let mut relative: bool = mem(ctx, "spring_rel", || false);
    let set = |app: &mut LoomApp, name: &str, value: f64, index: Option<u32>, relative: Option<bool>| {
        run(app, Command::SpringSetParameter { id, track: track.clone(), name: name.into(), value, index, relative });
    };
    egui::Window::new(format!("Spring : {title}")).id(egui::Id::new("Spring Controller")).open(open).resizable(false).default_width(260.0).show(ctx, |ui| {
        ui.label(egui::RichText::new("Spring Dynamics").strong());
        ui.group(|ui| {
            ui.label("Point");
            if let Some(v) = row(ui, "Mass:", s.mass, 0.0..=1.0e6, 1.0) {
                set(app, "Mass", v as f64, None, None);
            }
            if let Some(v) = row(ui, "Drag:", s.drag, 0.0..=10.0, 0.01) {
                set(app, "Drag", v as f64, None, None);
            }
        });
        ui.group(|ui| {
            ui.label("Springs");
            ui.horizontal(|ui| {
                let picking = app.state.pick_mode == Some(loom_commands::PickMode::SpringAdd);
                if ui.selectable_label(picking, "Add").on_hover_text("Click objects in a viewport; click Add again to finish").clicked() {
                    run(app, Command::SpringPickSprings);
                }
                if ui.add_enabled(sel > 0, egui::Button::new("Remove")).clicked() && run(app, Command::SpringRemoveSpring { id, track: track.clone(), index: sel as u32 }) {
                    sel -= 1;
                }
            });
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_min_height(60.0);
                let mut items = vec![("Self Influence".to_owned(), s.tension, s.dampening)];
                for l in &s.springs {
                    let name = app.state.scene.node(NodeId(l.source.node)).map(|n| n.name.clone()).unwrap_or_else(|| l.source.label.clone());
                    items.push((name, l.tension, l.dampening));
                }
                for (i, (n, t, d)) in items.iter().enumerate() {
                    if ui.selectable_label(sel == i, format!("{n}  {t:.2}/{d:.2}")).clicked() {
                        sel = i;
                    }
                }
            });
            let (t, d) = if sel == 0 { (s.tension, s.dampening) } else { (s.springs[sel - 1].tension, s.springs[sel - 1].dampening) };
            let shown = |v: f32| if relative { 0.0 } else { v };
            if let Some(v) = row(ui, "Tension:", shown(t), -1.0e6..=1.0e6, 0.01) {
                set(app, "Tension", v as f64, Some(sel as u32), Some(relative));
            }
            if let Some(v) = row(ui, "Dampening:", shown(d), -1.0e6..=1.0e6, 0.01) {
                set(app, "Dampening", v as f64, Some(sel as u32), Some(relative));
            }
            ui.horizontal(|ui| {
                ui.radio_value(&mut relative, true, "Relative");
                ui.radio_value(&mut relative, false, "Absolute");
            });
        });
        ui.label(egui::RichText::new("Forces, Limits and Precision").strong());
        ui.group(|ui| {
            ui.label("Calculation Parameters");
            if let Some(v) = row(ui, "Start Frame:", s.start, -100_000.0..=100_000.0, 1.0) {
                set(app, "Start Frame", v as f64, None, None);
            }
            let mut it = s.iterations;
            ui.horizontal(|ui| {
                ui.label("Iterations:");
                if ui.add(crate::spinner::spin(&mut it, 0..=4)).changed() {
                    set(app, "Iterations", it as f64, None, None);
                }
            });
            for (i, l) in ["X Effect:", "Y Effect:", "Z Effect:"].iter().enumerate() {
                if let Some(v) = row(ui, l, s.effect[i], 0.0..=1000.0, 1.0) {
                    set(app, &l[..8], v as f64, None, None);
                }
            }
        });
    });
    store(ctx, "spring_sel", sel);
    store(ctx, "spring_rel", relative);
}

pub fn audio(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let Some((id, track, title)) = target(app) else {
        *open = false;
        return;
    };
    let Ok((tp, a)) = read_ctl::<loom_scene::audio::AudioCtrl>(&app.state, NodeId(id), &track, "Audio") else {
        *open = false;
        return;
    };
    let role = tracks::Role::of(&tp);
    let stereo = a.wave().is_some_and(|w| w.channels.len() > 1);
    let set = |app: &mut LoomApp, name: &str, value: f64, index: Option<u32>| {
        run(app, Command::AudioSetParameter { id, track: track.clone(), name: name.into(), value, index });
    };
    egui::Window::new(format!("Audio Controller : {title}")).id(egui::Id::new("Audio Controller")).open(open).resizable(false).show(ctx, |ui| {
        ui.group(|ui| {
            ui.label("Audio File");
            ui.label(if a.path.is_empty() { "No sound".to_owned() } else { std::path::Path::new(&a.path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() });
            ui.horizontal(|ui| {
                if ui.button("Choose Sound").clicked() {
                    run(app, Command::AudioSoundDialog);
                }
                if ui.add_enabled(!a.path.is_empty(), egui::Button::new("Remove Sound")).clicked() {
                    run(app, Command::AudioChooseSound { id, track: track.clone(), path: None });
                }
            });
            let mut abs = a.absolute;
            if ui.checkbox(&mut abs, "Absolute Value").changed() {
                set(app, "Absolute Value", if abs { 1.0 } else { 0.0 }, None);
            }
        });
        ui.group(|ui| {
            ui.label("Sample");
            if let Some(v) = row(ui, "Threshold:", a.threshold, 0.0..=1.0, 0.01) {
                set(app, "Threshold", v as f64, None);
            }
            let mut o = a.oversampling;
            ui.horizontal(|ui| {
                ui.label("Oversampling:");
                if ui.add(crate::spinner::spin(&mut o, 1..=1000)).changed() {
                    set(app, "Oversampling", o as f64, None);
                }
            });
        });
        // Scale is shown in percent.
        let k = if role == tracks::Role::Scale { 100.0 } else { 1.0 };
        if a.base.len() == 1 {
            ui.group(|ui| {
                ui.label("Controller Range");
                if let Some(v) = row(ui, "Min.:", a.base[0], -1.0e6..=1.0e6, 0.1) {
                    set(app, "Base", v as f64, Some(0));
                }
                if let Some(v) = row(ui, "Max.:", a.target[0], -1.0e6..=1.0e6, 0.1) {
                    set(app, "Target", v as f64, Some(0));
                }
            });
        } else {
            let word = match role {
                tracks::Role::Scale => "Scale",
                tracks::Role::Rotation => "Rotation",
                _ => "Position",
            };
            for (field, label, vals) in [("Base", format!("Base {word}"), &a.base), ("Target", format!("Target {word}"), &a.target)] {
                ui.group(|ui| {
                    ui.label(label);
                    ui.horizontal(|ui| {
                        for i in 0..3 {
                            ui.label(["X:", "Y:", "Z:"][i]);
                            let mut x = vals[i] * k;
                            if ui.add(crate::spinner::spin(&mut x, ..).speed(0.5)).changed() {
                                set(app, field, (x / k) as f64, Some(i as u32));
                            }
                        }
                    });
                });
            }
        }
        ui.add_enabled_ui(stereo, |ui| {
            ui.group(|ui| {
                ui.label("Channel");
                ui.horizontal(|ui| {
                    for (i, (c, l)) in [(loom_scene::audio::AudioChannel::Left, "Left"), (loom_scene::audio::AudioChannel::Right, "Right"), (loom_scene::audio::AudioChannel::Mix, "Mix")].into_iter().enumerate() {
                        if ui.radio(a.channel == c, l).clicked() {
                            set(app, "Channel", i as f64, None);
                        }
                    }
                });
            });
        });
    });
}
