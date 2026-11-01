//! Animation > Walkthrough Assistant: a camera on a path (Path Constraint,
//! Follow), its path moved to eye level, the head turned and tilted (the
//! camera's own rotation, relative to the path), a targeted camera's target
//! on a path or an object, and a small preview render through the camera.

use crate::execute::commit;
use crate::state::{AppState, PickMode, RenderRequest};
use crate::Command;
use glam::{Quat, Vec3};
use loom_scene::constraint::{PathConstraint, PositionConstraint, Target};
use loom_scene::{Change, NodeId, ObjectKind};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// The floater's state: the camera it works on, the paths' heights before
/// Move Path to Eye Level, and the preview image.
#[derive(Debug, Clone, Default)]
pub struct WalkthroughUi {
    pub camera: Option<NodeId>,
    pub path_heights: HashMap<NodeId, f32>,
    pub preview: Option<Arc<image::RgbaImage>>,
    /// Bumped when `preview` changes.
    pub preview_version: u64,
}

/// Preview size in pixels (fits the floater).
pub const PREVIEW_SIZE: [u32; 2] = [200, 150];

pub fn handles(c: &Command) -> bool {
    use Command as C;
    matches!(
        c,
        C::WalkthroughAssistant
            | C::WalkthroughCreateCamera { .. }
            | C::WalkthroughSelectCamera { .. }
            | C::WalkthroughPickPath { .. }
            | C::WalkthroughSetPath { .. }
            | C::WalkthroughEyeLevel { .. }
            | C::WalkthroughHead { .. }
            | C::WalkthroughRemoveHeadAnimation { .. }
            | C::WalkthroughSetTarget { .. }
            | C::WalkthroughSetViewport { .. }
            | C::WalkthroughRenderPreview { .. }
    )
}

fn missing(id: NodeId) -> String {
    format!("no node {}", id.0)
}

fn camera(state: &AppState, id: u64) -> Result<NodeId, String> {
    let id = NodeId(id);
    match state.scene.node(id).map(|n| &n.object) {
        Some(ObjectKind::Camera { .. }) => Ok(id),
        Some(_) => Err(format!("{} is not a camera", id.0)),
        None => Err(missing(id)),
    }
}

/// The camera's rotation in the path's frame (X along the path, Z up) for a
/// head turn (degrees about Z, positive to the left) and tilt (degrees, up
/// positive): the camera looks down the path when both are 0.
pub fn head_rotation(turn: f32, tilt: f32) -> Quat {
    let look = Quat::from_rotation_z(turn.to_radians()) * Quat::from_rotation_y(-tilt.to_radians()) * Vec3::X;
    loom_scene::constraint::aim(look, 2, true, Vec3::Z)
}

/// The head turn and tilt (degrees) of a camera rotation in the path frame.
pub fn head_angles(q: Quat) -> (f32, f32) {
    let d = q * Vec3::NEG_Z;
    (d.y.atan2(d.x).to_degrees(), d.z.clamp(-1.0, 1.0).asin().to_degrees())
}

/// The path a camera (or its target) follows.
pub fn path_of(state: &AppState, id: NodeId) -> Option<NodeId> {
    match &state.scene.node(id)?.constraints.position {
        Some(PositionConstraint::Path(p)) => p.targets.first().map(|t| t.node),
        _ => None,
    }
}

fn set_head(state: &mut AppState, cam: NodeId, label: &str, q: Quat) -> Result<(), String> {
    let before = state.scene.node(cam).ok_or_else(|| missing(cam))?.transform.clone();
    let mut after = before.clone();
    after.set_rotation_quat(q);
    commit(state, label, Change::SetTransform { id: cam, before, after })
}

fn constrain(state: &mut AppState, id: NodeId, label: &str, position: Option<PositionConstraint>) -> Result<(), String> {
    let n = state.scene.node(id).ok_or_else(|| missing(id))?;
    let mut after = n.constraints.clone();
    after.position = position;
    if after == n.constraints {
        return Ok(());
    }
    commit(state, label, Change::SetConstraints { id, before: Box::new(n.constraints.clone()), after: Box::new(after) })
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    match cmd {
        C::WalkthroughAssistant => {
            state.dialog = if state.dialog == Some(crate::Dialog::WalkthroughAssistant) { None } else { Some(crate::Dialog::WalkthroughAssistant) };
            Ok(json!({}))
        }
        C::WalkthroughCreateCamera { targeted } => {
            let eye = Vec3::ZERO;
            let r = crate::camera::create(state, targeted, eye, eye + Vec3::X * 100.0)?;
            let id = NodeId(r["id"].as_u64().ok_or("no camera")?);
            // Walkthrough_Cam01, 02, …
            let mut n = 1;
            let name = loop {
                let c = format!("Walkthrough_Cam{n:02}");
                if !state.scene.nodes().any(|x| x.name == c) {
                    break c;
                }
                n += 1;
            };
            let t = r["target"].as_u64().map(NodeId);
            crate::execute::execute(state, C::Rename { id: id.0, name: name.clone() })?;
            state.walkthrough.camera = Some(id);
            Ok(json!({ "id": id.0, "name": name, "target": t.map(|t| t.0) }))
        }
        C::WalkthroughSelectCamera { id } => {
            state.walkthrough.camera = Some(camera(state, id)?);
            Ok(json!({}))
        }
        C::WalkthroughPickPath { target } => {
            let mode = if target { PickMode::WalkthroughTargetPath } else { PickMode::WalkthroughPath };
            state.pick_mode = if state.pick_mode == Some(mode) { None } else { Some(mode) };
            Ok(json!({}))
        }
        C::WalkthroughSetPath { camera: cam, path } => {
            let cam = camera(state, cam)?;
            let Some(path) = path.map(NodeId) else {
                constrain(state, cam, "Clear Path", None)?;
                return Ok(json!({}));
            };
            state.scene.node(path).and_then(|n| n.object.shape()).ok_or("the path must be a shape")?;
            let pc = PathConstraint { targets: vec![Target::new(path)], percent: crate::controllers::percent_param(state), follow: true, ..Default::default() };
            let n = state.scene.node(cam).ok_or_else(|| missing(cam))?;
            let mut constraints = n.constraints.clone();
            constraints.position = Some(PositionConstraint::Path(pc));
            // The head starts level, looking down the path.
            let mut after = n.transform.clone();
            after.set_rotation_quat(head_rotation(0.0, 0.0));
            let changes = vec![Change::SetConstraints { id: cam, before: Box::new(n.constraints.clone()), after: Box::new(constraints) }, Change::SetTransform { id: cam, before: n.transform.clone(), after }];
            commit(state, "Pick Path", Change::Group(changes))?;
            Ok(json!({ "path": path.0 }))
        }
        C::WalkthroughEyeLevel { camera: cam, on, level } => {
            let cam = camera(state, cam)?;
            let path = path_of(state, cam).ok_or("pick a path first")?;
            let before = state.scene.node(path).ok_or_else(|| missing(path))?.transform.clone();
            let mut after = before.clone();
            let mut p = before.position.get();
            if on {
                state.walkthrough.path_heights.entry(path).or_insert(p.z);
                p.z = level;
            } else if let Some(z) = state.walkthrough.path_heights.remove(&path) {
                p.z = z;
            }
            after.position.set(p);
            commit(state, "Eye Level", Change::SetTransform { id: path, before, after })?;
            Ok(json!({ "height": p.z }))
        }
        C::WalkthroughHead { camera: cam, turn, tilt } => {
            let cam = camera(state, cam)?;
            let (t0, a0) = head_angles(state.scene.node(cam).ok_or_else(|| missing(cam))?.transform.rotation_quat());
            let (turn, tilt) = (turn.unwrap_or(t0), tilt.unwrap_or(a0));
            if !(-180.0..=180.0).contains(&turn) || !(-89.0..=89.0).contains(&tilt) {
                return Err("turn -180 to 180, tilt -89 to 89 degrees".into());
            }
            set_head(state, cam, "Turn Head", head_rotation(turn, tilt))?;
            Ok(json!({ "turn": turn, "tilt": tilt }))
        }
        C::WalkthroughRemoveHeadAnimation { camera: cam } => {
            let cam = camera(state, cam)?;
            let before = state.scene.node(cam).ok_or_else(|| missing(cam))?.transform.clone();
            let mut after = before.clone();
            let (x, y, z) = head_rotation(0.0, 0.0).to_euler(glam::EulerRot::XYZ);
            after.rotation = loom_scene::param::Param::new(Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees()));
            crate::execute::commit_exact(state, "Remove All Head Animation", Change::SetTransform { id: cam, before, after })?;
            Ok(json!({}))
        }
        C::WalkthroughSetTarget { camera: cam, path, node } => {
            let cam = camera(state, cam)?;
            let Some(ObjectKind::Camera { target: Some(t), .. }) = state.scene.node(cam).map(|n| &n.object) else {
                return Err("the camera is not Targeted".into());
            };
            let t = *t;
            let Some(node) = node.map(NodeId) else {
                constrain(state, t, "Clear Target Path", None)?;
                return Ok(json!({}));
            };
            state.scene.node(node).ok_or_else(|| missing(node))?;
            let pc = if path {
                state.scene.node(node).and_then(|n| n.object.shape()).ok_or("the target path must be a shape")?;
                PositionConstraint::Path(PathConstraint { targets: vec![Target::new(node)], percent: crate::controllers::percent_param(state), ..Default::default() })
            } else {
                PositionConstraint::Position { targets: vec![Target::new(node)], keep_offset: false, offset: Vec3::ZERO }
            };
            constrain(state, t, "Pick Target", Some(pc))?;
            Ok(json!({ "target": t.0 }))
        }
        C::WalkthroughSetViewport { camera: cam } => {
            let cam = camera(state, cam)?;
            crate::execute::set_view(state, crate::state::ViewKind::Camera(cam))
        }
        C::WalkthroughRenderPreview { camera: cam } => {
            let cam = camera(state, cam)?;
            let mut settings = state.scene.render.clone();
            let aspect = settings.image_aspect().max(1e-3);
            let [pw, ph] = PREVIEW_SIZE;
            let (w, h) = if aspect >= pw as f32 / ph as f32 { (pw, ((pw as f32 / aspect).round() as u32).max(1)) } else { (((ph as f32 * aspect).round() as u32).max(1), ph) };
            settings.width = w;
            settings.height = h;
            settings.pixel_aspect = 1.0;
            settings.area = loom_scene::render::RenderArea::View;
            settings.bevy.supersampling = 1;
            let viewport = crate::render::render_viewport(state);
            crate::render::start(state, RenderRequest { viewport, settings, camera: Some(cam), preview: true, walkthrough_preview: true, ..Default::default() })
        }
        _ => Err("not a Walkthrough Assistant command".into()),
    }
}
