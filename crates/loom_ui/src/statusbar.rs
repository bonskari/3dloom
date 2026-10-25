//! The status bar: prompt line, Selection Lock, coordinate display
//! (Absolute/Offset, X Y Z), Grid size, Auto Key / Set Key, time controls
//! and the viewport navigation buttons.

use crate::icons::{self, Icon};
use crate::shortcuts::HoverShortcut;
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, SelectionMode, Tool};
use loom_input::NavMode;

fn nav(ui: &mut egui::Ui, app: &mut LoomApp, glyph: &str, hover: &str, mode: NavMode, cmd: Command) {
    let on = app.state.nav_mode == mode;
    if Icon::new(glyph, cmd.name()).tip(hover).on(on).show(ui, &app.state).clicked() {
        let _ = app.dispatch(cmd);
    }
}

/// A flyout entry: its glyph, the colour that tells the variants apart (none
/// for the plain tool), its tooltip and command.
type FlyoutItem = (&'static str, Option<egui::Color32>, &'static str, Command);

fn flyout_icon<'a>(glyph: &'a str, color: Option<egui::Color32>, hover: &'a str, cmd: &'a Command) -> Icon<'a> {
    let i = Icon::new(glyph, cmd.name()).tip(hover);
    match color {
        Some(c) => i.color(c),
        None => i,
    }
}

fn flyout<const N: usize>(ui: &mut egui::Ui, app: &mut LoomApp, id: &str, current: usize, items: [FlyoutItem; N]) {
    let (glyph, color, hover, cmd) = &items[current];
    let resp = flyout_icon(glyph, *color, hover, cmd).hint("right-click: flyout").show(ui, &app.state);
    if resp.clicked() {
        let _ = app.dispatch(cmd.clone());
    }
    let popup = ui.id().with(id);
    if resp.secondary_clicked() || resp.long_touched() {
        ui.memory_mut(|m| m.toggle_popup(popup));
    }
    egui::popup::popup_above_or_below_widget(ui, popup, &resp, egui::AboveOrBelow::Above, egui::PopupCloseBehavior::CloseOnClick, |ui| {
        for (g, color, t, c) in &items {
            if flyout_icon(g, *color, t, c).show(ui, &app.state).clicked() {
                let _ = app.dispatch(c.clone());
            }
        }
    });
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp) {
    if app.state.expert_mode {
        // The only control left in Expert Mode, in the bottom-right corner.
        egui::Area::new(egui::Id::new("cancel_expert_mode")).anchor(egui::Align2::RIGHT_BOTTOM, [-6.0, -6.0]).show(ctx, |ui| {
            if ui.button("Cancel Expert Mode").on_hover_cmd(&app.state, "Cancel Expert Mode", Command::ExpertModeToggle.name()).clicked() {
                let _ = app.dispatch(Command::ExpertModeToggle);
            }
        });
        return;
    }
    egui::TopBottomPanel::bottom("statusbar").show(ctx, |ui| {
        // The status bar is two rows: selection / lock / coordinates / grid over
        // the prompt line, then the animation and viewport navigation blocks.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let s = &app.state;
                    let n = s.selection.len();
                    let label = match (s.selection_mode, n) {
                        (SelectionMode::Object, 0) => "None Selected".to_owned(),
                        (SelectionMode::Object, 1) => {
                            let id = s.selection.iter().next().unwrap();
                            s.scene.node(*id).map(|n| n.name.clone()).unwrap_or_default()
                        }
                        (SelectionMode::Object, n) => format!("{n} Objects Selected"),
                        (mode, _) => format!("{} {} Selected", s.sub_selection.len(), s.level_label(mode)),
                    };
                    ui.label(label);
                    ui.separator();
                    // Selection Lock Toggle (Space).
                    let locked = app.state.selection_locked;
                    if Icon::new(if locked { icons::LOCKED } else { icons::UNLOCKED }, "Selection Lock Toggle").on(locked).show(ui, &app.state).clicked() {
                        let _ = app.dispatch(Command::SelectionLockToggle);
                    }
                    // Absolute / Offset coordinate display.
                    let offset = app.state.offset_mode;
                    if Icon::new(if offset { icons::OFFSET } else { icons::ABSOLUTE }, "Absolute/Offset Mode Transform Type-In").tip(if offset { "Offset Mode Transform Type-In" } else { "Absolute Mode Transform Type-In" }).on(offset).show(ui, &app.state).clicked() {
                        let _ = app.dispatch(Command::AbsoluteOffsetToggle);
                    }
                    coordinate_fields(ui, app);
                    ui.separator();
                    ui.label(format!("Grid = {}", app.state.home_grid.spacing));
                });
                ui.horizontal(|ui| {
                    let s = &app.state;
                    let prompt = match (&app.create, s.pick_mode, s.nav_mode) {
                        (Some(c), _, _) => c.prompt().to_owned(),
                        (None, Some(_), _) => "Click an object".to_owned(),
                        (None, None, NavMode::None) => s.status_prompt.clone(),
                        (None, None, m) => format!("{m:?}: drag in a viewport, right-click to end"),
                    };
                    match &app.last_error {
                        Some(err) => {
                            ui.colored_label(egui::Color32::from_rgb(255, 120, 90), err);
                        }
                        None => {
                            ui.label(prompt);
                        }
                    }
                });
            });
            // The animation and navigation blocks sit at the right edge: pad by
            // their width from the previous frame.
            let width_id = egui::Id::new("statusbar_right_width");
            let right_w: f32 = ui.ctx().data(|d| d.get_temp(width_id)).unwrap_or(0.0);
            ui.add_space((ui.available_width() - right_w).max(0.0));
            let r = ui.horizontal(|ui| {
                animation_controls(ui, app);
                ui.separator();
                navigation_controls(ui, app);
            });
            let w = r.response.rect.width();
            if (w - right_w).abs() > 0.5 {
                ui.ctx().data_mut(|d| d.insert_temp(width_id, w));
                ui.ctx().request_repaint();
            }
        });
    });
}

/// Viewport navigation controls, a 2 × 4 block. A camera view swaps in the
/// camera viewport controls, which move the camera itself.
fn navigation_controls(ui: &mut egui::Ui, app: &mut LoomApp) {
    if let loom_commands::ViewKind::Camera(id) = app.state.active().kind {
        return camera_controls(ui, app, id);
    }
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            nav(ui, app, icons::ZOOM, "Zoom Mode", NavMode::Zoom, Command::ZoomMode);
            nav(ui, app, icons::ZOOM_ALL, "Zoom All", NavMode::ZoomAll, Command::ZoomAll);
            let sel = Some(icons::SELECTED_VARIANT);
            flyout(ui, app, "zoom_extents", 0, [
                (icons::ZOOM_EXTENTS, None, "Zoom Extents", Command::ZoomExtents),
                (icons::ZOOM_EXTENTS, sel, "Zoom Extents Selected", Command::ZoomExtentsSelected),
            ]);
            flyout(ui, app, "zoom_extents_all", 0, [
                (icons::ZOOM_EXTENTS_ALL, None, "Zoom Extents All", Command::ZoomExtentsAll),
                (icons::ZOOM_EXTENTS_ALL, sel, "Zoom Extents All Selected", Command::ZoomExtentsAllSelected),
            ]);
        });
        ui.horizontal(|ui| {
            if app.state.active().is_orthographic() {
                nav(ui, app, icons::ZOOM_REGION, "Zoom Region Mode", NavMode::ZoomRegion, Command::ZoomRegionMode);
            } else {
                // Perspective: the Field-of-View / Zoom Region flyout.
                let current = usize::from(app.state.nav_mode == NavMode::ZoomRegion);
                flyout(ui, app, "fov_region", current, [
                    (icons::FIELD_OF_VIEW, None, "Field-of-View", Command::FieldOfViewMode),
                    (icons::ZOOM_REGION, None, "Zoom Region Mode", Command::ZoomRegionMode),
                ]);
            }
            nav(ui, app, icons::PAN, "Pan View", NavMode::Pan, Command::PanView);
            let orbit_current = match app.state.nav_mode { NavMode::OrbitSelected => 1, NavMode::OrbitSubObject => 2, _ => 0 };
            flyout(ui, app, "orbit", orbit_current, [
                (icons::ORBIT, None, "Orbit View Mode", Command::OrbitViewMode),
                (icons::ORBIT, Some(icons::SELECTED_VARIANT), "Orbit Selected", Command::OrbitSelectedMode),
                (icons::ORBIT, Some(icons::SUB_OBJECT_VARIANT), "Orbit SubObject", Command::OrbitSubObjectMode),
            ]);
            let max = app.state.maximized;
            if Icon::new(icons::MAXIMIZE, "Maximize Viewport Toggle").on(max).show(ui, &app.state).clicked() {
                let _ = app.dispatch(Command::MaximizeViewportToggle);
            }
        });
    });
}

/// Camera viewport controls: Dolly flyout (Target cameras also Dolly Target
/// and Dolly Camera + Target), Perspective, Roll Camera, Zoom Extents All;
/// Field-of-View, Truck Camera, Orbit / Pan Camera, Maximize.
fn camera_controls(ui: &mut egui::Ui, app: &mut LoomApp, id: loom_scene::NodeId) {
    let has_target = matches!(app.state.scene.node(id).map(|n| &n.object), Some(loom_scene::ObjectKind::Camera { target: Some(t), .. }) if app.state.scene.node(*t).is_some());
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            if has_target {
                let current = match app.state.nav_mode { NavMode::DollyTarget => 1, NavMode::DollyBoth => 2, _ => 0 };
                flyout(ui, app, "dolly", current, [
                    (icons::DOLLY, None, "Dolly Camera Mode", Command::DollyCameraMode),
                    (icons::DOLLY, Some(icons::SELECTED_VARIANT), "Dolly Target Mode", Command::DollyTargetMode),
                    (icons::DOLLY, Some(icons::SUB_OBJECT_VARIANT), "Dolly Camera + Target Mode", Command::DollyCameraTargetMode),
                ]);
            } else {
                nav(ui, app, icons::DOLLY, "Dolly Camera Mode", NavMode::DollyCamera, Command::DollyCameraMode);
            }
            nav(ui, app, icons::PERSPECTIVE_MODE, "Perspective Mode", NavMode::Perspective, Command::PerspectiveMode);
            nav(ui, app, icons::ROLL, "Roll Camera Mode", NavMode::RollCamera, Command::RollCameraMode);
            flyout(ui, app, "zoom_extents_all", 0, [
                (icons::ZOOM_EXTENTS_ALL, None, "Zoom Extents All", Command::ZoomExtentsAll),
                (icons::ZOOM_EXTENTS_ALL, Some(icons::SELECTED_VARIANT), "Zoom Extents All Selected", Command::ZoomExtentsAllSelected),
            ]);
        });
        ui.horizontal(|ui| {
            nav(ui, app, icons::FIELD_OF_VIEW, "Field-of-View", NavMode::Fov, Command::FieldOfViewMode);
            nav(ui, app, icons::TRUCK, "Truck Camera Mode", NavMode::TruckCamera, Command::TruckCameraMode);
            let current = usize::from(app.state.nav_mode == NavMode::PanCamera);
            flyout(ui, app, "orbit_camera", current, [
                (icons::ORBIT, None, "Orbit Camera Mode", Command::OrbitCameraMode),
                (icons::PAN_CAMERA, None, "Pan Camera Mode", Command::PanCameraMode),
            ]);
            let max = app.state.maximized;
            if Icon::new(icons::MAXIMIZE, "Maximize Viewport Toggle").on(max).show(ui, &app.state).clicked() {
                let _ = app.dispatch(Command::MaximizeViewportToggle);
            }
        });
    });
}

/// X/Y/Z Transform Type-In fields: position, rotation degrees or scale
/// percent for the active transform, absolute or offset.
fn coordinate_fields(ui: &mut egui::Ui, app: &mut LoomApp) {
    let has = !app.state.selection.is_empty();
    let offset = app.state.offset_mode;
    let tool = app.state.active_tool;
    let rest = if tool == Tool::Scale { glam::Vec3::splat(100.0) } else { glam::Vec3::ZERO };
    let current = if offset { rest } else { loom_commands::type_in_values(&app.state) };
    // Keep the typed buffer while a field is being edited.
    let buf_id = egui::Id::new("statusbar_type_in_buffer");
    let mut v: glam::Vec3 = ui.ctx().data(|d| d.get_temp(buf_id)).unwrap_or(current);
    let mut changed = false;
    let mut focused = false;
    ui.add_enabled_ui(has, |ui| {
        for (c, label) in [(&mut v.x, "X:"), (&mut v.y, "Y:"), (&mut v.z, "Z:")] {
            ui.label(label);
            let r = if tool == Tool::Move || tool == Tool::Select {
                ui.add(app.length_drag(ui, c, app.spinner_speed(0.1)))
            } else {
                ui.add(crate::spinner::spin(ui, c, ..).speed(app.spinner_speed(0.1)).fixed_decimals(app.state.prefs.spinner_precision))
            };
            changed |= r.lost_focus() || r.drag_stopped() || r.secondary_clicked();
            focused |= r.has_focus() || r.dragged();
        }
    });
    ui.ctx().data_mut(|d| if focused { d.insert_temp(buf_id, v) } else { d.remove::<glam::Vec3>(buf_id) });
    if changed && has && v != current {
        let _ = app.dispatch(Command::TransformTypeIn { absolute: !offset, values: v.to_array() });
    }
}

fn red(on: bool) -> egui::Color32 {
    if on { egui::Color32::from_rgb(200, 50, 50) } else { egui::Color32::from_gray(70) }
}

/// The animation controls: the big Set Keys button, Auto Key over
/// Set Key, the Selection List over the default tangents and Key Filters,
/// playback over Key Mode / current frame / Time Configuration.
fn animation_controls(ui: &mut egui::Ui, app: &mut LoomApp) {
    // The big key: Set Keys (K), red while Set Key mode waits for a pose.
    let big = egui::Button::new(egui::RichText::new(icons::SET_KEYS).size(26.0)).min_size(egui::vec2(44.0, 44.0)).fill(if app.state.set_key_mode { red(true) } else { egui::Color32::from_gray(70) });
    let r = ui.add(big);
    icons::named(&r, egui::WidgetType::Button, "Set Keys");
    if r.on_hover_name(&app.state, "Set Keys").clicked() {
        let _ = app.dispatch(Command::SetKeys);
    }
    ui.vertical(|ui| {
        let auto = app.state.auto_key;
        let set = app.state.set_key_mode;
        let w = egui::vec2(64.0, 0.0);
        if ui.add(egui::Button::new(egui::RichText::new("Auto Key").color(egui::Color32::WHITE)).fill(red(auto)).min_size(w)).on_hover_name(&app.state, "Auto Key Mode Toggle").clicked() {
            let _ = app.dispatch(Command::AutoKeyModeToggle);
        }
        if ui.add(egui::Button::new(egui::RichText::new("Set Key").color(egui::Color32::WHITE)).fill(red(set)).min_size(w)).on_hover_name(&app.state, "Set Key Mode").clicked() {
            let _ = app.dispatch(Command::SetKeyMode);
        }
    });
    ui.vertical(|ui| {
        selection_list(ui, app);
        ui.horizontal(|ui| {
            default_tangents(ui, app);
            key_filters(ui, app);
        });
    });
    ui.vertical(|ui| {
        ui.horizontal(|ui| playback_buttons(ui, app));
        ui.horizontal(|ui| {
            let key_mode = app.state.key_step_mode;
            let r = Icon::new(icons::KEY_MODE, Command::NextPreviousKeyModeToggle.name()).tip("Key Mode Toggle").on(key_mode).show(ui, &app.state);
            time_control(app, r, Command::NextPreviousKeyModeToggle);
            let mut frame = app.state.current_frame;
            if ui.add(crate::spinner::spin(ui, &mut frame, app.state.frame_range.0..=app.state.frame_range.1)).on_hover_text("Current Frame (Go To Frame)").changed() {
                let _ = app.dispatch(Command::SetFrame { frame });
            }
            let r = Icon::new(icons::TIME_CONFIGURATION, Command::TimeConfiguration.name()).tip("Time Configuration").show(ui, &app.state);
            time_control(app, r, Command::TimeConfiguration);
        });
    });
}

/// Selection List: which objects Set Keys keys ("Selected" or a named set).
fn selection_list(ui: &mut egui::Ui, app: &mut LoomApp) {
    let current = app.state.key_selection_list.clone();
    let names: Vec<String> = app.state.named_sets.keys().cloned().collect();
    egui::ComboBox::from_id_salt("key_selection_list")
        .selected_text(current.as_deref().unwrap_or("Selected"))
        .width(120.0)
        .show_ui(ui, |ui| {
            if ui.selectable_label(current.is_none(), "Selected").clicked() {
                let _ = app.dispatch(Command::SetKeySelectionList { name: None });
            }
            for n in names {
                if ui.selectable_label(current.as_deref() == Some(n.as_str()), &n).clicked() {
                    let _ = app.dispatch(Command::SetKeySelectionList { name: Some(n) });
                }
            }
        })
        .response
        .on_hover_text("Selection List");
}

fn key_filters(ui: &mut egui::Ui, app: &mut LoomApp) {
    let f = app.state.key_filters;
    ui.menu_button("Key Filters...", |ui| {
        for (label, on, cmd) in [
            ("All", f.position && f.rotation && f.scale && f.ik_parameters && f.object_params && f.custom_attributes && f.modifiers && f.materials && f.other, Command::SetKeyAllFilter),
            ("Position", f.position, Command::SetKeyPositionFilter),
            ("Rotation", f.rotation, Command::SetKeyRotationFilter),
            ("Scale", f.scale, Command::SetKeyScaleFilter),
            ("IK Parameters", f.ik_parameters, Command::SetKeyIkParametersFilter),
            ("Object Parameters", f.object_params, Command::SetKeyObjectParamsFilter),
            ("Custom Attributes", f.custom_attributes, Command::SetKeyCustomAttributesFilter),
            ("Modifiers", f.modifiers, Command::SetKeyModifiersFilter),
            ("Materials", f.materials, Command::SetKeyMaterialsFilter),
            ("Other", f.other, Command::SetKeyOtherFilter),
        ] {
            let mut v = on;
            if ui.checkbox(&mut v, label).clicked() {
                let _ = app.dispatch(cmd);
            }
        }
    });
}

/// Default In / Out Tangents for new keys.
fn default_tangents(ui: &mut egui::Ui, app: &mut LoomApp) {
    use loom_scene::anim::TangentType as T;
    let names = [(T::Auto, "Auto"), (T::Smooth, "Smooth"), (T::Linear, "Linear"), (T::Step, "Step"), (T::Fast, "Fast"), (T::Slow, "Slow"), (T::Custom, "Spline")];
    let (tin, tout) = app.state.default_tangent;
    let label = |t: T| names.iter().find(|(x, _)| *x == t).map(|(_, l)| *l).unwrap_or("Auto");
    ui.menu_button(format!("{} / {}", label(tin), label(tout)), |ui| {
        ui.label("New Keys: In / Out");
        ui.columns(2, |c| {
            for (t, l) in names {
                if c[0].radio(tin == t, l).clicked() {
                    let _ = app.dispatch(Command::SetDefaultTangents { tangent_in: Some(t), tangent_out: None });
                }
                if c[1].radio(tout == t, l).clicked() {
                    let _ = app.dispatch(Command::SetDefaultTangents { tangent_in: None, tangent_out: Some(t) });
                }
            }
        });
    })
    .response
    .on_hover_text("Default In/Out Tangents for New Keys");
}

/// Right-clicking a time control opens Time Configuration, as in the reference.
fn time_control(app: &mut LoomApp, r: egui::Response, cmd: Command) {
    if r.clicked() {
        let _ = app.dispatch(cmd);
    }
    if r.secondary_clicked() {
        let _ = app.dispatch(Command::TimeConfiguration);
    }
}

/// Go To Start, Previous, Play / Stop, Next, Go To End.
fn playback_buttons(ui: &mut egui::Ui, app: &mut LoomApp) {
    let r = Icon::new(icons::GO_TO_START, "Go to Start Frame").show(ui, &app.state);
    time_control(app, r, Command::GoToStartFrame);
    let key_mode = app.state.key_step_mode;
    let r = Icon::new(if key_mode { icons::PREVIOUS_KEY } else { icons::PREVIOUS_FRAME }, Command::BackupTimeOneUnit.name()).tip(if key_mode { "Previous Key" } else { "Previous Frame" }).show(ui, &app.state);
    time_control(app, r, Command::BackupTimeOneUnit);
    let playing = app.state.playing;
    let r = Icon::new(if playing { icons::STOP } else { icons::PLAY }, Command::PlayAnimation.name()).tip("Play / Stop Animation").on(playing).show(ui, &app.state);
    time_control(app, r, Command::PlayAnimation);
    let r = Icon::new(if key_mode { icons::NEXT_KEY } else { icons::NEXT_FRAME }, Command::ForwardTimeOneUnit.name()).tip(if key_mode { "Next Key" } else { "Next Frame" }).show(ui, &app.state);
    time_control(app, r, Command::ForwardTimeOneUnit);
    let r = Icon::new(icons::GO_TO_END, "Go to End Frame").show(ui, &app.state);
    time_control(app, r, Command::GoToEndFrame);
}
