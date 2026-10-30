//! The `.loom` file: the scene plus the settings kept in a scene file.
//! See `docs/LOOM-FORMAT.md`.

use crate::state::{AppState, SubObjectId, ViewKind};
use crate::viewport::{Shading, Viewport, ViewportLayout};
use crate::UnitsSetup;
use glam::Vec3;
use loom_scene::{Change, NodeId, Scene};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

/// Bumped when a change cannot be read by older programs.
pub const FORMAT: u32 = 1;

/// One viewport as stored (no undo history, no reload counter).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewportSave {
    pub kind: ViewKind,
    pub target: [f32; 3],
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    #[serde(default)]
    pub roll: f32,
    pub shading: Shading,
    pub edged_faces: bool,
    pub show_grid: bool,
    pub disabled: bool,
    pub safe_frames: bool,
    #[serde(default)]
    pub clipping: bool,
    #[serde(default = "default_clip")]
    pub clip: (f32, f32),
    pub fov_y_radians: f32,
    pub scene_lights: bool,
    #[serde(default = "one")]
    pub default_lights: u32,
    pub background: Option<String>,
    pub background_locked: bool,
    #[serde(default)]
    pub background_anchor: Option<([f32; 3], f32)>,
    #[serde(default)]
    pub background_aspect: crate::viewport::BackgroundAspect,
    #[serde(default = "yes")]
    pub background_display: bool,
    #[serde(default)]
    pub transparency: crate::viewport::Transparency,
    #[serde(default)]
    pub disable_textures: bool,
    #[serde(default)]
    pub force_two_sided: bool,
    #[serde(default)]
    pub background_use_environment: bool,
}

fn yes() -> bool {
    true
}

fn one() -> u32 {
    1
}

fn default_clip() -> (f32, f32) {
    (0.0, 1000.0)
}

impl ViewportSave {
    pub(crate) fn from_viewport(v: &Viewport) -> Self {
        Self {
            kind: v.kind,
            target: v.camera.target.to_array(),
            distance: v.camera.distance,
            yaw: v.camera.yaw,
            pitch: v.camera.pitch,
            roll: v.camera.roll,
            shading: v.shading,
            edged_faces: v.edged_faces,
            show_grid: v.show_grid,
            disabled: v.disabled,
            safe_frames: v.safe_frames,
            clipping: v.clipping,
            clip: v.clip,
            fov_y_radians: v.fov_y_radians,
            scene_lights: v.scene_lights,
            default_lights: v.default_lights,
            background: v.background.clone(),
            background_locked: v.background_locked,
            background_anchor: v.background_anchor.map(|(t, d)| (t.to_array(), d)),
            background_aspect: v.background_aspect,
            background_display: v.background_display,
            transparency: v.transparency,
            disable_textures: v.disable_textures,
            force_two_sided: v.force_two_sided,
            background_use_environment: v.background_use_environment,
        }
    }

    pub(crate) fn to_viewport(&self) -> Viewport {
        let mut v = Viewport::new(self.kind);
        v.camera.target = Vec3::from_array(self.target);
        v.camera.distance = self.distance;
        v.camera.yaw = self.yaw;
        v.camera.pitch = self.pitch;
        v.camera.roll = self.roll;
        v.shading = self.shading;
        v.edged_faces = self.edged_faces;
        v.show_grid = self.show_grid;
        v.disabled = self.disabled;
        v.safe_frames = self.safe_frames;
        v.clipping = self.clipping;
        v.clip = self.clip;
        v.fov_y_radians = self.fov_y_radians;
        v.scene_lights = self.scene_lights;
        v.default_lights = self.default_lights;
        v.background = self.background.clone();
        v.background_locked = self.background_locked;
        // A file from before the anchor: locked where its view is.
        v.background_anchor = self.background_locked.then(|| self.background_anchor.map(|(t, d)| (Vec3::from_array(t), d)).unwrap_or((v.camera.target, v.camera.distance)));
        v.background_aspect = self.background_aspect;
        v.background_display = self.background_display;
        v.transparency = self.transparency;
        v.disable_textures = self.disable_textures;
        v.force_two_sided = self.force_two_sided;
        v.background_use_environment = self.background_use_environment;
        v
    }
}

/// What the scene file stores besides the objects. Missing keys
/// take a new scene's values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SceneSettings {
    pub layout: ViewportLayout,
    pub viewports: Vec<ViewportSave>,
    pub active_viewport: usize,
    pub units: UnitsSetup,
    pub named_sets: BTreeMap<String, Vec<NodeId>>,
    pub named_sub_selections: BTreeMap<String, Vec<(NodeId, SubObjectId)>>,
    pub selection: Vec<NodeId>,
    pub frame_range: (i32, i32),
    pub fps: f32,
    pub current_frame: i32,
    /// Time Configuration > Playback.
    pub real_time: bool,
    pub loop_playback: bool,
    pub play_speed: f32,
    pub play_direction: crate::anim_cmds::PlayDirection,
    /// Default In / Out tangents for new keys.
    pub default_tangent: (loom_scene::anim::TangentType, loom_scene::anim::TangentType),
    /// Display Color, Hide Frozen / Bone Objects, statistics, safe frames, Object Color options.
    pub display: crate::display::DisplaySettings,
    /// The ViewCube Home view.
    pub home_view: Option<crate::state::SavedView>,
    /// Views > Save Active View slots.
    pub saved_views: BTreeMap<String, crate::state::SavedView>,
    /// File > File Properties and the Summary Info description.
    pub file_properties: FileProperties,
    /// The status bar's time tags.
    pub time_tags: Vec<crate::time_tags::TimeTag>,
    /// Viewport Configuration > Adaptive Degradation.
    pub degradation: crate::viewport::Degradation,
    /// Time Configuration > Time Display and Key Steps.
    pub time_config: crate::time_display::TimeConfig,
    /// The Scene Explorers (Tools menu), open or closed.
    pub scene_explorers: crate::explorer::Explorers,
    /// The Schematic Views (Graph Editors menu), open or closed.
    pub schematic_views: crate::schematic::Schematics,
}

/// File > File Properties: the Summary fields, the Summary Info
/// description and the Custom name / value list.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileProperties {
    pub title: String,
    pub subject: String,
    pub author: String,
    pub manager: String,
    pub company: String,
    pub category: String,
    pub keywords: String,
    pub comments: String,
    /// Summary Info > Description.
    pub description: String,
    pub custom: BTreeMap<String, String>,
}

impl FileProperties {
    /// The Summary tab fields in dialog order (plus Description).
    pub const FIELDS: [&'static str; 9] = ["Title", "Subject", "Author", "Manager", "Company", "Category", "Keywords", "Comments", "Description"];

    pub fn field_mut(&mut self, name: &str) -> Option<&mut String> {
        Some(match name.to_ascii_lowercase().as_str() {
            "title" => &mut self.title,
            "subject" => &mut self.subject,
            "author" => &mut self.author,
            "manager" => &mut self.manager,
            "company" => &mut self.company,
            "category" => &mut self.category,
            "keywords" => &mut self.keywords,
            "comments" => &mut self.comments,
            "description" => &mut self.description,
            _ => return None,
        })
    }

    pub fn field(&self, name: &str) -> Option<&str> {
        Some(match name.to_ascii_lowercase().as_str() {
            "title" => &self.title,
            "subject" => &self.subject,
            "author" => &self.author,
            "manager" => &self.manager,
            "company" => &self.company,
            "category" => &self.category,
            "keywords" => &self.keywords,
            "comments" => &self.comments,
            "description" => &self.description,
            _ => return None,
        })
    }
}

impl Default for SceneSettings {
    fn default() -> Self {
        Self::from_state(&AppState::new())
    }
}

impl SceneSettings {
    pub fn from_state(s: &AppState) -> Self {
        Self {
            layout: s.layout,
            viewports: s.viewports.iter().map(ViewportSave::from_viewport).collect(),
            active_viewport: s.active_viewport,
            units: s.units.clone(),
            named_sets: s.named_sets.clone(),
            named_sub_selections: s.named_sub_selections.clone(),
            selection: s.selection.iter().copied().collect(),
            frame_range: s.frame_range,
            fps: s.fps,
            current_frame: s.current_frame,
            real_time: s.playback.real_time,
            loop_playback: s.playback.loop_playback,
            play_speed: s.playback.speed,
            play_direction: s.playback.direction,
            default_tangent: s.default_tangent,
            display: s.display.clone(),
            home_view: s.home_view,
            saved_views: s.saved_views.clone(),
            file_properties: s.file_properties.clone(),
            time_tags: s.time_tags.clone(),
            degradation: s.degradation.clone(),
            time_config: s.time_config,
            scene_explorers: s.explorers.clone(),
            schematic_views: s.schematics.clone(),
        }
    }

    pub fn apply_to(&self, s: &mut AppState) {
        s.layout = self.layout;
        let kinds = crate::viewport::default_kinds(self.layout);
        s.viewports = if self.viewports.len() == kinds.len() {
            self.viewports.iter().map(ViewportSave::to_viewport).collect()
        } else {
            // A file whose viewports do not fit its layout (hand-edited, older): the layout's defaults.
            kinds.into_iter().map(Viewport::new).collect()
        };
        s.active_viewport = self.active_viewport.min(s.viewports.len().saturating_sub(1));
        s.units = self.units.clone();
        s.named_sets = self.named_sets.clone();
        s.named_sub_selections = self.named_sub_selections.clone();
        s.selection = self.selection.iter().copied().filter(|id| s.scene.node(*id).is_some()).collect();
        s.sub_selection.clear();
        s.frame_range = self.frame_range;
        s.fps = self.fps;
        s.current_frame = self.current_frame.clamp(self.frame_range.0, self.frame_range.1);
        s.playback.real_time = self.real_time;
        s.playback.loop_playback = self.loop_playback;
        s.playback.speed = if self.play_speed > 0.0 { self.play_speed } else { 1.0 };
        s.playback.direction = self.play_direction;
        s.default_tangent = self.default_tangent;
        s.display = self.display.clone();
        s.home_view = self.home_view;
        s.saved_views = self.saved_views.clone();
        s.file_properties = self.file_properties.clone();
        s.time_tags = self.time_tags.clone();
        s.degradation = self.degradation.clone();
        s.time_config = self.time_config;
        s.explorers = self.scene_explorers.clone();
        s.schematics = self.schematic_views.clone();
        s.scene.set_time(s.current_frame as f32);
    }
}

/// The whole file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub format: u32,
    pub app: String,
    pub scene: Scene,
    pub settings: SceneSettings,
}

/// Writes `doc` to `path` through a temporary file in the same directory, so
/// a crash mid-write never leaves a truncated scene.
pub fn write(path: &Path, doc: &Document) -> Result<(), String> {
    use std::io::Write;
    let text = serde_json::to_string_pretty(doc).map_err(|e| format!("{}: {e}", path.display()))?;
    let tmp = path.with_extension("loom.tmp");
    let written = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        // Durable before the rename: a crash must not leave an empty scene file.
        f.sync_all()?;
        std::fs::rename(&tmp, path)?;
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            if let Ok(d) = std::fs::File::open(dir) {
                let _ = d.sync_all();
            }
        }
        Ok(())
    })();
    written.map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e}", path.display())
    })
}

/// Files from before the Environment dialog kept the background and ambient
/// colours in Render Setup's Renderer panel: they become the Environment's.
fn legacy_environment(value: &mut Value) {
    let Some(scene) = value.get_mut("scene").and_then(Value::as_object_mut) else { return };
    if scene.contains_key("environment") {
        return;
    }
    let bevy = scene.get("render").and_then(|r| r.get("bevy"));
    let colour = |k: &str| bevy.and_then(|b| b.get(k)).and_then(|c| serde_json::from_value::<[f32; 3]>(c.clone()).ok());
    let (bg, ambient) = (colour("background"), colour("ambient"));
    let mut env = loom_scene::environment::Environment::default();
    if let Some(c) = bg {
        env.background = loom_scene::Param::new(Vec3::from(c));
    }
    if let Some(c) = ambient {
        env.ambient = loom_scene::Param::new(Vec3::from(c));
    }
    if let Ok(v) = serde_json::to_value(env) {
        scene.insert("environment".into(), v);
    }
}

/// Reads a `.loom` file. A `format` newer than [`FORMAT`] is refused.
pub fn read(path: &Path) -> Result<Document, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let value: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let format = value.get("format").and_then(Value::as_u64).ok_or_else(|| format!("{}: not a .loom file (no format)", path.display()))?;
    if format > FORMAT as u64 {
        return Err(format!("{}: format {format} is newer than this program's {FORMAT}", path.display()));
    }
    let mut value = value;
    legacy_environment(&mut value);
    let mut doc: Document = serde_json::from_value(value).map_err(|e| format!("{}: {e}", path.display()))?;
    let ids: Vec<NodeId> = doc.scene.nodes().map(|n| n.id).collect();
    for id in ids {
        doc.scene.touch(id);
    }
    Ok(doc)
}

/// A .loom file, or a .max file read as a document (with what it left out).
pub fn read_any(path: &Path) -> Result<(Document, Vec<String>), String> {
    if crate::max_io::is_max(path) {
        crate::max_io::document(path)
    } else {
        Ok((read(path)?, Vec::new()))
    }
}

/// A GUI-less overview of a document (the MCP `loom_read_file` tool).
pub fn summary(doc: &Document) -> Value {
    let nodes: Vec<Value> = doc
        .scene
        .nodes()
        .map(|n| {
            let e = doc.scene.eval_mesh(n.id);
            json!({
                "id": n.id.0,
                "name": n.name,
                "kind": n.object.base_name(),
                "parent": n.parent.map(|p| p.0),
                "layer": n.layer,
                "hidden": n.hidden,
                "frozen": n.frozen,
                "stack": n.modifiers.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
                "vertices": e.value.mesh.vertex_count(),
                "faces": e.value.mesh.face_count(),
            })
        })
        .collect();
    json!({
        "format": doc.format,
        "app": doc.app,
        "nodes": nodes,
        "layers": doc.scene.layers.iter().map(|l| json!({ "id": l.id, "name": l.name })).collect::<Vec<_>>(),
        "settings": {
            "layout": format!("{:?}", doc.settings.layout),
            "units": serde_json::to_value(&doc.settings.units).unwrap_or(Value::Null),
            "frame_range": doc.settings.frame_range,
            "fps": doc.settings.fps,
            "named_sets": doc.settings.named_sets.keys().collect::<Vec<_>>(),
        },
    })
}

/// The Duplicate Name answer for one merged object whose name the scene
/// already has: `merge` (keep both names), `skip`, `delete_old` (the scene's
/// objects of that name go), `rename` (to `new_name`) or `auto_rename`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MergeDuplicate {
    pub name: String,
    pub action: String,
    #[serde(default)]
    pub new_name: Option<String>,
}

/// Merges `doc`'s nodes (all, or those named in `names`) into the scene as
/// one undo entry: fresh ids, references remapped, parents outside the set
/// dropped (world transform kept), instances of nodes outside the set
/// turned into copies, layers kept by name (created when the scene lacks
/// them). A name the scene already has follows its answer in `duplicates`
/// (auto-renamed without one). Their materials come along (a library
/// material of the same name and settings is shared). Returns the new ids.
pub fn merge(state: &mut AppState, doc: &Document, names: Option<&[String]>, duplicates: &[MergeDuplicate], label: &str) -> Result<Vec<NodeId>, String> {
    // New ids follow the merge order (parents first).
    let mut ids: Vec<NodeId> = merge_map(state, doc, names, duplicates, label)?.into_values().collect();
    ids.sort();
    Ok(ids)
}

/// Copies `id` (and the sub-materials it holds) from `src` into `library`,
/// sharing a library material of the same name and settings; returns its id there.
pub(crate) fn carry_material(src: &loom_scene::MaterialLibrary, id: loom_scene::MaterialId, library: &mut loom_scene::MaterialLibrary, done: &mut BTreeMap<loom_scene::MaterialId, loom_scene::MaterialId>, depth: u32) -> Option<loom_scene::MaterialId> {
    if let Some(d) = done.get(&id) {
        return Some(*d);
    }
    let mut m = src.get(&id)?.clone();
    if depth > 16 {
        return None;
    }
    // Every sub-material button of every material type.
    for c in m.children_mut() {
        *c = c.and_then(|c| carry_material(src, c, library, done, depth + 1));
    }
    let to = match library.iter().find(|(_, x)| **x == m) {
        Some((k, _)) => *k,
        None => {
            let k = loom_scene::material::next_material_id(library);
            library.insert(k, m);
            k
        }
    };
    done.insert(id, to);
    Some(to)
}

/// `merge`, returning each merged node's file id → its new id.
pub fn merge_map(state: &mut AppState, doc: &Document, names: Option<&[String]>, duplicates: &[MergeDuplicate], label: &str) -> Result<BTreeMap<NodeId, NodeId>, String> {
    let (changes, map) = merge_changes(state, doc, names, duplicates)?;
    if changes.is_empty() {
        return Ok(map);
    }
    state.undo.commit(label, Change::Group(changes), &mut state.scene)?;
    state.mesh_dirty = true;
    let mut ids: Vec<NodeId> = map.values().copied().collect();
    ids.sort();
    state.selection = ids.into_iter().collect();
    state.sub_selection.clear();
    Ok(map)
}

/// What `merge` commits (layers, materials, the deleted old objects, the new
/// nodes) and the file id → new id map, without committing.
pub(crate) fn merge_changes(state: &AppState, doc: &Document, names: Option<&[String]>, duplicates: &[MergeDuplicate]) -> Result<(Vec<Change>, BTreeMap<NodeId, NodeId>), String> {
    let src = &doc.scene;
    let mut chosen: Vec<NodeId> = match names {
        None => src.nodes().map(|n| n.id).collect(),
        Some(names) => {
            let mut ids = Vec::new();
            for name in names {
                let node = src.nodes().find(|n| &n.name == name).ok_or_else(|| format!("no object {name:?} in the file"))?;
                ids.push(node.id);
            }
            ids
        }
    };
    if chosen.is_empty() {
        return Err("nothing to merge".into());
    }
    for d in duplicates {
        if !matches!(d.action.as_str(), "merge" | "skip" | "delete_old" | "rename" | "auto_rename") {
            return Err(format!("unknown duplicate-name answer {:?} (merge / skip / delete_old / rename / auto_rename)", d.action));
        }
    }
    let answer = |name: &str| duplicates.iter().find(|d| d.name == name);
    let in_scene: std::collections::BTreeSet<String> = state.scene.nodes().map(|n| n.name.clone()).collect();
    // Skipped objects stay behind.
    chosen.retain(|id| {
        let name = &src.node(*id).expect("chosen from the file").name;
        !(in_scene.contains(name) && answer(name).is_some_and(|d| d.action == "skip"))
    });
    if chosen.is_empty() {
        return Ok((Vec::new(), BTreeMap::new()));
    }
    // Parents before children, so Scene::add_node sees the parent.
    chosen.sort_by_key(|id| (src.depth(*id), id.0));
    chosen.dedup();
    let mut changes = Vec::with_capacity(chosen.len() + 2);
    // Materials the merged objects use, into the scene's library.
    let mut library = state.scene.materials.clone();
    let mut material_map = BTreeMap::new();
    for id in &chosen {
        if let Some(m) = src.node(*id).and_then(|n| n.material) {
            carry_material(&src.materials, m, &mut library, &mut material_map, 0);
        }
    }
    if library != state.scene.materials {
        let slots = state.scene.material_editor.clone();
        changes.push(Change::SetMaterials { before: Box::new((state.scene.materials.clone(), slots.clone())), after: Box::new((library, slots)) });
    }
    // Layers by name: reuse the scene's, create the missing ones.
    let mut layers = state.scene.layers.clone();
    let mut layer_map: BTreeMap<u32, u32> = BTreeMap::new();
    for id in &chosen {
        let l = src.node(*id).expect("chosen from the file").layer;
        if layer_map.contains_key(&l) {
            continue;
        }
        let to = match src.layer(l) {
            None => state.scene.current_layer,
            Some(sl) => match layers.iter().find(|x| x.name == sl.name) {
                Some(x) => x.id,
                None => {
                    let nid = layers.iter().map(|x| x.id + 1).max().unwrap_or(1);
                    let mut nl = sl.clone();
                    nl.id = nid;
                    layers.push(nl);
                    nid
                }
            },
        };
        layer_map.insert(l, to);
    }
    if layers != state.scene.layers {
        changes.push(Change::SetLayers { before: (state.scene.layers.clone(), state.scene.current_layer), after: (layers, state.scene.current_layer) });
    }
    // Delete Old: the scene's objects of that name go first (children first).
    let merged_names: std::collections::BTreeSet<&str> = chosen.iter().filter_map(|c| src.node(*c)).map(|n| n.name.as_str()).collect();
    let mut old: Vec<NodeId> = state.scene.nodes().filter(|n| merged_names.contains(n.name.as_str()) && answer(&n.name).is_some_and(|d| d.action == "delete_old")).map(|n| n.id).collect();
    old.sort_by_key(|id| std::cmp::Reverse(state.scene.depth(*id)));
    changes.extend(old.iter().map(|id| Change::RemoveNode(*id)));
    let first = state.scene.next_id();
    let map: BTreeMap<NodeId, NodeId> = chosen.iter().enumerate().map(|(i, id)| (*id, NodeId(first + i as u64))).collect();
    let remap = |id: NodeId| map.get(&id).copied();
    let mut taken: std::collections::BTreeSet<String> = state.scene.nodes().filter(|n| !old.contains(&n.id)).map(|n| n.name.clone()).collect();
    for old_id in &chosen {
        let mut node = src.node(*old_id).expect("chosen from the file").clone();
        // A parent left behind: keep the world transform.
        if node.parent.is_some_and(|p| !map.contains_key(&p)) {
            node.transform = loom_scene::Transform::from_matrix(src.world_matrix(*old_id));
        }
        // An instance whose source stays behind becomes a copy with the resolved stack.
        if node.instance_of.is_some_and(|s| !map.contains_key(&s)) {
            node.instance_of = None;
        }
        for (i, slot) in node.modifiers.iter_mut().enumerate() {
            if slot.instance_of.is_some_and(|(s, _)| !map.contains_key(&s)) {
                if let Some(m) = src.resolved_modifier(*old_id, i) {
                    slot.modifier = m;
                }
                slot.instance_of = None;
            }
        }
        node.remap_ids(&remap);
        node.layer = layer_map.get(&node.layer).copied().unwrap_or(state.scene.current_layer);
        node.material = node.material.and_then(|m| material_map.get(&m).copied());
        let action = if in_scene.contains(&node.name) { answer(&node.name).map(|d| d.action.as_str()).unwrap_or("auto_rename") } else { "auto_rename" };
        match action {
            "merge" | "delete_old" => {}
            "rename" => {
                let to = answer(&node.name).and_then(|d| d.new_name.clone()).map(|n| n.trim().to_owned()).unwrap_or_default();
                if to.is_empty() || taken.contains(&to) {
                    return Err(format!("{to:?} is not a free name for {:?}", node.name));
                }
                node.name = to;
            }
            _ if taken.contains(&node.name) => {
                let base = node.name.trim_end_matches(|c: char| c.is_ascii_digit()).to_owned();
                let base = if base.is_empty() { node.name.clone() } else { base };
                let mut n = 1;
                node.name = loop {
                    let candidate = format!("{base}{n:03}");
                    if !taken.contains(&candidate) {
                        break candidate;
                    }
                    n += 1;
                };
            }
            _ => {}
        }
        taken.insert(node.name.clone());
        changes.push(Change::AddNode(node));
    }
    Ok((changes, map))
}
