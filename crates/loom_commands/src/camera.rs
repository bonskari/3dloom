//! Create > Cameras: Target and Free cameras, and switching a camera's type.
//! A Target camera aims its local -Z at its target node through a LookAt
//! rotation constraint (world Z up); a Free camera keeps its own rotation.

use crate::execute::commit;
use crate::state::{AppState, ViewKind};
use crate::Command;
use glam::{Mat4, Quat, Vec3};
use loom_scene::constraint::{RotationConstraint, Target};
use loom_input::OrbitCamera;
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

/// The view a camera node gives: its eye, the point it looks at (target node
/// or Target Distance) and its roll, as an orbit.
pub fn orbit_of(scene: &loom_scene::Scene, id: NodeId) -> Option<OrbitCamera> {
    let world = scene.world_matrix(id);
    let eye = world.w_axis.truncate();
    let forward = -world.z_axis.truncate().normalize_or_zero();
    let distance = target_distance(scene, id)?.max(1e-3);
    Some(OrbitCamera::looking(eye, eye + forward * distance, world.y_axis.truncate().normalize_or_zero()))
}

/// Moves camera `id` (and its target node) so it looks as `orbit` does: the
/// camera at the orbit's eye, the target at its target, the roll kept (on a
/// Target camera as the LookAt constraint's offset). `fov` sets the FOV;
/// `free_distance` writes a Free camera's Target Distance. One undo step.
fn place(state: &mut AppState, id: NodeId, orbit: &OrbitCamera, fov: Option<f32>, free_distance: bool, label: &str) -> Result<Value, String> {
    let node = state.scene.node(id).ok_or_else(|| missing(id))?;
    let ObjectKind::Camera { target, .. } = &node.object else {
        return Err(format!("{} is not a camera", node.name));
    };
    let target = target.filter(|t| state.scene.node(*t).is_some());
    let eye = orbit.eye_position();
    let (right, up, to_eye) = orbit.basis();
    let world = Mat4::from_cols(right.extend(0.0), up.extend(0.0), to_eye.extend(0.0), eye.extend(1.0));
    let parent_of = |n: &Node| n.parent.map(|p| state.scene.world_matrix(p)).unwrap_or(Mat4::IDENTITY);
    let local = parent_of(node).inverse() * world;
    let (_, rot, pos) = local.to_scale_rotation_translation();
    let mut after = node.transform.clone();
    after.position.set(pos);
    let mut changes = Vec::new();
    match target {
        Some(t) => {
            let tnode = state.scene.node(t).ok_or_else(|| missing(t))?;
            let mut tafter = tnode.transform.clone();
            tafter.position.set(parent_of(tnode).inverse().transform_point3(orbit.target));
            changes.push(Change::SetTransform { id: t, before: tnode.transform.clone(), after: tafter });
            // The LookAt keeps the camera upright; its offset carries the roll.
            let upright = loom_scene::constraint::aim(orbit.target - eye, 2, true, Vec3::Z);
            let (_, wrot, _) = world.to_scale_rotation_translation();
            let offset = (upright.inverse() * wrot).normalize();
            let rolled = offset.angle_between(Quat::IDENTITY) > 1e-4;
            let mut constraints = node.constraints.clone();
            if let Some(RotationConstraint::LookAt { keep_offset, offset: o, .. }) = constraints.rotation.as_mut() {
                *keep_offset = rolled;
                *o = if rolled { offset } else { Quat::IDENTITY };
            }
            if constraints != node.constraints {
                changes.push(Change::SetConstraints { id, before: Box::new(node.constraints.clone()), after: Box::new(constraints) });
            }
        }
        None => after.set_rotation_quat(rot),
    }
    changes.push(Change::SetTransform { id, before: node.transform.clone(), after });
    let mut object = node.object.clone();
    if let ObjectKind::Camera { fov: f, target_distance, .. } = &mut object {
        if let Some(v) = fov {
            f.set(v.clamp(1.0, 175.0));
        }
        if free_distance && target.is_none() {
            target_distance.set(orbit.distance);
        }
    }
    if object != node.object {
        changes.push(Change::ReplaceObject { id, before: Box::new(node.object.clone()), after: Box::new(object) });
    }
    commit(state, label, Change::Group(changes))?;
    Ok(json!({ "eye": eye.to_array(), "target": orbit.target.to_array() }))
}

/// Horizontal FOV (degrees) of a Perspective viewport's view at the render aspect.
fn view_fov_x(fov_y: f32) -> f32 {
    (2.0 * ((fov_y * 0.5).tan() * loom_scene::camera::RENDER_ASPECT).atan()).to_degrees()
}

/// Views > Create Camera From View: a Target camera matching the active
/// Perspective view (the viewport then looks through it), or, with a camera
/// selected, that camera moved to the view.
fn create_from_view(state: &mut AppState) -> Result<Value, String> {
    let vp = state.active();
    if vp.kind != ViewKind::Perspective {
        return Err("Create Camera From View needs an active Perspective viewport".into());
    }
    let orbit = vp.camera.clone();
    let fov = view_fov_x(vp.fov_y_radians);
    let selected: Vec<NodeId> = state.selection.iter().copied().filter(|id| matches!(state.scene.node(*id).map(|n| &n.object), Some(ObjectKind::Camera { .. }))).collect();
    if let [id] = selected[..] {
        return place(state, id, &orbit, Some(fov), true, "Match Camera to View");
    }
    state.undo.begin("Create Camera From View");
    let made = create(state, true, orbit.eye_position(), orbit.target).and_then(|r| {
        let id = NodeId(r["id"].as_u64().ok_or("no camera id")?);
        place(state, id, &orbit, Some(fov), false, "Create Camera From View")?;
        Ok(id)
    });
    state.undo.end();
    let id = made?;
    state.record_view();
    state.active_mut().kind = ViewKind::Camera(id);
    Ok(json!({ "id": id.0 }))
}

/// The camera viewport controls (see `Command::NavigateCamera`).
fn navigate(state: &mut AppState, op: &str, x: f32, y: f32) -> Result<Value, String> {
    use loom_input::camera_control::wrap_angle;
    let ViewKind::Camera(id) = state.active().kind else {
        return Err("the active viewport is not a camera view".into());
    };
    let mut o = orbit_of(&state.scene, id).ok_or_else(|| missing(id))?;
    let has_target = matches!(state.scene.node(id).map(|n| &n.object), Some(ObjectKind::Camera { target: Some(t), .. }) if state.scene.node(*t).is_some());
    let forward = -o.basis().2;
    let d = o.distance;
    let factor = x.max(1e-3);
    let mut fov = None;
    let mut free_distance = false;
    let label = match op {
        "dolly" => {
            if has_target {
                o.distance = (d * factor).max(1e-3);
            } else {
                // A Free camera keeps its Target Distance: its implied target moves with it.
                o.target += forward * d * (1.0 - factor);
            }
            "Dolly Camera"
        }
        "dolly_target" | "dolly_both" if !has_target => return Err("Dolly Target needs a Target camera".into()),
        "dolly_target" => {
            let eye = o.eye_position();
            o.distance = (d * factor).max(1e-3);
            o.target = eye + forward * o.distance;
            "Dolly Target"
        }
        "dolly_both" => {
            o.target += forward * d * (1.0 - factor);
            "Dolly Camera + Target"
        }
        "perspective" => {
            // Keep the frame's width at the target: distance * tan(fov / 2) stays.
            let current = match state.scene.node(id).map(|n| &n.object) {
                Some(ObjectKind::Camera { fov, .. }) => fov.get(),
                _ => return Err(missing(id)),
            };
            o.distance = (d * factor).max(1e-3);
            let half = ((current.to_radians() * 0.5).tan() * d / o.distance).atan();
            fov = Some((2.0 * half).to_degrees().min(175.0));
            free_distance = true;
            "Perspective"
        }
        "roll" => {
            o.roll = wrap_angle(o.roll - x.to_radians());
            "Roll Camera"
        }
        "truck" => {
            let (right, up, _) = o.basis();
            o.target += right * x + up * y;
            "Truck Camera"
        }
        "orbit" => {
            o.yaw = wrap_angle(o.yaw + x.to_radians());
            o.pitch = wrap_angle(o.pitch + y.to_radians());
            "Orbit Camera"
        }
        "pan" => {
            let eye = o.eye_position();
            o.yaw = wrap_angle(o.yaw + x.to_radians());
            o.pitch = wrap_angle(o.pitch - y.to_radians());
            o.target = eye - o.basis().2 * d;
            "Pan Camera"
        }
        "fov" => {
            fov = Some(x);
            "Field-of-View"
        }
        other => return Err(format!("unknown camera control `{other}` (dolly, dolly_target, dolly_both, perspective, roll, truck, orbit, pan, fov)")),
    };
    place(state, id, &o, fov, free_distance, label)
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    match cmd {
        Command::CreateCameraFromView => create_from_view(state),
        Command::NavigateCamera { op, x, y } => navigate(state, &op, x, y.unwrap_or(0.0)),
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
