//! Viewport Configuration dialog: Rendering Method (shading and the
//! rendering options), Layout, Safe Frames, Statistics and ViewCube.

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
    Statistics,
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
            "Statistics" => Tab::Statistics,
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
                ui.selectable_value(&mut tab, Tab::Statistics, "Statistics");
                ui.selectable_value(&mut tab, Tab::ViewCube, "ViewCube");
            });
            ui.separator();
            match tab {
                Tab::Layout => layout_tab(ui, app),
                Tab::SafeFrames => safe_frames_tab(ui, app),
                Tab::Statistics => statistics_tab(ui, app),
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
                    let mut sel_edged = app.state.display.display_selected_edged;
                    if ui.checkbox(&mut sel_edged, "Display Selected with Edged Faces").changed() {
                        let _ = app.dispatch(Command::DisplaySelectedEdgedFacesToggle);
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
                        if ui.add(crate::spinner::spin(ui, &mut fov, 1.0..=175.0).speed(0.5).fixed_decimals(1)).changed() {
                            let _ = app.dispatch(Command::SetFov { degrees: fov });
                        }
                    });
                }
                Tab::ViewCube => viewcube_tab(ui, app),
            }
        });
    ctx.data_mut(|d| d.insert_temp(tab_id, tab));
}

/// Safe Frames panel: which frames show, their sizes (percent smaller than
/// the live area, horizontal / vertical), the 12-Field Grid, and Show Safe
/// Frames in Active View.
fn safe_frames_tab(ui: &mut egui::Ui, app: &mut LoomApp) {
    let s = app.state.display.safe_frames;
    let none = Command::SetSafeFrames { live_area: None, action_safe: None, title_safe: None, user_safe: None, grid: None, action: None, title: None, user: None, grid_4x3: None };
    let with = |f: &dyn Fn(&mut Command)| {
        let mut c = none.clone();
        f(&mut c);
        c
    };
    egui::Grid::new("safe_frames").num_columns(3).show(ui, |ui| {
        let mut live = s.live_area;
        if ui.checkbox(&mut live, "Live Area").changed() {
            let _ = app.dispatch(with(&|c| if let Command::SetSafeFrames { live_area, .. } = c { *live_area = Some(live) }));
        }
        ui.label("Horizontal");
        ui.label("Vertical");
        ui.end_row();
        for (label, on, pct, which) in [("Action Safe", s.action_safe, s.action, 0), ("Title Safe", s.title_safe, s.title, 1), ("User Safe", s.user_safe, s.user, 2)] {
            let mut v = on;
            if ui.checkbox(&mut v, label).changed() {
                let _ = app.dispatch(with(&|c| if let Command::SetSafeFrames { action_safe, title_safe, user_safe, .. } = c { *[action_safe, title_safe, user_safe][which] = Some(v) }));
            }
            let mut p = pct;
            let h = ui.add(crate::spinner::spin(ui, &mut p[0], 0.0..=99.0).speed(0.5).suffix("%")).changed();
            let w = ui.add(crate::spinner::spin(ui, &mut p[1], 0.0..=99.0).speed(0.5).suffix("%")).changed();
            if h || w {
                let _ = app.dispatch(with(&|c| if let Command::SetSafeFrames { action, title, user, .. } = c { *[action, title, user][which] = Some(p) }));
            }
            ui.end_row();
        }
    });
    ui.horizontal(|ui| {
        let mut grid = s.grid;
        if ui.checkbox(&mut grid, "12-Field Grid").changed() {
            let _ = app.dispatch(with(&|c| if let Command::SetSafeFrames { grid: g, .. } = c { *g = Some(grid) }));
        }
        if ui.radio(s.grid_4x3, "4x3").clicked() {
            let _ = app.dispatch(with(&|c| if let Command::SetSafeFrames { grid_4x3, .. } = c { *grid_4x3 = Some(true) }));
        }
        if ui.radio(!s.grid_4x3, "12x9").clicked() {
            let _ = app.dispatch(with(&|c| if let Command::SetSafeFrames { grid_4x3, .. } = c { *grid_4x3 = Some(false) }));
        }
    });
    ui.label(egui::RichText::new(format!("Live area aspect from the render output: {:.3}", app.state.scene.render.image_aspect())).small().weak());
    ui.separator();
    let mut on = app.state.active().safe_frames;
    if ui.checkbox(&mut on, "Show Safe Frames in Active View").with_shortcut(&app.state, Command::ViewportSafeframesToggle.name()).changed() {
        let _ = app.dispatch(Command::ViewportSafeframesToggle);
    }
}

/// Statistics panel: Setup (the counts, FPS, Total / Selection / Total +
/// Selection), Show Statistics In Active View, Default Settings.
fn statistics_tab(ui: &mut egui::Ui, app: &mut LoomApp) {
    use loom_commands::display::StatisticsScope;
    let s = app.state.display.statistics;
    let set = |polygons, triangles, edges, vertices, fps, scope: Option<&str>| Command::SetStatistics { polygons, triangles, edges, vertices, fps, scope: scope.map(str::to_owned), default_settings: None };
    ui.label(egui::RichText::new("Setup").strong());
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            for (label, on, i) in [("Polygon Count", s.polygons, 0), ("Triangle Count", s.triangles, 1), ("Edge Count", s.edges, 2), ("Vertex Count", s.vertices, 3), ("Frames Per Second", s.fps, 4)] {
                let mut v = on;
                if ui.checkbox(&mut v, label).changed() {
                    let mut f = [None; 5];
                    f[i] = Some(v);
                    let _ = app.dispatch(set(f[0], f[1], f[2], f[3], f[4], None));
                }
            }
        });
        ui.vertical(|ui| {
            for (label, scope) in [("Total", StatisticsScope::Total), ("Selection", StatisticsScope::Selection), ("Total + Selection", StatisticsScope::TotalAndSelection)] {
                if ui.radio(s.scope == scope, label).clicked() {
                    let _ = app.dispatch(set(None, None, None, None, None, Some(label)));
                }
            }
        });
    });
    ui.separator();
    ui.label(egui::RichText::new("Application").strong());
    let mut on = app.state.show_statistics;
    if ui.checkbox(&mut on, "Show Statistics In Active View").with_shortcut(&app.state, Command::ShowStatisticsToggle.name()).changed() {
        let _ = app.dispatch(Command::ShowStatisticsToggle);
    }
    if ui.button("Default Settings").clicked() {
        let _ = app.dispatch(Command::SetStatistics { polygons: None, triangles: None, edges: None, vertices: None, fps: None, scope: None, default_settings: Some(true) });
    }
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
