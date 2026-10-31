//! Customize > Custom UI and Defaults Switcher: the defaults set for tool
//! options and the UI scheme 3dLoom uses. UI schemes: Default UI, Modular
//! Toolbars UI (the hidden toolbars docked under the main toolbar) and every
//! .ui file in the config folder's `ui` folder (Save Custom UI Scheme puts
//! them there). Defaults sets: Default (the program's preferences and units)
//! and every set saved to the config folder's `defaults` folder.

use crate::command::Command as C;
use crate::AppState;
use serde_json::{json, Value};
use std::path::PathBuf;

pub const BUILTIN_SCHEMES: [&str; 2] = ["Default UI", "Modular Toolbars UI"];

/// The folder of the user's UI schemes.
pub fn schemes_dir() -> Option<PathBuf> {
    crate::settings::config_dir().map(|d| d.join("ui"))
}

/// The folder of the user's defaults sets.
pub fn defaults_dir() -> Option<PathBuf> {
    crate::settings::config_dir().map(|d| d.join("defaults"))
}

fn names_in(dir: Option<PathBuf>, ext: &str) -> Vec<String> {
    let Some(dir) = dir else { return Vec::new() };
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|r| r.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == ext)).filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned())).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// Every UI scheme the switcher lists.
pub fn schemes() -> Vec<String> {
    BUILTIN_SCHEMES.iter().map(|s| (*s).to_owned()).chain(names_in(schemes_dir(), "ui")).collect()
}

/// Every defaults set the switcher lists.
pub fn defaults_sets() -> Vec<String> {
    std::iter::once("Default".to_owned()).chain(names_in(defaults_dir(), "json")).collect()
}

/// A defaults set file: preferences and units.
#[derive(serde::Serialize, serde::Deserialize)]
struct DefaultsSet {
    format: String,
    prefs: crate::state::Prefs,
    units: crate::state::UnitsSetup,
}

const DEFAULTS_FORMAT: &str = "3dloom-defaults-1";

pub fn handles(c: &C) -> bool {
    matches!(c, C::GrabViewport | C::CreateAnimatedSequenceFile | C::MakePreview { .. } | C::CustomUiDefaultsSwitcher | C::SwitchUiScheme { .. } | C::SwitchDefaults { .. } | C::SaveDefaultsSet { .. })
}

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    match cmd {
        C::GrabViewport => {
            // The UI takes the picture (the command layer has no screen).
            state.grab_request = Some(state.active_viewport);
            Ok(json!({ "viewport": state.active_viewport }))
        }
        C::CreateAnimatedSequenceFile => {
            state.dialog = Some(crate::viewport::Dialog::MakePreview);
            Ok(json!({ "dialog": "MakePreview", "start": state.frame_range.0, "end": state.frame_range.1 }))
        }
        C::MakePreview { start, end, every_nth, percent, path } => {
            let s = start.unwrap_or(state.frame_range.0);
            let e = end.unwrap_or(state.frame_range.1).max(s);
            let step = every_nth.unwrap_or(1).max(1) as usize;
            let pct = percent.unwrap_or(50).clamp(1, 100);
            let w = (state.scene.render.width * pct / 100).max(1);
            let h = (state.scene.render.height * pct / 100).max(1);
            let frames: Vec<i32> = (s..=e).step_by(step).collect();
            let out = path.unwrap_or_else(|| crate::autosave::loom_home().join("previews").join("_scene.avi").to_string_lossy().into_owned());
            state.grab_request = Some(state.active_viewport);
            state.status_prompt = format!("Preview: {} frame(s) ({w}x{h}) -> {out}", frames.len());
            Ok(json!({ "frames": frames, "width": w, "height": h, "path": out }))
        }
        C::CustomUiDefaultsSwitcher => {
            state.dialog = Some(crate::viewport::Dialog::Switcher);
            Ok(json!({ "schemes": schemes(), "defaults": defaults_sets() }))
        }
        C::SwitchUiScheme { name } => {
            match name.as_str() {
                "Default UI" => {
                    crate::execute::execute(state, C::ResetToolbars)?;
                    crate::execute::execute(state, C::ResetMenus)?;
                    crate::execute::execute(state, C::ResetQuadOptions)?;
                    crate::execute::execute(state, C::ResetUiColors)?;
                    state.ui_custom.rollout_order.clear();
                    crate::settings::UiLayout::DEFAULT.apply(state);
                }
                "Modular Toolbars UI" => {
                    for t in ["Axis Constraints", "Layers", "Extras", "Snaps"] {
                        crate::execute::execute(state, C::DockToolbar { name: t.into(), dock: "Top".into(), pos: None })?;
                    }
                }
                other => {
                    let path = schemes_dir().map(|d| d.join(format!("{other}.ui"))).filter(|p| p.is_file()).ok_or_else(|| format!("no UI scheme {other:?}"))?;
                    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                    crate::settings::apply_scheme_text(state, &text)?;
                }
            }
            Ok(json!({ "scheme": name }))
        }
        C::SwitchDefaults { name } => {
            let (prefs, units) = if name == "Default" {
                (crate::state::Prefs::default(), crate::state::UnitsSetup::default())
            } else {
                let path = defaults_dir().map(|d| d.join(format!("{name}.json"))).filter(|p| p.is_file()).ok_or_else(|| format!("no defaults set {name:?}"))?;
                let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                let d: DefaultsSet = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
                if d.format != DEFAULTS_FORMAT {
                    return Err(format!("{}: not a defaults set", path.display()));
                }
                (d.prefs, d.units)
            };
            state.prefs = prefs;
            state.undo.set_limit(state.prefs.undo_levels);
            state.units = units.clone();
            state.default_units = units;
            Ok(json!({ "defaults": name }))
        }
        C::SaveDefaultsSet { name } => {
            let name = name.trim().to_owned();
            if name.is_empty() || name == "Default" || name.contains(['/', '\\']) {
                return Err(format!("cannot save a defaults set named {name:?}"));
            }
            let dir = defaults_dir().ok_or("no config folder")?;
            let d = DefaultsSet { format: DEFAULTS_FORMAT.into(), prefs: state.prefs.clone(), units: state.default_units.clone() };
            let path = dir.join(format!("{name}.json"));
            crate::execute::write_text(&path.to_string_lossy(), &serde_json::to_string_pretty(&d).expect("plain data"))?;
            Ok(json!({ "path": path }))
        }
        other => Err(format!("{} is not a switcher command", other.name())),
    }
}
