//! Customize > Configure User Paths: the File I/O folders the file dialogs
//! and Auto Backup start in, and the External Files folders where a bitmap
//! that is not at its path is looked for. Saved in the settings file.

use crate::command::Command as C;
use crate::AppState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The File I/O entries, in the panel's order.
pub const FILE_IO: [&str; 8] = ["Animations", "Archives", "Auto Backup", "Export", "Import", "Render Output", "Scenes", "Viewport Captures"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct UserPaths {
    /// File I/O folders by entry name (unset: the program's choice).
    pub file_io: BTreeMap<String, PathBuf>,
    /// External Files: bitmap search folders, in search order.
    pub bitmaps: Vec<PathBuf>,
}

impl UserPaths {
    /// The folder of a File I/O entry, if set and present.
    pub fn dir(&self, entry: &str) -> Option<&Path> {
        self.file_io.get(entry).map(PathBuf::as_path).filter(|p| p.is_dir())
    }

    /// Where a bitmap is: its own path when the file is there, else the first
    /// External Files folder holding a file of that name.
    pub fn resolve_bitmap(&self, path: &str) -> Option<PathBuf> {
        let p = Path::new(path);
        if p.is_file() {
            return Some(p.to_path_buf());
        }
        let name = p.file_name()?;
        self.bitmaps.iter().map(|d| d.join(name)).find(|c| c.is_file())
    }
}

pub fn handles(c: &C) -> bool {
    matches!(c, C::ConfigureUserPaths | C::SetUserPath { .. } | C::SetBitmapPaths { .. })
}

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    match cmd {
        C::ConfigureUserPaths => {
            state.dialog = Some(crate::viewport::Dialog::ConfigurePaths);
            Ok(json!({}))
        }
        C::SetUserPath { name, path } => {
            if !FILE_IO.contains(&name.as_str()) {
                return Err(format!("unknown File I/O entry {name:?} ({})", FILE_IO.join(", ")));
            }
            let path = path.trim();
            if path.is_empty() {
                state.paths.file_io.remove(&name);
            } else {
                let p = PathBuf::from(path);
                if !p.is_dir() {
                    return Err(format!("{path}: not a folder"));
                }
                state.paths.file_io.insert(name.clone(), p);
            }
            Ok(json!({ "name": name, "path": state.paths.file_io.get(&name) }))
        }
        C::SetBitmapPaths { paths } => {
            let mut list: Vec<PathBuf> = Vec::new();
            for p in paths.iter().map(|p| PathBuf::from(p.trim())) {
                if !p.is_dir() {
                    return Err(format!("{}: not a folder", p.display()));
                }
                if !list.contains(&p) {
                    list.push(p);
                }
            }
            state.paths.bitmaps = list;
            state.bitmap_generation += 1;
            state.mesh_dirty = true;
            Ok(json!({ "bitmaps": state.paths.bitmaps }))
        }
        other => Err(format!("{} is not a Configure User Paths command", other.name())),
    }
}
