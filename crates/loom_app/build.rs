//! Records the commit this binary is built from; the self-updater compares
//! it with the newest release.
use std::process::Command;

fn main() {
    let commit = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default();
    let branch = std::env::var("LOOM_UPDATE_CHANNEL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        })
        .map(|b| if b.is_empty() || b == "HEAD" || b == "nightly" { "main".to_owned() } else { b })
        .unwrap_or_else(|| "main".to_owned());
    println!("cargo:rustc-env=LOOM_COMMIT={commit}");
    println!("cargo:rustc-env=LOOM_BRANCH={branch}");
    println!("cargo:rerun-if-env-changed=LOOM_UPDATE_CHANNEL");
    let git_dir = Command::new("git").args(["rev-parse", "--absolute-git-dir"]).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    if let Some(dir) = git_dir.filter(|d| !d.is_empty()) {
        println!("cargo:rerun-if-changed={dir}/HEAD");
        println!("cargo:rerun-if-changed={dir}/refs/heads");
    }
}
