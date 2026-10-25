//! The Compact Material Editor: 24 sample slots (3 × 2, 5 × 3 or 6 × 4 shown)
//! with sphere / cylinder / cube previews, the toolbars beside and below them,
//! the name field and Type button, and the rollouts of the level being
//! edited: a Standard material (Shader / Blinn Basic Parameters, Maps), a
//! Multi/Sub-Object or Blend material, or a map (Coordinates, Bitmap /
//! Checker / Gradient / Noise / Mix parameters). Also the Material/Map
//! Browser, Replace Material / Map, Material Editor Options, Specify
//! Cropping/Placement, Copy (Instance) Map and the Material/Map Navigator.
//! Every change is a command.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{BrowserFor, Command, Dialog};
use loom_scene::material::{map_at, resolve_level, Level, Map, MapType, MaterialKind, NavStep, SampleType, StandardMaterial, Surface, Surfaces, MAP_CHANNELS, SLOT_COUNT};
use loom_scene::MaterialId;

mod map_rollouts;

/// A sample slot being dragged (onto another slot, an object or selected polygons).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotDrag(pub usize);

/// A map button being dragged onto another (its `slot_index`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MapDrag(u8);

fn slot_index_of(name: &str) -> u8 {
    loom_scene::map_params::slot_index(name)
}

// ---------------------------------------------------------------------------
// Sample slot previews
// ---------------------------------------------------------------------------

/// What a sample slot shows: a material's surfaces, or a map flat (Display Maps as 2D).
#[derive(Debug, Clone, PartialEq)]
pub enum Sample {
    Surfaces(Surfaces),
    Map(Box<Map>),
}

/// How a preview is drawn: the sample object, its rotation (Drag/Rotate),
/// the editor's lights, background and UV tiling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewSetup {
    pub kind: SampleType,
    pub yaw: f32,
    pub pitch: f32,
    pub top: glam::Vec3,
    pub back: Option<glam::Vec3>,
    pub ambient: glam::Vec3,
    pub background: Option<f32>,
    pub uv_tiling: f32,
}

impl PreviewSetup {
    pub fn from_options(o: &loom_scene::material::EditorOptions, kind: SampleType, rotation: (f32, f32)) -> Self {
        Self {
            kind,
            yaw: rotation.0,
            pitch: rotation.1,
            top: o.top_light * o.top_multiplier,
            back: o.backlight.then_some(o.back_light * o.back_multiplier),
            ambient: o.ambient,
            // Background off: a plain grey of Background Intensity; on: the checker.
            background: (!o.background).then_some(o.background_intensity),
            uv_tiling: o.uv_tiling.max(1) as f32,
        }
    }
}

/// Where a view ray through the preview's pixel (u, v in -1–1) hits the
/// sample object: (point, normal, map UV), in the object's frame before rotation.
fn hit(kind: SampleType, o: glam::Vec3, d: glam::Vec3) -> Option<(glam::Vec3, glam::Vec3, [f32; 2])> {
    use std::f32::consts::PI;
    match kind {
        SampleType::Sphere => {
            let r = 0.9;
            let b = o.dot(d);
            let c = o.length_squared() - r * r;
            let disc = b * b - c;
            if disc < 0.0 {
                return None;
            }
            let p = o + d * (-b - disc.sqrt());
            let n = p.normalize();
            Some((p, n, [0.5 + n.x.atan2(n.z) / (2.0 * PI), 0.5 + n.y.asin() / PI]))
        }
        SampleType::Cylinder => {
            let (r, h) = (0.62, 0.72);
            let mut best: Option<(f32, glam::Vec3, [f32; 2])> = None;
            // Side.
            let a = d.x * d.x + d.z * d.z;
            if a > 1e-9 {
                let b = o.x * d.x + o.z * d.z;
                let c = o.x * o.x + o.z * o.z - r * r;
                let disc = b * b - a * c;
                if disc >= 0.0 {
                    let t = (-b - disc.sqrt()) / a;
                    let p = o + d * t;
                    if t > 0.0 && p.y.abs() <= h {
                        let n = glam::Vec3::new(p.x, 0.0, p.z).normalize();
                        best = Some((t, n, [0.5 + n.x.atan2(n.z) / (2.0 * PI), (p.y + h) / (2.0 * h)]));
                    }
                }
            }
            // Caps.
            for s in [1.0f32, -1.0] {
                if d.y.abs() > 1e-9 {
                    let t = (s * h - o.y) / d.y;
                    let p = o + d * t;
                    if t > 0.0 && p.x * p.x + p.z * p.z <= r * r && best.is_none_or(|b| t < b.0) {
                        best = Some((t, glam::Vec3::Y * s, [p.x / (2.0 * r) + 0.5, p.z / (2.0 * r) + 0.5]));
                    }
                }
            }
            best.map(|(t, n, uv)| (o + d * t, n, uv))
        }
        SampleType::Cube => {
            let h = 0.58;
            let mut best: Option<(f32, glam::Vec3, [f32; 2])> = None;
            for axis in 0..3 {
                for s in [1.0f32, -1.0] {
                    let (da, oa) = (d[axis], o[axis]);
                    if da.abs() < 1e-9 {
                        continue;
                    }
                    let t = (s * h - oa) / da;
                    let p = o + d * t;
                    let (i, j) = ((axis + 1) % 3, (axis + 2) % 3);
                    if t > 0.0 && p[i].abs() <= h && p[j].abs() <= h && best.is_none_or(|b| t < b.0) {
                        let mut n = glam::Vec3::ZERO;
                        n[axis] = s;
                        best = Some((t, n, [p[i] / (2.0 * h) + 0.5, p[j] / (2.0 * h) + 0.5]));
                    }
                }
            }
            best.map(|(t, n, uv)| (o + d * t, n, uv))
        }
    }
}

/// The sample lights: (direction, colour), the top light first.
fn sample_lights(setup: &PreviewSetup) -> Vec<(glam::Vec3, glam::Vec3)> {
    let mut lights = vec![(glam::Vec3::new(-0.5, 0.6, 0.65).normalize(), setup.top)];
    if let Some(b) = setup.back {
        lights.push((glam::Vec3::new(0.7, -0.3, -0.65).normalize(), b * 0.6));
    }
    lights
}

/// An anisotropic (Ward) highlight at `n` with tangent `t` turned by
/// `orientation` degrees: Glossiness sets its size, Anisotropy 0–100 narrows it one way.
fn ward(n: glam::Vec3, t: glam::Vec3, h: glam::Vec3, glossiness: f32, anisotropy: f32, orientation: f32) -> f32 {
    let exponent = 2.0 * 2f32.powf(glossiness / 10.0);
    let a = (2.0 / (exponent + 2.0)).sqrt();
    let (ax, ay) = (a * (1.0 - 0.95 * anisotropy / 100.0).max(0.02), a);
    let t = glam::Quat::from_axis_angle(n, orientation.to_radians()) * t;
    let b = n.cross(t);
    let nh = n.dot(h).max(1e-4);
    (-((h.dot(t) / ax).powi(2) + (h.dot(b) / ay).powi(2)) / (nh * nh)).exp()
}

/// One surface shaded by its shader under the sample lights at the shaded
/// point `ctx` (view along -Z; `up` the sample object's axis, for
/// anisotropic highlights), its maps evaluated there.
fn shade(s: &Surface, ctx: &loom_scene::maps::Ctx, up: glam::Vec3, setup: &PreviewSetup, images: &loom_scene::maps::Images) -> (glam::Vec3, f32) {
    use loom_scene::material::Shader;
    let n = ctx.normal;
    let sh = &s.shading;
    let mut d = s.diffuse;
    if let Some(m) = &s.diffuse_map {
        let t = loom_scene::maps::eval_ctx(m, ctx, images);
        d = d.lerp(glam::Vec3::new(t[0], t[1], t[2]), t[3] * (m.amount / 100.0).clamp(0.0, 1.0));
    }
    if sh.shader.has_diffuse_level() {
        d *= sh.diffuse_level / 100.0;
    }
    let view = glam::Vec3::Z;
    let blinn = |g: f32| 2.0 * 2f32.powf(g / 10.0);
    let exponent = blinn(s.glossiness);
    let level = s.specular_level / 100.0;
    // The highlight colour and how much the diffuse keeps.
    let (spec_color, diffuse_k) = match sh.shader {
        Shader::Metal => (d * level, 1.0 - 0.75 * level.min(1.0)),
        Shader::Strauss => {
            let (g, m) = (s.glossiness / 100.0, sh.metalness / 100.0);
            (glam::Vec3::ONE.lerp(d, m) * (0.1 + 0.9 * g), (1.0 - m * g) * (1.0 - g * g * g))
        }
        _ => (s.specular, 1.0),
    };
    let tangent = up.cross(n).try_normalize().unwrap_or(glam::Vec3::X);
    let sigma2 = (sh.roughness / 100.0).powi(2);
    let mut lit = s.ambient * setup.ambient + d * 0.08;
    let mut spec = glam::Vec3::ZERO;
    let lights = sample_lights(setup);
    let highlight = |n: glam::Vec3, l: glam::Vec3| -> glam::Vec3 {
        let h = (l + view).normalize();
        match sh.shader {
            // Phong: the reflected light against the view, a wider highlight than Blinn's.
            Shader::Phong => spec_color * (2.0 * n.dot(l) * n - l).dot(view).max(0.0).powf(exponent),
            Shader::Anisotropic => spec_color * ward(n, tangent, h, s.glossiness, sh.anisotropy, sh.orientation),
            Shader::MultiLayer => {
                spec_color * ward(n, tangent, h, s.glossiness, sh.anisotropy, sh.orientation)
                    + sh.specular2 * (sh.level2 / 100.0) * ward(n, tangent, h, sh.glossiness2, sh.anisotropy2, sh.orientation2)
            }
            _ => spec_color * n.dot(h).max(0.0).powf(exponent),
        }
    };
    for (l, c) in lights {
        let ndl = n.dot(l);
        let diff = if sh.shader.has_roughness() && sigma2 > 0.0 {
            // Oren-Nayar: rough surfaces flatten and darken.
            let (a, b) = (1.0 - 0.5 * sigma2 / (sigma2 + 0.33), 0.45 * sigma2 / (sigma2 + 0.09));
            let (ti, tr) = (ndl.clamp(-1.0, 1.0).acos(), n.dot(view).clamp(-1.0, 1.0).acos());
            let pl = (l - n * ndl).normalize_or_zero();
            let pv = (view - n * n.dot(view)).normalize_or_zero();
            let cos_phi = pl.dot(pv).max(0.0);
            ndl.max(0.0) * (a + b * cos_phi * ti.max(tr).sin() * ti.min(tr).tan())
        } else {
            ndl.max(0.0)
        };
        lit += d * c * diff * diffuse_k;
        if sh.shader == Shader::Translucent {
            // Light from behind scatters through in the Translucent colour times the Filter colour.
            lit += sh.translucent_color * s.extended.filter_color * c * (-ndl).max(0.0);
            if sh.backside_specular && ndl < 0.0 {
                spec += highlight(-n, l) * c;
            }
        }
        if ndl > 0.0 {
            spec += highlight(n, l) * c;
        }
    }
    let c = lit * (1.0 - s.self_illum) + d * s.self_illum + s.self_illum_color + spec;
    let mut a = s.opacity;
    if let Some(m) = &s.opacity_map {
        let t = loom_scene::maps::eval_ctx(m, ctx, images);
        let lum = (t[0] + t[1] + t[2]) / 3.0;
        a += (lum - a) * (m.amount / 100.0).clamp(0.0, 1.0);
    }
    // Advanced Transparency falloff: In clears the facing middle, Out the edges.
    let e = &s.extended;
    if e.falloff_amount > 0.0 && a < 1.0 {
        let facing = n.dot(view).abs();
        let extreme = 1.0 - e.falloff_amount / 100.0;
        a = if e.falloff_out { extreme + (a - extreme) * facing } else { a + (extreme - a) * facing };
    }
    (c, a)
}

/// The sample slot image (`size` × `size`): the sample object lit by the
/// editor's top (and back) light over the background; a Multi/Sub-Object
/// material shows a patchwork of its sub-materials, a map level the map flat.
pub fn preview_pixels(sample: &Sample, size: usize, setup: &PreviewSetup, images: &loom_scene::maps::Images) -> Vec<egui::Color32> {
    let mut px = Vec::with_capacity(size * size);
    let rot = glam::Mat3::from_rotation_x(setup.pitch) * glam::Mat3::from_rotation_y(setup.yaw);
    let inv = rot.transpose();
    let to8 = |f: f32| (f.clamp(0.0, 1.0) * 255.0).round() as u8;
    for y in 0..size {
        for x in 0..size {
            let u = (x as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let v = 1.0 - (y as f32 + 0.5) / size as f32 * 2.0;
            let bg = match setup.background {
                Some(g) => glam::Vec3::splat(g),
                None => glam::Vec3::splat(if ((x * 8 / size) + (y * 8 / size)).is_multiple_of(2) { 0.25 } else { 0.45 }),
            };
            let c = match sample {
                Sample::Map(m) => {
                    let t = loom_scene::maps::eval(m, [(u * 0.5 + 0.5) * setup.uv_tiling, (v * 0.5 + 0.5) * setup.uv_tiling], images);
                    bg.lerp(glam::Vec3::new(t[0], t[1], t[2]), t[3])
                }
                Sample::Surfaces(surfaces) => {
                    // Orthographic view down -Z onto the rotated sample object.
                    let o = inv * glam::Vec3::new(u, v, 3.0);
                    let d = inv * glam::Vec3::new(0.0, 0.0, -1.0);
                    match hit(setup.kind, o, d) {
                        None => bg,
                        Some((p_local, n_local, uv)) => {
                            let n = rot * n_local;
                            let uv = [uv[0] * setup.uv_tiling, uv[1] * setup.uv_tiling];
                            // The sample object is 100 units across (`maps::UV_UNITS`), viewed down -Z.
                            let scale = loom_scene::maps::UV_UNITS / 2.0;
                            let light = sample_lights(setup).first().map_or(1.0, |(l, _)| n.dot(*l).max(0.0));
                            let ctx = loom_scene::maps::Ctx {
                                uv,
                                p_obj: p_local * scale,
                                p_world: rot * p_local * scale,
                                normal: n,
                                view: glam::Vec3::Z,
                                cam_x: glam::Vec3::X,
                                cam_y: glam::Vec3::Y,
                                obj_axes: rot,
                                dist: (3.0 - (rot * p_local).z) * scale,
                                light,
                            };
                            let s = match surfaces {
                                Surfaces::Single(s) => std::borrow::Cow::Borrowed(s.as_ref()),
                                Surfaces::PerId(list) => {
                                    // Patchwork: the sub-materials in turn over a 4 × 2 grid of the sample's UVs.
                                    let ids: Vec<&u16> = list.keys().collect();
                                    if ids.is_empty() {
                                        std::borrow::Cow::Owned(Surface::black())
                                    } else {
                                        let cell = ((uv[0].rem_euclid(1.0) * 4.0) as usize + 4 * (uv[1].rem_euclid(1.0) * 2.0) as usize) % ids.len();
                                        surfaces.for_face(*ids[cell])
                                    }
                                }
                            };
                            let (c, a) = shade(&s, &ctx, rot * glam::Vec3::Y, setup, images);
                            bg.lerp(c, a)
                        }
                    }
                }
            };
            px.push(egui::Color32::from_rgb(to8(c.x), to8(c.y), to8(c.z)));
        }
    }
    px
}

/// The images every map of `sample` reads.
fn sample_images(app: &mut LoomApp, sample: &Sample) -> loom_scene::maps::Images {
    let mut maps: Vec<Map> = Vec::new();
    let mut add = |s: &Surface| {
        maps.extend(s.diffuse_map.iter().cloned());
        maps.extend(s.opacity_map.iter().cloned());
    };
    match sample {
        Sample::Map(m) => maps.push((**m).clone()),
        Sample::Surfaces(Surfaces::Single(s)) => add(s),
        Sample::Surfaces(Surfaces::PerId(l)) => l.values().for_each(add),
    }
    let mut images = loom_scene::maps::Images::new();
    for m in maps {
        images.extend(app.map_images(&m));
    }
    images
}

/// The sample a material shows (resolved through the library when it has an id).
fn material_sample(app: &LoomApp, id: Option<MaterialId>, fallback: &StandardMaterial) -> Sample {
    let surfaces = id.and_then(|i| loom_scene::material::surfaces(&app.state.scene.materials, i)).unwrap_or_else(|| Surfaces::Single(Box::new(Surface::from_standard(fallback))));
    Sample::Surfaces(surfaces)
}

/// A preview texture, rebuilt only when what it shows changes.
fn preview_texture(ctx: &egui::Context, app: &mut LoomApp, key_id: egui::Id, sample: &Sample, size: usize, setup: &PreviewSetup) -> egui::TextureHandle {
    let key = format!("{sample:?}|{size}|{setup:?}|{}", app.state.bitmap_generation);
    if let Some((k, tex)) = ctx.data(|d| d.get_temp::<(String, egui::TextureHandle)>(key_id)) {
        if k == key {
            return tex;
        }
    }
    let images = sample_images(app, sample);
    let image = egui::ColorImage { size: [size, size], pixels: preview_pixels(sample, size, setup, &images) };
    let tex = ctx.load_texture(format!("{key_id:?}"), image, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(key_id, (key, tex.clone())));
    tex
}

fn rotation(ctx: &egui::Context, slot: usize) -> (f32, f32) {
    ctx.data(|d| d.get_temp::<(f32, f32)>(egui::Id::new(("material_slot_rotation", slot)))).unwrap_or((0.0, 0.0))
}

/// What slot `slot` shows: the active slot at a map level its map (Display
/// Maps as 2D), at a sub-material level that sub-material.
fn slot_sample(app: &LoomApp, slot: usize) -> Sample {
    let ed = &app.state.scene.material_editor;
    let m = app.state.scene.slot_material(slot);
    let top = ed.slots[slot];
    if slot == ed.active && !ed.path.is_empty() {
        if let Some(top) = top {
            let (level, _) = resolve_level(&app.state.scene.materials, top, &ed.path);
            match level {
                Level::Map { owner, maps } => {
                    if let Some(map) = app.state.scene.materials.get(&owner).and_then(|o| map_at(o, &maps)) {
                        if ed.options.maps_as_2d {
                            return Sample::Map(Box::new(map.clone()));
                        }
                        let mut s = Surface::from_standard(&StandardMaterial::new(""));
                        s.diffuse_map = Some(Map { amount: 100.0, ..map.clone() });
                        return Sample::Surfaces(Surfaces::Single(Box::new(s)));
                    }
                }
                Level::Material(id) => {
                    let sub = app.state.scene.materials.get(&id).cloned().unwrap_or(m);
                    return material_sample(app, Some(id), &sub);
                }
            }
        }
    }
    material_sample(app, top.filter(|t| app.state.scene.materials.contains_key(t)), &m)
}

// ---------------------------------------------------------------------------
// Small widgets
// ---------------------------------------------------------------------------

fn rgb_of(c: glam::Vec3) -> [u8; 3] {
    [(c.x * 255.0).round() as u8, (c.y * 255.0).round() as u8, (c.z * 255.0).round() as u8]
}

/// A spinner whose drag is one undo step; `cmd` builds the command for a value.
fn spinner_with(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, value: f32, range: std::ops::RangeInclusive<f32>, decimals: usize, cmd: impl Fn(f64) -> Command) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut v = value;
        let r = ui.add(crate::spinner::spin(ui, &mut v, range).fixed_decimals(decimals).speed(if decimals > 1 { 0.01 } else { 0.5 }));
        if r.drag_started() {
            app.state.undo.begin(label);
        }
        if r.changed() {
            let _ = app.dispatch(cmd(v as f64));
        }
        if r.drag_stopped() {
            app.state.undo.end();
        }
    });
}

fn spinner(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, value: f32, range: std::ops::RangeInclusive<f32>, decimals: usize) {
    let name = label.to_owned();
    spinner_with(ui, app, label, value, range, decimals, move |v| Command::SetMaterialParameter { slot: None, name: name.clone(), value: v });
}

fn map_spinner(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, name: &str, value: f32, range: std::ops::RangeInclusive<f32>, decimals: usize) {
    let name = name.to_owned();
    spinner_with(ui, app, label, value, range, decimals, move |v| Command::SetMapParameter { name: name.clone(), value: v });
}

fn check(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, name: &str, on: bool) {
    let mut v = on;
    if ui.checkbox(&mut v, label).changed() {
        let _ = app.dispatch(Command::SetMaterialParameter { slot: None, name: name.into(), value: if v { 1.0 } else { 0.0 } });
    }
}

fn map_check(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, name: &str, on: bool) {
    let mut v = on;
    if ui.checkbox(&mut v, label).changed() {
        let _ = app.dispatch(Command::SetMapParameter { name: name.into(), value: if v { 1.0 } else { 0.0 } });
    }
}

/// A map button for slot `slot` at the current level: with a map it goes to
/// the map's level, empty it opens the Material/Map Browser; it can be
/// dragged onto another map button (Copy (Instance) Map). `short` draws the
/// small shortcut button beside a Blinn swatch ("M" when a map is there).
fn map_button(ui: &mut egui::Ui, app: &mut LoomApp, slot: &str, map: Option<&Map>, short: bool) {
    let label = match (map, short) {
        (Some(_), true) => "M".to_owned(),
        (None, true) => " ".to_owned(),
        (Some(m), false) => m.label(),
        (None, false) => "None".to_owned(),
    };
    let tip = map.map(|m| m.label()).unwrap_or_else(|| format!("{slot}: None"));
    let button = egui::Button::new(label).sense(egui::Sense::click_and_drag());
    let button = if short { button.min_size(egui::vec2(14.0, 14.0)) } else { button.min_size(egui::vec2(120.0, 0.0)) };
    let r = ui.add(button).on_hover_text(tip);
    let index = slot_index_of(slot);
    if r.drag_started() && map.is_some() {
        egui::DragAndDrop::set_payload(ui.ctx(), MapDrag(index));
    }
    if let Some(from) = r.dnd_release_payload::<MapDrag>() {
        if from.0 != index {
            app.state.dialog = Some(Dialog::CopyMap { from: from.0, to: index });
        }
        return;
    }
    if r.clicked() {
        if map.is_some() {
            let _ = app.dispatch(Command::MaterialEditorGoToMap { map: slot.into() });
        } else {
            app.state.dialog = Some(Dialog::MaterialBrowser(BrowserFor::Map(index)));
        }
    }
}

/// The level being edited: (its material id, the material, the map steps below it).
fn current_level(app: &LoomApp) -> (Option<MaterialId>, StandardMaterial, Vec<String>) {
    let ed = &app.state.scene.material_editor;
    let lib = &app.state.scene.materials;
    match ed.slots[ed.active].filter(|t| lib.contains_key(t)) {
        None => (None, app.state.scene.slot_material(ed.active), Vec::new()),
        Some(top) => {
            let (level, _) = resolve_level(lib, top, &ed.path);
            let (id, maps) = match level {
                Level::Material(id) => (id, Vec::new()),
                Level::Map { owner, maps } => (owner, maps),
            };
            (Some(id), lib.get(&id).cloned().unwrap_or_else(|| StandardMaterial::new("")), maps)
        }
    }
}

fn material_label(app: &LoomApp, id: Option<MaterialId>) -> String {
    match id.and_then(|i| app.state.scene.materials.get(&i)) {
        Some(m) => format!("{} ({})", m.name, m.kind.label()),
        None => "None".into(),
    }
}

// ---------------------------------------------------------------------------
// The editor window
// ---------------------------------------------------------------------------

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let window = egui::Window::new("Material Editor").id(egui::Id::new("Material Editor")).open(open).resizable(true).default_pos([16.0, 40.0]).default_width(400.0).default_height(740.0).vscroll(true);
    let r = window.show(ctx, |ui| {
        menu(ui, app);
        slots(ui, app);
        ui.separator();
        toolbar(ui, app);
        let (_, m, maps) = current_level(app);
        name_row(ui, app, &m, &maps);
        if maps.is_empty() {
            match &m.kind {
                MaterialKind::Standard => standard_rollouts(ui, app, &m),
                MaterialKind::MultiSub(_) => multi_sub_rollout(ui, app, &m),
                MaterialKind::Blend(_) => blend_rollout(ui, app, &m),
                MaterialKind::Shell(_) => shell_rollout(ui, app, &m),
                MaterialKind::Raytrace(_) => raytrace_rollouts(ui, app, &m),
                MaterialKind::Architectural(_) => architectural_rollouts(ui, app, &m),
                MaterialKind::DoubleSided(_) | MaterialKind::TopBottom(_) | MaterialKind::Shellac(_) => two_material_rollout(ui, app, &m),
                MaterialKind::Composite(_) => composite_rollout(ui, app, &m),
                MaterialKind::MatteShadow(_) => matte_shadow_rollout(ui, app, &m),
                MaterialKind::InkPaint(_) => ink_paint_rollouts(ui, app, &m),
            }
        } else if let Some(map) = map_at(&m, &maps).cloned() {
            map_rollouts(ui, app, &map);
        }
    });
    // A sample slot dropped outside the editor: onto an object or its selected polygons.
    let editor = r.as_ref().map(|r| r.response.rect);
    if let Some(drag) = egui::DragAndDrop::payload::<SlotDrag>(ctx) {
        let released = ctx.input(|i| i.pointer.any_released());
        let pos = ctx.input(|i| i.pointer.interact_pos().or(i.pointer.latest_pos()));
        if let (true, Some(p)) = (released, pos) {
            if !editor.is_some_and(|e| e.contains(p)) {
                egui::DragAndDrop::clear_payload(ctx);
                drop_on_viewport(app, drag.0, p);
            }
        }
    }
}

/// A sample slot dropped at screen point `p`: on selected polygons of the
/// object under it they get the material (building a Multi/Sub-Object
/// material), else the object does.
fn drop_on_viewport(app: &mut LoomApp, slot: usize, p: egui::Pos2) {
    let Some(node) = crate::viewport_widget::node_at(app, p) else { return };
    let polygon_level = matches!(app.state.selection_mode, loom_commands::SelectionMode::Face | loom_commands::SelectionMode::Element);
    let faces_selected = app.state.selection.contains(&node) && polygon_level && app.state.sub_selection.by_node().any(|(n, s)| n == node && !s.is_empty());
    let _ = if faces_selected {
        app.dispatch(Command::AssignMaterialToFaces { slot: Some(slot as u32) })
    } else {
        app.dispatch(Command::AssignMaterialToNode { node: node.0, slot: Some(slot as u32) })
    };
}

fn menu(ui: &mut egui::Ui, app: &mut LoomApp) {
    egui::menu::bar(ui, |ui| {
        ui.menu_button("Material", |ui| {
            if ui.button("Get Material...").clicked() {
                app.state.dialog = Some(Dialog::MaterialBrowser(BrowserFor::GetMaterial));
                ui.close_menu();
            }
            if ui.button("Pick from Object").clicked() {
                let _ = app.dispatch(Command::PickMaterialFromObject);
                ui.close_menu();
            }
            if ui.add_enabled(!app.state.selection.is_empty(), egui::Button::new("Assign to Selection")).clicked() {
                let _ = app.dispatch(Command::AssignMaterialToSelection);
                ui.close_menu();
            }
            if ui.button("Reset Map/Mtl to Default Settings").clicked() {
                let _ = app.dispatch(Command::ResetMaterial { slot: None });
                ui.close_menu();
            }
            if ui.button("Change Material/Map Type...").clicked() {
                let at_map = app.state.scene.material_editor.path.iter().any(|s| matches!(s, NavStep::Map(_)));
                app.state.dialog = Some(Dialog::MaterialBrowser(if at_map { BrowserFor::MapType } else { BrowserFor::MaterialType }));
                ui.close_menu();
            }
            ui.separator();
            if workflow_buttons(ui, app) {
                ui.close_menu();
            }
        });
        ui.menu_button("Navigation", |ui| {
            if ui.button("Go to Parent").clicked() {
                let _ = app.dispatch(Command::MaterialEditorGoToParent);
                ui.close_menu();
            }
            if ui.button("Go Forward to Sibling").clicked() {
                let _ = app.dispatch(Command::MaterialEditorGoForwardToSibling);
                ui.close_menu();
            }
        });
        ui.menu_button("Options", |ui| {
            let o = app.state.scene.material_editor.options.clone();
            for (label, n) in [("3 X 2 Sample Windows", 6u32), ("5 X 3 Sample Windows", 15), ("6 X 4 Sample Windows", 24)] {
                if ui.radio(o.slots_shown == n, label).clicked() {
                    let _ = app.dispatch(Command::SetMaterialEditorOption { name: "Slots".into(), value: n as f64 });
                    ui.close_menu();
                }
            }
            if ui.button("Options...").clicked() {
                app.state.dialog = Some(Dialog::MaterialEditorOptions);
                ui.close_menu();
            }
        });
    });
}

fn slots(ui: &mut egui::Ui, app: &mut LoomApp) {
    let ctx = ui.ctx().clone();
    let shown = app.state.scene.material_editor.options.slots_shown;
    let (cols, rows) = match shown {
        6 => (3, 2),
        15 => (5, 3),
        _ => (6, 4),
    };
    let size = match cols {
        3 => 96usize,
        5 => 64,
        _ => 56,
    };
    ui.horizontal_top(|ui| {
        // Fewer windows show bigger samples; a scroll bar reaches the other slots.
        let height = rows as f32 * (size as f32 + 4.0) + 2.0;
        egui::ScrollArea::vertical().id_salt("material_slots_scroll").max_height(height).min_scrolled_height(height).show(ui, |ui| {
            egui::Grid::new("material_slots").spacing([4.0, 4.0]).show(ui, |ui| {
                for slot in 0..SLOT_COUNT {
                    slot_widget(ui, app, &ctx, slot, size);
                    if slot % cols == cols - 1 {
                        ui.end_row();
                    }
                }
            });
        });
        side_toolbar(ui, app);
    });
}

fn slot_widget(ui: &mut egui::Ui, app: &mut LoomApp, ctx: &egui::Context, slot: usize, size: usize) {
    let ed = &app.state.scene.material_editor;
    let active = ed.active;
    let setup = PreviewSetup::from_options(&ed.options, ed.sample_type(slot), rotation(ctx, slot));
    let sample = slot_sample(app, slot);
    let tex = preview_texture(ctx, app, egui::Id::new(("material_preview", slot)), &sample, size, &setup);
    let (rect, r) = ui.allocate_exact_size(egui::vec2(size as f32, size as f32), egui::Sense::click_and_drag());
    ui.painter().image(tex.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
    // Hot material (used in the scene): white corner triangles; the active slot a white border.
    let in_scene = app.state.scene.material_editor.slots[slot].is_some_and(|id| app.state.scene.nodes().any(|n| n.material == Some(id)));
    if in_scene {
        let c = egui::Color32::WHITE;
        let k = 9.0;
        for (a, b, d) in [
            (rect.left_top(), rect.left_top() + egui::vec2(k, 0.0), rect.left_top() + egui::vec2(0.0, k)),
            (rect.right_top(), rect.right_top() - egui::vec2(k, 0.0), rect.right_top() + egui::vec2(0.0, k)),
            (rect.left_bottom(), rect.left_bottom() + egui::vec2(k, 0.0), rect.left_bottom() - egui::vec2(0.0, k)),
            (rect.right_bottom(), rect.right_bottom() - egui::vec2(k, 0.0), rect.right_bottom() - egui::vec2(0.0, k)),
        ] {
            ui.painter().add(egui::Shape::convex_polygon(vec![a, b, d], c, egui::Stroke::NONE));
        }
    }
    if slot == active {
        ui.painter().rect_stroke(rect, 0.0, egui::Stroke::new(2.0_f32, egui::Color32::WHITE), egui::StrokeKind::Inside);
    }
    let name = app.state.scene.slot_material(slot).name;
    let r = r.on_hover_text(name);
    let rotate_mode = ctx.data(|d| d.get_temp::<bool>(egui::Id::new("material_drag_rotate"))).unwrap_or(false);
    if rotate_mode && r.dragged() {
        // Drag/Rotate: the sample object turns with the mouse.
        let (yaw, pitch) = rotation(ctx, slot);
        let d = r.drag_delta();
        ctx.data_mut(|m| m.insert_temp(egui::Id::new(("material_slot_rotation", slot)), (yaw + d.x * 0.02, (pitch + d.y * 0.02).clamp(-1.5, 1.5))));
    } else if r.drag_started() {
        // Drag/Copy: onto another slot, an object or selected polygons.
        egui::DragAndDrop::set_payload(ctx, SlotDrag(slot));
    }
    if let Some(from) = r.dnd_release_payload::<SlotDrag>() {
        if from.0 != slot {
            let _ = app.dispatch(Command::CopyMaterialToSlot { from: from.0 as u32, to: slot as u32 });
        }
        return;
    }
    if r.clicked() {
        let _ = app.dispatch(Command::MaterialEditorSlot { slot: slot as u32 });
    }
    if r.double_clicked() {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("material_magnify"), Some(slot)));
    }
    r.context_menu(|ui| {
        let mut mode = rotate_mode;
        if ui.radio(!mode, "Drag/Copy").clicked() {
            mode = false;
            ui.close_menu();
        }
        if ui.radio(mode, "Drag/Rotate").clicked() {
            mode = true;
            ui.close_menu();
        }
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("material_drag_rotate"), mode));
        if ui.button("Reset Rotation").clicked() {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new(("material_slot_rotation", slot)), (0.0f32, 0.0f32)));
            ui.close_menu();
        }
        ui.separator();
        if ui.button("Options...").clicked() {
            app.state.dialog = Some(Dialog::MaterialEditorOptions);
            ui.close_menu();
        }
        if ui.button("Magnify...").clicked() {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("material_magnify"), Some(slot)));
            ui.close_menu();
        }
        let used = app.state.scene.material_editor.slots[slot].is_some_and(|id| app.state.scene.nodes().any(|n| n.material == Some(id)));
        if ui.add_enabled(used, egui::Button::new("Select By Material")).clicked() {
            let _ = app.dispatch(Command::MaterialEditorSlot { slot: slot as u32 });
            select_by_material(app);
            ui.close_menu();
        }
        ui.separator();
        let shown = app.state.scene.material_editor.options.slots_shown;
        for (label, n) in [("3 X 2 Sample Windows", 6u32), ("5 X 3 Sample Windows", 15), ("6 X 4 Sample Windows", 24)] {
            if ui.radio(shown == n, label).clicked() {
                let _ = app.dispatch(Command::SetMaterialEditorOption { name: "Slots".into(), value: n as f64 });
                ui.close_menu();
            }
        }
    });
    if slot + 1 == SLOT_COUNT {
        magnify(ctx, app);
    }
}

/// Magnify: the slot in a window of its own, at any size.
fn magnify(ctx: &egui::Context, app: &mut LoomApp) {
    let id = egui::Id::new("material_magnify");
    let Some(slot) = ctx.data(|d| d.get_temp::<Option<usize>>(id)).flatten() else { return };
    let mut open = true;
    let name = app.state.scene.slot_material(slot).name;
    egui::Window::new(format!("{name} (magnified)")).id(egui::Id::new("material_magnify_window")).open(&mut open).default_size([300.0, 300.0]).resizable(true).show(ctx, |ui| {
        let side = ui.available_width().min(ui.available_height().max(200.0)).clamp(64.0, 512.0) as usize;
        let ed = &app.state.scene.material_editor;
        let setup = PreviewSetup::from_options(&ed.options, ed.sample_type(slot), rotation(ctx, slot));
        let sample = slot_sample(app, slot);
        let tex = preview_texture(ctx, app, egui::Id::new(("material_magnified", slot)), &sample, side, &setup);
        ui.image(egui::load::SizedTexture::new(tex.id(), egui::vec2(side as f32, side as f32)));
    });
    if !open {
        ctx.data_mut(|d| d.insert_temp(id, None::<usize>));
    }
}

/// The toolbar right of the slots: Sample Type, Backlight, Background,
/// Sample UV Tiling, Options, Material/Map Navigator.
fn side_toolbar(ui: &mut egui::Ui, app: &mut LoomApp) {
    ui.vertical(|ui| {
        let ed = &app.state.scene.material_editor;
        let kind = ed.sample_type(ed.active);
        let o = ed.options.clone();
        egui::ComboBox::from_id_salt("material_sample_type").width(70.0).selected_text(match kind {
            SampleType::Sphere => "Sphere",
            SampleType::Cylinder => "Cylinder",
            SampleType::Cube => "Cube",
        }).show_ui(ui, |ui| {
            for (t, label) in [(SampleType::Sphere, "Sphere"), (SampleType::Cylinder, "Cylinder"), (SampleType::Cube, "Cube")] {
                if ui.selectable_label(kind == t, label).clicked() {
                    let _ = app.dispatch(Command::SetSampleType { kind: label.into() });
                }
            }
        }).response.on_hover_text("Sample Type");
        if ui.selectable_label(o.backlight, "Backlight").clicked() {
            let _ = app.dispatch(Command::SetMaterialEditorOption { name: "Backlight".into(), value: if o.backlight { 0.0 } else { 1.0 } });
        }
        if ui.selectable_label(o.background, "Background").clicked() {
            let _ = app.dispatch(Command::SetMaterialEditorOption { name: "Background".into(), value: if o.background { 0.0 } else { 1.0 } });
        }
        egui::ComboBox::from_id_salt("material_uv_tiling").width(70.0).selected_text(format!("{0}x{0}", o.uv_tiling)).show_ui(ui, |ui| {
            for n in 1..=4u32 {
                if ui.selectable_label(o.uv_tiling == n, format!("{n}x{n}")).clicked() {
                    let _ = app.dispatch(Command::SetMaterialEditorOption { name: "Sample UV Tiling".into(), value: n as f64 });
                }
            }
        }).response.on_hover_text("Sample UV Tiling");
        if ui.button("Options").on_hover_text("Material Editor Options").clicked() {
            app.state.dialog = Some(Dialog::MaterialEditorOptions);
        }
        if ui.selectable_label(app.state.material_navigator_open, "Navigator").on_hover_text("Material/Map Navigator").clicked() {
            let _ = app.dispatch(Command::MaterialMapNavigator);
        }
    });
}

/// The toolbar below the slots.
fn toolbar(ui: &mut egui::Ui, app: &mut LoomApp) {
    let (_, m, maps) = current_level(app);
    let below_top = !app.state.scene.material_editor.path.is_empty();
    ui.horizontal_wrapped(|ui| {
        if ui.button("Get Material").on_hover_text("Get Material: the Material/Map Browser").clicked() {
            app.state.dialog = Some(Dialog::MaterialBrowser(BrowserFor::GetMaterial));
        }
        if ui.add_enabled(!app.state.selection.is_empty(), egui::Button::new("Assign to Selection")).on_hover_text("Assign Material to Selection").clicked() {
            let _ = app.dispatch(Command::AssignMaterialToSelection);
        }
        if ui.button("Reset").on_hover_text("Reset Map/Mtl to Default Settings").clicked() {
            let _ = app.dispatch(Command::ResetMaterial { slot: None });
        }
        let shown = m.show_map_in_viewport;
        if ui.add_enabled(maps.is_empty(), egui::Button::new("Show Map in Viewport").selected(shown)).on_hover_text("Show Standard Map in Viewport").clicked() {
            let _ = app.dispatch(Command::SetMaterialParameter { slot: None, name: "Show Map in Viewport".into(), value: if shown { 0.0 } else { 1.0 } });
        }
        if ui.add_enabled(below_top, egui::Button::new("Go to Parent")).clicked() {
            let _ = app.dispatch(Command::MaterialEditorGoToParent);
        }
        if ui.add_enabled(below_top, egui::Button::new("Go Forward to Sibling")).clicked() {
            let _ = app.dispatch(Command::MaterialEditorGoForwardToSibling);
        }
        workflow_buttons(ui, app);
    });
}

/// Put Material to Scene, Make Material Copy, Make Unique, Put to Library,
/// Select By Material (toolbar and Material menu alike).
fn workflow_buttons(ui: &mut egui::Ui, app: &mut LoomApp) -> bool {
    let ed = &app.state.scene.material_editor;
    let top = ed.slots[ed.active].filter(|id| app.state.scene.materials.contains_key(id));
    let at_sub = matches!(ed.path.last(), Some(NavStep::Sub(_)));
    let used = top.is_some_and(|id| app.state.scene.nodes().any(|n| n.material == Some(id)));
    let mut clicked = false;
    if ui.add_enabled(top.is_some(), egui::Button::new("Put Material to Scene")).on_hover_text("Objects using a material of this name get this one").clicked() {
        let _ = app.dispatch(Command::PutMaterialToScene);
        clicked = true;
    }
    if ui.add_enabled(used, egui::Button::new("Make Material Copy")).on_hover_text("The slot gets a copy no object uses").clicked() {
        let _ = app.dispatch(Command::MakeMaterialCopy);
        clicked = true;
    }
    if ui.add_enabled(at_sub, egui::Button::new("Make Unique")).on_hover_text("This sub-material gets a copy of its own").clicked() {
        let _ = app.dispatch(Command::MaterialMakeUnique);
        clicked = true;
    }
    if ui.button("Put to Library").clicked() {
        app.state.dialog = Some(Dialog::PutToLibrary);
        clicked = true;
    }
    if ui.add_enabled(used, egui::Button::new("Select By Material")).clicked() {
        select_by_material(app);
        clicked = true;
    }
    clicked
}

/// Select By Material: the objects using the slot's material are selected
/// and Select From Scene opens with them highlighted.
fn select_by_material(app: &mut LoomApp) {
    if app.dispatch(Command::SelectByMaterial).is_ok() {
        let _ = app.dispatch(Command::SelectByName);
    }
}

/// Put to Library: the name, then OK puts the slot's material into the current library.
pub fn put_to_library(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("put_to_library_name");
    let mut name: String = ctx.data(|d| d.get_temp(id)).unwrap_or_else(|| app.state.scene.slot_material(app.state.scene.material_editor.active).name);
    let mut done = None;
    egui::Window::new("Put to Library").open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.text_edit_singleline(&mut name);
        });
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                done = Some(true);
            }
            if ui.button("Cancel").clicked() {
                done = Some(false);
            }
        });
    });
    match done {
        Some(ok) => {
            if ok {
                let _ = app.dispatch(Command::PutToLibrary { name: Some(name) });
            }
            ctx.data_mut(|d| d.remove::<String>(id));
            *open = false;
        }
        None => ctx.data_mut(|d| d.insert_temp(id, name)),
    }
}

/// Eyedropper, the name field and the Type button.
fn name_row(ui: &mut egui::Ui, app: &mut LoomApp, m: &StandardMaterial, maps: &[String]) {
    ui.horizontal(|ui| {
        let picking = app.state.pick_mode == Some(loom_commands::PickMode::PickMaterial);
        if ui.selectable_label(picking, "Pick").on_hover_text("Pick Material From Object: click an object in a viewport").clicked() {
            let _ = app.dispatch(if picking { Command::ExitPickMode } else { Command::PickMaterialFromObject });
        }
        let map = map_at(m, maps);
        let current = map.map(|x| x.name.clone()).unwrap_or_else(|| m.name.clone());
        let level_key = format!("{:?}", app.state.scene.material_editor.path);
        let name_id = egui::Id::new(("material_name", app.state.scene.material_editor.active, level_key));
        let mut name: String = ui.ctx().data(|d| d.get_temp(name_id)).unwrap_or_else(|| current.clone());
        let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(180.0));
        if r.lost_focus() && name != current && !name.trim().is_empty() {
            let _ = app.dispatch(Command::RenameMaterial { slot: None, name: name.clone() });
        }
        if r.has_focus() {
            ui.ctx().data_mut(|d| d.insert_temp(name_id, name));
        } else {
            ui.ctx().data_mut(|d| d.remove::<String>(name_id));
        }
        let type_label = map.map(|x| x.kind.label()).unwrap_or(m.kind.label());
        if ui.button(type_label).on_hover_text("Type: choose a material or map type").clicked() {
            app.state.dialog = Some(Dialog::MaterialBrowser(if map.is_some() { BrowserFor::MapType } else { BrowserFor::MaterialType }));
        }
    });
}

// ---------------------------------------------------------------------------
// Rollouts per level
// ---------------------------------------------------------------------------

/// A colour swatch of the material at the current level (`which` as Set Material Color names it).
fn swatch(ui: &mut egui::Ui, app: &mut LoomApp, which: &str, c: glam::Vec3) {
    let mut rgb = rgb_of(c);
    if ui.color_edit_button_srgb(&mut rgb).on_hover_text(which).changed() {
        let _ = app.dispatch(Command::SetMaterialColor { slot: None, which: which.into(), rgb });
    }
}

/// The highlight curve graph: intensity across the highlight for each lobe
/// (Specular Level the height, Glossiness the width; an anisotropic lobe's
/// narrow axis in white).
fn highlight_graph(ui: &mut egui::Ui, lobes: &[(f32, f32, f32)]) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(120.0, 46.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, egui::Color32::from_gray(30));
    for (i, (level, gloss, aniso)) in lobes.iter().enumerate() {
        let exponent = 2.0 * 2f32.powf(gloss / 10.0);
        for (narrow, color) in [(false, egui::Color32::from_gray(150)), (true, egui::Color32::WHITE)] {
            if narrow && *aniso <= 0.0 {
                continue;
            }
            let w = if narrow { 1.0 - 0.95 * aniso / 100.0 } else { 1.0 };
            let pts: Vec<egui::Pos2> = (0..=60)
                .map(|k| {
                    let x = k as f32 / 60.0 * 2.0 - 1.0;
                    let a = (x / w.max(0.05)).clamp(-1.0, 1.0) * std::f32::consts::FRAC_PI_2;
                    let y = (level / 100.0) * a.cos().max(0.0).powf(exponent / 8.0);
                    egui::pos2(rect.center().x + x * rect.width() / 2.0, rect.bottom() - 2.0 - y.min(1.2) / 1.2 * (rect.height() - 4.0))
                })
                .collect();
            p.add(egui::Shape::line(pts, egui::Stroke::new(1.0_f32, if i == 0 { color } else { egui::Color32::from_rgb(200, 200, 90) })));
        }
    }
}

fn standard_rollouts(ui: &mut egui::Ui, app: &mut LoomApp, m: &StandardMaterial) {
    use loom_scene::material::Shader;
    let sh = m.shading.clone();
    crate::command_panel::rollout(ui, "Shader Basic Parameters", true, |ui| {
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("material_shader").width(130.0).selected_text(sh.shader.label()).show_ui(ui, |ui| {
                for s in Shader::ALL {
                    if ui.selectable_label(sh.shader == s, s.label()).clicked() {
                        let _ = app.dispatch(Command::SetShader { shader: s.label().into() });
                    }
                }
            });
            check(ui, app, "Wire", "Wire", m.wire);
            check(ui, app, "2-Sided", "2-Sided", m.two_sided);
            check(ui, app, "Faceted", "Faceted", m.faceted);
        });
    });
    let title = format!("{} Basic Parameters", sh.shader.label().trim_end_matches(" Shader"));
    crate::command_panel::rollout(ui, &title, true, |ui| {
        egui::Grid::new("blinn_colours").num_columns(3).show(ui, |ui| {
            // Strauss has only Color; Metal makes its own specular colour.
            let rows: &[(&str, &str, &str)] = match sh.shader {
                Shader::Strauss => &[("Color", "Diffuse", "Diffuse Color")],
                Shader::Metal => &[("Ambient", "Ambient", "Ambient Color"), ("Diffuse", "Diffuse", "Diffuse Color")],
                Shader::MultiLayer => &[("Ambient", "Ambient", "Ambient Color"), ("Diffuse", "Diffuse", "Diffuse Color")],
                _ => &[("Ambient", "Ambient", "Ambient Color"), ("Diffuse", "Diffuse", "Diffuse Color"), ("Specular", "Specular", "Specular Color")],
            };
            for (label, which, channel) in rows {
                let c = match *which {
                    "Ambient" => m.ambient.get(),
                    "Diffuse" => m.diffuse.get(),
                    _ => m.specular.get(),
                };
                ui.label(format!("{label}:"));
                swatch(ui, app, which, c);
                map_button(ui, app, channel, m.map(channel), true);
                ui.end_row();
            }
        });
        if sh.shader != Shader::Strauss {
            ui.horizontal(|ui| {
                check(ui, app, "Lock Ambient and Diffuse", "Ambient/Diffuse Lock", m.ambient_locked);
                if !matches!(sh.shader, Shader::Metal | Shader::MultiLayer) {
                    check(ui, app, "Lock Diffuse and Specular", "Diffuse/Specular Lock", sh.diffuse_specular_locked);
                }
            });
        }
        if sh.shader.has_diffuse_level() || sh.shader.has_roughness() {
            ui.label(egui::RichText::new("Advanced Diffuse").strong());
            if sh.shader.has_diffuse_level() {
                spinner(ui, app, "Diffuse Level", sh.diffuse_level, 0.0..=400.0, 0);
            }
            if sh.shader.has_roughness() {
                spinner(ui, app, "Roughness", sh.roughness, 0.0..=100.0, 0);
            }
        }
        if sh.shader != Shader::Strauss {
            ui.label(egui::RichText::new("Self-Illumination").strong());
            ui.horizontal(|ui| {
                check(ui, app, "Color", "Self-Illum Color On", sh.self_illum_color_on);
                if sh.self_illum_color_on {
                    swatch(ui, app, "Self-Illum Color", sh.self_illum_color);
                } else {
                    spinner(ui, app, "Self-Illumination", m.self_illum.get(), 0.0..=100.0, 0);
                }
                map_button(ui, app, "Self-Illumination", m.map("Self-Illumination"), true);
            });
        }
        ui.horizontal(|ui| {
            spinner(ui, app, "Opacity", m.opacity.get(), 0.0..=100.0, 0);
            map_button(ui, app, "Opacity", m.map("Opacity"), true);
        });
        if sh.shader == Shader::Translucent {
            ui.label(egui::RichText::new("Translucency").strong());
            egui::Grid::new("translucency").num_columns(3).show(ui, |ui| {
                ui.label("Translucent Clr:");
                swatch(ui, app, "Translucent Color", sh.translucent_color);
                ui.label("");
                ui.end_row();
                ui.label("Filter Color:");
                swatch(ui, app, "Filter Color", m.extended.filter_color);
                map_button(ui, app, "Filter Color", m.map("Filter Color"), true);
                ui.end_row();
            });
        }
        let first = if sh.shader == Shader::MultiLayer { "First Specular Layer" } else { "Specular Highlights" };
        ui.label(egui::RichText::new(first).strong());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                if sh.shader == Shader::MultiLayer {
                    ui.horizontal(|ui| {
                        ui.label("Color:");
                        swatch(ui, app, "Specular", m.specular.get());
                        map_button(ui, app, "Specular Color", m.map("Specular Color"), true);
                    });
                }
                if sh.shader != Shader::Strauss {
                    ui.horizontal(|ui| {
                        let label = if sh.shader == Shader::MultiLayer { "Level" } else { "Specular Level" };
                        spinner_with(ui, app, label, m.specular_level.get(), 0.0..=999.0, 0, |v| Command::SetMaterialParameter { slot: None, name: "Specular Level".into(), value: v });
                        map_button(ui, app, "Specular Level", m.map("Specular Level"), true);
                    });
                }
                ui.horizontal(|ui| {
                    spinner(ui, app, "Glossiness", m.glossiness.get(), 0.0..=100.0, 0);
                    map_button(ui, app, "Glossiness", m.map("Glossiness"), true);
                });
                match sh.shader {
                    Shader::Strauss => spinner(ui, app, "Metalness", sh.metalness, 0.0..=100.0, 0),
                    Shader::Anisotropic | Shader::MultiLayer => {
                        spinner(ui, app, "Anisotropy", sh.anisotropy, 0.0..=100.0, 0);
                        spinner(ui, app, "Orientation", sh.orientation, 0.0..=9999.0, 0);
                    }
                    Shader::Metal => {}
                    _ => spinner(ui, app, "Soften", m.soften.get(), 0.0..=1.0, 2),
                }
                if sh.shader == Shader::Translucent {
                    check(ui, app, "Backside specular", "Backside Specular", sh.backside_specular);
                }
            });
            let level = if sh.shader == Shader::Strauss { 50.0 + sh.metalness / 2.0 } else { m.specular_level.get() };
            let aniso = if matches!(sh.shader, Shader::Anisotropic | Shader::MultiLayer) { sh.anisotropy } else { 0.0 };
            let mut lobes = vec![(level, m.glossiness.get(), aniso)];
            if sh.shader == Shader::MultiLayer {
                lobes.push((sh.level2, sh.glossiness2, sh.anisotropy2));
            }
            highlight_graph(ui, &lobes);
        });
        if sh.shader == Shader::MultiLayer {
            ui.label(egui::RichText::new("Second Specular Layer").strong());
            ui.horizontal(|ui| {
                ui.label("Color:");
                swatch(ui, app, "Specular 2", sh.specular2);
            });
            spinner(ui, app, "Level 2", sh.level2, 0.0..=999.0, 0);
            spinner(ui, app, "Glossiness 2", sh.glossiness2, 0.0..=100.0, 0);
            spinner(ui, app, "Anisotropy 2", sh.anisotropy2, 0.0..=100.0, 0);
            spinner(ui, app, "Orientation 2", sh.orientation2, 0.0..=9999.0, 0);
        }
    });
    extended_rollout(ui, app, m);
    maps_rollout(ui, app, m);
}

/// The Maps rollout: each channel's check box, Amount and map button.
fn maps_rollout(ui: &mut egui::Ui, app: &mut LoomApp, m: &StandardMaterial) {
    crate::command_panel::rollout(ui, "Maps", false, |ui| {
        egui::Grid::new("material_maps").num_columns(4).show(ui, |ui| {
            ui.label("");
            ui.label("Amount");
            ui.label("Map");
            ui.end_row();
            for (channel, _) in MAP_CHANNELS.iter() {
                let map = m.map(channel).cloned();
                let mut on = map.as_ref().is_some_and(|b| b.enabled);
                if ui.add_enabled(map.is_some(), egui::Checkbox::new(&mut on, *channel)).changed() {
                    let _ = app.dispatch(Command::SetMaterialMapEnabled { map: (*channel).into(), enabled: on });
                }
                let mut amount = map.as_ref().map(|b| b.amount).unwrap_or(0.0);
                let range = if *channel == "Bump" { -999.0..=999.0 } else { 0.0..=100.0 };
                let r = ui.add_enabled(map.is_some(), crate::spinner::spin(ui, &mut amount, range).speed(0.5));
                if r.drag_started() {
                    app.state.undo.begin("Map Amount");
                }
                if r.changed() {
                    let _ = app.dispatch(Command::SetMaterialMapAmount { map: (*channel).into(), amount });
                }
                if r.drag_stopped() {
                    app.state.undo.end();
                }
                ui.horizontal(|ui| {
                    map_button(ui, app, channel, map.as_ref(), false);
                    if map.is_some() && crate::icons::small(ui, crate::icons::CLEAR, "Clear the map").clicked() {
                        let _ = app.dispatch(Command::SetMaterialMap { slot: None, map: (*channel).into(), path: None, amount: None });
                    }
                });
                ui.end_row();
            }
        });
    });
}

/// Extended Parameters: Advanced Transparency (blank for the Translucent
/// shader), Index of Refraction, Wire size, Reflection Dimming.
fn extended_rollout(ui: &mut egui::Ui, app: &mut LoomApp, m: &StandardMaterial) {
    let e = m.extended.clone();
    let translucent = m.shading.shader == loom_scene::material::Shader::Translucent;
    crate::command_panel::rollout(ui, "Extended Parameters", false, |ui| {
        let set = |app: &mut LoomApp, name: &str, v: f64| {
            let _ = app.dispatch(Command::SetMaterialParameter { slot: None, name: name.into(), value: v });
        };
        ui.label(egui::RichText::new("Advanced Transparency").strong());
        if !translucent {
            ui.horizontal(|ui| {
                ui.label("Falloff:");
                if ui.radio(!e.falloff_out, "In").clicked() {
                    set(app, "Falloff Out", 0.0);
                }
                if ui.radio(e.falloff_out, "Out").clicked() {
                    set(app, "Falloff Out", 1.0);
                }
                spinner_with(ui, app, "Amt:", e.falloff_amount, 0.0..=100.0, 0, |v| Command::SetMaterialParameter { slot: None, name: "Falloff Amount".into(), value: v });
            });
            ui.horizontal(|ui| {
                ui.label("Type:");
                for (i, label) in ["Filter", "Subtractive", "Additive"].iter().enumerate() {
                    if ui.radio(e.transparency_type == i as u8, *label).clicked() {
                        set(app, "Transparency Type", i as f64);
                    }
                    if i == 0 {
                        swatch(ui, app, "Filter Color", e.filter_color);
                        map_button(ui, app, "Filter Color", m.map("Filter Color"), true);
                    }
                }
            });
        }
        spinner(ui, app, "Index of Refraction", e.ior, 0.01..=10.0, 3);
        ui.label(egui::RichText::new("Wire").strong());
        ui.horizontal(|ui| {
            spinner(ui, app, "Wire Size", e.wire_size, 0.0..=10000.0, 1);
            ui.label("In:");
            if ui.radio(!e.wire_units, "Pixels").clicked() {
                set(app, "Wire Units", 0.0);
            }
            if ui.radio(e.wire_units, "Units").clicked() {
                set(app, "Wire Units", 1.0);
            }
        });
        if m.shading.shader != loom_scene::material::Shader::Strauss {
            ui.label(egui::RichText::new("Reflection Dimming").strong());
            ui.horizontal(|ui| {
                check(ui, app, "Apply", "Reflection Dimming", e.reflection_dimming);
                spinner(ui, app, "Dim Level", e.dim_level, 0.0..=1.0, 2);
                spinner(ui, app, "Refl. Level", e.refl_level, 0.1..=10.0, 2);
            });
        }
    });
}

/// A sub-material button (list position `index`) with its on/off check box (`on` = None: no check box).
fn sub_button(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, index: usize, id: Option<MaterialId>, on: Option<(bool, &str)>) {
    ui.label(label);
    if ui.add(egui::Button::new(material_label(app, id)).min_size(egui::vec2(150.0, 0.0))).clicked() {
        let _ = app.dispatch(Command::MaterialEditorGoToSub { index: index as u32 });
    }
    if let Some((enabled, name)) = on {
        let mut v = enabled;
        if ui.checkbox(&mut v, "").changed() {
            let _ = app.dispatch(Command::SetMaterialParameter { slot: None, name: name.into(), value: if v { 1.0 } else { 0.0 } });
        }
    }
    ui.end_row();
}

/// A colour row: label, swatch, and the map button of `channel` if any.
fn color_line(ui: &mut egui::Ui, app: &mut LoomApp, m: &StandardMaterial, label: &str, which: &str, c: glam::Vec3, channel: Option<&str>) {
    ui.label(label);
    swatch(ui, app, which, c);
    match channel {
        Some(ch) => map_button(ui, app, ch, m.map(ch), true),
        None => {
            ui.label("");
        }
    }
    ui.end_row();
}

/// Raytrace Basic Parameters (with the shader's highlight), Extended Parameters and Maps.
fn raytrace_rollouts(ui: &mut egui::Ui, app: &mut LoomApp, m: &StandardMaterial) {
    use loom_scene::material::Shader;
    let MaterialKind::Raytrace(r) = &m.kind else { return };
    let r = (**r).clone();
    let sh = m.shading.shader;
    crate::command_panel::rollout(ui, "Raytrace Basic Parameters", true, |ui| {
        ui.horizontal(|ui| {
            ui.label("Shading:");
            egui::ComboBox::from_id_salt("raytrace_shader").width(120.0).selected_text(sh.label()).show_ui(ui, |ui| {
                for s in [Shader::Anisotropic, Shader::Blinn, Shader::Metal, Shader::OrenNayarBlinn, Shader::Phong] {
                    if ui.selectable_label(sh == s, s.label()).clicked() {
                        let _ = app.dispatch(Command::SetShader { shader: s.label().into() });
                    }
                }
            });
            check(ui, app, "2-Sided", "2-Sided", m.two_sided);
            check(ui, app, "Wire", "Wire", m.wire);
            check(ui, app, "Faceted", "Faceted", m.faceted);
        });
        egui::Grid::new("raytrace_colours").num_columns(3).show(ui, |ui| {
            color_line(ui, app, m, "Ambient:", "Ambient", r.ambient, Some("Ambient Color"));
            color_line(ui, app, m, "Diffuse:", "Diffuse", m.diffuse.get(), Some("Diffuse Color"));
            color_line(ui, app, m, "Reflect:", "Reflect", r.reflect, Some("Reflection"));
            color_line(ui, app, m, "Luminosity:", "Luminosity", r.luminosity, Some("Self-Illumination"));
            color_line(ui, app, m, "Transparency:", "Transparency", r.transparency, Some("Refraction"));
        });
        check(ui, app, "Fresnel", "Fresnel", r.fresnel);
        spinner(ui, app, "Index of Refr.", r.ior, 0.001..=10.0, 2);
        ui.label(egui::RichText::new("Specular Highlight").strong());
        egui::Grid::new("raytrace_spec").num_columns(3).show(ui, |ui| {
            color_line(ui, app, m, "Specular Color:", "Specular", m.specular.get(), Some("Specular Color"));
        });
        spinner(ui, app, "Specular Level", m.specular_level.get(), 0.0..=999.0, 0);
        spinner(ui, app, "Glossiness", m.glossiness.get(), 0.0..=100.0, 0);
        match sh {
            Shader::Anisotropic => {
                spinner(ui, app, "Anisotropy", m.shading.anisotropy, 0.0..=100.0, 0);
                spinner(ui, app, "Orientation", m.shading.orientation, 0.0..=9999.0, 0);
            }
            Shader::OrenNayarBlinn => {
                spinner(ui, app, "Diffuse Level", m.shading.diffuse_level, 0.0..=400.0, 0);
                spinner(ui, app, "Roughness", m.shading.roughness, 0.0..=100.0, 0);
            }
            _ => spinner(ui, app, "Soften", m.soften.get(), 0.0..=1.0, 2),
        }
    });
    crate::command_panel::rollout(ui, "Extended Parameters", false, |ui| {
        egui::Grid::new("raytrace_ext").num_columns(3).show(ui, |ui| {
            color_line(ui, app, m, "Extra Lighting:", "Extra Lighting", r.extra_lighting, None);
            color_line(ui, app, m, "Translucency:", "Translucency", r.translucency, None);
            color_line(ui, app, m, "Fluorescence:", "Fluorescence", r.fluorescence, None);
        });
        spinner(ui, app, "Fluorescence Bias", r.fluorescence_bias, 0.0..=1.0, 2);
    });
    maps_rollout(ui, app, m);
}

/// Architectural: Templates, Physical Qualities, Special Effects.
fn architectural_rollouts(ui: &mut egui::Ui, app: &mut LoomApp, m: &StandardMaterial) {
    use loom_scene::material_types::ARCH_TEMPLATES;
    let MaterialKind::Architectural(a) = &m.kind else { return };
    let a = (**a).clone();
    crate::command_panel::rollout(ui, "Templates", true, |ui| {
        let current = ARCH_TEMPLATES.get(a.template as usize).map_or("User Defined", |t| t.0);
        egui::ComboBox::from_id_salt("arch_template").width(200.0).selected_text(current).show_ui(ui, |ui| {
            for (i, t) in ARCH_TEMPLATES.iter().enumerate() {
                if ui.selectable_label(a.template as usize == i, t.0).clicked() {
                    let _ = app.dispatch(Command::SetMaterialParameter { slot: None, name: "Template".into(), value: i as f64 });
                }
            }
        });
    });
    crate::command_panel::rollout(ui, "Physical Qualities", true, |ui| {
        egui::Grid::new("arch_physical").num_columns(3).show(ui, |ui| {
            color_line(ui, app, m, "Diffuse Color:", "Diffuse", m.diffuse.get(), Some("Diffuse Color"));
        });
        for (label, value, max, channel) in [("Shininess", a.shininess, 100.0, "Glossiness"), ("Transparency", a.transparency, 100.0, "Opacity"), ("Translucency", a.translucency, 100.0, "Filter Color")] {
            ui.horizontal(|ui| {
                spinner(ui, app, label, value, 0.0..=max, 1);
                map_button(ui, app, channel, m.map(channel), true);
            });
        }
        ui.horizontal(|ui| {
            spinner(ui, app, "Index of Refraction", a.ior, 1.0..=2.5, 2);
            map_button(ui, app, "Refraction", m.map("Refraction"), true);
        });
        ui.horizontal(|ui| {
            spinner(ui, app, "Luminance cd/m2", a.luminance, 0.0..=1.0e7, 1);
            map_button(ui, app, "Self-Illumination", m.map("Self-Illumination"), true);
        });
        ui.horizontal(|ui| {
            check(ui, app, "2-Sided", "2-Sided", m.two_sided);
            check(ui, app, "Raw Diffuse Texture", "Raw Diffuse Texture", a.raw_diffuse);
        });
    });
    crate::command_panel::rollout(ui, "Special Effects", false, |ui| {
        egui::Grid::new("arch_effects").num_columns(2).show(ui, |ui| {
            for ch in ["Bump", "Displacement"] {
                ui.label(format!("{ch}:"));
                map_button(ui, app, ch, m.map(ch), false);
                ui.end_row();
            }
        });
    });
}

/// Double Sided, Top/Bottom and Shellac: two sub-materials and their values.
fn two_material_rollout(ui: &mut egui::Ui, app: &mut LoomApp, m: &StandardMaterial) {
    match &m.kind {
        MaterialKind::DoubleSided(d) => {
            let d = (**d).clone();
            crate::command_panel::rollout(ui, "Double Sided Basic Parameters", true, |ui| {
                spinner(ui, app, "Translucency", d.translucency, 0.0..=100.0, 1);
                egui::Grid::new("double_sided_rows").num_columns(3).show(ui, |ui| {
                    sub_button(ui, app, "Facing Material:", 0, d.materials[0], Some((d.enabled[0], "Facing Material On")));
                    sub_button(ui, app, "Back Material:", 1, d.materials[1], Some((d.enabled[1], "Back Material On")));
                });
            });
        }
        MaterialKind::TopBottom(t) => {
            let t = (**t).clone();
            crate::command_panel::rollout(ui, "Top/Bottom Basic Parameters", true, |ui| {
                egui::Grid::new("top_bottom_rows").num_columns(3).show(ui, |ui| {
                    sub_button(ui, app, "Top Material:", 0, t.materials[0], Some((t.enabled[0], "Top Material On")));
                    sub_button(ui, app, "Bottom Material:", 1, t.materials[1], Some((t.enabled[1], "Bottom Material On")));
                });
                if ui.button("Swap").clicked() {
                    let _ = app.dispatch(Command::SetMaterialParameter { slot: None, name: "Swap".into(), value: 1.0 });
                }
                ui.horizontal(|ui| {
                    ui.label("Coordinates:");
                    if ui.radio(!t.local, "World").clicked() {
                        let _ = app.dispatch(Command::SetMaterialParameter { slot: None, name: "Local".into(), value: 0.0 });
                    }
                    if ui.radio(t.local, "Local").clicked() {
                        let _ = app.dispatch(Command::SetMaterialParameter { slot: None, name: "Local".into(), value: 1.0 });
                    }
                });
                spinner(ui, app, "Blend", t.blend, 0.0..=100.0, 0);
                spinner(ui, app, "Position", t.position, 0.0..=100.0, 0);
            });
        }
        MaterialKind::Shellac(sh) => {
            let sh = (**sh).clone();
            crate::command_panel::rollout(ui, "Shellac Basic Parameters", true, |ui| {
                egui::Grid::new("shellac_rows").num_columns(2).show(ui, |ui| {
                    sub_button(ui, app, "Base Material:", 0, sh.materials[0], None);
                    sub_button(ui, app, "Shellac Material:", 1, sh.materials[1], None);
                });
                spinner(ui, app, "Shellac Color Blend", sh.blend, 0.0..=1000.0, 1);
            });
        }
        _ => {}
    }
}

/// Composite Basic Parameters: Base Material and Mat 1–9 with their A/S/M mode and Amount.
fn composite_rollout(ui: &mut egui::Ui, app: &mut LoomApp, m: &StandardMaterial) {
    let MaterialKind::Composite(c) = &m.kind else { return };
    let c = (**c).clone();
    crate::command_panel::rollout(ui, "Composite Basic Parameters", true, |ui| {
        egui::Grid::new("composite_base").num_columns(2).show(ui, |ui| {
            sub_button(ui, app, "Base Material:", 0, c.base, None);
        });
        egui::Grid::new("composite_rows").num_columns(5).show(ui, |ui| {
            for (i, l) in c.layers.iter().enumerate() {
                let n = i + 1;
                let mut on = l.enabled;
                if ui.checkbox(&mut on, format!("Mat.{n}:")).changed() {
                    let _ = app.dispatch(Command::SetMaterialParameter { slot: None, name: format!("Mat {n} On"), value: if on { 1.0 } else { 0.0 } });
                }
                if ui.add(egui::Button::new(material_label(app, l.material)).min_size(egui::vec2(130.0, 0.0))).clicked() {
                    let _ = app.dispatch(Command::MaterialEditorGoToSub { index: n as u32 });
                }
                ui.horizontal(|ui| {
                    for (k, label) in ["A", "S", "M"].iter().enumerate() {
                        if ui.selectable_label(l.mode == k as u8, *label).on_hover_text(["Additive", "Subtractive", "Mix"][k]).clicked() {
                            let _ = app.dispatch(Command::SetMaterialParameter { slot: None, name: format!("Mat {n} Mode"), value: k as f64 });
                        }
                    }
                });
                let max = if l.mode == 2 { 100.0 } else { 200.0 };
                let name = format!("Mat {n} Amount");
                spinner_with(ui, app, "", l.amount, 0.0..=max, 1, move |v| Command::SetMaterialParameter { slot: None, name: name.clone(), value: v });
                ui.end_row();
            }
        });
    });
}

/// Matte/Shadow Basic Parameters.
fn matte_shadow_rollout(ui: &mut egui::Ui, app: &mut LoomApp, m: &StandardMaterial) {
    let MaterialKind::MatteShadow(ms) = &m.kind else { return };
    let ms = (**ms).clone();
    crate::command_panel::rollout(ui, "Matte/Shadow Basic Parameters", true, |ui| {
        ui.label(egui::RichText::new("Matte").strong());
        check(ui, app, "Opaque Alpha", "Opaque Alpha", ms.opaque_alpha);
        ui.label(egui::RichText::new("Atmosphere").strong());
        check(ui, app, "Apply Atmosphere", "Apply Atmosphere", ms.apply_atmosphere);
        ui.label(egui::RichText::new("Shadow").strong());
        ui.horizontal(|ui| {
            check(ui, app, "Receive Shadows", "Receive Shadows", ms.receive_shadows);
            check(ui, app, "Affect Alpha", "Affect Alpha", ms.affect_alpha);
        });
        ui.horizontal(|ui| {
            spinner(ui, app, "Shadow Brightness", ms.shadow_brightness, 0.0..=1.0, 2);
            ui.label("Color:");
            swatch(ui, app, "Color", ms.color);
        });
        ui.label(egui::RichText::new("Reflection").strong());
        ui.horizontal(|ui| {
            spinner(ui, app, "Amount", ms.reflection_amount, 0.0..=100.0, 1);
            ui.label("Map:");
            map_button(ui, app, "Reflection", m.map("Reflection"), false);
        });
    });
}

/// Ink 'n Paint: Paint Controls and Ink Controls.
fn ink_paint_rollouts(ui: &mut egui::Ui, app: &mut LoomApp, m: &StandardMaterial) {
    let MaterialKind::InkPaint(ink) = &m.kind else { return };
    let ink = (**ink).clone();
    crate::command_panel::rollout(ui, "Paint Controls", true, |ui| {
        ui.horizontal(|ui| {
            check(ui, app, "Lighted", "Lighted On", ink.lighted_on);
            swatch(ui, app, "Lighted", ink.lighted);
            map_button(ui, app, "Diffuse Color", m.map("Diffuse Color"), true);
        });
        let levels = ink.paint_levels as f32;
        spinner_with(ui, app, "Paint Levels", levels, 1.0..=255.0, 0, |v| Command::SetMaterialParameter { slot: None, name: "Paint Levels".into(), value: v });
        ui.horizontal(|ui| {
            check(ui, app, "Shaded", "Shaded On", ink.shaded_on);
            if ink.shaded_on {
                spinner_with(ui, app, "", ink.shaded_percent, 0.0..=100.0, 1, |v| Command::SetMaterialParameter { slot: None, name: "Shaded Percent".into(), value: v });
            } else {
                swatch(ui, app, "Shaded", ink.shaded);
            }
        });
        ui.horizontal(|ui| {
            check(ui, app, "Highlight", "Highlight On", ink.highlight_on);
            swatch(ui, app, "Highlight", ink.highlight);
        });
        spinner(ui, app, "Glossiness", ink.glossiness, 0.0..=100.0, 1);
    });
    crate::command_panel::rollout(ui, "Ink Controls", true, |ui| {
        ui.horizontal(|ui| {
            check(ui, app, "Ink", "Ink", ink.ink);
            let q = ink.ink_quality as f32;
            spinner_with(ui, app, "Ink Quality", q, 1.0..=3.0, 0, |v| Command::SetMaterialParameter { slot: None, name: "Ink Quality".into(), value: v });
        });
        ui.horizontal(|ui| {
            check(ui, app, "Variable Width", "Variable Width", ink.variable_width);
            if ink.variable_width {
                spinner(ui, app, "Ink Min", ink.ink_min, 0.0..=1000.0, 1);
                spinner(ui, app, "Ink Max", ink.ink_max, 0.0..=1000.0, 1);
            } else {
                spinner(ui, app, "Ink Width", ink.ink_width, 0.0..=1000.0, 1);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Ink Color:");
            swatch(ui, app, "Ink Color", ink.ink_color);
        });
        ui.horizontal_wrapped(|ui| {
            for (label, on) in [("Outline", ink.outline), ("Overlap", ink.overlap), ("Underlap", ink.underlap), ("SmGroup", ink.smgroup), ("Mat ID", ink.mat_id)] {
                check(ui, app, label, label, on);
            }
        });
    });
}

fn multi_sub_rollout(ui: &mut egui::Ui, app: &mut LoomApp, m: &StandardMaterial) {
    let MaterialKind::MultiSub(list) = &m.kind else { return };
    let list = list.clone();
    let ctx = ui.ctx().clone();
    crate::command_panel::rollout(ui, "Multi/Sub-Object Basic Parameters", true, |ui| {
        let sel_id = egui::Id::new("multi_sub_selected_row");
        let selected: usize = ctx.data(|d| d.get_temp(sel_id)).unwrap_or(0);
        ui.horizontal(|ui| {
            ui.label(format!("{}", list.len()));
            if ui.button("Set Number").clicked() {
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("set_number_dialog"), Some(list.len() as u32)));
            }
            if ui.button("Add").clicked() {
                let _ = app.dispatch(Command::MultiSubAdd);
            }
            if ui.add_enabled(selected < list.len(), egui::Button::new("Delete")).clicked() {
                let _ = app.dispatch(Command::MultiSubDelete { index: selected as u32 });
            }
        });
        if list.iter().any(|s| list.iter().filter(|o| o.id == s.id).count() > 1) {
            ui.colored_label(egui::Color32::YELLOW, "Duplicate IDs are used.");
        }
        egui::ScrollArea::vertical().id_salt("multi_sub_list").max_height(10.0 * 24.0).show(ui, |ui| {
            egui::Grid::new("multi_sub_rows").num_columns(6).spacing([6.0, 3.0]).show(ui, |ui| {
                // The ID / Name / Sub-Material headers sort the list.
                for (h, sort) in [("", None), ("ID", Some("ID")), ("Name", Some("Name")), ("Sub-Material", Some("Sub")), ("", None), ("On/Off", None)] {
                    match sort {
                        Some(by) => {
                            if ui.small_button(h).on_hover_text(format!("Sort by {h}")).clicked() {
                                let _ = app.dispatch(Command::MultiSubSort { by: by.into() });
                            }
                        }
                        None => {
                            ui.label(egui::RichText::new(h).small());
                        }
                    }
                }
                ui.end_row();
                for (i, s) in list.iter().enumerate() {
                    // The small sample sphere selects the row.
                    let sub = s.material.and_then(|id| app.state.scene.materials.get(&id).cloned()).unwrap_or_else(|| StandardMaterial::new(""));
                    let sample = material_sample(app, s.material, &sub);
                    let setup = PreviewSetup::from_options(&app.state.scene.material_editor.options, SampleType::Sphere, (0.0, 0.0));
                    let tex = preview_texture(&ctx, app, egui::Id::new(("multi_sub_preview", i)), &sample, 20, &setup);
                    let r = ui.add(egui::ImageButton::new(egui::load::SizedTexture::new(tex.id(), egui::vec2(20.0, 20.0))).selected(i == selected));
                    if r.clicked() {
                        ctx.data_mut(|d| d.insert_temp(sel_id, i));
                    }
                    let mut id = s.id as u32;
                    if ui.add(crate::spinner::spin(ui, &mut id, 1..=65535)).changed() {
                        let _ = app.dispatch(Command::MultiSubSetEntry { index: i as u32, id: Some(id), name: None, enabled: None, color: None });
                    }
                    let name_id = egui::Id::new(("multi_sub_name", i));
                    let mut name: String = ctx.data(|d| d.get_temp(name_id)).unwrap_or_else(|| s.name.clone());
                    let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(70.0));
                    if r.lost_focus() && name != s.name {
                        let _ = app.dispatch(Command::MultiSubSetEntry { index: i as u32, id: None, name: Some(name.clone()), enabled: None, color: None });
                    }
                    if r.has_focus() {
                        ctx.data_mut(|d| d.insert_temp(name_id, name));
                    } else {
                        ctx.data_mut(|d| d.remove::<String>(name_id));
                    }
                    if ui.add(egui::Button::new(material_label(app, s.material)).min_size(egui::vec2(130.0, 0.0))).clicked() {
                        let _ = app.dispatch(Command::MaterialEditorGoToSub { index: i as u32 });
                    }
                    let mut rgb = rgb_of(sub.diffuse.get());
                    if ui.color_edit_button_srgb(&mut rgb).changed() {
                        let _ = app.dispatch(Command::MultiSubSetEntry { index: i as u32, id: None, name: None, enabled: None, color: Some(rgb) });
                    }
                    let mut on = s.enabled;
                    if ui.checkbox(&mut on, "").changed() {
                        let _ = app.dispatch(Command::MultiSubSetEntry { index: i as u32, id: None, name: None, enabled: Some(on), color: None });
                    }
                    ui.end_row();
                }
            });
        });
    });
    // Set Number of Materials.
    let dlg = egui::Id::new("set_number_dialog");
    if let Some(mut n) = ctx.data(|d| d.get_temp::<Option<u32>>(dlg)).flatten() {
        let mut keep = true;
        egui::Window::new("Set Number of Materials").collapsible(false).resizable(false).show(&ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Number of Materials:");
                ui.add(crate::spinner::spin(ui, &mut n, 1..=1000));
            });
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    let _ = app.dispatch(Command::MultiSubSetNumber { number: n });
                    keep = false;
                }
                if ui.button("Cancel").clicked() {
                    keep = false;
                }
            });
        });
        ctx.data_mut(|d| d.insert_temp(dlg, keep.then_some(n)));
    }
}

/// Shell Material Parameters: Original / Baked Material buttons and which
/// of them viewports and renders show.
fn shell_rollout(ui: &mut egui::Ui, app: &mut LoomApp, m: &StandardMaterial) {
    let MaterialKind::Shell(sh) = &m.kind else { return };
    let sh = (**sh).clone();
    crate::command_panel::rollout(ui, "Shell Material Parameters", true, |ui| {
        egui::Grid::new("shell_rows").num_columns(4).show(ui, |ui| {
            ui.label("");
            ui.label("");
            ui.label("Viewport");
            ui.label("Render");
            ui.end_row();
            for (i, label) in ["Original Material:", "Baked Material:"].into_iter().enumerate() {
                ui.label(label);
                if ui.add(egui::Button::new(material_label(app, sh.materials[i])).min_size(egui::vec2(150.0, 0.0))).clicked() {
                    let _ = app.dispatch(Command::MaterialEditorGoToSub { index: i as u32 });
                }
                if ui.radio(sh.viewport == i as u8, "").on_hover_text(format!("Viewport: the {} material", if i == 0 { "original" } else { "baked" })).clicked() {
                    let _ = app.dispatch(Command::SetMaterialParameter { slot: None, name: "Viewport".into(), value: i as f64 });
                }
                if ui.radio(sh.render == i as u8, "").on_hover_text(format!("Render: the {} material", if i == 0 { "original" } else { "baked" })).clicked() {
                    let _ = app.dispatch(Command::SetMaterialParameter { slot: None, name: "Render".into(), value: i as f64 });
                }
                ui.end_row();
            }
        });
    });
}

fn blend_rollout(ui: &mut egui::Ui, app: &mut LoomApp, m: &StandardMaterial) {
    let MaterialKind::Blend(b) = &m.kind else { return };
    let b = (**b).clone();
    crate::command_panel::rollout(ui, "Blend Basic Parameters", true, |ui| {
        egui::Grid::new("blend_rows").num_columns(4).show(ui, |ui| {
            for i in 0..2 {
                ui.label(format!("Material {}:", i + 1));
                if ui.add(egui::Button::new(material_label(app, b.materials[i])).min_size(egui::vec2(150.0, 0.0))).clicked() {
                    let _ = app.dispatch(Command::MaterialEditorGoToSub { index: i as u32 });
                }
                let mut on = b.enabled[i];
                if ui.checkbox(&mut on, "").changed() {
                    let _ = app.dispatch(Command::SetMaterialParameter { slot: None, name: format!("Material {} On", i + 1), value: if on { 1.0 } else { 0.0 } });
                }
                if ui.radio(b.interactive == i as u8, "Interactive").clicked() {
                    let _ = app.dispatch(Command::SetMaterialParameter { slot: None, name: "Interactive".into(), value: i as f64 });
                }
                ui.end_row();
            }
            ui.label("Mask:");
            map_button(ui, app, "Mask", b.mask.as_ref(), false);
            let mut on = b.mask.as_ref().is_some_and(|m| m.enabled);
            if ui.add_enabled(b.mask.is_some(), egui::Checkbox::new(&mut on, "")).changed() {
                let _ = app.dispatch(Command::SetMaterialParameter { slot: None, name: "Mask On".into(), value: if on { 1.0 } else { 0.0 } });
            }
            if ui.radio(b.interactive == 2, "Interactive").clicked() {
                let _ = app.dispatch(Command::SetMaterialParameter { slot: None, name: "Interactive".into(), value: 2.0 });
            }
            ui.end_row();
        });
        let masked = b.mask.as_ref().is_some_and(|m| m.enabled);
        ui.add_enabled_ui(!masked, |ui| spinner(ui, app, "Mix Amount", b.mix_amount.get(), 0.0..=100.0, 1));
        ui.label(egui::RichText::new("Mixing curve").strong());
        ui.add_enabled_ui(masked, |ui| {
            check(ui, app, "Use Curve", "Use Curve", b.use_curve);
            ui.label("Transition zone:");
            spinner(ui, app, "Upper", b.upper, 0.0..=1.0, 2);
            spinner(ui, app, "Lower", b.lower, 0.0..=1.0, 2);
        });
    });
}

fn color_row(ui: &mut egui::Ui, app: &mut LoomApp, map: &Map, i: usize) {
    let slot = map.slot_names()[i].clone();
    ui.label(&slot);
    if map.slot_has_color(i) {
        let mut rgb = rgb_of(map.color(i));
        if ui.color_edit_button_srgb(&mut rgb).changed() {
            let _ = app.dispatch(Command::SetMapColor { which: slot.clone(), rgb });
        }
    } else {
        ui.label("");
    }
    if map.slot_has_map(i) {
        map_button(ui, app, &slot, map.sub_map(i), false);
        let mut on = map.sub_map(i).is_some_and(|s| s.enabled);
        if ui.add_enabled(map.sub_map(i).is_some(), egui::Checkbox::new(&mut on, "")).changed() {
            let _ = app.dispatch(Command::SetMapParameter { name: format!("{slot} Map On"), value: if on { 1.0 } else { 0.0 } });
        }
    }
    ui.end_row();
}

fn map_rollouts(ui: &mut egui::Ui, app: &mut LoomApp, map: &Map) {
    if map.kind.is_3d() {
        map_rollouts::coords3(ui, app, map);
    } else if map.kind.has_2d_coords() {
        crate::command_panel::rollout(ui, "Coordinates", true, |ui| {
            let c = &map.coords;
            ui.label("Texture: Explicit Map Channel");
            egui::Grid::new("map_coords").num_columns(5).show(ui, |ui| {
                for h in ["", "Offset", "Tiling", "Mirror", "Tile"] {
                    ui.label(h);
                }
                ui.end_row();
                for (axis, i) in [("U", 0usize), ("V", 1)] {
                    ui.label(axis);
                    map_spinner(ui, app, "", &format!("Offset {axis}"), c.offset[i], -1000.0..=1000.0, 3);
                    map_spinner(ui, app, "", &format!("Tiling {axis}"), c.tiling[i], -1000.0..=1000.0, 3);
                    map_check(ui, app, "", &format!("Mirror {axis}"), c.mirror[i]);
                    map_check(ui, app, "", &format!("Tile {axis}"), c.tile[i]);
                    ui.end_row();
                }
            });
            map_spinner(ui, app, "Angle W:", "Angle W", c.angle, -360.0..=360.0, 1);
            map_spinner(ui, app, "Blur:", "Blur", c.blur, 0.01..=100.0, 2);
            map_spinner(ui, app, "Map Channel:", "Map Channel", c.map_channel as f32, 1.0..=99.0, 0);
        });
    }
    match map.kind {
        MapType::Bitmap => crate::command_panel::rollout(ui, "Bitmap Parameters", true, |ui| {
            ui.horizontal(|ui| {
                ui.label("Bitmap:");
                let label = if map.path.is_empty() { "None".to_owned() } else { map.path.clone() };
                if ui.add(egui::Button::new(label).truncate()).clicked() {
                    app.state.dialog = Some(Dialog::MaterialBitmap);
                }
            });
            if ui.button("Reload").clicked() {
                let _ = app.dispatch(Command::ReloadBitmaps);
            }
            ui.label(egui::RichText::new("Cropping/Placement").strong());
            let k = &map.crop;
            ui.horizontal(|ui| {
                map_check(ui, app, "Apply", "Apply", k.apply);
                if ui.add_enabled(!map.path.is_empty(), egui::Button::new("View Image")).clicked() {
                    app.state.dialog = Some(Dialog::SpecifyCropping);
                }
            });
            ui.horizontal(|ui| {
                if ui.radio(!k.place, "Crop").clicked() {
                    let _ = app.dispatch(Command::SetMapParameter { name: "Crop".into(), value: 1.0 });
                }
                if ui.radio(k.place, "Place").clicked() {
                    let _ = app.dispatch(Command::SetMapParameter { name: "Place".into(), value: 1.0 });
                }
            });
            ui.horizontal(|ui| {
                map_spinner(ui, app, "U:", "U", k.u, 0.0..=1.0, 3);
                map_spinner(ui, app, "W:", "W", k.w, 0.0..=1.0, 3);
            });
            ui.horizontal(|ui| {
                map_spinner(ui, app, "V:", "V", k.v, 0.0..=1.0, 3);
                map_spinner(ui, app, "H:", "H", k.h, 0.0..=1.0, 3);
            });
        }),
        MapType::Checker => crate::command_panel::rollout(ui, "Checker Parameters", true, |ui| {
            map_spinner(ui, app, "Soften:", "Soften", map.params.soften, 0.0..=1.0, 3);
            if ui.button("Swap").clicked() {
                let _ = app.dispatch(Command::SwapMapColors);
            }
            egui::Grid::new("checker_colors").num_columns(4).show(ui, |ui| {
                for i in 0..2 {
                    color_row(ui, app, map, i);
                }
            });
        }),
        MapType::Gradient => crate::command_panel::rollout(ui, "Gradient Parameters", true, |ui| {
            egui::Grid::new("gradient_colors").num_columns(4).show(ui, |ui| {
                for i in 0..3 {
                    color_row(ui, app, map, i);
                }
            });
            map_spinner(ui, app, "Color 2 Position:", "Color 2 Position", map.params.position, 0.0..=1.0, 2);
            ui.horizontal(|ui| {
                ui.label("Gradient Type:");
                if ui.radio(!map.params.radial, "Linear").clicked() {
                    let _ = app.dispatch(Command::SetMapParameter { name: "Radial".into(), value: 0.0 });
                }
                if ui.radio(map.params.radial, "Radial").clicked() {
                    let _ = app.dispatch(Command::SetMapParameter { name: "Radial".into(), value: 1.0 });
                }
            });
        }),
        MapType::Noise => crate::command_panel::rollout(ui, "Noise Parameters", true, |ui| {
            let p = &map.params;
            ui.horizontal(|ui| {
                ui.label("Noise Type:");
                for (i, label) in ["Regular", "Fractal", "Turbulence"].iter().enumerate() {
                    if ui.radio(p.noise_type == i as u32, *label).clicked() {
                        let _ = app.dispatch(Command::SetMapParameter { name: "Noise Type".into(), value: i as f64 });
                    }
                }
            });
            ui.horizontal(|ui| {
                map_spinner(ui, app, "High:", "High", p.high, 0.0..=1.0, 3);
                map_spinner(ui, app, "Low:", "Low", p.low, 0.0..=1.0, 3);
            });
            ui.horizontal(|ui| {
                map_spinner(ui, app, "Size:", "Size", p.size, 0.001..=1_000_000.0, 1);
                map_spinner(ui, app, "Phase:", "Phase", p.phase, -1000.0..=1000.0, 2);
            });
            ui.add_enabled_ui(p.noise_type != 0, |ui| map_spinner(ui, app, "Levels:", "Levels", p.levels, 1.0..=10.0, 1));
            if ui.button("Swap").clicked() {
                let _ = app.dispatch(Command::SwapMapColors);
            }
            egui::Grid::new("noise_colors").num_columns(4).show(ui, |ui| {
                for i in 0..2 {
                    color_row(ui, app, map, i);
                }
            });
        }),
        MapType::Mix => crate::command_panel::rollout(ui, "Mix Parameters", true, |ui| {
            if ui.button("Swap").clicked() {
                let _ = app.dispatch(Command::SwapMapColors);
            }
            egui::Grid::new("mix_colors").num_columns(4).show(ui, |ui| {
                for i in 0..3 {
                    color_row(ui, app, map, i);
                }
            });
            let masked = map.sub_maps[2].as_ref().is_some_and(|m| m.enabled);
            ui.add_enabled_ui(!masked, |ui| map_spinner(ui, app, "Mix Amount:", "Mix Amount", map.params.mix_amount, 0.0..=100.0, 1));
            map_rollouts::mixing_curve(ui, app, map);
        }),
        MapType::NormalBump => crate::command_panel::rollout(ui, "Parameters", true, |ui| {
            ui.horizontal(|ui| {
                ui.label("Normal:");
                map_button(ui, app, "Normal", map.sub_maps[0].as_deref(), false);
                let mut on = map.sub_maps[0].as_ref().is_some_and(|s| s.enabled);
                if ui.add_enabled(map.sub_maps[0].is_some(), egui::Checkbox::new(&mut on, "")).changed() {
                    let _ = app.dispatch(Command::SetMapParameter { name: "Normal Map On".into(), value: if on { 1.0 } else { 0.0 } });
                }
            });
            ui.label(egui::RichText::new("Channel Direction").strong());
            ui.horizontal(|ui| {
                map_check(ui, app, "Flip Red (X)", "Flip Red (X)", map.params.flip_red);
                map_check(ui, app, "Flip Green (Y)", "Flip Green (Y)", map.params.flip_green);
            });
        }),
        _ => map_rollouts::parameters(ui, app, map),
    }
    if map.kind.has_output() {
        map_rollouts::output(ui, app, map);
    }
}

// ---------------------------------------------------------------------------
// Dialogs
// ---------------------------------------------------------------------------

/// Material/Map Browser: Browse From Mtl Editor / Active Slot / Selected /
/// Scene / New; double-click or OK picks.
pub fn browser(ctx: &egui::Context, app: &mut LoomApp, for_: BrowserFor, open: &mut bool) {
    let from_id = egui::Id::new(("material_browser_from", format!("{for_:?}")));
    // 0 Mtl Editor, 1 Active Slot, 2 Selected, 3 Scene, 4 New.
    let default_from = if for_ == BrowserFor::GetMaterial { 3 } else { 4 };
    let mut from: u8 = ctx.data(|d| d.get_temp(from_id)).unwrap_or(default_from);
    let sel_id = egui::Id::new("material_browser_selected");
    let mut selected: Option<String> = ctx.data(|d| d.get_temp(sel_id)).flatten();
    let wants_map = matches!(for_, BrowserFor::Map(_) | BrowserFor::MapType);
    let mut picked: Option<String> = None;
    let mut cancel = false;
    let icons_id = egui::Id::new("material_browser_icons");
    let mut icons: bool = ctx.data(|d| d.get_temp(icons_id)).unwrap_or(false);
    egui::Window::new("Material/Map Browser").open(open).collapsible(false).resizable(false).default_width(360.0).show(ctx, |ui| {
        ui.label(egui::RichText::new("Browse From:").strong());
        ui.horizontal_wrapped(|ui| {
            // 5 Mtl Library comes first, as in the reference.
            for (i, label) in [(5u8, "Mtl Library"), (0, "Mtl Editor"), (1, "Active Slot"), (2, "Selected"), (3, "Scene"), (4, "New")] {
                // Maps and material types come only from New; Get Material only from existing materials.
                let enabled = if for_ == BrowserFor::GetMaterial { i != 4 } else { i == 4 };
                if ui.add_enabled(enabled, egui::RadioButton::new(from == i, label)).clicked() {
                    from = i;
                    selected = None;
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Show:").strong());
            let (mut materials, mut maps) = (!wants_map, wants_map);
            ui.add_enabled(false, egui::Checkbox::new(&mut materials, "Materials"));
            ui.add_enabled(false, egui::Checkbox::new(&mut maps, "Maps"));
            ui.separator();
            ui.label(egui::RichText::new("View:").strong());
            ui.radio_value(&mut icons, false, "List");
            ui.radio_value(&mut icons, true, "List + Icons");
        });
        if from == 5 {
            library_file_row(ui, app, selected.as_deref());
        }
        ui.separator();
        let entries: Vec<(String, String)> = if from == 4 {
            if wants_map {
                MapType::ALL.iter().map(|t| (t.label().to_owned(), t.label().to_owned())).collect()
            } else {
                MaterialKind::TYPES.iter().map(|t| (t.to_string(), t.to_string())).collect()
            }
        } else if from == 5 {
            app.state.material_library.entries.iter().map(|e| (format!("lib:{}", e.name), format!("{} [{}]", e.name, e.material().map_or("?", |m| m.kind.label())))).collect()
        } else {
            browse_materials(app, from)
        };
        egui::ScrollArea::vertical().id_salt("material_browser_list").max_height(260.0).auto_shrink([false, true]).show(ui, |ui| {
            if entries.is_empty() {
                ui.weak("(none)");
            }
            for (key, label) in &entries {
                let r = ui
                    .horizontal(|ui| {
                        if icons && from != 4 {
                            if let Some(sample) = browser_sample(app, key) {
                                let setup = PreviewSetup::from_options(&app.state.scene.material_editor.options, SampleType::Sphere, (0.0, 0.0));
                                let tex = preview_texture(ui.ctx(), app, egui::Id::new(("material_browser_icon", key.clone())), &sample, 24, &setup);
                                ui.image(egui::load::SizedTexture::new(tex.id(), egui::vec2(24.0, 24.0)));
                            }
                        }
                        ui.selectable_label(selected.as_deref() == Some(key.as_str()), label)
                    })
                    .inner;
                if r.clicked() {
                    selected = Some(key.clone());
                }
                if r.double_clicked() {
                    picked = Some(key.clone());
                }
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.add_enabled(selected.is_some(), egui::Button::new("OK")).clicked() {
                picked = selected.clone();
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
    });
    ctx.data_mut(|d| {
        d.insert_temp(from_id, from);
        d.insert_temp(sel_id, selected.clone());
        d.insert_temp(icons_id, icons);
    });
    if cancel {
        *open = false;
    }
    let Some(choice) = picked else { return };
    ctx.data_mut(|d| d.insert_temp(sel_id, None::<String>));
    *open = false;
    match for_ {
        BrowserFor::MaterialType => {
            let kind = MaterialKind::TYPES.iter().position(|t| *t == choice).unwrap_or(0) as u8;
            // Only a compound type can keep the old material as a sub-material.
            if matches!(choice.as_str(), "Standard" | "Raytrace" | "Architectural" | "Matte/Shadow" | "Ink 'n Paint") {
                let _ = app.dispatch(Command::SetMaterialType { kind: choice, keep_old: None });
                app.state.dialog = None;
            } else {
                // Replace Material: discard the old material or keep it as a sub-material.
                *open = true;
                app.state.dialog = Some(Dialog::ReplaceMaterial { kind, map: false });
            }
        }
        BrowserFor::MapType => {
            let ty = MapType::from_label(&choice).unwrap_or_default();
            if ty.color_slots().is_empty() {
                let _ = app.dispatch(Command::SetMapType { kind: choice, keep_old: None });
                app.state.dialog = if ty == MapType::Bitmap { *open = true; Some(Dialog::MaterialBitmap) } else { None };
            } else {
                *open = true;
                app.state.dialog = Some(Dialog::ReplaceMaterial { kind: MapType::ALL.iter().position(|t| *t == ty).unwrap_or(0) as u8, map: true });
            }
        }
        BrowserFor::Map(i) => {
            let slot = loom_scene::map_params::slot_name(i);
            let ok = app.dispatch(Command::NewMap { map: slot, kind: choice.clone() }).is_ok();
            // A new Bitmap asks for its image file next, as in the reference.
            if ok && choice == "Bitmap" {
                *open = true;
                app.state.dialog = Some(Dialog::MaterialBitmap);
            } else {
                app.state.dialog = None;
            }
        }
        BrowserFor::GetMaterial => {
            if let Some(name) = choice.strip_prefix("lib:") {
                let _ = app.dispatch(Command::GetFromLibrary { name: name.into() });
            } else if let Ok(id) = choice.parse::<u32>() {
                let _ = app.dispatch(Command::GetMaterial { material: id });
            }
            app.state.dialog = None;
        }
    }
}

/// The sample a Browser row shows (List + Icons): a scene or library material.
fn browser_sample(app: &LoomApp, key: &str) -> Option<Sample> {
    if let Some(name) = key.strip_prefix("lib:") {
        let e = app.state.material_library.entries.iter().find(|e| e.name == name)?;
        let s = loom_scene::material::surfaces(&e.materials, e.root)?;
        return Some(Sample::Surfaces(s));
    }
    let id = MaterialId(key.parse().ok()?);
    Some(Sample::Surfaces(loom_scene::material::surfaces(&app.state.scene.materials, id)?))
}

/// Mtl Library > File: Open, Merge, Save, Save As, New, and Delete for the selected material.
fn library_file_row(ui: &mut egui::Ui, app: &mut LoomApp, selected: Option<&str>) {
    let lib = &app.state.material_library;
    let title = match &lib.path {
        Some(p) => format!("{}{}", std::path::Path::new(p).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default(), if lib.dirty { " *" } else { "" }),
        None => format!("(unsaved library){}", if lib.dirty { " *" } else { "" }),
    };
    let has_path = lib.path.is_some();
    ui.label(egui::RichText::new(title).italics());
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("File:").strong());
        if ui.button("Open...").clicked() {
            app.state.dialog = Some(Dialog::OpenMaterialLibrary);
        }
        if ui.button("Merge...").clicked() {
            app.state.dialog = Some(Dialog::MergeMaterialLibrary);
        }
        if ui.add_enabled(has_path, egui::Button::new("Save")).clicked() {
            let _ = app.dispatch(Command::SaveMaterialLibrary { path: None });
        }
        if ui.button("Save As...").clicked() {
            app.state.dialog = Some(Dialog::SaveMaterialLibrary);
        }
        if ui.button("New").clicked() {
            let _ = app.dispatch(Command::NewMaterialLibrary);
        }
        let name = selected.and_then(|s| s.strip_prefix("lib:"));
        if ui.add_enabled(name.is_some(), egui::Button::new("Delete from Library")).clicked() {
            if let Some(n) = name {
                let _ = app.dispatch(Command::DeleteFromLibrary { name: n.into() });
            }
        }
    });
}

/// Materials to browse: (id, label) from the editor's slots, the active
/// slot, the selected objects or the whole scene.
fn browse_materials(app: &LoomApp, from: u8) -> Vec<(String, String)> {
    let lib = &app.state.scene.materials;
    let ed = &app.state.scene.material_editor;
    let ids: Vec<MaterialId> = match from {
        0 => ed.slots.iter().flatten().copied().collect(),
        1 => ed.slots[ed.active].into_iter().collect(),
        2 => app.state.selection.iter().filter_map(|n| app.state.scene.node(*n)?.material).collect(),
        _ => {
            // Scene: what objects use, with their sub-materials.
            let mut out = Vec::new();
            let mut stack: Vec<MaterialId> = app.state.scene.nodes().filter_map(|n| n.material).collect();
            while let Some(id) = stack.pop() {
                if out.contains(&id) {
                    continue;
                }
                out.push(id);
                if let Some(m) = lib.get(&id) {
                    stack.extend(m.children().into_iter().flatten());
                }
            }
            out.sort();
            out
        }
    };
    let mut seen = std::collections::BTreeSet::new();
    ids.into_iter().filter(|id| seen.insert(*id)).filter_map(|id| lib.get(&id).map(|m| (id.0.to_string(), format!("{} [{}]", m.name, m.kind.label())))).collect()
}

/// Replace Material / Replace Map: discard the old one or keep it as a sub-material / sub-map.
pub fn replace_material(ctx: &egui::Context, app: &mut LoomApp, kind: u8, map: bool, open: &mut bool) {
    let id = egui::Id::new("replace_material_keep");
    let mut keep: bool = ctx.data(|d| d.get_temp(id)).unwrap_or(false);
    let mut done = None;
    let title = if map { "Replace Map" } else { "Replace Material" };
    egui::Window::new(title).open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        let (discard, keep_label) = if map { ("Discard old map?", "Keep old map as sub-map?") } else { ("Discard old material?", "Keep old material as sub-material?") };
        ui.radio_value(&mut keep, false, discard);
        ui.radio_value(&mut keep, true, keep_label);
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                done = Some(true);
            }
            if ui.button("Cancel").clicked() {
                done = Some(false);
            }
        });
    });
    ctx.data_mut(|d| d.insert_temp(id, keep));
    match done {
        Some(true) => {
            let _ = if map {
                app.dispatch(Command::SetMapType { kind: MapType::ALL.get(kind as usize).copied().unwrap_or_default().label().into(), keep_old: Some(keep) })
            } else {
                app.dispatch(Command::SetMaterialType { kind: MaterialKind::TYPES.get(kind as usize).copied().unwrap_or("Standard").into(), keep_old: Some(keep) })
            };
            *open = false;
        }
        Some(false) => *open = false,
        None => {}
    }
}

/// Material Editor Options: lights, background, slots; Apply / OK / Cancel.
pub fn options(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("material_editor_options_edit");
    let current = app.state.scene.material_editor.options.clone();
    let (start, mut o): (loom_scene::material::EditorOptions, loom_scene::material::EditorOptions) = ctx.data(|d| d.get_temp(id)).unwrap_or((current.clone(), current.clone()));
    let mut action = None;
    egui::Window::new("Material Editor Options").open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.checkbox(&mut o.antialias, "Antialias");
        ui.checkbox(&mut o.simple_multi, "Simple Multi Display Below Top Level");
        ui.checkbox(&mut o.maps_as_2d, "Display Maps as 2D");
        egui::Grid::new("mtl_options_lights").num_columns(2).show(ui, |ui| {
            ui.label("Top Light Multiplier:");
            ui.add(crate::spinner::spin(ui, &mut o.top_multiplier, 0.0..=10.0).speed(0.05).fixed_decimals(2));
            ui.end_row();
            ui.label("Back Light Multiplier:");
            ui.add(crate::spinner::spin(ui, &mut o.back_multiplier, 0.0..=10.0).speed(0.05).fixed_decimals(2));
            ui.end_row();
            ui.label("Background Intensity:");
            ui.add(crate::spinner::spin(ui, &mut o.background_intensity, 0.0..=1.0).speed(0.01).fixed_decimals(2));
            ui.end_row();
        });
        ui.label(egui::RichText::new("Slots").strong());
        ui.horizontal(|ui| {
            ui.radio_value(&mut o.slots_shown, 6, "3 X 2");
            ui.radio_value(&mut o.slots_shown, 15, "5 X 3");
            ui.radio_value(&mut o.slots_shown, 24, "6 X 4");
        });
        ui.horizontal(|ui| {
            if ui.button("Apply").clicked() {
                action = Some(0);
            }
            if ui.button("OK").clicked() {
                action = Some(1);
            }
            if ui.button("Cancel").clicked() {
                action = Some(2);
            }
        });
    });
    let apply = |app: &mut LoomApp, o: &loom_scene::material::EditorOptions, slots: bool| {
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        let mut list = vec![
            ("Antialias", b(o.antialias)),
            ("Simple Multi Display Below Top Level", b(o.simple_multi)),
            ("Display Maps as 2D", b(o.maps_as_2d)),
            ("Top Light Multiplier", o.top_multiplier as f64),
            ("Back Light Multiplier", o.back_multiplier as f64),
            ("Background Intensity", o.background_intensity as f64),
        ];
        if slots {
            list.push(("Slots", o.slots_shown as f64));
        }
        for (name, value) in list {
            let _ = app.dispatch(Command::SetMaterialEditorOption { name: name.into(), value });
        }
    };
    match action {
        // Apply: everything but the Slots group.
        Some(0) => apply(app, &o, false),
        Some(1) => {
            apply(app, &o, true);
            *open = false;
        }
        Some(2) => {
            apply(app, &start, true);
            *open = false;
        }
        _ => {}
    }
    if *open {
        ctx.data_mut(|d| d.insert_temp(id, (start, o)));
    } else {
        ctx.data_mut(|d| d.remove::<(loom_scene::material::EditorOptions, loom_scene::material::EditorOptions)>(id));
    }
}

/// Specify Cropping/Placement: the bitmap with the region outline; U/V/W/H
/// spinners; drag inside the region to move it, its corners to size it.
pub fn specify_cropping(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let (_, m, maps) = current_level(app);
    let Some(map) = map_at(&m, &maps).filter(|x| x.kind == MapType::Bitmap).cloned() else {
        *open = false;
        return;
    };
    let tex_id = egui::Id::new(("cropping_image", map.path.clone(), app.state.bitmap_generation));
    let tex: Option<egui::TextureHandle> = match ctx.data(|d| d.get_temp::<Option<egui::TextureHandle>>(tex_id)) {
        Some(t) => t,
        None => {
            let images = app.map_images(&map);
            let t = images.get(&map.path).cloned().flatten().map(|img| ctx.load_texture(format!("crop {}", map.path), egui::ColorImage::from_rgba_unmultiplied([img.width as usize, img.height as usize], &img.rgba), egui::TextureOptions::LINEAR));
            ctx.data_mut(|d| d.insert_temp(tex_id, t.clone()));
            t
        }
    };
    egui::Window::new("Specify Cropping/Placement").open(open).resizable(true).default_size([420.0, 380.0]).show(ctx, |ui| {
        let k = map.crop.clone();
        ui.horizontal(|ui| {
            map_spinner(ui, app, "U:", "U", k.u, 0.0..=1.0, 3);
            map_spinner(ui, app, "V:", "V", k.v, 0.0..=1.0, 3);
            map_spinner(ui, app, "W:", "W", k.w, 0.0..=1.0, 3);
            map_spinner(ui, app, "H:", "H", k.h, 0.0..=1.0, 3);
        });
        let Some(tex) = tex else {
            ui.label(format!("could not load {}", map.path));
            return;
        };
        let avail = ui.available_size();
        let [w, h] = tex.size();
        let scale = (avail.x / w as f32).min((avail.y - 4.0).max(64.0) / h as f32).min(4.0);
        let (rect, r) = ui.allocate_exact_size(egui::vec2(w as f32 * scale, h as f32 * scale), egui::Sense::drag());
        ui.painter().image(tex.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        let region = egui::Rect::from_min_size(rect.min + egui::vec2(k.u * rect.width(), k.v * rect.height()), egui::vec2(k.w * rect.width(), k.h * rect.height()));
        ui.painter().rect_stroke(region, 0.0, egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(255, 220, 0)), egui::StrokeKind::Middle);
        for c in [region.left_top(), region.right_top(), region.left_bottom(), region.right_bottom()] {
            ui.painter().rect_filled(egui::Rect::from_center_size(c, egui::vec2(7.0, 7.0)), 0.0, egui::Color32::from_rgb(255, 220, 0));
        }
        let drag_id = egui::Id::new("cropping_drag");
        if r.drag_started() {
            let p = r.interact_pointer_pos().unwrap_or(rect.center());
            // Which handle: a corner sizes, inside moves.
            let corner = [region.right_bottom(), region.left_top(), region.right_top(), region.left_bottom()].iter().position(|c| c.distance(p) < 10.0);
            ctx.data_mut(|d| d.insert_temp(drag_id, corner.map(|c| c as i32).unwrap_or(-1)));
            app.state.undo.begin("Cropping");
        }
        if r.dragged() {
            let d = r.drag_delta();
            let (du, dv) = (d.x / rect.width(), d.y / rect.height());
            let mode: i32 = ctx.data(|m| m.get_temp(drag_id)).unwrap_or(-1);
            let set = |app: &mut LoomApp, name: &str, v: f32| {
                let _ = app.dispatch(Command::SetMapParameter { name: name.into(), value: v as f64 });
            };
            match mode {
                -1 => {
                    set(app, "U", (k.u + du).clamp(0.0, 1.0 - k.w));
                    set(app, "V", (k.v + dv).clamp(0.0, 1.0 - k.h));
                }
                0 => {
                    set(app, "W", (k.w + du).clamp(0.01, 1.0 - k.u));
                    set(app, "H", (k.h + dv).clamp(0.01, 1.0 - k.v));
                }
                1 => {
                    set(app, "U", (k.u + du).clamp(0.0, k.u + k.w - 0.01));
                    set(app, "W", (k.w - du).clamp(0.01, 1.0));
                    set(app, "V", (k.v + dv).clamp(0.0, k.v + k.h - 0.01));
                    set(app, "H", (k.h - dv).clamp(0.01, 1.0));
                }
                2 => {
                    set(app, "W", (k.w + du).clamp(0.01, 1.0 - k.u));
                    set(app, "V", (k.v + dv).clamp(0.0, k.v + k.h - 0.01));
                    set(app, "H", (k.h - dv).clamp(0.01, 1.0));
                }
                _ => {
                    set(app, "U", (k.u + du).clamp(0.0, k.u + k.w - 0.01));
                    set(app, "W", (k.w - du).clamp(0.01, 1.0));
                    set(app, "H", (k.h + dv).clamp(0.01, 1.0 - k.v));
                }
            }
        }
        if r.drag_stopped() {
            app.state.undo.end();
        }
    });
}

/// Copy (Instance) Map: a map button dropped onto another.
pub fn copy_map(ctx: &egui::Context, app: &mut LoomApp, from: u8, to: u8, open: &mut bool) {
    let id = egui::Id::new("copy_map_method");
    let mut swap: bool = ctx.data(|d| d.get_temp(id)).unwrap_or(false);
    let mut done = None;
    egui::Window::new("Copy (Instance) Map").open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.label("Method");
        ui.add_enabled(false, egui::RadioButton::new(false, "Instance"));
        ui.radio_value(&mut swap, false, "Copy");
        ui.radio_value(&mut swap, true, "Swap");
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                done = Some(true);
            }
            if ui.button("Cancel").clicked() {
                done = Some(false);
            }
        });
    });
    ctx.data_mut(|d| d.insert_temp(id, swap));
    match done {
        Some(true) => {
            let name = loom_scene::map_params::slot_name;
            let _ = app.dispatch(Command::CopyMap { from: name(from), to: name(to), swap });
            *open = false;
        }
        Some(false) => *open = false,
        None => {}
    }
}

/// Material/Map Navigator: the active slot's material tree; a click goes to that level.
pub fn navigator(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let ed = &app.state.scene.material_editor;
    let lib = &app.state.scene.materials;
    let mut rows: Vec<(usize, String, Vec<String>)> = Vec::new();
    match ed.slots[ed.active].filter(|t| lib.contains_key(t)) {
        None => rows.push((0, format!("{} (Standard)", app.state.scene.slot_material(ed.active).name), Vec::new())),
        Some(top) => {
            fn walk_map(map: &Map, slot: &str, depth: usize, path: Vec<String>, rows: &mut Vec<(usize, String, Vec<String>)>) {
                rows.push((depth, format!("{slot}: {} ({})", if map.name.is_empty() { "Map" } else { &map.name }, map.kind.label()), path.clone()));
                for (i, s) in map.slot_names().iter().enumerate() {
                    if let Some(sub) = map.sub_map(i) {
                        let mut p = path.clone();
                        p.push(s.clone());
                        walk_map(sub, s, depth + 1, p, rows);
                    }
                }
            }
            fn walk(lib: &loom_scene::MaterialLibrary, id: MaterialId, depth: usize, path: Vec<String>, rows: &mut Vec<(usize, String, Vec<String>)>) {
                let Some(m) = lib.get(&id).filter(|_| depth < 16) else { return };
                rows.push((depth, format!("{} ({})", m.name, m.kind.label()), path.clone()));
                for (i, c) in m.children().iter().enumerate() {
                    if let Some(c) = c {
                        let mut p = path.clone();
                        p.push(i.to_string());
                        walk(lib, *c, depth + 1, p, rows);
                    }
                }
                for s in m.map_slots() {
                    if let Some(map) = m.map(s) {
                        let mut p = path.clone();
                        p.push(s.to_owned());
                        walk_map(map, s, depth + 1, p, rows);
                    }
                }
            }
            walk(lib, top, 0, Vec::new(), &mut rows);
        }
    }
    let current: Vec<String> = ed.path.iter().map(|s| match s {
        NavStep::Sub(i) => i.to_string(),
        NavStep::Map(m) => m.clone(),
    }).collect();
    let title = app.state.scene.slot_material(ed.active).name;
    egui::Window::new("Material/Map Navigator").id(egui::Id::new("Material/Map Navigator")).open(open).resizable(true).default_width(280.0).show(ctx, |ui| {
        ui.label(egui::RichText::new(title).strong());
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            for (depth, label, path) in rows {
                ui.horizontal(|ui| {
                    ui.add_space(depth as f32 * 14.0);
                    if ui.selectable_label(path == current, label).clicked() {
                        let _ = app.dispatch(Command::MaterialEditorNavigate { path });
                    }
                });
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(kind: SampleType) -> PreviewSetup {
        PreviewSetup::from_options(&loom_scene::material::EditorOptions::default(), kind, (0.0, 0.0))
    }

    #[test]
    fn preview_shows_the_diffuse_colour_and_the_highlight() {
        let size = 56;
        let mut m = StandardMaterial::slot_default(0);
        m.diffuse.set(glam::Vec3::new(1.0, 0.0, 0.0));
        m.ambient.set(glam::Vec3::new(1.0, 0.0, 0.0));
        let images = loom_scene::maps::Images::new();
        let sample = |m: &StandardMaterial| Sample::Surfaces(Surfaces::Single(Box::new(Surface::from_standard(m))));
        let dull = preview_pixels(&sample(&m), size, &setup(SampleType::Sphere), &images);
        let mid = dull[size * (size / 2) + size / 2];
        assert!(mid.r() > 100 && mid.g() < 40, "the sphere is red: {mid:?}");
        m.specular_level.set(100.0);
        m.glossiness.set(30.0);
        let shiny = preview_pixels(&sample(&m), size, &setup(SampleType::Sphere), &images);
        let brightest = |p: &[egui::Color32]| p.iter().map(|c| c.g() as u32).max().unwrap();
        assert!(brightest(&shiny) > brightest(&dull) + 40, "a highlight appears");
    }

    #[test]
    fn each_shader_shades_the_sample_its_own_way() {
        use loom_scene::material::Shader;
        let size = 64;
        let images = loom_scene::maps::Images::new();
        // The top light only: one highlight.
        let mut lights = setup(SampleType::Sphere);
        lights.back = None;
        let px = |m: &StandardMaterial| preview_pixels(&Sample::Surfaces(Surfaces::Single(Box::new(Surface::from_standard(m)))), size, &lights, &images);
        let mut m = StandardMaterial::slot_default(0);
        m.diffuse.set(glam::Vec3::new(0.8, 0.0, 0.0));
        m.ambient.set(glam::Vec3::new(0.8, 0.0, 0.0));
        m.specular_level.set(150.0);
        m.glossiness.set(40.0);
        // Over the sphere only (red, or a highlight), not the grey background.
        let brightest_g = |p: &[egui::Color32]| p.iter().filter(|c| c.r() as i32 > c.g() as i32 + 40 || c.g() > 200).map(|c| c.g()).max().unwrap_or(0);
        assert!(brightest_g(&px(&m)) > 200, "Blinn: a white highlight on red");
        m.shading.shader = Shader::Metal;
        assert!(brightest_g(&px(&m)) < 40, "Metal: the highlight is the diffuse colour");
        // Anisotropic: the highlight is long one way; turning it 90° swaps its extent.
        let extent = |p: &[egui::Color32]| {
            let hot: Vec<(usize, usize)> = (0..size * size).filter(|i| p[*i].g() > 120).map(|i| (i % size, i / size)).collect();
            let w = hot.iter().map(|h| h.0).max().unwrap() - hot.iter().map(|h| h.0).min().unwrap();
            let h = hot.iter().map(|h| h.1).max().unwrap() - hot.iter().map(|h| h.1).min().unwrap();
            (w as f32 + 1.0, h as f32 + 1.0)
        };
        m.shading.shader = Shader::Anisotropic;
        m.shading.anisotropy = 90.0;
        let (w0, h0) = extent(&px(&m));
        m.shading.orientation = 90.0;
        let (w1, h1) = extent(&px(&m));
        assert!((w0 / h0 - w1 / h1).abs() > 0.5, "orientation turns the highlight: {w0}x{h0} vs {w1}x{h1}");
        // Oren-Nayar-Blinn Roughness darkens.
        let sum_r = |p: &[egui::Color32]| p.iter().map(|c| c.r() as u32).sum::<u32>();
        m.specular_level.set(0.0);
        m.shading.shader = Shader::OrenNayarBlinn;
        let smooth = sum_r(&px(&m));
        m.shading.roughness = 100.0;
        assert!(sum_r(&px(&m)) < smooth, "rough is darker");
        // Translucent: light from behind comes through in the Translucent colour.
        m.shading.shader = Shader::Translucent;
        m.diffuse.set(glam::Vec3::ZERO);
        m.ambient.set(glam::Vec3::ZERO);
        let sum_g = |p: &[egui::Color32]| p.iter().map(|c| c.g() as u32).sum::<u32>();
        let dark = sum_g(&px(&m));
        m.shading.translucent_color = glam::Vec3::Y;
        m.extended.filter_color = glam::Vec3::ONE;
        assert!(sum_g(&px(&m)) > dark + 500, "translucent green");
    }

    #[test]
    fn opacity_falloff_in_clears_the_facing_middle() {
        let size = 32;
        let images = loom_scene::maps::Images::new();
        let mut m = StandardMaterial::slot_default(0);
        m.diffuse.set(glam::Vec3::ONE);
        m.ambient.set(glam::Vec3::ONE);
        m.opacity.set(60.0);
        let px = |m: &StandardMaterial| preview_pixels(&Sample::Surfaces(Surfaces::Single(Box::new(Surface::from_standard(m)))), size, &setup(SampleType::Sphere), &images);
        let bg = px(&m)[0];
        let before = px(&m)[size * (size / 2) + size / 2];
        m.extended.falloff_amount = 100.0;
        let after = px(&m)[size * (size / 2) + size / 2];
        assert!((after.r() as i32 - bg.r() as i32).abs() < (before.r() as i32 - bg.r() as i32).abs(), "the middle shows more background: {before:?} {after:?} {bg:?}");
    }

    #[test]
    fn sample_types_have_their_silhouettes() {
        let size = 40;
        let m = StandardMaterial::slot_default(0);
        let images = loom_scene::maps::Images::new();
        let s = Sample::Surfaces(Surfaces::Single(Box::new(Surface::from_standard(&m))));
        let bg = preview_pixels(&s, size, &setup(SampleType::Sphere), &images)[0];
        let covered = |kind| preview_pixels(&s, size, &setup(kind), &images).iter().filter(|c| **c != bg).count();
        let (sphere, cube, cyl) = (covered(SampleType::Sphere), covered(SampleType::Cube), covered(SampleType::Cylinder));
        assert!(sphere > 0 && cube > 0 && cyl > 0);
        // The sphere's corner pixel near (0.8, 0.8) is background; the cube's front face reaches it at 0.55.
        let at = |kind, x: usize, y: usize| preview_pixels(&s, size, &setup(kind), &images)[y * size + x];
        assert_eq!(at(SampleType::Sphere, 33, 6), bg);
        assert_ne!(at(SampleType::Cube, 30, 9), bg);
        assert_ne!(sphere, cube);
    }

    #[test]
    fn a_map_level_shows_the_map_flat() {
        let images = loom_scene::maps::Images::new();
        let checker = Map::new(MapType::Checker, 100.0);
        let px = preview_pixels(&Sample::Map(Box::new(checker)), 8, &setup(SampleType::Sphere), &images);
        // Bottom-left check black, bottom-right white.
        assert_eq!(px[7 * 8], egui::Color32::from_rgb(0, 0, 0));
        assert_eq!(px[7 * 8 + 7], egui::Color32::from_rgb(255, 255, 255));
    }
}
