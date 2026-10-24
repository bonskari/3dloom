//! Icon buttons: each is found by its glyph inside its panel, shows the
//! command's tooltip (chord included) when hovered, and runs the command when clicked.

use super::Harness;
use crate::command_panel::{create::Category, Tab};
use crate::icons;
use eframe::egui;
use loom_commands::{Command, PrimitiveKind, Tool};

/// The screen rect of a panel ("main_toolbar", "statusbar", "command_panel", "time_slider").
fn panel(h: &Harness, id: &str) -> egui::Rect {
    egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new(id)).unwrap_or_else(|| panic!("no {id} panel")).rect
}

/// The one icon `glyph` shown inside `area`.
fn icon_in(h: &Harness, glyph: &str, area: egui::Rect) -> egui::Pos2 {
    let all: Vec<egui::Pos2> = h.text_centers(glyph).into_iter().filter(|p| area.contains(*p)).collect();
    assert_eq!(all.len(), 1, "icon {glyph:?} in {area:?} {} times", all.len());
    all[0]
}

/// Rests the pointer on `p` until the tooltip shows, and returns whether `tip` is shown.
fn tooltip_shows(h: &mut Harness, p: egui::Pos2, tip: &str) -> bool {
    // Arrive by a move: egui shows no tooltip while the last click is newer than the last movement.
    h.run(vec![egui::Event::PointerMoved(p + egui::vec2(3.0, 0.0))]);
    h.run(vec![egui::Event::PointerMoved(p)]);
    h.frames(25);
    !h.text_centers(tip).is_empty()
}

/// The texts painted last frame that start with `prefix` (for failure messages).
fn painted(h: &Harness, prefix: &str) -> Vec<String> {
    fn walk(shape: &egui::Shape, prefix: &str, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, prefix, out)),
            egui::Shape::Text(t) if t.galley.text().starts_with(prefix) => out.push(t.galley.text().to_owned()),
            _ => {}
        }
    }
    let mut out = Vec::new();
    h.shapes.iter().for_each(|s| walk(&s.shape, prefix, &mut out));
    out
}

/// Hovers the icon (checking its tooltip), then clicks it.
fn click_icon(h: &mut Harness, panel_id: &str, glyph: &str, tip: &str) {
    let p = icon_in(h, glyph, panel(h, panel_id));
    assert!(tooltip_shows(h, p, tip), "hovering {glyph:?} in {panel_id} shows {tip:?}; painted: {:?}; hit: {}", painted(h, tip.split(' ').next().unwrap_or("")), h.app.ui_hit(&h.ctx, p));
    h.click(p);
}

#[test]
fn the_icon_font_is_loaded_with_the_ui() {
    let h = Harness::new();
    let font = egui::FontId::proportional(icons::SIZE);
    h.ctx.fonts(|f| {
        assert!(f.lock().fonts.definitions().font_data.contains_key(icons::FONT), "the icon font is installed by the first frame");
        for g in [icons::UNDO, icons::MOVE, icons::TAB_MODIFY, icons::GEOMETRY, icons::LOCKED, icons::ORBIT, icons::PLAY, icons::SETTINGS, icons::FILTERS, icons::HIDE] {
            assert!(f.has_glyphs(&font, g), "{g:?} has a glyph");
        }
    });
    // The toolbar draws its icons, not words.
    let bar = panel(&h, "main_toolbar");
    for g in [icons::UNDO, icons::LINK, icons::SELECT_OBJECT, icons::MOVE, icons::ROTATE, icons::SCALE, icons::MIRROR, icons::ALIGN, icons::LAYERS, icons::CURVE_EDITOR, icons::MATERIAL_EDITOR] {
        icon_in(&h, g, bar);
    }
    assert!(h.text_centers("Move").iter().all(|p| !bar.contains(*p)), "no Move label on the toolbar");
}

#[test]
fn main_toolbar_icons_run_their_commands() {
    let mut h = Harness::new();
    click_icon(&mut h, "main_toolbar", icons::ROTATE, "Select and Rotate (E); right-click: Transform Type-In");
    assert_eq!(h.app.state.active_tool, Tool::Rotate);
    click_icon(&mut h, "main_toolbar", icons::MOVE, "Select and Move (W); right-click: Transform Type-In");
    assert_eq!(h.app.state.active_tool, Tool::Move);
    click_icon(&mut h, "main_toolbar", icons::SCALE, "Select and Uniform Scale (R); right-click: Transform Type-In");
    assert_eq!(h.app.state.active_tool, Tool::Scale);
    click_icon(&mut h, "main_toolbar", icons::SELECT_OBJECT, "Select Object (Q)");
    assert_eq!(h.app.state.active_tool, Tool::Select);

    let snaps = h.app.state.snaps_on;
    let magnet = format!("{}3", icons::SNAP);
    click_icon(&mut h, "main_toolbar", &magnet, "Snaps Toggle (S); right-click: Grid and Snap Settings");
    assert_ne!(h.app.state.snaps_on, snaps);
    let angle = h.app.state.angle_snap_on;
    click_icon(&mut h, "main_toolbar", icons::ANGLE_SNAP, "Angle Snap Toggle (A); right-click: Grid and Snap Settings");
    assert_ne!(h.app.state.angle_snap_on, angle);
    let percent = h.app.state.percent_snap_on;
    click_icon(&mut h, "main_toolbar", icons::PERCENT_SNAP, "Percent Snap Toggle (Ctrl+Shift+P); right-click: Grid and Snap Settings");
    assert_ne!(h.app.state.percent_snap_on, percent);
    let spinner = h.app.state.prefs.spinner_snap;
    click_icon(&mut h, "main_toolbar", icons::SPINNER_SNAP, "Spinner Snap Toggle");
    assert_ne!(h.app.state.prefs.spinner_snap, spinner);

    let window = h.app.state.window_selection;
    let glyph = if window { icons::WINDOW } else { icons::CROSSING };
    let tip = if window { "Window Selection" } else { "Crossing Selection" };
    click_icon(&mut h, "main_toolbar", glyph, tip);
    assert_ne!(h.app.state.window_selection, window);

    click_icon(&mut h, "main_toolbar", icons::LINK, "Select and Link");
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::Link));
    click_icon(&mut h, "main_toolbar", icons::SELECT_OBJECT, "Select Object (Q)");

    click_icon(&mut h, "main_toolbar", icons::LAYERS, "Manage Layers (Layer Manager)");
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::LayerManager));
    click_icon(&mut h, "main_toolbar", icons::CURVE_EDITOR, "Curve Editor (Open)");
    assert!(h.app.state.track_view.open, "the Curve Editor opens");
    click_icon(&mut h, "main_toolbar", icons::MATERIAL_EDITOR, "Material Editor (M)");
    assert!(h.app.state.material_editor_open, "the Material Editor opens");
}

#[test]
fn main_toolbar_undo_and_redo_icons() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    click_icon(&mut h, "main_toolbar", icons::UNDO, "Undo Scene Operation (Ctrl+Z)");
    assert_eq!(h.app.state.scene.len(), 0, "the box is undone");
    click_icon(&mut h, "main_toolbar", icons::REDO, "Redo Scene Operation (Ctrl+Y)");
    assert_eq!(h.app.state.scene.len(), 1, "and redone");
}

#[test]
fn status_bar_icons_run_their_commands() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    click_icon(&mut h, "statusbar", icons::UNLOCKED, "Selection Lock Toggle (Space)");
    assert!(h.app.state.selection_locked);
    click_icon(&mut h, "statusbar", icons::LOCKED, "Selection Lock Toggle (Space)");
    assert!(!h.app.state.selection_locked);
    click_icon(&mut h, "statusbar", icons::ABSOLUTE, "Absolute Mode Transform Type-In");
    assert!(h.app.state.offset_mode);

    click_icon(&mut h, "statusbar", icons::PAN, "Pan View (Ctrl+P)");
    assert_eq!(h.app.state.nav_mode, loom_input::NavMode::Pan);
    click_icon(&mut h, "statusbar", icons::ZOOM, "Zoom Mode (Alt+Z)");
    assert_eq!(h.app.state.nav_mode, loom_input::NavMode::Zoom);
    let max = h.app.state.maximized;
    click_icon(&mut h, "statusbar", icons::MAXIMIZE, "Maximize Viewport Toggle (Alt+W)");
    assert_ne!(h.app.state.maximized, max);

    click_icon(&mut h, "statusbar", icons::GO_TO_END, "Go to End Frame (End)");
    assert_eq!(h.app.state.current_frame, h.app.state.frame_range.1);
    click_icon(&mut h, "statusbar", icons::GO_TO_START, "Go to Start Frame (Home)");
    assert_eq!(h.app.state.current_frame, h.app.state.frame_range.0);
    click_icon(&mut h, "statusbar", icons::NEXT_FRAME, "Next Frame (Period)");
    assert_eq!(h.app.state.current_frame, h.app.state.frame_range.0 + 1);
    click_icon(&mut h, "statusbar", icons::KEY_MODE, "Key Mode Toggle");
    assert!(h.app.state.key_step_mode);
    click_icon(&mut h, "statusbar", icons::PLAY, "Play / Stop Animation (Slash)");
    assert!(h.app.state.playing);
    click_icon(&mut h, "statusbar", icons::STOP, "Play / Stop Animation (Slash)");
    assert!(!h.app.state.playing);
}

#[test]
fn time_slider_arrows_step_a_frame() {
    let mut h = Harness::new();
    let f = h.app.state.current_frame;
    click_icon(&mut h, "time_slider", icons::NEXT_FRAME, "Forward Time One Unit (Period)");
    assert_eq!(h.app.state.current_frame, f + 1);
    click_icon(&mut h, "time_slider", icons::PREVIOUS_FRAME, "Backup Time One Unit (Comma)");
    assert_eq!(h.app.state.current_frame, f);
}

#[test]
fn command_panel_tab_and_category_icons() {
    let mut h = Harness::new();
    click_icon(&mut h, "command_panel", icons::TAB_MODIFY, "Modify");
    assert_eq!(h.app.panel_tab, Tab::Modify);
    click_icon(&mut h, "command_panel", icons::TAB_CREATE, "Create");
    assert_eq!(h.app.panel_tab, Tab::Create);
    click_icon(&mut h, "command_panel", icons::SHAPES, "Shapes");
    assert_eq!(h.app.create_category, Category::Shapes);
    // Scene tab: the object's kind icon precedes its name.
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    click_icon(&mut h, "command_panel", icons::TAB_SCENE, "Scene");
    assert_eq!(h.app.panel_tab, Tab::Scene);
    icon_in(&h, icons::KIND_GEOMETRY, panel(&h, "command_panel"));
}

#[test]
fn modifier_stack_icons_run_their_commands() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    h.app.panel_tab = Tab::Modify;
    h.frames(2);
    click_icon(&mut h, "command_panel", icons::PIN_STACK, "Pin Stack");
    assert!(h.app.state.pin_stack);
    let end = h.app.state.show_end_result;
    click_icon(&mut h, "command_panel", icons::SHOW_END_RESULT, "Show End Result On/Off Toggle");
    assert_ne!(h.app.state.show_end_result, end);
}

