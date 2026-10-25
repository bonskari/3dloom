//! Native motion authoring through a user-configured Kimodo GPU queue.
use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;
use serde_json::{json, Value};
use std::io::Read;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

pub(crate) struct Panel {
    prompt: String,
    duration: f32,
    status: String,
    pending: Option<Receiver<Event>>,
    ready: Option<std::path::PathBuf>,
}

impl Default for Panel {
    fn default() -> Self {
        Self { prompt: String::new(), duration: 5., status: String::new(), pending: None, ready: None }
    }
}

enum Event {
    Status(String),
    Connected,
    Motion { job: String, bytes: Vec<u8>, scene: u64, folder: std::path::PathBuf },
    Error(String),
}

fn base_url(text: &str) -> Result<url::Url, String> {
    let mut url = url::Url::parse(text.trim()).map_err(|_| "Enter a valid Kimodo API URL in Settings.".to_owned())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("Kimodo API URL must use http or https.".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("Use the API base URL without a query or fragment.".into());
    }
    let path = format!("{}/", url.path().trim_end_matches('/'));
    url.set_path(&path);
    Ok(url)
}

fn endpoint(base: &url::Url, path: &str) -> Result<url::Url, String> {
    let url = base.join(path.trim_start_matches('/')).map_err(|e| e.to_string())?;
    if url.origin() != base.origin() {
        return Err("Kimodo returned an artifact outside its API server.".into());
    }
    Ok(url)
}

fn response(agent: &ureq::Agent, base: &url::Url, path: &str, body: Option<&Value>) -> Result<ureq::Response, String> {
    let url = endpoint(base, path)?;
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let reply = match body {
            Some(body) => agent.post(url.as_str()).send_json(body.clone()),
            None => agent.get(url.as_str()).call(),
        };
        match reply {
            Ok(reply) => return Ok(reply),
            Err(ureq::Error::Status(503, reply)) if Instant::now() < deadline => {
                let seconds = reply.header("Retry-After").and_then(|v| v.parse::<u64>().ok()).unwrap_or(5).clamp(1, 30);
                std::thread::sleep(Duration::from_secs(seconds));
            }
            Err(ureq::Error::Status(503, _)) => return Err("GPU service is still busy or starting. Try again when it is available.".into()),
            Err(e) => return Err(format!("Kimodo API: {e}")),
        }
    }
}

fn request(agent: &ureq::Agent, base: &url::Url, path: &str, body: Option<Value>) -> Result<Value, String> {
    response(agent,base,path,body.as_ref())?.into_json().map_err(|e| format!("Kimodo API returned invalid JSON: {e}"))
}

fn generate(base: &url::Url, prompt: &str, duration: f32, scene: u64, folder: std::path::PathBuf, tx: &mpsc::Sender<Event>) -> Result<Event, String> {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(20)).build();
    let reply = request(&agent, base, "_queue/jobs", Some(json!({
        "backend":"kimodo", "priority":5, "label":"3D Loom motion",
        "steps":[{"type":"http", "method":"POST", "path":"/generate",
                  "body":{"prompt":prompt,"duration":duration,"seed":42}}]
    })))?;
    let job = reply["id"].as_str().filter(|id| !id.is_empty() && id.len() <= 64 && id.bytes().all(|c| c.is_ascii_hexdigit())).ok_or("Kimodo queue did not return a valid job ID.")?.to_owned();
    let deadline = Instant::now() + Duration::from_secs(1800);
    let mut previous = String::new();
    while Instant::now() < deadline {
        let state = request(&agent, base, &format!("_queue/jobs/{job}"), None)?;
        let phase = state["state"].as_str().ok_or("Kimodo queue did not return a job state.")?;
        if phase != previous {
            let _ = tx.send(Event::Status(format!("Kimodo: {phase}")));
            previous = phase.into();
        }
        match phase {
            "done" => {
                let result = &state["results"][0];
                if result["status"].as_u64() != Some(200) {
                    return Err(format!("Kimodo generation failed: {}", result["body"]));
                }
                let path = result["body"]["artifacts"]["bvh"].as_str()
                    .ok_or("Kimodo API needs artifact-download support before Loom can import its result.")?;
                let response = response(&agent,base,path,None)?;
                let mut bytes = Vec::new();
                response.into_reader().take(32 * 1024 * 1024 + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
                if bytes.len() > 32 * 1024 * 1024 { return Err("Kimodo BVH exceeds 32 MiB.".into()); }
                if bytes.is_empty() { return Err("Kimodo returned an empty animation.".into()); }
                return Ok(Event::Motion { job, bytes, scene, folder });
            }
            "failed" | "cancelled" => return Err(format!("Kimodo {phase}: {}", state["error"])),
            _ => std::thread::sleep(Duration::from_millis(500)),
        }
    }
    Err(format!("Kimodo is still pending. Queue job: {job}"))
}

fn start(app: &mut LoomApp, ctx: &egui::Context, connection_test: bool) {
    let base = match base_url(&app.state.prefs.kimodo_api_url) {
        Ok(base) => base,
        Err(e) => { app.kimodo.status = e; return; }
    };
    let folder = app.state.file.as_ref().and_then(|p| p.parent()).map(|p| p.to_path_buf())
        .or_else(loom_commands::settings::config_dir);
    let Some(folder) = folder else { app.kimodo.status = "No folder available for generated motion.".into(); return; };
    let scene = app.state.scene.instance_id();
    let (tx, rx) = mpsc::channel();
    app.kimodo.pending = Some(rx);
    app.kimodo.status = if connection_test { "Testing connection…" } else { "Submitting motion…" }.into();
    let prompt = app.kimodo.prompt.clone();
    let duration = app.kimodo.duration;
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let result = if connection_test {
            let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(10)).build();
            request(&agent, &base, "health", None).and_then(|info| {
                let supported = info["features"].as_array().is_some_and(|features| features.iter().any(|v| v == "artifacts-v1"));
                if supported { Ok(Event::Connected) } else { Err("This URL does not expose a Kimodo API with animation downloads.".into()) }
            })
        } else { generate(&base, &prompt, duration, scene, folder, &tx) };
        let _ = tx.send(result.unwrap_or_else(Event::Error));
        ctx.request_repaint();
    });
}

pub(crate) fn poll(app: &mut LoomApp, ctx: &egui::Context) {
    let events: Vec<_> = app.kimodo.pending.as_ref().map(|rx| rx.try_iter().collect()).unwrap_or_default();
    for event in events {
        match event {
            Event::Status(text) => app.kimodo.status = text,
            Event::Connected => { app.kimodo.status = "Connected to Kimodo API.".into(); app.kimodo.pending = None; }
            Event::Error(e) => { app.kimodo.status = e; app.kimodo.pending = None; }
            Event::Motion { job, bytes, scene, folder } => {
                app.kimodo.pending = None;
                let saved = (|| -> Result<std::path::PathBuf, String> {
                    let folder = folder.join("kimodo-motions");
                    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
                    let path = folder.join(format!("{job}.bvh"));
                    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
                    Ok(path)
                })();
                match saved {
                    Err(e) => app.kimodo.status = format!("Saving motion failed: {e}"),
                    Ok(path) if app.state.scene.instance_id() != scene => {
                        app.kimodo.status = "Motion ready for the previous scene. Choose Import Result to add it here.".into();
                        app.kimodo.ready = Some(path);
                    }
                    Ok(path) => import_result(app, path),
                }
            }
        }
    }
    if app.kimodo.pending.is_some() { ctx.request_repaint_after(Duration::from_millis(200)); }
}

fn import_result(app: &mut LoomApp, path: std::path::PathBuf) {
    let imported = (|| -> Result<(), String> {
        app.dispatch(Command::ImportFile { path: Some(path.to_string_lossy().into_owned()) })?;
        if app.state.display.hide_bones { app.dispatch(Command::HideBoneObjectsToggle)?; }
        app.dispatch(Command::ZoomExtentsSelected)?;
        if !app.state.playing { app.dispatch(Command::PlayAnimation)?; }
        Ok(())
    })();
    match imported {
        Ok(()) => { app.kimodo.status = "Animation imported. Playing on the timeline.".into(); app.kimodo.ready = None; }
        Err(e) => { app.kimodo.status = format!("Import failed: {e}"); app.kimodo.ready = Some(path); }
    }
}

pub(crate) fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    super::command_panel::rollout(ui, "Kimodo", true, |ui| {
        ui.add(egui::TextEdit::multiline(&mut app.kimodo.prompt).hint_text("Describe the motion…").desired_rows(3).desired_width(f32::INFINITY));
        ui.horizontal(|ui| {
            ui.label("Duration (seconds)");
            ui.add(egui::DragValue::new(&mut app.kimodo.duration).range(0.5..=30.).speed(0.1));
        });
        let busy = app.kimodo.pending.is_some();
        if ui.add_enabled(!busy && !app.kimodo.prompt.trim().is_empty(), egui::Button::new("Generate")).clicked() {
            start(app, ui.ctx(), false);
        }
        ui.collapsing("Settings", |ui| {
            ui.label("API URL");
            ui.add(egui::TextEdit::singleline(&mut app.state.prefs.kimodo_api_url).desired_width(f32::INFINITY));
            if ui.add_enabled(!busy, egui::Button::new("Test Connection")).clicked() { start(app, ui.ctx(), true); }
            ui.small("Saved on this computer.");
        });
        if let Some(path) = app.kimodo.ready.clone() {
            if ui.add_enabled(!busy, egui::Button::new("Import Result")).clicked() { import_result(app, path); }
        }
        if busy { ui.spinner(); }
        if !app.kimodo.status.is_empty() { ui.label(&app.kimodo.status); }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_address_is_user_configured_and_preserves_base_path() {
        assert!(base_url("").is_err());
        assert!(base_url("file:///tmp/model").is_err());
        assert!(base_url("https://example.org/?token=value").is_err());
        let base = base_url(" https://example.org/kimodo ").unwrap();
        assert_eq!(endpoint(&base,"/_queue/jobs").unwrap().as_str(), "https://example.org/kimodo/_queue/jobs");
        assert!(endpoint(&base,"https://other.example.org/motion.bvh").is_err());
    }
    #[test]
    #[ignore = "requires a running Kimodo GPU queue; set LOOM_KIMODO_API_URL"]
    fn real_queue_generation_downloads_imports_and_starts_playback() {
        let api = std::env::var("LOOM_KIMODO_API_URL").expect("LOOM_KIMODO_API_URL");
        let mut app = LoomApp::headless();
        app.state.prefs.kimodo_api_url = api;
        app.kimodo.prompt = "A man sits quietly in a chair, breathing gently, hands resting on his thighs.".into();
        app.kimodo.duration = 5.;
        let folder = std::env::temp_dir().join(format!("loom-kimodo-live-{}",std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        app.state.file = Some(folder.join("test.loom"));
        let ctx = egui::Context::default();
        start(&mut app, &ctx, false);
        let deadline = Instant::now()+Duration::from_secs(900);
        while app.kimodo.pending.is_some() && Instant::now()<deadline {
            poll(&mut app, &ctx);
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(app.kimodo.status.starts_with("Animation imported."),"{}",app.kimodo.status);
        assert!(app.state.playing);
        assert_eq!(app.state.frame_range,(0,149));
        assert!(app.state.scene.nodes().count() >= 77);
        assert!((app.state.fps-30.).abs()<0.01);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn result_from_another_scene_is_preserved_without_modifying_current_scene() {
        let mut app = LoomApp::headless();
        let previous = app.state.scene.instance_id();
        app.state.scene = loom_scene::Scene::new();
        assert_ne!(previous,app.state.scene.instance_id());
        let folder=std::env::temp_dir().join(format!("loom-kimodo-context-{}",std::process::id()));
        let (tx,rx)=mpsc::channel();
        app.kimodo.pending=Some(rx);
        tx.send(Event::Motion { job:"abcdef123456".into(), bytes:b"preserved result".to_vec(),scene:previous,folder:folder.clone() }).unwrap();
        poll(&mut app,&egui::Context::default());
        assert_eq!(app.state.scene.nodes().count(),0);
        assert!(!app.state.playing);
        let path=app.kimodo.ready.as_ref().unwrap();
        assert_eq!(std::fs::read(path).unwrap(),b"preserved result");
        let serialized=serde_json::to_string(&app.state.scene).unwrap();
        let reopened:loom_scene::Scene=serde_json::from_str(&serialized).unwrap();
        assert_ne!(reopened.instance_id(),app.state.scene.instance_id());
        assert_eq!(app.state.scene.clone().instance_id(),app.state.scene.instance_id());
        std::fs::remove_dir_all(folder).unwrap();
    }

}
