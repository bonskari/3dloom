//! Graphite Modeling Tools ribbon: tabs Graphite Modeling Tools /
//! Freeform / Selection, docked under the Main Toolbar, toggled by the
//! toolbar button or Customize > Show UI > Show Ribbon. Every button
//! dispatches the same command as its Modify-panel counterpart.
//!
//! As in the reference the ribbon starts open and minimized to panel titles;
//! the Minimize / Maximize button cycles maximized, panel buttons, panel
//! titles and tabs (or toggles one state chosen from its drop-down). Outside
//! Modify mode, or while a modifier other than Edit Poly is the current stack
//! entry, the modeling tab shows only the Polygon Modeling panel; at a
//! sub-object level the level's own panel (Vertices, Edges, Borders,
//! Polygons, Elements) and the Loops panel join in, and the Soft Selection
//! and Use NURMS panels open at the right end while those modes are on.
//! Shift+click opens a tool's settings, Ctrl+click Remove at Edge level also
//! removes the leftover vertices, Ctrl+click (Shift+click) on a level
//! converts the selection touching (contained).

use crate::command_panel::Tab;
use crate::icons::{self, Icon};
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Axis, Command, PaintMode, PickMode, PolyConstraint, PolyDrag, PreviewSelect, SelectionMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RibbonTab {
    #[default]
    Modeling,
    Freeform,
    Selection,
}

impl RibbonTab {
    pub const ALL: [RibbonTab; 3] = [RibbonTab::Modeling, RibbonTab::Freeform, RibbonTab::Selection];
    pub fn label(self) -> &'static str {
        match self {
            RibbonTab::Modeling => "Graphite Modeling Tools",
            RibbonTab::Freeform => "Freeform",
            RibbonTab::Selection => "Selection",
        }
    }
}

/// How much of the ribbon shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RibbonState {
    /// Every panel with its tools.
    Maximized,
    /// Tabs, panel titles and one icon per panel; a panel opens under its title.
    PanelButtons,
    /// Tabs and panel titles; a panel opens under its title (the default).
    #[default]
    PanelTitles,
    /// Tab names only; clicking a tab shows its panels until the mouse leaves.
    Tabs,
}

impl RibbonState {
    /// The Cycle Through All order: maximized, panel buttons, panel titles, tabs.
    pub fn next(self) -> Self {
        match self {
            RibbonState::Maximized => RibbonState::PanelButtons,
            RibbonState::PanelButtons => RibbonState::PanelTitles,
            RibbonState::PanelTitles => RibbonState::Tabs,
            RibbonState::Tabs => RibbonState::Maximized,
        }
    }
}

/// The Minimize drop-down's choice: what the Minimize / Maximize button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MinimizeOption {
    Tabs,
    PanelTitles,
    PanelButtons,
    /// Each click goes to the next state (the default).
    #[default]
    CycleThroughAll,
}

impl MinimizeOption {
    pub const ALL: [MinimizeOption; 4] = [MinimizeOption::Tabs, MinimizeOption::PanelTitles, MinimizeOption::PanelButtons, MinimizeOption::CycleThroughAll];
    pub fn label(self) -> &'static str {
        match self {
            MinimizeOption::Tabs => "Minimize to Tabs",
            MinimizeOption::PanelTitles => "Minimize to Panel Titles",
            MinimizeOption::PanelButtons => "Minimize to Panel Buttons",
            MinimizeOption::CycleThroughAll => "Cycle through All",
        }
    }
    fn state(self) -> Option<RibbonState> {
        match self {
            MinimizeOption::Tabs => Some(RibbonState::Tabs),
            MinimizeOption::PanelTitles => Some(RibbonState::PanelTitles),
            MinimizeOption::PanelButtons => Some(RibbonState::PanelButtons),
            MinimizeOption::CycleThroughAll => None,
        }
    }
}

/// The Minimize / Maximize button: the next state of the cycle, or maximized
/// and back to the chosen minimize state.
pub fn toggle_minimize(app: &mut LoomApp) {
    app.ribbon_state = match app.ribbon_minimize.state() {
        None => app.ribbon_state.next(),
        Some(s) if app.ribbon_state == RibbonState::Maximized => s,
        Some(_) => RibbonState::Maximized,
    };
    app.ribbon_popup = None;
    app.ribbon_peek = false;
}

/// The ribbon's tools work on the selected Editable Poly (or Edit Poly)
/// when the Modify panel is current and that stack level is active.
pub fn editing(app: &LoomApp) -> bool {
    app.panel_tab == Tab::Modify
        && app.state.selection.iter().any(|id| app.state.scene.node(*id).is_some_and(|n| n.edit_poly().is_some() && (n.active_entry.is_none() || n.editing_entry().is_some())))
}

/// Is the button a toggle that is on right now?
fn on(app: &LoomApp, action: &str) -> bool {
    let s = &app.state;
    let poly = s.selection.iter().next().and_then(|id| s.scene.node(*id)).and_then(|n| n.edit_poly());
    match action {
        "Use Soft Selection" => poly.is_some_and(|p| p.soft.on),
        "Ignore Backfacing in Selections" => s.ignore_backfacing,
        "Select By Vertex" => s.select_by_vertex,
        "Preview Select Mode" => s.preview_selection != PreviewSelect::Off,
        "Cut" => matches!(s.pick_mode, Some(PickMode::CutA | PickMode::CutB)),
        "Quickslice Mode" => matches!(s.pick_mode, Some(PickMode::QuickSliceA | PickMode::QuickSliceB)),
        "Target Weld Mode" => matches!(s.pick_mode, Some(PickMode::TargetWeldSource | PickMode::TargetWeldTarget)),
        "Swift Loop Mode" => s.pick_mode == Some(PickMode::SwiftLoop),
        "Insert Vertex Mode" => s.pick_mode == Some(PickMode::InsertVertex),
        "Create" => s.pick_mode == Some(PickMode::CreatePolygon),
        "Extrude along Spline Mode" => s.pick_mode == Some(PickMode::ExtrudeAlongSpline),
        "Attach" => s.pick_mode == Some(PickMode::AttachObject),
        "Extrude Mode" => s.poly_drag == Some(PolyDrag::Extrude),
        "Bevel Mode" => s.poly_drag == Some(PolyDrag::Bevel),
        "Inset Mode" => s.poly_drag == Some(PolyDrag::Inset),
        "Outline Mode" => s.poly_drag == Some(PolyDrag::Outline),
        "Chamfer Mode" => s.poly_drag == Some(PolyDrag::Chamfer),
        "Hinge from Edge Mode" => s.poly_drag == Some(PolyDrag::Hinge),
        "Slice Plane Mode" => s.slice_plane_mode,
        "Loop Mode" => s.loop_mode,
        "Branches" => s.branches,
        "Show End Result Toggle" => s.show_end_result,
        "Pin Stack Toggle" => s.pin_stack,
        "Paint Deform Push/Pull" => s.paint_deform == Some(PaintMode::PushPull),
        "Paint Deform Relax" => s.paint_deform == Some(PaintMode::Relax),
        "Paint Deform Revert" => s.paint_deform == Some(PaintMode::Revert),
        _ => false,
    }
}

/// A ribbon tool: icon and label, pressed while its mode is on. Shift+click
/// runs `shift` (the tool's settings), Ctrl+click runs `ctrl`.
fn tool(ui: &mut egui::Ui, app: &mut LoomApp, glyph: &str, label: &str, cmd: Command, shift: Option<Command>, ctrl: Option<Command>) {
    let hint = match (&shift, &ctrl) {
        (Some(s), _) => Some(format!("Shift+click: {}", s.name())),
        (_, Some(c)) => Some(format!("Ctrl+click: {}", c.name())),
        _ => None,
    };
    let mut icon = Icon::new(glyph, cmd.name()).tip(label).label(label).on(on(app, cmd.name()));
    if let Some(h) = hint.as_deref() {
        icon = icon.hint(h);
    }
    if icon.show(ui, &app.state).clicked() {
        let mods = ui.input(|i| i.modifiers);
        let cmd = match (shift, ctrl) {
            (Some(s), _) if mods.shift => s,
            (_, Some(c)) if mods.ctrl || mods.command => c,
            _ => cmd,
        };
        if let Err(e) = app.dispatch(cmd) {
            app.last_error = Some(e);
        }
    }
}

/// A plain tool (no Shift / Ctrl variant).
fn plain(ui: &mut egui::Ui, app: &mut LoomApp, glyph: &str, label: &str, cmd: Command) {
    tool(ui, app, glyph, label, cmd, None, None);
}

/// One panel of the current tab: its title, the icon it shows minimized to
/// panel buttons, and its tools.
struct Panel {
    title: &'static str,
    glyph: &'static str,
    width: f32,
    body: fn(&mut egui::Ui, &mut LoomApp),
}

/// The panels the current tab shows, left to right.
fn panels(app: &LoomApp) -> Vec<Panel> {
    let p = |title, glyph, width, body| Panel { title, glyph, width, body };
    let level = app.state.selection_mode;
    let edit = editing(app);
    match app.ribbon_tab {
        RibbonTab::Modeling => {
            let mut v = vec![p("Polygon Modeling", icons::LEVEL_POLYGON, 360.0, polygon_modeling as fn(&mut egui::Ui, &mut LoomApp))];
            if !edit {
                return v;
            }
            v.push(p("Modify Selection", icons::LOOP, 260.0, modify_selection));
            v.push(p("Edit", icons::SWIFT_LOOP, 330.0, edit_panel));
            v.push(p("Geometry (All)", icons::ATTACH, 300.0, geometry_all));
            match level {
                SelectionMode::Vertex => v.push(p("Vertices", icons::LEVEL_VERTEX, 330.0, vertices)),
                SelectionMode::Edge => v.push(p("Edges", icons::LEVEL_EDGE, 380.0, edges)),
                SelectionMode::Border => v.push(p("Borders", icons::LEVEL_BORDER, 330.0, borders)),
                SelectionMode::Face => v.push(p("Polygons", icons::LEVEL_POLYGON, 380.0, polygons)),
                SelectionMode::Element => v.push(p("Elements", icons::LEVEL_ELEMENT, 380.0, polygons)),
                SelectionMode::Object => {}
            }
            if matches!(level, SelectionMode::Vertex | SelectionMode::Edge | SelectionMode::Border) {
                v.push(p("Loops", icons::CONNECT, 120.0, loops));
            }
            v.push(p("Subdivision", icons::MSMOOTH, 220.0, subdivision));
            v.push(p("Align", icons::MAKE_PLANAR, 240.0, align));
            v.push(p("Properties", icons::AUTO_SMOOTH, 240.0, properties));
            // Contextual panels open at the right end.
            let poly = app.state.selection.iter().next().and_then(|id| app.state.scene.node(*id)).and_then(|n| n.edit_poly());
            if level != SelectionMode::Object && poly.is_some_and(|p| p.soft.on) {
                v.push(p("Soft Selection", icons::SOFT_SELECTION, 260.0, soft_selection));
            }
            if poly.is_some_and(|p| p.subdiv.use_nurms.get()) {
                v.push(p("Use NURMS", icons::USE_NURMS, 260.0, use_nurms));
            }
            v
        }
        RibbonTab::Freeform => vec![p("PolyDraw", icons::POLYDRAW, 300.0, polydraw), p("Paint Deform", icons::PAINT_DEFORM, 520.0, paint_deform)],
        RibbonTab::Selection => vec![p("Modify Selection", icons::LOOP, 560.0, selection_tab_modify), p("By Half", icons::BY_HALF, 200.0, by_half), p("By Material / SG", icons::MATERIAL_ID, 360.0, by_material)],
    }
}

/// Are the panel's tools usable right now?
fn panel_enabled(app: &LoomApp, title: &str) -> bool {
    match title {
        "Polygon Modeling" => !app.state.selection.is_empty(),
        _ => editing(app),
    }
}

/// One maximized panel: a fixed-width group of wrapped tools with its title
/// underneath (explicit widths keep nested wrapping from growing unbounded).
fn panel_frame(ui: &mut egui::Ui, title: &str, width: f32, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style()).inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
        ui.set_max_width(width);
        ui.set_min_height(44.0);
        ui.vertical(|ui| {
            ui.set_max_width(width);
            ui.horizontal_wrapped(|ui| {
                ui.set_max_width(width);
                ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
                body(ui);
            });
            ui.label(egui::RichText::new(title).small().weak());
        });
    });
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp) {
    if !app.state.show_ribbon || app.state.expert_mode {
        return;
    }
    let r = egui::TopBottomPanel::top("ribbon").resizable(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            for tab in RibbonTab::ALL {
                let r = ui.selectable_label(app.ribbon_tab == tab, tab.label());
                if r.clicked() {
                    app.ribbon_tab = tab;
                    app.ribbon_popup = None;
                    // Minimized to tabs: the tab's panels show until the mouse leaves.
                    app.ribbon_peek = app.ribbon_state == RibbonState::Tabs;
                }
                if r.double_clicked() {
                    toggle_minimize(app);
                }
            }
            ui.add_space(8.0);
            let glyph = if app.ribbon_state == RibbonState::Maximized { icons::UP } else { icons::DOWN };
            let tip = match (app.ribbon_minimize.state(), app.ribbon_state) {
                (None, s) => format!("Click to {}", match s.next() {
                    RibbonState::Maximized => "show the full ribbon",
                    RibbonState::PanelButtons => "minimize to panel buttons",
                    RibbonState::PanelTitles => "minimize to panel titles",
                    RibbonState::Tabs => "minimize to tabs",
                }),
                (Some(_), RibbonState::Maximized) => format!("Click to {}", app.ribbon_minimize.label().to_lowercase()),
                (Some(_), _) => "Click to show the full ribbon".to_owned(),
            };
            let r = ui.small_button(glyph);
            icons::named(&r, egui::WidgetType::Button, "Minimize / Maximize");
            if r.on_hover_text(tip).clicked() {
                toggle_minimize(app);
            }
            let m = ui.menu_button(icons::SUBMENU, |ui| {
                for opt in MinimizeOption::ALL {
                    if ui.selectable_label(app.ribbon_minimize == opt, opt.label()).clicked() {
                        app.ribbon_minimize = opt;
                        if let Some(s) = opt.state() {
                            app.ribbon_state = s;
                        }
                        ui.close_menu();
                    }
                }
            });
            icons::named(&m.response, egui::WidgetType::Button, "Minimize Options");
        });
        let list = panels(app);
        let full = app.ribbon_state == RibbonState::Maximized || (app.ribbon_state == RibbonState::Tabs && app.ribbon_peek);
        if full {
            egui::ScrollArea::horizontal().auto_shrink([false, true]).show(ui, |ui| {
                ui.horizontal(|ui| {
                    for p in &list {
                        let enabled = panel_enabled(app, p.title);
                        ui.add_enabled_ui(enabled, |ui| panel_frame(ui, p.title, p.width, |ui| (p.body)(ui, app)));
                    }
                });
            });
        } else if app.ribbon_state != RibbonState::Tabs {
            minimized_panels(ui, app, &list);
        }
    });
    // Minimized to tabs: the panels close when the mouse leaves the ribbon.
    if app.ribbon_peek {
        let inside = ctx.input(|i| i.pointer.hover_pos()).is_none_or(|p| r.response.rect.expand(4.0).contains(p));
        if !inside && !ctx.memory(|m| m.any_popup_open()) {
            app.ribbon_peek = false;
        }
    }
}

/// Minimized to panel titles / buttons: one button per panel; the panel
/// opens under it on hover or click and stays while the mouse is over either.
fn minimized_panels(ui: &mut egui::Ui, app: &mut LoomApp, list: &[Panel]) {
    let buttons = app.ribbon_state == RibbonState::PanelButtons;
    let area_id = egui::Id::new("ribbon_panel_popup");
    let mut title_rect = None;
    ui.horizontal(|ui| {
        for p in list {
            let text = if buttons { icons::labeled(p.glyph, p.title) } else { p.title.to_owned() };
            let r = ui.selectable_label(app.ribbon_popup == Some(p.title), text);
            icons::named(&r, egui::WidgetType::Button, p.title);
            if r.clicked() || r.hovered() {
                app.ribbon_popup = Some(p.title);
            }
            if app.ribbon_popup == Some(p.title) {
                title_rect = Some(r.rect);
            }
        }
    });
    let (Some(open), Some(anchor)) = (app.ribbon_popup, title_rect) else {
        app.ribbon_popup = None;
        return;
    };
    let Some(p) = list.iter().find(|p| p.title == open) else { return };
    let enabled = panel_enabled(app, p.title);
    let area = egui::Area::new(area_id).order(egui::Order::Foreground).fixed_pos(anchor.left_bottom()).show(ui.ctx(), |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.add_enabled_ui(enabled, |ui| panel_frame(ui, p.title, p.width, |ui| (p.body)(ui, app)));
        });
    });
    let keep = ui.ctx().input(|i| i.pointer.hover_pos()).is_none_or(|pos| anchor.expand(4.0).contains(pos) || area.response.rect.expand(8.0).contains(pos));
    if !keep && !ui.ctx().memory(|m| m.any_popup_open()) {
        app.ribbon_popup = None;
    }
}

/// The stack entry Previous / Next Modifier go to (`Some(None)` = the base object).
fn stack_step(app: &LoomApp, up: bool) -> Option<Option<u32>> {
    let node = app.state.selection.iter().next().and_then(|id| app.state.scene.node(*id))?;
    let len = node.modifiers.len();
    match (node.active_entry, up) {
        (None, true) => (len > 0).then_some(Some(0)),
        (None, false) => None,
        (Some(i), true) => (i + 1 < len).then_some(Some(i as u32 + 1)),
        (Some(0), false) => Some(None),
        (Some(i), false) => Some(Some(i as u32 - 1)),
    }
}

fn polygon_modeling(ui: &mut egui::Ui, app: &mut LoomApp) {
    let edit = editing(app);
    // The wide button: Modify Mode outside the Modify panel, else the current stack level.
    let node = app.state.selection.iter().next().and_then(|id| app.state.scene.node(*id));
    let level_name = node.map(|n| match n.active_entry {
        Some(i) => n.modifiers.get(i).map(|m| m.name.clone()).unwrap_or_default(),
        None => n.object.stack_name().to_owned(),
    });
    let poly = node.is_some_and(|n| n.object.poly().is_some());
    let modify = app.panel_tab == Tab::Modify;
    let wide = if modify { level_name.clone().unwrap_or_else(|| "Modify Mode".into()) } else { "Modify Mode".into() };
    let r = Icon::new(icons::MODIFY_MODE, "Modify Mode").tip(&wide).label(&wide).show(ui, &app.state);
    if r.clicked() && !modify {
        app.panel_tab = Tab::Modify;
    }
    if Icon::new(icons::TOGGLE_COMMAND_PANEL, Command::ShowCommandPanelToggle.name()).tip("Toggle Command Panel").show(ui, &app.state).clicked() {
        let _ = app.dispatch(Command::ShowCommandPanelToggle);
        app.panel_tab = Tab::Modify;
    }
    ui.add_enabled_ui(modify, |ui| {
        if Icon::new(icons::PIN_STACK, Command::PinStackToggle.name()).tip("Pin Stack").on(app.state.pin_stack).show(ui, &app.state).clicked() {
            let _ = app.dispatch(Command::PinStackToggle);
        }
        if Icon::new(icons::SHOW_END_RESULT, Command::ShowEndResultToggle.name()).tip("Show End Result").on(app.state.show_end_result).show(ui, &app.state).clicked() {
            let _ = app.dispatch(Command::ShowEndResultToggle);
        }
        let (prev, next) = (stack_step(app, false), stack_step(app, true));
        if Icon::new(icons::PREVIOUS_MODIFIER, Command::SelectStackEntry { index: None }.name()).tip("Previous Modifier").enabled(prev.is_some()).show(ui, &app.state).clicked() {
            if let Some(index) = prev {
                let _ = app.dispatch(Command::SelectStackEntry { index });
            }
        }
        if Icon::new(icons::NEXT_MODIFIER, Command::SelectStackEntry { index: None }.name()).tip("Next Modifier").enabled(next.is_some()).show(ui, &app.state).clicked() {
            if let Some(index) = next {
                let _ = app.dispatch(Command::SelectStackEntry { index });
            }
        }
    });
    ui.end_row();
    ui.add_enabled_ui(edit, |ui| {
        for (mode, glyph) in [(SelectionMode::Vertex, icons::LEVEL_VERTEX), (SelectionMode::Edge, icons::LEVEL_EDGE), (SelectionMode::Border, icons::LEVEL_BORDER), (SelectionMode::Face, icons::LEVEL_POLYGON), (SelectionMode::Element, icons::LEVEL_ELEMENT)] {
            let selected = app.state.selection_mode == mode;
            let action = crate::shortcuts::level_action(mode);
            let r = Icon::new(glyph, action).tip(mode.label()).on(selected).hint("Ctrl+click converts the selection (touching), Shift+click contained").show(ui, &app.state);
            if r.clicked() {
                let mods = ui.input(|i| i.modifiers);
                if (mods.ctrl || mods.command || mods.shift) && app.state.selection_mode != SelectionMode::Object && !selected {
                    let _ = app.dispatch(Command::ConvertSelection { to: mode, contained: mods.shift });
                } else {
                    // Clicking the active level returns to the top level.
                    let _ = app.dispatch(Command::SelectionMode { mode: if selected { SelectionMode::Object } else { mode } });
                }
            }
        }
        let preview = match app.state.preview_selection {
            PreviewSelect::Off => "Preview Off",
            PreviewSelect::SubObj => "Preview SubObject",
            PreviewSelect::Multi => "Preview Multi",
        };
        if Icon::new(icons::PREVIEW_SELECTION, Command::PreviewSelectMode.name()).tip(preview).on(app.state.preview_selection != PreviewSelect::Off).show(ui, &app.state).clicked() {
            let _ = app.dispatch(Command::PreviewSelectMode);
        }
        if Icon::new(icons::IGNORE_BACKFACING, Command::IgnoreBackfacingInSelections.name()).tip("Ignore Backfacing").on(app.state.ignore_backfacing).show(ui, &app.state).clicked() {
            let _ = app.dispatch(Command::IgnoreBackfacingInSelections);
        }
        let soft = on(app, "Use Soft Selection");
        let level = app.state.selection_mode != SelectionMode::Object;
        if Icon::new(icons::SOFT_SELECTION, Command::UseSoftSelection.name()).tip("Use Soft Selection").on(soft).enabled(level).show(ui, &app.state).clicked() {
            let _ = app.dispatch(Command::UseSoftSelection);
        }
    });
    ui.end_row();
    let has_stack = app.state.selection.iter().any(|id| app.state.scene.node(*id).is_some_and(|n| !n.modifiers.is_empty()));
    if Icon::new(icons::COLLAPSE_STACK, Command::CollapseAll.name()).tip("Collapse Stack").enabled(has_stack).show(ui, &app.state).clicked() {
        let _ = app.dispatch(Command::CollapseAll);
    }
    if Icon::new(icons::CONVERT_TO_POLY, Command::ConvertToEditablePoly.name()).tip("Convert to Poly").enabled(!poly || has_stack).show(ui, &app.state).clicked() {
        let _ = app.dispatch(Command::ConvertToEditablePoly);
        app.panel_tab = Tab::Modify;
    }
    if Icon::new(icons::APPLY_EDIT_POLY, Command::AddModifier { name: String::new() }.name()).tip("Apply Edit Poly Mod").show(ui, &app.state).clicked() {
        let _ = app.dispatch(Command::AddModifier { name: "Edit Poly".into() });
        app.panel_tab = Tab::Modify;
    }
    if ui.add_enabled(has_stack, egui::Button::new("Make Unique")).on_hover_text("Make the instanced modifier / object unique").clicked() {
        let _ = app.dispatch(Command::MakeUnique);
    }
    egui::ComboBox::from_id_salt("ribbon_modifier_list").selected_text("Modifier List").width(120.0).show_ui(ui, |ui| {
        for name in loom_scene::Modifier::LIST {
            if ui.button(*name).clicked() {
                let _ = app.dispatch(Command::AddModifier { name: (*name).to_owned() });
            }
        }
    });
}

fn modify_selection(ui: &mut egui::Ui, app: &mut LoomApp) {
    plain(ui, app, icons::GROW, "Grow", Command::GrowSelection);
    plain(ui, app, icons::SHRINK, "Shrink", Command::ShrinkSelection);
    plain(ui, app, icons::LOOP, "Loop", Command::SelectEdgeLoop);
    plain(ui, app, icons::RING, "Ring", Command::SelectEdgeRing);
    plain(ui, app, icons::LOOP_MODE, "Loop Mode", Command::LoopMode);
}

fn edit_panel(ui: &mut egui::Ui, app: &mut LoomApp) {
    plain(ui, app, icons::REPEAT_LAST, "Repeat Last", Command::RepeatLastOperation);
    plain(ui, app, icons::QUICKSLICE, "QuickSlice", Command::QuickSliceMode);
    plain(ui, app, icons::SWIFT_LOOP, "Swift Loop", Command::SwiftLoopMode);
    plain(ui, app, icons::CUT, "Cut", Command::Cut);
    let nurms = app.state.selection.iter().next().and_then(|id| app.state.scene.node(*id)).and_then(|n| n.edit_poly()).is_some_and(|p| p.subdiv.use_nurms.get());
    if Icon::new(icons::USE_NURMS, Command::SubdivisionSurface { use_nurms: None, smooth_result: None, isoline_display: None, show_cage: None, iterations: None, smoothness: None, separate_by_smoothing: None, separate_by_materials: None }.name())
        .tip("Use NURMS")
        .label("Use NURMS")
        .on(nurms)
        .show(ui, &app.state)
        .clicked()
    {
        let _ = app.dispatch(Command::SubdivisionSurface { use_nurms: Some(!nurms), smooth_result: None, isoline_display: None, show_cage: None, iterations: None, smoothness: None, separate_by_smoothing: None, separate_by_materials: None });
    }
    ui.end_row();
    ui.label("Constraints:");
    let c = app.state.poly_constraint;
    for (kind, glyph, label, name, cmd) in [
        (PolyConstraint::None, icons::CONSTRAIN_NONE, "None", "Constrain To None", Command::ConstrainToNone),
        (PolyConstraint::Edges, icons::CONSTRAIN_EDGE, "Edge", "Constrain To Edge", Command::ConstrainToEdges),
        (PolyConstraint::Faces, icons::CONSTRAIN_FACE, "Face", "Constrain To Face", Command::ConstrainToFaces),
        (PolyConstraint::Normals, icons::CONSTRAIN_NORMAL, "Normal", "Constrain To Normal", Command::ConstrainToNormals),
    ] {
        if Icon::new(glyph, cmd.name()).tip(name).label(label).on(c == kind).show(ui, &app.state).clicked() {
            let _ = app.dispatch(cmd);
        }
    }
}

fn geometry_all(ui: &mut egui::Ui, app: &mut LoomApp) {
    let ps = app.state.poly_settings;
    tool(ui, app, icons::RELAX, "Relax", Command::Relax { amount: ps.relax_amount, iterations: ps.relax_iterations, hold_boundary: ps.relax_hold_boundary, hold_outer: ps.relax_hold_outer }, Some(Command::RelaxSettings), None);
    plain(ui, app, icons::CREATE, "Create", Command::Create);
    tool(ui, app, icons::ATTACH, "Attach", Command::Attach, Some(Command::AttachList), None);
    if app.state.selection_mode != SelectionMode::Object {
        plain(ui, app, icons::COLLAPSE_SUB, "Collapse", Command::Collapse);
        plain(ui, app, icons::DETACH, "Detach", Command::Detach);
        plain(ui, app, icons::SLICE_PLANE, "Slice Plane", Command::SlicePlaneMode);
    }
    plain(ui, app, icons::HIDE, "Hide", Command::Hide);
    plain(ui, app, icons::HIDE, "Hide Unselected", Command::HideUnselected);
    plain(ui, app, icons::UNHIDE_ALL, "Unhide All", Command::UnhideAll);
}

/// Weld with the stored threshold; Shift+click opens its settings.
fn weld(ui: &mut egui::Ui, app: &mut LoomApp) {
    let th = app.state.poly_settings.weld_threshold;
    tool(ui, app, icons::WELD, "Weld", Command::Weld { threshold: th }, Some(Command::WeldSettings), None);
}

fn bridge(ui: &mut egui::Ui, app: &mut LoomApp) {
    let ps = app.state.poly_settings;
    tool(ui, app, icons::BRIDGE, "Bridge", Command::Bridge { segments: ps.bridge_segments, smooth: ps.bridge_smooth, bridge_adjacent: ps.bridge_adjacent, twist1: ps.bridge_twist1, twist2: ps.bridge_twist2 }, Some(Command::BridgeSettings), None);
}

fn extrude(ui: &mut egui::Ui, app: &mut LoomApp) {
    tool(ui, app, icons::EXTRUDE, "Extrude", Command::ExtrudeMode, Some(Command::ExtrudeSettings), None);
}

fn chamfer(ui: &mut egui::Ui, app: &mut LoomApp) {
    tool(ui, app, icons::CHAMFER, "Chamfer", Command::ChamferMode, Some(Command::ChamferSettings), None);
}

fn create_shape(ui: &mut egui::Ui, app: &mut LoomApp) {
    let smooth = app.state.spline_settings.create_shape_smooth;
    plain(ui, app, icons::CREATE_SHAPE, "Create Shape", Command::CreateShapeFromEdges { name: None, smooth });
}

fn vertices(ui: &mut egui::Ui, app: &mut LoomApp) {
    extrude(ui, app);
    chamfer(ui, app);
    weld(ui, app);
    plain(ui, app, icons::REMOVE, "Remove", Command::Remove);
    plain(ui, app, icons::BREAK, "Break", Command::Break);
    plain(ui, app, icons::TARGET_WELD, "Target Weld", Command::TargetWeldMode);
    plain(ui, app, icons::REMOVE, "Remove Isolated Verts", Command::RemoveIsolatedVertices);
    plain(ui, app, icons::REMOVE, "Remove Unused Map Verts", Command::RemoveUnusedMapVertices);
}

fn edges(ui: &mut egui::Ui, app: &mut LoomApp) {
    extrude(ui, app);
    chamfer(ui, app);
    weld(ui, app);
    bridge(ui, app);
    // Ctrl+click: Clean Remove, the leftover vertices go too.
    tool(ui, app, icons::REMOVE, "Remove", Command::Remove, None, Some(Command::CleanRemove));
    plain(ui, app, icons::SPLIT, "Split", Command::SplitEdges);
    plain(ui, app, icons::TARGET_WELD, "Target Weld", Command::TargetWeldMode);
    plain(ui, app, icons::INSERT_VERTEX, "Insert Vertex", Command::InsertVertexMode);
    create_shape(ui, app);
}

fn borders(ui: &mut egui::Ui, app: &mut LoomApp) {
    extrude(ui, app);
    bridge(ui, app);
    chamfer(ui, app);
    plain(ui, app, icons::CAP, "Cap", Command::Cap);
    create_shape(ui, app);
}

fn polygons(ui: &mut egui::Ui, app: &mut LoomApp) {
    extrude(ui, app);
    tool(ui, app, icons::BEVEL, "Bevel", Command::BevelMode, Some(Command::BevelSettings), None);
    bridge(ui, app);
    plain(ui, app, icons::FLIP, "Flip", Command::FlipNormals);
    tool(ui, app, icons::HINGE, "Hinge", Command::HingeFromEdgeMode, Some(Command::HingeFromEdgeSettings), None);
    tool(ui, app, icons::INSET, "Inset", Command::InsetMode, Some(Command::InsetSettings), None);
    tool(ui, app, icons::OUTLINE, "Outline", Command::OutlineMode, Some(Command::OutlineSettings), None);
    tool(ui, app, icons::EXTRUDE_ON_SPLINE, "Extrude On Spline", Command::ExtrudeAlongSplineMode, Some(Command::ExtrudeAlongSplineSettings), None);
    plain(ui, app, icons::INSERT_VERTEX, "Insert Vertex", Command::InsertVertexMode);
}

fn loops(ui: &mut egui::Ui, app: &mut LoomApp) {
    if app.state.selection_mode == SelectionMode::Vertex {
        plain(ui, app, icons::CONNECT, "Connect", Command::ConnectVertices);
    } else {
        let ps = app.state.poly_settings;
        tool(ui, app, icons::CONNECT, "Connect", Command::ConnectEdges { segments: ps.connect_segments, pinch: ps.connect_pinch, slide: ps.connect_slide }, Some(Command::ConnectEdgeSettings), None);
    }
}

fn subdivision(ui: &mut egui::Ui, app: &mut LoomApp) {
    tool(ui, app, icons::MSMOOTH, "MSmooth", Command::MeshSmooth, Some(Command::MeshSmoothSettings), None);
    tool(ui, app, icons::TESSELLATE, "Tessellate", Command::Tessellate, Some(Command::TessellateSettings), None);
}

fn align(ui: &mut egui::Ui, app: &mut LoomApp) {
    plain(ui, app, icons::MAKE_PLANAR, "Make Planar", Command::MakePlanar);
    for (a, l) in [(Axis::X, "X"), (Axis::Y, "Y"), (Axis::Z, "Z")] {
        if ui.button(l).on_hover_text(format!("Make Planar {l}")).clicked() {
            let _ = app.dispatch(Command::MakePlanarAxis { axis: a });
        }
    }
    plain(ui, app, icons::ALIGN_TO_VIEW, "View Align", Command::ViewAlign);
    plain(ui, app, icons::GRID_ALIGN, "Grid Align", Command::AlignToGrid);
}

fn properties(ui: &mut egui::Ui, app: &mut LoomApp) {
    let mut id: u16 = ui.ctx().data(|d| d.get_temp(egui::Id::new("ribbon_set_id"))).unwrap_or(1);
    ui.label("Mat ID");
    if ui.add(crate::spinner::spin(ui, &mut id, 1..=65535).speed(0.1)).changed() {
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("ribbon_set_id"), id));
    }
    if ui.button("Set ID").clicked() {
        let _ = app.dispatch(Command::SetMaterialId { id });
    }
    plain(ui, app, icons::AUTO_SMOOTH, "Auto Smooth", Command::AutoSmooth);
    plain(ui, app, icons::CLEAR_SMOOTHING, "Clear SG", Command::ClearSmoothingGroups);
}

/// The Soft Selection panel (opens with Use Soft Selection at a sub-object level).
fn soft_selection(ui: &mut egui::Ui, app: &mut LoomApp) {
    let Some(soft) = app.state.selection.iter().next().and_then(|id| app.state.scene.node(*id)).and_then(|n| n.edit_poly()).map(|p| p.soft.clone()) else { return };
    let settings = |falloff, pinch, bubble, edge_distance| Command::SoftSelectionSettings { falloff, pinch, bubble, edge_distance, affect_backfacing: None };
    let speed = app.spinner_speed(0.1);
    let mut falloff = soft.falloff.get();
    ui.label("Falloff");
    if ui.add(app.length_drag(ui, &mut falloff, speed)).changed() {
        let _ = app.dispatch(settings(Some(falloff), None, None, None));
    }
    let mut pinch = soft.pinch.get();
    ui.label("Pinch");
    if ui.add(crate::spinner::spin(ui, &mut pinch, -1.0..=1.0).speed(0.01).fixed_decimals(2)).changed() {
        let _ = app.dispatch(settings(None, Some(pinch), None, None));
    }
    let mut bubble = soft.bubble.get();
    ui.label("Bubble");
    if ui.add(crate::spinner::spin(ui, &mut bubble, -1.0..=1.0).speed(0.01).fixed_decimals(2)).changed() {
        let _ = app.dispatch(settings(None, None, Some(bubble), None));
    }
    let mut ed = soft.edge_distance.is_some();
    if ui.checkbox(&mut ed, "Edge Distance").changed() {
        let _ = app.dispatch(settings(None, None, None, Some(if ed { 1 } else { 0 })));
    }
    let mut n = soft.edge_distance.unwrap_or(1);
    if ui.add_enabled(ed, crate::spinner::spin(ui, &mut n, 1..=999).speed(0.1)).changed() {
        let _ = app.dispatch(settings(None, None, None, Some(n)));
    }
    let mut ab = soft.affect_backfacing;
    if ui.checkbox(&mut ab, "Affect Backfacing").changed() {
        let _ = app.dispatch(Command::AffectBackfacing);
    }
}

/// The Use NURMS panel (opens with Use NURMS on).
fn use_nurms(ui: &mut egui::Ui, app: &mut LoomApp) {
    let Some(s) = app.state.selection.iter().next().and_then(|id| app.state.scene.node(*id)).and_then(|n| n.edit_poly()).map(|p| p.subdiv.clone()) else { return };
    let sub = |iterations, smoothness, show_cage, isoline_display| Command::SubdivisionSurface { use_nurms: None, smooth_result: None, isoline_display, show_cage, iterations, smoothness, separate_by_smoothing: None, separate_by_materials: None };
    let mut it = s.iterations.get();
    ui.label("Iterations");
    if ui.add(crate::spinner::spin(ui, &mut it, 0..=4).speed(0.1)).changed() {
        let _ = app.dispatch(sub(Some(it), None, None, None));
    }
    let mut sm = s.smoothness.get();
    ui.label("Smoothness");
    if ui.add(crate::spinner::spin(ui, &mut sm, 0.0..=1.0).speed(0.01).fixed_decimals(2)).changed() {
        let _ = app.dispatch(sub(None, Some(sm), None, None));
    }
    if ui.selectable_label(s.show_cage, "Show Cage").on_hover_text("Show Cage: draw the unsmoothed cage over the NURMS result").clicked() {
        let _ = app.dispatch(sub(None, None, Some(!s.show_cage), None));
    }
    if ui.selectable_label(s.isoline_display, "Isoline Display").clicked() {
        let _ = app.dispatch(sub(None, None, None, Some(!s.isoline_display)));
    }
}

fn polydraw(ui: &mut egui::Ui, app: &mut LoomApp) {
    plain(ui, app, icons::BRANCHES, "Branches", Command::BranchesMode);
    let ps = app.state.poly_settings;
    let (mut taper, mut dist) = (ps.branch_taper, ps.branch_min_distance);
    ui.label("Branch Taper");
    if ui.add(crate::spinner::spin(ui, &mut taper, -10.0..=10.0).speed(0.01).fixed_decimals(2)).changed() {
        let _ = app.dispatch(Command::BranchSettings { taper: Some(taper), min_distance: None });
    }
    ui.label("Minimum Distance");
    if ui.add(crate::spinner::spin(ui, &mut dist, 1.0..=500.0).speed(0.1)).on_hover_text("Pixels the mouse moves between branch segments").changed() {
        let _ = app.dispatch(Command::BranchSettings { taper: None, min_distance: Some(dist) });
    }
}

fn paint_deform(ui: &mut egui::Ui, app: &mut LoomApp) {
    plain(ui, app, icons::PAINT_DEFORM, "Push/Pull", Command::PaintDeformPushPull);
    plain(ui, app, icons::RELAX, "Relax", Command::PaintDeformRelax);
    plain(ui, app, icons::UNDO, "Revert", Command::PaintDeformRevert);
    if ui.button("Commit").clicked() {
        let _ = app.dispatch(Command::PaintDeformCommit);
    }
    if ui.button("Cancel").clicked() {
        let _ = app.dispatch(Command::PaintDeformCancel);
    }
    let paint = app.state.selection.iter().next().and_then(|id| app.state.scene.node(*id)).and_then(|n| n.edit_poly()).map(|p| p.paint.clone());
    if let Some(p) = paint {
        let (mut size, mut strength) = (p.brush_size, p.brush_strength);
        ui.label("Size");
        if ui.add(crate::spinner::spin(ui, &mut size, 0.01..=f32::MAX).speed(0.1)).changed() {
            let _ = app.dispatch(Command::PaintDeformSettings { value: None, size: Some(size), strength: None, direction: None });
        }
        ui.label("Strength");
        if ui.add(crate::spinner::spin(ui, &mut strength, 0.0..=1.0).speed(0.01)).changed() {
            let _ = app.dispatch(Command::PaintDeformSettings { value: None, size: None, strength: Some(strength), direction: None });
        }
    }
}

fn selection_tab_modify(ui: &mut egui::Ui, app: &mut LoomApp) {
    modify_selection(ui, app);
    if ui.selectable_label(app.state.select_by_vertex, "By Vertex").on_hover_text(crate::shortcuts::name_tip(&app.state, "Select By Vertex")).clicked() {
        let _ = app.dispatch(Command::SelectByVertex);
    }
    if ui.selectable_label(app.state.ignore_backfacing, "Ignore Backfacing").on_hover_text(crate::shortcuts::name_tip(&app.state, "Ignore Backfacing in Selections")).clicked() {
        let _ = app.dispatch(Command::IgnoreBackfacingInSelections);
    }
    if ui.selectable_label(app.state.preview_selection != PreviewSelect::Off, "Preview").on_hover_text(crate::shortcuts::name_tip(&app.state, "Preview Select Mode")).clicked() {
        let _ = app.dispatch(Command::PreviewSelectMode);
    }
}

/// Selection > By Half: pick the axis, Select takes the positive half, Invert
/// Axis the other one.
fn by_half(ui: &mut egui::Ui, app: &mut LoomApp) {
    let key = egui::Id::new("ribbon_by_half_axis");
    let axis: u8 = ui.ctx().data(|d| d.get_temp(key)).unwrap_or(0);
    let axes = [(Axis::X, "X"), (Axis::Y, "Y"), (Axis::Z, "Z")];
    for (i, (_, l)) in axes.iter().enumerate() {
        if ui.selectable_label(axis == i as u8, *l).on_hover_text(format!("By Half: {l} axis")).clicked() {
            ui.ctx().data_mut(|d| d.insert_temp(key, i as u8));
        }
    }
    let a = axes[axis as usize].0;
    if Icon::new(icons::BY_HALF, Command::SelectByHalf { axis: a, invert: false }.name()).tip("Select").label("Select").show(ui, &app.state).clicked() {
        let _ = app.dispatch(Command::SelectByHalf { axis: a, invert: false });
    }
    if Icon::new(icons::INVERT_AXIS, Command::SelectByHalf { axis: a, invert: true }.name()).tip("Invert Axis").label("Invert Axis").show(ui, &app.state).clicked() {
        let _ = app.dispatch(Command::SelectByHalf { axis: a, invert: true });
    }
}

fn by_material(ui: &mut egui::Ui, app: &mut LoomApp) {
    let mut id: u16 = ui.ctx().data(|d| d.get_temp(egui::Id::new("ribbon_mat_id"))).unwrap_or(1);
    ui.label("ID");
    if ui.add(crate::spinner::spin(ui, &mut id, 1..=65535).speed(0.1)).changed() {
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("ribbon_mat_id"), id));
    }
    if ui.button("By Material ID").clicked() {
        let _ = app.dispatch(Command::SelectByMaterialId { id, clear: true });
    }
    if ui.button("By Smoothing Group").on_hover_text("Polygons sharing a group with the selection").clicked() {
        let mut bits = 0u32;
        for (n, ids) in app.state.sub_selection.by_node() {
            if let Some(node) = app.state.scene.node(n) {
                for id in ids {
                    if let loom_commands::SubObjectId::Face(f) = id {
                        bits |= node.cage_mesh().faces().get(f.0 as usize).map(|x| x.smoothing_groups).unwrap_or(0);
                    }
                }
            }
        }
        let _ = app.dispatch(Command::SelectBySmoothingGroup { bits, clear: true });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_through_all_visits_every_state_in_the_reference_order() {
        let mut s = RibbonState::Maximized;
        let mut seen = Vec::new();
        for _ in 0..4 {
            s = s.next();
            seen.push(s);
        }
        assert_eq!(seen, [RibbonState::PanelButtons, RibbonState::PanelTitles, RibbonState::Tabs, RibbonState::Maximized]);
        assert_eq!(RibbonState::default(), RibbonState::PanelTitles, "the ribbon starts minimized to panel titles");
        assert_eq!(MinimizeOption::default(), MinimizeOption::CycleThroughAll);
    }
}
