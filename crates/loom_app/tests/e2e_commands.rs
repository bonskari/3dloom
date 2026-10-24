//! Drives the real binary on a virtual display through the control socket.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct App {
    child: Child,
    stream: TcpStream,
    _serial: std::sync::MutexGuard<'static, ()>,
}

/// `xvfb-run -a` races when two instances start at once (both pick the
/// same free display), so the e2e tests run one at a time.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Kills the launched process group even if startup panics before `App` exists.
struct KillGuard(Option<Child>);

impl Drop for KillGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let pgid = child.id();
            let _ = Command::new("kill").args(["-TERM", "--", &format!("-{pgid}")]).status();
            let _ = child.wait();
        }
    }
}

/// A port nobody is listening on, so a stale instance can never answer.
fn free_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port()
}

/// A fresh directory for a launched instance's control socket and log, so
/// tests never touch the user's (`current`, log files).
fn private_dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("loom-e2e-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

impl App {
    /// Launches on the persistent Xvnc display `:90` (where xdotool can drive
    /// the window), or returns None when that display or xdotool is missing.
    fn launch_on_xvnc() -> Option<App> {
        let alive = Command::new("xdpyinfo").arg("-display").arg(":90").output().is_ok_and(|o| o.status.success());
        let xdotool = Command::new("xdotool").arg("version").output().is_ok();
        if !alive || !xdotool {
            eprintln!("Xvnc :90 or xdotool not available; skipping input e2e");
            return None;
        }
        let serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let port = free_port();
        let dir = private_dir(&port.to_string());
        let bin = env!("CARGO_BIN_EXE_loom");
        let child = Command::new(bin)
            .env("LOOM_CONTROL_PORT", port.to_string())
            .env("XDG_RUNTIME_DIR", &dir)
            .env("XDG_STATE_HOME", &dir)
            .env("DISPLAY", ":90")
            .env_remove("WAYLAND_DISPLAY")
            .env("XDG_SESSION_TYPE", "x11")
            .env("WINIT_UNIX_BACKEND", "x11")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .process_group(0)
            .spawn()
            .expect("spawn loom on :90");
        let mut guard = KillGuard(Some(child));
        let deadline = Instant::now() + Duration::from_secs(60);
        let stream = loop {
            if let Ok(s) = TcpStream::connect(("127.0.0.1", port)) {
                break s;
            }
            assert!(Instant::now() < deadline, "loom did not open control port");
            std::thread::sleep(Duration::from_millis(200));
        };
        stream.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
        let child = guard.0.take().expect("child still owned");
        Some(App { child, stream, _serial: serial })
    }

    fn launch() -> Option<App> {
        if Command::new("xvfb-run").arg("--help").output().is_err() {
            eprintln!("xvfb-run not found; skipping e2e");
            return None;
        }
        let serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let port = free_port();
        let dir = private_dir(&port.to_string());
        let bin = env!("CARGO_BIN_EXE_loom");
        let child = Command::new("xvfb-run")
            .args(["-a", "-s", "-screen 0 1600x1000x24", bin])
            .env("LOOM_CONTROL_PORT", port.to_string())
            .env("XDG_RUNTIME_DIR", &dir)
            .env("XDG_STATE_HOME", &dir)
            // winit prefers Wayland when WAYLAND_DISPLAY is set and would
            // open the window on the user's real screen instead of Xvfb.
            .env_remove("WAYLAND_DISPLAY")
            .env("XDG_SESSION_TYPE", "x11")
            .env("WINIT_UNIX_BACKEND", "x11")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            // Own process group so Drop can kill xvfb-run, Xvfb and loom together.
            .process_group(0)
            .spawn()
            .expect("spawn loom under xvfb-run");
        let mut guard = KillGuard(Some(child));
        let deadline = Instant::now() + Duration::from_secs(60);
        let stream = loop {
            if let Ok(s) = TcpStream::connect(("127.0.0.1", port)) {
                break s;
            }
            assert!(Instant::now() < deadline, "loom did not open control port");
            std::thread::sleep(Duration::from_millis(200));
        };
        stream.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
        let child = guard.0.take().expect("child still owned");
        Some(App { child, stream, _serial: serial })
    }

    fn send(&mut self, body: Value) -> Value {
        writeln!(self.stream, "{body}").unwrap();
        let mut line = String::new();
        BufReader::new(self.stream.try_clone().unwrap()).read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("bad reply {line:?}: {e}"))
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let pgid = self.child.id();
        let _ = Command::new("kill").args(["-TERM", "--", &format!("-{pgid}")]).status();
        let _ = self.child.wait();
    }
}

#[test]
fn create_undo_and_screenshot_through_commands() {
    let Some(mut app) = App::launch() else { return };
    let r = app.send(json!({ "cmd": "ping" }));
    assert_eq!(r["ok"], true);
    let r = app.send(json!({ "cmd": "Create Primitive", "args": { "kind": "box", "position": [0, 0, 0], "size": [2, 2, 2] } }));
    assert_eq!(r["ok"], true, "{r}");
    let r = app.send(json!({ "cmd": "State" }));
    assert_eq!(r["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(r["nodes"][0]["name"], "Box001");
    let r = app.send(json!({ "cmd": "Undo Scene Operation" }));
    assert_eq!(r["ok"], true);
    let r = app.send(json!({ "cmd": "State" }));
    assert_eq!(r["nodes"].as_array().unwrap().len(), 0);
    app.send(json!({ "cmd": "Test Room" }));
    let dir = std::env::temp_dir().join("loom-e2e");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("e2e.png");
    let r = app.send(json!({ "cmd": "screenshot", "path": path.to_str().unwrap() }));
    assert_eq!(r["ok"], true, "{r}");
    assert!(std::fs::metadata(&path).map(|m| m.len() > 1000).unwrap_or(false));
}

#[test]
fn save_open_round_trip_through_the_socket() {
    let Some(mut app) = App::launch() else { return };
    let dir = std::env::temp_dir().join("loom-e2e");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("round.loom");
    let _ = std::fs::remove_file(&path);
    app.send(json!({ "cmd": "Create Primitive", "args": { "kind": "box", "position": [0, 0, 0], "size": [2, 2, 2] } }));
    let r = app.send(json!({ "cmd": "State" }));
    assert_eq!(r["dirty"], true);
    assert_eq!(r["title"], "Untitled * - 3dLoom");
    let r = app.send(json!({ "cmd": "Save File", "args": { "path": path.to_str().unwrap() } }));
    assert_eq!(r["ok"], true, "{r}");
    let r = app.send(json!({ "cmd": "State" }));
    assert_eq!(r["dirty"], false);
    assert_eq!(r["title"], "round.loom - 3dLoom");
    app.send(json!({ "cmd": "New Scene" }));
    assert_eq!(app.send(json!({ "cmd": "State" }))["nodes"].as_array().unwrap().len(), 0);
    let r = app.send(json!({ "cmd": "Open File", "args": { "path": path.to_str().unwrap() } }));
    assert_eq!(r["ok"], true, "{r}");
    let r = app.send(json!({ "cmd": "State" }));
    assert_eq!(r["nodes"][0]["name"], "Box001");
    assert_eq!(r["file"], path.to_str().unwrap());
    // The window redraws the loaded scene.
    let shot = dir.join("round.png");
    let r = app.send(json!({ "cmd": "screenshot", "path": shot.to_str().unwrap() }));
    assert_eq!(r["ok"], true, "{r}");
}

#[test]
fn edit_uvws_window_opens_and_screenshots() {
    let Some(mut app) = App::launch() else { return };
    app.send(json!({ "cmd": "Create Primitive", "args": { "kind": "box", "position": [0, 0, 0], "size": [10, 10, 10] } }));
    let r = app.send(json!({ "cmd": "Add Modifier", "args": { "name": "Unwrap UVW" } }));
    assert_eq!(r["ok"], true, "{r}");
    let r = app.send(json!({ "cmd": "Edit UVW's" }));
    assert_eq!(r["ok"], true, "{r}");
    let r = app.send(json!({ "cmd": "Flatten Map" }));
    assert_eq!(r["ok"], true, "{r}");
    let r = app.send(json!({ "cmd": "State" }));
    assert_eq!(r["unwrap"]["editor"]["open"], true);
    assert!(r["unwrap"]["tvs"].as_u64().unwrap() >= 24, "{}", r["unwrap"]);
    std::thread::sleep(Duration::from_millis(300));
    let dir = std::env::temp_dir().join("loom-e2e");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("uv.png");
    let r = app.send(json!({ "cmd": "screenshot", "path": path.to_str().unwrap() }));
    assert_eq!(r["ok"], true, "{r}");
    assert!(std::fs::metadata(&path).map(|m| m.len() > 1000).unwrap_or(false));
}

#[test]
fn all_fourteen_layouts_render_differently() {
    let Some(mut app) = App::launch() else { return };
    app.send(json!({ "cmd": "Create Primitive", "args": { "kind": "teapot", "position": [0, 0, 0], "size": [3, 3, 3] } }));
    app.send(json!({ "cmd": "Zoom Extents All" }));
    let layouts = [
        "Single", "TwoV", "TwoH", "ThreeLeftBig", "ThreeTopBig", "ThreeRightBig", "ThreeBottomBig", "ThreeCols", "ThreeRows",
        "FourGrid", "FourLeftBig", "FourRightBig", "FourTopBig", "FourBottomBig",
    ];
    let dir = std::env::temp_dir().join("loom-e2e");
    std::fs::create_dir_all(&dir).unwrap();
    let mut hashes = std::collections::HashSet::new();
    for (i, l) in layouts.iter().enumerate() {
        let r = app.send(json!({ "cmd": "Viewport Layout", "args": { "layout": l } }));
        assert_eq!(r["ok"], true, "{r}");
        std::thread::sleep(Duration::from_millis(250));
        let path = dir.join(format!("1b-layout-{:02}.png", i + 1));
        let r = app.send(json!({ "cmd": "screenshot", "path": path.to_str().unwrap() }));
        assert_eq!(r["ok"], true, "{r}");
        let bytes = std::fs::read(&path).unwrap();
        // Cheap content hash: PNG bytes differ when the pixels differ.
        let hash = bytes.iter().fold(0u64, |h, b| h.wrapping_mul(1099511628211).wrapping_add(*b as u64));
        assert!(hashes.insert(hash), "layout {l} rendered identically to an earlier one");
    }
}

/// xdotool on :90: focuses the 3dLoom window once, then clicks.
fn xdo(args: &[&str]) {
    let st = Command::new("xdotool").env("DISPLAY", ":90").args(args).status().expect("xdotool");
    assert!(st.success(), "xdotool {args:?}");
}

#[test]
fn clicking_an_object_in_a_viewport_selects_it() {
    let Some(mut app) = App::launch_on_xvnc() else { return };
    std::thread::sleep(Duration::from_millis(1500));
    let r = app.send(json!({ "cmd": "Create Primitive", "args": { "kind": "box", "position": [0, 0, 0], "size": [30, 30, 30] } }));
    assert_eq!(r["ok"], true, "{r}");
    app.send(json!({ "cmd": "Select None" }));
    app.send(json!({ "cmd": "Hotkey", "args": { "chord": "Alt+W" } }));
    app.send(json!({ "cmd": "Hotkey", "args": { "chord": "T" } }));
    app.send(json!({ "cmd": "Zoom Extents All" }));
    std::thread::sleep(Duration::from_millis(400));
    // The box fills the middle of the maximized Top view; the window is at the display origin.
    let geo = Command::new("xdotool").env("DISPLAY", ":90").args(["search", "--name", "3dLoom"]).output().expect("xdotool search");
    let wid = String::from_utf8_lossy(&geo.stdout).split_whitespace().last().expect("window id").to_owned();
    xdo(&["windowfocus", "--sync", &wid]);
    let out = Command::new("xdotool").env("DISPLAY", ":90").args(["getwindowgeometry", "--shell", &wid]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let get = |k: &str| text.lines().find_map(|l| l.strip_prefix(k).and_then(|v| v.trim_start_matches('=').parse::<i32>().ok())).unwrap();
    let (x, y, w, h) = (get("X"), get("Y"), get("WIDTH"), get("HEIGHT"));
    // Viewport area is roughly the left 72 % under the toolbars; its centre holds the box.
    let (cx, cy) = (x + (w as f32 * 0.36) as i32, y + (h as f32 * 0.48) as i32);
    xdo(&["mousemove", &cx.to_string(), &cy.to_string(), "click", "1"]);
    std::thread::sleep(Duration::from_millis(500));
    let st = app.send(json!({ "cmd": "State" }));
    assert_eq!(st["selection"]["selected"], json!([1]), "click at ({cx},{cy}) should select Box001: {st}");
    // Clicking empty space clears it.
    xdo(&["mousemove", &(x + 40).to_string(), &(y + h - 120).to_string(), "click", "1"]);
    std::thread::sleep(Duration::from_millis(500));
    let st = app.send(json!({ "cmd": "State" }));
    assert_eq!(st["selection"]["selected"], json!([]), "{st}");
}

#[test]
fn import_fbx_through_the_socket_and_screenshot() {
    let Some(mut app) = App::launch() else { return };
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../loom_fbx/tests/fixtures/character_for_mixamo.fbx");
    let r = app.send(json!({ "cmd": "Import File", "args": { "path": fixture } }));
    assert_eq!(r["ok"], true, "{r}");
    let nodes = r["nodes"].as_array().unwrap().clone();
    assert!(!nodes.is_empty());
    app.send(json!({ "cmd": "Zoom Extents All" }));
    let dir = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join("Pictures/3dloom");
    std::fs::create_dir_all(&dir).unwrap();
    let shot = dir.join("6-import-mixamo.png");
    let r = app.send(json!({ "cmd": "screenshot", "path": shot.to_str().unwrap() }));
    assert_eq!(r["ok"], true, "{r}");
    let state = app.send(json!({ "cmd": "State" }));
    assert!(state["nodes"].as_array().unwrap().len() >= nodes.len());
}

#[test]
fn rig_through_the_socket_and_screenshot() {
    let Some(mut app) = App::launch() else { return };
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../loom_fbx/tests/fixtures/sitting_idle.fbx");
    let r = app.send(json!({ "cmd": "Import File", "args": { "path": fixture } }));
    assert_eq!(r["ok"], true, "{r}");
    let state = app.send(json!({ "cmd": "State" }));
    let nodes = state["nodes"].as_array().unwrap();
    assert!(nodes.len() >= 66, "{}", nodes.len());
    let arm = nodes.iter().find(|n| n["name"] == "mixamorig:LeftArm").expect("LeftArm bone")["id"].as_u64().unwrap();
    let mesh = nodes.iter().find(|n| n["kind"] == "Object" || n["kind"] == "Editable Poly").map(|n| n["id"].as_u64().unwrap());
    let r = app.send(json!({ "cmd": "Select Nodes", "args": { "ids": [arm], "op": "replace" } }));
    assert_eq!(r["ok"], true, "{r}");
    let r = app.send(json!({ "cmd": "Rotate Selection", "args": { "axis": "z", "degrees": 60.0 } }));
    assert_eq!(r["ok"], true, "{r}");
    if let Some(m) = mesh {
        app.send(json!({ "cmd": "Select Nodes", "args": { "ids": [m], "op": "replace" } }));
        let st = app.send(json!({ "cmd": "State" }));
        assert!(!st["skin"].is_null(), "the mesh reports its Skin: {}", st["skin"]);
    }
    app.send(json!({ "cmd": "Zoom Extents All" }));
    let dir = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join("Pictures/3dloom");
    std::fs::create_dir_all(&dir).unwrap();
    let shot = dir.join("7-mixamo-bent.png");
    let r = app.send(json!({ "cmd": "screenshot", "path": shot.to_str().unwrap() }));
    assert_eq!(r["ok"], true, "{r}");
    let r = app.send(json!({ "cmd": "Display Bones" }));
    assert_eq!(r["ok"], true, "{r}");
    let r = app.send(json!({ "cmd": "Display Bones and Objects" }));
    assert_eq!(r["ok"], true, "{r}");
}

#[test]
fn track_view_opens_through_the_socket() {
    let Some(mut app) = App::launch() else { return };
    app.send(json!({ "cmd": "Create Primitive", "args": { "kind": "box", "position": [0, 0, 0], "size": [20, 20, 20] } }));
    let st = app.send(json!({ "cmd": "State" }));
    let id = st["nodes"][0]["id"].as_u64().unwrap();
    app.send(json!({ "cmd": "Auto Key Mode Toggle" }));
    for (f, d) in [(25, [40.0, 0.0, 10.0]), (50, [-20.0, 30.0, 0.0]), (100, [-20.0, -30.0, -10.0])] {
        app.send(json!({ "cmd": "Set Frame", "args": { "frame": f } }));
        let r = app.send(json!({ "cmd": "Move Selection", "args": { "delta": d } }));
        assert_eq!(r["ok"], true, "{r}");
    }
    app.send(json!({ "cmd": "Auto Key Mode Toggle" }));
    app.send(json!({ "cmd": "Set Frame", "args": { "frame": 40 } }));
    let r = app.send(json!({ "cmd": "Open Curve Editor" }));
    assert_eq!(r["ok"], true, "{r}");
    let r = app.send(json!({ "cmd": "Track View Highlight", "args": { "items": [format!("{id}:Position")], "op": "replace" } }));
    assert_eq!(r["ok"], true, "{r}");
    app.send(json!({ "cmd": "Track View Select Keys", "args": { "keys": [{ "id": id, "track": "Position", "comp": 0, "time": 25.0 }], "op": "replace" } }));
    app.send(json!({ "cmd": "Set Tangents to Spline" }));
    app.send(json!({ "cmd": "Track View Set View", "args": { "start": -5.0, "end": 105.0, "low": -40.0, "high": 60.0 } }));
    let st = app.send(json!({ "cmd": "State" }));
    assert_eq!(st["animation"]["track_view"]["open"], true, "{}", st["animation"]);
    assert_eq!(st["animation"]["track_view"]["selected_keys"], 1);
    std::thread::sleep(Duration::from_millis(300));
    let dir = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join("Pictures/3dloom");
    std::fs::create_dir_all(&dir).unwrap();
    let shot = dir.join("8-curve-editor.png");
    let r = app.send(json!({ "cmd": "screenshot", "path": shot.to_str().unwrap() }));
    assert_eq!(r["ok"], true, "{r}");
    let r = app.send(json!({ "cmd": "Open Dope Sheet" }));
    assert_eq!(r["ok"], true, "{r}");
    app.send(json!({ "cmd": "Track View Highlight", "args": { "items": [format!("{id}")], "op": "replace" } }));
    app.send(json!({ "cmd": "Track View Select Time", "args": { "start": 20.0, "end": 60.0 } }));
    std::thread::sleep(Duration::from_millis(300));
    let shot = dir.join("8-dope-sheet.png");
    let r = app.send(json!({ "cmd": "screenshot", "path": shot.to_str().unwrap() }));
    assert_eq!(r["ok"], true, "{r}");
    assert!(std::fs::metadata(&shot).map(|m| m.len() > 1000).unwrap_or(false));
}

#[test]
fn animation_plays_through_the_socket() {
    let Some(mut app) = App::launch() else { return };
    app.send(json!({ "cmd": "Create Primitive", "args": { "kind": "teapot", "position": [0, 0, 0], "size": [20, 20, 20] } }));
    app.send(json!({ "cmd": "Set Time Configuration", "args": { "start": 0, "end": 30 } }));
    app.send(json!({ "cmd": "Auto Key Mode Toggle" }));
    app.send(json!({ "cmd": "Set Frame", "args": { "frame": 30 } }));
    let r = app.send(json!({ "cmd": "Move Selection", "args": { "delta": [60.0, 0.0, 0.0] } }));
    assert_eq!(r["ok"], true, "{r}");
    app.send(json!({ "cmd": "Rotate Selection", "args": { "axis": "z", "degrees": 180.0 } }));
    app.send(json!({ "cmd": "Auto Key Mode Toggle" }));
    app.send(json!({ "cmd": "Set Frame", "args": { "frame": 0 } }));
    app.send(json!({ "cmd": "Show Trajectories Toggle" }));
    app.send(json!({ "cmd": "Zoom Extents All" }));
    let r = app.send(json!({ "cmd": "Play Animation" }));
    assert_eq!(r["ok"], true, "{r}");
    std::thread::sleep(Duration::from_millis(400));
    let st = app.send(json!({ "cmd": "State" }));
    assert_eq!(st["animation"]["playing"], true);
    // Stop on a known frame and draw it.
    app.send(json!({ "cmd": "Play Animation" }));
    app.send(json!({ "cmd": "Set Frame", "args": { "frame": 15 } }));
    let st = app.send(json!({ "cmd": "State" }));
    assert_eq!(st["frame"], 15);
    let x = st["nodes"][0]["position"][0].as_f64().unwrap();
    assert!(x > 5.0 && x < 55.0, "half way along: {x}");
    std::thread::sleep(Duration::from_millis(300));
    let dir = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join("Pictures/3dloom");
    std::fs::create_dir_all(&dir).unwrap();
    let shot = dir.join("8-playback.png");
    let r = app.send(json!({ "cmd": "screenshot", "path": shot.to_str().unwrap() }));
    assert_eq!(r["ok"], true, "{r}");
}

#[test]
fn phase9_modifiers_through_the_socket_and_screenshot() {
    let Some(mut app) = App::launch() else { return };
    let ok = |r: Value| assert_eq!(r["ok"], true, "{r}");
    // A box rounded by HSDS, a plane pulled by Affect Region, a box bent round a circle (PathDeform (WSM)).
    ok(app.send(json!({ "cmd": "Create Primitive", "args": { "kind": "box", "position": [0, 0, 0], "size": [20, 20, 20] } })));
    ok(app.send(json!({ "cmd": "Add Modifier", "args": { "name": "HSDS" } })));
    ok(app.send(json!({ "cmd": "Set Modifier Parameter", "args": { "index": 0, "name": "Smooth Result", "value": 1 } })));
    ok(app.send(json!({ "cmd": "HSDS Adaptive Subdivide", "args": { "max_level": 2, "max_angle": 30 } })));
    ok(app.send(json!({ "cmd": "Create Primitive", "args": { "kind": "plane", "position": [40, 0, 0], "size": [40, 40, 0] } })));
    let st = app.send(json!({ "cmd": "State" }));
    let plane = st["nodes"].as_array().unwrap().last().unwrap()["id"].as_u64().unwrap();
    ok(app.send(json!({ "cmd": "Set Parameter", "args": { "id": plane, "name": "Length Segs", "value": 16 } })));
    ok(app.send(json!({ "cmd": "Set Parameter", "args": { "id": plane, "name": "Width Segs", "value": 16 } })));
    ok(app.send(json!({ "cmd": "Add Modifier", "args": { "name": "Affect Region" } })));
    ok(app.send(json!({ "cmd": "Create Shape", "args": { "kind": "circle", "position": [0, 50, 0], "size": [40, 40, 0] } })));
    let st = app.send(json!({ "cmd": "State" }));
    let circle = st["nodes"].as_array().unwrap().last().unwrap()["id"].as_u64().unwrap();
    ok(app.send(json!({ "cmd": "Create Primitive", "args": { "kind": "box", "position": [0, 50, 0], "size": [3, 3, 50] } })));
    let st = app.send(json!({ "cmd": "State" }));
    let rod = st["nodes"].as_array().unwrap().last().unwrap()["id"].as_u64().unwrap();
    ok(app.send(json!({ "cmd": "Set Parameter", "args": { "id": rod, "name": "Height Segs", "value": 20 } })));
    ok(app.send(json!({ "cmd": "Add Modifier", "args": { "name": "PathDeform (WSM)" } })));
    ok(app.send(json!({ "cmd": "Pick Modifier Node", "args": { "index": 0, "node": circle } })));
    ok(app.send(json!({ "cmd": "Set Modifier Parameter", "args": { "index": 0, "name": "Path Deform Axis", "value": 2 } })));
    let st = app.send(json!({ "cmd": "State" }));
    let rod_state = st["nodes"].as_array().unwrap().iter().find(|n| n["id"] == rod).unwrap().clone();
    assert_eq!(rod_state["stack"][0]["modifier"], "PathDeform (WSM)", "{rod_state}");
    app.send(json!({ "cmd": "Select Nodes", "args": { "ids": [], "op": "replace" } }));
    app.send(json!({ "cmd": "Zoom Extents All" }));
    let dir = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join("Pictures/3dloom");
    std::fs::create_dir_all(&dir).unwrap();
    let shot = dir.join("9-modifiers.png");
    ok(app.send(json!({ "cmd": "screenshot", "path": shot.to_str().unwrap() })));
}

/// A Unix socket client (the control socket every run has).
#[cfg(unix)]
fn ask(stream: &mut std::os::unix::net::UnixStream, body: Value) -> Value {
    writeln!(stream, "{body}").unwrap();
    let mut line = String::new();
    BufReader::new(stream.try_clone().unwrap()).read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap_or_else(|e| panic!("bad reply {line:?}: {e}"))
}

#[cfg(unix)]
#[test]
fn every_run_answers_on_its_private_unix_socket_with_diagnostics_input_and_logs() {
    use std::os::unix::fs::PermissionsExt;
    if Command::new("xvfb-run").arg("--help").output().is_err() {
        eprintln!("xvfb-run not found; skipping e2e");
        return;
    }
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = private_dir("unix");
    let bin = env!("CARGO_BIN_EXE_loom");
    // No LOOM_CONTROL_PORT: started the way a person starts it.
    let child = Command::new("xvfb-run")
        .args(["-a", "-s", "-screen 0 1600x1000x24", bin])
        .env_remove("LOOM_CONTROL_PORT")
        .env("XDG_RUNTIME_DIR", &dir)
        .env("XDG_STATE_HOME", &dir)
        .env_remove("WAYLAND_DISPLAY")
        .env("XDG_SESSION_TYPE", "x11")
        .env("WINIT_UNIX_BACKEND", "x11")
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .process_group(0)
        .spawn()
        .expect("spawn loom under xvfb-run");
    let _guard = KillGuard(Some(child));
    let rt = dir.join("3dloom");
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut s = loop {
        if let Ok(s) = std::os::unix::net::UnixStream::connect(rt.join("current")) {
            break s;
        }
        assert!(Instant::now() < deadline, "loom did not open its unix socket");
        std::thread::sleep(Duration::from_millis(200));
    };
    s.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
    assert_eq!(std::fs::metadata(&rt).unwrap().permissions().mode() & 0o777, 0o700);
    let ping = ask(&mut s, json!({ "cmd": "ping" }));
    assert_eq!(ping["ok"], true, "{ping}");
    let pid = ping["pid"].as_u64().unwrap();
    let sock = rt.join(format!("{pid}.sock"));
    assert_eq!(ping["socket"], sock.to_string_lossy().as_ref());
    assert_eq!(std::fs::metadata(&sock).unwrap().permissions().mode() & 0o777, 0o600);

    // A registry command, as over TCP.
    let r = ask(&mut s, json!({ "cmd": "Create Primitive", "args": { "kind": "box", "position": [0, 0, 0], "size": [20, 20, 20] } }));
    assert_eq!(r["ok"], true, "{r}");
    let r = ask(&mut s, json!({ "cmd": "State" }));
    assert_eq!(r["nodes"][0]["name"], "Box001");

    // Diagnostics: the interaction state, the viewport rectangles.
    let d = ask(&mut s, json!({ "cmd": "diagnostics" }));
    assert_eq!(d["ok"], true, "{d}");
    assert_eq!(d["interaction"]["nav_mode"], "None");
    assert_eq!(d["interaction"]["camera_drag"], Value::Null);
    let active = d["viewports"]["active"].as_u64().unwrap() as usize;
    let rect: Vec<f64> = d["viewports"]["rects"][active].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    let (cx, cy) = ((rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0);
    let before = d["viewports"]["active_view"]["camera"].clone();

    // Injected input: a middle-button drag in the active viewport pans it.
    let r = ask(&mut s, json!({ "cmd": "input", "args": { "events": [{ "type": "drag", "button": "middle", "from": [cx, cy], "to": [cx + 80.0, cy + 40.0] }] } }));
    assert_eq!(r["ok"], true, "{r}");
    let d = ask(&mut s, json!({ "cmd": "diagnostics" }));
    assert_ne!(d["viewports"]["active_view"]["camera"]["target"], before["target"], "the injected drag panned: {}", d["viewports"]);
    assert!(d["input_events"].as_array().unwrap().iter().any(|e| e["event"].as_str().unwrap().contains("Middle")), "{}", d["input_events"]);
    let hit = ask(&mut s, json!({ "cmd": "ui_hit", "x": cx, "y": cy }));
    assert_eq!(hit["viewport"], active, "{hit}");
    let r = ask(&mut s, json!({ "cmd": "reset_interaction" }));
    assert_eq!(r["ok"], true, "{r}");

    // The log: this run's file, with the commands and their results.
    let p = ask(&mut s, json!({ "cmd": "log_path" }));
    let log = std::path::PathBuf::from(p["path"].as_str().unwrap());
    assert!(log.starts_with(dir.join("3dloom/logs")), "{}", log.display());
    assert!(log.file_name().unwrap().to_string_lossy().ends_with(&format!("-{pid}.log")));
    let t = ask(&mut s, json!({ "cmd": "log_tail", "lines": 50 }));
    let text = t["text"].as_str().unwrap();
    assert!(text.contains("Create Primitive") && text.contains("\"box\""), "{text}");
    assert!(text.contains("pid ") && text.contains("started"), "{text}");

    let shot = dir.join("shot.png");
    let r = ask(&mut s, json!({ "cmd": "screenshot", "path": shot.to_string_lossy() }));
    assert_eq!(r["ok"], true, "{r}");
    assert!(shot.exists());

    // The MCP server finds the same instance through the runtime directory
    // (built next to `loom` when the workspace is tested).
    let mcp = std::path::Path::new(bin).with_file_name("loom-mcp");
    if mcp.exists() {
        let mut child = Command::new(&mcp)
            .env_remove("LOOM_CONTROL_PORT")
            .env("XDG_RUNTIME_DIR", &dir)
            .env("XDG_STATE_HOME", &dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn loom-mcp");
        let mut stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let mut call = |id: u64, name: &str, args: Value| -> Value {
            writeln!(stdin, "{}", json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } })).unwrap();
            let mut line = String::new();
            stdout.read_line(&mut line).unwrap();
            let v: Value = serde_json::from_str(&line).unwrap();
            assert!(v["result"]["isError"] != true, "{name}: {v}");
            serde_json::from_str(v["result"]["content"][0]["text"].as_str().unwrap()).unwrap_or(Value::Null)
        };
        let list = call(1, "loom_instances", json!({}));
        let me = list["instances"].as_array().unwrap().iter().find(|i| i["pid"] == pid).cloned().unwrap_or_else(|| panic!("{list}"));
        assert_eq!(me["answers"], true, "{me}");
        assert_eq!(me["attached"], true, "{me}");
        let attached = call(2, "loom_attach", json!({ "pid": pid }));
        assert_eq!(attached["pid"], pid);
        let d = call(3, "loom_diagnostics", json!({}));
        assert_eq!(d["interaction"]["nav_mode"], "None", "{d}");
        let st = call(4, "loom_state", json!({}));
        assert_eq!(st["nodes"][0]["name"], "Box001");
        let logs = call(5, "loom_logs", json!({ "lines": 20 }));
        assert!(logs["text"].as_str().unwrap().contains("loom::cmd"), "{logs}");
        drop(stdin);
        let _ = child.wait();
    } else {
        eprintln!("{} not built; skipping the MCP part", mcp.display());
    }
    let _ = std::fs::remove_dir_all(&dir);
}
