//! Command Panel (right side) with the six tabs.

pub mod compound;
pub mod create;
pub mod display;
pub mod hierarchy;
pub mod modify;
pub mod modify_poly;
pub mod modify_spline;
pub mod motion;
pub mod skin_rollouts;
pub mod morph_rollouts;
mod normals_rollout;
pub mod utilities;

use crate::LoomApp;
use eframe::egui;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Create,
    Modify,
    Hierarchy,
    Motion,
    Display,
    Utilities,
}

impl Tab {
    pub const ALL: [Tab; 6] = [Tab::Create, Tab::Modify, Tab::Hierarchy, Tab::Motion, Tab::Display, Tab::Utilities];
    pub fn label(self) -> &'static str {
        match self {
            Tab::Create => "Create",
            Tab::Modify => "Modify",
            Tab::Hierarchy => "Hierarchy",
            Tab::Motion => "Motion",
            Tab::Display => "Display",
            Tab::Utilities => "Utilities",
        }
    }
}

/// A rollout: a collapsible section with a title bar.
pub fn rollout(ui: &mut egui::Ui, title: &str, default_open: bool, body: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(egui::RichText::new(title).strong())
        .default_open(default_open)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            body(ui);
        });
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp) {
    if app.state.expert_mode {
        return;
    }
    if app.state.focus_modify {
        app.panel_tab = Tab::Modify;
        app.state.focus_modify = false;
    }
    egui::SidePanel::right("command_panel")
        .default_width(270.0)
        .resizable(!app.state.ui_locked)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                for tab in Tab::ALL {
                    if ui.selectable_label(app.panel_tab == tab, tab.label()).clicked() {
                        app.panel_tab = tab;
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| match app.panel_tab {
                Tab::Create => create::show(ui, app),
                Tab::Modify => modify::show(ui, app),
                Tab::Hierarchy => hierarchy::show(ui, app),
                Tab::Motion => motion::show(ui, app),
                Tab::Display => display::show(ui, app),
                Tab::Utilities => utilities::show(ui, app),
            });
        });
}
