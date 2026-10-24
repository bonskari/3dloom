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
    // Signed self-update (CI release builds only, see `update::enabled`).
    let update = update::enabled(option_env!("LOOM_UPDATE_CHANNEL"), std::env::var("LOOM_CONTROL_PORT").ok().as_deref(), std::env::var("LOOM_NO_UPDATE").ok().as_deref()).then(|| {
        let status = std::sync::Arc::new(std::sync::Mutex::new(update::Status::Checking));
        let out = status.clone();
        std::thread::spawn(move || {
            let s = update::run(env!("LOOM_COMMIT"));
            if let update::Status::Failed(e) = &s {
                eprintln!("3dLoom update check failed: {e}");
            }
            *out.lock().unwrap() = s;
        });
        status
    });
    eframe::run_native(
        "3dLoom",
        options,
        Box::new(|cc| {
            Ok(Box::new(ControlledApp {
                inner: loom_ui::LoomApp::new(cc),
                control,
                pending_shot: None,
                update,
                update_dismissed: false,
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
    /// The self-update check running in the background, if this build updates itself.
    update: Option<std::sync::Arc<std::sync::Mutex<update::Status>>>,
    update_dismissed: bool,
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
        self.update_notice(ctx);
    }
}

impl ControlledApp {
    /// "Updated" in the corner once a new version is installed, with a button
    /// that opens it beside this window.
    fn update_notice(&mut self, ctx: &egui::Context) {
        if self.update_dismissed {
            return;
        }
        let Some(status) = self.update.as_ref().map(|s| s.lock().unwrap().clone()) else { return };
        match status {
            update::Status::Checking => ctx.request_repaint_after(std::time::Duration::from_secs(2)),
            update::Status::Updated(commit) => {
                egui::Window::new("3dLoom updated")
                    .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -60.0))
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.label(format!("Version {} is installed; it runs from the next start.", &commit[..commit.len().min(7)]));
                        ui.horizontal(|ui| {
                            // The new version opens beside this one; this window stays
                            // open until it is closed (and asks to save as usual).
                            if ui.button("Open new version").clicked() {
                                if let Ok(exe) = std::env::current_exe() {
                                    if std::process::Command::new(exe).args(std::env::args_os().skip(1)).spawn().is_ok() {
                                        self.update_dismissed = true;
                                    }
                                }
                            }
                            if ui.button("Later").clicked() {
                                self.update_dismissed = true;
                            }
                        });
                    });
            }
            update::Status::UpToDate | update::Status::Failed(_) => self.update_dismissed = true,
        }
    }

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
