//! Views > SteeringWheels: the wheel drawn over the viewports, following the
//! cursor (a tracking menu); press a wedge and drag to use its tool, release
//! to return to the wheel; right-click, Esc or the x closes it.

use crate::LoomApp;
use loom_commands::steering::WheelKind;
use loom_commands::Command;

#[derive(Clone, Copy, Default)]
struct Drag {
    tool: usize,
    press: egui::Pos2,
    last: egui::Pos2,
    moved: bool,
}

fn radius(app: &LoomApp) -> f32 {
    let p = &app.state.prefs.wheels;
    if app.state.wheel.mini {
        [22.0, 30.0, 38.0][p.mini_size.min(2) as usize]
    } else {
        [70.0, 90.0, 110.0][p.big_size.min(2) as usize]
    }
}

fn tooltip(tool: &str) -> &'static str {
    match tool {
        "Zoom" => "Adjusts the magnification of the view",
        "Pan" => "Repositions the view by panning",
        "Orbit" => "Rotates the view around the pivot point",
        "Center" => "Drag onto an object: it becomes the view centre and pivot",
        "Rewind" => "Restores the previous view",
        "Forward" => "Moves toward the pivot point",
        "Look" => "Swivels the view",
        "Up/Down" => "Moves the view up or down",
        "Walk" => "Walks through the scene: the cursor's offset steers",
        _ => "",
    }
}

/// Which wedge `p` is on (None: outside the wheel).
fn wedge_at(centre: egui::Pos2, r: f32, n: usize, p: egui::Pos2) -> Option<usize> {
    let d = p - centre;
    if d.length() > r {
        return None;
    }
    // Clockwise from the top; wedge 0 is centred on 12 o'clock.
    let a = d.x.atan2(-d.y).rem_euclid(std::f32::consts::TAU);
    let step = std::f32::consts::TAU / n as f32;
    Some(((a + step * 0.5) / step).floor() as usize % n)
}

/// The wedges' label positions (for drawing and for tests).
pub fn wedge_centres(app: &LoomApp, ctx: &egui::Context) -> Vec<(String, egui::Pos2)> {
    let centre: egui::Pos2 = ctx.data(|d| d.get_temp(egui::Id::new("wheel_centre"))).unwrap_or_default();
    let w = &app.state.wheel;
    let tools = w.kind.tools(w.mini);
    let r = radius(app) * 0.62;
    let step = std::f32::consts::TAU / tools.len() as f32;
    tools.iter().enumerate().map(|(i, t)| {
        let a = step * i as f32;
        (t.to_string(), centre + egui::vec2(a.sin(), -a.cos()) * r)
    }).collect()
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp) {
    if !app.state.wheel.open {
        return;
    }
    let cid = egui::Id::new("wheel_centre");
    let did = egui::Id::new("wheel_drag");
    let r = radius(app);
    let pointer = ctx.input(|i| i.pointer.hover_pos());
    let mut drag: Option<Drag> = ctx.data(|d| d.get_temp(did));
    let mut centre: egui::Pos2 = ctx.data(|d| d.get_temp(cid)).unwrap_or_else(|| {
        pointer.or_else(|| app.viewport_rects.get(app.state.active_viewport).map(|r| r.center())).unwrap_or(egui::pos2(400.0, 300.0))
    });
    // The tracking menu: the cursor pushes the wheel along at its rim.
    if let (Some(p), None) = (pointer, drag) {
        let d = p - centre;
        if d.length() > r {
            centre = p - d.normalized() * r;
        }
    }
    ctx.data_mut(|d| d.insert_temp(cid, centre));
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        let _ = app.dispatch(Command::ToggleSteeringWheels);
        return;
    }
    let w = app.state.wheel.clone();
    let tools = w.kind.tools(w.mini);
    let prefs = app.state.prefs.wheels;
    let opacity = if w.mini { prefs.mini_opacity } else { prefs.big_opacity };
    let area = egui::Rect::from_center_size(centre, egui::vec2(r * 2.0 + 24.0, r * 2.0 + 24.0));
    let mut close = false;
    let mut cmds: Vec<Command> = Vec::new();
    let mut center_release: Option<egui::Pos2> = None;
    egui::Area::new(egui::Id::new("steering_wheel")).order(egui::Order::Foreground).fixed_pos(area.min).show(ctx, |ui| {
        let (rect, resp) = ui.allocate_exact_size(area.size(), egui::Sense::click_and_drag());
        let p = ui.painter().clone();
        let a = (opacity.clamp(0.05, 1.0) * 255.0) as u8;
        p.circle_filled(centre, r, egui::Color32::from_rgba_unmultiplied(60, 64, 72, a));
        p.circle_stroke(centre, r, egui::Stroke::new(1.0_f32, egui::Color32::from_gray(200)));
        let n = tools.len();
        let step = std::f32::consts::TAU / n as f32;
        let hover = pointer.and_then(|q| wedge_at(centre, r, n, q));
        for i in 0..n {
            let edge = step * (i as f32 + 0.5);
            p.line_segment([centre, centre + egui::vec2(edge.sin(), -edge.cos()) * r], egui::Stroke::new(1.0_f32, egui::Color32::from_gray(150)));
        }
        if let Some(h) = drag.map(|d| d.tool).or(hover) {
            let a0 = step * (h as f32 - 0.5);
            let pts: Vec<egui::Pos2> = std::iter::once(centre).chain((0..=12).map(|k| {
                let t = a0 + step * k as f32 / 12.0;
                centre + egui::vec2(t.sin(), -t.cos()) * r
            })).collect();
            p.add(egui::Shape::convex_polygon(pts, egui::Color32::from_rgba_unmultiplied(120, 170, 255, 90), egui::Stroke::NONE));
        }
        let font = egui::FontId::proportional(if w.mini { 8.0 } else { 12.0 });
        for (i, t) in tools.iter().enumerate() {
            let ang = step * i as f32;
            let at = centre + egui::vec2(ang.sin(), -ang.cos()) * r * 0.62;
            let label = if w.mini { &t[..1] } else { t };
            p.text(at, egui::Align2::CENTER_CENTER, label, font.clone(), egui::Color32::WHITE);
        }
        // The x (close) and the Wheel menu button.
        let x_rect = egui::Rect::from_center_size(rect.right_top() + egui::vec2(-8.0, 8.0), egui::vec2(14.0, 14.0));
        if ui.put(x_rect, egui::Button::new("x").small()).on_hover_text("Close").clicked() {
            close = true;
        }
        let m_rect = egui::Rect::from_center_size(rect.right_bottom() + egui::vec2(-8.0, -8.0), egui::vec2(16.0, 14.0));
        ui.put(m_rect, |ui: &mut egui::Ui| {
            ui.menu_button("v", |ui| {
                let mut entry = |ui: &mut egui::Ui, label: &str, c: Command| {
                    if ui.button(label).clicked() {
                        cmds.push(c);
                        ui.close_menu();
                    }
                };
                for k in [WheelKind::ViewObject, WheelKind::TourBuilding, WheelKind::FullNavigation] {
                    entry(ui, &format!("Mini {} Wheel", k.name()), Command::SteeringWheel { wheel: format!("Mini {}", k.name()) });
                }
                entry(ui, "Full Navigation Wheel", Command::SteeringWheel { wheel: "Full Navigation".into() });
                ui.menu_button("Basic Wheels", |ui| {
                    for k in [WheelKind::ViewObject, WheelKind::TourBuilding] {
                        entry(ui, &format!("{} Wheel", k.name()), Command::SteeringWheel { wheel: k.name().into() });
                    }
                });
                ui.separator();
                entry(ui, "Go Home", Command::HomeView);
                entry(ui, "Restore Original Center", Command::SteeringWheelRestoreCenter);
                entry(ui, "Increase Walk Speed", Command::SteeringWheelWalkSpeed { increase: true });
                entry(ui, "Decrease Walk Speed", Command::SteeringWheelWalkSpeed { increase: false });
                ui.separator();
                entry(ui, "Configure...", Command::ConfigureViewports { panel: Some("SteeringWheels".into()) });
            })
            .response
        });
        if resp.secondary_clicked() {
            close = true;
        }
        if prefs.tool_tips && drag.is_none() {
            if let Some(h) = hover {
                p.text(centre + egui::vec2(0.0, r + 16.0), egui::Align2::CENTER_TOP, tooltip(tools[h]), egui::FontId::proportional(11.0), egui::Color32::WHITE);
            }
        }
        let press = ui.input(|i| i.pointer.primary_pressed());
        let down = ui.input(|i| i.pointer.primary_down());
        let pos = ui.input(|i| i.pointer.interact_pos());
        if press && resp.hovered() {
            if let (Some(q), None) = (pos, drag) {
                if let Some(t) = wedge_at(centre, r, n, q) {
                    drag = Some(Drag { tool: t, press: q, last: q, moved: false });
                }
            }
        }
        if let (Some(mut d), Some(q)) = (drag, pos) {
            let tool = tools[d.tool];
            if prefs.tool_messages && !w.mini {
                p.text(centre + egui::vec2(0.0, -r - 10.0), egui::Align2::CENTER_BOTTOM, tooltip(tool), egui::FontId::proportional(11.0), egui::Color32::YELLOW);
            }
            if down {
                let delta = q - d.last;
                let begin = !d.moved;
                match tool {
                    "Walk" => {
                        let off = q - d.press;
                        cmds.push(Command::SteeringWheelTool { tool: tool.into(), dx: off.x, dy: off.y, begin: Some(begin), click: None });
                        d.moved = true;
                        ui.ctx().request_repaint();
                    }
                    "Center" | "Rewind" => {}
                    _ if delta != egui::Vec2::ZERO => {
                        cmds.push(Command::SteeringWheelTool { tool: tool.into(), dx: delta.x, dy: delta.y, begin: Some(begin), click: None });
                        d.moved = true;
                    }
                    _ => {}
                }
                d.last = q;
                drag = Some(d);
            } else {
                match tool {
                    "Center" => center_release = Some(q),
                    "Rewind" => cmds.push(Command::SteeringWheelRewind { steps: 1 }),
                    "Zoom" | "Forward" if !d.moved => cmds.push(Command::SteeringWheelTool { tool: tool.into(), dx: 0.0, dy: 0.0, begin: None, click: Some(true) }),
                    _ => {}
                }
                drag = None;
            }
        }
    });
    ctx.data_mut(|d| match drag {
        Some(v) => d.insert_temp(did, v),
        None => d.remove::<Drag>(did),
    });
    if let Some(q) = center_release {
        let index = app.state.active_viewport;
        if let Some(rect) = app.viewport_rects.get(index).copied() {
            let camera = app.camera_for(index);
            let (view, proj) = (camera.view_matrix(), camera.projection_matrix(rect.width() / rect.height().max(1.0)));
            if let Some((_, point, _)) = crate::viewport_widget::surface_hit_node(app, q, rect, view, proj) {
                cmds.push(Command::SteeringWheelCenter { point: point.to_array() });
            }
        }
    }
    for c in cmds {
        let _ = app.dispatch(c);
    }
    if close {
        let _ = app.dispatch(Command::ToggleSteeringWheels);
    }
}
