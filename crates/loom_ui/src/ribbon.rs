//! Graphite Modeling Tools ribbon: tabs Graphite Modeling Tools /
//! Freeform / Selection, docked under the Main Toolbar, toggled by the
//! toolbar button or Customize > Show UI > Show Ribbon. Every button
//! dispatches the same command as its Modify-panel counterpart.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, PaintMode, PolyDrag, PreviewSelect, SelectionMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RibbonTab {
    #[default]
    Modeling,
    Freeform,
    Selection,
}

impl RibbonTab {
    pub const ALL: [RibbonTab; 3] = [RibbonTab::Modeling, RibbonTab::Freeform, RibbonTab::Selection];
    pub fn label(self) -> &'static str {
        match self {
            RibbonTab::Modeling => "Graphite Modeling Tools",
            RibbonTab::Freeform => "Freeform",
            RibbonTab::Selection => "Selection",
        }
    }
}

/// A plain ribbon button: (tab, panel, label, command name). Buttons that
/// need arguments or toggle state are drawn by `show` directly.
pub const BUTTONS: &[(RibbonTab, &str, &str, &str)] = &[
    (RibbonTab::Modeling, "Polygon Modeling", "Repeat Last", "Repeat Last Operation"),
    (RibbonTab::Modeling, "Polygon Modeling", "Use Soft Selection", "Use Soft Selection"),
    (RibbonTab::Modeling, "Polygon Modeling", "Ignore Backfacing", "Ignore Backfacing in Selections"),
    (RibbonTab::Modeling, "Edit", "Cut", "Cut"),
    (RibbonTab::Modeling, "Edit", "QuickSlice", "Quickslice Mode"),
    (RibbonTab::Modeling, "Edit", "Extrude", "Extrude Mode"),
    (RibbonTab::Modeling, "Edit", "Bevel", "Bevel Mode"),
    (RibbonTab::Modeling, "Edit", "Inset", "Inset Mode"),
    (RibbonTab::Modeling, "Edit", "Outline", "Outline Mode"),
    (RibbonTab::Modeling, "Edit", "Chamfer", "Chamfer Mode"),
    (RibbonTab::Modeling, "Edit", "Hinge", "Hinge from Edge Mode"),
    (RibbonTab::Modeling, "Edit", "Target Weld", "Target Weld Mode"),
    (RibbonTab::Modeling, "Edit", "Connect", "Connect"),
    (RibbonTab::Modeling, "Edit", "Remove", "Remove"),
    (RibbonTab::Modeling, "Edit", "Cap", "Cap"),
    (RibbonTab::Modeling, "Edit", "Flip", "Flip Normals"),
    (RibbonTab::Modeling, "Edit", "Make Planar", "Make Planar"),
    (RibbonTab::Modeling, "Geometry (All)", "Attach", "Attach"),
    (RibbonTab::Modeling, "Geometry (All)", "Detach", "Detach"),
    (RibbonTab::Modeling, "Geometry (All)", "Collapse", "Collapse"),
    (RibbonTab::Modeling, "Geometry (All)", "Slice Plane", "Slice Plane Mode"),
    (RibbonTab::Modeling, "Geometry (All)", "MSmooth", "MeshSmooth"),
    (RibbonTab::Modeling, "Geometry (All)", "Tessellate", "Tessellate"),
    (RibbonTab::Modeling, "Geometry (All)", "Hide", "Hide"),
    (RibbonTab::Modeling, "Geometry (All)", "Hide Unselected", "Hide Unselected"),
    (RibbonTab::Modeling, "Geometry (All)", "Unhide All", "Unhide All"),
    (RibbonTab::Modeling, "Align", "Make Planar", "Make Planar"),
    (RibbonTab::Modeling, "Align", "View Align", "Align To View"),
    (RibbonTab::Modeling, "Align", "Grid Align", "Align To Grid"),
    (RibbonTab::Modeling, "Properties", "Auto Smooth", "Auto Smooth"),
    (RibbonTab::Modeling, "Properties", "Clear SG", "Clear Smoothing Groups"),
    (RibbonTab::Freeform, "Paint Deform", "Push/Pull", "Paint Deform Push/Pull"),
    (RibbonTab::Freeform, "Paint Deform", "Relax", "Paint Deform Relax"),
    (RibbonTab::Freeform, "Paint Deform", "Revert", "Paint Deform Revert"),
    (RibbonTab::Freeform, "Paint Deform", "Commit", "Paint Deform Commit"),
    (RibbonTab::Freeform, "Paint Deform", "Cancel", "Paint Deform Cancel"),
    (RibbonTab::Selection, "Modify Selection", "Grow", "Grow Selection"),
    (RibbonTab::Selection, "Modify Selection", "Shrink", "Shrink Selection"),
    (RibbonTab::Selection, "Modify Selection", "Loop", "Select Edge Loop"),
    (RibbonTab::Selection, "Modify Selection", "Ring", "Select Edge Ring"),
    (RibbonTab::Selection, "Modify Selection", "By Vertex", "Select By Vertex"),
    (RibbonTab::Selection, "Modify Selection", "Ignore Backfacing", "Ignore Backfacing in Selections"),
    (RibbonTab::Selection, "Modify Selection", "Preview", "Preview Select Mode"),
];

/// Is the button a toggle that is on right now?
fn on(app: &LoomApp, action: &str) -> bool {
    let s = &app.state;
    let poly = s.selection.iter().next().and_then(|id| s.scene.node(*id)).and_then(|n| n.edit_poly());
    match action {
        "Use Soft Selection" => poly.is_some_and(|p| p.soft.on),
        "Ignore Backfacing in Selections" => s.ignore_backfacing,
        "Select By Vertex" => s.select_by_vertex,
        "Preview Select Mode" => s.preview_selection != PreviewSelect::Off,
        "Cut" => matches!(s.pick_mode, Some(loom_commands::PickMode::CutA | loom_commands::PickMode::CutB)),
        "Quickslice Mode" => matches!(s.pick_mode, Some(loom_commands::PickMode::QuickSliceA | loom_commands::PickMode::QuickSliceB)),
        "Target Weld Mode" => matches!(s.pick_mode, Some(loom_commands::PickMode::TargetWeldSource | loom_commands::PickMode::TargetWeldTarget)),
        "Attach" => s.pick_mode == Some(loom_commands::PickMode::AttachObject),
        "Extrude Mode" => s.poly_drag == Some(PolyDrag::Extrude),
        "Bevel Mode" => s.poly_drag == Some(PolyDrag::Bevel),
        "Inset Mode" => s.poly_drag == Some(PolyDrag::Inset),
        "Outline Mode" => s.poly_drag == Some(PolyDrag::Outline),
        "Chamfer Mode" => s.poly_drag == Some(PolyDrag::Chamfer),
        "Hinge from Edge Mode" => s.poly_drag == Some(PolyDrag::Hinge),
        "Slice Plane Mode" => s.slice_plane_mode,
        "Paint Deform Push/Pull" => s.paint_deform == Some(PaintMode::PushPull),
        "Paint Deform Relax" => s.paint_deform == Some(PaintMode::Relax),
        "Paint Deform Revert" => s.paint_deform == Some(PaintMode::Revert),
        _ => false,
    }
}

/// One ribbon panel: a fixed-width group of wrapped buttons with its title
/// underneath (explicit widths keep nested wrapping from growing unbounded).
fn panel(ui: &mut egui::Ui, title: &str, width: f32, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style()).inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
        ui.set_max_width(width);
        ui.set_min_height(44.0);
        ui.vertical(|ui| {
            ui.set_max_width(width);
            ui.horizontal_wrapped(|ui| {
                ui.set_max_width(width);
                ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
                body(ui);
            });
            ui.label(egui::RichText::new(title).small().weak());
        });
    });
}

fn buttons(ui: &mut egui::Ui, app: &mut LoomApp, tab: RibbonTab, panel_title: &str) {
    for (t, p, label, action) in BUTTONS.iter().filter(|(t, p, _, _)| *t == tab && *p == panel_title) {
        let _ = t;
        let _ = p;
        if ui.selectable_label(on(app, action), *label).on_hover_text(crate::shortcuts::name_tip(&app.state, action)).clicked() {
            app.dispatch_named(action);
        }
    }
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp) {
    if !app.state.show_ribbon || app.state.expert_mode {
        return;
    }
    egui::TopBottomPanel::top("ribbon").resizable(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            for tab in RibbonTab::ALL {
                let r = ui.selectable_label(app.ribbon_tab == tab, tab.label());
                if r.clicked() {
                    app.ribbon_tab = tab;
                }
                // Double-click a tab minimizes the ribbon to its tab bar.
                if r.double_clicked() {
                    app.ribbon_minimized = !app.ribbon_minimized;
                }
            }
            ui.add_space(8.0);
            if ui.small_button(if app.ribbon_minimized { crate::icons::DOWN } else { crate::icons::UP }).on_hover_text("Minimize / show the ribbon panels").clicked() {
                app.ribbon_minimized = !app.ribbon_minimized;
            }
        });
        if app.ribbon_minimized {
            return;
        }
        let poly = app.state.poly_selected();
        ui.add_enabled_ui(poly, |ui| {
            egui::ScrollArea::horizontal().auto_shrink([false, true]).show(ui, |ui| {
                ui.horizontal(|ui| match app.ribbon_tab {
                    RibbonTab::Modeling => modeling_tab(ui, app),
                    RibbonTab::Freeform => {
                        panel(ui, "Paint Deform", 520.0, |ui| {
                            buttons(ui, app, RibbonTab::Freeform, "Paint Deform");
                            let paint = app.state.selection.iter().next().and_then(|id| app.state.scene.node(*id)).and_then(|n| n.edit_poly()).map(|p| p.paint.clone());
                            if let Some(p) = paint {
                                let (mut size, mut strength) = (p.brush_size, p.brush_strength);
                                ui.label("Size");
                                if ui.add(crate::spinner::spin(ui, &mut size, 0.01..=f32::MAX).speed(0.1)).changed() {
                                    let _ = app.dispatch(Command::PaintDeformSettings { value: None, size: Some(size), strength: None, direction: None });
                                }
                                ui.label("Strength");
                                if ui.add(crate::spinner::spin(ui, &mut strength, 0.0..=1.0).speed(0.01)).changed() {
                                    let _ = app.dispatch(Command::PaintDeformSettings { value: None, size: None, strength: Some(strength), direction: None });
                                }
                            }
                        });
                    }
                    RibbonTab::Selection => {
                        panel(ui, "Modify Selection", 560.0, |ui| buttons(ui, app, RibbonTab::Selection, "Modify Selection"));
                        panel(ui, "By Material / SG", 360.0, |ui| {
                            let mut id: u16 = ui.ctx().data(|d| d.get_temp(egui::Id::new("ribbon_mat_id"))).unwrap_or(1);
                            ui.label("ID");
                            if ui.add(crate::spinner::spin(ui, &mut id, 1..=65535).speed(0.1)).changed() {
                                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("ribbon_mat_id"), id));
                            }
                            if ui.button("By Material ID").clicked() {
                                let _ = app.dispatch(Command::SelectByMaterialId { id, clear: true });
                            }
                            if ui.button("By Smoothing Group").on_hover_text("Polygons sharing a group with the selection").clicked() {
                                let mut bits = 0u32;
                                for (n, ids) in app.state.sub_selection.by_node() {
                                    if let Some(node) = app.state.scene.node(n) {
                                        for id in ids {
                                            if let loom_commands::SubObjectId::Face(f) = id {
                                                bits |= node.cage_mesh().faces().get(f.0 as usize).map(|x| x.smoothing_groups).unwrap_or(0);
                                            }
                                        }
                                    }
                                }
                                let _ = app.dispatch(Command::SelectBySmoothingGroup { bits, clear: true });
                            }
                        });
                    }
                });
            });
        });
    });
}

fn modeling_tab(ui: &mut egui::Ui, app: &mut LoomApp) {
    panel(ui, "Polygon Modeling", 380.0, |ui| {
        for mode in SelectionMode::ALL {
            if mode == SelectionMode::Object {
                continue;
            }
            let selected = app.state.selection_mode == mode;
            if ui.selectable_label(selected, mode.label()).clicked() {
                let _ = app.dispatch(Command::SelectionMode { mode: if selected { SelectionMode::Object } else { mode } });
            }
        }
        buttons(ui, app, RibbonTab::Modeling, "Polygon Modeling");
        let has_stack = app.state.selection.iter().any(|id| app.state.scene.node(*id).is_some_and(|n| !n.modifiers.is_empty()));
        if ui.add_enabled(has_stack, egui::Button::new("Collapse Stack")).on_hover_text("Collapse the whole modifier stack into an Editable Poly").clicked() {
            let _ = app.dispatch(Command::CollapseAll);
        }
        if ui.add_enabled(has_stack, egui::Button::new("Make Unique")).on_hover_text("Make the instanced modifier / object unique").clicked() {
            let _ = app.dispatch(Command::MakeUnique);
        }
        egui::ComboBox::from_id_salt("ribbon_modifier_list").selected_text("Modifier List").width(120.0).show_ui(ui, |ui| {
            for name in loom_scene::Modifier::LIST {
                if ui.button(*name).clicked() {
                    let _ = app.dispatch(Command::AddModifier { name: (*name).to_owned() });
                }
            }
        });
    });
    panel(ui, "Edit", 420.0, |ui| {
        ui.add_enabled(false, egui::Button::new("Swift Loop")).on_disabled_hover_text("Swift Loop (insert a loop at the cursor) is not implemented yet");
        buttons(ui, app, RibbonTab::Modeling, "Edit");
        let ps = app.state.poly_settings;
        if ui.button("Weld").clicked() {
            let _ = app.dispatch(Command::Weld { threshold: ps.weld_threshold });
        }
        if ui.button("Bridge").clicked() {
            let _ = app.dispatch(Command::Bridge { segments: ps.bridge_segments, smooth: ps.bridge_smooth, bridge_adjacent: ps.bridge_adjacent, twist1: ps.bridge_twist1, twist2: ps.bridge_twist2 });
        }
    });
    panel(ui, "Geometry (All)", 300.0, |ui| {
        buttons(ui, app, RibbonTab::Modeling, "Geometry (All)");
        let ps = app.state.poly_settings;
        if ui.button("Relax").clicked() {
            let _ = app.dispatch(Command::Relax { amount: ps.relax_amount, iterations: ps.relax_iterations, hold_boundary: ps.relax_hold_boundary, hold_outer: ps.relax_hold_outer });
        }
    });
    panel(ui, "Subdivision", 170.0, |ui| {
        let sub = app.state.selection.iter().next().and_then(|id| app.state.scene.node(*id)).and_then(|n| n.edit_poly()).map(|p| p.subdiv.clone());
        if let Some(s) = sub {
            let mut nurms = s.use_nurms.get();
            if ui.selectable_label(nurms, "Use NURMS").clicked() {
                nurms = !nurms;
                let _ = app.dispatch(Command::SubdivisionSurface { use_nurms: Some(nurms), smooth_result: None, isoline_display: None, show_cage: None, iterations: None, smoothness: None, separate_by_smoothing: None, separate_by_materials: None });
            }
            let mut it = s.iterations.get();
            ui.label("Iterations");
            if ui.add(crate::spinner::spin(ui, &mut it, 0..=4).speed(0.1)).changed() {
                let _ = app.dispatch(Command::SubdivisionSurface { use_nurms: None, smooth_result: None, isoline_display: None, show_cage: None, iterations: Some(it), smoothness: None, separate_by_smoothing: None, separate_by_materials: None });
            }
        }
    });
    panel(ui, "Align", 200.0, |ui| {
        buttons(ui, app, RibbonTab::Modeling, "Align");
        for (a, l) in [(loom_commands::Axis::X, "X"), (loom_commands::Axis::Y, "Y"), (loom_commands::Axis::Z, "Z")] {
            if ui.button(l).on_hover_text(format!("Make Planar {l}")).clicked() {
                let _ = app.dispatch(Command::MakePlanarAxis { axis: a });
            }
        }
    });
    panel(ui, "Properties", 220.0, |ui| {
        let mut id: u16 = ui.ctx().data(|d| d.get_temp(egui::Id::new("ribbon_set_id"))).unwrap_or(1);
        ui.label("Mat ID");
        if ui.add(crate::spinner::spin(ui, &mut id, 1..=65535).speed(0.1)).changed() {
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("ribbon_set_id"), id));
        }
        if ui.button("Set ID").clicked() {
            let _ = app.dispatch(Command::SetMaterialId { id });
        }
        buttons(ui, app, RibbonTab::Modeling, "Properties");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ribbon_button_dispatches_a_registered_command() {
        let registered: std::collections::HashSet<&str> = loom_commands::registry().iter().map(|i| i.name).collect();
        for (_, _, label, action) in BUTTONS {
            assert!(registered.contains(action), "ribbon button {label:?} dispatches unregistered {action:?}");
        }
        // Each tab has at least one panel of buttons.
        for tab in RibbonTab::ALL {
            assert!(BUTTONS.iter().any(|(t, _, _, _)| *t == tab), "{tab:?} has no buttons");
        }
    }
}
