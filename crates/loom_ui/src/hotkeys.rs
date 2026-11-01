//! Routes egui key presses through the keymap to the command registry.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Chord, Command, Key};

/// Maps an egui key to the chart key, if the chart knows it.
pub fn chart_key(key: egui::Key) -> Option<Key> {
    use egui::Key as E;
    Some(match key {
        E::A => Key::A, E::B => Key::B, E::C => Key::C, E::D => Key::D, E::E => Key::E,
        E::F => Key::F, E::G => Key::G, E::H => Key::H, E::I => Key::I, E::J => Key::J,
        E::K => Key::K, E::L => Key::L, E::M => Key::M, E::N => Key::N, E::O => Key::O,
        E::P => Key::P, E::Q => Key::Q, E::R => Key::R, E::S => Key::S, E::T => Key::T,
        E::U => Key::U, E::V => Key::V, E::W => Key::W, E::X => Key::X, E::Y => Key::Y,
        E::Z => Key::Z,
        E::Num0 => Key::Num0, E::Num1 => Key::Num1, E::Num2 => Key::Num2, E::Num3 => Key::Num3,
        E::Num4 => Key::Num4, E::Num5 => Key::Num5, E::Num6 => Key::Num6, E::Num7 => Key::Num7,
        E::Num8 => Key::Num8, E::Num9 => Key::Num9,
        E::F1 => Key::F1, E::F2 => Key::F2, E::F3 => Key::F3, E::F4 => Key::F4, E::F5 => Key::F5,
        E::F6 => Key::F6, E::F7 => Key::F7, E::F8 => Key::F8, E::F9 => Key::F9, E::F10 => Key::F10,
        E::F11 => Key::F11, E::F12 => Key::F12,
        E::Space => Key::Space, E::Enter => Key::Enter, E::Insert => Key::Insert, E::Delete => Key::Delete,
        E::Home => Key::Home, E::End => Key::End, E::PageUp => Key::PageUp, E::PageDown => Key::PageDown,
        E::Backspace => Key::Backspace, E::Semicolon => Key::Semicolon,
        E::Comma => Key::Comma, E::Period => Key::Period, E::Slash => Key::Slash,
        E::Backslash => Key::Backslash, E::OpenBracket => Key::OpenBracket,
        E::CloseBracket => Key::CloseBracket, E::Backtick => Key::Backtick, E::Quote => Key::Quote,
        E::Equals => Key::Equals, E::Minus => Key::Minus, E::Plus => Key::NumPadAdd,
        E::ArrowUp => Key::UpArrow, E::ArrowDown => Key::DownArrow, E::ArrowLeft => Key::LeftArrow, E::ArrowRight => Key::RightArrow,
        _ => return None,
    })
}

pub fn chord_of(key: egui::Key, mods: egui::Modifiers) -> Option<Chord> {
    Some(Chord {
        key: chart_key(key)?,
        ctrl: mods.ctrl || mods.command,
        alt: mods.alt,
        shift: mods.shift,
    })
}

/// Key presses in this frame's events. egui-winit turns Ctrl+C/X/V into
/// Copy/Cut/Paste events and never emits the Key event, so those come
/// back as their chords here.
pub fn pressed_keys(events: &[egui::Event]) -> Vec<(egui::Key, egui::Modifiers)> {
    events
        .iter()
        .filter_map(|e| match e {
            egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. } => {
                Some((*key, *modifiers))
            }
            egui::Event::Copy => Some((egui::Key::C, egui::Modifiers::CTRL)),
            egui::Event::Cut => Some((egui::Key::X, egui::Modifiers::CTRL)),
            egui::Event::Paste(_) => Some((egui::Key::V, egui::Modifiers::CTRL)),
            _ => None,
        })
        .collect()
}

pub fn handle(ctx: &egui::Context, app: &mut LoomApp) {
    if ctx.wants_keyboard_input() {
        return;
    }
    let pressed = ctx.input(|i| pressed_keys(&i.events));
    // Overrides Active: the held mode's key came up after Delay To Override, so the mode ends.
    if let Some((key, name, since, before)) = app.held_override {
        let (released, now) = ctx.input(|i| (i.events.iter().any(|e| matches!(e, egui::Event::Key { key: k, pressed: false, .. } if *k == key)), i.time));
        if released {
            app.held_override = None;
            if app.state.prefs.overrides_active && (now - since) * 1000.0 >= app.state.prefs.override_delay_ms as f64 {
                app.dispatch_named(name);
                // Back to the operation the override interrupted.
                if let Some(m) = before.filter(|_| app.state.poly_drag.is_none()) {
                    app.dispatch_named(drag_mode_name(m));
                }
            }
        }
    }
    for (key, mods) in pressed {
        // Backspace at an edge level removes edges (hard-coded).
        let plain = !(mods.ctrl || mods.alt || mods.shift || mods.command);
        if key == egui::Key::Backspace && plain && app.line.is_some() {
            if let Some(line) = app.line.clone() {
                line.backspace(app);
            }
            continue;
        }
        if key == egui::Key::Backspace && plain && app.state.selection_mode.is_edge_level() {
            let _ = app.dispatch(Command::DissolveEdges);
            continue;
        }
        // Ctrl+Backspace: Clean Remove (the edges and their leftover vertices).
        let ctrl_only = (mods.ctrl || mods.command) && !mods.alt && !mods.shift;
        if key == egui::Key::Backspace && ctrl_only && app.state.selection_mode.is_edge_level() {
            let _ = app.dispatch(Command::CleanRemove);
            continue;
        }
        // Backspace at Vertex level is Remove.
        if key == egui::Key::Backspace && plain && app.state.selection_mode == loom_commands::SelectionMode::Vertex && !app.state.spline_selected() {
            let _ = app.dispatch(Command::Remove);
            continue;
        }
        // Delete over the track bar removes its selected keys.
        if key == egui::Key::Delete && plain && app.track_bar.hovered && !app.track_bar.selected.is_empty() {
            for f in std::mem::take(&mut app.track_bar.selected) {
                let _ = app.dispatch(Command::DeleteKeys { from: f, to: f });
            }
            continue;
        }
        // Walk Through navigation: W/S/A/D/E/C and Arrow keys step the eye in the active viewport;
        // Q / Z speed, [ / ] / Alt+[ step size, Space locks vertical rotation, Shift+Space levels.
        if app.state.nav_mode == loom_input::NavMode::WalkThrough {
            let other = if plain {
                match key {
                    egui::Key::Q => Some(Command::WalkThroughAccelerateToggle),
                    egui::Key::Z => Some(Command::WalkThroughDecelerateToggle),
                    egui::Key::OpenBracket => Some(Command::WalkThroughDecreaseStepSize),
                    egui::Key::CloseBracket => Some(Command::WalkThroughIncreaseStepSize),
                    egui::Key::Space => Some(Command::WalkThroughLockVerticalToggle),
                    _ => None,
                }
            } else if mods.alt && !mods.ctrl && !mods.shift && key == egui::Key::OpenBracket {
                Some(Command::WalkThroughResetStepSize)
            } else if mods.shift && !mods.ctrl && !mods.alt && key == egui::Key::Space {
                Some(Command::WalkThroughLevel)
            } else if mods.shift && !mods.ctrl && !mods.alt && matches!(key, egui::Key::ArrowUp | egui::Key::ArrowDown) {
                // Shift+Up / Down Arrow: Up / Down.
                let k = app.state.walk.factor();
                let up = if key == egui::Key::ArrowUp { k } else { -k };
                Some(Command::WalkThroughStep { forward: 0.0, right: 0.0, up })
            } else {
                None
            };
            if let Some(c) = other {
                let _ = app.dispatch(c);
                continue;
            }
        }
        if plain && app.state.nav_mode == loom_input::NavMode::WalkThrough {
            let step = match key {
                egui::Key::W | egui::Key::ArrowUp => Some((1.0, 0.0, 0.0)),
                egui::Key::S | egui::Key::ArrowDown => Some((-1.0, 0.0, 0.0)),
                egui::Key::D | egui::Key::ArrowRight => Some((0.0, 1.0, 0.0)),
                egui::Key::A | egui::Key::ArrowLeft => Some((0.0, -1.0, 0.0)),
                egui::Key::E | egui::Key::PageUp => Some((0.0, 0.0, 1.0)),
                egui::Key::C | egui::Key::PageDown => Some((0.0, 0.0, -1.0)),
                _ => None,
            };
            if let Some((forward, right, up)) = step {
                let k = app.state.walk.factor();
                let _ = app.dispatch(Command::WalkThroughStep { forward: forward * k, right: right * k, up: up * k });
                continue;
            }
        }
        let Some(chord) = chord_of(key, mods) else { continue };
        // A Quad Shortcut (Customize User Interface > Quads) opens its quad set at the pointer.
        if let Some(set) = app.state.ui_custom.quad_for_shortcut(&chord.chart_text()).map(str::to_owned) {
            app.state.quad_request = Some(set);
            continue;
        }
        // Editable Poly group first while an Editable Poly is selected, then Main UI.
        if let Some(name) = app.state.hotkey(chord) {
            use loom_commands::PendingFileAction as A;
            match name {
                // These discard the scene: ask about unsaved changes first, as the menu does.
                "New Scene" => app.new_scene(),
                "Open File" => app.request_file_action(A::Open),
                "Reset File" => app.request_file_action(A::Reset),
                // I: the point under the cursor becomes the view's centre.
                "Pan Viewport" => {
                    let point = ctx.input(|i| i.pointer.hover_pos()).and_then(|p| crate::viewport_widget::view_point_under(app, p));
                    if let Err(e) = app.dispatch(Command::PanViewport { point: point.map(|p| p.to_array()) }) {
                        app.last_error = Some(e);
                    }
                }
                _ => {
                    let before = app.state.poly_drag;
                    app.dispatch_named(name);
                    // Overrides Active: an Editable Poly mode turned on by this key may be a temporary override.
                    if app.state.prefs.overrides_active && is_override_mode(name) && !ctx.input(|i| i.key_released(key)) {
                        app.held_override = Some((key, name, ctx.input(|i| i.time), before));
                    }
                }
            }
        }
    }
}

/// The Editable Poly modes a held shortcut can override the current
/// operation with.
fn is_override_mode(name: &str) -> bool {
    name.ends_with(" Mode") && name != "Preview Select Mode" && loom_commands::registry().iter().any(|i| i.name == name && i.group == loom_commands::Group::EditablePoly)
}

fn drag_mode_name(m: loom_commands::PolyDrag) -> &'static str {
    use loom_commands::PolyDrag as D;
    match m {
        D::Extrude => "Extrude Mode",
        D::Chamfer => "Chamfer Mode",
        D::Inset => "Inset Mode",
        D::Bevel => "Bevel Mode",
        D::Outline => "Outline Mode",
        D::Hinge => "Hinge from Edge Mode",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_event_becomes_ctrl_v_clone() {
        let keys = pressed_keys(&[egui::Event::Paste(String::new())]);
        let c = chord_of(keys[0].0, keys[0].1).unwrap();
        assert_eq!(loom_commands::Keymap::main_ui().lookup(c), Some("Clone"));
    }

    #[test]
    fn ctrl_z_maps_to_z_with_ctrl() {
        let c = chord_of(egui::Key::Z, egui::Modifiers::CTRL).unwrap();
        assert_eq!(c, Chord { key: Key::Z, ctrl: true, alt: false, shift: false });
        assert_eq!(loom_commands::Keymap::main_ui().lookup(c), Some("Undo Scene Operation"));
    }
}
