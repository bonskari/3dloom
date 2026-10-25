//! Scene Explorer windows (Tools > New Scene Explorer): a modeless table of
//! the scene with a row per object and a column per property.
//!
//! As in the reference: a click highlights a row (Ctrl toggles, Shift a
//! range); with Sync Selection the highlight is the scene selection. Clicking
//! a column heading sorts by it (again: reversed), right-clicking it offers
//! Sort Ascending / Descending, Configure Columns, Remove and Replace Column.
//! Clicking an on/off cell sets it on every highlighted row. Dragging an
//! object's icon onto another object links it as a child, onto Scene Root
//! unlinks it. F2 or a click on the active (yellow) row edits the name; the
//! other highlighted rows take numbered versions of it.

use crate::icons::{self, Icon};
use crate::LoomApp;
use eframe::egui;
use loom_commands::explorer::{self, Category, Cell, Column, SceneExplorer, SearchTerm};
use loom_commands::{Command, SelectOp};
use std::collections::BTreeSet;

/// The Scene Root row (the drop target that unlinks).
pub const SCENE_ROOT: u64 = u64::MAX;
const ROW_H: f32 = 18.0;
const NAME_W: f32 = 200.0;
const INDENT: f32 = 14.0;

/// The width of a column.
pub fn column_width(c: Column) -> f32 {
    match c {
        Column::Name => NAME_W,
        Column::Type => 70.0,
        Column::Color => 44.0,
        Column::Faces => 56.0,
        Column::Material | Column::Layer => 100.0,
        c => (c.label().len() as f32 * 7.0).max(56.0),
    }
}

/// The window id of the explorer with stable id `uid`.
pub fn window_id(uid: u32) -> egui::Id {
    egui::Id::new(("scene_explorer", uid))
}

/// The row icon of a type.
pub fn category_glyph(c: Category) -> &'static str {
    match c {
        Category::Geometry => icons::KIND_GEOMETRY,
        Category::Shape => icons::KIND_SHAPE,
        Category::Light => icons::LIGHTS,
        Category::Camera => icons::KIND_CAMERA,
        Category::Helper => icons::KIND_HELPER,
        Category::Bone => icons::KIND_BONE,
        Category::Group => icons::KIND_GROUP,
    }
}

/// Per-window interaction state (not saved).
#[derive(Clone, Default)]
struct ExUi {
    find: String,
    anchor: Option<u64>,
    /// The last row clicked explicitly (yellow).
    active: Option<u64>,
    /// The row whose name is being edited, and the text.
    rename: Option<(u64, String)>,
    rename_focus: bool,
    rename_request: bool,
    /// The row whose icon is being dragged.
    drag: Option<u64>,
    columns_open: bool,
    search_open: bool,
    search: SearchDraft,
}

#[derive(Clone)]
struct SearchDraft {
    property: usize,
    condition: usize,
    value: String,
    terms: Vec<SearchTerm>,
    picked: Option<usize>,
    match_all: bool,
}

impl Default for SearchDraft {
    fn default() -> Self {
        SearchDraft { property: 0, condition: 0, value: String::new(), terms: Vec::new(), picked: None, match_all: true }
    }
}

fn ui_id(uid: u32) -> egui::Id {
    window_id(uid).with("ui")
}

/// What a frame's clicks ask for, applied after the window is drawn.
enum Act {
    Cmd(Command),
    /// The new highlight (the scene selection with Sync Selection).
    Highlight(BTreeSet<u64>),
    /// Link the nodes to a parent (None: Scene Root) in one undo step.
    Link(Vec<u64>, Option<u64>),
    Collapse(u64, bool),
    SetCollapsed(BTreeSet<u64>),
    /// Become the most recently used explorer.
    Touch,
}

/// Keys the explorer under the pointer takes before the shortcuts see them
/// (F2 renames there instead of its Main UI action).
pub fn pre_hotkeys(ctx: &egui::Context, app: &mut LoomApp) {
    if ctx.wants_keyboard_input() {
        return;
    }
    let Some(p) = ctx.input(|i| i.pointer.hover_pos()) else { return };
    for e in app.state.explorers.list.iter().filter(|e| e.open) {
        let over = ctx.memory(|m| m.area_rect(window_id(e.id))).is_some_and(|r| r.contains(p));
        if over && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F2)) {
            let id = ui_id(e.id);
            let mut st: ExUi = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
            st.rename_request = true;
            ctx.data_mut(|d| d.insert_temp(id, st));
        }
    }
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp) {
    let open: Vec<u32> = app.state.explorers.list.iter().filter(|e| e.open).map(|e| e.id).collect();
    for uid in open {
        window(ctx, app, uid);
    }
}

fn window(ctx: &egui::Context, app: &mut LoomApp, uid: u32) {
    let Some(i) = app.state.explorers.list.iter().position(|e| e.id == uid) else { return };
    let ex = app.state.explorers.list[i].clone();
    let mut st: ExUi = ctx.data(|d| d.get_temp(ui_id(uid))).unwrap_or_default();
    let rows = explorer::rows(&app.state, &ex);
    let hl: BTreeSet<u64> = explorer::highlighted(&app.state, &ex).iter().map(|n| n.0).collect();
    let mut acts: Vec<Act> = Vec::new();
    let mut open = true;
    let offset = (uid % 6) as f32 * 24.0;
    let resp = egui::Window::new(ex.name.clone())
        .id(window_id(uid))
        .open(&mut open)
        .default_size([560.0, 380.0])
        .default_pos([180.0 + offset, 120.0 + offset])
        .resizable(true)
        .show(ctx, |ui| body(ui, app, &ex, &rows, &hl, &mut st, &mut acts));
    if let Some(r) = resp {
        if ctx.input(|inp| inp.pointer.any_pressed() && inp.pointer.press_origin().is_some_and(|p| r.response.rect.contains(p))) {
            acts.push(Act::Touch);
        }
    }
    if st.columns_open {
        configure_columns(ctx, app, &ex, &mut st, &mut acts);
    }
    if st.search_open {
        advanced_search(ctx, app, &ex, &mut st);
    }
    ctx.data_mut(|d| d.insert_temp(ui_id(uid), st));
    for a in acts {
        apply(app, uid, a);
    }
    if !open {
        let _ = app.dispatch(Command::CloseSceneExplorer { name: Some(ex.name.clone()) });
    }
}

fn apply(app: &mut LoomApp, uid: u32, a: Act) {
    let Some(i) = app.state.explorers.list.iter().position(|e| e.id == uid) else { return };
    match a {
        Act::Cmd(c) => {
            if let Err(e) = app.dispatch(c) {
                app.state.status_prompt = e;
            }
        }
        Act::Highlight(ids) => {
            if app.state.explorers.list[i].sync_selection {
                let _ = app.dispatch(Command::SelectNodes { ids: ids.into_iter().collect(), op: SelectOp::Replace });
            } else {
                app.state.explorers.list[i].highlighted = ids;
            }
        }
        Act::Link(ids, parent) => {
            // One undo step for the whole drag.
            app.state.undo.begin("Link");
            for id in ids {
                if let Err(e) = app.dispatch(Command::SetParent { id, parent }) {
                    app.state.status_prompt = e;
                }
            }
            app.state.undo.end();
        }
        Act::Collapse(id, on) => {
            let c = &mut app.state.explorers.list[i].collapsed;
            if on {
                c.insert(id);
            } else {
                c.remove(&id);
            }
        }
        Act::SetCollapsed(set) => app.state.explorers.list[i].collapsed = set,
        Act::Touch => app.state.explorers.last = Some(app.state.explorers.list[i].name.clone()),
    }
}

fn opt(ex: &SceneExplorer, option: &str, on: bool) -> Act {
    Act::Cmd(Command::SceneExplorerOption { explorer: Some(ex.name.clone()), option: option.into(), on })
}

/// A menu entry with a check mark when on.
fn check(ui: &mut egui::Ui, label: &str, on: bool) -> bool {
    let text = if on { format!("{}  {label}", icons::CHECKED) } else { format!("      {label}") };
    let r = ui.button(text);
    r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, label));
    let clicked = r.clicked();
    if clicked {
        ui.close_menu();
    }
    clicked
}

fn item(ui: &mut egui::Ui, label: &str) -> bool {
    let clicked = ui.button(label).clicked();
    if clicked {
        ui.close_menu();
    }
    clicked
}

/// The descendants of `id` among the rows' nodes (Select Children).
fn with_children(app: &LoomApp, ids: impl IntoIterator<Item = u64>) -> BTreeSet<u64> {
    let mut out = BTreeSet::new();
    for id in ids {
        out.insert(id);
        out.extend(app.state.scene.descendants(loom_scene::NodeId(id)).into_iter().map(|n| n.0));
    }
    out
}

fn body(ui: &mut egui::Ui, app: &LoomApp, ex: &SceneExplorer, rows: &[explorer::Row], hl: &BTreeSet<u64>, st: &mut ExUi, acts: &mut Vec<Act>) {
    let all: BTreeSet<u64> = rows.iter().map(|r| r.id.0).collect();
    let listed_all: BTreeSet<u64> = app.state.scene.nodes().filter(|n| explorer::listed(&app.state, ex, n)).map(|n| n.id.0).collect();
    let tree_ids: BTreeSet<u64> = app.state.scene.nodes().filter(|n| explorer::listed(&app.state, ex, n) && !app.state.scene.children(n.id).is_empty()).map(|n| n.id.0).collect();
    egui::menu::bar(ui, |ui| {
        ui.menu_button("Select", |ui| {
            if item(ui, "Select All") {
                acts.push(Act::Highlight(listed_all.clone()));
            }
            if item(ui, "Select None") {
                acts.push(Act::Highlight(BTreeSet::new()));
            }
            if item(ui, "Select Invert") {
                acts.push(Act::Highlight(listed_all.difference(hl).copied().collect()));
            }
            ui.separator();
            if check(ui, "Select Children", ex.select_children) {
                acts.push(opt(ex, "Select Children", !ex.select_children));
            }
            ui.separator();
            if check(ui, "Find Case Sensitive", ex.find_case_sensitive) {
                acts.push(opt(ex, "Find Case Sensitive", !ex.find_case_sensitive));
            }
            if check(ui, "Find Using Wildcards", ex.find_wildcards) {
                acts.push(opt(ex, "Find Using Wildcards", !ex.find_wildcards));
            }
            if check(ui, "Find Using Regular Expressions", ex.find_regex) {
                acts.push(opt(ex, "Find Using Regular Expressions", !ex.find_regex));
            }
            ui.separator();
            if check(ui, "Sync Selection", ex.sync_selection) {
                acts.push(opt(ex, "Sync Selection", !ex.sync_selection));
            }
            ui.separator();
            if item(ui, "Search...") {
                st.search_open = true;
            }
        });
        ui.menu_button("Display", |ui| {
            ui.menu_button("Object Types", |ui| {
                for c in Category::ALL {
                    let on = ex.shown.contains(&c);
                    if check(ui, c.filter_label(), on) {
                        acts.push(opt(ex, c.filter_label(), !on));
                    }
                }
                if check(ui, "Frozen", ex.show_frozen) {
                    acts.push(opt(ex, "Frozen", !ex.show_frozen));
                }
                if check(ui, "Hidden", ex.show_hidden) {
                    acts.push(opt(ex, "Hidden", !ex.show_hidden));
                }
                ui.separator();
                for o in ["Display All", "Display None", "Invert Display"] {
                    if item(ui, o) {
                        acts.push(opt(ex, o, true));
                    }
                }
            });
            ui.separator();
            if check(ui, "Display Children", ex.display_children) {
                acts.push(opt(ex, "Display Children", !ex.display_children));
            }
            ui.separator();
            if item(ui, "Collapse All") {
                acts.push(Act::SetCollapsed(tree_ids.clone()));
            }
            if item(ui, "Collapse Selected") {
                acts.push(Act::SetCollapsed(ex.collapsed.union(&tree_ids.intersection(hl).copied().collect()).copied().collect()));
            }
            if item(ui, "Expand All") {
                acts.push(Act::SetCollapsed(BTreeSet::new()));
            }
            if item(ui, "Expand Selected") {
                acts.push(Act::SetCollapsed(ex.collapsed.difference(hl).copied().collect()));
            }
        });
        ui.menu_button("Customize", |ui| {
            if item(ui, "Configure Columns...") {
                st.columns_open = true;
            }
            if check(ui, "Lock Cell Editing", ex.lock_cell_editing) {
                acts.push(opt(ex, "Lock Cell Editing", !ex.lock_cell_editing));
            }
        });
    });
    // Upper toolbar: Find and View.
    ui.horizontal(|ui| {
        ui.label("Find:");
        let r = ui.add(egui::TextEdit::singleline(&mut st.find).desired_width(160.0));
        icons::named(&r, egui::WidgetType::TextEdit, "Find");
        if r.changed() {
            // Highlights the matches as you type.
            let hits: BTreeSet<u64> = rows.iter().filter(|row| app.state.scene.node(row.id).is_some_and(|n| explorer::find_matches(ex, &st.find, &n.name))).map(|r| r.id.0).collect();
            if !st.find.is_empty() {
                acts.push(Act::Highlight(hits));
            }
        }
        ui.label("View:");
        egui::ComboBox::from_id_salt(("explorer_view", ex.id)).selected_text(&ex.name).show_ui(ui, |ui| {
            for e in &app.state.explorers.list {
                if ui.selectable_label(e.name == ex.name, &e.name).clicked() && e.name != ex.name {
                    acts.push(Act::Cmd(Command::OpenSceneExplorer { name: Some(e.name.clone()) }));
                }
            }
        });
    });
    ui.separator();
    // Lower toolbar first so the table takes the rest.
    egui::TopBottomPanel::bottom(egui::Id::new(("explorer_lower", ex.id))).show_inside(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("Selection Set:");
            egui::ComboBox::from_id_salt(("explorer_sets", ex.id)).selected_text("").width(110.0).show_ui(ui, |ui| {
                for (name, ids) in &app.state.named_sets {
                    if ui.selectable_label(false, name).clicked() {
                        acts.push(Act::Highlight(ids.iter().map(|n| n.0).collect()));
                    }
                }
            });
            if Icon::new(icons::SELECT_ALL, "Select All").show(ui, &app.state).clicked() {
                acts.push(Act::Highlight(listed_all.clone()));
            }
            if Icon::new(icons::SELECT_NONE, "Select None").show(ui, &app.state).clicked() {
                acts.push(Act::Highlight(BTreeSet::new()));
            }
            if Icon::new(icons::SELECT_INVERT, "Select Invert").show(ui, &app.state).clicked() {
                acts.push(Act::Highlight(listed_all.difference(hl).copied().collect()));
            }
            if Icon::new(icons::SYNC_SELECTION, "Sync Selection").on(ex.sync_selection).show(ui, &app.state).clicked() {
                acts.push(opt(ex, "Sync Selection", !ex.sync_selection));
            }
        });
    });
    // Left-hand (Display) toolbar.
    egui::SidePanel::left(egui::Id::new(("explorer_display", ex.id))).resizable(false).exact_width(30.0).show_inside(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        for c in Category::ALL {
            let on = ex.shown.contains(&c);
            let tip = format!("Display {}", c.filter_label());
            if Icon::new(category_glyph(c), &tip).on(on).show(ui, &app.state).clicked() {
                acts.push(opt(ex, c.filter_label(), !on));
            }
        }
        if Icon::new(icons::FREEZE, "Display Frozen Objects").on(ex.show_frozen).show(ui, &app.state).clicked() {
            acts.push(opt(ex, "Frozen", !ex.show_frozen));
        }
        if Icon::new(icons::HIDE, "Display Hidden Objects").on(ex.show_hidden).show(ui, &app.state).clicked() {
            acts.push(opt(ex, "Hidden", !ex.show_hidden));
        }
        ui.separator();
        if Icon::new(icons::DISPLAY_ALL, "Display All").show(ui, &app.state).clicked() {
            acts.push(opt(ex, "Display All", true));
        }
        if Icon::new(icons::DISPLAY_NONE, "Display None").show(ui, &app.state).clicked() {
            acts.push(opt(ex, "Display None", true));
        }
        if Icon::new(icons::INVERT_DISPLAY, "Invert Display").show(ui, &app.state).clicked() {
            acts.push(opt(ex, "Invert Display", true));
        }
        ui.separator();
        if Icon::new(icons::LOCK_CELL_EDITING, "Lock Cell Editing").on(ex.lock_cell_editing).show(ui, &app.state).clicked() {
            acts.push(opt(ex, "Lock Cell Editing", !ex.lock_cell_editing));
        }
    });
    egui::CentralPanel::default().show_inside(ui, |ui| table(ui, app, ex, rows, hl, &all, st, acts));
}

/// The column headings and the rows.
fn table(ui: &mut egui::Ui, app: &LoomApp, ex: &SceneExplorer, rows: &[explorer::Row], hl: &BTreeSet<u64>, all: &BTreeSet<u64>, st: &mut ExUi, acts: &mut Vec<Act>) {
    let widths: Vec<f32> = ex.columns.iter().map(|c| column_width(*c)).collect();
    let total: f32 = widths.iter().sum::<f32>().max(ui.available_width());
    let order: Vec<u64> = rows.iter().map(|r| r.id.0).collect();
    let text_color = ui.visuals().text_color();
    let weak = ui.visuals().weak_text_color();
    egui::ScrollArea::both().auto_shrink([false, false]).drag_to_scroll(false).show(ui, |ui| {
        // Headings.
        let (hrect, _) = ui.allocate_exact_size(egui::vec2(total, ROW_H + 2.0), egui::Sense::hover());
        let mut x = hrect.left();
        for (ci, col) in ex.columns.iter().enumerate() {
            let r = egui::Rect::from_min_size(egui::pos2(x, hrect.top()), egui::vec2(widths[ci], hrect.height()));
            x += widths[ci];
            let resp = ui.interact(r, ui.id().with(("heading", ci)), egui::Sense::click());
            icons::named(&resp, egui::WidgetType::Button, col.label());
            let fill = if resp.hovered() { ui.visuals().widgets.hovered.bg_fill } else { ui.visuals().widgets.inactive.bg_fill };
            ui.painter().rect_filled(r.shrink(0.5), 0.0, fill);
            ui.painter().text(r.left_center() + egui::vec2(4.0, 0.0), egui::Align2::LEFT_CENTER, col.label(), egui::FontId::proportional(12.0), text_color);
            if let Some((_, asc)) = ex.sort.iter().find(|(c, _)| c == col) {
                ui.painter().text(r.right_center() - egui::vec2(4.0, 0.0), egui::Align2::RIGHT_CENTER, if *asc { icons::SORT_ASCENDING } else { icons::SORT_DESCENDING }, egui::FontId::proportional(11.0), weak);
            }
            if resp.clicked() {
                // A click sorts by the column; a second click reverses it.
                let asc = !matches!(ex.sort.first(), Some((c, true)) if c == col);
                acts.push(Act::Cmd(Command::SceneExplorerSort { explorer: Some(ex.name.clone()), column: col.label().into(), ascending: asc, add: None }));
            }
            resp.context_menu(|ui| {
                for (label, asc) in [("Sort Ascending", true), ("Sort Descending", false)] {
                    if item(ui, label) {
                        acts.push(Act::Cmd(Command::SceneExplorerSort { explorer: Some(ex.name.clone()), column: col.label().into(), ascending: asc, add: Some(true) }));
                    }
                }
                ui.separator();
                if item(ui, "Configure Columns...") {
                    st.columns_open = true;
                }
                let names: Vec<String> = ex.columns.iter().map(|c| c.label().to_owned()).collect();
                let set = |cols: Vec<String>| Act::Cmd(Command::SceneExplorerColumns { explorer: Some(ex.name.clone()), columns: cols });
                if ex.columns.len() > 1 && item(ui, "Remove Column") {
                    acts.push(set(names.iter().enumerate().filter(|(j, _)| *j != ci).map(|(_, n)| n.clone()).collect()));
                }
                ui.menu_button("Replace Column", |ui| {
                    for c in Column::ALL.into_iter().filter(|c| !ex.columns.contains(c)) {
                        if item(ui, c.label()) {
                            let mut v = names.clone();
                            v[ci] = c.label().into();
                            acts.push(set(v));
                        }
                    }
                });
                if ci > 0 && item(ui, "Move Left") {
                    let mut v = names.clone();
                    v.swap(ci, ci - 1);
                    acts.push(set(v));
                }
                if ci + 1 < names.len() && item(ui, "Move Right") {
                    let mut v = names.clone();
                    v.swap(ci, ci + 1);
                    acts.push(set(v));
                }
            });
        }
        // Rows: Scene Root first (the drop target that unlinks), then the objects.
        let mut row_rects: Vec<(u64, egui::Rect)> = Vec::new();
        if ex.display_children {
            let (r, _) = ui.allocate_exact_size(egui::vec2(total, ROW_H), egui::Sense::hover());
            ui.painter().text(r.left_center() + egui::vec2(4.0, 0.0), egui::Align2::LEFT_CENTER, "Scene Root", egui::FontId::proportional(13.0), weak);
            row_rects.push((SCENE_ROOT, r));
        }
        let mut drag_stopped: Option<u64> = None;
        let mut rename_commit: Option<(u64, String)> = None;
        if st.rename_request {
            st.rename_request = false;
            if let Some(a) = st.active.filter(|a| all.contains(a) && !ex.lock_cell_editing) {
                if let Some(n) = app.state.scene.node(loom_scene::NodeId(a)) {
                    st.rename = Some((a, n.name.clone()));
                    st.rename_focus = true;
                }
            }
        }
        for row in rows {
            let id = row.id.0;
            let Some(n) = app.state.scene.node(row.id) else { continue };
            let (rect, row_resp) = ui.allocate_exact_size(egui::vec2(total, ROW_H), egui::Sense::click());
            row_rects.push((id, rect));
            let on = hl.contains(&id);
            if on {
                // The last explicitly clicked row is yellow, the others blue.
                let fill = if st.active == Some(id) { egui::Color32::from_rgb(214, 190, 80) } else { ui.visuals().selection.bg_fill };
                ui.painter().rect_filled(rect, 0.0, fill);
            }
            let fg = if on && st.active == Some(id) { egui::Color32::BLACK } else if explorer::is_frozen(&app.state, n) { weak } else { text_color };
            let mut x = rect.left();
            let mut name_clicked = false;
            for (ci, col) in ex.columns.iter().enumerate() {
                let cr = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(widths[ci], ROW_H));
                x += widths[ci];
                if *col == Column::Name {
                    let mut cx = cr.left() + 2.0 + row.depth as f32 * INDENT;
                    if row.has_children {
                        let er = egui::Rect::from_min_size(egui::pos2(cx, cr.top() + 2.0), egui::vec2(14.0, 14.0));
                        let collapsed = ex.collapsed.contains(&id);
                        let e = ui.interact(er, ui.id().with(("expander", id)), egui::Sense::click());
                        icons::named(&e, egui::WidgetType::Button, if collapsed { "Expand" } else { "Collapse" });
                        ui.painter().text(er.center(), egui::Align2::CENTER_CENTER, if collapsed { icons::EXPAND } else { icons::COLLAPSE }, egui::FontId::proportional(12.0), fg);
                        if e.clicked() {
                            acts.push(Act::Collapse(id, !collapsed));
                        }
                    }
                    cx += 16.0;
                    // The type icon: the drag handle that links.
                    let ir = egui::Rect::from_min_size(egui::pos2(cx, cr.top() + 1.0), egui::vec2(16.0, 16.0));
                    let cat = explorer::category(&n.object);
                    let icon = ui.interact(ir, ui.id().with(("icon", id)), egui::Sense::click_and_drag());
                    icon.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("{} icon", n.name)));
                    ui.painter().text(ir.center(), egui::Align2::CENTER_CENTER, category_glyph(cat), egui::FontId::proportional(13.0), crate::command_panel::scene::IconKind::color(crate::command_panel::scene::icon_kind(&n.object)));
                    if icon.drag_started() {
                        st.drag = Some(id);
                    }
                    if icon.drag_stopped() {
                        drag_stopped = Some(id);
                    }
                    if icon.clicked() {
                        name_clicked = true;
                    }
                    cx += 18.0;
                    let tr = egui::Rect::from_min_max(egui::pos2(cx, cr.top()), cr.right_bottom());
                    if st.rename.as_ref().is_some_and(|(r, _)| *r == id) {
                        let (_, text) = st.rename.as_mut().expect("checked");
                        let te = ui.put(tr, egui::TextEdit::singleline(text).desired_width(tr.width()));
                        icons::named(&te, egui::WidgetType::TextEdit, "Rename");
                        if st.rename_focus {
                            // The whole name starts highlighted: typing replaces it.
                            te.request_focus();
                            if let Some(mut ts) = egui::TextEdit::load_state(ui.ctx(), te.id) {
                                let len = text.chars().count();
                                ts.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(len))));
                                ts.store(ui.ctx(), te.id);
                            }
                            st.rename_focus = false;
                        }
                        let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Tab), i.key_pressed(egui::Key::Escape)));
                        if esc {
                            st.rename = None;
                        } else if te.lost_focus() || enter {
                            rename_commit = st.rename.take();
                        }
                    } else {
                        let mut rt = egui::RichText::new(&n.name).size(13.0).color(fg);
                        if explorer::is_hidden(&app.state, n) {
                            rt = rt.italics();
                        }
                        if matches!(n.object, loom_scene::ObjectKind::Group { .. }) {
                            rt = egui::RichText::new(format!("[{}]", n.name)).size(13.0).color(fg);
                        }
                        let galley = egui::WidgetText::from(rt).into_galley(ui, Some(egui::TextWrapMode::Truncate), tr.width(), egui::TextStyle::Body);
                        let nr = ui.interact(tr, ui.id().with(("name", id)), egui::Sense::click());
                        icons::named(&nr, egui::WidgetType::Label, &n.name);
                        ui.painter().galley(egui::pos2(tr.left(), tr.center().y - galley.size().y / 2.0), galley, fg);
                        if nr.clicked() {
                            // A click on the active row's name edits it.
                            if st.active == Some(id) && on && !ex.lock_cell_editing && ui.input(|i| i.modifiers.is_none()) {
                                st.rename = Some((id, n.name.clone()));
                                st.rename_focus = true;
                            } else {
                                name_clicked = true;
                            }
                        }
                        if nr.secondary_clicked() && !on {
                            name_clicked = true;
                        }
                        nr.context_menu(|ui| row_menu(ui, app, ex, hl, id, st, acts));
                    }
                    continue;
                }
                let resp = ui.interact(cr, ui.id().with(("cell", id, ci)), egui::Sense::click());
                match explorer::cell(&app.state, n, *col) {
                    Cell::Bool(v) => {
                        let b = egui::Rect::from_center_size(cr.center(), egui::vec2(12.0, 12.0));
                        ui.painter().rect_stroke(b, 2.0, egui::Stroke::new(1.0_f32, fg), egui::StrokeKind::Inside);
                        if v {
                            ui.painter().text(b.center(), egui::Align2::CENTER_CENTER, icons::CHECKED, egui::FontId::proportional(11.0), fg);
                        }
                        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, v, format!("{} {}", n.name, col.label())));
                        // A click anywhere in the cell toggles it, on every highlighted row.
                        if resp.clicked() && !ex.lock_cell_editing {
                            let ids: Vec<u64> = if on { order.iter().copied().filter(|r| hl.contains(r)).collect() } else { vec![id] };
                            acts.push(Act::Cmd(Command::SetNodesProperty { ids, name: col.property().expect("on/off column").into(), value: !v }));
                        }
                    }
                    Cell::Color(c) => {
                        let sw = egui::Rect::from_center_size(cr.center(), egui::vec2(cr.width() - 12.0, 12.0));
                        ui.painter().rect_filled(sw, 1.0, egui::Color32::from_rgb(c[0], c[1], c[2]));
                        icons::named(&resp, egui::WidgetType::Button, &format!("{} Color", n.name));
                        if resp.clicked() && !ex.lock_cell_editing {
                            // The swatch opens the colour picker for the highlighted rows.
                            if !on {
                                acts.push(Act::Highlight([id].into_iter().collect()));
                            }
                            acts.push(Act::Cmd(Command::ObjectColorDialog));
                        }
                    }
                    Cell::Text(t) => {
                        ui.painter().text(cr.left_center() + egui::vec2(4.0, 0.0), egui::Align2::LEFT_CENTER, t, egui::FontId::proportional(12.0), fg);
                        name_clicked |= resp.clicked();
                    }
                    Cell::Number(v) => {
                        ui.painter().text(cr.right_center() - egui::vec2(6.0, 0.0), egui::Align2::RIGHT_CENTER, v.to_string(), egui::FontId::proportional(12.0), fg);
                        name_clicked |= resp.clicked();
                    }
                    Cell::Empty => name_clicked |= resp.clicked(),
                }
            }
            name_clicked |= row_resp.clicked();
            if name_clicked {
                let mods = ui.input(|i| i.modifiers);
                let mut next = hl.clone();
                if mods.shift {
                    let a = st.anchor.and_then(|a| order.iter().position(|x| *x == a));
                    let b = order.iter().position(|x| *x == id);
                    if let (Some(a), Some(b)) = (a, b) {
                        if !mods.ctrl {
                            next.clear();
                        }
                        next.extend(order[a.min(b)..=a.max(b)].iter().copied());
                    }
                } else if mods.ctrl || mods.command {
                    if !next.remove(&id) {
                        next.insert(id);
                    }
                    st.anchor = Some(id);
                } else {
                    next = [id].into_iter().collect();
                    st.anchor = Some(id);
                }
                if ex.select_children && next.contains(&id) {
                    next = next.union(&with_children(app, [id])).copied().collect();
                }
                st.active = Some(id);
                st.rename = None;
                acts.push(Act::Highlight(next));
            }
        }
        // While an icon is dragged: a yellow arrow at the row it would link to.
        let pointer = ui.ctx().pointer_latest_pos();
        let target = |p: egui::Pos2| row_rects.iter().find(|(_, r)| r.top() <= p.y && p.y < r.bottom() && p.x >= r.left() - 40.0 && p.x <= r.right() + 40.0).map(|(id, _)| *id);
        if let (Some(src), Some(p)) = (st.drag, pointer) {
            if let Some((_, r)) = target(p).and_then(|t| row_rects.iter().find(|(id, _)| *id == t && *id != src)) {
                let y = r.center().y;
                ui.painter().arrow(egui::pos2(r.left() + 2.0, y), egui::vec2(10.0, 0.0), egui::Stroke::new(2.5_f32, egui::Color32::from_rgb(240, 210, 40)));
            }
            if let Some(n) = app.state.scene.node(loom_scene::NodeId(src)) {
                ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, ui.id().with("drag_label"))).text(p + egui::vec2(14.0, 0.0), egui::Align2::LEFT_CENTER, &n.name, egui::FontId::proportional(12.0), text_color);
            }
        }
        if let Some(src) = drag_stopped {
            st.drag = None;
            if let Some(t) = pointer.and_then(target).filter(|t| *t != src) {
                // A highlighted icon drags every highlighted row along.
                let ids: Vec<u64> = if hl.contains(&src) { order.iter().copied().filter(|r| hl.contains(r) && *r != t).collect() } else { vec![src] };
                acts.push(Act::Link(ids, (t != SCENE_ROOT).then_some(t)));
            }
        }
        if let Some((id, name)) = rename_commit {
            let name = name.trim().to_owned();
            if !name.is_empty() {
                // The edited row takes the name, the other highlighted rows numbered versions.
                let mut ids = vec![id];
                if hl.contains(&id) {
                    ids.extend(order.iter().copied().filter(|r| hl.contains(r) && *r != id));
                }
                acts.push(Act::Cmd(Command::RenameNodes { ids, name }));
            }
        }
    });
}

/// The right-click menu of a row.
fn row_menu(ui: &mut egui::Ui, app: &LoomApp, ex: &SceneExplorer, hl: &BTreeSet<u64>, id: u64, st: &mut ExUi, acts: &mut Vec<Act>) {
    let ids: Vec<u64> = if hl.contains(&id) { hl.iter().copied().collect() } else { vec![id] };
    if !ex.sync_selection {
        if item(ui, "Select In Scene") {
            acts.push(Act::Cmd(Command::SelectNodes { ids: ids.clone(), op: SelectOp::Replace }));
        }
        if item(ui, "Pull Selection from Scene") {
            acts.push(Act::Highlight(app.state.selection.iter().map(|n| n.0).collect()));
        }
        ui.separator();
    }
    if !ex.lock_cell_editing && item(ui, "Rename") {
        st.active = Some(id);
        st.rename_request = true;
    }
    if item(ui, "Delete From Scene") {
        acts.push(Act::Cmd(Command::DeleteNodes { ids: ids.clone() }));
    }
    ui.separator();
    for (label, prop, value) in [("Hide", "Hidden", true), ("Unhide", "Hidden", false), ("Freeze", "Frozen", true), ("Unfreeze", "Frozen", false)] {
        if item(ui, label) {
            acts.push(Act::Cmd(Command::SetNodesProperty { ids: ids.clone(), name: prop.into(), value }));
        }
    }
    ui.separator();
    ui.menu_button("Groups", |ui| {
        // The Group menu's commands act on the scene selection: select the rows first.
        for (label, cmd) in [("Open Group", Command::GroupOpen), ("Close Group", Command::GroupClose), ("Ungroup", Command::Ungroup), ("Detach", Command::GroupDetach)] {
            if item(ui, label) {
                acts.push(Act::Cmd(Command::SelectNodes { ids: ids.clone(), op: SelectOp::Replace }));
                acts.push(Act::Cmd(cmd));
            }
        }
    });
    ui.separator();
    if item(ui, "Expand All") {
        acts.push(Act::SetCollapsed(BTreeSet::new()));
    }
    if item(ui, "Collapse All") {
        acts.push(Act::SetCollapsed(app.state.scene.nodes().filter(|n| !app.state.scene.children(n.id).is_empty()).map(|n| n.id.0).collect()));
    }
    ui.separator();
    if item(ui, "Configure Columns...") {
        st.columns_open = true;
    }
}

/// Configure Columns: every column, a check adds or removes it.
fn configure_columns(ctx: &egui::Context, app: &LoomApp, ex: &SceneExplorer, st: &mut ExUi, acts: &mut Vec<Act>) {
    let mut open = true;
    egui::Window::new("Configure Columns").id(window_id(ex.id).with("columns")).open(&mut open).collapsible(false).resizable(false).default_pos([760.0, 140.0]).show(ctx, |ui| {
        let _ = app;
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            for c in Column::ALL {
                let mut on = ex.columns.contains(&c);
                let only = on && ex.columns.len() == 1;
                if ui.add_enabled(!only, egui::Checkbox::new(&mut on, c.label())).changed() {
                    let mut cols: Vec<String> = ex.columns.iter().map(|c| c.label().to_owned()).collect();
                    if on {
                        cols.push(c.label().into());
                    } else {
                        cols.retain(|n| n != c.label());
                    }
                    acts.push(Act::Cmd(Command::SceneExplorerColumns { explorer: Some(ex.name.clone()), columns: cols }));
                }
            }
        });
    });
    if !open {
        st.columns_open = false;
    }
}

/// The choices of a Reference Value for a property that has a fixed set.
fn value_choices(property: &str) -> Option<Vec<String>> {
    match property {
        "Type" => Some(Category::ALL.iter().map(|c| c.label().to_owned()).collect()),
        "Hidden" | "Frozen" | "Renderable" | "Has Material" => Some(vec!["Yes".into(), "No".into()]),
        _ => None,
    }
}

/// Advanced Search: search phrases combined with Match All / Match Any.
fn advanced_search(ctx: &egui::Context, app: &mut LoomApp, ex: &SceneExplorer, st: &mut ExUi) {
    let mut open = true;
    let mut run = false;
    let mut cancel = false;
    let props = explorer::SEARCH_PROPERTIES;
    let d = &mut st.search;
    egui::Window::new("Advanced Search").id(window_id(ex.id).with("search")).open(&mut open).collapsible(false).resizable(false).default_pos([700.0, 200.0]).show(ctx, |ui| {
        egui::Grid::new("adv_search_grid").num_columns(2).show(ui, |ui| {
            ui.label("Property:");
            egui::ComboBox::from_id_salt("adv_search_property").selected_text(props[d.property].0).show_ui(ui, |ui| {
                for (k, (p, _)) in props.iter().enumerate() {
                    if ui.selectable_label(k == d.property, *p).clicked() {
                        d.property = k;
                        d.condition = 0;
                        d.value = value_choices(p).map(|v| v[0].clone()).unwrap_or_default();
                    }
                }
            });
            ui.end_row();
            ui.label("Condition:");
            let conds = props[d.property].1;
            d.condition = d.condition.min(conds.len() - 1);
            egui::ComboBox::from_id_salt("adv_search_condition").selected_text(conds[d.condition]).show_ui(ui, |ui| {
                for (k, c) in conds.iter().enumerate() {
                    if ui.selectable_label(k == d.condition, *c).clicked() {
                        d.condition = k;
                    }
                }
            });
            ui.end_row();
            ui.label("Reference Value:");
            match value_choices(props[d.property].0) {
                Some(choices) => {
                    if !choices.contains(&d.value) {
                        d.value = choices[0].clone();
                    }
                    egui::ComboBox::from_id_salt("adv_search_value").selected_text(d.value.clone()).show_ui(ui, |ui| {
                        for c in &choices {
                            if ui.selectable_label(*c == d.value, c).clicked() {
                                d.value = c.clone();
                            }
                        }
                    });
                }
                None => {
                    let r = ui.text_edit_singleline(&mut d.value);
                    icons::named(&r, egui::WidgetType::TextEdit, "Reference Value");
                }
            }
            ui.end_row();
        });
        ui.horizontal(|ui| {
            if ui.button("Add").clicked() {
                d.terms.push(SearchTerm { property: props[d.property].0.into(), condition: props[d.property].1[d.condition].into(), value: d.value.clone() });
            }
            if ui.add_enabled(d.picked.is_some(), egui::Button::new("Remove")).clicked() {
                if let Some(k) = d.picked.take() {
                    if k < d.terms.len() {
                        d.terms.remove(k);
                    }
                }
            }
            ui.radio_value(&mut d.match_all, true, "Match All");
            ui.radio_value(&mut d.match_all, false, "Match Any");
        });
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_size(egui::vec2(320.0, 90.0));
            for (k, t) in d.terms.iter().enumerate() {
                if ui.selectable_label(d.picked == Some(k), format!("{} {} {}", t.property, t.condition, t.value)).clicked() {
                    d.picked = Some(k);
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(!d.terms.is_empty(), egui::Button::new("Select")).clicked() {
                run = true;
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
    });
    if run {
        let terms = st.search.terms.clone();
        if let Err(e) = app.dispatch(Command::SceneExplorerSearch { explorer: Some(ex.name.clone()), terms, match_all: st.search.match_all }) {
            app.state.status_prompt = e;
        }
    }
    if !open || run || cancel {
        st.search_open = false;
    }
}

/// Tools > Manage Scene Explorer.
pub fn manage(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("manage_scene_explorer");
    let (mut picked, mut rename): (Option<String>, Option<String>) = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
    let names: Vec<String> = app.state.explorers.list.iter().map(|e| e.name.clone()).collect();
    let default = app.state.explorers.default.is_some();
    let mut acts: Vec<Command> = Vec::new();
    egui::Window::new("Manage Scene Explorer").open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_size(egui::vec2(240.0, 120.0));
            for n in &names {
                if ui.selectable_label(picked.as_deref() == Some(n.as_str()), n).clicked() {
                    picked = Some(n.clone());
                    rename = None;
                }
            }
        });
        let one = picked.as_ref().filter(|p| names.contains(p)).cloned();
        if let Some(text) = rename.as_mut() {
            ui.horizontal(|ui| {
                ui.label("Name:");
                let r = ui.text_edit_singleline(text);
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if let Some(p) = &one {
                        acts.push(Command::RenameSceneExplorer { explorer: Some(p.clone()), name: text.clone() });
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            if ui.add_enabled(one.is_some(), egui::Button::new("Delete")).clicked() {
                if let Some(p) = &one {
                    acts.push(Command::DeleteSceneExplorer { explorer: p.clone() });
                }
            }
            if ui.add_enabled(one.is_some(), egui::Button::new("Rename")).clicked() {
                rename = one.clone();
            }
            if ui.add_enabled(one.is_some(), egui::Button::new("Set as Default")).clicked() {
                if let Some(p) = &one {
                    acts.push(Command::SetDefaultSceneExplorer { explorer: p.clone() });
                }
            }
        });
        if default {
            ui.weak("New explorers take the default setup.");
        }
    });
    for c in acts {
        let renamed = matches!(&c, Command::RenameSceneExplorer { .. });
        if let Err(e) = app.dispatch(c) {
            app.state.status_prompt = e;
        } else if renamed {
            picked = rename.take();
        }
    }
    if *open {
        ctx.data_mut(|d| d.insert_temp(id, (picked, rename)));
    } else {
        ctx.data_mut(|d| d.remove::<(Option<String>, Option<String>)>(id));
    }
}
