use crate::node::{Node, NodeId, NodeProps, ObjectKind, Transform};
use crate::scene::Scene;
use serde::{Deserialize, Serialize};

/// One reversible scene edit. Mesh edits are whole-object replacements in
/// phase 1a; phase 2 adds finer mesh changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Change {
    AddNode(Node),
    RemoveNode(NodeId),
    SetTransform {
        id: NodeId,
        before: Transform,
        after: Transform,
    },
    SetParent {
        id: NodeId,
        before: Option<NodeId>,
        after: Option<NodeId>,
    },
    ReplaceObject {
        id: NodeId,
        before: Box<ObjectKind>,
        after: Box<ObjectKind>,
    },
    SetName {
        id: NodeId,
        before: String,
        after: String,
    },
    SetWireColor {
        id: NodeId,
        before: [u8; 3],
        after: [u8; 3],
    },
    SetNodeProps {
        id: NodeId,
        before: Box<NodeProps>,
        after: Box<NodeProps>,
    },
    /// (hidden, frozen)
    SetFlags {
        id: NodeId,
        before: (bool, bool),
        after: (bool, bool),
    },
    /// Move To Layer.
    SetLayer {
        id: NodeId,
        before: u32,
        after: u32,
    },
    /// The whole modifier stack of a node.
    SetStack {
        id: NodeId,
        before: Vec<crate::modifier::ModifierSlot>,
        after: Vec<crate::modifier::ModifierSlot>,
    },
    /// The whole layer table and the current layer (Layer Manager edits).
    SetLayers {
        before: (Vec<crate::scene::Layer>, u32),
        after: (Vec<crate::scene::Layer>, u32),
    },
    /// The IK chain table (Hierarchy > IK edits).
    SetIkChains {
        before: Vec<crate::ik::IkChain>,
        after: Vec<crate::ik::IkChain>,
    },
    /// A node's constraints (Animation > Constraints, Motion panel).
    SetConstraints {
        id: NodeId,
        before: Box<crate::constraint::Constraints>,
        after: Box<crate::constraint::Constraints>,
    },
    /// Note tracks, track sets and track flags (Track View).
    SetTrackData {
        before: Box<crate::tracks::TrackData>,
        after: Box<crate::tracks::TrackData>,
    },
    /// A node's visibility track.
    SetVisibility {
        id: NodeId,
        before: Option<crate::param::Param<f32>>,
        after: Option<crate::param::Param<f32>>,
    },
    /// The material library and the Material Editor slots together.
    SetMaterials {
        before: Box<(crate::material::MaterialLibrary, crate::material::MaterialEditorSlots)>,
        after: Box<(crate::material::MaterialLibrary, crate::material::MaterialEditorSlots)>,
    },
    /// A node's assigned material.
    SetMaterial {
        id: NodeId,
        before: Option<crate::material::MaterialId>,
        after: Option<crate::material::MaterialId>,
    },
    /// Render Setup values.
    SetRenderSettings {
        before: Box<crate::render::RenderSettings>,
        after: Box<crate::render::RenderSettings>,
    },
    /// The XRef records (File > XRef Scene / XRef Objects).
    SetXrefs {
        before: Vec<crate::scene::XrefRecord>,
        after: Vec<crate::scene::XrefRecord>,
    },
    Group(Vec<Change>),
}

impl Change {
    /// Rewrites every SetTransform so it lands on the node's existing
    /// controllers (`Transform::merged`): a tool that builds a fresh
    /// transform then keys under Auto Key, and edits or offsets keys
    /// otherwise, instead of wiping the animation.
    pub fn merged_transforms(self, scene: &Scene) -> Change {
        fn walk(c: Change, scene: &Scene, latest: &mut std::collections::HashMap<NodeId, Transform>) -> Change {
            match c {
                Change::SetTransform { id, after, .. } => {
                    let Some(cur) = latest.get(&id).cloned().or_else(|| scene.node(id).map(|n| n.transform.clone())) else {
                        return Change::SetTransform { id, before: after.clone(), after };
                    };
                    let after = cur.merged(&after);
                    latest.insert(id, after.clone());
                    Change::SetTransform { id, before: cur, after }
                }
                Change::Group(steps) => Change::Group(steps.into_iter().map(|s| walk(s, scene, latest)).collect()),
                other => other,
            }
        }
        walk(self, scene, &mut Default::default())
    }

    /// Applies the change. On error the scene is left exactly as it was.
    pub fn apply(&self, scene: &mut Scene) -> Result<(), String> {
        match self {
            Change::AddNode(node) => {
                if node.id != NodeId(0) && scene.node(node.id).is_some() {
                    return Err(format!("node {} already exists", node.id.0));
                }
                scene.add_node(node.clone());
                Ok(())
            }
            Change::RemoveNode(id) => scene
                .remove_node(*id)
                .map(|_| ())
                .ok_or_else(|| missing(*id)),
            Change::SetTransform { id, after, .. } => {
                let node = scene.transform_mut(*id).ok_or_else(|| missing(*id))?;
                node.transform = after.clone();
                Ok(())
            }
            Change::SetParent { id, after, .. } => {
                // Sets only the parent field; callers that want the world
                // transform preserved emit a SetTransform alongside.
                if let Some(p) = after {
                    scene.node(*p).ok_or_else(|| missing(*p))?;
                    if *p == *id || scene.is_descendant(*p, *id) {
                        return Err(format!("linking {} to {} would make a cycle", id.0, p.0));
                    }
                }
                let node = scene.node_mut(*id).ok_or_else(|| missing(*id))?;
                node.parent = *after;
                Ok(())
            }
            Change::ReplaceObject { id, after, .. } => {
                let node = scene.node_mut(*id).ok_or_else(|| missing(*id))?;
                node.object = (**after).clone();
                Ok(())
            }
            Change::SetName { id, after, .. } => {
                let node = scene.node_mut(*id).ok_or_else(|| missing(*id))?;
                node.name = after.clone();
                Ok(())
            }
            Change::SetFlags { id, after, .. } => {
                let node = scene.node_mut(*id).ok_or_else(|| missing(*id))?;
                node.hidden = after.0;
                node.frozen = after.1;
                Ok(())
            }
            Change::SetWireColor { id, after, .. } => {
                let node = scene.node_mut(*id).ok_or_else(|| missing(*id))?;
                node.wire_color = *after;
                Ok(())
            }
            Change::SetNodeProps { id, after, .. } => {
                let node = scene.node_mut(*id).ok_or_else(|| missing(*id))?;
                node.set_props((**after).clone());
                Ok(())
            }
            Change::SetLayer { id, after, .. } => {
                scene.layer(*after).ok_or_else(|| format!("no layer {after}"))?;
                let node = scene.node_mut(*id).ok_or_else(|| missing(*id))?;
                node.layer = *after;
                Ok(())
            }
            Change::SetTrackData { after, .. } => {
                scene.track_data = (**after).clone();
                Ok(())
            }
            Change::SetVisibility { id, after, .. } => {
                let node = scene.transform_mut(*id).ok_or_else(|| missing(*id))?;
                node.visibility = after.clone();
                Ok(())
            }
            Change::SetConstraints { id, after, .. } => {
                let node = scene.transform_mut(*id).ok_or_else(|| missing(*id))?;
                node.constraints = (**after).clone();
                Ok(())
            }
            Change::SetLayers { after, .. } => {
                scene.layers = after.0.clone();
                scene.current_layer = after.1;
                Ok(())
            }
            Change::SetXrefs { after, .. } => {
                scene.xrefs = after.clone();
                Ok(())
            }
            Change::SetMaterials { after, .. } => {
                scene.materials = after.0.clone();
                scene.material_editor = after.1.clone();
                scene.bump_version();
                Ok(())
            }
            Change::SetMaterial { id, after, .. } => {
                if let Some(m) = after {
                    if !scene.materials.contains_key(m) {
                        return Err(format!("no material {}", m.0));
                    }
                }
                let node = scene.node_mut(*id).ok_or_else(|| missing(*id))?;
                node.material = *after;
                Ok(())
            }
            Change::SetRenderSettings { after, .. } => {
                scene.render = (**after).clone();
                Ok(())
            }
            Change::SetIkChains { after, .. } => {
                scene.ik_chains = after.clone();
                scene.bump_version();
                Ok(())
            }
            Change::SetStack { id, after, .. } => {
                let node = scene.node_mut(*id).ok_or_else(|| missing(*id))?;
                node.modifiers = after.clone();
                if node.active_entry.is_some_and(|a| a >= node.modifiers.len()) {
                    node.active_entry = node.modifiers.len().checked_sub(1);
                }
                Ok(())
            }
            Change::Group(changes) => {
                let next_id = scene.next_id();
                let mut applied: Vec<Change> = Vec::with_capacity(changes.len());
                for c in changes {
                    let inverse = c.inverse(scene);
                    if let Err(e) = c.apply(scene) {
                        for inv in applied.iter().rev() {
                            let _ = inv.apply(scene);
                        }
                        // Ids handed out by the rolled-back prefix never
                        // became visible, so the counter rewinds too.
                        scene.restore_next_id(next_id);
                        return Err(e);
                    }
                    applied.push(inverse);
                }
                Ok(())
            }
        }
    }

    /// The change that undoes `self`, computed against the scene *before*
    /// `self` is applied.
    pub fn inverse(&self, before: &Scene) -> Change {
        match self {
            Change::AddNode(node) => {
                // The id is assigned on apply; predict it the way Scene does.
                let id = if node.id == NodeId(0) {
                    NodeId(before.next_id())
                } else {
                    node.id
                };
                Change::RemoveNode(id)
            }
            Change::RemoveNode(id) => {
                let Some(n) = before.node(*id) else {
                    return Change::Group(Vec::new());
                };
                // remove_node reparents the children to the grandparent
                // and rewrites their local transforms; undo both.
                let mut sim = before.clone();
                sim.remove_node(*id);
                let mut steps = vec![Change::AddNode(n.clone())];
                for child in before.children(*id) {
                    let (Some(old), Some(new)) = (before.node(child), sim.node(child)) else {
                        continue;
                    };
                    steps.push(Change::SetParent {
                        id: child,
                        before: new.parent,
                        after: Some(*id),
                    });
                    steps.push(Change::SetTransform {
                        id: child,
                        before: new.transform.clone(),
                        after: old.transform.clone(),
                    });
                }
                Change::Group(steps)
            }
            Change::SetTransform { id, before: b, after: a } => Change::SetTransform {
                id: *id,
                before: a.clone(),
                after: b.clone(),
            },
            Change::SetParent { id, before: b, after: a } => Change::SetParent {
                id: *id,
                before: *a,
                after: *b,
            },
            Change::ReplaceObject { id, before: b, after: a } => Change::ReplaceObject {
                id: *id,
                before: a.clone(),
                after: b.clone(),
            },
            Change::SetName { id, before: b, after: a } => Change::SetName {
                id: *id,
                before: a.clone(),
                after: b.clone(),
            },
            Change::SetFlags { id, before: b, after: a } => Change::SetFlags {
                id: *id,
                before: *a,
                after: *b,
            },
            Change::SetWireColor { id, before: b, after: a } => Change::SetWireColor {
                id: *id,
                before: *a,
                after: *b,
            },
            Change::SetNodeProps { id, before: b, after: a } => Change::SetNodeProps {
                id: *id,
                before: a.clone(),
                after: b.clone(),
            },
            Change::SetLayer { id, before: b, after: a } => Change::SetLayer { id: *id, before: *a, after: *b },
            Change::SetConstraints { id, before: b, after: a } => Change::SetConstraints { id: *id, before: a.clone(), after: b.clone() },
            Change::SetTrackData { before: b, after: a } => Change::SetTrackData { before: a.clone(), after: b.clone() },
            Change::SetVisibility { id, before: b, after: a } => Change::SetVisibility { id: *id, before: a.clone(), after: b.clone() },
            Change::SetLayers { before: b, after: a } => Change::SetLayers { before: a.clone(), after: b.clone() },
            Change::SetXrefs { before: b, after: a } => Change::SetXrefs { before: a.clone(), after: b.clone() },
            Change::SetStack { id, before: b, after: a } => Change::SetStack { id: *id, before: a.clone(), after: b.clone() },
            Change::SetIkChains { before: b, after: a } => Change::SetIkChains { before: a.clone(), after: b.clone() },
            Change::SetMaterials { before: b, after: a } => Change::SetMaterials { before: a.clone(), after: b.clone() },
            Change::SetMaterial { id, before: b, after: a } => Change::SetMaterial { id: *id, before: *a, after: *b },
            Change::SetRenderSettings { before: b, after: a } => Change::SetRenderSettings { before: a.clone(), after: b.clone() },
            Change::Group(changes) => {
                // Inverses must be computed against the progressively
                // changed scene, so simulate on a copy.
                let mut sim = before.clone();
                let mut inverses = Vec::with_capacity(changes.len());
                for c in changes {
                    inverses.push(c.inverse(&sim));
                    let _ = c.apply(&mut sim);
                }
                inverses.reverse();
                Change::Group(inverses)
            }
        }
    }
}

fn missing(id: NodeId) -> String {
    format!("no node {}", id.0)
}
