//! The persistent log: every run writes `loom-<date>-<pid>.log` in the log
//! directory (`loom_commands::live::log_dir`), the newest `KEEP_LOGS` are
//! kept. The file gets warnings and errors from every crate (wgpu, Bevy
//! through `tracing`'s log records), info from 3dLoom's own targets
//! (commands, updates, control socket) and panics with a backtrace. Standard
//! error keeps the `RUST_LOG` filter (default `warn`).

use loom_commands::live;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// A log file is restarted (the old half kept as `.log.1`) past this size.
const MAX_FILE_BYTES: u64 = 32 << 20;

struct FileSink {
    path: PathBuf,
    file: std::fs::File,
    written: u64,
}

impl FileSink {
    fn write_line(&mut self, line: &str) {
        if self.written + line.len() as u64 > MAX_FILE_BYTES {
            let _ = std::fs::rename(&self.path, self.path.with_extension("log.1"));
            if let Ok(f) = open(&self.path) {
                self.file = f;
                self.written = 0;
            }
        }
        if self.file.write_all(line.as_bytes()).is_ok() {
            self.written += line.len() as u64;
        }
    }
}

fn open(path: &Path) -> std::io::Result<std::fs::File> {
    let mut o = std::fs::OpenOptions::new();
    o.create(true).append(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    o.open(path)
}

pub struct Logger {
    stderr: env_filter::Filter,
    file: Mutex<Option<FileSink>>,
}

/// Whether a record goes to the file: warnings from everyone, info from 3dLoom.
fn file_wants(meta: &log::Metadata) -> bool {
    meta.level() <= log::Level::Warn || (meta.level() <= log::Level::Info && (meta.target().starts_with("loom")))
}

impl log::Log for Logger {
    fn enabled(&self, meta: &log::Metadata) -> bool {
        file_wants(meta) || self.stderr.enabled(meta)
    }

    fn log(&self, record: &log::Record) {
        let to_file = file_wants(record.metadata());
        let to_stderr = self.stderr.matches(record);
        if !to_file && !to_stderr {
            return;
        }
        let line = format!("{} {:<5} {}: {}\n", live::timestamp(std::time::SystemTime::now()), record.level(), record.target(), record.args());
        if to_stderr {
            let _ = std::io::stderr().write_all(line.as_bytes());
        }
        if to_file {
            if let Some(sink) = self.file.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                sink.write_line(&line);
            }
        }
    }

    fn flush(&self) {}
}

/// Where this run's log is written, once `init` has opened it.
static PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn path() -> Option<PathBuf> {
    PATH.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Opens this run's log file in `dir` (pruning old ones) and returns its path.
fn open_run_file(dir: &Path) -> std::io::Result<(PathBuf, std::fs::File)> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let path = dir.join(live::log_file_name(secs, std::process::id()));
    let file = open(&path)?;
    live::prune_logs(dir, live::KEEP_LOGS);
    Ok((path, file))
}

/// Installs the logger and the panic hook. Without a log directory (or when
/// it cannot be written) only standard error is used.
pub fn init() {
    let stderr = env_filter::Builder::new().parse(&std::env::var("RUST_LOG").unwrap_or_else(|_| "warn".into())).build();
    let max = stderr.filter().max(log::LevelFilter::Info);
    let mut open_error = None;
    let sink = match live::log_dir().map(|d| open_run_file(&d)) {
        Some(Ok((path, file))) => {
            *PATH.lock().unwrap_or_else(|e| e.into_inner()) = Some(path.clone());
            Some(FileSink { path, file, written: 0 })
        }
        Some(Err(e)) => {
            open_error = Some(e.to_string());
            None
        }
        None => None,
    };
    if log::set_boxed_logger(Box::new(Logger { stderr, file: Mutex::new(sink) })).is_ok() {
        log::set_max_level(max);
    }
    if let Some(e) = open_error {
        log::warn!(target: "loom", "could not open the log file: {e}");
    }
    install_panic_hook();
    log::info!(target: "loom", "3dLoom {} (commit {}) pid {} started", env!("CARGO_PKG_VERSION"), env!("LOOM_COMMIT"), std::process::id());
}

/// Panics go to the log with a backtrace, then to the default hook.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let backtrace = std::backtrace::Backtrace::force_capture();
        log::error!(target: "loom::panic", "thread '{}' panicked: {info}\n{backtrace}", thread.name().unwrap_or("<unnamed>"));
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_gets_warnings_from_everyone_and_info_from_loom() {
        let m = |level, target| log::Metadata::builder().level(level).target(target).build();
        assert!(file_wants(&m(log::Level::Error, "wgpu_core::device")));
        assert!(file_wants(&m(log::Level::Warn, "bevy_render")));
        assert!(!file_wants(&m(log::Level::Info, "wgpu_core::device")));
        assert!(file_wants(&m(log::Level::Info, "loom::cmd")));
        assert!(file_wants(&m(log::Level::Info, "loom_app::update")));
        assert!(!file_wants(&m(log::Level::Debug, "loom::cmd")));
    }

    #[test]
    fn a_run_file_is_private_and_old_runs_are_pruned() {
        let dir = std::env::temp_dir().join(format!("loom-logging-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..live::KEEP_LOGS + 3 {
            let p = dir.join(format!("loom-2000-01-01-{i}.log"));
            std::fs::write(&p, "old").unwrap();
            std::fs::File::options().write(true).open(&p).unwrap().set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(i as u64)).unwrap();
        }
        let (path, mut file) = open_run_file(&dir).unwrap();
        writeln!(file, "hello").unwrap();
        assert_eq!(live::log_files(&dir).len(), live::KEEP_LOGS);
        assert_eq!(live::log_files(&dir)[0], path, "this run's file is the newest");
        assert_eq!(live::tail(&path, 1).unwrap(), "hello");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
            assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        }
    }

    #[test]
    fn a_full_file_restarts_and_keeps_its_older_half() {
        let dir = std::env::temp_dir().join(format!("loom-logging-rotate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("loom-x.log");
        let mut sink = FileSink { file: open(&path).unwrap(), path: path.clone(), written: MAX_FILE_BYTES - 3 };
        sink.write_line("new line\n");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new line\n");
        assert!(path.with_extension("log.1").exists());
    }
}
