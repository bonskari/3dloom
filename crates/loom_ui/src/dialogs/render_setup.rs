//! Render Setup (F10): the Common panel (Time Output, Output Size, Options,
//! Render Output, Assign Renderer) and the Bevy renderer's panel, with the
//! dialog's bottom controls (Production / Iterative, Viewport, Lock View,
//! Render). Every change is a command, one undo step (a spinner drag is one).

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;
use loom_scene::render::{AaFilter, AoQuality, Tonemapper};

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
    save_file: Option<bool>,
}

impl Setup {
    fn cmd(self) -> Command {
        let Setup { width, height, image_aspect, pixel_aspect, lock_image_aspect, lock_pixel_aspect, render_hidden, force_two_sided, save_file } = self;
        Command::RenderSetup { width, height, image_aspect, pixel_aspect, lock_image_aspect, lock_pixel_aspect, render_hidden, force_two_sided, save_file, output: None }
    }
}

/// Bevy Renderer Settings arguments.
#[derive(Default)]
struct Bevy {
    antialiasing: Option<bool>,
    filter: Option<AaFilter>,
    filter_size: Option<f32>,
    supersampling: Option<u32>,
    passes: Option<u32>,
    shadows: Option<bool>,
    shadow_map_size: Option<u32>,
    area_shadows: Option<bool>,
    background: Option<[f32; 3]>,
    ambient: Option<[f32; 3]>,
    ambient_occlusion: Option<bool>,
    ao_quality: Option<AoQuality>,
    ao_radius: Option<f32>,
    reflections: Option<bool>,
    sky_lighting: Option<bool>,
    sky_intensity: Option<f32>,
    ground: Option<f32>,
    tonemapper: Option<Tonemapper>,
    exposure: Option<f32>,
    dither: Option<bool>,
}

impl Bevy {
    fn cmd(self) -> Command {
        let Bevy { antialiasing, filter, filter_size, supersampling, passes, shadows, shadow_map_size, area_shadows, background, ambient, ambient_occlusion, ao_quality, ao_radius, reflections, sky_lighting, sky_intensity, ground, tonemapper, exposure, dither } = self;
        Command::BevyRendererSettings { antialiasing, filter, filter_size, supersampling, passes, shadows, shadow_map_size, area_shadows, background, ambient, ambient_occlusion, ao_quality, ao_radius, reflections, sky_lighting, sky_intensity, ground, tonemapper, exposure, dither }
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

/// A renderer spinner: a drag is one undo step.
fn spin_f32(ui: &mut egui::Ui, app: &mut LoomApp, value: f32, range: std::ops::RangeInclusive<f32>, args: impl FnOnce(f32) -> Bevy) {
    let mut v = value;
    let r = ui.add(crate::spinner::spin(ui, &mut v, range).speed(0.01).max_decimals(3));
    merge_drag(app, &r);
    if r.changed() {
        run(app, args(v).cmd());
    }
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
    ui.label(egui::RichText::new("Antialiasing").strong());
    ui.horizontal(|ui| {
        let mut aa = b.antialiasing;
        if ui.checkbox(&mut aa, "Antialiasing").changed() {
            run(app, Bevy { antialiasing: Some(aa), ..Default::default() }.cmd());
        }
        ui.add_enabled_ui(b.antialiasing, |ui| {
            ui.label("Filter:");
            egui::ComboBox::from_id_salt("render_aa_filter").selected_text(b.filter.label()).show_ui(ui, |ui| {
                for f in AaFilter::ALL {
                    if ui.selectable_label(f == b.filter, f.label()).clicked() {
                        run(app, Bevy { filter: Some(f), ..Default::default() }.cmd());
                    }
                }
            });
            ui.label("Filter Size:");
            spin_f32(ui, app, b.filter_size, 1.0..=10.0, |v| Bevy { filter_size: Some(v), ..Default::default() });
        });
    });
    ui.horizontal(|ui| {
        ui.label("Supersampling:");
        egui::ComboBox::from_id_salt("render_supersampling").selected_text(format!("{}x{}", b.supersampling, b.supersampling)).show_ui(ui, |ui| {
            for n in 1..=4u32 {
                if ui.selectable_label(n == b.supersampling, format!("{n}x{n}")).clicked() {
                    run(app, Bevy { supersampling: Some(n), ..Default::default() }.cmd());
                }
            }
        });
        ui.label("Passes:");
        let mut passes = b.passes;
        let r = ui.add(crate::spinner::spin(ui, &mut passes, 1..=64u32));
        merge_drag(app, &r);
        if r.changed() {
            run(app, Bevy { passes: Some(passes), ..Default::default() }.cmd());
        }
    });
    ui.label(egui::RichText::new("Shadows").strong());
    ui.horizontal(|ui| {
        let mut sh = b.shadows;
        if ui.checkbox(&mut sh, "Shadows").changed() {
            run(app, Bevy { shadows: Some(sh), ..Default::default() }.cmd());
        }
        ui.add_enabled_ui(b.shadows, |ui| {
            ui.label("Map Size:");
            egui::ComboBox::from_id_salt("render_shadow_map").selected_text(b.shadow_map_size.to_string()).show_ui(ui, |ui| {
                for n in [512u32, 1024, 2048, 4096, 8192] {
                    if ui.selectable_label(n == b.shadow_map_size, n.to_string()).clicked() {
                        run(app, Bevy { shadow_map_size: Some(n), ..Default::default() }.cmd());
                    }
                }
            });
            let mut area = b.area_shadows;
            if ui.checkbox(&mut area, "Area Shadows").on_hover_text("Soft shadows from each light's Source Radius, over the passes").changed() {
                run(app, Bevy { area_shadows: Some(area), ..Default::default() }.cmd());
            }
        });
    });
    ui.label(egui::RichText::new("Global Illumination").strong());
    ui.horizontal(|ui| {
        let mut ao = b.ambient_occlusion;
        if ui.checkbox(&mut ao, "Ambient Occlusion").changed() {
            run(app, Bevy { ambient_occlusion: Some(ao), ..Default::default() }.cmd());
        }
        ui.add_enabled_ui(b.ambient_occlusion, |ui| {
            ui.label("Quality:");
            egui::ComboBox::from_id_salt("render_ao_quality").selected_text(b.ao_quality.label()).show_ui(ui, |ui| {
                for q in AoQuality::ALL {
                    if ui.selectable_label(q == b.ao_quality, q.label()).clicked() {
                        run(app, Bevy { ao_quality: Some(q), ..Default::default() }.cmd());
                    }
                }
            });
            ui.label("Radius:");
            spin_f32(ui, app, b.ao_radius, 0.001..=1.0e6, |v| Bevy { ao_radius: Some(v), ..Default::default() });
        });
    });
    ui.horizontal(|ui| {
        let mut refl = b.reflections;
        if ui.checkbox(&mut refl, "Reflections").changed() {
            run(app, Bevy { reflections: Some(refl), ..Default::default() }.cmd());
        }
        let mut sky = b.sky_lighting;
        if ui.checkbox(&mut sky, "Sky Lighting").on_hover_text("The ambient light comes from a sky dome that glossy surfaces reflect").changed() {
            run(app, Bevy { sky_lighting: Some(sky), ..Default::default() }.cmd());
        }
        ui.add_enabled_ui(b.sky_lighting, |ui| {
            ui.label("Intensity:");
            spin_f32(ui, app, b.sky_intensity, 0.0..=10.0, |v| Bevy { sky_intensity: Some(v), ..Default::default() });
            ui.label("Ground:");
            spin_f32(ui, app, b.ground, 0.0..=1.0, |v| Bevy { ground: Some(v), ..Default::default() });
        });
    });
    ui.label(egui::RichText::new("Exposure Control").strong());
    ui.horizontal(|ui| {
        ui.label("Tone Mapping:");
        egui::ComboBox::from_id_salt("render_tonemapper").selected_text(b.tonemapper.label()).show_ui(ui, |ui| {
            for t in Tonemapper::ALL {
                if ui.selectable_label(t == b.tonemapper, t.label()).clicked() {
                    run(app, Bevy { tonemapper: Some(t), ..Default::default() }.cmd());
                }
            }
        });
        ui.label("Exposure:");
        spin_f32(ui, app, b.exposure, -10.0..=10.0, |v| Bevy { exposure: Some(v), ..Default::default() });
        let mut dither = b.dither;
        if ui.checkbox(&mut dither, "Dither").changed() {
            run(app, Bevy { dither: Some(dither), ..Default::default() }.cmd());
        }
    });
    ui.label(egui::RichText::new("Environment").strong());
    ui.horizontal(|ui| {
        // The settings hold sRGB 0-1, as the swatches show them.
        let to_u8 = |c: [f32; 3]| c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
        let to_f = |c: [u8; 3]| c.map(|v| v as f32 / 255.0);
        let mut bg = to_u8(b.background);
        ui.label("Background:");
        if ui.color_edit_button_srgb(&mut bg).changed() {
            run(app, Bevy { background: Some(to_f(bg)), ..Default::default() }.cmd());
        }
        let mut amb = to_u8(b.ambient);
        ui.label("Ambient:");
        if ui.color_edit_button_srgb(&mut amb).changed() {
            run(app, Bevy { ambient: Some(to_f(amb)), ..Default::default() }.cmd());
        }
    });
}
