//! Self-update from signed per-branch GitHub releases.
//!
//! Each branch published by CI uploads a signed `latest.json` and `latest.json.sig`
//! under its release tag (`nightly` for `main`, `branch-<slug>` for other branches).
//! After the window opens, a background check fetches available release branches,
//! downloads the selected branch's `latest.json` and ed25519 signature, checks the
//! signature against the key built into the program, and lists changes since this
//! version. Only when the user picks Update does it download the binary for this
//! platform, check its SHA-256, and replace the executable; the new version runs
//! after a restart.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;

/// The release key's public half (the private half signs in CI).
const PUBLIC_KEY: [u8; 32] = hex32("f02444c7ca846b68a5f543f5e76439dd7753af32a225890ff7b6904d18a424bd");
const RELEASES_DOWNLOAD_BASE: &str = "https://github.com/bonskari/3dloom/releases/download";
const RELEASES_API_URL: &str = "https://api.github.com/repos/bonskari/3dloom/releases?per_page=50";

/// `latest.json`: the branch/commit the release was built from and its binaries.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub branch: Option<String>,
    pub commit: String,
    pub assets: BTreeMap<String, Asset>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Asset {
    pub name: String,
    pub sha256: String,
}

/// Whether this run shows the Update menu: enabled for interactive runs unless
/// remote-controlled or explicitly disabled via `LOOM_NO_UPDATE`.
pub fn enabled(_channel: Option<&str>, control_port: Option<&str>, no_update: Option<&str>) -> bool {
    control_port.is_none_or(|p| p.trim().is_empty() || p.trim() == "0")
        && no_update.is_none_or(|v| v.is_empty() || v == "0")
}

/// Normalizes a branch or channel name (`nightly` / empty -> `main`).
pub fn normalize_branch(branch: &str) -> String {
    let b = branch.trim();
    if b.is_empty() || b == "nightly" || b == "HEAD" {
        "main".to_owned()
    } else {
        b.to_owned()
    }
}

/// Maps a Git branch name to its GitHub release tag (`main` -> `nightly`,
/// other branches -> `branch-<sanitized-slug>`).
pub fn release_tag(branch: &str) -> String {
    let b = normalize_branch(branch);
    if b == "main" {
        return "nightly".to_owned();
    }
    if let Some(rest) = b.strip_prefix("branch-") {
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) {
            return b;
        }
    }
    let slug: String = b
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "nightly".to_owned()
    } else {
        format!("branch-{slug}")
    }
}

/// Download base URL for a given branch's release assets.
pub fn release_url_for_branch(branch: &str) -> String {
    format!("{RELEASES_DOWNLOAD_BASE}/{}", release_tag(branch))
}

/// Default update branches always shown in the in-app selector.
pub const DEFAULT_BRANCHES: [&str; 2] = ["main", "experimental"];

/// Parses GitHub Releases API JSON and returns branches that publish a signed
/// `latest.json` + `latest.json.sig`, always starting with `main` and `experimental`.
pub fn parse_release_branches(body: &[u8]) -> Vec<String> {
    let mut out: Vec<String> = DEFAULT_BRANCHES.iter().map(|&b| b.to_owned()).collect();
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) else {
        return out;
    };
    let Some(releases) = v.as_array() else {
        return out;
    };
    for rel in releases {
        let Some(assets) = rel["assets"].as_array() else { continue };
        let has_manifest = assets.iter().any(|a| a["name"].as_str() == Some("latest.json"));
        let has_sig = assets.iter().any(|a| a["name"].as_str() == Some("latest.json.sig"));
        if !(has_manifest && has_sig) {
            continue;
        }
        let tag = rel["tag_name"].as_str().unwrap_or("");
        let title = rel["name"].as_str().unwrap_or("");
        let branch = if tag == "nightly" {
            "main".to_owned()
        } else if let Some(inner) = title.strip_prefix("3dLoom (").and_then(|s| s.strip_suffix(')')) {
            normalize_branch(inner)
        } else if let Some(rest) = tag.strip_prefix("branch-") {
            normalize_branch(rest)
        } else if !tag.is_empty() {
            normalize_branch(tag)
        } else {
            continue;
        };
        if !out.contains(&branch) {
            out.push(branch);
        }
    }
    out
}

/// Queries GitHub Releases for available signed branches, ensuring `current`,
/// `main`, and `experimental` are always present.
pub fn fetch_branches(current: &str) -> Vec<String> {
    let mut branches = match fetch(RELEASES_API_URL, 10) {
        Ok(body) => parse_release_branches(&body),
        Err(_) => DEFAULT_BRANCHES.iter().map(|&b| b.to_owned()).collect(),
    };
    let current = normalize_branch(current);
    if !branches.contains(&current) {
        branches.push(current);
    }
    branches
}

fn branch_pref_path() -> Option<std::path::PathBuf> {
    #[cfg(unix)]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
            .map(|dir| dir.join("3dloom").join("update-branch"))
    }
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA")
            .map(std::path::PathBuf::from)
            .map(|dir| dir.join("3dloom").join("update-branch"))
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

/// Loads the user's persisted update branch preference, falling back to `default_branch`.
pub fn load_saved_branch(default_branch: &str) -> String {
    if let Some(path) = branch_pref_path() {
        if let Ok(contents) = std::fs::read_to_string(path) {
            let trimmed = contents.trim();
            if !trimmed.is_empty() {
                return normalize_branch(trimmed);
            }
        }
    }
    normalize_branch(default_branch)
}

/// Persists the user's selected update branch preference locally.
pub fn save_branch(branch: &str) {
    let Some(path) = branch_pref_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, format!("{}\n", normalize_branch(branch)));
}

/// This platform's key in the manifest.
pub fn platform() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("linux-x64"),
        ("windows", "x86_64") => Some("windows-x64"),
        _ => None,
    }
}

/// Parses `latest.json` after checking its signature (64 raw bytes).
pub fn verify(manifest: &[u8], signature: &[u8], key: &[u8; 32]) -> Result<Manifest, String> {
    let key = VerifyingKey::from_bytes(key).map_err(|e| format!("bad public key: {e}"))?;
    let sig: [u8; 64] = signature.try_into().map_err(|_| format!("signature is {} bytes, not 64", signature.len()))?;
    key.verify(manifest, &Signature::from_bytes(&sig)).map_err(|_| "the release signature does not match".to_owned())?;
    serde_json::from_slice(manifest).map_err(|e| format!("bad latest.json: {e}"))
}

/// The asset to install, if the release is another commit than this build.
pub fn pending<'a>(manifest: &'a Manifest, own_commit: &str, platform: &str) -> Option<&'a Asset> {
    (manifest.commit != own_commit).then(|| manifest.assets.get(platform)).flatten()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// The signed manifest and this platform's asset for `branch`, when the release is another commit.
pub fn check_branch(own_commit: &str, branch: &str) -> Result<Option<(Manifest, Asset)>, String> {
    let platform = platform().ok_or("no release build for this platform")?;
    let base_url = release_url_for_branch(branch);
    let manifest = fetch(&format!("{base_url}/latest.json"), 15)?;
    let signature = fetch(&format!("{base_url}/latest.json.sig"), 15)?;
    let manifest = verify(&manifest, &signature, &PUBLIC_KEY)?;
    let asset = pending(&manifest, own_commit, platform).cloned();
    Ok(asset.map(|a| (manifest, a)))
}

/// The first lines of the commit messages from `own` to `latest`, newest
/// first, from the public repository (empty when they cannot be fetched).
pub fn changes(own: &str, latest: &str) -> Vec<String> {
    let url = format!("https://api.github.com/repos/bonskari/3dloom/compare/{own}...{latest}");
    let Ok(body) = fetch(&url, 15) else { return Vec::new() };
    commit_titles(&body)
}

/// Commit titles from a GitHub compare response, newest first.
pub fn commit_titles(body: &[u8]) -> Vec<String> {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) else { return Vec::new() };
    let mut out: Vec<String> = v["commits"]
        .as_array()
        .map(|cs| cs.iter().filter_map(|c| c["commit"]["message"].as_str()).map(|m| m.lines().next().unwrap_or("").to_owned()).filter(|t| !t.is_empty()).collect())
        .unwrap_or_default();
    out.reverse();
    out
}

/// Downloads the asset from `branch`'s release, checks its SHA-256 and replaces this executable.
pub fn install_from_branch(branch: &str, manifest: &Manifest, asset: &Asset) -> Result<(), String> {
    let base_url = release_url_for_branch(branch);
    let binary = fetch(&format!("{base_url}/{}", asset.name), 300)?;
    if sha256_hex(&binary) != asset.sha256 {
        return Err(format!("{} does not match its checksum", asset.name));
    }
    let staged = std::env::temp_dir().join(format!("loom-update-{}", &manifest.commit[..manifest.commit.len().min(12)]));
    std::fs::write(&staged, &binary).map_err(|e| format!("writing the update: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755));
    }
    let replaced = self_replace::self_replace(&staged).map_err(|e| format!("replacing the program: {e}"));
    let _ = std::fs::remove_file(&staged);
    replaced
}

/// The program's own path, taken before the update replaces the file: on
/// Linux the running image's path afterwards reads "<path> (deleted)".
pub fn own_path() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok().map(|p| live_path(&p))
}

/// Drops the " (deleted)" Linux appends to the path of a replaced executable.
pub fn live_path(p: &std::path::Path) -> std::path::PathBuf {
    let s = p.to_string_lossy();
    match s.strip_suffix(" (deleted)") {
        Some(live) => std::path::PathBuf::from(live),
        None => p.to_path_buf(),
    }
}

/// Starts the replaced executable at `exe` (its path from before the
/// update) with this run's arguments; the caller then exits.
pub fn relaunch(exe: &std::path::Path) {
    let _ = std::process::Command::new(exe).args(std::env::args_os().skip(1)).spawn();
}

fn fetch(url: &str, timeout_secs: u64) -> Result<Vec<u8>, String> {
    let resp = ureq::get(url).timeout(std::time::Duration::from_secs(timeout_secs)).call().map_err(|e| format!("{url}: {e}"))?;
    let mut out = Vec::new();
    resp.into_reader().take(512 * 1024 * 1024).read_to_end(&mut out).map_err(|e| format!("{url}: {e}"))?;
    Ok(out)
}

const fn hex32(s: &str) -> [u8; 32] {
    let b = s.as_bytes();
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = nibble(b[2 * i]) << 4 | nibble(b[2 * i + 1]);
        i += 1;
    }
    out
}

const fn nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => panic!("not a lowercase hex digit"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn signed(json: &str) -> (Vec<u8>, Vec<u8>, [u8; 32]) {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let sig = key.sign(json.as_bytes()).to_bytes().to_vec();
        (json.as_bytes().to_vec(), sig, key.verifying_key().to_bytes())
    }

    const JSON: &str = r#"{"commit":"abc123","assets":{"linux-x64":{"name":"loom-linux-x64","sha256":"00"},"windows-x64":{"name":"loom-windows-x64.exe","sha256":"11"}}}"#;

    #[test]
    fn a_good_signature_gives_the_manifest() {
        let (m, s, k) = signed(JSON);
        let man = verify(&m, &s, &k).unwrap();
        assert_eq!(man.commit, "abc123");
        assert_eq!(man.assets["windows-x64"].name, "loom-windows-x64.exe");
    }

    #[test]
    fn a_tampered_manifest_or_wrong_key_is_refused() {
        let (mut m, s, k) = signed(JSON);
        m[12] ^= 1;
        assert!(verify(&m, &s, &k).is_err());
        let (m, s, _) = signed(JSON);
        assert!(verify(&m, &s, &PUBLIC_KEY).is_err(), "signed with another key");
        assert!(verify(&m, &s[..63], &k).is_err());
    }

    #[test]
    fn only_another_commit_is_installed() {
        let (m, s, k) = signed(JSON);
        let man = verify(&m, &s, &k).unwrap();
        assert!(pending(&man, "abc123", "linux-x64").is_none());
        assert_eq!(pending(&man, "def456", "linux-x64").map(|a| a.name.as_str()), Some("loom-linux-x64"));
        assert!(pending(&man, "def456", "mac-arm64").is_none());
    }

    #[test]
    fn updates_only_in_interactive_builds_run_by_a_person() {
        assert!(enabled(Some("nightly"), None, None));
        assert!(enabled(None, None, None), "interactive local builds also show the Update branch menu");
        assert!(!enabled(Some("nightly"), Some("7841"), None), "remote-controlled");
        assert!(!enabled(Some("nightly"), None, Some("1")), "the launcher handles it");
        assert!(enabled(Some("nightly"), Some("0"), Some("0")));
    }

    #[test]
    fn release_tag_and_discovery_map_branches_cleanly() {
        assert_eq!(release_tag("main"), "nightly");
        assert_eq!(release_tag("nightly"), "nightly");
        assert_eq!(release_tag("experimental"), "branch-experimental");
        assert_eq!(release_tag("dev"), "branch-dev");
        assert_eq!(release_tag("feat/ik-contacts"), "branch-feat-ik-contacts");
        let api = br#"[
            {"tag_name":"nightly","name":"3dLoom nightly","assets":[{"name":"latest.json"},{"name":"latest.json.sig"}]},
            {"tag_name":"branch-experimental","name":"3dLoom (experimental)","assets":[{"name":"latest.json"},{"name":"latest.json.sig"}]},
            {"tag_name":"v1.0","name":"Unsigned release","assets":[{"name":"loom-linux-x64"}]}
        ]"#;
        assert_eq!(parse_release_branches(api), vec!["main", "experimental"]);
        assert_eq!(parse_release_branches(b"[]"), vec!["main", "experimental"]);
    }

    #[test]
    fn a_replaced_executable_path_drops_the_deleted_mark() {
        assert_eq!(live_path(std::path::Path::new("/tmp/upd/loom (deleted)")), std::path::PathBuf::from("/tmp/upd/loom"));
        assert_eq!(live_path(std::path::Path::new("/opt/loom")), std::path::PathBuf::from("/opt/loom"));
    }

    #[test]
    fn compare_response_gives_commit_titles_newest_first() {
        let body = br#"{"commits":[{"commit":{"message":"First change\n\nbody"}},{"commit":{"message":"Second change"}}]}"#;
        assert_eq!(commit_titles(body), vec!["Second change", "First change"]);
        assert!(commit_titles(b"not json").is_empty());
    }

    #[test]
    fn checksum_is_lowercase_hex_sha256() {
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(PUBLIC_KEY[0], 0xf0);
    }
}
