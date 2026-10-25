//! Viewport Configuration panels beyond Layout / Safe Frames / Statistics /
//! ViewCube: Rendering Method with Apply To, Adaptive Degradation, Lighting
//! and Shadows, Regions, and the startup viewport configuration.

use crate::command::Command as C;
use crate::viewport::{Shading, Transparency};
use crate::AppState;
use serde_json::{json, Value};

/// The viewports an Apply To choice means.
pub fn apply_to(state: &AppState, choice: Option<&str>) -> Result<Vec<usize>, String> {
    let n = state.viewports.len();
    let a = state.active_viewport;
    Ok(match choice.map(|c| c.trim().to_ascii_lowercase()).as_deref() {
        None | Some("active only") | Some("active viewport only") | Some("active") => vec![a],
        Some("all viewports") | Some("all") => (0..n).collect(),
        Some("all but active") => (0..n).filter(|i| *i != a).collect(),
        Some(other) => return Err(format!("unknown Apply To {other:?} (Active Only, All Viewports, All But Active)")),
    })
}

fn level(name: &str) -> Result<Shading, String> {
    Shading::parse(name).ok_or_else(|| format!("unknown rendering level {name:?}"))
}

/// The file the startup viewport configuration lives in.
pub fn defaults_path() -> Option<std::path::PathBuf> {
    crate::settings::config_dir().map(|d| d.join("viewport-defaults.json"))
}

/// What Save as Default keeps: the layout and the views as a scene file keeps them.
#[derive(serde::Serialize, serde::Deserialize)]
struct Defaults {
    format: String,
    layout: crate::viewport::ViewportLayout,
    viewports: Vec<crate::file::ViewportSave>,
}

const DEFAULTS_FORMAT: &str = "3dloom-viewport-defaults-1";

/// Puts the saved startup viewport configuration (if any) into `state`.
pub fn apply_defaults(state: &mut AppState) {
    let Some(text) = defaults_path().and_then(|p| std::fs::read_to_string(p).ok()) else { return };
    let Ok(d) = serde_json::from_str::<Defaults>(&text) else { return };
    if d.format != DEFAULTS_FORMAT || d.viewports.len() != crate::viewport::default_kinds(d.layout).len() {
        return;
    }
    state.layout = d.layout;
    state.viewports = d.viewports.iter().map(|v| v.to_viewport()).collect();
    state.active_viewport = state.active_viewport.min(state.viewports.len() - 1);
}

pub fn handles(c: &C) -> bool {
    matches!(c, C::SetViewportRendering { .. } | C::AdaptiveDegradationToggle | C::SetAdaptiveDegradation { .. } | C::SetViewportLighting { .. } | C::SetRenderRegions { .. } | C::SaveViewportDefaults)
}

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    match cmd {
        C::SetViewportRendering { apply_to: to, shading, edged_faces, transparency, disable_textures, force_two_sided } => {
            let views = apply_to(state, to.as_deref())?;
            let shading = shading.map(|s| level(&s)).transpose()?;
            let transparency = match transparency {
                Some(t) => Some(Transparency::ALL.into_iter().find(|x| x.label().eq_ignore_ascii_case(t.trim())).ok_or_else(|| format!("unknown transparency {t:?} (None, Simple, Best)"))?),
                None => None,
            };
            for i in &views {
                let vp = &mut state.viewports[*i];
                if let Some(s) = shading {
                    vp.shading = s;
                }
                if let Some(e) = edged_faces {
                    vp.edged_faces = e;
                }
                if let Some(t) = transparency {
                    vp.transparency = t;
                }
                if let Some(d) = disable_textures {
                    vp.disable_textures = d;
                }
                if let Some(f) = force_two_sided {
                    vp.force_two_sided = f;
                }
            }
            Ok(json!({ "viewports": views }))
        }
        C::AdaptiveDegradationToggle => {
            state.degradation.on = !state.degradation.on;
            Ok(json!({ "adaptive_degradation": state.degradation.on }))
        }
        C::SetAdaptiveDegradation { general, active, maintain_fps, never_degrade_selected, degrade_to_default_lighting } => {
            let d = &mut state.degradation;
            if let Some(g) = general {
                d.general = level(&g)?;
            }
            if let Some(a) = active {
                d.active = level(&a)?;
            }
            if let Some(f) = maintain_fps {
                d.maintain_fps = f.clamp(1.0, 120.0);
            }
            if let Some(n) = never_degrade_selected {
                d.never_degrade_selected = n;
            }
            if let Some(l) = degrade_to_default_lighting {
                d.degrade_to_default_lighting = l;
            }
            Ok(json!({ "general": d.general.label(), "active": d.active.label(), "maintain_fps": d.maintain_fps }))
        }
        C::SetViewportLighting { apply_to: to, scene_lights, default_lights, hardware } => {
            let views = apply_to(state, to.as_deref())?;
            for i in &views {
                let vp = &mut state.viewports[*i];
                if let Some(s) = scene_lights {
                    vp.scene_lights = s;
                }
                if let Some(n) = default_lights {
                    vp.default_lights = n.clamp(1, 2);
                }
                match hardware {
                    Some(true) => vp.shading = Shading::Realistic,
                    Some(false) if vp.shading == Shading::Realistic => vp.shading = Shading::SmoothHighlights,
                    _ => {}
                }
            }
            Ok(json!({ "viewports": views }))
        }
        C::SetRenderRegions { region, blowup } => {
            let fix = |r: [f32; 4]| {
                let (x0, x1) = (r[0].min(r[2]).clamp(0.0, 1.0), r[0].max(r[2]).clamp(0.0, 1.0));
                let (y0, y1) = (r[1].min(r[3]).clamp(0.0, 1.0), r[1].max(r[3]).clamp(0.0, 1.0));
                [x0, y0, x1.max(x0 + 1e-3).min(1.0), y1.max(y0 + 1e-3).min(1.0)]
            };
            if let Some(r) = region {
                state.scene.render.region = fix(r);
            }
            if let Some(b) = blowup {
                state.scene.render.blowup = fix(b);
            }
            Ok(json!({ "region": state.scene.render.region, "blowup": state.scene.render.blowup }))
        }
        C::SaveViewportDefaults => {
            let path = defaults_path().ok_or("no config folder")?;
            let d = Defaults { format: DEFAULTS_FORMAT.into(), layout: state.layout, viewports: state.viewports.iter().map(crate::file::ViewportSave::from_viewport).collect() };
            crate::execute::write_text(&path.to_string_lossy(), &serde_json::to_string_pretty(&d).expect("plain data"))?;
            Ok(json!({ "path": path }))
        }
        other => Err(format!("{} is not a Viewport Configuration command", other.name())),
    }
}
