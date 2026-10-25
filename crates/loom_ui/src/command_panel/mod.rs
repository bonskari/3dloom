//! Command Panel (right side): Scene (the object hierarchy) and the six tabs.

pub mod biped;
pub mod compound;
pub mod create;
pub mod display;
pub mod hierarchy;
pub mod light_rollouts;
pub mod modify;
pub mod modify_mesh;
pub mod modify_patch;
pub mod modify_poly;
pub mod modify_spline;
pub mod motion;
pub mod scene;
pub mod skin_rollouts;
pub mod morph_rollouts;
mod normals_rollout;
pub mod utilities;
pub mod rollouts;

use crate::LoomApp;
use eframe::egui;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    Scene,
    #[default]
    Create,
    Modify,
    Hierarchy,
    Motion,
    Display,
    Utilities,
}

impl Tab {
    pub const ALL: [Tab; 7] = [Tab::Scene, Tab::Create, Tab::Modify, Tab::Hierarchy, Tab::Motion, Tab::Display, Tab::Utilities];
    pub fn label(self) -> &'static str {
        match self {
            Tab::Scene => "Scene",
            Tab::Create => "Create",
            Tab::Modify => "Modify",
            Tab::Hierarchy => "Hierarchy",
            Tab::Motion => "Motion",
            Tab::Display => "Display",
            Tab::Utilities => "Utilities",
        }
    }
    /// The tab's icon; its label is the tooltip.
    pub fn icon(self) -> &'static str {
        use crate::icons as i;
        match self {
            Tab::Scene => i::TAB_SCENE,
            Tab::Create => i::TAB_CREATE,
            Tab::Modify => i::TAB_MODIFY,
            Tab::Hierarchy => i::TAB_HIERARCHY,
            Tab::Motion => i::TAB_MOTION,
            Tab::Display => i::TAB_DISPLAY,
            Tab::Utilities => i::TAB_UTILITIES,
        }
    }
}

pub use rollouts::rollout;

pub fn show(ctx: &egui::Context, app: &mut LoomApp) {
    if app.state.expert_mode || !app.state.show_command_panel {
        return;
    }
    if app.state.focus_modify {
        app.panel_tab = Tab::Modify;
        app.state.focus_modify = false;
    }
    let body = |ui: &mut egui::Ui, app: &mut LoomApp| {
        // The panel's empty area (above the tabs): the Customize Display menu (dock left / right, float).
        let bg = ui.interact(ui.max_rect(), ui.id().with("command_panel_bg"), egui::Sense::click());
        bg.context_menu(|ui| crate::toolbars::customize_display_menu(ui, app, "Command Panel"));
        ui.horizontal_wrapped(|ui| {
            for tab in Tab::ALL {
                if crate::icons::Icon::new(tab.icon(), tab.label()).on(app.panel_tab == tab).show(ui, &app.state).clicked() {
                    app.panel_tab = tab;
                }
            }
        });
        ui.separator();
        let panel = app.panel_tab.label();
        rollouts::begin(ui.ctx(), panel, app.state.ui_custom.rollout_order.get(panel).cloned());
        egui::ScrollArea::vertical().show(ui, |ui| {
            // The empty area between and under the rollouts: the rollout menu.
            let bg = ui.interact(ui.max_rect().intersect(ui.clip_rect()), ui.id().with("rollout_area_bg"), egui::Sense::click());
            bg.context_menu(rollouts::menu);
            match app.panel_tab {
                Tab::Scene => scene::show(ui, app),
                Tab::Create => create::show(ui, app),
                Tab::Modify => modify::show(ui, app),
                // The selected object's name and colour head these panels too.
                Tab::Hierarchy => {
                    create::name_and_color(ui, app);
                    hierarchy::show(ui, app)
                }
                Tab::Motion => {
                    create::name_and_color(ui, app);
                    motion::show(ui, app)
                }
                Tab::Display => {
                    create::name_and_color(ui, app);
                    display::show(ui, app)
                }
                Tab::Utilities => utilities::show(ui, app),
            }
        });
        match rollouts::end(ui.ctx()) {
            Some(rollouts::Request::Order(order)) => {
                let _ = app.dispatch(loom_commands::Command::SetRolloutOrder { panel: panel.to_owned(), order });
            }
            Some(rollouts::Request::Reset) => {
                let _ = app.dispatch(loom_commands::Command::SetRolloutOrder { panel: panel.to_owned(), order: Vec::new() });
            }
            None => {}
        }
    };
    use loom_commands::ui_custom::Dock;
    match app.state.ui_custom.dock("Command Panel") {
        Dock::Float => {
            let mut open = true;
            let pos = app.state.ui_custom.float_pos("Command Panel");
            let r = egui::Window::new("Command Panel")
                .id(crate::toolbars::window_id("Command Panel"))
                .default_pos(egui::pos2(pos[0], pos[1]))
                .default_size(egui::vec2(270.0, 560.0))
                .collapsible(false)
                .movable(!app.state.ui_locked)
                .open(&mut open)
                .show(ctx, |ui| body(ui, app));
            if let Some(r) = r {
                let min = r.response.rect.min;
                let moved = (pos[0] - min.x).abs() > 1.0 || (pos[1] - min.y).abs() > 1.0;
                if moved && !ctx.input(|i| i.pointer.any_down()) {
                    let _ = app.dispatch(loom_commands::Command::DockToolbar { name: "Command Panel".into(), dock: "Float".into(), pos: Some([min.x, min.y]) });
                }
            }
            if !open {
                let _ = app.dispatch(loom_commands::Command::ShowToolbar { name: "Command Panel".into(), on: Some(false) });
            }
        }
        dock => {
            let panel = if dock == Dock::Left { egui::SidePanel::left("command_panel") } else { egui::SidePanel::right("command_panel") };
            panel.default_width(270.0).resizable(!app.state.ui_locked).show(ctx, |ui| body(ui, app));
        }
    }
}
