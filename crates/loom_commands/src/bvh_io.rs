//! BVH skeletons and motion as editable native bones and linear PRS keys.
//! Kimodo BVH uses centimetres and Y-up; convert once into scene units/Z-up.
use crate::{execute::unique_name, state::{AppState, SelectionMode}};
use glam::{Quat, Vec3};
use loom_scene::{Change, Node, NodeId, ObjectKind, Param};
use std::{collections::BTreeSet, path::Path};

struct Joint {
    name: String,
    parent: Option<usize>,
    offset: Vec3,
    channels: Vec<usize>,
    end: Option<Vec3>,
}
struct Reader<'a> { tokens: Vec<&'a str>, at: usize }
impl<'a> Reader<'a> {
    fn next(&mut self) -> Result<&'a str, String> {
        let t = self.tokens.get(self.at).copied().ok_or("unexpected end of BVH")?;
        self.at += 1;
        Ok(t)
    }
    fn expect(&mut self, wanted: &str) -> Result<(), String> {
        let t = self.next()?;
        if t == wanted { Ok(()) } else { Err(format!("expected {wanted}, got {t}")) }
    }
    fn number(&mut self) -> Result<f32, String> {
        let t = self.next()?;
        let n: f32 = t.parse().map_err(|_| format!("invalid BVH number: {t}"))?;
        if n.is_finite() { Ok(n) } else { Err("non-finite BVH number".into()) }
    }
    fn count(&mut self) -> Result<usize, String> {
        self.next()?.parse().map_err(|_| "invalid BVH count".into())
    }
    fn vector(&mut self) -> Result<Vec3, String> { Ok(Vec3::new(self.number()?, self.number()?, self.number()?)) }
    fn joint(&mut self, joints: &mut Vec<Joint>, parent: Option<usize>) -> Result<(), String> {
        // Bound recursion before parsing untrusted hierarchy nesting.
        if joints.len() >= 512 { return Err("BVH skeleton exceeds 512 joints".into()); }
        let name = self.next()?.to_owned();
        if name == "{" || name == "}" { return Err("missing BVH joint name".into()); }
        self.expect("{")?;
        self.expect("OFFSET")?;
        let offset = self.vector()?;
        self.expect("CHANNELS")?;
        let count = self.count()?;
        if count > 6 { return Err("BVH joint has more than six channels".into()); }
        let mut channels = Vec::new();
        for _ in 0..count {
            let t = self.next()?;
            let c = ["Xposition", "Yposition", "Zposition", "Xrotation", "Yrotation", "Zrotation"].iter().position(|v| *v == t).ok_or_else(|| format!("unsupported BVH channel: {t}"))?;
            if channels.contains(&c) { return Err(format!("duplicate BVH channel: {t}")); }
            channels.push(c);
        }
        let index = joints.len();
        joints.push(Joint { name, parent, offset, channels, end: None });
        loop {
            match self.next()? {
                "}" => break,
                "JOINT" => self.joint(joints, Some(index))?,
                "End" => {
                    if joints[index].end.is_some() { return Err("duplicate BVH End Site".into()); }
                    self.expect("Site")?;
                    self.expect("{")?;
                    self.expect("OFFSET")?;
                    joints[index].end = Some(self.vector()?);
                    self.expect("}")?;
                }
                t => return Err(format!("unexpected BVH hierarchy token: {t}")),
            }
        }
        Ok(())
    }
}
fn converted(v: Vec3, scale: f32) -> Vec3 { Vec3::new(v.x, -v.z, v.y) * scale }
fn euler_near(q: Quat, previous: Option<Vec3>) -> Vec3 {
    let (x, y, z) = q.to_euler(glam::EulerRot::XYZ);
    let mut r = Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees());
    if let Some(p) = previous {
        // XYZ has two equivalent Euler branches. Choose the closest to the
        // previous sample before unwrapping, avoiding artificial 180° flips.
        let near = |mut v: Vec3| { for c in 0..3 { v[c] += ((p[c] - v[c]) / 360.0).round() * 360.0; } v };
        r = near(r);
        let alternate = near(Vec3::new(r.x + 180.0, 180.0 - r.y, r.z + 180.0));
        if alternate.distance_squared(p) < r.distance_squared(p) { r = alternate; }
    }
    r
}
fn keyed(rest: Vec3, samples: &[Vec3], frame_scale: f32) -> Param<Vec3> {
    use loom_scene::{anim::{Interp, TangentType}, param::{Controller, KeyedTracks, ParamType}};
    let mut tracks = KeyedTracks::from_rest(ParamType::Vec3, rest.to_array().to_vec());
    for (c, track) in tracks.tracks.iter_mut().enumerate() {
        track.interp = Interp::Linear;
        for (frame, v) in samples.iter().enumerate() {
            track.set_key(frame as f32 * frame_scale, v[c], (TangentType::Linear, TangentType::Linear));
        }
    }
    Param { controller: Controller::Keys(tracks) }
}
pub fn import(state: &mut AppState, path: &Path) -> Result<Vec<NodeId>, String> {
    import_with(state, path, false)
}

/// Add source motion in the project's existing timebase.
pub fn import_animation(state: &mut AppState, path: &Path) -> Result<Vec<NodeId>, String> {
    import_with(state, path, true)
}

fn import_with(state: &mut AppState, path: &Path, preserve_timing: bool) -> Result<Vec<NodeId>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    // Braces need not have surrounding whitespace.
    let text = text.replace('{', " { ").replace('}', " } ");
    let mut r = Reader { tokens: text.split_whitespace().collect(), at: 0 };
    r.expect("HIERARCHY")?;
    r.expect("ROOT")?;
    let mut joints = Vec::new();
    r.joint(&mut joints, None)?;
    r.expect("MOTION")?;
    r.expect("Frames:")?;
    let frames = r.count()?;
    if frames == 0 || frames > i32::MAX as usize { return Err("invalid BVH frame count".into()); }
    r.expect("Frame")?;
    r.expect("Time:")?;
    let dt = r.number()?;
    if dt <= 0.0 || !(1.0 / dt).is_finite() { return Err("invalid BVH frame time".into()); }
    if preserve_timing && (!state.fps.is_finite() || state.fps <= 0.0) {
        return Err("project frame rate must be finite and positive".into());
    }
    let frame_scale = if preserve_timing { state.fps / (1.0 / dt) } else { 1.0 };
    let clip_end = (frames - 1) as f32 * frame_scale;
    if !frame_scale.is_finite() || frame_scale <= 0.0 || !clip_end.is_finite() || clip_end.ceil() as f64 > i32::MAX as f64 {
        return Err("BVH animation exceeds timeline frame limits".into());
    }
    let channels: usize = joints.iter().map(|j| j.channels.len()).sum();
    if channels == 0 || frames.checked_mul(channels) != Some(r.tokens.len() - r.at) {
        return Err("BVH motion sample count does not match frames and channels".into());
    }
    let units = state.units.system_unit.metres() * state.units.system_scale as f64;
    if !units.is_finite() || units <= 0.0 { return Err("invalid scene system units".into()); }
    let scale = (0.01 / units) as f32;
    if !scale.is_finite() || joints.iter().any(|j| !converted(j.offset, scale).is_finite() || j.end.is_some_and(|v| !converted(v, scale).is_finite())) {
        return Err("BVH offset exceeds scene units".into());
    }
    let basis = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    let mut positions = vec![Vec::with_capacity(frames); joints.len()];
    let mut rotations = vec![Vec::with_capacity(frames); joints.len()];
    for _ in 0..frames {
        for (i, j) in joints.iter().enumerate() {
            let mut p = j.offset;
            let mut q = Quat::IDENTITY;
            for &c in &j.channels {
                let value = r.number()?;
                if c < 3 {
                    // Position channels are the local joint position (Kimodo's
                    // Hips has a T-pose OFFSET as well as animated positions).
                    p[c] = value;
                } else {
                    q *= Quat::from_axis_angle([Vec3::X, Vec3::Y, Vec3::Z][c-3], value.to_radians());
                }
            }
            let position = converted(p, scale);
            let rotation = euler_near((basis * q * basis.inverse()).normalize(), rotations[i].last().copied());
            if !position.is_finite() || !rotation.is_finite() { return Err("BVH transform overflow".into()); }
            positions[i].push(position);
            rotations[i].push(rotation);
        }
    }
    let first = state.scene.next_id();
    let ids: Vec<_> = (0..joints.len()).map(|i| NodeId(first + i as u64)).collect();
    let mut names: BTreeSet<_> = state.scene.nodes().map(|n| n.name.clone()).collect();
    // A repeated skeleton needs an import namespace, not renamed joints:
    // Spine1/Finger1 are semantic identities, not disposable display counters.
    let namespace=if joints.iter().any(|j|names.contains(&j.name)) {
        let stem=path.file_stem().map(|s|s.to_string_lossy().into_owned()).unwrap_or_else(||"BVH".into());
        Some((0u64..).map(|i|if i==0{stem.clone()}else{format!("{stem}{i:03}")}).find(|prefix|!names.iter().any(|name|name.starts_with(&format!("{prefix}:")))).expect("finite scene has a free import namespace"))
    }else{None};
    let mut changes = Vec::new();
    for (i, j) in joints.iter().enumerate() {
        let direction = joints.iter().find(|c| c.parent == Some(i) && c.offset.length_squared() > 1e-8).map(|c| c.offset).or(j.end).unwrap_or(Vec3::ZERO);
        let direction = converted(direction, scale);
        let length = direction.length();
        if !length.is_finite() { return Err("BVH bone length overflow".into()); }
        let mut bone = ObjectKind::bone(length.max(0.001));
        let width = (length * 0.1).clamp(scale * 0.1, scale * 2.0);
        let _ = bone.set_parameter("Width", width as f64);
        let _ = bone.set_parameter("Height", width as f64);
        let _ = bone.set_parameter("Bone Axis", loom_scene::node::bone_axis_of(direction) as f64);
        let name = if let Some(prefix)=&namespace {format!("{prefix}:{}",j.name)} else if names.contains(&j.name) { unique_name(&j.name, &names) } else { j.name.clone() };
        names.insert(name.clone());
        let mut node = Node::new(&name, bone);
        node.id = ids[i];
        node.parent = j.parent.map(|p| ids[p]);
        node.layer = state.scene.current_layer;
        node.wire_color = [180, 180, 100];
        node.transform.position = keyed(converted(j.offset, scale), &positions[i], frame_scale);
        node.transform.rotation = keyed(Vec3::ZERO, &rotations[i], frame_scale);
        changes.push(Change::AddNode(node));
    }
    let before_clips = state.scene.animation_clips.clone();
    let mut after_clips = before_clips.clone();
    let clip_id = after_clips.iter().map(|c| c.id).max().unwrap_or(0).checked_add(1).ok_or("animation clip IDs exhausted")?;
    after_clips.push(loom_scene::AnimationClip {
        id: clip_id,
        name: path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "BVH".into()),
        nodes: ids.clone(), start: 0.0, end: clip_end,
    });
    changes.push(Change::SetAnimationClips { before: before_clips, after: after_clips });
    // Import owns one history step even if a drag release was lost.
    while state.undo.in_transaction() { state.undo.end(); }
    let timing_before = crate::timing_undo::snap(state);
    crate::execute::commit_exact(state, "Import BVH", Change::Group(changes))?;
    if preserve_timing {
        state.frame_range.0 = state.frame_range.0.min(0);
        state.frame_range.1 = state.frame_range.1.max(clip_end.ceil() as i32);
    } else {
        state.fps = 1.0 / dt;
        state.frame_range = (0, frames as i32 - 1);
        crate::execute::set_frame(state, 0)?;
    }
    crate::timing_undo::record(state, timing_before);
    state.selection = ids.iter().copied().collect();
    state.sub_selection.clear();
    state.selection_mode = SelectionMode::Object;
    Ok(ids)
}
