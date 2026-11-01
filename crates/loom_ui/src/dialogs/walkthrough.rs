//! Animation > Walkthrough Assistant floater: Main Controls (Camera Creation,
//! Cameras, Path Control), Render Preview, View Controls (free camera) or
//! Look-At-Camera (targeted), Advanced Controls.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, PickMode};
use loom_scene::constraint::PositionConstraint;
use loom_scene::{NodeId, ObjectKind};

#[derive(Clone, Copy)]
struct Settings {
    targeted: bool,
    eye_level: f32,
    /// Look-At-Camera: Path (true) or Object.
    target_path: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { targeted: false, eye_level: 60.0, target_path: true }
    }
}

fn run(app: &mut LoomApp, c: Command) {
    if let Err(e) = app.dispatch(c) {
        app.state.status_prompt = e;
    }
}

fn param(app: &LoomApp, id: NodeId, name: &str) -> f32 {
    app.state.scene.node(id).and_then(|n| n.object.parameters().into_iter().find(|(k, _)| *k == name)).map(|(_, v)| v.as_f64() as f32).unwrap_or(0.0)
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("walkthrough_assistant_dialog");
    let mut st: Settings = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
    let cameras: Vec<(NodeId, String)> = app.state.scene.nodes().filter(|n| matches!(n.object, ObjectKind::Camera { .. })).map(|n| (n.id, n.name.clone())).collect();
    let cam = app.state.walkthrough.camera.filter(|c| cameras.iter().any(|(i, _)| i == c));
    egui::Window::new("Walkthrough Assistant").open(open).resizable(false).show(ctx, |ui| {
        egui::CollapsingHeader::new("Main Controls").default_open(true).show(ui, |ui| {
            ui.label(egui::RichText::new("Camera Creation").strong());
            ui.horizontal(|ui| {
                if ui.button("Create New Camera").clicked() {
                    run(app, Command::WalkthroughCreateCamera { targeted: st.targeted });
                }
                ui.radio_value(&mut st.targeted, false, "Free");
                ui.radio_value(&mut st.targeted, true, "Targeted");
            });
            ui.label(egui::RichText::new("Cameras").strong());
            egui::ScrollArea::vertical().id_salt("walkthrough_cameras").max_height(80.0).show(ui, |ui| {
                for (i, name) in &cameras {
                    if ui.selectable_label(cam == Some(*i), name).clicked() {
                        run(app, Command::WalkthroughSelectCamera { id: i.0 });
                    }
                }
            });
            let Some(c) = cam else { return };
            let targeted = matches!(app.state.scene.node(c).map(|n| &n.object), Some(ObjectKind::Camera { target: Some(_), .. }));
            ui.horizontal(|ui| {
                let mut t = targeted;
                if ui.checkbox(&mut t, "Targeted").changed() {
                    run(app, Command::CameraType { id: c.0, target: t });
                }
                if ui.button("Set Viewport to Camera").clicked() {
                    run(app, Command::WalkthroughSetViewport { camera: c.0 });
                }
            });
            ui.label(egui::RichText::new("Path Control").strong());
            let path = loom_commands::walkthrough::path_of(&app.state, c);
            ui.horizontal(|ui| {
                let picking = app.state.pick_mode == Some(PickMode::WalkthroughPath);
                let label = path.and_then(|p| app.state.scene.node(p)).map(|n| n.name.clone()).unwrap_or_else(|| "Pick Path".into());
                if ui.selectable_label(picking, label).clicked() {
                    run(app, Command::WalkthroughPickPath { target: false });
                }
                if ui.add_enabled(path.is_some(), egui::Button::new("Clear Path")).clicked() {
                    run(app, Command::WalkthroughSetPath { camera: c.0, path: None });
                }
            });
            ui.add_enabled_ui(path.is_some(), |ui| {
                let mut on = path.is_some_and(|p| app.state.walkthrough.path_heights.contains_key(&p));
                if ui.checkbox(&mut on, "Move Path to Eye Level").changed() {
                    run(app, Command::WalkthroughEyeLevel { camera: c.0, on, level: st.eye_level });
                }
                ui.horizontal(|ui| {
                    ui.label("Eye Level:");
                    let speed = app.spinner_speed(0.1);
                    if ui.add(crate::spinner::spin(ui, &mut st.eye_level, ..).speed(speed).fixed_decimals(1)).changed() && on {
                        run(app, Command::WalkthroughEyeLevel { camera: c.0, on, level: st.eye_level });
                    }
                });
            });
        });
        let Some(c) = cam else { return };
        egui::CollapsingHeader::new("Render Preview").default_open(true).show(ui, |ui| {
            match app.state.walkthrough.preview.clone() {
                Some(img) => {
                    let tex = preview_texture(ui.ctx(), &img, app.state.walkthrough.preview_version);
                    ui.image((tex.id(), egui::vec2(img.width() as f32, img.height() as f32)));
                }
                None => {
                    let [w, h] = loom_commands::walkthrough::PREVIEW_SIZE;
                    let (r, _) = ui.allocate_exact_size(egui::vec2(w as f32, h as f32), egui::Sense::hover());
                    ui.painter().rect_filled(r, 0.0, egui::Color32::from_gray(30));
                }
            }
            let busy = app.render_job.is_some() || app.state.pending_render.is_some();
            if ui.add_enabled(!busy, egui::Button::new("Click to Render Preview")).clicked() {
                run(app, Command::WalkthroughRenderPreview { camera: c.0 });
            }
        });
        let node = app.state.scene.node(c);
        let target = match node.map(|n| &n.object) {
            Some(ObjectKind::Camera { target, .. }) => *target,
            _ => None,
        };
        match target {
            None => {
                egui::CollapsingHeader::new("View Controls").default_open(true).show(ui, |ui| {
                    let (turn, tilt) = loom_commands::walkthrough::head_angles(node.map(|n| n.transform.rotation_quat()).unwrap_or_default());
                    ui.label("Turn Head");
                    let (mut t, mut a) = (turn, tilt);
                    ui.horizontal(|ui| {
                        ui.label("Left");
                        // Left is a positive turn: the slider runs right to left.
                        let mut s = -t;
                        if ui.add(egui::Slider::new(&mut s, -90.0..=90.0).show_value(false)).changed() {
                            t = -s;
                            run(app, Command::WalkthroughHead { camera: c.0, turn: Some(t), tilt: None });
                        }
                        ui.label("Right");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Head Tilt Angle:");
                        if ui.add(crate::spinner::spin(ui, &mut a, -89.0..=89.0).speed(0.5).fixed_decimals(1)).changed() {
                            run(app, Command::WalkthroughHead { camera: c.0, turn: None, tilt: Some(a) });
                        }
                    });
                    if ui.button("Reset Eyes Level").clicked() {
                        run(app, Command::WalkthroughHead { camera: c.0, turn: None, tilt: Some(0.0) });
                    }
                    if ui.button("Remove All Head Animation").clicked() {
                        run(app, Command::WalkthroughRemoveHeadAnimation { camera: c.0 });
                    }
                });
            }
            Some(t) => {
                egui::CollapsingHeader::new("Look-At-Camera").default_open(true).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut st.target_path, true, "Path");
                        ui.radio_value(&mut st.target_path, false, "Object");
                    });
                    let on = match app.state.scene.node(t).and_then(|n| n.constraints.position.as_ref()) {
                        Some(PositionConstraint::Path(p)) => p.targets.first().map(|x| x.node),
                        Some(PositionConstraint::Position { targets, .. }) => targets.first().map(|x| x.node),
                        _ => None,
                    };
                    ui.horizontal(|ui| {
                        let picking = app.state.pick_mode == Some(PickMode::WalkthroughTargetPath);
                        let label = on.and_then(|p| app.state.scene.node(p)).map(|n| n.name.clone()).unwrap_or_else(|| if st.target_path { "Pick Target Path" } else { "Pick Target Object" }.into());
                        if ui.selectable_label(picking, label).clicked() {
                            run(app, Command::WalkthroughPickPath { target: true });
                        }
                        if ui.add_enabled(on.is_some(), egui::Button::new("Clear Path")).clicked() {
                            run(app, Command::WalkthroughSetTarget { camera: c.0, path: st.target_path, node: None });
                        }
                    });
                });
            }
        }
        egui::CollapsingHeader::new("Advanced Controls").default_open(false).show(ui, |ui| {
            egui::Grid::new("walkthrough_advanced").num_columns(2).show(ui, |ui| {
                ui.label("Field of View:");
                let mut fov = param(app, c, "FOV");
                if ui.add(crate::spinner::spin(ui, &mut fov, 1.0..=175.0).speed(0.5).fixed_decimals(1)).changed() {
                    run(app, Command::SetParameter { id: c.0, name: "FOV".into(), value: fov as f64 });
                }
                ui.end_row();
                ui.label("Target Distance:");
                let mut d = param(app, c, "Target Distance");
                let speed = app.spinner_speed(0.1);
                if ui.add(crate::spinner::spin(ui, &mut d, 0.001..=1.0e6).speed(speed).fixed_decimals(1)).changed() {
                    run(app, Command::SetParameter { id: c.0, name: "Target Distance".into(), value: d as f64 });
                }
                ui.end_row();
            });
            let pc = match app.state.scene.node(c).and_then(|n| n.constraints.position.as_ref()) {
                Some(PositionConstraint::Path(p)) => Some((p.constant_velocity, p.follow)),
                _ => None,
            };
            ui.add_enabled_ui(pc.is_some(), |ui| {
                let (mut cv, mut follow) = pc.unwrap_or_default();
                if ui.checkbox(&mut cv, "Constant Speed").changed() {
                    run(app, Command::SetConstraintParameter { id: c.0, name: "Constant Velocity".into(), value: cv as u8 as f64 });
                }
                if ui.checkbox(&mut follow, "Follow Path").changed() {
                    run(app, Command::SetConstraintParameter { id: c.0, name: "Follow".into(), value: follow as u8 as f64 });
                }
            });
        });
    });
    // Picking needs a camera; the pick turns off with the floater.
    if !*open && matches!(app.state.pick_mode, Some(PickMode::WalkthroughPath | PickMode::WalkthroughTargetPath)) {
        let _ = app.dispatch(Command::ExitPickMode);
    }
    ctx.data_mut(|d| d.insert_temp(id, st));
}

/// The preview as an egui texture, uploaded once per version.
fn preview_texture(ctx: &egui::Context, img: &image::RgbaImage, version: u64) -> egui::TextureHandle {
    let id = egui::Id::new("walkthrough_preview_texture");
    if let Some((v, t)) = ctx.data(|d| d.get_temp::<(u64, egui::TextureHandle)>(id)) {
        if v == version {
            return t;
        }
    }
    let px: Vec<u8> = img.pixels().flat_map(|p| [p.0[0], p.0[1], p.0[2], 255]).collect();
    let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], &px);
    let t = ctx.load_texture("walkthrough_preview", ci, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, (version, t.clone())));
    t
}
