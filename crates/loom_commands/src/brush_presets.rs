//! Brush Presets toolbar and Brush Preset Manager: up to 50 brush settings
//! for the paint tools (Paint Deformation, Paint Soft Selection, VertexPaint,
//! Skin). The Painter Options settings (strength, size, falloff, mirror) are
//! shared by every context; each preset also keeps its context settings.

use crate::execute::commit;
use crate::state::{AppState, PaintMode};
use crate::Command as C;
use loom_scene::modifier::Modifier;
use loom_scene::Change;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const MAX_PRESETS: usize = 50;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrushPreset {
    pub name: String,
    /// Painter Options: strength, size, their ranges, falloff, mirror.
    pub painter: crate::skin::Painter,
    /// PaintDeform: Push/Pull, Relax or Revert, and the Push/Pull Value.
    #[serde(default)]
    pub deform_mode: Option<String>,
    #[serde(default)]
    pub deform_value: Option<f32>,
    /// PaintSoftSel: Paint, Blur or Revert.
    #[serde(default)]
    pub soft_mode: Option<String>,
    /// VertexPaint: colour (0..1) and opacity.
    #[serde(default)]
    pub vp_color: Option<[f32; 3]>,
    #[serde(default)]
    pub vp_opacity: Option<f32>,
}

/// The presets (kept with the preferences) and the icon size range.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BrushPresets {
    pub presets: Vec<BrushPreset>,
    pub icon_min: f32,
    pub icon_max: f32,
}

impl Default for BrushPresets {
    fn default() -> Self {
        // Five presets, as in the reference; their sizes are this build's.
        let presets = [2.0, 5.0, 10.0, 20.0, 40.0]
            .iter()
            .map(|s| BrushPreset { name: format!("Size {s}"), painter: crate::skin::Painter { size: *s, ..Default::default() }, deform_mode: None, deform_value: None, soft_mode: None, vp_color: None, vp_opacity: None })
            .collect();
        Self { presets, icon_min: 0.0, icon_max: 40.0 }
    }
}

/// The paint tool in use: VertexPaint, PaintDeform, PaintSoftSel, Paint Skin Weights.
pub fn context(state: &AppState) -> Option<&'static str> {
    if state.paint_deform.is_some() {
        Some("PaintDeform")
    } else if state.soft_paint.is_some() {
        Some("PaintSoftSel")
    } else if state.vertex_paint_brush {
        Some("VertexPaint")
    } else if state.skin_paint {
        Some("Paint Skin Weights")
    } else {
        None
    }
}

pub fn handles(c: &C) -> bool {
    matches!(c, C::BrushPresetManager | C::AddBrushPreset { .. } | C::ApplyBrushPreset { .. } | C::DuplicateBrushPreset { .. } | C::DeleteBrushPreset { .. } | C::RenameBrushPreset { .. } | C::SaveBrushPresets { .. } | C::LoadBrushPresets { .. } | C::SetBrushIconRange { .. })
}

/// The current brush as a preset named `name`.
pub fn capture(state: &AppState, name: &str) -> BrushPreset {
    let poly = state.selection.iter().find_map(|id| state.scene.node(*id).and_then(|n| n.edit_poly()).map(|p| p.paint.clone()));
    let vp = crate::vertex_paint::entry(state).and_then(|(id, i)| match &state.scene.node(id)?.modifiers.get(i)?.modifier {
        Modifier::VertexPaint { color, opacity, .. } => Some(([color[0].get(), color[1].get(), color[2].get()], opacity.get())),
        _ => None,
    });
    BrushPreset {
        name: name.into(),
        painter: state.skin_painter.clone(),
        deform_mode: state.paint_deform.map(|m| format!("{m:?}")),
        deform_value: poly.map(|p| p.push_pull_value),
        soft_mode: state.soft_paint.map(|m| format!("{m:?}")),
        vp_color: vp.map(|v| v.0),
        vp_opacity: vp.map(|v| v.1),
    }
}

fn apply(state: &mut AppState, p: &BrushPreset) -> Result<(), String> {
    state.skin_painter = p.painter.clone();
    let size = p.painter.size;
    // The size is the same in every context.
    for id in state.selection.clone() {
        if let Some(e) = state.scene.node_mut(id).and_then(|n| n.edit_poly_mut()) {
            e.paint.brush_size = size.max(0.01);
            e.paint.brush_strength = p.painter.strength.clamp(0.0, 1.0);
            if let Some(v) = p.deform_value {
                e.paint.push_pull_value = v;
            }
        }
    }
    state.prefs.paint_brush_size = size.clamp(1.0, 500.0);
    if state.paint_deform.is_some() {
        if let Some(m) = p.deform_mode.as_deref() {
            state.paint_deform = Some(match m {
                "Relax" => PaintMode::Relax,
                "Revert" => PaintMode::Revert,
                _ => PaintMode::PushPull,
            });
        }
    }
    if state.soft_paint.is_some() {
        if let Some(m) = p.soft_mode.as_deref() {
            state.soft_paint = Some(match m {
                "Blur" => crate::state::SoftPaint::Blur,
                "Revert" => crate::state::SoftPaint::Revert,
                _ => crate::state::SoftPaint::Paint,
            });
        }
    }
    if let Some((id, i)) = crate::vertex_paint::entry(state) {
        let before = state.scene.node(id).ok_or("missing node")?.modifiers.clone();
        let mut after = before.clone();
        if let Modifier::VertexPaint { brush_size, opacity, color, .. } = &mut after[i].modifier {
            brush_size.set(size);
            if let Some(o) = p.vp_opacity {
                opacity.set(o);
            }
            if let Some(c) = p.vp_color {
                for (k, v) in color.iter_mut().zip(c) {
                    k.set(v);
                }
            }
        }
        if after != before {
            commit(state, "Brush Preset", Change::SetStack { id, before, after })?;
        }
    }
    Ok(())
}

fn status(state: &AppState) -> Value {
    json!({ "presets": state.prefs.brush_presets.presets.iter().map(|p| p.name.clone()).collect::<Vec<_>>(), "active": state.active_brush_preset, "context": context(state) })
}

fn unique_name(state: &AppState, name: Option<String>, fallback: &str) -> String {
    name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| {
        let n = state.prefs.brush_presets.presets.len() + 1;
        format!("{fallback}{n:02}")
    })
}

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    match cmd {
        C::BrushPresetManager => {
            state.dialog = Some(crate::viewport::Dialog::BrushPresetManager);
            Ok(status(state))
        }
        C::AddBrushPreset { name } => {
            if state.prefs.brush_presets.presets.len() >= MAX_PRESETS {
                return Err(format!("there are already {MAX_PRESETS} brush presets"));
            }
            let n = unique_name(state, name, "Preset");
            let p = capture(state, &n);
            state.prefs.brush_presets.presets.push(p);
            state.active_brush_preset = Some(state.prefs.brush_presets.presets.len() - 1);
            Ok(status(state))
        }
        C::ApplyBrushPreset { index } => {
            let i = index as usize;
            let p = state.prefs.brush_presets.presets.get(i).cloned().ok_or("no such brush preset")?;
            // Clicking the active preset again deactivates it; its settings stay.
            if state.active_brush_preset == Some(i) {
                state.active_brush_preset = None;
                return Ok(status(state));
            }
            apply(state, &p)?;
            state.active_brush_preset = Some(i);
            Ok(status(state))
        }
        C::DuplicateBrushPreset { index, name } => {
            if state.prefs.brush_presets.presets.len() >= MAX_PRESETS {
                return Err(format!("there are already {MAX_PRESETS} brush presets"));
            }
            let mut p = state.prefs.brush_presets.presets.get(index as usize).cloned().ok_or("no such brush preset")?;
            p.name = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| format!("{} copy", p.name));
            state.prefs.brush_presets.presets.push(p);
            Ok(status(state))
        }
        C::DeleteBrushPreset { index } => {
            let i = index as usize;
            if i >= state.prefs.brush_presets.presets.len() {
                return Err("no such brush preset".into());
            }
            state.prefs.brush_presets.presets.remove(i);
            state.active_brush_preset = match state.active_brush_preset {
                Some(a) if a == i => None,
                Some(a) if a > i => Some(a - 1),
                a => a,
            };
            Ok(status(state))
        }
        C::RenameBrushPreset { index, name } => {
            if name.trim().is_empty() {
                return Err("a preset needs a name".into());
            }
            state.prefs.brush_presets.presets.get_mut(index as usize).ok_or("no such brush preset")?.name = name;
            Ok(status(state))
        }
        C::SaveBrushPresets { path } => {
            let text = serde_json::to_string_pretty(&state.prefs.brush_presets).map_err(|e| e.to_string())?;
            crate::execute::write_text(&path, &text)?;
            Ok(json!({ "path": path }))
        }
        C::LoadBrushPresets { path } => {
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
            let mut p: BrushPresets = serde_json::from_str(&text).map_err(|e| format!("{path}: not a brush preset file ({e})"))?;
            p.presets.truncate(MAX_PRESETS);
            state.prefs.brush_presets = p;
            state.active_brush_preset = None;
            Ok(status(state))
        }
        C::SetBrushIconRange { min, max } => {
            let b = &mut state.prefs.brush_presets;
            b.icon_min = min.min(max).max(0.0);
            b.icon_max = max.max(min + 0.01);
            Ok(json!({ "min": b.icon_min, "max": b.icon_max }))
        }
        _ => Err("not a Brush Presets command".into()),
    }
}

/// An active preset remembers the brush changes made while it is active.
pub fn follow_active(state: &mut AppState) {
    let Some(i) = state.active_brush_preset else { return };
    let Some(name) = state.prefs.brush_presets.presets.get(i).map(|p| p.name.clone()) else {
        state.active_brush_preset = None;
        return;
    };
    let now = capture(state, &name);
    let p = &mut state.prefs.brush_presets.presets[i];
    // Context settings only change in their own context.
    let merged = BrushPreset {
        painter: now.painter,
        deform_mode: now.deform_mode.or(p.deform_mode.clone()),
        deform_value: if state.paint_deform.is_some() { now.deform_value.or(p.deform_value) } else { p.deform_value },
        soft_mode: now.soft_mode.or(p.soft_mode.clone()),
        vp_color: if state.vertex_paint_brush { now.vp_color.or(p.vp_color) } else { p.vp_color },
        vp_opacity: if state.vertex_paint_brush { now.vp_opacity.or(p.vp_opacity) } else { p.vp_opacity },
        name,
    };
    if *p != merged {
        *p = merged;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_add_apply_toggle_follow_delete_save_load() {
        let mut s = AppState::default();
        assert_eq!(s.prefs.brush_presets.presets.len(), 5);
        s.skin_painter.size = 33.0;
        execute(&mut s, C::AddBrushPreset { name: Some("Big".into()) }).unwrap();
        assert_eq!(s.active_brush_preset, Some(5));
        execute(&mut s, C::ApplyBrushPreset { index: 0 }).unwrap();
        assert_eq!(s.skin_painter.size, 2.0);
        assert_eq!(s.prefs.paint_brush_size, 2.0, "the size in every context");
        s.skin_painter.strength = 0.4;
        follow_active(&mut s);
        assert_eq!(s.prefs.brush_presets.presets[0].painter.strength, 0.4, "the active preset remembers");
        execute(&mut s, C::ApplyBrushPreset { index: 0 }).unwrap();
        assert_eq!(s.active_brush_preset, None, "clicked again: off");
        execute(&mut s, C::ApplyBrushPreset { index: 5 }).unwrap();
        assert_eq!(s.skin_painter.size, 33.0);
        execute(&mut s, C::DuplicateBrushPreset { index: 5, name: None }).unwrap();
        assert_eq!(s.prefs.brush_presets.presets[6].name, "Big copy");
        execute(&mut s, C::DeleteBrushPreset { index: 0 }).unwrap();
        assert_eq!(s.active_brush_preset, Some(4), "the active one shifts down");
        let dir = std::env::temp_dir().join(format!("loom-bpr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("set.bpr").to_string_lossy().to_string();
        execute(&mut s, C::SaveBrushPresets { path: path.clone() }).unwrap();
        execute(&mut s, C::DeleteBrushPreset { index: 0 }).unwrap();
        execute(&mut s, C::LoadBrushPresets { path }).unwrap();
        assert_eq!(s.prefs.brush_presets.presets.len(), 6);
        while s.prefs.brush_presets.presets.len() < MAX_PRESETS {
            execute(&mut s, C::AddBrushPreset { name: None }).unwrap();
        }
        assert!(execute(&mut s, C::AddBrushPreset { name: None }).is_err(), "50 at most");
        let _ = std::fs::remove_dir_all(dir);
    }
}
