//! Transform frames: the reference coordinate system (Main Toolbar list),
//! the transform centre (Use Center flyout) and axis constraints, as
//! applied to Move, Rotate and Scale.

use crate::state::{AppState, CenterMode, CoordSystem, SelectionMode, SubObjectId, Tool};
use glam::{Mat3, Mat4, Vec3};
use loom_scene::NodeId;

/// World-space frame a transform is expressed in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransformFrame {
    /// Transform centre (rotation/scale pivot) in world space.
    pub origin: Vec3,
    /// Columns = the X, Y, Z axes of the reference coordinate system.
    pub axes: Mat3,
}

/// Screen axes (right, up, toward the viewer) of a viewport camera.
pub(crate) fn screen_axes(state: &AppState, viewport: usize) -> Mat3 {
    let vp = &state.viewports[viewport.min(state.viewports.len() - 1)];
    let cam = &vp.camera;
    let toward_viewer = (cam.eye_position() - cam.target).normalize_or_zero();
    let right = Vec3::Z.cross(toward_viewer).normalize_or_zero();
    let right = if right.length_squared() < 1e-6 { Vec3::X } else { right };
    let up = toward_viewer.cross(right).normalize_or_zero();
    Mat3::from_cols(right, up, toward_viewer)
}

/// The active view's axes (X right, Y up, Z toward the viewer).
pub fn view_axes(state: &AppState, viewport: usize) -> Mat3 {
    let kind = state.viewports[viewport.min(state.viewports.len() - 1)].kind;
    if kind.angles().is_some() {
        // Axis views: exactly the view's plane.
        let (u, v, n) = kind.construction_plane();
        return Mat3::from_cols(u, v, n);
    }
    screen_axes(state, viewport)
}

fn rotation_only(m: Mat4) -> Mat3 {
    let (_, q, _) = m.to_scale_rotation_translation();
    Mat3::from_quat(q)
}

/// Whether Auto Key forces each object's pivot as the Rotate / Scale centre
/// (Preferences > Animation > Local Center During Animate).
pub fn animate_forces_pivot(state: &AppState) -> bool {
    state.auto_key && state.prefs.local_center_during_animate && state.selection_mode == SelectionMode::Object
}

/// The frame for the active transform of `id` (None = the selection as a
/// whole) in `viewport`, from the current coordinate system and centre.
/// Gimbal is Parent for the Move and Scale tools.
pub fn frame_for(state: &AppState, viewport: usize, id: Option<NodeId>) -> TransformFrame {
    let system = match state.coord_system {
        CoordSystem::Gimbal if matches!(state.active_tool, Tool::Move | Tool::Scale) => CoordSystem::Parent,
        s => s,
    };
    let mut f = frame_for_system(state, viewport, id, system);
    if animate_forces_pivot(state) && matches!(state.active_tool, Tool::Rotate | Tool::Scale) {
        f.origin = match id {
            Some(i) => state.scene.world_matrix(i).w_axis.truncate(),
            None => state.selection_pivot(),
        };
    }
    f
}

/// Gimbal axes of `id`: the Euler XYZ rotation axes in the world (X is the
/// parent's X, Y turned by X, Z turned by X and Y), so a rotation about one
/// of them changes only that Euler track.
fn gimbal_axes(state: &AppState, id: NodeId) -> Mat3 {
    let parent = rotation_only(state.scene.parent_frame(id));
    let r = state.scene.node(id).map(|n| n.euler_deg()).unwrap_or_default();
    let qx = glam::Quat::from_rotation_x(r.x.to_radians());
    let qy = glam::Quat::from_rotation_y(r.y.to_radians());
    parent * Mat3::from_cols(Vec3::X, qx * Vec3::Y, qx * qy * Vec3::Z)
}

/// Local axes of a sub-object selection: Z along its averaged world normal
/// (faces, or the faces around the selected vertices / edges), X the
/// object's X flattened onto that plane.
fn sub_object_axes(state: &AppState) -> Option<Mat3> {
    let mut n = Vec3::ZERO;
    let mut x_hint = Vec3::ZERO;
    for (node, ids) in state.sub_selection.by_node() {
        let Some(nd) = state.scene.node(node) else { continue };
        let mesh = nd.cage_mesh();
        let m = state.scene.object_matrix(node);
        let normal_m = Mat3::from_mat4(m).inverse().transpose();
        let mut local = Vec3::ZERO;
        for id in ids {
            let faces = match *id {
                SubObjectId::Face(f) => vec![f],
                SubObjectId::Vertex(v) => mesh.faces_around_vertex(v),
                SubObjectId::Edge(e) => mesh.edge_vertices(e).map(|(a, b)| [mesh.faces_around_vertex(a), mesh.faces_around_vertex(b)].concat()).unwrap_or_default(),
            };
            for f in faces.into_iter().filter(|f| (f.0 as usize) < mesh.face_count()) {
                local += mesh.face_normal(f);
            }
        }
        n += (normal_m * local).normalize_or_zero();
        x_hint += m.transform_vector3(Vec3::X);
    }
    let z = n.normalize_or_zero();
    if z == Vec3::ZERO {
        return None;
    }
    let mut x = (x_hint - z * x_hint.dot(z)).normalize_or_zero();
    if x == Vec3::ZERO {
        x = z.any_orthonormal_vector();
    }
    Some(Mat3::from_cols(x, z.cross(x), z))
}

/// `frame_for` with an explicit reference coordinate system.
pub fn frame_for_system(state: &AppState, viewport: usize, id: Option<NodeId>, system: CoordSystem) -> TransformFrame {
    let vp_kind = state.viewports[viewport.min(state.viewports.len() - 1)].kind;
    let axes = match system {
        CoordSystem::View => {
            if vp_kind.angles().is_some() {
                // Axis views: screen-aligned world axes (View = Screen in ortho views).
                let (u, v, n) = vp_kind.construction_plane();
                Mat3::from_cols(u, v, n)
            } else {
                Mat3::IDENTITY
            }
        }
        CoordSystem::Screen => screen_axes(state, viewport),
        CoordSystem::World => Mat3::IDENTITY,
        CoordSystem::Parent => match id.and_then(|i| state.scene.node(i)) {
            Some(_) => rotation_only(state.scene.parent_frame(id.unwrap())),
            None => Mat3::IDENTITY,
        },
        CoordSystem::Working => Mat3::from_quat(state.working_pivot.rotation),
        CoordSystem::Local if state.edit_working_pivot => Mat3::from_quat(state.working_pivot.rotation),
        CoordSystem::Local if state.selection_mode != SelectionMode::Object && !state.slice_plane_mode && !state.sub_selection.is_empty() => {
            sub_object_axes(state).unwrap_or(Mat3::IDENTITY)
        }
        CoordSystem::Local => match id {
            Some(i) => rotation_only(state.scene.world_matrix(i)),
            None => Mat3::IDENTITY,
        },
        CoordSystem::Gimbal => match id {
            Some(i) => gimbal_axes(state, i),
            None => Mat3::IDENTITY,
        },
        CoordSystem::Grid => {
            let (u, v, n) = vp_kind.construction_plane();
            Mat3::from_cols(u, v, n)
        }
        CoordSystem::Pick => match state.pick_coord_node {
            Some(p) => rotation_only(state.scene.world_matrix(p)),
            None => Mat3::IDENTITY,
        },
    };
    let normals = crate::normals::selected_center(state);
    let origin = match state.center_mode {
        // Edit Normals at the Normal level: the gizmo sits on the selected normals.
        _ if normals.is_some() => normals.unwrap_or_default(),
        // Edit Working Pivot: the tools turn the working pivot about itself.
        _ if state.edit_working_pivot => state.working_pivot.origin,
        CenterMode::TransformCoordinate if system == CoordSystem::Working => state.working_pivot.origin,
        _ if state.selection_mode != SelectionMode::Object => state.selection_pivot(),
        CenterMode::PivotPoint => match id {
            Some(i) => state.scene.world_matrix(i).w_axis.truncate(),
            None => state.selection_pivot(),
        },
        CenterMode::Selection => state.selection_pivot(),
        CenterMode::TransformCoordinate => match system {
            CoordSystem::Local | CoordSystem::Gimbal | CoordSystem::Parent => match id {
                Some(i) => state.scene.world_matrix(i).w_axis.truncate(),
                None => state.selection_pivot(),
            },
            CoordSystem::Working => state.working_pivot.origin,
            CoordSystem::Pick => state
                .pick_coord_node
                .map(|p| state.scene.world_matrix(p).w_axis.truncate())
                .unwrap_or(Vec3::ZERO),
            _ => Vec3::ZERO,
        },
    };
    TransformFrame { origin, axes }
}

impl TransformFrame {
    /// A translation given in frame axes.
    pub fn move_matrix(&self, delta_local: Vec3) -> Mat4 {
        Mat4::from_translation(self.axes * delta_local)
    }

    /// Rotation about a frame axis (unit vector in frame space) around the centre.
    pub fn rotate_matrix(&self, axis_local: Vec3, radians: f32) -> Mat4 {
        let axis = (self.axes * axis_local).normalize_or_zero();
        if axis.length_squared() < 1e-6 {
            return Mat4::IDENTITY;
        }
        Mat4::from_translation(self.origin) * Mat4::from_axis_angle(axis, radians) * Mat4::from_translation(-self.origin)
    }

    /// Rotation by a world-space quaternion around the centre (trackball / screen handle).
    pub fn rotate_quat_matrix(&self, q: glam::Quat) -> Mat4 {
        Mat4::from_translation(self.origin) * Mat4::from_quat(q) * Mat4::from_translation(-self.origin)
    }

    /// Scale along the frame axes around the centre.
    pub fn scale_matrix(&self, factors: Vec3) -> Mat4 {
        let basis = Mat4::from_mat3(self.axes);
        Mat4::from_translation(self.origin) * basis * Mat4::from_scale(factors) * basis.inverse() * Mat4::from_translation(-self.origin)
    }
}
