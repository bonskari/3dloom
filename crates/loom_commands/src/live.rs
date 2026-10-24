//! Where a running 3dLoom can be found and where it writes its log. The app
//! creates these; the MCP server and other tools read them.
//!
//! - Control sockets: `$XDG_RUNTIME_DIR/3dloom/<pid>.sock` (the directory is
//!   the user's only), and `current` there names the newest instance's socket.
//! - Logs: `$XDG_STATE_HOME/3dloom/logs/loom-<date>-<pid>.log`
//!   (`~/.local/state/...` by default, `%LOCALAPPDATA%\3dloom\logs` on Windows).

use std::path::{Path, PathBuf};

/// How many log files are kept (one per run, newest first).
pub const KEEP_LOGS: usize = 10;

/// The directory holding the control sockets.
pub fn runtime_dir() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()) {
        Some(d) => PathBuf::from(d).join("3dloom"),
        // No runtime dir (some minimal sessions): a per-user directory in the temp dir.
        None => std::env::temp_dir().join(format!("3dloom-{}", std::env::var("USER").unwrap_or_else(|_| "user".into()))),
    }
}

/// The directory holding the log files.
pub fn log_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        return std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("3dloom").join("logs"));
    }
    let base = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("state")))?;
    Some(base.join("3dloom").join("logs"))
}

pub fn socket_path(dir: &Path, pid: u32) -> PathBuf {
    dir.join(format!("{pid}.sock"))
}

/// The link (or, where links fail, a file with the path) to the newest instance's socket.
pub fn current_path(dir: &Path) -> PathBuf {
    dir.join("current")
}

/// A control socket in the runtime directory.
#[derive(Debug, Clone, PartialEq)]
pub struct Instance {
    pub pid: u32,
    pub socket: PathBuf,
    /// The process still exists (a crashed run leaves its socket behind).
    pub alive: bool,
    /// `current` points here.
    pub current: bool,
}

/// The sockets in `dir`, newest first.
pub fn instances(dir: &Path) -> Vec<Instance> {
    let current = current_target(dir);
    let mut found: Vec<(std::time::SystemTime, Instance)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let pid: u32 = path.file_name()?.to_str()?.strip_suffix(".sock")?.parse().ok()?;
            let time = e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
            let current = current.as_deref() == Some(path.as_path());
            Some((time, Instance { pid, alive: process_alive(pid), socket: path, current }))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.pid.cmp(&a.1.pid)));
    found.into_iter().map(|(_, i)| i).collect()
}

/// The socket `current` names, if any.
pub fn current_target(dir: &Path) -> Option<PathBuf> {
    let link = current_path(dir);
    let target = std::fs::read_link(&link).or_else(|_| std::fs::read_to_string(&link).map(|s| PathBuf::from(s.trim()))).ok()?;
    Some(if target.is_absolute() { target } else { dir.join(target) })
}

/// Whether a process with this id exists (Linux: `/proc`; elsewhere assumed).
pub fn process_alive(pid: u32) -> bool {
    if cfg!(target_os = "linux") {
        Path::new(&format!("/proc/{pid}")).exists()
    } else {
        true
    }
}

/// `loom-<yyyy-mm-dd>-<pid>.log` for a time in seconds since the epoch (UTC).
pub fn log_file_name(unix_secs: u64, pid: u32) -> String {
    let (y, m, d) = civil_date(unix_secs / 86_400);
    format!("loom-{y:04}-{m:02}-{d:02}-{pid}.log")
}

/// `yyyy-mm-ddThh:mm:ss.mmmZ` (UTC).
pub fn timestamp(t: std::time::SystemTime) -> String {
    let d = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs();
    let (y, m, day) = civil_date(secs / 86_400);
    let s = secs % 86_400;
    format!("{y:04}-{m:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z", s / 3600, s / 60 % 60, s % 60, d.subsec_millis())
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian.
fn civil_date(days: u64) -> (i64, u32, u32) {
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// The log files in `dir` (`loom-*.log`), newest first.
pub fn log_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("loom-") && n.ends_with(".log")))
        .map(|p| (std::fs::metadata(&p).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), p))
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    files.into_iter().map(|(_, p)| p).collect()
}

/// Deletes all but the `keep` newest log files (and their `.1` halves).
pub fn prune_logs(dir: &Path, keep: usize) {
    for old in log_files(dir).into_iter().skip(keep) {
        let _ = std::fs::remove_file(&old);
        let _ = std::fs::remove_file(old.with_extension("log.1"));
    }
}

/// The last `lines` lines of a text file (at most its last 4 MiB are read).
pub fn tail(path: &Path, lines: usize) -> std::io::Result<String> {
    use std::io::{Read, Seek, SeekFrom};
    const MAX: u64 = 4 << 20;
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    let start = len.saturating_sub(MAX);
    f.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    let text = String::from_utf8_lossy(&buf);
    // A cut first line is dropped.
    let text = if start > 0 { text.split_once('\n').map_or("", |(_, rest)| rest) } else { &text };
    let all: Vec<&str> = text.lines().collect();
    Ok(all[all.len().saturating_sub(lines)..].join("\n"))
}

/// The control socket's own requests (besides every registry command, sent as
/// `{"cmd": "<name>", "args": {..}}`): name, description and JSON schema of
/// the other fields. The MCP server makes a tool of each.
pub fn app_commands() -> Vec<(&'static str, &'static str, serde_json::Value)> {
    use serde_json::json;
    let none = || json!({ "type": "object", "properties": {}, "required": [] });
    vec![
        ("ping", "Check that the instance answers; returns its pid, version, commit, socket and log file.", none()),
        ("screenshot", "Capture the window as rendered now into a PNG.", json!({ "type": "object", "properties": {
            "path": { "type": "string", "description": "Where to save the PNG (default: a file in the temp dir)" }
        }, "required": [] })),
        ("diagnostics", "Live input and interaction state: egui modifiers, pointer buttons and focus, open popups and layers, drag/create/region/menu sessions, the camera navigator, dialogs, the active viewport and its camera, tool and sub-object level, selection lock, keyboard override, snaps, preferences and the last raw input events.", none()),
        ("reset_interaction", "Clear stuck interaction state (drags, region select, Alt-click, create sessions, quad menu, egui popups and drags, injected held modifiers). Drags in progress end where they are.", none()),
        ("ui_hit", "What is at a window point: panel, viewport index, egui layer and, if the pointer is there, the hovered widgets.", json!({ "type": "object", "properties": {
            "x": { "type": "number" }, "y": { "type": "number" }
        }, "required": ["x", "y"] })),
        ("ui_tree", "The window layout: panels, viewports with their kinds, floating layers (menus, popups, windows) and the open dialog.", none()),
        ("input", "Inject pointer and key input into the running window, one event per frame; replies when all were delivered. Event types: move {pos}, button {pos, button, pressed, mods}, click {pos, button, mods}, drag {from, to, button, mods, steps}, key {key, pressed?, mods}, text {text}, scroll {delta, mods}, modifiers {mods} (hold until changed), leave, wait {frames}. pos/from/to/delta are [x, y] in window points, button left|right|middle, mods a list of alt, ctrl, shift, key an egui key name (A, Escape, F3, Space, Delete, ...).", json!({ "type": "object", "properties": {
            "events": { "type": "array", "items": { "type": "object" } }
        }, "required": ["events"] })),
        ("log_path", "This run's log file.", none()),
        ("log_tail", "The last lines of this run's log file (commands with results, warnings, errors, panics, updates).", json!({ "type": "object", "properties": {
            "lines": { "type": "integer", "description": "How many lines (default 100)" }
        }, "required": [] })),
    ]
}
