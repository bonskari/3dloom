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
    matches!(c, C::GrabViewport | C::CreateAnimatedSequenceFile | C::MakePreview { .. } | C::ViewAnimatedSequenceFile | C::RenameAnimatedSequenceFile { .. } | C::CustomUiDefaultsSwitcher | C::SwitchUiScheme { .. } | C::SwitchDefaults { .. } | C::SaveDefaultsSet { .. })
}

/// The preview's file name (as the reference's _scene.avi) and its folder.
const PREVIEW_NAME: &str = "_scene";

fn preview_dir() -> std::path::PathBuf {
    crate::autosave::loom_home().join("previews")
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
            let step = every_nth.unwrap_or(1).max(1);
            let pct = percent.unwrap_or(50).clamp(1, 200);
            let w = (state.scene.render.width * pct / 100).max(1);
            let h = (state.scene.render.height * pct / 100).max(1);
            // The preview is a numbered image sequence _sceneNNNN.png in the
            // folder, listed by _scene.ifl (no video encoder here).
            let dir = path.map(std::path::PathBuf::from).unwrap_or_else(preview_dir);
            std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            let out = dir.join(format!("{PREVIEW_NAME}.png")).to_string_lossy().into_owned();
            let mut settings = state.scene.render.clone();
            settings.width = w;
            settings.height = h;
            settings.area = loom_scene::render::RenderArea::View;
            settings.time_output = loom_scene::render::TimeOutput::Range;
            settings.range = [s, e];
            settings.every_nth = step;
            settings.file_number_base = 0;
            settings.image_file_list = true;
            settings.skip_existing = false;
            settings.elements_active = false;
            // A quick look: one pass, no file options or gamma of the output.
            settings.bevy.passes = 1;
            settings.output_format = Default::default();
            settings.gamma = Default::default();
            let frames = settings.output_frames(state.frame_range, state.current_frame)?;
            let req = crate::state::RenderRequest { viewport: state.active_viewport, settings, save_path: Some(out.clone()), ..Default::default() };
            crate::render::start_sequence(state, req, frames.clone())?;
            let ifl = crate::render::image_file_list_path(&out);
            state.preview_file = Some(ifl.clone());
            state.status_prompt = format!("Preview: {} frame(s) ({w}x{h}) -> {ifl}", frames.len());
            Ok(json!({ "frames": frames, "width": w, "height": h, "path": ifl }))
        }
        C::ViewAnimatedSequenceFile => {
            let ifl = state.preview_file.clone().unwrap_or_else(|| crate::render::image_file_list_path(&preview_dir().join(format!("{PREVIEW_NAME}.png")).to_string_lossy()));
            if !std::path::Path::new(&ifl).is_file() {
                return Err("no preview: make one first (Create Animated Sequence File)".into());
            }
            crate::execute::execute(state, C::ViewImageFile { path: Some(ifl), sequence: None })
        }
        C::RenameAnimatedSequenceFile { name } => {
            let ifl = state.preview_file.clone().ok_or("no preview to rename: make one first (Create Animated Sequence File)")?;
            let ifl = std::path::PathBuf::from(ifl);
            let dir = ifl.parent().map(|d| d.to_path_buf()).unwrap_or_default();
            let name = name.trim().trim_end_matches(".ifl").to_owned();
            if name.is_empty() || name.contains(['/', '\\']) {
                return Err(format!("preview name {name:?}"));
            }
            // Each frame file and the list take the new name; the list then names the new files.
            let text = std::fs::read_to_string(&ifl).map_err(|e| format!("{}: {e}", ifl.display()))?;
            let mut lines = Vec::new();
            for l in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
                let renamed = format!("{name}{}", l.strip_prefix(PREVIEW_NAME).unwrap_or(l));
                std::fs::rename(dir.join(l), dir.join(&renamed)).map_err(|e| format!("{l}: {e}"))?;
                lines.push(renamed);
            }
            let to = dir.join(format!("{name}.ifl"));
            std::fs::write(&to, lines.join("\n") + "\n").map_err(|e| format!("{}: {e}", to.display()))?;
            let _ = std::fs::remove_file(&ifl);
            state.preview_file = None;
            Ok(json!({ "path": to, "files": lines.len() }))
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
