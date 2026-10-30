//! The Update menu beside Help: a background check fills it in; a red dot
//! shows when a newer version is out, with the list of its changes. Nothing
//! installs until the user picks it.

use eframe::egui;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq)]
pub enum UpdateStatus {
    Checking,
    UpToDate,
    /// A newer release: its commit and the changes since this version (newest first).
    Available { commit: String, changes: Vec<String> },
    Installing { commit: String },
    /// Installed over this program; it runs after a restart.
    Ready { commit: String },
    Failed(String),
}

/// Shared between the UI and the updater thread.
#[derive(Debug)]
pub struct UpdateState {
    pub branch: String,
    pub branches: Vec<String>,
    pub status: UpdateStatus,
    pub install_requested: bool,
    pub check_requested: bool,
    pub restart_requested: bool,
}

pub type UpdateHandle = Arc<Mutex<UpdateState>>;

pub fn new_handle() -> UpdateHandle {
    new_handle_for_branch("main")
}

pub fn new_handle_for_branch(branch: impl Into<String>) -> UpdateHandle {
    let branch = branch.into();
    let mut branches = vec!["main".to_owned(), "experimental".to_owned()];
    if !branch.is_empty() && !branches.contains(&branch) {
        branches.push(branch.clone());
    }
    Arc::new(Mutex::new(UpdateState {
        branch,
        branches,
        status: UpdateStatus::Checking,
        install_requested: false,
        check_requested: false,
        restart_requested: false,
    }))
}

/// Whether the menu title carries the red dot.
pub fn has_news(status: &UpdateStatus) -> bool {
    matches!(status, UpdateStatus::Available { .. } | UpdateStatus::Ready { .. })
}

fn short(c: &str) -> &str {
    &c[..c.len().min(7)]
}

fn title(news: bool) -> egui::WidgetText {
    let mut job = egui::text::LayoutJob::default();
    let font = egui::FontId::proportional(14.0);
    job.append("Update", 0.0, egui::TextFormat { font_id: font.clone(), color: egui::Color32::from_gray(220), ..Default::default() });
    if news {
        job.append(" •", 0.0, egui::TextFormat { font_id: font, color: egui::Color32::from_rgb(230, 40, 40), ..Default::default() });
    }
    job.into()
}

pub fn show(ui: &mut egui::Ui, handle: &UpdateHandle) {
    let (branch, branches, status) = {
        let s = handle.lock().unwrap();
        (s.branch.clone(), s.branches.clone(), s.status.clone())
    };
    ui.menu_button(title(has_news(&status)), |ui| {
        ui.set_min_width(320.0);
        ui.horizontal(|ui| {
            ui.label("Branch:");
            let mut selected = branch.clone();
            egui::ComboBox::from_id_salt("update_branch_select")
                .selected_text(&selected)
                .show_ui(ui, |ui| {
                    for b in &branches {
                        if ui.selectable_value(&mut selected, b.clone(), b).clicked() && selected != branch {
                            let mut s = handle.lock().unwrap();
                            s.branch = selected.clone();
                            s.status = UpdateStatus::Checking;
                            s.check_requested = true;
                        }
                    }
                });
        });
        ui.separator();
        match &status {
            UpdateStatus::Checking => {
                ui.weak(format!("Checking {branch} for updates…"));
            }
            UpdateStatus::UpToDate => {
                ui.label(format!("3dLoom ({branch}) is up to date."));
                if ui.button("Check again").clicked() {
                    handle.lock().unwrap().check_requested = true;
                    ui.close_menu();
                }
            }
            UpdateStatus::Available { commit, changes } => {
                ui.label(egui::RichText::new(format!("Version {} is available on {branch}", short(commit))).strong());
                ui.separator();
                if changes.is_empty() {
                    ui.weak("The list of changes is not available.");
                } else {
                    ui.label("Changes:");
                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        for c in changes {
                            ui.label(format!("• {c}"));
                        }
                    });
                }
                ui.separator();
                if ui.button(format!("Update to {} ({branch})", short(commit))).clicked() {
                    handle.lock().unwrap().install_requested = true;
                    ui.close_menu();
                }
            }
            UpdateStatus::Installing { commit } => {
                ui.weak(format!("Downloading version {} ({branch})…", short(commit)));
            }
            UpdateStatus::Ready { commit } => {
                ui.label(format!("Version {} ({branch}) is installed.", short(commit)));
                if ui.button("Restart now").on_hover_text("Unsaved changes are asked about first").clicked() {
                    handle.lock().unwrap().restart_requested = true;
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    ui.close_menu();
                }
            }
            UpdateStatus::Failed(e) => {
                ui.label(format!("Update failed: {e}"));
                if ui.button("Try again").clicked() {
                    handle.lock().unwrap().check_requested = true;
                    ui.close_menu();
                }
            }
        }
    });
    // Keep the dot in step with the background thread.
    if matches!(status, UpdateStatus::Checking | UpdateStatus::Installing { .. }) {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dot_shows_only_with_news() {
        assert!(!has_news(&UpdateStatus::Checking));
        assert!(!has_news(&UpdateStatus::UpToDate));
        assert!(has_news(&UpdateStatus::Available { commit: "abc".into(), changes: vec![] }));
        assert!(has_news(&UpdateStatus::Ready { commit: "abc".into() }));
        assert!(!has_news(&UpdateStatus::Failed("x".into())));
    }

    #[test]
    fn new_handle_seeds_main_and_experimental_branch() {
        let h = new_handle_for_branch("main");
        let s = h.lock().unwrap();
        assert_eq!(s.branch, "main");
        assert_eq!(s.branches, vec!["main".to_owned(), "experimental".to_owned()]);
    }
}
