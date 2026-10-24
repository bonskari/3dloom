//! Create tab: category buttons, Standard Primitives Object Type rollout,
//! Name and Color, Keyboard Entry and the selected object's Parameters.

use super::rollout;
use crate::{CreateKind, LineDrag, LoomApp};
use eframe::egui;
use loom_commands::{Command, PrimitiveKind, ShapeKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Category {
    #[default]
    Geometry,
    Shapes,
    Lights,
    Cameras,
    Helpers,
    SpaceWarps,
    Systems,
}

impl Category {
    pub const ALL: [Category; 7] = [
        Category::Geometry, Category::Shapes, Category::Lights, Category::Cameras, Category::Helpers, Category::SpaceWarps, Category::Systems,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Category::Geometry => "Geometry",
            Category::Shapes => "Shapes",
            Category::Lights => "Lights",
            Category::Cameras => "Cameras",
            Category::Helpers => "Helpers",
            Category::SpaceWarps => "Space Warps",
            Category::Systems => "Systems",
        }
    }
}

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    ui.horizontal_wrapped(|ui| {
        for c in Category::ALL {
            if ui.selectable_label(app.create_category == c, c.label()).clicked() {
                app.create_category = c;
                app.create_kind = None;
            }
        }
    });
    match app.create_category {
        Category::Geometry if app.create_compound => {
            egui::ComboBox::from_id_salt("create_geometry_class").selected_text("Compound Objects").show_ui(ui, |ui| {
                if ui.selectable_label(false, "Standard Primitives").clicked() {
                    app.create_compound = false;
                    app.compound_kind = None;
                }
                let _ = ui.selectable_label(true, "Compound Objects");
            });
            rollout(ui, "Object Type", true, |ui| {
                autogrid_and_pin(ui, app);
                egui::Grid::new("compound_type").num_columns(2).show(ui, |ui| {
                    for (i, kind) in super::compound::CompoundKind::ALL.iter().enumerate() {
                        let on = app.compound_kind == Some(*kind);
                        if ui.add_sized([110.0, 22.0], egui::SelectableLabel::new(on, kind.label())).clicked() {
                            app.compound_kind = if on { None } else { Some(*kind) };
                            app.create_kind = None;
                            let _ = app.dispatch(Command::SelectObject);
                        }
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
            });
            name_and_color(ui, app);
            super::compound::rollouts(ui, app, app.compound_kind);
            parameters(ui, app);
        }
        Category::Geometry => {
            egui::ComboBox::from_id_salt("create_geometry_class").selected_text("Standard Primitives").show_ui(ui, |ui| {
                let _ = ui.selectable_label(true, "Standard Primitives");
                if ui.selectable_label(false, "Compound Objects").clicked() {
                    app.create_compound = true;
                    app.create_kind = None;
                }
            });
            rollout(ui, "Object Type", true, |ui| {
                autogrid_and_pin(ui, app);
                egui::Grid::new("object_type").num_columns(2).show(ui, |ui| {
                    for (i, kind) in PrimitiveKind::ALL.iter().enumerate() {
                        let on = app.create_kind == Some(CreateKind::Primitive(*kind));
                        if ui.add_sized([110.0, 22.0], egui::SelectableLabel::new(on, kind.label())).clicked() {
                            app.start_create((!on).then_some(CreateKind::Primitive(*kind)));
                        }
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
            });
            name_and_color(ui, app);
            if let Some(CreateKind::Primitive(kind)) = app.create_kind {
                rollout(ui, "Creation Method", true, |ui| {
                    let current = app.creation_method(kind);
                    for m in crate::create_tool::creation_methods(kind) {
                        if ui.radio(current == *m, *m).clicked() {
                            app.set_creation_method(kind, m);
                        }
                    }
                });
                keyboard_entry(ui, app, kind);
            }
            parameters(ui, app);
        }
        Category::Shapes => {
            egui::ComboBox::from_id_salt("create_shape_class").selected_text("Splines").show_ui(ui, |ui| {
                let _ = ui.selectable_label(true, "Splines");
            });
            rollout(ui, "Object Type", true, |ui| {
                autogrid_and_pin(ui, app);
                egui::Grid::new("shape_type").num_columns(2).show(ui, |ui| {
                    let kinds: Vec<(CreateKind, &str)> = std::iter::once((CreateKind::Line, "Line")).chain(ShapeKind::ALL.iter().map(|k| (CreateKind::Shape(*k), k.label()))).collect();
                    for (i, (kind, label)) in kinds.iter().enumerate() {
                        let on = app.create_kind == Some(*kind);
                        if ui.add_sized([110.0, 22.0], egui::SelectableLabel::new(on, *label)).clicked() {
                            if let Some(line) = app.line.take() {
                                line.finish(app);
                            }
                            app.start_create((!on).then_some(*kind));
                        }
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
            });
            name_and_color(ui, app);
            if app.create_kind == Some(CreateKind::Line) {
                rollout(ui, "Creation Method", true, |ui| {
                    ui.label("Initial Type");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut app.line_initial_smooth, false, "Corner");
                        ui.radio_value(&mut app.line_initial_smooth, true, "Smooth");
                    });
                    ui.label("Drag Type");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut app.line_drag, LineDrag::Corner, "Corner");
                        ui.radio_value(&mut app.line_drag, LineDrag::Smooth, "Smooth");
                        ui.radio_value(&mut app.line_drag, LineDrag::Bezier, "Bezier");
                    });
                });
            }
            if let Some(CreateKind::Shape(kind)) = app.create_kind {
                rollout(ui, "Keyboard Entry", false, |ui| {
                    let speed = app.spinner_speed(0.1);
                    let ke = &mut app.keyboard_entry;
                    ui.horizontal(|ui| {
                        for (v, l) in ke.position.iter_mut().zip(["X:", "Y:", "Z:"]) {
                            ui.label(l);
                            ui.add(crate::spinner::spin(ui, v, ..).speed(speed));
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Size:");
                        for v in ke.size.iter_mut() {
                            ui.add(crate::spinner::spin(ui, v, 0.0..=1e6).speed(speed));
                        }
                    });
                    let (position, size) = (ke.position, ke.size);
                    if ui.button("Create").clicked() && app.dispatch(Command::CreateShape { kind, position, size }).is_ok() {
                        app.creation_finished();
                    }
                });
            }
            parameters(ui, app);
        }
        Category::Cameras => {
            rollout(ui, "Object Type", true, |ui| {
                autogrid_and_pin(ui, app);
                ui.horizontal(|ui| {
                    for (label, target) in [("Target", true), ("Free", false)] {
                        let kind = CreateKind::Camera { target };
                        let on = app.create_kind == Some(kind);
                        if ui.add_sized([110.0, 22.0], egui::SelectableLabel::new(on, label)).clicked() {
                            // Select Object first: it ends any create mode, then this one starts.
                            let _ = app.dispatch(Command::SelectObject);
                            app.create_kind = if on { None } else { Some(kind) };
                        }
                    }
                });
            });
            name_and_color(ui, app);
            parameters(ui, app);
        }
        Category::Helpers => {
            rollout(ui, "Object Type", true, |ui| {
                autogrid_and_pin(ui, app);
                egui::Grid::new("helper_type").num_columns(2).show(ui, |ui| {
                    for (i, (kind, label)) in [(crate::HelperKind::Dummy, "Dummy"), (crate::HelperKind::Point, "Point")].iter().enumerate() {
                        let on = app.create_kind == Some(CreateKind::Helper(*kind));
                        if ui.add_sized([110.0, 22.0], egui::SelectableLabel::new(on, *label)).clicked() {
                            app.start_create((!on).then_some(CreateKind::Helper(*kind)));
                        }
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
            });
            name_and_color(ui, app);
            parameters(ui, app);
        }
        Category::Systems => {
            rollout(ui, "Object Type", true, |ui| {
                autogrid_and_pin(ui, app);
                ui.horizontal(|ui| {
                    let on = app.create_kind == Some(CreateKind::Bones);
                    if ui.add_sized([110.0, 22.0], egui::SelectableLabel::new(on, "Bones")).clicked() {
                        app.bone_points.clear();
                        app.start_create((!on).then_some(CreateKind::Bones));
                    }
                    let on = app.create_kind == Some(CreateKind::Biped);
                    if ui.add_sized([110.0, 22.0], egui::SelectableLabel::new(on, "Biped")).clicked() {
                        app.start_create((!on).then_some(CreateKind::Biped));
                    }
                });
            });
            name_and_color(ui, app);
            if app.create_kind == Some(CreateKind::Biped) {
                super::biped::create_rollout(ui, app);
                return;
            }
            if app.create_kind == Some(CreateKind::Bones) {
                rollout(ui, "IK Chain Assignment", true, |ui| {
                    ui.label("Click joints in a viewport; right-click ends the chain.");
                    if ui.button("Bone Tools...").clicked() {
                        let _ = app.dispatch(Command::BoneTools);
                    }
                });
            }
            parameters(ui, app);
        }
        // Lights and Space Warps have no object types (out of scope); they show an empty Object Type rollout.
        _ => {
            rollout(ui, "Object Type", true, |ui| pin(ui, app));
        }
    }
}

/// The Object Type rollout's top row: AutoGrid and the Pin.
fn autogrid_and_pin(ui: &mut egui::Ui, app: &mut LoomApp) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut app.autogrid, "AutoGrid");
        pin(ui, app);
    });
}

/// The Pin (Keep Creating): on, the create tool stays on after each object.
fn pin(ui: &mut egui::Ui, app: &mut LoomApp) {
    use crate::shortcuts::HoverShortcut;
    let r = ui.add(egui::SelectableLabel::new(app.state.prefs.keep_creating, "Pin"));
    if r.on_hover_cmd(&app.state, "Keep Creating (pin): the create tool stays on after each object", "Keep Creating Toggle").clicked() {
        let _ = app.dispatch(Command::KeepCreatingToggle);
    }
}

pub fn name_and_color(ui: &mut egui::Ui, app: &mut LoomApp) {
    rollout(ui, "Name and Color", true, |ui| {
        let Some(id) = app.state.selection.iter().next().copied() else {
            ui.label("");
            return;
        };
        let Some(node) = app.state.scene.node(id) else { return };
        let mut name = node.name.clone();
        let mut rgb = node.wire_color;
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(170.0));
            if r.lost_focus() && name != app.state.scene.node(id).map(|n| n.name.clone()).unwrap_or_default() {
                let _ = app.dispatch(Command::Rename { id: id.0, name: name.clone() });
            }
            if ui.color_edit_button_srgb(&mut rgb).changed() {
                let _ = app.dispatch(Command::SetWireColor { id: id.0, rgb });
            }
        });
    });
}

fn keyboard_entry(ui: &mut egui::Ui, app: &mut LoomApp, kind: PrimitiveKind) {
    rollout(ui, "Keyboard Entry", false, |ui| {
        let speed = app.spinner_speed(0.1);
        let ke = &mut app.keyboard_entry;
        ui.horizontal(|ui| {
            for (v, l) in ke.position.iter_mut().zip(["X:", "Y:", "Z:"]) {
                ui.label(l);
                ui.add(crate::spinner::spin(ui, v, ..).speed(speed));
            }
        });
        if kind == PrimitiveKind::Box {
            // size is width X, length Y, height Z; the rollout lists Length, Width, Height.
            egui::Grid::new("box_keyboard_entry").num_columns(2).show(ui, |ui| {
                for (i, l) in [(1, "Length:"), (0, "Width:"), (2, "Height:")] {
                    ui.label(l);
                    ui.add(crate::spinner::spin(ui, &mut ke.size[i], 0.0..=1e6).speed(speed));
                    ui.end_row();
                }
            });
        } else {
            ui.horizontal(|ui| {
                ui.label("Size:");
                for v in ke.size.iter_mut() {
                    ui.add(crate::spinner::spin(ui, v, 0.0..=1e6).speed(speed));
                }
            });
        }
        let (position, size) = (ke.position, ke.size);
        if ui.button("Create").clicked() && app.dispatch(Command::CreatePrimitive { kind, position, size }).is_ok() {
            app.creation_finished();
        }
    });
}

/// Parameters rollout of the selected parametric object; spinner drags are
/// one undo entry each.
pub fn parameters(ui: &mut egui::Ui, app: &mut LoomApp) {
    let Some(id) = app.state.selection.iter().next().copied() else { return };
    let Some(node) = app.state.scene.node(id) else { return };
    let params = node.object.parameters();
    if matches!(node.object, loom_scene::ObjectKind::Camera { .. }) {
        camera_parameters(ui, app, id);
        return;
    }
    let (is_compound, is_shape) = (node.object.is_compound(), node.object.is_shape());
    if is_compound && !matches!(app.panel_tab, crate::Tab::Create) {
        super::compound::rollouts(ui, app, None);
    }
    if params.is_empty() {
        return;
    }
    if is_shape {
        // Shapes: Rendering, Interpolation and Parameters rollouts.
        let interp = ["Steps", "Optimize", "Adaptive"];
        let render: Vec<_> = loom_scene::node::SHAPE_COMMON_NAMES.iter().filter(|n| !interp.contains(n)).copied().collect();
        let own: Vec<_> = params.iter().filter(|(n, _)| !loom_scene::node::SHAPE_COMMON_NAMES.contains(n)).cloned().collect();
        let rend: Vec<_> = params.iter().filter(|(n, _)| render.contains(n)).cloned().collect();
        let intp: Vec<_> = params.iter().filter(|(n, _)| interp.contains(n)).cloned().collect();
        rollout(ui, "Rendering", false, |ui| param_grid(ui, app, id, "rendering", &rend));
        rollout(ui, "Interpolation", false, |ui| param_grid(ui, app, id, "interpolation", &intp));
        if !own.is_empty() {
            rollout(ui, "Parameters", true, |ui| param_grid(ui, app, id, "parameters", &own));
        }
        return;
    }
    rollout(ui, "Parameters", true, |ui| param_grid(ui, app, id, "parameters", &params));
}

/// One grid of spinners / checkboxes for the given parameters.
fn param_grid(ui: &mut egui::Ui, app: &mut LoomApp, id: loom_scene::NodeId, salt: &str, params: &[(&'static str, loom_scene::ParamValue)]) {
    let params = params.to_vec();
    {
        egui::Grid::new((salt, id.0)).num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            for (name, value) in params {
                ui.label(name);
                match value {
                    loom_scene::ParamValue::Float(f) => {
                        let mut v = f;
                        let length = ["Length", "Width", "Height", "Depth", "Radius", "Radius 1", "Radius 2"].contains(&name);
                        let r = if length {
                            ui.add(app.length_drag(ui, &mut v, app.spinner_speed(0.05)))
                        } else {
                            ui.add(crate::spinner::spin(ui, &mut v, ..).speed(app.spinner_speed(0.05)).fixed_decimals(app.state.prefs.spinner_precision))
                        };
                        spin(app, id, name, &r, v as f64, f as f64);
                    }
                    loom_scene::ParamValue::Int(i) if app.state.scene.node(id).and_then(|n| n.object.choices(name)).is_some() => {
                        let choices = app.state.scene.node(id).and_then(|n| n.object.choices(name)).unwrap();
                        ui.vertical(|ui| {
                            for (ci, c) in choices.iter().enumerate() {
                                if ui.radio(i as usize == ci, *c).clicked() && i as usize != ci {
                                    let _ = app.dispatch(Command::SetParameter { id: id.0, name: name.into(), value: ci as f64 });
                                }
                            }
                        });
                    }
                    loom_scene::ParamValue::Int(i) => {
                        let mut v = i;
                        let r = ui.add(crate::spinner::spin(ui, &mut v, ..).speed(app.spinner_speed(0.1)));
                        spin(app, id, name, &r, v as f64, i as f64);
                    }
                    loom_scene::ParamValue::Bool(b) => {
                        let mut v = b;
                        if ui.checkbox(&mut v, "").changed() {
                            let _ = app.dispatch(Command::SetParameter { id: id.0, name: name.into(), value: if v { 1.0 } else { 0.0 } });
                        }
                    }
                }
                ui.end_row();
            }
        });
    }
}

fn spin(app: &mut LoomApp, id: loom_scene::NodeId, name: &str, r: &egui::Response, new: f64, old: f64) {
    if r.drag_started() {
        app.state.undo.begin(name);
    }
    if r.changed() && (new - old).abs() > 1e-9 {
        let _ = app.dispatch(Command::SetParameter { id: id.0, name: name.into(), value: new });
    }
    if r.drag_stopped() {
        app.state.undo.end();
    }
}

/// The camera Parameters rollout: Lens / FOV, Orthographic Projection,
/// Stock Lenses, Type, Show Cone / Horizon, Environment Ranges, Clipping
/// Planes and Target Distance.
fn camera_parameters(ui: &mut egui::Ui, app: &mut LoomApp, id: loom_scene::NodeId) {
    use loom_scene::camera::{fov_from_direction, fov_in_direction, FOV_DIRECTIONS, RENDER_ASPECT, STOCK_LENSES};
    let Some(node) = app.state.scene.node(id) else { return };
    let loom_scene::ObjectKind::Camera { fov, target, fov_type, ortho, show_cone, show_horizon, show_ranges, near_range, far_range, clip_manually, near_clip, far_clip, target_distance } = &node.object else { return };
    let (fov, has_target, dir) = (fov.get(), target.is_some_and(|t| app.state.scene.node(t).is_some()), fov_type.get());
    let flags = [("Orthographic Projection", ortho.get()), ("Show Cone", show_cone.get()), ("Show Horizon", show_horizon.get())];
    let (ranges_on, ranges) = (show_ranges.get(), [("Near Range", near_range.get()), ("Far Range", far_range.get())]);
    let (clip_on, clips) = (clip_manually.get(), [("Near Clip", near_clip.get()), ("Far Clip", far_clip.get())]);
    let distance = loom_commands::camera::target_distance(&app.state.scene, id).unwrap_or(target_distance.get());
    let set = |app: &mut LoomApp, name: &str, value: f64| {
        let _ = app.dispatch(Command::SetParameter { id: id.0, name: name.into(), value });
    };
    let check = |ui: &mut egui::Ui, app: &mut LoomApp, name: &str, label: &str, on: bool| {
        let mut v = on;
        if ui.checkbox(&mut v, label).changed() {
            set(app, name, if v { 1.0 } else { 0.0 });
        }
    };
    let length = |ui: &mut egui::Ui, app: &mut LoomApp, name: &str, value: f32| {
        let mut v = value;
        let r = ui.add(app.length_drag(ui, &mut v, app.spinner_speed(0.5)));
        spin(app, id, name, &r, v as f64, value as f64);
    };
    rollout(ui, "Parameters", true, |ui| {
        let decimals = app.state.prefs.spinner_precision.max(3);
        egui::Grid::new(("camera_lens", id.0)).num_columns(3).spacing([6.0, 4.0]).show(ui, |ui| {
            ui.label("Lens:");
            let lens = loom_scene::camera::fov_to_lens(fov);
            let mut v = lens;
            let r = ui.add(crate::spinner::spin(ui, &mut v, 1.0..=100_000.0).speed(app.spinner_speed(0.1)).fixed_decimals(decimals).suffix("mm"));
            spin(app, id, "Lens", &r, v as f64, lens as f64);
            ui.end_row();
            // The FOV spinner shows the FOV measured in the chosen direction.
            ui.label("FOV:");
            let shown = fov_in_direction(fov, dir, RENDER_ASPECT);
            let mut v = shown;
            let r = ui.add(crate::spinner::spin(ui, &mut v, 1.0..=175.0).speed(app.spinner_speed(0.1)).fixed_decimals(decimals).suffix(" deg"));
            spin(app, id, "FOV", &r, fov_from_direction(v, dir, RENDER_ASPECT) as f64, fov as f64);
            egui::ComboBox::from_id_salt(("camera_fov_dir", id.0)).width(60.0).selected_text(&FOV_DIRECTIONS[dir.min(2) as usize][..1]).show_ui(ui, |ui| {
                for (i, d) in FOV_DIRECTIONS.iter().enumerate() {
                    if ui.selectable_label(dir == i as u32, *d).clicked() && dir != i as u32 {
                        set(app, "FOV Direction", i as f64);
                    }
                }
            });
            ui.end_row();
        });
        check(ui, app, "Orthographic Projection", "Orthographic Projection", flags[0].1);
        ui.separator();
        ui.label("Stock Lenses");
        egui::Grid::new(("camera_stock", id.0)).num_columns(3).spacing([4.0, 4.0]).show(ui, |ui| {
            for (i, mm) in STOCK_LENSES.iter().enumerate() {
                if ui.add_sized([52.0, 20.0], egui::Button::new(format!("{mm}mm"))).clicked() {
                    set(app, "Lens", *mm as f64);
                }
                if i % 3 == 2 {
                    ui.end_row();
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Type:");
            egui::ComboBox::from_id_salt(("camera_type", id.0)).selected_text(if has_target { "Target Camera" } else { "Free Camera" }).show_ui(ui, |ui| {
                for (label, t) in [("Target Camera", true), ("Free Camera", false)] {
                    if ui.selectable_label(has_target == t, label).clicked() && has_target != t {
                        let _ = app.dispatch(Command::CameraType { id: id.0, target: t });
                    }
                }
            });
        });
        check(ui, app, "Show Cone", "Show Cone", flags[1].1);
        check(ui, app, "Show Horizon", "Show Horizon", flags[2].1);
        ui.separator();
        ui.label("Environment Ranges");
        check(ui, app, "Show Ranges", "Show", ranges_on);
        egui::Grid::new(("camera_ranges", id.0)).num_columns(2).show(ui, |ui| {
            for (name, value) in ranges {
                ui.label(format!("{name}:"));
                length(ui, app, name, value);
                ui.end_row();
            }
        });
        ui.separator();
        ui.label("Clipping Planes");
        check(ui, app, "Clip Manually", "Clip Manually", clip_on);
        ui.add_enabled_ui(clip_on, |ui| {
            egui::Grid::new(("camera_clips", id.0)).num_columns(2).show(ui, |ui| {
                for (name, value) in clips {
                    ui.label(format!("{name}:"));
                    length(ui, app, name, value);
                    ui.end_row();
                }
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Target Distance:");
            if has_target {
                // A Target camera shows the distance to its target.
                let (factor, suffix) = app.unit_display();
                ui.label(format!("{:.*}{}", app.state.prefs.spinner_precision, distance as f64 * factor, suffix));
            } else {
                length(ui, app, "Target Distance", distance);
            }
        });
    });
}
