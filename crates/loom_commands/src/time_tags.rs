//! Time tags (the status bar's Time Tag box): named frames to jump to. A tag
//! can follow another at a fixed offset (Relative To) and can be locked so
//! it keeps its frame. Saved with the scene.

use crate::command::Command as C;
use crate::AppState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeTag {
    pub name: String,
    /// The tag's own frame, or its offset from the tag it is relative to.
    pub time: i32,
    /// Lock Time: the tag keeps its frame.
    pub locked: bool,
    /// Relative To: the name of the tag this one follows.
    pub relative_to: Option<String>,
}

/// The frame a tag stands for (following its Relative To chain).
pub fn tag_time(tags: &[TimeTag], name: &str) -> Option<i32> {
    let mut t = 0;
    let mut cur = tags.iter().find(|x| x.name == name)?;
    for _ in 0..=tags.len() {
        t += cur.time;
        match cur.relative_to.as_deref().and_then(|r| tags.iter().find(|x| x.name == r)) {
            Some(next) => cur = next,
            None => return Some(t),
        }
    }
    None // a loop of Relative To references
}

/// Would `name` end up relative to itself through `to`?
fn makes_loop(tags: &[TimeTag], name: &str, to: &str) -> bool {
    let mut cur = Some(to.to_owned());
    for _ in 0..=tags.len() {
        match cur {
            Some(c) if c == name => return true,
            Some(c) => cur = tags.iter().find(|x| x.name == c).and_then(|x| x.relative_to.clone()),
            None => return false,
        }
    }
    true
}

pub fn handles(c: &C) -> bool {
    matches!(c, C::AddTimeTag { .. } | C::EditTimeTag { .. } | C::DeleteTimeTag { .. } | C::GoToTimeTag { .. })
}

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    match cmd {
        C::AddTimeTag { name, time, lock, relative_to } => {
            let name = name.trim().to_owned();
            if name.is_empty() || state.time_tags.iter().any(|t| t.name == name) {
                return Err(format!("cannot add a tag named {name:?}"));
            }
            let frame = time.unwrap_or(state.current_frame);
            // Relative To: the tag keeps its frame now, stored as the offset.
            let rel = match relative_to {
                Some(r) => Some((tag_time(&state.time_tags, &r).ok_or_else(|| format!("no tag {r:?}"))?, r)),
                None => None,
            };
            let (time, relative_to) = match rel {
                Some((base, r)) => (frame - base, Some(r)),
                None => (frame, None),
            };
            state.time_tags.push(TimeTag { name: name.clone(), time, locked: lock.unwrap_or(false), relative_to });
            Ok(json!({ "tag": name, "frame": frame }))
        }
        C::EditTimeTag { name, new_name, time, lock, relative_to } => {
            let i = state.time_tags.iter().position(|t| t.name == name).ok_or_else(|| format!("no tag {name:?}"))?;
            let frame_now = tag_time(&state.time_tags, &name).unwrap_or(0);
            if let Some(l) = lock {
                state.time_tags[i].locked = l;
            }
            if let Some(r) = relative_to {
                // Empty: no longer relative; the tag keeps its frame.
                let r = r.trim().to_owned();
                if r.is_empty() {
                    state.time_tags[i].relative_to = None;
                    state.time_tags[i].time = frame_now;
                } else {
                    if makes_loop(&state.time_tags, &name, &r) {
                        return Err(format!("{name} cannot be relative to {r}: they would follow each other"));
                    }
                    let base = tag_time(&state.time_tags, &r).ok_or_else(|| format!("no tag {r:?}"))?;
                    state.time_tags[i].relative_to = Some(r);
                    state.time_tags[i].time = frame_now - base;
                }
            }
            if let Some(t) = time {
                if state.time_tags[i].locked {
                    return Err(format!("{name} is locked"));
                }
                let base = match state.time_tags[i].relative_to.clone() {
                    Some(r) => tag_time(&state.time_tags, &r).unwrap_or(0),
                    None => 0,
                };
                state.time_tags[i].time = t - base;
            }
            if let Some(n) = new_name {
                let n = n.trim().to_owned();
                if n.is_empty() || (n != name && state.time_tags.iter().any(|t| t.name == n)) {
                    return Err(format!("cannot rename the tag to {n:?}"));
                }
                for t in state.time_tags.iter_mut() {
                    if t.relative_to.as_deref() == Some(name.as_str()) {
                        t.relative_to = Some(n.clone());
                    }
                }
                state.time_tags[i].name = n;
            }
            let t = &state.time_tags[i];
            Ok(json!({ "tag": t.name, "frame": tag_time(&state.time_tags, &t.name), "locked": t.locked, "relative_to": t.relative_to }))
        }
        C::DeleteTimeTag { name } => {
            let frames: Vec<(String, i32)> = state.time_tags.iter().filter_map(|t| Some((t.name.clone(), tag_time(&state.time_tags, &t.name)?))).collect();
            let before = state.time_tags.len();
            state.time_tags.retain(|t| t.name != name);
            if state.time_tags.len() == before {
                return Err(format!("no tag {name:?}"));
            }
            // Tags that followed it keep their frames.
            for t in state.time_tags.iter_mut() {
                if t.relative_to.as_deref() == Some(name.as_str()) {
                    t.relative_to = None;
                    t.time = frames.iter().find(|(n, _)| *n == t.name).map(|(_, f)| *f).unwrap_or(t.time);
                }
            }
            Ok(json!({ "deleted": name }))
        }
        C::GoToTimeTag { name } => {
            let f = tag_time(&state.time_tags, &name).ok_or_else(|| format!("no tag {name:?}"))?;
            crate::execute::set_frame(state, f)
        }
        other => Err(format!("{} is not a time tag command", other.name())),
    }
}
