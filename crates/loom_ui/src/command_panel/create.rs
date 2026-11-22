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
    /// The category's icon; its label is the tooltip.
    pub fn icon(self) -> &'static str {
        use crate::icons as i;
        match self {
            Category::Geometry => i::GEOMETRY,
            Category::Shapes => i::SHAPES,
            Category::Lights => i::LIGHTS,
            Category::Cameras => i::CAMERAS,
            Category::Helpers => i::HELPERS,
            Category::SpaceWarps => i::SPACE_WARPS,
            Category::Systems => i::SYSTEMS,
        }
    }
}

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    ui.horizontal_wrapped(|ui| {
        for c in Category::ALL {
            if crate::icons::Icon::new(c.icon(), c.label()).on(app.create_category == c).show(ui, &app.state).clicked() {
                app.create_category = c;
                app.create_kind = None;
            }
        }
    });
    match app.create_category {
        Category::Geometry if app.create_patch_grids => {
            egui::ComboBox::from_id_salt("create_geometry_class").selected_text("Patch Grids").show_ui(ui, |ui| {
                if ui.selectable_label(false, "Standard Primitives").clicked() {
                    app.create_patch_grids = false;
                    app.create_kind = None;
                }
                if ui.selectable_label(false, "Compound Objects").clicked() {
                    app.create_patch_grids = false;
                    app.create_compound = true;
                    app.create_kind = None;
                }
                let _ = ui.selectable_label(true, "Patch Grids");
            });
            rollout(ui, "Object Type", true, |ui| {
                autogrid_and_pin(ui, app);
                ui.horizontal(|ui| {
                    for (quad, label) in [(true, "Quad Patch"), (false, "Tri Patch")] {
                        let kind = CreateKind::PatchGrid { quad };
                        let on = app.create_kind == Some(kind);
                        if ui.add_sized([110.0, 22.0], egui::SelectableLabel::new(on, label)).clicked() {
                            app.start_create((!on).then_some(kind));
                        }
                    }
                });
            });
            name_and_color(ui, app);
            parameters(ui, app);
        }
        Category::Geometry if app.create_compound => {
            egui::ComboBox::from_id_salt("create_geometry_class").selected_text("Compound Objects").show_ui(ui, |ui| {
                if ui.selectable_label(false, "Standard Primitives").clicked() {
                    app.create_compound = false;
                    app.compound_kind = None;
                }
                let _ = ui.selectable_label(true, "Compound Objects");
                if ui.selectable_label(false, "Patch Grids").clicked() {
                    app.create_compound = false;
                    app.compound_kind = None;
                    app.create_patch_grids = true;
                }
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
        Category::Geometry if app.create_extended => {
            egui::ComboBox::from_id_salt("create_geometry_class").selected_text("Extended Primitives").show_ui(ui, |ui| {
                if ui.selectable_label(false, "Standard Primitives").clicked() {
                    app.create_extended = false;
                    app.create_kind = None;
                }
                let _ = ui.selectable_label(true, "Extended Primitives");
                if ui.selectable_label(false, "Compound Objects").clicked() {
                    app.create_extended = false;
                    app.create_compound = true;
                    app.create_kind = None;
                }
                if ui.selectable_label(false, "Patch Grids").clicked() {
                    app.create_extended = false;
                    app.create_patch_grids = true;
                    app.create_kind = None;
                }
            });
            rollout(ui, "Object Type", true, |ui| {
                autogrid_and_pin(ui, app);
                egui::Grid::new("extended_type").num_columns(2).show(ui, |ui| {
                    for (i, kind) in loom_mesh::extended_primitives::ExtendedKind::ALL.iter().enumerate() {
                        let on = app.create_kind == Some(CreateKind::Extended(*kind));
                        if ui.add_sized([110.0, 22.0], egui::SelectableLabel::new(on, kind.label())).clicked() {
                            app.start_create((!on).then_some(CreateKind::Extended(*kind)));
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
        Category::Geometry => {
            egui::ComboBox::from_id_salt("create_geometry_class").selected_text("Standard Primitives").show_ui(ui, |ui| {
                let _ = ui.selectable_label(true, "Standard Primitives");
                if ui.selectable_label(false, "Extended Primitives").clicked() {
                    app.create_extended = true;
                    app.create_kind = None;
                }
                if ui.selectable_label(false, "Compound Objects").clicked() {
                    app.create_compound = true;
                    app.create_kind = None;
                }
                if ui.selectable_label(false, "Patch Grids").clicked() {
                    app.create_patch_grids = true;
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
        Category::Shapes if app.create_extended_shapes => {
            egui::ComboBox::from_id_salt("create_shape_class").selected_text("Extended Shapes").show_ui(ui, |ui| {
                if ui.selectable_label(false, "Splines").clicked() {
                    app.create_extended_shapes = false;
                    app.create_kind = None;
                }
                let _ = ui.selectable_label(true, "Extended Shapes");
            });
            rollout(ui, "Object Type", true, |ui| {
                autogrid_and_pin(ui, app);
                egui::Grid::new("extended_shape_type").num_columns(2).show(ui, |ui| {
                    for (i, kind) in loom_shape::parametric::ExtendedShape::ALL.iter().enumerate() {
                        let on = app.create_kind == Some(CreateKind::ExtendedShape(*kind));
                        if ui.add_sized([110.0, 22.0], egui::SelectableLabel::new(on, kind.label())).clicked() {
                            app.start_create((!on).then_some(CreateKind::ExtendedShape(*kind)));
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
        Category::Shapes => {
            egui::ComboBox::from_id_salt("create_shape_class").selected_text("Splines").show_ui(ui, |ui| {
                let _ = ui.selectable_label(true, "Splines");
                if ui.selectable_label(false, "Extended Shapes").clicked() {
                    app.create_extended_shapes = true;
                    app.create_kind = None;
                }
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
                            ui.add(crate::spinner::spin(v, ..).speed(speed));
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Size:");
                        for v in ke.size.iter_mut() {
                            ui.add(crate::spinner::spin(v, 0.0..=1e6).speed(speed));
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
        Category::Lights => {
            let photometric = app.photometric_lights;
            egui::ComboBox::from_id_salt("create_light_class").selected_text(if photometric { "Photometric" } else { "Standard" }).show_ui(ui, |ui| {
                for (label, p) in [("Photometric", true), ("Standard", false)] {
                    if ui.selectable_label(photometric == p, label).clicked() && photometric != p {
                        app.photometric_lights = p;
                        app.start_create(None);
                    }
                }
            });
            let kinds: &[loom_commands::light::LightKind] = if photometric { &loom_commands::light::LightKind::PHOTOMETRIC } else { &loom_commands::light::LightKind::ALL };
            rollout(ui, "Object Type", true, |ui| {
                autogrid_and_pin(ui, app);
                egui::Grid::new("light_type").num_columns(2).show(ui, |ui| {
                    for (i, kind) in kinds.iter().enumerate() {
                        let on = app.create_kind == Some(CreateKind::Light(*kind));
                        if ui.add_sized([110.0, 22.0], egui::SelectableLabel::new(on, kind.label())).clicked() {
                            // Select Object first: it ends any create mode, then this one starts.
                            let _ = app.dispatch(Command::SelectObject);
                            app.create_kind = if on { None } else { Some(CreateKind::Light(*kind)) };
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
        Category::Helpers => {
            egui::ComboBox::from_id_salt("create_helper_class").selected_text(app.helper_class).show_ui(ui, |ui| {
                for label in ["Standard", "Atmospheric Apparatus", "Assembly Heads", "Camera Match"] {
                    if ui.selectable_label(app.helper_class == label, label).clicked() && app.helper_class != label {
                        app.helper_class = label;
                        app.start_create(None);
                    }
                }
            });
            let kinds: &[crate::HelperKind] = match app.helper_class {
                "Camera Match" => &[crate::HelperKind::CamPoint],
                "Assembly Heads" => &[crate::HelperKind::Luminaire],
                "Atmospheric Apparatus" => &[crate::HelperKind::BoxGizmo, crate::HelperKind::SphereGizmo, crate::HelperKind::CylGizmo],
                _ => &[crate::HelperKind::Dummy, crate::HelperKind::Point, crate::HelperKind::Grid, crate::HelperKind::Tape, crate::HelperKind::Protractor, crate::HelperKind::Compass],
            };
            rollout(ui, "Object Type", true, |ui| {
                autogrid_and_pin(ui, app);
                egui::Grid::new("helper_type").num_columns(2).show(ui, |ui| {
                    for (i, (kind, label)) in kinds.iter().map(|k| (k, k.label())).enumerate() {
                        let on = app.create_kind == Some(CreateKind::Helper(*kind));
                        if ui.add_sized([110.0, 22.0], egui::SelectableLabel::new(on, label)).clicked() {
                            app.start_create((!on).then_some(CreateKind::Helper(*kind)));
                        }
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
            });
            name_and_color(ui, app);
            if app.create_kind == Some(CreateKind::Helper(crate::HelperKind::CamPoint)) {
                // A CamPoint at typed coordinates (measured points of the photo's scene).
                rollout(ui, "Keyboard Entry", false, |ui| {
                    let speed = app.spinner_speed(0.1);
                    let ke = &mut app.keyboard_entry;
                    ui.horizontal(|ui| {
                        for (v, l) in ke.position.iter_mut().zip(["X:", "Y:", "Z:"]) {
                            ui.label(l);
                            ui.add(crate::spinner::spin(v, ..).speed(speed));
                        }
                    });
                    let position = ke.position;
                    if ui.button("Create").clicked() && app.dispatch(Command::CreateHelper { kind: "campoint".into(), position, size: 0.0 }).is_ok() {
                        app.creation_finished();
                    }
                });
            }
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
                ui.horizontal(|ui| {
                    let on = app.create_kind == Some(CreateKind::RingArray);
                    if ui.add_sized([110.0, 22.0], egui::SelectableLabel::new(on, "Ring Array")).clicked() {
                        app.start_create((!on).then_some(CreateKind::RingArray));
                    }
                });
                ui.horizontal(|ui| {
                    for (label, daylight) in [("Sunlight", false), ("Daylight", true)] {
                        let kind = CreateKind::SunSystem { daylight };
                        let on = app.create_kind == Some(kind);
                        if ui.add_sized([110.0, 22.0], egui::SelectableLabel::new(on, label)).clicked() {
                            app.start_create((!on).then_some(kind));
                        }
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
                    let (solver, children, root) = (app.state.bones_ik_solver, app.state.bones_ik_chain, app.state.bones_ik_root);
                    egui::ComboBox::from_id_salt("bones_ik_solver").selected_text(solver.label()).show_ui(ui, |ui| {
                        use loom_scene::ik::IkSolver as S;
                        for s in [S::HI, S::HD, S::Limb] {
                            if ui.selectable_label(s == solver, s.label()).clicked() {
                                let _ = app.dispatch(Command::BonesIkAssignment { solver: Some(s.label().into()), children: None, root: None });
                            }
                        }
                    });
                    let (mut c, mut r) = (children, root);
                    if ui.checkbox(&mut c, "Assign To Children").changed() {
                        let _ = app.dispatch(Command::BonesIkAssignment { solver: None, children: Some(c), root: None });
                    }
                    if ui.checkbox(&mut r, "Assign To Root").changed() {
                        let _ = app.dispatch(Command::BonesIkAssignment { solver: None, children: None, root: Some(r) });
                    }
                    ui.label("Click joints in a viewport; right-click ends the chain.");
                    if ui.button("Bone Tools...").clicked() {
                        let _ = app.dispatch(Command::BoneTools);
                    }
                });
            }
            parameters(ui, app);
        }
        // Space Warps have no object types (out of scope); they show an empty Object Type rollout.
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

/// The Pin's tooltip (and its name for screen readers).
pub const PIN_TIP: &str = "Keep Creating (pin): the create tool stays on after each object";

/// The Pin (Keep Creating): on, the create tool stays on after each object.
fn pin(ui: &mut egui::Ui, app: &mut LoomApp) {
    let pin = crate::icons::Icon::new(crate::icons::KEEP_CREATING, Command::KeepCreatingToggle.name()).tip(PIN_TIP).on(app.state.prefs.keep_creating);
    if pin.show(ui, &app.state).clicked() {
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
        let rgb = node.wire_color;
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(170.0));
            if r.lost_focus() && name != app.state.scene.node(id).map(|n| n.name.clone()).unwrap_or_default() {
                let _ = app.dispatch(Command::Rename { id: id.0, name: name.clone() });
            }
            if crate::dialogs::object_color::swatch(ui, rgb).clicked() {
                let _ = app.dispatch(Command::ObjectColorDialog);
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
                ui.add(crate::spinner::spin(v, ..).speed(speed));
            }
        });
        if kind == PrimitiveKind::Box {
            // size is width X, length Y, height Z; the rollout lists Length, Width, Height.
            egui::Grid::new("box_keyboard_entry").num_columns(2).show(ui, |ui| {
                for (i, l) in [(1, "Length:"), (0, "Width:"), (2, "Height:")] {
                    ui.label(l);
                    ui.add(crate::spinner::spin(&mut ke.size[i], 0.0..=1e6).speed(speed));
                    ui.end_row();
                }
            });
        } else {
            ui.horizontal(|ui| {
                ui.label("Size:");
                for v in ke.size.iter_mut() {
                    ui.add(crate::spinner::spin(v, 0.0..=1e6).speed(speed));
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
    if matches!(node.object, loom_scene::ObjectKind::Light(_)) {
        super::light_rollouts::show(ui, app, id);
        return;
    }
    if matches!(node.object, loom_scene::ObjectKind::Tape { .. }) {
        tape_parameters(ui, app, id);
        return;
    }
    if let loom_scene::ObjectKind::Luminaire { filter_color, .. } = &node.object {
        // Luminaire Parameters: Dimmer and Filter Color pass to the lights in its assembly.
        let filter = filter_color.get();
        let own: Vec<_> = params.iter().filter(|(n, _)| !n.starts_with("Filter Color")).cloned().collect();
        rollout(ui, "Luminaire Parameters", true, |ui| {
            param_grid(ui, app, id, "luminaire", &own);
            ui.horizontal(|ui| {
                ui.label("Filter Color:");
                super::light_rollouts::swatch(ui, app, id, "Filter Color", filter);
            });
        });
        return;
    }
    if matches!(node.object, loom_scene::ObjectKind::Protractor { .. }) {
        protractor_parameters(ui, app, id);
        return;
    }
    if let loom_scene::ObjectKind::AtmosApparatus { shape, seed, .. } = &node.object {
        let title = ["Box Gizmo Parameters", "Sphere Gizmo Parameters", "Cylinder Gizmo Parameters"][(*shape as usize).min(2)];
        let seed = seed.get();
        rollout(ui, title, true, |ui| {
            param_grid(ui, app, id, "apparatus", &params);
            // New Seed: another seed, picked from the current one.
            if ui.button("New Seed").clicked() {
                let next = (seed.wrapping_mul(1_103_515_245).wrapping_add(12_345) >> 3) % 100_000;
                let _ = app.dispatch(Command::SetParameter { id: id.0, name: "Seed".into(), value: next as f64 });
            }
        });
        if !matches!(app.panel_tab, crate::Tab::Create) {
            apparatus_atmospheres(ui, app, id);
        }
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

/// Atmospheric apparatus > Atmospheres & Effects: the Fire Effect / Volume
/// Fog entries this gizmo fills. Add opens Add Atmosphere (New or Existing),
/// Delete takes the gizmo out of the highlighted one, Setup opens it in
/// Environment and Effects.
fn apparatus_atmospheres(ui: &mut egui::Ui, app: &mut LoomApp, id: loom_scene::NodeId) {
    let state_id = egui::Id::new("apparatus_atmospheres").with(id.0);
    let entries: Vec<(usize, String)> =
        app.state.scene.environment.atmospheres.iter().enumerate().filter(|(_, a)| a.kind.gizmos().is_some_and(|g| g.contains(&id))).map(|(i, a)| (i, a.name.clone())).collect();
    rollout(ui, "Atmospheres & Effects", true, |ui| {
        let mut sel: Option<usize> = ui.ctx().data(|d| d.get_temp(state_id)).filter(|i| entries.iter().any(|e| e.0 == *i));
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_size(egui::vec2(160.0, 50.0));
            for (i, name) in &entries {
                if ui.selectable_label(sel == Some(*i), name.as_str()).clicked() {
                    sel = Some(*i);
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Add").clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("add_atmosphere"), (id.0, true, Some(0usize))));
            }
            if ui.add_enabled(sel.is_some(), egui::Button::new("Delete")).clicked() {
                if let Some(i) = sel.take() {
                    let _ = app.dispatch(Command::AtmosphereGizmo { index: i as u32, add: None, remove: Some(id.0) });
                }
            }
            if ui.add_enabled(sel.is_some(), egui::Button::new("Setup")).clicked() {
                if let Some(i) = sel {
                    ui.ctx().data_mut(|d| d.insert_temp(crate::dialogs::environment::sel_id("atmosphere"), i));
                    app.state.environment_ui.open = Some(loom_commands::environment::EnvironmentTab::Environment);
                }
            }
        });
        ui.ctx().data_mut(|d| match sel {
            Some(i) => d.insert_temp(state_id, i),
            None => d.remove::<usize>(state_id),
        });
    });
    add_atmosphere_dialog(ui.ctx(), app);
}

/// Add Atmosphere (from an atmospheric apparatus): New lists the kinds an
/// apparatus takes; Existing the Fire Effect / Volume Fog entries other
/// apparatus use, and adds a copy. OK adds it with this apparatus as its gizmo.
fn add_atmosphere_dialog(ctx: &egui::Context, app: &mut LoomApp) {
    use loom_scene::environment::AtmosphereKind;
    let key = egui::Id::new("add_atmosphere");
    let Some((gizmo, mut new, mut pick)) = ctx.data(|d| d.get_temp::<(u64, bool, Option<usize>)>(key)) else { return };
    let mut close = false;
    let existing: Vec<(usize, String)> = app
        .state
        .scene
        .environment
        .atmospheres
        .iter()
        .enumerate()
        .filter(|(_, a)| a.kind.gizmos().is_some_and(|g| !g.is_empty() && !g.contains(&loom_scene::NodeId(gizmo))))
        .map(|(i, a)| (i, a.name.clone()))
        .collect();
    egui::Window::new("Add Atmosphere").collapsible(false).resizable(false).show(ctx, |ui| {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_size(egui::vec2(180.0, 60.0));
            if new {
                for (i, name) in AtmosphereKind::GIZMO_NAMES.iter().enumerate() {
                    if ui.selectable_label(pick == Some(i), *name).clicked() {
                        pick = Some(i);
                    }
                }
            } else {
                for (i, name) in &existing {
                    if ui.selectable_label(pick == Some(*i), name.as_str()).clicked() {
                        pick = Some(*i);
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.radio(new, "New").clicked() && !new {
                new = true;
                pick = Some(0);
            }
            if ui.radio(!new, "Existing").clicked() && new {
                new = false;
                pick = existing.first().map(|e| e.0);
            }
        });
        ui.horizontal(|ui| {
            let ok = if new { pick.is_some_and(|i| i < AtmosphereKind::GIZMO_NAMES.len()) } else { pick.is_some_and(|i| existing.iter().any(|e| e.0 == i)) };
            if ui.add_enabled(ok, egui::Button::new("OK")).clicked() {
                let cmd = match (new, pick) {
                    (true, Some(i)) => Command::ApparatusAddAtmosphere { id: gizmo, kind: Some(AtmosphereKind::GIZMO_NAMES[i].into()), existing: None },
                    (_, i) => Command::ApparatusAddAtmosphere { id: gizmo, kind: None, existing: i.map(|i| i as u32) },
                };
                let _ = app.dispatch(cmd);
                close = true;
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
        });
    });
    ctx.data_mut(|d| if close { d.remove::<(u64, bool, Option<usize>)>(key) } else { d.insert_temp(key, (gizmo, new, pick)) });
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
                            ui.add(app.length_drag(&mut v, app.spinner_speed(0.05)))
                        } else {
                            ui.add(crate::spinner::spin(&mut v, ..).speed(app.spinner_speed(0.05)).fixed_decimals(app.state.prefs.spinner_precision))
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
                    // Dummy > Color: a swatch (stored as 0xRRGGBB).
                    loom_scene::ParamValue::Int(c) if name == "Color" && app.state.scene.node(id).is_some_and(|n| matches!(n.object, loom_scene::ObjectKind::Dummy { .. })) => {
                        let mut rgb = [(c >> 16) as u8, (c >> 8) as u8, c as u8];
                        if crate::color_selector::srgb(ui, &mut rgb).changed() {
                            let v = ((rgb[0] as u32) << 16) | ((rgb[1] as u32) << 8) | rgb[2] as u32;
                            let _ = app.dispatch(Command::SetParameter { id: id.0, name: name.into(), value: v as f64 });
                        }
                    }
                    loom_scene::ParamValue::Int(i) => {
                        let mut v = i;
                        let r = ui.add(crate::spinner::spin(&mut v, ..).speed(app.spinner_speed(0.1)));
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

pub(super) fn spin(app: &mut LoomApp, id: loom_scene::NodeId, name: &str, r: &egui::Response, new: f64, old: f64) {
    // Shift+right-click: a key for this parameter at the current frame.
    if r.secondary_clicked() && r.ctx.input(|i| i.modifiers.shift) {
        let _ = app.dispatch(Command::KeyParameter { id: id.0, name: name.into(), modifier: None });
        return;
    }
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

/// Tape > Parameters: Length (the measured distance, read-only unless
/// Specify Length), Specify Length, and the World Space Angles read-outs.
fn tape_parameters(ui: &mut egui::Ui, app: &mut LoomApp, id: loom_scene::NodeId) {
    let Some(loom_scene::ObjectKind::Tape { specify_length, length, .. }) = app.state.scene.node(id).map(|n| &n.object) else { return };
    let (specified, typed) = (specify_length.get(), length.get());
    let r = loom_commands::helpers::readout(&app.state, id).unwrap_or_default();
    let f = |v: &serde_json::Value| v.as_f64().unwrap_or(0.0) as f32;
    rollout(ui, "Parameters", true, |ui| {
        egui::Grid::new(("tape_params", id.0)).num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("Length:");
            let shown = if specified { typed } else { f(&r["length"]) };
            let mut v = shown;
            let resp = ui.add_enabled(specified, app.length_drag(&mut v, app.spinner_speed(0.5)));
            spin(app, id, "Length", &resp, v as f64, shown as f64);
            ui.end_row();
        });
        let mut on = specified;
        if ui.checkbox(&mut on, "Specify Length").changed() {
            // Turning it on starts from the measured length.
            if on {
                let _ = app.dispatch(Command::SetParameter { id: id.0, name: "Length".into(), value: r["length"].as_f64().unwrap_or(0.0) });
            }
            let _ = app.dispatch(Command::SetParameter { id: id.0, name: "Specify Length".into(), value: if on { 1.0 } else { 0.0 } });
        }
        ui.separator();
        ui.label("World Space Angles");
        egui::Grid::new(("tape_angles", id.0)).num_columns(2).spacing([8.0, 2.0]).show(ui, |ui| {
            for (i, name) in ["To X Axis:", "To Y Axis:", "To Z Axis:"].iter().enumerate() {
                ui.label(*name);
                ui.label(format!("{:.3}", f(&r["to_axis"][i])));
                ui.end_row();
            }
            for (i, name) in ["To XY Plane:", "To YZ Plane:", "To ZX Plane:"].iter().enumerate() {
                ui.label(*name);
                ui.label(format!("{:.3}", f(&r["to_plane"][i])));
                ui.end_row();
            }
        });
    });
}

/// Protractor > Parameters: the picked objects' names above Pick Object 1 /
/// Pick Object 2, and the Angle read-out.
fn protractor_parameters(ui: &mut egui::Ui, app: &mut LoomApp, id: loom_scene::NodeId) {
    let r = loom_commands::helpers::readout(&app.state, id).unwrap_or_default();
    let picking = app.state.pick_mode;
    rollout(ui, "Parameters", true, |ui| {
        for which in [1u32, 2] {
            ui.label(r[format!("object{which}")].as_str().unwrap_or("None").to_owned());
            let on = picking == Some(loom_commands::PickMode::ProtractorObject(which as u8));
            let label = format!("Pick Object {which}");
            if ui.add_sized([150.0, 22.0], egui::SelectableLabel::new(on, label.as_str())).clicked() {
                if on {
                    let _ = app.dispatch(Command::ExitPickMode);
                } else if let Err(e) = app.dispatch(Command::ProtractorPickObject { which }) {
                    app.state.status_prompt = e;
                }
            }
        }
        ui.horizontal(|ui| {
            ui.label("Angle:");
            ui.label(r["angle"].as_f64().map(|a| format!("{a:.3}")).unwrap_or_else(|| "0.0".into()));
        });
    });
}

/// The camera Parameters rollout: Lens / FOV, Orthographic Projection,
/// Stock Lenses, Type, Show Cone / Horizon, Environment Ranges, Clipping
/// Planes and Target Distance.
fn camera_parameters(ui: &mut egui::Ui, app: &mut LoomApp, id: loom_scene::NodeId) {
    use loom_scene::camera::{fov_from_direction, fov_in_direction, FOV_DIRECTIONS, STOCK_LENSES};
    let Some(node) = app.state.scene.node(id) else { return };
    let loom_scene::ObjectKind::Camera { fov, target, fov_type, ortho, show_cone, show_horizon, show_ranges, near_range, far_range, clip_manually, near_clip, far_clip, target_distance, multi_pass, mp_use_target, mp_focal_depth, mp_passes, mp_radius, mp_effect, mp_blur } = &node.object else { return };
    let (mp_on, mp_target, mp_depth, mp_n, mp_r) = (multi_pass.get(), mp_use_target.get(), mp_focal_depth.get(), mp_passes.get(), mp_radius.get());
    let (effect, blur) = (mp_effect.get(), mp_blur.clone());
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
        let r = ui.add(app.length_drag(&mut v, app.spinner_speed(0.5)));
        spin(app, id, name, &r, v as f64, value as f64);
    };
    rollout(ui, "Parameters", true, |ui| {
        let decimals = app.state.prefs.spinner_precision.max(3);
        egui::Grid::new(("camera_lens", id.0)).num_columns(3).spacing([6.0, 4.0]).show(ui, |ui| {
            ui.label("Lens:");
            // Measured against Render Setup's Aperture Width: the view stays, the reading follows.
            let aperture = app.state.scene.render.aperture_width;
            let lens = loom_commands::render_output::lens_for(fov, aperture);
            let mut v = lens;
            let r = ui.add(crate::spinner::spin(&mut v, 1.0..=100_000.0).speed(app.spinner_speed(0.1)).fixed_decimals(decimals).suffix("mm"));
            spin(app, id, "FOV", &r, loom_commands::render_output::fov_for(v, aperture).clamp(1.0, 175.0) as f64, fov as f64);
            ui.end_row();
            // The FOV spinner shows the FOV measured in the chosen direction,
            // at Render Setup's image aspect.
            ui.label("FOV:");
            let aspect = app.state.scene.render.image_aspect();
            let shown = fov_in_direction(fov, dir, aspect);
            let mut v = shown;
            let r = ui.add(crate::spinner::spin(&mut v, 1.0..=175.0).speed(app.spinner_speed(0.1)).fixed_decimals(decimals).suffix(" deg"));
            spin(app, id, "FOV", &r, fov_from_direction(v, dir, aspect) as f64, fov as f64);
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
                    let aperture = app.state.scene.render.aperture_width;
                    set(app, "FOV", loom_commands::render_output::fov_for(*mm, aperture).clamp(1.0, 175.0) as f64);
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
            // A Target camera's spinner moves its target along the line of sight.
            length(ui, app, "Target Distance", distance);
        });
        ui.separator();
        // Multi-Pass Effect: Depth of Field or Motion Blur.
        ui.label("Multi-Pass Effect");
        ui.horizontal(|ui| {
            check(ui, app, "Multi-Pass Enable", "Enable", mp_on);
            const EFFECTS: [&str; 2] = ["Depth of Field", "Motion Blur"];
            egui::ComboBox::from_id_salt(("camera_mp_effect", id.0)).selected_text(EFFECTS[effect.min(1) as usize]).show_ui(ui, |ui| {
                for (i, e) in EFFECTS.iter().enumerate() {
                    if ui.selectable_label(effect == i as u32, *e).clicked() && effect != i as u32 {
                        set(app, "Multi-Pass Effect", i as f64);
                    }
                }
            });
        });
        if effect == 1 {
            ui.label("Motion Blur Parameters");
            ui.add_enabled_ui(mp_on, |ui| {
                ui.label("Sampling");
                ui.add_enabled(false, egui::Checkbox::new(&mut blur.display_passes.get(), "Display Passes")).on_disabled_hover_text("The rendered frame window shows the final result only");
                let float = |ui: &mut egui::Ui, app: &mut LoomApp, name: &str, value: f32, range: std::ops::RangeInclusive<f32>| {
                    let mut v = value;
                    let r = ui.add(crate::spinner::spin(&mut v, range).speed(app.spinner_speed(0.01)).fixed_decimals(2));
                    spin(app, id, name, &r, v as f64, value as f64);
                };
                egui::Grid::new(("camera_motion_blur", id.0)).num_columns(2).show(ui, |ui| {
                    ui.label("Total Passes:");
                    let mut n = blur.passes.get();
                    if ui.add(crate::spinner::spin(&mut n, ..).range(1..=64)).changed() {
                        set(app, "Motion Blur Total Passes", n as f64);
                    }
                    ui.end_row();
                    ui.label("Duration (frames):");
                    float(ui, app, "Duration (frames)", blur.duration.get(), 0.0..=1000.0);
                    ui.end_row();
                    ui.label("Bias:");
                    float(ui, app, "Bias", blur.bias.get(), 0.01..=0.99);
                    ui.end_row();
                });
                ui.label("Pass Blending");
                check(ui, app, "Normalize Weights", "Normalize Weights", blur.normalize_weights.get());
                egui::Grid::new(("camera_motion_blur_dither", id.0)).num_columns(2).show(ui, |ui| {
                    ui.label("Dither Strength:");
                    float(ui, app, "Dither Strength", blur.dither.get(), 0.0..=1.0);
                    ui.end_row();
                    ui.label("Tile Size:");
                    let mut n = blur.tile_size.get();
                    if ui.add(crate::spinner::spin(&mut n, ..).range(0..=100)).changed() {
                        set(app, "Tile Size", n as f64);
                    }
                    ui.end_row();
                });
                ui.label("Scanline Renderer Params");
                check(ui, app, "Disable Filtering", "Disable Filtering", blur.disable_filtering.get());
                check(ui, app, "Disable Antialiasing", "Disable Antialiasing", blur.disable_antialiasing.get());
            });
            return;
        }
        ui.label("Depth of Field Parameters");
        ui.add_enabled_ui(mp_on, |ui| {
            check(ui, app, "Use Target Distance", "Use Target Distance", mp_target);
            egui::Grid::new(("camera_multi_pass", id.0)).num_columns(2).show(ui, |ui| {
                ui.label("Focal Depth:");
                length(ui, app, "Focal Depth", mp_depth);
                ui.end_row();
                ui.label("Total Passes:");
                let mut n = mp_n;
                if ui.add(crate::spinner::spin(&mut n, ..).range(1..=64)).changed() {
                    set(app, "Total Passes", n as f64);
                }
                ui.end_row();
                ui.label("Sample Radius:");
                length(ui, app, "Sample Radius", mp_r);
                ui.end_row();
            });
        });
    });
}
