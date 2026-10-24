//! Viewport Configuration dialog: Layout, Safe Frames and Rendering Method
//! (shading only; rendering is out of scope).

use crate::shortcuts::HoverShortcut;
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, Shading, ViewKind, ViewportLayout};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Layout,
    SafeFrames,
    RenderingMethod,
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let tab_id = egui::Id::new("viewport_configuration_tab");
    let mut tab: Tab = ctx.data(|d| d.get_temp(tab_id)).unwrap_or_default();
    egui::Window::new("Viewport Configuration")
        .open(open)
        .resizable(false)
        .default_width(420.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut tab, Tab::RenderingMethod, "Rendering Method");
                ui.selectable_value(&mut tab, Tab::Layout, "Layout");
                ui.selectable_value(&mut tab, Tab::SafeFrames, "Safe Frames");
            });
            ui.separator();
            match tab {
                Tab::Layout => layout_tab(ui, app),
                Tab::SafeFrames => {
                    let mut on = app.state.active().safe_frames;
                    if ui.checkbox(&mut on, "Show Safe Frames in Active View").with_shortcut(&app.state, Command::ViewportSafeframesToggle.name()).changed() {
                        let _ = app.dispatch(Command::ViewportSafeframesToggle);
                    }
                }
                Tab::RenderingMethod => {
                    ui.label("Rendering Level (active viewport)");
                    let cur = app.state.active().shading;
                    for s in [Shading::SmoothHighlights, Shading::Wireframe, Shading::HiddenLine, Shading::Flat] {
                        if ui.radio(cur == s, s.label()).clicked() {
                            let _ = app.dispatch(Command::ViewportShading { shading: s, edged_faces: None });
                        }
                    }
                    let mut edged = app.state.active().edged_faces;
                    if ui.checkbox(&mut edged, "Edged Faces").with_shortcut(&app.state, Command::ViewEdgedFacesToggle.name()).changed() {
                        let _ = app.dispatch(Command::ViewEdgedFacesToggle);
                    }
                }
            }
        });
    ctx.data_mut(|d| d.insert_temp(tab_id, tab));
}

/// Draws a layout as a mini diagram of its cells.
fn layout_icon(ui: &mut egui::Ui, layout: ViewportLayout, selected: bool) -> egui::Response {
    let size = egui::vec2(44.0, 34.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let p = ui.painter();
    let bg = if selected { egui::Color32::from_rgb(90, 120, 180) } else { egui::Color32::from_gray(70) };
    p.rect_filled(rect, 2.0, bg);
    for c in layout.cells(&loom_commands::LayoutSplits::default()) {
        let r = egui::Rect::from_min_max(
            egui::pos2(rect.min.x + c[0] * rect.width(), rect.min.y + c[1] * rect.height()),
            egui::pos2(rect.min.x + c[2] * rect.width(), rect.min.y + c[3] * rect.height()),
        )
        .shrink(1.5);
        p.rect_stroke(r, 0.0, egui::Stroke::new(1.0, egui::Color32::from_gray(220)));
    }
    resp.on_hover_text(layout.label())
}

fn layout_tab(ui: &mut egui::Ui, app: &mut LoomApp) {
    ui.horizontal_wrapped(|ui| {
        for l in ViewportLayout::ALL {
            if layout_icon(ui, l, app.state.layout == l).clicked() {
                let _ = app.dispatch(Command::ViewportLayout { layout: l });
            }
        }
    });
    ui.add_space(8.0);
    ui.label("Click a viewport below to choose its view.");
    let size = egui::vec2(380.0, 240.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, egui::Color32::from_gray(60));
    let cells = app.state.layout.cells(&app.state.splits);
    for (i, c) in cells.iter().enumerate() {
        let r = egui::Rect::from_min_max(
            egui::pos2(rect.min.x + c[0] * rect.width(), rect.min.y + c[1] * rect.height()),
            egui::pos2(rect.min.x + c[2] * rect.width(), rect.min.y + c[3] * rect.height()),
        )
        .shrink(2.0);
        let active = i == app.state.active_viewport;
        painter.rect_stroke(r, 0.0, egui::Stroke::new(1.0, if active { crate::viewports::ACTIVE_FRAME } else { egui::Color32::from_gray(200) }));
        let label = app.state.viewports.get(i).map(|v| v.kind.label()).unwrap_or("");
        painter.text(r.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(14.0), egui::Color32::WHITE);
        let resp = ui.interact(r, ui.id().with(("layout_cell", i)), egui::Sense::click());
        let popup = ui.id().with(("layout_cell_popup", i));
        if resp.clicked() || resp.secondary_clicked() {
            ui.memory_mut(|m| m.toggle_popup(popup));
        }
        egui::popup::popup_below_widget(ui, popup, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
            ui.set_min_width(120.0);
            for kind in [ViewKind::Perspective, ViewKind::Orthographic, ViewKind::Front, ViewKind::Back, ViewKind::Top, ViewKind::Bottom, ViewKind::Left, ViewKind::Right] {
                if ui.button(kind.label()).clicked() {
                    let _ = app.dispatch(Command::ViewportView { index: i as u32, kind });
                }
            }
        });
    }
}
