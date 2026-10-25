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
                ui.add(app.length_drag(ui, &mut v, app.spinner_speed(0.5)))
            } else {
                ui.add(crate::spinner::spin(ui, &mut v, ..).speed(app.spinner_speed(0.05)).fixed_decimals(app.state.prefs.spinner_precision.max(1)))
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
        let r = ui.add(crate::spinner::spin(ui, &mut v, ..).speed(app.spinner_speed(1.0)));
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
    // A long choice (a lamp name) is cut to the room left on the row: a
    // wider box would widen the command panel and shift every viewport.
    let room = (ui.available_width() - ui.spacing().icon_width - ui.spacing().button_padding.x * 2.0 - ui.spacing().item_spacing.x).max(40.0);
    let text = choices.get(current as usize).copied().unwrap_or("");
    let font = egui::TextStyle::Button.resolve(ui.style());
    let width = ui.fonts(|f| f.layout_no_wrap(text.to_owned(), font, egui::Color32::WHITE).size().x);
    let mut cb = egui::ComboBox::from_id_salt((name, id.0)).selected_text(text);
    if width > room {
        cb = cb.width(room).truncate();
    }
    cb.show_ui(ui, |ui| {
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
    if l.photometric.is_some() {
        photometric(ui, app, id, &l);
        return;
    }
    if l.sun.is_some() {
        sunlight(ui, app, id, &l);
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
                // A target light's spinner moves its target along the beam.
                let mut v = distance;
                let r = ui.add(app.length_drag(ui, &mut v, app.spinner_speed(0.5)));
                spin(app, id, "Target Distance", &r, v as f64, distance as f64);
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

/// Photometric light rollouts: Templates, General Parameters (with Light
/// Distribution), Intensity/Color/Attenuation, Distribution (Spotlight),
/// Shape/Area Shadows and Shadow Map Params.
fn photometric(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, l: &LightObject) {
    use loom_scene::light::{DISTRIBUTIONS, DIST_SPOT, INTENSITY_UNITS, LAMP_NAMES, LIGHT_SHAPES, TEMPLATES};
    let Some(ph) = l.photometric.clone() else { return };
    rollout(ui, "Templates", true, |ui| {
        egui::ComboBox::from_id_salt(("photometric_template", id.0)).selected_text("(Select a template)").show_ui(ui, |ui| {
            for (i, t) in TEMPLATES.iter().enumerate() {
                if ui.selectable_label(false, t.0).clicked() {
                    let _ = app.dispatch(Command::PhotometricTemplate { id: id.0, index: i as u32 });
                }
            }
        });
    });
    let has_target = l.target.is_some_and(|t| app.state.scene.node(t).is_some());
    let distance = loom_scene::light::target_distance(&app.state.scene, id).unwrap_or(l.target_distance.get());
    rollout(ui, "General Parameters", true, |ui| {
        ui.label("Light Properties:");
        ui.horizontal(|ui| {
            check(ui, app, id, "On", "On", l.on.get());
            let mut t = has_target;
            if ui.checkbox(&mut t, "Targeted").changed() {
                let arg = loom_commands::light::LightTypeArg::of(l.light_type).expect("photometric lights are omni or spot");
                let _ = app.dispatch(Command::LightType { id: id.0, light_type: arg, targeted: t });
            }
        });
        ui.horizontal(|ui| {
            ui.label("Targ. Dist:");
            let mut v = distance;
            let r = ui.add(app.length_drag(ui, &mut v, app.spinner_speed(0.5)));
            spin(app, id, "Target Distance", &r, v as f64, distance as f64);
        });
        ui.separator();
        ui.label("Shadows:");
        ui.horizontal(|ui| {
            check(ui, app, id, "Shadows On", "On", l.shadows_on.get());
            combo(ui, app, id, "Shadow Type", &SHADOW_TYPES, l.shadow_type.get());
        });
        if ui.button("Exclude...").clicked() {
            app.state.dialog = Some(loom_commands::Dialog::LightExclude { light: id.0 });
        }
        ui.separator();
        ui.label("Light Distribution (Type):");
        combo(ui, app, id, "Distribution", &DISTRIBUTIONS, ph.distribution.get());
        if ph.distribution.get() == loom_scene::light::DIST_WEB {
            ui.label(egui::RichText::new("Drawn as Uniform Spherical (the renderers read no IES files).").small());
        }
    });
    rollout(ui, "Intensity/Color/Attenuation", true, |ui| {
        ui.label("Color:");
        let kelvin = ph.use_kelvin.get();
        ui.horizontal(|ui| {
            if ui.radio(!kelvin, "").clicked() && kelvin {
                set(app, id, "Use Kelvin", 0.0);
            }
            combo(ui, app, id, "Lamp", &LAMP_NAMES, ph.lamp.get());
        });
        ui.horizontal(|ui| {
            if ui.radio(kelvin, "Kelvin:").clicked() && !kelvin {
                set(app, id, "Use Kelvin", 1.0);
            }
            let k = ph.kelvin.get();
            let mut v = k;
            let r = ui.add_enabled(kelvin, crate::spinner::spin(ui, &mut v, 1000.0..=20000.0).speed(app.spinner_speed(10.0)).fixed_decimals(1));
            spin(app, id, "Kelvin", &r, v as f64, k as f64);
            let c = loom_scene::light::kelvin_to_srgb(ph.temperature());
            let (rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 16.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 2.0, egui::Color32::from_rgb((c.x * 255.0) as u8, (c.y * 255.0) as u8, (c.z * 255.0) as u8));
        });
        ui.horizontal(|ui| {
            ui.label("Filter Color:");
            swatch(ui, app, id, "Filter Color", ph.filter_color.get());
        });
        ui.separator();
        ui.label("Intensity:");
        let unit = ph.intensity_unit.get();
        ui.horizontal(|ui| {
            for (i, u) in INTENSITY_UNITS.iter().enumerate() {
                if ui.radio(unit == i as u32, *u).clicked() && unit != i as u32 {
                    set(app, id, "Intensity Unit", i as f64);
                }
            }
        });
        let value = ph.from_candela(ph.candela.get(), unit, l.falloff.get());
        ui.horizontal(|ui| {
            let mut v = value;
            let r = ui.add(crate::spinner::spin(ui, &mut v, 0.0..).speed(app.spinner_speed(1.0)).fixed_decimals(1));
            spin(app, id, "Intensity", &r, v as f64, value as f64);
            if unit == 2 {
                ui.label("at");
                let d = ph.lux_distance.get();
                let mut dv = d;
                let r = ui.add(app.length_drag(ui, &mut dv, app.spinner_speed(0.5)));
                spin(app, id, "Lux Distance", &r, dv as f64, d as f64);
            }
        });
        ui.separator();
        ui.label("Dimming:");
        let resulting = ph.from_candela(ph.candela.get() * ph.dim(), unit, l.falloff.get());
        ui.label(format!("Resulting Intensity: {resulting:.1} {}", INTENSITY_UNITS[unit.min(2) as usize]));
        // The toggle and the percentage on rows of their own: side by side they widened the panel.
        check(ui, app, id, "Dimming On", "Dimming", ph.dimming_on.get());
        number(ui, app, id, "Dimming", "Percent:", ph.dimming.get(), false);
        check(ui, app, id, "Incandescent Shift", "Incandescent lamp color shift when dimming", ph.incandescent_shift.get());
        ui.separator();
        ui.label("Far Attenuation:");
        ui.horizontal(|ui| {
            check(ui, app, id, "Far Use", "Use", l.far_use.get());
            check(ui, app, id, "Far Show", "Show", l.far_show.get());
        });
        number(ui, app, id, "Far Start", "Start:", l.far_start.get(), true);
        number(ui, app, id, "Far End", "End:", l.far_end.get(), true);
    });
    if ph.distribution.get() == DIST_SPOT {
        rollout(ui, "Distribution (Spotlight)", true, |ui| {
            number(ui, app, id, "Hotspot", "Hotspot/Beam:", l.hotspot.get(), false);
            number(ui, app, id, "Falloff", "Falloff/Field:", l.falloff.get(), false);
        });
    }
    rollout(ui, "Shape/Area Shadows", false, |ui| {
        ui.label("Emit light from (Shape):");
        combo(ui, app, id, "Light Shape", &LIGHT_SHAPES, ph.shape.get());
        match ph.shape.get() {
            1 => number(ui, app, id, "Shape Length", "Length:", ph.shape_length.get(), true),
            2 => {
                number(ui, app, id, "Shape Length", "Length:", ph.shape_length.get(), true);
                number(ui, app, id, "Shape Width", "Width:", ph.shape_width.get(), true);
            }
            3..=5 => number(ui, app, id, "Shape Radius", "Radius:", ph.shape_radius.get(), true),
            _ => {}
        }
        check(ui, app, id, "Render Shape", "Light Shape Visible in Rendering", ph.render_shape.get());
    });
    rollout(ui, "Shadow Map Params", false, |ui| {
        number(ui, app, id, "Map Bias", "Bias:", l.map_bias.get(), false);
        int(ui, app, id, "Map Size", "Size:", l.map_size.get());
    });
}

/// A sun's Control Parameters: location (Get Location), time and date,
/// time zone and daylight saving, orbital scale and North Direction.
fn sunlight(ui: &mut egui::Ui, app: &mut LoomApp, id: NodeId, l: &LightObject) {
    let Some(s) = l.sun.clone() else { return };
    let north = app
        .state
        .scene
        .node(id)
        .and_then(|n| n.parent)
        .and_then(|p| app.state.scene.node(p))
        .map(|c| c.transform.rotation.get().z)
        .unwrap_or(0.0);
    #[derive(Default)]
    struct Edit {
        latitude: Option<f32>,
        longitude: Option<f32>,
        time_zone: Option<f32>,
        year: Option<i32>,
        month: Option<u32>,
        day: Option<u32>,
        hour: Option<u32>,
        minute: Option<u32>,
        dst: Option<bool>,
        north: Option<f32>,
        orbital_scale: Option<f32>,
    }
    let mut e = Edit::default();
    let float = |ui: &mut egui::Ui, label: &str, v: f32, range: std::ops::RangeInclusive<f32>, out: &mut Option<f32>| {
        ui.horizontal(|ui| {
            ui.label(label);
            let mut x = v;
            let r = ui.add(crate::spinner::spin(ui, &mut x, range).speed(0.05).fixed_decimals(2));
            if (r.lost_focus() || r.drag_stopped() || r.changed() && !r.dragged()) && x != v {
                *out = Some(x);
            }
        });
    };
    let whole = |ui: &mut egui::Ui, label: &str, v: u32, range: std::ops::RangeInclusive<u32>, out: &mut Option<u32>| {
        ui.label(label);
        let mut x = v;
        let r = ui.add(crate::spinner::spin(ui, &mut x, range).speed(0.1));
        if (r.lost_focus() || r.drag_stopped() || r.changed() && !r.dragged()) && x != v {
            *out = Some(x);
        }
    };
    rollout(ui, "Control Parameters", true, |ui| {
        let (az, alt) = s.solar_position();
        ui.label(format!("Azimuth: {az:.1}   Altitude: {alt:.1}"));
        ui.separator();
        ui.label("Time");
        ui.horizontal(|ui| {
            whole(ui, "Hours:", s.hour, 0..=23, &mut e.hour);
            whole(ui, "Mins.:", s.minute, 0..=59, &mut e.minute);
        });
        ui.horizontal(|ui| {
            whole(ui, "Month:", s.month, 1..=12, &mut e.month);
            whole(ui, "Day:", s.day, 1..=31, &mut e.day);
        });
        // Year on its own row: three spinners side by side widened the panel.
        ui.horizontal(|ui| {
            ui.label("Year:");
            let mut y = s.year;
            let r = ui.add(crate::spinner::spin(ui, &mut y, 1900..=2100).speed(0.1));
            if (r.lost_focus() || r.drag_stopped() || r.changed() && !r.dragged()) && y != s.year {
                e.year = Some(y);
            }
        });
        float(ui, "Time Zone:", s.time_zone, -12.0..=14.0, &mut e.time_zone);
        let mut dst = s.dst;
        if ui.checkbox(&mut dst, "Daylight Saving Time").changed() {
            e.dst = Some(dst);
        }
        ui.separator();
        ui.label("Location");
        ui.horizontal(|ui| {
            if ui.button("Get Location...").clicked() {
                app.state.dialog = Some(loom_commands::Dialog::GeographicLocation { sun: id.0 });
            }
            ui.label(if s.location.is_empty() { "(custom)" } else { s.location.as_str() });
        });
        float(ui, "Latitude:", s.latitude, -90.0..=90.0, &mut e.latitude);
        float(ui, "Longitude:", s.longitude, -180.0..=180.0, &mut e.longitude);
        ui.separator();
        ui.label("Site");
        float(ui, "Orbital Scale:", s.orbital_scale, 0.001..=1e6, &mut e.orbital_scale);
        float(ui, "North Direction:", north, -360.0..=360.0, &mut e.north);
    });
    if e.latitude.is_some() || e.longitude.is_some() || e.time_zone.is_some() || e.year.is_some() || e.month.is_some() || e.day.is_some() || e.hour.is_some() || e.minute.is_some() || e.dst.is_some() || e.north.is_some() || e.orbital_scale.is_some() {
        let _ = app.dispatch(Command::SunlightParameters {
            id: id.0,
            location: None,
            latitude: e.latitude,
            longitude: e.longitude,
            time_zone: e.time_zone,
            year: e.year,
            month: e.month,
            day: e.day,
            hour: e.hour,
            minute: e.minute,
            dst: e.dst,
            north: e.north,
            orbital_scale: e.orbital_scale,
        });
    }
}
