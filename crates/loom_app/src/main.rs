// A GUI program on Windows: no console window behind it.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod commands;
mod control;
mod update;

use eframe::egui;
use eframe::NativeOptions;
use std::sync::mpsc::{Receiver, Sender};

fn main() -> eframe::Result<()> {
    // RUST_LOG=warn shows wgpu validation errors, which are otherwise silent.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("3dLoom")
            .with_inner_size([1280.0, 800.0]),
        renderer: eframe::Renderer::Wgpu,
        // Needed by the custom 3D viewport callback; without this, meshes are
        // drawn purely in submission order and room walls/floor overlap wrong.
        depth_buffer: 24,
        ..Default::default()
    };
    // Opt-in remote control: `LOOM_CONTROL_PORT=7833 loom`.
    let control = match std::env::var("LOOM_CONTROL_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .filter(|p| *p != 0)
    {
        Some(port) => match control::spawn(port) {
            Ok(rx) => {
                eprintln!("3dLoom control socket listening on 127.0.0.1:{port}");
                Some(rx)
            }
            Err(e) => {
                eprintln!("3dLoom control socket failed on port {port}: {e}");
                None
            }
        },
        None => None,
    };
    // Signed self-update before any window opens (CI release builds only, see
    // `update::enabled`). A newer release replaces this executable and the
    // start continues in it; LOOM_UPDATED stops the new process checking again.
    if update::enabled(option_env!("LOOM_UPDATE_CHANNEL"), std::env::var("LOOM_CONTROL_PORT").ok().as_deref(), std::env::var("LOOM_NO_UPDATE").ok().as_deref())
        && std::env::var_os("LOOM_UPDATED").is_none()
    {
        let exe = update::own_path();
        match update::run(env!("LOOM_COMMIT")) {
            update::Status::Updated(commit) => {
                if let Some(exe) = exe {
                    update::relaunch(&exe, &commit);
                }
            }
            update::Status::Failed(e) => eprintln!("3dLoom update check failed: {e}"),
            _ => {}
        }
    }
    eframe::run_native(
        "3dLoom",
        options,
        Box::new(|cc| {
            Ok(Box::new(ControlledApp {
                inner: loom_ui::LoomApp::new(cc),
                control,
                pending_shot: None,

            }))
        }),
    )
}

/// Wraps `LoomApp` so control-socket requests are applied on the UI thread.
struct ControlledApp {
    inner: loom_ui::LoomApp,
    control: Option<Receiver<control::Request>>,
    /// Screenshot requested last frame, waiting for the image event.
    pending_shot: Option<(String, Sender<serde_json::Value>)>,
}

impl eframe::App for ControlledApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.deliver_screenshot(ctx);
        if self.control.is_some() {
            self.drain_requests(ctx);
            // Control clients expect the view to keep advancing (view
            // animations, buffer rebuilds) without user input.
            ctx.request_repaint();
        }
        self.inner.update(ctx, frame);
    }
}

impl ControlledApp {
    fn drain_requests(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.control.as_ref() else {
            return;
        };
        let requests: Vec<control::Request> = rx.try_iter().collect();
        for req in requests {
            match commands::apply(&mut self.inner, &req.body) {
                commands::Outcome::Done(v) => {
                    let _ = req.reply.send(v);
                }
                commands::Outcome::Screenshot(shot) => {
                    if self.pending_shot.is_some() {
                        let _ = req.reply.send(serde_json::json!({
                            "ok": false,
                            "error": "a screenshot is already in flight"
                        }));
                        continue;
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                        egui::UserData::default(),
                    ));
                    self.pending_shot = Some((shot.path, req.reply));
                }
            }
        }
    }

    fn deliver_screenshot(&mut self, ctx: &egui::Context) {
        let Some((path, reply)) = self.pending_shot.take() else {
            return;
        };
        let image = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        let Some(image) = image else {
            // Not captured yet; wait another frame.
            self.pending_shot = Some((path, reply));
            ctx.request_repaint();
            return;
        };
        let reply_value = match save_png(&image, &path) {
            Ok(()) => serde_json::json!({
                "ok": true,
                "path": path,
                "width": image.size[0],
                "height": image.size[1],
            }),
            Err(e) => serde_json::json!({ "ok": false, "error": e }),
        };
        let _ = reply.send(reply_value);
    }
}

fn save_png(image: &egui::ColorImage, path: &str) -> Result<(), String> {
    let (w, h) = (image.size[0] as u32, image.size[1] as u32);
    let mut buf = Vec::with_capacity(image.pixels.len() * 4);
    for px in &image.pixels {
        buf.extend_from_slice(&px.to_array());
    }
    let img = image::RgbaImage::from_raw(w, h, buf).ok_or("pixel buffer size mismatch")?;
    img.save(path).map_err(|e| e.to_string())
}
