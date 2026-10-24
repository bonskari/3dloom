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
    // Signed self-update (CI release builds only, see `update::enabled`): a
    // background check after the window opens feeds the Update menu; nothing
    // installs until the user picks it.
    let exe = update::own_path();
    let update = update::enabled(option_env!("LOOM_UPDATE_CHANNEL"), std::env::var("LOOM_CONTROL_PORT").ok().as_deref(), std::env::var("LOOM_NO_UPDATE").ok().as_deref()).then(|| {
        let handle = loom_ui::update_menu::new_handle();
        let worker = handle.clone();
        std::thread::spawn(move || update_worker(worker));
        handle
    });
    eframe::run_native(
        "3dLoom",
        options,
        Box::new(|cc| {
            let mut inner = loom_ui::LoomApp::new(cc);
            inner.update = update.clone();
            Ok(Box::new(ControlledApp {
                inner,
                restart_exe: exe,
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
    /// This program's path from before an update replaced it (for Restart now).
    restart_exe: Option<std::path::PathBuf>,
}

impl Drop for ControlledApp {
    /// Update > Restart now: once the window has really closed (unsaved
    /// changes were asked about), the installed version starts.
    fn drop(&mut self) {
        let restart = self.inner.update.as_ref().is_some_and(|h| {
            let s = h.lock().unwrap();
            s.restart_requested && matches!(s.status, loom_ui::update_menu::UpdateStatus::Ready { .. })
        });
        if let (true, Some(exe)) = (restart, self.restart_exe.as_ref()) {
            update::relaunch(exe);
        }
    }
}

/// The Update menu's background work: check (with the list of changes), then
/// install when asked; Check again / Try again re-run the check.
fn update_worker(h: loom_ui::update_menu::UpdateHandle) {
    use loom_ui::update_menu::UpdateStatus as S;
    let own = env!("LOOM_COMMIT");
    loop {
        let found = update::check(own);
        let pending = match found {
            Ok(Some((manifest, asset))) => {
                let changes = update::changes(own, &manifest.commit);
                h.lock().unwrap().status = S::Available { commit: manifest.commit.clone(), changes };
                Some((manifest, asset))
            }
            Ok(None) => {
                h.lock().unwrap().status = S::UpToDate;
                None
            }
            Err(e) => {
                h.lock().unwrap().status = S::Failed(e);
                None
            }
        };
        // Wait for the user.
        loop {
            std::thread::sleep(std::time::Duration::from_millis(250));
            let mut s = h.lock().unwrap();
            if s.check_requested {
                s.check_requested = false;
                s.status = S::Checking;
                break;
            }
            if s.install_requested {
                s.install_requested = false;
                if let Some((manifest, asset)) = pending.as_ref() {
                    s.status = S::Installing { commit: manifest.commit.clone() };
                    drop(s);
                    let r = update::install(manifest, asset);
                    let mut s = h.lock().unwrap();
                    s.status = match r {
                        Ok(()) => S::Ready { commit: manifest.commit.clone() },
                        Err(e) => S::Failed(e),
                    };
                }
            }
        }
    }
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
