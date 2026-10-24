//! Bakes the commit the binary was built from into the window title, so a
//! running 3dLoom shows which version it is.
use std::process::Command;

fn main() {
    let version = Command::new("git")
        .args(["log", "-1", "--format=%h %cd", "--date=format:%d.%m. %H:%M"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown commit".to_owned());
    println!("cargo:rustc-env=LOOM_VERSION={version}");
    // Rebuild when HEAD moves (checkout, commit, merge).
    let git_dir = Command::new("git").args(["rev-parse", "--git-dir"]).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    if let Some(dir) = git_dir.filter(|d| !d.is_empty()) {
        println!("cargo:rerun-if-changed={dir}/HEAD");
        println!("cargo:rerun-if-changed={dir}/refs/heads");
    }
}
