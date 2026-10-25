//! Grid and Snap Settings dialog: Snaps, Options and Home Grid tabs.

use crate::shortcuts::HoverShortcut;
use crate::LoomApp;
use eframe::egui;
use loom_commands::snap::SnapTargets;
use loom_commands::Command;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Snaps,
    Options,
    HomeGrid,
    UserGrids,
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let tab_id = egui::Id::new("grid_snap_tab");
    let mut tab: Tab = ctx.data(|d| d.get_temp(tab_id)).unwrap_or_default();
    egui::Window::new("Grid and Snap Settings").open(open).resizable(false).default_width(300.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut tab, Tab::Snaps, "Snaps");
            ui.selectable_value(&mut tab, Tab::Options, "Options");
            ui.selectable_value(&mut tab, Tab::HomeGrid, "Home Grid");
            ui.selectable_value(&mut tab, Tab::UserGrids, "User Grids");
        });
        ui.separator();
        match tab {
            Tab::Snaps => snaps_tab(ui, app),
            Tab::Options => options_tab(ui, app),
            Tab::HomeGrid => home_grid_tab(ui, app),
            Tab::UserGrids => user_grids_tab(ui, app),
        }
    });
    ctx.data_mut(|d| d.insert_temp(tab_id, tab));
}

fn snaps_tab(ui: &mut egui::Ui, app: &mut LoomApp) {
    ui.label(egui::RichText::new("Standard").strong());
    let targets = app.state.snaps.targets;
    egui::Grid::new("snap_targets").num_columns(2).spacing([24.0, 4.0]).show(ui, |ui| {
        // The twelve boxes are laid out in two columns, six per column.
        for row in 0..6 {
            for col in 0..2 {
                let name = SnapTargets::NAMES[col * 6 + row];
                let mut on = targets.get(name).unwrap_or(false);
                if ui.checkbox(&mut on, name).changed() {
                    let _ = app.dispatch(Command::SnapTarget { name: name.to_owned(), on });
                }
            }
            ui.end_row();
        }
    });
    ui.horizontal(|ui| {
        // The Snap Override in use (Shift+right-click Snaps menu).
        ui.label(match &app.state.snaps.override_target {
            Some(name) => format!("Override {name}"),
            None => "Override OFF".to_owned(),
        });
        if ui.button("Clear All").clicked() {
            for name in SnapTargets::NAMES {
                let _ = app.dispatch(Command::SnapTarget { name: name.to_owned(), on: false });
            }
        }
    });
}

fn user_grids_tab(ui: &mut egui::Ui, app: &mut LoomApp) {
    let u = app.state.user_grids;
    ui.label(egui::RichText::new("Grid Object Automation").strong());
    let mut on = u.activate_when_created;
    if ui.checkbox(&mut on, "Activate grids when creating").changed() {
        let _ = app.dispatch(Command::UserGridSettings { activate_when_created: Some(on), autogrid_object_space: None });
    }
    ui.label(egui::RichText::new("AutoGrid").strong());
    let mut object = u.autogrid_object_space;
    let a = ui.radio_value(&mut object, false, "World space").changed();
    let b = ui.radio_value(&mut object, true, "Object space").changed();
    if a || b {
        let _ = app.dispatch(Command::UserGridSettings { activate_when_created: None, autogrid_object_space: Some(object) });
    }
}

fn options_tab(ui: &mut egui::Ui, app: &mut LoomApp) {
    let s = app.state.snaps.clone();
    let speed = app.spinner_speed(0.5);
    let set = |app: &mut LoomApp, c: Command| {
        let _ = app.dispatch(c);
    };
    ui.label(egui::RichText::new("Marker").strong());
    ui.horizontal(|ui| {
        let mut d = s.marker_display;
        if ui.checkbox(&mut d, "Display").changed() {
            set(app, Command::SnapSettings { mode: None, marker_display: Some(d), marker_size: None, preview_radius_px: None, radius_px: None, angle_deg: None, percent: None, use_axis_constraints: None, show_rubber_band: None });
        }
        ui.label("Color:");
        let mut c = egui::Color32::from_rgb(s.marker_color[0], s.marker_color[1], s.marker_color[2]);
        if ui.color_edit_button_srgba(&mut c).changed() {
            set(app, Command::SnapMarkerColor { rgb: [c.r(), c.g(), c.b()] });
        }
        ui.label("Size:");
        let mut v = s.marker_size;
        if ui.add(crate::spinner::spin(ui, &mut v, 1.0..=100.0).speed(speed)).changed() {
            set(app, Command::SnapSettings { mode: None, marker_display: None, marker_size: Some(v), preview_radius_px: None, radius_px: None, angle_deg: None, percent: None, use_axis_constraints: None, show_rubber_band: None });
        }
    });
    ui.label(egui::RichText::new("General").strong());
    egui::Grid::new("snap_general").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        let mut v = s.preview_radius_px;
        ui.label("Snap Preview Radius:");
        if ui.add(crate::spinner::spin(ui, &mut v, 1.0..=200.0).speed(speed)).changed() {
            set(app, Command::SnapSettings { mode: None, marker_display: None, marker_size: None, preview_radius_px: Some(v), radius_px: None, angle_deg: None, percent: None, use_axis_constraints: None, show_rubber_band: None });
        }
        ui.end_row();
        let mut v = s.radius_px;
        ui.label("Snap Radius:");
        if ui.add(crate::spinner::spin(ui, &mut v, 1.0..=200.0).speed(speed)).changed() {
            set(app, Command::SnapSettings { mode: None, marker_display: None, marker_size: None, preview_radius_px: None, radius_px: Some(v), angle_deg: None, percent: None, use_axis_constraints: None, show_rubber_band: None });
        }
        ui.end_row();
        let mut v = s.angle_deg;
        ui.label("Angle (deg):");
        if ui.add(crate::spinner::spin(ui, &mut v, 0.0..=180.0).speed(speed)).changed() {
            set(app, Command::SnapSettings { mode: None, marker_display: None, marker_size: None, preview_radius_px: None, radius_px: None, angle_deg: Some(v), percent: None, use_axis_constraints: None, show_rubber_band: None });
        }
        ui.end_row();
        let mut v = s.percent;
        ui.label("Percent (%):");
        if ui.add(crate::spinner::spin(ui, &mut v, 0.0..=100.0).speed(speed)).changed() {
            set(app, Command::SnapSettings { mode: None, marker_display: None, marker_size: None, preview_radius_px: None, radius_px: None, angle_deg: None, percent: Some(v), use_axis_constraints: None, show_rubber_band: None });
        }
        ui.end_row();
    });
    let mut f = s.snap_to_frozen;
    if ui.checkbox(&mut f, "Snap to frozen objects").with_shortcut(&app.state, Command::SnapToFrozenObjectsToggle.name()).changed() {
        set(app, Command::SnapToFrozenObjectsToggle);
    }
    ui.label(egui::RichText::new("Translation").strong());
    let mut a = s.use_axis_constraints;
    if ui.checkbox(&mut a, "Use Axis Constraints").with_shortcut(&app.state, Command::EnableAxisConstraintsInSnapsToggle.name()).changed() {
        set(app, Command::EnableAxisConstraintsInSnapsToggle);
    }
    let mut r = s.show_rubber_band;
    if ui.checkbox(&mut r, "Display rubber band").changed() {
        set(app, Command::SnapSettings { mode: None, marker_display: None, marker_size: None, preview_radius_px: None, radius_px: None, angle_deg: None, percent: None, use_axis_constraints: None, show_rubber_band: Some(r) });
    }
}

fn home_grid_tab(ui: &mut egui::Ui, app: &mut LoomApp) {
    let g = app.state.home_grid;
    let speed = app.spinner_speed(0.1);
    ui.label(egui::RichText::new("Grid Dimensions").strong());
    egui::Grid::new("home_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        let mut v = g.spacing;
        ui.label("Grid Spacing:");
        if ui.add(crate::spinner::spin(ui, &mut v, 0.001..=1e6).speed(speed)).changed() {
            let _ = app.dispatch(Command::HomeGrid { spacing: Some(v), major_every: None, perspective_extent: None, inhibit_subdivision: None, inhibit_resize: None, dynamic_update_all: None });
        }
        ui.end_row();
        let mut v = g.major_every;
        ui.label("Major Lines every Nth Grid Line:");
        if ui.add(crate::spinner::spin(ui, &mut v, 2..=100).speed(0.1)).changed() {
            let _ = app.dispatch(Command::HomeGrid { spacing: None, major_every: Some(v), perspective_extent: None, inhibit_subdivision: None, inhibit_resize: None, dynamic_update_all: None });
        }
        ui.end_row();
        let mut v = g.perspective_extent;
        ui.label("Perspective View Grid Extent:");
        if ui.add(crate::spinner::spin(ui, &mut v, 1..=100).speed(0.1)).changed() {
            let _ = app.dispatch(Command::HomeGrid { spacing: None, major_every: None, perspective_extent: Some(v), inhibit_subdivision: None, inhibit_resize: None, dynamic_update_all: None });
        }
        ui.end_row();
    });
    let mut i = g.inhibit_subdivision;
    if ui.checkbox(&mut i, "Inhibit Grid Subdivision Below Grid Spacing").changed() {
        let _ = app.dispatch(Command::HomeGrid { spacing: None, major_every: None, perspective_extent: None, inhibit_subdivision: Some(i), inhibit_resize: None, dynamic_update_all: None });
    }
    let mut r = g.inhibit_resize;
    if ui.checkbox(&mut r, "Inhibit Perspective View Grid Resize").changed() {
        let _ = app.dispatch(Command::HomeGrid { spacing: None, major_every: None, perspective_extent: None, inhibit_subdivision: None, inhibit_resize: Some(r), dynamic_update_all: None });
    }
    ui.label(egui::RichText::new("Dynamic Update").strong());
    ui.horizontal(|ui| {
        let mut all = g.dynamic_update_all;
        if ui.radio_value(&mut all, false, "Active Viewport").changed() || ui.radio_value(&mut all, true, "All Viewports").changed() {
            let _ = app.dispatch(Command::HomeGrid { spacing: None, major_every: None, perspective_extent: None, inhibit_subdivision: None, inhibit_resize: None, dynamic_update_all: Some(all) });
        }
    });
}
