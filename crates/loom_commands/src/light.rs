//! Create > Lights > Standard: Omni, Target / Free Spot, Target / Free
//! Direct and Skylight, the Light Type / Targeted switch, the Exclude/Include
//! list and the projector map. A target light aims its local -Z at its
//! target node through the same LookAt constraint as a target camera.

use crate::camera::look_at_target;
use crate::execute::commit;
use crate::state::AppState;
use crate::Command;
use glam::{Mat4, Vec3};
use loom_scene::light::{LightObject, LightType};
use loom_scene::{Change, Node, NodeId, ObjectKind, Transform};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;

/// Wire colour of new lights and their targets.
pub const LIGHT_COLOR: [u8; 3] = [255, 220, 0];
/// Size of a light target's box.
pub const TARGET_SIZE: f32 = 4.0;

/// Create > Lights > Standard object types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LightKind {
    TargetSpot,
    FreeSpot,
    TargetDirect,
    FreeDirect,
    Omni,
    Skylight,
}

impl LightKind {
    /// Object Type rollout order.
    pub const ALL: [LightKind; 6] = [LightKind::TargetSpot, LightKind::FreeSpot, LightKind::TargetDirect, LightKind::FreeDirect, LightKind::Omni, LightKind::Skylight];

    pub fn label(self) -> &'static str {
        match self {
            LightKind::TargetSpot => "Target Spot",
            LightKind::FreeSpot => "Free Spot",
            LightKind::TargetDirect => "Target Direct",
            LightKind::FreeDirect => "Free Direct",
            LightKind::Omni => "Omni",
            LightKind::Skylight => "Skylight",
        }
    }

    pub fn light_type(self) -> LightType {
        match self {
            LightKind::TargetSpot | LightKind::FreeSpot => LightType::Spot,
            LightKind::TargetDirect | LightKind::FreeDirect => LightType::Directional,
            LightKind::Omni => LightType::Omni,
            LightKind::Skylight => LightType::Skylight,
        }
    }

    /// Created by a drag from the light to its target.
    pub fn targeted(self) -> bool {
        matches!(self, LightKind::TargetSpot | LightKind::TargetDirect)
    }
}

impl crate::command::ArgSchema for LightKind {
    fn schema() -> Value {
        json!({"type":"string","enum":["target_spot","free_spot","target_direct","free_direct","omni","skylight"]})
    }
}

/// The Light Type list of General Parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LightTypeArg {
    Omni,
    Spot,
    Directional,
}

impl LightTypeArg {
    pub fn light_type(self) -> LightType {
        match self {
            LightTypeArg::Omni => LightType::Omni,
            LightTypeArg::Spot => LightType::Spot,
            LightTypeArg::Directional => LightType::Directional,
        }
    }
    pub fn of(t: LightType) -> Option<Self> {
        match t {
            LightType::Omni => Some(LightTypeArg::Omni),
            LightType::Spot => Some(LightTypeArg::Spot),
            LightType::Directional => Some(LightTypeArg::Directional),
            LightType::Skylight => None,
        }
    }
}

impl crate::command::ArgSchema for LightTypeArg {
    fn schema() -> Value {
        json!({"type":"string","enum":["omni","spot","directional"]})
    }
}

fn missing(id: NodeId) -> String {
    format!("no node {}", id.0)
}

fn target_node(name: &str, at: Vec3) -> Node {
    let mut t = Node::new(&format!("{name}.Target"), ObjectKind::dummy(TARGET_SIZE));
    t.transform.position.set(at);
    t.wire_color = LIGHT_COLOR;
    t
}

/// Creates a light at `position`. Target lights put their target node at
/// `target`; free spot and directional lights aim at it and keep the gap
/// as their Target Distance; omni and skylights ignore it.
pub fn create(state: &mut AppState, kind: LightKind, position: Vec3, target: Vec3) -> Result<Value, String> {
    let mut light = LightObject::new(kind.light_type());
    let id = NodeId(state.scene.next_id());
    let tid = NodeId(id.0 + 1);
    if kind.targeted() {
        light.target = Some(tid);
    }
    let name = state.scene.next_name(light.base_name());
    let dir = target - position;
    let aims = kind.light_type().aims();
    if aims && !kind.targeted() && dir.length() > 1e-4 {
        light.target_distance.set(dir.length());
    }
    let mut node = Node::new(&name, ObjectKind::Light(Box::new(light)));
    let rotation = if aims && dir.length() > 1e-4 { loom_scene::constraint::aim(dir, 2, true, Vec3::Z) } else { glam::Quat::IDENTITY };
    node.transform = Transform::from_matrix(Mat4::from_rotation_translation(rotation, position));
    node.wire_color = LIGHT_COLOR;
    let mut changes = Vec::new();
    if kind.targeted() {
        node.constraints.rotation = Some(look_at_target(tid));
        changes.push(Change::AddNode(node));
        changes.push(Change::AddNode(target_node(&name, target)));
    } else {
        changes.push(Change::AddNode(node));
    }
    commit(state, "Create Light", Change::Group(changes))?;
    state.selection = BTreeSet::from([id]);
    state.sub_selection.clear();
    Ok(json!({ "id": id.0, "target": kind.targeted().then_some(tid.0) }))
}

/// General Parameters > Light Type list and Targeted: changes the type
/// (Omni / Spot / Directional) and adds or removes the target, keeping the
/// aim (a new target goes at Target Distance).
pub fn set_type(state: &mut AppState, id: NodeId, light_type: LightType, targeted: bool) -> Result<Value, String> {
    let node = state.scene.node(id).ok_or_else(|| missing(id))?;
    let ObjectKind::Light(l) = &node.object else {
        return Err(format!("{} is not a light", node.name));
    };
    if l.light_type == LightType::Skylight || light_type == LightType::Skylight {
        return Err("a Skylight has no Light Type list".into());
    }
    // An omni has no direction: it is never targeted.
    let targeted = targeted && light_type.aims();
    let current = l.target.filter(|t| state.scene.node(*t).is_some());
    let world = state.scene.world_matrix(id);
    let eye = world.w_axis.truncate();
    let forward = -world.z_axis.truncate().normalize_or_zero();
    let mut object = node.object.clone();
    let mut constraints = node.constraints.clone();
    let mut changes = Vec::new();
    let ObjectKind::Light(after) = &mut object else { unreachable!() };
    after.light_type = light_type;
    match (current, targeted) {
        (Some(t), false) => {
            after.target = None;
            after.target_distance.set((state.scene.world_matrix(t).w_axis.truncate() - eye).length().max(1e-3));
            constraints.rotation = None;
            let parent = node.parent.map(|p| state.scene.world_matrix(p)).unwrap_or(Mat4::IDENTITY);
            let (_, rot, _) = (parent.inverse() * world).to_scale_rotation_translation();
            let mut t2 = node.transform.clone();
            t2.set_rotation_quat(rot);
            changes.push(Change::SetTransform { id, before: node.transform.clone(), after: t2 });
            changes.push(Change::RemoveNode(t));
        }
        (None, true) => {
            let tid = NodeId(state.scene.next_id());
            after.target = Some(tid);
            constraints.rotation = Some(look_at_target(tid));
            changes.push(Change::AddNode(target_node(&node.name, eye + forward * after.target_distance.get().max(1e-3))));
        }
        _ => {}
    }
    changes.push(Change::ReplaceObject { id, before: Box::new(node.object.clone()), after: Box::new(object) });
    changes.push(Change::SetConstraints { id, before: Box::new(node.constraints.clone()), after: Box::new(constraints) });
    commit(state, "Light Type", Change::Group(changes))?;
    Ok(json!({}))
}

fn replace_light(state: &mut AppState, id: NodeId, label: &str, f: impl FnOnce(&mut LightObject)) -> Result<Value, String> {
    let node = state.scene.node(id).ok_or_else(|| missing(id))?;
    let before = node.object.clone();
    let mut after = before.clone();
    let ObjectKind::Light(l) = &mut after else {
        return Err(format!("{} is not a light", node.name));
    };
    f(l);
    commit(state, label, Change::ReplaceObject { id, before: Box::new(before), after: Box::new(after) })?;
    Ok(json!({}))
}

/// The light nodes, in scene order (Light Lister, Lights submenu).
pub fn lights(scene: &loom_scene::Scene) -> Vec<NodeId> {
    scene.nodes().filter(|n| matches!(n.object, ObjectKind::Light(_))).map(|n| n.id).collect()
}

/// Whether the node is a light's target (it is drawn and handled with its light).
pub fn is_light_target(scene: &loom_scene::Scene, id: NodeId) -> bool {
    scene.nodes().any(|n| matches!(&n.object, ObjectKind::Light(l) if l.target == Some(id)))
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    match cmd {
        Command::CreateLight { kind, position, target } => create(state, kind, Vec3::from(position), Vec3::from(target)),
        Command::LightType { id, light_type, targeted } => set_type(state, NodeId(id), light_type.light_type(), targeted),
        Command::SetLightExclusion { id, include, scope, nodes } => replace_light(state, NodeId(id), "Exclude/Include", |l| {
            l.exclusion = loom_scene::light::Exclusion { include, scope: scope.min(2), nodes: nodes.into_iter().map(NodeId).collect() };
        }),
        Command::SetLightProjectorMap { id, path } => replace_light(state, NodeId(id), "Projector Map", |l| {
            l.projector.set(path.is_some());
            l.projector_map = path;
        }),
        Command::LightLister => {
            state.dialog = Some(crate::Dialog::LightLister);
            Ok(json!({ "dialog": "LightLister" }))
        }
        other => Err(format!("not a light command: {}", other.name())),
    }
}

/// What a light overlay line shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// Hotspot / beam outline (light blue).
    Hotspot,
    /// Falloff / field outline (dark gray).
    Falloff,
    NearStart,
    NearEnd,
    FarStart,
    FarEnd,
    /// The line from a target light to its target.
    TargetLine,
}

/// World-space overlay lines of a light: the hotspot and falloff cone (or
/// cylinder) when selected or Show Cone is on, out to the target distance;
/// the attenuation ranges when selected or Show is on (spheres for omni and
/// overshoot lights, cone sections otherwise); and the line to the target.
pub fn overlay_lines(scene: &loom_scene::Scene, id: NodeId, selected: bool) -> Vec<(Vec3, Vec3, LineKind)> {
    let Some(node) = scene.node(id) else { return Vec::new() };
    let ObjectKind::Light(l) = &node.object else { return Vec::new() };
    let world = scene.world_matrix(id);
    let eye = world.w_axis.truncate();
    let distance = loom_scene::light::target_distance(scene, id).unwrap_or(loom_scene::light::DEFAULT_TARGET_DISTANCE).max(1e-3);
    let mut out = Vec::new();
    const N: usize = 32;
    // The outline at `d` along -Z with half-size `r` (circle or rectangle).
    let outline = |out: &mut Vec<(Vec3, Vec3, LineKind)>, r: f32, d: f32, kind: LineKind| {
        let pts: Vec<Vec3> = if l.rectangle.get() {
            let (hx, hy) = (r, r / l.aspect.get().max(1e-3));
            vec![Vec3::new(-hx, hy, -d), Vec3::new(hx, hy, -d), Vec3::new(hx, -hy, -d), Vec3::new(-hx, -hy, -d)]
        } else {
            (0..N).map(|i| {
                let a = i as f32 / N as f32 * std::f32::consts::TAU;
                Vec3::new(r * a.cos(), r * a.sin(), -d)
            }).collect()
        };
        let pts: Vec<Vec3> = pts.iter().map(|p| world.transform_point3(*p)).collect();
        for i in 0..pts.len() {
            out.push((pts[i], pts[(i + 1) % pts.len()], kind));
        }
        pts
    };
    let aims = l.light_type.aims();
    let spot = l.light_type == LightType::Spot;
    // Half-size of the beam at distance d.
    let half = |angle_or_width: f32, d: f32| if spot { d * (angle_or_width.min(179.0) * 0.5).to_radians().tan() } else { angle_or_width * 0.5 };
    if aims && (selected || l.show_cone.get()) {
        for (value, kind) in [(l.hotspot.get(), LineKind::Hotspot), (l.falloff.get(), LineKind::Falloff)] {
            let far = outline(&mut out, half(value, distance), distance, kind);
            let step = (far.len() / 4).max(1);
            if spot {
                for p in far.iter().step_by(step) {
                    out.push((eye, *p, kind));
                }
            } else {
                let near = outline(&mut out, half(value, 0.0), 0.0, kind);
                for (a, b) in near.iter().zip(&far).step_by(step) {
                    out.push((*a, *b, kind));
                }
            }
        }
    }
    let ranges = [
        (l.near_use.get() || l.near_show.get(), l.near_show.get(), l.near_start.get(), LineKind::NearStart),
        (l.near_use.get() || l.near_show.get(), l.near_show.get(), l.near_end.get(), LineKind::NearEnd),
        (l.far_use.get() || l.far_show.get(), l.far_show.get(), l.far_start.get(), LineKind::FarStart),
        (l.far_use.get() || l.far_show.get(), l.far_show.get(), l.far_end.get(), LineKind::FarEnd),
    ];
    if l.light_type != LightType::Skylight {
        for (used, shown, d, kind) in ranges {
            if !(shown || (selected && used)) || d <= 0.0 {
                continue;
            }
            if aims && !l.overshoot.get() {
                outline(&mut out, half(l.falloff.get(), d), d, kind);
            } else {
                // A sphere: three great circles.
                for axes in [(Vec3::X, Vec3::Y), (Vec3::Y, Vec3::Z), (Vec3::Z, Vec3::X)] {
                    let pts: Vec<Vec3> = (0..N).map(|i| {
                        let a = i as f32 / N as f32 * std::f32::consts::TAU;
                        eye + (axes.0 * a.cos() + axes.1 * a.sin()) * d
                    }).collect();
                    for i in 0..N {
                        out.push((pts[i], pts[(i + 1) % N], kind));
                    }
                }
            }
        }
    }
    if let Some(t) = l.target.filter(|t| scene.node(*t).is_some()) {
        out.push((eye, scene.world_matrix(t).w_axis.truncate(), LineKind::TargetLine));
    }
    out
}
