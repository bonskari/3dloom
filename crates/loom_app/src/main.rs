// A GUI program on Windows: no console window behind it.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod commands;
mod control;
mod logging;
mod update;

use eframe::egui;
use eframe::NativeOptions;
use std::sync::mpsc::{Receiver, Sender};

fn main() -> eframe::Result<()> {
    // The log file and standard error (RUST_LOG, default warn: wgpu
    // validation errors are otherwise silent).
    logging::init();
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("3dLoom")
            .with_inner_size([1280.0, 800.0]),
        renderer: eframe::Renderer::Wgpu,
        // Needed by the custom 3D viewport callback; without this, meshes are
        // drawn purely in submission order and room walls/floor overlap wrong.
        depth_buffer: 24,
        // The Realistic (Bevy) renderer shares the window's device.
        wgpu_options: loom_ui::wgpu_configuration(),
        ..Default::default()
    };
    // Control sockets: always the user's own Unix socket; the localhost TCP
    // port only when asked for (`LOOM_CONTROL_PORT=7833 loom`).
    let (control_tx, control) = std::sync::mpsc::channel::<control::Request>();
    let waker = control::Waker::default();
    #[cfg(unix)]
    let unix_socket = match control::spawn_unix(&loom_commands::live::runtime_dir(), control_tx.clone(), waker.clone()) {
        Ok(s) => {
            log::info!(target: "loom::control", "control socket {}", s.path.display());
            Some(s)
        }
        Err(e) => {
            log::warn!(target: "loom::control", "control socket failed in {}: {e}", loom_commands::live::runtime_dir().display());
            None
        }
    };
    let tcp = match std::env::var("LOOM_CONTROL_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .filter(|p| *p != 0)
    {
        Some(port) => match control::spawn_tcp(port, control_tx, waker.clone()) {
            Ok(()) => {
                eprintln!("3dLoom control socket listening on 127.0.0.1:{port}");
                log::info!(target: "loom::control", "control port 127.0.0.1:{port}");
                true
            }
            Err(e) => {
                eprintln!("3dLoom control socket failed on port {port}: {e}");
                log::warn!(target: "loom::control", "control port {port} failed: {e}");
                false
            }
        },
        None => false,
    };
    let live = commands::Live {
        log_path: logging::path(),
        #[cfg(unix)]
        socket: unix_socket.as_ref().map(|s| s.path.clone()),
        ..Default::default()
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
        Box::new(move |cc| {
            let mut inner = loom_ui::LoomApp::new(cc);
            inner.update = update.clone();
            inner.open_command_line(std::env::args().skip(1));
            let _ = waker.set(cc.egui_ctx.clone());
            Ok(Box::new(ControlledApp {
                inner,
                restart_exe: exe,
                control,
                continuous_repaint: tcp,
                live,
                pending_input: Vec::new(),
                #[cfg(unix)]
                _unix_socket: unix_socket,
                pending_shot: None,
            }))
        }),
    )
}

/// Wraps `LoomApp` so control-socket requests are applied on the UI thread.
struct ControlledApp {
    inner: loom_ui::LoomApp,
    control: Receiver<control::Request>,
    /// TCP clients (LOOM_CONTROL_PORT) expect the view to keep advancing
    /// (view animations, buffer rebuilds) without user input.
    continuous_repaint: bool,
    /// Injected input, the log file and the socket path.
    live: commands::Live,
    /// `input` requests waiting for their events to be delivered.
    pending_input: Vec<(Sender<serde_json::Value>, usize)>,
    #[cfg(unix)]
    _unix_socket: Option<control::UnixSocket>,
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
        match &found {
            Ok(Some((m, _))) => log::info!(target: "loom::update", "update available: {} (running {own})", m.commit),
            Ok(None) => log::info!(target: "loom::update", "up to date ({own})"),
            Err(e) => log::warn!(target: "loom::update", "update check failed: {e}"),
        }
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
                    match &r {
                        Ok(()) => log::info!(target: "loom::update", "installed {}", manifest.commit),
                        Err(e) => log::warn!(target: "loom::update", "install of {} failed: {e}", manifest.commit),
                    }
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
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        // Injected events first, then the app's own hook (it ends interactions on focus loss).
        self.live.input.apply(raw_input);
        self.inner.raw_input_hook(ctx, raw_input);
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.deliver_screenshot(ctx);
        self.drain_requests(ctx);
        if self.continuous_repaint {
            ctx.request_repaint();
        }
        self.inner.update(ctx, frame);
        if self.live.input.is_empty() {
            for (reply, steps) in self.pending_input.drain(..) {
                let _ = reply.send(serde_json::json!({ "ok": true, "steps": steps }));
            }
        } else {
            ctx.request_repaint();
        }
    }
}

impl ControlledApp {
    fn drain_requests(&mut self, ctx: &egui::Context) {
        let requests: Vec<control::Request> = self.control.try_iter().collect();
        if !requests.is_empty() {
            // Let the view catch up with what the requests changed.
            ctx.request_repaint();
        }
        for req in requests {
            match commands::apply(&mut self.inner, ctx, &mut self.live, &req.body) {
                commands::Outcome::Done(v) => {
                    let _ = req.reply.send(v);
                }
                commands::Outcome::Input { steps } => {
                    ctx.request_repaint();
                    self.pending_input.push((req.reply, steps));
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
                    ctx.request_repaint();
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
