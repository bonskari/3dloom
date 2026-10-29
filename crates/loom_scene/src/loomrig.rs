//! Non-destructive animator controls layered over the original skeleton tracks.
use crate::{NodeId, Scene};
use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LimbMapping {
    pub name: String,
    pub start: NodeId,
    pub middle: NodeId,
    pub end: NodeId,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HumanoidMapping {
    pub root: NodeId,
    pub pelvis: NodeId,
    pub spine: Vec<NodeId>,
    pub head: NodeId,
    pub limbs: Vec<LimbMapping>,
}
impl HumanoidMapping {
    pub fn metahuman(scene: &Scene, root: NodeId) -> Result<Self, String> {
        let mut members = scene.descendants(root);
        members.push(root);
        let find = |name: &str| -> Result<NodeId, String> {
            let hits: Vec<_> = members
                .iter()
                .copied()
                .filter(|id| {
                    scene.node(*id).is_some_and(|n| {
                        n.object.is_bone() && n.name.rsplit([':', '|']).next() == Some(name)
                    })
                })
                .collect();
            if hits.len() == 1 {
                Ok(hits[0])
            } else {
                Err(format!(
                    "characterization requires one bone named {name}, found {}",
                    hits.len()
                ))
            }
        };
        let mut limbs = vec![];
        for (name, a, b, c) in [
            ("Left Arm", "upperarm_l", "lowerarm_l", "hand_l"),
            ("Right Arm", "upperarm_r", "lowerarm_r", "hand_r"),
            ("Left Leg", "thigh_l", "calf_l", "foot_l"),
            ("Right Leg", "thigh_r", "calf_r", "foot_r"),
        ] {
            limbs.push(LimbMapping {
                name: name.into(),
                start: find(a)?,
                middle: find(b)?,
                end: find(c)?,
            });
        }
        let mapping = Self {
            root,
            pelvis: find("pelvis")?,
            spine: (1..=5)
                .map(|i| find(&format!("spine_{i:02}")))
                .collect::<Result<_, _>>()?,
            head: find("head")?,
            limbs,
        };
        mapping.validate(scene)?;
        Ok(mapping)
    }
    pub fn validate(&self, scene: &Scene) -> Result<(), String> {
        if self.root == self.pelvis || self.root == self.head || self.pelvis == self.head {
            return Err("root, pelvis and head must map to distinct bones".into());
        }
        let mut names = std::collections::BTreeSet::new();
        let mut joints = std::collections::BTreeSet::new();
        for limb in &self.limbs {
            if limb.name.trim().is_empty() || !names.insert(&limb.name) {
                return Err("limb names must be nonempty and unique".into());
            }
            for id in [limb.start, limb.middle, limb.end] {
                if !joints.insert(id) {
                    return Err("limb chains may not share joints".into());
                }
            }
        }

        for id in [self.root, self.pelvis, self.head]
            .into_iter()
            .chain(self.spine.iter().copied())
            .chain(self.limbs.iter().flat_map(|l| [l.start, l.middle, l.end]))
        {
            if !scene.node(id).is_some_and(|n| n.object.is_bone()) {
                return Err(format!("mapped node {} is not a bone", id.0));
            }
            if id != self.root && !scene.ancestors(id).contains(&self.root) {
                return Err("mapped bones must belong to the characterized root".into());
            }
        }
        for l in &self.limbs {
            if scene.node(l.middle).and_then(|n| n.parent) != Some(l.start)
                || scene.node(l.end).and_then(|n| n.parent) != Some(l.middle)
            {
                return Err(format!(
                    "{} requires a contiguous start/middle/end chain",
                    l.name
                ));
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlBinding {
    pub bone: NodeId,
    pub control: NodeId,
    pub rest: Mat4,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigLimb {
    pub mapping: LimbMapping,
    pub goal: NodeId,
    pub pole: NodeId,
    pub goal_rest: Vec3,
    pub pole_rest: Vec3,
    pub solver: crate::ik::IkSolver,
    pub blend: crate::Param<f32>,
    #[serde(default)]
    pub pose_offsets: Vec<crate::Param<Vec3>>,
    #[serde(default, skip_serializing_if = "crate::loomrig_pose::RigPoseTrack::is_empty")]
    pub pose_matches: crate::loomrig_pose::RigPoseTrack,
    #[serde(default, skip_serializing_if = "crate::loomrig_contact::ContactTrack::is_empty")]
    pub contacts: crate::loomrig_contact::ContactTrack,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoomRig {
    pub id: u32,
    pub name: String,
    pub mapping: HumanoidMapping,
    pub master: NodeId,
    pub bindings: Vec<ControlBinding>,
    pub limbs: Vec<RigLimb>,
    #[serde(default, skip_serializing_if = "crate::loomrig_pose::RigPoseFrames::is_empty")]
    pub pose_frames: crate::loomrig_pose::RigPoseFrames,
}

impl LoomRig {
    /// Every skeleton and control node needed by this characterization.
    pub fn node_ids(&self) -> std::collections::BTreeSet<NodeId> {
        let m = &self.mapping;
        [m.root, m.pelvis, m.head, self.master]
            .into_iter()
            .chain(m.spine.iter().copied())
            .chain(m.limbs.iter().flat_map(|l| [l.start, l.middle, l.end]))
            .chain(self.bindings.iter().flat_map(|b| [b.bone, b.control]))
            .chain(self.limbs.iter().flat_map(|l| {
                [
                    l.mapping.start,
                    l.mapping.middle,
                    l.mapping.end,
                    l.goal,
                    l.pole,
                ]
            }))
            .collect()
    }

    /// A partial character has no usable rig; never retain references to nodes left behind.
    pub fn remapped(&self, ids: &std::collections::BTreeMap<NodeId, NodeId>) -> Option<Self> {
        let remap = |id: &mut NodeId| -> Option<()> {
            *id = *ids.get(id)?;
            Some(())
        };
        let mut rig = self.clone();
        remap(&mut rig.mapping.root)?;
        remap(&mut rig.mapping.pelvis)?;
        remap(&mut rig.mapping.head)?;
        for id in &mut rig.mapping.spine {
            remap(id)?;
        }
        for l in &mut rig.mapping.limbs {
            remap(&mut l.start)?;
            remap(&mut l.middle)?;
            remap(&mut l.end)?;
        }
        remap(&mut rig.master)?;
        for b in &mut rig.bindings {
            remap(&mut b.bone)?;
            remap(&mut b.control)?;
        }
        for l in &mut rig.limbs {
            remap(&mut l.mapping.start)?;
            remap(&mut l.mapping.middle)?;
            remap(&mut l.mapping.end)?;
            remap(&mut l.goal)?;
            remap(&mut l.pole)?;
        }
        Some(rig)
    }
}

impl RigLimb {
    pub fn pose_rotation(&self, index: usize) -> glam::Quat {
        let v = self
            .pose_offsets
            .get(index)
            .map(|p| p.get())
            .unwrap_or(Vec3::ZERO);
        glam::Quat::from_euler(
            glam::EulerRot::XYZ,
            v.x.to_radians(),
            v.y.to_radians(),
            v.z.to_radians(),
        )
    }
}
