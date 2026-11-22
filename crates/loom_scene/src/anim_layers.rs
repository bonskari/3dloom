//! Animation Layers (Layer controller) on a node's transform: the Base Layer
//! is the node's own Position / Rotation / Scale; every further layer adds
//! its own offset tracks, scaled by its weight, while it is on.

use crate::node::Transform;
use crate::param::Param;
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// One animation layer above the Base Layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimLayer {
    pub name: String,
    /// The light bulb: off hides the layer's animation.
    pub on: bool,
    /// Locked: its tracks cannot be changed.
    pub locked: bool,
    /// The plus / minus sign: whether the layer counts in the output track.
    pub output: bool,
    /// Weight in percent (animatable).
    pub weight: Param<f32>,
    pub position: Param<Vec3>,
    /// Euler XYZ degrees.
    pub rotation: Param<Vec3>,
    pub scale: Param<Vec3>,
}

impl AnimLayer {
    pub fn new(name: &str) -> Self {
        Self { name: name.into(), on: true, locked: false, output: true, weight: Param::new(100.0), position: Param::new(Vec3::ZERO), rotation: Param::new(Vec3::ZERO), scale: Param::new(Vec3::ZERO) }
    }
}

/// A node's Layer controller: which transform tracks it holds, the Base
/// Layer's lock, the layers above it and the active one (0 = Base Layer).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimLayers {
    pub position: bool,
    pub rotation: bool,
    pub scale: bool,
    pub base_name: String,
    /// The Base Layer's lock: its tracks cannot be changed while it is active.
    pub base_locked: bool,
    pub layers: Vec<AnimLayer>,
    pub active: usize,
}

impl AnimLayers {
    pub fn new(position: bool, rotation: bool, scale: bool) -> Self {
        Self { position, rotation, scale, base_name: "Base Layer".into(), base_locked: false, layers: Vec::new(), active: 0 }
    }

    /// Every layer's name, the Base Layer first.
    pub fn names(&self) -> Vec<String> {
        std::iter::once(self.base_name.clone()).chain(self.layers.iter().map(|l| l.name.clone())).collect()
    }

    /// The transform the node shows: the Base Layer (the node's own tracks)
    /// plus each layer's offsets on the layered tracks, by its weight, while
    /// it is on. `output` true: only the layers included in the output track.
    pub fn apply(&self, base: &Transform, output: bool) -> (Vec3, Vec3, Vec3) {
        let (mut p, mut r, mut s) = (base.position.get(), base.rotation.get(), base.scale.get());
        for l in self.layers.iter().filter(|l| l.on && (!output || l.output)) {
            let w = l.weight.get() / 100.0;
            if self.position {
                p += l.position.get() * w;
            }
            if self.rotation {
                r += l.rotation.get() * w;
            }
            if self.scale {
                s += l.scale.get() * w;
            }
        }
        (p, r, s)
    }

    /// An edit of the transform from `before` to `after` written into the
    /// active layer (when it is not the Base Layer): its offsets grow by the
    /// change over its weight. None when the active layer is locked or is
    /// the Base Layer.
    pub fn take_edit(&mut self, before: &Transform, after: &Transform) -> Option<()> {
        let a = self.active.checked_sub(1)?;
        let l = self.layers.get_mut(a)?;
        if l.locked {
            return None;
        }
        let w = (l.weight.get() / 100.0).max(1e-4);
        let dp = (after.position.get() - before.position.get()) / w;
        let dr = (after.rotation.get() - before.rotation.get()) / w;
        let ds = (after.scale.get() - before.scale.get()) / w;
        if self.position && dp.length_squared() > 0.0 {
            l.position.set(l.position.get() + dp);
        }
        if self.rotation && dr.length_squared() > 0.0 {
            l.rotation.set(l.rotation.get() + dr);
        }
        if self.scale && ds.length_squared() > 0.0 {
            l.scale.set(l.scale.get() + ds);
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_add_their_weighted_offsets_and_take_edits_while_active() {
        let base = Transform::default();
        let mut al = AnimLayers::new(true, true, false);
        al.layers.push(AnimLayer::new("Layer01"));
        al.active = 1;
        let mut moved = base.clone();
        moved.position.set(Vec3::new(10.0, 0.0, 0.0));
        al.take_edit(&base, &moved).expect("taken");
        assert_eq!(al.apply(&base, false).0, Vec3::new(10.0, 0.0, 0.0));
        al.layers[0].weight.set(50.0);
        assert_eq!(al.apply(&base, false).0, Vec3::new(5.0, 0.0, 0.0), "half weight, half the offset");
        al.layers[0].on = false;
        assert_eq!(al.apply(&base, false).0, Vec3::ZERO, "off: hidden");
        al.layers[0].on = true;
        al.layers[0].output = false;
        assert_eq!(al.apply(&base, true).0, Vec3::ZERO, "excluded from the output track");
        al.layers[0].locked = true;
        assert!(al.take_edit(&base, &moved).is_none(), "locked: no edit");
    }
}
