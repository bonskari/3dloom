//! Toolbars: each toolbar (and the command panel) docks at a window edge
//! or floats; a right-click on a toolbar's empty area opens the Customize
//! Display menu (Dock, Float, Customize..., the toolbar list). The
//! program's hidden toolbars (Axis Constraints, Extras, Snaps) and the
//! user's own toolbars (Customize User Interface > Toolbars) live here too;
//! actions dragged from the Customize dialog drop onto a user toolbar, and
//! Ctrl+drag (copy) / Alt+drag (move) carries buttons between them.

use crate::icons::{self, Icon};
use crate::shortcuts::HoverShortcut;
use crate::LoomApp;
use eframe::egui;
use loom_commands::ui_custom::{self, Dock, BUILTIN_TOOLBARS, SEPARATOR};
use loom_commands::{AxisConstraint, Command};

/// An action dragged from a Customize User Interface Action list (an action
/// name, `ui_custom::SEPARATOR` or `menu:<name>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionDrag(pub String);

/// A user toolbar button being Ctrl+dragged (copy) or Alt+dragged (move).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ButtonDrag {
    pub toolbar: String,
    pub index: usize,
    pub copy: bool,
}

/// What a toolbar's frame reports back: its own response (for drops) and
/// whether it lays out vertically.
pub struct Hosted {
    pub response: egui::Response,
    pub vertical: bool,
}

/// The panel id of a toolbar docked at an edge (the Main Toolbar keeps its
/// own id, which other code and tests know it by).
pub fn panel_id(name: &str) -> egui::Id {
    match name {
        "Main Toolbar" => egui::Id::new("main_toolbar"),
        "Layers" => egui::Id::new("layers_toolbar"),
        "Command Panel" => egui::Id::new("command_panel"),
        _ => egui::Id::new(("toolbar", name.to_owned())),
    }
}

/// The floating window's id.
pub fn window_id(name: &str) -> egui::Id {
    egui::Id::new(("toolbar_window", name.to_owned()))
}

/// Draws toolbar `name` where it is docked (or floating) with `contents`,
/// laid out in a row (top, bottom, floating) or a column (left, right).
/// `edges` limits which docks are drawn now: the top / bottom pass and the
/// left / right pass happen at different points of the frame.
pub fn host(ctx: &egui::Context, app: &mut LoomApp, name: &str, edges: &[Dock], contents: impl FnOnce(&mut egui::Ui, &mut LoomApp)) -> Option<Hosted> {
    let dock = app.state.ui_custom.dock(name);
    if !edges.contains(&dock) {
        return None;
    }
    let body = |ui: &mut egui::Ui, app: &mut LoomApp, vertical: bool| {
        // The empty area: a right-click there opens the Customize Display menu.
        let bg = ui.interact(ui.max_rect(), ui.id().with("toolbar_bg"), egui::Sense::click());
        bg.context_menu(|ui| customize_display_menu(ui, app, name));
        if vertical {
            ui.vertical(|ui| contents(ui, app));
        } else {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                contents(ui, app)
            });
        }
    };
    let locked = app.state.ui_locked;
    Some(match dock {
        Dock::Top | Dock::Bottom => {
            let panel = if dock == Dock::Top { egui::TopBottomPanel::top(panel_id(name)) } else { egui::TopBottomPanel::bottom(panel_id(name)) };
            let r = panel.show(ctx, |ui| body(ui, app, false));
            Hosted { response: r.response, vertical: false }
        }
        Dock::Left | Dock::Right => {
            let panel = if dock == Dock::Left { egui::SidePanel::left(panel_id(name)) } else { egui::SidePanel::right(panel_id(name)) };
            // A column: separators and drop-downs must not stretch it across the viewports.
            let r = panel.resizable(!locked).default_width(44.0).width_range(28.0..=96.0).show(ctx, |ui| body(ui, app, true));
            Hosted { response: r.response, vertical: true }
        }
        Dock::Float => {
            let mut open = true;
            let pos = app.state.ui_custom.float_pos(name);
            let r = egui::Window::new(name)
                .id(window_id(name))
                .default_pos(egui::pos2(pos[0], pos[1]))
                .collapsible(false)
                .resizable(true)
                .movable(!locked)
                .default_width(if name == "Main Toolbar" { 640.0 } else { 160.0 })
                .open(&mut open)
                .show(ctx, |ui| body(ui, app, false))?;
            remember_float_pos(ctx, app, name, r.response.rect.min);
            if !open {
                let _ = app.dispatch(Command::ShowToolbar { name: name.to_owned(), on: Some(false) });
            }
            Hosted { response: r.response, vertical: false }
        }
    })
}

/// A floating toolbar that was moved keeps its place (saved with the settings) once the drag ends.
fn remember_float_pos(ctx: &egui::Context, app: &mut LoomApp, name: &str, min: egui::Pos2) {
    let saved = app.state.ui_custom.float_pos(name);
    let moved = (saved[0] - min.x).abs() > 1.0 || (saved[1] - min.y).abs() > 1.0;
    if moved && !ctx.input(|i| i.pointer.any_down()) {
        let _ = app.dispatch(Command::DockToolbar { name: name.to_owned(), dock: "Float".into(), pos: Some([min.x, min.y]) });
    }
}

/// Every toolbar the right-click menu lists: the program's, then the user's.
pub fn all_toolbars(app: &LoomApp) -> Vec<String> {
    BUILTIN_TOOLBARS.iter().map(|s| (*s).to_owned()).chain(app.state.ui_custom.toolbars.iter().map(|t| t.name.clone())).collect()
}

/// The Customize Display right-click menu of toolbar `name`.
pub fn customize_display_menu(ui: &mut egui::Ui, app: &mut LoomApp, name: &str) {
    ui.set_min_width(170.0);
    let docks: &[Dock] = if name == "Command Panel" { &[Dock::Left, Dock::Right] } else { &[Dock::Top, Dock::Bottom, Dock::Left, Dock::Right] };
    let current = app.state.ui_custom.dock(name);
    ui.menu_button("Dock", |ui| {
        for d in docks {
            let label = if current == *d { format!("\u{2714} {}", d.label()) } else { format!("    {}", d.label()) };
            if ui.button(label).clicked() {
                let _ = app.dispatch(Command::DockToolbar { name: name.to_owned(), dock: d.label().into(), pos: None });
                ui.close_menu();
            }
        }
    });
    if ui.add_enabled(current != Dock::Float, egui::Button::new("Float")).clicked() {
        let _ = app.dispatch(Command::DockToolbar { name: name.to_owned(), dock: "Float".into(), pos: None });
        ui.close_menu();
    }
    ui.separator();
    if ui.button("Customize...").clicked() {
        crate::dialogs::customize_ui::open_on(ui.ctx(), crate::dialogs::customize_ui::Panel::Toolbars);
        if let Some(t) = app.state.ui_custom.toolbar(name) {
            crate::dialogs::customize_ui::pick_toolbar(ui.ctx(), &t.name.clone());
        }
        let _ = app.dispatch(Command::CustomizeUserInterface);
        ui.close_menu();
    }
    ui.separator();
    for t in all_toolbars(app) {
        let on = ui_custom::is_shown(&app.state, &t);
        let label = if on { format!("\u{2714} {t}") } else { format!("    {t}") };
        if ui.button(label).clicked() {
            let _ = app.dispatch(Command::ShowToolbar { name: t, on: Some(!on) });
            ui.close_menu();
        }
    }
}

/// The top / bottom pass: toolbars docked at the top or bottom edge.
pub fn show_edges(ctx: &egui::Context, app: &mut LoomApp, edges: &[Dock]) {
    if app.state.expert_mode {
        return;
    }
    if app.state.show_main_toolbar {
        crate::main_toolbar::show(ctx, app, edges);
    }
    if app.state.show_layers_toolbar {
        let _ = host(ctx, app, "Layers", edges, crate::layers_toolbar::contents);
    }
    for name in ["Axis Constraints", "Extras", "Snaps"] {
        if ui_custom::is_shown(&app.state, name) {
            let _ = host(ctx, app, name, edges, |ui, app| match name {
                "Axis Constraints" => axis_constraints(ui, app),
                "Extras" => extras(ui, app),
                _ => snaps(ui, app),
            });
        }
    }
    let users: Vec<String> = app.state.ui_custom.toolbars.iter().filter(|t| t.shown).map(|t| t.name.clone()).collect();
    for name in users {
        let rects = std::cell::RefCell::new(Vec::new());
        let hosted = host(ctx, app, &name, edges, |ui, app| *rects.borrow_mut() = user_toolbar(ui, app, &name));
        if let Some(h) = hosted {
            drops(ctx, app, &name, &h, &rects.into_inner());
        }
    }
}

/// Where a drop lands among a toolbar's buttons (`rects`, in order).
fn insert_index(rects: &[egui::Rect], at: egui::Pos2, vertical: bool) -> usize {
    rects.iter().filter(|r| if vertical { r.center().y < at.y } else { r.center().x < at.x || r.max.y < at.y }).count()
}

/// Actions dropped from the Customize dialog, and buttons Ctrl / Alt+dragged from another user toolbar.
fn drops(ctx: &egui::Context, app: &mut LoomApp, name: &str, h: &Hosted, rects: &[egui::Rect]) {
    let Some(at) = ctx.input(|i| i.pointer.interact_pos()) else { return };
    if !h.response.rect.contains(at) {
        return;
    }
    let released = ctx.input(|i| i.pointer.any_released());
    if !released {
        return;
    }
    let index = insert_index(rects, at, h.vertical) as u32;
    if let Some(a) = egui::DragAndDrop::payload::<ActionDrag>(ctx) {
        egui::DragAndDrop::clear_payload(ctx);
        if let Err(e) = app.dispatch(Command::AddToolbarButton { toolbar: name.to_owned(), action: a.0.clone(), index: Some(index) }) {
            app.state.status_prompt = e;
        }
    } else if let Some(b) = egui::DragAndDrop::payload::<ButtonDrag>(ctx) {
        egui::DragAndDrop::clear_payload(ctx);
        // Moving a button later along its own toolbar: its old slot goes first.
        let to = if b.toolbar == name && !b.copy && (b.index as u32) < index { index - 1 } else { index };
        let _ = app.dispatch(Command::MoveToolbarButton { toolbar: b.toolbar.clone(), index: b.index as u32, to_toolbar: name.to_owned(), to_index: Some(to), copy: b.copy });
    }
}

/// A user toolbar's buttons; returns their rects (drops land between them).
fn user_toolbar(ui: &mut egui::Ui, app: &mut LoomApp, name: &str) -> Vec<egui::Rect> {
    let Some(t) = app.state.ui_custom.toolbar(name).cloned() else { return Vec::new() };
    let mut rects = Vec::new();
    if t.items.is_empty() {
        ui.weak("Drag actions here");
    }
    for (i, item) in t.items.iter().enumerate() {
        if item == SEPARATOR {
            let r = ui.separator();
            rects.push(r.rect);
            continue;
        }
        let r = action_button(ui, app, item).interact(egui::Sense::drag());
        let (ctrl, alt) = ui.input(|i| (i.modifiers.ctrl, i.modifiers.alt));
        if r.drag_started() && (ctrl || alt) {
            r.dnd_set_drag_payload(ButtonDrag { toolbar: name.to_owned(), index: i, copy: ctrl });
        }
        if r.clicked() {
            app.dispatch_named(item);
        }
        r.context_menu(|ui| {
            if ui.button("Delete Button").clicked() {
                let _ = app.dispatch(Command::RemoveToolbarButton { toolbar: name.to_owned(), index: i as u32 });
                ui.close_menu();
            }
            if ui.button("Edit Button Appearance...").clicked() {
                app.button_look = Some(item.clone());
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Customize...").clicked() {
                crate::dialogs::customize_ui::open_on(ui.ctx(), crate::dialogs::customize_ui::Panel::Toolbars);
                crate::dialogs::customize_ui::pick_toolbar(ui.ctx(), name);
                let _ = app.dispatch(Command::CustomizeUserInterface);
                ui.close_menu();
            }
        });
        rects.push(r.rect);
    }
    rects
}

/// A button for `action` as Edit Button Appearance set it: its icon, or its
/// text, with its tooltip. Its name (for screen readers and tests) is the action.
pub fn action_button(ui: &mut egui::Ui, app: &LoomApp, action: &str) -> egui::Response {
    let look = app.state.ui_custom.looks.get(action).cloned().unwrap_or_default();
    let tip = if look.tooltip.is_empty() { action.to_owned() } else { look.tooltip.clone() };
    let custom_icon = (!look.icon.is_empty()).then(|| icons::by_name(&look.icon)).flatten();
    // Image Button with a chosen icon, or the action's own icon unless a Text Button was asked for.
    let glyph = if look.image { custom_icon.or_else(|| icons::for_action(action)) } else if look == Default::default() { icons::for_action(action) } else { None };
    match glyph {
        Some(g) => Icon::new(g, action).tip(&tip).show(ui, &app.state),
        None => {
            let text = if look.text.is_empty() { action.trim().to_owned() } else { look.text.clone() };
            let r = ui.button(text);
            icons::named(&r, egui::WidgetType::Button, action.trim());
            r.on_hover_cmd(&app.state, &tip, action)
        }
    }
}

/// Axis Constraints: Restrict to X, Y, Z, the plane flyout (XY, YZ, ZX), Snaps Use Axis Constraints.
fn axis_constraints(ui: &mut egui::Ui, app: &mut LoomApp) {
    let c = app.state.axis_constraint;
    for (label, on, action) in [("X", c == AxisConstraint::X, "Transform Gizmo X Constraint"), ("Y", c == AxisConstraint::Y, "Transform Gizmo Y Constraint"), ("Z", c == AxisConstraint::Z, "Transform Gizmo Z Constraint")] {
        let r = ui.add(egui::Button::new(label).selected(on));
        icons::named(&r, egui::WidgetType::Button, &format!("Restrict to {label}"));
        if r.on_hover_cmd(&app.state, &format!("Restrict to {label}"), action).clicked() {
            app.dispatch_named(action);
        }
    }
    // The flyout shows the plane last used; a click restricts to it, the arrow picks another.
    let planes = [(AxisConstraint::XY, "XY"), (AxisConstraint::YZ, "YZ"), (AxisConstraint::ZX, "ZX")];
    let last_id = egui::Id::new("axis_constraints_plane");
    let mut last: usize = ui.ctx().data(|d| d.get_temp(last_id)).unwrap_or(0);
    if let Some(i) = planes.iter().position(|(p, _)| *p == c) {
        last = i;
    }
    let (plane, label) = planes[last];
    let r = ui.add(egui::Button::new(label).selected(c == plane));
    icons::named(&r, egui::WidgetType::Button, &format!("Restrict to {label} Plane"));
    if r.on_hover_cmd(&app.state, &format!("Restrict to {label} Plane"), "Transform Gizmo Plane Constraint Cycle").clicked() {
        let _ = app.dispatch(Command::SetAxisConstraint { constraint: label.into() });
    }
    egui::ComboBox::from_id_salt("axis_plane_flyout").selected_text("").width(14.0).show_ui(ui, |ui| {
        for (i, (p, l)) in planes.iter().enumerate() {
            if ui.selectable_label(c == *p, format!("Restrict to {l} Plane")).clicked() {
                last = i;
                let _ = app.dispatch(Command::SetAxisConstraint { constraint: (*l).into() });
            }
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(last_id, last));
    ui.separator();
    let on = app.state.snaps.use_axis_constraints;
    let r = ui.add(egui::Button::new(icons::SNAP).selected(on));
    icons::named(&r, egui::WidgetType::Button, "Snaps Use Axis Constraints Toggle");
    if r.on_hover_name(&app.state, Command::EnableAxisConstraintsInSnapsToggle.name()).clicked() {
        let _ = app.dispatch(Command::EnableAxisConstraintsInSnapsToggle);
    }
}

/// Extras: AutoGrid and the Array flyout (Array, Snapshot, Spacing Tool, Clone and Align).
fn extras(ui: &mut egui::Ui, app: &mut LoomApp) {
    let r = ui.add(egui::Button::new("AutoGrid").selected(app.autogrid)).on_hover_text("AutoGrid: new objects are created on the surface under the cursor");
    if r.clicked() {
        app.autogrid = !app.autogrid;
    }
    let items: [(&str, Command); 4] = [("Array", Command::Array), ("Snapshot", Command::SnapshotDialog), ("Spacing Tool", Command::SpacingTool), ("Clone and Align", Command::CloneAndAlignDialog)];
    let last_id = egui::Id::new("extras_array_flyout");
    let mut last: usize = ui.ctx().data(|d| d.get_temp(last_id)).unwrap_or(0);
    let r = ui.button(items[last].0).on_hover_text(format!("{}; right-click: Array flyout", items[last].0));
    if r.clicked() {
        let _ = app.dispatch(items[last].1.clone());
    }
    let popup = ui.id().with("array_flyout");
    if r.secondary_clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup));
    }
    egui::popup::popup_below_widget(ui, popup, &r, egui::PopupCloseBehavior::CloseOnClick, |ui| {
        ui.set_min_width(130.0);
        for (i, (label, c)) in items.iter().enumerate() {
            if ui.button(*label).clicked() {
                last = i;
                let _ = app.dispatch(c.clone());
            }
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(last_id, last));
}

/// Snaps: the snap toggle, every standard snap target, Snap to Frozen Objects and Use Axis Constraints.
fn snaps(ui: &mut egui::Ui, app: &mut LoomApp) {
    let on = app.state.snaps_on;
    if Icon::new(icons::SNAP, "Snaps Toggle").on(on).show(ui, &app.state).clicked() {
        app.dispatch_named("Snaps Toggle");
    }
    ui.separator();
    for n in loom_commands::snap::SnapTargets::NAMES {
        let on = app.state.snaps.targets.get(n).unwrap_or(false);
        if ui.add(egui::Button::new(n).selected(on)).on_hover_text(format!("Snap to {n}")).clicked() {
            let _ = app.dispatch(Command::SnapTarget { name: n.to_owned(), on: !on });
        }
    }
    ui.separator();
    let frozen = app.state.snaps.snap_to_frozen;
    if ui.add(egui::Button::new("Frozen").selected(frozen)).on_hover_name(&app.state, Command::SnapToFrozenObjectsToggle.name()).clicked() {
        let _ = app.dispatch(Command::SnapToFrozenObjectsToggle);
    }
    let axis = app.state.snaps.use_axis_constraints;
    if ui.add(egui::Button::new("Axis").selected(axis)).on_hover_name(&app.state, Command::EnableAxisConstraintsInSnapsToggle.name()).clicked() {
        let _ = app.dispatch(Command::EnableAxisConstraintsInSnapsToggle);
    }
}

/// Edit Button Appearance (a user toolbar button's right-click): Image or
/// Text Button, the icon, the label and the tooltip.
pub fn button_appearance(ctx: &egui::Context, app: &mut LoomApp) {
    let Some(action) = app.button_look.clone() else { return };
    let id = egui::Id::new("button_appearance_edit");
    let mut look: loom_commands::ui_custom::ButtonLook = ctx.data(|d| d.get_temp(id)).unwrap_or_else(|| {
        let mut l = app.state.ui_custom.looks.get(&action).cloned().unwrap_or_default();
        if l == Default::default() {
            l.image = icons::for_action(&action).is_some();
        }
        l
    });
    let mut open = true;
    let mut done: Option<bool> = None;
    egui::Window::new("Edit Button Appearance").open(&mut open).collapsible(false).resizable(false).default_width(360.0).show(ctx, |ui| {
        ui.label(format!("Action: {}", action.trim()));
        ui.horizontal(|ui| {
            ui.label("Tooltip:");
            ui.add(egui::TextEdit::singleline(&mut look.tooltip).hint_text(action.trim()).desired_width(250.0));
        });
        ui.radio_value(&mut look.image, false, "Text Button");
        ui.add_enabled(!look.image, egui::TextEdit::singleline(&mut look.text).hint_text(action.trim()).desired_width(250.0));
        ui.radio_value(&mut look.image, true, "Image Button");
        ui.add_enabled_ui(look.image, |ui| {
            egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (name, glyph) in icons::GALLERY {
                        let on = look.icon == *name || (look.icon.is_empty() && icons::for_action(&action) == Some(*glyph));
                        let r = ui.add(egui::Button::new(egui::RichText::new(*glyph).size(icons::SIZE)).selected(on));
                        icons::named(&r, egui::WidgetType::Button, name);
                        if r.on_hover_text(*name).clicked() {
                            look.icon = (*name).to_owned();
                        }
                    }
                });
            });
        });
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                done = Some(true);
            }
            if ui.button("Cancel").clicked() {
                done = Some(false);
            }
        });
    });
    if !open {
        done = Some(false);
    }
    match done {
        Some(ok) => {
            if ok {
                let _ = app.dispatch(Command::SetButtonAppearance { action: action.clone(), image: Some(look.image), text: Some(look.text.clone()), icon: Some(look.icon.clone()), tooltip: Some(look.tooltip.clone()) });
            }
            app.button_look = None;
            ctx.data_mut(|d| d.remove::<loom_commands::ui_custom::ButtonLook>(id));
        }
        None => ctx.data_mut(|d| d.insert_temp(id, look)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_land_between_buttons() {
        let r = |x: f32| egui::Rect::from_min_size(egui::pos2(x, 0.0), egui::vec2(20.0, 20.0));
        let rects = [r(0.0), r(30.0), r(60.0)];
        assert_eq!(insert_index(&rects, egui::pos2(-5.0, 10.0), false), 0);
        assert_eq!(insert_index(&rects, egui::pos2(45.0, 10.0), false), 2);
        assert_eq!(insert_index(&rects, egui::pos2(200.0, 10.0), false), 3);
    }

    #[test]
    fn action_icons_name_registered_actions() {
        for (a, _) in icons::ACTION_ICONS {
            assert!(loom_commands::registry().iter().any(|i| i.name == *a), "{a:?} is not an action");
        }
    }
}
