//! The Track View window (Graph Editors > Track View): menus,
//! toolbars, the controller hierarchy, the Curve Editor key window and the
//! Dope Sheet. The window only draws and turns mouse input into commands.

use crate::shortcuts::HoverShortcut;
use crate::LoomApp;
use eframe::egui;
use loom_commands::track_view::{highlighted_tracks, track_id, tree, DopeMode, KeyArg, TvItemKind, TvMode, TvTool};
use loom_commands::{Command, SelectOp};
use loom_scene::anim::TangentType;
use loom_scene::tracks::{self, TrackPath};

/// A mouse drag in the key window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TvDrag {
    /// Rectangle select from a point (time, value).
    Region { start: egui::Pos2 },
    /// Moving the selected keys: frames / value already applied.
    Move { start: egui::Pos2, applied: (f32, f32) },
    /// Scale Keys / Scale Values / Slide / Scale Time / Insert Time / Select Time / Draw Curves.
    Tool { start: egui::Pos2 },
    Pan { last: egui::Pos2 },
    /// A tangent handle of key (track index in the visible list, comp, key index), In or Out.
    Handle { track: usize, comp: u32, key: usize, out: bool },
    /// Edit Ranges: dragging a range bar.
    Range { start: egui::Pos2, applied: f32 },
}

#[derive(Clone, Copy)]
struct KeyView {
    rect: egui::Rect,
    view: [f32; 4],
}

impl KeyView {
    fn x(&self, t: f32) -> f32 {
        self.rect.left() + (t - self.view[0]) / (self.view[1] - self.view[0]) * self.rect.width()
    }
    fn y(&self, v: f32) -> f32 {
        self.rect.bottom() - (v - self.view[2]) / (self.view[3] - self.view[2]) * self.rect.height()
    }
    fn pos(&self, t: f32, v: f32) -> egui::Pos2 {
        egui::pos2(self.x(t), self.y(v))
    }
    fn time(&self, x: f32) -> f32 {
        self.view[0] + (x - self.rect.left()) / self.rect.width() * (self.view[1] - self.view[0])
    }
    fn value(&self, y: f32) -> f32 {
        self.view[2] + (self.rect.bottom() - y) / self.rect.height() * (self.view[3] - self.view[2])
    }
    fn per_px(&self) -> (f32, f32) {
        ((self.view[1] - self.view[0]) / self.rect.width(), (self.view[3] - self.view[2]) / self.rect.height())
    }
}

/// Hierarchy and Dope Sheet row height, and the time ruler's height above both.
const ROW_H: f32 = 21.0;
const RULER_H: f32 = 16.0;

const AXIS: [egui::Color32; 3] = [egui::Color32::from_rgb(230, 70, 70), egui::Color32::from_rgb(80, 210, 80), egui::Color32::from_rgb(90, 140, 255)];
const FLOAT_COLORS: [egui::Color32; 4] = [egui::Color32::from_rgb(235, 190, 60), egui::Color32::from_rgb(200, 120, 230), egui::Color32::from_rgb(80, 210, 210), egui::Color32::from_rgb(240, 140, 90)];

fn color(track_index: usize, comp: usize, dims: usize) -> egui::Color32 {
    if dims == 3 { AXIS[comp % 3] } else { FLOAT_COLORS[track_index % FLOAT_COLORS.len()] }
}

fn run(app: &mut LoomApp, cmd: Command) {
    if let Err(e) = app.dispatch(cmd) {
        app.state.status_prompt = e;
    }
}

fn item(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, cmd: Command) {
    if crate::shortcuts::menu_button(ui, &app.state, label, cmd.name()).clicked() {
        run(app, cmd);
        ui.close_menu();
    }
}

fn check(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, on: bool, cmd: Command) {
    let mut v = on;
    if ui.checkbox(&mut v, label).with_shortcut(&app.state, cmd.name()).clicked() {
        run(app, cmd);
        ui.close_menu();
    }
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp) {
    if !app.state.track_view.open {
        return;
    }
    // Opening on an empty highlight shows the selection's animated tracks (Auto Select).
    if app.state.track_view.highlighted.is_empty() {
        let sel: Vec<loom_scene::NodeId> = app.state.selection.iter().copied().collect();
        let items: Vec<String> = sel.iter().flat_map(|id| tracks::list_tracks(&app.state.scene, *id)).filter(|t| tracks::read(&app.state.scene, t).is_some_and(|p| p.is_animated())).map(|t| track_id(&t)).collect();
        if !items.is_empty() {
            app.state.track_view.highlighted = items;
        }
    }
    let mode = app.state.track_view.mode;
    let title = format!("Track View - {}", if mode == TvMode::CurveEditor { "Curve Editor" } else { "Dope Sheet" });
    let mut open = true;
    let max_h = (ctx.screen_rect().height() - 60.0).max(240.0);
    let resp = egui::Window::new(title).id(egui::Id::new("track_view")).open(&mut open).default_size([900.0, 420.0]).default_pos([60.0, 300.0]).max_height(max_h).resizable(true).show(ctx, |ui| {
        menus(ui, app);
        toolbar(ui, app);
        ui.separator();
        let avail = ui.available_size();
        // Room for the bottom bar and the spacing around it, so the window does not grow each frame.
        let bottom_h = 44.0;
        let body_h = (avail.y - bottom_h).max(140.0);
        ui.horizontal_top(|ui| {
            let list_w = 230.0f32.min(avail.x * 0.35);
            ui.allocate_ui_with_layout(egui::vec2(list_w, body_h), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(list_w);
                hierarchy(ui, app, body_h)
            });
            ui.separator();
            let w = ui.available_width();
            let (response, painter) = ui.allocate_painter(egui::vec2(w, body_h), egui::Sense::click_and_drag());
            if app.state.track_view.mode == TvMode::CurveEditor {
                curve_editor(ui, app, &response, &painter);
            } else {
                dope_sheet(ui, app, &response, &painter);
            }
        });
        bottom_bar(ui, app);
    });
    // Keyboard focus follows the last click: inside the window its shortcuts apply.
    if let Some(r) = resp.as_ref() {
        let pressed = ctx.input(|i| i.pointer.any_pressed().then(|| i.pointer.interact_pos()).flatten());
        if let Some(p) = pressed {
            app.state.track_view.focused = r.response.rect.contains(p);
        }
    }
    if !open {
        if matches!(app.tv_drag.take(), Some(TvDrag::Move { .. } | TvDrag::Range { .. })) {
            app.state.undo.end();
        }
        run(app, Command::CloseTrackView);
    }
}

fn menus(ui: &mut egui::Ui, app: &mut LoomApp) {
    let tv = app.state.track_view.clone();
    let curve = tv.mode == TvMode::CurveEditor;
    egui::menu::bar(ui, |ui| {
        ui.menu_button("Modes", |ui| {
            item(ui, app, "Curve Editor", Command::OpenCurveEditor);
            item(ui, app, "Dope Sheet", Command::OpenDopeSheet);
        });
        ui.menu_button("Controller", |ui| {
            item(ui, app, "Assign...", Command::AssignControllerTrackView);
            item(ui, app, "Delete", Command::DeleteController);
            item(ui, app, "Keyable", Command::KeyablePropertyToggle);
            item(ui, app, "Lock", Command::Locked);
            ui.separator();
            item(ui, app, "Copy", Command::CopyController);
            item(ui, app, "Paste", Command::PasteController);
            item(ui, app, "Collapse", Command::CollapseController { start: None, end: None, samples: None });
            item(ui, app, "Make Unique", Command::MakeControllerUnique);
            ui.separator();
            item(ui, app, "Out of Range Types...", Command::ParameterCurveOutOfRangeTypes);
            item(ui, app, "Properties...", Command::PropertiesTrackView);
        });
        ui.menu_button("Tracks", |ui| {
            ui.menu_button("Note Track", |ui| {
                item(ui, app, "Add", Command::AddNoteTrack);
                item(ui, app, "Remove", Command::DeleteNoteTrack);
            });
            ui.menu_button("Visibility Track", |ui| {
                item(ui, app, "Add", Command::AddVisibilityTrack);
                item(ui, app, "Remove", Command::DeleteVisibilityTrack);
            });
        });
        ui.menu_button("Keys", |ui| {
            item(ui, app, "Add Keys", Command::AddKeys);
            item(ui, app, "Reduce Keys...", Command::SimplifyCurve);
            ui.menu_button("Move", |ui| {
                item(ui, app, "Move", Command::MoveKeys);
                item(ui, app, "Move Horizontal", Command::MoveKeysHorizontal);
                item(ui, app, "Move Vertical", Command::MoveKeysVertical);
            });
            item(ui, app, "Slide", Command::SlideKeys);
            if curve {
                item(ui, app, "Scale Values", Command::ScaleValues);
            }
            item(ui, app, "Scale Keys - Time", Command::ScaleKeys);
            if curve {
                item(ui, app, "Region Tool", Command::RegionKeysTool);
                item(ui, app, "Draw Curves", Command::DrawCurves);
            }
            ui.separator();
            check(ui, app, "Use Soft Select", tv.soft, Command::TrackViewOption { name: "Use Soft Select".into(), on: !tv.soft });
            item(ui, app, "Soft Select Settings...", Command::TrackViewUtilities);
            ui.separator();
            item(ui, app, "Align to Cursor", Command::AlignKeys);
            check(ui, app, "Snap Frames", tv.snap_frames, Command::SnapFrames);
            check(ui, app, "Isolate Curve", tv.isolate, Command::IsolateCurve);
        });
        if !curve {
            ui.menu_button("Time", |ui| {
                item(ui, app, "Select", Command::SelectTime);
                item(ui, app, "Insert", Command::InsertTime);
                item(ui, app, "Cut", Command::CutTime);
                item(ui, app, "Copy", Command::CopyTime);
                item(ui, app, "Paste", Command::PasteTime);
                item(ui, app, "Reverse", Command::ReverseTime);
                item(ui, app, "Scale", Command::ScaleTime);
                item(ui, app, "Delete", Command::TrackViewDeleteTime);
                ui.separator();
                check(ui, app, "Exclude Left End Point", tv.exclude_left, Command::ExcludeLeftEndPointToggle);
                check(ui, app, "Exclude Right End Point", tv.exclude_right, Command::ExcludeRightEndPointToggle);
            });
        }
        ui.menu_button("Options", |ui| {
            check(ui, app, "Interactive Update", tv.interactive_update, Command::TrackViewOption { name: "Interactive Update".into(), on: !tv.interactive_update });
            check(ui, app, "Sync Cursor Time", tv.sync_cursor_time, Command::TrackViewOption { name: "Sync Cursor Time".into(), on: !tv.sync_cursor_time });
            ui.menu_button("Auto Expand", |ui| {
                check(ui, app, "Animated", tv.auto_expand_animated, Command::AutoExpandAnimated);
                check(ui, app, "Keyable", tv.auto_expand_keyable, Command::AutoExpandKeyable);
            });
            check(ui, app, "Modify Subtree", tv.modify_subtree, Command::ModifySubtreeToggle);
            check(ui, app, "Manual Navigation", tv.manual_navigation, Command::TrackViewOption { name: "Manual Navigation".into(), on: !tv.manual_navigation });
        });
        if curve {
            ui.menu_button("Display", |ui| {
                check(ui, app, "Selected Key Stats", tv.key_stats, Command::ShowSelectedKeyStats);
                check(ui, app, "All Tangents", tv.all_tangents, Command::ViewAllTangents);
                check(ui, app, "Show Tangents", tv.show_tangents, Command::ShowTangentsToggle);
                ui.separator();
                check(ui, app, "Hide Non-Selected Curves", tv.hide_nonselected, Command::TrackViewOption { name: "Hide Non-Selected Curves".into(), on: !tv.hide_nonselected });
                check(ui, app, "Freeze Non-Selected Curves", tv.freeze_nonselected, Command::FreezeNonselectedCurves);
                check(ui, app, "Show Keys on Frozen Curves", tv.keys_on_frozen, Command::ShowKeysOnFrozenCurves);
                ui.separator();
                item(ui, app, "Filters...", Command::Filters);
            });
        }
        ui.menu_button("View", |ui| {
            item(ui, app, "Pan", Command::PanTrackView);
            item(ui, app, "Zoom", Command::Zoom);
            item(ui, app, "Zoom Time", Command::ZoomTime);
            if curve {
                item(ui, app, "Zoom Values", Command::ZoomValues);
            }
            item(ui, app, "Zoom Region", Command::ZoomRegion);
            ui.separator();
            item(ui, app, "Zoom Horizontal Extents", Command::FrameHorizontalExtents);
            item(ui, app, "Zoom Horizontal Extents Keys", Command::FrameHorizontalExtentsKeys);
            if curve {
                item(ui, app, "Zoom Value Extents", Command::FrameValueExtents);
            }
            item(ui, app, "Zoom Selected Object", Command::ZoomSelectedObject);
        });
        ui.menu_button("Utilities", |ui| {
            item(ui, app, "Track View Utilities...", Command::TrackViewUtilities);
        });
    });
}

fn tool_button(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, tip: &str, on: bool, cmd: Command) {
    if ui.add(egui::Button::new(label).selected(on).min_size(egui::vec2(26.0, 22.0))).on_hover_cmd(&app.state, tip, cmd.name()).clicked() {
        run(app, cmd);
    }
}

fn toolbar(ui: &mut egui::Ui, app: &mut LoomApp) {
    let tv = app.state.track_view.clone();
    ui.horizontal_wrapped(|ui| {
        tool_button(ui, app, "Flt", "Filters", false, Command::Filters);
        ui.separator();
        if tv.mode == TvMode::DopeSheet {
            tool_button(ui, app, "Keys", "Edit Keys", tv.dope == DopeMode::EditKeys, Command::EditKeysMode);
            tool_button(ui, app, "Rng", "Edit Ranges", tv.dope == DopeMode::EditRanges, Command::EditRangesMode);
            ui.separator();
        }
        tool_button(ui, app, "Mov", "Move Keys", tv.tool == TvTool::Move, Command::MoveKeys);
        tool_button(ui, app, "MvH", "Move Keys Horizontal", tv.tool == TvTool::MoveHorizontal, Command::MoveKeysHorizontal);
        if tv.mode == TvMode::CurveEditor {
            tool_button(ui, app, "MvV", "Move Keys Vertical", tv.tool == TvTool::MoveVertical, Command::MoveKeysVertical);
        }
        tool_button(ui, app, "Sld", "Slide Keys", tv.tool == TvTool::Slide, Command::SlideKeys);
        tool_button(ui, app, "ScK", "Scale Keys - Time", tv.tool == TvTool::ScaleKeys, Command::ScaleKeys);
        if tv.mode == TvMode::CurveEditor {
            tool_button(ui, app, "ScV", "Scale Values", tv.tool == TvTool::ScaleValues, Command::ScaleValues);
        }
        tool_button(ui, app, "Add", "Add Keys", tv.tool == TvTool::AddKeys, Command::AddKeys);
        if tv.mode == TvMode::CurveEditor {
            tool_button(ui, app, "Drw", "Draw Curves", tv.tool == TvTool::DrawCurves, Command::DrawCurves);
            tool_button(ui, app, "Rgn", "Region Tool", tv.tool == TvTool::Region, Command::RegionKeysTool);
            tool_button(ui, app, "Red", "Reduce Keys", false, Command::SimplifyCurve);
            ui.separator();
            for (label, tip, c) in [
                ("Aut", "Set Tangents to Auto", Command::SetTangentsToAuto),
                ("Spl", "Set Tangents to Spline", Command::SetTangentsToSpline),
                ("Fst", "Set Tangents to Fast", Command::SetTangentsToFast),
                ("Slw", "Set Tangents to Slow", Command::SetTangentsToSlow),
                ("Stp", "Set Tangents to Step", Command::SetTangentsToStepped),
                ("Lin", "Set Tangents to Linear", Command::SetTangentsToLinear),
                ("Smt", "Set Tangents to Smooth", Command::SetTangentsToSmooth),
            ] {
                tool_button(ui, app, label, tip, false, c);
            }
            ui.separator();
            tool_button(ui, app, "Brk", "Break Tangents", false, Command::BreakTangents);
            tool_button(ui, app, "Uni", "Unify Tangents", false, Command::UnifyTangents);
        } else {
            ui.separator();
            tool_button(ui, app, "SlT", "Select Time", tv.tool == TvTool::SelectTime, Command::SelectTime);
            tool_button(ui, app, "DlT", "Delete Time", false, Command::TrackViewDeleteTime);
            tool_button(ui, app, "RvT", "Reverse Time", false, Command::ReverseTime);
            tool_button(ui, app, "ScT", "Scale Time", tv.tool == TvTool::ScaleTime, Command::ScaleTime);
            tool_button(ui, app, "InT", "Insert Time", tv.tool == TvTool::InsertTime, Command::InsertTime);
            tool_button(ui, app, "CuT", "Cut Time", false, Command::CutTime);
            tool_button(ui, app, "CpT", "Copy Time", false, Command::CopyTime);
            tool_button(ui, app, "PsT", "Paste Time", false, Command::PasteTime);
        }
        ui.separator();
        tool_button(ui, app, "Snp", "Snap Frames", tv.snap_frames, Command::SnapFrames);
        tool_button(ui, app, "LkS", "Lock Selection", tv.lock_selection, Command::LockSelection);
        tool_button(ui, app, "ORT", "Parameter Curve Out-of-Range Types", false, Command::ParameterCurveOutOfRangeTypes);
        if tv.mode == TvMode::CurveEditor {
            tool_button(ui, app, "LkT", "Lock Tangents", tv.lock_tangents, Command::LockTangentsToggle);
            tool_button(ui, app, "Tan", "Show Tangents", tv.show_tangents, Command::ShowTangentsToggle);
        } else {
            tool_button(ui, app, "Sub", "Modify Subtree", tv.modify_subtree, Command::ModifySubtreeToggle);
        }
        ui.separator();
        tool_button(ui, app, "Pan", "Pan", tv.tool == TvTool::Pan, Command::PanTrackView);
        tool_button(ui, app, "HEx", "Zoom Horizontal Extents", false, Command::FrameHorizontalExtents);
        tool_button(ui, app, "HEK", "Zoom Horizontal Extents Keys", false, Command::FrameHorizontalExtentsKeys);
        if tv.mode == TvMode::CurveEditor {
            tool_button(ui, app, "VEx", "Zoom Value Extents", false, Command::FrameValueExtents);
        }
        tool_button(ui, app, "Zm", "Zoom", tv.tool == TvTool::Zoom, Command::Zoom);
        tool_button(ui, app, "ZmR", "Zoom Region", tv.tool == TvTool::ZoomRegion, Command::ZoomRegion);
    });
}

fn hierarchy(ui: &mut egui::Ui, app: &mut LoomApp, h: f32) {
    let rows = tree(&app.state);
    let highlighted = app.state.track_view.highlighted.clone();
    let collapsed = app.state.track_view.collapsed.clone();
    let controller_types = app.state.track_view.filters.controller_types;
    let scroll = app.state.track_view.scroll;
    let w = ui.available_width();
    ui.spacing_mut().item_spacing.y = 0.0;
    // The ruler gap keeps rows level with the Dope Sheet's.
    ui.add_space(RULER_H);
    let visible = ((h - RULER_H) / ROW_H).floor().max(1.0) as usize;
    for r in rows.iter().skip(scroll).take(visible) {
        ui.allocate_ui_with_layout(egui::vec2(w, ROW_H), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_height(ROW_H);
            ui.add_space(r.depth as f32 * 14.0);
            let has_children = !matches!(r.kind, TvItemKind::Track(_) | TvItemKind::Note(_));
            if has_children {
                let sign = if collapsed.contains(&r.id) { "+" } else { "-" };
                if ui.small_button(sign).clicked() {
                    app.state.track_view.highlighted = vec![r.id.clone()];
                    run(app, Command::ExpandTrackToggle);
                }
            }
            let mut label = r.label.clone();
            if controller_types && !r.controller.is_empty() {
                label = format!("{label} : {}", r.controller);
            }
            if r.locked {
                label.push_str(" (Locked)");
            }
            let mut text = egui::RichText::new(label);
            if !r.keyable {
                text = text.italics();
            }
            if r.animated {
                text = text.strong();
            }
            let resp = ui.selectable_label(highlighted.contains(&r.id), text);
            if resp.clicked() {
                let ctrl = ui.input(|i| i.modifiers.ctrl);
                let op = if ctrl && highlighted.contains(&r.id) { SelectOp::Subtract } else if ctrl { SelectOp::Add } else { SelectOp::Replace };
                run(app, Command::TrackViewHighlight { items: vec![r.id.clone()], op });
            }
            resp.context_menu(|ui| {
                if !highlighted.contains(&r.id) {
                    app.state.track_view.highlighted = vec![r.id.clone()];
                }
                item(ui, app, "Assign Controller...", Command::AssignControllerTrackView);
                item(ui, app, "Copy", Command::CopyController);
                item(ui, app, "Paste", Command::PasteController);
                item(ui, app, "Make Unique", Command::MakeControllerUnique);
                item(ui, app, "Properties...", Command::PropertiesTrackView);
                ui.separator();
                item(ui, app, "Expand Objects", Command::ExpandObjects);
                item(ui, app, "Expand Tracks", Command::ExpandTracks);
                item(ui, app, "Expand All", Command::ExpandAll);
                item(ui, app, "Collapse Objects", Command::CollapseObjects);
                item(ui, app, "Collapse Tracks", Command::CollapseTracks);
                item(ui, app, "Collapse All", Command::CollapseAllTrackView);
                ui.separator();
                item(ui, app, "Select All", Command::SelectAllTrackView);
                item(ui, app, "Select Invert", Command::SelectInvertTrackView);
                item(ui, app, "Select Children", Command::SelectChildrenTrackView);
                item(ui, app, "Select None", Command::SelectNoneTrackView);
            });
        });
    }
    // The wheel over the list scrolls it (Scroll Up / Down).
    let r = ui.interact(ui.max_rect(), egui::Id::new("tv_tree_scroll"), egui::Sense::hover());
    if r.hovered() {
        let d = ui.input(|i| i.raw_scroll_delta.y);
        if d > 0.0 {
            run(app, Command::ScrollUp);
        } else if d < 0.0 {
            run(app, Command::ScrollDown);
        }
    }
}

/// The tracks the Curve Editor draws: highlighted ones, or every visible
/// track when non-selected curves are shown.
fn shown_tracks(app: &LoomApp) -> (Vec<TrackPath>, Vec<TrackPath>) {
    let hi = highlighted_tracks(&app.state);
    let all: Vec<TrackPath> = if app.state.track_view.hide_nonselected { Vec::new() } else { tree(&app.state).into_iter().filter_map(|i| if let TvItemKind::Track(t) = i.kind { Some(t) } else { None }).filter(|t| !hi.contains(t)).collect() };
    (hi, all)
}

fn ruler(painter: &egui::Painter, kv: &KeyView, frame: i32, top: bool) {
    let span = kv.view[1] - kv.view[0];
    let step = [1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0].into_iter().find(|s| span / s <= 20.0).unwrap_or(1000.0);
    let y = if top { kv.rect.top() } else { kv.rect.bottom() - 14.0 };
    let mut t = (kv.view[0] / step).floor() * step;
    while t <= kv.view[1] {
        let x = kv.x(t);
        painter.line_segment([egui::pos2(x, kv.rect.top()), egui::pos2(x, kv.rect.bottom())], egui::Stroke::new(1.0_f32, egui::Color32::from_gray(58)));
        painter.text(egui::pos2(x + 2.0, y), egui::Align2::LEFT_TOP, format!("{}", t as i32), egui::FontId::proportional(10.0), egui::Color32::from_gray(170));
        t += step;
    }
    let x = kv.x(frame as f32);
    painter.line_segment([egui::pos2(x, kv.rect.top()), egui::pos2(x, kv.rect.bottom())], egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(120, 160, 255)));
}

fn curve_editor(ui: &mut egui::Ui, app: &mut LoomApp, response: &egui::Response, painter: &egui::Painter) {
    let rect = response.rect;
    painter.rect_filled(rect, 0.0, egui::Color32::from_gray(44));
    let kv = KeyView { rect, view: app.state.track_view.view };
    ruler(painter, &kv, app.state.current_frame, true);
    // Value grid.
    let vspan = kv.view[3] - kv.view[2];
    let vstep = [0.1, 0.5, 1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0].into_iter().find(|s| vspan / s <= 12.0).unwrap_or(1000.0);
    let mut v = (kv.view[2] / vstep).floor() * vstep;
    while v <= kv.view[3] {
        let y = kv.y(v);
        painter.line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)], egui::Stroke::new(1.0_f32, if v.abs() < vstep * 0.01 { egui::Color32::from_gray(90) } else { egui::Color32::from_gray(52) }));
        painter.text(egui::pos2(rect.left() + 2.0, y), egui::Align2::LEFT_BOTTOM, format!("{v}"), egui::FontId::proportional(10.0), egui::Color32::from_gray(150));
        v += vstep;
    }
    let tv = app.state.track_view.clone();
    let (hi, others) = shown_tracks(app);
    let sel = tv.selected_keys.clone();
    // Keys hit-tested this frame: (screen pos, track, comp, key time, key index, track index).
    let mut hits: Vec<(egui::Pos2, TrackPath, u32, f32, usize, usize)> = Vec::new();
    let mut handles: Vec<(egui::Pos2, usize, u32, usize, bool)> = Vec::new();
    let draw = |painter: &egui::Painter, list: &[TrackPath], frozen: bool, hits: &mut Vec<_>, handles: &mut Vec<_>| {
        for (ti, t) in list.iter().enumerate() {
            let Some(p) = tracks::read(&app.state.scene, t) else { continue };
            let dims = match t.ty {
                loom_scene::param::ParamType::Vec3 => 3,
                _ => 1,
            };
            let n = rect.width().max(2.0) as usize / 2;
            let samples: Vec<Vec<f32>> = (0..=n).map(|i| p.comps_at(kv.time(rect.left() + i as f32 * 2.0))).collect();
            for c in 0..dims {
                let col = color(ti, c, dims);
                let stroke = if frozen { egui::Stroke::new(1.0_f32, col.gamma_multiply(0.45)) } else { egui::Stroke::new(1.5_f32, col) };
                let pts: Vec<egui::Pos2> = samples.iter().enumerate().map(|(i, s)| egui::pos2(rect.left() + i as f32 * 2.0, kv.y(s[c]))).collect();
                if frozen {
                    for w in pts.chunks(2).step_by(2) {
                        if let [a, b] = w {
                            painter.line_segment([*a, *b], stroke);
                        }
                    }
                } else {
                    painter.add(egui::Shape::line(pts, stroke));
                }
                let Some(k) = p.keys() else { continue };
                let Some(tr) = k.tracks.get(c) else { continue };
                if frozen && !tv.keys_on_frozen {
                    continue;
                }
                for (ki, key) in tr.keys.iter().enumerate() {
                    let pos = kv.pos(key.time, key.value);
                    let is_sel = sel.iter().any(|s| s.track == t.key() && s.comp == c as u32 && (s.time - key.time).abs() < 1e-3);
                    painter.rect_filled(egui::Rect::from_center_size(pos, egui::vec2(6.0, 6.0)), 0.0, if is_sel { egui::Color32::WHITE } else { egui::Color32::from_gray(20) });
                    painter.rect_stroke(egui::Rect::from_center_size(pos, egui::vec2(6.0, 6.0)), 0.0, egui::Stroke::new(1.0_f32, col));
                    if !frozen {
                        hits.push((pos, t.clone(), c as u32, key.time, ki, ti));
                    }
                    if !frozen && tv.show_tangents && (is_sel || tv.all_tangents) && tr.interp == loom_scene::anim::Interp::Bezier {
                        let (tpp, vpp) = kv.per_px();
                        let len_t = 24.0 * tpp;
                        for (out, ty, slope) in [(false, key.in_type, key.in_tan), (true, key.out_type, key.out_tan)] {
                            if ty != TangentType::Custom {
                                continue;
                            }
                            let dt = if out { len_t } else { -len_t };
                            let hp = egui::pos2(kv.x(key.time + dt), kv.y(key.value + slope * dt));
                            let _ = vpp;
                            painter.line_segment([pos, hp], egui::Stroke::new(1.0_f32, egui::Color32::from_gray(200)));
                            painter.circle_filled(hp, 3.0, egui::Color32::from_rgb(240, 220, 120));
                            handles.push((hp, ti, c as u32, ki, out));
                        }
                    }
                    if is_sel && tv.key_stats {
                        painter.text(pos + egui::vec2(6.0, -6.0), egui::Align2::LEFT_BOTTOM, format!("{}, {:.2}", key.time, key.value), egui::FontId::proportional(10.0), egui::Color32::WHITE);
                    }
                }
            }
        }
    };
    draw(painter, &others, tv.freeze_nonselected, &mut hits, &mut handles);
    draw(painter, &hi, false, &mut hits, &mut handles);
    key_window_input(ui, app, response, &kv, &hits, &handles, &hi);
}

fn select_keys(app: &mut LoomApp, keys: Vec<(TrackPath, u32, f32)>, op: SelectOp) {
    let keys = keys.into_iter().map(|(t, c, time)| KeyArg { id: t.node.0, track: t.name.clone(), comp: c, time }).collect();
    run(app, Command::TrackViewSelectKeys { keys, op });
}

fn key_window_input(ui: &mut egui::Ui, app: &mut LoomApp, response: &egui::Response, kv: &KeyView, hits: &[(egui::Pos2, TrackPath, u32, f32, usize, usize)], handles: &[(egui::Pos2, usize, u32, usize, bool)], shown: &[TrackPath]) {
    let tool = app.state.track_view.tool;
    let mods = ui.input(|i| i.modifiers);
    let painter = ui.painter_at(kv.rect);
    // Wheel zooms time and value about the pointer; Ctrl time only, Shift values only.
    if response.hovered() {
        let scroll = ui.input(|i| i.raw_scroll_delta.y);
        if scroll.abs() > 0.0 {
            if let Some(p) = response.hover_pos() {
                let f = (1.0 - scroll * 0.0015).clamp(0.5, 2.0);
                let (t, v) = (kv.time(p.x), kv.value(p.y));
                let mut view = kv.view;
                if !mods.shift {
                    view[0] = t + (view[0] - t) * f;
                    view[1] = t + (view[1] - t) * f;
                }
                if !mods.ctrl {
                    view[2] = v + (view[2] - v) * f;
                    view[3] = v + (view[3] - v) * f;
                }
                run(app, Command::TrackViewSetView { start: view[0], end: view[1], low: view[2], high: view[3] });
            }
        }
    }
    let middle = ui.input(|i| i.pointer.middle_down());
    if response.drag_started() {
        let Some(p) = response.interact_pointer_pos() else { return };
        if middle || tool == TvTool::Pan {
            app.tv_drag = Some(TvDrag::Pan { last: p });
        } else if let Some((_, ti, c, ki, out)) = handles.iter().find(|h| h.0.distance(p) < 6.0) {
            app.tv_drag = Some(TvDrag::Handle { track: *ti, comp: *c, key: *ki, out: *out });
        } else if let Some(h) = hits.iter().find(|h| h.0.distance(p) < 6.0).filter(|_| matches!(tool, TvTool::Move | TvTool::MoveHorizontal | TvTool::MoveVertical | TvTool::Region | TvTool::Slide | TvTool::ScaleKeys | TvTool::ScaleValues)) {
            let already = app.state.track_view.selected_keys.iter().any(|s| s.track == h.1.key() && s.comp == h.2 && (s.time - h.3).abs() < 1e-3);
            if !already {
                select_keys(app, vec![(h.1.clone(), h.2, h.3)], if mods.ctrl { SelectOp::Add } else { SelectOp::Replace });
            }
            if matches!(tool, TvTool::Move | TvTool::MoveHorizontal | TvTool::MoveVertical | TvTool::Region) {
                app.state.undo.begin("Move Keys");
                app.tv_drag = Some(TvDrag::Move { start: p, applied: (0.0, 0.0) });
            } else {
                app.tv_drag = Some(TvDrag::Tool { start: p });
            }
        } else if matches!(tool, TvTool::DrawCurves | TvTool::ScaleKeys | TvTool::ScaleValues | TvTool::Slide | TvTool::Zoom | TvTool::ZoomTime | TvTool::ZoomValues | TvTool::ZoomRegion) {
            app.tv_drag = Some(TvDrag::Tool { start: p });
        } else {
            app.tv_drag = Some(TvDrag::Region { start: p });
        }
    }
    if let Some(drag) = app.tv_drag {
        let Some(p) = ui.input(|i| i.pointer.interact_pos()) else { return };
        match drag {
            TvDrag::Pan { last } => {
                let (tpp, vpp) = kv.per_px();
                let d = p - last;
                let v = kv.view;
                run(app, Command::TrackViewSetView { start: v[0] - d.x * tpp, end: v[1] - d.x * tpp, low: v[2] + d.y * vpp, high: v[3] + d.y * vpp });
                app.tv_drag = Some(TvDrag::Pan { last: p });
            }
            TvDrag::Move { start, applied } => {
                let (tpp, vpp) = kv.per_px();
                let mut dt = (p.x - start.x) * tpp;
                let mut dv = -(p.y - start.y) * vpp;
                if tool == TvTool::MoveVertical || (mods.shift && dv.abs() > dt.abs() / tpp * vpp) {
                    dt = 0.0;
                }
                if tool == TvTool::MoveHorizontal {
                    dv = 0.0;
                }
                if app.state.track_view.snap_frames {
                    dt = dt.round();
                }
                let (ddt, ddv) = (dt - applied.0, dv - applied.1);
                if ddt.abs() > 1e-4 || ddv.abs() > 1e-6 {
                    run(app, Command::TrackViewMoveKeys { dt: ddt, dv: ddv });
                }
                app.tv_drag = Some(TvDrag::Move { start, applied: (dt, dv) });
            }
            TvDrag::Region { start } => {
                painter.rect_stroke(egui::Rect::from_two_pos(start, p), 0.0, egui::Stroke::new(1.0_f32, egui::Color32::WHITE));
            }
            TvDrag::Tool { start } => match tool {
                TvTool::DrawCurves => {
                    let id = egui::Id::new("tv_draw_points");
                    ui.ctx().data_mut(|d| d.get_temp_mut_or_default::<Vec<[f32; 2]>>(id).push([kv.time(p.x), kv.value(p.y)]));
                    let pts: Vec<[f32; 2]> = ui.ctx().data(|d| d.get_temp::<Vec<[f32; 2]>>(id).unwrap_or_default());
                    let line: Vec<egui::Pos2> = pts.iter().map(|q| kv.pos(q[0], q[1])).collect();
                    painter.add(egui::Shape::line(line, egui::Stroke::new(1.5_f32, egui::Color32::WHITE)));
                }
                TvTool::ZoomRegion => {
                    painter.rect_stroke(egui::Rect::from_two_pos(start, p), 0.0, egui::Stroke::new(1.0_f32, egui::Color32::YELLOW));
                }
                _ => {
                    painter.line_segment([start, p], egui::Stroke::new(1.0_f32, egui::Color32::YELLOW));
                }
            },
            TvDrag::Handle { track, comp, key, out } => {
                if let Some(t) = shown.get(track) {
                    if let Some(k) = tracks::read(&app.state.scene, t).and_then(|pp| pp.keys().and_then(|k| k.tracks.get(comp as usize).and_then(|tr| tr.keys.get(key).cloned()))) {
                        let dt = kv.time(p.x) - k.time;
                        if dt.abs() > 1e-4 {
                            let slope = (kv.value(p.y) - k.value) / dt;
                            let both = !k.broken || app.state.track_view.lock_tangents;
                            select_keys(app, vec![(t.clone(), comp, k.time)], if app.state.track_view.lock_tangents { SelectOp::Add } else { SelectOp::Replace });
                            run(app, Command::TrackViewSetKeyInfo { in_type: None, out_type: None, in_tan: if !out || both { Some(slope) } else { None }, out_tan: if out || both { Some(slope) } else { None }, tension: None, continuity: None, bias: None, ease_to: None, ease_from: None });
                        }
                    }
                }
            }
            TvDrag::Range { .. } => {}
        }
    }
    if response.drag_stopped() {
        let p = response.interact_pointer_pos().unwrap_or_default();
        match app.tv_drag.take() {
            Some(TvDrag::Move { .. }) => app.state.undo.end(),
            Some(TvDrag::Region { start }) => {
                let r = egui::Rect::from_two_pos(start, p);
                let keys: Vec<(TrackPath, u32, f32)> = hits.iter().filter(|h| r.contains(h.0)).map(|h| (h.1.clone(), h.2, h.3)).collect();
                select_keys(app, keys, if mods.ctrl { SelectOp::Add } else if mods.alt { SelectOp::Subtract } else { SelectOp::Replace });
            }
            Some(TvDrag::Tool { start }) => {
                let (tpp, vpp) = kv.per_px();
                match tool {
                    TvTool::ScaleKeys => run(app, Command::TrackViewScaleKeys { factor: (1.0 + (p.x - start.x) / 100.0).max(0.01), origin: None }),
                    TvTool::ScaleValues => run(app, Command::TrackViewScaleValues { factor: (1.0 - (p.y - start.y) / 100.0).max(0.0), origin: 0.0 }),
                    TvTool::Slide => {
                        let dt = (p.x - start.x) * tpp;
                        run(app, Command::TrackViewSlideKeys { dt: if app.state.track_view.snap_frames { dt.round() } else { dt } });
                    }
                    TvTool::DrawCurves => {
                        let id = egui::Id::new("tv_draw_points");
                        let pts: Vec<[f32; 2]> = ui.ctx().data_mut(|d| d.remove_temp::<Vec<[f32; 2]>>(id).unwrap_or_default());
                        if let Some(t) = shown.first() {
                            run(app, Command::TrackViewDrawCurve { id: t.node.0, track: t.name.clone(), comp: 0, points: pts });
                        }
                    }
                    TvTool::Zoom | TvTool::ZoomTime | TvTool::ZoomValues => {
                        let f = (1.0 - (p.x - start.x) / 300.0).clamp(0.1, 10.0);
                        let (t, v) = (kv.time(start.x), kv.value(start.y));
                        let mut view = kv.view;
                        if tool != TvTool::ZoomValues {
                            view[0] = t + (view[0] - t) * f;
                            view[1] = t + (view[1] - t) * f;
                        }
                        if tool != TvTool::ZoomTime {
                            view[2] = v + (view[2] - v) * f;
                            view[3] = v + (view[3] - v) * f;
                        }
                        run(app, Command::TrackViewSetView { start: view[0], end: view[1], low: view[2], high: view[3] });
                    }
                    TvTool::ZoomRegion => {
                        let (a, b) = (kv.time(start.x.min(p.x)), kv.time(start.x.max(p.x)));
                        let (lo, hi) = (kv.value(start.y.max(p.y)), kv.value(start.y.min(p.y)));
                        if b - a > tpp && hi - lo > vpp {
                            run(app, Command::TrackViewSetView { start: a, end: b, low: lo, high: hi });
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    if response.clicked() {
        let Some(p) = response.interact_pointer_pos() else { return };
        if p.y < kv.rect.top() + 14.0 {
            run(app, Command::SetFrame { frame: kv.time(p.x).round() as i32 });
            return;
        }
        match tool {
            TvTool::AddKeys => {
                // The nearest shown curve at the click gets the key.
                let t = kv.time(p.x);
                let mut best: Option<(f32, TrackPath, u32)> = None;
                for tr in shown {
                    if let Some(pp) = tracks::read(&app.state.scene, tr) {
                        for (c, v) in pp.comps_at(t).into_iter().enumerate() {
                            let d = (kv.y(v) - p.y).abs();
                            if best.as_ref().is_none_or(|b| d < b.0) {
                                best = Some((d, tr.clone(), c as u32));
                            }
                        }
                    }
                }
                if let Some((_, tr, c)) = best {
                    let time = if app.state.track_view.snap_frames { t.round() } else { t };
                    run(app, Command::TrackViewInsertKey { id: tr.node.0, track: tr.name.clone(), time, value: Some(kv.value(p.y)), comp: Some(c) });
                }
            }
            _ => match hits.iter().find(|h| h.0.distance(p) < 6.0) {
                Some(h) => select_keys(app, vec![(h.1.clone(), h.2, h.3)], if mods.ctrl { SelectOp::Add } else { SelectOp::Replace }),
                None if !mods.ctrl => select_keys(app, Vec::new(), SelectOp::Replace),
                None => {}
            },
        }
    }
    if response.secondary_clicked() {
        if let Some(p) = response.interact_pointer_pos() {
            if let Some(h) = hits.iter().find(|h| h.0.distance(p) < 6.0) {
                select_keys(app, vec![(h.1.clone(), h.2, h.3)], SelectOp::Replace);
                run(app, Command::PropertiesTrackView);
            }
        }
    }
}

fn dope_sheet(ui: &mut egui::Ui, app: &mut LoomApp, response: &egui::Response, painter: &egui::Painter) {
    let rect = response.rect;
    painter.rect_filled(rect, 0.0, egui::Color32::from_gray(48));
    let tv = app.state.track_view.clone();
    let kv = KeyView { rect, view: [tv.view[0], tv.view[1], 0.0, 1.0] };
    ruler(painter, &kv, app.state.current_frame, true);
    let rows = tree(&app.state);
    let row_h = ROW_H;
    let top = rect.top() + RULER_H;
    if let Some((s, e)) = tv.time_selection {
        let r = egui::Rect::from_min_max(egui::pos2(kv.x(s), top), egui::pos2(kv.x(e), rect.bottom()));
        painter.rect_filled(r, 0.0, egui::Color32::from_rgba_unmultiplied(220, 200, 150, 50));
        painter.line_segment([r.left_top(), r.left_bottom()], egui::Stroke::new(1.0_f32, egui::Color32::YELLOW));
        painter.line_segment([r.right_top(), r.right_bottom()], egui::Stroke::new(1.0_f32, egui::Color32::YELLOW));
    }
    let sel = tv.selected_keys.clone();
    let mut hits: Vec<(egui::Pos2, TrackPath, u32, f32)> = Vec::new();
    for (ri, r) in rows.iter().enumerate().skip(tv.scroll) {
        let y = top + (ri - tv.scroll) as f32 * row_h;
        if y > rect.bottom() {
            break;
        }
        let band = egui::Rect::from_min_size(egui::pos2(rect.left(), y), egui::vec2(rect.width(), row_h - 1.0));
        if tv.highlighted.contains(&r.id) {
            painter.rect_filled(band, 0.0, egui::Color32::from_gray(62));
        }
        let paths: Vec<TrackPath> = match &r.kind {
            TvItemKind::Track(t) => vec![t.clone()],
            TvItemKind::Object(id) => tracks::list_tracks(&app.state.scene, *id),
            TvItemKind::Group(id, g) => tracks::list_tracks(&app.state.scene, *id).into_iter().filter(|t| matches!((t.owner, *g), (loom_scene::tracks::TrackOwner::Transform | loom_scene::tracks::TrackOwner::Constraints, "Transform") | (loom_scene::tracks::TrackOwner::Object, "Object") | (loom_scene::tracks::TrackOwner::Modifier(_), "Modified Object"))).collect(),
            TvItemKind::Note(i) => {
                if let Some(nt) = app.state.scene.track_data.note_tracks.get(*i) {
                    for k in &nt.keys {
                        let p = egui::pos2(kv.x(k.time), y + row_h * 0.5);
                        painter.rect_filled(egui::Rect::from_center_size(p, egui::vec2(6.0, row_h - 6.0)), 0.0, egui::Color32::from_rgb(230, 230, 120));
                        painter.text(p + egui::vec2(6.0, 0.0), egui::Align2::LEFT_CENTER, k.text.lines().next().unwrap_or(""), egui::FontId::proportional(10.0), egui::Color32::WHITE);
                    }
                }
                Vec::new()
            }
        };
        let mut times: Vec<(f32, bool)> = Vec::new();
        let mut range: Option<(f32, f32)> = None;
        for t in &paths {
            let Some(p) = tracks::read(&app.state.scene, t) else { continue };
            let Some(k) = p.keys() else { continue };
            for (c, tr) in k.tracks.iter().enumerate() {
                for key in &tr.keys {
                    let is_sel = sel.iter().any(|s| s.track == t.key() && s.comp == c as u32 && (s.time - key.time).abs() < 1e-3);
                    times.push((key.time, is_sel));
                    range = Some(range.map_or((key.time, key.time), |(a, b)| (a.min(key.time), b.max(key.time))));
                    if matches!(r.kind, TvItemKind::Track(_)) {
                        hits.push((egui::pos2(kv.x(key.time), y + row_h * 0.5), t.clone(), c as u32, key.time));
                    }
                }
            }
        }
        if tv.dope == DopeMode::EditRanges {
            if let Some((a, b)) = range {
                let bar = egui::Rect::from_min_max(egui::pos2(kv.x(a), y + 4.0), egui::pos2(kv.x(b).max(kv.x(a) + 2.0), y + row_h - 5.0));
                painter.rect_filled(bar, 2.0, egui::Color32::from_rgb(110, 110, 150));
            }
            continue;
        }
        for (t, s) in times {
            let p = egui::pos2(kv.x(t), y + row_h * 0.5);
            let c = if s { egui::Color32::WHITE } else if matches!(r.kind, TvItemKind::Object(_)) { egui::Color32::from_gray(150) } else { egui::Color32::from_rgb(200, 90, 90) };
            painter.rect_filled(egui::Rect::from_center_size(p, egui::vec2(7.0, row_h - 6.0)), 0.0, c);
        }
    }
    dope_input(ui, app, response, &kv, &hits, &rows, top, row_h);
}

#[allow(clippy::too_many_arguments)]
fn dope_input(ui: &mut egui::Ui, app: &mut LoomApp, response: &egui::Response, kv: &KeyView, hits: &[(egui::Pos2, TrackPath, u32, f32)], rows: &[loom_commands::track_view::TvItem], top: f32, row_h: f32) {
    let tool = app.state.track_view.tool;
    let dope = app.state.track_view.dope;
    let mods = ui.input(|i| i.modifiers);
    let (tpp, _) = kv.per_px();
    let scroll = app.state.track_view.scroll;
    let row_at = |y: f32| -> Option<String> {
        let i = ((y - top) / row_h).floor();
        (i >= 0.0).then(|| rows.get(i as usize + scroll).map(|r| r.id.clone())).flatten()
    };
    if response.drag_started() {
        let Some(p) = response.interact_pointer_pos() else { return };
        if ui.input(|i| i.pointer.middle_down()) || tool == TvTool::Pan {
            app.tv_drag = Some(TvDrag::Pan { last: p });
        } else if dope == DopeMode::EditRanges {
            if let Some(id) = row_at(p.y) {
                if !app.state.track_view.highlighted.contains(&id) {
                    run(app, Command::TrackViewHighlight { items: vec![id], op: SelectOp::Replace });
                }
                app.state.undo.begin("Move Range");
                app.tv_drag = Some(TvDrag::Range { start: p, applied: 0.0 });
            }
        } else if matches!(tool, TvTool::SelectTime | TvTool::InsertTime | TvTool::ScaleTime) || dope == DopeMode::EditTime {
            app.tv_drag = Some(TvDrag::Tool { start: p });
        } else if let Some(h) = hits.iter().find(|h| (h.0.x - p.x).abs() < 5.0 && (h.0.y - p.y).abs() < row_h * 0.5) {
            if !app.state.track_view.selected_keys.iter().any(|s| s.track == h.1.key() && s.comp == h.2 && (s.time - h.3).abs() < 1e-3) {
                select_keys(app, vec![(h.1.clone(), h.2, h.3)], if mods.ctrl { SelectOp::Add } else { SelectOp::Replace });
            }
            app.state.undo.begin("Move Keys");
            app.tv_drag = Some(TvDrag::Move { start: p, applied: (0.0, 0.0) });
        } else {
            app.tv_drag = Some(TvDrag::Region { start: p });
        }
    }
    if let Some(drag) = app.tv_drag {
        let Some(p) = ui.input(|i| i.pointer.interact_pos()) else { return };
        let painter = ui.painter_at(kv.rect);
        match drag {
            TvDrag::Pan { last } => {
                let v = app.state.track_view.view;
                let dx = (p.x - last.x) * tpp;
                run(app, Command::TrackViewSetView { start: v[0] - dx, end: v[1] - dx, low: v[2], high: v[3] });
                app.tv_drag = Some(TvDrag::Pan { last: p });
            }
            TvDrag::Move { start, applied } => {
                let mut dt = (p.x - start.x) * tpp;
                if app.state.track_view.snap_frames {
                    dt = dt.round();
                }
                if (dt - applied.0).abs() > 1e-4 {
                    run(app, Command::TrackViewMoveKeys { dt: dt - applied.0, dv: 0.0 });
                }
                app.tv_drag = Some(TvDrag::Move { start, applied: (dt, 0.0) });
            }
            TvDrag::Range { start, applied } => {
                let dt = ((p.x - start.x) * tpp).round();
                if (dt - applied).abs() > 1e-4 {
                    run(app, Command::TrackViewMoveRange { dt: dt - applied });
                }
                app.tv_drag = Some(TvDrag::Range { start, applied: dt });
            }
            TvDrag::Region { start } | TvDrag::Tool { start } => {
                painter.rect_stroke(egui::Rect::from_two_pos(start, p), 0.0, egui::Stroke::new(1.0_f32, egui::Color32::WHITE));
            }
            TvDrag::Handle { .. } => {}
        }
    }
    if response.drag_stopped() {
        let p = response.interact_pointer_pos().unwrap_or_default();
        match app.tv_drag.take() {
            Some(TvDrag::Move { .. } | TvDrag::Range { .. }) => app.state.undo.end(),
            Some(TvDrag::Region { start }) => {
                let r = egui::Rect::from_two_pos(start, p);
                let keys: Vec<(TrackPath, u32, f32)> = hits.iter().filter(|h| r.contains(h.0)).map(|h| (h.1.clone(), h.2, h.3)).collect();
                select_keys(app, keys, if mods.ctrl { SelectOp::Add } else { SelectOp::Replace });
            }
            Some(TvDrag::Tool { start }) => {
                let (a, b) = (kv.time(start.x), kv.time(p.x));
                let snap = |t: f32| t.round();
                match tool {
                    TvTool::InsertTime => run(app, Command::TrackViewInsertTime { at: snap(a.min(b)), amount: snap((b - a).abs()) }),
                    TvTool::ScaleTime => {
                        if let Some((s, e)) = app.state.track_view.time_selection {
                            let f = ((e - s) + (b - a)) / (e - s).max(1e-3);
                            if f > 0.0 {
                                run(app, Command::TrackViewScaleTime { factor: f });
                            }
                        }
                    }
                    _ => {
                        let ids: Vec<String> = [start.y, p.y].iter().filter_map(|y| row_at(*y)).collect();
                        if !ids.is_empty() && app.state.track_view.highlighted.is_empty() {
                            run(app, Command::TrackViewHighlight { items: ids, op: SelectOp::Replace });
                        }
                        run(app, Command::TrackViewSelectTime { start: snap(a.min(b)), end: snap(a.max(b)) });
                    }
                }
            }
            _ => {}
        }
    }
    if response.clicked() {
        let Some(p) = response.interact_pointer_pos() else { return };
        if p.y < top {
            run(app, Command::SetFrame { frame: kv.time(p.x).round() as i32 });
            return;
        }
        if tool == TvTool::AddKeys {
            if let Some(id) = row_at(p.y) {
                if let Some(TvItemKind::Track(t)) = rows.iter().find(|r| r.id == id).map(|r| r.kind.clone()) {
                    run(app, Command::TrackViewInsertKey { id: t.node.0, track: t.name.clone(), time: kv.time(p.x).round(), value: None, comp: None });
                } else if let Some(TvItemKind::Note(i)) = rows.iter().find(|r| r.id == id).map(|r| r.kind.clone()) {
                    run(app, Command::AddNoteKey { note: i as u32, time: kv.time(p.x).round(), text: String::new() });
                }
            }
            return;
        }
        match hits.iter().find(|h| (h.0.x - p.x).abs() < 5.0 && (h.0.y - p.y).abs() < row_h * 0.5) {
            Some(h) => select_keys(app, vec![(h.1.clone(), h.2, h.3)], if mods.ctrl { SelectOp::Add } else { SelectOp::Replace }),
            None => {
                if let Some(id) = row_at(p.y) {
                    run(app, Command::TrackViewHighlight { items: vec![id], op: if mods.ctrl { SelectOp::Add } else { SelectOp::Replace } });
                }
                if !mods.ctrl {
                    select_keys(app, Vec::new(), SelectOp::Replace);
                }
            }
        }
    }
}

fn bottom_bar(ui: &mut egui::Ui, app: &mut LoomApp) {
    ui.horizontal(|ui| {
        // Select By Name: wildcards over the visible hierarchy.
        let id = egui::Id::new("tv_select_name");
        let mut name: String = ui.ctx().data_mut(|d| d.get_temp_mut_or_default::<String>(id).clone());
        let focus = app.state.track_view.focus_field.clone();
        let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(120.0).hint_text("Select By Name"));
        if focus.as_deref() == Some("hierarchy name") {
            r.request_focus();
            app.state.track_view.focus_field = None;
        }
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !name.is_empty() {
            let pat = name.to_lowercase();
            let wild = |s: &str| {
                let s = s.to_lowercase();
                match pat.split_once('*') {
                    Some((a, b)) => s.starts_with(a) && s.ends_with(b),
                    None => s.contains(&pat),
                }
            };
            let items: Vec<String> = tree(&app.state).into_iter().filter(|i| wild(&i.label)).map(|i| i.id).collect();
            run(app, Command::TrackViewHighlight { items, op: SelectOp::Replace });
        }
        ui.ctx().data_mut(|d| d.insert_temp(id, name));
        // Track sets.
        let sets: Vec<String> = app.state.scene.track_data.track_sets.iter().map(|(n, _)| n.clone()).collect();
        egui::ComboBox::from_id_salt("tv_track_sets").width(110.0).selected_text("Track Sets").show_ui(ui, |ui| {
            for s in sets {
                if ui.selectable_label(false, &s).clicked() {
                    let op = if ui.input(|i| i.modifiers.ctrl) { SelectOp::Add } else if ui.input(|i| i.modifiers.alt) { SelectOp::Subtract } else { SelectOp::Replace };
                    run(app, Command::TrackSetSelect { name: s, op });
                }
            }
        });
        if ui.small_button("Edit").on_hover_text("Edit Track Set").clicked() {
            run(app, Command::EditTrackSet);
        }
        ui.separator();
        // Key Stats: the selected key's time and value (Access Time / Value Field).
        if let Some((_, _, _, key, _)) = loom_commands::track_view::first_selected_key(&app.state) {
            let (mut t, mut v) = (key.time, key.value);
            ui.label("Time:");
            let rt = ui.add(egui::DragValue::new(&mut t).speed(1.0));
            if focus.as_deref() == Some("time") {
                rt.request_focus();
                app.state.track_view.focus_field = None;
            }
            if rt.changed() {
                run(app, Command::TrackViewSetKeyValues { time: Some(t), value: None, relative: Some(false) });
            }
            ui.label("Value:");
            let rv = ui.add(egui::DragValue::new(&mut v).speed(0.1));
            if focus.as_deref() == Some("value") {
                rv.request_focus();
                app.state.track_view.focus_field = None;
            }
            if rv.changed() {
                run(app, Command::TrackViewSetKeyValues { time: None, value: Some(v), relative: Some(false) });
            }
        } else {
            ui.label(format!("{} keys selected", app.state.track_view.selected_keys.len()));
        }
        ui.separator();
        let mut name = app.state.track_view.name.clone();
        let rn = ui.add(egui::TextEdit::singleline(&mut name).desired_width(100.0));
        if focus.as_deref() == Some("track view name") {
            rn.request_focus();
            app.state.track_view.focus_field = None;
        }
        app.state.track_view.name = name;
    });
}
