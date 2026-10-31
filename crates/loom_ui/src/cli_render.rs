//! Command-line rendering: `loom --render scene.loom [--camera NAME]
//! [--output FILE] [--size WxH] [--frames A[-B]] [--preset FILE.rps]`
//! opens the scene without a window, renders it on the GPU as a one-task
//! Batch Render and writes the files.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

/// The switches of a command-line render.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RenderArgs {
    pub scene: String,
    pub camera: Option<String>,
    pub output: Option<String>,
    pub size: Option<[u32; 2]>,
    pub frames: Option<[i32; 2]>,
    pub preset: Option<String>,
}

pub const USAGE: &str = "loom --render SCENE.loom [--camera NAME] [--output FILE] [--size WxH] [--frames A or A-B] [--preset FILE.rps]";

/// The render switches in `args` (program name excluded); None without `--render`.
pub fn parse(args: &[String]) -> Option<Result<RenderArgs, String>> {
    args.iter().position(|a| a == "--render")?;
    Some(parse_all(args))
}

fn parse_all(args: &[String]) -> Result<RenderArgs, String> {
    let mut r = RenderArgs::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().cloned().ok_or_else(|| format!("{a} needs a value ({USAGE})"));
        match a.as_str() {
            "--render" => r.scene = value()?,
            "--camera" => r.camera = Some(value()?),
            "--output" => r.output = Some(value()?),
            "--preset" => r.preset = Some(value()?),
            "--size" => {
                let v = value()?;
                let (w, h) = v.split_once(['x', 'X']).ok_or_else(|| format!("--size {v}: WIDTHxHEIGHT"))?;
                let n = |s: &str| s.trim().parse::<u32>().map_err(|_| format!("--size {v}: WIDTHxHEIGHT"));
                r.size = Some([n(w)?, n(h)?]);
            }
            "--frames" => {
                let v = value()?;
                let n = |s: &str| s.trim().parse::<i32>().map_err(|_| format!("--frames {v}: a frame or a range A-B"));
                let split = v.char_indices().skip(1).find(|(_, c)| *c == '-').map(|(i, _)| i);
                r.frames = Some(match split {
                    Some(i) => [n(&v[..i])?, n(&v[i + 1..])?],
                    None => [n(&v)?; 2],
                });
            }
            other => return Err(format!("unknown switch {other} ({USAGE})")),
        }
    }
    if r.scene.is_empty() {
        return Err(format!("--render needs a scene file ({USAGE})"));
    }
    Ok(r)
}

fn ok(app: &mut LoomApp, cmd: Command) -> Result<serde_json::Value, String> {
    app.dispatch(cmd).map_err(|e| e.to_string())
}

/// Renders `args` without a window; the files written, from the render log.
pub fn run(args: &RenderArgs) -> Result<Vec<String>, String> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor { backends: wgpu::Backends::PRIMARY, ..Default::default() });
    let rs = pollster::block_on(egui_wgpu::RenderState::create(&crate::wgpu_configuration(), &instance, None, None, 1, false)).map_err(|e| format!("no GPU renderer: {e}"))?;
    if !loom_bevy::supported(&rs.adapter) {
        return Err("no GPU renderer: the adapter lacks what the renderer needs".into());
    }
    let mut app = LoomApp::headless();
    app.gpu = crate::realistic::Gpu::Ready(rs);
    ok(&mut app, Command::OpenFile { path: Some(args.scene.clone()) })?;
    if let Some(p) = &args.preset {
        ok(&mut app, Command::LoadRenderPreset { path: p.clone(), common: None, renderer: None, elements: None })?;
    }
    let camera = match &args.camera {
        Some(name) => {
            let n = app.state.scene.nodes().find(|n| n.name == *name && matches!(n.object, loom_scene::ObjectKind::Camera { .. })).ok_or_else(|| format!("--camera {name}: no camera of that name"))?;
            Some(n.id.0)
        }
        None => None,
    };
    let s = app.state.scene.render.clone();
    let output = args.output.clone().or_else(|| s.save_file.then(|| s.output_path.clone()).flatten()).ok_or("no output file: give --output (or save the scene with Render Output > Save File)")?;
    let [w, h] = args.size.unwrap_or([s.width, s.height]);
    // One Batch Render task: the camera, the output, and the frames and size overriding the scene's.
    let task = ok(&mut app, Command::BatchRenderAdd { duplicate: None })?["index"].as_u64().unwrap_or(0) as u32;
    let frames = args.frames.unwrap_or_else(|| s.output_frames(app.state.frame_range, app.state.current_frame).map(|f| [f[0], *f.last().unwrap_or(&f[0])]).unwrap_or([app.state.current_frame; 2]));
    let override_preset = args.frames.is_some() || args.size.is_some();
    ok(&mut app, Command::BatchRenderTask { index: task, name: Some("Command Line".into()), enabled: Some(true), camera: Some(camera.unwrap_or(0)), preset: None, override_preset: Some(override_preset), frames: Some(frames), width: Some(w), height: Some(h), pixel_aspect: None, output: Some(output) })?;
    // Only this task renders.
    let n = app.state.scene.render.batch.len() as u32;
    for i in (0..n).filter(|i| *i != task) {
        ok(&mut app, Command::BatchRenderTask { index: i, name: None, enabled: Some(false), camera: None, preset: None, override_preset: None, frames: None, width: None, height: None, pixel_aspect: None, output: None })?;
    }
    ok(&mut app, Command::BatchRenderRun { confirm: Some(true), update_path: None })?;
    let ctx = egui::Context::default();
    let started = std::time::Instant::now();
    let mut time = 0.0;
    loop {
        time += 1.0 / 30.0;
        let mut input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))), time: Some(time), ..Default::default() };
        app.raw_input_hook(&ctx, &mut input);
        let _ = ctx.run(input, |ctx| app.frame(ctx));
        let r = &app.state.rendered;
        if r.progress.is_none() && app.state.pending_render.is_none() && r.batch.run.is_none() && app.render_job.is_none() {
            break;
        }
        if started.elapsed() > std::time::Duration::from_secs(3600) {
            return Err("the render did not finish in an hour".into());
        }
    }
    let errors: Vec<String> = app.state.rendered.messages.iter().filter(|m| m.level == loom_commands::render::MessageLevel::Error).map(|m| m.text.clone()).collect();
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    Ok(app.state.rendered.messages.iter().filter_map(|m| m.text.split_once("saved to ").map(|(_, p)| p.to_owned())).collect())
}
