//! Every animatable parameter of a node as a track: the transform's
//! Position / Rotation / Scale, and each `Param` found in the serialised
//! object and modifiers (a `Param` serialises as an object whose only key
//! is "controller"). Set Key filters, the track bar, Track View and the
//! Motion panel all reach parameters this way, without a hand-written
//! visitor per object type.

use crate::node::{Node, NodeId};
use crate::param::{KeyedTracks, Param, ParamType};
use crate::scene::Scene;
use glam::Vec3;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TrackOwner {
    Transform,
    Object,
    Modifier(usize),
    /// Constraint parameters (% Along Path, target weights).
    Constraints,
    /// The node's visibility track.
    Visibility,
}

/// A track's identity, stable across edits (track sets, flags, key selection).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TrackKey {
    pub node: NodeId,
    pub owner: TrackOwner,
    pub pointer: String,
}

impl TrackPath {
    pub fn key(&self) -> TrackKey {
        TrackKey { node: self.node, owner: self.owner, pointer: self.pointer.clone() }
    }
}

/// Track View > Controller > Keyable, and the track Lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackFlags {
    pub not_keyable: bool,
    pub locked: bool,
}

/// A note key: text at a frame; a locked key ignores Move / Slide / Scale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteKey {
    pub time: f32,
    pub text: String,
    #[serde(default)]
    pub locked: bool,
}

/// A note track under a Track View item (an object, or one of its tracks).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteTrack {
    pub node: NodeId,
    #[serde(default)]
    pub under: Option<TrackKey>,
    pub keys: Vec<NoteKey>,
}

/// Scene-wide Track View data saved with the file.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackData {
    pub note_tracks: Vec<NoteTrack>,
    /// Named track sets (case-sensitive), in creation order.
    pub track_sets: Vec<(String, Vec<TrackKey>)>,
    pub flags: Vec<(TrackKey, TrackFlags)>,
}

impl TrackData {
    pub fn flags_of(&self, k: &TrackKey) -> TrackFlags {
        self.flags.iter().find(|(x, _)| x == k).map(|(_, f)| *f).unwrap_or_default()
    }

    pub fn set_flags(&mut self, k: TrackKey, f: TrackFlags) {
        self.flags.retain(|(x, _)| *x != k);
        if f != TrackFlags::default() {
            self.flags.push((k, f));
        }
    }
}

/// One animatable parameter: its owner, a JSON pointer into the owner's
/// serialised form, a display name and its value type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackPath {
    pub node: NodeId,
    pub owner: TrackOwner,
    pub pointer: String,
    pub name: String,
    pub ty: ParamType,
}

/// A parameter of any value type.
#[derive(Debug, Clone, PartialEq)]
pub enum AnyParam {
    F32(Param<f32>),
    U32(Param<u32>),
    Bool(Param<bool>),
    Vec3(Param<Vec3>),
}

macro_rules! each {
    ($self:expr, $p:ident => $e:expr) => {
        match $self {
            AnyParam::F32($p) => $e,
            AnyParam::U32($p) => $e,
            AnyParam::Bool($p) => $e,
            AnyParam::Vec3($p) => $e,
        }
    };
}

impl AnyParam {
    pub fn from_json(v: &Value, ty: ParamType) -> Option<Self> {
        let v = v.clone();
        Some(match ty {
            ParamType::F32 => AnyParam::F32(serde_json::from_value(v).ok()?),
            ParamType::U32 => AnyParam::U32(serde_json::from_value(v).ok()?),
            ParamType::Bool => AnyParam::Bool(serde_json::from_value(v).ok()?),
            ParamType::Vec3 => AnyParam::Vec3(serde_json::from_value(v).ok()?),
        })
    }

    pub fn to_json(&self) -> Value {
        each!(self, p => serde_json::to_value(p).expect("params serialise"))
    }

    pub fn is_animated(&self) -> bool {
        each!(self, p => p.is_animated())
    }

    pub fn keys(&self) -> Option<&KeyedTracks> {
        each!(self, p => p.keys())
    }

    pub fn ensure_keys(&mut self) -> Option<&mut KeyedTracks> {
        each!(self, p => p.ensure_keys())
    }

    pub fn key_now(&mut self) {
        each!(self, p => p.key_now())
    }

    /// Delete Selected Animation: a constant holding the value at `t`.
    pub fn freeze(&mut self, t: f32) {
        each!(self, p => p.controller = crate::param::Controller::Constant(p.get_at(t)))
    }

    pub fn key_comp(&mut self, i: usize) {
        each!(self, p => p.key_comp(i))
    }

    pub fn comps_at(&self, t: f32) -> Vec<f32> {
        each!(self, p => p.comps_at(t))
    }

    pub fn set_comps(&mut self, c: &[f32]) {
        each!(self, p => p.set_comps(c))
    }

    /// The controller as JSON (Copy / Paste Controller).
    pub fn controller_json(&self) -> Value {
        self.to_json()["controller"].clone()
    }

    /// Replaces the controller from JSON of the same value type.
    pub fn set_controller_json(&mut self, c: &Value) -> Result<(), String> {
        let ty = self.ty();
        *self = AnyParam::from_json(&serde_json::json!({ "controller": c }), ty).ok_or("controller of another value type")?;
        Ok(())
    }

    pub fn ty(&self) -> ParamType {
        match self {
            AnyParam::F32(_) => ParamType::F32,
            AnyParam::U32(_) => ParamType::U32,
            AnyParam::Bool(_) => ParamType::Bool,
            AnyParam::Vec3(_) => ParamType::Vec3,
        }
    }
}

const TRANSFORM_TRACKS: [(&str, &str); 3] = [("/position", "Position"), ("/rotation", "Rotation"), ("/scale", "Scale")];

/// The owner's serialised form.
pub fn owner_json(node: &Node, owner: TrackOwner) -> Option<Value> {
    match owner {
        TrackOwner::Transform => serde_json::to_value(&node.transform).ok(),
        TrackOwner::Object => serde_json::to_value(&node.object).ok(),
        TrackOwner::Modifier(i) => serde_json::to_value(&node.modifiers.get(i)?.modifier).ok(),
        TrackOwner::Constraints => serde_json::to_value(&node.constraints).ok(),
        TrackOwner::Visibility => serde_json::to_value(&node.visibility).ok(),
    }
}

/// Writes the owner back from its edited serialised form.
pub fn set_owner_json(node: &mut Node, owner: TrackOwner, v: Value) -> Result<(), String> {
    match owner {
        TrackOwner::Transform => node.transform = serde_json::from_value(v).map_err(|e| e.to_string())?,
        TrackOwner::Object => node.object = serde_json::from_value(v).map_err(|e| e.to_string())?,
        TrackOwner::Modifier(i) => {
            let slot = node.modifiers.get_mut(i).ok_or("no such modifier")?;
            slot.modifier = serde_json::from_value(v).map_err(|e| e.to_string())?;
        }
        TrackOwner::Constraints => node.constraints = serde_json::from_value(v).map_err(|e| e.to_string())?,
        TrackOwner::Visibility => node.visibility = serde_json::from_value(v).map_err(|e| e.to_string())?,
    }
    Ok(())
}

fn controller_ty(c: &Value, in_transform: bool) -> Option<ParamType> {
    let (tag, body) = c.as_object()?.iter().next()?;
    match tag.as_str() {
        "Constant" => match body {
            Value::Bool(_) => Some(ParamType::Bool),
            Value::Number(n) if n.is_u64() => Some(ParamType::U32),
            Value::Number(_) => Some(ParamType::F32),
            Value::Array(a) if a.len() == 3 => Some(ParamType::Vec3),
            _ => None,
        },
        "Keys" => serde_json::from_value(body.get("ty")?.clone()).ok(),
        "List" => body.as_array()?.first().and_then(|i| controller_ty(i.get("controller")?, in_transform)).or(Some(if in_transform { ParamType::Vec3 } else { ParamType::F32 })),
        _ => Some(if in_transform { ParamType::Vec3 } else { ParamType::F32 }),
    }
}

fn walk(v: &Value, ptr: &mut String, out: &mut Vec<(String, ParamType)>) {
    match v {
        Value::Object(m) => {
            if m.len() == 1 {
                if let Some(c) = m.get("controller") {
                    if let Some(ty) = controller_ty(c, false) {
                        out.push((ptr.clone(), ty));
                    }
                    return;
                }
            }
            for (k, child) in m {
                if matches!(child, Value::Object(_) | Value::Array(_)) {
                    let len = ptr.len();
                    ptr.push('/');
                    ptr.push_str(&k.replace('~', "~0").replace('/', "~1"));
                    walk(child, ptr, out);
                    ptr.truncate(len);
                }
            }
        }
        Value::Array(a) => {
            for (i, child) in a.iter().enumerate() {
                if matches!(child, Value::Object(_) | Value::Array(_)) {
                    let len = ptr.len();
                    ptr.push('/');
                    ptr.push_str(&i.to_string());
                    walk(child, ptr, out);
                    ptr.truncate(len);
                }
            }
        }
        _ => {}
    }
}

fn pretty(seg: &str) -> String {
    seg.split('_').filter(|w| !w.is_empty()).map(|w| {
        let mut c = w.chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
    }).collect::<Vec<_>>().join(" ")
}

/// "Height" from "/Box/height"; "Gizmo Position" from "/Bend/d/gizmo/position".
fn display_name(pointer: &str) -> String {
    let segs: Vec<&str> = pointer.split('/').filter(|s| !s.is_empty() && s.parse::<usize>().is_err()).collect();
    let Some(last) = segs.last() else { return String::new() };
    match segs.len() {
        n if n >= 3 && segs[n - 2].len() > 1 => format!("{} {}", pretty(segs[n - 2]), pretty(last)),
        _ => pretty(last),
    }
}

fn owner_tracks(node: &Node, owner: TrackOwner, v: &Value, out: &mut Vec<TrackPath>) {
    let mut found = Vec::new();
    walk(v, &mut String::new(), &mut found);
    for (pointer, ty) in found {
        let name = match owner {
            TrackOwner::Modifier(i) => format!("{} / {}", node.modifiers[i].name, display_name(&pointer)),
            TrackOwner::Constraints => constraint_track_name(node, &pointer),
            _ => display_name(&pointer),
        };
        out.push(TrackPath { node: node.id, owner, pointer, name, ty });
    }
}

/// Every track of a node: transform first, then the object, then the
/// modifiers bottom to top.
pub fn node_tracks(node: &Node) -> Vec<TrackPath> {
    let mut out: Vec<TrackPath> = TRANSFORM_TRACKS.iter().map(|(p, n)| TrackPath { node: node.id, owner: TrackOwner::Transform, pointer: (*p).into(), name: (*n).into(), ty: ParamType::Vec3 }).collect();
    if let Some(v) = owner_json(node, TrackOwner::Object) {
        owner_tracks(node, TrackOwner::Object, &v, &mut out);
    }
    for i in 0..node.modifiers.len() {
        if let Some(v) = owner_json(node, TrackOwner::Modifier(i)) {
            owner_tracks(node, TrackOwner::Modifier(i), &v, &mut out);
        }
    }
    if !node.constraints.is_empty() {
        if let Some(v) = owner_json(node, TrackOwner::Constraints) {
            owner_tracks(node, TrackOwner::Constraints, &v, &mut out);
        }
    }
    if node.visibility.is_some() {
        out.push(TrackPath { node: node.id, owner: TrackOwner::Visibility, pointer: String::new(), name: "Visibility".into(), ty: ParamType::F32 });
    }
    out
}

/// "Path Constraint / Percent", "LookAt Constraint / Weight 2".
fn constraint_track_name(node: &Node, pointer: &str) -> String {
    let c = &node.constraints;
    let kind = if pointer.starts_with("/position") { c.position_name() } else { c.rotation_name() }.unwrap_or("Constraint");
    let segs: Vec<&str> = pointer.split('/').collect();
    match segs.iter().position(|s| *s == "targets") {
        Some(i) => format!("{kind} / Weight {}", segs.get(i + 1).and_then(|n| n.parse::<usize>().ok()).map(|n| n + 1).unwrap_or(1)),
        None => format!("{kind} / {}", pretty(pointer.rsplit('/').next().unwrap_or_default())),
    }
}

pub fn list_tracks(scene: &Scene, id: NodeId) -> Vec<TrackPath> {
    scene.node(id).map(node_tracks).unwrap_or_default()
}

pub fn read_node(node: &Node, track: &TrackPath) -> Option<AnyParam> {
    if track.owner == TrackOwner::Visibility {
        return node.visibility.clone().map(AnyParam::F32);
    }
    if track.owner == TrackOwner::Transform {
        return Some(AnyParam::Vec3(match track.pointer.as_str() {
            "/position" => node.transform.position.clone(),
            "/rotation" => node.transform.rotation.clone(),
            "/scale" => node.transform.scale.clone(),
            _ => return None,
        }));
    }
    let v = owner_json(node, track.owner)?;
    AnyParam::from_json(v.pointer(&track.pointer)?, track.ty)
}

pub fn read(scene: &Scene, track: &TrackPath) -> Option<AnyParam> {
    read_node(scene.node(track.node)?, track)
}

/// Edits several tracks of one node, serialising each owner once.
pub fn edit_node(node: &mut Node, tracks: &[TrackPath], mut f: impl FnMut(&TrackPath, &mut AnyParam)) -> Result<(), String> {
    let mut owners: Vec<TrackOwner> = tracks.iter().map(|t| t.owner).collect();
    owners.sort();
    owners.dedup();
    for owner in owners {
        let mine = tracks.iter().filter(|t| t.owner == owner);
        if owner == TrackOwner::Visibility {
            for t in mine {
                if let Some(v) = node.visibility.as_mut() {
                    let mut a = AnyParam::F32(v.clone());
                    f(t, &mut a);
                    if let AnyParam::F32(n) = a {
                        *v = n;
                    }
                }
            }
            continue;
        }
        if owner == TrackOwner::Transform {
            for t in mine {
                let p = match t.pointer.as_str() {
                    "/position" => &mut node.transform.position,
                    "/rotation" => &mut node.transform.rotation,
                    "/scale" => &mut node.transform.scale,
                    _ => continue,
                };
                let mut a = AnyParam::Vec3(p.clone());
                f(t, &mut a);
                if let AnyParam::Vec3(n) = a {
                    *p = n;
                }
            }
            continue;
        }
        let mut v = owner_json(node, owner).ok_or("no such owner")?;
        for t in mine {
            let Some(slot) = v.pointer_mut(&t.pointer) else { continue };
            let Some(mut a) = AnyParam::from_json(slot, t.ty) else { continue };
            f(t, &mut a);
            *slot = a.to_json();
        }
        set_owner_json(node, owner, v)?;
    }
    Ok(())
}

/// What a track animates, for controller names and Assign Controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Position,
    Rotation,
    Scale,
    Float,
    OnOff,
}

impl Role {
    pub fn of(t: &TrackPath) -> Role {
        match (t.owner, t.pointer.as_str(), t.ty) {
            (TrackOwner::Visibility, _, _) => Role::Float,
            (TrackOwner::Transform, "/position", _) => Role::Position,
            (TrackOwner::Transform, "/rotation", _) => Role::Rotation,
            (TrackOwner::Transform, _, _) => Role::Scale,
            (_, _, ParamType::Bool) => Role::OnOff,
            _ => Role::Float,
        }
    }

    fn word(self) -> &'static str {
        match self {
            Role::Position => "Position",
            Role::Rotation => "Rotation",
            Role::Scale => "Scale",
            Role::Float | Role::OnOff => "Float",
        }
    }

    /// The controllers Assign Controller offers for this track.
    pub fn choices(self) -> Vec<String> {
        let w = self.word();
        match self {
            Role::OnOff => vec!["On Off".into()],
            Role::Float => vec!["Bezier Float".into(), "TCB Float".into(), "Linear Float".into(), "Noise Float".into(), "Float List".into()],
            Role::Position => vec!["Position XYZ".into(), "Bezier Position".into(), "TCB Position".into(), "Linear Position".into(), "Noise Position".into(), "Position List".into(), "Path Constraint".into(), "Position Constraint".into(), "Attachment".into()],
            Role::Rotation => vec!["Euler XYZ".into(), format!("Bezier {w}"), format!("TCB {w}"), format!("Linear {w}"), format!("Noise {w}"), format!("{w} List"), "LookAt Constraint".into(), "Orientation Constraint".into()],
            Role::Scale => vec!["ScaleXYZ".into(), format!("Bezier {w}"), format!("TCB {w}"), format!("Linear {w}"), format!("Noise {w}"), format!("{w} List")],
        }
    }
}

fn interp_word(i: crate::anim::Interp) -> &'static str {
    match i {
        crate::anim::Interp::Bezier => "Bezier",
        crate::anim::Interp::Tcb => "TCB",
        crate::anim::Interp::Linear => "Linear",
    }
}

fn controller_name_json(c: &Value, role: Role) -> String {
    let Some((tag, body)) = c.as_object().and_then(|m| m.iter().next()) else { return String::new() };
    let w = role.word();
    let xyz = match role {
        Role::Position => "Position XYZ",
        Role::Rotation => "Euler XYZ",
        Role::Scale => "ScaleXYZ",
        _ => "",
    };
    match tag.as_str() {
        "Noise" => format!("Noise {w}"),
        "List" => format!("{w} List"),
        _ if role == Role::OnOff => "On Off".into(),
        "Constant" if !xyz.is_empty() => xyz.into(),
        "Constant" => "Bezier Float".into(),
        _ => {
            let k: Option<KeyedTracks> = serde_json::from_value(body.clone()).ok();
            match k {
                Some(k) if k.linked || xyz.is_empty() => format!("{} {w}", interp_word(k.tracks.first().map(|t| t.interp).unwrap_or_default())),
                _ => xyz.into(),
            }
        }
    }
}

impl AnyParam {
    /// The controller name ("Position XYZ", "TCB Float", "Noise Rotation"…).
    pub fn controller_name(&self, role: Role) -> String {
        controller_name_json(&self.controller_json(), role)
    }

    /// The names of a List controller's entries with their weights.
    pub fn list_entries(&self, role: Role) -> Vec<(String, f32)> {
        let c = self.controller_json();
        c.get("List").and_then(|l| l.as_array()).map(|items| items.iter().map(|i| (i["name"].as_str().map(str::to_owned).unwrap_or_else(|| controller_name_json(&i["controller"], role)), i["weight"].as_f64().unwrap_or(100.0) as f32)).collect()).unwrap_or_default()
    }
}

/// A fresh controller of type `name` for a track, built from the
/// current one: keys are kept when the new type is keyed, a constant holds
/// the current value otherwise. Constraint names are not controllers here.
pub fn convert_controller(cur: &AnyParam, role: Role, name: &str, seed: u32) -> Result<Value, String> {
    if !role.choices().iter().any(|c| c == name) {
        return Err(format!("{name} cannot drive a {} track", role.word()));
    }
    let t = crate::anim::time();
    let comps = cur.comps_at(t);
    let dims = comps.len();
    let constant = |c: &[f32]| -> Value {
        let v: Value = match (cur.ty(), c) {
            (ParamType::Vec3, c) => serde_json::json!([c[0], c[1], c[2]]),
            (ParamType::Bool, c) => Value::Bool(c[0] > 0.5),
            (ParamType::U32, c) => serde_json::json!(c[0].round().max(0.0) as u32),
            (ParamType::F32, c) => serde_json::json!(c[0]),
        };
        serde_json::json!({ "Constant": v })
    };
    let w = role.word();
    let keyed = |interp: crate::anim::Interp, linked: bool| -> Value {
        let mut k = cur.keys().cloned().unwrap_or_else(|| KeyedTracks::from_rest(cur.ty(), comps.clone()));
        if cur.keys().is_none() && !cur.is_animated() {
            k.rest = comps.clone();
        }
        for tr in &mut k.tracks {
            tr.interp = interp;
        }
        if linked {
            // Every component gets a key wherever any has one.
            let mut times: Vec<f32> = k.tracks.iter().flat_map(|t| t.keys.iter().map(|k| k.time)).collect();
            times.sort_by(f32::total_cmp);
            times.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
            let values: Vec<Vec<f32>> = times.iter().map(|tm| k.comps_at(*tm)).collect();
            let tangents = crate::anim::default_tangents();
            for (tm, v) in times.iter().zip(values) {
                for (i, tr) in k.tracks.iter_mut().enumerate() {
                    if tr.key_at(*tm).is_none() {
                        tr.set_key(*tm, v[i], tangents);
                    }
                }
            }
        }
        k.linked = linked;
        serde_json::json!({ "Keys": k })
    };
    let interp = |s: &str| match s {
        "TCB" => crate::anim::Interp::Tcb,
        "Linear" => crate::anim::Interp::Linear,
        _ => crate::anim::Interp::Bezier,
    };
    let cur_c = cur.controller_json();
    let is_keys = cur_c.get("Keys").is_some();
    Ok(match name {
        "Position XYZ" | "Euler XYZ" | "ScaleXYZ" => {
            if is_keys { keyed(crate::anim::Interp::Bezier, false) } else { constant(&comps) }
        }
        "On Off" => {
            if is_keys { keyed(crate::anim::Interp::Bezier, false) } else { constant(&comps) }
        }
        n if n.starts_with("Noise") => {
            let mut nc = crate::param::NoiseCtrl::new(seed);
            nc.strength = vec![if role == Role::Scale { 0.5 } else { 50.0 }; dims];
            serde_json::json!({ "Noise": nc })
        }
        n if n.ends_with(" List") => {
            let first = controller_name_json(&cur_c, role);
            serde_json::json!({ "List": [ { "name": first, "weight": 100.0, "controller": cur_c } ] })
        }
        n => {
            let (word, _) = n.split_once(' ').unwrap_or((n, ""));
            let linked = role != Role::Float && n.ends_with(w);
            if is_keys || !cur.is_animated() {
                keyed(interp(word), linked)
            } else {
                // From Noise / List: start keyed from the current value.
                let mut k = KeyedTracks::from_rest(cur.ty(), comps.clone());
                for tr in &mut k.tracks {
                    tr.interp = interp(word);
                }
                k.linked = linked;
                serde_json::json!({ "Keys": k })
            }
        }
    })
}
