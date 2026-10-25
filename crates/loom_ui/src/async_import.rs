//! Background FBX IO; the original command remains the one native scene commit.
use crate::LoomApp;
use eframe::egui;
use loom_commands::{exchange::FbxImportOptions, fbx_io, AppState, Command};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc, Arc,
};
use std::time::Duration;

fn units(state: &AppState) -> f64 {
    state.units.system_unit.metres() * state.units.system_scale as f64
}
#[derive(Clone)]
struct Guard {
    scene: u64,
    revision: u64,
    units: f64,
    fps: f32,
}
impl Guard {
    fn capture(state: &AppState) -> Self {
        Self {
            scene: state.scene.instance_id(),
            revision: state.scene.revision(),
            units: units(state),
            fps: state.fps,
        }
    }
    fn matches(&self, state: &AppState) -> bool {
        self.scene == state.scene.instance_id()
            && self.revision == state.scene.revision()
            && self.units == units(state)
            && self.fps == state.fps
    }
}
#[derive(Clone, PartialEq)]
struct InfoKey {
    path: PathBuf,
    target_units: f64,
    units: f64,
    fps: f32,
}
#[derive(Clone, Copy)]
enum Kind {
    Info,
    Import(bool),
}
struct Job {
    id: u64,
    kind: Kind,
    path: PathBuf,
    options: FbxImportOptions,
    units: f64,
    fps: f32,
}
enum Event {
    Info(u64, Result<Value, String>),
    Import(u64, Result<fbx_io::PreparedFbxImport, String>),
}
#[derive(Default)]
struct Epochs {
    import: AtomicU64,
    info: AtomicU64,
}
struct Pending {
    id: u64,
    path: PathBuf,
    animation: bool,
    guard: Guard,
}
struct Ready {
    prepared: fbx_io::PreparedFbxImport,
    command: Command,
    animation: bool,
}

pub(crate) struct AsyncFbxImport {
    tx: mpsc::Sender<Job>,
    rx: mpsc::Receiver<Event>,
    epochs: Arc<Epochs>,
    next: u64,
    pending: Option<Pending>,
    info_key: Option<InfoKey>,
    info_result: Option<Result<Value, String>>,
    info_pending: bool,
    status: String,
    failed: bool,
}
impl Default for AsyncFbxImport {
    fn default() -> Self {
        let (tx, jobs) = mpsc::channel::<Job>();
        let (results, rx) = mpsc::channel();
        let epochs = Arc::new(Epochs::default());
        let worker = epochs.clone();
        std::thread::Builder::new()
            .name("loom-fbx-prepare".into())
            .spawn(move || {
                while let Ok(job) = jobs.recv() {
                    let active = match job.kind {
                        Kind::Info => &worker.info,
                        Kind::Import(_) => &worker.import,
                    };
                    if active.load(Ordering::Relaxed) != job.id {
                        continue;
                    }
                    let result = match job.kind {
                        Kind::Info => Event::Info(
                            job.id,
                            fbx_io::file_info_with(&job.path, &job.options, job.units, job.fps),
                        ),
                        Kind::Import(animation) => Event::Import(
                            job.id,
                            fbx_io::prepare(&job.path, &job.options, job.units, animation),
                        ),
                    };
                    if active.load(Ordering::Relaxed) == job.id && results.send(result).is_err() {
                        break;
                    }
                }
            })
            .expect("start FBX reader");
        Self {
            tx,
            rx,
            epochs,
            next: 1,
            pending: None,
            info_key: None,
            info_result: None,
            info_pending: false,
            status: String::new(),
            failed: false,
        }
    }
}
impl AsyncFbxImport {
    fn id(&mut self) -> u64 {
        let id = self.next;
        self.next = self.next.wrapping_add(1);
        id
    }
    fn begin(&mut self, state: &AppState, path: PathBuf, animation: bool) -> Result<(), String> {
        if self.pending.is_some() {
            return Err(
                "An FBX import is already being prepared. Cancel it before importing another file."
                    .into(),
            );
        }
        let id = self.id();
        let job = Job {
            id,
            kind: Kind::Import(animation),
            path: path.clone(),
            options: state.exchange.fbx_import.clone(),
            units: units(state),
            fps: state.fps,
        };
        self.epochs.import.store(id, Ordering::Relaxed);
        self.tx
            .send(job)
            .map_err(|_| "FBX reader stopped".to_owned())?;
        self.pending = Some(Pending {
            id,
            path: path.clone(),
            animation,
            guard: Guard::capture(state),
        });
        self.status = format!(
            "Preparing FBX {}…",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        self.failed = false;
        Ok(())
    }
    pub(crate) fn cancel(&mut self) {
        let id = self.id();
        self.epochs.import.store(id, Ordering::Relaxed);
        self.pending = None;
        self.status = "FBX import cancelled.".into();
        self.failed = false;
    }
    /// None means real metadata IO is pending; the dialog displays a loading state.
    pub(crate) fn file_info(
        &mut self,
        state: &AppState,
        path: &Path,
    ) -> Option<Result<Value, String>> {
        let key = InfoKey {
            path: path.to_owned(),
            target_units: if state.exchange.fbx_import.units_automatic {
                units(state)
            } else {
                state.exchange.fbx_import.units.metres()
            },
            units: units(state),
            fps: state.fps,
        };
        if self.info_key.as_ref() != Some(&key) {
            let id = self.id();
            self.epochs.info.store(id, Ordering::Relaxed);
            self.info_key = Some(key.clone());
            self.info_result = None;
            self.info_pending = true;
            if self
                .tx
                .send(Job {
                    id,
                    kind: Kind::Info,
                    path: key.path,
                    options: state.exchange.fbx_import.clone(),
                    units: key.units,
                    fps: key.fps,
                })
                .is_err()
            {
                self.info_result = Some(Err("FBX reader stopped".into()));
                self.info_pending = false;
            }
        }
        self.info_result.clone()
    }
    pub(crate) fn clear_info(&mut self) {
        let id = self.id();
        self.epochs.info.store(id, Ordering::Relaxed);
        self.info_key = None;
        self.info_result = None;
        self.info_pending = false;
    }
    fn take_ready(&mut self, state: &AppState) -> Option<Result<Ready, String>> {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Info(id, result) if id == self.epochs.info.load(Ordering::Relaxed) => {
                    self.info_result = Some(result);
                    self.info_pending = false;
                }
                Event::Import(id, result) if self.pending.as_ref().is_some_and(|p| p.id == id) => {
                    let pending = self.pending.take().unwrap();
                    if !pending.guard.matches(state) {
                        return Some(Err("Scene, pose, units or frame rate changed while preparing the FBX. Import it again in the current scene.".into()));
                    }
                    let prepared = match result {
                        Ok(prepared) => prepared,
                        Err(error) => return Some(Err(error)),
                    };
                    let path = Some(pending.path.to_string_lossy().into_owned());
                    let command = if pending.animation {
                        Command::ImportAnimationClip { path }
                    } else {
                        Command::ImportFile { path }
                    };
                    return Some(Ok(Ready {
                        prepared,
                        command,
                        animation: pending.animation,
                    }));
                }
                _ => {}
            }
        }
        None
    }
    pub(crate) fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
}
impl Drop for AsyncFbxImport {
    fn drop(&mut self) {
        self.epochs.import.fetch_add(1, Ordering::Relaxed);
        self.epochs.info.fetch_add(1, Ordering::Relaxed);
    }
}

pub(crate) fn start(app: &mut LoomApp, path: PathBuf) -> Result<(), String> {
    app.async_fbx.begin(&app.state, path, false)?;
    app.state.status_prompt = app.async_fbx.status.clone();
    Ok(())
}
pub(crate) fn start_animation(app: &mut LoomApp, path: PathBuf) -> Result<(), String> {
    app.async_fbx.begin(&app.state, path, true)?;
    app.state.status_prompt = app.async_fbx.status.clone();
    Ok(())
}
pub(crate) fn poll(app: &mut LoomApp, ctx: &egui::Context) {
    if let Some(ready) = app.async_fbx.take_ready(&app.state) {
        let result = ready.and_then(|ready| {
            app.state.prepared_fbx_import = Some(ready.prepared);
            let result = app.dispatch(ready.command);
            app.state.prepared_fbx_import = None;
            let result = result?;
            if ready.animation {
                crate::clip_timeline::preview_import(app, &result)?;
            }
            Ok(())
        });
        match result {
            Ok(()) => {
                app.async_fbx.status = "FBX imported.".into();
                app.async_fbx.failed = false;
                app.state.status_prompt = app.async_fbx.status.clone();
            }
            Err(error) => {
                app.async_fbx.status = format!("FBX import failed: {error}");
                app.async_fbx.failed = true;
                app.state.status_prompt = app.async_fbx.status.clone();
            }
        }
    }
    if app.async_fbx.is_pending() || app.async_fbx.info_pending {
        ctx.request_repaint_after(Duration::from_millis(33));
    }
}
pub(crate) fn show_status(ui: &mut egui::Ui, app: &mut LoomApp) {
    if app.async_fbx.status.is_empty() {
        return;
    }
    ui.horizontal(|ui| {
        if app.async_fbx.is_pending() {
            ui.spinner();
            ui.label(&app.async_fbx.status);
            if ui.small_button("Cancel FBX import").clicked() {
                app.async_fbx.cancel();
                app.state.status_prompt = app.async_fbx.status.clone();
            }
        } else if app.async_fbx.failed {
            ui.colored_label(ui.visuals().error_fg_color, &app.async_fbx.status);
        } else {
            ui.label(&app.async_fbx.status);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use loom_scene::{Node, ObjectKind};
    use std::time::Instant;

    fn source(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("loom-ui-async-fbx-{name}.fbx"));
        let doc = loom_fbx::FbxDocument {
            nodes: vec![
                loom_fbx::FbxNode::new(
                    "joint",
                    loom_fbx::FbxKind::Bone {
                        length: 1.0,
                        size: 0.1,
                    },
                ),
                loom_fbx::FbxNode::new(
                    "box",
                    loom_fbx::FbxKind::Mesh {
                        mesh: loom_mesh::box_primitive(Vec3::ONE),
                        materials: vec![],
                    },
                ),
            ],
            unit_metres: 0.0254,
            ..Default::default()
        };
        std::fs::write(
            &path,
            loom_fbx::write(&doc, &loom_fbx::WriteOpts::default()),
        )
        .unwrap();
        path
    }
    fn ready(worker: &mut AsyncFbxImport, state: &AppState) -> Result<Ready, String> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(result) = worker.take_ready(state) {
                return result;
            }
            assert!(Instant::now() < deadline, "FBX worker timed out");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    #[test]
    fn preparation_is_background_only_and_original_command_commits_one_undo() {
        let path = source("success");
        let mut state = AppState::new();
        let mut worker = AsyncFbxImport::default();
        worker.begin(&state, path.clone(), false).unwrap();
        assert!(worker.is_pending());
        assert_eq!(state.scene.nodes().count(), 0);
        state.exchange.fbx_import.bones_as_dummies = true;
        let ready = ready(&mut worker, &state).unwrap();
        std::fs::remove_file(path).unwrap();
        state.prepared_fbx_import = Some(ready.prepared);
        let steps = state.undo.len();
        loom_commands::execute(&mut state, ready.command).unwrap();
        assert_eq!(state.undo.len(), steps + 1);
        assert_eq!(
            state.scene.nodes().filter(|n| n.object.is_bone()).count(),
            1
        );
        loom_commands::execute(&mut state, Command::UndoSceneOperation).unwrap();
        assert_eq!(state.scene.nodes().count(), 0);
    }
    #[test]
    fn changing_scene_pose_units_or_frame_rate_rejects_completion_without_mutation() {
        let path = source("guard");
        for changed in ["scene", "pose", "units", "fps"] {
            let mut state = AppState::new();
            let mut worker = AsyncFbxImport::default();
            worker.begin(&state, path.clone(), false).unwrap();
            match changed {
                "scene" => state.scene.renew_instance(),
                "pose" => {
                    state
                        .scene
                        .add_node(Node::new("User edit", ObjectKind::dummy(1.0)));
                }
                "units" => state.units.system_scale = 2.0,
                "fps" => state.fps = 60.0,
                _ => unreachable!(),
            }
            let before = serde_json::to_value(&state.scene).unwrap();
            let undo = state.undo.len();
            assert!(ready(&mut worker, &state)
                .err()
                .unwrap()
                .contains("changed while preparing"));
            assert_eq!(serde_json::to_value(&state.scene).unwrap(), before);
            assert_eq!(state.undo.len(), undo);
        }
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn cancellation_and_reader_errors_leave_scene_unchanged_and_allow_retry() {
        let path = source("cancel");
        let state = AppState::new();
        let mut worker = AsyncFbxImport::default();
        worker.begin(&state, path.clone(), false).unwrap();
        worker.cancel();
        assert!(!worker.is_pending());
        worker
            .begin(&state, path.with_extension("missing.fbx"), false)
            .unwrap();
        assert!(ready(&mut worker, &state).is_err());
        assert_eq!(state.scene.nodes().count(), 0);
        worker.begin(&state, path.clone(), false).unwrap();
        assert!(ready(&mut worker, &state).is_ok());
        assert_eq!(state.scene.nodes().count(), 0);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn file_statistics_use_worker_and_report_real_source_counts() {
        let path = source("info");
        let state = AppState::new();
        let mut worker = AsyncFbxImport::default();
        assert!(worker.file_info(&state, &path).is_none());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            worker.take_ready(&state);
            if let Some(result) = worker.file_info(&state, &path) {
                let info = result.unwrap();
                assert_eq!(info["meshes"], 1);
                assert_eq!(info["elements"], 2);
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn external_large_fbx_preparation_keeps_egui_frames_running_and_scene_untouched() {
        let Some(path) = std::env::var_os("LOOM_METAHUMAN_FBX") else {
            return;
        };
        let mut state = AppState::new();
        state.exchange.fbx_import.metahuman_body_only = true;
        let before = serde_json::to_value(&state.scene).unwrap();
        let mut worker = AsyncFbxImport::default();
        let ctx = egui::Context::default();
        let start = Instant::now();
        worker.begin(&state, path.into(), false).unwrap();
        let queued_ms = start.elapsed().as_secs_f64() * 1000.0;
        let mut frames = 0;
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.label("Preparing source");
                });
            });
            frames += 1;
            assert_eq!(serde_json::to_value(&state.scene).unwrap(), before);
            if let Some(ready) = worker.take_ready(&state) {
                ready.unwrap();
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(frames > 1);
        eprintln!(
            "ASYNC_FBX queue_ms={queued_ms:.3} responsive_egui_frames={frames} elapsed_ms={:.3}",
            start.elapsed().as_secs_f64() * 1000.0
        );
    }
}
