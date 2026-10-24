//! Editable Poly > Selection rollout: level buttons, By Vertex, Ignore
//! Backfacing, By Angle, Shrink / Grow, Ring / Loop with shift spinners,
//! Preview Selection and the selection readout.

use crate::command_panel::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, PreviewSelect, SelectionMode};

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    rollout(ui, "Selection", true, |ui| {
        ui.horizontal_wrapped(|ui| {
            for mode in SelectionMode::ALL {
                let selected = app.state.selection_mode == mode;
                let r = ui.selectable_label(selected, mode.label()).on_hover_text(format!("{}; Ctrl-click converts touching, Shift-click contained", crate::shortcuts::tip(&app.state, mode.label(), crate::shortcuts::level_action(mode))));
                if r.clicked() {
                    let mods = ui.input(|i| i.modifiers);
                    if (mods.ctrl || mods.shift) && app.state.selection_mode != SelectionMode::Object && !selected {
                        let _ = app.dispatch(Command::ConvertSelection { to: mode, contained: mods.shift });
                    } else {
                        // Clicking the active level returns to the top level.
                        let mode = if selected { SelectionMode::Object } else { mode };
                        let _ = app.dispatch(Command::SelectionMode { mode });
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            let mut bv = app.state.select_by_vertex;
            if ui.checkbox(&mut bv, "By Vertex").changed() {
                let _ = app.dispatch(Command::SelectByVertex);
            }
            let mut ib = app.state.ignore_backfacing;
            if ui.checkbox(&mut ib, "Ignore Backfacing").changed() {
                let _ = app.dispatch(Command::IgnoreBackfacingInSelections);
            }
        });
        ui.horizontal(|ui| {
            let mut by_angle = app.by_angle.is_some();
            if ui.checkbox(&mut by_angle, "By Angle:").changed() {
                app.by_angle = if by_angle { Some(45.0) } else { None };
            }
            let mut a = app.by_angle.unwrap_or(45.0);
            if ui.add_enabled(by_angle, crate::spinner::spin(ui, &mut a, 0.0..=180.0).speed(app.spinner_speed(0.5)).fixed_decimals(1)).changed() {
                app.by_angle = Some(a);
            }
        });
        let sub = app.state.selection_mode != SelectionMode::Object;
        ui.add_enabled_ui(sub, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Shrink").on_hover_text(crate::shortcuts::name_tip(&app.state, "Shrink Selection")).clicked() {
                    let _ = app.dispatch(Command::ShrinkSelection);
                }
                if ui.button("Grow").on_hover_text(crate::shortcuts::name_tip(&app.state, "Grow Selection")).clicked() {
                    let _ = app.dispatch(Command::GrowSelection);
                }
            });
            let edges = app.state.selection_mode.is_edge_level();
            ui.add_enabled_ui(edges, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Ring").on_hover_text(crate::shortcuts::name_tip(&app.state, "Select Edge Ring")).clicked() {
                        let _ = app.dispatch(Command::SelectEdgeRing);
                    }
                    if ui.small_button("^").on_hover_text("Ring Up (Ctrl: add, Alt: subtract)").clicked() {
                        let m = ui.input(|i| i.modifiers);
                        let _ = app.dispatch(if m.ctrl { Command::SelectEdgeRingUpAdd } else if m.alt { Command::SelectEdgeRingUpSubtract } else { Command::SelectEdgeRingUp });
                    }
                    if ui.small_button("v").on_hover_text("Ring Down (Ctrl: add, Alt: subtract)").clicked() {
                        let m = ui.input(|i| i.modifiers);
                        let _ = app.dispatch(if m.ctrl { Command::SelectEdgeRingDownAdd } else if m.alt { Command::SelectEdgeRingDownSubtract } else { Command::SelectEdgeRingDown });
                    }
                    if ui.button("Loop").on_hover_text(crate::shortcuts::name_tip(&app.state, "Select Edge Loop")).clicked() {
                        let _ = app.dispatch(Command::SelectEdgeLoop);
                    }
                    if ui.small_button("^").on_hover_text("Loop Up (Ctrl: add, Alt: subtract)").clicked() {
                        let m = ui.input(|i| i.modifiers);
                        let _ = app.dispatch(if m.ctrl { Command::SelectEdgeLoopUpAdd } else if m.alt { Command::SelectEdgeLoopUpSubtract } else { Command::SelectEdgeLoopUp });
                    }
                    if ui.small_button("v").on_hover_text("Loop Down (Ctrl: add, Alt: subtract)").clicked() {
                        let m = ui.input(|i| i.modifiers);
                        let _ = app.dispatch(if m.ctrl { Command::SelectEdgeLoopDownAdd } else if m.alt { Command::SelectEdgeLoopDownSubtract } else { Command::SelectEdgeLoopDown });
                    }
                });
            });
        });
        // Label above the radios, as in Max: on one row they widened the panel.
        ui.label("Preview Selection");
        ui.horizontal(|ui| {
            for (p, l) in [(PreviewSelect::Off, "Off"), (PreviewSelect::SubObj, "SubObj"), (PreviewSelect::Multi, "Multi")] {
                if ui.radio(app.state.preview_selection == p, l).clicked() && app.state.preview_selection != p {
                    while app.state.preview_selection != p {
                        let _ = app.dispatch(Command::PreviewSelectMode);
                    }
                }
            }
        });
        let n = app.state.sub_selection.len();
        // One sub-object: its number, 1-based as the reference counts them.
        let single = if n == 1 { app.state.sub_selection.iter().next() } else { None };
        let numbered = single.and_then(|(node, id)| {
            let mesh = app.state.scene.node(node)?.cage_mesh().into_owned();
            Some(match (app.state.selection_mode, id) {
                (SelectionMode::Vertex, loom_commands::SubObjectId::Vertex(v)) => format!("Vertex {} Selected", v.0 + 1),
                (SelectionMode::Edge | SelectionMode::Border, loom_commands::SubObjectId::Edge(e)) => format!("Edge {} Selected", mesh.edge_number(e)?),
                (SelectionMode::Face, loom_commands::SubObjectId::Face(f)) => format!("Polygon {} Selected", f.0 + 1),
                _ => return None,
            })
        });
        let what = if let Some(s) = numbered { s } else { match app.state.selection_mode {
            SelectionMode::Object => String::new(),
            SelectionMode::Vertex => format!("{n} Vertices Selected"),
            SelectionMode::Edge | SelectionMode::Border => format!("{n} Edges Selected"),
            SelectionMode::Face => format!("{n} Polygons Selected"),
            SelectionMode::Element => format!("{n} Elements Selected"),
        } };
        if !what.is_empty() {
            ui.label(what);
        }
    });
}
