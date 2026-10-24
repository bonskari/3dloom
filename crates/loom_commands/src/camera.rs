//! Create > Cameras: Target and Free cameras, and switching a camera's type.
//! A Target camera aims its local -Z at its target node through a LookAt
//! rotation constraint (world Z up); a Free camera keeps its own rotation.

use crate::execute::commit;
use crate::state::AppState;
use crate::Command;
use glam::{Mat4, Quat, Vec3};
use loom_scene::constraint::{RotationConstraint, Target};
use loom_scene::{Change, Node, NodeId, ObjectKind, Transform};
use serde_json::{json, Value};
use std::collections::BTreeSet;

/// Wire colour of new cameras and their targets.
pub const CAMERA_COLOR: [u8; 3] = [0, 160, 255];
/// Size of a camera target's box.
pub const TARGET_SIZE: f32 = 4.0;

fn missing(id: NodeId) -> String {
    format!("no node {}", id.0)
}

/// The LookAt constraint that aims a camera's -Z at `target`.
pub fn look_at_target(target: NodeId) -> RotationConstraint {
    RotationConstraint::LookAt { targets: vec![Target::new(target)], axis: 2, flip: true, upnode: None, keep_offset: false, offset: Quat::IDENTITY }
}

/// Rotation that points local -Z along `dir` with local +Y as close to world +Z as it allows.
fn view_rotation(dir: Vec3) -> Quat {
    loom_scene::constraint::aim(dir, 2, true, Vec3::Z)
}

fn target_node(name: &str, at: Vec3) -> Node {
    let mut t = Node::new(&format!("{name}.Target"), ObjectKind::dummy(TARGET_SIZE));
    t.transform.position.set(at);
    t.wire_color = CAMERA_COLOR;
    t
}

/// Creates a camera at `eye` looking at `look_at`: a Target camera with its
/// target node there, or a Free camera whose Target Distance is the gap.
pub fn create(state: &mut AppState, target: bool, eye: Vec3, look_at: Vec3) -> Result<Value, String> {
    let name = state.scene.next_name("Camera");
    let fov = loom_scene::camera::DEFAULT_FOV;
    let cam_id = NodeId(state.scene.next_id());
    let mut node = Node::new(&name, ObjectKind::camera_object(fov));
    let dir = look_at - eye;
    node.transform = Transform::from_matrix(Mat4::from_rotation_translation(view_rotation(dir), eye));
    node.wire_color = CAMERA_COLOR;
    let mut changes = Vec::new();
    let mut target_id = None;
    if target {
        let tid = NodeId(cam_id.0 + 1);
        node.object = ObjectKind::target_camera_object(fov, tid);
        node.constraints.rotation = Some(look_at_target(tid));
        changes.push(Change::AddNode(node));
        changes.push(Change::AddNode(target_node(&name, look_at)));
        target_id = Some(tid.0);
    } else {
        if dir.length() > 1e-4 {
            let _ = node.object.set_parameter("Target Distance", dir.length() as f64);
        }
        changes.push(Change::AddNode(node));
    }
    commit(state, "Create Camera", Change::Group(changes))?;
    state.selection = BTreeSet::from([cam_id]);
    state.sub_selection.clear();
    Ok(json!({ "id": cam_id.0, "target": target_id }))
}

/// Type: Target <-> Free. Going Free removes the target node and keeps the
/// camera's current aim; going Target adds a target at Target Distance.
pub fn set_type(state: &mut AppState, id: NodeId, target: bool) -> Result<Value, String> {
    let node = state.scene.node(id).ok_or_else(|| missing(id))?;
    let ObjectKind::Camera { target: current, target_distance, .. } = &node.object else {
        return Err(format!("{} is not a camera", node.name));
    };
    let current = current.filter(|t| state.scene.node(*t).is_some());
    if current.is_some() == target {
        return Ok(json!({}));
    }
    let world = state.scene.world_matrix(id);
    let eye = world.w_axis.truncate();
    let forward = -world.z_axis.truncate().normalize_or_zero();
    let mut object = node.object.clone();
    let mut constraints = node.constraints.clone();
    let mut changes = Vec::new();
    match current {
        Some(t) => {
            let distance = (state.scene.world_matrix(t).w_axis.truncate() - eye).length();
            if let ObjectKind::Camera { target, target_distance, .. } = &mut object {
                *target = None;
                target_distance.set(distance);
            }
            constraints.rotation = None;
            // Keep the aim the constraint gave it, in the parent's frame.
            let parent = node.parent.map(|p| state.scene.world_matrix(p)).unwrap_or(Mat4::IDENTITY);
            let local = parent.inverse() * world;
            let mut after = node.transform.clone();
            let (_, rot, _) = local.to_scale_rotation_translation();
            after.set_rotation_quat(rot);
            changes.push(Change::SetTransform { id, before: node.transform.clone(), after });
            changes.push(Change::RemoveNode(t));
        }
        None => {
            let tid = NodeId(state.scene.next_id());
            let at = eye + forward * target_distance.get().max(1e-3);
            if let ObjectKind::Camera { target, .. } = &mut object {
                *target = Some(tid);
            }
            constraints.rotation = Some(look_at_target(tid));
            changes.push(Change::AddNode(target_node(&node.name, at)));
        }
    }
    changes.push(Change::ReplaceObject { id, before: Box::new(node.object.clone()), after: Box::new(object) });
    changes.push(Change::SetConstraints { id, before: Box::new(node.constraints.clone()), after: Box::new(constraints) });
    commit(state, "Camera Type", Change::Group(changes))?;
    Ok(json!({}))
}

/// Distance from a camera to the point it looks at: its target node, or a
/// Free camera's Target Distance.
pub fn target_distance(scene: &loom_scene::Scene, id: NodeId) -> Option<f32> {
    let ObjectKind::Camera { target, target_distance, .. } = &scene.node(id)?.object else { return None };
    match target.filter(|t| scene.node(*t).is_some()) {
        Some(t) => Some((scene.world_matrix(t).w_axis.truncate() - scene.world_matrix(id).w_axis.truncate()).length()),
        None => Some(target_distance.get()),
    }
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    match cmd {
        Command::CreateCamera { target } => {
            let cam = state.active().camera.clone();
            create(state, target, cam.eye_position(), cam.target)
        }
        Command::CreateCameraAt { target, position, look_at } => create(state, target, Vec3::from(position), Vec3::from(look_at)),
        Command::CameraType { id, target } => set_type(state, NodeId(id), target),
        other => Err(format!("not a camera command: {}", other.name())),
    }
}

/// What a camera overlay line shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// The field-of-view pyramid.
    Cone,
    /// Environment Ranges rectangles.
    Range,
    /// Clipping plane rectangles (with diagonals).
    Clip,
    /// The line from a Target camera to its target.
    TargetLine,
}

/// World-space overlay lines of a camera for the other viewports: the cone
/// (when `cone` — selected — or Show Cone is on) out to the target distance
/// in the render aspect, the range and clipping rectangles inside it, and
/// the line to the target.
pub fn overlay_lines(scene: &loom_scene::Scene, id: NodeId, cone: bool) -> Vec<(Vec3, Vec3, LineKind)> {
    use loom_scene::camera::{frame_corners, RENDER_ASPECT};
    let Some(node) = scene.node(id) else { return Vec::new() };
    let ObjectKind::Camera { fov, target, ortho, show_cone, show_ranges, near_range, far_range, clip_manually, near_clip, far_clip, .. } = &node.object else {
        return Vec::new();
    };
    let world = scene.world_matrix(id);
    let eye = world.w_axis.truncate();
    let distance = target_distance(scene, id).unwrap_or(loom_scene::camera::DEFAULT_TARGET_DISTANCE);
    let ortho = ortho.get().then_some(distance);
    let at = |d: f32| frame_corners(fov.get(), RENDER_ASPECT, d, ortho).map(|c| world.transform_point3(c));
    let mut out = Vec::new();
    let rect = |out: &mut Vec<(Vec3, Vec3, LineKind)>, c: [Vec3; 4], kind: LineKind| {
        for i in 0..4 {
            out.push((c[i], c[(i + 1) % 4], kind));
        }
    };
    if cone || show_cone.get() {
        let far = at(distance);
        let near = if ortho.is_some() { at(0.0) } else { [eye; 4] };
        for i in 0..4 {
            out.push((near[i], far[i], LineKind::Cone));
        }
        rect(&mut out, far, LineKind::Cone);
        if ortho.is_some() {
            rect(&mut out, near, LineKind::Cone);
        }
    }
    if show_ranges.get() {
        for d in [near_range.get(), far_range.get()] {
            rect(&mut out, at(d), LineKind::Range);
        }
    }
    if clip_manually.get() {
        for d in [near_clip.get(), far_clip.get()] {
            let c = at(d);
            rect(&mut out, c, LineKind::Clip);
            out.push((c[0], c[2], LineKind::Clip));
            out.push((c[1], c[3], LineKind::Clip));
        }
    }
    if let Some(t) = target.filter(|t| scene.node(*t).is_some()) {
        out.push((eye, scene.world_matrix(t).w_axis.truncate(), LineKind::TargetLine));
    }
    out
}
