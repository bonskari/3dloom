//! Validated contact metadata. Skeleton evaluation is intentionally separate.
use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactSpace {
    Body,
    World,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContactReference {
    pub space: ContactSpace,
    pub position: Vec3,
    pub rotation: Quat,
    pub pole_position: Vec3,
}
impl ContactReference {
    fn validate(&self) -> Result<(), String> {
        if !self.position.is_finite()
            || !self.pole_position.is_finite()
            || !self.rotation.is_finite()
            || (self.rotation.length_squared() - 1.).abs() > 1e-4
        {
            return Err("contact reference requires a finite position and unit rotation".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContactKey {
    pub time: f32,
    pub reference: u64,
    pub blend: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContactState {
    pub reference: u64,
    pub blend: f32,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ContactData")]
pub struct ContactTrack {
    references: BTreeMap<u64, ContactReference>,
    keys: Vec<ContactKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    constant: Option<ContactState>,
}
#[derive(Deserialize)]
struct ContactData {
    references: BTreeMap<u64, ContactReference>,
    keys: Vec<ContactKey>,
    #[serde(default)]
    constant: Option<ContactState>,
}
impl TryFrom<ContactData> for ContactTrack {
    type Error = String;
    fn try_from(data: ContactData) -> Result<Self, String> {
        let mut track = Self::new(data.references, data.keys)?;
        if let Some(state) = &data.constant {
            track.validate_value(state.reference, state.blend)?;
        }
        track.constant = data.constant;
        Ok(track)
    }
}
impl ContactTrack {
    pub fn new(
        references: BTreeMap<u64, ContactReference>,
        keys: Vec<ContactKey>,
    ) -> Result<Self, String> {
        let track = Self {
            references,
            keys,
            constant: None,
        };
        for reference in track.references.values() {
            reference.validate()?;
        }
        for key in &track.keys {
            track.validate_key(key)?;
        }
        if track
            .keys
            .windows(2)
            .any(|pair| pair[0].time >= pair[1].time)
        {
            return Err("contact key times must be strictly increasing".into());
        }
        Ok(track)
    }
    fn validate_value(&self, reference: u64, blend: f32) -> Result<(), String> {
        if !blend.is_finite() || !(0. ..=1.).contains(&blend) {
            return Err("contact blend must be finite and in 0..=1".into());
        }
        if !self.references.contains_key(&reference) {
            return Err("contact references a missing pose".into());
        }
        Ok(())
    }
    fn validate_key(&self, key: &ContactKey) -> Result<(), String> {
        if !key.time.is_finite() {
            return Err("contact time must be finite".into());
        }
        self.validate_value(key.reference, key.blend)
    }
    pub fn constant(reference: ContactReference, blend: f32) -> Result<Self, String> {
        let mut track = Self::new([(1, reference)].into(), vec![])?;
        track.validate_value(1, blend)?;
        track.constant = Some(ContactState {
            reference: 1,
            blend,
        });
        Ok(track)
    }
    pub fn constant_state(&self) -> Option<&ContactState> {
        self.constant.as_ref()
    }
    /// Freeze without a key time; retain a shared reference space, or capture
    /// the current world target when interpolating between different spaces.
    pub fn frozen(&self, time: f32, body_frame: glam::Mat4) -> Result<Self, String> {
        let sample = self.sample(time, body_frame)?;
        if self.keys.is_empty() {
            return Ok(self.clone());
        }
        match sample {
            Some(sample) => {
                let key = self.active_key(time)?;
                let mut space = key
                    .map(|key| self.references[&key.reference].space.clone())
                    .or_else(|| {
                        self.constant
                            .as_ref()
                            .map(|state| self.references[&state.reference].space.clone())
                    })
                    .unwrap_or(ContactSpace::World);
                if let Some(key) = key {
                    if let Some(next) = self
                        .keys
                        .get(self.keys.partition_point(|key| key.time <= time))
                    {
                        if time > key.time && self.references[&next.reference].space != space {
                            space = ContactSpace::World;
                        }
                    }
                }
                let reference = ContactReference::capture(
                    space,
                    body_frame,
                    glam::Mat4::from_rotation_translation(sample.rotation, sample.position),
                    sample.pole_position,
                )?;
                Self::constant(reference, sample.blend)
            }
            None => Ok(Self::default()),
        }
    }
    fn index(&self, time: f32) -> Result<usize, String> {
        if !time.is_finite() {
            return Err("contact time must be finite".into());
        }
        self.keys
            .iter()
            .position(|key| key.time == time)
            .ok_or_else(|| "contact key not found".into())
    }
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty() && self.references.is_empty()
    }
    pub fn keys(&self) -> &[ContactKey] {
        &self.keys
    }
    pub fn references(&self) -> &BTreeMap<u64, ContactReference> {
        &self.references
    }
    pub fn insert_reference(&mut self, id: u64, reference: ContactReference) -> Result<(), String> {
        reference.validate()?;
        if self.references.contains_key(&id) {
            return Err("contact reference already exists".into());
        }
        self.references.insert(id, reference);
        Ok(())
    }
    pub fn update_reference(&mut self, id: u64, reference: ContactReference) -> Result<(), String> {
        reference.validate()?;
        let target = self
            .references
            .get_mut(&id)
            .ok_or("contact reference not found")?;
        *target = reference;
        Ok(())
    }
    pub fn insert_key(&mut self, key: ContactKey) -> Result<(), String> {
        self.validate_key(&key)?;
        let index = self.keys.partition_point(|old| old.time < key.time);
        if self.keys.get(index).is_some_and(|old| old.time == key.time) {
            return Err("contact key time already exists".into());
        }
        self.keys.insert(index, key);
        Ok(())
    }
    pub fn update_key(&mut self, key: ContactKey) -> Result<(), String> {
        self.validate_key(&key)?;
        let index = self.index(key.time)?;
        self.keys[index] = key;
        Ok(())
    }
    pub fn move_key(&mut self, from: f32, to: f32) -> Result<(), String> {
        let index = self.index(from)?;
        if !to.is_finite() {
            return Err("contact time must be finite".into());
        }
        if from == to {
            return Ok(());
        }
        if self.keys.iter().any(|key| key.time == to) {
            return Err("contact key time already exists".into());
        }
        let mut key = self.keys.remove(index);
        key.time = to;
        let index = self.keys.partition_point(|old| old.time < to);
        self.keys.insert(index, key);
        Ok(())
    }
    pub fn remove_key(&mut self, time: f32) -> Result<(), String> {
        let index = self.index(time)?;
        let key = self.keys.remove(index);
        if !self
            .keys
            .iter()
            .any(|other| other.reference == key.reference)
            && !self
                .constant
                .as_ref()
                .is_some_and(|state| state.reference == key.reference)
        {
            self.references.remove(&key.reference);
        }
        Ok(())
    }
    /// Held metadata lookup, including time-dependence before the first key.
    pub fn active_key(&self, time: f32) -> Result<Option<&ContactKey>, String> {
        if !time.is_finite() {
            return Err("contact time must be finite".into());
        }
        if !self.keys.is_empty() {
            crate::anim::note_animated_read();
        }
        let end = self.keys.partition_point(|key| key.time <= time);
        Ok(end.checked_sub(1).map(|index| &self.keys[index]))
    }
}

/// Effective world-space input shared by contact solving and controller editing.
#[derive(Debug, Clone)]
pub struct ContactSample {
    pub position: Vec3,
    pub rotation: Quat,
    pub pole_position: Vec3,
    pub blend: f32,
}

/// A proper polar rotation with a fixed X reflection convention for mirrored frames.
/// The remaining stretch is retained separately when replacing an end orientation.
pub fn affine_rotation(matrix: glam::Mat4) -> Result<Quat, String> {
    let mut basis = glam::Mat3::from_mat4(matrix);
    if !matrix.is_finite()
        || !matrix.row(3).abs_diff_eq(glam::Vec4::W, 1e-6)
        || !basis.inverse().is_finite()
    {
        return Err("contact frame must be finite, affine and invertible".into());
    }
    if basis.determinant() < 0. {
        basis.x_axis = -basis.x_axis;
    }
    let scale = basis
        .x_axis
        .length()
        .max(basis.y_axis.length())
        .max(basis.z_axis.length());
    basis *= scale.recip();
    for _ in 0..64 {
        let next = (basis + basis.inverse().transpose()) * 0.5;
        if !next.is_finite() {
            return Err("contact frame rotation is undefined".into());
        }
        let converged = next.abs_diff_eq(basis, 1e-6);
        basis = next;
        if converged {
            break;
        }
    }
    if !(basis.transpose() * basis).abs_diff_eq(glam::Mat3::IDENTITY, 1e-4) {
        return Err("contact frame rotation did not converge".into());
    }
    Ok(Quat::from_mat3(&basis).normalize())
}

impl ContactTrack {
    pub fn sample(
        &self,
        time: f32,
        body_frame: glam::Mat4,
    ) -> Result<Option<ContactSample>, String> {
        let key = self.active_key(time)?;
        let state = key
            .map(|key| ContactState {
                reference: key.reference,
                blend: key.blend,
            })
            .or_else(|| self.constant.clone());
        let Some(state) = state else {
            return Ok(None);
        };
        let resolve = |id| -> Result<ContactSample, String> {
            let reference = &self.references[&id];
            let frame = match reference.space {
                ContactSpace::World => glam::Mat4::IDENTITY,
                ContactSpace::Body => body_frame,
            };
            let rotation = affine_rotation(frame * glam::Mat4::from_quat(reference.rotation))?;
            let position = frame.transform_point3(reference.position);
            let pole_position = frame.transform_point3(reference.pole_position);
            if !position.is_finite() || !pole_position.is_finite() {
                return Err("contact frame produces a nonfinite target".into());
            }
            Ok(ContactSample {
                position,
                rotation,
                pole_position,
                blend: state.blend,
            })
        };
        let mut result = resolve(state.reference)?;
        let next = key.and_then(|_| {
            self.keys
                .get(self.keys.partition_point(|other| other.time <= time))
        });
        if let (Some(key), Some(next)) = (key, next) {
            let amount = ((time as f64 - key.time as f64) / (next.time as f64 - key.time as f64))
                .clamp(0., 1.) as f32;
            result.blend = key.blend + (next.blend - key.blend) * amount;
            if next.reference != key.reference && amount > 0. {
                let target = resolve(next.reference)?;
                let interpolate = |a: Vec3, b: Vec3| {
                    let amount = amount as f64;
                    Vec3::new(
                        ((1. - amount) * a.x as f64 + amount * b.x as f64) as f32,
                        ((1. - amount) * a.y as f64 + amount * b.y as f64) as f32,
                        ((1. - amount) * a.z as f64 + amount * b.z as f64) as f32,
                    )
                };
                result.position = interpolate(result.position, target.position);
                result.rotation = result.rotation.slerp(target.rotation, amount).normalize();
                result.pole_position = interpolate(result.pole_position, target.pole_position);
            }
        }
        Ok(Some(result))
    }
}

/// Evaluation failure for a specific rig limb; runtime-only, never saved in a scene.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ContactEvaluationError {
    pub rig: u32,
    pub limb: String,
    pub message: String,
}

impl ContactReference {
    /// Capture the solved world pose without interpreting a sheared parent as TRS.
    pub fn capture(
        space: ContactSpace,
        body_frame: glam::Mat4,
        end_world: glam::Mat4,
        pole_world: Vec3,
    ) -> Result<Self, String> {
        let frame = match space {
            ContactSpace::World => glam::Mat4::IDENTITY,
            ContactSpace::Body => body_frame,
        };
        let rotation = rotation_in_frame(frame, affine_rotation(end_world)?)?;
        let inverse = frame.inverse();
        let result = Self {
            space,
            position: inverse.transform_point3(end_world.w_axis.truncate()),
            rotation,
            pole_position: inverse.transform_point3(pole_world),
        };
        result.validate()?;
        Ok(result)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContactControlKind {
    Goal,
    Pole,
}

#[derive(Debug, Clone)]
pub struct ContactControl {
    pub rig: u32,
    pub limb: usize,
    pub kind: ContactControlKind,
    pub sample: ContactSample,
}

impl ContactTrack {
    /// References with positive weight at this time, including a constant base.
    pub fn active_references(&self, time: f32) -> Result<Vec<u64>, String> {
        if let Some(key) = self.active_key(time)? {
            let mut ids = vec![key.reference];
            if time > key.time {
                if let Some(next) = self
                    .keys
                    .get(self.keys.partition_point(|key| key.time <= time))
                {
                    if next.reference != key.reference {
                        ids.push(next.reference);
                    }
                }
            }
            Ok(ids)
        } else {
            Ok(self
                .constant
                .as_ref()
                .map(|state| vec![state.reference])
                .unwrap_or_default())
        }
    }
}

/// Convert a proper world orientation to a local orientation using the same
/// fixed reflection convention as contact sampling.
pub fn rotation_in_frame(frame: glam::Mat4, desired: Quat) -> Result<Quat, String> {
    if !desired.is_finite() || (desired.length_squared() - 1.).abs() > 1e-4 {
        return Err("contact rotation must be finite and unit length".into());
    }
    let frame_rotation = affine_rotation(frame)?;
    let mut rotation = (frame_rotation.inverse() * desired).normalize();
    if frame.determinant() < 0. {
        let reflection = glam::Mat3::from_diagonal(Vec3::new(-1., 1., 1.));
        rotation = Quat::from_mat3(&(reflection * glam::Mat3::from_quat(rotation) * reflection))
            .normalize();
    }
    Ok(rotation)
}
