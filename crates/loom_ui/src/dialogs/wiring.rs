//! Animation > Wire Parameters (Parameter Wiring dialog), Add Custom
//! Attributes and the Parameter Collector. Every change is a command.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;
use loom_scene::NodeId;

/// The transform tracks every node offers, then its custom attributes.
fn parameters(app: &LoomApp, id: NodeId) -> Vec<String> {
    app.state.scene.node(id).map(loom_commands::extras::reaction_parameters).unwrap_or_default()
}

fn node_name(app: &LoomApp, id: NodeId) -> String {
    app.state.scene.node(id).map(|n| n.name.clone()).unwrap_or_else(|| format!("#{}", id.0))
}

#[derive(Clone)]
struct Wiring {
    source: Option<(NodeId, String)>,
    target: Option<(NodeId, String)>,
    two_way: bool,
    scale: f32,
    offset: f32,
    expression: String,
}

impl Default for Wiring {
    fn default() -> Self {
        Self { source: None, target: None, two_way: false, scale: 1.0, offset: 0.0, expression: String::new() }
    }
}

/// One side of the dialog: a tree of nodes with their parameters.
fn side(ui: &mut egui::Ui, app: &LoomApp, salt: &str, pick: &mut Option<(NodeId, String)>) {
    let ids: Vec<NodeId> = app.state.scene.nodes().map(|n| n.id).collect();
    egui::ScrollArea::vertical().id_salt(salt).max_height(260.0).show(ui, |ui| {
        for id in ids {
            egui::CollapsingHeader::new(node_name(app, id)).id_salt((salt, id.0)).show(ui, |ui| {
                for p in parameters(app, id) {
                    let on = pick.as_ref() == Some(&(id, p.clone()));
                    if ui.selectable_label(on, &p).clicked() {
                        *pick = Some((id, p));
                    }
                }
            });
        }
    });
}

pub fn parameter_wiring(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let fid = egui::Id::new("parameter_wiring_form");
    let mut f: Wiring = ctx.data(|d| d.get_temp(fid)).unwrap_or_default();
    egui::Window::new("Parameter Wiring").open(open).resizable(true).default_width(560.0).show(ctx, |ui| {
        ui.columns(2, |c| {
            c[0].label(egui::RichText::new("Source").strong());
            side(&mut c[0], app, "wire_source", &mut f.source);
            c[1].label(egui::RichText::new("Target").strong());
            side(&mut c[1], app, "wire_target", &mut f.target);
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.radio_value(&mut f.two_way, false, "One-way (Source -> Target)");
            ui.radio_value(&mut f.two_way, true, "Two-way");
        });
        ui.horizontal(|ui| {
            ui.label("Scale:");
            ui.add(crate::spinner::spin(ui, &mut f.scale, -1000.0..=1000.0));
            ui.label("Offset:");
            ui.add(crate::spinner::spin(ui, &mut f.offset, -100000.0..=100000.0));
        });
        ui.horizontal(|ui| {
            ui.label("Expression:");
            ui.add(egui::TextEdit::singleline(&mut f.expression).desired_width(320.0).hint_text("e.g. Source * 2"));
        });
        let ready = f.source.is_some() && f.target.is_some() && f.source != f.target;
        if ui.add_enabled(ready, egui::Button::new("Connect")).clicked() {
            if let (Some((sn, sp)), Some((tn, tp))) = (f.source.clone(), f.target.clone()) {
                let expression = (!f.expression.trim().is_empty()).then(|| f.expression.clone());
                let _ = app.dispatch(Command::WireParameter {
                    source_node: sn.0,
                    source_param: sp,
                    target_node: tn.0,
                    target_param: tp,
                    two_way: Some(f.two_way),
                    scale: Some(f.scale),
                    offset: Some(f.offset),
                    expression,
                });
            }
        }
        ui.separator();
        ui.label(egui::RichText::new("Wires").strong());
        let wires = app.state.scene.parameter_wires.clone();
        if wires.is_empty() {
            ui.weak("No wires.");
        }
        for (i, w) in wires.iter().enumerate() {
            ui.horizontal(|ui| {
                let arrow = if w.two_way { "↔" } else { "->" };
                ui.label(format!("{}.{} {arrow} {}.{}", node_name(app, w.source_node), w.source_param, node_name(app, w.target_node), w.target_param));
                if ui.small_button("Disconnect").clicked() {
                    let _ = app.dispatch(Command::DisconnectWireParameter { index: i as u32 });
                }
            });
        }
    });
    ctx.data_mut(|d| d.insert_temp(fid, f));
}

#[derive(Clone)]
struct NewAttribute {
    name: String,
    kind: String,
    value: f32,
    min: f32,
    max: f32,
}

impl Default for NewAttribute {
    fn default() -> Self {
        Self { name: "Param1".into(), kind: "float".into(), value: 0.0, min: -100.0, max: 100.0 }
    }
}

pub fn add_custom_attributes(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let fid = egui::Id::new("add_custom_attributes_form");
    let mut f: NewAttribute = ctx.data(|d| d.get_temp(fid)).unwrap_or_default();
    egui::Window::new("Add Parameter").open(open).resizable(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.text_edit_singleline(&mut f.name);
        });
        ui.horizontal(|ui| {
            ui.label("Parameter Type:");
            egui::ComboBox::from_id_salt("custom_attr_type").selected_text(match f.kind.as_str() {
                "integer" => "Integer",
                "boolean" => "Boolean",
                _ => "Float",
            }).show_ui(ui, |ui| {
                ui.selectable_value(&mut f.kind, "float".to_string(), "Float");
                ui.selectable_value(&mut f.kind, "integer".to_string(), "Integer");
                ui.selectable_value(&mut f.kind, "boolean".to_string(), "Boolean");
            });
        });
        if f.kind != "boolean" {
            ui.horizontal(|ui| {
                ui.label("Range From:");
                ui.add(crate::spinner::spin(ui, &mut f.min, -1.0e6..=1.0e6));
                ui.label("To:");
                ui.add(crate::spinner::spin(ui, &mut f.max, -1.0e6..=1.0e6));
            });
        }
        ui.horizontal(|ui| {
            ui.label("Default:");
            ui.add(crate::spinner::spin(ui, &mut f.value, -1.0e6..=1.0e6));
        });
        let can = !f.name.trim().is_empty() && !app.state.selection.is_empty();
        if ui.add_enabled(can, egui::Button::new("Add")).on_disabled_hover_text("Select an object and name the parameter").clicked() {
            let _ = app.dispatch(Command::AddCustomAttribute {
                id: None,
                name: f.name.trim().to_string(),
                attr_type: Some(f.kind.clone()),
                value: Some(f.value),
                min: Some(f.min),
                max: Some(f.max),
            });
        }
    });
    ctx.data_mut(|d| d.insert_temp(fid, f));
}

pub fn parameter_collector(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    egui::Window::new("Parameter Collector").open(open).resizable(true).default_width(360.0).show(ctx, |ui| {
        let ids: Vec<NodeId> = app.state.selection.iter().copied().collect();
        if ids.is_empty() {
            ui.weak("Select objects to collect their custom attributes.");
        }
        for id in ids {
            let Some(n) = app.state.scene.node(id) else { continue };
            let name = n.name.clone();
            let attrs = n.custom_attributes.clone();
            ui.label(egui::RichText::new(name).strong());
            if attrs.is_empty() {
                ui.weak("  No custom attributes.");
            }
            for a in attrs {
                ui.horizontal(|ui| {
                    ui.label(&a.name);
                    let mut v = a.value;
                    let changed = match a.attr_type.as_str() {
                        "boolean" => {
                            let mut b = v != 0.0;
                            let r = ui.checkbox(&mut b, "").changed();
                            v = if b { 1.0 } else { 0.0 };
                            r
                        }
                        "integer" => {
                            let mut i = v.round() as i32;
                            let r = ui.add(crate::spinner::spin(ui, &mut i, a.min.round() as i32..=a.max.round() as i32)).changed();
                            v = i as f32;
                            r
                        }
                        _ => ui.add(crate::spinner::spin(ui, &mut v, a.min..=a.max)).changed(),
                    };
                    if changed {
                        let _ = app.dispatch(Command::SetCustomAttribute { id: id.0, name: a.name.clone(), value: v });
                    }
                    if ui.small_button("Delete").clicked() {
                        let _ = app.dispatch(Command::DeleteCustomAttribute { id: id.0, name: a.name.clone() });
                    }
                });
            }
        }
    });
}

/// The Reaction Manager's picked track and highlighted row (reaction, slave).
type ReactionForm = (Option<(NodeId, String)>, Option<(usize, Option<usize>)>);

/// Animation > Reaction Manager: the Reactions list (masters with their
/// slaves), a parameter picker to add masters and slaves, and the States of
/// the highlighted slave (master value, slave value) with Create State and
/// Delete State.
pub fn reaction_manager(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let fid = egui::Id::new("reaction_manager_form");
    let (mut pick, mut hl): ReactionForm = ctx.data(|d| d.get_temp(fid)).unwrap_or_default();
    egui::Window::new("Reaction Manager").open(open).resizable(true).default_width(620.0).show(ctx, |ui| {
        ui.columns(2, |c| {
            c[0].label(egui::RichText::new("Pick a track").strong());
            if c[0].selectable_label(app.state.pick_mode == Some(loom_commands::PickMode::ReactionTrack), "Pick from Viewport").on_hover_text("Click an object in a viewport, then one of its tracks").clicked() {
                let _ = app.dispatch(Command::ReactionPickFromViewport);
            }
            if let Some(id) = app.state.reaction_picked_node.filter(|id| app.state.scene.node(*id).is_some()) {
                c[0].label(format!("Picked: {}", node_name(app, id)));
                egui::ScrollArea::vertical().id_salt("reaction_picked").max_height(140.0).show(&mut c[0], |ui| {
                    for p in parameters(app, id) {
                        let on = pick.as_ref() == Some(&(id, p.clone()));
                        if ui.selectable_label(on, &p).clicked() {
                            pick = Some((id, p));
                        }
                    }
                });
                c[0].separator();
            }
            side(&mut c[0], app, "reaction_pick", &mut pick);
            let ui = &mut c[1];
            ui.label(egui::RichText::new("Reactions").strong());
            ui.horizontal(|ui| {
                if ui.add_enabled(pick.is_some(), egui::Button::new("Add Master")).clicked() {
                    if let Some((n, p)) = pick.clone() {
                        let _ = app.dispatch(Command::AddReactionMaster { node: n.0, param: p });
                        hl = Some((app.state.scene.reactions.len().saturating_sub(1), None));
                    }
                }
                let master = hl.map(|h| h.0);
                if ui.add_enabled(pick.is_some() && master.is_some(), egui::Button::new("Add Slave")).clicked() {
                    if let (Some((n, p)), Some(r)) = (pick.clone(), master) {
                        let _ = app.dispatch(Command::AddReactionSlave { reaction: r as u32, node: n.0, param: p });
                        hl = Some((r, app.state.scene.reactions.get(r).map(|x| x.slaves.len().saturating_sub(1))));
                    }
                }
                if ui.add_enabled(hl.is_some(), egui::Button::new("Delete Selected")).clicked() {
                    if let Some((r, s)) = hl {
                        let _ = app.dispatch(Command::DeleteReaction { reaction: r as u32, slave: s.map(|s| s as u32) });
                        hl = None;
                    }
                }
            });
            let reactions = app.state.scene.reactions.clone();
            for (ri, r) in reactions.iter().enumerate() {
                if ui.selectable_label(hl == Some((ri, None)), format!("{} / {}", node_name(app, r.master_node), r.master_param)).clicked() {
                    hl = Some((ri, None));
                }
                for (si, s) in r.slaves.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.selectable_label(hl == Some((ri, Some(si))), format!("    {} / {}", node_name(app, s.node), s.param)).clicked() {
                            hl = Some((ri, Some(si)));
                        }
                        // Start / End: the frames the reaction drives this slave in.
                        let [mut a, mut b] = s.range.unwrap_or([app.state.frame_range.0, app.state.frame_range.1]);
                        ui.label("Start");
                        if ui.add(crate::spinner::spin(ui, &mut a, ..).speed(0.2)).changed() {
                            let _ = app.dispatch(Command::SetReactionSlaveRange { reaction: ri as u32, slave: si as u32, start: Some(a), end: None });
                        }
                        ui.label("End");
                        if ui.add(crate::spinner::spin(ui, &mut b, ..).speed(0.2)).changed() {
                            let _ = app.dispatch(Command::SetReactionSlaveRange { reaction: ri as u32, slave: si as u32, start: None, end: Some(b) });
                        }
                        let mut curve = s.curve;
                        if ui.checkbox(&mut curve, "Curve").on_hover_text("Off: Strength / Influence / Falloff blend the states").changed() {
                            let _ = app.dispatch(Command::SetReactionSlaveCurve { reaction: ri as u32, slave: si as u32, curve });
                        }
                    });
                }
            }
            ui.separator();
            ui.label(egui::RichText::new("States").strong());
            if let Some((ri, Some(si))) = hl {
                if let Some(s) = reactions.get(ri).and_then(|r| r.slaves.get(si)) {
                    ui.horizontal(|ui| {
                        let cm = app.state.reaction_create_mode;
                        if ui.selectable_label(cm, "Create Mode").on_hover_text("Slaves stop following: pose master and slaves, then Create State").clicked() {
                            let _ = app.dispatch(Command::SetReactionCreateMode { on: !cm });
                        }
                        if ui.button("Create State").on_hover_text("A state from the master's and the slave's current values").clicked() {
                            let _ = app.dispatch(Command::SetReactionState { reaction: ri as u32, slave: si as u32, state: None, master_value: None, slave_value: None });
                        }
                    });
                    let blend = !s.curve;
                    egui::Grid::new("reaction_states").num_columns(if blend { 7 } else { 4 }).show(ui, |ui| {
                        ui.label("State");
                        ui.label("Master Value");
                        ui.label("Slave Value");
                        if blend {
                            ui.label("Strength");
                            ui.label("Influence");
                            ui.label("Falloff");
                        }
                        ui.end_row();
                        for (k, (m, v)) in s.states.iter().enumerate() {
                            // The state's name: edit it in place (Enter or clicking away renames).
                            let nid = egui::Id::new(("reaction_state_name", ri, si, k));
                            let mut name: String = ui.data(|d| d.get_temp(nid)).unwrap_or_else(|| s.name_of(k));
                            let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(80.0));
                            if r.lost_focus() {
                                if name != s.name_of(k) {
                                    let _ = app.dispatch(Command::RenameReactionState { reaction: ri as u32, slave: si as u32, state: k as u32, name: name.clone() });
                                }
                                ui.data_mut(|d| d.remove::<String>(nid));
                            } else if r.has_focus() {
                                ui.data_mut(|d| d.insert_temp(nid, name));
                            }
                            let (mut m2, mut v2) = (*m, *v);
                            if ui.add(crate::spinner::spin(ui, &mut m2, ..).speed(0.1)).changed() {
                                let _ = app.dispatch(Command::SetReactionState { reaction: ri as u32, slave: si as u32, state: Some(k as u32), master_value: Some(m2), slave_value: Some(*v) });
                            }
                            if ui.add(crate::spinner::spin(ui, &mut v2, ..).speed(0.1)).changed() {
                                let _ = app.dispatch(Command::SetReactionState { reaction: ri as u32, slave: si as u32, state: Some(k as u32), master_value: Some(*m), slave_value: Some(v2) });
                            }
                            if blend {
                                let b = s.blend_of(k);
                                let (mut st, mut inf, mut fo) = (b.strength, b.influence, b.falloff);
                                let one = |strength, influence, falloff| Command::SetReactionStateBlend { reaction: ri as u32, slave: si as u32, state: k as u32, strength, influence, falloff };
                                if ui.add(crate::spinner::spin(ui, &mut st, 0.0..).speed(0.01)).changed() {
                                    let _ = app.dispatch(one(Some(st), None, None));
                                }
                                if ui.add(crate::spinner::spin(ui, &mut inf, 0.0..).speed(0.1)).changed() {
                                    let _ = app.dispatch(one(None, Some(inf), None));
                                }
                                if ui.add(crate::spinner::spin(ui, &mut fo, 0.0..).speed(0.01)).changed() {
                                    let _ = app.dispatch(one(None, None, Some(fo)));
                                }
                            }
                            if ui.small_button("Delete State").clicked() {
                                let _ = app.dispatch(Command::DeleteReactionState { reaction: ri as u32, slave: si as u32, state: k as u32 });
                            }
                            ui.end_row();
                        }
                    });
                    reaction_graph(ui, app, ri, si, s);
                }
            } else {
                ui.weak("Highlight a slave to see its states.");
            }
        });
    });
    ctx.data_mut(|d| d.insert_temp(fid, (pick, hl)));
}

/// Animation > Animation Layers: the layer toolbar for the selected objects
/// (Enable, the layer list with its bulb / lock / output / weight, Add,
/// Delete, Copy, Paste, Collapse, Disable).
pub fn animation_layers(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    egui::Window::new("Animation Layers").open(open).resizable(false).default_width(420.0).show(ctx, |ui| {
        let al = app.state.selection.iter().find_map(|id| app.state.scene.node(*id).and_then(|n| n.anim_layers.clone()));
        let Some(al) = al else {
            ui.label("Select objects, then enable their transform tracks for layers.");
            if ui.add_enabled(!app.state.selection.is_empty(), egui::Button::new("Enable Anim Layers")).clicked() {
                let _ = app.dispatch(Command::EnableAnimLayers { position: true, rotation: true, scale: true });
            }
            return;
        };
        let mut cmds = Vec::new();
        egui::Grid::new("anim_layers_list").striped(true).show(ui, |ui| {
            for i in (0..=al.layers.len()).rev() {
                let idx = i as u32;
                if ui.selectable_label(al.active == i, al.names()[i].as_str()).clicked() {
                    cmds.push(Command::SetActiveAnimLayer { index: idx });
                }
                let prop = |name: &str, value: f32| Command::SetAnimLayerProperty { index: idx, name: name.into(), value };
                if i == 0 {
                    ui.label("");
                    if ui.selectable_label(al.base_locked, "Lock").clicked() {
                        cmds.push(prop("locked", if al.base_locked { 0.0 } else { 1.0 }));
                    }
                    ui.label("");
                    ui.label("");
                } else {
                    let l = &al.layers[i - 1];
                    if ui.selectable_label(l.on, "On").on_hover_text("Light bulb: the layer's animation shows").clicked() {
                        cmds.push(prop("on", if l.on { 0.0 } else { 1.0 }));
                    }
                    if ui.selectable_label(l.locked, "Lock").clicked() {
                        cmds.push(prop("locked", if l.locked { 0.0 } else { 1.0 }));
                    }
                    if ui.selectable_label(l.output, if l.output { "+" } else { "-" }).on_hover_text("Include in the output track").clicked() {
                        cmds.push(prop("output", if l.output { 0.0 } else { 1.0 }));
                    }
                    let mut w = l.weight.get();
                    if ui.add(egui::DragValue::new(&mut w).range(0.0..=100.0).suffix("%")).changed() {
                        cmds.push(prop("weight", w));
                    }
                }
                ui.end_row();
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (label, c) in [
                ("Add Layer", Command::AddAnimLayer { name: None }),
                ("Delete Layer", Command::DeleteAnimLayer),
                ("Copy", Command::CopyAnimLayer),
                ("Paste Active", Command::PasteActiveAnimLayer),
                ("Paste New", Command::PasteNewAnimLayer { name: None }),
                ("Collapse", Command::CollapseAnimLayer),
                ("Disable", Command::DisableAnimLayers),
            ] {
                if ui.button(label).clicked() {
                    cmds.push(c);
                }
            }
        });
        for c in cmds {
            let _ = app.dispatch(c);
        }
    });
}

/// The Reaction Manager graph's toolbar tool.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum GraphTool {
    #[default]
    Move,
    MoveHorizontal,
    MoveVertical,
    Scale,
    AddCorner,
    AddBezier,
}

/// Reaction Manager > the graph: master values across, slave values up, the
/// slave's curve and its states as points. The toolbar's Move (both ways,
/// horizontal, vertical) and Scale Points drag the points, Add Point adds a
/// Corner or Bezier state where the graph is clicked; right-clicking a point
/// sets its type, and a selected Bezier point's handles drag.
fn reaction_graph(ui: &mut egui::Ui, app: &mut LoomApp, ri: usize, si: usize, s: &loom_scene::ReactionSlave) {
    use loom_scene::PointKind;
    let tool_id = egui::Id::new("reaction_graph_tool");
    let sel_id = egui::Id::new("reaction_graph_selected");
    let mut tool: GraphTool = ui.data(|d| d.get_temp(tool_id)).unwrap_or_default();
    let mut selected: Option<usize> = ui.data(|d| d.get_temp::<Option<usize>>(sel_id)).flatten().filter(|k| *k < s.states.len());
    ui.horizontal(|ui| {
        for (t, label, tip) in [
            (GraphTool::Move, "Move", "Drag points both ways"),
            (GraphTool::MoveHorizontal, "Move H", "Drag points along the master axis only"),
            (GraphTool::MoveVertical, "Move V", "Drag points along the value axis only"),
            (GraphTool::Scale, "Scale Points", "Drag up / down to scale the points' values"),
            (GraphTool::AddCorner, "Add Corner", "Click the graph to add a Corner state"),
            (GraphTool::AddBezier, "Add Bezier", "Click the graph to add a Bezier state"),
        ] {
            if ui.selectable_label(tool == t, label).on_hover_text(tip).clicked() {
                tool = t;
            }
        }
    });
    let (rect, bg) = ui.allocate_exact_size(egui::vec2(ui.available_width().max(200.0), 160.0), egui::Sense::click());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
    if s.states.is_empty() {
        ui.data_mut(|d| d.insert_temp(tool_id, tool));
        return;
    }
    let (mut x0, mut x1, mut y0, mut y1) = (f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY, f32::NEG_INFINITY);
    for (m, v) in &s.states {
        x0 = x0.min(*m);
        x1 = x1.max(*m);
        y0 = y0.min(*v);
        y1 = y1.max(*v);
    }
    let (px, py) = (((x1 - x0) * 0.1).max(1.0), ((y1 - y0) * 0.15).max(1.0));
    let (x0, x1, y0, y1) = (x0 - px, x1 + px, y0 - py, y1 + py);
    let inner = rect.shrink(6.0);
    let to = |m: f32, v: f32| egui::pos2(inner.left() + (m - x0) / (x1 - x0) * inner.width(), inner.bottom() - (v - y0) / (y1 - y0) * inner.height());
    let from = |q: egui::Pos2| (x0 + (q.x - inner.left()) / inner.width() * (x1 - x0), y0 + (inner.bottom() - q.y) / inner.height() * (y1 - y0));
    let pts: Vec<egui::Pos2> = (0..=120).filter_map(|i| {
        let m = x0 + (x1 - x0) * i as f32 / 120.0;
        s.value(m).map(|v| to(m, v))
    }).collect();
    p.add(egui::Shape::line(pts, egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(220, 60, 60))));
    let set = |app: &mut LoomApp, k: usize, m: f32, v: f32| {
        let _ = app.dispatch(Command::SetReactionState { reaction: ri as u32, slave: si as u32, state: Some(k as u32), master_value: Some(m), slave_value: Some(v) });
    };
    // Add Point: a click on the graph adds a state there.
    if matches!(tool, GraphTool::AddCorner | GraphTool::AddBezier) && bg.clicked() {
        if let Some(q) = bg.interact_pointer_pos() {
            let (m, v) = from(q);
            let n = s.states.len() as u32;
            let _ = app.dispatch(Command::SetReactionState { reaction: ri as u32, slave: si as u32, state: None, master_value: Some(m), slave_value: Some(v) });
            let kind = if tool == GraphTool::AddCorner { PointKind::Corner } else { PointKind::Bezier };
            let _ = app.dispatch(Command::SetReactionPoint { reaction: ri as u32, slave: si as u32, state: n, kind: Some(kind), in_handle: None, out_handle: None });
            selected = Some(n as usize);
        }
    }
    // Where the points are, for the regression tests' drags.
    let centres: Vec<egui::Pos2> = s.states.iter().map(|(m, v)| to(*m, *v)).collect();
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("reaction_graph_points"), centres));
    // The selected Bezier point's handles.
    let mut handle_ends = Vec::new();
    if let Some(k) = selected.filter(|k| s.point_of(*k).kind != PointKind::Corner) {
        let (m, v) = s.states[k];
        let c = to(m, v);
        for (side, h) in s.handles(k).into_iter().enumerate() {
            let end = to(m + h[0], v + h[1]);
            if end.distance(c) < 1.0 {
                continue;
            }
            p.line_segment([c, end], egui::Stroke::new(1.0_f32, egui::Color32::LIGHT_GRAY));
            let id = ui.id().with(("reaction_handle", ri, si, k, side));
            let r = ui.interact(egui::Rect::from_center_size(end, egui::vec2(9.0, 9.0)), id, egui::Sense::drag());
            let at = r.interact_pointer_pos().filter(|_| r.dragged()).unwrap_or(end);
            p.circle_filled(at, 3.5, egui::Color32::LIGHT_GRAY);
            handle_ends.push((side, end));
            if r.drag_stopped() {
                if let Some(q) = r.interact_pointer_pos() {
                    let (hm, hv) = from(q);
                    let off = [hm - m, hv - v];
                    let (inn, out) = if side == 0 { (Some(off), None) } else { (None, Some(off)) };
                    let _ = app.dispatch(Command::SetReactionPoint { reaction: ri as u32, slave: si as u32, state: k as u32, kind: None, in_handle: inn, out_handle: out });
                }
            }
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("reaction_graph_handles"), handle_ends));
    for (k, (m, v)) in s.states.iter().enumerate() {
        let c = to(*m, *v);
        let id = ui.id().with(("reaction_point", ri, si, k));
        let r = ui.interact(egui::Rect::from_center_size(c, egui::vec2(10.0, 10.0)), id, egui::Sense::click_and_drag());
        if r.clicked() || r.drag_started() {
            selected = Some(k);
        }
        let shown = match (r.dragged(), r.interact_pointer_pos()) {
            (true, Some(q)) => match tool {
                GraphTool::MoveHorizontal => egui::pos2(q.x, c.y),
                GraphTool::MoveVertical | GraphTool::Scale => egui::pos2(c.x, q.y),
                _ => q,
            },
            _ => c,
        };
        let fill = if r.dragged() || selected == Some(k) { egui::Color32::WHITE } else { egui::Color32::YELLOW };
        if s.point_of(k).kind == PointKind::Corner {
            p.rect_filled(egui::Rect::from_center_size(shown, egui::vec2(7.0, 7.0)), 0.0, fill);
        } else {
            p.circle_filled(shown, 4.0, fill);
        }
        if r.drag_stopped() {
            if let Some(q) = r.interact_pointer_pos() {
                let (nm, nv) = from(q);
                match tool {
                    GraphTool::MoveHorizontal => set(app, k, nm, *v),
                    GraphTool::MoveVertical => set(app, k, *m, nv),
                    // Scale Points: up doubles the value per 100 px, down halves it.
                    GraphTool::Scale => set(app, k, *m, *v * 2f32.powf((c.y - q.y) / 100.0)),
                    _ => set(app, k, nm, nv),
                }
            }
        }
        r.context_menu(|ui| {
            for (kind, label) in [(PointKind::Corner, "Corner"), (PointKind::Bezier, "Bezier Smooth"), (PointKind::BezierCorner, "Bezier Corner")] {
                if ui.radio(s.point_of(k).kind == kind, label).clicked() {
                    let _ = app.dispatch(Command::SetReactionPoint { reaction: ri as u32, slave: si as u32, state: k as u32, kind: Some(kind), in_handle: None, out_handle: None });
                    selected = Some(k);
                    ui.close_menu();
                }
            }
        });
    }
    ui.data_mut(|d| {
        d.insert_temp(tool_id, tool);
        d.insert_temp(sel_id, selected);
    });
}

/// Brush Presets > Brush Preset Manager: the context, the presets with their
/// settings (double-click a name to rename), Add, Duplicate, Delete, Icon
/// Size Min / Max, Save and Load (.bpr).
pub fn brush_preset_manager(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let fid = egui::Id::new("brush_preset_manager_form");
    let (mut hl, mut renaming, mut path): (Option<usize>, Option<(usize, String)>, String) = ctx.data(|d| d.get_temp(fid)).unwrap_or_default();
    let mut cmds = Vec::new();
    egui::Window::new("Brush Preset Manager").open(open).resizable(true).default_width(560.0).show(ctx, |ui| {
        ui.label(format!("Context: {}", loom_commands::brush_presets::context(&app.state).unwrap_or("none (start a paint tool)")));
        let b = app.state.prefs.brush_presets.clone();
        ui.horizontal(|ui| {
            for (label, c) in [
                ("Add", Some(Command::AddBrushPreset { name: None })),
                ("Duplicate", hl.map(|i| Command::DuplicateBrushPreset { index: i as u32, name: None })),
                ("Delete", hl.map(|i| Command::DeleteBrushPreset { index: i as u32 })),
            ] {
                if ui.add_enabled(c.is_some(), egui::Button::new(label)).clicked() {
                    cmds.extend(c);
                }
            }
            let (mut lo, mut hi) = (b.icon_min, b.icon_max);
            ui.label("Icon Size Min");
            let a = ui.add(crate::spinner::spin(ui, &mut lo, 0.0..).speed(0.1)).changed();
            ui.label("Max");
            let c = ui.add(crate::spinner::spin(ui, &mut hi, 0.0..).speed(0.1)).changed();
            if a || c {
                cmds.push(Command::SetBrushIconRange { min: lo, max: hi });
            }
        });
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            egui::Grid::new("brush_presets_list").striped(true).show(ui, |ui| {
                for h in ["Name", "Strength", "Size", "Mirror", "Mode", "Context"] {
                    ui.strong(h);
                }
                ui.end_row();
                for (i, p) in b.presets.iter().enumerate() {
                    match &mut renaming {
                        Some((k, text)) if *k == i => {
                            let r = ui.text_edit_singleline(text);
                            if r.lost_focus() {
                                cmds.push(Command::RenameBrushPreset { index: i as u32, name: text.clone() });
                                renaming = None;
                            } else {
                                r.request_focus();
                            }
                        }
                        _ => {
                            let r = ui.selectable_label(hl == Some(i), &p.name);
                            if r.clicked() {
                                hl = Some(i);
                            }
                            if r.double_clicked() {
                                renaming = Some((i, p.name.clone()));
                            }
                        }
                    }
                    let q = &p.painter;
                    ui.label(format!("{:.2}-{:.2}", q.min_strength, q.strength));
                    ui.label(format!("{:.1}-{:.1}", q.min_size, q.size));
                    ui.label(if q.mirror { format!("{} {:.1}", ["X", "Y", "Z"][q.mirror_axis.min(2) as usize], q.mirror_offset) } else { "Off".into() });
                    ui.label(p.deform_mode.clone().or(p.soft_mode.clone()).unwrap_or_default());
                    ui.label(match p.vp_color {
                        Some(c) => format!("RGB {:.0} {:.0} {:.0}", c[0] * 255.0, c[1] * 255.0, c[2] * 255.0),
                        None => String::new(),
                    });
                    ui.end_row();
                }
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("File");
            ui.text_edit_singleline(&mut path);
            if ui.add_enabled(!path.trim().is_empty(), egui::Button::new("Save")).clicked() {
                cmds.push(Command::SaveBrushPresets { path: path.clone() });
            }
            if ui.add_enabled(!path.trim().is_empty(), egui::Button::new("Load")).clicked() {
                cmds.push(Command::LoadBrushPresets { path: path.clone() });
            }
        });
    });
    for c in cmds {
        if let Err(e) = app.dispatch(c) {
            app.state.status_prompt = e;
        }
    }
    ctx.data_mut(|d| d.insert_temp(fid, (hl, renaming, path)));
}
