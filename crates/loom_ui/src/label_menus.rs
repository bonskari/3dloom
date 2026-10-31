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
        MenuEntry { label: "ViewCube".into(), checked: false, action: Action::Submenu({
            let mut v = vec![check("Show the ViewCube", s.viewcube.show, Command::ViewCubeToggle), sep()];
            v.extend(viewcube_entries(app, index));
            v
        }) },
        sep(),
        MenuEntry {
            label: "Create Preview".into(),
            checked: false,
            action: Action::Submenu(vec![
                item("Create Preview Animation...", Command::CreateAnimatedSequenceFile),
                item("Still Image (Grab Viewport)", Command::GrabViewport),
            ]),
        },
        sep(),
        item("Configure Viewports...", Command::ConfigureViewports { panel: None }),
    ]
}

/// The ViewCube menu (right-click the cube; also [+] > ViewCube).
pub fn viewcube_entries(app: &LoomApp, index: usize) -> Vec<MenuEntry> {
    let vp = &app.state.viewports[index];
    vec![
        item("Home", Command::HomeView),
        check("Orthographic", vp.is_orthographic(), Command::OrthographicUserView),
        check("Perspective", !vp.is_orthographic(), Command::PerspectiveUserView),
        sep(),
        item("Set Current View as Home", Command::SetCurrentViewAsHome),
        item("Set Current View as Front", Command::SetCurrentViewAsFront),
        item("Reset Front", Command::ResetFront),
        sep(),
        item("Configure...", Command::ConfigureViewports { panel: Some("ViewCube".into()) }),
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
    // A camera view: select the camera, or its target.
    if let ViewKind::Camera(id) = vp.kind {
        if let Some(loom_scene::ObjectKind::Camera { target, .. }) = s.scene.node(id).map(|n| &n.object) {
            v.push(sep());
            v.push(item("Select Camera", Command::SelectNodes { ids: vec![id.0], op: loom_commands::SelectOp::Replace }));
            if let Some(t) = target.filter(|t| s.scene.node(*t).is_some()) {
                v.push(item("Select Camera Target", Command::SelectNodes { ids: vec![t.0], op: loom_commands::SelectOp::Replace }));
            }
        }
    }
    v.push(sep());
    v.push(check("Show Safe Frames", vp.safe_frames, Command::ViewportSafeframesToggle));
    v.push(check("Viewport Clipping", vp.clipping, Command::ViewportClipping));
    // The change it would undo, as the reference names it ("Undo View Zoom").
    v.push(item(&format!("Undo View {}", vp.view_undo.undo_label().unwrap_or("Change")), Command::UndoViewportOperation));
    v.push(item(&format!("Redo View {}", vp.view_undo.redo_label().unwrap_or("Change")), Command::RedoViewportOperation));
    v
}

/// Shading viewport label menu: the rendering levels (the less used under
/// Other), Edged Faces, Transparency, Lighting and Shadows, Viewport Background.
pub fn shading_entries(app: &LoomApp, index: usize) -> Vec<MenuEntry> {
    let vp = &app.state.viewports[index];
    let set = |s: Shading| Command::ViewportShading { shading: s, edged_faces: None };
    let level = |s: Shading| check(s.label(), vp.shading == s, set(s));
    let transparency = |t: loom_commands::Transparency| check(t.label(), vp.transparency == t, Command::SetViewportRendering { apply_to: None, shading: None, edged_faces: None, transparency: Some(t.label().into()), disable_textures: None, force_two_sided: None });
    vec![
        level(Shading::SmoothHighlights),
        level(Shading::Wireframe),
        check("Realistic (Bevy)", vp.shading == Shading::Realistic, set(Shading::Realistic)),
        MenuEntry {
            label: "Other".into(),
            checked: false,
            action: Action::Submenu(vec![level(Shading::Smooth), level(Shading::FacetsHighlights), level(Shading::Facets), level(Shading::Flat), level(Shading::HiddenLine), level(Shading::BoundingBox)]),
        },
        sep(),
        check("Edged Faces", vp.edged_faces, Command::ViewEdgedFacesToggle),
        MenuEntry { label: "Transparency".into(), checked: false, action: Action::Submenu(loom_commands::Transparency::ALL.into_iter().map(transparency).collect()) },
        MenuEntry { label: "Lighting and Shadows".into(), checked: false, action: Action::Submenu(lighting_entries(vp)) },
        MenuEntry {
            label: "Viewport Background".into(),
            checked: false,
            action: Action::Submenu(vec![
                check("Show Background", vp.background.is_some() && vp.background_display, Command::SetViewportBackground { path: None, display: Some(!(vp.background.is_some() && vp.background_display)), aspect: None, all_views: None, lock: None, use_environment: None }),
                item("Viewport Background...", Command::ViewportBackground),
                item("Update Background Image", Command::UpdateBackgroundImage),
                item("Reset Background Transform", Command::ResetBackgroundTransform),
            ]),
        },
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
                "Show Safe Frames", "Viewport Clipping", "Undo View Change", "Redo View Change"
            ]
        );
    }

    #[test]
    fn general_menu_toggles_reflect_state() {
        let mut app = LoomApp::headless();
        let g = general_entries(&app, 3);
        assert_eq!(g[0].label, "Maximize Viewport");
        assert!(g.iter().any(|e| e.label == "Show Grids" && e.checked));
        assert!(!g.iter().any(|e| e.label == "Viewport Clipping"), "Viewport Clipping is on the POV menu");
        assert!(pov_entries(&app, 3).iter().any(|e| e.label == "Viewport Clipping" && !e.checked));
        app.state.maximized = true;
        app.state.viewports[3].show_grid = false;
        let g = general_entries(&app, 3);
        assert_eq!(g[0].label, "Minimize Viewport");
        assert!(g.iter().any(|e| e.label == "Show Grids" && !e.checked));
        let shading = labels(&shading_entries(&app, 3));
        assert_eq!(shading, vec!["Smooth + Highlights", "Wireframe", "Realistic (Bevy)", "Other", "", "Edged Faces", "Transparency", "Lighting and Shadows", "Viewport Background"]);
        let other = shading_entries(&app, 3).into_iter().find(|e| e.label == "Other").unwrap();
        let Action::Submenu(o) = other.action else { panic!("a submenu") };
        assert_eq!(labels(&o), ["Smooth", "Facets + Highlights", "Facets", "Flat", "Hidden Line", "Bounding Box"]);
    }
}
