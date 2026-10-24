//! Self-update from the nightly release.
//!
//! Release builds made by CI carry an update channel. Before any window opens
//! they fetch the release's `latest.json` and its ed25519 signature, check the
//! signature against the key built into the program, and when the release is
//! another commit they download the binary for this platform, check its
//! SHA-256, replace their own executable and continue the start in it.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;

/// The release key's public half (the private half signs in CI).
const PUBLIC_KEY: [u8; 32] = hex32("f02444c7ca846b68a5f543f5e76439dd7753af32a225890ff7b6904d18a424bd");
const RELEASE_URL: &str = "https://github.com/bonskari/3dloom/releases/download/nightly";

/// `latest.json`: the commit the release was built from and its binaries.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Manifest {
    pub commit: String,
    pub assets: BTreeMap<String, Asset>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Asset {
    pub name: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    UpToDate,
    /// The executable was replaced; the commit it now holds.
    Updated(String),
    Failed(String),
}

/// Whether this run updates itself: only CI release builds (they carry a
/// channel), and not when remote-controlled or told not to (the desktop
/// launcher already fetches the newest build).
pub fn enabled(channel: Option<&str>, control_port: Option<&str>, no_update: Option<&str>) -> bool {
    channel.is_some_and(|c| !c.is_empty())
        && control_port.is_none_or(|p| p.trim().is_empty() || p.trim() == "0")
        && no_update.is_none_or(|v| v.is_empty() || v == "0")
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

/// Checks the release and, when it is newer, installs it over this executable.
pub fn run(own_commit: &str) -> Status {
    match check_and_install(own_commit) {
        Ok(Some(commit)) => Status::Updated(commit),
        Ok(None) => Status::UpToDate,
        Err(e) => Status::Failed(e),
    }
}

fn check_and_install(own_commit: &str) -> Result<Option<String>, String> {
    let platform = platform().ok_or("no release build for this platform")?;
    // A quick check: an unreachable release must not hold up the start.
    let manifest = fetch(&format!("{RELEASE_URL}/latest.json"), 8)?;
    let signature = fetch(&format!("{RELEASE_URL}/latest.json.sig"), 8)?;
    let manifest = verify(&manifest, &signature, &PUBLIC_KEY)?;
    let Some(asset) = pending(&manifest, own_commit, platform) else { return Ok(None) };
    let binary = fetch(&format!("{RELEASE_URL}/{}", asset.name), 300)?;
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
    replaced?;
    Ok(Some(manifest.commit))
}

/// Continues this start in the replaced executable: the same process on Unix
/// (exec), a new one on Windows. Returns only if that fails.
pub fn relaunch(commit: &str) {
    let Ok(exe) = std::env::current_exe() else { return };
    let mut cmd = std::process::Command::new(exe);
    cmd.args(std::env::args_os().skip(1)).env("LOOM_UPDATED", commit);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let e = cmd.exec();
        eprintln!("3dLoom: starting the updated program failed: {e}");
    }
    #[cfg(not(unix))]
    {
        if cmd.spawn().is_ok() {
            std::process::exit(0);
        }
    }
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
    fn updates_only_in_release_builds_run_by_a_person() {
        assert!(enabled(Some("nightly"), None, None));
        assert!(!enabled(None, None, None), "local builds never update");
        assert!(!enabled(Some("nightly"), Some("7841"), None), "remote-controlled");
        assert!(!enabled(Some("nightly"), None, Some("1")), "the launcher handles it");
        assert!(enabled(Some("nightly"), Some("0"), Some("0")));
    }

    #[test]
    fn checksum_is_lowercase_hex_sha256() {
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(PUBLIC_KEY[0], 0xf0);
    }
}
