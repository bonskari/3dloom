//! Hover text and menu labels that carry the chord of the command they run,
//! looked up in the live key maps so a tooltip can never disagree with the keyboard.

use eframe::egui;
use loom_commands::{AppState, Command, SelectionMode};

/// The chord that runs `action` (a command's display name) here, e.g. "W".
pub fn chord(state: &AppState, action: &str) -> Option<String> {
    state.shortcut_label(action)
}

/// `text` with the chord of `action`: "Select and Move (W)". The chord follows the
/// name, so in "Cut: click two points" it goes before the colon ("Cut (Alt+C): click
/// two points"). Without a chord the text comes back unchanged.
pub fn tip(state: &AppState, text: &str, action: &str) -> String {
    match chord(state, action) {
        Some(c) => match text.split_once(": ") {
            Some((name, rest)) => format!("{name} ({c}): {rest}"),
            None => format!("{text} ({c})"),
        },
        None => text.to_owned(),
    }
}

/// The command a sub-object level button stands for: the level commands carry the chords
/// (Editable Poly 1-6), while the buttons dispatch `SelectionMode`.
pub fn level_action(mode: SelectionMode) -> &'static str {
    match mode {
        SelectionMode::Object => Command::ObjectLevel.name(),
        SelectionMode::Vertex => Command::VertexLevel.name(),
        SelectionMode::Edge => Command::EdgeLevel.name(),
        SelectionMode::Border => Command::BorderLevel.name(),
        SelectionMode::Face => Command::FaceLevel.name(),
        SelectionMode::Element => Command::ElementLevel.name(),
    }
}

/// The action name whose chord a widget running `cmd` shows: the command itself, or
/// the keyboard action that does the same (a level, a view on the active viewport).
pub fn action_of(cmd: &Command) -> &'static str {
    use loom_commands::ViewKind as V;
    match cmd {
        Command::SelectionMode { mode } => level_action(*mode),
        Command::ViewportView { kind, .. } => match kind {
            V::Top => Command::TopView.name(),
            V::Bottom => Command::BottomView.name(),
            V::Front => Command::FrontView.name(),
            V::Back => Command::BackView.name(),
            V::Left => Command::LeftView.name(),
            V::Right => Command::RightView.name(),
            V::Perspective => Command::PerspectiveUserView.name(),
            V::Orthographic => Command::OrthographicUserView.name(),
            V::Camera(_) => cmd.name(),
        },
        c => c.name(),
    }
}

/// `action`'s own name with its chord, for buttons whose tooltip is just the command.
pub fn name_tip(state: &AppState, action: &str) -> String {
    tip(state, action, action)
}

/// A menu row with the chord right-aligned, as desktop menus show it, and in its hover text.
pub fn menu_button(ui: &mut egui::Ui, state: &AppState, label: &str, action: &str) -> egui::Response {
    ui.add(menu_widget(state, label, action)).with_shortcut(state, action)
}

/// The button of [`menu_button`], for callers that add it themselves (enabled state, sizes).
pub fn menu_widget<'a>(state: &AppState, label: &'a str, action: &str) -> egui::Button<'a> {
    let b = egui::Button::new(label);
    match chord(state, action) {
        Some(c) => b.shortcut_text(c),
        None => b,
    }
}

/// `on_hover_text` with the chord of the command a widget runs.
pub trait HoverShortcut {
    /// Hover text `text (chord)`.
    fn on_hover_cmd(self, state: &AppState, text: &str, action: &str) -> Self;
    /// Hover text `action (chord)`.
    fn on_hover_name(self, state: &AppState, action: &str) -> Self;
    /// Hover text `action (chord)` only when `action` has a chord here (widgets without a tooltip of their own).
    fn with_shortcut(self, state: &AppState, action: &str) -> Self;
}

impl HoverShortcut for egui::Response {
    fn on_hover_cmd(self, state: &AppState, text: &str, action: &str) -> Self {
        self.on_hover_text(tip(state, text, action))
    }
    fn on_hover_name(self, state: &AppState, action: &str) -> Self {
        self.on_hover_text(name_tip(state, action))
    }
    fn with_shortcut(self, state: &AppState, action: &str) -> Self {
        match chord(state, action) {
            Some(c) => self.on_hover_text(format!("{action} ({c})")),
            None => self,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_commands::{execute, Command, PrimitiveKind};

    #[test]
    fn main_ui_command_gets_its_chord() {
        let s = AppState::new();
        assert_eq!(tip(&s, "Select and Move", "Select and Move"), "Select and Move (W)");
        assert_eq!(chord(&s, "Undo Scene Operation").as_deref(), Some("Ctrl+Z"));
        assert_eq!(chord(&s, "Select and Scale").as_deref(), Some("R"));
        // The poly group is not live without an Editable Poly selected.
        assert_eq!(chord(&s, "Edge Level"), None);
        let top = Command::ViewportView { index: 0, kind: loom_commands::ViewKind::Top };
        assert_eq!(chord(&s, action_of(&top)).as_deref(), Some("T"));
    }

    #[test]
    fn editable_poly_levels_use_the_poly_group() {
        let mut s = AppState::new();
        execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [1.0; 3] }).unwrap();
        execute(&mut s, Command::ConvertToEditablePoly).unwrap();
        assert_eq!(chord(&s, "Edge Level").as_deref(), Some("2"));
        assert_eq!(tip(&s, "Edge", "Edge Level"), "Edge (2)");
        assert_eq!(tip(&s, "Cut: click the start point", "Cut"), "Cut (Alt+C): click the start point");
        assert_eq!(chord(&s, "Vertex Level").as_deref(), Some("1"));
        assert_eq!(chord(&s, action_of(&Command::SelectionMode { mode: SelectionMode::Face })).as_deref(), Some("4"));
    }

    #[test]
    fn command_without_a_chord_has_no_suffix() {
        let s = AppState::new();
        assert_eq!(chord(&s, "Unlink Selection"), None);
        assert_eq!(tip(&s, "Unlink Selection", "Unlink Selection"), "Unlink Selection");
        assert_eq!(name_tip(&s, "No Such Command"), "No Such Command");
    }
}
