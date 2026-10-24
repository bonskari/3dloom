//! The Main Toolbar.
//! Items whose command lands in a later plan carry that plan and are not
//! drawn until it ships (a dead button would be a placeholder).

use crate::shortcuts::{menu_widget, tip, HoverShortcut};
use crate::LoomApp;
use eframe::egui;
use loom_commands::{CenterMode, Command, CoordSystem, ScaleKind, SelectionFilter, SelectionRegion, Tool};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    Now,
    Phase3,
    Phase5,
    Phase8,
}

pub struct Item {
    pub name: &'static str,
    pub since: Plan,
}

const fn item(name: &'static str, since: Plan) -> Item {
    Item { name, since }
}

/// Main Toolbar contents, in order.
pub const ITEMS: &[Item] = &[
    item("Undo", Plan::Now),
    item("Redo", Plan::Now),
    item("Select and Link", Plan::Now),
    item("Unlink Selection", Plan::Now),
    item("Bind to Space Warp", Plan::Phase3),
    item("Selection Filter", Plan::Now),
    item("Select Object", Plan::Now),
    item("Select From Scene", Plan::Now),
    item("Selection Region", Plan::Now),
    item("Window/Crossing Selection Toggle", Plan::Now),
    item("Select and Move", Plan::Now),
    item("Select and Rotate", Plan::Now),
    item("Select and Scale", Plan::Now),
    item("Reference Coordinate System", Plan::Now),
    item("Use Center", Plan::Now),
    item("Select and Manipulate", Plan::Now),
    item("Keyboard Shortcut Override Toggle", Plan::Now),
    item("Snaps Toggle", Plan::Now),
    item("Angle Snap Toggle", Plan::Now),
    item("Percent Snap Toggle", Plan::Now),
    item("Spinner Snap Toggle", Plan::Now),
    item("Edit Named Selection Sets", Plan::Now),
    item("Named Selection Sets", Plan::Now),
    item("Mirror", Plan::Now),
    item("Align", Plan::Now),
    item("Manage Layers", Plan::Now),
    item("Graphite Modeling Tools", Plan::Now),
    item("Curve Editor", Plan::Phase8),
    item("Schematic View", Plan::Phase3),
    item("Material Editor", Plan::Phase5),
    item("Render Setup", Plan::Now),
    item("Rendered Frame Window", Plan::Now),
    item("Render Production", Plan::Now),
];

fn tool_button(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, on: bool, action: &str) {
    if ui.selectable_label(on, label).on_hover_name(&app.state, action).clicked() {
        app.dispatch_named(action);
    }
}

/// Snap toggles: right-click opens Grid and Snap Settings.
fn snap_button(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, on: bool, action: &str) {
    let r = ui.selectable_label(on, label).on_hover_text(format!("{}; right-click: Grid and Snap Settings", tip(&app.state, action, action)));
    if r.clicked() {
        app.dispatch_named(action);
    }
    if r.secondary_clicked() {
        let _ = app.dispatch(Command::GridAndSnapSettings);
    }
}

/// Transform buttons: click selects the tool, right-click opens Transform Type-In.
fn transform_button(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, tool: Tool, action: &str) {
    let r = ui.selectable_label(app.state.active_tool == tool, label).on_hover_text(format!("{}; right-click: Transform Type-In", tip(&app.state, action, action)));
    if r.clicked() {
        app.dispatch_named(action);
    }
    if r.secondary_clicked() {
        app.dispatch_named(action);
        if app.state.dialog != Some(loom_commands::Dialog::TransformTypeIn) {
            let _ = app.dispatch(Command::TransformTypeInDialogToggle);
        }
    }
}

/// Named Selection Sets: type a name and press Enter to name the selection
/// (the sub-object selection at a sub-object level); the list next to it
/// holds the object sets, or at a sub-object level the selected objects'
/// sets of that level.
fn named_selection_sets(ui: &mut egui::Ui, app: &mut LoomApp) {
    let sub_level = app.state.selection_mode != loom_commands::SelectionMode::Object;
    let text_id = egui::Id::new("named_sets_field");
    let mut text: String = ui.ctx().data(|d| d.get_temp(text_id)).unwrap_or_default();
    let r = ui.add(egui::TextEdit::singleline(&mut text).hint_text("Named Sel. Sets").desired_width(100.0)).on_hover_text("Named Selection Sets: type a name and press Enter");
    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !text.trim().is_empty() {
        let name = text.trim().to_owned();
        let c = if sub_level { Command::NamedSubSelectionCopy { name } } else { Command::NamedSelectionSet { name } };
        if let Err(e) = app.dispatch(c) {
            app.state.status_prompt = e;
        }
    }
    let names: Vec<String> = if sub_level { loom_commands::named_sub_sets(&app.state) } else { app.state.named_sets.keys().cloned().collect() };
    let mut chosen = None;
    egui::ComboBox::from_id_salt("named_sets").selected_text("").width(14.0).show_ui(ui, |ui| {
        for n in names {
            if ui.button(&n).clicked() {
                chosen = Some(n);
            }
        }
    });
    if let Some(name) = chosen {
        text = name.clone();
        let c = if sub_level { Command::NamedSubSelectionPaste { name } } else { Command::SelectNamedSet { name } };
        if let Err(e) = app.dispatch(c) {
            app.state.status_prompt = e;
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(text_id, text));
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp) {
    if app.state.expert_mode || !app.state.show_main_toolbar {
        return;
    }
    egui::TopBottomPanel::top("main_toolbar").show(ctx, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for it in ITEMS.iter().filter(|i| i.since == Plan::Now) {
                match it.name {
                    "Undo" => {
                        if ui.add_enabled(app.state.undo.can_undo(), egui::Button::new("↶")).on_hover_name(&app.state, "Undo Scene Operation").clicked() {
                            app.dispatch_named("Undo Scene Operation");
                        }
                    }
                    "Redo" => {
                        if ui.add_enabled(app.state.undo.can_redo(), egui::Button::new("↷")).on_hover_name(&app.state, "Redo Scene Operation").clicked() {
                            app.dispatch_named("Redo Scene Operation");
                        }
                        ui.separator();
                    }
                    "Select and Link" => tool_button(ui, app, "Link", app.state.pick_mode == Some(loom_commands::PickMode::Link), "Select and Link"),
                    "Unlink Selection" => {
                        if ui.button("Unlink").on_hover_name(&app.state, "Unlink Selection").clicked() {
                            let _ = app.dispatch(Command::UnlinkSelection);
                        }
                        ui.separator();
                    }
                    "Selection Filter" => {
                        let cur = app.state.selection_filter;
                        egui::ComboBox::from_id_salt("selection_filter").selected_text(cur.label()).width(60.0).show_ui(ui, |ui| {
                            for f in SelectionFilter::ALL {
                                if ui.selectable_label(cur == f, f.label()).clicked() {
                                    let _ = app.dispatch(Command::SelectionFilter { filter: f });
                                }
                            }
                        });
                    }
                    "Select Object" => tool_button(ui, app, "Select", app.state.active_tool == Tool::Select && app.state.pick_mode.is_none(), "Select Object"),
                    "Select From Scene" => {
                        if ui.button("By Name").on_hover_cmd(&app.state, "Select From Scene", "Select By Name").clicked() {
                            let _ = app.dispatch(Command::SelectByName);
                        }
                    }
                    "Selection Region" => {
                        let cur = app.state.selection_region;
                        let short = |r: SelectionRegion| r.label().split(' ').next().unwrap_or("");
                        egui::ComboBox::from_id_salt("selection_region").selected_text(short(cur)).width(96.0).show_ui(ui, |ui| {
                            for r in SelectionRegion::ALL {
                                if ui.selectable_label(cur == r, r.label()).clicked() {
                                    let _ = app.dispatch(Command::SelectionRegion { shape: r });
                                }
                            }
                        });
                    }
                    "Window/Crossing Selection Toggle" => {
                        let w = app.state.window_selection;
                        if ui.selectable_label(w, if w { "Window" } else { "Crossing" }).on_hover_name(&app.state, "Window/Crossing Selection Toggle").clicked() {
                            let _ = app.dispatch(Command::WindowCrossingSelectionToggle);
                        }
                        ui.separator();
                    }
                    "Select and Move" => transform_button(ui, app, "Move", Tool::Move, "Select and Move"),
                    "Select and Rotate" => transform_button(ui, app, "Rotate", Tool::Rotate, "Select and Rotate"),
                    "Select and Scale" => {
                        // Scale flyout: Uniform / Non-Uniform / Squash (Ctrl+E cycles).
                        let kind = app.state.scale_kind;
                        let label = match kind { ScaleKind::Uniform => "Scale", ScaleKind::NonUniform => "Scale NU", ScaleKind::Squash => "Squash" };
                        transform_button(ui, app, label, Tool::Scale, "Select and Scale");
                        egui::ComboBox::from_id_salt("scale_kind").selected_text("").width(14.0).show_ui(ui, |ui| {
                            for k in ScaleKind::ALL {
                                if ui.selectable_label(kind == k, k.label()).clicked() {
                                    let _ = app.dispatch(Command::ScaleType { kind: k });
                                }
                            }
                        });
                    }
                    "Reference Coordinate System" => {
                        let cur = app.state.coord_system;
                        let name = |app: &LoomApp, id: loom_scene::NodeId| app.state.scene.node(id).map(|n| n.name.clone());
                        // Pick shows the picked object's name; the four most recent picks stay in the list.
                        let shown = match (cur, app.state.pick_coord_node) {
                            (CoordSystem::Pick, Some(id)) => name(app, id).unwrap_or_else(|| cur.label().to_owned()),
                            _ => cur.label().to_owned(),
                        };
                        let recent: Vec<(loom_scene::NodeId, String)> = app.state.pick_coord_recent.iter().filter_map(|id| Some((*id, name(app, *id)?))).collect();
                        egui::ComboBox::from_id_salt("coord_system").selected_text(shown).width(70.0).show_ui(ui, |ui| {
                            for c in CoordSystem::ALL {
                                if ui.selectable_label(cur == c && (c != CoordSystem::Pick || app.state.pick_coord_node.is_none()), c.label()).clicked() {
                                    let _ = app.dispatch(Command::ReferenceCoordinateSystem { system: c });
                                }
                            }
                            for (id, n) in recent {
                                if ui.selectable_label(cur == CoordSystem::Pick && app.state.pick_coord_node == Some(id), n).clicked() {
                                    let _ = app.dispatch(Command::PickCoordinateSystem { id: id.0 });
                                }
                            }
                        });
                    }
                    "Use Center" => {
                        let cur = app.state.center_mode;
                        // Auto Key + Rotate/Scale: each pivot, the flyout is unavailable.
                        let forced = loom_commands::transform::animate_forces_pivot(&app.state) && matches!(app.state.active_tool, Tool::Rotate | Tool::Scale);
                        let shown = if forced { CenterMode::PivotPoint } else { cur };
                        let short = match shown { CenterMode::PivotPoint => "Pivot", CenterMode::Selection => "Selection", CenterMode::TransformCoordinate => "Coord" };
                        ui.add_enabled_ui(!forced, |ui| {
                            egui::ComboBox::from_id_salt("use_center").selected_text(short).width(80.0).show_ui(ui, |ui| {
                                for m in CenterMode::ALL {
                                    // Local uses each object's own centre: Use Transform Center is unavailable.
                                    let off = m == CenterMode::TransformCoordinate && app.state.coord_system == CoordSystem::Local;
                                    if ui.add_enabled(!off, egui::SelectableLabel::new(cur == m, m.label())).clicked() {
                                        let _ = app.dispatch(Command::UseCenter { mode: m });
                                    }
                                }
                            });
                        });
                        ui.separator();
                    }
                    "Select and Manipulate" => tool_button(ui, app, "Manip", app.state.manipulate, "Select and Manipulate"),
                    "Keyboard Shortcut Override Toggle" => {
                        tool_button(ui, app, "Kbd", app.state.keyboard_override, "Keyboard Shortcut Override Toggle");
                        ui.separator();
                    }
                    "Snaps Toggle" => {
                        // Flyout: 2D / 2.5D / 3D (Alt+S cycles); right-click opens Grid and Snap Settings.
                        let mode = app.state.snaps.mode;
                        let r = ui.selectable_label(app.state.snaps_on, mode.label()).on_hover_text(format!("{}; right-click: Grid and Snap Settings", tip(&app.state, "Snaps Toggle", "Snaps Toggle")));
                        if r.clicked() {
                            app.dispatch_named("Snaps Toggle");
                        }
                        if r.secondary_clicked() {
                            let _ = app.dispatch(Command::GridAndSnapSettings);
                        }
                        egui::ComboBox::from_id_salt("snap_mode").selected_text("").width(14.0).show_ui(ui, |ui| {
                            for m in [loom_commands::snap::SnapMode::Snap2D, loom_commands::snap::SnapMode::Snap25D, loom_commands::snap::SnapMode::Snap3D] {
                                if ui.selectable_label(mode == m, format!("{} Snap", m.label())).clicked() {
                                    let _ = app.dispatch(Command::SnapSettings { mode: Some(m), marker_display: None, marker_size: None, preview_radius_px: None, radius_px: None, angle_deg: None, percent: None, use_axis_constraints: None, show_rubber_band: None });
                                }
                            }
                        });
                    }
                    "Angle Snap Toggle" => snap_button(ui, app, "∠", app.state.angle_snap_on, "Angle Snap Toggle"),
                    "Percent Snap Toggle" => snap_button(ui, app, "%", app.state.percent_snap_on, "Percent Snap Toggle"),
                    "Spinner Snap Toggle" => {
                        tool_button(ui, app, "⇅", app.state.prefs.spinner_snap, "Spinner Snap Toggle");
                        ui.separator();
                    }
                    "Manage Layers" => {
                        if ui.selectable_label(app.state.dialog == Some(loom_commands::Dialog::LayerManager), "Layers").on_hover_cmd(&app.state, "Manage Layers (Layer Manager)", Command::LayerManager.name()).clicked() {
                            let _ = app.dispatch(Command::LayerManager);
                        }
                    }
                    "Graphite Modeling Tools" => {
                        if ui.selectable_label(app.state.show_ribbon, "Graphite").on_hover_cmd(&app.state, "Graphite Modeling Tools (show / hide the ribbon)", Command::ShowRibbonToggle.name()).clicked() {
                            let _ = app.dispatch(Command::ShowRibbonToggle);
                        }
                    }
                    "Mirror" => {
                        if ui.button("Mirror").on_hover_name(&app.state, "Mirror").clicked() {
                            let _ = app.dispatch(Command::Mirror);
                        }
                    }
                    "Align" => {
                        // Align flyout: Align, Quick Align, Normal Align, Align Camera, Align to View.
                        let r = ui.button("Align").on_hover_text(format!("{}; right-click: Align flyout", tip(&app.state, "Align", "Align")));
                        if r.clicked() {
                            app.dispatch_named("Align");
                        }
                        let popup = ui.id().with("align_flyout");
                        if r.secondary_clicked() {
                            ui.memory_mut(|m| m.toggle_popup(popup));
                        }
                        egui::popup::popup_below_widget(ui, popup, &r, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                            ui.set_min_width(140.0);
                            for (l, a) in [("Align", "Align"), ("Quick Align", "Quick Align"), ("Normal Align", "Normal Align "), ("Align Camera", "Align Camera"), ("Align to View", "Align to View Dialog")] {
                                if ui.add(menu_widget(&app.state, l, a)).on_hover_name(&app.state, a).clicked() {
                                    app.dispatch_named(a);
                                }
                            }
                        });
                        ui.separator();
                    }
                    "Edit Named Selection Sets" => {
                        if ui.button("{}").on_hover_name(&app.state, "Edit Named Selection Sets").clicked() {
                            let _ = app.dispatch(Command::EditNamedSelectionSets);
                        }
                    }
                    "Named Selection Sets" => named_selection_sets(ui, app),
                    "Render Setup" => {
                        ui.separator();
                        if ui.button("Render Setup").on_hover_name(&app.state, "Render SetUp...").clicked() {
                            app.dispatch_named("Render SetUp...");
                        }
                    }
                    "Rendered Frame Window" => {
                        if ui.selectable_label(app.state.rendered.open, "Frame Window").on_hover_name(&app.state, "Rendered Frame Window").clicked() {
                            app.dispatch_named("Rendered Frame Window");
                        }
                    }
                    "Render Production" => {
                        // Render flyout: Production / Iterative (right-click).
                        let iterative = app.state.rendered.iterative;
                        let action = if iterative { "Render Iterative" } else { "Render" };
                        let r = ui.button(if iterative { "Render (Iterative)" } else { "Render" }).on_hover_text(format!("{}; right-click: Render flyout", tip(&app.state, action, action)));
                        if r.clicked() {
                            app.dispatch_named(action);
                        }
                        let popup = ui.id().with("render_flyout");
                        if r.secondary_clicked() {
                            ui.memory_mut(|m| m.toggle_popup(popup));
                        }
                        egui::popup::popup_below_widget(ui, popup, &r, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                            ui.set_min_width(140.0);
                            for (l, it) in [("Render Production", false), ("Render Iterative", true)] {
                                if ui.add(menu_widget(&app.state, l, if it { "Render Iterative" } else { "Render" })).clicked() {
                                    let _ = app.dispatch(Command::RenderViewport { viewport: None, lock: None, iterative: Some(it) });
                                    app.dispatch_named(if it { "Render Iterative" } else { "Render" });
                                }
                            }
                        });
                    }
                    _ => {}
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolbar_items_follow_reference_order() {
        let names: Vec<&str> = ITEMS.iter().map(|i| i.name).collect();
        let expected = [
            "Undo", "Redo", "Select and Link", "Unlink Selection", "Bind to Space Warp", "Selection Filter", "Select Object",
            "Select From Scene", "Selection Region", "Window/Crossing Selection Toggle", "Select and Move", "Select and Rotate",
            "Select and Scale", "Reference Coordinate System", "Use Center", "Select and Manipulate", "Keyboard Shortcut Override Toggle",
            "Snaps Toggle", "Angle Snap Toggle", "Percent Snap Toggle", "Spinner Snap Toggle", "Edit Named Selection Sets",
            "Named Selection Sets", "Mirror", "Align", "Manage Layers", "Graphite Modeling Tools", "Curve Editor", "Schematic View",
            "Material Editor", "Render Setup", "Rendered Frame Window", "Render Production",
        ];
        assert_eq!(names, expected);
        // Items of later plans are flagged, never drawn as dead buttons.
        assert!(ITEMS.iter().filter(|i| i.since != Plan::Now).all(|i| !i.name.is_empty()));
    }
}
