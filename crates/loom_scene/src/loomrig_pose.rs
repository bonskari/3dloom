//! Source-preserving affine FK matching at authored rig pose keys.
use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigPoseKey {
    pub time: f32,
    pub offsets: [Mat4; 3],
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "PoseData")]
pub struct RigPoseTrack {
    keys: Vec<RigPoseKey>,
    #[serde(skip_serializing_if = "Option::is_none")]
    constant: Option<[Mat4; 3]>,
}

#[derive(Deserialize)]
struct PoseData {
    #[serde(default)]
    keys: Vec<RigPoseKey>,
    #[serde(default)]
    constant: Option<[Mat4; 3]>,
}

fn validate(offsets: &[Mat4; 3]) -> Result<(), String> {
    for matrix in offsets {
        crate::loomrig_contact::affine_rotation(*matrix)?;
        if matrix.determinant() <= 0. {
            return Err("FK pose corrections must preserve handedness".into());
        }
    }
    Ok(())
}

impl TryFrom<PoseData> for RigPoseTrack {
    type Error = String;
    fn try_from(data: PoseData) -> Result<Self, String> {
        let mut previous = None;
        for key in &data.keys {
            if !key.time.is_finite() || previous.is_some_and(|time| key.time <= time) {
                return Err("rig pose key times must be finite and strictly increasing".into());
            }
            validate(&key.offsets)?;
            previous = Some(key.time);
        }
        if let Some(offsets) = &data.constant {
            validate(offsets)?;
        }
        Ok(Self {
            keys: data.keys,
            constant: data.constant,
        })
    }
}

impl RigPoseTrack {
    pub fn constant(offsets:[Mat4;3]) -> Result<Self,String> {
        validate(&offsets)?;
        Ok(Self {keys:Vec::new(),constant:Some(offsets)})
    }
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty() && self.constant.is_none()
    }
    pub fn keys(&self) -> &[RigPoseKey] {
        &self.keys
    }

    pub fn set_key(&mut self, key: RigPoseKey) -> Result<(), String> {
        if !key.time.is_finite() {
            return Err("rig pose key time must be finite".into());
        }
        validate(&key.offsets)?;
        match self
            .keys
            .binary_search_by(|k| k.time.partial_cmp(&key.time).unwrap())
        {
            Ok(index) => self.keys[index] = key,
            Err(index) => self.keys.insert(index, key),
        }
        Ok(())
    }

    pub fn move_key(&mut self, from: f32, to: f32) -> Result<(), String> {
        if !from.is_finite() || !to.is_finite() {
            return Err("rig pose key times must be finite".into());
        }
        let Some(index) = self.keys.iter().position(|key| key.time == from) else {
            return Ok(());
        };
        if from == to {
            return Ok(());
        }
        if self.keys.iter().any(|key| key.time == to) {
            return Err("rig pose key time already exists".into());
        }
        let mut key = self.keys.remove(index);
        key.time = to;
        self.set_key(key)
    }

    pub fn remove_key(&mut self, time: f32) {
        self.keys.retain(|key| key.time != time);
    }

    pub fn sample(&self, time: f32) -> Result<[Mat4; 3], String> {
        if !time.is_finite() {
            return Err("rig pose sample time must be finite".into());
        }
        if !self.keys.is_empty() {
            crate::anim::note_animated_read();
        }
        let next = self.keys.partition_point(|key| key.time <= time);
        if next == 0 {
            return Ok(self.constant.unwrap_or([Mat4::IDENTITY; 3]));
        }
        let a = &self.keys[next - 1];
        let Some(b) = self.keys.get(next) else {
            return Ok(a.offsets);
        };
        let weight = ((time as f64 - a.time as f64) / (b.time as f64 - a.time as f64)) as f32;
        let mut result = [Mat4::IDENTITY; 3];
        for (i, matrix) in result.iter_mut().enumerate() {
            let ra = crate::loomrig_contact::affine_rotation(a.offsets[i])?;
            let rb = crate::loomrig_contact::affine_rotation(b.offsets[i])?;
            let sa = (Mat4::from_quat(ra.inverse()) * a.offsets[i]).to_cols_array();
            let sb = (Mat4::from_quat(rb.inverse()) * b.offsets[i]).to_cols_array();
            let lerp = |x: f32, y: f32| {
                ((x as f64) * (1. - weight as f64) + (y as f64) * weight as f64) as f32
            };
            let mut stretch = [0.; 16];
            for j in 0..12 {
                stretch[j] = lerp(sa[j], sb[j]);
            }
            stretch[15] = 1.;
            *matrix =
                Mat4::from_quat(ra.slerp(rb, weight).normalize()) * Mat4::from_cols_array(&stretch);
            let pa = a.offsets[i].w_axis.truncate();
            let pb = b.offsets[i].w_axis.truncate();
            matrix.w_axis =
                Vec3::new(lerp(pa.x, pb.x), lerp(pa.y, pb.y), lerp(pa.z, pb.z)).extend(1.);
        }
        validate(&result)?;
        Ok(result)
    }

    pub fn frozen(&self, time: f32) -> Result<Self, String> {
        if self.is_empty() {
            return Ok(self.clone());
        }
        Ok(Self {
            keys: Vec::new(),
            constant: Some(self.sample(time)?),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Quat;

    #[test]
    fn rotation_interpolation_stays_invertible_and_preserves_shear() {
        let mut track = RigPoseTrack::default();
        let mut shear = Mat4::IDENTITY;
        shear.y_axis.x = 0.4;
        let a = Mat4::from_rotation_z(0.2) * shear;
        let b = Mat4::from_rotation_z(2.8) * shear;
        track
            .set_key(RigPoseKey {
                time: 0.,
                offsets: [a; 3],
            })
            .unwrap();
        track
            .set_key(RigPoseKey {
                time: 10.,
                offsets: [b; 3],
            })
            .unwrap();
        let mid = track.sample(5.).unwrap()[0];
        let ra = crate::loomrig_contact::affine_rotation(a).unwrap();
        let rb = crate::loomrig_contact::affine_rotation(b).unwrap();
        let expected = Mat4::from_quat(ra.slerp(rb, 0.5)) * Mat4::from_quat(ra.inverse()) * a;
        assert!(mid.abs_diff_eq(expected, 1e-5));
        assert!(mid.determinant() > 0.99);
        let mut half_turn = RigPoseTrack::default();
        half_turn
            .set_key(RigPoseKey {
                time: 0.,
                offsets: [Mat4::IDENTITY; 3],
            })
            .unwrap();
        half_turn
            .set_key(RigPoseKey {
                time: 10.,
                offsets: [Mat4::from_quat(Quat::from_rotation_x(std::f32::consts::PI)); 3],
            })
            .unwrap();
        assert!(half_turn.sample(5.).unwrap()[0].determinant() > 0.99);
    }

    #[test]
    fn invalid_pose_keys_are_atomic_and_rejected_during_loading() {
        let mut track = RigPoseTrack::default();
        track
            .set_key(RigPoseKey {
                time: -0.,
                offsets: [Mat4::IDENTITY; 3],
            })
            .unwrap();
        track
            .set_key(RigPoseKey {
                time: 0.,
                offsets: [Mat4::from_translation(Vec3::X); 3],
            })
            .unwrap();
        assert_eq!(track.keys().len(), 1);
        let before = track.clone();
        assert!(track
            .set_key(RigPoseKey {
                time: 1.,
                offsets: [Mat4::ZERO; 3]
            })
            .is_err());
        assert_eq!(track, before);
        let duplicate = serde_json::json!({"keys":[track.keys()[0],track.keys()[0]]});
        assert!(serde_json::from_value::<RigPoseTrack>(duplicate).is_err());
        let invalid = [Mat4::ZERO; 3];
        let singular = serde_json::json!({"keys":[{"time":0.,"offsets":invalid}]});
        assert!(serde_json::from_value::<RigPoseTrack>(singular).is_err());
        assert_eq!(
            serde_json::from_str::<RigPoseTrack>(&serde_json::to_string(&track).unwrap()).unwrap(),
            track
        );
    }

    #[test]
    fn frozen_matches_preserve_pose_without_time_dependency() {
        let mut track = RigPoseTrack::default();
        track
            .set_key(RigPoseKey {
                time: 0.,
                offsets: [Mat4::IDENTITY; 3],
            })
            .unwrap();
        track
            .set_key(RigPoseKey {
                time: 10.,
                offsets: [Mat4::from_rotation_y(0.7); 3],
            })
            .unwrap();
        let frozen = track.frozen(7.).unwrap();
        let expected = track.sample(7.).unwrap();
        let reads = crate::anim::animated_reads();
        for time in [-10., 0., 7., 50.] {
            assert_eq!(frozen.sample(time).unwrap(), expected);
        }
        assert_eq!(crate::anim::animated_reads(), reads);
        assert!(frozen.keys().is_empty());
    }
}
