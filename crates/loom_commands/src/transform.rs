//! Transform frames: the reference coordinate system (Main Toolbar list),
//! the transform centre (Use Center flyout) and axis constraints, as
//! applied to Move, Rotate and Scale.

use crate::state::{AppState, CenterMode, CoordSystem, SelectionMode};
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
fn screen_axes(state: &AppState, viewport: usize) -> Mat3 {
    let vp = &state.viewports[viewport.min(state.viewports.len() - 1)];
    let cam = &vp.camera;
    let toward_viewer = (cam.eye_position() - cam.target).normalize_or_zero();
    let right = Vec3::Z.cross(toward_viewer).normalize_or_zero();
    let right = if right.length_squared() < 1e-6 { Vec3::X } else { right };
    let up = toward_viewer.cross(right).normalize_or_zero();
    Mat3::from_cols(right, up, toward_viewer)
}

fn rotation_only(m: Mat4) -> Mat3 {
    let (_, q, _) = m.to_scale_rotation_translation();
    Mat3::from_quat(q)
}

/// The frame for the active transform of `id` (None = the selection as a
/// whole) in `viewport`, from the current coordinate system and centre.
pub fn frame_for(state: &AppState, viewport: usize, id: Option<NodeId>) -> TransformFrame {
    frame_for_system(state, viewport, id, state.coord_system)
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
        CoordSystem::Local | CoordSystem::Gimbal => match id {
            Some(i) => rotation_only(state.scene.world_matrix(i)),
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
    let origin = match state.center_mode {
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
