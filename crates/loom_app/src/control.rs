//! Line-delimited JSON control sockets.
//!
//! Every run listens on a Unix socket only the user can open
//! (`$XDG_RUNTIME_DIR/3dloom/<pid>.sock`, see `loom_commands::live`);
//! `LOOM_CONTROL_PORT` adds the localhost TCP port as before. Background
//! threads accept connections and forward each request to the UI thread,
//! which owns `LoomApp`. The connection thread blocks on the reply channel so
//! every request gets exactly one response line.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, OnceLock};
use std::thread;

pub struct Request {
    pub body: serde_json::Value,
    pub reply: Sender<serde_json::Value>,
}

/// Wakes the UI thread when a request arrives (set once the window exists),
/// so an idle window still answers without repainting all the time.
pub type Waker = Arc<OnceLock<eframe::egui::Context>>;

/// Answers one connection: a request per line, a reply per line.
pub fn serve(input: impl Read, mut out: impl Write, tx: &Sender<Request>, waker: &Waker) {
    for line in BufReader::new(input).lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let body: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                if writeln!(out, "{}", err(&format!("bad json: {e}"))).is_err() {
                    break;
                }
                continue;
            }
        };
        let (rtx, rrx) = channel();
        if tx.send(Request { body, reply: rtx }).is_err() {
            break;
        }
        if let Some(ctx) = waker.get() {
            ctx.request_repaint();
        }
        let resp = match rrx.recv() {
            Ok(v) => v,
            Err(_) => break,
        };
        if writeln!(out, "{resp}").is_err() {
            break;
        }
    }
}

/// Binds `127.0.0.1:port` and serves it into `tx`.
pub fn spawn_tcp(port: u16, tx: Sender<Request>, waker: Waker) -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let (tx, waker) = (tx.clone(), waker.clone());
            thread::spawn(move || {
                if let Ok(out) = stream.try_clone() {
                    serve(stream, out, &tx, &waker);
                }
            });
        }
    });
    Ok(())
}

/// This run's Unix control socket; removed (and `current` with it, when it
/// still points here) when dropped.
#[cfg(unix)]
pub struct UnixSocket {
    pub path: std::path::PathBuf,
    dir: std::path::PathBuf,
}

#[cfg(unix)]
impl Drop for UnixSocket {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        if loom_commands::live::current_target(&self.dir).as_deref() == Some(self.path.as_path()) {
            let _ = std::fs::remove_file(loom_commands::live::current_path(&self.dir));
        }
    }
}

/// Creates `dir` (0700, refusing one another user owns), binds `<pid>.sock`
/// there (0600), points `current` at it, removes sockets of runs that are
/// gone and serves the socket into `tx`.
#[cfg(unix)]
pub fn spawn_unix(dir: &std::path::Path, tx: Sender<Request>, waker: Waker) -> std::io::Result<UnixSocket> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    use std::os::unix::net::UnixListener;
    use loom_commands::live;
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    let meta = std::fs::symlink_metadata(dir)?;
    let own_uid = std::fs::metadata("/proc/self").map(|m| m.uid()).ok();
    if !meta.is_dir() || own_uid.is_some_and(|u| u != meta.uid()) {
        return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, format!("{} is not a directory of this user", dir.display())));
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    for stale in live::instances(dir).into_iter().filter(|i| !i.alive) {
        let _ = std::fs::remove_file(&stale.socket);
    }
    let path = live::socket_path(dir, std::process::id());
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    // `current`: replaced atomically so a reader never sees it missing.
    let tmp = dir.join(format!(".current-{}", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    std::os::unix::fs::symlink(path.file_name().expect("socket file name"), &tmp)?;
    std::fs::rename(&tmp, live::current_path(dir))?;
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let (tx, waker) = (tx.clone(), waker.clone());
            thread::spawn(move || {
                if let Ok(out) = stream.try_clone() {
                    serve(stream, out, &tx, &waker);
                }
            });
        }
    });
    Ok(UnixSocket { path, dir: dir.to_path_buf() })
}

fn err(msg: &str) -> serde_json::Value {
    serde_json::json!({ "ok": false, "error": msg })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixStream;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("loom-control-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// A UI thread stand-in: echoes each request's `cmd` back.
    fn echo_ui() -> Sender<Request> {
        let (tx, rx) = channel::<Request>();
        thread::spawn(move || {
            for req in rx {
                let _ = req.reply.send(serde_json::json!({ "ok": true, "cmd": req.body["cmd"] }));
            }
        });
        tx
    }

    fn ask(stream: &mut UnixStream, line: &str) -> serde_json::Value {
        writeln!(stream, "{line}").unwrap();
        let mut reply = String::new();
        BufReader::new(stream.try_clone().unwrap()).read_line(&mut reply).unwrap();
        serde_json::from_str(&reply).unwrap()
    }

    #[test]
    fn a_line_gets_one_reply_and_bad_json_an_error() {
        let (a, b) = UnixStream::pair().unwrap();
        let tx = echo_ui();
        let waker = Waker::default();
        thread::spawn(move || serve(b.try_clone().unwrap(), b, &tx, &waker));
        let mut a = a;
        assert_eq!(ask(&mut a, r#"{"cmd":"ping"}"#)["cmd"], "ping");
        let bad = ask(&mut a, "{not json");
        assert_eq!(bad["ok"], false);
        assert!(bad["error"].as_str().unwrap().starts_with("bad json"));
        // Blank lines are skipped, the connection stays usable.
        assert_eq!(ask(&mut a, "\n{\"cmd\":\"State\"}")["cmd"], "State");
    }

    #[test]
    fn the_unix_socket_is_private_and_current_points_at_it() {
        let dir = temp_dir("unix");
        // A socket left by a run that is gone.
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("4294967295.sock"), "").unwrap();
        let sock = spawn_unix(&dir, echo_ui(), Waker::default()).unwrap();
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(std::fs::metadata(&sock.path).unwrap().permissions().mode() & 0o777, 0o600);
        assert!(!dir.join("4294967295.sock").exists(), "stale sockets are removed");
        let current = loom_commands::live::current_target(&dir).unwrap();
        assert_eq!(current, sock.path);
        let mut s = UnixStream::connect(dir.join("current")).unwrap();
        assert_eq!(ask(&mut s, r#"{"cmd":"ping"}"#)["ok"], true);
        let path = sock.path.clone();
        drop(sock);
        assert!(!path.exists() && !dir.join("current").exists(), "a closed run cleans up");
    }
}
