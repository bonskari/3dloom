//! Time Configuration dialog: Frame Rate, Playback (loop/real time state)
//! and Animation (active time segment) groups.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

/// The settings when the dialog opened (Cancel puts them back).
#[derive(Clone)]
struct Snapshot {
    range: (i32, i32),
    fps: f32,
    frame: i32,
    playback: loom_commands::anim_cmds::Playback,
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let (start, end) = app.state.frame_range;
    let fps = app.state.fps;
    let snap_id = egui::Id::new("time_configuration_snapshot");
    let snap: Snapshot = ctx.data_mut(|d| d.get_temp_mut_or_insert_with(snap_id, || Snapshot { range: app.state.frame_range, fps: app.state.fps, frame: app.state.current_frame, playback: app.state.playback.clone() }).clone());
    let mut done = false;
    egui::Window::new("Time Configuration").open(open).resizable(false).show(ctx, |ui| {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Frame Rate").strong());
            ui.horizontal(|ui| {
                for (label, f) in [("NTSC", 30.0), ("Film", 24.0), ("PAL", 25.0)] {
                    if ui.radio((fps - f).abs() < 1e-3, label).clicked() {
                        let _ = app.dispatch(Command::SetTimeConfiguration { start: None, end: None, fps: Some(f) });
                    }
                }
                let custom = ![30.0, 24.0, 25.0].iter().any(|f| (fps - f).abs() < 1e-3);
                let _ = ui.radio(custom, "Custom");
            });
            ui.horizontal(|ui| {
                ui.label("FPS:");
                let mut v = fps;
                if ui.add(crate::spinner::spin(ui, &mut v, 1.0..=1000.0)).changed() {
                    let _ = app.dispatch(Command::SetTimeConfiguration { start: None, end: None, fps: Some(v) });
                }
            });
        });
        ui.group(|ui| {
            ui.label(egui::RichText::new("Playback").strong());
            let pb = app.state.playback;
            ui.horizontal(|ui| {
                let mut rt = pb.real_time;
                if ui.checkbox(&mut rt, "Real Time").changed() {
                    let _ = app.dispatch(Command::SetPlayback { real_time: Some(rt), loop_playback: None, speed: None, direction: None });
                }
                let mut lp = pb.loop_playback;
                if ui.checkbox(&mut lp, "Loop").changed() {
                    let _ = app.dispatch(Command::SetPlayback { real_time: None, loop_playback: Some(lp), speed: None, direction: None });
                }
            });
            ui.add_enabled_ui(pb.real_time, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Speed:");
                    for (label, v) in [("1/4x", 0.25), ("1/2x", 0.5), ("1x", 1.0), ("2x", 2.0), ("4x", 4.0)] {
                        if ui.radio((pb.speed - v).abs() < 1e-3, label).clicked() {
                            let _ = app.dispatch(Command::SetPlayback { real_time: None, loop_playback: None, speed: Some(v), direction: None });
                        }
                    }
                });
            });
            ui.add_enabled_ui(!pb.real_time, |ui| {
                ui.horizontal(|ui| {
                    use loom_commands::anim_cmds::PlayDirection as D;
                    ui.label("Direction:");
                    for (label, d) in [("Forward", D::Forward), ("Reverse", D::Reverse), ("Ping-Pong", D::PingPong)] {
                        if ui.radio(pb.direction == d, label).clicked() {
                            let _ = app.dispatch(Command::SetPlayback { real_time: None, loop_playback: None, speed: None, direction: Some(d) });
                        }
                    }
                });
            });
        });
        ui.group(|ui| {
            ui.label(egui::RichText::new("Animation").strong());
            let (mut s, mut e) = (start, end);
            ui.horizontal(|ui| {
                ui.label("Start Time:");
                if ui.add(crate::spinner::spin(ui, &mut s, ..)).changed() {
                    let _ = app.dispatch(Command::SetTimeConfiguration { start: Some(s), end: None, fps: None });
                }
                ui.label("End Time:");
                if ui.add(crate::spinner::spin(ui, &mut e, ..)).changed() {
                    let _ = app.dispatch(Command::SetTimeConfiguration { start: None, end: Some(e), fps: None });
                }
            });
            ui.horizontal(|ui| {
                ui.label(format!("Length: {}", end - start));
                ui.label(format!("Frame Count: {}", end - start + 1));
                ui.label("Current Time:");
                let mut f = app.state.current_frame;
                if ui.add(crate::spinner::spin(ui, &mut f, start..=end)).changed() {
                    let _ = app.dispatch(Command::SetFrame { frame: f });
                }
            });
            // Re-scale Time: stretch every key from the active segment to a new one.
            let id = egui::Id::new("rescale_time");
            let (mut ns, mut ne) = ui.ctx().data_mut(|d| *d.get_temp_mut_or_insert_with(id, || (start, end)));
            ui.horizontal(|ui| {
                ui.label("Re-scale Time:");
                ui.add(crate::spinner::spin(ui, &mut ns, ..));
                ui.add(crate::spinner::spin(ui, &mut ne, ..));
                if ui.button("Re-scale").clicked() {
                    if let Err(e) = app.dispatch(Command::RescaleTime { start: ns, end: ne }) {
                        app.state.status_prompt = e;
                    }
                }
            });
            ui.ctx().data_mut(|d| d.insert_temp(id, (ns, ne)));
        });
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                done = true;
            }
            if ui.button("Cancel").clicked() {
                let p = &snap.playback;
                let _ = app.dispatch(Command::SetTimeConfiguration { start: Some(snap.range.0), end: Some(snap.range.1), fps: Some(snap.fps) });
                let _ = app.dispatch(Command::SetPlayback { real_time: Some(p.real_time), loop_playback: Some(p.loop_playback), speed: Some(p.speed), direction: Some(p.direction) });
                let _ = app.dispatch(Command::SetFrame { frame: snap.frame });
                done = true;
            }
        });
    });
    if done || !*open {
        ctx.data_mut(|d| d.remove::<Snapshot>(snap_id));
    }
    if done {
        let _ = app.dispatch(Command::CloseDialog);
    }
}
