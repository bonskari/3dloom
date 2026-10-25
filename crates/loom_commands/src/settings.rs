//! The user's settings file, `settings.json` in the config dir: what the
//! reference keeps in its ini, .kbd and startup UI files. Preferences, Units
//! Setup, the Show UI toggles, keyboard shortcut edits, recent files and the
//! last file-dialog folder. The UI loads it at startup and writes it when
//! something in it changes. Viewport layout is not here: as in the
//! reference it is saved with the scene.

use crate::keymap::GroupRows;
use crate::state::{AppState, Prefs, UnitsSetup};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// 3dLoom's per-user config folder: `$LOOM_CONFIG_DIR` (tests and agents
/// point it elsewhere), else `%APPDATA%\3dloom`, `$XDG_CONFIG_HOME/3dloom`
/// or `~/.config/3dloom`.
pub fn config_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("LOOM_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(d));
    }
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("3dloom"))
}

pub fn settings_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("settings.json"))
}

/// Customize > Show UI toggles (what a UI scheme and Revert to Startup Layout carry).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiLayout {
    pub show_main_toolbar: bool,
    pub show_ribbon: bool,
    pub show_command_panel: bool,
    pub show_track_bar: bool,
    pub show_layers_toolbar: bool,
}

impl UiLayout {
    /// A fresh `AppState`'s toggles.
    pub const DEFAULT: UiLayout = UiLayout { show_main_toolbar: true, show_ribbon: true, show_command_panel: true, show_track_bar: true, show_layers_toolbar: false };

    pub fn of(state: &AppState) -> Self {
        UiLayout { show_main_toolbar: state.show_main_toolbar, show_ribbon: state.show_ribbon, show_command_panel: state.show_command_panel, show_track_bar: state.show_track_bar, show_layers_toolbar: state.show_layers_toolbar }
    }

    pub fn apply(&self, state: &mut AppState) {
        state.show_main_toolbar = self.show_main_toolbar;
        state.show_ribbon = self.show_ribbon;
        state.show_command_panel = self.show_command_panel;
        state.show_track_bar = self.show_track_bar;
        state.show_layers_toolbar = self.show_layers_toolbar;
    }
}

impl Default for UiLayout {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub prefs: Prefs,
    pub units: UnitsSetup,
    pub ui: UiLayout,
    /// Edited shortcut groups (whole groups; unedited ones stay default).
    pub shortcuts: BTreeMap<String, GroupRows>,
    pub recent_files: Vec<PathBuf>,
    /// The folder the file dialogs opened last (the UI fills it in).
    pub last_folder: Option<PathBuf>,
    /// File > Set Project Folder.
    pub project_folder: Option<PathBuf>,
    /// FBX / OBJ import and export options and saved presets.
    pub exchange: crate::exchange::ExchangeOptions,
}

impl Default for Settings {
    fn default() -> Self {
        Settings::capture(&AppState::new())
    }
}

impl Settings {
    pub fn capture(state: &AppState) -> Self {
        Settings {
            prefs: state.prefs.clone(),
            units: state.default_units.clone(),
            ui: UiLayout::of(state),
            shortcuts: state.custom_keys.edited_rows(),
            recent_files: state.recent_files.clone(),
            last_folder: None,
            exchange: state.exchange.clone(),
            project_folder: state.project_folder.clone(),
        }
    }

    /// Puts the settings into `state`. A shortcut group the file gets wrong
    /// (unknown action or chord) keeps its defaults.
    pub fn apply(&self, state: &mut AppState) {
        state.prefs = self.prefs.clone();
        state.undo.set_limit(state.prefs.undo_levels);
        state.units = self.units.clone();
        state.default_units = self.units.clone();
        self.ui.apply(state);
        state.startup_ui = self.ui;
        state.custom_keys.reset();
        for (group, rows) in &self.shortcuts {
            let one = BTreeMap::from([(group.clone(), rows.clone())]);
            let _ = state.custom_keys.set_rows(&one);
        }
        state.recent_files = self.recent_files.clone();
        state.recent_files.truncate(state.prefs.recent_files_count);
        state.exchange = self.exchange.clone();
        state.project_folder = self.project_folder.clone();
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("plain data")
    }

    /// Missing fields take their defaults, so older files still load.
    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| format!("settings: {e}"))
    }
}

/// Customize > Save / Load Custom UI Scheme: a `.ui` file with the Show UI
/// toggles and every shortcut group.
#[derive(Serialize, Deserialize)]
struct UiScheme {
    format: String,
    ui: UiLayout,
    shortcuts: BTreeMap<String, GroupRows>,
}

const SCHEME_FORMAT: &str = "3dloom-ui-scheme-1";

pub fn scheme_to_text(state: &AppState) -> String {
    serde_json::to_string_pretty(&UiScheme { format: SCHEME_FORMAT.into(), ui: UiLayout::of(state), shortcuts: state.custom_keys.all_rows() }).expect("plain data")
}

pub fn apply_scheme_text(state: &mut AppState, text: &str) -> Result<(), String> {
    let s: UiScheme = serde_json::from_str(text).map_err(|e| format!("not a 3dLoom UI scheme: {e}"))?;
    if s.format != SCHEME_FORMAT {
        return Err(format!("unknown UI scheme format {:?}", s.format));
    }
    let mut keys = state.custom_keys.clone();
    keys.set_rows(&s.shortcuts)?;
    state.custom_keys = keys;
    s.ui.apply(state);
    Ok(())
}

/// Puts `path` first in File > Recent (once), keeping the preference's count.
pub fn note_recent(state: &mut AppState, path: &std::path::Path) {
    let path = path.to_path_buf();
    state.recent_files.retain(|p| *p != path);
    state.recent_files.insert(0, path);
    state.recent_files.truncate(state.prefs.recent_files_count);
}
