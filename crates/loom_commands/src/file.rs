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
    pub background: Option<String>,
    pub background_locked: bool,
}

fn default_clip() -> (f32, f32) {
    (0.0, 1000.0)
}

impl ViewportSave {
    fn from_viewport(v: &Viewport) -> Self {
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
            background: v.background.clone(),
            background_locked: v.background_locked,
        }
    }

    fn to_viewport(&self) -> Viewport {
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
        v.background = self.background.clone();
        v.background_locked = self.background_locked;
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
    /// The ViewCube Home view.
    pub home_view: Option<crate::state::SavedView>,
    /// Views > Save Active View slots.
    pub saved_views: BTreeMap<String, crate::state::SavedView>,
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
            home_view: s.home_view,
            saved_views: s.saved_views.clone(),
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
        s.home_view = self.home_view;
        s.saved_views = self.saved_views.clone();
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

/// Reads a `.loom` file. A `format` newer than [`FORMAT`] is refused.
pub fn read(path: &Path) -> Result<Document, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let value: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let format = value.get("format").and_then(Value::as_u64).ok_or_else(|| format!("{}: not a .loom file (no format)", path.display()))?;
    if format > FORMAT as u64 {
        return Err(format!("{}: format {format} is newer than this program's {FORMAT}", path.display()));
    }
    let mut doc: Document = serde_json::from_value(value).map_err(|e| format!("{}: {e}", path.display()))?;
    let ids: Vec<NodeId> = doc.scene.nodes().map(|n| n.id).collect();
    for id in ids {
        doc.scene.touch(id);
    }
    Ok(doc)
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

/// Merges `doc`'s nodes (all, or those named in `names`) into the scene as
/// one undo entry: fresh ids, references remapped, colliding names renamed,
/// parents outside the set dropped (world transform kept), instances of
/// nodes outside the set turned into copies, merged nodes on the current
/// layer. Returns the new ids.
pub fn merge(state: &mut AppState, doc: &Document, names: Option<&[String]>, label: &str) -> Result<Vec<NodeId>, String> {
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
    // Parents before children, so Scene::add_node sees the parent.
    chosen.sort_by_key(|id| (src.depth(*id), id.0));
    chosen.dedup();
    let first = state.scene.next_id();
    let map: BTreeMap<NodeId, NodeId> = chosen.iter().enumerate().map(|(i, id)| (*id, NodeId(first + i as u64))).collect();
    let remap = |id: NodeId| map.get(&id).copied();
    let mut taken: std::collections::BTreeSet<String> = state.scene.nodes().map(|n| n.name.clone()).collect();
    let mut changes = Vec::with_capacity(chosen.len());
    let mut ids = Vec::with_capacity(chosen.len());
    for old in &chosen {
        let mut node = src.node(*old).expect("chosen from the file").clone();
        // A parent left behind: keep the world transform.
        if node.parent.is_some_and(|p| !map.contains_key(&p)) {
            node.transform = loom_scene::Transform::from_matrix(src.world_matrix(*old));
        }
        // An instance whose source stays behind becomes a copy with the resolved stack.
        if node.instance_of.is_some_and(|s| !map.contains_key(&s)) {
            node.instance_of = None;
        }
        for (i, slot) in node.modifiers.iter_mut().enumerate() {
            if slot.instance_of.is_some_and(|(s, _)| !map.contains_key(&s)) {
                if let Some(m) = src.resolved_modifier(*old, i) {
                    slot.modifier = m;
                }
                slot.instance_of = None;
            }
        }
        node.remap_ids(&remap);
        node.layer = state.scene.current_layer;
        if taken.contains(&node.name) {
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
        taken.insert(node.name.clone());
        ids.push(node.id);
        changes.push(Change::AddNode(node));
    }
    state.undo.commit(label, Change::Group(changes), &mut state.scene)?;
    state.mesh_dirty = true;
    state.selection = ids.iter().copied().collect();
    state.sub_selection.clear();
    Ok(ids)
}
