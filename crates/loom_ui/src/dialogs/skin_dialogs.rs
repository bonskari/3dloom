//! Skin dialogs: Weight Table, Painter Options, Paste to Multiple Bones,
//! Load Envelopes and Edit Angle Keys Curves. Every edit dispatches a command.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, SelectOp};

fn run(app: &mut LoomApp, cmd: Command) {
    if let Err(e) = app.dispatch(cmd) {
        app.state.status_prompt = e;
    }
}

/// Weight Table options (Options menu, the vertex drop-down, the set name being typed).
#[derive(Debug, Clone)]
struct TableOpts {
    /// 0 All Vertices, 1 Selected Vertices, 2 Selected Bone.
    shown: u8,
    affected_only: bool,
    attributes: bool,
    flip: bool,
    exclusions: bool,
    global: bool,
    set_ui: bool,
    mouse_up: bool,
    set_name: String,
    /// Update On Mouse Up: the cell being dragged (vertices, bone, value), applied on release.
    pending: Option<(Vec<u32>, u64, f32)>,
}

impl Default for TableOpts {
    fn default() -> Self {
        Self { shown: 0, affected_only: false, attributes: true, flip: false, exclusions: false, global: false, set_ui: false, mouse_up: false, set_name: String::new(), pending: None }
    }
}

/// One weight cell: drag / type the weight, right-click 0, Ctrl+right-click 1;
/// with Show Exclusions a box before it toggles the vertex's exclusion.
#[allow(clippy::too_many_arguments)]
fn cell(ui: &mut egui::Ui, app: &mut LoomApp, o: &mut TableOpts, targets: Vec<u32>, bone: u64, value: f32, excluded: Option<bool>, width: f32, height: f32) {
    if let Some(ex) = excluded {
        let mut x = ex;
        if ui.add_sized([14.0, height], egui::Checkbox::without_text(&mut x)).on_hover_text("Excluded").changed() {
            for v in &targets {
                run(app, Command::SkinSetExclusion { vertex: *v, bone, excluded: x });
            }
        }
    }
    let shown = match &o.pending {
        Some((t, b, w)) if *b == bone && *t == targets => *w,
        _ => value,
    };
    let mut x = shown;
    let r = ui.add_sized([width, height], crate::spinner::spin(ui, &mut x, 0.0..=1.0).speed(0.01).fixed_decimals(3));
    if o.mouse_up {
        // Update On Mouse Up: the drag shows its value and applies on release.
        if r.changed() {
            o.pending = Some((targets.clone(), bone, x));
        }
        if (r.drag_stopped() || (r.changed() && !r.dragged())) && o.pending.is_some() {
            if let Some((t, b, w)) = o.pending.take() {
                run(app, Command::SkinSetVertexWeights { vertices: t, bone: b, weight: w });
            }
        }
    } else {
        if r.drag_started() {
            app.state.undo.begin("Weight Table");
        }
        if r.changed() {
            run(app, Command::SkinSetVertexWeights { vertices: targets.clone(), bone, weight: x });
        }
        if r.drag_stopped() {
            app.state.undo.end();
        }
    }
    if r.secondary_clicked() {
        let one = ui.input(|i| i.modifiers.ctrl);
        run(app, Command::SkinSetVertexWeights { vertices: targets, bone, weight: if one { 1.0 } else { 0.0 } });
    }
}

/// Skin > Weight Table: vertices down the left and bones across the top
/// (Flip UI swaps them). Drag or type a weight; an edit on a selected row
/// sets every selected vertex; right-click enters 0, Ctrl+right-click 1.
pub fn weight_table(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let Ok(e) = loom_commands::skin::entry(&app.state) else {
        *open = false;
        return;
    };
    let id = egui::Id::new("weight_table_opts");
    let mut o: TableOpts = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
    let sel = loom_commands::skin::selection(&app.state);
    let sel_set: std::collections::BTreeSet<u32> = sel.iter().copied().collect();
    let current = app.state.skin_bone.or_else(|| e.data.bones.first().map(|b| b.node));
    let weights: Vec<Vec<(u32, f32)>> = (0..e.input.vertex_count()).map(|v| e.data.vertex_weights(v, e.data.mesh_bind.transform_point3(e.input.vertices()[v].position))).collect();
    let cur_index = e.data.bones.iter().position(|b| Some(b.node) == current).map(|i| i as u32);
    let rows: Vec<usize> = match o.shown {
        1 => sel.iter().map(|v| *v as usize).collect(),
        2 => (0..weights.len()).filter(|v| cur_index.is_some_and(|b| weights[*v].iter().any(|(x, w)| *x == b && *w > 0.0))).collect(),
        _ => (0..weights.len()).collect(),
    };
    let bones: Vec<(usize, String)> = e
        .data
        .bones
        .iter()
        .enumerate()
        .filter(|(i, _)| !o.affected_only || rows.iter().any(|v| weights[*v].iter().any(|(x, w)| *x as usize == *i && *w > 0.0)))
        .map(|(i, b)| (i, app.state.scene.node(b.node).map(|n| n.name.clone()).unwrap_or_default()))
        .collect();
    let patch_nv = loom_commands::skin::input_patch(&app.state).map(|p| p.verts.len());
    let window = egui::Window::new("Weight Table").resizable(true).default_width(560.0);
    crate::os_window::OsWindow::new("Weight Table", "Weight Table", [580.0, 420.0]).show(ctx, open, window, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("Edit", |ui| {
                for (label, cmd) in [("Copy", Command::CopyWeightsSkin), ("Paste", Command::PasteWeightsSkin), ("Remove Zero Weights", Command::RemoveZeroWeightsSkin)] {
                    if ui.button(label).clicked() {
                        run(app, cmd);
                        ui.close_menu();
                    }
                }
                ui.separator();
                let all: Vec<u32> = (0..weights.len() as u32).collect();
                let invert: Vec<u32> = all.iter().copied().filter(|v| !sel_set.contains(v)).collect();
                for (label, ids) in [("Select All", all), ("Select None", Vec::new()), ("Select Invert", invert)] {
                    if ui.button(label).clicked() {
                        run(app, Command::SkinSelectVertices { ids, op: SelectOp::Replace });
                        ui.close_menu();
                    }
                }
            });
            ui.menu_button("Vertex Sets", |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut o.set_name).desired_width(100.0).hint_text("set name"));
                    if ui.button("Create Set").clicked() {
                        run(app, Command::SkinAddVertexSet { name: o.set_name.clone() });
                        ui.close_menu();
                    }
                });
                for (name, _) in &e.data.vertex_sets {
                    ui.horizontal(|ui| {
                        if ui.button(name).clicked() {
                            run(app, Command::SkinSelectVertexSet { name: name.clone() });
                            ui.close_menu();
                        }
                        if ui.small_button("Delete").clicked() {
                            run(app, Command::SkinRemoveVertexSet { name: name.clone() });
                            ui.close_menu();
                        }
                    });
                }
            });
            ui.menu_button("Options", |ui| {
                ui.checkbox(&mut o.flip, "Flip UI");
                ui.checkbox(&mut o.mouse_up, "Update On Mouse Up");
                ui.checkbox(&mut o.affected_only, "Show Affected Bones");
                ui.checkbox(&mut o.attributes, "Show Attributes");
                ui.checkbox(&mut o.exclusions, "Show Exclusions");
                ui.checkbox(&mut o.global, "Show Global");
                ui.checkbox(&mut o.set_ui, "Show Set Sets UI");
            });
        });
        const CELL: f32 = 64.0;
        const ATTR: f32 = 16.0;
        let row_h = ui.spacing().interact_size.y + 2.0;
        let excluded = |v: usize, b: usize| e.data.excluded.get(b).is_some_and(|x| x.contains(&(v as u32)));
        let weight = |v: usize, b: usize| weights[v].iter().find(|(x, _)| *x as usize == b).map(|x| x.1).unwrap_or(0.0);
        let attrs = |v: usize| -> [(&'static str, bool); 5] {
            let sum: f32 = weights[v].iter().map(|(_, w)| *w).sum();
            [
                ("S", sel_set.contains(&(v as u32))),
                ("M", e.data.explicit.get(v).copied().unwrap_or(false)),
                ("N", (sum - 1.0).abs() < 1e-3),
                ("R", e.data.rigid.get(v).copied().unwrap_or(false)),
                ("H", patch_nv.is_some_and(|nv| v < nv) && e.data.rigid_handles.get(v).copied().unwrap_or(false)),
            ]
        };
        let targets_for = |v: usize| -> Vec<u32> { if sel_set.contains(&(v as u32)) { sel.clone() } else { vec![v as u32] } };
        let all_rows: Vec<u32> = rows.iter().map(|v| *v as u32).collect();
        if !o.flip {
            egui::ScrollArea::horizontal().id_salt("weight_table_h").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_sized([56.0, row_h], egui::Label::new("Vertex ID"));
                    if o.attributes {
                        for a in ["S", "M", "N", "R", "H"] {
                            ui.add_sized([ATTR, row_h], egui::Label::new(a));
                        }
                    }
                    for (i, name) in &bones {
                        let text = if Some(e.data.bones[*i].node) == current { egui::RichText::new(name.as_str()).strong() } else { egui::RichText::new(name.as_str()) };
                        // A click on a bone name makes it the current bone.
                        if ui.add_sized([CELL + if o.exclusions { 18.0 } else { 0.0 }, row_h], egui::Label::new(text).truncate().sense(egui::Sense::click())).clicked() {
                            run(app, Command::SkinSelectBone { id: e.data.bones[*i].node.0 });
                        }
                    }
                });
                if o.global {
                    // Show Global: one row for every displayed vertex (the average shows).
                    ui.horizontal(|ui| {
                        ui.add_sized([56.0, row_h], egui::Label::new("Global"));
                        if o.attributes {
                            ui.add_space(5.0 * (ATTR + ui.spacing().item_spacing.x));
                        }
                        for (i, _) in &bones {
                            let avg = if rows.is_empty() { 0.0 } else { rows.iter().map(|v| weight(*v, *i)).sum::<f32>() / rows.len() as f32 };
                            cell(ui, app, &mut o, all_rows.clone(), e.data.bones[*i].node.0, avg, None, CELL, row_h);
                        }
                    });
                }
                // Only the rows in view are laid out (a 6k-vertex mesh × 50 bones stays interactive).
                egui::ScrollArea::vertical().id_salt("weight_table_v").max_height(360.0).show_rows(ui, row_h, rows.len(), |ui, range| {
                    for v in rows[range].iter().copied() {
                        let selected = sel_set.contains(&(v as u32));
                        ui.horizontal(|ui| {
                            // Vertex ID: click selects (Ctrl adds, Alt removes); a double-click shows it pink.
                            let r = ui.add_sized([56.0, row_h], egui::SelectableLabel::new(selected, format!("#{v}")));
                            if r.double_clicked() {
                                run(app, Command::SkinHighlightVertex { vertex: Some(v as u32) });
                            } else if r.clicked() {
                                let m = ui.input(|i| i.modifiers);
                                let op = if m.ctrl { SelectOp::Add } else if m.alt { SelectOp::Subtract } else { SelectOp::Replace };
                                run(app, Command::SkinSelectVertices { ids: vec![v as u32], op });
                            }
                            if o.attributes {
                                for (_, f) in attrs(v) {
                                    ui.add_sized([ATTR, row_h], egui::Label::new(if f { "x" } else { "" }));
                                }
                            }
                            for (i, _) in &bones {
                                let ex = o.exclusions.then(|| excluded(v, *i));
                                cell(ui, app, &mut o, targets_for(v), e.data.bones[*i].node.0, weight(v, *i), ex, CELL, row_h);
                            }
                        });
                    }
                });
            });
        } else {
            // Flip UI: the bones down the left, the vertices across the top.
            let col_w = CELL + if o.exclusions { 18.0 } else { 0.0 } + ui.spacing().item_spacing.x;
            egui::ScrollArea::both().id_salt("weight_table_flip").max_height(360.0).show_viewport(ui, |ui, view| {
                let first = ((view.min.x - 120.0) / col_w).floor().max(0.0) as usize;
                let last = (((view.max.x - 120.0) / col_w).ceil().max(0.0) as usize + 1).min(rows.len());
                ui.set_width(120.0 + col_w * rows.len() as f32);
                ui.horizontal(|ui| {
                    ui.add_sized([120.0, row_h], egui::Label::new("Bone / Vertex ID"));
                    ui.add_space(col_w * first as f32);
                    for v in rows[first.min(last)..last].iter().copied() {
                        let selected = sel_set.contains(&(v as u32));
                        let r = ui.add_sized([col_w - ui.spacing().item_spacing.x, row_h], egui::SelectableLabel::new(selected, format!("#{v}")));
                        if r.double_clicked() {
                            run(app, Command::SkinHighlightVertex { vertex: Some(v as u32) });
                        } else if r.clicked() {
                            let m = ui.input(|i| i.modifiers);
                            let op = if m.ctrl { SelectOp::Add } else if m.alt { SelectOp::Subtract } else { SelectOp::Replace };
                            run(app, Command::SkinSelectVertices { ids: vec![v as u32], op });
                        }
                    }
                });
                if o.attributes {
                    for a in 0..5 {
                        ui.horizontal(|ui| {
                            ui.add_sized([120.0, row_h], egui::Label::new(["S", "M", "N", "R", "H"][a]));
                            ui.add_space(col_w * first as f32);
                            for v in rows[first.min(last)..last].iter().copied() {
                                ui.add_sized([col_w - ui.spacing().item_spacing.x, row_h], egui::Label::new(if attrs(v)[a].1 { "x" } else { "" }));
                            }
                        });
                    }
                }
                for (i, name) in &bones {
                    ui.horizontal(|ui| {
                        let text = if Some(e.data.bones[*i].node) == current { egui::RichText::new(name.as_str()).strong() } else { egui::RichText::new(name.as_str()) };
                        if ui.add_sized([120.0, row_h], egui::Label::new(text).truncate().sense(egui::Sense::click())).clicked() {
                            run(app, Command::SkinSelectBone { id: e.data.bones[*i].node.0 });
                        }
                        ui.add_space(col_w * first as f32);
                        for v in rows[first.min(last)..last].iter().copied() {
                            let ex = o.exclusions.then(|| excluded(v, *i));
                            cell(ui, app, &mut o, targets_for(v), e.data.bones[*i].node.0, weight(v, *i), ex, CELL, row_h);
                        }
                    });
                }
            });
        }
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("weight_table_rows").selected_text(["All Vertices", "Selected Vertices", "Selected Bone"][o.shown as usize % 3]).show_ui(ui, |ui| {
                for (k, label) in ["All Vertices", "Selected Vertices", "Selected Bone"].iter().enumerate() {
                    ui.selectable_value(&mut o.shown, k as u8, *label);
                }
            });
            // The vertex set drop-down at the lower left.
            if !e.data.vertex_sets.is_empty() {
                egui::ComboBox::from_id_salt("weight_table_sets").selected_text("Vertex Set").show_ui(ui, |ui| {
                    for (name, _) in &e.data.vertex_sets {
                        if ui.selectable_label(false, name).clicked() {
                            run(app, Command::SkinSelectVertexSet { name: name.clone() });
                        }
                    }
                });
            }
            if ui.button("Copy").clicked() {
                run(app, Command::CopyWeightsSkin);
            }
            if ui.button("Paste").clicked() {
                run(app, Command::PasteWeightsSkin);
            }
            ui.label(format!("{} of {} vertices", rows.len(), weights.len()));
        });
        if o.set_ui {
            // Show Set Sets UI: the sets and their sizes.
            for (name, ids) in &e.data.vertex_sets {
                ui.label(format!("Set {name}: {} vertices", ids.len()));
            }
            if e.data.vertex_sets.is_empty() {
                ui.label("No vertex sets (Vertex Sets > Create Set)");
            }
        }
    });
    ctx.data_mut(|d| d.insert_temp(id, o));
}

/// Skin > Weight Properties > Painter Options: the brush for Paint Weights.
pub fn painter_options(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    if loom_commands::skin::entry(&app.state).is_err() {
        *open = false;
        return;
    }
    let p = app.state.skin_painter.clone();
    let set = |app: &mut LoomApp, name: &str, value: f64| {
        let _ = app.dispatch(Command::SkinSetPainterOption { name: name.into(), value });
    };
    egui::Window::new("Painter Options").open(open).resizable(false).show(ctx, |ui| {
        ui.label("Brush Properties");
        egui::Grid::new("painter_brush").num_columns(4).show(ui, |ui| {
            ui.label("Min. Strength:");
            let mut s0 = p.min_strength;
            if ui.add(crate::spinner::spin(ui, &mut s0, 0.0..=1.0).speed(0.01).fixed_decimals(2)).changed() {
                set(app, "Min Strength", s0 as f64);
            }
            ui.label("Max. Strength:");
            let mut s = p.strength;
            if ui.add(crate::spinner::spin(ui, &mut s, 0.0..=1.0).speed(0.01).fixed_decimals(2)).changed() {
                set(app, "Max Strength", s as f64);
            }
            ui.end_row();
            ui.label("Min. Size:");
            let mut z0 = p.min_size;
            if ui.add(crate::spinner::spin(ui, &mut z0, 0.0..=1e6).speed(app.spinner_speed(0.1)).fixed_decimals(2)).changed() {
                set(app, "Min Size", z0 as f64);
            }
            ui.label("Max. Size:");
            let mut z = p.size;
            if ui.add(crate::spinner::spin(ui, &mut z, 0.01..=1e6).speed(app.spinner_speed(0.1)).fixed_decimals(2)).changed() {
                set(app, "Max Size", z as f64);
            }
            ui.end_row();
        });
        let mut additive = p.additive;
        if ui.checkbox(&mut additive, "Additive").changed() {
            set(app, "Additive", if additive { 1.0 } else { 0.0 });
        }
        // The falloff graph: strength (up) against the distance from the brush centre (right); drag its points.
        falloff_graph(ui, app, &p);
        ui.horizontal(|ui| {
            for (i, l) in ["Linear", "Smooth", "Slow", "Fast", "Flat"].iter().enumerate() {
                if ui.selectable_label(p.falloff == i as u32, *l).clicked() {
                    set(app, "Falloff", i as f64);
                }
            }
        });
        ui.separator();
        ui.label("Display Options");
        let mut ring = p.draw_ring;
        if ui.checkbox(&mut ring, "Draw Ring").changed() {
            set(app, "Draw Ring", if ring { 1.0 } else { 0.0 });
        }
        ui.separator();
        ui.label("Pressure Options");
        let mut pr = p.pressure_on;
        if ui.checkbox(&mut pr, "Enable Pressure Sensitivity").changed() {
            set(app, "Enable Pressure Sensitivity", if pr { 1.0 } else { 0.0 });
        }
        ui.horizontal(|ui| {
            ui.label("Pressure Affects:");
            const AFFECTS: [&str; 4] = ["None", "Strength", "Size", "Size and Strength"];
            let mut a = p.pressure_affects as usize;
            ui.add_enabled_ui(p.pressure_on, |ui| {
                egui::ComboBox::from_id_salt("painter_pressure").selected_text(AFFECTS[a.min(3)]).show_ui(ui, |ui| {
                    for (k, l) in AFFECTS.iter().enumerate() {
                        ui.selectable_value(&mut a, k, *l);
                    }
                });
            });
            if a as u32 != p.pressure_affects {
                set(app, "Pressure Affects", a as f64);
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            let mut m = p.mirror;
            if ui.checkbox(&mut m, "Mirror").changed() {
                set(app, "Mirror", if m { 1.0 } else { 0.0 });
            }
            for (i, l) in ["X", "Y", "Z"].iter().enumerate() {
                if ui.selectable_label(p.mirror_axis == i as u32, *l).clicked() {
                    set(app, "Mirror Axis", i as f64);
                }
            }
            ui.label("Offset:");
            let mut o = p.mirror_offset;
            if ui.add(crate::spinner::spin(ui, &mut o, ..).speed(app.spinner_speed(0.1))).changed() {
                set(app, "Mirror Offset", o as f64);
            }
        });
    });
}

/// Painter Options' falloff graph: the curve's points drag with the mouse;
/// a double-click on the curve adds a point.
fn falloff_graph(ui: &mut egui::Ui, app: &mut LoomApp, p: &loom_commands::skin::Painter) {
    let size = egui::vec2(220.0, 90.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Falloff graph"));
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, egui::Color32::from_gray(30));
    let pts: Vec<[f32; 2]> = if p.curve.len() >= 2 { p.curve.clone() } else { loom_commands::skin::Painter::preset_curve(p.falloff) };
    let to_screen = |q: [f32; 2]| egui::pos2(rect.left() + q[0] * rect.width(), rect.bottom() - q[1] * rect.height());
    let from_screen = |s: egui::Pos2| [((s.x - rect.left()) / rect.width()).clamp(0.0, 1.0), ((rect.bottom() - s.y) / rect.height()).clamp(0.0, 1.0)];
    let line: Vec<egui::Pos2> = (0..=40).map(|i| i as f32 / 40.0).map(|t| to_screen([t, p.falloff_at(t)])).collect();
    painter.add(egui::Shape::line(line, egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(230, 200, 80))));
    for q in &pts {
        painter.circle_filled(to_screen(*q), 3.0, egui::Color32::WHITE);
    }
    let drag_id = ui.id().with("painter_graph_drag");
    if response.drag_started() {
        if let Some(at) = response.interact_pointer_pos() {
            let k = pts.iter().enumerate().map(|(k, q)| (k, to_screen(*q).distance(at))).filter(|(_, d)| *d <= 8.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(k, _)| k);
            ui.data_mut(|d| d.insert_temp(drag_id, k));
        }
    }
    if response.dragged() {
        let k: Option<usize> = ui.data(|d| d.get_temp(drag_id)).flatten();
        if let (Some(k), Some(at)) = (k, response.interact_pointer_pos()) {
            let mut c = pts.clone();
            let mut q = from_screen(at);
            // The end points stay at the centre and the rim.
            if k == 0 {
                q[0] = 0.0;
            } else if k + 1 == c.len() {
                q[0] = 1.0;
            }
            c[k] = q;
            run(app, Command::SkinSetPainterCurve { points: c.iter().map(|x| [x[0], x[1]]).collect() });
        }
    }
    if response.double_clicked() {
        if let Some(at) = response.interact_pointer_pos() {
            let mut c = pts.clone();
            c.push(from_screen(at));
            run(app, Command::SkinSetPainterCurve { points: c });
        }
    }
}

/// Envelope Properties > Paste to Multiple Bones: tick the bones that take the copied envelope.
pub fn paste_to_multiple_bones(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let Ok(e) = loom_commands::skin::entry(&app.state) else {
        *open = false;
        return;
    };
    let id = egui::Id::new("paste_to_multiple_bones");
    let mut picked: std::collections::BTreeSet<u64> = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
    let mut done = false;
    egui::Window::new("Paste to Multiple Bones").open(open).resizable(false).show(ctx, |ui| {
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            for b in &e.data.bones {
                let name = app.state.scene.node(b.node).map(|n| n.name.clone()).unwrap_or_default();
                let mut on = picked.contains(&b.node.0);
                if ui.checkbox(&mut on, name).changed() {
                    if on {
                        picked.insert(b.node.0);
                    } else {
                        picked.remove(&b.node.0);
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(!picked.is_empty(), egui::Button::new("Paste")).clicked() {
                run(app, Command::SkinPasteToBones { ids: picked.iter().copied().collect() });
                done = true;
            }
            if ui.button("Cancel").clicked() {
                done = true;
            }
        });
    });
    if done {
        *open = false;
        picked.clear();
    }
    ctx.data_mut(|d| d.insert_temp(id, picked));
}

/// The name without its prefix (the text up to the first space), for matching.
fn strip_prefix(name: &str) -> &str {
    name.split_once(' ').map(|(_, rest)| rest).unwrap_or(name)
}

/// The Load Envelopes dialog's own state: the incoming list lined up
/// against the current bones (None = a blank row), the selected row,
/// the prefixes stripped for matching and the load options.
#[derive(Debug, Clone, Default)]
struct LoadState {
    rows: Vec<Option<usize>>,
    selected: Option<usize>,
    strip_incoming: bool,
    strip_current: bool,
    options: [bool; 5],
}

/// Advanced Parameters > Load: line the incoming envelopes up with the
/// current bones (Move Up / Down, Create Blank, Delete, Match by Name,
/// Remove Incoming / Current Prefix) and pick what loads.
pub fn load_envelopes(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let (Ok(e), Some(file)) = (loom_commands::skin::entry(&app.state), app.state.skin_ui.load.clone()) else {
        *open = false;
        return;
    };
    let id = egui::Id::new("load_envelopes");
    let mut st: LoadState = ctx.data(|d| d.get_temp(id)).unwrap_or_else(|| LoadState { rows: (0..file.bones.len()).map(Some).collect(), selected: None, strip_incoming: false, strip_current: false, options: [true, true, false, false, true] });
    let current: Vec<String> = e.data.bones.iter().map(|b| app.state.scene.node(b.node).map(|n| n.name.clone()).unwrap_or_default()).collect();
    let mut done = false;
    egui::Window::new("Load Envelopes").open(open).resizable(true).default_width(420.0).show(ctx, |ui| {
        ui.columns(2, |cols| {
            cols[0].label("Incoming");
            for (r, row) in st.rows.clone().iter().enumerate() {
                let name = match row {
                    Some(k) => {
                        let n = &file.bones[*k].name;
                        if st.strip_incoming { strip_prefix(n).to_owned() } else { n.clone() }
                    }
                    None => "(blank)".into(),
                };
                if cols[0].selectable_label(st.selected == Some(r), name).clicked() {
                    st.selected = Some(r);
                }
            }
            cols[1].label("Current");
            for n in &current {
                cols[1].label(if st.strip_current { strip_prefix(n) } else { n.as_str() });
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("Move Up").clicked() {
                if let Some(r) = st.selected.filter(|r| *r > 0) {
                    st.rows.swap(r, r - 1);
                    st.selected = Some(r - 1);
                }
            }
            if ui.button("Move Down").clicked() {
                if let Some(r) = st.selected.filter(|r| r + 1 < st.rows.len()) {
                    st.rows.swap(r, r + 1);
                    st.selected = Some(r + 1);
                }
            }
            if ui.button("Create Blank").clicked() {
                let at = st.selected.unwrap_or(st.rows.len());
                st.rows.insert(at, None);
            }
            if ui.button("Delete").clicked() {
                if let Some(r) = st.selected.filter(|r| *r < st.rows.len()) {
                    st.rows.remove(r);
                    st.selected = None;
                }
            }
            if ui.button("Match by Name").clicked() {
                let key_in = |k: usize| if st.strip_incoming { strip_prefix(&file.bones[k].name).to_owned() } else { file.bones[k].name.clone() };
                let key_cur = |n: &String| if st.strip_current { strip_prefix(n).to_owned() } else { n.clone() };
                let mut used = vec![false; file.bones.len()];
                let mut rows: Vec<Option<usize>> = current.iter().map(|n| (0..file.bones.len()).find(|k| key_in(*k) == key_cur(n)).inspect(|k| used[*k] = true)).collect();
                // Unmatched incoming envelopes go to the end.
                rows.extend((0..file.bones.len()).filter(|k| !used[*k]).map(Some));
                st.rows = rows;
            }
            ui.checkbox(&mut st.strip_incoming, "Remove Incoming Prefix");
            ui.checkbox(&mut st.strip_current, "Remove Current Prefix");
        });
        for (k, label) in ["Load End Points", "Load Cross Sections", "Load Vertex Data", "Load Exclusion Lists", "Load Vertices By Index"].iter().enumerate() {
            ui.checkbox(&mut st.options[k], *label);
        }
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                let incoming: Vec<i32> = (0..current.len()).map(|i| st.rows.get(i).copied().flatten().map(|k| k as i32).unwrap_or(-1)).collect();
                let [end_points, cross_sections, vertex_data, exclusions, by_index] = st.options;
                run(app, Command::SkinApplyLoadedEnvelopes { incoming, end_points, cross_sections, vertex_data, exclusions, by_index });
                done = true;
            }
            if ui.button("Cancel").clicked() {
                app.state.skin_ui.load = None;
                done = true;
            }
        });
    });
    if done {
        *open = false;
        ctx.data_mut(|d| d.remove::<LoadState>(id));
    } else {
        ctx.data_mut(|d| d.insert_temp(id, st));
    }
}

/// Gizmo Parameters > Edit Angle Keys Curves: each selected lattice
/// point's offset (X red, Y green, Z blue) against the bone angle, one
/// point per key; drag a key point up or down to change that offset.
pub fn angle_keys_curves(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let Ok(e) = loom_commands::skin::entry(&app.state) else {
        *open = false;
        return;
    };
    let Some(g) = app.state.skin_gizmo.and_then(|i| e.data.gizmos.get(i)).cloned() else {
        *open = false;
        return;
    };
    let Some(lat) = g.lattice.clone() else {
        *open = false;
        return;
    };
    let points: Vec<usize> = if app.state.skin_ui.lattice_selection.is_empty() { (0..lat.count()).collect() } else { app.state.skin_ui.lattice_selection.iter().map(|i| *i as usize).collect() };
    egui::Window::new("Edit Angle Keys Curves").open(open).resizable(true).default_size([420.0, 240.0]).show(ctx, |ui| {
        let mut keys: Vec<(usize, f32)> = g.keys.iter().enumerate().map(|(k, key)| (k, key.angle_deg)).collect();
        keys.sort_by(|a, b| a.1.total_cmp(&b.1));
        let (amin, amax) = (keys.first().map(|k| k.1).unwrap_or(0.0).min(0.0), keys.last().map(|k| k.1).unwrap_or(0.0).max(1.0));
        let offset = |k: usize, p: usize, c: usize| g.keys[k].lattice.get(p).map(|v| v[c]).unwrap_or(0.0);
        let mut vmax = 1e-3f32;
        for (k, _) in &keys {
            for p in &points {
                for c in 0..3 {
                    vmax = vmax.max(offset(*k, *p, c).abs());
                }
            }
        }
        let size = egui::vec2(ui.available_width().max(200.0), 180.0);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Angle keys curves"));
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 2.0, egui::Color32::from_gray(28));
        painter.line_segment([egui::pos2(rect.left(), rect.center().y), egui::pos2(rect.right(), rect.center().y)], egui::Stroke::new(1.0_f32, egui::Color32::from_gray(70)));
        let x_of = |a: f32| rect.left() + (a - amin) / (amax - amin).max(1e-3) * rect.width();
        let y_of = |v: f32| rect.center().y - v / (vmax * 1.2) * rect.height() * 0.5;
        let colours = [egui::Color32::from_rgb(230, 60, 60), egui::Color32::from_rgb(60, 200, 60), egui::Color32::from_rgb(70, 110, 255)];
        let mut handles: Vec<(egui::Pos2, usize, usize, usize)> = Vec::new();
        for p in &points {
            for c in 0..3 {
                let line: Vec<egui::Pos2> = keys.iter().map(|(k, a)| egui::pos2(x_of(*a), y_of(offset(*k, *p, c)))).collect();
                for (i, q) in line.iter().enumerate() {
                    painter.circle_filled(*q, 2.5, colours[c]);
                    handles.push((*q, keys[i].0, *p, c));
                }
                if line.len() >= 2 {
                    painter.add(egui::Shape::line(line, egui::Stroke::new(1.0_f32, colours[c])));
                }
            }
        }
        let drag_id = ui.id().with("angle_curve_drag");
        if response.drag_started() {
            if let Some(at) = response.interact_pointer_pos() {
                let h = handles.iter().map(|h| (h.0.distance(at), (h.1, h.2, h.3))).filter(|(d, _)| *d <= 8.0).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, h)| h);
                ui.data_mut(|d| d.insert_temp(drag_id, h));
                app.state.undo.begin("Edit Angle Keys Curves");
            }
        }
        if response.dragged() {
            let h: Option<(usize, usize, usize)> = ui.data(|d| d.get_temp(drag_id)).flatten();
            if let (Some((k, p, c)), Some(at)) = (h, response.interact_pointer_pos()) {
                let v = (rect.center().y - at.y) / (rect.height() * 0.5) * vmax * 1.2;
                let mut off = g.keys[k].lattice.get(p).copied().unwrap_or_default();
                off[c] = v;
                run(app, Command::SkinSetLatticeOffset { key: k as u32, point: p as u32, offset: off.to_array() });
            }
        }
        if response.drag_stopped() {
            app.state.undo.end();
        }
        ui.label(format!("{} lattice point(s), {} key(s), {:.0}° … {:.0}°", points.len(), keys.len(), amin, amax));
    });
}
