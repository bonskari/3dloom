//! IK/FK curves belong to the rig, independently of source animation clips.
use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;
#[derive(Clone)]
struct Drag {
    from: f32,
    to: f32,
    origin: f32,
}
pub(crate) fn show(ctx: &egui::Context, app: &mut LoomApp) {
    if app.state.scene.loom_rigs.is_empty() {
        return;
    }
    let scene = app.state.scene.instance_id();
    let rigs = app.state.scene.loom_rigs.clone();
    let first = app.state.frame_range.0 as f32;
    let span = (app.state.frame_range.1 as f32 - first).max(1.);
    let mut command = None;
    egui::TopBottomPanel::bottom("loomrig_tracks").show(ctx, |ui| {
        egui::CollapsingHeader::new("LoomRig IK / FK Tracks")
            .default_open(true)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(104.)
                    .show(ui, |ui| {
                        for rig in &rigs {
                            for limb in &rig.limbs {
                                let (row, _) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), 23.),
                                    egui::Sense::hover(),
                                );
                                let axis = app.track_bar.key_rect.unwrap_or(row);
                                let ppf = axis.width() / span;
                                ui.painter().text(
                                    row.left_center(),
                                    egui::Align2::LEFT_CENTER,
                                    format!("{} · {}", rig.name, limb.mapping.name),
                                    egui::FontId::proportional(11.),
                                    egui::Color32::LIGHT_GRAY,
                                );
                                let line = egui::Rect::from_min_max(
                                    egui::pos2(axis.left(), row.top()),
                                    egui::pos2(axis.right(), row.bottom()),
                                );
                                let painter = ui.painter().with_clip_rect(line);
                                let value_y = |value: f32| {
                                    row.bottom() - 3. - value.clamp(0., 1.) * (row.height() - 6.)
                                };
                                let frame_x = |time: f32| axis.left() + (time - first) * ppf;
                                painter.line_segment(
                                    [
                                        egui::pos2(axis.left(), row.bottom() - 2.),
                                        egui::pos2(axis.right(), row.bottom() - 2.),
                                    ],
                                    egui::Stroke::new(1., egui::Color32::from_gray(45)),
                                );
                                if let Some(keys) = limb.blend.keys() {
                                    let track = &keys.tracks[0];
                                    let points: Vec<_> = (0..=80)
                                        .map(|i| {
                                            let t = first + span * i as f32 / 80.;
                                            egui::pos2(frame_x(t), value_y(limb.blend.get_at(t)))
                                        })
                                        .collect();
                                    painter.add(egui::Shape::line(
                                        points,
                                        egui::Stroke::new(
                                            1.,
                                            egui::Color32::from_rgb(80, 180, 195),
                                        ),
                                    ));
                                    for key in &track.keys {
                                        let id = egui::Id::new((
                                            "loomrig_key",
                                            scene,
                                            rig.id,
                                            &limb.mapping.name,
                                            key.time.to_bits(),
                                        ));
                                        let mut drag = ctx.data_mut(|d| d.get_temp::<Drag>(id));
                                        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                                            drag = None;
                                            ctx.data_mut(|d| d.remove::<Drag>(id));
                                        }
                                        let time = drag.as_ref().map(|d| d.to).unwrap_or(key.time);
                                        let center = egui::pos2(frame_x(time), row.center().y);
                                        let rect = egui::Rect::from_center_size(
                                            center,
                                            egui::vec2(12., 18.),
                                        );
                                        let response =
                                            ui.interact(rect, id, egui::Sense::click_and_drag());
                                        response.widget_info(|| {
                                            egui::WidgetInfo::labeled(
                                                egui::WidgetType::Button,
                                                ui.is_enabled(),
                                                format!(
                                                    "{} IK/FK @ {}",
                                                    limb.mapping.name, key.time
                                                ),
                                            )
                                        });
                                        painter.add(egui::Shape::convex_polygon(
                                            vec![
                                                center + egui::vec2(0., -5.),
                                                center + egui::vec2(5., 0.),
                                                center + egui::vec2(0., 5.),
                                                center + egui::vec2(-5., 0.),
                                            ],
                                            egui::Color32::from_rgb(225, 180, 70),
                                            egui::Stroke::NONE,
                                        ));
                                        if response.clicked() {
                                            command = Some(Command::SetFrame {
                                                frame: key.time.round() as i32,
                                            });
                                        }
                                        if response.drag_started() {
                                            if let Some(origin) =
                                                ctx.input(|i| i.pointer.press_origin())
                                            {
                                                drag = Some(Drag {
                                                    from: key.time,
                                                    to: key.time,
                                                    origin: origin.x,
                                                });
                                            }
                                        }
                                        if let Some(d) = drag.as_mut() {
                                            if let Some(pointer) = response.interact_pointer_pos() {
                                                d.to =
                                                    d.from + ((pointer.x - d.origin) / ppf).round();
                                            }
                                        }
                                        if response.drag_stopped() {
                                            if let Some(d) = drag.take() {
                                                command = Some(Command::MoveLoomRigBlendKey {
                                                    rig: rig.id,
                                                    limb: limb.mapping.name.clone(),
                                                    from: d.from,
                                                    to: d.to,
                                                });
                                            }
                                        }
                                        ctx.data_mut(|data| {
                                            if let Some(d) = drag {
                                                data.insert_temp(id, d);
                                            } else {
                                                data.remove::<Drag>(id);
                                            }
                                        });
                                        response.context_menu(|ui| {
                                            if ui.button("Delete IK / FK Key").clicked() {
                                                command = Some(Command::SetLoomRigBlendKey {
                                                    rig: rig.id,
                                                    limb: limb.mapping.name.clone(),
                                                    time: key.time,
                                                    value: None,
                                                });
                                                ui.close_menu();
                                            }
                                        });
                                    }
                                }
                                let x = frame_x(app.state.current_frame as f32);
                                painter.line_segment(
                                    [egui::pos2(x, row.top()), egui::pos2(x, row.bottom())],
                                    egui::Stroke::new(1., egui::Color32::WHITE),
                                );
                            }
                        }
                    });
            });
    });
    if let Some(command) = command {
        if let Err(e) = app.dispatch(command) {
            app.state.status_prompt = e;
        }
    }
}
