//! The rollouts of the 2D maps Swirl, Tiles and Gradient Ramp, the 3D maps,
//! the compositor and colour modifier maps and Falloff, the 3D Coordinates
//! rollout and the Output rollout. Every control dispatches SetMapParameter /
//! SetMapColor by its label.

use super::{color_row, map_button, map_check, map_spinner, rgb_of};
use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;
use loom_scene::map_params::{Coords3, BLEND_MODES, CHANNEL_OPS, CHANNEL_SOURCES, FALLOFF_DIRECTIONS, FALLOFF_TYPES, INTERPOLATIONS, RAMP_TYPES, RGB_ALPHA_FROM, SUB_CHANNELS, TILE_PRESETS};
use loom_scene::material::{Map, MapType};

fn set(app: &mut LoomApp, name: &str, value: f64) {
    let _ = app.dispatch(Command::SetMapParameter { name: name.into(), value });
}

/// A drop-down list of `options` setting `name` to the picked index.
fn combo(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, name: &str, current: u8, options: &[&str]) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut pick = None;
        egui::ComboBox::from_id_salt(("map_combo", name)).selected_text(options.get(current as usize).copied().unwrap_or("")).show_ui(ui, |ui| {
            for (i, o) in options.iter().enumerate() {
                if ui.selectable_label(i == current as usize, *o).clicked() {
                    pick = Some(i);
                }
            }
        });
        if let Some(i) = pick {
            set(app, name, i as f64);
        }
    });
}

/// Radio buttons of `options` setting `name` to the picked index.
fn radios(ui: &mut egui::Ui, app: &mut LoomApp, name: &str, current: u8, options: &[&str]) {
    ui.horizontal_wrapped(|ui| {
        for (i, o) in options.iter().enumerate() {
            if ui.radio(current as usize == i, *o).clicked() {
                set(app, name, i as f64);
            }
        }
    });
}

fn swap(ui: &mut egui::Ui, app: &mut LoomApp, label: &str) {
    if ui.button(label).clicked() {
        let _ = app.dispatch(Command::SwapMapColors);
    }
}

/// Colour component rows `range` in a four-column grid.
fn colors(ui: &mut egui::Ui, app: &mut LoomApp, map: &Map, id: &str, range: std::ops::Range<usize>) {
    egui::Grid::new(id).num_columns(4).show(ui, |ui| {
        for i in range {
            color_row(ui, app, map, i);
        }
    });
}

/// A map button with its check box for component `i` (no swatch).
fn map_row(ui: &mut egui::Ui, app: &mut LoomApp, map: &Map, i: usize, label: &str) {
    let slot = map.slot_names()[i].clone();
    ui.horizontal(|ui| {
        ui.label(label);
        map_button(ui, app, &slot, map.sub_map(i), false);
        let mut on = map.sub_map(i).is_some_and(|s| s.enabled);
        if ui.add_enabled(map.sub_map(i).is_some(), egui::Checkbox::new(&mut on, "")).changed() {
            set(app, &format!("{slot} Map On"), if on { 1.0 } else { 0.0 });
        }
    });
}

fn seed(ui: &mut egui::Ui, app: &mut LoomApp, map: &Map) {
    map_spinner(ui, app, "Random Seed:", "Random Seed", map.extra.seed as f32, 0.0..=65535.0, 0);
}

/// Coordinates rollout of a 3D map.
pub(super) fn coords3(ui: &mut egui::Ui, app: &mut LoomApp, map: &Map) {
    crate::command_panel::rollout(ui, "Coordinates", true, |ui| {
        let c = &map.coords3;
        combo(ui, app, "Source:", "Source", c.source, &Coords3::SOURCES);
        ui.add_enabled_ui(c.source == 2, |ui| map_spinner(ui, app, "Map Channel:", "Map Channel", map.coords.map_channel as f32, 1.0..=99.0, 0));
        egui::Grid::new("map_coords3").num_columns(4).show(ui, |ui| {
            for h in ["", "Offset", "Tiling", "Angle"] {
                ui.label(h);
            }
            ui.end_row();
            for (i, axis) in ["X", "Y", "Z"].iter().enumerate() {
                ui.label(*axis);
                map_spinner(ui, app, "", &format!("Offset {axis}"), c.offset[i], -100_000.0..=100_000.0, 3);
                map_spinner(ui, app, "", &format!("Tiling {axis}"), c.tiling[i], -1000.0..=1000.0, 3);
                map_spinner(ui, app, "", &format!("Angle {axis}"), c.angle[i], -360.0..=360.0, 1);
                ui.end_row();
            }
        });
        ui.horizontal(|ui| {
            map_spinner(ui, app, "Blur:", "Blur", map.coords.blur, 0.01..=100.0, 2);
            map_spinner(ui, app, "Blur Offset:", "Blur Offset", c.blur_offset, 0.0..=1.0, 3);
        });
    });
}

/// Output rollout.
pub(super) fn output(ui: &mut egui::Ui, app: &mut LoomApp, map: &Map) {
    crate::command_panel::rollout(ui, "Output", map.kind == MapType::Output, |ui| {
        let o = &map.output;
        egui::Grid::new("map_output").num_columns(2).show(ui, |ui| {
            map_check(ui, app, "Invert", "Invert", o.invert);
            map_spinner(ui, app, "Output Amount:", "Output Amount", o.amount, 0.0..=100.0, 2);
            ui.end_row();
            map_check(ui, app, "Clamp", "Clamp", o.clamp);
            map_spinner(ui, app, "RGB Offset:", "RGB Offset", o.rgb_offset, -100.0..=100.0, 2);
            ui.end_row();
            map_check(ui, app, "Alpha from RGB Intensity", "Alpha from RGB Intensity", o.alpha_from_rgb);
            map_spinner(ui, app, "RGB Level:", "RGB Level", o.rgb_level, 0.0..=100.0, 2);
            ui.end_row();
            ui.label("");
            map_spinner(ui, app, "Bump Amount:", "Bump Amount", o.bump_amount, -1000.0..=1000.0, 2);
            ui.end_row();
        });
    });
}

/// The Mix map's Mixing Curve group.
pub(super) fn mixing_curve(ui: &mut egui::Ui, app: &mut LoomApp, map: &Map) {
    let e = &map.extra;
    ui.label(egui::RichText::new("Mixing curve").strong());
    map_check(ui, app, "Use Curve", "Use Curve", e.use_curve);
    ui.add_enabled_ui(e.use_curve, |ui| {
        ui.label("Transition zone:");
        ui.horizontal(|ui| {
            map_spinner(ui, app, "Upper:", "Upper", e.upper, 0.0..=1.0, 2);
            map_spinner(ui, app, "Lower:", "Lower", e.lower, 0.0..=1.0, 2);
        });
    });
}

/// The parameter rollouts of the maps `material_editor::map_rollouts` does not draw itself.
pub(super) fn parameters(ui: &mut egui::Ui, app: &mut LoomApp, map: &Map) {
    let e = &map.extra;
    let p = &map.params;
    let title = format!("{} Parameters", map.kind.label());
    match map.kind {
        MapType::Swirl => crate::command_panel::rollout(ui, "Swirl Parameters", true, |ui| {
            ui.label(egui::RichText::new("Swirl Color Setup").strong());
            colors(ui, app, map, "swirl_colors", 0..2);
            swap(ui, app, "Swap");
            map_spinner(ui, app, "Color Contrast:", "Color Contrast", e.color_contrast, 0.0..=4.0, 2);
            map_spinner(ui, app, "Swirl Intensity:", "Swirl Intensity", e.swirl_intensity, -10.0..=10.0, 2);
            map_spinner(ui, app, "Swirl Amount:", "Swirl Amount", e.swirl_amount, 0.0..=3.0, 2);
            ui.label(egui::RichText::new("Swirl Appearance").strong());
            map_spinner(ui, app, "Twist:", "Twist", e.twist, -20.0..=20.0, 2);
            map_spinner(ui, app, "Constant Detail:", "Constant Detail", e.constant_detail, 0.0..=10.0, 0);
            ui.label(egui::RichText::new("Swirl Location").strong());
            map_spinner(ui, app, "Center Position X:", "Center Position X", e.center[0], -100.0..=100.0, 3);
            map_spinner(ui, app, "Center Position Y:", "Center Position Y", e.center[1], -100.0..=100.0, 3);
            map_check(ui, app, "Lock", "Lock", e.center_lock);
            ui.label(egui::RichText::new("Configuration").strong());
            seed(ui, app, map);
        }),
        MapType::Tiles => {
            crate::command_panel::rollout(ui, "Standard Controls", true, |ui| {
                ui.label(egui::RichText::new("Pattern Setup").strong());
                combo(ui, app, "Preset Type:", "Preset Type", e.preset, &TILE_PRESETS);
            });
            crate::command_panel::rollout(ui, "Advanced Controls", true, |ui| {
                ui.label(egui::RichText::new("Tiles Setup").strong());
                colors(ui, app, map, "tiles_tile", 0..1);
                map_spinner(ui, app, "Horiz. Count:", "Horiz. Count", e.horiz_count, 0.0..=100.0, 1);
                map_spinner(ui, app, "Vert. Count:", "Vert. Count", e.vert_count, 0.0..=100.0, 1);
                map_spinner(ui, app, "Color Variance:", "Color Variance", e.color_variance, 0.0..=100.0, 2);
                map_spinner(ui, app, "Fade Variance:", "Fade Variance", e.fade_variance, 0.0..=1.0, 2);
                ui.label(egui::RichText::new("Grout Setup").strong());
                colors(ui, app, map, "tiles_grout", 1..2);
                map_spinner(ui, app, "Horizontal Gap:", "Horizontal Gap", e.gap[0], 0.0..=100.0, 2);
                map_spinner(ui, app, "Vertical Gap:", "Vertical Gap", e.gap[1], 0.0..=100.0, 2);
                map_check(ui, app, "Lock Gaps", "Gap Lock", e.gap_lock);
                map_spinner(ui, app, "% Holes:", "% Holes", e.holes, 0.0..=100.0, 0);
                map_spinner(ui, app, "Rough:", "Rough", e.rough, 0.0..=200.0, 2);
                ui.label(egui::RichText::new("Miscellaneous").strong());
                seed(ui, app, map);
                swap(ui, app, "Swap Texture Entries");
                let custom = e.preset == 0;
                ui.label(egui::RichText::new("Stacking Layout").strong());
                ui.add_enabled_ui(custom, |ui| {
                    map_spinner(ui, app, "Line Shift:", "Line Shift", e.line_shift, 0.0..=100.0, 2);
                    map_spinner(ui, app, "Random Shift:", "Random Shift", e.random_shift, 0.0..=100.0, 2);
                });
                ui.label(egui::RichText::new("Row and Column Editing").strong());
                ui.add_enabled_ui(custom, |ui| {
                    map_check(ui, app, "Row Modify", "Row Modify", e.row_modify);
                    ui.horizontal(|ui| {
                        map_spinner(ui, app, "Per Row:", "Per Row", e.per_row as f32, 0.0..=50.0, 0);
                        map_spinner(ui, app, "Change:", "Row Change", e.row_change, 0.0..=5.0, 2);
                    });
                    map_check(ui, app, "Column Modify", "Column Modify", e.column_modify);
                    ui.horizontal(|ui| {
                        map_spinner(ui, app, "Per Column:", "Per Column", e.per_column as f32, 0.0..=50.0, 0);
                        map_spinner(ui, app, "Change:", "Column Change", e.column_change, 0.0..=5.0, 2);
                    });
                });
            });
        }
        MapType::GradientRamp => crate::command_panel::rollout(ui, "Gradient Ramp Parameters", true, |ui| gradient_ramp(ui, app, map)),
        MapType::Cellular => crate::command_panel::rollout(ui, "Cellular Parameters", true, |ui| {
            ui.label(egui::RichText::new("Cell Color").strong());
            colors(ui, app, map, "cell_color", 0..1);
            map_spinner(ui, app, "Variation:", "Variation", e.variation, 0.0..=100.0, 1);
            ui.label(egui::RichText::new("Division Colors").strong());
            colors(ui, app, map, "cell_division", 1..3);
            ui.label(egui::RichText::new("Cell Characteristics").strong());
            radios(ui, app, "Chips", e.chips as u8, &["Circular", "Chips"]);
            map_spinner(ui, app, "Size:", "Size", p.size, 0.001..=1_000_000.0, 2);
            map_spinner(ui, app, "Spread:", "Spread", e.spread, 0.001..=100.0, 2);
            map_spinner(ui, app, "Bump Smoothing:", "Bump Smoothing", e.bump_smoothing, 0.0..=10.0, 2);
            map_check(ui, app, "Fractal", "Fractal", e.fractal);
            ui.add_enabled_ui(e.fractal, |ui| {
                map_spinner(ui, app, "Iterations:", "Iterations", e.iterations, 1.0..=10.0, 1);
                map_check(ui, app, "Adaptive", "Adaptive", e.adaptive);
                map_spinner(ui, app, "Roughness:", "Roughness", e.roughness, 0.0..=1.0, 2);
            });
            ui.label(egui::RichText::new("Thresholds").strong());
            ui.horizontal(|ui| {
                map_spinner(ui, app, "Low:", "Low", p.low, 0.0..=1.0, 2);
                map_spinner(ui, app, "Mid:", "Mid", e.mid, 0.0..=1.0, 2);
                map_spinner(ui, app, "High:", "High", p.high, 0.0..=1.0, 2);
            });
        }),
        MapType::Dent => two_color(ui, app, map, &title, |ui, app| {
            map_spinner(ui, app, "Size:", "Size", p.size, 0.001..=1_000_000.0, 1);
            map_spinner(ui, app, "Strength:", "Strength", e.strength, 0.0..=1000.0, 1);
            map_spinner(ui, app, "Iterations:", "Iterations", e.iterations, 1.0..=10.0, 0);
        }),
        MapType::Marble => two_color(ui, app, map, &title, |ui, app| {
            map_spinner(ui, app, "Size:", "Size", p.size, 0.001..=1_000_000.0, 1);
            map_spinner(ui, app, "Vein Width:", "Vein Width", e.vein_width, 0.0..=100.0, 3);
        }),
        MapType::PerlinMarble => crate::command_panel::rollout(ui, "Perlin Marble Parameters", true, |ui| {
            map_spinner(ui, app, "Size:", "Size", p.size, 0.001..=1_000_000.0, 1);
            map_spinner(ui, app, "Levels:", "Levels", p.levels, 1.0..=10.0, 1);
            for (i, sat) in [("Color 1 Saturation", e.saturation[0]), ("Color 2 Saturation", e.saturation[1])].into_iter().enumerate() {
                colors(ui, app, map, if i == 0 { "perlin_c1" } else { "perlin_c2" }, i..i + 1);
                map_spinner(ui, app, "Saturation:", sat.0, sat.1, 1.0..=100.0, 0);
            }
            swap(ui, app, "Swap");
        }),
        MapType::Planet => crate::command_panel::rollout(ui, "Planet Parameters", true, |ui| {
            ui.label(egui::RichText::new("Continent Characteristics").strong());
            map_spinner(ui, app, "Continent Size:", "Continent Size", e.continent_size, 0.001..=1_000_000.0, 1);
            map_spinner(ui, app, "Island Factor:", "Island Factor", e.island_factor, 0.0..=100.0, 2);
            map_spinner(ui, app, "Ocean %:", "Ocean %", e.ocean, 0.0..=100.0, 1);
            seed(ui, app, map);
            map_check(ui, app, "Blend Water/Land", "Blend Water/Land", e.blend_water_land);
            for (label, range) in [("Water Colors", 0..3), ("Land Colors", 3..8)] {
                ui.label(egui::RichText::new(label).strong());
                ui.horizontal(|ui| {
                    for i in range {
                        let which = map.slot_names()[i].clone();
                        let mut rgb = rgb_of(map.color(i));
                        if ui.color_edit_button_srgb(&mut rgb).on_hover_text(&which).changed() {
                            let _ = app.dispatch(Command::SetMapColor { which, rgb });
                        }
                    }
                });
            }
        }),
        MapType::Smoke => two_color(ui, app, map, &title, |ui, app| {
            map_spinner(ui, app, "Size:", "Size", p.size, 0.001..=1_000_000.0, 1);
            map_spinner(ui, app, "Phase:", "Phase", p.phase, -1000.0..=1000.0, 2);
            map_spinner(ui, app, "# Iterations:", "Iterations", e.iterations, 1.0..=20.0, 0);
            map_spinner(ui, app, "Exponent:", "Exponent", e.exponent, 0.0..=100.0, 2);
        }),
        MapType::Speckle => two_color(ui, app, map, &title, |ui, app| {
            map_spinner(ui, app, "Size:", "Size", p.size, 0.001..=1_000_000.0, 3);
        }),
        MapType::Splat => two_color(ui, app, map, &title, |ui, app| {
            map_spinner(ui, app, "Size:", "Size", p.size, 0.001..=1_000_000.0, 1);
            map_spinner(ui, app, "# Iterations:", "Iterations", e.iterations, 1.0..=20.0, 0);
            map_spinner(ui, app, "Threshold:", "Threshold", e.threshold, 0.0..=1.0, 2);
        }),
        MapType::Stucco => two_color(ui, app, map, &title, |ui, app| {
            map_spinner(ui, app, "Size:", "Size", p.size, 0.001..=1_000_000.0, 1);
            map_spinner(ui, app, "Thickness:", "Thickness", e.thickness, 0.0..=1.0, 2);
            map_spinner(ui, app, "Threshold:", "Threshold", e.threshold, 0.0..=1.0, 2);
        }),
        MapType::Waves => two_color(ui, app, map, &title, |ui, app| {
            map_spinner(ui, app, "Num Wave Sets:", "Num Wave Sets", e.wave_sets as f32, 1.0..=50.0, 0);
            map_spinner(ui, app, "Wave Radius:", "Wave Radius", e.wave_radius, 0.0..=1_000_000.0, 1);
            map_spinner(ui, app, "Wave Len Max:", "Wave Len Max", e.wave_len_max, 0.0..=1_000_000.0, 1);
            map_spinner(ui, app, "Wave Len Min:", "Wave Len Min", e.wave_len_min, 0.0..=1_000_000.0, 1);
            map_spinner(ui, app, "Amplitude:", "Amplitude", e.amplitude, 0.0..=10_000.0, 2);
            map_spinner(ui, app, "Phase:", "Phase", p.phase, -1000.0..=1000.0, 2);
            ui.horizontal(|ui| {
                ui.label("Distribution:");
                radios(ui, app, "Distribution 2D", e.distribution_2d as u8, &["3D", "2D"]);
            });
            seed(ui, app, map);
        }),
        MapType::Wood => two_color(ui, app, map, &title, |ui, app| {
            map_spinner(ui, app, "Grain Thickness:", "Grain Thickness", e.grain_thickness, 0.0..=1_000_000.0, 2);
            map_spinner(ui, app, "Radial Noise:", "Radial Noise", e.radial_noise, 0.0..=100.0, 2);
            map_spinner(ui, app, "Axial Noise:", "Axial Noise", e.axial_noise, 0.0..=100.0, 2);
        }),
        MapType::Composite => composite(ui, app, map),
        MapType::Mask => crate::command_panel::rollout(ui, "Mask Parameters", true, |ui| {
            map_row(ui, app, map, 0, "Map:");
            map_row(ui, app, map, 1, "Mask:");
            map_check(ui, app, "Invert Mask", "Invert Mask", e.invert_mask);
        }),
        MapType::RgbMultiply => crate::command_panel::rollout(ui, "RGB Multiply Parameters", true, |ui| {
            colors(ui, app, map, "rgb_multiply", 0..2);
            ui.label(egui::RichText::new("Alpha From").strong());
            radios(ui, app, "Alpha From", e.alpha_from, &RGB_ALPHA_FROM);
        }),
        MapType::RgbTint => crate::command_panel::rollout(ui, "RGB Tint Parameters", true, |ui| {
            ui.horizontal(|ui| {
                for (i, which) in ["R", "G", "B"].into_iter().enumerate() {
                    ui.label(which);
                    let mut rgb = rgb_of(map.color(i));
                    if ui.color_edit_button_srgb(&mut rgb).changed() {
                        let _ = app.dispatch(Command::SetMapColor { which: which.into(), rgb });
                    }
                }
            });
            map_row(ui, app, map, 3, "Map:");
        }),
        MapType::Output => crate::command_panel::rollout(ui, "Output Parameters", true, |ui| map_row(ui, app, map, 0, "Map:")),
        MapType::ColorCorrection => {
            crate::command_panel::rollout(ui, "Basic Parameters", true, |ui| colors(ui, app, map, "cc_basic", 0..1));
            crate::command_panel::rollout(ui, "Channels", true, |ui| {
                radios(ui, app, "Channels", e.channels, &CHANNEL_OPS);
                ui.add_enabled_ui(e.channels == 3, |ui| {
                    for (i, ch) in ["Red", "Green", "Blue", "Alpha"].into_iter().enumerate() {
                        combo(ui, app, &format!("{ch}:"), ch, e.custom_channels[i], &CHANNEL_SOURCES);
                    }
                });
            });
            crate::command_panel::rollout(ui, "Color", true, |ui| {
                map_spinner(ui, app, "Hue Shift:", "Hue Shift", e.hue_shift, -180.0..=180.0, 1);
                map_spinner(ui, app, "Saturation:", "Saturation", e.cc_saturation, -100.0..=100.0, 1);
                ui.horizontal(|ui| {
                    ui.label("Hue Tint:");
                    let mut rgb = rgb_of(map.color(1));
                    if ui.color_edit_button_srgb(&mut rgb).changed() {
                        let _ = app.dispatch(Command::SetMapColor { which: "Hue Tint".into(), rgb });
                    }
                });
                map_spinner(ui, app, "Strength:", "Hue Tint Strength", e.tint_strength, 0.0..=100.0, 1);
            });
            crate::command_panel::rollout(ui, "Lightness", true, |ui| {
                map_spinner(ui, app, "Brightness:", "Brightness", e.brightness, -100.0..=100.0, 1);
                map_spinner(ui, app, "Contrast:", "Contrast", e.contrast, -100.0..=100.0, 1);
            });
        }
        MapType::VertexColor => crate::command_panel::rollout(ui, "Vertex Color Parameters", true, |ui| {
            map_spinner(ui, app, "Map Channel:", "Map Channel", map.coords.map_channel as f32, 0.0..=99.0, 0);
            combo(ui, app, "Sub Channel:", "Sub Channel", e.sub_channel, &SUB_CHANNELS);
        }),
        MapType::Falloff => falloff(ui, app, map),
        MapType::Raytrace => crate::command_panel::rollout(ui, "Raytracer Parameters", true, |ui| {
            combo(ui, app, "Trace Mode:", "Trace Mode", e.trace_mode, &["Auto Detect", "Reflection", "Refraction"]);
        }),
        MapType::ReflectRefract => crate::command_panel::rollout(ui, "Reflect/Refract Parameters", true, |ui| {
            map_spinner(ui, app, "Size:", "Env Size", e.env_size, 1.0..=4096.0, 0);
            map_spinner(ui, app, "Blur:", "Env Blur", e.env_blur, 0.0..=100.0, 2);
        }),
        MapType::FlatMirror => crate::command_panel::rollout(ui, "Flat Mirror Parameters", true, |ui| {
            map_spinner(ui, app, "Blur:", "Env Blur", e.env_blur, 0.0..=100.0, 2);
            map_spinner(ui, app, "Distortion Amount:", "Distortion Amount", e.distortion, 0.0..=100.0, 2);
        }),
        MapType::ThinWall => crate::command_panel::rollout(ui, "Thin Wall Refraction Parameters", true, |ui| {
            map_spinner(ui, app, "Thickness Offset:", "Thickness Offset", e.thickness_offset, 0.0..=10.0, 2);
        }),
        _ => {}
    }
}

/// A 3D map's rollout: its spinners, Swap and Color #1 / #2.
fn two_color(ui: &mut egui::Ui, app: &mut LoomApp, map: &Map, title: &str, spinners: impl FnOnce(&mut egui::Ui, &mut LoomApp)) {
    crate::command_panel::rollout(ui, title, true, |ui| {
        spinners(ui, app);
        swap(ui, app, "Swap");
        colors(ui, app, map, "two_colors", 0..2);
    });
}

fn gradient_ramp(ui: &mut egui::Ui, app: &mut LoomApp, map: &Map) {
    let e = &map.extra;
    let p = &map.params;
    // The gradient bar: a click on it adds a flag there.
    let (rect, r) = ui.allocate_exact_size(egui::vec2(220.0, 22.0), egui::Sense::click());
    let n = 64;
    for k in 0..n {
        let t = (k as f32 + 0.5) / n as f32;
        let c = map.ramp_color_at(t * 100.0);
        let x0 = rect.left() + rect.width() * k as f32 / n as f32;
        let x1 = rect.left() + rect.width() * (k + 1) as f32 / n as f32;
        let [r8, g8, b8] = rgb_of(c);
        ui.painter().rect_filled(egui::Rect::from_min_max(egui::pos2(x0, rect.top()), egui::pos2(x1, rect.bottom())), 0.0, egui::Color32::from_rgb(r8, g8, b8));
    }
    for f in &map.flags {
        let x = rect.left() + rect.width() * f.position / 100.0;
        ui.painter().line_segment([egui::pos2(x, rect.bottom() - 6.0), egui::pos2(x, rect.bottom())], egui::Stroke::new(2.0_f32, egui::Color32::WHITE));
    }
    let r = r.on_hover_text("Click to add a flag");
    if r.clicked() {
        if let Some(pos) = r.interact_pointer_pos() {
            let t = ((pos.x - rect.left()) / rect.width() * 100.0).clamp(0.0, 100.0);
            set(app, "Add Flag", t as f64);
        }
    }
    egui::Grid::new("ramp_flags").num_columns(6).show(ui, |ui| {
        for (i, f) in map.flags.iter().enumerate() {
            color_row_inline(ui, app, map, 1 + i);
            let name = format!("Flag #{} Position", i + 1);
            ui.add_enabled_ui(i >= 2, |ui| map_spinner(ui, app, "", &name, f.position, 0.0..=100.0, 1));
            if ui.add_enabled(i >= 2, egui::Button::new("Delete")).on_hover_text(format!("Delete Flag #{}", i + 1)).clicked() {
                set(app, "Delete Flag", (i + 1) as f64);
            }
            ui.end_row();
        }
    });
    combo(ui, app, "Gradient Type:", "Gradient Type", e.ramp_type, &RAMP_TYPES);
    combo(ui, app, "Interpolation:", "Interpolation", e.interpolation, &INTERPOLATIONS);
    ui.add_enabled_ui(e.ramp_type == 5, |ui| map_row(ui, app, map, 0, "Source Map:"));
    ui.label(egui::RichText::new("Noise").strong());
    map_spinner(ui, app, "Amount:", "Noise Amount", e.noise_amount, 0.0..=1.0, 2);
    ui.horizontal(|ui| {
        for (i, label) in ["Regular", "Fractal", "Turbulence"].iter().enumerate() {
            if ui.radio(p.noise_type == i as u32, *label).clicked() {
                set(app, "Noise Type", i as f64);
            }
        }
    });
    ui.horizontal(|ui| {
        map_spinner(ui, app, "Size:", "Size", p.size, 0.001..=1_000_000.0, 2);
        map_spinner(ui, app, "Phase:", "Phase", p.phase, -1000.0..=1000.0, 2);
    });
    ui.add_enabled_ui(p.noise_type != 0, |ui| map_spinner(ui, app, "Levels:", "Levels", p.levels, 1.0..=10.0, 1));
    ui.label(egui::RichText::new("Noise Threshold").strong());
    ui.horizontal(|ui| {
        map_spinner(ui, app, "Low:", "Low", p.low, 0.0..=1.0, 2);
        map_spinner(ui, app, "High:", "High", p.high, 0.0..=1.0, 2);
        map_spinner(ui, app, "Smooth:", "Smooth", e.smooth, 0.0..=1.0, 2);
    });
}

/// A colour row's four cells without ending the grid row.
fn color_row_inline(ui: &mut egui::Ui, app: &mut LoomApp, map: &Map, i: usize) {
    let slot = map.slot_names()[i].clone();
    ui.label(&slot);
    let mut rgb = rgb_of(map.color(i));
    if ui.color_edit_button_srgb(&mut rgb).changed() {
        let _ = app.dispatch(Command::SetMapColor { which: slot.clone(), rgb });
    }
    map_button(ui, app, &slot, map.sub_map(i), false);
    let mut on = map.sub_map(i).is_some_and(|s| s.enabled);
    if ui.add_enabled(map.sub_map(i).is_some(), egui::Checkbox::new(&mut on, "")).changed() {
        set(app, &format!("{slot} Map On"), if on { 1.0 } else { 0.0 });
    }
}

fn composite(ui: &mut egui::Ui, app: &mut LoomApp, map: &Map) {
    crate::command_panel::rollout(ui, "Composite Layers", true, |ui| {
        ui.horizontal(|ui| {
            ui.label(format!("Total Layers: {}", map.layers.len()));
            if ui.button("Add a New Layer").clicked() {
                set(app, "Add Layer", 1.0);
            }
        });
    });
    // The top layer first, as the reference stacks them.
    for (i, layer) in map.layers.iter().enumerate().rev() {
        let n = i + 1;
        crate::command_panel::rollout(ui, &format!("Layer {n}"), true, |ui| {
            ui.horizontal(|ui| {
                let mut shown = layer.visible;
                if ui.checkbox(&mut shown, "Visible").on_hover_text("Hide this layer").changed() {
                    set(app, &format!("Layer {n} Visible"), if shown { 1.0 } else { 0.0 });
                }
                if ui.add_enabled(map.layers.len() > 1, egui::Button::new("Delete")).on_hover_text("Delete this layer").clicked() {
                    set(app, "Delete Layer", n as f64);
                }
                if ui.button("Duplicate").on_hover_text("Duplicate this layer").clicked() {
                    set(app, "Duplicate Layer", n as f64);
                }
            });
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label("Map:");
                    map_button(ui, app, &format!("Layer {n}"), map.sub_map(2 * i), false);
                    if ui.button("Color Correct").on_hover_text("Color Correct This Texture").clicked() {
                        set(app, &format!("Layer {n} Color Correct"), 1.0);
                    }
                });
                ui.vertical(|ui| {
                    ui.label("Mask:");
                    map_button(ui, app, &format!("Mask {n}"), map.sub_map(2 * i + 1), false);
                    let mut shown = layer.mask_visible;
                    if ui.checkbox(&mut shown, "Mask Visible").on_hover_text("Hide the mask of this layer").changed() {
                        set(app, &format!("Mask {n} Visible"), if shown { 1.0 } else { 0.0 });
                    }
                });
            });
            map_spinner(ui, app, "Opacity:", &format!("Layer {n} Opacity"), layer.opacity, 0.0..=100.0, 1);
            combo(ui, app, "Blend Mode:", &format!("Layer {n} Blend Mode"), layer.blend, &BLEND_MODES);
        });
    }
}

fn falloff(ui: &mut egui::Ui, app: &mut LoomApp, map: &Map) {
    let e = &map.extra;
    crate::command_panel::rollout(ui, "Falloff Parameters", true, |ui| {
        let names = ["Towards : Away", "Front : Side", "Front : Side", "Shadow : Light", "Near : Far"];
        ui.label(egui::RichText::new(names[e.falloff_type.min(4) as usize]).strong());
        egui::Grid::new("falloff_rows").num_columns(5).show(ui, |ui| {
            for (i, (name, amount)) in [("Front Amount", e.front_amount), ("Side Amount", e.side_amount)].into_iter().enumerate() {
                color_row_inline(ui, app, map, i);
                map_spinner(ui, app, "", name, amount, 0.0..=100.0, 1);
                ui.end_row();
            }
        });
        swap(ui, app, "Swap");
        combo(ui, app, "Falloff Type:", "Falloff Type", e.falloff_type, &FALLOFF_TYPES);
        combo(ui, app, "Falloff Direction:", "Falloff Direction", e.falloff_direction, &FALLOFF_DIRECTIONS);
        ui.label(egui::RichText::new("Mode Specific Parameters").strong());
        ui.add_enabled_ui(e.falloff_type == 2, |ui| {
            map_check(ui, app, "Override Material IOR", "Override Material IOR", e.override_ior);
            ui.add_enabled_ui(e.override_ior, |ui| map_spinner(ui, app, "Index of Refraction:", "Index of Refraction", e.ior, 1.0..=50.0, 2));
        });
        ui.add_enabled_ui(e.falloff_type == 4, |ui| {
            map_spinner(ui, app, "Near Distance:", "Near Distance", e.near_distance, 0.0..=1_000_000.0, 1);
            map_spinner(ui, app, "Far Distance:", "Far Distance", e.far_distance, 0.0..=1_000_000.0, 1);
            map_check(ui, app, "Extrapolate", "Extrapolate", e.extrapolate);
        });
    });
}
