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
    RED.with(|c| c.set(app.color("Auto Key Background")));
    egui::TopBottomPanel::bottom("statusbar").show(ctx, |ui| {
        // The status bar is two rows: selection / lock / coordinates / grid over
        // the prompt line, then the animation and viewport navigation blocks.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(selection_status(&app.state));
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
                    ui.label(format!("Grid = {}", app.state.format_units(app.state.grid_spacing())));
                    ui.separator();
                    // Adaptive Degradation: gold while on, aqua while it degrades the views.
                    let d = &app.state.degradation;
                    let fill = if app.degrading { egui::Color32::from_rgb(60, 200, 210) } else if d.on { egui::Color32::from_rgb(210, 170, 40) } else { egui::Color32::from_gray(70) };
                    let text = egui::RichText::new("AD").color(if d.on { egui::Color32::BLACK } else { egui::Color32::WHITE });
                    let r = ui.add(egui::Button::new(text).fill(fill));
                    icons::named(&r, egui::WidgetType::Button, Command::AdaptiveDegradationToggle.name());
                    let r = r.on_hover_name(&app.state, Command::AdaptiveDegradationToggle.name());
                    if r.clicked() {
                        let _ = app.dispatch(Command::AdaptiveDegradationToggle);
                    }
                    if r.secondary_clicked() {
                        let _ = app.dispatch(Command::ConfigureViewports { panel: Some("Adaptive Degradation".into()) });
                    }
                });
                ui.horizontal(|ui| {
                    let s = &app.state;
                    // Tooltips show on the prompt line as the pointer passes over buttons.
                    let hovered: Option<String> = ui.ctx().data(|d| d.get_temp(egui::Id::new("prompt_line_tooltip_last")));
                    let prompt = match (&app.create, s.pick_mode, s.nav_mode) {
                        _ if hovered.is_some() => hovered.unwrap_or_default(),
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

/// The status line: "None Selected", the count and type when every selected
/// object is of one type ("2 Cameras Selected"), else "6 Objects Selected";
/// at a sub-object level the sub-objects.
pub fn selection_status(s: &loom_commands::AppState) -> String {
    let n = s.selection.len();
    if s.selection_mode != SelectionMode::Object {
        return format!("{} {} Selected", s.sub_selection.len(), s.level_label(s.selection_mode));
    }
    if n == 0 {
        return "None Selected".to_owned();
    }
    let kind = |id: &loom_scene::NodeId| {
        let o = s.scene.node(*id).map(|n| &n.object);
        match o {
            Some(loom_scene::ObjectKind::Camera { .. }) => "Camera",
            Some(loom_scene::ObjectKind::Light(_)) => "Light",
            Some(loom_scene::ObjectKind::Group { .. }) => "Group",
            Some(o) if o.is_shape() => "Shape",
            Some(o) if o.is_bone() => "Bone",
            Some(o) if o.is_helper() => "Helper",
            _ => "Object",
        }
    };
    let kinds: std::collections::BTreeSet<&str> = s.selection.iter().map(kind).collect();
    let k = if kinds.len() == 1 { *kinds.iter().next().expect("one kind") } else { "Object" };
    format!("{n} {k}{} Selected", if n == 1 { "" } else { "s" })
}

/// X/Y/Z Transform Type-In fields: position, rotation degrees or scale
/// percent for the active transform, absolute or offset. Moving over the
/// active viewport with nothing selected, or while creating, they show the
/// cursor's world position; during a transform drag, the drag's offset.
fn coordinate_fields(ui: &mut egui::Ui, app: &mut LoomApp) {
    let readout = if let Some(d) = app.transform_drag {
        Some(match app.state.active_tool {
            Tool::Rotate => (d.rotate_deg, false),
            Tool::Scale => (d.factors * 100.0, false),
            _ => (d.offset, true),
        })
    } else if app.state.selection.is_empty() || app.create.is_some() {
        let p = ui.ctx().input(|i| i.pointer.hover_pos());
        p.and_then(|p| crate::viewport_widget::cursor_world(app, p)).map(|w| (w, true))
    } else {
        None
    };
    if readout.is_some() || app.state.selection.is_empty() {
        // Read only: the cursor or the drag (blank with nothing to show).
        ui.add_enabled_ui(false, |ui| {
            for (i, label) in ["X:", "Y:", "Z:"].iter().enumerate() {
                ui.label(*label);
                let text = match readout {
                    Some((v, true)) => app.state.format_units(v[i]),
                    Some((v, false)) => format!("{:.*}", app.state.prefs.spinner_precision, v[i]),
                    None => String::new(),
                };
                let mut t = text;
                ui.add(egui::TextEdit::singleline(&mut t).desired_width(64.0));
            }
        });
        return;
    }
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
    if on { RED.with(|c| c.get()) } else { egui::Color32::from_gray(70) }
}

thread_local! {
    /// Auto Key Background (Customize User Interface > Colors), set each frame.
    static RED: std::cell::Cell<egui::Color32> = const { std::cell::Cell::new(egui::Color32::from_rgb(200, 50, 50)) };
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
    time_tag_box(ui, app);
}

/// The Add / Edit Time Tag dialog being filled in.
#[derive(Debug, Clone, PartialEq)]
pub struct TagDialog {
    /// Edit: the tag being edited (None: Add Time Tag).
    pub editing: Option<String>,
    pub name: String,
    pub time: i32,
    pub lock: bool,
    pub relative_to: String,
}

/// The Time Tag box: the tag at the current frame; its list has Add Tag,
/// Edit Tag and every tag (a click jumps to it).
fn time_tag_box(ui: &mut egui::Ui, app: &mut LoomApp) {
    let tags = app.state.time_tags.clone();
    let frame = app.state.current_frame;
    let here = tags.iter().find(|t| loom_commands::time_tags::tag_time(&tags, &t.name) == Some(frame)).map(|t| t.name.clone()).unwrap_or_default();
    let r = egui::ComboBox::from_id_salt("time_tag_box").selected_text(here).width(90.0).show_ui(ui, |ui| {
        if ui.button("Add Tag").clicked() {
            app.tag_dialog = Some(TagDialog { editing: None, name: format!("Tag {}", tags.len() + 1), time: frame, lock: false, relative_to: String::new() });
            ui.close_menu();
        }
        if ui.add_enabled(!tags.is_empty(), egui::Button::new("Edit Tag")).clicked() {
            let t = tags.iter().find(|t| loom_commands::time_tags::tag_time(&tags, &t.name) == Some(frame)).or(tags.first()).cloned();
            if let Some(t) = t {
                let time = loom_commands::time_tags::tag_time(&tags, &t.name).unwrap_or(t.time);
                app.tag_dialog = Some(TagDialog { editing: Some(t.name.clone()), name: t.name.clone(), time, lock: t.locked, relative_to: t.relative_to.clone().unwrap_or_default() });
            }
            ui.close_menu();
        }
        if !tags.is_empty() {
            ui.separator();
        }
        for t in &tags {
            let f = loom_commands::time_tags::tag_time(&tags, &t.name).unwrap_or(t.time);
            if ui.selectable_label(f == frame, format!("{}  ({f})", t.name)).clicked() {
                let _ = app.dispatch(Command::GoToTimeTag { name: t.name.clone() });
            }
        }
    });
    icons::named(&r.response, egui::WidgetType::ComboBox, "Time Tag");
    r.response.on_hover_text("Time Tag");
    tag_dialog(ui.ctx(), app);
}

/// Add Time Tag / Edit Time Tag: Name, Time, Lock Time, Relative To (and Delete when editing).
fn tag_dialog(ctx: &egui::Context, app: &mut LoomApp) {
    let Some(mut d) = app.tag_dialog.clone() else { return };
    let tags = app.state.time_tags.clone();
    let title = if d.editing.is_some() { "Edit Time Tag" } else { "Add Time Tag" };
    let mut open = true;
    let mut done = false;
    egui::Window::new(title).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        if let Some(editing) = d.editing.clone() {
            ui.horizontal(|ui| {
                ui.label("Tag:");
                egui::ComboBox::from_id_salt("edit_tag_pick").selected_text(editing.clone()).show_ui(ui, |ui| {
                    for t in &tags {
                        if ui.selectable_label(t.name == editing, &t.name).clicked() {
                            let time = loom_commands::time_tags::tag_time(&tags, &t.name).unwrap_or(t.time);
                            d = TagDialog { editing: Some(t.name.clone()), name: t.name.clone(), time, lock: t.locked, relative_to: t.relative_to.clone().unwrap_or_default() };
                        }
                    }
                });
            });
        }
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(140.0));
        });
        ui.horizontal(|ui| {
            ui.label("Time:");
            ui.add_enabled(!(d.lock && d.editing.is_some()), crate::spinner::spin(ui, &mut d.time, ..));
        });
        ui.checkbox(&mut d.lock, "Lock Time");
        ui.horizontal(|ui| {
            ui.label("Relative To:");
            let shown = if d.relative_to.is_empty() { "None".to_owned() } else { d.relative_to.clone() };
            egui::ComboBox::from_id_salt("tag_relative_to").selected_text(shown).show_ui(ui, |ui| {
                ui.selectable_value(&mut d.relative_to, String::new(), "None");
                for t in tags.iter().filter(|t| Some(&t.name) != d.editing.as_ref()) {
                    ui.selectable_value(&mut d.relative_to, t.name.clone(), &t.name);
                }
            });
        });
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                let r = match &d.editing {
                    None => app.dispatch(Command::AddTimeTag { name: d.name.clone(), time: Some(d.time), lock: Some(d.lock), relative_to: (!d.relative_to.is_empty()).then(|| d.relative_to.clone()) }),
                    Some(name) => {
                        let old_time = loom_commands::time_tags::tag_time(&tags, name);
                        // Unlock first so a new time can apply, lock last.
                        let _ = app.dispatch(Command::EditTimeTag { name: name.clone(), new_name: None, time: None, lock: Some(false), relative_to: Some(d.relative_to.clone()) });
                        let moved = (old_time != Some(d.time)).then_some(d.time);
                        let r = app.dispatch(Command::EditTimeTag { name: name.clone(), new_name: Some(d.name.clone()), time: moved, lock: Some(d.lock), relative_to: None });
                        r
                    }
                };
                if r.is_ok() {
                    done = true;
                }
            }
            if d.editing.is_some() && ui.button("Delete").clicked() {
                if let Some(name) = d.editing.clone() {
                    let _ = app.dispatch(Command::DeleteTimeTag { name });
                }
                done = true;
            }
            if ui.button("Cancel").clicked() {
                done = true;
            }
        });
    });
    app.tag_dialog = (open && !done).then_some(d);
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
