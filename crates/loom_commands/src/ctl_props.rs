//! Controller properties: the Expression, Noise, Spring and Audio
//! controller dialogs' edits. Every edit is one undoable step on the track.

use crate::command::Command;
use crate::controllers::find_track;
use crate::state::AppState;
use crate::viewport::Dialog;
use loom_scene::expr::{self, ExprCtrl, ExprSource, ExprVar};
use loom_scene::tracks::{self, TrackPath};
use loom_scene::NodeId;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

pub fn handles(c: &Command) -> bool {
    use Command as C;
    matches!(
        c,
        C::ControllerProperties { .. }
            | C::ExpressionCreateVariable { .. }
            | C::ExpressionDeleteVariable { .. }
            | C::ExpressionRenameVariable { .. }
            | C::ExpressionAssignConstant { .. }
            | C::ExpressionAssignController { .. }
            | C::ExpressionSetTickOffset { .. }
            | C::ExpressionSet { .. }
            | C::ExpressionSave { .. }
            | C::ExpressionLoad { .. }
            | C::ExpressionFileDialog { .. }
            | C::NoiseSetParameter { .. }
            | C::SpringSetParameter { .. }
            | C::SpringAddSpring { .. }
            | C::SpringRemoveSpring { .. }
            | C::SpringPickSprings
            | C::AudioSetParameter { .. }
            | C::AudioChooseSound { .. }
            | C::AudioSoundDialog
    )
}

/// The controller's kind ("Expression", "Noise", "Spring", "Audio", "Keys"…).
pub fn controller_kind(state: &AppState, id: NodeId, track: &str) -> Option<String> {
    let t = find_track(state, id, track).ok()?;
    let p = tracks::read(&state.scene, &t)?;
    p.controller_json().as_object()?.keys().next().cloned()
}

/// The typed body of the track's controller of kind `tag`.
pub fn read_ctl<T: DeserializeOwned>(state: &AppState, id: NodeId, track: &str, tag: &str) -> Result<(TrackPath, T), String> {
    let t = find_track(state, id, track)?;
    let p = tracks::read(&state.scene, &t).ok_or("no such track")?;
    let c = p.controller_json();
    let body = c.get(tag).ok_or_else(|| format!("{} has no {tag} controller", t.name))?;
    Ok((t, serde_json::from_value(body.clone()).map_err(|e| e.to_string())?))
}

/// Edits the track's `tag` controller as one undo step.
fn edit_ctl<T: DeserializeOwned + Serialize>(state: &mut AppState, id: NodeId, track: &str, tag: &str, label: &str, f: impl FnOnce(&mut T, &AppState) -> Result<(), String>) -> Result<Value, String> {
    let (t, mut body) = read_ctl::<T>(state, id, track, tag)?;
    f(&mut body, state)?;
    let c = json!({ tag: serde_json::to_value(&body).map_err(|e| e.to_string())? });
    crate::anim_cmds::edit_nodes(state, label, &[id], |x| *x == t, |_, p| {
        let _ = p.set_controller_json(&c);
    })?;
    Ok(json!({ "id": id.0, "track": t.name }))
}

fn expr_edit(state: &mut AppState, id: u64, track: &str, label: &str, f: impl FnOnce(&mut ExprCtrl, &AppState) -> Result<(), String>) -> Result<Value, String> {
    edit_ctl(state, NodeId(id), track, "Expression", label, f)
}

fn var_mut<'a>(e: &'a mut ExprCtrl, name: &str) -> Result<&'a mut ExprVar, String> {
    e.vars.iter_mut().find(|v| v.name == name).ok_or_else(|| format!("no variable \"{name}\""))
}

/// Whether the controller JSON `v` reads the track (`node`, `owner`, `pointer`) anywhere.
fn reads(v: &Value, node: u64, t: &TrackPath) -> bool {
    match v {
        Value::Object(m) => {
            if m.get("node").and_then(Value::as_u64) == Some(node)
                && m.get("pointer").and_then(Value::as_str) == Some(t.pointer.as_str())
                && m.get("owner").and_then(|o| serde_json::from_value::<tracks::TrackOwner>(o.clone()).ok()) == Some(t.owner)
            {
                return true;
            }
            m.values().any(|x| reads(x, node, t))
        }
        Value::Array(a) => a.iter().any(|x| reads(x, node, t)),
        _ => false,
    }
}

/// A source for a variable or spring: track `source_track` of node
/// `source`, refused when it reads `target` (directly or through others).
fn source_of(state: &AppState, target: &TrackPath, source: NodeId, source_track: &str, vector: Option<bool>) -> Result<ExprSource, String> {
    let st = find_track(state, source, source_track)?;
    if st == *target {
        return Err("a variable cannot read its own track".into());
    }
    let p = tracks::read(&state.scene, &st).ok_or("no such track")?;
    match (p.ty(), vector) {
        (loom_scene::param::ParamType::Vec3, Some(false)) => return Err("a scalar variable reads a float track".into()),
        (loom_scene::param::ParamType::F32, Some(true)) => return Err("a vector variable reads a position, scale or colour track".into()),
        (loom_scene::param::ParamType::F32 | loom_scene::param::ParamType::Vec3, _) => {}
        _ => return Err("only float and vector tracks can be read".into()),
    }
    if reads(&p.controller_json(), target.node.0, target) {
        return Err(format!("{} already depends on this track", st.name));
    }
    let node = state.scene.node(source).map(|n| n.name.clone()).unwrap_or_default();
    let copy = tracks::source_copy(&state.scene, &st).and_then(|c| serde_json::from_value(c).ok());
    Ok(ExprSource { node: source.0, owner: st.owner, pointer: st.pointer.clone(), label: format!("{node} \\ {}", st.name), copy, link: Default::default() })
}

fn open(state: &mut AppState, id: u64, track: String, d: Dialog) -> Result<Value, String> {
    state.ctl_track = Some((id, track));
    state.dialog = Some(d);
    Ok(json!({ "dialog": format!("{d:?}") }))
}

fn ctl_track(state: &AppState) -> Result<(u64, String), String> {
    state.ctl_track.clone().ok_or_else(|| "no controller dialog is open".into())
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    match cmd {
        C::ControllerProperties { id, track } => {
            let d = match controller_kind(state, NodeId(id), &track).as_deref() {
                Some("Expression") => Dialog::ExpressionController,
                Some("Noise") => Dialog::NoiseController,
                Some("Spring") => Dialog::SpringController,
                Some("Audio") => Dialog::AudioController,
                _ => return Err(format!("{track}'s controller has no properties dialog")),
            };
            open(state, id, track, d)
        }
        C::ExpressionCreateVariable { id, track, name, vector } => expr_edit(state, id, &track, "Expression Create Variable", |e, _| {
            if !expr::valid_name(&name) {
                return Err(format!("\"{name}\" is not a valid variable name"));
            }
            if e.var(&name).is_some() {
                return Err(format!("a variable \"{name}\" exists"));
            }
            e.vars.push(ExprVar { name, vector, tick_offset: 0, constant: [0.0; 3], source: None });
            Ok(())
        }),
        C::ExpressionDeleteVariable { id, track, name } => expr_edit(state, id, &track, "Expression Delete Variable", |e, _| {
            let i = e.vars.iter().position(|v| v.name == name).ok_or_else(|| format!("no variable \"{name}\""))?;
            e.vars.remove(i);
            Ok(())
        }),
        C::ExpressionRenameVariable { id, track, name, new_name } => expr_edit(state, id, &track, "Expression Rename Variable", |e, _| {
            if !expr::valid_name(&new_name) {
                return Err(format!("\"{new_name}\" is not a valid variable name"));
            }
            if new_name != name && e.var(&new_name).is_some() {
                return Err(format!("a variable \"{new_name}\" exists"));
            }
            var_mut(e, &name)?.name = new_name;
            Ok(())
        }),
        C::ExpressionAssignConstant { id, track, name, value } => expr_edit(state, id, &track, "Expression Assign Constant", |e, _| {
            let v = var_mut(e, &name)?;
            v.constant = value;
            v.source = None;
            Ok(())
        }),
        C::ExpressionAssignController { id, track, name, source_id, source_track } => {
            let target = find_track(state, NodeId(id), &track)?;
            let vector = read_ctl::<ExprCtrl>(state, NodeId(id), &track, "Expression")?.1.var(&name).map(|v| v.vector).ok_or_else(|| format!("no variable \"{name}\""))?;
            let src = source_of(state, &target, NodeId(source_id), &source_track, Some(vector))?;
            expr_edit(state, id, &track, "Expression Assign Controller", move |e, _| {
                var_mut(e, &name)?.source = Some(src);
                Ok(())
            })
        }
        C::ExpressionSetTickOffset { id, track, name, ticks } => expr_edit(state, id, &track, "Expression Tick Offset", |e, _| {
            var_mut(e, &name)?.tick_offset = ticks;
            Ok(())
        }),
        C::ExpressionSet { id, track, expression, description } => {
            let dims = find_track(state, NodeId(id), &track)?.ty;
            let dims = if dims == loom_scene::param::ParamType::Vec3 { 3 } else { 1 };
            expr_edit(state, id, &track, "Expression Evaluate", |e, _| {
                if let Some(x) = expression {
                    e.check(&x, dims)?;
                    e.expression = x;
                }
                if let Some(d) = description {
                    e.description = d;
                }
                Ok(())
            })
        }
        C::ExpressionSave { id, track, path } => {
            let (_, e) = read_ctl::<ExprCtrl>(state, NodeId(id), &track, "Expression")?;
            std::fs::write(&path, &e.expression).map_err(|err| format!("{path}: {err}"))?;
            if state.ctl_track.is_some() {
                state.dialog = Some(Dialog::ExpressionController);
            }
            Ok(json!({ "path": path }))
        }
        C::ExpressionLoad { id, track, path } => {
            // A saved expression holds no variables: only the text comes back.
            let text = std::fs::read_to_string(&path).map_err(|err| format!("{path}: {err}"))?;
            let r = expr_edit(state, id, &track, "Expression Load", |e, _| {
                e.expression = text.trim().to_owned();
                Ok(())
            })?;
            if state.ctl_track.is_some() {
                state.dialog = Some(Dialog::ExpressionController);
            }
            Ok(r)
        }
        C::ExpressionFileDialog { save } => {
            ctl_track(state)?;
            state.dialog = Some(Dialog::ExpressionFile { save });
            Ok(json!({}))
        }
        C::NoiseSetParameter { id, track, name, value, index } => edit_ctl(state, NodeId(id), &track, "Noise", "Noise Parameter", |n: &mut loom_scene::param::NoiseCtrl, _| {
            let v = value as f32;
            let i = index.unwrap_or(0) as usize;
            match name.to_ascii_lowercase().as_str() {
                "seed" => n.seed = value.max(0.0) as u32,
                "frequency" => n.frequency = v.max(0.001),
                "strength" => {
                    if n.strength.len() <= i {
                        let last = n.strength.last().copied().unwrap_or(50.0);
                        n.strength.resize(i + 1, last);
                    }
                    n.strength[i] = v;
                }
                ">0" => {
                    if n.positive.len() <= i {
                        n.positive.resize(i + 1, false);
                    }
                    n.positive[i] = value != 0.0;
                }
                "ramp in" => n.ramp_in = v.max(0.0),
                "ramp out" => n.ramp_out = v.max(0.0),
                "fractal noise" => n.fractal = value != 0.0,
                "roughness" => n.roughness = v.clamp(0.0, 1.0),
                _ => return Err(format!("no Noise parameter \"{name}\"")),
            }
            Ok(())
        }),
        C::SpringSetParameter { id, track, name, value, index, relative } => edit_ctl(state, NodeId(id), &track, "Spring", "Spring Parameter", |s: &mut loom_scene::param::SpringCtrl<glam::Vec3>, _| {
            let v = value as f32;
            let rel = relative.unwrap_or(false);
            let set = |x: &mut f32, lo: f32, hi: f32| *x = (if rel { *x + v } else { v }).clamp(lo, hi);
            let i = index.unwrap_or(0) as usize;
            match name.to_ascii_lowercase().as_str() {
                "mass" => set(&mut s.mass, 0.0, 1.0e6),
                "drag" => set(&mut s.drag, 0.0, 10.0),
                "tension" | "dampening" => {
                    let damp = name.eq_ignore_ascii_case("dampening");
                    let slot = if i == 0 {
                        if damp { &mut s.dampening } else { &mut s.tension }
                    } else {
                        let l = s.springs.get_mut(i - 1).ok_or("no such spring")?;
                        if damp { &mut l.dampening } else { &mut l.tension }
                    };
                    set(slot, 0.0, 1.0e6);
                }
                "start frame" => s.start = v,
                "iterations" => s.iterations = (value.round().clamp(0.0, 4.0)) as u32,
                "x effect" | "y effect" | "z effect" => {
                    let a = (name.as_bytes()[0].to_ascii_lowercase() - b'x') as usize;
                    set(&mut s.effect[a], 0.0, 1000.0);
                }
                _ => return Err(format!("no Spring parameter \"{name}\"")),
            }
            Ok(())
        }),
        C::SpringAddSpring { id, track, source_id } => {
            let target = find_track(state, NodeId(id), &track)?;
            if source_id == id {
                return Err("the object's own spring is Self Influence".into());
            }
            let src = source_of(state, &target, NodeId(source_id), "Position", Some(true))?;
            edit_ctl(state, NodeId(id), &track, "Spring", "Spring Add", move |s: &mut loom_scene::param::SpringCtrl<glam::Vec3>, _| {
                if s.springs.iter().any(|l| l.source.node == source_id) {
                    return Err("that object is already a spring".into());
                }
                s.springs.push(loom_scene::param::SpringLink { source: src, tension: s.tension, dampening: s.dampening });
                Ok(())
            })
        }
        C::SpringRemoveSpring { id, track, index } => edit_ctl(state, NodeId(id), &track, "Spring", "Spring Remove", |s: &mut loom_scene::param::SpringCtrl<glam::Vec3>, _| {
            if index == 0 {
                return Err("Self Influence cannot be removed".into());
            }
            if index as usize > s.springs.len() {
                return Err("no such spring".into());
            }
            s.springs.remove(index as usize - 1);
            Ok(())
        }),
        C::SpringPickSprings => {
            ctl_track(state)?;
            let on = state.pick_mode != Some(crate::state::PickMode::SpringAdd);
            state.pick_mode = on.then_some(crate::state::PickMode::SpringAdd);
            Ok(json!({ "picking": on }))
        }
        C::AudioSetParameter { id, track, name, value, index } => edit_ctl(state, NodeId(id), &track, "Audio", "Audio Parameter", |a: &mut loom_scene::audio::AudioCtrl, _| {
            let v = value as f32;
            let i = index.unwrap_or(0) as usize;
            let at = |x: &mut Vec<f32>| -> Result<(), String> {
                *x.get_mut(i).ok_or("no such component")? = v;
                Ok(())
            };
            match name.to_ascii_lowercase().as_str() {
                "absolute value" => a.absolute = value != 0.0,
                "threshold" => a.threshold = v.clamp(0.0, 1.0),
                "oversampling" => a.oversampling = value.round().clamp(1.0, 1000.0) as u32,
                "base" => at(&mut a.base)?,
                "target" => at(&mut a.target)?,
                "channel" => {
                    a.channel = match value.round() as i64 {
                        0 => loom_scene::audio::AudioChannel::Left,
                        1 => loom_scene::audio::AudioChannel::Right,
                        _ => loom_scene::audio::AudioChannel::Mix,
                    }
                }
                _ => return Err(format!("no Audio parameter \"{name}\"")),
            }
            Ok(())
        }),
        C::AudioChooseSound { id, track, path } => {
            if let Some(p) = &path {
                let bytes = std::fs::read(p).map_err(|e| format!("{p}: {e}"))?;
                loom_scene::audio::read_wav(&bytes).map_err(|e| format!("{p}: {e}"))?;
            }
            let r = edit_ctl(state, NodeId(id), &track, "Audio", if path.is_some() { "Audio Choose Sound" } else { "Audio Remove Sound" }, |a: &mut loom_scene::audio::AudioCtrl, _| {
                a.path = path.unwrap_or_default();
                Ok(())
            })?;
            if state.ctl_track.is_some() {
                state.dialog = Some(Dialog::AudioController);
            }
            Ok(r)
        }
        C::AudioSoundDialog => {
            ctl_track(state)?;
            state.dialog = Some(Dialog::AudioSoundFile);
            Ok(json!({}))
        }
        _ => Err("not a controller properties command".into()),
    }
}

/// After Assign Controller: the new controller's dialog opens, as it does
/// in the reference for Expression, Noise, Spring and Audio.
pub fn open_after_assign(state: &mut AppState, id: u64, track: &str) {
    let d = match controller_kind(state, NodeId(id), track).as_deref() {
        Some("Expression") => Dialog::ExpressionController,
        Some("Noise") => Dialog::NoiseController,
        Some("Spring") => Dialog::SpringController,
        Some("Audio") => Dialog::AudioController,
        _ => return,
    };
    let _ = open(state, id, track.to_owned(), d);
}
