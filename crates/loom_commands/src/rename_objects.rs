//! Tools > Rename Objects: rename several objects at once (Base Name,
//! Prefix, Remove First / Last N characters, Suffix, Numbered).

use crate::command::Command as C;
use crate::execute::commit;
use crate::AppState;
use loom_scene::{Change, NodeId};
use serde_json::{json, Value};

/// The Rename Objects fields: each `Some` is a checked option.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RenameSpec {
    pub base_name: Option<String>,
    pub prefix: Option<String>,
    pub remove_first: Option<usize>,
    pub suffix: Option<String>,
    pub remove_last: Option<usize>,
    /// Numbered: base number and step.
    pub numbered: Option<(i64, i64)>,
}

/// The `i`-th object's new name (its old name `name`).
pub fn new_name(name: &str, i: usize, s: &RenameSpec) -> String {
    let mut chars: Vec<char> = s.base_name.as_deref().unwrap_or(name).chars().collect();
    if let Some(n) = s.remove_first {
        chars.drain(..n.min(chars.len()));
    }
    if let Some(n) = s.remove_last {
        chars.truncate(chars.len().saturating_sub(n));
    }
    let mut out: String = s.prefix.clone().unwrap_or_default();
    out.extend(chars);
    out.push_str(s.suffix.as_deref().unwrap_or(""));
    if let Some((base, step)) = s.numbered {
        out.push_str(&(base + step * i as i64).to_string());
    }
    out
}

pub fn handles(c: &C) -> bool {
    matches!(c, C::RenameObjectsDialog | C::RenameObjects { .. })
}

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    match cmd {
        C::RenameObjectsDialog => {
            state.dialog = Some(crate::viewport::Dialog::RenameObjects);
            Ok(json!({}))
        }
        C::RenameObjects { ids, base_name, prefix, remove_first, suffix, remove_last, base_number, step } => {
            let ids: Vec<NodeId> = match ids {
                Some(v) => v.into_iter().map(NodeId).collect(),
                None => state.selection.iter().copied().collect(),
            };
            if ids.is_empty() {
                return Err("nothing to rename".into());
            }
            let spec = RenameSpec { base_name, prefix, remove_first: remove_first.map(|n| n as usize), suffix, remove_last: remove_last.map(|n| n as usize), numbered: base_number.map(|b| (b as i64, step.unwrap_or(1) as i64)) };
            let mut changes = Vec::new();
            let mut names = Vec::new();
            for (i, id) in ids.iter().enumerate() {
                let before = state.scene.node(*id).ok_or_else(|| format!("no node {}", id.0))?.name.clone();
                let after = new_name(&before, i, &spec);
                names.push(after.clone());
                if after != before {
                    changes.push(Change::SetName { id: *id, before, after });
                }
            }
            if !changes.is_empty() {
                commit(state, "Rename Objects", Change::Group(changes))?;
            }
            Ok(json!({ "names": names }))
        }
        other => Err(format!("{} is not a Rename Objects command", other.name())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_option_changes_the_name() {
        let s = |f: fn(&mut RenameSpec)| {
            let mut s = RenameSpec::default();
            f(&mut s);
            s
        };
        assert_eq!(new_name("Box001", 0, &s(|s| s.base_name = Some("Crate".into()))), "Crate");
        assert_eq!(new_name("Box001", 0, &s(|s| s.prefix = Some("L_".into()))), "L_Box001");
        assert_eq!(new_name("Box001", 0, &s(|s| s.remove_first = Some(3))), "001");
        assert_eq!(new_name("Box001", 0, &s(|s| s.remove_last = Some(3))), "Box");
        assert_eq!(new_name("Box", 0, &s(|s| s.remove_last = Some(9))), "");
        assert_eq!(new_name("Box001", 0, &s(|s| s.suffix = Some("_hi".into()))), "Box001_hi");
        let numbered = s(|s| {
            s.base_name = Some("Wall".into());
            s.numbered = Some((10, 5));
        });
        assert_eq!((new_name("a", 0, &numbered), new_name("b", 2, &numbered)), ("Wall10".into(), "Wall20".into()));
    }
}
