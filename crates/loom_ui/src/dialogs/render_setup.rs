//! Render Setup (F10): the Common panel (Time Output, Output Size, Options,
//! Render Output, Assign Renderer) and the Bevy renderer's panel, with the
//! dialog's bottom controls (Production / Iterative, Viewport, Lock View,
//! Render). Every change is a command, one undo step (a spinner drag is one).

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Panel {
    #[default]
    Common,
    Renderer,
}

const PRESETS: [(u32, u32); 4] = [(320, 240), (640, 480), (1280, 720), (1920, 1080)];

/// Render SetUp... arguments; the omitted ones keep their value.
#[derive(Default)]
struct Setup {
    width: Option<u32>,
    height: Option<u32>,
    image_aspect: Option<f32>,
    pixel_aspect: Option<f32>,
    lock_image_aspect: Option<bool>,
    lock_pixel_aspect: Option<bool>,
    render_hidden: Option<bool>,
    force_two_sided: Option<bool>,
    atmospherics: Option<bool>,
    effects: Option<bool>,
    save_file: Option<bool>,
}

impl Setup {
    fn cmd(self) -> Command {
        let Setup { width, height, image_aspect, pixel_aspect, lock_image_aspect, lock_pixel_aspect, render_hidden, force_two_sided, atmospherics, effects, save_file } = self;
        Command::RenderSetup { width, height, image_aspect, pixel_aspect, lock_image_aspect, lock_pixel_aspect, render_hidden, force_two_sided, atmospherics, effects, save_file, output: None }
    }
}

/// Bevy Renderer Settings arguments.
#[derive(Default)]
struct Bevy {
    antialiasing: Option<bool>,
    supersampling: Option<u32>,
    shadows: Option<bool>,
    shadow_map_size: Option<u32>,
}

impl Bevy {
    fn cmd(self) -> Command {
        let Bevy { antialiasing, supersampling, shadows, shadow_map_size } = self;
        Command::BevyRendererSettings { antialiasing, supersampling, shadows, shadow_map_size }
    }
}

/// A spinner drag is one undo step.
fn merge_drag(app: &mut LoomApp, r: &egui::Response) {
    if r.drag_started() {
        app.state.undo.begin("Render Setup");
    }
    if r.drag_stopped() {
        app.state.undo.end();
    }
}

fn run(app: &mut LoomApp, c: Command) {
    let _ = app.dispatch(c);
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let panel_id = egui::Id::new("render_setup_panel");
    let mut panel: Panel = ctx.data(|d| d.get_temp(panel_id)).unwrap_or_default();
    egui::Window::new("Render Setup").open(open).resizable(false).default_width(420.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut panel, Panel::Common, "Common");
            ui.selectable_value(&mut panel, Panel::Renderer, "Renderer");
        });
        ui.separator();
        match panel {
            Panel::Common => common(ui, app),
            Panel::Renderer => renderer(ui, app),
        }
        ui.separator();
        crate::rendered_frame::viewport_controls(ui, app, "render_setup_viewport", "Lock View");
        crate::rendered_frame::render_buttons(ui, app, "render_setup_mode", false);
    });
    ctx.data_mut(|d| d.insert_temp(panel_id, panel));
}

fn common(ui: &mut egui::Ui, app: &mut LoomApp) {
    let s = app.state.scene.render.clone();
    ui.label(egui::RichText::new("Time Output").strong());
    ui.label("Single: the current frame");
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Output Size").strong());
    egui::Grid::new("render_setup_size").num_columns(4).show(ui, |ui| {
        let (mut w, mut h) = (s.width, s.height);
        ui.label("Width:");
        let a = ui.add(crate::spinner::spin(ui, &mut w, 1..=loom_commands::render::MAX_RENDER_SIZE));
        merge_drag(app, &a);
        if a.changed() {
            run(app, Setup { width: Some(w), ..Default::default() }.cmd());
        }
        ui.label("Height:");
        let b = ui.add(crate::spinner::spin(ui, &mut h, 1..=loom_commands::render::MAX_RENDER_SIZE));
        merge_drag(app, &b);
        if b.changed() {
            run(app, Setup { height: Some(h), ..Default::default() }.cmd());
        }
        ui.end_row();
        let mut lock = s.lock_image_aspect;
        if ui.checkbox(&mut lock, "").on_hover_text("Lock Image Aspect").changed() {
            run(app, Setup { lock_image_aspect: Some(lock), ..Default::default() }.cmd());
        }
        ui.label("Image Aspect:");
        if s.lock_image_aspect {
            ui.label(format!("{:.3}", s.image_aspect()));
        } else {
            let mut aspect = s.image_aspect();
            let r = ui.add(crate::spinner::spin(ui, &mut aspect, 0.01..=100.0).speed(0.01).max_decimals(3));
            merge_drag(app, &r);
            if r.changed() {
                run(app, Setup { image_aspect: Some(aspect), ..Default::default() }.cmd());
            }
        }
        ui.end_row();
        let mut lock = s.lock_pixel_aspect;
        if ui.checkbox(&mut lock, "").on_hover_text("Lock Pixel Aspect").changed() {
            run(app, Setup { lock_pixel_aspect: Some(lock), ..Default::default() }.cmd());
        }
        ui.label("Pixel Aspect:");
        if s.lock_pixel_aspect {
            ui.label(format!("{:.3}", s.pixel_aspect));
        } else {
            let mut pa = s.pixel_aspect;
            let r = ui.add(crate::spinner::spin(ui, &mut pa, 0.01..=100.0).speed(0.01).max_decimals(3));
            merge_drag(app, &r);
            if r.changed() {
                run(app, Setup { pixel_aspect: Some(pa), ..Default::default() }.cmd());
            }
        }
        ui.end_row();
    });
    ui.horizontal(|ui| {
        for (pw, ph) in PRESETS {
            if ui.selectable_label((s.width, s.height) == (pw, ph), format!("{pw}x{ph}")).clicked() {
                run(app, Setup { width: Some(pw), height: Some(ph), ..Default::default() }.cmd());
            }
        }
    });
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Options").strong());
    ui.horizontal(|ui| {
        let mut atmospherics = s.atmospherics;
        if ui.checkbox(&mut atmospherics, "Atmospherics").on_hover_text("Render the Environment's atmospheric effects").changed() {
            run(app, Setup { atmospherics: Some(atmospherics), ..Default::default() }.cmd());
        }
        let mut effects = s.effects;
        if ui.checkbox(&mut effects, "Effects").on_hover_text("Apply the rendering effects").changed() {
            run(app, Setup { effects: Some(effects), ..Default::default() }.cmd());
        }
    });
    ui.horizontal(|ui| {
        let mut hidden = s.render_hidden;
        if ui.checkbox(&mut hidden, "Render Hidden Geometry").changed() {
            run(app, Setup { render_hidden: Some(hidden), ..Default::default() }.cmd());
        }
        let mut two = s.force_two_sided;
        if ui.checkbox(&mut two, "Force 2-Sided").changed() {
            run(app, Setup { force_two_sided: Some(two), ..Default::default() }.cmd());
        }
    });
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Render Output").strong());
    ui.horizontal(|ui| {
        let mut save = s.save_file;
        if ui.add_enabled(s.output_path.is_some(), egui::Checkbox::new(&mut save, "Save File")).changed() {
            run(app, Setup { save_file: Some(save), ..Default::default() }.cmd());
        }
        if ui.button("Files...").clicked() {
            app.state.dialog = Some(loom_commands::Dialog::RenderFile);
        }
    });
    ui.label(s.output_path.as_deref().unwrap_or("(no output file)"));
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Assign Renderer").strong());
    ui.label("Production: Bevy (PBR rasteriser)");
}

fn renderer(ui: &mut egui::Ui, app: &mut LoomApp) {
    let b = app.state.scene.render.bevy.clone();
    ui.label(egui::RichText::new("Bevy Renderer").strong());
    ui.label(egui::RichText::new("Image Quality").strong());
    ui.horizontal(|ui| {
        let mut aa = b.antialiasing;
        if ui.checkbox(&mut aa, "Antialiasing (4x MSAA)").changed() {
            run(app, Bevy { antialiasing: Some(aa), ..Default::default() }.cmd());
        }
        ui.label("Supersampling:");
        egui::ComboBox::from_id_salt("render_supersampling").selected_text(format!("{}x{}", b.supersampling, b.supersampling)).show_ui(ui, |ui| {
            for n in 1..=4u32 {
                if ui.selectable_label(n == b.supersampling, format!("{n}x{n}")).clicked() {
                    run(app, Bevy { supersampling: Some(n), ..Default::default() }.cmd());
                }
            }
        });
    });
    ui.label(egui::RichText::new("Key Light").strong());
    ui.horizontal(|ui| {
        let mut sh = b.shadows;
        if ui.checkbox(&mut sh, "Shadows").changed() {
            run(app, Bevy { shadows: Some(sh), ..Default::default() }.cmd());
        }
        ui.label("Map Size:");
        ui.add_enabled_ui(b.shadows, |ui| {
            egui::ComboBox::from_id_salt("render_shadow_map").selected_text(b.shadow_map_size.to_string()).show_ui(ui, |ui| {
                for n in [512u32, 1024, 2048, 4096, 8192] {
                    if ui.selectable_label(n == b.shadow_map_size, n.to_string()).clicked() {
                        run(app, Bevy { shadow_map_size: Some(n), ..Default::default() }.cmd());
                    }
                }
            });
        });
    });
}
