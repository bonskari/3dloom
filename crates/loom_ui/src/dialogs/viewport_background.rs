//! Viewport Background dialog (Alt+B): the image file (Files... opens the
//! file browser) or Use Environment Background, Aspect Ratio, Lock Zoom/Pan, Display Background, Apply
//! Source and Display to All Views / Active Only, and the Viewport list that
//! picks which viewport the dialog sets up. OK applies, Cancel discards.

use crate::shortcuts::HoverShortcut;
use crate::LoomApp;
use eframe::egui;
use loom_commands::viewport::BackgroundAspect;
use loom_commands::{Command, Dialog};

/// The dialog's fields, for the viewport it was filled from.
#[derive(Clone)]
struct Fields {
    viewport: usize,
    path: String,
    use_environment: bool,
    aspect: BackgroundAspect,
    lock: bool,
    display: bool,
    all_views: bool,
}

fn id() -> egui::Id {
    egui::Id::new("viewport_background_fields")
}

fn fields_of(app: &LoomApp, i: usize) -> Fields {
    let vp = &app.state.viewports[i];
    Fields { viewport: i, path: vp.background.clone().unwrap_or_default(), use_environment: vp.background_use_environment, aspect: vp.background_aspect, lock: vp.background_locked, display: vp.background_display, all_views: false }
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let active = app.state.active_viewport;
    let mut f: Fields = ctx.data(|d| d.get_temp(id())).filter(|f: &Fields| f.viewport == active).unwrap_or_else(|| fields_of(app, active));
    // Files... came back with a file.
    if let Some(p) = app.background_pick.take() {
        f.path = p;
    }
    let mut ok = false;
    let mut cancel = false;
    let mut browse = false;
    let mut switch_to = None;
    egui::Window::new("Viewport Background").open(open).default_width(460.0).show(ctx, |ui| {
        ui.label(egui::RichText::new("Background Source").strong());
        ui.horizontal(|ui| {
            if ui.button("Files...").clicked() {
                browse = true;
            }
            ui.add(egui::TextEdit::singleline(&mut f.path).desired_width(320.0).hint_text("/path/to/image.png"));
        });
        ui.label(format!("Current: {}", if f.path.trim().is_empty() { "None" } else { f.path.trim() }));
        ui.checkbox(&mut f.use_environment, "Use Environment Background").on_hover_text("Show the Environment's background colour or map (Rendering > Environment)");
        ui.separator();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Aspect Ratio").strong());
                for a in BackgroundAspect::ALL {
                    if ui.radio(f.aspect == a, a.label()).clicked() {
                        f.aspect = a;
                    }
                }
            });
            ui.add_space(24.0);
            ui.vertical(|ui| {
                ui.checkbox(&mut f.lock, "Lock Zoom/Pan").on_hover_text(crate::shortcuts::name_tip(&app.state, "Background Lock Toggle"));
                ui.checkbox(&mut f.display, "Display Background");
            });
        });
        ui.separator();
        ui.label(egui::RichText::new("Apply Source and Display to").strong());
        ui.horizontal(|ui| {
            if ui.radio(f.all_views, "All Views").clicked() {
                f.all_views = true;
            }
            if ui.radio(!f.all_views, "Active Only").clicked() {
                f.all_views = false;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Viewport:");
            egui::ComboBox::from_id_salt("viewport_background_viewport").selected_text(app.state.view_label(active)).show_ui(ui, |ui| {
                for i in 0..app.state.viewports.len() {
                    if ui.selectable_label(i == active, app.state.view_label(i)).clicked() && i != active {
                        switch_to = Some(i);
                    }
                }
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                ok = true;
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
            if ui.button("Update").with_shortcut(&app.state, Command::UpdateBackgroundImage.name()).clicked() {
                let _ = app.dispatch(Command::UpdateBackgroundImage);
            }
        });
    });
    if let Some(i) = switch_to {
        // The Viewport list: that viewport becomes active and the dialog shows its settings.
        let _ = app.dispatch(Command::ActiveViewport { index: i as u32 });
        f = fields_of(app, i);
    }
    if browse {
        ctx.data_mut(|d| d.insert_temp(id(), f));
        app.state.dialog = Some(Dialog::BackgroundFile);
        return;
    }
    if ok {
        let path = f.path.trim().to_owned();
        let r = app.dispatch(Command::SetViewportBackground { path: Some(path), use_environment: Some(f.use_environment), aspect: Some(f.aspect.label().into()), lock: Some(f.lock), display: Some(f.display), all_views: Some(f.all_views) });
        if let Err(e) = r {
            app.last_error = Some(e);
        }
        *open = false;
    }
    if cancel {
        *open = false;
    }
    if *open {
        ctx.data_mut(|d| d.insert_temp(id(), f));
    } else {
        ctx.data_mut(|d| d.remove::<Fields>(id()));
    }
}
