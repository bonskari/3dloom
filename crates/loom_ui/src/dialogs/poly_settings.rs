//! Editable Poly settings windows (spinners, Apply / OK /
//! Cancel, live preview on the current selection).

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, Dialog, ExtrudeKind, PolySettings, PolySettingsKind, SubObjectId};
use loom_scene::NodeId;

pub fn show(ctx: &egui::Context, app: &mut LoomApp, kind: PolySettingsKind, open: &mut bool) {
    let dialog = Dialog::PolySettings(kind);
    let id = egui::Id::new("poly_settings").with(format!("{kind:?}"));
    let sel_id = id.with("selection");
    let mut st: PolySettings = ctx.data(|d| d.get_temp(id)).unwrap_or(app.state.poly_settings);
    if kind == PolySettingsKind::Hinge && st.hinge_edge != app.state.poly_settings.hinge_edge {
        st.hinge_edge = app.state.poly_settings.hinge_edge;
    }
    // Hinge needs its edge before anything can preview.
    let can_preview = kind != PolySettingsKind::Hinge || st.hinge_edge.is_some();
    // The selection the preview applies to (ops replace the sub-selection).
    let sel: Vec<(u64, SubObjectId)> = ctx.data(|d| d.get_temp(sel_id)).unwrap_or_else(|| app.state.sub_selection.iter().map(|(n, s)| (n.0, s)).collect());
    let applied = app.preview_dialog() == Some(dialog);
    let speed = app.spinner_speed(0.1);
    let mut changed = false;
    let (mut ok, mut apply, mut cancel) = (false, false, false);
    let title = match kind {
        PolySettingsKind::Weld => "Weld Vertices",
        PolySettingsKind::Chamfer => "Chamfer",
        PolySettingsKind::ConnectEdges => "Connect Edges",
        PolySettingsKind::Bridge => "Bridge",
        PolySettingsKind::Extrude => "Extrude",
        PolySettingsKind::Inset => "Inset Polygons",
        PolySettingsKind::Bevel => "Bevel Polygons",
        PolySettingsKind::Outline => "Outline Polygons",
        PolySettingsKind::Hinge => "Hinge Polygons From Edge",
        PolySettingsKind::MSmooth => "MeshSmooth Selection",
        PolySettingsKind::Tessellate => "Tessellate Selection",
        PolySettingsKind::Relax => "Relax",
        PolySettingsKind::ExtrudeAlongSpline => "Extrude Polygons Along Spline",
    };
    egui::Window::new(title).id(id).open(open).resizable(false).show(ctx, |ui| {
        egui::Grid::new(id.with("grid")).num_columns(2).show(ui, |ui| {
            let f = |ui: &mut egui::Ui, label: &str, v: &mut f32, min: f32| -> bool {
                ui.label(label);
                let r = ui.add(egui::DragValue::new(v).speed(speed).range(min..=f32::MAX).fixed_decimals(3)).changed();
                ui.end_row();
                r
            };
            let u = |ui: &mut egui::Ui, label: &str, v: &mut u32| -> bool {
                ui.label(label);
                let r = ui.add(egui::DragValue::new(v).speed(0.1).range(1..=100)).changed();
                ui.end_row();
                r
            };
            match kind {
                PolySettingsKind::Weld => {
                    changed |= f(ui, "Weld Threshold:", &mut st.weld_threshold, 0.0);
                }
                PolySettingsKind::Chamfer => {
                    changed |= f(ui, "Chamfer Amount:", &mut st.chamfer_amount, 0.0);
                    changed |= u(ui, "Segments:", &mut st.chamfer_segments);
                    ui.label("");
                    changed |= ui.checkbox(&mut st.chamfer_open, "Open").changed();
                    ui.end_row();
                }
                PolySettingsKind::ConnectEdges => {
                    changed |= u(ui, "Segments:", &mut st.connect_segments);
                    ui.label("Pinch:");
                    changed |= ui.add(egui::DragValue::new(&mut st.connect_pinch).speed(1.0).range(-100.0..=100.0)).changed();
                    ui.end_row();
                    ui.label("Slide:");
                    changed |= ui.add(egui::DragValue::new(&mut st.connect_slide).speed(1.0).range(-100.0..=100.0)).changed();
                    ui.end_row();
                }
                PolySettingsKind::Bridge => {
                    changed |= u(ui, "Segments:", &mut st.bridge_segments);
                    ui.label("Smooth:");
                    changed |= ui.add(egui::DragValue::new(&mut st.bridge_smooth).speed(1.0).range(0.0..=180.0)).changed();
                    ui.end_row();
                    ui.label("Bridge Adjacent:");
                    changed |= ui.add(egui::DragValue::new(&mut st.bridge_adjacent).speed(1.0).range(0.0..=180.0)).changed();
                    ui.end_row();
                    ui.label("Twist 1:");
                    changed |= ui.add(egui::DragValue::new(&mut st.bridge_twist1).speed(0.1)).changed();
                    ui.end_row();
                    ui.label("Twist 2:");
                    changed |= ui.add(egui::DragValue::new(&mut st.bridge_twist2).speed(0.1)).changed();
                    ui.end_row();
                }
                PolySettingsKind::Inset => {
                    changed |= f(ui, "Inset Amount:", &mut st.inset_amount, 0.0);
                    ui.label("Inset Type:");
                    ui.vertical(|ui| {
                        changed |= ui.radio_value(&mut st.inset_by_polygon, false, "Group").changed();
                        changed |= ui.radio_value(&mut st.inset_by_polygon, true, "By Polygon").changed();
                    });
                    ui.end_row();
                }
                PolySettingsKind::Bevel => {
                    changed |= f(ui, "Height:", &mut st.bevel_height, f32::MIN);
                    changed |= f(ui, "Outline Amount:", &mut st.bevel_outline, f32::MIN);
                    ui.label("Bevel Type:");
                    ui.vertical(|ui| {
                        for (k, l) in [(ExtrudeKind::Group, "Group"), (ExtrudeKind::LocalNormal, "Local Normal"), (ExtrudeKind::ByPolygon, "By Polygon")] {
                            changed |= ui.radio_value(&mut st.bevel_kind, k, l).changed();
                        }
                    });
                    ui.end_row();
                }
                PolySettingsKind::Outline => {
                    changed |= f(ui, "Amount:", &mut st.outline_amount, f32::MIN);
                }
                PolySettingsKind::Hinge => {
                    ui.label("Angle:");
                    changed |= ui.add(egui::DragValue::new(&mut st.hinge_angle).speed(1.0).range(-360.0..=360.0).suffix("°")).changed();
                    ui.end_row();
                    changed |= u(ui, "Segments:", &mut st.hinge_segments);
                    ui.label("Hinge:");
                    let picking = app.state.pick_mode == Some(loom_commands::PickMode::HingeEdge);
                    let label = match st.hinge_edge {
                        Some((_, e)) => format!("Edge {e}"),
                        None => "Pick Hinge".to_owned(),
                    };
                    if ui.selectable_label(picking, label).on_hover_text("Click an edge in a viewport").clicked() {
                        let _ = app.dispatch(Command::PickHinge);
                    }
                    ui.end_row();
                }
                PolySettingsKind::MSmooth => {
                    ui.label("Smoothness:");
                    changed |= ui.add(egui::DragValue::new(&mut st.msmooth_smoothness).speed(0.01).range(0.0..=1.0).fixed_decimals(2)).changed();
                    ui.end_row();
                    ui.label("Separate By:");
                    ui.vertical(|ui| {
                        changed |= ui.checkbox(&mut st.msmooth_separate_by_smoothing, "Smoothing Groups").changed();
                        changed |= ui.checkbox(&mut st.msmooth_separate_by_materials, "Materials").changed();
                    });
                    ui.end_row();
                }
                PolySettingsKind::Tessellate => {
                    ui.label("Operation:");
                    ui.vertical(|ui| {
                        changed |= ui.radio_value(&mut st.tess_kind, loom_commands::TessKind::Edge, "Edge").changed();
                        changed |= ui.radio_value(&mut st.tess_kind, loom_commands::TessKind::Face, "Face").changed();
                    });
                    ui.end_row();
                    ui.label("Tension:");
                    changed |= ui.add(egui::DragValue::new(&mut st.tess_tension).speed(1.0).range(-100.0..=100.0)).changed();
                    ui.end_row();
                }
                PolySettingsKind::Relax => {
                    ui.label("Amount:");
                    changed |= ui.add(egui::DragValue::new(&mut st.relax_amount).speed(0.01).range(0.0..=1.0).fixed_decimals(2)).changed();
                    ui.end_row();
                    changed |= u(ui, "Iterations:", &mut st.relax_iterations);
                    ui.label("");
                    ui.vertical(|ui| {
                        changed |= ui.checkbox(&mut st.relax_hold_boundary, "Hold Boundary Points").changed();
                        changed |= ui.checkbox(&mut st.relax_hold_outer, "Hold Outer Points").changed();
                    });
                    ui.end_row();
                }
                PolySettingsKind::ExtrudeAlongSpline => {
                    let ss = app.state.spline_settings;
                    let mut segs = ss.extrude_segments;
                    ui.label("Segments:");
                    if ui.add(egui::DragValue::new(&mut segs).range(1..=100)).changed() {
                        let _ = app.dispatch(Command::SplineSettings { name: "Extrude Segments".into(), value: segs as f64 });
                        changed = true;
                    }
                    ui.end_row();
                    let mut taper = ss.extrude_taper;
                    ui.label("Taper Amount:");
                    if ui.add(egui::DragValue::new(&mut taper).speed(0.01).range(-1.0..=10.0).fixed_decimals(2)).changed() {
                        let _ = app.dispatch(Command::SplineSettings { name: "Extrude Taper".into(), value: taper as f64 });
                        changed = true;
                    }
                    ui.end_row();
                    let mut curve = ss.extrude_taper_curve;
                    ui.label("Taper Curve:");
                    if ui.add(egui::DragValue::new(&mut curve).speed(0.01).range(-10.0..=10.0).fixed_decimals(2)).changed() {
                        let _ = app.dispatch(Command::SplineSettings { name: "Extrude Taper Curve".into(), value: curve as f64 });
                        changed = true;
                    }
                    ui.end_row();
                    let mut twist = ss.extrude_twist;
                    ui.label("Twist:");
                    if ui.add(egui::DragValue::new(&mut twist).speed(1.0).suffix("°")).changed() {
                        let _ = app.dispatch(Command::SplineSettings { name: "Extrude Twist".into(), value: twist as f64 });
                        changed = true;
                    }
                    ui.end_row();
                    let mut align = ss.extrude_align;
                    ui.label("");
                    if ui.checkbox(&mut align, "Align to face normal").changed() {
                        let _ = app.dispatch(Command::SplineSettings { name: "Extrude Align".into(), value: if align { 1.0 } else { 0.0 } });
                        changed = true;
                    }
                    ui.end_row();
                    let mut rotation = ss.extrude_rotation;
                    ui.label("Rotation:");
                    if ui.add_enabled(align, egui::DragValue::new(&mut rotation).speed(1.0).suffix("°")).changed() {
                        let _ = app.dispatch(Command::SplineSettings { name: "Extrude Rotation".into(), value: rotation as f64 });
                        changed = true;
                    }
                    ui.end_row();
                    ui.label("Spline:");
                    let picking = app.state.pick_mode == Some(loom_commands::PickMode::ExtrudeAlongSpline);
                    let label = match ss.extrude_path.and_then(|p| app.state.scene.node(p)) {
                        Some(n) => n.name.clone(),
                        None => "Pick Spline".to_owned(),
                    };
                    if ui.selectable_label(picking, label).on_hover_text("Click a shape in a viewport").clicked() {
                        let _ = app.dispatch(Command::ExtrudeAlongSplineMode);
                    }
                    ui.end_row();
                }
                PolySettingsKind::Extrude => {
                    changed |= f(ui, "Extrusion Height:", &mut st.extrude_height, f32::MIN);
                    changed |= f(ui, "Extrusion Base Width:", &mut st.extrude_width, 0.0);
                    if app.state.selection_mode.is_face_level() {
                        ui.label("Extrusion Type:");
                        ui.vertical(|ui| {
                            for (k, l) in [(ExtrudeKind::Group, "Group"), (ExtrudeKind::LocalNormal, "Local Normal"), (ExtrudeKind::ByPolygon, "By Polygon")] {
                                changed |= ui.radio_value(&mut st.extrude_kind, k, l).changed();
                            }
                        });
                        ui.end_row();
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Apply").on_hover_text("Keep this result and start another with the same values").clicked() {
                apply = true;
            }
            if ui.button("OK").clicked() {
                ok = true;
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
    });
    let cmd = match kind {
        PolySettingsKind::Weld => Command::Weld { threshold: st.weld_threshold },
        PolySettingsKind::Chamfer => Command::Chamfer { amount: st.chamfer_amount, segments: st.chamfer_segments, open: st.chamfer_open },
        PolySettingsKind::ConnectEdges => Command::ConnectEdges { segments: st.connect_segments, pinch: st.connect_pinch, slide: st.connect_slide },
        PolySettingsKind::Bridge => Command::Bridge { segments: st.bridge_segments, smooth: st.bridge_smooth, bridge_adjacent: st.bridge_adjacent, twist1: st.bridge_twist1, twist2: st.bridge_twist2 },
        PolySettingsKind::Extrude => Command::Extrude { height: st.extrude_height, width: st.extrude_width, kind: st.extrude_kind },
        PolySettingsKind::Inset => Command::Inset { amount: st.inset_amount, by_polygon: st.inset_by_polygon },
        PolySettingsKind::Bevel => Command::Bevel { height: st.bevel_height, outline: st.bevel_outline, kind: st.bevel_kind },
        PolySettingsKind::Outline => Command::Outline { amount: st.outline_amount },
        PolySettingsKind::Hinge => Command::HingeFromEdge { angle: st.hinge_angle, segments: st.hinge_segments, edge: None },
        PolySettingsKind::MSmooth => Command::MSmooth { smoothness: st.msmooth_smoothness, separate_by_smoothing: st.msmooth_separate_by_smoothing, separate_by_materials: st.msmooth_separate_by_materials },
        PolySettingsKind::Tessellate => Command::TessellateFaces { kind: st.tess_kind, tension: st.tess_tension },
        PolySettingsKind::Relax => Command::Relax { amount: st.relax_amount, iterations: st.relax_iterations, hold_boundary: st.relax_hold_boundary, hold_outer: st.relax_hold_outer },
        // Extrude Along Spline previews once its spline is picked.
        PolySettingsKind::ExtrudeAlongSpline => {
            let ss = app.state.spline_settings;
            Command::ExtrudeAlongSpline { spline: ss.extrude_path.map(|p| p.0).unwrap_or(0), segments: ss.extrude_segments, taper: ss.extrude_taper, taper_curve: ss.extrude_taper_curve, twist: ss.extrude_twist, rotation: ss.extrude_rotation, align: ss.extrude_align }
        }
    };
    let can_preview = can_preview && (kind != PolySettingsKind::ExtrudeAlongSpline || app.state.spline_settings.extrude_path.is_some_and(|p| app.state.scene.node(p).is_some()));
    let restore = |app: &mut LoomApp, sel: &[(u64, SubObjectId)]| {
        app.state.sub_selection.clear();
        for (n, s) in sel {
            app.state.sub_selection.insert(NodeId(*n), *s);
        }
    };
    let run = |app: &mut LoomApp, sel: &[(u64, SubObjectId)]| {
        restore(app, sel);
        let _ = app.dispatch(cmd.clone());
    };
    if changed && can_preview && app.state.full_interactivity {
        app.preview_undo();
        app.preview_apply(dialog, |app| run(app, &sel));
    } else if !applied && !ok && !cancel && !apply && *open && can_preview {
        app.preview_apply(dialog, |app| run(app, &sel));
    }
    app.state.poly_settings = st;
    ctx.data_mut(|d| {
        d.insert_temp(id, st);
        d.insert_temp(sel_id, sel.clone());
    });
    if apply {
        // Apply: commit and immediately preview the same op on the new selection.
        app.preview_keep();
        let new_sel: Vec<(u64, SubObjectId)> = app.state.sub_selection.iter().map(|(n, s)| (n.0, s)).collect();
        ctx.data_mut(|d| d.insert_temp(sel_id, new_sel.clone()));
        app.preview_apply(dialog, |app| run(app, &new_sel));
    }
    if ok {
        if !app.state.full_interactivity && !applied {
            app.preview_apply(dialog, |app| run(app, &sel));
        }
        app.preview_keep();
        *open = false;
    }
    if cancel || (!*open && !ok) {
        app.preview_undo();
        restore(app, &sel);
        *open = false;
    }
}
