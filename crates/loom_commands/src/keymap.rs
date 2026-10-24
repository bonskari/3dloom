//! Default keyboard shortcuts, parsed from the keyboard chart in
//! `docs/reference/main-ui-shortcuts.tsv`.

use crate::command::{DEFERRED, OUT_OF_SCOPE};
use std::collections::HashMap;

const CHART: &str = include_str!("../../../docs/reference/main-ui-shortcuts.tsv");
/// The chart's "Editable Poly" action table (action, chord).
const POLY_CHART: &str = include_str!("../../../docs/reference/editable-poly-shortcuts.tsv");
/// The chart's "FFD" action table (action, chord).
/// The chart's "Edit Poly" action table: the Editable Poly actions again
/// (two rows spell them differently), bound while an Edit Poly entry is active.
const EDIT_POLY_CHART: &str = include_str!("../../../docs/reference/edit-poly-shortcuts.tsv");
/// The chart's "(Spline)" rows of the Main UI table plus the Edit Spline table.
const SPLINE_CHART: &str = include_str!("../../../docs/reference/editable-spline-shortcuts.tsv");
const FFD_CHART: &str = include_str!("../../../docs/reference/ffd-shortcuts.tsv");
/// The chart's "Unwrap UVW" table (`docs/reference/unwrap-uvw-shortcuts.tsv`).
const UNWRAP_CHART: &str = include_str!("../../../docs/reference/unwrap-uvw-shortcuts.tsv");
/// The "Skin" group (no default chords in the 2015 chart).
const SKIN_CHART: &str = include_str!("../../../docs/reference/skin-shortcuts.tsv");
const TRACK_VIEW_CHART: &str = include_str!("../../../docs/reference/track-view-shortcuts.tsv");
const EDIT_NORMALS_CHART: &str = include_str!("../../../docs/reference/edit-normals-shortcuts.tsv");

/// The chart's Edit Normals rows (action, chords), aliased where another group owns the name.
pub fn edit_normals_chart_rows() -> Vec<(&'static str, &'static str)> {
    EDIT_NORMALS_CHART
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(a, c)| (crate::command::EDIT_NORMALS_ALIASES.iter().find(|(from, _)| *from == a).map(|(_, to)| *to).unwrap_or(a), c))
        .collect()
}

/// The chart's Track View rows (action, chords), names aliased where another group owns them.
pub fn track_view_chart_rows() -> Vec<(&'static str, &'static str)> {
    TRACK_VIEW_CHART
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(a, c)| (crate::command::TRACK_VIEW_ALIASES.iter().find(|(from, _)| *from == a).map(|(_, to)| *to).unwrap_or(a), c))
        .collect()
}

/// Keys that appear in the chart. Letters and digits are their
/// uppercase/ASCII names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    Num0, Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    Space, Enter, Insert, Delete, Home, End, PageUp, PageDown, Backspace, Semicolon,
    Comma, Period, Slash, Backslash, OpenBracket, CloseBracket, Backtick, Quote, Equals, Minus,
    NumPadAdd, NumPadSubtract, NumPadDivide, NumPadMultiply,
    NumPad0, NumPad1, NumPad2, NumPad3, NumPad4, NumPad5, NumPad6, NumPad7, NumPad8, NumPad9,
    UpArrow, DownArrow, LeftArrow, RightArrow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Chord {
    pub key: Key,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// Parses one chord such as `Alt+Ctrl+Z`, `NumPad +` or `Ctrl+\``.
pub fn parse_chord(s: &str) -> Result<Chord, String> {
    let s = s.trim();
    let (mods, key_str) = match s.rsplit_once('+') {
        // "NumPad +" and "Ctrl+=" style: the key itself may be '+'.
        Some((m, "")) if m.ends_with("NumPad ") => (m.trim_end_matches("NumPad ").to_owned(), "NumPad +".to_owned()),
        Some((m, "")) => (m.to_owned(), "+".to_owned()),
        Some((m, k)) => (m.to_owned(), k.to_owned()),
        None => (String::new(), s.to_owned()),
    };
    let mut chord = Chord { key: Key::A, ctrl: false, alt: false, shift: false };
    for m in mods.split('+').filter(|m| !m.is_empty()) {
        match m {
            "Ctrl" => chord.ctrl = true,
            "Alt" => chord.alt = true,
            "Shift" => chord.shift = true,
            other => return Err(format!("unknown modifier `{other}` in `{s}`")),
        }
    }
    chord.key = parse_key(&key_str).ok_or_else(|| format!("unknown key `{key_str}` in `{s}`"))?;
    Ok(chord)
}

/// Every key with its chart spelling (what `parse_chord` reads and
/// `Chord::chart_text` writes).
const KEY_NAMES: [(Key, &str); 86] = {
    use Key::*;
    [
        (A, "A"), (B, "B"), (C, "C"), (D, "D"), (E, "E"), (F, "F"), (G, "G"), (H, "H"), (I, "I"),
        (J, "J"), (K, "K"), (L, "L"), (M, "M"), (N, "N"), (O, "O"), (P, "P"), (Q, "Q"), (R, "R"),
        (S, "S"), (T, "T"), (U, "U"), (V, "V"), (W, "W"), (X, "X"), (Y, "Y"), (Z, "Z"),
        (Num0, "0"), (Num1, "1"), (Num2, "2"), (Num3, "3"), (Num4, "4"),
        (Num5, "5"), (Num6, "6"), (Num7, "7"), (Num8, "8"), (Num9, "9"),
        (F1, "F1"), (F2, "F2"), (F3, "F3"), (F4, "F4"), (F5, "F5"), (F6, "F6"),
        (F7, "F7"), (F8, "F8"), (F9, "F9"), (F10, "F10"), (F11, "F11"), (F12, "F12"),
        (Space, "Space"), (Enter, "Enter"), (Insert, "Insert"), (Delete, "Delete"), (Home, "Home"), (End, "End"),
        (PageUp, "PageUp"), (PageDown, "PageDown"), (Backspace, "Backspace"), (Semicolon, ";"),
        (Comma, ","), (Period, "."), (Slash, "/"), (Backslash, "\\"), (OpenBracket, "["),
        (CloseBracket, "]"), (Backtick, "`"), (Quote, "'"), (Equals, "="), (Minus, "-"),
        (NumPadAdd, "NumPad +"), (NumPadSubtract, "NumPad -"), (NumPadDivide, "NumPad /"),
        (NumPadMultiply, "NumPad *"),
        (NumPad0, "NumPad 0"), (NumPad1, "NumPad 1"), (NumPad2, "NumPad 2"), (NumPad3, "NumPad 3"),
        (NumPad4, "NumPad 4"), (NumPad5, "NumPad 5"), (NumPad6, "NumPad 6"), (NumPad7, "NumPad 7"),
        (NumPad8, "NumPad 8"), (NumPad9, "NumPad 9"),
        (UpArrow, "Up Arrow"), (DownArrow, "Down Arrow"), (LeftArrow, "Left Arrow"), (RightArrow, "Right Arrow"),
    ]
};

/// Every key the chart knows.
pub const ALL_KEYS: [Key; 86] = {
    let mut out = [Key::A; 86];
    let mut i = 0;
    while i < 86 {
        out[i] = KEY_NAMES[i].0;
        i += 1;
    }
    out
};

fn parse_key(k: &str) -> Option<Key> {
    KEY_NAMES.iter().find(|(_, n)| *n == k).map(|(key, _)| *key)
}

/// `(action, chords)` rows of the Main UI chart.
///
/// One export artifact is corrected: the chart prints `Delete Objects` as
/// `.` because Write Keyboard Chart emits VK_DELETE (0x2E) as the ASCII
/// character 0x2E. The User Reference binds Delete Objects to the Delete
/// key, and `.` belongs to Forward Time One Unit.
pub fn chart_rows() -> Vec<(&'static str, Vec<&'static str>)> {
    CHART
        .lines()
        .filter_map(|l| {
            let (action, chords) = l.split_once('\t')?;
            let chords = if action == "Delete Objects" { vec!["Delete"] } else { chords.split(", ").collect() };
            Some((action, chords))
        })
        .collect()
}

/// Bindings the chart does not carry for 2010 (2015 moved them to the
/// Smart Select / Smart Scale macro scripts). See the phase 1a plan.
/// Shift+, / Shift+. step between keys (spec §6.6).
const EXTRA_2010: &[(&str, &str)] = &[("Q", "Select Object"), ("R", "Select and Scale"), ("Shift+,", "Go To Previous Key"), ("Shift+.", "Go To Next Key")];

/// Ring / Loop chords the 2010 poly tables leave unbound. Graphite and the 2015
/// chart (`Select Sub-object Ring/Loop`) use Alt+R / Alt+L.
const POLY_EXTRA: &[(&str, &str)] = &[("Alt+R", "Select Edge Ring"), ("Alt+L", "Select Edge Loop")];

/// One row of the Customize User Interface > Keyboard list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChartRow {
    pub group: &'static str,
    pub action: &'static str,
    /// Chord label, empty when unbound.
    pub shortcut: String,
}

/// `(action, chord)` rows of the Editable Poly chart (unbound rows have "").
pub fn poly_chart_rows() -> Vec<(&'static str, &'static str)> {
    POLY_CHART
        .lines()
        .filter_map(|l| {
            let (action, chord) = l.split_once('\t')?;
            let action = action.trim();
            (!action.is_empty()).then_some((action, chord.trim()))
        })
        .collect()
}

/// `(action, chord)` rows of the Edit Poly chart, with its two spellings
/// mapped to the Editable Poly action names.
pub fn edit_poly_chart_rows() -> Vec<(&'static str, &'static str)> {
    EDIT_POLY_CHART
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(a, c)| (a.trim(), c.trim()))
        .filter(|(a, _)| !a.is_empty())
        .map(|(a, c)| (match a { "Polygon Level" => "Face Level", "Select Edge Loop Up Subtract" => "Select Edge Loop Up SubTract", other => other }, c))
        .collect()
}

/// `(action, chord)` rows of the Editable Spline chart.
pub fn spline_chart_rows() -> Vec<(&'static str, &'static str)> {
    SPLINE_CHART.lines().filter_map(|l| l.split_once('\t')).map(|(a, c)| (a.trim(), c.trim())).filter(|(a, _)| !a.is_empty()).collect()
}

/// `(action, chord)` rows of the Unwrap UVW chart, with the names that
/// collide with other groups mapped to their "(Unwrap)" registrations.
pub fn unwrap_chart_rows() -> Vec<(&'static str, &'static str)> {
    UNWRAP_CHART
        .lines()
        .filter_map(|l| l.split_once('\t').or(Some((l, ""))))
        .map(|(a, c)| (a.trim(), c.trim()))
        .filter(|(a, _)| !a.is_empty() && !a.starts_with("----"))
        .map(|(a, c)| (crate::command::UNWRAP_ALIASES.iter().find(|(from, _)| *from == a).map(|(_, to)| *to).unwrap_or(a), c))
        .collect()
}

/// `(action, chord)` rows of the FFD chart.
pub fn ffd_chart_rows() -> Vec<(&'static str, &'static str)> {
    FFD_CHART.lines().filter_map(|l| l.split_once('\t')).map(|(a, c)| (a.trim(), c.trim())).filter(|(a, _)| !a.is_empty()).collect()
}

/// `(action, chord)` rows of the Skin chart.
pub fn skin_chart_rows() -> Vec<(&'static str, &'static str)> {
    SKIN_CHART.lines().filter_map(|l| l.split_once('\t').or(Some((l, "")))).map(|(a, c)| (a.trim(), c.trim())).filter(|(a, _)| !a.is_empty()).collect()
}

/// Every registered command with its shortcut, in registry order.
pub fn keyboard_chart() -> Vec<ChartRow> {
    keyboard_chart_with(&CustomKeys::default())
}

/// The chart with the user's shortcut edits.
pub fn keyboard_chart_with(keys: &CustomKeys) -> Vec<ChartRow> {
    let main = keys.get("Main UI");
    let poly = keys.get("Editable Poly");
    let ffd = keys.get("FFD");
    let unwrap = keys.get("Unwrap UVW");
    let skin = keys.get("Skin");
    let track_view = keys.get("Track View");
    let edit_normals = keys.get("Edit Normals");
    crate::registry()
        .iter()
        .map(|info| ChartRow {
            group: match info.group {
                crate::Group::MainUI => "Main UI",
                crate::Group::EditablePoly => "Editable Poly",
                crate::Group::FFD => "FFD",
                crate::Group::UnwrapUVW => "Unwrap UVW",
                crate::Group::Skin => "Skin",
                crate::Group::TrackView => "Track View",
                crate::Group::EditNormals => "Edit Normals",
                crate::Group::Loom => "Loom",
            },
            action: info.name,
            shortcut: {
                let km = match info.group {
                    crate::Group::EditablePoly => poly,
                    crate::Group::FFD => ffd,
                    crate::Group::UnwrapUVW => unwrap,
                    crate::Group::Skin => skin,
                    crate::Group::TrackView => track_view,
                    crate::Group::EditNormals => edit_normals,
                    _ => main,
                };
                let mut chords: Vec<String> = km.bindings().filter(|(_, n)| *n == info.name).map(|(c, _)| c.label()).collect();
                chords.sort();
                chords.join(", ")
            },
        })
        .collect()
}

/// Customize User Interface > Write Keyboard Chart: tab-separated rows.
pub fn write_keyboard_chart(path: &std::path::Path) -> std::io::Result<()> {
    write_keyboard_chart_with(&CustomKeys::default(), path)
}

/// Write Keyboard Chart with the user's shortcut edits.
pub fn write_keyboard_chart_with(keys: &CustomKeys, path: &std::path::Path) -> std::io::Result<()> {
    let mut text = String::from("Group\tAction\tShortcut\n");
    for r in keyboard_chart_with(keys) {
        text.push_str(&format!("{}\t{}\t{}\n", r.group, r.action, r.shortcut));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, text)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Keymap {
    map: HashMap<Chord, &'static str>,
}

impl Keymap {
    /// The "Main UI" group: every chart row whose action is
    /// implemented. Out-of-scope and deferred actions are left unbound.
    pub fn main_ui() -> Keymap {
        let skip = |a: &str| {
            OUT_OF_SCOPE.iter().chain(DEFERRED).any(|(n, _)| *n == a)
        };
        let mut map = HashMap::new();
        for (action, chords) in chart_rows() {
            if skip(action) {
                continue;
            }
            for c in chords {
                let chord = parse_chord(c).expect("chart chords are tested to parse");
                map.insert(chord, action);
            }
        }
        for (c, action) in EXTRA_2010 {
            map.insert(parse_chord(c).expect("static"), *action);
        }
        Keymap { map }
    }

    /// The "Editable Poly" group: every implemented chart row of
    /// that table. Active while an Editable Poly is selected.
    pub fn editable_poly() -> Keymap {
        let skip = |a: &str| crate::command::DEFERRED_POLY.iter().any(|(n, _)| *n == a);
        let mut map = HashMap::new();
        for (action, chord) in poly_chart_rows() {
            if chord.is_empty() || skip(action) {
                continue;
            }
            let chord = parse_chord(chord).expect("poly chart chords are tested to parse");
            map.insert(chord, action);
        }
        for (c, action) in POLY_EXTRA {
            map.insert(parse_chord(c).expect("static"), *action);
        }
        Keymap { map }
    }

    /// The "Edit Poly" group: the chart's Edit Poly table (it binds
    /// Break to Shift+B where the Editable Poly table leaves it free).
    /// Active while an Edit Poly entry is the active stack entry.
    pub fn edit_poly() -> Keymap {
        let skip = |a: &str| crate::command::DEFERRED_POLY.iter().any(|(n, _)| *n == a);
        let mut map = HashMap::new();
        for (action, chord) in edit_poly_chart_rows() {
            if chord.is_empty() || skip(action) {
                continue;
            }
            map.insert(parse_chord(chord).expect("Edit Poly chart chords are tested to parse"), action);
        }
        for (c, action) in POLY_EXTRA {
            map.insert(parse_chord(c).expect("static"), *action);
        }
        Keymap { map }
    }

    /// The "Editable Spline" rows (the Main UI "(Spline)" actions and
    /// the Edit Spline table): the chart binds only Edit Soft Selection (7),
    /// so the group is live with a spline selected but mostly empty.
    pub fn editable_spline() -> Keymap {
        let skip = |a: &str| crate::command::DEFERRED_SPLINE.iter().any(|(n, _)| *n == a);
        let mut map = HashMap::new();
        for (action, chord) in spline_chart_rows() {
            if chord.is_empty() || skip(action) {
                continue;
            }
            map.insert(parse_chord(chord).expect("Editable Spline chart chords are tested to parse"), action);
        }
        Keymap { map }
    }

    /// The "Unwrap UVW" group: live while the Edit UVWs window is open.
    pub fn unwrap_uvw() -> Keymap {
        let skip = |a: &str| crate::command::DEFERRED_UNWRAP.iter().any(|(n, _)| *n == a);
        let mut map = HashMap::new();
        for (action, chord) in unwrap_chart_rows() {
            if chord.is_empty() || skip(action) {
                continue;
            }
            for c in chord.split(", ") {
                map.insert(parse_chord(c).expect("Unwrap UVW chart chords are tested to parse"), action);
            }
        }
        Keymap { map }
    }

    /// The "Track View" group: live while a Track View window has focus.
    /// Delete removes the selected keys there.
    pub fn track_view() -> Keymap {
        let skip = |a: &str| crate::command::DEFERRED_TRACK_VIEW.iter().any(|(n, _)| *n == a);
        let mut map = HashMap::new();
        for (action, chord) in track_view_chart_rows() {
            if chord.is_empty() || skip(action) {
                continue;
            }
            for c in chord.split(", ") {
                map.insert(parse_chord(c).expect("Track View chart chords are tested to parse"), action);
            }
        }
        map.insert(parse_chord("Delete").expect("static"), "Track View Delete Keys");
        Keymap { map }
    }

    /// The "Edit Normals" group: live at an Edit Normals entry's Normal level.
    pub fn edit_normals() -> Keymap {
        let mut map = HashMap::new();
        for (action, chord) in edit_normals_chart_rows() {
            for c in chord.split(", ").filter(|c| !c.is_empty()) {
                map.insert(parse_chord(c).expect("Edit Normals chart chords are tested to parse"), action);
            }
        }
        Keymap { map }
    }

    /// The "Skin" group: the chart's Skin table (no default chords).
    /// Active while a Skin entry is at its Envelope level.
    pub fn skin() -> Keymap {
        let mut map = HashMap::new();
        for (action, chord) in skin_chart_rows() {
            if chord.is_empty() {
                continue;
            }
            for c in chord.split(", ") {
                map.insert(parse_chord(c).expect("Skin chart chords are tested to parse"), action);
            }
        }
        Keymap { map }
    }

    /// The "FFD" group: the chart's FFD table. Active while an FFD
    /// entry is the active stack entry of a selected object.
    pub fn ffd() -> Keymap {
        let map = ffd_chart_rows().into_iter().map(|(action, chord)| (parse_chord(chord).expect("FFD chart chords are tested to parse"), action)).collect();
        Keymap { map }
    }

    pub fn lookup(&self, chord: Chord) -> Option<&'static str> {
        self.map.get(&chord).copied()
    }

    pub fn bindings(&self) -> impl Iterator<Item = (Chord, &'static str)> + '_ {
        self.map.iter().map(|(c, n)| (*c, *n))
    }

    /// First chord bound to an action, for menu labels ("Undo  Ctrl+Z").
    /// Plain keys come before modified ones; ties go by label, so the pick is stable.
    pub fn chord_for(&self, action: &str) -> Option<Chord> {
        let mut found: Vec<Chord> = self.map.iter().filter(|(_, n)| **n == action).map(|(c, _)| *c).collect();
        found.sort_by_key(|c| (c.ctrl, c.alt, c.shift, c.label()));
        found.first().copied()
    }
}

/// Each key map built once and shared (keyboard routing and tooltips).
pub mod cached {
    use super::Keymap;
    use std::sync::OnceLock;
    macro_rules! cached {
        ($($name:ident),*) => { $(pub fn $name() -> &'static Keymap {
            static CELL: OnceLock<Keymap> = OnceLock::new();
            CELL.get_or_init(Keymap::$name)
        })* };
    }
    cached!(main_ui, editable_poly, edit_poly, editable_spline, unwrap_uvw, track_view, edit_normals, skin, ffd);
}

/// The shortcut groups of Customize User Interface > Keyboard, in list order.
pub const GROUPS: [&str; 9] = ["Main UI", "Editable Poly", "Edit Poly", "Editable Spline", "FFD", "Unwrap UVW", "Skin", "Track View", "Edit Normals"];

/// A group's default shortcuts.
pub fn default_keymap(group: &str) -> Option<&'static Keymap> {
    Some(match group {
        "Main UI" => cached::main_ui(),
        "Editable Poly" => cached::editable_poly(),
        "Edit Poly" => cached::edit_poly(),
        "Editable Spline" => cached::editable_spline(),
        "FFD" => cached::ffd(),
        "Unwrap UVW" => cached::unwrap_uvw(),
        "Skin" => cached::skin(),
        "Track View" => cached::track_view(),
        "Edit Normals" => cached::edit_normals(),
        _ => return None,
    })
}

/// The actions a group's Action list shows (and the ones it can bind).
/// Main UI lists the API-only Loom commands too, as their keys go there.
pub fn group_actions(group: &str) -> Vec<&'static str> {
    use crate::Group as G;
    let by_group = |g: &[G]| crate::registry().iter().filter(|i| g.contains(&i.group)).map(|i| i.name).collect();
    match group {
        "Main UI" => by_group(&[G::MainUI, G::Loom]),
        "Editable Poly" | "Edit Poly" => by_group(&[G::EditablePoly]),
        "Editable Spline" => {
            let mut v: Vec<&'static str> = Vec::new();
            for (a, _) in spline_chart_rows() {
                if let Some(name) = static_action(a) {
                    if !v.contains(&name) {
                        v.push(name);
                    }
                }
            }
            v
        }
        "FFD" => by_group(&[G::FFD]),
        "Unwrap UVW" => by_group(&[G::UnwrapUVW]),
        "Skin" => by_group(&[G::Skin]),
        "Track View" => by_group(&[G::TrackView]),
        "Edit Normals" => by_group(&[G::EditNormals]),
        _ => Vec::new(),
    }
}

/// The registry's own copy of an action name.
pub fn static_action(name: &str) -> Option<&'static str> {
    crate::registry().iter().find(|i| i.name == name).map(|i| i.name)
}

/// Customize User Interface > Keyboard edits: the groups whose shortcuts
/// differ from the defaults, each as its whole key map.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CustomKeys {
    edited: std::collections::BTreeMap<String, Keymap>,
}

/// One group's shortcuts as text: (chord in chart spelling, action), sorted.
pub type GroupRows = Vec<(String, String)>;

impl CustomKeys {
    /// The live shortcuts of `group` (defaults unless edited).
    pub fn get(&self, group: &str) -> &Keymap {
        self.edited.get(group).or_else(|| default_keymap(group)).unwrap_or_else(|| cached::main_ui())
    }

    /// `action` as a static name, if `group` lists it or binds it by default.
    fn check(group: &str, action: &str) -> Result<&'static str, String> {
        let default = default_keymap(group).ok_or_else(|| format!("unknown shortcut group {group:?}"))?;
        group_actions(group)
            .into_iter()
            .chain(default.bindings().map(|(_, a)| a))
            .find(|a| *a == action)
            .ok_or_else(|| format!("{action:?} is not an action of the {group} group"))
    }

    fn edit(&mut self, group: &str, f: impl FnOnce(&mut Keymap)) {
        let default = default_keymap(group).expect("checked group");
        let map = self.edited.entry(group.to_owned()).or_insert_with(|| default.clone());
        f(map);
        if map == default {
            self.edited.remove(group);
        }
    }

    /// Binds `chord` to `action` in `group`; returns the action that had it.
    pub fn assign(&mut self, group: &str, action: &str, chord: Chord) -> Result<Option<&'static str>, String> {
        let action = Self::check(group, action)?;
        let previous = self.get(group).lookup(chord).filter(|a| *a != action);
        self.edit(group, |m| {
            m.map.insert(chord, action);
        });
        Ok(previous)
    }

    /// Unbinds every chord of `action` in `group`; returns how many.
    pub fn remove(&mut self, group: &str, action: &str) -> Result<usize, String> {
        let action = Self::check(group, action)?;
        let mut n = 0;
        self.edit(group, |m| {
            let before = m.map.len();
            m.map.retain(|_, a| *a != action);
            n = before - m.map.len();
        });
        Ok(n)
    }

    pub fn reset(&mut self) {
        self.edited.clear();
    }

    /// The edited groups as text (what the settings file stores).
    pub fn edited_rows(&self) -> std::collections::BTreeMap<String, GroupRows> {
        self.edited.iter().map(|(g, m)| (g.clone(), rows_of(m))).collect()
    }

    /// Every group as text (what a shortcut file stores).
    pub fn all_rows(&self) -> std::collections::BTreeMap<String, GroupRows> {
        GROUPS.iter().map(|g| (g.to_string(), rows_of(self.get(g)))).collect()
    }

    /// Replaces the groups in `rows` with their listed shortcuts. An unknown
    /// group, action or chord is an error and changes nothing.
    pub fn set_rows(&mut self, rows: &std::collections::BTreeMap<String, GroupRows>) -> Result<(), String> {
        let mut parsed = Vec::new();
        for (group, list) in rows {
            let mut map = HashMap::new();
            default_keymap(group).ok_or_else(|| format!("unknown shortcut group {group:?}"))?;
            for (chord, action) in list {
                let action = static_action(action).ok_or_else(|| format!("unknown action {action:?}"))?;
                map.insert(parse_chord(chord)?, action);
            }
            parsed.push((group.clone(), Keymap { map }));
        }
        for (group, map) in parsed {
            self.edited.remove(&group);
            if default_keymap(&group) != Some(&map) {
                self.edited.insert(group, map);
            }
        }
        Ok(())
    }
}

fn rows_of(m: &Keymap) -> GroupRows {
    let mut rows: GroupRows = m.bindings().map(|(c, a)| (c.chart_text(), a.to_owned())).collect();
    rows.sort();
    rows
}

/// Shortcut files (Customize User Interface > Keyboard > Save / Load):
/// 3dLoom's own JSON, every group with its shortcuts.
#[derive(serde::Serialize, serde::Deserialize)]
struct ShortcutFile {
    format: String,
    groups: std::collections::BTreeMap<String, GroupRows>,
}

const SHORTCUT_FORMAT: &str = "3dloom-shortcuts-1";

pub fn shortcuts_to_file_text(keys: &CustomKeys) -> String {
    serde_json::to_string_pretty(&ShortcutFile { format: SHORTCUT_FORMAT.into(), groups: keys.all_rows() }).expect("plain data")
}

pub fn shortcuts_from_file_text(text: &str) -> Result<std::collections::BTreeMap<String, GroupRows>, String> {
    let f: ShortcutFile = serde_json::from_str(text).map_err(|e| format!("not a 3dLoom shortcut file: {e}"))?;
    if f.format != SHORTCUT_FORMAT {
        return Err(format!("unknown shortcut file format {:?}", f.format));
    }
    Ok(f.groups)
}

impl Chord {
    /// The chord as the chart spells it (`Alt+Ctrl+Z`, `NumPad +`, `Shift+,`): what `parse_chord` reads.
    pub fn chart_text(&self) -> String {
        let mut s = String::new();
        if self.alt { s.push_str("Alt+"); }
        if self.ctrl { s.push_str("Ctrl+"); }
        if self.shift { s.push_str("Shift+"); }
        s.push_str(KEY_NAMES.iter().find(|(k, _)| *k == self.key).map(|(_, n)| *n).unwrap_or("?"));
        s
    }

    /// Chord label, e.g. `Alt+Ctrl+Z`.
    pub fn label(&self) -> String {
        let mut s = String::new();
        if self.alt { s.push_str("Alt+"); }
        if self.ctrl { s.push_str("Ctrl+"); }
        if self.shift { s.push_str("Shift+"); }
        let k = match self.key {
            Key::UpArrow => "Up Arrow".to_owned(),
            Key::DownArrow => "Down Arrow".to_owned(),
            Key::LeftArrow => "Left Arrow".to_owned(),
            Key::RightArrow => "Right Arrow".to_owned(),
            k => format!("{k:?}").replace("Num", "").replace("PadAdd", "NumPad +"),
        };
        s.push_str(&k);
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{registry, DEFERRED, OUT_OF_SCOPE};
    use std::collections::{HashMap, HashSet};

    #[test]
    fn parses_every_chord_in_chart() {
        for (action, chords) in chart_rows() {
            for c in chords {
                parse_chord(c).unwrap_or_else(|e| panic!("{action}: {c}: {e}"));
            }
        }
        assert_eq!(parse_chord("Alt+Ctrl+Z").unwrap(), Chord { key: Key::Z, ctrl: true, alt: true, shift: false });
        assert_eq!(parse_chord("NumPad +").unwrap().key, Key::NumPadAdd);
        assert_eq!(parse_chord("Ctrl+`").unwrap().key, Key::Backtick);
        assert_eq!(parse_chord(",").unwrap().key, Key::Comma);
    }

    #[test]
    fn every_chart_action_is_registered_or_out_of_scope() {
        let registered: HashSet<&str> = registry().iter().map(|i| i.name).collect();
        let excluded: HashSet<&str> = OUT_OF_SCOPE.iter().chain(DEFERRED).map(|(n, _)| *n).collect();
        let missing: Vec<&str> = chart_rows()
            .iter()
            .map(|(a, _)| *a)
            .filter(|a| !registered.contains(a) && !excluded.contains(a))
            .collect();
        assert!(missing.is_empty(), "unclassified actions: {missing:?}");
        for (name, section) in OUT_OF_SCOPE {
            assert!(!section.is_empty(), "{name} lacks a spec section");
        }
    }

    #[test]
    fn every_editable_poly_chart_action_is_registered_or_deferred() {
        let registered: HashSet<&str> = registry().iter().map(|i| i.name).collect();
        let deferred: HashSet<&str> = crate::command::DEFERRED_POLY.iter().map(|(n, _)| *n).collect();
        let missing: Vec<&str> = poly_chart_rows().iter().map(|(a, _)| *a).filter(|a| !registered.contains(a) && !deferred.contains(a)).collect();
        assert!(missing.is_empty(), "unclassified Editable Poly actions: {missing:?}");
        for (_, chord) in poly_chart_rows() {
            if !chord.is_empty() {
                parse_chord(chord).unwrap_or_else(|e| panic!("{chord}: {e}"));
            }
        }
        for (name, _) in crate::command::DEFERRED_POLY {
            assert!(!registered.contains(name), "{name} is registered but still listed as deferred");
        }
        let km = Keymap::editable_poly();
        for (_, name) in km.bindings() {
            assert!(registered.contains(name), "{name} bound but not registered");
        }
    }

    #[test]
    fn every_edit_poly_chart_action_is_registered_or_deferred() {
        let registered: HashSet<&str> = registry().iter().map(|i| i.name).collect();
        let deferred: HashSet<&str> = crate::command::DEFERRED_POLY.iter().map(|(n, _)| *n).collect();
        for (action, chord) in edit_poly_chart_rows() {
            assert!(registered.contains(action) || deferred.contains(action), "unclassified Edit Poly action {action}");
            if !chord.is_empty() {
                parse_chord(chord).unwrap_or_else(|e| panic!("{chord}: {e}"));
            }
        }
        let km = Keymap::edit_poly();
        assert_eq!(km.lookup(parse_chord("Shift+B").unwrap()), Some("Break"));
        assert_eq!(km.lookup(parse_chord("4").unwrap()), Some("Face Level"));
        for (_, name) in Keymap::editable_spline().bindings() {
            assert!(registered.contains(name), "{name} bound but not registered");
        }
    }

    #[test]
    fn every_spline_chart_action_is_registered_or_deferred() {
        let registered: HashSet<&str> = registry().iter().map(|i| i.name).collect();
        let deferred: HashSet<&str> = crate::command::DEFERRED_SPLINE.iter().map(|(n, _)| *n).collect();
        for (action, chord) in spline_chart_rows() {
            if !chord.is_empty() {
                parse_chord(chord).unwrap_or_else(|e| panic!("{chord}: {e}"));
            }
            assert!(registered.contains(action) || deferred.contains(action), "unclassified Editable Spline action {action}");
        }
        for (name, _) in crate::command::DEFERRED_SPLINE {
            assert!(!registered.contains(name), "{name} is registered but still listed as deferred");
        }
    }

    #[test]
    fn every_unwrap_chart_action_is_registered_or_deferred() {
        let registered: HashSet<&str> = registry().iter().map(|i| i.name).collect();
        let deferred: HashSet<&str> = crate::command::DEFERRED_UNWRAP.iter().map(|(n, _)| *n).collect();
        assert!(unwrap_chart_rows().len() > 190);
        for (action, chord) in unwrap_chart_rows() {
            for c in chord.split(", ").filter(|c| !c.is_empty()) {
                parse_chord(c).unwrap_or_else(|e| panic!("{c}: {e}"));
            }
            assert!(registered.contains(action) || deferred.contains(action), "unclassified Unwrap UVW action {action}");
        }
        for (name, _) in crate::command::DEFERRED_UNWRAP {
            assert!(!registered.contains(name), "{name} is registered but still listed as deferred");
        }
        let km = Keymap::unwrap_uvw();
        assert_eq!(km.lookup(parse_chord("Ctrl+B").unwrap()), Some("Break Selected Vertices"));
        assert_eq!(km.lookup(parse_chord("Enter").unwrap()), Some("Planar map faces/patches"));
        assert_eq!(km.lookup(parse_chord("Alt+Shift+Ctrl+L").unwrap()), Some("Load UVW"));
    }

    #[test]
    fn every_ffd_chart_action_is_registered() {
        let registered: HashSet<&str> = registry().iter().map(|i| i.name).collect();
        assert_eq!(ffd_chart_rows().len(), 4);
        for (action, chord) in ffd_chart_rows() {
            parse_chord(chord).unwrap_or_else(|e| panic!("{chord}: {e}"));
            assert!(registered.contains(action), "FFD action {action} not registered");
        }
    }

    #[test]
    fn no_duplicate_chords_in_main_ui() {
        let mut seen: HashMap<Chord, &str> = HashMap::new();
        for (action, chords) in chart_rows() {
            if OUT_OF_SCOPE.iter().any(|(n, _)| *n == action) {
                continue;
            }
            for c in chords {
                let chord = parse_chord(c).unwrap();
                if let Some(prev) = seen.insert(chord, action) {
                    panic!("{c} bound to both {prev} and {action}");
                }
            }
        }
    }

    /// Space locks the selection only when Preferences > General > "Space
    /// Locks Selection" is on (off by default): pressed by accident it left
    /// the viewports unable to pick anything.
    #[test]
    fn space_locks_the_selection_only_when_enabled() {
        let mut s = crate::AppState::new();
        let space = parse_chord("Space").unwrap();
        assert!(!s.prefs.space_selection_lock);
        assert_eq!(s.hotkey(space), None);
        crate::execute(&mut s, crate::Command::SetPreference { name: "Space Locks Selection".into(), value: 1.0 }).unwrap();
        assert_eq!(s.hotkey(space), Some("Selection Lock Toggle"));
    }

    #[test]
    fn w_is_select_and_move_and_ctrl_z_is_undo() {
        let km = Keymap::main_ui();
        assert_eq!(km.lookup(parse_chord("W").unwrap()), Some("Select and Move"));
        assert_eq!(km.lookup(parse_chord("Ctrl+Z").unwrap()), Some("Undo Scene Operation"));
        assert_eq!(km.lookup(parse_chord("Q").unwrap()), Some("Select Object"));
        assert_eq!(km.lookup(parse_chord("R").unwrap()), Some("Select and Scale"));
        for (name, _) in DEFERRED {
            assert!(!km.bindings().any(|(_, n)| n == *name), "deferred action {name} must not be bound");
        }
        assert_eq!(km.lookup(parse_chord("F9").unwrap()), None, "out-of-scope actions are not bound");
        assert_eq!(km.lookup(parse_chord("Shift+Q").unwrap()), Some("Render"));
        assert_eq!(km.lookup(parse_chord("F10").unwrap()), Some("Render SetUp..."));
    }

    #[test]
    fn alt_r_rings_and_alt_l_loops_in_both_poly_groups() {
        for km in [Keymap::editable_poly(), Keymap::edit_poly()] {
            assert_eq!(km.lookup(parse_chord("Alt+R").unwrap()), Some("Select Edge Ring"));
            assert_eq!(km.lookup(parse_chord("Alt+L").unwrap()), Some("Select Edge Loop"));
        }
    }

    #[test]
    fn main_ui_bindings_only_reference_registered_commands() {
        let registered: HashSet<&str> = registry().iter().map(|i| i.name).collect();
        for (_, name) in Keymap::main_ui().bindings() {
            assert!(registered.contains(name), "{name} bound but not registered");
        }
    }
}
