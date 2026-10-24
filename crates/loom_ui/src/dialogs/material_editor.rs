//! Material Editor: 24 sample slots with sphere previews, the active slot's
//! Standard material (Shader and Blinn Basic Parameters, Maps) and the
//! Get / Assign / Reset buttons. Every change is a command.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, Dialog};
use loom_scene::material::{StandardMaterial, MAP_CHANNELS, SLOT_COUNT};

/// Preview size in pixels (square).
const PREVIEW: usize = 56;

/// A sphere lit by a key light from the upper left, Blinn-shaded with the
/// material's colours, over a checker so opacity shows.
pub fn preview_pixels(m: &StandardMaterial) -> Vec<egui::Color32> {
    let d = m.diffuse.get();
    let a = m.ambient.get();
    let s = m.specular.get() * (m.specular_level.get() / 100.0);
    let exponent = 2.0 * 2f32.powf(m.glossiness.get() / 10.0);
    let opacity = (m.opacity.get() / 100.0).clamp(0.0, 1.0);
    let self_illum = (m.self_illum.get() / 100.0).clamp(0.0, 1.0);
    let light = glam::Vec3::new(-0.5, 0.6, 0.65).normalize();
    let view = glam::Vec3::Z;
    let half = (light + view).normalize();
    let mut px = Vec::with_capacity(PREVIEW * PREVIEW);
    for y in 0..PREVIEW {
        for x in 0..PREVIEW {
            let u = (x as f32 + 0.5) / PREVIEW as f32 * 2.0 - 1.0;
            let v = 1.0 - (y as f32 + 0.5) / PREVIEW as f32 * 2.0;
            let checker = if ((x / 8) + (y / 8)) % 2 == 0 { 0.25 } else { 0.45 };
            let bg = glam::Vec3::splat(checker);
            let r2 = u * u + v * v;
            let c = if r2 <= 0.92 * 0.92 {
                let n = glam::Vec3::new(u, v, (0.92 * 0.92 - r2).max(0.0).sqrt()).normalize();
                let diff = n.dot(light).max(0.0);
                let spec = n.dot(half).max(0.0).powf(exponent);
                let lit = a * 0.25 + d * diff;
                let shaded = lit * (1.0 - self_illum) + d * self_illum + s * spec;
                bg * (1.0 - opacity) + shaded * opacity
            } else {
                bg
            };
            let to8 = |f: f32| (f.clamp(0.0, 1.0) * 255.0) as u8;
            px.push(egui::Color32::from_rgb(to8(c.x), to8(c.y), to8(c.z)));
        }
    }
    px
}

fn preview_texture(ctx: &egui::Context, slot: usize, m: &StandardMaterial) -> egui::TextureHandle {
    // One texture per slot, rebuilt only when the material changes.
    let key = serde_json::to_string(m).unwrap_or_default();
    let id = egui::Id::new(("material_preview", slot));
    if let Some((k, tex)) = ctx.data(|d| d.get_temp::<(String, egui::TextureHandle)>(id)) {
        if k == key {
            return tex;
        }
    }
    let image = egui::ColorImage { size: [PREVIEW, PREVIEW], pixels: preview_pixels(m) };
    let tex = ctx.load_texture(format!("material_slot_{slot}"), image, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, (key, tex.clone())));
    tex
}

fn rgb_of(c: glam::Vec3) -> [u8; 3] {
    [(c.x * 255.0).round() as u8, (c.y * 255.0).round() as u8, (c.z * 255.0).round() as u8]
}

/// A spinner whose drag is one undo step.
fn spinner(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, value: f32, range: std::ops::RangeInclusive<f32>, decimals: usize) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut v = value;
        let r = ui.add(crate::spinner::spin(ui, &mut v, range).fixed_decimals(decimals).speed(0.5));
        if r.drag_started() {
            app.state.undo.begin(label);
        }
        if r.changed() {
            let _ = app.dispatch(Command::SetMaterialParameter { slot: None, name: label.into(), value: v as f64 });
        }
        if r.drag_stopped() {
            app.state.undo.end();
        }
    });
}

fn check(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, name: &str, on: bool) {
    let mut v = on;
    if ui.checkbox(&mut v, label).changed() {
        let _ = app.dispatch(Command::SetMaterialParameter { slot: None, name: name.into(), value: if v { 1.0 } else { 0.0 } });
    }
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    egui::Window::new("Material Editor").open(open).resizable(false).default_width(380.0).show(ctx, |ui| {
        let active = app.state.scene.material_editor.active;
        // Sample slots, 6 × 4.
        egui::Grid::new("material_slots").spacing([4.0, 4.0]).show(ui, |ui| {
            for slot in 0..SLOT_COUNT {
                let m = app.state.scene.slot_material(slot);
                let tex = preview_texture(ctx, slot, &m);
                let size = egui::vec2(PREVIEW as f32, PREVIEW as f32);
                let r = ui.add(egui::ImageButton::new(egui::load::SizedTexture::new(tex.id(), size)).selected(slot == active)).on_hover_text(&m.name);
                if r.clicked() {
                    let _ = app.dispatch(Command::MaterialEditorSlot { slot: slot as u32 });
                }
                if slot % 6 == 5 {
                    ui.end_row();
                }
            }
        });
        ui.separator();
        let m = app.state.scene.slot_material(active);
        ui.horizontal(|ui| {
            if ui.button("Get Material").on_hover_text("Pick Material From Object: the selected object's material into this slot").clicked() {
                let _ = app.dispatch(Command::GetMaterialFromObject { node: None });
            }
            if ui.add_enabled(!app.state.selection.is_empty(), egui::Button::new("Assign to Selection")).clicked() {
                let _ = app.dispatch(Command::AssignMaterialToSelection);
            }
            if ui.button("Reset").on_hover_text("Reset Map/Mtl to Default Settings").clicked() {
                let _ = app.dispatch(Command::ResetMaterial { slot: None });
            }
            let shown = m.show_map_in_viewport;
            if ui.selectable_label(shown, "Show Map in Viewport").clicked() {
                let _ = app.dispatch(Command::SetMaterialParameter { slot: None, name: "Show Map in Viewport".into(), value: if shown { 0.0 } else { 1.0 } });
            }
        });
        ui.horizontal(|ui| {
            let name_id = egui::Id::new(("material_name", active));
            let mut name: String = ui.ctx().data(|d| d.get_temp(name_id)).unwrap_or_else(|| m.name.clone());
            let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(200.0));
            if r.lost_focus() && name != m.name && !name.trim().is_empty() {
                let _ = app.dispatch(Command::RenameMaterial { slot: None, name: name.clone() });
            }
            if r.has_focus() {
                ui.ctx().data_mut(|d| d.insert_temp(name_id, name));
            } else {
                ui.ctx().data_mut(|d| d.remove::<String>(name_id));
            }
            ui.label("Standard");
        });
        crate::command_panel::rollout(ui, "Shader Basic Parameters", true, |ui| {
            ui.horizontal(|ui| {
                ui.label("Blinn");
                check(ui, app, "Wire", "Wire", m.wire);
                check(ui, app, "2-Sided", "2-Sided", m.two_sided);
                check(ui, app, "Faceted", "Faceted", m.faceted);
            });
        });
        crate::command_panel::rollout(ui, "Blinn Basic Parameters", true, |ui| {
            egui::Grid::new("blinn_colours").num_columns(2).show(ui, |ui| {
                for (label, c) in [("Ambient", m.ambient.get()), ("Diffuse", m.diffuse.get()), ("Specular", m.specular.get())] {
                    ui.label(format!("{label}:"));
                    let mut rgb = rgb_of(c);
                    if ui.color_edit_button_srgb(&mut rgb).changed() {
                        let _ = app.dispatch(Command::SetMaterialColor { slot: None, which: label.into(), rgb });
                    }
                    ui.end_row();
                }
            });
            check(ui, app, "Lock Ambient and Diffuse", "Ambient/Diffuse Lock", m.ambient_locked);
            spinner(ui, app, "Self-Illumination", m.self_illum.get(), 0.0..=100.0, 0);
            spinner(ui, app, "Opacity", m.opacity.get(), 0.0..=100.0, 0);
            ui.label(egui::RichText::new("Specular Highlights").strong());
            spinner(ui, app, "Specular Level", m.specular_level.get(), 0.0..=999.0, 0);
            spinner(ui, app, "Glossiness", m.glossiness.get(), 0.0..=100.0, 0);
            spinner(ui, app, "Soften", m.soften.get(), 0.0..=1.0, 2);
        });
        crate::command_panel::rollout(ui, "Maps", true, |ui| {
            egui::Grid::new("material_maps").num_columns(3).show(ui, |ui| {
                for (i, (channel, _)) in MAP_CHANNELS.iter().enumerate() {
                    ui.label(*channel);
                    let map = m.map(channel).cloned();
                    let mut amount = map.as_ref().map(|b| b.amount).unwrap_or(0.0);
                    let range = if *channel == "Bump" { -999.0..=999.0 } else { 0.0..=100.0 };
                    if ui.add_enabled(map.is_some(), crate::spinner::spin(ui, &mut amount, range).speed(0.5)).changed() {
                        let _ = app.dispatch(Command::SetMaterialMap { slot: None, map: (*channel).into(), path: map.as_ref().map(|b| b.path.clone()), amount: Some(amount) });
                    }
                    ui.horizontal(|ui| {
                        let label = map.as_ref().and_then(|b| std::path::Path::new(&b.path).file_name().map(|f| f.to_string_lossy().into_owned())).unwrap_or_else(|| "None".into());
                        if ui.button(label).on_hover_text("Choose a bitmap").clicked() {
                            app.state.dialog = Some(Dialog::MaterialMap { channel: i as u8 });
                        }
                        if map.is_some() && ui.small_button("✕").on_hover_text("Clear the map").clicked() {
                            let _ = app.dispatch(Command::SetMaterialMap { slot: None, map: (*channel).into(), path: None, amount: None });
                        }
                    });
                    ui.end_row();
                }
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_shows_the_diffuse_colour_and_the_highlight() {
        let mut m = StandardMaterial::slot_default(0);
        m.diffuse.set(glam::Vec3::new(1.0, 0.0, 0.0));
        m.ambient.set(glam::Vec3::new(1.0, 0.0, 0.0));
        let dull = preview_pixels(&m);
        let mid = dull[PREVIEW * (PREVIEW / 2) + PREVIEW / 2];
        assert!(mid.r() > 100 && mid.g() < 40, "the sphere is red: {mid:?}");
        m.specular_level.set(100.0);
        m.glossiness.set(30.0);
        let shiny = preview_pixels(&m);
        let brightest = |p: &[egui::Color32]| p.iter().map(|c| c.g() as u32).max().unwrap();
        assert!(brightest(&shiny) > brightest(&dull) + 40, "a highlight appears");
    }
}
