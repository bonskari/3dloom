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
}
impl ContactReference {
    fn validate(&self) -> Result<(), String> {
        if !self.position.is_finite()
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
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ContactData")]
pub struct ContactTrack {
    references: BTreeMap<u64, ContactReference>,
    keys: Vec<ContactKey>,
}
#[derive(Deserialize)]
struct ContactData {
    references: BTreeMap<u64, ContactReference>,
    keys: Vec<ContactKey>,
}
impl TryFrom<ContactData> for ContactTrack {
    type Error = String;
    fn try_from(data: ContactData) -> Result<Self, String> {
        Self::new(data.references, data.keys)
    }
}
impl ContactTrack {
    pub fn new(
        references: BTreeMap<u64, ContactReference>,
        keys: Vec<ContactKey>,
    ) -> Result<Self, String> {
        let track = Self { references, keys };
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
    fn validate_key(&self, key: &ContactKey) -> Result<(), String> {
        if !key.time.is_finite() || !key.blend.is_finite() || !(0. ..=1.).contains(&key.blend) {
            return Err("contact key requires finite time and blend in 0..=1".into());
        }
        if !self.references.contains_key(&key.reference) {
            return Err("contact key references a missing pose".into());
        }
        Ok(())
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
