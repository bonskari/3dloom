//! Units Setup dialog: System Unit Setup and Display Unit Scale.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, DisplayUnits, SystemUnit};

fn set(app: &mut LoomApp, display: Option<&str>, custom: (Option<String>, Option<f32>, Option<&str>)) {
    let _ = app.dispatch(Command::SetUnits {
        system_unit: None,
        system_scale: None,
        display: display.map(str::to_owned),
        custom_name: custom.0,
        custom_value: custom.1,
        custom_unit: custom.2.map(str::to_owned),
    });
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let units = app.state.units.clone();
    let speed = app.spinner_speed(0.1);
    egui::Window::new("Units Setup").open(open).resizable(false).show(ctx, |ui| {
        ui.group(|ui| {
            ui.label(egui::RichText::new("System Unit Setup").strong());
            ui.horizontal(|ui| {
                ui.label("1 Unit =");
                let mut scale = units.system_scale;
                if ui.add(crate::spinner::spin(ui, &mut scale, 0.000001..=1e9).speed(speed)).changed() {
                    let _ = app.dispatch(Command::SetUnits { system_unit: None, system_scale: Some(scale), display: None, custom_name: None, custom_value: None, custom_unit: None });
                }
                egui::ComboBox::from_id_salt("system_unit").selected_text(units.system_unit.label()).show_ui(ui, |ui| {
                    for u in SystemUnit::ALL {
                        if ui.selectable_label(units.system_unit == u, u.label()).clicked() {
                            let _ = app.dispatch(Command::SetUnits { system_unit: Some(u.label().to_owned()), system_scale: None, display: None, custom_name: None, custom_value: None, custom_unit: None });
                        }
                    }
                });
            });
            ui.label(egui::RichText::new("Change the system unit only before creating or importing geometry.").small().weak());
        });
        ui.group(|ui| {
            ui.label(egui::RichText::new("Display Unit Scale").strong());
            let label = units.display.label();
            let metric = matches!(units.display, DisplayUnits::Millimeters | DisplayUnits::Centimeters | DisplayUnits::Meters | DisplayUnits::Kilometers);
            let us = matches!(units.display, DisplayUnits::DecimalInches | DisplayUnits::DecimalFeet | DisplayUnits::FeetDecimalInches);
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
                    for n in ["Decimal Inches", "Decimal Feet", "Feet w/ Decimal Inches"] {
                        if ui.selectable_label(label == n, n).clicked() {
                            set(app, Some(n), (None, None, None));
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
        ui.group(|ui| {
            ui.label(egui::RichText::new("Lighting Units").strong());
            ui.add_enabled(false, egui::RadioButton::new(true, "International")).on_disabled_hover_text("Lights are out of scope");
        });
        if ui.button("OK").clicked() {
            let _ = app.dispatch(Command::CloseDialog);
        }
    });
}
