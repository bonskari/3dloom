//! Hierarchy tab: Pivot (Adjust Pivot, Adjust Transform), IK (solver, properties,
//! Spline IK shape) and Link Info (Locks, Inherit).

use super::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;
use loom_scene::Axes3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sub {
    #[default]
    Pivot,
    Ik,
    LinkInfo,
}

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    ui.horizontal(|ui| {
        ui.selectable_value(&mut app.hierarchy_sub, Sub::Pivot, "Pivot");
        ui.selectable_value(&mut app.hierarchy_sub, Sub::Ik, "IK");
        ui.selectable_value(&mut app.hierarchy_sub, Sub::LinkInfo, "Link Info");
    });
    match app.hierarchy_sub {
        Sub::Pivot => pivot(ui, app),
        Sub::Ik => ik(ui, app),
        Sub::LinkInfo => link_info(ui, app),
    }
}

fn pivot(ui: &mut egui::Ui, app: &mut LoomApp) {
    rollout(ui, "Adjust Pivot", true, |ui| {
        ui.label("Move/Rotate/Scale:");
        let w = ui.available_width();
        if ui.add_sized([w, 22.0], egui::SelectableLabel::new(app.state.affect_pivot_only, "Affect Pivot Only")).clicked() {
            let _ = app.dispatch(Command::AffectPivotOnly);
        }
        if ui.add_sized([w, 22.0], egui::SelectableLabel::new(app.state.affect_object_only, "Affect Object Only")).clicked() {
            let _ = app.dispatch(Command::AffectObjectOnly);
        }
        if ui.add_sized([w, 22.0], egui::SelectableLabel::new(app.state.affect_hierarchy_only, "Affect Hierarchy Only")).clicked() {
            let _ = app.dispatch(Command::AffectHierarchyOnly);
        }
        ui.separator();
        ui.label("Alignment:");
        let has = !app.state.selection.is_empty();
        // The buttons act on the pivot (Affect Pivot Only) or the object
        // (Affect Object Only), and are off otherwise.
        let object = app.state.affect_object_only;
        let buttons = if object {
            [("Center to Pivot", Command::CenterObjectToPivot), ("Align to Pivot", Command::AlignObjectToPivot), ("Align to World", Command::AlignPivotToWorld)]
        } else {
            [("Center to Object", Command::CenterPivotToObject), ("Align to Object", Command::AlignPivotToObject), ("Align to World", Command::AlignPivotToWorld)]
        };
        ui.add_enabled_ui(has && (object || app.state.affect_pivot_only), |ui| {
            for (label, cmd) in buttons {
                if ui.add_sized([w, 22.0], egui::Button::new(label)).clicked() {
                    let _ = app.dispatch(cmd);
                }
            }
        });
        ui.separator();
        ui.label("Pivot:");
        ui.add_enabled_ui(has, |ui| {
            if ui.add_sized([w, 22.0], egui::Button::new("Reset Pivot")).clicked() {
                let _ = app.dispatch(Command::ResetPivot);
            }
        });
    });
    rollout(ui, "Working Pivot", true, |ui| {
        let w = ui.available_width();
        if ui.add_sized([w, 22.0], egui::SelectableLabel::new(app.state.edit_working_pivot, "Edit Working Pivot")).clicked() {
            let _ = app.dispatch(Command::EditWorkingPivot);
        }
        if ui.add_sized([w, 22.0], egui::SelectableLabel::new(app.state.use_working_pivot, "Use Working Pivot")).clicked() {
            let _ = app.dispatch(Command::UseWorkingPivot);
        }
        ui.add_enabled_ui(app.state.edit_working_pivot || app.state.use_working_pivot, |ui| {
            if ui.add_sized([w, 22.0], egui::Button::new("Align To View")).clicked() {
                let _ = app.dispatch(Command::WorkingPivotAlignToView);
            }
        });
        if ui.add_sized([w, 22.0], egui::Button::new("Reset")).clicked() {
            let _ = app.dispatch(Command::WorkingPivotReset);
        }
        ui.label("Place Pivot To:");
        ui.horizontal(|ui| {
            let pm = app.state.pick_mode;
            if ui.selectable_label(pm == Some(loom_commands::PickMode::WorkingPivotView), "View").clicked() {
                let _ = app.dispatch(Command::PlacePivotToView);
            }
            if ui.selectable_label(pm == Some(loom_commands::PickMode::WorkingPivotSurface), "Surface").clicked() {
                let _ = app.dispatch(Command::PlacePivotToSurface);
            }
        });
        let mut atv = app.state.working_pivot_align_to_view;
        if ui.checkbox(&mut atv, "Align To View").changed() {
            let _ = app.dispatch(Command::PlacePivotAlignToViewToggle);
        }
    });
    rollout(ui, "Adjust Transform", true, |ui| {
        let w = ui.available_width();
        ui.label("Move/Rotate/Scale:");
        if ui.add_sized([w, 22.0], egui::SelectableLabel::new(app.state.dont_affect_children, "Don't Affect Children")).clicked() {
            let _ = app.dispatch(Command::DontAffectChildren);
        }
        ui.separator();
        ui.label("Reset:");
        ui.add_enabled_ui(!app.state.selection.is_empty(), |ui| {
            if ui.add_sized([w, 22.0], egui::Button::new("Transform")).clicked() {
                let _ = app.dispatch(Command::ResetTransform);
            }
            if ui.add_sized([w, 22.0], egui::Button::new("Scale")).clicked() {
                let _ = app.dispatch(Command::ResetScale);
            }
            ui.separator();
            ui.label("Freeze:");
            if ui.add_sized([w, 22.0], egui::Button::new("Freeze Transform")).clicked() {
                let _ = app.dispatch(Command::FreezeTransform);
            }
            if ui.add_sized([w, 22.0], egui::Button::new("Transform to Zero")).clicked() {
                let _ = app.dispatch(Command::TransformToZero);
            }
        });
    });
}

fn axes_row(ui: &mut egui::Ui, label: &str, mut v: [bool; 3]) -> Option<[bool; 3]> {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(format!("{label}:"));
        for (b, l) in v.iter_mut().zip(["X", "Y", "Z"]) {
            changed |= ui.checkbox(b, l).changed();
        }
    });
    changed.then_some(v)
}

fn link_info(ui: &mut egui::Ui, app: &mut LoomApp) {
    let Some(id) = app.state.selection.iter().next().copied() else {
        return;
    };
    let Some(node) = app.state.scene.node(id) else { return };
    let (locks, inherit) = (node.locks, node.inherit);
    rollout(ui, "Locks", true, |ui| {
        axes_group(ui, app, id, locks, true);
    });
    rollout(ui, "Inherit", true, |ui| {
        axes_group(ui, app, id, inherit, false);
    });
}

fn axes_group(ui: &mut egui::Ui, app: &mut LoomApp, id: loom_scene::NodeId, a: Axes3, locks: bool) {
    let (mut m, mut r, mut s) = (None, None, None);
    if let Some(v) = axes_row(ui, "Move", a.move_axes) {
        m = Some(v);
    }
    if let Some(v) = axes_row(ui, "Rotate", a.rotate_axes) {
        r = Some(v);
    }
    if let Some(v) = axes_row(ui, "Scale", a.scale_axes) {
        s = Some(v);
    }
    if m.is_some() || r.is_some() || s.is_some() {
        let _ = app.dispatch(if locks {
            Command::SetNodeLocks { id: id.0, move_axes: m, rotate_axes: r, scale_axes: s }
        } else {
            Command::SetNodeInherit { id: id.0, move_axes: m, rotate_axes: r, scale_axes: s }
        });
    }
}

fn ik(ui: &mut egui::Ui, app: &mut LoomApp) {
    use loom_scene::ik::IkSolver;
    let selected = app.state.selection.iter().next().copied();
    let chain = selected.and_then(|id| app.state.scene.ik_chain_of(id).cloned());
    rollout(ui, "IK Solver", true, |ui| {
        let w = ui.available_width();
        let Some(c) = chain.as_ref() else {
            ui.label("Select bones, then assign a solver:");
            for (label, cmd) in [("HI Solver", Command::HistoryIndependentIkSolver), ("HD Solver", Command::HistoryDependentIkSolver), ("IK Limb Solver", Command::IkLimbSolver), ("Spline IK Solver", Command::SplineIkSolver)] {
                if ui.add_sized([w, 22.0], egui::Button::new(label)).clicked() {
                    let _ = app.dispatch(cmd);
                }
            }
            return;
        };
        ui.label(&c.name);
        let mut solver = c.solver;
        egui::ComboBox::from_id_salt("ik_solver").selected_text(solver.label()).show_ui(ui, |ui| {
            for s in [IkSolver::HI, IkSolver::HD, IkSolver::Limb, IkSolver::Spline] {
                ui.selectable_value(&mut solver, s, s.label());
            }
        });
        if solver != c.solver {
            let v = match solver { IkSolver::HI => 0.0, IkSolver::Limb => 1.0, IkSolver::Spline => 2.0, IkSolver::HD => 3.0 };
            let _ = app.dispatch(Command::SetIkChainParameter { chain: c.id, name: "Solver".into(), value: v });
        }
        if ui.add_sized([w, 22.0], egui::SelectableLabel::new(c.enabled, "Enabled")).clicked() {
            let _ = app.dispatch(Command::SetIkChainParameter { chain: c.id, name: "Enabled".into(), value: if c.enabled { 0.0 } else { 1.0 } });
        }
        let mut fk = c.ik_for_fk;
        if ui.checkbox(&mut fk, "IK for FK Pose").changed() {
            let _ = app.dispatch(Command::SetIkChainParameter { chain: c.id, name: "IK For FK Pose".into(), value: if fk { 1.0 } else { 0.0 } });
        }
        ui.label(format!("Start Joint: {}", app.state.scene.node(c.start).map(|n| n.name.clone()).unwrap_or_default()));
        ui.label(format!("End Joint: {}", app.state.scene.node(c.end).map(|n| n.name.clone()).unwrap_or_default()));
        ui.horizontal(|ui| {
            for (label, cmd) in [("FK Snap", Command::IkChainFkSnap), ("IK Snap", Command::IkChainIkSnap)] {
                if ui.button(label).clicked() {
                    let _ = app.dispatch(cmd);
                }
            }
        });
    });
    let Some(c) = chain else { return };
    rollout(ui, "IK Solver Properties", true, |ui| {
        let speed = app.spinner_speed(0.5);
        egui::Grid::new("ik_props").num_columns(2).show(ui, |ui| {
            for (label, name, value, range) in [("Swivel Angle:", "Swivel Angle", c.swivel_deg as f64, -360.0..=360.0), ("Threshold:", "Threshold", c.threshold as f64, 0.001..=100.0), ("Iterations:", "Iterations", c.iterations as f64, 1.0..=1000.0)] {
                ui.label(label);
                let mut v = value;
                if ui.add(crate::spinner::spin(ui, &mut v, range).speed(speed)).changed() {
                    let _ = app.dispatch(Command::SetIkChainParameter { chain: c.id, name: name.into(), value: v });
                }
                ui.end_row();
            }
        });
    });
    if c.solver == IkSolver::Spline {
        rollout(ui, "Spline IK Solver", true, |ui| {
            let current = c.spline.and_then(|s| app.state.scene.node(s)).map(|n| n.name.clone()).unwrap_or_else(|| "None".into());
            let shapes: Vec<(loom_scene::NodeId, String)> = app.state.scene.nodes().filter(|n| n.object.shape().is_some()).map(|n| (n.id, n.name.clone())).collect();
            egui::ComboBox::from_id_salt("spline_ik_shape").selected_text(format!("Pick Shape: {current}")).show_ui(ui, |ui| {
                for (id, name) in shapes {
                    if ui.selectable_label(c.spline == Some(id), name).clicked() {
                        let _ = app.dispatch(Command::PickSplineIkShape { chain: c.id, node: id.0 });
                    }
                }
            });
        });
    }
}
