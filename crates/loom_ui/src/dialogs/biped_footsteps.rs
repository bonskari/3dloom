//! Biped > Footstep Creation > Create Multiple Footsteps: the dialog for the
//! selected gait. OK creates the footsteps through the registry.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;
use loom_scene::biped::Gait;

#[derive(Clone)]
struct St {
    start_left: bool,
    alternate: bool,
    count: u32,
    stride_width: f32,
    stride_length: f32,
    stride_height: f32,
    at_current_frame: bool,
}

impl St {
    fn defaults(gait: Gait) -> Self {
        St { start_left: true, alternate: true, count: 4, stride_width: 1.0, stride_length: gait.default_stride(), stride_height: 0.0, at_current_frame: false }
    }
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let Some(s) = app.state.selection.iter().find_map(|id| loom_commands::biped::root_of(&app.state, *id)).and_then(|r| loom_commands::biped::structure(&app.state, r).cloned()) else {
        *open = false;
        return;
    };
    let gait = s.gait.gait;
    let id = egui::Id::new("biped_multiple_footsteps").with(gait.label());
    let mut st: St = ctx.data(|d| d.get_temp(id)).unwrap_or_else(|| St::defaults(gait));
    let (leg, pelvis) = loom_commands::biped_footsteps::leg_and_pelvis(&s);
    let mut done = false;
    egui::Window::new(format!("Create Multiple Footsteps: {}", gait.label())).id(egui::Id::new("biped_multiple_footsteps_window")).open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.radio_value(&mut st.start_left, true, "Start Left");
            ui.radio_value(&mut st.start_left, false, "Start Right");
            // Walks always alternate.
            let mut alt = st.alternate || gait == Gait::Walk;
            if ui.add_enabled(gait != Gait::Walk, egui::Checkbox::new(&mut alt, "Alternate")).changed() {
                st.alternate = alt;
            }
        });
        egui::Grid::new("biped_multiple_footsteps_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("Number of Footsteps:");
            ui.add(crate::spinner::spin(ui, &mut st.count, 1..=1000).speed(0.1));
            ui.end_row();
            ui.label("Parametric Stride Width:");
            ui.add(crate::spinner::spin(ui, &mut st.stride_width, 0.0..=10.0).speed(0.01));
            ui.end_row();
            ui.label("Actual Stride Width:");
            ui.label(format!("{:.2}", st.stride_width * pelvis));
            ui.end_row();
        });
        ui.separator();
        ui.label("Timing");
        ui.radio_value(&mut st.at_current_frame, false, "Start After Last Footstep");
        ui.radio_value(&mut st.at_current_frame, true, "Start at Current Frame");
        ui.separator();
        ui.label("First Step");
        egui::Grid::new("biped_multiple_footsteps_first").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("Parametric Stride Length:");
            ui.add(crate::spinner::spin(ui, &mut st.stride_length, -10.0..=10.0).speed(0.01));
            ui.end_row();
            ui.label("Actual Stride Length:");
            ui.label(format!("{:.2}", st.stride_length * leg));
            ui.end_row();
            ui.label("Actual Stride Height:");
            ui.add(crate::spinner::spin(ui, &mut st.stride_height, f32::MIN..=f32::MAX).speed(0.1));
            ui.end_row();
        });
        let steps = if gait == Gait::Jump { st.count.div_ceil(2) } else { st.count };
        ui.label(format!("Total Distance: {:.2}", (st.stride_length * leg * steps.saturating_sub(1) as f32).abs()));
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                let cmd = Command::BipedCreateMultipleFootsteps {
                    gait: Some(gait.label().into()),
                    count: Some(st.count),
                    start_left: Some(st.start_left),
                    alternate: Some(st.alternate),
                    stride_width: Some(st.stride_width),
                    stride_length: Some(st.stride_length),
                    stride_height: Some(st.stride_height),
                    at_current_frame: Some(st.at_current_frame),
                };
                if let Err(e) = app.dispatch(cmd) {
                    app.state.status_prompt = e;
                }
                done = true;
            }
            if ui.button("Cancel").clicked() {
                done = true;
            }
            if ui.button("Default").clicked() {
                st = St::defaults(gait);
            }
        });
    });
    if done || !*open {
        *open = false;
    }
    ctx.data_mut(|d| d.insert_temp(id, st));
}
