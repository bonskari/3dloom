//! Render Setup (F10): the Common panel (Time Output, Output Size, Options,
//! Render Output, Assign Renderer) and the Bevy renderer's panel, with the
//! dialog's bottom controls (Production / Iterative, Viewport, Lock View,
//! Render). Every change is a command, one undo step (a spinner drag is one).

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;
use loom_scene::render::{AaFilter, AdvancedPlugin, AoQuality, ColorRange, TimeOutput, Tonemapper};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Panel {
    #[default]
    Common,
    Renderer,
    Elements,
    Raytracer,
    AdvancedLighting,
}

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
    let window = egui::Window::new("Render Setup").resizable(false).default_width(420.0);
    crate::os_window::OsWindow::new("Render Setup", "Render Setup", [440.0, 640.0]).fixed().show(ctx, open, window, |ui| {
        ui.horizontal(|ui| {
            ui.label("Preset:");
            ui.menu_button("Presets", |ui| {
                if ui.button("Load Preset...").clicked() {
                    app.state.dialog = Some(loom_commands::Dialog::LoadRenderPreset);
                    ui.close_menu();
                }
                if ui.button("Save Preset...").clicked() {
                    app.state.dialog = Some(loom_commands::Dialog::SaveRenderPreset);
                    ui.close_menu();
                }
            });
        });
        ui.horizontal(|ui| {
            ui.selectable_value(&mut panel, Panel::Common, "Common");
            ui.selectable_value(&mut panel, Panel::Renderer, "Renderer");
            ui.selectable_value(&mut panel, Panel::Elements, "Render Elements");
            ui.selectable_value(&mut panel, Panel::Raytracer, "Raytracer");
            ui.selectable_value(&mut panel, Panel::AdvancedLighting, "Advanced Lighting");
        });
        ui.separator();
        match panel {
            Panel::Common => common(ui, app),
            Panel::Renderer => renderer(ui, app),
            Panel::Elements => elements(ui, app),
            Panel::Raytracer => raytracer(ui, app),
            Panel::AdvancedLighting => advanced_lighting(ui, app),
        }
        ui.separator();
        crate::rendered_frame::viewport_controls(ui, app, "render_setup_viewport", "Lock View");
        crate::rendered_frame::render_buttons(ui, app, "render_setup_mode", false);
    });
    ctx.data_mut(|d| d.insert_temp(panel_id, panel));
}

/// Render Time Output arguments.
#[derive(Default)]
struct Time {
    time_output: Option<TimeOutput>,
    range: Option<[i32; 2]>,
    frames: Option<String>,
    every_nth: Option<u32>,
    file_number_base: Option<i32>,
    skip_existing: Option<bool>,
    image_file_list: Option<bool>,
}

impl Time {
    fn cmd(self) -> Command {
        let Time { time_output, range, frames, every_nth, file_number_base, skip_existing, image_file_list } = self;
        Command::RenderTimeOutput { time_output, range, frames, every_nth, file_number_base, skip_existing, image_file_list }
    }
}

fn time_output(ui: &mut egui::Ui, app: &mut LoomApp, s: &loom_scene::render::RenderSettings) {
    ui.label(egui::RichText::new("Time Output").strong());
    let choose = |ui: &mut egui::Ui, app: &mut LoomApp, t: TimeOutput| {
        if ui.radio(s.time_output == t, t.label()).clicked() && s.time_output != t {
            run(app, Time { time_output: Some(t), ..Default::default() }.cmd());
        }
    };
    ui.horizontal(|ui| {
        choose(ui, app, TimeOutput::Single);
        ui.add_space(12.0);
        ui.label("Every Nth Frame:");
        let mut n = s.every_nth;
        let r = ui.add_enabled(matches!(s.time_output, TimeOutput::ActiveSegment | TimeOutput::Range), crate::spinner::spin(ui, &mut n, 1..=100_000u32));
        merge_drag(app, &r);
        if r.changed() {
            run(app, Time { every_nth: Some(n), ..Default::default() }.cmd());
        }
    });
    ui.horizontal(|ui| {
        choose(ui, app, TimeOutput::ActiveSegment);
        let (a, b) = app.state.frame_range;
        ui.label(format!("{a} To {b}"));
    });
    ui.horizontal(|ui| {
        choose(ui, app, TimeOutput::Range);
        let [mut a, mut b] = s.range;
        let r = ui.add(crate::spinner::spin(ui, &mut a, -100_000..=100_000i32));
        merge_drag(app, &r);
        if r.changed() {
            run(app, Time { range: Some([a, b.max(a)]), ..Default::default() }.cmd());
        }
        ui.label("To");
        let r = ui.add(crate::spinner::spin(ui, &mut b, -100_000..=100_000i32));
        merge_drag(app, &r);
        if r.changed() {
            run(app, Time { range: Some([a.min(b), b]), ..Default::default() }.cmd());
        }
        ui.label("File Number Base:");
        let mut base = s.file_number_base;
        let r = ui.add_enabled(matches!(s.time_output, TimeOutput::ActiveSegment | TimeOutput::Range), crate::spinner::spin(ui, &mut base, -99_999..=99_999i32));
        merge_drag(app, &r);
        if r.changed() {
            run(app, Time { file_number_base: Some(base), ..Default::default() }.cmd());
        }
    });
    ui.horizontal(|ui| {
        choose(ui, app, TimeOutput::Frames);
        // Applied when the field loses focus (an unfinished list is not an error yet).
        let id = egui::Id::new("render_setup_frames");
        let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| s.frames.clone());
        let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(140.0));
        if r.lost_focus() {
            if text != s.frames {
                if let Err(e) = app.dispatch(Time { frames: Some(text.clone()), ..Default::default() }.cmd()) {
                    app.last_error = Some(e.to_string());
                }
            }
            ui.data_mut(|d| d.remove::<String>(id));
        } else if r.has_focus() {
            ui.data_mut(|d| d.insert_temp(id, text));
        }
    });
}

fn common(ui: &mut egui::Ui, app: &mut LoomApp) {
    let s = app.state.scene.render.clone();
    time_output(ui, app, &s);
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Output Size").strong());
    ui.horizontal(|ui| {
        let size_format = |format: Option<String>, aperture_width: Option<f32>| Command::OutputSizeFormat { format, aperture_width };
        let cur = s.film_format.clone().unwrap_or_else(|| "Custom".into());
        egui::ComboBox::from_id_salt("render_setup_format").selected_text(&cur).width(230.0).show_ui(ui, |ui| {
            if ui.selectable_label(s.film_format.is_none(), "Custom").clicked() {
                run(app, size_format(Some("Custom".into()), None));
            }
            for f in loom_scene::render::FILM_FORMATS {
                if ui.selectable_label(cur == f.name, f.name).clicked() {
                    run(app, size_format(Some(f.name.into()), None));
                }
            }
        });
        ui.label("Aperture Width(mm):");
        if s.film_format.is_some() {
            ui.label(format!("{:.3}", s.aperture_width));
        } else {
            let mut a = s.aperture_width;
            let r = ui.add(crate::spinner::spin(ui, &mut a, 0.1..=1000.0f32).speed(0.1).max_decimals(3));
            merge_drag(app, &r);
            if r.changed() {
                run(app, size_format(None, Some(a)));
            }
        }
    });
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
        let presets = app.state.prefs.size_presets;
        for (i, (pw, ph, pa)) in presets.into_iter().enumerate() {
            let r = ui.selectable_label((s.width, s.height) == (pw, ph), format!("{pw}x{ph}"));
            if r.clicked() {
                run(app, Setup { width: Some(pw), height: Some(ph), pixel_aspect: (pa != s.pixel_aspect).then_some(pa), ..Default::default() }.cmd());
            }
            // Right-click: Configure Preset.
            r.context_menu(|ui| {
                ui.label("Configure Preset");
                let configure = |app: &mut LoomApp, width, height, pixel_aspect, get_current| run(app, Command::ConfigurePreset { button: i as u32, width, height, pixel_aspect, get_current });
                let (mut w, mut h, mut a) = (pw, ph, pa);
                egui::Grid::new(("configure_preset", i)).num_columns(2).show(ui, |ui| {
                    ui.label("Width:");
                    if ui.add(egui::DragValue::new(&mut w).range(1..=loom_commands::render::MAX_RENDER_SIZE)).changed() {
                        configure(app, Some(w), None, None, None);
                    }
                    ui.end_row();
                    ui.label("Height:");
                    if ui.add(egui::DragValue::new(&mut h).range(1..=loom_commands::render::MAX_RENDER_SIZE)).changed() {
                        configure(app, None, Some(h), None, None);
                    }
                    ui.end_row();
                    ui.label("Pixel Aspect:");
                    if ui.add(egui::DragValue::new(&mut a).range(0.001..=1000.0).speed(0.01)).changed() {
                        configure(app, None, None, Some(a), None);
                    }
                    ui.end_row();
                });
                if ui.button("Get Current Settings").clicked() {
                    configure(app, None, None, None, Some(true));
                }
            });
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
    ui.horizontal(|ui| {
        let mut points = s.area_as_points;
        if ui.checkbox(&mut points, "Area Lights/Shadows as Points").on_hover_text("Soft shadows render sharp, from each light's centre").changed() {
            run(app, Opts { area_as_points: Some(points), ..Default::default() }.cmd());
        }
        let mut use_al = s.advanced.use_advanced;
        if ui.checkbox(&mut use_al, "Use Advanced Lighting").on_hover_text("Renders trace the Advanced Lighting panel's Light Tracer").changed() {
            run(app, Light { use_advanced: Some(use_al), ..Default::default() }.cmd());
        }
    });
    ui.horizontal(|ui| {
        let options = |video_color_check, super_black| Command::RenderOptions { video_color_check, super_black, super_black_threshold: None, force_wireframe: None, wire_thickness: None };
        let mut check = s.video_color_check;
        if ui.checkbox(&mut check, "Video Color Check").on_hover_text("Pixels beyond the NTSC limit render black").changed() {
            run(app, options(Some(check), None));
        }
        let mut black = s.super_black;
        if ui.checkbox(&mut black, "Super Black").on_hover_text("Rendered geometry is no darker than the Super Black threshold").changed() {
            run(app, options(None, Some(black)));
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
    output_setup(ui, app, &s);
    ui.horizontal(|ui| {
        let mut ifl = s.image_file_list;
        if ui.checkbox(&mut ifl, "Put Image File List in Output Path").on_hover_text("A sequence also writes a .ifl listing its files").changed() {
            run(app, Time { image_file_list: Some(ifl), ..Default::default() }.cmd());
        }
        let mut skip = s.skip_existing;
        if ui.checkbox(&mut skip, "Skip Existing Images").changed() {
            run(app, Time { skip_existing: Some(skip), ..Default::default() }.cmd());
        }
    });
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Assign Renderer").strong());
    ui.label("Production: Bevy (PBR rasteriser)");
}

/// Render Output File > Setup: the output format's options, and the Gamma group.
fn output_setup(ui: &mut egui::Ui, app: &mut LoomApp, s: &loom_scene::render::RenderSettings) {
    let ext = s.output_path.as_deref().and_then(|p| std::path::Path::new(p).extension()).and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    let f = &s.output_format;
    let none = Command::RenderOutputSetup { jpeg_quality: None, png_type: None, png_alpha: None, tga_bits: None, tga_compress: None, tga_alpha_split: None, tga_premultiplied: None, exr_full_float: None, exr_alpha: None };
    egui::CollapsingHeader::new("Setup (file format options)").id_salt("render_output_setup").show(ui, |ui| {
        let set = |app: &mut LoomApp, edit: &dyn Fn(&mut Command)| {
            let mut c = none.clone();
            edit(&mut c);
            run(app, c);
        };
        match ext.as_str() {
            "jpg" | "jpeg" => {
                ui.horizontal(|ui| {
                    ui.label("JPEG Quality:");
                    let mut q = f.jpeg_quality as u32;
                    if ui.add(egui::Slider::new(&mut q, 1..=100)).changed() {
                        set(app, &|c| if let Command::RenderOutputSetup { jpeg_quality, .. } = c { *jpeg_quality = Some(q) });
                    }
                });
            }
            "png" => {
                use loom_scene::render::PngType;
                ui.horizontal(|ui| {
                    for (t, label) in [(PngType::Rgb24, "RGB 24 bit"), (PngType::Rgb48, "RGB 48 bit"), (PngType::Gray8, "Grayscale 8 bit"), (PngType::Gray16, "Grayscale 16 bit")] {
                        if ui.radio(f.png_type == t, label).clicked() {
                            set(app, &|c| if let Command::RenderOutputSetup { png_type, .. } = c { *png_type = Some(t) });
                        }
                    }
                });
                let mut a = f.png_alpha;
                if ui.checkbox(&mut a, "Alpha Channel").changed() {
                    set(app, &|c| if let Command::RenderOutputSetup { png_alpha, .. } = c { *png_alpha = Some(a) });
                }
            }
            "tga" => {
                ui.horizontal(|ui| {
                    for b in [24u32, 32] {
                        if ui.radio(f.tga_bits == b, format!("{b} bits")).clicked() {
                            set(app, &|c| if let Command::RenderOutputSetup { tga_bits, .. } = c { *tga_bits = Some(b) });
                        }
                    }
                });
                ui.horizontal(|ui| {
                    let (mut z, mut split, mut pre) = (f.tga_compress, f.tga_alpha_split, f.tga_premultiplied);
                    if ui.checkbox(&mut z, "Compress").changed() {
                        set(app, &|c| if let Command::RenderOutputSetup { tga_compress, .. } = c { *tga_compress = Some(z) });
                    }
                    if ui.checkbox(&mut split, "Alpha Split").changed() {
                        set(app, &|c| if let Command::RenderOutputSetup { tga_alpha_split, .. } = c { *tga_alpha_split = Some(split) });
                    }
                    if ui.checkbox(&mut pre, "Pre-Multiplied Alpha").changed() {
                        set(app, &|c| if let Command::RenderOutputSetup { tga_premultiplied, .. } = c { *tga_premultiplied = Some(pre) });
                    }
                });
            }
            "exr" => {
                ui.horizontal(|ui| {
                    for (full, label) in [(false, "Half Float (16 bits/Channel)"), (true, "Full Float (32 bits/Channel)")] {
                        if ui.radio(f.exr_full_float == full, label).clicked() {
                            set(app, &|c| if let Command::RenderOutputSetup { exr_full_float, .. } = c { *exr_full_float = Some(full) });
                        }
                    }
                });
                let mut a = f.exr_alpha;
                if ui.checkbox(&mut a, "RGBA (off: RGB)").changed() {
                    set(app, &|c| if let Command::RenderOutputSetup { exr_alpha, .. } = c { *exr_alpha = Some(a) });
                }
            }
            _ => {
                ui.label("No options for this file type.");
            }
        }
        ui.separator();
        ui.label("Gamma");
        let g = &s.gamma;
        let gamma = |enabled, output_gamma, override_gamma, use_system| Command::GammaSettings { enabled, output_gamma, override_gamma, use_system };
        let mut on = g.enabled;
        if ui.checkbox(&mut on, "Enable Gamma Correction").changed() {
            run(app, gamma(Some(on), None, None, None));
        }
        ui.add_enabled_ui(g.enabled, |ui| {
            ui.horizontal(|ui| {
                if ui.radio(g.override_output.is_none(), format!("Use System Default Gamma ({:.2})", g.output)).clicked() {
                    run(app, gamma(None, None, None, Some(true)));
                }
                if ui.radio(g.override_output.is_some(), "Override").clicked() && g.override_output.is_none() {
                    run(app, gamma(None, None, Some(g.output), None));
                }
                if let Some(v) = g.override_output {
                    let mut v = v;
                    let r = ui.add(crate::spinner::spin(ui, &mut v, 0.01..=5.0f32).speed(0.01).max_decimals(2));
                    merge_drag(app, &r);
                    if r.changed() {
                        run(app, gamma(None, None, Some(v), None));
                    }
                }
            });
        });
    });
}

fn renderer(ui: &mut egui::Ui, app: &mut LoomApp) {
    let b = app.state.scene.render.bevy.clone();
    ui.label(egui::RichText::new("Bevy Renderer").strong());
    ui.label(egui::RichText::new("Image Quality").strong());
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
    ui.horizontal(|ui| {
        let s = &app.state.scene.render;
        let (wire, thickness) = (s.force_wireframe, s.wire_thickness);
        let options = |force_wireframe, wire_thickness| Command::RenderOptions { video_color_check: None, super_black: None, super_black_threshold: None, force_wireframe, wire_thickness };
        let mut on = wire;
        if ui.checkbox(&mut on, "Force Wireframe").on_hover_text("Every surface renders as its polygon edges").changed() {
            run(app, options(Some(on), None));
        }
        ui.label("Wire Thickness:");
        let mut t = thickness;
        let r = ui.add_enabled(wire, crate::spinner::spin(ui, &mut t, 0.1..=100.0f32).speed(0.05).max_decimals(2));
        merge_drag(app, &r);
        if r.changed() {
            run(app, options(None, Some(t)));
        }
    });
    ui.label(egui::RichText::new("Shadow Options").strong());
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
    ui.label(egui::RichText::new("Options").strong());
    ui.horizontal(|ui| {
        let mut auto = b.auto_reflect;
        if ui.checkbox(&mut auto, "Auto Reflect/Refract and Mirrors").on_hover_text("Off: Reflect/Refract and Flat Mirror maps are left out").changed() {
            run(app, Opts { auto_reflect: Some(auto), ..Default::default() }.cmd());
        }
        let mut fm = b.filter_maps;
        if ui.checkbox(&mut fm, "Filter Maps").on_hover_text("Bitmaps are filtered; off: the nearest texel").changed() {
            run(app, Opts { filter_maps: Some(fm), ..Default::default() }.cmd());
        }
    });
    ui.horizontal(|ui| {
        ui.label("Auto Reflect/Refract Maps Rendering Iterations:");
        let mut it = b.reflect_iterations;
        let r = ui.add(crate::spinner::spin(ui, &mut it, 1..=10u32));
        merge_drag(app, &r);
        if r.changed() {
            run(app, Opts { reflect_iterations: Some(it), ..Default::default() }.cmd());
        }
    });
    ui.label(egui::RichText::new("Global SuperSampling").strong());
    ui.horizontal(|ui| {
        let mut off = b.disable_all_samplers;
        if ui.checkbox(&mut off, "Disable all Samplers").changed() {
            run(app, Opts { disable_all_samplers: Some(off), ..Default::default() }.cmd());
        }
        let mut global = b.global_supersampler;
        if ui.add_enabled(!b.disable_all_samplers, egui::Checkbox::new(&mut global, "Enable Global Supersampler")).on_hover_text("The Supersampling factor applies").changed() {
            run(app, Opts { global_supersampler: Some(global), ..Default::default() }.cmd());
        }
    });
    ui.label(egui::RichText::new("Object Motion Blur").strong());
    ui.horizontal(|ui| {
        let mut on = b.object_motion_blur;
        if ui.checkbox(&mut on, "Apply").on_hover_text("Objects whose Motion Blur is Object render over the shutter").changed() {
            run(app, Opts { object_motion_blur: Some(on), ..Default::default() }.cmd());
        }
        ui.label("Duration (frames):");
        opts_spin(ui, app, b.object_blur_duration, 0.0..=100.0, |v| Opts { object_blur_duration: Some(v), ..Default::default() });
        ui.label("Samples:");
        let mut n = b.object_blur_samples;
        let r = ui.add(crate::spinner::spin(ui, &mut n, 1..=32u32));
        merge_drag(app, &r);
        if r.changed() {
            run(app, Opts { object_blur_samples: Some(n), ..Default::default() }.cmd());
        }
        ui.label("Duration Subdivisions:");
        let mut n = b.object_blur_subdivisions;
        let r = ui.add(crate::spinner::spin(ui, &mut n, 1..=32u32));
        merge_drag(app, &r);
        if r.changed() {
            run(app, Opts { object_blur_subdivisions: Some(n), ..Default::default() }.cmd());
        }
    });
    ui.label(egui::RichText::new("Image Motion Blur").strong());
    ui.horizontal(|ui| {
        let mut on = b.image_motion_blur;
        if ui.checkbox(&mut on, "Apply").on_hover_text("Objects whose Motion Blur is Image smear by their motion").changed() {
            run(app, Opts { image_motion_blur: Some(on), ..Default::default() }.cmd());
        }
        ui.label("Duration (frames):");
        opts_spin(ui, app, b.image_blur_duration, 0.0..=100.0, |v| Opts { image_blur_duration: Some(v), ..Default::default() });
        let mut env = b.image_blur_environment;
        if ui.checkbox(&mut env, "Apply to Environment Map").changed() {
            run(app, Opts { image_blur_environment: Some(env), ..Default::default() }.cmd());
        }
    });
    ui.label(egui::RichText::new("Color Range Limiting").strong());
    ui.horizontal(|ui| {
        for (c, label) in [(ColorRange::Clamp, "Clamp"), (ColorRange::Scale, "Scale")] {
            if ui.radio(b.color_range == c, label).clicked() && b.color_range != c {
                run(app, Opts { color_range: Some(c), ..Default::default() }.cmd());
            }
        }
    });
}

/// Renderer Options arguments.
#[derive(Default)]
struct Opts {
    area_as_points: Option<bool>,
    auto_reflect: Option<bool>,
    reflect_iterations: Option<u32>,
    filter_maps: Option<bool>,
    disable_all_samplers: Option<bool>,
    global_supersampler: Option<bool>,
    object_motion_blur: Option<bool>,
    object_blur_duration: Option<f32>,
    object_blur_samples: Option<u32>,
    object_blur_subdivisions: Option<u32>,
    image_motion_blur: Option<bool>,
    image_blur_duration: Option<f32>,
    image_blur_environment: Option<bool>,
    color_range: Option<ColorRange>,
}

impl Opts {
    fn cmd(self) -> Command {
        let Opts { area_as_points, auto_reflect, reflect_iterations, filter_maps, disable_all_samplers, global_supersampler, object_motion_blur, object_blur_duration, object_blur_samples, object_blur_subdivisions, image_motion_blur, image_blur_duration, image_blur_environment, color_range } = self;
        Command::RendererOptions { area_as_points, auto_reflect, reflect_iterations, filter_maps, disable_all_samplers, global_supersampler, object_motion_blur, object_blur_duration, object_blur_samples, object_blur_subdivisions, image_motion_blur, image_blur_duration, image_blur_environment, color_range }
    }
}

fn opts_spin(ui: &mut egui::Ui, app: &mut LoomApp, value: f32, range: std::ops::RangeInclusive<f32>, args: impl FnOnce(f32) -> Opts) {
    let mut v = value;
    let r = ui.add(crate::spinner::spin(ui, &mut v, range).speed(0.01).max_decimals(3));
    merge_drag(app, &r);
    if r.changed() {
        run(app, args(v).cmd());
    }
}

/// Raytracer Settings arguments.
#[derive(Default)]
struct Rt {
    enabled: Option<bool>,
    max_depth: Option<u32>,
    cutoff: Option<f32>,
    max_depth_color: Option<[f32; 3]>,
    max_depth_background: Option<bool>,
    antialias: Option<bool>,
    samples: Option<u32>,
    blur: Option<f32>,
    self_reflect: Option<bool>,
}

impl Rt {
    fn cmd(self) -> Command {
        let Rt { enabled, max_depth, cutoff, max_depth_color, max_depth_background, antialias, samples, blur, self_reflect } = self;
        Command::RaytracerSettings { enabled, max_depth, cutoff, max_depth_color, max_depth_background, antialias, samples, blur, self_reflect, exclude: None }
    }
}

fn rt_spin(ui: &mut egui::Ui, app: &mut LoomApp, value: f32, range: std::ops::RangeInclusive<f32>, args: impl FnOnce(f32) -> Rt) {
    let mut v = value;
    let r = ui.add(crate::spinner::spin(ui, &mut v, range).speed(0.01).max_decimals(3));
    merge_drag(app, &r);
    if r.changed() {
        run(app, args(v).cmd());
    }
}

/// Raytracer panel: Raytracer Global Parameters (Ray Depth Control, Global
/// Ray Antialiaser, Global Raytrace Engine Options, Exclude).
fn raytracer(ui: &mut egui::Ui, app: &mut LoomApp) {
    let r = app.state.scene.render.raytracer.clone();
    ui.label(egui::RichText::new("Raytracer Global Parameters").strong());
    ui.label(egui::RichText::new("Ray Depth Control").strong());
    ui.horizontal(|ui| {
        ui.label("Maximum Depth:");
        let mut d = r.max_depth;
        let resp = ui.add(crate::spinner::spin(ui, &mut d, 0..=100u32));
        merge_drag(app, &resp);
        if resp.changed() {
            run(app, Rt { max_depth: Some(d), ..Default::default() }.cmd());
        }
        ui.label("Cutoff Threshold:");
        rt_spin(ui, app, r.cutoff, 0.0..=1.0, |v| Rt { cutoff: Some(v), ..Default::default() });
    });
    ui.horizontal(|ui| {
        ui.label("Color to use at Max. Depth:");
        if ui.radio(r.max_depth_color.is_some(), "Specify").clicked() && r.max_depth_color.is_none() {
            run(app, Rt { max_depth_color: Some([0.0; 3]), ..Default::default() }.cmd());
        }
        if ui.radio(r.max_depth_color.is_none(), "Background").clicked() && r.max_depth_color.is_some() {
            run(app, Rt { max_depth_background: Some(true), ..Default::default() }.cmd());
        }
        if let Some(c) = r.max_depth_color {
            let mut rgb = c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
            if ui.color_edit_button_srgb(&mut rgb).changed() {
                run(app, Rt { max_depth_color: Some(rgb.map(|v| v as f32 / 255.0)), ..Default::default() }.cmd());
            }
        }
    });
    ui.label(egui::RichText::new("Global Ray Antialiaser").strong());
    ui.horizontal(|ui| {
        let mut on = r.antialias;
        if ui.checkbox(&mut on, "On").on_hover_text("Traced rays take several samples: Raytrace reflections and refractions blur by Blur").changed() {
            run(app, Rt { antialias: Some(on), ..Default::default() }.cmd());
        }
        ui.add_enabled_ui(r.antialias, |ui| {
            ui.label("Rays:");
            let mut n = r.samples;
            let resp = ui.add(crate::spinner::spin(ui, &mut n, 2..=64u32));
            merge_drag(app, &resp);
            if resp.changed() {
                run(app, Rt { samples: Some(n), ..Default::default() }.cmd());
            }
            ui.label("Blur:");
            rt_spin(ui, app, r.blur, 0.0..=45.0, |v| Rt { blur: Some(v), ..Default::default() });
        });
    });
    ui.label(egui::RichText::new("Global Raytrace Engine Options").strong());
    ui.horizontal(|ui| {
        let mut on = r.enabled;
        if ui.checkbox(&mut on, "Enable Raytracing").on_hover_text("Off: the renderer's screen-space approximations instead").changed() {
            run(app, Rt { enabled: Some(on), ..Default::default() }.cmd());
        }
        let mut sr = r.self_reflect;
        if ui.checkbox(&mut sr, "Enable Self Reflect/Refract").changed() {
            run(app, Rt { self_reflect: Some(sr), ..Default::default() }.cmd());
        }
    });
    ui.horizontal(|ui| {
        let names: Vec<String> = r.exclude.iter().filter_map(|id| app.state.scene.node(loom_scene::NodeId(*id)).map(|n| n.name.clone())).collect();
        ui.label(format!("Excluded: {}", if names.is_empty() { "none".to_owned() } else { names.join(", ") }));
        let selection: Vec<u64> = {
            let mut v: Vec<u64> = app.state.selection.iter().map(|n| n.0).collect();
            v.sort();
            v
        };
        if ui.add_enabled(!selection.is_empty(), egui::Button::new("Exclude Selected")).clicked() {
            let mut list = r.exclude.clone();
            list.extend(selection.iter().filter(|id| !r.exclude.contains(id)));
            run(app, Command::RaytracerSettings { enabled: None, max_depth: None, cutoff: None, max_depth_color: None, max_depth_background: None, antialias: None, samples: None, blur: None, self_reflect: None, exclude: Some(list) });
        }
        if ui.add_enabled(!r.exclude.is_empty(), egui::Button::new("Clear")).clicked() {
            run(app, Command::RaytracerSettings { enabled: None, max_depth: None, cutoff: None, max_depth_color: None, max_depth_background: None, antialias: None, samples: None, blur: None, self_reflect: None, exclude: Some(Vec::new()) });
        }
    });
}

/// Advanced Lighting arguments.
#[derive(Default)]
struct Light {
    plugin: Option<AdvancedPlugin>,
    active: Option<bool>,
    use_advanced: Option<bool>,
    global_multiplier: Option<f32>,
    object_multiplier: Option<f32>,
    sky_lights_on: Option<bool>,
    sky_lights: Option<f32>,
    color_bleed: Option<f32>,
    rays_per_sample: Option<u32>,
    color_filter: Option<[f32; 3]>,
    extra_ambient: Option<[f32; 3]>,
    filter_size: Option<f32>,
    ray_bias: Option<f32>,
    bounces: Option<u32>,
    cone_angle: Option<f32>,
}

impl Light {
    fn cmd(self) -> Command {
        let Light { plugin, active, use_advanced, global_multiplier, object_multiplier, sky_lights_on, sky_lights, color_bleed, rays_per_sample, color_filter, extra_ambient, filter_size, ray_bias, bounces, cone_angle } = self;
        Command::AdvancedLighting { plugin, active, use_advanced, global_multiplier, object_multiplier, sky_lights_on, sky_lights, color_bleed, rays_per_sample, color_filter, extra_ambient, filter_size, ray_bias, bounces, cone_angle }
    }
}

fn light_spin(ui: &mut egui::Ui, app: &mut LoomApp, value: f32, range: std::ops::RangeInclusive<f32>, args: impl FnOnce(f32) -> Light) {
    let mut v = value;
    let r = ui.add(crate::spinner::spin(ui, &mut v, range).speed(0.01).max_decimals(3));
    merge_drag(app, &r);
    if r.changed() {
        run(app, args(v).cmd());
    }
}

/// Advanced Lighting panel: Select Advanced Lighting (the plug-in, Active)
/// and the Light Tracer's Parameters.
fn advanced_lighting(ui: &mut egui::Ui, app: &mut LoomApp) {
    let a = app.state.scene.render.advanced.clone();
    ui.label(egui::RichText::new("Select Advanced Lighting").strong());
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("render_advanced_plugin").selected_text(a.plugin.label()).show_ui(ui, |ui| {
            for p in AdvancedPlugin::ALL {
                if ui.selectable_label(p == a.plugin, p.label()).clicked() {
                    run(app, Light { plugin: Some(p), ..Default::default() }.cmd());
                }
            }
        });
        let mut active = a.active;
        if ui.checkbox(&mut active, "Active").changed() {
            run(app, Light { active: Some(active), ..Default::default() }.cmd());
        }
    });
    if a.plugin != AdvancedPlugin::LightTracer {
        ui.label(egui::RichText::new("Radiosity is not available: choose the Light Tracer.").small().weak());
        return;
    }
    let to_u8 = |c: [f32; 3]| c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    let to_f = |c: [u8; 3]| c.map(|v| v as f32 / 255.0);
    ui.label(egui::RichText::new("Parameters").strong());
    egui::Grid::new("light_tracer_params").num_columns(4).show(ui, |ui| {
        ui.label("Global Multiplier:");
        light_spin(ui, app, a.global_multiplier, 0.0..=1000.0, |v| Light { global_multiplier: Some(v), ..Default::default() });
        ui.label("Object Mult.:");
        light_spin(ui, app, a.object_multiplier, 0.0..=1000.0, |v| Light { object_multiplier: Some(v), ..Default::default() });
        ui.end_row();
        let mut sky = a.sky_lights_on;
        if ui.checkbox(&mut sky, "Sky Lights:").changed() {
            run(app, Light { sky_lights_on: Some(sky), ..Default::default() }.cmd());
        }
        light_spin(ui, app, a.sky_lights, 0.0..=1000.0, |v| Light { sky_lights: Some(v), ..Default::default() });
        ui.label("Color Bleed:");
        light_spin(ui, app, a.color_bleed, 0.0..=1000.0, |v| Light { color_bleed: Some(v), ..Default::default() });
        ui.end_row();
        ui.label("Rays/Sample:");
        let mut rays = a.rays_per_sample;
        let r = ui.add(crate::spinner::spin(ui, &mut rays, 1..=10000u32));
        merge_drag(app, &r);
        if r.changed() {
            run(app, Light { rays_per_sample: Some(rays), ..Default::default() }.cmd());
        }
        ui.label("Color Filter:");
        let mut cf = to_u8(a.color_filter);
        if ui.color_edit_button_srgb(&mut cf).changed() {
            run(app, Light { color_filter: Some(to_f(cf)), ..Default::default() }.cmd());
        }
        ui.end_row();
        ui.label("Filter Size:");
        light_spin(ui, app, a.filter_size, 0.0..=100.0, |v| Light { filter_size: Some(v), ..Default::default() });
        ui.label("Extra Ambient:");
        let mut ea = to_u8(a.extra_ambient);
        if ui.color_edit_button_srgb(&mut ea).changed() {
            run(app, Light { extra_ambient: Some(to_f(ea)), ..Default::default() }.cmd());
        }
        ui.end_row();
        ui.label("Ray Bias:");
        light_spin(ui, app, a.ray_bias, 0.0..=100.0, |v| Light { ray_bias: Some(v), ..Default::default() });
        ui.label("Bounces:");
        let mut b = a.bounces;
        let r = ui.add(crate::spinner::spin(ui, &mut b, 0..=10u32));
        merge_drag(app, &r);
        if r.changed() {
            run(app, Light { bounces: Some(b), ..Default::default() }.cmd());
        }
        ui.end_row();
        ui.label("Cone Angle:");
        light_spin(ui, app, a.cone_angle, 33.0..=90.0, |v| Light { cone_angle: Some(v), ..Default::default() });
        ui.end_row();
    });
}

/// Render Element Parameters for the selected entry; omitted fields keep their value.
fn element_cmd(index: usize, f: impl FnOnce(&mut (Option<String>, Option<bool>, Option<String>, Option<f32>, Option<f32>))) -> Command {
    let mut a = (None, None, None, None, None);
    f(&mut a);
    Command::RenderElementParameters { index: index as u32, name: a.0, enabled: a.1, path: a.2, z_min: a.3, z_max: a.4 }
}

/// A text field applied when it loses focus.
fn text_field(ui: &mut egui::Ui, id: egui::Id, value: &str, width: f32) -> Option<String> {
    let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| value.to_owned());
    let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(width));
    if r.lost_focus() {
        ui.data_mut(|d| d.remove::<String>(id));
        return (text != value).then_some(text);
    }
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text));
    }
    None
}

/// Render Elements panel: Elements Active, Display Elements, Add / Delete,
/// the list, and the selected element's parameters.
/// The element's own rollout: Diffuse's Lighting, Lighting's parts, the HDR
/// elements' Scale Factor, Matte's ID and objects, Blend's parts.
fn element_options(ui: &mut egui::Ui, app: &mut LoomApp, index: usize, e: &loom_scene::render::RenderElement) {
    use loom_scene::render::ElementKind as K;
    let base = Command::RenderElementOptions { index: index as u32, lighting: None, direct: None, indirect: None, shadows: None, scale: None, matte_gbuffer: None, matte_objects: None, matte_exclude: None, blend_parts: None };
    let set = |app: &mut LoomApp, edit: &dyn Fn(&mut Command)| {
        let mut c = base.clone();
        edit(&mut c);
        run(app, c);
    };
    match e.kind {
        K::Diffuse => {
            let mut v = e.lighting;
            if ui.checkbox(&mut v, "Lighting").on_hover_text("Off: the diffuse colour and maps before lighting").changed() {
                set(app, &|c| if let Command::RenderElementOptions { lighting, .. } = c { *lighting = Some(v) });
            }
        }
        K::Lighting => {
            ui.horizontal(|ui| {
                let (mut d, mut i, mut s) = (e.direct, e.indirect, e.shadows);
                if ui.checkbox(&mut d, "Direct Light On").changed() {
                    set(app, &|c| if let Command::RenderElementOptions { direct, .. } = c { *direct = Some(d) });
                }
                if ui.checkbox(&mut i, "Indirect Light On").changed() {
                    set(app, &|c| if let Command::RenderElementOptions { indirect, .. } = c { *indirect = Some(i) });
                }
                if ui.checkbox(&mut s, "Shadows On").changed() {
                    set(app, &|c| if let Command::RenderElementOptions { shadows, .. } = c { *shadows = Some(s) });
                }
            });
        }
        K::IlluminanceHdr | K::LuminanceHdr => {
            ui.horizontal(|ui| {
                ui.label("Scale Factor:");
                let mut v = e.scale;
                let r = ui.add(crate::spinner::spin(ui, &mut v, 0.0001..=1000.0f32).speed(0.01).max_decimals(4));
                merge_drag(app, &r);
                if r.changed() {
                    set(app, &|c| if let Command::RenderElementOptions { scale, .. } = c { *scale = Some(v) });
                }
            });
        }
        K::Matte => {
            ui.horizontal(|ui| {
                let mut on = e.matte_gbuffer.is_some();
                let mut id = e.matte_gbuffer.unwrap_or(0) as i32;
                let a = ui.checkbox(&mut on, "G-Buffer ID").changed();
                let b = ui.add_enabled(on, egui::DragValue::new(&mut id).range(0..=65535)).changed();
                if a || b {
                    let v = if on { id } else { -1 };
                    set(app, &|c| if let Command::RenderElementOptions { matte_gbuffer, .. } = c { *matte_gbuffer = Some(v) });
                }
            });
            ui.horizontal(|ui| {
                let sel: Vec<u64> = app.state.selection.iter().map(|n| n.0).collect();
                if ui.button("Include Selected").on_hover_text("The selected objects render white").clicked() {
                    set(app, &|c| if let Command::RenderElementOptions { matte_objects, matte_exclude, .. } = c { (*matte_objects, *matte_exclude) = (Some(sel.clone()), Some(false)) });
                }
                if ui.button("Exclude Selected").on_hover_text("Everything is white but the selected objects").clicked() {
                    set(app, &|c| if let Command::RenderElementOptions { matte_objects, matte_exclude, .. } = c { (*matte_objects, *matte_exclude) = (Some(sel.clone()), Some(true)) });
                }
                ui.label(format!("{} object(s) {}", e.matte_objects.len(), if e.matte_exclude { "excluded" } else { "included" }));
            });
        }
        K::Blend => {
            ui.horizontal_wrapped(|ui| {
                let mut b = e.blend;
                for name in loom_scene::render::BlendParts::NAMES {
                    let v = b.get_mut(name).expect("a Blend part");
                    let mut on = *v;
                    if ui.checkbox(&mut on, name).changed() {
                        *v = on;
                        let mut parts: Vec<String> = Vec::new();
                        let mut all = b;
                        for n in loom_scene::render::BlendParts::NAMES {
                            if *all.get_mut(n).expect("a Blend part") {
                                parts.push(n.to_owned());
                            }
                        }
                        set(app, &|c| if let Command::RenderElementOptions { blend_parts, .. } = c { *blend_parts = Some(parts.clone()) });
                        break;
                    }
                }
            });
        }
        _ => {}
    }
}

fn elements(ui: &mut egui::Ui, app: &mut LoomApp) {
    use loom_scene::render::ElementKind;
    let s = app.state.scene.render.clone();
    let sel_id = egui::Id::new("render_elements_selected");
    let mut sel: usize = ui.data(|d| d.get_temp(sel_id)).unwrap_or(0);
    ui.horizontal(|ui| {
        let mut active = s.elements_active;
        if ui.checkbox(&mut active, "Elements Active").changed() {
            run(app, Command::RenderElements { active: Some(active), display: None });
        }
        let mut display = s.display_elements;
        if ui.checkbox(&mut display, "Display Elements").changed() {
            run(app, Command::RenderElements { active: None, display: Some(display) });
        }
    });
    ui.horizontal(|ui| {
        ui.menu_button("Add...", |ui| {
            for k in ElementKind::ALL {
                if ui.button(k.label()).clicked() {
                    if let Ok(r) = app.dispatch(Command::AddRenderElement { kind: k, name: None }) {
                        sel = r["index"].as_u64().unwrap_or(0) as usize;
                    }
                    ui.close_menu();
                }
            }
        });
        if ui.add_enabled(sel < s.elements.len(), egui::Button::new("Delete")).clicked() {
            run(app, Command::DeleteRenderElement { index: sel as u32 });
        }
    });
    egui::Grid::new("render_elements_list").num_columns(3).striped(true).show(ui, |ui| {
        ui.strong("Name");
        ui.strong("Enabled");
        ui.strong("Path");
        ui.end_row();
        for (i, e) in s.elements.iter().enumerate() {
            if ui.selectable_label(i == sel, &e.name).clicked() {
                sel = i;
            }
            ui.label(if e.enabled { "On" } else { "Off" });
            ui.label(e.output_path(s.output_path.as_deref()).unwrap_or_default());
            ui.end_row();
        }
    });
    if let Some(e) = s.elements.get(sel) {
        ui.separator();
        ui.label(egui::RichText::new("Selected Element Parameters").strong());
        ui.horizontal(|ui| {
            let mut on = e.enabled;
            if ui.checkbox(&mut on, "Enable").changed() {
                run(app, element_cmd(sel, |a| a.1 = Some(on)));
            }
            ui.label("Name:");
            if let Some(n) = text_field(ui, egui::Id::new(("render_element_name", sel)), &e.name, 140.0) {
                if let Err(err) = app.dispatch(element_cmd(sel, |a| a.0 = Some(n))) {
                    app.last_error = Some(err.to_string());
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("File:");
            if let Some(p) = text_field(ui, egui::Id::new(("render_element_path", sel)), e.path.as_deref().unwrap_or(""), 260.0) {
                run(app, element_cmd(sel, |a| a.2 = Some(p)));
            }
        });
        element_options(ui, app, sel, e);
        if e.kind == ElementKind::ZDepth {
            ui.horizontal(|ui| {
                ui.label("Z Min:");
                let mut z = e.z_min;
                let r = ui.add(crate::spinner::spin(ui, &mut z, -1.0e6..=1.0e6f32));
                merge_drag(app, &r);
                if r.changed() {
                    run(app, element_cmd(sel, |a| a.3 = Some(z)));
                }
                ui.label("Z Max:");
                let mut z = e.z_max;
                let r = ui.add(crate::spinner::spin(ui, &mut z, -1.0e6..=1.0e6f32));
                merge_drag(app, &r);
                if r.changed() {
                    run(app, element_cmd(sel, |a| a.4 = Some(z)));
                }
            });
        }
    }
    ui.data_mut(|d| d.insert_temp(sel_id, sel));
}
