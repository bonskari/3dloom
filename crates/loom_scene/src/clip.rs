//! A source motion grouped on the timeline. Playback still evaluates the
//! members' editable native controllers; this is not a mixer or a baked cache.
use crate::NodeId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationClip {
    pub id: u64,
    pub name: String,
    pub nodes: Vec<NodeId>,
    pub start: f32,
    pub end: f32,
}
