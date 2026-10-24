//! Instance and log file discovery shared by the app and the MCP server.

use loom_commands::live;
use std::path::PathBuf;

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("loom-live-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn log_file_names_carry_the_utc_date_and_pid() {
    assert_eq!(live::log_file_name(0, 7), "loom-1970-01-01-7.log");
    // 2026-10-24 12:00:00 UTC.
    assert_eq!(live::log_file_name(1_792_843_200, 4242), "loom-2026-10-24-4242.log");
    // A leap day.
    assert_eq!(live::log_file_name(951_782_400, 1), "loom-2000-02-29-1.log");
    let t = std::time::UNIX_EPOCH + std::time::Duration::from_millis(1_792_843_200_000 + 3_723_045);
    assert_eq!(live::timestamp(t), "2026-10-24T13:02:03.045Z");
}

#[test]
fn tail_returns_the_last_lines() {
    let d = temp_dir("tail");
    let p = d.join("loom-x.log");
    std::fs::write(&p, "one\ntwo\nthree\nfour\n").unwrap();
    assert_eq!(live::tail(&p, 2).unwrap(), "three\nfour");
    assert_eq!(live::tail(&p, 10).unwrap(), "one\ntwo\nthree\nfour");
    assert!(live::tail(&d.join("missing.log"), 3).is_err());
}

#[test]
fn prune_keeps_the_newest_logs() {
    let d = temp_dir("prune");
    for i in 0..5 {
        let p = d.join(format!("loom-2026-01-0{}-1.log", i + 1));
        std::fs::write(&p, "x").unwrap();
        // Distinct modification times, oldest first.
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000 + i * 60);
        std::fs::File::options().write(true).open(&p).unwrap().set_modified(t).unwrap();
    }
    std::fs::write(d.join("other.txt"), "kept").unwrap();
    live::prune_logs(&d, 2);
    let names: Vec<String> = live::log_files(&d).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
    assert_eq!(names, ["loom-2026-01-05-1.log", "loom-2026-01-04-1.log"]);
    assert!(d.join("other.txt").exists(), "only log files are pruned");
}

#[cfg(unix)]
#[test]
fn instances_list_sockets_and_mark_the_current_one() {
    let d = temp_dir("instances");
    let own = std::process::id();
    // A socket file of this (alive) process and one of a process that cannot exist.
    let mine = live::socket_path(&d, own);
    let _l = std::os::unix::net::UnixListener::bind(&mine).unwrap();
    std::fs::write(d.join("4294967295.sock"), "").unwrap();
    std::fs::write(d.join("notes.txt"), "").unwrap();
    std::os::unix::fs::symlink(format!("{own}.sock"), live::current_path(&d)).unwrap();
    let list = live::instances(&d);
    assert_eq!(list.len(), 2, "{list:?}");
    let me = list.iter().find(|i| i.pid == own).unwrap();
    assert!(me.alive && me.current);
    let gone = list.iter().find(|i| i.pid == u32::MAX).unwrap();
    assert!(!gone.alive && !gone.current);
    assert_eq!(live::current_target(&d).as_deref(), Some(mine.as_path()));
}

#[test]
fn runtime_and_log_dirs_follow_xdg() {
    // Read only: the variables are whatever the test runner has.
    if let Some(rt) = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()) {
        assert_eq!(live::runtime_dir(), PathBuf::from(rt).join("3dloom"));
    }
    if cfg!(unix) && std::env::var_os("XDG_STATE_HOME").is_none() {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        assert_eq!(live::log_dir().unwrap(), home.join(".local/state/3dloom/logs"));
    }
}
