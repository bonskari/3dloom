//! Viewport Configuration dialog: Rendering Method (shading and the
//! rendering options), Layout, Safe Frames and ViewCube.

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
    ViewCube,
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let tab_id = egui::Id::new("viewport_configuration_tab");
    let mut tab: Tab = ctx.data(|d| d.get_temp(tab_id)).unwrap_or_default();
    // A command can open the dialog on a panel (ViewCube menu > Configure).
    if let Some(panel) = app.state.config_panel.take() {
        tab = match panel.as_str() {
            "Rendering Method" => Tab::RenderingMethod,
            "Safe Frames" => Tab::SafeFrames,
            "ViewCube" => Tab::ViewCube,
            _ => Tab::Layout,
        };
    }
    egui::Window::new("Viewport Configuration")
        .open(open)
        .resizable(false)
        .default_width(420.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut tab, Tab::RenderingMethod, "Rendering Method");
                ui.selectable_value(&mut tab, Tab::Layout, "Layout");
                ui.selectable_value(&mut tab, Tab::SafeFrames, "Safe Frames");
                ui.selectable_value(&mut tab, Tab::ViewCube, "ViewCube");
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
                    ui.separator();
                    ui.label("Rendering Options");
                    let mut disabled = app.state.active().disabled;
                    if ui.checkbox(&mut disabled, "Disable View").with_shortcut(&app.state, Command::DisableViewport.name()).changed() {
                        let _ = app.dispatch(Command::DisableViewport);
                    }
                    let mut shade = app.state.shade_selected_faces;
                    if ui.checkbox(&mut shade, "Shade Selected Faces").with_shortcut(&app.state, Command::ShadeSelectedFacesToggle.name()).changed() {
                        let _ = app.dispatch(Command::ShadeSelectedFacesToggle);
                    }
                    let mut brackets = app.state.selection_brackets;
                    if ui.checkbox(&mut brackets, "Use Selection Brackets").with_shortcut(&app.state, Command::SelectionBracketToggle.name()).changed() {
                        let _ = app.dispatch(Command::SelectionBracketToggle);
                    }
                    let mut clipping = app.state.active().clipping;
                    if ui.checkbox(&mut clipping, "Viewport Clipping").changed() {
                        let _ = app.dispatch(Command::ViewportClipping);
                    }
                    ui.separator();
                    ui.label("Perspective User View");
                    let mut fov = app.state.active().fov_y_radians.to_degrees();
                    ui.horizontal(|ui| {
                        ui.label("Field Of View:");
                        if ui.add(egui::DragValue::new(&mut fov).range(1.0..=175.0).speed(0.5).fixed_decimals(1)).changed() {
                            let _ = app.dispatch(Command::SetFov { degrees: fov });
                        }
                    });
                }
                Tab::ViewCube => viewcube_tab(ui, app),
            }
        });
    ctx.data_mut(|d| d.insert_temp(tab_id, tab));
}

/// ViewCube panel: Show the ViewCube, in all views or the active one, size,
/// inactive opacity, compass.
fn viewcube_tab(ui: &mut egui::Ui, app: &mut LoomApp) {
    let v = app.state.viewcube;
    let set = |app: &mut LoomApp, c: Command| {
        let _ = app.dispatch(c);
    };
    let mut show = v.show;
    if ui.checkbox(&mut show, "Show the ViewCube").changed() {
        set(app, Command::ViewCubeToggle);
    }
    ui.add_enabled_ui(v.show, |ui| {
        if ui.radio(v.all_views, "In All Views").clicked() {
            set(app, cube_options(Some(true), None, None, None));
        }
        if ui.radio(!v.all_views, "Only in Active View").clicked() {
            set(app, cube_options(Some(false), None, None, None));
        }
        ui.separator();
        ui.label("Display Options");
        let sizes = ["Large", "Normal", "Small", "Tiny"];
        egui::ComboBox::from_label("ViewCube Size").selected_text(sizes[v.size.min(3) as usize]).show_ui(ui, |ui| {
            for (i, name) in sizes.iter().enumerate() {
                if ui.selectable_label(v.size as usize == i, *name).clicked() {
                    set(app, cube_options(None, Some(i as u32), None, None));
                }
            }
        });
        let opacity = (v.inactive_opacity * 100.0).round() as u32;
        egui::ComboBox::from_label("Inactive Opacity").selected_text(format!("{opacity}%")).show_ui(ui, |ui| {
            for o in [0u32, 25, 50, 75, 100] {
                if ui.selectable_label(opacity == o, format!("{o}%")).clicked() {
                    set(app, cube_options(None, None, Some(o as f32), None));
                }
            }
        });
        ui.separator();
        let mut compass = v.compass;
        if ui.checkbox(&mut compass, "Show Compass Below the ViewCube").changed() {
            set(app, cube_options(None, None, None, Some(compass)));
        }
    });
}

fn cube_options(all_views: Option<bool>, size: Option<u32>, inactive_opacity: Option<f32>, compass: Option<bool>) -> Command {
    Command::SetViewCubeOptions { show: None, all_views, size, inactive_opacity, compass }
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
        p.rect_stroke(r, 0.0, egui::Stroke::new(1.0, egui::Color32::from_gray(220)), egui::StrokeKind::Middle);
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
        painter.rect_stroke(r, 0.0, egui::Stroke::new(1.0, if active { crate::viewports::ACTIVE_FRAME } else { egui::Color32::from_gray(200) }), egui::StrokeKind::Middle);
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
