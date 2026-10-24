//! Standard light rollouts: General Parameters, Intensity/Color/Attenuation,
//! Spotlight / Directional Parameters, Advanced Effects, Shadow Parameters,
//! Shadow Map Params and the renderer's own light properties; Skylight
//! Parameters for a skylight. Controls the active renderer (Bevy) ignores
//! are shown disabled with a note, so what is editable is what renders.

use super::create::spin;
use super::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;
use loom_scene::light::{LightObject, LightType, DECAY_TYPES, SHADOW_TYPES};
use loom_scene::NodeId;

const UNSUPPORTED: &str = "Not used by the Bevy renderer (kept in the file)";

fn set(app: &mut LoomApp, id: NodeId, name: &str, value: f64) {
    let _ = app.dispatch(Command::SetParameter { id: id.0, name: name.into(), value });
}

/// A checkbox bound to a Bool parameter.
fn check(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, name: &str, label: &str, on: bool) {
    ui.add_enabled_ui(loom_bevy::lights::supports(name), |ui| {
        let mut v = on;
        let r = ui.checkbox(&mut v, label);
        let r = if loom_bevy::lights::supports(name) { r } else { r.on_disabled_hover_text(UNSUPPORTED) };
        if r.changed() {
            set(app, id, name, if v { 1.0 } else { 0.0 });
        }
    });
}

/// A labelled spinner bound to a Float parameter (`length` uses scene units).
fn number(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, name: &str, label: &str, value: f32, length: bool) {
    let supported = loom_bevy::lights::supports(name);
    ui.add_enabled_ui(supported, |ui| {
        ui.horizontal(|ui| {
            ui.label(label);
            let mut v = value;
            let r = if length {
                ui.add(app.length_drag(&mut v, app.spinner_speed(0.5)))
            } else {
                ui.add(egui::DragValue::new(&mut v).speed(app.spinner_speed(0.05)).fixed_decimals(app.state.prefs.spinner_precision.max(1)))
            };
            let r = if supported { r } else { r.on_disabled_hover_text(UNSUPPORTED) };
            spin(app, id, name, &r, v as f64, value as f64);
        });
    });
}

fn int(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, name: &str, label: &str, value: u32) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut v = value;
        let r = ui.add(egui::DragValue::new(&mut v).speed(app.spinner_speed(1.0)));
        spin(app, id, name, &r, v as f64, value as f64);
    });
}

/// A colour swatch over three 0..1 channel parameters ("<prefix> R/G/B"), one undo step.
fn swatch(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, prefix: &str, color: glam::Vec3) {
    let supported = loom_bevy::lights::supports(prefix);
    ui.add_enabled_ui(supported, |ui| {
        let mut rgb = [0, 1, 2].map(|i| (color[i].clamp(0.0, 1.0) * 255.0).round() as u8);
        let r = ui.color_edit_button_srgb(&mut rgb);
        let r = if supported { r } else { r.on_disabled_hover_text(UNSUPPORTED) };
        if r.changed() {
            app.state.undo.begin(prefix);
            for (c, v) in ["R", "G", "B"].iter().zip(rgb) {
                set(app, id, &format!("{prefix} {c}"), v as f64 / 255.0);
            }
            app.state.undo.end();
        }
    });
}

fn combo(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, name: &str, choices: &[&str], current: u32) {
    egui::ComboBox::from_id_salt((name, id.0)).selected_text(choices.get(current as usize).copied().unwrap_or("")).show_ui(ui, |ui| {
        for (i, c) in choices.iter().enumerate() {
            if ui.selectable_label(current == i as u32, *c).clicked() && current != i as u32 {
                set(app, id, name, i as f64);
            }
        }
    });
}

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId) {
    let Some(l) = app.state.scene.node(id).and_then(|n| n.object.light()).cloned() else { return };
    if l.light_type == LightType::Skylight {
        skylight(ui, app, id, &l);
        return;
    }
    general(ui, app, id, &l);
    intensity(ui, app, id, &l);
    if l.light_type.aims() {
        let title = if l.light_type == LightType::Spot { "Spotlight Parameters" } else { "Directional Parameters" };
        rollout(ui, title, true, |ui| cone(ui, app, id, &l));
    }
    rollout(ui, "Advanced Effects", false, |ui| {
        ui.label("Affect Surfaces:");
        number(ui, app, id, "Contrast", "Contrast:", l.contrast.get(), false);
        number(ui, app, id, "Soften Diffuse Edge", "Soften Diff. Edge:", l.soften_diffuse.get(), false);
        check(ui, app, id, "Affect Diffuse", "Diffuse", l.affect_diffuse.get());
        check(ui, app, id, "Affect Specular", "Specular", l.affect_specular.get());
        check(ui, app, id, "Ambient Only", "Ambient Only", l.ambient_only.get());
        ui.separator();
        ui.label("Projector Map:");
        let supported = loom_bevy::lights::supports("Projector");
        ui.add_enabled_ui(supported, |ui| {
            ui.horizontal(|ui| {
                let mut path = l.projector_map.clone().unwrap_or_default();
                let r = ui.add(egui::TextEdit::singleline(&mut path).hint_text("None").desired_width(150.0));
                let r = if supported { r } else { r.on_disabled_hover_text(UNSUPPORTED) };
                if r.lost_focus() && Some(path.clone()) != l.projector_map && !(path.is_empty() && l.projector_map.is_none()) {
                    let _ = app.dispatch(Command::SetLightProjectorMap { id: id.0, path: (!path.is_empty()).then_some(path) });
                }
                if ui.button("None").clicked() && l.projector_map.is_some() {
                    let _ = app.dispatch(Command::SetLightProjectorMap { id: id.0, path: None });
                }
            });
        });
    });
    rollout(ui, "Shadow Parameters", false, |ui| {
        ui.label("Object Shadows:");
        ui.horizontal(|ui| {
            ui.label("Color:");
            swatch(ui, app, id, "Shadow Color", l.shadow_color.get());
        });
        number(ui, app, id, "Shadow Density", "Dens.:", l.shadow_density.get(), false);
    });
    if l.shadow_type.get() == 0 {
        rollout(ui, "Shadow Map Params", false, |ui| {
            number(ui, app, id, "Map Bias", "Bias:", l.map_bias.get(), false);
            int(ui, app, id, "Map Size", "Size:", l.map_size.get());
            number(ui, app, id, "Sample Range", "Sample Range:", l.sample_range.get(), false);
        });
    } else {
        rollout(ui, "Ray Traced Shadow Params", false, |ui| {
            ui.label("The renderer draws ray-traced shadows with shadow maps.");
            number(ui, app, id, "Map Bias", "Ray Bias:", l.map_bias.get(), false);
        });
    }
    rollout(ui, "Bevy Light", false, |ui| {
        number(ui, app, id, "Source Radius", "Source Radius:", l.source_radius.get(), true);
        number(ui, app, id, "Shadow Normal Bias", "Normal Bias:", l.shadow_normal_bias.get(), false);
    });
}

fn general(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, l: &LightObject) {
    let has_target = l.target.is_some_and(|t| app.state.scene.node(t).is_some());
    let distance = loom_scene::light::target_distance(&app.state.scene, id).unwrap_or(l.target_distance.get());
    rollout(ui, "General Parameters", true, |ui| {
        ui.label("Light Type");
        ui.horizontal(|ui| {
            check(ui, app, id, "On", "On", l.on.get());
            egui::ComboBox::from_id_salt(("light_type", id.0)).selected_text(l.light_type.label()).show_ui(ui, |ui| {
                for t in LightType::LIST {
                    if ui.selectable_label(l.light_type == t, t.label()).clicked() && l.light_type != t {
                        let arg = loom_commands::light::LightTypeArg::of(t).expect("listed types");
                        let _ = app.dispatch(Command::LightType { id: id.0, light_type: arg, targeted: has_target });
                    }
                }
            });
        });
        ui.add_enabled_ui(l.light_type.aims(), |ui| {
            ui.horizontal(|ui| {
                let mut t = has_target;
                if ui.checkbox(&mut t, "Targeted").changed() {
                    let arg = loom_commands::light::LightTypeArg::of(l.light_type).expect("aiming types");
                    let _ = app.dispatch(Command::LightType { id: id.0, light_type: arg, targeted: t });
                }
                if has_target {
                    let (factor, suffix) = app.unit_display();
                    ui.label(format!("{:.*}{}", app.state.prefs.spinner_precision, distance as f64 * factor, suffix));
                } else {
                    let mut v = distance;
                    let r = ui.add(app.length_drag(&mut v, app.spinner_speed(0.5)));
                    spin(app, id, "Target Distance", &r, v as f64, distance as f64);
                }
            });
        });
        ui.separator();
        ui.label("Shadows");
        ui.horizontal(|ui| {
            check(ui, app, id, "Shadows On", "On", l.shadows_on.get());
            combo(ui, app, id, "Shadow Type", &SHADOW_TYPES, l.shadow_type.get());
        });
        if ui.button("Exclude...").clicked() {
            app.state.dialog = Some(loom_commands::Dialog::LightExclude { light: id.0 });
        }
    });
}

fn intensity(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, l: &LightObject) {
    rollout(ui, "Intensity/Color/Attenuation", true, |ui| {
        ui.horizontal(|ui| {
            number(ui, app, id, "Multiplier", "Multiplier:", l.multiplier.get(), false);
            swatch(ui, app, id, "Color", l.color.get());
        });
        ui.separator();
        ui.label("Decay");
        ui.horizontal(|ui| {
            ui.label("Type:");
            combo(ui, app, id, "Decay Type", &DECAY_TYPES, l.decay.get());
        });
        number(ui, app, id, "Decay Start", "Start:", l.decay_start.get(), true);
        for (title, prefix, (use_, show, start, end)) in [
            ("Near Attenuation", "Near", (l.near_use.get(), l.near_show.get(), l.near_start.get(), l.near_end.get())),
            ("Far Attenuation", "Far", (l.far_use.get(), l.far_show.get(), l.far_start.get(), l.far_end.get())),
        ] {
            ui.separator();
            ui.label(title);
            ui.horizontal(|ui| {
                check(ui, app, id, &format!("{prefix} Use"), "Use", use_);
                check(ui, app, id, &format!("{prefix} Show"), "Show", show);
            });
            number(ui, app, id, &format!("{prefix} Start"), "Start:", start, true);
            number(ui, app, id, &format!("{prefix} End"), "End:", end, true);
        }
    });
}

fn cone(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, l: &LightObject) {
    ui.label("Light Cone");
    ui.horizontal(|ui| {
        check(ui, app, id, "Show Cone", "Show Cone", l.show_cone.get());
        check(ui, app, id, "Overshoot", "Overshoot", l.overshoot.get());
    });
    let spot = l.light_type == LightType::Spot;
    number(ui, app, id, "Hotspot", "Hotspot/Beam:", l.hotspot.get(), !spot);
    number(ui, app, id, "Falloff", "Falloff/Field:", l.falloff.get(), !spot);
    let supported = loom_bevy::lights::supports("Rectangle");
    ui.add_enabled_ui(supported, |ui| {
        ui.horizontal(|ui| {
            let rect = l.rectangle.get();
            for (label, v) in [("Circle", false), ("Rectangle", true)] {
                let r = ui.radio(rect == v, label);
                let r = if supported { r } else { r.on_disabled_hover_text(UNSUPPORTED) };
                if r.clicked() && rect != v {
                    set(app, id, "Rectangle", if v { 1.0 } else { 0.0 });
                }
            }
        });
    });
    number(ui, app, id, "Aspect", "Aspect:", l.aspect.get(), false);
}

fn skylight(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, l: &LightObject) {
    rollout(ui, "Skylight Parameters", true, |ui| {
        check(ui, app, id, "On", "On", l.on.get());
        number(ui, app, id, "Multiplier", "Multiplier:", l.multiplier.get(), false);
        ui.separator();
        ui.label("Sky Color");
        let env = l.use_scene_environment.get();
        let supported = loom_bevy::lights::supports("Use Scene Environment");
        ui.add_enabled_ui(supported, |ui| {
            let r = ui.radio(env, "Use Scene Environment");
            let r = if supported { r } else { r.on_disabled_hover_text(UNSUPPORTED) };
            if r.clicked() && !env {
                set(app, id, "Use Scene Environment", 1.0);
            }
        });
        ui.horizontal(|ui| {
            if ui.radio(!env, "Sky Color").clicked() && env {
                set(app, id, "Use Scene Environment", 0.0);
            }
            swatch(ui, app, id, "Color", l.color.get());
        });
        ui.separator();
        ui.label("Render");
        check(ui, app, id, "Cast Shadows", "Cast Shadows", l.shadows_on.get());
        ui.add_enabled_ui(loom_bevy::lights::supports("Rays per Sample"), |ui| {
            int(ui, app, id, "Rays per Sample", "Rays per Sample:", l.rays_per_sample.get());
        });
        number(ui, app, id, "Ray Bias", "Ray Bias:", l.ray_bias.get(), false);
    });
}
