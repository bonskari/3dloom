//! Rendering > Environment and Effects (8): background, global lighting,
//! exposure control, the atmosphere list and the rendering effects list.
//! Every edit is one undo step (`Change::SetEnvironment`); the values are
//! saved with the scene and the Bevy renderer applies them.

use crate::command::Command as C;
use crate::state::RenderRequest;
use crate::AppState;
use glam::Vec3;
use loom_scene::environment::{Atmosphere, AtmosphereKind, EffectKind, Environment, ExposureKind, RenderEffect};
use loom_scene::{Change, NodeId, ObjectKind};
use serde_json::{json, Value};
use std::sync::Arc;

/// Exposure Control > Render Preview thumbnail size.
pub const PREVIEW_SIZE: [u32; 2] = [160, 120];

/// The dialog's panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvironmentTab {
    Environment,
    Effects,
}

/// The Environment and Effects dialog: which panel is open (None: closed)
/// and the Exposure Control preview thumbnail.
#[derive(Debug, Clone, Default)]
pub struct EnvironmentUi {
    pub open: Option<EnvironmentTab>,
    pub preview: Option<Arc<image::RgbaImage>>,
    /// Bumped when `preview` changes.
    pub preview_version: u64,
}

pub fn handles(c: &C) -> bool {
    matches!(
        c,
        C::EnvironmentDialogToggle
            | C::RenderingEffects
            | C::EnvironmentSettings { .. }
            | C::ExposureControl { .. }
            | C::ExposureRenderPreview
            | C::AddAtmosphere { .. }
            | C::DeleteAtmosphere { .. }
            | C::AtmosphereEntry { .. }
            | C::FogParameters { .. }
            | C::VolumeFogParameters { .. }
            | C::VolumeLightParameters { .. }
            | C::FireEffectParameters { .. }
            | C::AtmosphereGizmo { .. }
            | C::ApparatusAddAtmosphere { .. }
            | C::AddEffect { .. }
            | C::DeleteEffect { .. }
            | C::MergeEffects { .. }
            | C::EffectEntry { .. }
            | C::GlowParameters { .. }
            | C::LensElementParameters { .. }
            | C::LensRingParameters { .. }
            | C::LensGlobalsParameters { .. }
            | C::LensSecondaryParameters { .. }
            | C::BlurParameters { .. }
            | C::BrightnessContrastParameters { .. }
            | C::ColorBalanceParameters { .. }
            | C::FilmGrainParameters { .. }
            | C::FileOutputParameters { .. }
            | C::MotionBlurParameters { .. }
            | C::DepthOfFieldParameters { .. }
    )
}

/// One undo step for the environment.
fn commit(state: &mut AppState, label: &str, after: Environment) -> Result<(), String> {
    if after == state.scene.environment {
        return Ok(());
    }
    let before = Box::new(state.scene.environment.clone());
    state.undo.commit(label, Change::SetEnvironment { before, after: Box::new(after) }, &mut state.scene)
}

fn colour(c: [f32; 3]) -> Result<Vec3, String> {
    if c.iter().all(|v| (0.0..=1.0).contains(v)) {
        Ok(Vec3::from(c))
    } else {
        Err(format!("colour {c:?}: 0-1"))
    }
}

fn range(name: &str, v: f32, lo: f32, hi: f32) -> Result<f32, String> {
    if v.is_finite() && (lo..=hi).contains(&v) {
        Ok(v)
    } else {
        Err(format!("{name} {v}: {lo}-{hi}"))
    }
}

fn percent(name: &str, v: f32) -> Result<f32, String> {
    if v.is_finite() && (0.0..=100.0).contains(&v) {
        Ok(v)
    } else {
        Err(format!("{name} {v}: 0-100"))
    }
}

fn set_opt<T: loom_scene::param::Animatable>(p: &mut loom_scene::Param<T>, v: Option<T>) {
    if let Some(v) = v {
        p.set(v);
    }
}

/// Moves `list[i]` by `by` places, clamped to the list.
fn move_entry<T>(list: &mut [T], i: usize, by: i32) -> usize {
    let to = (i as i64 + by as i64).clamp(0, list.len() as i64 - 1) as usize;
    if to < i {
        list[to..=i].rotate_right(1);
    } else if to > i {
        list[i..=to].rotate_left(1);
    }
    to
}

fn atmosphere_mut(e: &mut Environment, index: u32) -> Result<&mut Atmosphere, String> {
    e.atmospheres.get_mut(index as usize).ok_or_else(|| format!("no atmosphere {index}"))
}

fn effect_mut(e: &mut Environment, index: u32) -> Result<&mut RenderEffect, String> {
    e.effects.get_mut(index as usize).ok_or_else(|| format!("no effect {index}"))
}

/// Checks that `id` is an atmospheric apparatus.
fn apparatus(state: &AppState, id: u64) -> Result<(), String> {
    match state.scene.node(NodeId(id)).map(|n| &n.object) {
        Some(ObjectKind::AtmosApparatus { .. }) => Ok(()),
        Some(_) => Err(format!("node {id} is not an atmospheric apparatus")),
        None => Err(format!("no node {id}")),
    }
}

fn not_a(index: u32, what: &str) -> String {
    format!("entry {index} is not a {what}")
}

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    let mut e = state.scene.environment.clone();
    match cmd {
        C::EnvironmentDialogToggle => {
            let ui = &mut state.environment_ui;
            ui.open = if ui.open == Some(EnvironmentTab::Environment) { None } else { Some(EnvironmentTab::Environment) };
            Ok(json!({ "open": ui.open.is_some() }))
        }
        C::RenderingEffects => {
            state.environment_ui.open = Some(EnvironmentTab::Effects);
            Ok(json!({ "open": true }))
        }
        C::EnvironmentSettings { background, map, use_map, mapping, tint, level, ambient } => {
            if let Some(c) = background {
                e.background.set(colour(c)?);
            }
            if let Some(m) = map {
                let m = m.trim().to_owned();
                // Choosing a map turns Use Map on; removing it turns it off.
                e.use_map = !m.is_empty();
                e.background_map = (!m.is_empty()).then_some(m);
            }
            if let Some(u) = use_map {
                e.use_map = u;
            }
            if let Some(m) = mapping {
                e.map_mapping = m;
            }
            if let Some(c) = tint {
                e.tint.set(colour(c)?);
            }
            if let Some(l) = level {
                e.level.set(range("level", l, 0.0, 100.0)?);
            }
            if let Some(c) = ambient {
                e.ambient.set(colour(c)?);
            }
            commit(state, "Environment", e)?;
            Ok(environment_json(&state.scene.environment))
        }
        C::ExposureControl { kind, active, process_background, brightness, contrast, exposure_value, mid_tones } => {
            let x = &mut e.exposure;
            if let Some(k) = kind {
                x.kind = k;
            }
            if let Some(v) = active {
                x.active = v;
            }
            if let Some(v) = process_background {
                x.process_background = v;
            }
            let bright_max = if x.kind == ExposureKind::Logarithmic { 200.0 } else { 100.0 };
            if let Some(v) = brightness {
                x.brightness.set(range("brightness", v, 0.0, bright_max)?);
            } else if x.brightness.get() > bright_max {
                // Another control's range: keep the value inside it.
                x.brightness.set(bright_max);
            }
            set_opt(&mut x.contrast, contrast.map(|v| range("contrast", v, 0.0, 100.0)).transpose()?);
            set_opt(&mut x.exposure_value, exposure_value.map(|v| range("exposure value", v, -5.0, 5.0)).transpose()?);
            set_opt(&mut x.mid_tones, mid_tones.map(|v| range("mid tones", v, 0.01, 20.0)).transpose()?);
            commit(state, "Exposure Control", e)?;
            Ok(serde_json::to_value(&state.scene.environment.exposure).unwrap_or(Value::Null))
        }
        C::ExposureRenderPreview => {
            let mut settings = state.scene.render.clone();
            // The thumbnail keeps the output's shape inside PREVIEW_SIZE.
            let aspect = settings.image_aspect().max(1e-3);
            let [pw, ph] = PREVIEW_SIZE;
            let (w, h) = if aspect >= pw as f32 / ph as f32 { (pw, ((pw as f32 / aspect).round() as u32).max(1)) } else { (((ph as f32 * aspect).round() as u32).max(1), ph) };
            settings.width = w;
            settings.height = h;
            settings.pixel_aspect = 1.0;
            settings.area = loom_scene::render::RenderArea::View;
            settings.bevy.supersampling = 1;
            let viewport = crate::render::render_viewport(state);
            crate::render::start(state, RenderRequest { viewport, settings, selection: Vec::new(), save_path: None, iterative: false, preview: true, ..Default::default() })
        }
        C::AddAtmosphere { kind, name } => {
            let k = AtmosphereKind::new(&kind).ok_or_else(|| format!("unknown atmosphere {kind:?} ({})", AtmosphereKind::NAMES.join(", ")))?;
            let names: Vec<&str> = e.atmospheres.iter().map(|a| a.name.as_str()).collect();
            let name = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| Environment::unique_name(&names, k.type_name()));
            e.atmospheres.push(Atmosphere { name, active: true, kind: k });
            let index = e.atmospheres.len() - 1;
            commit(state, "Add Atmosphere", e)?;
            Ok(json!({ "index": index }))
        }
        C::DeleteAtmosphere { index } => {
            atmosphere_mut(&mut e, index)?;
            e.atmospheres.remove(index as usize);
            commit(state, "Delete Atmosphere", e)?;
            Ok(json!({ "count": state.scene.environment.atmospheres.len() }))
        }
        C::AtmosphereEntry { index, name, active, move_by } => {
            let a = atmosphere_mut(&mut e, index)?;
            if let Some(n) = name.filter(|n| !n.trim().is_empty()) {
                a.name = n;
            }
            if let Some(v) = active {
                a.active = v;
            }
            let at = move_by.map_or(index as usize, |by| move_entry(&mut e.atmospheres, index as usize, by));
            commit(state, "Atmosphere", e)?;
            Ok(json!({ "index": at }))
        }
        C::FogParameters { index, color, fog_background, layered, exponential, near, far, top, bottom, density, falloff } => {
            let AtmosphereKind::Fog(f) = &mut atmosphere_mut(&mut e, index)?.kind else { return Err(not_a(index, "Fog")) };
            set_opt(&mut f.color, color.map(colour).transpose()?);
            if let Some(v) = fog_background {
                f.fog_background = v;
            }
            if let Some(v) = layered {
                f.layered = v;
            }
            if let Some(v) = exponential {
                f.exponential = v;
            }
            set_opt(&mut f.near, near.map(|v| percent("near", v)).transpose()?);
            set_opt(&mut f.far, far.map(|v| percent("far", v)).transpose()?);
            set_opt(&mut f.density, density.map(|v| percent("density", v)).transpose()?);
            let (t, b) = (top.unwrap_or(f.top.get()), bottom.unwrap_or(f.bottom.get()));
            if !(t.is_finite() && b.is_finite()) || t < b {
                return Err(format!("Top {t} is below Bottom {b}"));
            }
            set_opt(&mut f.top, top);
            set_opt(&mut f.bottom, bottom);
            if let Some(v) = falloff {
                f.falloff = v;
            }
            commit(state, "Fog Parameters", e)?;
            Ok(json!({ "index": index }))
        }
        C::VolumeFogParameters { index, color, density, fog_background } => {
            let AtmosphereKind::VolumeFog(f) = &mut atmosphere_mut(&mut e, index)?.kind else { return Err(not_a(index, "Volume Fog")) };
            set_opt(&mut f.color, color.map(colour).transpose()?);
            set_opt(&mut f.density, density.map(|v| range("density", v, 0.0, 20.0)).transpose()?);
            if let Some(v) = fog_background {
                f.fog_background = v;
            }
            commit(state, "Volume Fog Parameters", e)?;
            Ok(json!({ "index": index }))
        }
        C::VolumeLightParameters { index, add_light, remove_light, fog_color, density } => {
            if let Some(id) = add_light {
                match state.scene.node(NodeId(id)).map(|n| &n.object) {
                    Some(ObjectKind::Light(_)) => {}
                    Some(_) => return Err(format!("node {id} is not a light")),
                    None => return Err(format!("no node {id}")),
                }
            }
            let AtmosphereKind::VolumeLight(l) = &mut atmosphere_mut(&mut e, index)?.kind else { return Err(not_a(index, "Volume Light")) };
            if let Some(id) = add_light {
                if !l.lights.contains(&NodeId(id)) {
                    l.lights.push(NodeId(id));
                }
            }
            if let Some(id) = remove_light {
                l.lights.retain(|n| n.0 != id);
            }
            set_opt(&mut l.fog_color, fog_color.map(colour).transpose()?);
            set_opt(&mut l.density, density.map(|v| percent("density", v)).transpose()?);
            commit(state, "Volume Light Parameters", e)?;
            Ok(json!({ "index": index }))
        }
        C::FireEffectParameters { index, inner_color, outer_color, smoke_color, tendril, stretch, regularity, flame_size, flame_detail, density, samples, phase, drift, explosion, smoke, fury } => {
            let AtmosphereKind::Fire(f) = &mut atmosphere_mut(&mut e, index)?.kind else { return Err(not_a(index, "Fire Effect")) };
            set_opt(&mut f.inner_color, inner_color.map(colour).transpose()?);
            set_opt(&mut f.outer_color, outer_color.map(colour).transpose()?);
            set_opt(&mut f.smoke_color, smoke_color.map(colour).transpose()?);
            if let Some(v) = tendril {
                f.tendril = v;
            }
            set_opt(&mut f.stretch, stretch.map(|v| range("stretch", v, 0.0, 1000.0)).transpose()?);
            set_opt(&mut f.regularity, regularity.map(|v| range("regularity", v, 0.0, 1.0)).transpose()?);
            set_opt(&mut f.flame_size, flame_size.map(|v| range("flame size", v, 0.0, 10000.0)).transpose()?);
            set_opt(&mut f.flame_detail, flame_detail.map(|v| range("flame detail", v, 0.0, 10.0)).transpose()?);
            set_opt(&mut f.density, density.map(|v| range("density", v, 0.0, 10000.0)).transpose()?);
            set_opt(&mut f.samples, samples.map(|v| v.clamp(1, 50)));
            set_opt(&mut f.phase, phase.map(|v| range("phase", v, -100000.0, 100000.0)).transpose()?);
            set_opt(&mut f.drift, drift.map(|v| range("drift", v, -100000.0, 100000.0)).transpose()?);
            if let Some(v) = explosion {
                f.explosion = v;
            }
            if let Some(v) = smoke {
                f.smoke = v;
            }
            set_opt(&mut f.fury, fury.map(|v| range("fury", v, 0.0, 1000.0)).transpose()?);
            commit(state, "Fire Effect Parameters", e)?;
            Ok(json!({ "index": index }))
        }
        C::AtmosphereGizmo { index, add, remove } => {
            if let Some(id) = add {
                apparatus(state, id)?;
            }
            let a = atmosphere_mut(&mut e, index)?;
            let what = a.kind.type_name();
            let gizmos = a.kind.gizmos_mut().ok_or_else(|| format!("{what} takes no gizmos (Fire Effect, Volume Fog do)"))?;
            if let Some(id) = add {
                if !gizmos.contains(&NodeId(id)) {
                    gizmos.push(NodeId(id));
                }
            }
            if let Some(id) = remove {
                gizmos.retain(|n| n.0 != id);
            }
            commit(state, "Atmosphere Gizmo", e)?;
            Ok(json!({ "index": index }))
        }
        C::ApparatusAddAtmosphere { id, kind, existing } => {
            apparatus(state, id)?;
            let mut a = match (existing, kind) {
                // Existing: a new atmosphere whose settings start as the chosen one's.
                (Some(i), _) => {
                    let src = atmosphere_mut(&mut e, i)?.clone();
                    if src.kind.gizmos().is_none() {
                        return Err(not_a(i, "Fire Effect or Volume Fog"));
                    }
                    src
                }
                (None, Some(k)) => {
                    let kind = AtmosphereKind::new(&k).filter(|k| k.gizmos().is_some()).ok_or_else(|| format!("an apparatus takes {} (not {k:?})", AtmosphereKind::GIZMO_NAMES.join(", ")))?;
                    Atmosphere { name: String::new(), active: true, kind }
                }
                (None, None) => return Err("give a `kind` or an `existing` entry".into()),
            };
            let names: Vec<&str> = e.atmospheres.iter().map(|a| a.name.as_str()).collect();
            a.name = Environment::unique_name(&names, a.kind.type_name());
            if let Some(g) = a.kind.gizmos_mut() {
                *g = vec![NodeId(id)];
            }
            e.atmospheres.push(a);
            let index = e.atmospheres.len() - 1;
            commit(state, "Add Atmosphere", e)?;
            Ok(json!({ "index": index }))
        }
        C::AddEffect { kind, name } => {
            let k = EffectKind::new(&kind).ok_or_else(|| format!("unknown effect {kind:?} ({})", EffectKind::NAMES.join(", ")))?;
            let names: Vec<&str> = e.effects.iter().map(|a| a.name.as_str()).collect();
            let name = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| Environment::unique_name(&names, k.type_name()));
            e.effects.push(RenderEffect { name, active: true, kind: k });
            let index = e.effects.len() - 1;
            commit(state, "Add Effect", e)?;
            Ok(json!({ "index": index }))
        }
        C::MergeEffects { path, effects } => {
            let (doc, _) = crate::file::read_any(std::path::Path::new(&path))?;
            let from = doc.scene.environment.effects;
            if let Some(i) = effects.iter().find(|i| **i as usize >= from.len()) {
                return Err(format!("{path} has no effect {i} (it has {})", from.len()));
            }
            let mut names = Vec::new();
            for (_, fx) in from.into_iter().enumerate().filter(|(i, _)| effects.is_empty() || effects.contains(&(*i as u32))) {
                let taken: Vec<&str> = e.effects.iter().map(|a| a.name.as_str()).collect();
                let name = Environment::unique_name(&taken, &fx.name);
                names.push(name.clone());
                e.effects.push(RenderEffect { name, ..fx });
            }
            commit(state, "Merge Effects", e)?;
            Ok(json!({ "merged": names }))
        }
        C::DeleteEffect { index } => {
            effect_mut(&mut e, index)?;
            e.effects.remove(index as usize);
            commit(state, "Delete Effect", e)?;
            Ok(json!({ "count": state.scene.environment.effects.len() }))
        }
        C::EffectEntry { index, name, active, move_by } => {
            let a = effect_mut(&mut e, index)?;
            if let Some(n) = name.filter(|n| !n.trim().is_empty()) {
                a.name = n;
            }
            if let Some(v) = active {
                a.active = v;
            }
            let at = move_by.map_or(index as usize, |by| move_entry(&mut e.effects, index as usize, by));
            commit(state, "Effect", e)?;
            Ok(json!({ "index": at }))
        }
        C::GlowParameters { index, size, intensity, use_source_color, inner_color, outer_color, bright } => {
            let EffectKind::Glow(g) = &mut effect_mut(&mut e, index)?.kind else { return Err(not_a(index, "Lens Effects Glow")) };
            set_opt(&mut g.size, size.map(|v| range("size", v, 0.0, 100.0)).transpose()?);
            set_opt(&mut g.intensity, intensity.map(|v| range("intensity", v, 0.0, 1000.0)).transpose()?);
            set_opt(&mut g.use_source_color, use_source_color.map(|v| percent("use source color", v)).transpose()?);
            set_opt(&mut g.inner_color, inner_color.map(colour).transpose()?);
            set_opt(&mut g.outer_color, outer_color.map(colour).transpose()?);
            set_opt(&mut g.bright, bright.map(|v| range("bright", v, 0.0, 255.0)).transpose()?);
            commit(state, "Glow Parameters", e)?;
            Ok(json!({ "index": index }))
        }
        C::LensElementParameters { index, element, on, size, intensity, width, angle, taper, sharp, qty, use_source_color, inner_color, outer_color } => {
            let EffectKind::Glow(g) = &mut effect_mut(&mut e, index)?.kind else { return Err(not_a(index, "Lens Effects")) };
            let el = match element.to_ascii_lowercase().as_str() {
                "star" => &mut g.star,
                "streak" => &mut g.streak,
                "ray" => &mut g.ray,
                _ => return Err(format!("unknown lens element {element:?} (Star, Streak, Ray)")),
            };
            if let Some(v) = on {
                el.on = v;
            }
            if let Some(q) = qty {
                if element.eq_ignore_ascii_case("streak") && q != 2 {
                    return Err("a Streak has two spokes".into());
                }
                el.qty = if element.eq_ignore_ascii_case("ray") { q.min(1000) } else { q.min(30) };
            }
            set_opt(&mut el.size, size.map(|v| range("size", v, 0.0, 100.0)).transpose()?);
            set_opt(&mut el.intensity, intensity.map(|v| range("intensity", v, 0.0, 1000.0)).transpose()?);
            set_opt(&mut el.width, width.map(|v| range("width", v, 0.0, 100.0)).transpose()?);
            set_opt(&mut el.angle, angle.map(|v| range("angle", v, -3600.0, 3600.0)).transpose()?);
            set_opt(&mut el.taper, taper.map(|v| range("taper", v, 0.0, 1000.0)).transpose()?);
            set_opt(&mut el.sharp, sharp.map(|v| range("sharp", v, 0.0, 10.0)).transpose()?);
            set_opt(&mut el.use_source_color, use_source_color.map(|v| percent("use source color", v)).transpose()?);
            set_opt(&mut el.inner_color, inner_color.map(colour).transpose()?);
            set_opt(&mut el.outer_color, outer_color.map(colour).transpose()?);
            commit(state, "Lens Element Parameters", e)?;
            Ok(json!({ "index": index }))
        }
        C::LensGlobalsParameters { index, size, intensity, seed, angle } => {
            let EffectKind::Glow(g) = &mut effect_mut(&mut e, index)?.kind else { return Err(not_a(index, "Lens Effects")) };
            let gl = &mut g.globals;
            set_opt(&mut gl.size, size.map(|v| range("size", v, 0.0, 1000.0)).transpose()?);
            set_opt(&mut gl.intensity, intensity.map(|v| range("intensity", v, 0.0, 1000.0)).transpose()?);
            set_opt(&mut gl.angle, angle.map(|v| range("angle", v, -3600.0, 3600.0)).transpose()?);
            if let Some(s) = seed {
                gl.seed = s;
            }
            commit(state, "Lens Globals Parameters", e)?;
            Ok(json!({ "index": index }))
        }
        C::LensSecondaryParameters { index, element, on, size, min, max, axis, plane, intensity, qty, sides, use_source_color, inner_color, outer_color } => {
            let EffectKind::Glow(g) = &mut effect_mut(&mut e, index)?.kind else { return Err(not_a(index, "Lens Effects")) };
            if let Some(n) = sides {
                if n != 0 && !(3..=8).contains(&n) {
                    return Err(format!("sides {n}: 0 (circular) or 3-8"));
                }
            }
            let manual = match element.to_ascii_lowercase().as_str() {
                "manual" => true,
                "auto" => false,
                _ => return Err(format!("unknown secondary element {element:?} (Manual, Auto)")),
            };
            if manual {
                if min.is_some() || max.is_some() || axis.is_some() || qty.is_some() {
                    return Err("Min, Max, Axis and Qty belong to the Auto Secondary".into());
                }
                let m = &mut g.manual_secondary;
                if let Some(v) = on {
                    m.on = v;
                }
                if let Some(n) = sides {
                    m.sides = n;
                }
                set_opt(&mut m.size, size.map(|v| range("size", v, 0.0, 100.0)).transpose()?);
                set_opt(&mut m.plane, plane.map(|v| range("plane", v, -1000.0, 1000.0)).transpose()?);
                set_opt(&mut m.intensity, intensity.map(|v| range("intensity", v, 0.0, 1000.0)).transpose()?);
                set_opt(&mut m.use_source_color, use_source_color.map(|v| percent("use source color", v)).transpose()?);
                set_opt(&mut m.inner_color, inner_color.map(colour).transpose()?);
                set_opt(&mut m.outer_color, outer_color.map(colour).transpose()?);
            } else {
                if size.is_some() || plane.is_some() {
                    return Err("Size and Plane belong to the Manual Secondary".into());
                }
                let a = &mut g.auto_secondary;
                if let Some(v) = on {
                    a.on = v;
                }
                if let Some(n) = sides {
                    a.sides = n;
                }
                if let Some(q) = qty {
                    a.qty = q.min(100);
                }
                set_opt(&mut a.min, min.map(|v| range("min", v, 0.0, 100.0)).transpose()?);
                set_opt(&mut a.max, max.map(|v| range("max", v, 0.0, 100.0)).transpose()?);
                set_opt(&mut a.axis, axis.map(|v| range("axis", v, 0.0, 10.0)).transpose()?);
                set_opt(&mut a.intensity, intensity.map(|v| range("intensity", v, 0.0, 1000.0)).transpose()?);
                set_opt(&mut a.use_source_color, use_source_color.map(|v| percent("use source color", v)).transpose()?);
                set_opt(&mut a.inner_color, inner_color.map(colour).transpose()?);
                set_opt(&mut a.outer_color, outer_color.map(colour).transpose()?);
            }
            commit(state, "Lens Secondary Parameters", e)?;
            Ok(json!({ "index": index }))
        }
        C::LensRingParameters { index, on, size, intensity, plane, thickness, use_source_color, inner_color, outer_color } => {
            let EffectKind::Glow(g) = &mut effect_mut(&mut e, index)?.kind else { return Err(not_a(index, "Lens Effects")) };
            let r = &mut g.ring;
            if let Some(v) = on {
                r.on = v;
            }
            set_opt(&mut r.size, size.map(|v| range("size", v, 0.0, 100.0)).transpose()?);
            set_opt(&mut r.intensity, intensity.map(|v| range("intensity", v, 0.0, 1000.0)).transpose()?);
            set_opt(&mut r.plane, plane.map(|v| range("plane", v, -1000.0, 1000.0)).transpose()?);
            set_opt(&mut r.thickness, thickness.map(|v| range("thickness", v, 0.0, 1000.0)).transpose()?);
            set_opt(&mut r.use_source_color, use_source_color.map(|v| percent("use source color", v)).transpose()?);
            set_opt(&mut r.inner_color, inner_color.map(colour).transpose()?);
            set_opt(&mut r.outer_color, outer_color.map(colour).transpose()?);
            commit(state, "Lens Ring Parameters", e)?;
            Ok(json!({ "index": index }))
        }
        C::BlurParameters { index, blur_type, radius, u_radius, v_radius, u_trail, v_trail, rotation, radial_trail, origin, affect_alpha, non_background, brighten, blend } => {
            let EffectKind::Blur(b) = &mut effect_mut(&mut e, index)?.kind else { return Err(not_a(index, "Blur")) };
            if let Some(t) = blur_type {
                b.blur_type = t;
            }
            let px = |n: &str, v: f32| range(n, v, 0.0, 1000.0);
            let trail = |n: &str, v: f32| range(n, v, -100.0, 100.0);
            set_opt(&mut b.radius, radius.map(|v| px("pixel radius", v)).transpose()?);
            set_opt(&mut b.u_radius, u_radius.map(|v| px("U pixel radius", v)).transpose()?);
            set_opt(&mut b.v_radius, v_radius.map(|v| px("V pixel radius", v)).transpose()?);
            set_opt(&mut b.u_trail, u_trail.map(|v| trail("U trail", v)).transpose()?);
            set_opt(&mut b.v_trail, v_trail.map(|v| trail("V trail", v)).transpose()?);
            set_opt(&mut b.rotation, rotation.map(|v| range("rotation", v, -360.0, 360.0)).transpose()?);
            set_opt(&mut b.radial_trail, radial_trail.map(|v| trail("trail", v)).transpose()?);
            if let Some(o) = origin {
                if !o.iter().all(|v| v.is_finite()) {
                    return Err(format!("origin {o:?}"));
                }
                b.origin = o;
            }
            if let Some(v) = affect_alpha {
                b.affect_alpha = v;
            }
            if let Some(v) = non_background {
                b.non_background = v;
            }
            set_opt(&mut b.brighten, brighten.map(|v| percent("brighten", v)).transpose()?);
            set_opt(&mut b.blend, blend.map(|v| percent("blend", v)).transpose()?);
            commit(state, "Blur Parameters", e)?;
            Ok(json!({ "index": index }))
        }
        C::BrightnessContrastParameters { index, brightness, contrast, ignore_background } => {
            let EffectKind::BrightnessContrast(b) = &mut effect_mut(&mut e, index)?.kind else { return Err(not_a(index, "Brightness and Contrast")) };
            set_opt(&mut b.brightness, brightness.map(|v| range("brightness", v, 0.0, 1.0)).transpose()?);
            set_opt(&mut b.contrast, contrast.map(|v| range("contrast", v, 0.0, 1.0)).transpose()?);
            if let Some(v) = ignore_background {
                b.ignore_background = v;
            }
            commit(state, "Brightness and Contrast Parameters", e)?;
            Ok(json!({ "index": index }))
        }
        C::ColorBalanceParameters { index, cyan_red, magenta_green, yellow_blue, preserve_luminosity, ignore_background } => {
            let EffectKind::ColorBalance(c) = &mut effect_mut(&mut e, index)?.kind else { return Err(not_a(index, "Color Balance")) };
            let bal = |n: &str, v: f32| range(n, v, -100.0, 100.0);
            set_opt(&mut c.cyan_red, cyan_red.map(|v| bal("cyan/red", v)).transpose()?);
            set_opt(&mut c.magenta_green, magenta_green.map(|v| bal("magenta/green", v)).transpose()?);
            set_opt(&mut c.yellow_blue, yellow_blue.map(|v| bal("yellow/blue", v)).transpose()?);
            if let Some(v) = preserve_luminosity {
                c.preserve_luminosity = v;
            }
            if let Some(v) = ignore_background {
                c.ignore_background = v;
            }
            commit(state, "Color Balance Parameters", e)?;
            Ok(json!({ "index": index }))
        }
        C::FilmGrainParameters { index, grain, ignore_background } => {
            let EffectKind::FilmGrain(f) = &mut effect_mut(&mut e, index)?.kind else { return Err(not_a(index, "Film Grain")) };
            set_opt(&mut f.grain, grain.map(|v| range("grain", v, 0.0, 10.0)).transpose()?);
            if let Some(v) = ignore_background {
                f.ignore_background = v;
            }
            commit(state, "Film Grain Parameters", e)?;
            Ok(json!({ "index": index }))
        }
        C::MotionBlurParameters { index, duration } => {
            let EffectKind::MotionBlur(m) = &mut effect_mut(&mut e, index)?.kind else { return Err(not_a(index, "Motion Blur")) };
            set_opt(&mut m.duration, duration.map(|v| range("duration", v, 0.0, 100.0)).transpose()?);
            commit(state, "Motion Blur Parameters", e)?;
            Ok(json!({ "index": index }))
        }
        C::FileOutputParameters { index, path, channel, affect_source } => {
            let EffectKind::FileOutput(f) = &mut effect_mut(&mut e, index)?.kind else { return Err(not_a(index, "File Output")) };
            if let Some(p) = path {
                f.path = Some(p).filter(|p| !p.is_empty());
            }
            if let Some(c) = channel {
                if c as usize >= loom_scene::environment::FileOutput::CHANNELS.len() {
                    return Err(format!("channel {c}: 0 Whole Image, 1 Luminance, 2 Alpha"));
                }
                f.channel = c;
            }
            if let Some(v) = affect_source {
                f.affect_source = v;
            }
            commit(state, "File Output Parameters", e)?;
            Ok(json!({ "index": index }))
        }
        C::DepthOfFieldParameters { index, focal_node, horiz_loss, vert_loss, focal_range, focal_limit } => {
            if let Some(id) = focal_node.filter(|id| *id != 0) {
                if state.scene.node(NodeId(id)).is_none() {
                    return Err(format!("no node {id}"));
                }
            }
            let EffectKind::DepthOfField(d) = &mut effect_mut(&mut e, index)?.kind else { return Err(not_a(index, "Depth of Field")) };
            if let Some(id) = focal_node {
                d.focal_node = (id != 0).then_some(NodeId(id));
            }
            let loss = |n: &str, v: f32| range(n, v, 0.0, 100.0);
            set_opt(&mut d.horiz_loss, horiz_loss.map(|v| loss("horiz focal loss", v)).transpose()?);
            set_opt(&mut d.vert_loss, vert_loss.map(|v| loss("vert focal loss", v)).transpose()?);
            let (r, l) = (focal_range.unwrap_or(d.focal_range.get()), focal_limit.unwrap_or(d.focal_limit.get()));
            if !(r.is_finite() && l.is_finite()) || r < 0.0 || l < r {
                return Err(format!("Focal Limit {l} must be at least the Focal Range {r} (and both 0 or more)"));
            }
            set_opt(&mut d.focal_range, focal_range);
            set_opt(&mut d.focal_limit, focal_limit);
            commit(state, "Depth of Field Parameters", e)?;
            Ok(json!({ "index": index }))
        }
        _ => unreachable!("routed by execute_inner"),
    }
}

fn environment_json(e: &Environment) -> Value {
    json!({
        "background": e.background.get().to_array(),
        "map": e.background_map,
        "use_map": e.use_map,
        "mapping": e.map_mapping,
        "tint": e.tint.get().to_array(),
        "level": e.level.get(),
        "ambient": e.ambient.get().to_array(),
    })
}

/// `State` report of the environment.
pub fn state_json(state: &AppState) -> Value {
    let e = &state.scene.environment;
    let mut v = environment_json(e);
    v["open"] = json!(state.environment_ui.open.map(|t| format!("{t:?}")));
    v["exposure"] = serde_json::to_value(&e.exposure).unwrap_or(Value::Null);
    v["atmospheres"] = json!(e.atmospheres.iter().map(|a| json!({ "name": a.name, "type": a.kind.type_name(), "active": a.active })).collect::<Vec<_>>());
    v["effects"] = json!(e.effects.iter().map(|a| json!({ "name": a.name, "type": a.kind.type_name(), "active": a.active })).collect::<Vec<_>>());
    v
}
