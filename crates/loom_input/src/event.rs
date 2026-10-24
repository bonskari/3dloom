use glam::Vec2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub alt: bool,
    pub ctrl: bool,
    pub shift: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum InputEvent {
    MouseDown {
        button: MouseButton,
        pos: Vec2,
        mods: Modifiers,
    },
    MouseUp {
        button: MouseButton,
        pos: Vec2,
        mods: Modifiers,
    },
    MouseMove {
        pos: Vec2,
        mods: Modifiers,
    },
    Scroll {
        delta_y: f32,
        pos: Vec2,
        mods: Modifiers,
    },
}
