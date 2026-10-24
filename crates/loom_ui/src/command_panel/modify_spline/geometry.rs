//! Editable Spline > Geometry rollout, in order: New Vertex Type,
//! Create Line / Break / Attach / Attach Mult. / Reorient, Refine (Connect,
//! Linear), End Point Auto-Welding, Weld / Connect / Insert, Make First /
//! Fuse / Reverse / Cycle, Fillet / Chamfer, Outline (Center), Boolean,
//! Mirror, Tangent Copy / Paste, Hide / Unhide All, Delete / Close /
//! Divide / Detach (Same Shp / Reorient / Copy), Explode (Splines / Objects),
//! Cross Section (Linear / Bezier / Bezier Corner / Smooth), Cross Insert,
//! Trim / Extend, Bind / Unbind.

use super::{setting_check, setting_spin};
use crate::command_panel::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, PickMode, SelectionMode};

fn pick(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, hover: &str, mode: PickMode, cmd: Command) {
    let on = app.state.pick_mode == Some(mode);
    if ui.selectable_label(on, label).on_hover_text(hover).clicked() {
        let _ = app.dispatch(cmd);
    }
}

/// Flat indices of the selected vertices.
fn selected_knots(app: &LoomApp) -> Vec<u32> {
    app.state.sub_selection.iter().filter_map(|(_, s)| match s { loom_commands::SubObjectId::Vertex(v) => Some(v.0), _ => None }).collect()
}

fn act(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, hover: &str, cmd: Command) {
    if ui.button(label).on_hover_text(hover).clicked() {
        let _ = app.dispatch(cmd);
    }
}

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    rollout(ui, "Geometry", true, |ui| {
        let ss = app.state.spline_settings;
        let level = app.state.selection_mode;
        let vertex = level == SelectionMode::Vertex;
        let segment = matches!(level, SelectionMode::Edge | SelectionMode::Border);
        let spline = matches!(level, SelectionMode::Face | SelectionMode::Element);
        let has = level != SelectionMode::Object && !app.state.sub_selection.is_empty();
        ui.label("New Vertex Type:");
        ui.horizontal_wrapped(|ui| {
            for (i, l) in ["Linear", "Smooth", "Bezier", "Bezier Corner"].iter().enumerate() {
                if ui.radio(ss.new_vertex_type == i as u32, *l).clicked() {
                    let _ = app.dispatch(Command::SplineSettings { name: "New Vertex Type".into(), value: i as f64 });
                }
            }
        });
        ui.horizontal(|ui| {
            let line_on = app.create_kind == Some(crate::CreateKind::Line) && app.state.spline_create_line_target.is_some();
            if ui.selectable_label(line_on, "Create Line").on_hover_text("Draw a line that joins this shape").clicked() {
                if line_on {
                    app.create_kind = None;
                    app.state.spline_create_line_target = None;
                } else {
                    let _ = app.dispatch(Command::CreateLineSpline);
                    app.create_kind = Some(crate::CreateKind::Line);
                }
            }
            ui.add_enabled_ui(vertex && has, |ui| act(ui, app, "Break", "Break the spline at the selected vertices", Command::BreakVerticesSpline));
        });
        ui.horizontal(|ui| {
            let on = app.state.pick_mode == Some(PickMode::SplineAttach);
            if ui.selectable_label(on, "Attach").on_hover_text("Attach: click a shape").clicked() {
                let _ = app.dispatch(Command::AttachSpline);
            }
            act(ui, app, "Attach Mult.", "Attach the other selected shapes", Command::AttachMultipleSpline);
            setting_check(ui, app, "Reorient", "Attach Reorient", ss.attach_reorient);
        });
        ui.horizontal(|ui| {
            let on = app.state.pick_mode == Some(PickMode::SplineRefine);
            if ui.selectable_label(on, "Refine").on_hover_text("Refine: click a segment to add a vertex").clicked() {
                let _ = app.dispatch(Command::RefineSpline);
            }
            setting_check(ui, app, "Connect", "Refine Connect", ss.refine_connect);
            setting_check(ui, app, "Linear", "Refine Linear", ss.refine_linear);
        });
        ui.horizontal(|ui| {
            setting_check(ui, app, "End Point Auto-Welding", "End Point Auto-Welding", ss.end_point_auto_weld);
            setting_spin(ui, app, "Auto-Weld Threshold", ss.auto_weld_threshold, 0.1, 0.0);
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(vertex && has, |ui| act(ui, app, "Weld", "Weld the selected vertices within the threshold", Command::WeldVerticesSpline));
            setting_spin(ui, app, "Weld Threshold", ss.weld_threshold, 0.01, 0.0);
            ui.add_enabled_ui(vertex && has, |ui| act(ui, app, "Connect", "Connect two end vertices", Command::ConnectSpline));
        });
        ui.horizontal(|ui| {
            let on = app.state.pick_mode == Some(PickMode::SplineInsert);
            if ui.selectable_label(on, "Insert").on_hover_text("Insert: click a segment, then move the new vertex").clicked() {
                let _ = app.dispatch(Command::InsertSelectionSpline);
            }
            ui.add_enabled_ui(vertex && has, |ui| {
                act(ui, app, "Make First", "The selected vertex becomes first", Command::MakeFirstSpline);
                act(ui, app, "Fuse", "Move the selected vertices together", Command::FuseVerticesSpline);
            });
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(has, |ui| act(ui, app, "Reverse", "Reverse the spline direction", Command::ReverseSpline));
            ui.add_enabled_ui(vertex && has, |ui| act(ui, app, "Cycle", "Select the next vertex", Command::CycleVerticesSpline));
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(vertex && has, |ui| act(ui, app, "Fillet", "Round the selected vertices", Command::FilletSpline));
            setting_spin(ui, app, "Fillet", ss.fillet_amount, 0.05, 0.0);
            ui.add_enabled_ui(vertex && has, |ui| act(ui, app, "Chamfer", "Cut the selected vertices", Command::ChamferVertexSpline));
            setting_spin(ui, app, "Chamfer", ss.chamfer_amount, 0.05, 0.0);
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui((spline || segment) && has, |ui| act(ui, app, "Outline", "Offset copy of the selected splines", Command::OutlineSpline));
            setting_spin(ui, app, "Outline", ss.outline_amount, 0.05, f32::MIN);
            setting_check(ui, app, "Center", "Outline Center", ss.outline_center);
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(spline && has, |ui| {
                if ui.button("Boolean").on_hover_text("Boolean the two selected closed splines").clicked() {
                    let op = ["Union", "Subtract", "Intersect"][ss.boolean_op.min(2) as usize];
                    let flat: Vec<u32> = app.state.sub_selection.iter().filter_map(|(_, s)| match s { loom_commands::SubObjectId::Face(f) => Some(f.0), _ => None }).collect();
                    if flat.len() >= 2 {
                        let _ = app.dispatch(Command::SelectSub { node: None, ids: vec![flat[0]], op: loom_commands::SelectOp::Replace });
                        let _ = app.dispatch(Command::SplineBoolean { op: op.into(), other: flat[1] });
                    }
                }
            });
            for (i, l) in ["Union", "Subtract", "Intersect"].iter().enumerate() {
                if ui.radio(ss.boolean_op == i as u32, *l).clicked() {
                    let _ = app.dispatch(Command::SplineSettings { name: "Boolean".into(), value: i as f64 });
                }
            }
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(spline && has, |ui| {
                if ui.button("Mirror").on_hover_text("Mirror the selected splines").clicked() {
                    let _ = app.dispatch(match ss.mirror_axis { 0 => Command::MirrorHorizontallySpline, 1 => Command::MirrorVerticallySpline, _ => Command::MirrorBothSpline });
                }
            });
            for (i, l) in ["H", "V", "Both"].iter().enumerate() {
                if ui.radio(ss.mirror_axis == i as u32, *l).clicked() {
                    let _ = app.dispatch(Command::SplineSettings { name: "Mirror Axis".into(), value: i as f64 });
                }
            }
        });
        ui.horizontal(|ui| {
            setting_check(ui, app, "Copy", "Mirror Copy", ss.mirror_copy);
            setting_check(ui, app, "About Pivot", "Mirror About Pivot", ss.mirror_about_pivot);
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(spline && has, |ui| {
                if ui.button("Cross Section").on_hover_text("Connect the selected splines' vertices in selection order").clicked() {
                    let flat: Vec<u32> = app.state.sub_selection.iter().filter_map(|(_, s)| match s { loom_commands::SubObjectId::Face(f) => Some(f.0), _ => None }).collect();
                    let _ = app.dispatch(Command::SplineCrossSection { splines: flat, kind: ss.cross_section_kind });
                }
            });
        });
        ui.horizontal(|ui| {
            for (i, l) in ["Linear", "Smooth", "Bezier", "Bezier Corner"].iter().enumerate() {
                if ui.radio(ss.cross_section_kind == i as u32, *l).clicked() {
                    let _ = app.dispatch(Command::SplineSettings { name: "Cross Section Type".into(), value: i as f64 });
                }
            }
        });
        ui.horizontal(|ui| {
            pick(ui, app, "Cross Insert", "Cross Insert: click where two splines cross", PickMode::SplineCrossInsert, Command::CrossInsertSpline);
            setting_spin(ui, app, "Cross Insert Threshold", ss.cross_threshold, 0.01, 0.0);
        });
        ui.horizontal(|ui| {
            pick(ui, app, "Trim", "Trim: click the piece to remove up to the crossings", PickMode::SplineTrim, Command::TrimSpline);
            pick(ui, app, "Extend", "Extend: click near an open end to grow it to the next spline", PickMode::SplineExtend, Command::ExtendSpline);
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(vertex && has, |ui| {
                pick(ui, app, "Bind", "Bind: the selected end vertices follow the segment you click", PickMode::SplineBind, Command::BindSpline);
                act(ui, app, "Unbind", "Free the selected bound vertices", Command::UnbindSpline);
            });
        });
        ui.horizontal(|ui| {
            ui.label("Tangent:");
            let knots = selected_knots(app);
            ui.add_enabled_ui(vertex && knots.len() == 1, |ui| {
                if ui.button("Copy").on_hover_text("Copy the selected vertex's out handle").clicked() {
                    let _ = app.dispatch(Command::SplineTangentCopy { knot: knots[0], side_in: false });
                }
            });
            ui.add_enabled_ui(vertex && !knots.is_empty() && app.state.tangent_clipboard.is_some(), |ui| {
                if ui.button("Paste").on_hover_text("Paste the copied handle onto the selected vertices' out handles").clicked() {
                    for k in &knots {
                        let _ = app.dispatch(Command::SplineTangentPaste { knot: *k, side_in: false, length: ss.paste_length });
                    }
                }
            });
            setting_check(ui, app, "Paste Length", "Paste Length", ss.paste_length);
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(has, |ui| act(ui, app, "Hide", "Hide the selection's splines", Command::HideSpline));
            act(ui, app, "Unhide All", "Unhide every spline", Command::UnhideAllSpline);
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(has, |ui| {
                act(ui, app, "Delete", "Delete the selection", Command::DeleteObjects);
                act(ui, app, "Close", "Close the selected splines", Command::CloseSpline);
            });
            ui.add_enabled_ui(segment && has, |ui| act(ui, app, "Divide", "Add vertices along the selected segments", Command::DivideSpline));
            let mut n = ss.divide_count;
            if ui.add(egui::DragValue::new(&mut n).range(1..=100)).changed() {
                let _ = app.dispatch(Command::SplineSettings { name: "Divisions".into(), value: n as f64 });
            }
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui((segment || spline) && has, |ui| act(ui, app, "Detach", "Detach the selected segments / splines", Command::DetachSplineSpline));
            setting_check(ui, app, "Same Shp", "Detach Same Shape", ss.detach_same_shape);
            setting_check(ui, app, "Reorient", "Detach Reorient", ss.detach_reorient);
            setting_check(ui, app, "Copy", "Detach Copy", ss.detach_copy);
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(spline && has, |ui| act(ui, app, "Explode", "Explode the selected splines into segments", Command::ExplodeSpline));
            ui.label("To:");
            if ui.radio(!ss.explode_to_objects, "Splines").clicked() {
                let _ = app.dispatch(Command::SplineSettings { name: "Explode To Objects".into(), value: 0.0 });
            }
            if ui.radio(ss.explode_to_objects, "Objects").clicked() {
                let _ = app.dispatch(Command::SplineSettings { name: "Explode To Objects".into(), value: 1.0 });
            }
        });
        if vertex && has {
            ui.horizontal(|ui| {
                ui.label("Vertex Type:");
                for l in ["Corner", "Smooth", "Bezier", "Bezier Corner"] {
                    if ui.button(l).clicked() {
                        let _ = app.dispatch(Command::SetVertexType { kind: l.into() });
                    }
                }
            });
        }
    });
}
