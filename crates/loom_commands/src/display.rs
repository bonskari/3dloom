//! Display panel and viewport display settings that are not per object:
//! Display Color, Hide Frozen Objects, Hide by Category > Bone Objects,
//! the viewport statistics (Viewport Configuration > Statistics), Display
//! Selected with Edged Faces, Safe Frames, and the Object Color dialog.

use crate::command::Command as C;
use crate::execute::commit;
use crate::AppState;
use loom_scene::Change;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Display Color rollout: what colours an object's wireframe or shaded faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ColorSource {
    #[default]
    Object,
    Material,
}

impl ColorSource {
    pub fn label(self) -> &'static str {
        match self {
            ColorSource::Object => "Object Color",
            ColorSource::Material => "Material Color",
        }
    }

    fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "object" | "object color" => Ok(ColorSource::Object),
            "material" | "material color" => Ok(ColorSource::Material),
            _ => Err(format!("unknown display colour `{s}` (Object Color or Material Color)")),
        }
    }
}

/// Statistics panel: Total / Selection / Total + Selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StatisticsScope {
    #[default]
    Total,
    Selection,
    TotalAndSelection,
}

/// Viewport Configuration > Statistics: what the statistics show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Statistics {
    pub polygons: bool,
    pub triangles: bool,
    pub edges: bool,
    pub vertices: bool,
    pub fps: bool,
    pub scope: StatisticsScope,
}

impl Default for Statistics {
    fn default() -> Self {
        Self { polygons: true, triangles: true, edges: true, vertices: true, fps: true, scope: StatisticsScope::Total }
    }
}

/// Viewport Configuration > Safe Frames values.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SafeFrames {
    pub live_area: bool,
    pub action_safe: bool,
    pub title_safe: bool,
    pub user_safe: bool,
    pub grid: bool,
    /// How much smaller than the live area each frame is, percent, horizontal / vertical.
    pub action: [f32; 2],
    pub title: [f32; 2],
    pub user: [f32; 2],
    /// 12-Field Grid: 4 x 3 when true, else 12 x 9.
    pub grid_4x3: bool,
}

impl Default for SafeFrames {
    fn default() -> Self {
        Self { live_area: true, action_safe: true, title_safe: true, user_safe: false, grid: false, action: [10.0, 10.0], title: [20.0, 20.0], user: [30.0, 30.0], grid_4x3: true }
    }
}

/// The settings of this module (saved with the scene where the reference
/// keeps them with the file).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplaySettings {
    pub wireframe_color: ColorSource,
    pub shaded_color: ColorSource,
    pub hide_frozen: bool,
    pub hide_bones: bool,
    pub statistics: Statistics,
    pub display_selected_edged: bool,
    pub safe_frames: SafeFrames,
    /// Object Color dialog: new objects get a palette colour by turn; off,
    /// they get `current_color`.
    pub assign_random_colors: bool,
    pub current_color: [u8; 3],
    pub custom_colors: Vec<[u8; 3]>,
    /// The Object Color dialog is open (UI only).
    #[serde(skip)]
    pub object_color_open: bool,
}

/// Object Color dialog: the number of custom colour slots.
pub const CUSTOM_COLORS: usize = 16;

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            wireframe_color: ColorSource::Object,
            shaded_color: ColorSource::Material,
            hide_frozen: false,
            hide_bones: false,
            statistics: Statistics::default(),
            display_selected_edged: false,
            safe_frames: SafeFrames::default(),
            assign_random_colors: true,
            current_color: loom_scene::OBJECT_COLORS[0],
            custom_colors: vec![[255, 255, 255]; CUSTOM_COLORS],
            object_color_open: false,
        }
    }
}

impl DisplaySettings {
    /// The colour a new object gets (Assign Random Colors on: the palette by id).
    pub fn new_object_color(&self, next_id: u64) -> [u8; 3] {
        if self.assign_random_colors {
            loom_scene::OBJECT_COLORS[next_id as usize % loom_scene::OBJECT_COLORS.len()]
        } else {
            self.current_color
        }
    }
}

pub fn handles(c: &C) -> bool {
    matches!(
        c,
        C::SetDisplayColor { .. }
            | C::HideFrozenObjectsToggle
            | C::HideBoneObjectsToggle
            | C::SetStatistics { .. }
            | C::DisplaySelectedEdgedFacesToggle
            | C::SetSafeFrames { .. }
            | C::ObjectColorDialog
            | C::SetObjectColor { .. }
            | C::SetAssignRandomColors { .. }
            | C::SetCustomColor { .. }
            | C::SelectByColor { .. }
    )
}

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    // Every setting here changes what the viewports draw.
    state.mesh_dirty = true;
    let d = &mut state.display;
    match cmd {
        C::SetDisplayColor { wireframe, shaded } => {
            if let Some(w) = wireframe {
                d.wireframe_color = ColorSource::parse(&w)?;
            }
            if let Some(s) = shaded {
                d.shaded_color = ColorSource::parse(&s)?;
            }
            Ok(json!({ "wireframe": d.wireframe_color.label(), "shaded": d.shaded_color.label() }))
        }
        C::HideFrozenObjectsToggle => {
            d.hide_frozen = !d.hide_frozen;
            let on = d.hide_frozen;
            crate::execute::prune_selection_hidden(state);
            Ok(json!({ "hide_frozen": on }))
        }
        C::HideBoneObjectsToggle => {
            d.hide_bones = !d.hide_bones;
            let on = d.hide_bones;
            crate::execute::prune_selection_hidden(state);
            Ok(json!({ "hide_bones": on }))
        }
        C::SetStatistics { polygons, triangles, edges, vertices, fps, scope, default_settings } => {
            let s = &mut d.statistics;
            if default_settings == Some(true) {
                *s = Statistics::default();
            }
            for (v, f) in [(polygons, &mut s.polygons), (triangles, &mut s.triangles), (edges, &mut s.edges), (vertices, &mut s.vertices), (fps, &mut s.fps)] {
                if let Some(v) = v {
                    *f = v;
                }
            }
            if let Some(sc) = scope {
                s.scope = match sc.to_ascii_lowercase().replace(' ', "").as_str() {
                    "total" => StatisticsScope::Total,
                    "selection" => StatisticsScope::Selection,
                    "total+selection" | "totalandselection" => StatisticsScope::TotalAndSelection,
                    _ => return Err(format!("unknown statistics scope `{sc}` (Total, Selection, Total + Selection)")),
                };
            }
            Ok(serde_json::to_value(*s).unwrap_or(Value::Null))
        }
        C::DisplaySelectedEdgedFacesToggle => {
            d.display_selected_edged = !d.display_selected_edged;
            Ok(json!({ "display_selected_edged": d.display_selected_edged }))
        }
        C::SetSafeFrames { live_area, action_safe, title_safe, user_safe, grid, action, title, user, grid_4x3 } => {
            let s = &mut d.safe_frames;
            for (v, f) in [(live_area, &mut s.live_area), (action_safe, &mut s.action_safe), (title_safe, &mut s.title_safe), (user_safe, &mut s.user_safe), (grid, &mut s.grid), (grid_4x3, &mut s.grid_4x3)] {
                if let Some(v) = v {
                    *f = v;
                }
            }
            for (v, f) in [(action, &mut s.action), (title, &mut s.title), (user, &mut s.user)] {
                if let Some(v) = v {
                    *f = [v[0].clamp(0.0, 99.0), v[1].clamp(0.0, 99.0)];
                }
            }
            Ok(serde_json::to_value(*s).unwrap_or(Value::Null))
        }
        C::ObjectColorDialog => {
            d.object_color_open = true;
            Ok(json!({}))
        }
        C::SetAssignRandomColors { on } => {
            d.assign_random_colors = on;
            Ok(json!({ "assign_random_colors": on }))
        }
        C::SetCustomColor { index, rgb } => {
            let slot = d.custom_colors.get_mut(index as usize).ok_or_else(|| format!("no custom colour slot {index} (0-{})", CUSTOM_COLORS - 1))?;
            *slot = rgb;
            Ok(json!({}))
        }
        C::SetObjectColor { rgb } => {
            d.current_color = rgb;
            let changes: Vec<Change> = state
                .selection
                .iter()
                .filter_map(|id| state.scene.node(*id))
                .filter(|n| n.wire_color != rgb)
                .map(|n| Change::SetWireColor { id: n.id, before: n.wire_color, after: rgb })
                .collect();
            if !changes.is_empty() {
                commit(state, "Object Color", Change::Group(changes))?;
            }
            Ok(json!({}))
        }
        C::SelectByColor { rgb } => {
            let ids: Vec<u64> = state.scene.nodes().filter(|n| n.wire_color == rgb && state.is_visible(n) && !n.frozen).map(|n| n.id.0).collect();
            crate::execute::execute(state, C::SelectNodes { ids: ids.clone(), op: crate::SelectOp::Replace })?;
            Ok(json!({ "selected": ids }))
        }
        other => Err(format!("{} is not a display command", other.name())),
    }
}
