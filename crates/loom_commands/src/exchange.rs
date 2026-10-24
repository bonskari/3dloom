//! The FBX and OBJ import / export options: what the format dialogs that
//! open after File > Import / Export picks a file show, with presets. The
//! options live in `AppState::exchange` and the settings file, so the last
//! used ones are the next default (the reference keeps its "User defined"
//! state the same way).

use crate::state::SystemUnit;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// The Axis Conversion choice of the FBX exporter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UpAxis {
    #[default]
    #[serde(rename = "Y-up")]
    Y,
    #[serde(rename = "Z-up")]
    Z,
}

/// FBX Import > Include > File content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FileContent {
    #[serde(rename = "Add")]
    Add,
    #[default]
    #[serde(rename = "Add and update animation")]
    AddAndUpdateAnimation,
    #[serde(rename = "Update animation")]
    UpdateAnimation,
}

impl FileContent {
    pub const ALL: [FileContent; 3] = [FileContent::Add, FileContent::AddAndUpdateAnimation, FileContent::UpdateAnimation];
    pub fn label(self) -> &'static str {
        match self {
            FileContent::Add => "Add",
            FileContent::AddAndUpdateAnimation => "Add and update animation",
            FileContent::UpdateAnimation => "Update animation",
        }
    }
}

/// OBJ Export > Faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ObjFaces {
    Triangles,
    Quads,
    #[default]
    Polygons,
}

/// OBJ Import > Normals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ObjNormals {
    /// The file's `vn` normals decide where edges are hard (no `vn`: the smoothing groups).
    #[default]
    #[serde(rename = "Import from file")]
    FromFile,
    #[serde(rename = "From SM group")]
    FromSmoothingGroups,
    #[serde(rename = "Auto Smooth")]
    AutoSmooth,
    Faceted,
}

impl ObjNormals {
    pub const ALL: [ObjNormals; 4] = [ObjNormals::FromFile, ObjNormals::FromSmoothingGroups, ObjNormals::AutoSmooth, ObjNormals::Faceted];
    pub fn label(self) -> &'static str {
        match self {
            ObjNormals::FromFile => "Import from file",
            ObjNormals::FromSmoothingGroups => "From SM group",
            ObjNormals::AutoSmooth => "Auto Smooth",
            ObjNormals::Faceted => "Faceted",
        }
    }
}

pub const USER_DEFINED: &str = "User defined";

/// The FBX Exporter dialog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FbxExportOptions {
    pub preset: String,
    // Include > Geometry
    pub smoothing_groups: bool,
    pub split_normals: bool,
    pub tangents_and_binormals: bool,
    /// A top TurboSmooth is written as its cage plus subdivision levels.
    pub turbosmooth: bool,
    pub preserve_instances: bool,
    pub selection_sets: bool,
    pub convert_deforming_dummies_to_bones: bool,
    pub triangulate: bool,
    // Include > Animation
    pub animation: bool,
    pub use_scene_name: bool,
    pub remove_single_key: bool,
    /// On: resample every `bake_step` frames from `bake_start` to `bake_end`;
    /// off: one key per frame that holds a transform key.
    pub bake_animation: bool,
    /// Bake Start / End / Step come from the timeline, not from presets.
    #[serde(skip)]
    pub bake_start: Option<i32>,
    #[serde(skip)]
    pub bake_end: Option<i32>,
    #[serde(skip)]
    pub bake_step: Option<u32>,
    pub deformations: bool,
    pub skins: bool,
    pub morphs: bool,
    pub constant_key_reducer: bool,
    pub translation_precision: f32,
    pub rotation_precision: f32,
    pub scaling_precision: f32,
    pub other_precision: f32,
    pub key_reducer: bool,
    pub key_reducer_precision: f32,
    pub xyz_curve_sync: bool,
    // Include
    pub cameras: bool,
    pub embed_media: bool,
    // Advanced options
    /// Automatic: the file is written in the system unit; off: in `units`.
    pub units_automatic: bool,
    pub units: SystemUnit,
    pub up_axis: UpAxis,
    pub ascii: bool,
    /// 7400 (FBX 2014/2015) or 7500 (FBX 2016 and later).
    pub version: u32,
}

impl Default for FbxExportOptions {
    fn default() -> Self {
        Self {
            preset: "Media & Entertainment".into(),
            smoothing_groups: true,
            split_normals: false,
            tangents_and_binormals: false,
            turbosmooth: false,
            preserve_instances: true,
            selection_sets: false,
            convert_deforming_dummies_to_bones: true,
            triangulate: false,
            animation: true,
            use_scene_name: false,
            remove_single_key: false,
            bake_animation: true,
            bake_start: None,
            bake_end: None,
            bake_step: None,
            deformations: true,
            skins: true,
            morphs: true,
            constant_key_reducer: false,
            translation_precision: 0.0001,
            rotation_precision: 0.009,
            scaling_precision: 0.004,
            other_precision: 0.009,
            key_reducer: false,
            key_reducer_precision: 1.0,
            xyz_curve_sync: false,
            cameras: true,
            embed_media: false,
            // Centimetres by default: the file Unreal's default import reads at the right size.
            units_automatic: false,
            units: SystemUnit::Centimeters,
            up_axis: UpAxis::Y,
            ascii: false,
            version: 7400,
        }
    }
}

pub const FBX_VERSIONS: [(u32, &str); 2] = [(7400, "FBX 2014/2015 (7.4)"), (7500, "FBX 2016 and later (7.5)")];

/// The FBX Importer dialog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FbxImportOptions {
    pub preset: String,
    pub file_content: FileContent,
    pub smoothing_groups: bool,
    pub animation: bool,
    /// The take to import (None: the file's first).
    #[serde(skip)]
    pub take: Option<String>,
    pub fill_timeline: bool,
    pub bake_animation_layers: bool,
    pub deformations: bool,
    pub skins: bool,
    pub morphs: bool,
    pub constant_key_reducer: bool,
    pub translation_precision: f32,
    pub rotation_precision: f32,
    pub scaling_precision: f32,
    pub other_precision: f32,
    pub key_reducer: bool,
    pub key_reducer_precision: f32,
    pub xyz_curve_sync: bool,
    /// Bone creation: Convert as Dummy instead of Leave as Bones.
    pub bones_as_dummies: bool,
    /// Bone Width / Height / Taper, used where the file gives no bone size.
    pub bone_width: f32,
    pub bone_height: f32,
    pub bone_taper: f32,
    pub lock_width_height: bool,
    pub cameras: bool,
    /// Automatic: converted to the system unit; off: the file's numbers are read as `units`.
    pub units_automatic: bool,
    pub units: SystemUnit,
}

impl Default for FbxImportOptions {
    fn default() -> Self {
        Self {
            preset: "Media & Entertainment".into(),
            file_content: FileContent::AddAndUpdateAnimation,
            smoothing_groups: true,
            animation: true,
            take: None,
            fill_timeline: true,
            bake_animation_layers: true,
            deformations: true,
            skins: true,
            morphs: true,
            constant_key_reducer: false,
            translation_precision: 0.0001,
            rotation_precision: 0.009,
            scaling_precision: 0.004,
            other_precision: 0.009,
            key_reducer: false,
            key_reducer_precision: 1.0,
            xyz_curve_sync: false,
            bones_as_dummies: false,
            bone_width: 1.0,
            bone_height: 1.0,
            bone_taper: 90.0,
            lock_width_height: true,
            cameras: true,
            units_automatic: true,
            units: SystemUnit::Inches,
        }
    }
}

/// The OBJ Exporter dialog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ObjExportOptions {
    pub preset: String,
    pub flip_yz: bool,
    pub hidden_objects: bool,
    pub faces: ObjFaces,
    pub texture_coordinates: bool,
    pub normals: bool,
    pub smoothing_groups: bool,
    pub scale: f32,
    pub use_material: bool,
    pub create_mat_library: bool,
    pub force_black_ambient: bool,
    /// Windows (CR LF) or Unix (LF) line ends.
    pub target_windows: bool,
    pub relative_numbers: bool,
    /// Decimal places of the vertex data.
    pub precision: u32,
}

impl Default for ObjExportOptions {
    fn default() -> Self {
        Self {
            preset: "Default".into(),
            flip_yz: false,
            hidden_objects: false,
            faces: ObjFaces::Polygons,
            texture_coordinates: true,
            normals: true,
            smoothing_groups: true,
            scale: 1.0,
            use_material: true,
            create_mat_library: true,
            force_black_ambient: false,
            target_windows: false,
            relative_numbers: false,
            precision: 6,
        }
    }
}

/// The OBJ Importer dialog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ObjImportOptions {
    pub preset: String,
    pub reset_scene: bool,
    /// Prepended to each object name.
    pub prefix: String,
    pub single_mesh: bool,
    pub retriangulate: bool,
    /// The objects to import (the dialog's checked list); None = all.
    #[serde(skip)]
    pub objects: Option<Vec<String>>,
    pub flip_zy: bool,
    pub center_pivots: bool,
    pub texture_coordinates: bool,
    pub smoothing_groups: bool,
    pub normals: ObjNormals,
    pub auto_smooth_angle: f32,
    pub flip_normals: bool,
    /// Convert: the file's numbers are in `model_units`; off: scaled by `object_scale`.
    pub convert_units: bool,
    pub model_units: SystemUnit,
    pub object_scale: f32,
    pub unique_wire_color: bool,
    pub import_materials: bool,
    pub material_prefix: String,
    /// Bump Amount where the MTL gives none.
    pub default_bump: f32,
    pub force_black_ambient: bool,
    /// Put the imported materials into Material Editor slots.
    pub import_into_mat_editor: bool,
    pub show_maps_in_viewport: bool,
}

impl Default for ObjImportOptions {
    fn default() -> Self {
        Self {
            preset: "Default".into(),
            reset_scene: false,
            prefix: String::new(),
            single_mesh: false,
            retriangulate: false,
            objects: None,
            flip_zy: false,
            center_pivots: false,
            texture_coordinates: true,
            smoothing_groups: true,
            normals: ObjNormals::FromFile,
            auto_smooth_angle: 30.0,
            flip_normals: false,
            convert_units: false,
            model_units: SystemUnit::Centimeters,
            object_scale: 1.0,
            unique_wire_color: true,
            import_materials: true,
            material_prefix: String::new(),
            default_bump: 30.0,
            force_black_ambient: false,
            import_into_mat_editor: true,
            show_maps_in_viewport: true,
        }
    }
}

/// All four dialogs plus the user's saved presets (dialog → name → options).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ExchangeOptions {
    pub fbx_export: FbxExportOptions,
    pub fbx_import: FbxImportOptions,
    pub obj_export: ObjExportOptions,
    pub obj_import: ObjImportOptions,
    pub user_presets: BTreeMap<String, BTreeMap<String, Value>>,
}

/// The dialog keys `SetExchangeOption` takes.
pub const DIALOGS: [&str; 4] = ["fbx_export", "fbx_import", "obj_export", "obj_import"];

fn dialog_key(dialog: &str) -> Result<&'static str, String> {
    let d = dialog.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    DIALOGS.into_iter().find(|k| *k == d).ok_or_else(|| format!("unknown options dialog {dialog:?} ({})", DIALOGS.join(", ")))
}

/// The built-in presets of a dialog: (name, options as JSON).
pub fn builtin_presets(dialog: &str) -> Vec<(String, Value)> {
    let to = |v: &impl Serialize| serde_json::to_value(v).expect("plain data");
    match dialog {
        "fbx_export" => vec![("Media & Entertainment".into(), to(&FbxExportOptions::default()))],
        "fbx_import" => {
            // Architectural: geometry only, smoothing from the normals, added to the scene.
            let arch = FbxImportOptions { preset: "Architectural".into(), file_content: FileContent::Add, smoothing_groups: false, animation: false, ..Default::default() };
            vec![("Media & Entertainment".into(), to(&FbxImportOptions::default())), ("Architectural".into(), to(&arch))]
        }
        "obj_export" => vec![("Default".into(), to(&ObjExportOptions::default()))],
        "obj_import" => vec![("Default".into(), to(&ObjImportOptions::default()))],
        _ => Vec::new(),
    }
}

impl ExchangeOptions {
    fn get(&self, key: &str) -> Value {
        match key {
            "fbx_export" => serde_json::to_value(&self.fbx_export),
            "fbx_import" => serde_json::to_value(&self.fbx_import),
            "obj_export" => serde_json::to_value(&self.obj_export),
            _ => serde_json::to_value(&self.obj_import),
        }
        .expect("plain data")
    }

    /// Replaces one dialog's options from JSON, keeping the session-only fields.
    fn put(&mut self, key: &str, v: Value) -> Result<(), String> {
        let bad = |e: serde_json::Error| format!("bad option value: {e}");
        match key {
            "fbx_export" => {
                let keep = (self.fbx_export.bake_start, self.fbx_export.bake_end, self.fbx_export.bake_step);
                self.fbx_export = serde_json::from_value(v).map_err(bad)?;
                (self.fbx_export.bake_start, self.fbx_export.bake_end, self.fbx_export.bake_step) = keep;
            }
            "fbx_import" => {
                let take = self.fbx_import.take.take();
                self.fbx_import = serde_json::from_value(v).map_err(bad)?;
                self.fbx_import.take = take;
            }
            "obj_export" => self.obj_export = serde_json::from_value(v).map_err(bad)?,
            _ => {
                let objects = self.obj_import.objects.take();
                self.obj_import = serde_json::from_value(v).map_err(bad)?;
                self.obj_import.objects = objects;
            }
        }
        Ok(())
    }

    /// The dialog's options as JSON, with the session-only fields.
    pub fn describe(&self, dialog: &str) -> Result<Value, String> {
        let key = dialog_key(dialog)?;
        let mut v = self.get(key);
        let o = v.as_object_mut().expect("an object");
        match key {
            "fbx_export" => {
                o.insert("bake_start".into(), json!(self.fbx_export.bake_start));
                o.insert("bake_end".into(), json!(self.fbx_export.bake_end));
                o.insert("bake_step".into(), json!(self.fbx_export.bake_step));
            }
            "fbx_import" => {
                o.insert("take".into(), json!(self.fbx_import.take));
            }
            "obj_import" => {
                o.insert("objects".into(), json!(self.obj_import.objects));
            }
            _ => {}
        }
        let mut presets: Vec<String> = builtin_presets(key).into_iter().map(|p| p.0).collect();
        presets.extend(self.user_presets.get(key).into_iter().flat_map(|m| m.keys().cloned()));
        Ok(json!({ "dialog": key, "options": v, "presets": presets }))
    }

    /// Sets one option by its field name. `preset` loads a preset; any other
    /// change makes the preset "User defined".
    pub fn set(&mut self, dialog: &str, name: &str, value: Value) -> Result<(), String> {
        let key = dialog_key(dialog)?;
        if name == "preset" {
            let wanted = value.as_str().ok_or("preset: a preset name")?.to_owned();
            let found = builtin_presets(key).into_iter().find(|p| p.0 == wanted).map(|p| p.1).or_else(|| self.user_presets.get(key).and_then(|m| m.get(&wanted)).cloned());
            let mut v = found.ok_or_else(|| format!("no preset {wanted:?}"))?;
            v["preset"] = json!(wanted);
            return self.put(key, v);
        }
        match (key, name) {
            ("fbx_export", "bake_start") => self.fbx_export.bake_start = opt_int(&value)?.map(|x| x as i32),
            ("fbx_export", "bake_end") => self.fbx_export.bake_end = opt_int(&value)?.map(|x| x as i32),
            ("fbx_export", "bake_step") => self.fbx_export.bake_step = opt_int(&value)?.map(|x| x.max(1) as u32),
            ("fbx_import", "take") => self.fbx_import.take = value.as_str().map(str::to_owned),
            ("obj_import", "objects") => self.obj_import.objects = if value.is_null() { None } else { Some(serde_json::from_value(value).map_err(|e| format!("objects: a list of names ({e})"))?) },
            _ => {
                let mut v = self.get(key);
                let o = v.as_object_mut().expect("an object");
                if !o.contains_key(name) || name == "preset" {
                    let names: Vec<&String> = o.keys().filter(|k| *k != "preset").collect();
                    return Err(format!("{key} has no option {name:?} ({})", names.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
                }
                o.insert(name.to_owned(), value);
                o.insert("preset".into(), json!(USER_DEFINED));
                self.put(key, v)?;
            }
        }
        // The session-only fields are not part of a preset either.
        Ok(())
    }

    /// Saves the dialog's current options as a user preset.
    pub fn save_preset(&mut self, dialog: &str, name: &str) -> Result<(), String> {
        let key = dialog_key(dialog)?;
        let name = name.trim();
        if name.is_empty() || name == USER_DEFINED || builtin_presets(key).iter().any(|p| p.0 == name) {
            return Err(format!("preset name {name:?} is taken"));
        }
        let mut v = self.get(key);
        v["preset"] = json!(name);
        self.user_presets.entry(key.into()).or_default().insert(name.into(), v.clone());
        self.put(key, v)
    }

    pub fn delete_preset(&mut self, dialog: &str, name: &str) -> Result<(), String> {
        let key = dialog_key(dialog)?;
        self.user_presets.get_mut(key).and_then(|m| m.remove(name)).map(|_| ()).ok_or_else(|| format!("no user preset {name:?}"))
    }
}

fn opt_int(v: &Value) -> Result<Option<i64>, String> {
    if v.is_null() {
        return Ok(None);
    }
    v.as_f64().map(|x| Some(x.round() as i64)).ok_or_else(|| "a number or null".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_an_option_makes_the_preset_user_defined_and_presets_load_back() {
        let mut x = ExchangeOptions::default();
        assert_eq!(x.fbx_export.preset, "Media & Entertainment");
        x.set("fbx_export", "triangulate", json!(true)).unwrap();
        assert!(x.fbx_export.triangulate);
        assert_eq!(x.fbx_export.preset, USER_DEFINED);
        x.save_preset("FBX Export", "Game").unwrap();
        x.set("fbx_export", "preset", json!("Media & Entertainment")).unwrap();
        assert!(!x.fbx_export.triangulate);
        x.set("fbx_export", "preset", json!("Game")).unwrap();
        assert!(x.fbx_export.triangulate && x.fbx_export.preset == "Game");
        assert!(x.set("fbx_export", "nonsense", json!(1)).unwrap_err().contains("triangulate"));
        assert!(x.set("fbx_export", "triangulate", json!("yes")).is_err());
        x.set("fbx_export", "bake_step", json!(2)).unwrap();
        x.set("fbx_export", "preset", json!("Media & Entertainment")).unwrap();
        assert_eq!(x.fbx_export.bake_step, Some(2), "bake range is not part of a preset");
        x.set("fbx_import", "preset", json!("Architectural")).unwrap();
        assert!(!x.fbx_import.animation && x.fbx_import.file_content == FileContent::Add);
    }
}
