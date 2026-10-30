//! Viewport Configuration dialog: Rendering Method (rendering level,
//! transparency, Apply To, rendering options), Layout, Safe Frames,
//! Adaptive Degradation, Regions, Statistics, Lighting and Shadows and
//! ViewCube; Save as Default keeps the configuration for new scenes.

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
    Degradation,
    Regions,
    Statistics,
    Lighting,
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
            "Adaptive Degradation" => Tab::Degradation,
            "Regions" => Tab::Regions,
            "Statistics" => Tab::Statistics,
            "Lighting and Shadows" => Tab::Lighting,
            "ViewCube" => Tab::ViewCube,
            _ => Tab::Layout,
        };
    }
    egui::Window::new("Viewport Configuration")
        .open(open)
        .resizable(false)
        .default_width(460.0)
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut tab, Tab::RenderingMethod, "Rendering Method");
                ui.selectable_value(&mut tab, Tab::Layout, "Layout");
                ui.selectable_value(&mut tab, Tab::SafeFrames, "Safe Frames");
                ui.selectable_value(&mut tab, Tab::Degradation, "Adaptive Degradation");
                ui.selectable_value(&mut tab, Tab::Regions, "Regions");
                ui.selectable_value(&mut tab, Tab::Statistics, "Statistics");
                ui.selectable_value(&mut tab, Tab::Lighting, "Lighting and Shadows");
                ui.selectable_value(&mut tab, Tab::ViewCube, "ViewCube");
            });
            ui.separator();
            match tab {
                Tab::Layout => layout_tab(ui, app),
                Tab::SafeFrames => safe_frames_tab(ui, app),
                Tab::Statistics => statistics_tab(ui, app),
                Tab::RenderingMethod => rendering_tab(ui, app),
                Tab::Degradation => degradation_tab(ui, app),
                Tab::Regions => regions_tab(ui, app),
                Tab::Lighting => lighting_tab(ui, app),
                Tab::ViewCube => viewcube_tab(ui, app),
            }
            ui.separator();
            if ui.button("Save as Default").on_hover_text("New and reset scenes start with this viewport configuration").clicked() {
                let _ = app.dispatch(Command::SaveViewportDefaults);
            }
        });
    ctx.data_mut(|d| d.insert_temp(tab_id, tab));
}

/// The Apply To choice of the Rendering Method and Lighting panels (kept while the dialog shows).
fn apply_to(ui: &mut egui::Ui, id: &str) -> Option<String> {
    let key = egui::Id::new(("viewport_config_apply_to", id.to_owned()));
    let mut choice: String = ui.ctx().data(|d| d.get_temp(key)).unwrap_or_else(|| "Active Only".to_owned());
    ui.label(egui::RichText::new("Apply To").strong());
    ui.horizontal(|ui| {
        for c in ["Active Only", "All Viewports", "All But Active"] {
            ui.radio_value(&mut choice, c.to_owned(), c);
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(key, choice.clone()));
    Some(choice)
}

/// Rendering Method: Rendering Level, Edged Faces, Transparency, Apply To,
/// the rendering options and the Perspective view's field of view.
fn rendering_tab(ui: &mut egui::Ui, app: &mut LoomApp) {
    let vp = app.state.active().clone();
    let to = apply_to(ui, "rendering");
    let render = |app: &mut LoomApp, shading: Option<String>, edged: Option<bool>, transparency: Option<String>, textures: Option<bool>, two: Option<bool>| {
        let _ = app.dispatch(Command::SetViewportRendering { apply_to: to.clone(), shading, edged_faces: edged, transparency, disable_textures: textures, force_two_sided: two });
    };
    ui.separator();
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("Rendering Level").strong());
            for s in Shading::LEVELS.into_iter().chain([Shading::Realistic]) {
                if ui.radio(vp.shading == s, s.label()).clicked() {
                    render(app, Some(s.label().into()), None, None, None, None);
                }
            }
            let mut edged = vp.edged_faces;
            if ui.checkbox(&mut edged, "Edged Faces").with_shortcut(&app.state, Command::ViewEdgedFacesToggle.name()).changed() {
                render(app, None, Some(edged), None, None, None);
            }
        });
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("Transparency").strong());
            for t in loom_commands::Transparency::ALL {
                if ui.radio(vp.transparency == t, t.label()).clicked() {
                    render(app, None, None, Some(t.label().into()), None, None);
                }
            }
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Rendering Options").strong());
            let mut disabled = vp.disabled;
            if ui.checkbox(&mut disabled, "Disable View").with_shortcut(&app.state, Command::DisableViewport.name()).changed() {
                let _ = app.dispatch(Command::DisableViewport);
            }
            let mut textures = vp.disable_textures;
            if ui.checkbox(&mut textures, "Disable Textures").changed() {
                render(app, None, None, None, Some(textures), None);
            }
            let mut two = vp.force_two_sided;
            if ui.checkbox(&mut two, "Force 2-Sided").changed() {
                render(app, None, None, None, None, Some(two));
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
            let mut clipping = vp.clipping;
            if ui.checkbox(&mut clipping, "Viewport Clipping").changed() {
                let _ = app.dispatch(Command::ViewportClipping);
            }
        });
    });
    ui.separator();
    ui.label("Perspective User View");
    let mut fov = vp.fov_y_radians.to_degrees();
    ui.horizontal(|ui| {
        ui.label("Field Of View:");
        if ui.add(crate::spinner::spin(ui, &mut fov, 1.0..=175.0).speed(0.5).fixed_decimals(1)).changed() {
            let _ = app.dispatch(Command::SetFov { degrees: fov });
        }
    });
}

/// Adaptive Degradation: on / off (O), the levels the active and the other
/// views drop to, Maintain FPS, Never Degrade Selected, Degrade to Default Lighting.
fn degradation_tab(ui: &mut egui::Ui, app: &mut LoomApp) {
    let d = app.state.degradation.clone();
    let set = |app: &mut LoomApp, general: Option<Shading>, active: Option<Shading>, fps: Option<f32>, never: Option<bool>, lighting: Option<bool>| {
        let _ = app.dispatch(Command::SetAdaptiveDegradation { general: general.map(|s| s.label().into()), active: active.map(|s| s.label().into()), maintain_fps: fps, never_degrade_selected: never, degrade_to_default_lighting: lighting });
    };
    let mut on = d.on;
    if ui.checkbox(&mut on, "Adaptive Degradation").with_shortcut(&app.state, Command::AdaptiveDegradationToggle.name()).changed() {
        let _ = app.dispatch(Command::AdaptiveDegradationToggle);
    }
    let levels = [Shading::BoundingBox, Shading::Wireframe, Shading::Facets, Shading::Smooth, Shading::SmoothHighlights];
    ui.horizontal_top(|ui| {
        for (title, cur, general) in [("General Degradation", d.general, true), ("Active Degradation", d.active, false)] {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(title).strong());
                for l in levels {
                    if ui.radio(cur == l, l.label()).clicked() {
                        if general { set(app, Some(l), None, None, None, None) } else { set(app, None, Some(l), None, None, None) }
                    }
                }
            });
        }
    });
    ui.horizontal(|ui| {
        ui.label("Maintain FPS:");
        let mut fps = d.maintain_fps;
        if ui.add(crate::spinner::spin(ui, &mut fps, 1.0..=120.0).speed(0.2).fixed_decimals(1)).changed() {
            set(app, None, None, Some(fps), None, None);
        }
    });
    let mut never = d.never_degrade_selected;
    if ui.checkbox(&mut never, "Never Degrade Selected").changed() {
        set(app, None, None, None, Some(never), None);
    }
    let mut lighting = d.degrade_to_default_lighting;
    if ui.checkbox(&mut lighting, "Degrade to Default Lighting").changed() {
        set(app, None, None, None, None, Some(lighting));
    }
}

/// Regions: the Blowup and Sub Region rectangles in render output pixels.
fn regions_tab(ui: &mut egui::Ui, app: &mut LoomApp) {
    let r = &app.state.scene.render;
    let (w, h) = (r.width.max(1) as f32, r.height.max(1) as f32);
    for (title, rect, blowup) in [("Blowup Region", r.blowup, true), ("Sub Region", r.region, false)] {
        ui.label(egui::RichText::new(title).strong());
        let mut px = [rect[0] * w, rect[1] * h, (rect[2] - rect[0]) * w, (rect[3] - rect[1]) * h];
        let mut changed = false;
        egui::Grid::new(title).num_columns(4).show(ui, |ui| {
            for (i, label) in ["X:", "Y:", "Width:", "Height:"].iter().enumerate() {
                ui.label(*label);
                let max = if i % 2 == 0 { w } else { h };
                changed |= ui.add(crate::spinner::spin(ui, &mut px[i], 0.0..=max).speed(1.0).fixed_decimals(0)).changed();
                if i == 1 {
                    ui.end_row();
                }
            }
        });
        if changed {
            let fr = [px[0] / w, px[1] / h, (px[0] + px[2]) / w, (px[1] + px[3]) / h];
            let c = if blowup { Command::SetRenderRegions { region: None, blowup: Some(fr) } } else { Command::SetRenderRegions { region: Some(fr), blowup: None } };
            let _ = app.dispatch(c);
        }
        ui.add_space(6.0);
    }
}

/// Lighting and Shadows: Illuminate Scene With (scene or default lights, 1
/// or 2 default lights), Enable Hardware Shading (the Realistic view), Apply To.
fn lighting_tab(ui: &mut egui::Ui, app: &mut LoomApp) {
    let vp = app.state.active().clone();
    let to = apply_to(ui, "lighting");
    let set = |app: &mut LoomApp, scene: Option<bool>, lights: Option<u32>, hw: Option<bool>| {
        let _ = app.dispatch(Command::SetViewportLighting { apply_to: to.clone(), scene_lights: scene, default_lights: lights, hardware: hw });
    };
    ui.separator();
    ui.label(egui::RichText::new("Illuminate Scene With").strong());
    if ui.radio(vp.scene_lights, "Scene Lights").clicked() {
        set(app, Some(true), None, None);
    }
    if ui.radio(!vp.scene_lights, "Default Lights").clicked() {
        set(app, Some(false), None, None);
    }
    ui.indent("default_lights", |ui| {
        if ui.radio(vp.default_lights < 2, "1 Light").clicked() {
            set(app, None, Some(1), None);
        }
        if ui.radio(vp.default_lights >= 2, "2 Lights").clicked() {
            set(app, None, Some(2), None);
        }
    });
    ui.separator();
    let mut hw = vp.shading == Shading::Realistic;
    if ui.checkbox(&mut hw, "Enable Hardware Shading").on_hover_text("The view shows Realistic shading (lights, shadows) from the Bevy renderer").changed() {
        set(app, None, None, Some(hw));
    }
}

/// Safe Frames panel: which frames show, their sizes (percent smaller than
/// the live area, horizontal / vertical), the 12-Field Grid, and Show Safe
/// Frames in Active View.
fn safe_frames_tab(ui: &mut egui::Ui, app: &mut LoomApp) {
    let s = app.state.display.safe_frames;
    let none = Command::SetSafeFrames { live_area: None, action_safe: None, title_safe: None, user_safe: None, grid: None, action: None, title: None, user: None, grid_4x3: None, lock: None, region: None, default_settings: None };
    let with = |f: &dyn Fn(&mut Command)| {
        let mut c = none.clone();
        f(&mut c);
        c
    };
    egui::Grid::new("safe_frames").num_columns(4).show(ui, |ui| {
        let mut live = s.live_area;
        if ui.checkbox(&mut live, "Live Area").changed() {
            let _ = app.dispatch(with(&|c| if let Command::SetSafeFrames { live_area, .. } = c { *live_area = Some(live) }));
        }
        ui.label("Horizontal");
        ui.label("Vertical");
        ui.label("");
        ui.end_row();
        let mut region = s.region;
        if ui.checkbox(&mut region, "Region (when Region Rendering)").changed() {
            let _ = app.dispatch(with(&|c| if let Command::SetSafeFrames { region: r, .. } = c { *r = Some(region) }));
        }
        ui.end_row();
        for (label, on, pct, which) in [("Action Safe", s.action_safe, s.action, 0), ("Title Safe", s.title_safe, s.title, 1), ("User Safe", s.user_safe, s.user, 2)] {
            let mut v = on;
            if ui.checkbox(&mut v, label).changed() {
                let _ = app.dispatch(with(&|c| if let Command::SetSafeFrames { action_safe, title_safe, user_safe, .. } = c { *[action_safe, title_safe, user_safe][which] = Some(v) }));
            }
            let mut p = pct;
            let locked = s.lock[which];
            let h = ui.add(crate::spinner::spin(ui, &mut p[0], 0.0..=99.0).speed(0.5).suffix("%")).changed();
            // Locked: one Both value for the two directions.
            let w = ui.add_enabled(!locked, crate::spinner::spin(ui, &mut p[1], 0.0..=99.0).speed(0.5).suffix("%")).changed();
            if h || w {
                let _ = app.dispatch(with(&|c| if let Command::SetSafeFrames { action, title, user, .. } = c { *[action, title, user][which] = Some(p) }));
            }
            let mut l = locked;
            if ui.checkbox(&mut l, "Lock").changed() {
                let mut v = s.lock.to_vec();
                v[which] = l;
                let _ = app.dispatch(with(&|c| if let Command::SetSafeFrames { lock, .. } = c { *lock = Some(v.clone()) }));
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
    if ui.button("Default Settings").clicked() {
        let _ = app.dispatch(with(&|c| if let Command::SetSafeFrames { default_settings, .. } = c { *default_settings = Some(true) }));
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
        ui.label(egui::RichText::new("When Dragging on the ViewCube").strong());
        let mut snap = v.snap_to_closest;
        if ui.checkbox(&mut snap, "Snap to Closest View").changed() {
            set(app, cube_behaviour(Some(snap), None, None, None, None));
        }
        ui.label(egui::RichText::new("When Clicking on the ViewCube").strong());
        let mut fit = v.fit_to_view;
        if ui.checkbox(&mut fit, "Fit-to-View on View Change").changed() {
            set(app, cube_behaviour(None, Some(fit), None, None, None));
        }
        let mut animated = v.animated;
        if ui.checkbox(&mut animated, "Use Animated Transitions when Switching Views").changed() {
            set(app, cube_behaviour(None, None, Some(animated), None, None));
        }
        let mut upright = v.keep_upright;
        if ui.checkbox(&mut upright, "Keep Scene Upright").changed() {
            set(app, cube_behaviour(None, None, None, Some(upright), None));
        }
        ui.separator();
        let mut compass = v.compass;
        if ui.checkbox(&mut compass, "Show Compass Below the ViewCube").changed() {
            set(app, cube_options(None, None, None, Some(compass)));
        }
        ui.horizontal(|ui| {
            ui.label("Angle of North:");
            let mut n = v.north;
            if ui.add(crate::spinner::spin(ui, &mut n, -360.0..=360.0).speed(1.0).fixed_decimals(1)).changed() {
                set(app, cube_behaviour(None, None, None, None, Some(n)));
            }
        });
    });
}

fn cube_options(all_views: Option<bool>, size: Option<u32>, inactive_opacity: Option<f32>, compass: Option<bool>) -> Command {
    Command::SetViewCubeOptions { show: None, all_views, size, inactive_opacity, compass, snap_to_closest: None, fit_to_view: None, animated: None, keep_upright: None, north: None }
}

fn cube_behaviour(snap_to_closest: Option<bool>, fit_to_view: Option<bool>, animated: Option<bool>, keep_upright: Option<bool>, north: Option<f32>) -> Command {
    Command::SetViewCubeOptions { show: None, all_views: None, size: None, inactive_opacity: None, compass: None, snap_to_closest, fit_to_view, animated, keep_upright, north }
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
        let label = if i < app.state.viewports.len() { app.state.view_label(i) } else { String::new() };
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
            // Cameras and the lights a view can look through.
            let views: Vec<(loom_scene::NodeId, String)> = app.state.scene.nodes().filter(|n| matches!(&n.object, loom_scene::ObjectKind::Camera { .. }) || matches!(&n.object, loom_scene::ObjectKind::Light(l) if l.light_type.aims())).map(|n| (n.id, n.name.clone())).collect();
            if !views.is_empty() {
                ui.separator();
            }
            for (id, name) in views {
                if ui.button(name).clicked() {
                    let _ = app.dispatch(Command::ViewportView { index: i as u32, kind: ViewKind::Camera(id) });
                }
            }
        });
    }
}
