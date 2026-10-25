//! Standard helpers beyond Dummy / Point: Tape, Protractor, Compass and Grid
//! (the active grid is the construction plane), the line art every helper
//! draws in the viewports, and Tools > Measure Distance.

use crate::execute::commit;
use crate::snap::Plane;
use crate::state::{AppState, PickMode};
use crate::Command;
use glam::{Mat4, Vec3};
use loom_scene::{Change, Node, NodeId, ObjectKind};
use serde_json::{json, Value};
use std::collections::BTreeSet;

/// Wire colour of new helpers.
pub const HELPER_COLOR: [u8; 3] = [120, 200, 120];
/// Size of a tape target's box.
pub const TAPE_TARGET_SIZE: f32 = 2.0;

fn missing(id: NodeId) -> String {
    format!("no node {}", id.0)
}

/// Creates Tape## at `position` and Tape##.Target at `target`; the tape
/// aims its -Z at the target through a LookAt constraint.
pub fn create_tape(state: &mut AppState, position: Vec3, target: Vec3) -> Result<Value, String> {
    let name = state.scene.next_name("Tape");
    let id = NodeId(state.scene.next_id());
    let tid = NodeId(id.0 + 1);
    let mut node = Node::new(&name, ObjectKind::tape(tid));
    node.transform.position.set(position);
    node.wire_color = HELPER_COLOR;
    node.constraints.rotation = Some(crate::camera::look_at_target(tid));
    let mut t = Node::new(&format!("{name}.Target"), ObjectKind::dummy(TAPE_TARGET_SIZE));
    t.transform.position.set(target);
    t.wire_color = HELPER_COLOR;
    commit(state, "Create Tape", Change::Group(vec![Change::AddNode(node), Change::AddNode(t)]))?;
    state.selection = BTreeSet::from([id]);
    state.sub_selection.clear();
    Ok(json!({ "id": id.0, "target": tid.0 }))
}

/// The other end of a tape: a tape's target, or the tape that targets `id`.
pub fn tape_partner(state: &AppState, id: NodeId) -> Option<NodeId> {
    if let Some(ObjectKind::Tape { target: Some(t), .. }) = state.scene.node(id).map(|n| &n.object) {
        return Some(*t);
    }
    state.scene.nodes().find(|n| matches!(n.object, ObjectKind::Tape { target: Some(t), .. } if t == id)).map(|n| n.id)
}

fn pivot(state: &AppState, id: NodeId) -> Vec3 {
    state.scene.world_matrix(id).w_axis.truncate()
}

/// A tape's start and the end of its line: the target, or Specify Length
/// along the direction to it.
pub fn tape_ends(state: &AppState, id: NodeId) -> Option<(Vec3, Vec3)> {
    let ObjectKind::Tape { target, specify_length, length } = &state.scene.node(id)?.object else { return None };
    let start = pivot(state, id);
    let end = match target.filter(|t| state.scene.node(*t).is_some()) {
        Some(t) => pivot(state, t),
        None => start - state.scene.world_matrix(id).z_axis.truncate().normalize_or_zero() * length.get(),
    };
    if specify_length.get() {
        let dir = (end - start).normalize_or(-state.scene.world_matrix(id).z_axis.truncate().normalize_or_zero());
        return Some((start, start + dir * length.get()));
    }
    Some((start, end))
}

/// The angle in degrees at the protractor between the pivots of its two
/// objects (None until both are picked).
pub fn protractor_angle(state: &AppState, id: NodeId) -> Option<f32> {
    let ObjectKind::Protractor { object1, object2 } = &state.scene.node(id)?.object else { return None };
    let (a, b) = (object1.filter(|n| state.scene.node(*n).is_some())?, object2.filter(|n| state.scene.node(*n).is_some())?);
    let at = pivot(state, id);
    let (u, v) = ((pivot(state, a) - at).normalize_or_zero(), (pivot(state, b) - at).normalize_or_zero());
    if u == Vec3::ZERO || v == Vec3::ZERO {
        return Some(0.0);
    }
    Some(u.dot(v).clamp(-1.0, 1.0).acos().to_degrees())
}

fn name_of(state: &AppState, id: Option<NodeId>) -> Value {
    id.and_then(|n| state.scene.node(n)).map(|n| Value::from(n.name.clone())).unwrap_or(Value::Null)
}

/// Read-only values a helper's rollout shows: a tape's Length and World
/// Space Angles, a protractor's object names and Angle.
pub fn readout(state: &AppState, id: NodeId) -> Result<Value, String> {
    let node = state.scene.node(id).ok_or_else(|| missing(id))?;
    match &node.object {
        ObjectKind::Tape { .. } => {
            let (a, b) = tape_ends(state, id).ok_or("not a tape")?;
            let d = (b - a).normalize_or_zero();
            let to_axis = [Vec3::X, Vec3::Y, Vec3::Z].map(|ax| if d == Vec3::ZERO { 0.0 } else { d.dot(ax).clamp(-1.0, 1.0).acos().to_degrees() });
            // XY, YZ, ZX planes have normals Z, X, Y.
            let to_plane = [Vec3::Z, Vec3::X, Vec3::Y].map(|n| d.dot(n).abs().clamp(0.0, 1.0).asin().to_degrees());
            Ok(json!({ "length": (b - a).length(), "to_axis": to_axis, "to_plane": to_plane }))
        }
        ObjectKind::Protractor { object1, object2 } => Ok(json!({
            "angle": protractor_angle(state, id),
            "object1": name_of(state, *object1),
            "object2": name_of(state, *object2),
        })),
        _ => Err(format!("{} is not a Tape or Protractor", node.name)),
    }
}

/// Sets Pick Object 1 / 2 of a protractor (`object` None clears it).
pub fn set_protractor_object(state: &mut AppState, id: NodeId, which: u32, object: Option<NodeId>) -> Result<Value, String> {
    let node = state.scene.node(id).ok_or_else(|| missing(id))?;
    let mut after = node.object.clone();
    let ObjectKind::Protractor { object1, object2 } = &mut after else { return Err(format!("{} is not a Protractor", node.name)) };
    if let Some(o) = object {
        state.scene.node(o).ok_or_else(|| missing(o))?;
        if o == id {
            return Err("pick another object than the protractor".into());
        }
    }
    match which {
        1 => *object1 = object,
        2 => *object2 = object,
        _ => return Err("which is 1 or 2".into()),
    }
    let before = Box::new(node.object.clone());
    commit(state, &format!("Pick Object {which}"), Change::ReplaceObject { id, before, after: Box::new(after) })?;
    readout(state, id)
}

impl AppState {
    /// The activated grid object (None: the home grid), if it still exists.
    pub fn active_grid(&self) -> Option<NodeId> {
        self.active_grid.filter(|g| self.scene.node(*g).is_some_and(|n| matches!(n.object, ObjectKind::Grid { .. })))
    }

    /// The construction plane objects are built on and grid snaps use, as
    /// (origin, u, v, normal): the active grid's displayed plane in every
    /// viewport, else the home grid plane of the viewport's view.
    pub fn construction_plane(&self, viewport: usize) -> Plane {
        if let Some((o, u, v, n)) = self.active_grid().and_then(|g| grid_plane(self, g)) {
            return (o, u, v, n);
        }
        let kind = self.viewports[viewport.min(self.viewports.len().saturating_sub(1))].kind;
        let (u, v, n) = kind.construction_plane();
        (Vec3::ZERO, u, v, n)
    }

    /// Grid spacing of the active grid (the home grid's spacing without one).
    pub fn grid_spacing(&self) -> f32 {
        match self.active_grid().and_then(|g| self.scene.node(g)).map(|n| &n.object) {
            Some(ObjectKind::Grid { spacing, .. }) => spacing.get(),
            _ => self.home_grid.spacing,
        }
    }
}

/// A grid object's displayed plane in world space (origin, u, v, normal).
pub fn grid_plane(state: &AppState, id: NodeId) -> Option<Plane> {
    let ObjectKind::Grid { plane, .. } = &state.scene.node(id)?.object else { return None };
    let world = state.scene.world_matrix(id);
    let (lu, lv) = loom_scene::node::grid_plane_axes(plane.get());
    let u = world.transform_vector3(lu).normalize_or_zero();
    let v = world.transform_vector3(lv).normalize_or_zero();
    Some((world.w_axis.truncate(), u, v, u.cross(v).normalize_or_zero()))
}

/// How a helper line is coloured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// The object (wire) colour; white when selected.
    Wire,
    /// The Target Line colour (Protractor's line to object 1).
    Target,
    /// Axis Tripod lines: X red, Y green, Z blue.
    Axis(u8),
    /// An active grid's lines and its main axes.
    GridMinor,
    GridMajor,
    /// A fixed sRGB colour (biped footsteps: left blue, right green).
    Rgb([u8; 3]),
}

/// The lines a helper draws in the viewports (world space). `units_per_px`
/// gives world units per pixel at a point (Point > Constant Screen Size).
pub fn helper_lines(state: &AppState, id: NodeId, units_per_px: &dyn Fn(Vec3) -> f32) -> Vec<(Vec3, Vec3, LineKind)> {
    let Some(node) = state.scene.node(id) else { return Vec::new() };
    let world = state.scene.world_matrix(id);
    let mut out = Vec::new();
    match &node.object {
        ObjectKind::Point { size, axis_tripod, cross, box_, center_marker, constant_screen_size, .. } => {
            let origin = world.w_axis.truncate();
            // Unscaled axes: the point keeps its size under object scale.
            let (_, rot, _) = world.to_scale_rotation_translation();
            let s = if constant_screen_size.get() { size.get() * units_per_px(origin) } else { size.get() };
            let h = s * 0.5;
            let at = |p: Vec3| origin + rot * p;
            let mut seg = |a: Vec3, b: Vec3, k: LineKind| out.push((at(a), at(b), k));
            if center_marker.get() {
                let m = h * 0.2;
                seg(Vec3::new(-m, -m, 0.0), Vec3::new(m, m, 0.0), LineKind::Wire);
                seg(Vec3::new(-m, m, 0.0), Vec3::new(m, -m, 0.0), LineKind::Wire);
            }
            if axis_tripod.get() {
                for (i, ax) in [Vec3::X, Vec3::Y, Vec3::Z].into_iter().enumerate() {
                    seg(Vec3::ZERO, ax * h, LineKind::Axis(i as u8));
                }
            }
            if cross.get() {
                for ax in [Vec3::X, Vec3::Y, Vec3::Z] {
                    seg(-ax * h, ax * h, LineKind::Wire);
                }
            }
            if box_.get() {
                let c = |i: u32| Vec3::new(if i & 1 == 0 { -h } else { h }, if i & 2 == 0 { -h } else { h }, if i & 4 == 0 { -h } else { h });
                for i in 0..8u32 {
                    for bit in [1u32, 2, 4] {
                        if i & bit == 0 {
                            seg(c(i), c(i | bit), LineKind::Wire);
                        }
                    }
                }
            }
        }
        ObjectKind::Tape { .. } => {
            if let Some((a, b)) = tape_ends(state, id) {
                out.push((a, b, LineKind::Wire));
            }
        }
        ObjectKind::Protractor { object1, object2 } => {
            let at = world.w_axis.truncate();
            if let Some(o) = object1.filter(|n| state.scene.node(*n).is_some()) {
                out.push((at, pivot(state, o), LineKind::Target));
            }
            if let Some(o) = object2.filter(|n| state.scene.node(*n).is_some()) {
                out.push((at, pivot(state, o), LineKind::Wire));
            }
        }
        ObjectKind::Grid { length, width, spacing, plane, .. } => {
            let (lu, lv) = loom_scene::node::grid_plane_axes(plane.get());
            let (hw, hl) = (width.get() * 0.5, length.get() * 0.5);
            let p = |a: f32, b: f32| world.transform_point3(lu * a + lv * b);
            // Outline and the main axes dividing it into quadrants.
            for (a, b) in [((-hw, -hl), (hw, -hl)), ((hw, -hl), (hw, hl)), ((hw, hl), (-hw, hl)), ((-hw, hl), (-hw, -hl))] {
                out.push((p(a.0, a.1), p(b.0, b.1), LineKind::Wire));
            }
            let active = state.active_grid() == Some(id);
            let axis = if active { LineKind::GridMajor } else { LineKind::Wire };
            out.push((p(-hw, 0.0), p(hw, 0.0), axis));
            out.push((p(0.0, -hl), p(0.0, hl), axis));
            if active {
                // The internal grid structure shows while the grid is active.
                let s = spacing.get().max(1e-3);
                let lines = |half: f32| (1..=((half / s).floor() as i32).min(2000)).map(move |i| i as f32 * s);
                for x in lines(hw) {
                    for x in [x, -x] {
                        if x.abs() < hw {
                            out.push((p(x, -hl), p(x, hl), LineKind::GridMinor));
                        }
                    }
                }
                for y in lines(hl) {
                    for y in [y, -y] {
                        if y.abs() < hl {
                            out.push((p(-hw, y), p(hw, y), LineKind::GridMinor));
                        }
                    }
                }
            }
        }
        ObjectKind::Biped(b) => out.extend(crate::biped_footsteps::footprint_lines(b)),
        _ => {}
    }
    out
}

/// Text labels a helper draws: the compass rose's N / E / S / W, a biped's footstep numbers.
pub fn helper_labels(state: &AppState, id: NodeId) -> Vec<(Vec3, String)> {
    let Some(node) = state.scene.node(id) else { return Vec::new() };
    match &node.object {
        ObjectKind::Compass { show_rose, radius } if show_rose.get() => {
            let world = state.scene.world_matrix(id);
            let r = radius.get() * 1.15;
            [(Vec3::Y, "N"), (Vec3::X, "E"), (-Vec3::Y, "S"), (-Vec3::X, "W")].map(|(d, l)| (world.transform_point3(d * r), l.to_owned())).to_vec()
        }
        ObjectKind::Biped(b) => crate::biped_footsteps::footstep_labels(b),
        _ => Vec::new(),
    }
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    match cmd {
        C::CreateTape { position, target } => create_tape(state, Vec3::from(position), Vec3::from(target)),
        C::HelperReadout { id } => readout(state, NodeId(id)),
        C::ProtractorPickObject { which } => {
            if !(1..=2).contains(&which) {
                return Err("which is 1 or 2".into());
            }
            let ok = state.selection.len() == 1 && state.selection.iter().all(|id| state.scene.node(*id).is_some_and(|n| matches!(n.object, ObjectKind::Protractor { .. })));
            if !ok {
                return Err("select one protractor first".into());
            }
            state.pick_mode = Some(PickMode::ProtractorObject(which as u8));
            state.status_prompt = format!("Pick Object {which}");
            Ok(json!({}))
        }
        C::SetProtractorObject { id, which, object } => set_protractor_object(state, NodeId(id), which, object.map(NodeId)),
        C::ActivateGridObject => {
            let grid = state.selection.iter().copied().find(|id| state.scene.node(*id).is_some_and(|n| matches!(n.object, ObjectKind::Grid { .. })));
            let grid = grid.ok_or("select a grid object first")?;
            state.active_grid = Some(grid);
            state.mesh_dirty = true;
            Ok(json!({ "grid": grid.0 }))
        }
        C::ActivateHomeGrid => {
            state.active_grid = None;
            state.mesh_dirty = true;
            Ok(json!({}))
        }
        C::AlignGridToView => {
            let g = state.active_grid().ok_or("activate a grid object first")?;
            // The displayed plane faces the viewer: X right, Y up, Z toward the eye.
            let view = crate::transform::screen_axes(state, state.active_viewport);
            let (lu, lv) = loom_scene::node::grid_plane_axes(match &state.scene.node(g).map(|n| &n.object) {
                Some(ObjectKind::Grid { plane, .. }) => plane.get(),
                _ => 0,
            });
            let local = glam::Mat3::from_cols(lu, lv, lu.cross(lv));
            let rot = glam::Quat::from_mat3(&(view * local.transpose())).normalize();
            let world = state.scene.world_matrix(g);
            let (scale, _, at) = world.to_scale_rotation_translation();
            let local_m = state.scene.parent_frame(g).inverse() * Mat4::from_scale_rotation_translation(scale, rot, at);
            let node = state.scene.node(g).ok_or_else(|| missing(g))?;
            commit(state, "Align Grid to View", Change::SetTransform { id: g, before: node.transform.clone(), after: loom_scene::Transform::from_matrix(local_m) })?;
            Ok(json!({}))
        }
        C::NudgeGridUp => nudge_grid(state, 1.0),
        C::NudgeGridDown => nudge_grid(state, -1.0),
        C::MeasureDistance => {
            state.pick_mode = Some(PickMode::MeasureDistanceA);
            state.measure_from = None;
            state.status_prompt = "Click the first point".into();
            Ok(json!({}))
        }
        _ => Err("not a helper command".into()),
    }
}

/// Nudge Grid Up / Down: the active grid moves along its normal by the
/// Grid Nudge Distance (`sign` +1 up, -1 down).
fn nudge_grid(state: &mut AppState, sign: f32) -> Result<Value, String> {
    let g = state.active_grid().ok_or("activate a grid object first")?;
    let (_, _, _, n) = grid_plane(state, g).ok_or("not a grid")?;
    let world = state.scene.world_matrix(g);
    let local_m = state.scene.parent_frame(g).inverse() * Mat4::from_translation(n * state.prefs.grid_nudge_distance * sign) * world;
    let node = state.scene.node(g).ok_or_else(|| missing(g))?;
    commit(state, "Nudge Grid", Change::SetTransform { id: g, before: node.transform.clone(), after: loom_scene::Transform::from_matrix(local_m) })?;
    Ok(json!({}))
}

/// Tools > Measure Distance: a point pick; the second one reports the
/// distance and the X / Y / Z deltas in the status bar.
pub fn measure_pick(state: &mut AppState, point: Vec3) -> Result<Value, String> {
    match (state.pick_mode, state.measure_from) {
        (Some(PickMode::MeasureDistanceA), _) | (Some(PickMode::MeasureDistanceB), None) => {
            state.measure_from = Some(point);
            state.pick_mode = Some(PickMode::MeasureDistanceB);
            state.status_prompt = "Click the second point".into();
            Ok(json!({ "stage": "B" }))
        }
        (Some(PickMode::MeasureDistanceB), Some(from)) => {
            state.pick_mode = None;
            state.measure_from = None;
            let d = point - from;
            let f = |v: f32| state.format_units(v);
            state.status_prompt = format!("Distance: {}   Delta X: {}   Delta Y: {}   Delta Z: {}", f(d.length()), f(d.x), f(d.y), f(d.z));
            Ok(json!({ "distance": d.length(), "delta": [d.x, d.y, d.z] }))
        }
        _ => Err("Measure Distance is not picking".into()),
    }
}

/// Utilities > Measure over `ids`: the name (or "Multiple Objects Selected"),
/// world dimensions, surface area, volume (with `volume_open` when a mesh
/// has holes), the solid's centre of mass, and the summed shape length.
pub fn measure(state: &AppState, ids: &[NodeId]) -> Value {
    use glam::DVec3;
    let ids: Vec<NodeId> = ids.iter().copied().filter(|id| state.scene.node(*id).is_some()).collect();
    let name = match ids.as_slice() {
        [] => Value::Null,
        [one] => Value::from(state.scene.node(*one).map(|n| n.name.clone()).unwrap_or_default()),
        _ => Value::from("Multiple Objects Selected"),
    };
    let (mut area, mut volume, mut moment) = (0.0f64, 0.0f64, DVec3::ZERO);
    let (mut vsum, mut n_pts) = (DVec3::ZERO, 0usize);
    let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    let (mut open, mut shape_length) = (false, 0.0f64);
    for id in &ids {
        let Some(node) = state.scene.node(*id) else { continue };
        let world = state.scene.object_matrix(*id);
        if let Some(shape) = node.object.shape() {
            shape_length += shape.transformed(world).splines.iter().map(|sp| sp.length() as f64).sum::<f64>();
        }
        let mesh = node.cage_mesh();
        for v in mesh.vertices() {
            let p = world.transform_point3(v.position);
            min = min.min(p);
            max = max.max(p);
            vsum += p.as_dvec3();
            n_pts += 1;
        }
        if mesh.face_count() > 0 && !loom_mesh::select_ops::border_edges(&mesh).is_empty() {
            open = true;
        }
        for f in 0..mesh.face_count() as u32 {
            let Some(pts) = mesh.face_positions(loom_mesh::FaceId(f)) else { continue };
            let pts: Vec<DVec3> = pts.iter().map(|p| world.transform_point3(*p).as_dvec3()).collect();
            for i in 1..pts.len().saturating_sub(1) {
                let (a, b, c) = (pts[0], pts[i], pts[i + 1]);
                area += (b - a).cross(c - a).length() * 0.5;
                // Signed tetrahedron with the origin: its volume and centroid.
                let v = a.dot(b.cross(c)) / 6.0;
                volume += v;
                moment += (a + b + c) * (v / 4.0);
            }
        }
    }
    if n_pts == 0 {
        return json!({ "name": name, "dimensions": Value::Null, "surface_area": 0.0, "volume": 0.0, "volume_open": false, "center_of_mass": Value::Null, "shape_length": shape_length });
    }
    let d = max - min;
    // The solid's centroid; open or flat meshes (no volume) use the vertex average.
    let c = if volume.abs() > 1e-9 && !open { moment / volume } else { vsum / n_pts as f64 };
    json!({
        "name": name,
        "dimensions": [d.x, d.y, d.z],
        "surface_area": area,
        "volume": volume.abs(),
        "volume_open": open,
        "center_of_mass": [c.x, c.y, c.z],
        "shape_length": shape_length,
    })
}

/// `ids` plus the other end of every tape among them.
pub fn with_tape_partners(state: &AppState, ids: Vec<NodeId>) -> Vec<NodeId> {
    let mut out = ids.clone();
    for id in ids {
        if let Some(p) = tape_partner(state, id) {
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// World units per pixel of a viewport at `p` (for Constant Screen Size):
/// `view_proj` maps world to clip space of a `height_px` tall viewport.
pub fn units_per_pixel(view_proj: Mat4, height_px: f32, p: Vec3, up: Vec3) -> f32 {
    let a = view_proj * p.extend(1.0);
    let b = view_proj * (p + up).extend(1.0);
    if a.w <= 1e-6 || b.w <= 1e-6 {
        return 1.0;
    }
    let dy = (b.y / b.w - a.y / a.w).abs() * 0.5 * height_px;
    if dy <= 1e-9 { 1.0 } else { 1.0 / dy }
}
