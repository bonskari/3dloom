//! Effective contact transforms and atomic controller edits.
use crate::AppState;
use glam::Vec3;
use loom_scene::loomrig_contact::{ContactReference, ContactSpace};
use loom_scene::{Change, Scene};
use serde_json::{json, Value};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlSpace {
    Local,
    World,
}

#[derive(Clone, Copy)]
pub(crate) struct ControlEdit {
    pub space: ControlSpace,
    pub id: loom_scene::NodeId,
    pub position: Option<[f32; 3]>,
    pub rotation_deg: Option<[f32; 3]>,
    pub scale: Option<[f32; 3]>,
}

pub(crate) fn set_control_transform(
    state: &mut AppState,
    id: loom_scene::NodeId,
    position: Option<[f32; 3]>,
    rotation_deg: Option<[f32; 3]>,
    scale: Option<[f32; 3]>,
) -> Result<Option<Value>, String> {
    let Some(control) = state.scene.contact_control(id)? else {
        return Ok(None);
    };
    let changes = control_changes(
        state,
        &[ControlEdit {
            space: ControlSpace::Local,
            id,
            position,
            rotation_deg,
            scale,
        }],
        &[],
    )?;
    if !changes.is_empty() {
        crate::execute::commit(state, "Transform Contact", Change::Group(changes))?;
    }
    Ok(Some(json!({"rig":control.rig,"controller":id.0})))
}

/// Build one metadata change for the whole selection, so sibling goal/pole
/// edits cannot overwrite each other's shared-reference fields.
pub(crate) fn control_changes(
    state: &AppState,
    edits: &[ControlEdit],
    pending: &[Change],
) -> Result<Vec<Change>, String> {
    use glam::{Mat4, Quat};
    use loom_scene::loomrig_contact::{affine_rotation, ContactControlKind};
    if edits.is_empty() {
        return Ok(Vec::new());
    }
    let before = state.scene.loom_rigs.clone();
    let mut after = before.clone();
    let mut rig_ids = std::collections::BTreeSet::new();
    for edit in edits {
        let control = state
            .scene
            .contact_control(edit.id)?
            .ok_or("active contact controller not found")?;
        rig_ids.insert(control.rig);
        for value in [edit.position, edit.rotation_deg, edit.scale]
            .into_iter()
            .flatten()
        {
            if !Vec3::from(value).is_finite() {
                return Err("contact transform must be finite".into());
            }
        }
    }
    let needs_probe = !pending.is_empty()
        || rig_ids.len() > 1
        || edits.iter().any(|edit| {
            state
                .scene
                .node(edit.id)
                .is_some_and(|node| node.parent.is_some())
        });
    // Complex group edits evaluate the actual scene in a private draft. The
    // ordinary single-rig, unparented goal/pole gesture never clones geometry.
    let mut probe = needs_probe.then(|| state.scene.clone());
    if let Some(probe) = &mut probe {
        let pending = Change::Group(pending.to_vec()).merged_transforms(&state.scene);
        validate_pending_transforms(&pending)?;
        pending.apply(probe)?;
    }
    for _ in 0..=before.len().max(1) {
        let mut changes = Vec::new();
        for edit in edits {
            let evaluation = probe.as_ref().unwrap_or(&state.scene);
            let ControlEdit {
                space,
                id,
                position,
                rotation_deg,
                scale,
            } = *edit;
            let control = evaluation
                .contact_control(id)?
                .ok_or("active contact controller not found")?;
            let rig_index = state
                .scene
                .loom_rigs
                .iter()
                .position(|rig| rig.id == control.rig)
                .ok_or("LoomRig not found")?;
            let rig = &state.scene.loom_rigs[rig_index];
            let body_frame = evaluation.world_matrix(rig.mapping.root);
            let parent = evaluation.parent_frame(id);
            let current = if control.kind == ContactControlKind::Goal {
                control.sample.position
            } else {
                control.sample.pole_position
            };
            let desired_position = position
                .map(|p| {
                    if space == ControlSpace::World {
                        Vec3::from(p)
                    } else {
                        parent.transform_point3(Vec3::from(p))
                    }
                })
                .unwrap_or(current);
            let desired_rotation = if control.kind == ContactControlKind::Goal {
                if let Some(r) = rotation_deg {
                    let r = Vec3::from(r) * (std::f32::consts::PI / 180.);
                    let rotation = Quat::from_euler(glam::EulerRot::XYZ, r.x, r.y, r.z);
                    if space == ControlSpace::World {
                        rotation
                    } else {
                        affine_rotation(parent * Mat4::from_quat(rotation))?
                    }
                } else {
                    control.sample.rotation
                }
            } else {
                control.sample.rotation
            };
            let delta = desired_position - current;
            let rotation_delta = (desired_rotation * control.sample.rotation.inverse()).normalize();
            let track = &mut after[rig_index].limbs[control.limb].contacts;
            if position.is_some()
                || (control.kind == ContactControlKind::Goal && rotation_deg.is_some())
            {
                for reference in track.active_references(state.scene.time())? {
                    let mut pose = track.references()[&reference].clone();
                    let local_delta = match pose.space {
                        ContactSpace::World => delta,
                        ContactSpace::Body => body_frame.inverse().transform_vector3(delta),
                    };
                    if control.kind == ContactControlKind::Goal {
                        if position.is_some() {
                            pose.position += local_delta;
                        }
                        if rotation_deg.is_some() {
                            pose.rotation = match pose.space {
                                ContactSpace::World => (rotation_delta * pose.rotation).normalize(),
                                ContactSpace::Body => {
                                    let world_rotation = affine_rotation(
                                        body_frame * Mat4::from_quat(pose.rotation),
                                    )?;
                                    ContactReference::capture(
                                        ContactSpace::Body,
                                        body_frame,
                                        Mat4::from_rotation_translation(
                                            (rotation_delta * world_rotation).normalize(),
                                            body_frame.transform_point3(pose.position),
                                        ),
                                        body_frame.transform_point3(pose.pole_position),
                                    )?
                                    .rotation
                                }
                            };
                        }
                    } else if position.is_some() {
                        pose.pole_position += local_delta;
                    }
                    track.update_reference(reference, pose)?;
                }
            }
            let node = state.scene.node(id).ok_or("rig control is missing")?;
            let mut visual = node.transform.clone();
            if let Some(scale) = scale {
                visual.scale.set(Vec3::from(scale));
            }
            if control.kind == ContactControlKind::Pole {
                if let Some(rotation) = rotation_deg {
                    let rotation = if space == ControlSpace::Local {
                        Vec3::from(rotation)
                    } else {
                        let r = Vec3::from(rotation) * (std::f32::consts::PI / 180.);
                        let world = Quat::from_euler(glam::EulerRot::XYZ, r.x, r.y, r.z);
                        let local = loom_scene::loomrig_contact::rotation_in_frame(parent, world)?;
                        let (x, y, z) = local.to_euler(glam::EulerRot::XYZ);
                        Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
                    };
                    visual.rotation.set(rotation);
                }
            }
            let visual_change = (visual != node.transform).then(|| Change::SetTransform {
                id,
                before: node.transform.clone(),
                after: visual,
            });
            if let Some(change) = &visual_change {
                changes.push(change.clone());
            }
            if let Some(probe) = &mut probe {
                probe.loom_rigs = after.clone();
                probe.bump_version();
                if let Some(change) = visual_change {
                    change.apply(probe)?;
                }
            }
        }
        if probe
            .as_ref()
            .is_none_or(|probe| control_targets_match(probe, edits))
        {
            if state.auto_key {
                let time=state.current_frame as f32;
                let evaluation=probe.as_ref().unwrap_or(&state.scene);
                for (ri,rig) in after.iter_mut().enumerate() {
                    let body_frame=evaluation.world_matrix(rig.mapping.root);
                    for (li,limb) in rig.limbs.iter_mut().enumerate() {
                        let original=&before[ri].limbs[li].contacts;
                        if limb.contacts==*original {continue;}
                        let target=limb.contacts.sample(time,body_frame)?.ok_or("contact target has no active pose")?;
                        let mut keyed=original.clone();
                        crate::loomrig_contacts::key_contact_target(&mut keyed,time,&target,body_frame)?;
                        limb.contacts=keyed;
                    }
                }
            }
            if before != after {
                changes.push(Change::SetLoomRigs { before, after });
            }
            return Ok(changes);
        }
    }
    Err("dependent contact transforms did not converge; no changes were committed".into())
}

pub(crate) fn local_control_transform(
    state: &AppState,
    id: loom_scene::NodeId,
) -> Result<Option<loom_scene::Transform>, String> {
    use glam::Mat4;
    use loom_scene::loomrig_contact::ContactControlKind;
    let Some(control) = state.scene.contact_control(id)? else {
        return Ok(None);
    };
    let node = state.scene.node(id).ok_or("rig control is missing")?;
    let parent = state.scene.parent_frame(id);
    let position = if control.kind == ContactControlKind::Goal {
        control.sample.position
    } else {
        control.sample.pole_position
    };
    let local = parent.inverse().transform_point3(position);
    if !local.is_finite() {
        return Err("contact controller parent frame must be invertible".into());
    }
    let mut transform = node.transform.clone();
    transform.position = loom_scene::Param::new(local);
    if control.kind == ContactControlKind::Goal {
        let q = ContactReference::capture(
            ContactSpace::Body,
            parent,
            Mat4::from_rotation_translation(control.sample.rotation, position),
            control.sample.pole_position,
        )?
        .rotation;
        let (x, y, z) = q.to_euler(glam::EulerRot::XYZ);
        transform.rotation =
            loom_scene::Param::new(Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees()));
    }
    Ok(Some(transform))
}

/// Translate a normal transform gesture into the controller's local fields.
pub(crate) fn matrix_control_edit(
    state: &AppState,
    id: loom_scene::NodeId,
    matrix: glam::Mat4,
    label: &str,
) -> Result<Option<ControlEdit>, String> {
    use glam::Mat4;
    use loom_scene::loomrig_contact::{affine_rotation, ContactControlKind};
    let Some(control) = state.scene.contact_control(id)? else {
        return Ok(None);
    };
    if state.affect_pivot_only
        || (state.dont_affect_children && !state.scene.children(id).is_empty())
    {
        return Err("leave pivot-only or child compensation mode to edit a contact target".into());
    }
    let node = state.scene.node(id).ok_or("rig control is missing")?;
    if node.locks == loom_scene::Axes3::NONE {
        let world = state.scene.world_matrix(id);
        let position = matrix.transform_point3(world.w_axis.truncate());
        let (rotation_deg, scale) = match label {
            "Rotate" => {
                let rotation =
                    (affine_rotation(matrix)? * world_control_rotation(state, id)?).normalize();
                let (x, y, z) = rotation.to_euler(glam::EulerRot::XYZ);
                (Some([x.to_degrees(), y.to_degrees(), z.to_degrees()]), None)
            }
            "Scale" => (
                None,
                Some(
                    glyph_scale(
                        node.transform.scale.get(),
                        world_control_rotation(state, id)?,
                        matrix,
                    )
                    .to_array(),
                ),
            ),
            _ => (None, None),
        };
        return Ok(Some(ControlEdit {
            space: ControlSpace::World,
            id,
            position: Some(position.to_array()),
            rotation_deg,
            scale,
        }));
    }
    let before = local_control_transform(state, id)?.ok_or("contact controller not found")?;
    let node = state.scene.node(id).ok_or("rig control is missing")?;
    let parent = state.scene.parent_frame(id);
    let world = state.scene.world_matrix(id);
    let target = matrix.transform_point3(world.w_axis.truncate());
    let mut after = before.clone();
    after.position = loom_scene::Param::new(parent.inverse().transform_point3(target));
    if label == "Rotate" {
        let orientation = if control.kind == ContactControlKind::Goal {
            control.sample.rotation
        } else {
            affine_rotation(parent * Mat4::from_quat(node.transform.rotation_quat()))?
        };
        let rotation = (affine_rotation(matrix)? * orientation).normalize();
        let local = ContactReference::capture(
            ContactSpace::Body,
            parent,
            Mat4::from_rotation_translation(rotation, target),
            control.sample.pole_position,
        )?
        .rotation;
        let (x, y, z) = local.to_euler(glam::EulerRot::XYZ);
        let mut degrees = Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees());
        let previous = before.rotation.get();
        for axis in 0..3 {
            degrees[axis] += 360. * ((previous[axis] - degrees[axis]) / 360.).round();
        }
        after.rotation = loom_scene::Param::new(degrees);
    }
    if label == "Scale" {
        let orientation = if control.kind == ContactControlKind::Goal {
            control.sample.rotation
        } else {
            affine_rotation(parent * Mat4::from_quat(node.transform.rotation_quat()))?
        };
        after.scale = loom_scene::Param::new(glyph_scale(before.scale.get(), orientation, matrix));
    }
    let after = crate::execute::apply_locks(&before, after, node.locks);
    let rotation_deg = if label == "Rotate" && after.rotation.get() != before.rotation.get() {
        let r = after.rotation_quat();
        let world = affine_rotation(parent * Mat4::from_quat(r))?;
        let (x, y, z) = world.to_euler(glam::EulerRot::XYZ);
        Some([x.to_degrees(), y.to_degrees(), z.to_degrees()])
    } else {
        None
    };
    Ok(Some(ControlEdit {
        space: ControlSpace::World,
        id,
        position: (after.position.get() != before.position.get()).then(|| parent.transform_point3(after.position.get()).to_array()),
        rotation_deg,
        scale: (label == "Scale" && after.scale.get() != before.scale.get()).then(|| after.scale.get().to_array()),
    }))
}

fn control_targets_match(scene: &Scene, edits: &[ControlEdit]) -> bool {
    use glam::{Mat4, Quat};
    use loom_scene::loomrig_contact::{affine_rotation, ContactControlKind};
    edits.iter().all(|edit| {
        let Ok(Some(control)) = scene.contact_control(edit.id) else {
            return false;
        };
        let parent = scene.parent_frame(edit.id);
        if let Some(position) = edit.position {
            let expected = if edit.space == ControlSpace::World {
                Vec3::from(position)
            } else {
                parent.transform_point3(Vec3::from(position))
            };
            let actual = if control.kind == ContactControlKind::Goal {
                control.sample.position
            } else {
                control.sample.pole_position
            };
            let tolerance = 1e-4 + 32. * f32::EPSILON * expected.abs().max_element().max(1.);
            if !actual.abs_diff_eq(expected, tolerance) {
                return false;
            }
        }
        if let Some(rotation) = edit.rotation_deg {
            let r = Vec3::from(rotation) * (std::f32::consts::PI / 180.);
            let q = Quat::from_euler(glam::EulerRot::XYZ, r.x, r.y, r.z);
            let expected = if edit.space == ControlSpace::World {
                Ok(q)
            } else {
                affine_rotation(parent * Mat4::from_quat(q))
            };
            let actual = if control.kind == ContactControlKind::Goal {
                Ok(control.sample.rotation)
            } else {
                scene
                    .node(edit.id)
                    .ok_or_else(|| "control missing".to_string())
                    .and_then(|node| {
                        affine_rotation(parent * Mat4::from_quat(node.transform.rotation_quat()))
                    })
            };
            let (Ok(expected), Ok(actual)) = (expected, actual) else {
                return false;
            };
            if !actual.abs_diff_eq(expected, 8e-6) && !actual.abs_diff_eq(-expected, 8e-6) {
                return false;
            }
        }
        true
    })
}

fn validate_pending_transforms(change: &Change) -> Result<(), String> {
    match change {
        Change::SetTransform { after, .. } if !after.matrix().is_finite() => {
            Err("contact group transform must be finite".into())
        }
        Change::Group(changes) => {
            for change in changes {
                validate_pending_transforms(change)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Controller orientation is independent of its visual glyph scale.
pub(crate) fn world_control_rotation(
    state: &AppState,
    id: loom_scene::NodeId,
) -> Result<glam::Quat, String> {
    use loom_scene::loomrig_contact::{affine_rotation, ContactControlKind};
    let control = state
        .scene
        .contact_control(id)?
        .ok_or("contact controller not found")?;
    if control.kind == ContactControlKind::Goal {
        Ok(control.sample.rotation)
    } else {
        let node = state.scene.node(id).ok_or("rig control is missing")?;
        affine_rotation(
            state.scene.parent_frame(id) * glam::Mat4::from_quat(node.transform.rotation_quat()),
        )
    }
}

fn glyph_scale(scale: Vec3, orientation: glam::Quat, matrix: glam::Mat4) -> Vec3 {
    let axes = glam::Mat3::from_quat(orientation);
    let factors = Vec3::new(
        matrix.transform_vector3(axes.x_axis).length(),
        matrix.transform_vector3(axes.y_axis).length(),
        matrix.transform_vector3(axes.z_axis).length(),
    );
    let mut scale = scale * factors;
    if matrix.determinant() < 0. {
        scale.x = -scale.x;
    }
    scale
}
