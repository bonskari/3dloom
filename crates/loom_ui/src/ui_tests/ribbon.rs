//! The Graphite Modeling Tools ribbon, clicked as a person does: its
//! minimize states, Modify Mode and the stack buttons, the per-level panels,
//! Shift / Ctrl+click variants, the contextual panels, By Half and Branches.

use super::*;
use crate::ribbon::{MinimizeOption, RibbonState, RibbonTab};
use egui::accesskit::Role;

/// A wide window, so every panel of the maximized ribbon is on screen.
fn wide() -> Harness {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(3200.0, 1000.0));
    h.frames(2);
    h
}

/// A poly box in the Modify panel with the ribbon maximized.
fn modeling(h: &mut Harness) {
    poly_box(h);
    h.app.panel_tab = crate::command_panel::Tab::Modify;
    h.app.ribbon_state = RibbonState::Maximized;
    h.frames(2);
}

/// The ribbon's area this frame.
fn ribbon_rect(h: &Harness) -> egui::Rect {
    egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new("ribbon")).expect("the ribbon is open").rect
}

/// The ribbon button `name`, if the ribbon shows it.
fn tool(h: &Harness, name: &str) -> Option<egui::Rect> {
    let area = ribbon_rect(h).expand(2.0);
    h.find_all(Role::Button, name).into_iter().find(|r| area.contains_rect(*r))
}

/// Clicks the ribbon button `name` (else the topmost one, e.g. in an open panel).
fn press(h: &mut Harness, name: &str) {
    let r = tool(h, name).unwrap_or_else(|| h.find(Role::Button, name));
    h.click(r.center());
}

/// How many times the text shows in the ribbon (panel titles).
fn ribbon_text(h: &Harness, text: &str) -> usize {
    let area = ribbon_rect(h);
    h.text_centers(text).into_iter().filter(|p| area.contains(*p)).count()
}

fn faces(h: &Harness) -> usize {
    let id = *h.app.state.selection.iter().next().unwrap();
    h.app.state.scene.node(id).unwrap().cage_mesh().face_count()
}

fn verts(h: &Harness) -> usize {
    let id = *h.app.state.selection.iter().next().unwrap();
    h.app.state.scene.node(id).unwrap().cage_mesh().vertex_count()
}

#[test]
fn the_ribbon_starts_minimized_to_panel_titles_and_the_button_cycles_to_full() {
    let mut h = Harness::new();
    assert!(h.app.state.show_ribbon, "the ribbon is open by default");
    assert_eq!(h.app.ribbon_state, RibbonState::PanelTitles);
    assert!(h.has(Role::Button, "Polygon Modeling"), "the panel title shows");
    assert!(tool(&h, "Modify Mode").is_none(), "its tools do not");
    let mut seen = Vec::new();
    for _ in 0..4 {
        press(&mut h, "Minimize / Maximize");
        seen.push(h.app.ribbon_state);
    }
    assert_eq!(seen, [RibbonState::Tabs, RibbonState::Maximized, RibbonState::PanelButtons, RibbonState::PanelTitles]);
    // Minimized to tabs only the tab names show.
    press(&mut h, "Minimize / Maximize");
    assert!(!h.has(Role::Button, "Polygon Modeling"));
    // The drop-down picks one minimize state; the button then toggles it with the full ribbon.
    press(&mut h, "Minimize Options");
    press(&mut h, "Minimize to Panel Buttons");
    assert_eq!((h.app.ribbon_minimize, h.app.ribbon_state), (MinimizeOption::PanelButtons, RibbonState::PanelButtons));
    press(&mut h, "Minimize / Maximize");
    assert_eq!(h.app.ribbon_state, RibbonState::Maximized);
    assert!(tool(&h, "Modify Mode").is_some(), "the full ribbon shows the tools");
    press(&mut h, "Minimize / Maximize");
    assert_eq!(h.app.ribbon_state, RibbonState::PanelButtons);
}

#[test]
fn a_minimized_panel_opens_under_its_title() {
    let mut h = Harness::new();
    poly_box(&mut h);
    h.app.panel_tab = crate::command_panel::Tab::Modify;
    h.frames(2);
    press(&mut h, "Polygon Modeling");
    assert_eq!(h.app.ribbon_popup, Some("Polygon Modeling"));
    press(&mut h, "Vertex");
    assert_eq!(h.app.state.selection_mode, SelectionMode::Vertex, "a tool in the open panel works");
    // Moving away closes it.
    let away = h.app.viewport_rects[h.app.state.active_viewport].center();
    h.run(vec![egui::Event::PointerMoved(away)]);
    h.frames(2);
    assert_eq!(h.app.ribbon_popup, None);
    // Minimized to tabs a clicked tab shows its panels until the mouse leaves.
    h.app.ribbon_state = RibbonState::Tabs;
    h.frames(1);
    press(&mut h, "Selection");
    assert!(tool(&h, "Invert Axis").is_some(), "the Selection tab's panels show");
    h.run(vec![egui::Event::PointerMoved(away)]);
    h.frames(2);
    assert!(tool(&h, "Invert Axis").is_none());
}

#[test]
fn modify_mode_previous_modifier_and_show_end_result() {
    let mut h = wide();
    poly_box(&mut h);
    h.app.panel_tab = crate::command_panel::Tab::Create;
    h.app.ribbon_state = RibbonState::Maximized;
    h.frames(2);
    assert!(tool(&h, "Swift Loop").is_none(), "outside Modify mode only Polygon Modeling shows");
    press(&mut h, "Modify Mode");
    assert_eq!(h.app.panel_tab, crate::command_panel::Tab::Modify);
    assert!(tool(&h, "Swift Loop").is_some(), "Modify mode brings the full tool set");
    h.cmd(Command::AddModifier { name: "Symmetry".into() });
    assert!(tool(&h, "Swift Loop").is_none(), "a Symmetry entry limits the tools");
    assert!(tool(&h, "Symmetry").is_some(), "the wide button names the stack level");
    press(&mut h, "Previous Modifier");
    let id = *h.app.state.selection.iter().next().unwrap();
    assert_eq!(h.app.state.scene.node(id).unwrap().active_entry, None, "back on the Editable Poly");
    assert!(tool(&h, "Swift Loop").is_some());
    press(&mut h, "Next Modifier");
    assert_eq!(h.app.state.scene.node(id).unwrap().active_entry, Some(0));
    press(&mut h, "Previous Modifier");
    assert!(!h.app.state.show_end_result);
    press(&mut h, "Show End Result");
    assert!(h.app.state.show_end_result);
}

#[test]
fn the_level_panel_follows_the_sub_object_level() {
    let mut h = wide();
    modeling(&mut h);
    for (level, title, name) in [("Vertex", "Vertices", "Break"), ("Edge", "Edges", "Split"), ("Border", "Borders", "Cap"), ("Polygon", "Polygons", "Extrude On Spline"), ("Element", "Elements", "Inset")] {
        press(&mut h, level);
        assert_eq!(ribbon_text(&h, title), 1, "{level} level shows the {title} panel");
        assert!(tool(&h, name).is_some(), "{title} has {name}");
    }
    press(&mut h, "Polygon");
    assert!(tool(&h, "Connect").is_none(), "no Loops panel at Polygon level");
    press(&mut h, "Edge");
    assert!(tool(&h, "Connect").is_some(), "Loops > Connect at Edge level");
}

#[test]
fn shift_click_opens_settings_and_ctrl_click_remove_takes_the_vertices() {
    let mut h = wide();
    modeling(&mut h);
    press(&mut h, "Polygon");
    h.mods = egui::Modifiers::SHIFT;
    press(&mut h, "Extrude");
    h.mods = egui::Modifiers::NONE;
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::PolySettings(loom_commands::PolySettingsKind::Extrude)), "Shift+click opens Extrude's settings");
    assert_eq!(h.app.state.poly_drag, None, "and does not start the drag mode");
    h.app.state.dialog = None;
    // A loop across the box, then Ctrl+click Remove takes its vertices too.
    press(&mut h, "Edge");
    h.cmd(Command::SelectSub { node: None, ids: vec![0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::SelectEdgeRing);
    h.cmd(Command::ConnectEdges { segments: 1, pinch: 0.0, slide: 0.0 });
    assert_eq!(verts(&h), 12);
    h.mods = egui::Modifiers::CTRL;
    press(&mut h, "Remove");
    h.mods = egui::Modifiers::NONE;
    assert_eq!(verts(&h), 8, "Ctrl+click Remove is Clean Remove");
}

#[test]
fn ctrl_click_on_a_level_converts_the_selection() {
    let mut h = wide();
    modeling(&mut h);
    press(&mut h, "Vertex");
    h.cmd(Command::SelectSub { node: None, ids: vec![0], op: loom_commands::SelectOp::Replace });
    h.mods = egui::Modifiers::CTRL;
    press(&mut h, "Polygon");
    h.mods = egui::Modifiers::NONE;
    assert_eq!(h.app.state.selection_mode, SelectionMode::Face);
    assert_eq!(h.sub_count(), 3, "the three polygons touching the vertex");
}

#[test]
fn edit_panel_swift_loop_loop_mode_and_constraints() {
    let mut h = wide();
    modeling(&mut h);
    press(&mut h, "Edge");
    press(&mut h, "Swift Loop");
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::SwiftLoop));
    press(&mut h, "Swift Loop");
    assert_eq!(h.app.state.pick_mode, None);
    press(&mut h, "Loop Mode");
    assert!(h.app.state.loop_mode);
    press(&mut h, "Constrain To Edge");
    assert_eq!(h.app.state.poly_constraint, loom_commands::PolyConstraint::Edges);
    press(&mut h, "Constrain To Normal");
    assert_eq!(h.app.state.poly_constraint, loom_commands::PolyConstraint::Normals);
}

#[test]
fn soft_selection_and_use_nurms_open_their_panels() {
    let mut h = wide();
    modeling(&mut h);
    press(&mut h, "Vertex");
    assert_eq!(ribbon_text(&h, "Soft Selection"), 0);
    press(&mut h, "Use Soft Selection");
    assert_eq!(ribbon_text(&h, "Soft Selection"), 1, "the Soft Selection panel opens");
    // Right-click its Falloff spinner: 0.
    let r = h.spinner_after("Falloff");
    right_click(&mut h, r.center());
    let id = *h.app.state.selection.iter().next().unwrap();
    assert_eq!(h.app.state.scene.node(id).unwrap().edit_poly().unwrap().soft.falloff.get(), 0.0);
    press(&mut h, "Use NURMS");
    let sub = |h: &Harness| h.app.state.scene.node(id).unwrap().edit_poly().unwrap().subdiv.clone();
    assert!(sub(&h).use_nurms.get());
    let cage = sub(&h).show_cage;
    press(&mut h, "Show Cage");
    assert_eq!(sub(&h).show_cage, !cage, "the Use NURMS panel's Show Cage");
}

#[test]
fn selection_tab_by_half_selects_and_inverts() {
    let mut h = wide();
    modeling(&mut h);
    press(&mut h, "Polygon");
    h.app.ribbon_tab = RibbonTab::Selection;
    h.frames(2);
    press(&mut h, "Y");
    press(&mut h, "Select");
    assert_eq!(h.sub_count(), 1, "the polygon on the +Y side");
    let first: Vec<_> = h.app.state.sub_selection.iter().collect();
    press(&mut h, "Invert Axis");
    assert_eq!(h.sub_count(), 1);
    assert_ne!(h.app.state.sub_selection.iter().collect::<Vec<_>>(), first, "the -Y polygon instead");
}

#[test]
fn branches_shift_drag_grows_a_branch_from_the_selected_polygon() {
    let mut h = wide();
    modeling(&mut h);
    press(&mut h, "Polygon");
    let p = h.screen_of(top_face_point());
    h.click(p);
    assert_eq!(h.sub_count(), 1);
    h.app.ribbon_tab = RibbonTab::Freeform;
    h.frames(2);
    press(&mut h, "Branches");
    assert!(h.app.state.branches);
    let before = faces(&h);
    let undo = h.app.state.undo.len();
    h.mods = egui::Modifiers::SHIFT;
    h.drag(p, p + egui::vec2(40.0, -120.0));
    h.mods = egui::Modifiers::NONE;
    assert!(faces(&h) >= before + 4 * 4, "several segments of four sides each: {} -> {}", before, faces(&h));
    assert_eq!(h.app.state.undo.len(), undo + 1, "one undo entry per branch");
    assert_eq!(h.sub_count(), 1, "the selection is untouched by the drag");
}

#[test]
fn without_a_poly_object_the_ribbon_tools_are_inactive() {
    let mut h = wide();
    h.app.ribbon_state = RibbonState::Maximized;
    h.frames(2);
    press(&mut h, "Modify Mode");
    assert_ne!(h.app.panel_tab, crate::command_panel::Tab::Modify, "nothing selected: the panel is inactive");
}
