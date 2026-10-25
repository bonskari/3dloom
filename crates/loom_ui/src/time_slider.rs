//! Time slider and track bar (bottom of the viewports): the frame
//! slider with its "current / end" readout and step arrows (red while Auto
//! Key is on), and the track bar with the selection's keys: click selects,
//! drag moves, Shift-drag copies, right-click opens the track bar menu.
//! A right-click on the slider opens the Create Key dialog; a right-drag
//! fills it with the frame it started on and the frame it ended on.

use crate::LoomApp;
use eframe::egui;
use loom_commands::anim_cmds::TrackBarFilter;
use loom_commands::Command;

/// Track bar key selection and drag, kept between frames.
#[derive(Debug, Clone, Default)]
pub struct TrackBarState {
    pub selected: Vec<i32>,
    pub drag_from: Option<f32>,
    pub menu_frame: Option<i32>,
    /// The open Create Key dialog.
    pub create_key: Option<CreateKeyDialog>,
    /// The frame a right-drag on the slider started on.
    pub right_drag_from: Option<i32>,
    /// The Mini Curve Editor replaces the track bar.
    pub mini_curve_editor: bool,
}

/// Create Key: copy the pose at Source Time to keys at Destination Time.
#[derive(Debug, Clone, Copy)]
pub struct CreateKeyDialog {
    pub source: i32,
    pub destination: i32,
    pub position: bool,
    pub rotation: bool,
    pub scale: bool,
}

impl CreateKeyDialog {
    pub fn new(source: i32, destination: i32) -> Self {
        Self { source, destination, position: true, rotation: true, scale: true }
    }
}

fn run(app: &mut LoomApp, cmd: Command) {
    if let Err(e) = app.dispatch(cmd) {
        app.state.status_prompt = e;
    }
}

const THUMB_WIDTH: f32 = 64.0;

/// The "frame / end" thumb. It travels inside the track: flush left
/// at the first frame, flush right at the last, so its label is never cut.
fn thumb_rect(track: egui::Rect, frame: i32, (start, end): (i32, i32)) -> egui::Rect {
    let t = (frame - start) as f32 / (end - start).max(1) as f32;
    let x = track.min.x + t.clamp(0.0, 1.0) * (track.width() - THUMB_WIDTH);
    egui::Rect::from_min_size(egui::pos2(x, track.center().y - 8.0), egui::vec2(THUMB_WIDTH, 16.0))
}

/// The frame whose thumb is centred nearest to `x`.
fn frame_at(track: egui::Rect, x: f32, (start, end): (i32, i32)) -> i32 {
    let span = (track.width() - THUMB_WIDTH).max(1.0);
    let t = ((x - track.min.x - THUMB_WIDTH * 0.5) / span).clamp(0.0, 1.0);
    start + (t * (end - start) as f32).round() as i32
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp) {
    if app.state.expert_mode {
        return;
    }
    let mini = app.state.show_track_bar && app.track_bar.mini_curve_editor;
    let h = if app.state.show_track_bar && !mini { 44.0 } else { 24.0 };
    egui::TopBottomPanel::bottom("time_slider").exact_height(h).show(ctx, |ui| {
        let (start, end) = app.state.frame_range;
        let frame = app.state.current_frame;
        ui.horizontal(|ui| {
            if crate::icons::Icon::new(crate::icons::PREVIOUS_FRAME, "Backup Time One Unit").show(ui, &app.state).clicked() {
                run(app, Command::BackupTimeOneUnit);
            }
            let avail = ui.available_width() - 40.0;
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(avail, 18.0), egui::Sense::click_and_drag());
            let painter = ui.painter_at(rect);
            // Auto Key turns the slider red.
            let bg = if app.state.auto_key { app.color("Auto Key Background").gamma_multiply(0.65) } else { egui::Color32::from_gray(60) };
            painter.rect_filled(rect, 2.0, bg);
            let thumb = thumb_rect(rect, frame, (start, end));
            painter.rect_filled(thumb, 2.0, egui::Color32::from_gray(120));
            painter.text(thumb.center(), egui::Align2::CENTER_CENTER, format!("{frame} / {end}"), egui::FontId::proportional(12.0), egui::Color32::WHITE);
            if resp.dragged() || resp.clicked() {
                if let Some(p) = resp.interact_pointer_pos() {
                    let f = frame_at(rect, p.x, (start, end));
                    if f != frame {
                        run(app, Command::SetFrame { frame: f });
                    }
                }
            }
            if resp.drag_started_by(egui::PointerButton::Secondary) {
                app.track_bar.right_drag_from = Some(frame);
            }
            if resp.drag_stopped_by(egui::PointerButton::Secondary) {
                if let Some(source) = app.track_bar.right_drag_from.take() {
                    app.track_bar.create_key = Some(CreateKeyDialog::new(source, app.state.current_frame));
                }
            }
            if resp.secondary_clicked() {
                app.track_bar.create_key = Some(CreateKeyDialog::new(frame, frame));
            }
            if crate::icons::Icon::new(crate::icons::NEXT_FRAME, "Forward Time One Unit").show(ui, &app.state).clicked() {
                run(app, Command::ForwardTimeOneUnit);
            }
        });
        if app.state.show_track_bar && !mini {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if crate::icons::Icon::new(crate::icons::CURVE_EDITOR, "Open Mini Curve Editor").show(ui, &app.state).clicked() {
                    app.track_bar.mini_curve_editor = true;
                }
                track_bar(ui, app);
            });
        }
    });
    if mini {
        // Above the time slider, where the track bar was; a fixed height so the content never grows the panel (and moves its rows) frame to frame.
        egui::TopBottomPanel::bottom("mini_curve_editor").exact_height(280.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if crate::icons::Icon::new(crate::icons::CURVE_EDITOR, "Close Mini Curve Editor").on(true).show(ui, &app.state).clicked() {
                    app.track_bar.mini_curve_editor = false;
                }
                ui.label("Mini Curve Editor");
            });
            ui.push_id("mini_curve_editor", |ui| crate::track_view::mini_curve_editor(ui, app));
        });
    }
    create_key_dialog(ctx, app);
}

fn create_key_dialog(ctx: &egui::Context, app: &mut LoomApp) {
    let Some(mut d) = app.track_bar.create_key else { return };
    let mut open = true;
    let mut done = false;
    egui::Window::new("Create Key").collapsible(false).resizable(false).open(&mut open).show(ctx, |ui| {
        egui::Grid::new("create_key_times").show(ui, |ui| {
            ui.label("Source Time:");
            ui.add(crate::spinner::spin(ui, &mut d.source, ..));
            ui.end_row();
            ui.label("Destination Time:");
            ui.add(crate::spinner::spin(ui, &mut d.destination, ..));
            ui.end_row();
        });
        ui.checkbox(&mut d.position, "Position");
        ui.checkbox(&mut d.rotation, "Rotation");
        ui.checkbox(&mut d.scale, "Scale");
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                run(app, Command::CreateKey { source: Some(d.source), destination: Some(d.destination), position: Some(d.position), rotation: Some(d.rotation), scale: Some(d.scale) });
                done = true;
            }
            if ui.button("Cancel").clicked() {
                done = true;
            }
        });
    });
    app.track_bar.create_key = (open && !done).then_some(d);
}

fn track_bar(ui: &mut egui::Ui, app: &mut LoomApp) {
    let (start, end) = app.state.frame_range;
    let frame = app.state.current_frame;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 16.0), egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, egui::Color32::from_gray(45));
    let frames = (end - start).max(1) as f32;
    let x_of = |f: f32| rect.min.x + (f - start as f32) / frames * rect.width();
    let f_of = |x: f32| start as f32 + (x - rect.min.x) / rect.width() * frames;
    let step = ((frames / (rect.width() / 60.0).max(1.0)).ceil() as i32).max(1);
    let mut f = start;
    while f <= end {
        let x = x_of(f as f32);
        painter.line_segment([egui::pos2(x, rect.max.y - 4.0), egui::pos2(x, rect.max.y)], egui::Stroke::new(1.0_f32, egui::Color32::from_gray(150)));
        painter.text(egui::pos2(x + 2.0, rect.min.y), egui::Align2::LEFT_TOP, f.to_string(), egui::FontId::proportional(10.0), egui::Color32::from_gray(190));
        f += step;
    }
    let x = x_of(frame as f32);
    painter.line_segment([egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)], egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(100, 170, 255)));
    // The selection's keys (by the track bar filter), coloured by what they key:
    // position, rotation and scale keys apart from the others.
    let kinds = loom_commands::anim_cmds::selection_key_kinds(&app.state);
    let times: Vec<f32> = kinds.iter().map(|(t, _)| *t).collect();
    let mut tb = app.track_bar.clone();
    tb.selected.retain(|s| times.iter().any(|t| (*t - *s as f32).abs() < 0.5));
    let drag_dt = tb.drag_from.zip(resp.interact_pointer_pos()).map(|(a, p)| (f_of(p.x) - a).round()).unwrap_or(0.0);
    for (t, k) in &kinds {
        let sel = tb.selected.iter().any(|s| (*t - *s as f32).abs() < 0.5);
        let shown = if sel && resp.dragged() { *t + drag_dt } else { *t };
        let r = egui::Rect::from_center_size(egui::pos2(x_of(shown), rect.center().y + 2.0), egui::vec2(5.0, 11.0));
        if sel {
            painter.rect_filled(r, 0.0, app.color("Selected Keys"));
            continue;
        }
        // A key of several kinds shows a band of each colour.
        let bands: Vec<egui::Color32> = [(k.position, "Position Keys"), (k.rotation, "Rotation Keys"), (k.scale, "Scale Keys"), (k.other, "Other Keys")].iter().filter(|(on, _)| *on).map(|(_, e)| app.color(e)).collect();
        let h = r.height() / bands.len().max(1) as f32;
        for (i, c) in bands.iter().enumerate() {
            painter.rect_filled(egui::Rect::from_min_size(egui::pos2(r.min.x, r.min.y + i as f32 * h), egui::vec2(r.width(), h)), 0.0, *c);
        }
    }
    let key_at = |x: f32| times.iter().copied().find(|t| (x_of(*t) - x).abs() < 4.0);
    if resp.clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            match key_at(p.x) {
                Some(t) => {
                    let ft = t.round() as i32;
                    if ui.input(|i| i.modifiers.ctrl) {
                        if !tb.selected.contains(&ft) {
                            tb.selected.push(ft);
                        }
                    } else {
                        tb.selected = vec![ft];
                    }
                }
                None => tb.selected.clear(),
            }
        }
    }
    if resp.drag_started() {
        if let Some(p) = resp.interact_pointer_pos() {
            if let Some(t) = key_at(p.x) {
                let ft = t.round() as i32;
                if !tb.selected.contains(&ft) {
                    tb.selected = vec![ft];
                }
                tb.drag_from = Some(f_of(p.x));
            } else {
                // A drag on empty track bar selects the keys it spans.
                tb.drag_from = None;
            }
        }
    }
    if resp.drag_stopped() {
        if tb.drag_from.take().is_some() && drag_dt != 0.0 && !tb.selected.is_empty() {
            let copy = ui.input(|i| i.modifiers.shift);
            let d = drag_dt as i32;
            let mut sel = tb.selected.clone();
            sel.sort();
            for s in &sel {
                let cmd = if copy { Command::CopyKeys { from: *s, to: *s, delta: d } } else { Command::MoveKeysInTime { from: *s, to: *s, delta: d } };
                run(app, cmd);
            }
            if !copy {
                tb.selected = sel.iter().map(|s| s + d).collect();
            }
        }
    }
    if resp.secondary_clicked() {
        tb.menu_frame = resp.interact_pointer_pos().map(|p| key_at(p.x).map(|t| t.round() as i32).unwrap_or_else(|| f_of(p.x).round() as i32));
    }
    let menu_frame = tb.menu_frame;
    app.track_bar = tb;
    resp.context_menu(|ui| {
        let at = menu_frame.unwrap_or(frame);
        // Key Properties: the tracks keyed at this frame open in Key Info.
        let keyed: Vec<(loom_scene::tracks::TrackPath, u32)> = app
            .state
            .selection
            .iter()
            .flat_map(|id| loom_scene::tracks::list_tracks(&app.state.scene, *id))
            .filter(|t| loom_commands::anim_cmds::track_bar_allows(&app.state, t))
            .flat_map(|t| {
                let comps: Vec<u32> = loom_scene::tracks::read(&app.state.scene, &t).and_then(|p| p.keys().map(|k| k.tracks.iter().enumerate().filter(|(_, tr)| tr.key_at(at as f32).is_some()).map(|(c, _)| c as u32).collect())).unwrap_or_default();
                comps.into_iter().map(move |c| (t.clone(), c))
            })
            .collect();
        ui.menu_button("Key Properties", |ui| {
            if keyed.is_empty() {
                ui.label("No keys at this frame");
            }
            for (t, c) in &keyed {
                let name = app.state.scene.node(t.node).map(|n| n.name.clone()).unwrap_or_default();
                let axis = if t.ty == loom_scene::param::ParamType::Vec3 { [" X", " Y", " Z"][*c as usize % 3] } else { "" };
                if ui.button(format!("{name}: {}{axis}", t.name)).clicked() {
                    run(app, Command::TrackViewSelectKeys { keys: vec![loom_commands::track_view::KeyArg { id: t.node.0, track: t.name.clone(), comp: *c, time: at as f32 }], op: loom_commands::SelectOp::Replace });
                    run(app, Command::PropertiesTrackView);
                    ui.close_menu();
                }
            }
        });
        ui.separator();
        if ui.button("Delete Key").clicked() {
            let sel = app.track_bar.selected.clone();
            if sel.is_empty() {
                run(app, Command::DeleteKeys { from: at, to: at });
            } else {
                for s in sel {
                    run(app, Command::DeleteKeys { from: s, to: s });
                }
            }
            app.track_bar.selected.clear();
            ui.close_menu();
        }
        ui.menu_button("Filter", |ui| {
            for (label, f) in [("All Keys", TrackBarFilter::AllKeys), ("All Transform Keys", TrackBarFilter::AllTransformKeys), ("Current Transform", TrackBarFilter::CurrentTransform), ("Object", TrackBarFilter::Object), ("Material", TrackBarFilter::Material)] {
                if ui.radio(app.state.track_bar_filter == f, label).clicked() {
                    run(app, Command::SetTrackBarFilter { filter: f });
                    ui.close_menu();
                }
            }
        });
        ui.separator();
        if ui.button("Create Key...").clicked() {
            app.track_bar.create_key = Some(CreateKeyDialog::new(frame, at));
            ui.close_menu();
        }
        if ui.button(format!("Go to Time ({at})")).clicked() {
            run(app, Command::SetFrame { frame: at });
            ui.close_menu();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumb_stays_inside_the_track_and_round_trips() {
        let track = egui::Rect::from_min_size(egui::pos2(20.0, 0.0), egui::vec2(800.0, 18.0));
        assert_eq!(thumb_rect(track, 0, (0, 100)).min.x, track.min.x);
        assert_eq!(thumb_rect(track, 100, (0, 100)).max.x, track.max.x);
        for f in [0, 1, 37, 99, 100] {
            assert_eq!(frame_at(track, thumb_rect(track, f, (0, 100)).center().x, (0, 100)), f);
        }
    }
}
