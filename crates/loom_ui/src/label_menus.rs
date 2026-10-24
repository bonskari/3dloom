//! Viewport label menus: General ([+]), Point-Of-View and Shading, with the
//! entries of the menus of the same names.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, Shading, ViewKind};

#[derive(Debug, Clone)]
pub enum Action {
    Command(Command),
    Submenu(Vec<MenuEntry>),
    Separator,
}

#[derive(Debug, Clone)]
pub struct MenuEntry {
    pub label: String,
    pub checked: bool,
    pub action: Action,
}

fn item(label: &str, cmd: Command) -> MenuEntry {
    MenuEntry { label: label.to_owned(), checked: false, action: Action::Command(cmd) }
}

fn check(label: &str, checked: bool, cmd: Command) -> MenuEntry {
    MenuEntry { label: label.to_owned(), checked, action: Action::Command(cmd) }
}

fn sep() -> MenuEntry {
    MenuEntry { label: String::new(), checked: false, action: Action::Separator }
}

/// General viewport label menu ([+]).
pub fn general_entries(app: &LoomApp, index: usize) -> Vec<MenuEntry> {
    let s = &app.state;
    let vp = &s.viewports[index];
    let active: Vec<MenuEntry> = s
        .viewports
        .iter()
        .enumerate()
        .map(|(i, v)| check(v.kind.label(), i == s.active_viewport, Command::ActiveViewport { index: i as u32 }))
        .collect();
    vec![
        item(if s.maximized { "Minimize Viewport" } else { "Maximize Viewport" }, Command::MaximizeViewportToggle),
        MenuEntry { label: "Active Viewport".into(), checked: false, action: Action::Submenu(active) },
        check("Disable Viewport", vp.disabled, Command::DisableViewport),
        sep(),
        check("Show Grids", vp.show_grid, Command::HideGridsToggle),
        sep(),
        check("Viewport Clipping", vp.clipping, Command::ViewportClipping),
        sep(),
        item("Configure Viewports...", Command::ConfigureViewports),
    ]
}

/// Point-Of-View viewport label menu.
pub fn pov_entries(app: &LoomApp, index: usize) -> Vec<MenuEntry> {
    let s = &app.state;
    let vp = &s.viewports[index];
    let idx = index as u32;
    let mut v = Vec::new();
    let cameras: Vec<MenuEntry> = s
        .scene
        .nodes()
        .filter(|n| matches!(n.object, loom_scene::ObjectKind::Camera { .. }))
        .map(|n| check(&n.name, vp.kind == ViewKind::Camera(n.id), Command::ViewportView { index: idx, kind: ViewKind::Camera(n.id) }))
        .collect();
    if !cameras.is_empty() {
        v.push(MenuEntry { label: "Cameras".into(), checked: false, action: Action::Submenu(cameras) });
    }
    // Lights: the spot and directional lights a light view can look through.
    let lights: Vec<MenuEntry> = s
        .scene
        .nodes()
        .filter(|n| matches!(&n.object, loom_scene::ObjectKind::Light(l) if l.light_type.aims()))
        .map(|n| check(&n.name, vp.kind == ViewKind::Camera(n.id), Command::ViewportView { index: idx, kind: ViewKind::Camera(n.id) }))
        .collect();
    if !lights.is_empty() {
        v.push(MenuEntry { label: "Lights".into(), checked: false, action: Action::Submenu(lights) });
    }
    for kind in [
        ViewKind::Perspective,
        ViewKind::Orthographic,
        ViewKind::Front,
        ViewKind::Back,
        ViewKind::Top,
        ViewKind::Bottom,
        ViewKind::Left,
        ViewKind::Right,
    ] {
        v.push(check(kind.label(), vp.kind == kind, Command::ViewportView { index: idx, kind }));
    }
    v.push(sep());
    v.push(check("Show Safe Frames", vp.safe_frames, Command::ViewportSafeframesToggle));
    v.push(item("Undo View Change", Command::UndoViewportOperation));
    v.push(item("Redo View Change", Command::RedoViewportOperation));
    v
}

/// Shading viewport label menu (legacy names).
pub fn shading_entries(app: &LoomApp, index: usize) -> Vec<MenuEntry> {
    let vp = &app.state.viewports[index];
    let set = |s: Shading| Command::ViewportShading { shading: s, edged_faces: None };
    vec![
        check("Smooth + Highlights", vp.shading == Shading::SmoothHighlights, set(Shading::SmoothHighlights)),
        check("Wireframe", vp.shading == Shading::Wireframe, set(Shading::Wireframe)),
        check("Realistic (Bevy)", vp.shading == Shading::Realistic, set(Shading::Realistic)),
        MenuEntry {
            label: "Other".into(),
            checked: false,
            action: Action::Submenu(vec![
                check("Hidden Line", vp.shading == Shading::HiddenLine, set(Shading::HiddenLine)),
                check("Flat", vp.shading == Shading::Flat, set(Shading::Flat)),
            ]),
        },
        sep(),
        check("Edged Faces", vp.edged_faces, Command::ViewEdgedFacesToggle),
        MenuEntry { label: "Lighting and Shadows".into(), checked: false, action: Action::Submenu(lighting_entries(vp)) },
    ]
}

/// Shading > Lighting and Shadows: scene or default lights, and how many
/// default lights. Choosing the current entry keeps it.
fn lighting_entries(vp: &loom_commands::Viewport) -> Vec<MenuEntry> {
    let keep = Command::ViewportDefaultLights { count: vp.default_lights };
    let switch = |to_scene: bool| if vp.scene_lights == to_scene { keep.clone() } else { Command::ViewportLightingToggle };
    vec![
        check("Illuminate with Scene Lights", vp.scene_lights, switch(true)),
        check("Illuminate with Default Lights", !vp.scene_lights, switch(false)),
        sep(),
        check("Default Lights: 1 Light", vp.default_lights < 2, Command::ViewportDefaultLights { count: 1 }),
        check("Default Lights: 2 Lights", vp.default_lights >= 2, Command::ViewportDefaultLights { count: 2 }),
    ]
}

/// Renders a menu model inside an egui popup/menu. Commands run on the
/// viewport `index` (it is activated first so view commands hit it).
pub fn render(ui: &mut egui::Ui, app: &mut LoomApp, index: usize, entries: &[MenuEntry]) {
    for e in entries {
        match &e.action {
            Action::Separator => {
                ui.separator();
            }
            Action::Submenu(children) => {
                ui.menu_button(&e.label, |ui| render(ui, app, index, children));
            }
            Action::Command(cmd) => {
                let label = if e.checked { format!("✔ {}", e.label) } else { format!("    {}", e.label) };
                if crate::shortcuts::menu_button(ui, &app.state, &label, crate::shortcuts::action_of(cmd)).clicked() {
                    if app.state.active_viewport != index {
                        let _ = app.dispatch(Command::ActiveViewport { index: index as u32 });
                    }
                    let _ = app.dispatch(cmd.clone());
                    ui.close_menu();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(entries: &[MenuEntry]) -> Vec<String> {
        entries.iter().map(|e| e.label.clone()).collect()
    }

    #[test]
    fn pov_menu_lists_views_in_order() {
        let app = LoomApp::headless();
        let l = labels(&pov_entries(&app, 3));
        assert_eq!(
            l,
            vec![
                "Perspective", "Orthographic", "Front", "Back", "Top", "Bottom", "Left", "Right", "",
                "Show Safe Frames", "Undo View Change", "Redo View Change"
            ]
        );
    }

    #[test]
    fn general_menu_toggles_reflect_state() {
        let mut app = LoomApp::headless();
        let g = general_entries(&app, 3);
        assert_eq!(g[0].label, "Maximize Viewport");
        assert!(g.iter().any(|e| e.label == "Show Grids" && e.checked));
        assert!(g.iter().any(|e| e.label == "Viewport Clipping" && !e.checked));
        app.state.maximized = true;
        app.state.viewports[3].show_grid = false;
        let g = general_entries(&app, 3);
        assert_eq!(g[0].label, "Minimize Viewport");
        assert!(g.iter().any(|e| e.label == "Show Grids" && !e.checked));
        let shading = labels(&shading_entries(&app, 3));
        assert_eq!(shading, vec!["Smooth + Highlights", "Wireframe", "Realistic (Bevy)", "Other", "", "Edged Faces", "Lighting and Shadows"]);
    }
}
