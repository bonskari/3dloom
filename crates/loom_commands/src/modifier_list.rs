//! The Modifier List's most-used section and search.

use loom_scene::Modifier;
use std::collections::BTreeMap;

/// How often each modifier was added (persisted per user by the UI).
pub type Usage = BTreeMap<String, u32>;

/// The common modelling modifiers a fresh install shows on top.
const COMMON: [&str; 12] = ["Edit Poly", "TurboSmooth", "Symmetry", "UVW Map", "Unwrap UVW", "Shell", "FFD 4x4x4", "Skin", "Bend", "Turn to Poly", "Edit Normals", "Noise"];

/// The `n` names for the top section: the user's most used first (ties by
/// name), then the common ones not yet listed.
pub fn most_used(usage: &Usage, n: usize) -> Vec<String> {
    let mut used: Vec<(&String, &u32)> = usage.iter().filter(|(name, c)| **c > 0 && Modifier::by_name(name).is_some()).collect();
    used.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let mut out: Vec<String> = used.into_iter().map(|(name, _)| name.clone()).take(n).collect();
    for c in COMMON {
        if out.len() >= n {
            break;
        }
        if !out.iter().any(|o| o == c) && Modifier::by_name(c).is_some() {
            out.push(c.to_owned());
        }
    }
    out
}

/// Every Modifier List name containing `query` (any case), world-space ones first.
pub fn search(query: &str) -> Vec<String> {
    let q = query.trim().to_lowercase();
    Modifier::WSM_LIST.iter().chain(Modifier::LIST.iter()).filter(|n| n.to_lowercase().contains(&q)).map(|n| (*n).to_owned()).collect()
}

pub fn usage_to_json(usage: &Usage) -> String {
    serde_json::to_string_pretty(usage).unwrap_or_default()
}

/// Unreadable files give an empty table (the common list then shows).
pub fn usage_from_json(text: &str) -> Usage {
    serde_json::from_str(text).unwrap_or_default()
}
