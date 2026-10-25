//! Units Setup dialog: the System Unit Setup button (its own dialog: 1 Unit
//! =, Respect System Units in Files, Origin and the resulting accuracy) and
//! Display Unit Scale (Metric, US Standard with the fractional units and
//! Default Units, Custom, Generic). Also the Units Mismatch dialog a file in
//! other system units opens.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, DisplayUnits, SystemUnit};

fn units_cmd() -> Command {
    Command::SetUnits { system_unit: None, system_scale: None, display: None, custom_name: None, custom_value: None, custom_unit: None, fraction: None, default_feet: None, respect_system_units: None }
}

fn set(app: &mut LoomApp, display: Option<&str>, custom: (Option<String>, Option<f32>, Option<&str>)) {
    let mut c = units_cmd();
    if let Command::SetUnits { display: d, custom_name, custom_value, custom_unit, .. } = &mut c {
        *d = display.map(str::to_owned);
        *custom_name = custom.0;
        *custom_value = custom.1;
        *custom_unit = custom.2.map(str::to_owned);
    }
    let _ = app.dispatch(c);
}

fn with(f: impl FnOnce(&mut Command)) -> Command {
    let mut c = units_cmd();
    f(&mut c);
    c
}

/// egui memory: the System Unit Setup dialog is open.
const SYSTEM_ID: &str = "system_unit_setup_open";

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let units = app.state.units.clone();
    let speed = app.spinner_speed(0.1);
    egui::Window::new("Units Setup").open(open).resizable(false).show(ctx, |ui| {
        if ui.button("System Unit Setup").clicked() {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new(SYSTEM_ID), true));
        }
        ui.label(egui::RichText::new(format!("1 Unit = {} {}", units.system_scale, units.system_unit.label())).small().weak());
        ui.group(|ui| {
            ui.label(egui::RichText::new("Display Unit Scale").strong());
            let label = units.display.label();
            let metric = matches!(units.display, DisplayUnits::Millimeters | DisplayUnits::Centimeters | DisplayUnits::Meters | DisplayUnits::Kilometers);
            let us = DisplayUnits::US.contains(&label);
            ui.horizontal(|ui| {
                if ui.radio(metric, "Metric").clicked() && !metric {
                    set(app, Some("Meters"), (None, None, None));
                }
                egui::ComboBox::from_id_salt("metric_unit").selected_text(if metric { label } else { "Meters" }).show_ui(ui, |ui| {
                    for n in ["Millimeters", "Centimeters", "Meters", "Kilometers"] {
                        if ui.selectable_label(label == n, n).clicked() {
                            set(app, Some(n), (None, None, None));
                        }
                    }
                });
            });
            ui.horizontal(|ui| {
                if ui.radio(us, "US Standard").clicked() && !us {
                    set(app, Some("Decimal Inches"), (None, None, None));
                }
                egui::ComboBox::from_id_salt("us_unit").selected_text(if us { label } else { "Decimal Inches" }).show_ui(ui, |ui| {
                    for n in DisplayUnits::US {
                        if ui.selectable_label(label == n, n).clicked() {
                            set(app, Some(n), (None, None, None));
                        }
                    }
                });
                // The fraction list is live for the fractional units.
                ui.add_enabled_ui(us && units.display.fractional(), |ui| {
                    egui::ComboBox::from_id_salt("us_fraction").selected_text(format!("1/{}", units.fraction)).width(60.0).show_ui(ui, |ui| {
                        for f in DisplayUnits::FRACTIONS {
                            if ui.selectable_label(units.fraction == f, format!("1/{f}")).clicked() {
                                let _ = app.dispatch(with(|c| if let Command::SetUnits { fraction, .. } = c { *fraction = Some(f) }));
                            }
                        }
                    });
                });
            });
            ui.add_enabled_ui(us && units.display.feet_and_inches(), |ui| {
                ui.horizontal(|ui| {
                    ui.label("Default Units:");
                    for (name, feet) in [("Feet", true), ("Inches", false)] {
                        if ui.radio(units.default_feet == feet, name).clicked() {
                            let _ = app.dispatch(with(|c| if let Command::SetUnits { default_feet, .. } = c { *default_feet = Some(feet) }));
                        }
                    }
                });
            });
            let (name, value, unit) = match &units.display {
                DisplayUnits::Custom { name, value, unit } => (name.clone(), *value, *unit),
                _ => ("FL".to_owned(), 1.0, SystemUnit::Inches),
            };
            ui.horizontal(|ui| {
                let custom = matches!(units.display, DisplayUnits::Custom { .. });
                if ui.radio(custom, "Custom").clicked() && !custom {
                    set(app, Some("Custom"), (Some(name.clone()), Some(value), Some(unit.label())));
                }
                ui.add_enabled_ui(custom, |ui| {
                    let mut n = name.clone();
                    if ui.add(egui::TextEdit::singleline(&mut n).desired_width(40.0)).lost_focus() && n != name {
                        set(app, None, (Some(n), None, None));
                    }
                    ui.label("=");
                    let mut v = value;
                    if ui.add(crate::spinner::spin(ui, &mut v, 0.000001..=1e9).speed(speed)).changed() {
                        set(app, None, (None, Some(v), None));
                    }
                    egui::ComboBox::from_id_salt("custom_unit").selected_text(unit.label()).show_ui(ui, |ui| {
                        for u in SystemUnit::ALL {
                            if ui.selectable_label(unit == u, u.label()).clicked() {
                                set(app, None, (None, None, Some(u.label())));
                            }
                        }
                    });
                });
            });
            if ui.radio(units.display == DisplayUnits::Generic, "Generic Units").clicked() {
                set(app, Some("Generic"), (None, None, None));
            }
            ui.label(format!("Example: 100 units = {}", app.state.format_units(100.0)));
        });
        if ui.button("OK").clicked() {
            let _ = app.dispatch(Command::CloseDialog);
        }
    });
    system_unit_setup(ctx, app);
}

/// System Unit Setup: 1 Unit = N system units, Respect System Units in
/// Files, and the accuracy of a float coordinate at a distance from the origin.
fn system_unit_setup(ctx: &egui::Context, app: &mut LoomApp) {
    let id = egui::Id::new(SYSTEM_ID);
    let mut open: bool = ctx.data(|d| d.get_temp(id)).unwrap_or(false);
    if !open {
        return;
    }
    let units = app.state.units.clone();
    let speed = app.spinner_speed(0.1);
    let origin_id = egui::Id::new("system_unit_origin");
    let mut origin: f32 = ctx.data(|d| d.get_temp(origin_id)).unwrap_or(16_777_215.0);
    let mut ok = false;
    egui::Window::new("System Unit Setup").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("1 Unit =");
            let mut scale = units.system_scale;
            if ui.add(crate::spinner::spin(ui, &mut scale, 0.000001..=1e9).speed(speed)).changed() {
                let _ = app.dispatch(with(|c| if let Command::SetUnits { system_scale, .. } = c { *system_scale = Some(scale) }));
            }
            egui::ComboBox::from_id_salt("system_unit").selected_text(units.system_unit.label()).show_ui(ui, |ui| {
                for u in SystemUnit::ALL {
                    if ui.selectable_label(units.system_unit == u, u.label()).clicked() {
                        let _ = app.dispatch(with(|c| if let Command::SetUnits { system_unit, .. } = c { *system_unit = Some(u.label().to_owned()) }));
                    }
                }
            });
        });
        let mut respect = app.state.default_units.respect_system_units;
        if ui.checkbox(&mut respect, "Respect System Units in Files").on_hover_text("Opening a file of other system units asks to rescale or adopt").changed() {
            let _ = app.dispatch(with(|c| if let Command::SetUnits { respect_system_units, .. } = c { *respect_system_units = Some(respect) }));
        }
        ui.separator();
        ui.label(egui::RichText::new("Origin").strong());
        ui.add(egui::Slider::new(&mut origin, 1.0..=1.0e9).logarithmic(true).show_value(false));
        ui.horizontal(|ui| {
            ui.label("Distance from origin:");
            ui.add(crate::spinner::spin(ui, &mut origin, 1.0..=1.0e9).speed(10.0).fixed_decimals(1));
        });
        // A coordinate at that distance moves in steps of one float ulp.
        ui.label(format!("Resulting Accuracy: {}", app.state.format_units(accuracy(origin))));
        ui.label(egui::RichText::new("Change the system unit only before creating or importing geometry.").small().weak());
        if ui.button("OK").clicked() {
            ok = true;
        }
    });
    ctx.data_mut(|d| {
        d.insert_temp(id, open && !ok);
        d.insert_temp(origin_id, origin);
    });
}

/// The smallest step of a single-precision coordinate at `distance` from the origin.
pub fn accuracy(distance: f32) -> f32 {
    let d = distance.abs().max(f32::MIN_POSITIVE);
    f32::from_bits(d.to_bits() + 1) - d
}

/// Units Mismatch: Rescale the file's objects to the system unit scale, or Adopt the file's unit scale.
pub fn units_mismatch(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("units_mismatch_rescale");
    let mut rescale: bool = ctx.data(|d| d.get_temp(id)).unwrap_or(true);
    let file = app.state.units_mismatch.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
    let ours = &app.state.default_units;
    let ours = format!("1 Unit = {} {}", ours.system_scale, ours.system_unit.label());
    let mut reply = None;
    egui::Window::new("Units Mismatch").open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.label(format!("The file's system unit scale differs from the system unit scale ({ours}):"));
        ui.label(egui::RichText::new(file).small().weak());
        ui.radio_value(&mut rescale, true, "Rescale the File Objects to the System Unit Scale");
        ui.radio_value(&mut rescale, false, "Adopt the File's Unit Scale");
        if ui.button("OK").clicked() {
            reply = Some(rescale);
        }
    });
    ctx.data_mut(|d| d.insert_temp(id, rescale));
    if let Some(r) = reply {
        let _ = app.dispatch(Command::UnitsMismatchReply { rescale: r });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn accuracy_grows_with_the_distance_from_the_origin() {
        assert!(super::accuracy(1.0) < 1e-6);
        assert_eq!(super::accuracy(16_777_216.0), 2.0);
    }
}
