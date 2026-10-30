//! Schematic View windows (Graph Editors > New Schematic View, the Main
//! Toolbar button): the scene as a graph of named boxes.
//!
//! As in the reference: a click selects an entity (Ctrl adds or removes,
//! Alt removes), a drag on empty space selects by region; with Select and
//! Always Arrange off, dragging moves the selected boxes, which become free
//! (an open end); with Connect, a drag from a child object to another object
//! links it (from a material: assigns it). Hierarchy Mode indents children
//! below their parent, References Mode sets objects side by side with what
//! they use below. The middle button pans, the wheel zooms. A box with a
//! white fill is selected in the view, a white border marks an object
//! selected in the viewports, a red border an animated entity.

use crate::icons::{self, Icon};
use crate::LoomApp;
use eframe::egui;
use loom_commands::schematic::{self, Entity, EntityKind, SchMode, SchematicView, WireKind};
use loom_commands::{Command, SelectOp};
use std::collections::{BTreeMap, BTreeSet};

pub fn window_id(uid: u32) -> egui::Id {
    egui::Id::new(("schematic_view", uid))
}

fn ui_id(uid: u32) -> egui::Id {
    window_id(uid).with("ui")
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tool {
    #[default]
    Select,
    Connect,
}

#[derive(Clone, Copy, PartialEq)]
enum ViewOp {
    ZoomExtents,
    ZoomExtentsSelected,
    PanToSelected,
}

#[derive(Clone, Default)]
struct SvUi {
    tool: Tool,
    /// Boxes being moved (they were made free when the drag began).
    moving: Vec<String>,
    region: Option<egui::Pos2>,
    connect_from: Option<String>,
    floater: bool,
    name: Option<String>,
    find: String,
    pending: Option<ViewOp>,
}

enum Act {
    Cmd(Command),
    Select(Vec<String>, SelectOp),
    Collapse(String, bool),
    SetCollapsed(BTreeSet<String>),
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp) {
    let open: Vec<u32> = app.state.schematics.list.iter().filter(|v| v.open).map(|v| v.id).collect();
    for uid in open {
        window(ctx, app, uid);
    }
}

/// The fill of an entity box by what it is.
fn fill(e: &Entity, app: &LoomApp) -> egui::Color32 {
    match e.kind {
        EntityKind::Object => app.state.scene.node(e.node).map(|n| crate::command_panel::scene::icon_kind(&n.object).color().gamma_multiply(0.75)).unwrap_or(egui::Color32::GRAY),
        EntityKind::BaseObject => egui::Color32::from_rgb(150, 150, 150),
        EntityKind::Modifier(_) => egui::Color32::from_rgb(170, 130, 210),
        EntityKind::Material(_) => egui::Color32::from_rgb(90, 180, 160),
        EntityKind::Transform | EntityKind::Controller => egui::Color32::from_rgb(220, 160, 80),
    }
}

fn window(ctx: &egui::Context, app: &mut LoomApp, uid: u32) {
    let Some(i) = app.state.schematics.list.iter().position(|v| v.id == uid) else { return };
    let v = app.state.schematics.list[i].clone();
    let mut st: SvUi = ctx.data(|d| d.get_temp(ui_id(uid))).unwrap_or_default();
    let (all, wires) = schematic::graph(&app.state, &v);
    let ents = schematic::visible(&v, &all);
    let pos = schematic::positions(&v, &ents);
    let sel = schematic::selected(&app.state, &v);
    let mut acts: Vec<Act> = Vec::new();
    let mut open = true;
    let offset = (uid % 6) as f32 * 24.0;
    egui::Window::new(v.name.clone())
        .id(window_id(uid))
        .open(&mut open)
        .default_size([720.0, 440.0])
        .default_pos([140.0 + offset, 160.0 + offset])
        .resizable(true)
        .show(ctx, |ui| body(ui, app, i, &v, &all, &ents, &wires, &pos, &sel, &mut st, &mut acts));
    if st.floater {
        display_floater(ctx, &v, &mut st, &mut acts);
    }
    ctx.data_mut(|d| d.insert_temp(ui_id(uid), st));
    for a in acts {
        let Some(i) = app.state.schematics.list.iter().position(|v| v.id == uid) else { break };
        let name = app.state.schematics.list[i].name.clone();
        match a {
            Act::Cmd(c) => {
                if let Err(e) = app.dispatch(c) {
                    app.state.status_prompt = e;
                }
            }
            Act::Select(keys, op) => {
                let _ = app.dispatch(Command::SchematicSelect { view: Some(name), keys, op });
            }
            Act::Collapse(k, on) => {
                let c = &mut app.state.schematics.list[i].collapsed;
                if on {
                    c.insert(k);
                } else {
                    c.remove(&k);
                }
            }
            Act::SetCollapsed(set) => app.state.schematics.list[i].collapsed = set,
        }
    }
    if !open {
        let _ = app.dispatch(Command::CloseSchematicView { name: Some(v.name.clone()) });
    }
}

fn opt(v: &SchematicView, option: &str, on: bool) -> Act {
    Act::Cmd(Command::SchematicOption { view: Some(v.name.clone()), option: option.into(), on })
}

fn item(ui: &mut egui::Ui, label: &str) -> bool {
    let c = ui.button(label).clicked();
    if c {
        ui.close_menu();
    }
    c
}

fn check(ui: &mut egui::Ui, label: &str, on: bool) -> bool {
    let text = if on { format!("{}  {label}", icons::CHECKED) } else { format!("      {label}") };
    let r = ui.button(text);
    r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, label));
    let c = r.clicked();
    if c {
        ui.close_menu();
    }
    c
}

/// The menu entries shared by the menu bar and the right-click menu.
fn select_entries(ui: &mut egui::Ui, v: &SchematicView, ents: &[Entity], sel: &BTreeSet<String>, acts: &mut Vec<Act>) {
    let keys: Vec<String> = ents.iter().map(|e| e.key.clone()).collect();
    if item(ui, "Select All") {
        acts.push(Act::Select(keys.clone(), SelectOp::Replace));
    }
    if item(ui, "Select None") {
        acts.push(Act::Select(Vec::new(), SelectOp::Replace));
    }
    if item(ui, "Select Invert") {
        acts.push(Act::Select(keys.iter().filter(|k| !sel.contains(*k)).cloned().collect(), SelectOp::Replace));
    }
    let kids = schematic::descendants(ents, sel);
    if item(ui, "Select Children") {
        acts.push(Act::Select(kids.iter().cloned().collect(), SelectOp::Add));
    }
    if item(ui, "Deselect Children") {
        acts.push(Act::Select(kids.iter().filter(|k| sel.contains(*k)).cloned().collect(), SelectOp::Subtract));
    }
    ui.separator();
    if check(ui, "Sync Selection", v.sync_selection) {
        acts.push(opt(v, "Sync Selection", !v.sync_selection));
    }
}

fn edit_entries(ui: &mut egui::Ui, v: &SchematicView, st: &mut SvUi, acts: &mut Vec<Act>) {
    if check(ui, "Connect", st.tool == Tool::Connect) {
        st.tool = if st.tool == Tool::Connect { Tool::Select } else { Tool::Connect };
    }
    if item(ui, "Unlink Selected") {
        acts.push(Act::Cmd(Command::SchematicUnlinkSelected { view: Some(v.name.clone()) }));
    }
    if item(ui, "Delete") {
        acts.push(Act::Cmd(Command::SchematicDeleteSelected { view: Some(v.name.clone()) }));
    }
    if item(ui, "Object Properties...") {
        acts.push(Act::Cmd(Command::ObjectProperties));
    }
}

fn layout_entries(ui: &mut egui::Ui, v: &SchematicView, ents: &[Entity], sel: &BTreeSet<String>, acts: &mut Vec<Act>) {
    if item(ui, "Arrange Selected") {
        acts.push(opt(v, "Arrange Selected", true));
    }
    if item(ui, "Free Selected") {
        acts.push(opt(v, "Free Selected", true));
    }
    if item(ui, "Free All") {
        acts.push(opt(v, "Free All", true));
    }
    ui.separator();
    let has_kids: BTreeSet<String> = ents.iter().filter_map(|e| e.owner.clone()).collect();
    if item(ui, "Expand Selected") {
        acts.push(Act::SetCollapsed(v.collapsed.difference(sel).cloned().collect()));
    }
    if item(ui, "Collapse Selected") {
        acts.push(Act::SetCollapsed(v.collapsed.union(&sel.intersection(&has_kids).cloned().collect()).cloned().collect()));
    }
}

fn mode_entries(ui: &mut egui::Ui, v: &SchematicView, acts: &mut Vec<Act>) {
    if check(ui, "Always Arrange", v.always_arrange) {
        acts.push(opt(v, "Always Arrange", !v.always_arrange));
    }
    if check(ui, "Hierarchy Mode", v.mode == SchMode::Hierarchy) {
        acts.push(opt(v, "Hierarchy Mode", true));
    }
    if check(ui, "References Mode", v.mode == SchMode::References) {
        acts.push(opt(v, "References Mode", true));
    }
    if check(ui, "Move Children", v.move_children) {
        acts.push(opt(v, "Move Children", !v.move_children));
    }
}

#[allow(clippy::too_many_arguments)]
fn body(ui: &mut egui::Ui, app: &mut LoomApp, i: usize, v: &SchematicView, all: &[Entity], ents: &[Entity], wires: &[schematic::Wire], pos: &BTreeMap<String, [f32; 2]>, sel: &BTreeSet<String>, st: &mut SvUi, acts: &mut Vec<Act>) {
    egui::menu::bar(ui, |ui| {
        ui.menu_button("Edit", |ui| edit_entries(ui, v, st, acts));
        ui.menu_button("Select", |ui| {
            if item(ui, "Select Tool") {
                st.tool = Tool::Select;
            }
            select_entries(ui, v, ents, sel, acts);
            ui.separator();
            if item(ui, "Select to Scene") {
                let ids = sel.iter().filter_map(|k| schematic::key_node(k)).map(|n| n.0).collect();
                acts.push(Act::Cmd(Command::SelectNodes { ids, op: SelectOp::Replace }));
            }
            if item(ui, "Select from Scene") {
                let keys = app.state.selection.iter().map(|n| schematic::obj_key(*n)).collect();
                acts.push(Act::Select(keys, SelectOp::Replace));
            }
        });
        ui.menu_button("Layout", |ui| layout_entries(ui, v, ents, sel, acts));
        ui.menu_button("Options", |ui| mode_entries(ui, v, acts));
        ui.menu_button("Display", |ui| {
            if check(ui, "Display Floater", st.floater) {
                st.floater = !st.floater;
            }
            if item(ui, "Expand All") {
                acts.push(Act::SetCollapsed(BTreeSet::new()));
            }
            if item(ui, "Collapse All") {
                acts.push(Act::SetCollapsed(all.iter().filter_map(|e| e.owner.clone()).collect()));
            }
        });
        ui.menu_button("View", |ui| {
            if item(ui, "Zoom Extents") {
                st.pending = Some(ViewOp::ZoomExtents);
            }
            if item(ui, "Zoom Extents Selected") {
                st.pending = Some(ViewOp::ZoomExtentsSelected);
            }
            if item(ui, "Pan to Selected") {
                st.pending = Some(ViewOp::PanToSelected);
            }
            if check(ui, "Show Grid", v.show_grid) {
                acts.push(opt(v, "Show Grid", !v.show_grid));
            }
        });
    });
    // Upper toolbar.
    ui.horizontal(|ui| {
        if Icon::new(icons::DISPLAY_FLOATER, "Display Floater").on(st.floater).show(ui, &app.state).clicked() {
            st.floater = !st.floater;
        }
        ui.separator();
        if Icon::new(icons::SELECT_OBJECT, "Select").on(st.tool == Tool::Select).show(ui, &app.state).clicked() {
            st.tool = Tool::Select;
        }
        if Icon::new(icons::CONNECT_TOOL, "Connect").on(st.tool == Tool::Connect).show(ui, &app.state).clicked() {
            st.tool = if st.tool == Tool::Connect { Tool::Select } else { Tool::Connect };
        }
        if Icon::new(icons::UNLINK, "Unlink Selected").show(ui, &app.state).clicked() {
            acts.push(Act::Cmd(Command::SchematicUnlinkSelected { view: Some(v.name.clone()) }));
        }
        if Icon::new(icons::DELETE_OBJECTS, "Delete Objects").show(ui, &app.state).clicked() {
            acts.push(Act::Cmd(Command::SchematicDeleteSelected { view: Some(v.name.clone()) }));
        }
        ui.separator();
        if Icon::new(icons::HIERARCHY_MODE, "Hierarchy Mode").on(v.mode == SchMode::Hierarchy).show(ui, &app.state).clicked() {
            acts.push(opt(v, "Hierarchy Mode", true));
        }
        if Icon::new(icons::REFERENCES_MODE, "References Mode").on(v.mode == SchMode::References).show(ui, &app.state).clicked() {
            acts.push(opt(v, "References Mode", true));
        }
        ui.separator();
        if Icon::new(icons::ALWAYS_ARRANGE, "Always Arrange").on(v.always_arrange).show(ui, &app.state).clicked() {
            acts.push(opt(v, "Always Arrange", !v.always_arrange));
        }
        if Icon::new(icons::ARRANGE_SELECTED, "Arrange Selected").show(ui, &app.state).clicked() {
            acts.push(opt(v, "Arrange Selected", true));
        }
        if Icon::new(icons::FREE_ALL, "Free All").show(ui, &app.state).clicked() {
            acts.push(opt(v, "Free All", true));
        }
        if Icon::new(icons::FREE_SELECTED, "Free Selected").show(ui, &app.state).clicked() {
            acts.push(opt(v, "Free Selected", true));
        }
        if Icon::new(icons::MOVE_CHILDREN, "Move Children").on(v.move_children).show(ui, &app.state).clicked() {
            acts.push(opt(v, "Move Children", !v.move_children));
        }
        ui.separator();
        let has_kids: BTreeSet<String> = ents.iter().filter_map(|e| e.owner.clone()).collect();
        if Icon::new(icons::EXPAND, "Expand Selected").show(ui, &app.state).clicked() {
            acts.push(Act::SetCollapsed(v.collapsed.difference(sel).cloned().collect()));
        }
        if Icon::new(icons::COLLAPSE, "Collapse Selected").show(ui, &app.state).clicked() {
            acts.push(Act::SetCollapsed(v.collapsed.union(&sel.intersection(&has_kids).cloned().collect()).cloned().collect()));
        }
        ui.separator();
        // The view's name field: Enter renames it (Saved Schematic Views lists it so).
        let name = st.name.get_or_insert_with(|| v.name.clone());
        let r = ui.add(egui::TextEdit::singleline(name).desired_width(120.0));
        icons::named(&r, egui::WidgetType::TextEdit, "Schematic View Name");
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && *name != v.name {
            acts.push(Act::Cmd(Command::RenameSchematicView { view: Some(v.name.clone()), name: name.clone() }));
        }
        if !r.has_focus() && !r.lost_focus() && *name != v.name {
            *name = v.name.clone();
        }
    });
    // Lower toolbar.
    egui::TopBottomPanel::bottom(egui::Id::new(("schematic_lower", v.id))).show_inside(ui, |ui| {
        ui.horizontal(|ui| {
            let zoom_obj = Icon::new(icons::ZOOM_SELECTED, "Zoom Selected Viewport Object").show(ui, &app.state).clicked();
            let r = ui.add(egui::TextEdit::singleline(&mut st.find).desired_width(110.0).hint_text("object name"));
            icons::named(&r, egui::WidgetType::TextEdit, "Selected Object");
            let typed = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if zoom_obj || typed {
                // The typed object (or the viewport selection) is selected and shown.
                let found: Vec<String> = if st.find.is_empty() {
                    app.state.selection.iter().map(|n| schematic::obj_key(*n)).collect()
                } else {
                    app.state.scene.nodes().filter(|n| n.name.eq_ignore_ascii_case(st.find.trim())).map(|n| schematic::obj_key(n.id)).collect()
                };
                if !found.is_empty() {
                    acts.push(Act::Select(found, SelectOp::Replace));
                    st.pending = Some(ViewOp::ZoomExtentsSelected);
                }
            }
            ui.separator();
            let prompt = match (st.tool, sel.len()) {
                (Tool::Connect, _) => "Drag from a child to its parent".to_owned(),
                (_, 0) => "Click or drag to select".to_owned(),
                (_, 1) => "1 entity selected".to_owned(),
                (_, n) => format!("{n} entities selected"),
            };
            ui.label(prompt);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if Icon::new(icons::PAN_TO_SELECTED, "Pan to Selected").show(ui, &app.state).clicked() {
                    st.pending = Some(ViewOp::PanToSelected);
                }
                if Icon::new(icons::ZOOM_SELECTED, "Zoom Extents Selected").show(ui, &app.state).clicked() {
                    st.pending = Some(ViewOp::ZoomExtentsSelected);
                }
                if Icon::new(icons::ZOOM_EXTENTS, "Zoom Extents").show(ui, &app.state).clicked() {
                    st.pending = Some(ViewOp::ZoomExtents);
                }
            });
        });
    });
    egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| canvas(ui, app, i, v, all, ents, wires, pos, sel, st, acts));
}

#[allow(clippy::too_many_arguments)]
fn canvas(ui: &mut egui::Ui, app: &mut LoomApp, i: usize, v: &SchematicView, all: &[Entity], ents: &[Entity], wires: &[schematic::Wire], pos: &BTreeMap<String, [f32; 2]>, sel: &BTreeSet<String>, st: &mut SvUi, acts: &mut Vec<Act>) {
    let rect = ui.available_rect_before_wrap();
    let bg = ui.interact(rect, ui.id().with("schematic_canvas"), egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, egui::Color32::from_gray(if ui.visuals().dark_mode { 52 } else { 170 }));
    let (pan, zoom) = (egui::vec2(v.pan[0], v.pan[1]), v.zoom.max(0.05));
    // Boxes are as wide as their labels.
    let widths: BTreeMap<&str, f32> = all.iter().map(|e| (e.key.as_str(), schematic::box_width(&e.label))).collect();
    let width_of = |k: &str| widths.get(k).copied().unwrap_or(schematic::BOX_W);
    // Pending zoom / pan from the menus and buttons.
    if let Some(op) = st.pending.take() {
        let keys: Vec<&String> = match op {
            ViewOp::ZoomExtents => pos.keys().collect(),
            _ => pos.keys().filter(|k| sel.contains(*k)).collect(),
        };
        let pts: Vec<egui::Rect> = keys.iter().filter_map(|k| pos.get(*k).map(|p| (k.as_str(), p))).map(|(k, p)| egui::Rect::from_min_size(egui::pos2(p[0], p[1]), egui::vec2(width_of(k), schematic::BOX_H))).collect();
        if let Some(b) = pts.iter().copied().reduce(|a, b| a.union(b)) {
            let b = b.expand(20.0);
            let vs = &mut app.state.schematics.list[i];
            if op == ViewOp::PanToSelected {
                let c = b.center().to_vec2() - rect.size() / (2.0 * zoom);
                vs.pan = [c.x, c.y];
            } else {
                let z = (rect.width() / b.width()).min(rect.height() / b.height()).clamp(0.1, 2.0);
                vs.zoom = z;
                let c = b.center().to_vec2() - rect.size() / (2.0 * z);
                vs.pan = [c.x, c.y];
            }
        }
        return;
    }
    let to_screen = |p: egui::Pos2| rect.min + (p.to_vec2() - pan) * zoom;
    let to_graph = |p: egui::Pos2| ((p - rect.min) / zoom + pan).to_pos2();
    if v.show_grid {
        let step = 20.0 * zoom;
        if step > 4.0 {
            let c = egui::Color32::from_gray(if ui.visuals().dark_mode { 60 } else { 160 });
            let off = egui::vec2((-pan.x * zoom).rem_euclid(step), (-pan.y * zoom).rem_euclid(step));
            let mut x = rect.left() + off.x;
            while x < rect.right() {
                painter.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], egui::Stroke::new(1.0_f32, c));
                x += step;
            }
            let mut y = rect.top() + off.y;
            while y < rect.bottom() {
                painter.line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)], egui::Stroke::new(1.0_f32, c));
                y += step;
            }
        }
    }
    let box_of = |k: &str| pos.get(k).map(|p| egui::Rect::from_min_size(to_screen(egui::pos2(p[0], p[1])), egui::vec2(width_of(k), schematic::BOX_H) * zoom));
    // Wires under the boxes.
    let wire_c = egui::Color32::from_gray(if ui.visuals().dark_mode { 190 } else { 40 });
    for w in wires {
        let (Some(a), Some(b)) = (box_of(&w.from), box_of(&w.to)) else { continue };
        match w.kind {
            WireKind::Shared => {
                painter.extend(egui::Shape::dashed_line(&[a.center(), b.center()], egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(120, 170, 230)), 5.0, 4.0));
            }
            _ => {
                let start = egui::pos2(a.left() + 8.0 * zoom, a.bottom());
                let end = b.left_center();
                if end.x > start.x {
                    painter.add(egui::Shape::line(vec![start, egui::pos2(start.x, end.y), end], egui::Stroke::new(1.0_f32, wire_c)));
                } else {
                    painter.line_segment([a.center_bottom(), b.center_top()], egui::Stroke::new(1.0_f32, wire_c));
                }
                painter.arrow(end - egui::vec2(6.0 * zoom, 0.0), egui::vec2(5.0 * zoom, 0.0), egui::Stroke::new(1.0_f32, wire_c));
            }
        }
    }
    let has_children: BTreeSet<&str> = all.iter().filter_map(|e| e.owner.as_deref()).collect();
    let viewport_sel: BTreeSet<String> = app.state.selection.iter().map(|n| schematic::obj_key(*n)).collect();
    let mods = ui.input(|i| i.modifiers);
    let mut hit_entity = false;
    let mut boxes: Vec<(String, egui::Rect)> = Vec::new();
    for e in ents {
        let Some(r) = box_of(&e.key) else { continue };
        boxes.push((e.key.clone(), r));
        let resp = ui.interact(r, ui.id().with(("entity", &e.key)), egui::Sense::click_and_drag());
        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, sel.contains(&e.key), &e.label));
        let selected = sel.contains(&e.key);
        let base = fill(e, app);
        painter.rect_filled(r, 3.0 * zoom, if selected { egui::Color32::WHITE } else { base });
        // White border: selected in the viewports; red: animated.
        let border = if e.animated {
            egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(230, 50, 40))
        } else if viewport_sel.contains(&e.key) {
            egui::Stroke::new(2.0_f32, egui::Color32::WHITE)
        } else {
            egui::Stroke::new(1.0_f32, egui::Color32::from_gray(30))
        };
        painter.rect_stroke(r, 3.0 * zoom, border, egui::StrokeKind::Inside);
        // The left end: solid when arranged, open (a hole) when free.
        let end = egui::Rect::from_min_size(r.min, egui::vec2(6.0 * zoom, r.height()));
        if v.free.contains_key(&e.key) && !v.always_arrange {
            painter.circle_stroke(end.center(), 2.0 * zoom, egui::Stroke::new(1.0_f32, egui::Color32::BLACK));
        } else {
            painter.rect_filled(end.shrink(1.0), 1.0, egui::Color32::from_gray(40));
        }
        if zoom > 0.35 {
            let font = egui::FontId::proportional(12.0 * zoom.min(1.5));
            let tc = egui::Color32::BLACK;
            let at = r.left_center() + egui::vec2(9.0 * zoom, 0.0);
            let clip = painter.with_clip_rect(r.intersect(rect));
            clip.text(at, egui::Align2::LEFT_CENTER, &e.label, font.clone(), tc);
            if e.instanced {
                // Instances read bold.
                clip.text(at + egui::vec2(0.6, 0.0), egui::Align2::LEFT_CENTER, &e.label, font, tc);
            }
        }
        // The arrow under a box that has children: collapse / expand them.
        if has_children.contains(e.key.as_str()) {
            let collapsed = v.collapsed.contains(&e.key);
            let ar = egui::Rect::from_center_size(egui::pos2(r.left() + 8.0 * zoom, r.bottom() + 5.0 * zoom), egui::vec2(10.0, 10.0) * zoom.max(0.6));
            let a = ui.interact(ar, ui.id().with(("arrow", &e.key)), egui::Sense::click());
            icons::named(&a, egui::WidgetType::Button, if collapsed { "Expand" } else { "Collapse" });
            painter.text(ar.center(), egui::Align2::CENTER_CENTER, if collapsed { icons::DOWN } else { icons::UP }, egui::FontId::proportional(10.0 * zoom.max(0.6)), wire_c);
            if a.clicked() {
                acts.push(Act::Collapse(e.key.clone(), !collapsed));
            }
        }
        if resp.hovered() || resp.dragged() {
            hit_entity = true;
        }
        if resp.clicked() {
            let op = if mods.alt {
                SelectOp::Subtract
            } else if mods.ctrl || mods.command {
                if selected { SelectOp::Subtract } else { SelectOp::Add }
            } else {
                SelectOp::Replace
            };
            acts.push(Act::Select(vec![e.key.clone()], op));
        }
        if resp.secondary_clicked() && !selected {
            acts.push(Act::Select(vec![e.key.clone()], SelectOp::Replace));
        }
        if resp.drag_started_by(egui::PointerButton::Primary) {
            match st.tool {
                Tool::Connect => st.connect_from = Some(e.key.clone()),
                Tool::Select if !v.always_arrange => {
                    // The dragged box and, when it is selected, the whole selection move.
                    let mut keys: BTreeSet<String> = if selected { sel.iter().filter(|k| pos.contains_key(*k)).cloned().collect() } else { [e.key.clone()].into_iter().collect() };
                    if !selected {
                        acts.push(Act::Select(vec![e.key.clone()], SelectOp::Replace));
                    }
                    if v.move_children {
                        let kids = schematic::descendants(ents, &keys);
                        keys.extend(kids);
                    }
                    let vs = &mut app.state.schematics.list[i];
                    for k in &keys {
                        if let Some(p) = pos.get(k) {
                            vs.free.insert(k.clone(), *p);
                        }
                    }
                    st.moving = keys.into_iter().collect();
                }
                Tool::Select => {}
            }
        }
        if resp.dragged_by(egui::PointerButton::Primary) && !st.moving.is_empty() {
            let d = resp.drag_delta() / zoom;
            let vs = &mut app.state.schematics.list[i];
            for k in &st.moving {
                if let Some(p) = vs.free.get_mut(k) {
                    p[0] += d.x;
                    p[1] += d.y;
                }
            }
        }
        if resp.drag_stopped() {
            st.moving.clear();
            if let Some(from) = st.connect_from.take() {
                let at = ui.ctx().pointer_latest_pos();
                let to = at.and_then(|p| boxes_hit(ents, &pos.keys().filter_map(|k| box_of(k).map(|r| (k.clone(), r))).collect(), p)).filter(|k| *k != from);
                if let Some(to) = to {
                    acts.push(Act::Cmd(Command::SchematicConnect { from, to }));
                }
            }
        }
        resp.context_menu(|ui| {
            ui.menu_button("Select", |ui| select_entries(ui, v, ents, sel, acts));
            ui.menu_button("Edit", |ui| edit_entries(ui, v, st, acts));
            ui.menu_button("Layout", |ui| layout_entries(ui, v, ents, sel, acts));
            ui.menu_button("Options", |ui| mode_entries(ui, v, acts));
            if check(ui, "Display Floater", st.floater) {
                st.floater = !st.floater;
            }
        });
    }
    // The Connect line follows the pointer.
    if let (Some(from), Some(p)) = (st.connect_from.as_ref(), ui.ctx().pointer_latest_pos()) {
        if let Some(a) = box_of(from) {
            painter.extend(egui::Shape::dashed_line(&[a.center(), p], egui::Stroke::new(1.5_f32, egui::Color32::YELLOW), 4.0, 3.0));
        }
    }
    // Empty space: click clears, drag selects by region, middle drag pans, wheel zooms.
    if bg.clicked() && !hit_entity {
        acts.push(Act::Select(Vec::new(), SelectOp::Replace));
    }
    if bg.drag_started_by(egui::PointerButton::Primary) && !hit_entity {
        st.region = bg.interact_pointer_pos();
    }
    if let (Some(a), Some(b)) = (st.region, ui.ctx().pointer_latest_pos()) {
        let r = egui::Rect::from_two_pos(a, b);
        painter.rect_stroke(r, 0.0, egui::Stroke::new(1.0_f32, egui::Color32::WHITE), egui::StrokeKind::Middle);
        if bg.drag_stopped() {
            st.region = None;
            let keys: Vec<String> = boxes.iter().filter(|(_, br)| br.intersects(r)).map(|(k, _)| k.clone()).collect();
            let op = if mods.ctrl || mods.command { SelectOp::Add } else if mods.alt { SelectOp::Subtract } else { SelectOp::Replace };
            acts.push(Act::Select(keys, op));
        }
    }
    if bg.dragged_by(egui::PointerButton::Middle) {
        let d = bg.drag_delta() / zoom;
        let vs = &mut app.state.schematics.list[i];
        vs.pan = [vs.pan[0] - d.x, vs.pan[1] - d.y];
    }
    if bg.hovered() {
        let scroll = ui.input(|i| i.raw_scroll_delta.y);
        if scroll != 0.0 {
            if let Some(p) = bg.hover_pos() {
                let g = to_graph(p);
                let z = (zoom * (1.0 + scroll * 0.0015)).clamp(0.1, 4.0);
                let vs = &mut app.state.schematics.list[i];
                vs.zoom = z;
                let np = g.to_vec2() - (p - rect.min) / z;
                vs.pan = [np.x, np.y];
            }
        }
    }
}

/// The topmost box under `p`.
fn boxes_hit(ents: &[Entity], rects: &BTreeMap<String, egui::Rect>, p: egui::Pos2) -> Option<String> {
    ents.iter().rev().find(|e| rects.get(&e.key).is_some_and(|r| r.contains(p))).map(|e| e.key.clone())
}

/// The Display floater: which entities the view shows.
fn display_floater(ctx: &egui::Context, v: &SchematicView, st: &mut SvUi, acts: &mut Vec<Act>) {
    let mut open = true;
    egui::Window::new("Display").id(window_id(v.id).with("floater")).open(&mut open).collapsible(false).resizable(false).default_pos([880.0, 180.0]).show(ctx, |ui| {
        ui.label(egui::RichText::new("Entities").strong());
        for (label, on) in [("Base Objects", v.base_objects), ("Modifier Stack", v.modifiers), ("Materials", v.materials), ("Controllers", v.controllers)] {
            let mut b = on;
            if ui.toggle_value(&mut b, label).clicked() {
                acts.push(opt(v, label, b));
            }
        }
    });
    if !open {
        st.floater = false;
    }
}
