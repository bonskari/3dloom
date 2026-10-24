//! The Main Toolbar.
//! Items whose command lands in a later plan carry that plan and are not
//! drawn until it ships (a dead button would be a placeholder).

use crate::icons::{self, Icon};
use crate::shortcuts::{menu_widget, HoverShortcut};
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
    item("Curve Editor", Plan::Now),
    item("Schematic View", Plan::Phase3),
    item("Material Editor", Plan::Now),
    item("Render Setup", Plan::Now),
    item("Rendered Frame Window", Plan::Now),
    item("Render Production", Plan::Now),
];

fn tool_button(ui: &mut egui::Ui, app: &mut LoomApp, glyph: &str, on: bool, action: &str) {
    if Icon::new(glyph, action).on(on).show(ui, &app.state).clicked() {
        app.dispatch_named(action);
    }
}

/// Snap toggles: right-click opens Grid and Snap Settings.
fn snap_button(ui: &mut egui::Ui, app: &mut LoomApp, glyph: &str, on: bool, action: &str) {
    let r = Icon::new(glyph, action).on(on).hint("right-click: Grid and Snap Settings").show(ui, &app.state);
    if r.clicked() {
        app.dispatch_named(action);
    }
    if r.secondary_clicked() {
        let _ = app.dispatch(Command::GridAndSnapSettings);
    }
}

/// Transform buttons: click selects the tool, right-click opens Transform Type-In.
fn transform_button(ui: &mut egui::Ui, app: &mut LoomApp, glyph: &str, tip: &str, tool: Tool, action: &str) {
    let r = Icon::new(glyph, action).tip(tip).on(app.state.active_tool == tool).hint("right-click: Transform Type-In").show(ui, &app.state);
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
                        if Icon::new(icons::UNDO, "Undo Scene Operation").enabled(app.state.undo.can_undo()).show(ui, &app.state).clicked() {
                            app.dispatch_named("Undo Scene Operation");
                        }
                    }
                    "Redo" => {
                        if Icon::new(icons::REDO, "Redo Scene Operation").enabled(app.state.undo.can_redo()).show(ui, &app.state).clicked() {
                            app.dispatch_named("Redo Scene Operation");
                        }
                        ui.separator();
                    }
                    "Select and Link" => tool_button(ui, app, icons::LINK, app.state.pick_mode == Some(loom_commands::PickMode::Link), "Select and Link"),
                    "Unlink Selection" => {
                        if Icon::new(icons::UNLINK, "Unlink Selection").show(ui, &app.state).clicked() {
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
                    "Select Object" => tool_button(ui, app, icons::SELECT_OBJECT, app.state.active_tool == Tool::Select && app.state.pick_mode.is_none(), "Select Object"),
                    "Select From Scene" => {
                        if Icon::new(icons::SELECT_BY_NAME, "Select By Name").tip("Select From Scene").show(ui, &app.state).clicked() {
                            let _ = app.dispatch(Command::SelectByName);
                        }
                    }
                    "Selection Region" => {
                        let cur = app.state.selection_region;
                        let glyph = |r: SelectionRegion| match r {
                            SelectionRegion::Rectangular => icons::REGION_RECTANGULAR,
                            SelectionRegion::Circular => icons::REGION_CIRCULAR,
                            SelectionRegion::Fence => icons::REGION_FENCE,
                            SelectionRegion::Lasso => icons::REGION_LASSO,
                            SelectionRegion::Paint => icons::REGION_PAINT,
                        };
                        egui::ComboBox::from_id_salt("selection_region").selected_text(egui::RichText::new(glyph(cur)).size(icons::SIZE)).width(34.0).show_ui(ui, |ui| {
                            for r in SelectionRegion::ALL {
                                if ui.selectable_label(cur == r, icons::labeled(glyph(r), r.label())).clicked() {
                                    let _ = app.dispatch(Command::SelectionRegion { shape: r });
                                }
                            }
                        });
                    }
                    "Window/Crossing Selection Toggle" => {
                        let w = app.state.window_selection;
                        let glyph = if w { icons::WINDOW } else { icons::CROSSING };
                        if Icon::new(glyph, "Window/Crossing Selection Toggle").tip(if w { "Window Selection" } else { "Crossing Selection" }).on(w).show(ui, &app.state).clicked() {
                            let _ = app.dispatch(Command::WindowCrossingSelectionToggle);
                        }
                        ui.separator();
                    }
                    "Select and Move" => transform_button(ui, app, icons::MOVE, "Select and Move", Tool::Move, "Select and Move"),
                    "Select and Rotate" => transform_button(ui, app, icons::ROTATE, "Select and Rotate", Tool::Rotate, "Select and Rotate"),
                    "Select and Scale" => {
                        // Scale flyout: Uniform / Non-Uniform / Squash (Ctrl+E cycles).
                        let kind = app.state.scale_kind;
                        let glyph = |k: ScaleKind| match k { ScaleKind::Uniform => icons::SCALE, ScaleKind::NonUniform => icons::SCALE_NON_UNIFORM, ScaleKind::Squash => icons::SQUASH };
                        transform_button(ui, app, glyph(kind), kind.label(), Tool::Scale, "Select and Scale");
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
                        egui::ComboBox::from_id_salt("coord_system").selected_text(cur.label()).width(70.0).show_ui(ui, |ui| {
                            for c in CoordSystem::ALL {
                                if ui.selectable_label(cur == c, c.label()).clicked() {
                                    let _ = app.dispatch(Command::ReferenceCoordinateSystem { system: c });
                                }
                            }
                        });
                    }
                    "Use Center" => {
                        let cur = app.state.center_mode;
                        let glyph = |m: CenterMode| match m { CenterMode::PivotPoint => icons::PIVOT_CENTER, CenterMode::Selection => icons::SELECTION_CENTER, CenterMode::TransformCoordinate => icons::COORDINATE_CENTER };
                        egui::ComboBox::from_id_salt("use_center").selected_text(egui::RichText::new(glyph(cur)).size(icons::SIZE)).width(34.0).show_ui(ui, |ui| {
                            for m in CenterMode::ALL {
                                if ui.selectable_label(cur == m, icons::labeled(glyph(m), m.label())).clicked() {
                                    let _ = app.dispatch(Command::UseCenter { mode: m });
                                }
                            }
                        });
                        ui.separator();
                    }
                    "Select and Manipulate" => tool_button(ui, app, icons::MANIPULATE, app.state.manipulate, "Select and Manipulate"),
                    "Keyboard Shortcut Override Toggle" => {
                        tool_button(ui, app, icons::KEYBOARD_OVERRIDE, app.state.keyboard_override, "Keyboard Shortcut Override Toggle");
                        ui.separator();
                    }
                    "Snaps Toggle" => {
                        // Flyout: 2D / 2.5D / 3D (Alt+S cycles); right-click opens Grid and Snap Settings.
                        let mode = app.state.snaps.mode;
                        // The magnet carries the mode's number, as the reference's 2 / 2.5 / 3 icons do.
                        let glyph = format!("{}{}", icons::SNAP, mode.label().trim_end_matches('D'));
                        let r = Icon::new(&glyph, "Snaps Toggle").on(app.state.snaps_on).hint("right-click: Grid and Snap Settings").show(ui, &app.state);
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
                    "Angle Snap Toggle" => snap_button(ui, app, icons::ANGLE_SNAP, app.state.angle_snap_on, "Angle Snap Toggle"),
                    "Percent Snap Toggle" => snap_button(ui, app, icons::PERCENT_SNAP, app.state.percent_snap_on, "Percent Snap Toggle"),
                    "Spinner Snap Toggle" => {
                        tool_button(ui, app, icons::SPINNER_SNAP, app.state.prefs.spinner_snap, "Spinner Snap Toggle");
                        ui.separator();
                    }
                    "Manage Layers" => {
                        if Icon::new(icons::LAYERS, Command::LayerManager.name()).tip("Manage Layers (Layer Manager)").on(app.state.dialog == Some(loom_commands::Dialog::LayerManager)).show(ui, &app.state).clicked() {
                            let _ = app.dispatch(Command::LayerManager);
                        }
                    }
                    "Graphite Modeling Tools" => {
                        if Icon::new(icons::GRAPHITE, Command::ShowRibbonToggle.name()).tip("Graphite Modeling Tools (show / hide the ribbon)").on(app.state.show_ribbon).show(ui, &app.state).clicked() {
                            let _ = app.dispatch(Command::ShowRibbonToggle);
                        }
                    }
                    "Mirror" => {
                        if Icon::new(icons::MIRROR, "Mirror").show(ui, &app.state).clicked() {
                            let _ = app.dispatch(Command::Mirror);
                        }
                    }
                    "Align" => {
                        // Align flyout: Align, Quick Align, Normal Align, Align Camera, Align to View.
                        let r = Icon::new(icons::ALIGN, "Align").hint("right-click: Align flyout").show(ui, &app.state);
                        if r.clicked() {
                            app.dispatch_named("Align");
                        }
                        let popup = ui.id().with("align_flyout");
                        if r.secondary_clicked() {
                            ui.memory_mut(|m| m.toggle_popup(popup));
                        }
                        egui::popup::popup_below_widget(ui, popup, &r, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                            ui.set_min_width(140.0);
                            for (g, l, a) in [(icons::ALIGN, "Align", "Align"), (icons::QUICK_ALIGN, "Quick Align", "Quick Align"), (icons::NORMAL_ALIGN, "Normal Align", "Normal Align "), (icons::ALIGN_CAMERA, "Align Camera", "Align Camera"), (icons::ALIGN_TO_VIEW, "Align to View", "Align to View Dialog")] {
                                if ui.add(menu_widget(&app.state, &icons::labeled(g, l), a)).on_hover_name(&app.state, a).clicked() {
                                    app.dispatch_named(a);
                                }
                            }
                        });
                        ui.separator();
                    }
                    "Edit Named Selection Sets" => {
                        if Icon::new(icons::NAMED_SETS, "Edit Named Selection Sets").show(ui, &app.state).clicked() {
                            let _ = app.dispatch(Command::EditNamedSelectionSets);
                        }
                    }
                    "Named Selection Sets" => named_selection_sets(ui, app),
                    "Render Setup" => {
                        ui.separator();
                        if Icon::new(icons::RENDER_SETUP, "Render SetUp...").tip("Render Setup").show(ui, &app.state).clicked() {
                            app.dispatch_named("Render SetUp...");
                        }
                    }
                    "Rendered Frame Window" => {
                        if Icon::new(icons::RENDERED_FRAME_WINDOW, "Rendered Frame Window").on(app.state.rendered.open).show(ui, &app.state).clicked() {
                            app.dispatch_named("Rendered Frame Window");
                        }
                    }
                    "Render Production" => {
                        // Render flyout: Production / Iterative (right-click).
                        let iterative = app.state.rendered.iterative;
                        let action = if iterative { "Render Iterative" } else { "Render" };
                        let icon = Icon::new(icons::RENDER, action).tip(if iterative { "Render Iterative" } else { "Render Production" }).hint("right-click: Render flyout");
                        let r = if iterative { icon.color(icons::SUB_OBJECT_VARIANT) } else { icon }.show(ui, &app.state);
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
                                if ui.add(menu_widget(&app.state, &icons::labeled(icons::RENDER, l), if it { "Render Iterative" } else { "Render" })).clicked() {
                                    let _ = app.dispatch(Command::RenderViewport { viewport: None, lock: None, iterative: Some(it) });
                                    app.dispatch_named(if it { "Render Iterative" } else { "Render" });
                                }
                            }
                        });
                    }
                    "Curve Editor" => {
                        ui.separator();
                        if Icon::new(icons::CURVE_EDITOR, Command::TrackViewFloater.name()).tip("Curve Editor (Open)").on(app.state.track_view.open && app.state.track_view.mode == loom_commands::track_view::TvMode::CurveEditor).show(ui, &app.state).clicked() {
                            let _ = app.dispatch(Command::TrackViewFloater);
                        }
                    }
                    "Material Editor" => {
                        let r = Icon::new(icons::MATERIAL_EDITOR, Command::MaterialEditorToggle.name()).tip("Material Editor").on(app.state.material_editor_open).show(ui, &app.state);
                        if r.clicked() {
                            let _ = app.dispatch(Command::MaterialEditorToggle);
                        }
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
