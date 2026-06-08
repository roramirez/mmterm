//! Once-per-day update check + per-OS apply. All network/IO shells out to
//! `curl`/`wget`/`shasum`; no extra Rust dependencies. Failure paths are silent.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::Sender;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const REPO: &str = "roramirez/mmterm";
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    /// Parse `MAJOR.MINOR.PATCH`, ignoring a leading `v` and any `+build` / `-pre` suffix.
    pub fn parse(s: &str) -> Option<Version> {
        let s = s.trim();
        let s = s.strip_prefix('v').unwrap_or(s);
        let core = s.split(['+', '-']).next().unwrap_or(s);
        let mut it = core.split('.');
        let major = it.next()?.parse().ok()?;
        let minor = it.next()?.parse().ok()?;
        let patch = it.next()?.parse().ok()?;
        if it.next().is_some() {
            return None;
        }
        Some(Version {
            major,
            minor,
            patch,
        })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Extract a version from a resolved latest-release URL like `.../releases/tag/v0.7.0`.
pub fn parse_latest_tag(resolved_url: &str) -> Option<Version> {
    let tag = resolved_url.trim_end_matches('/').rsplit("/tag/").next()?;
    if tag == resolved_url.trim_end_matches('/') {
        return None;
    }
    Version::parse(tag)
}

/// True when `interval` has elapsed since `last`, or there is no prior check.
pub fn should_check(last: Option<SystemTime>, now: SystemTime, interval: Duration) -> bool {
    match last {
        None => true,
        Some(t) => now
            .duration_since(t)
            .map(|d| d >= interval)
            .unwrap_or(false),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateState {
    pub last_check: Option<i64>,        // unix seconds
    pub latest_version: Option<String>, // last-seen latest, e.g. "0.7.0"
}

impl UpdateState {
    /// Load state; any missing/unreadable/garbage file yields `Default`.
    pub fn load(path: &Path) -> UpdateState {
        match std::fs::read_to_string(path) {
            Ok(s) => toml::from_str(&s).unwrap_or_default(),
            Err(_) => UpdateState::default(),
        }
    }

    /// Atomically persist (`.tmp` -> rename), creating the parent dir as needed.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, body)?;
        std::fs::rename(&tmp, path)
    }
}

/// `~/.config/mmterm/update-check.toml` (falls back to `./` if no config dir).
pub fn state_path() -> PathBuf {
    dirs_next::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mmterm")
        .join("update-check.toml")
}

/// `SystemTime` -> unix seconds (saturating at 0 for pre-epoch).
pub fn unix_secs(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Where a Linux self-replace would write, and whether it is eligible.
#[derive(Debug)]
#[allow(dead_code)] // used on linux only (drives self-replace eligibility)
pub enum InstallTarget {
    /// `current_exe()` lives in a user-writable dir; silent self-replace is allowed.
    Writable(PathBuf),
    /// System/packaged install (dir not writable) -> notify-only.
    NotWritable,
    /// Dev build (`+hash` version, or running from a `target/` dir) -> never self-update.
    DevBuild,
}

/// Decide eligibility from the running exe path and the baked version string.
#[allow(dead_code)] // used on linux only (called from on_update_available)
pub fn detect_install_target(current_exe: &Path, version_str: &str) -> InstallTarget {
    if version_str.contains('+') || current_exe.components().any(|c| c.as_os_str() == "target") {
        return InstallTarget::DevBuild;
    }
    let Some(dir) = current_exe.parent() else {
        return InstallTarget::NotWritable;
    };
    let probe = dir.join(".mmterm-write-probe");
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            InstallTarget::Writable(current_exe.to_path_buf())
        }
        Err(_) => InstallTarget::NotWritable,
    }
}

/// Find the sha256 for `file` in a `checksums-sha256.txt` body. Lines look like
/// `<hash>  artifacts/<file>` (or a bare `<hash>  <file>`); matches on the basename.
pub(crate) fn find_checksum(sums_text: &str, file: &str) -> Option<String> {
    sums_text.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let hash = it.next()?;
        let name = it.next()?.rsplit('/').next()?;
        (name == file).then(|| hash.to_lowercase())
    })
}

/// Atomically replace `target` with `src` (same-dir temp -> rename), mode 0755.
#[allow(dead_code)] // used on linux only (called from apply_linux_update)
pub fn atomic_replace(target: &Path, src: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let dir = target.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "target has no parent dir")
    })?;
    let tmp = dir.join(format!(".mmterm-update-{}.tmp", std::process::id()));
    std::fs::copy(src, &tmp)?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    std::fs::rename(&tmp, target)
}

/// Outcome delivered from the background worker to the UI thread.
#[derive(Debug)]
pub enum CheckOutcome {
    UpToDate,
    Newer(Version),
}

fn latest_release_url() -> String {
    format!("https://github.com/{REPO}/releases/latest")
}

/// Probe the latest release via the redirect; returns the newest published Version.
fn probe_latest() -> Option<Version> {
    let out = Command::new("curl")
        .args([
            "--proto",
            "=https",
            "--tlsv1.2",
            "-fsSL",
            "--max-time",
            "5",
            "-o",
            "/dev/null",
            "-w",
            "%{url_effective}",
            &latest_release_url(),
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&out.stdout);
    parse_latest_tag(url.trim())
}

/// Detached worker: throttle-gate, probe, persist state, report a newer Version.
/// Every failure path is a silent no-op.
pub fn spawn_check(current: Version, state_file: PathBuf, tx: Sender<CheckOutcome>) {
    std::thread::spawn(move || {
        let state = UpdateState::load(&state_file);
        let last = state
            .last_check
            .map(|s| UNIX_EPOCH + Duration::from_secs(s.max(0) as u64));
        if !should_check(last, SystemTime::now(), CHECK_INTERVAL) {
            if let Some(v) = state.latest_version.as_deref().and_then(Version::parse) {
                let _ = tx.send(if v > current {
                    CheckOutcome::Newer(v)
                } else {
                    CheckOutcome::UpToDate
                });
            }
            return;
        }
        let Some(latest) = probe_latest() else {
            return;
        };
        let new_state = UpdateState {
            last_check: Some(unix_secs(SystemTime::now())),
            latest_version: Some(latest.to_string()),
        };
        let _ = new_state.save(&state_file);
        let _ = tx.send(if latest > current {
            CheckOutcome::Newer(latest)
        } else {
            CheckOutcome::UpToDate
        });
    });
}

fn release_asset_url(file: &str) -> String {
    format!("https://github.com/{REPO}/releases/latest/download/{file}")
}

fn sha256_of(path: &Path) -> Option<String> {
    // Prefer shasum (always on macOS); fall back to sha256sum (Linux coreutils).
    let run = |prog: &str, args: &[&str]| -> Option<String> {
        let out = Command::new(prog).args(args).arg(path).output().ok()?;
        if !out.status.success() {
            return None;
        }
        String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .next()
            .map(str::to_lowercase)
    };
    run("shasum", &["-a", "256"]).or_else(|| run("sha256sum", &[]))
}

fn fetch(url: &str, out: &Path) -> std::io::Result<()> {
    let status = Command::new("curl")
        .args([
            "--proto",
            "=https",
            "--tlsv1.2",
            "-fsSL",
            "--max-time",
            "60",
            "-o",
        ])
        .arg(out)
        .arg(url)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("download failed"))
    }
}

fn gh_attest_ok(path: &Path) -> bool {
    // Opportunistic: if `gh` is absent the call errors -> treat as OK (no hard dep).
    // A present-but-failing verify (Ok(status) with non-success) -> false.
    match Command::new("gh")
        .args(["attestation", "verify"])
        .arg(path)
        .args(["--repo", REPO])
        .output()
    {
        Ok(o) => o.status.success(),
        Err(_) => true,
    }
}

/// Download `file` (TLS-pinned), verify sha256 against `checksums-sha256.txt`,
/// return the verified path inside `dir` together with the expected hash.
/// Fail-closed on any problem.
fn download_and_verify(file: &str, dir: &Path) -> std::io::Result<(PathBuf, String)> {
    let artifact = dir.join(file);
    let sums = dir.join("checksums-sha256.txt");
    fetch(&release_asset_url(file), &artifact)?;
    fetch(&release_asset_url("checksums-sha256.txt"), &sums)?;

    let body = std::fs::read_to_string(&sums)?;
    let want =
        find_checksum(&body, file).ok_or_else(|| std::io::Error::other("no checksum entry"))?;
    let got =
        sha256_of(&artifact).ok_or_else(|| std::io::Error::other("sha256 tool unavailable"))?;
    if got != want {
        return Err(std::io::Error::other("checksum mismatch"));
    }
    if !gh_attest_ok(&artifact) {
        return Err(std::io::Error::other("provenance verification failed"));
    }
    Ok((artifact, want))
}

/// 128 bits of entropy as lowercase hex, from /dev/urandom (Linux/macOS).
/// Falls back to pid+nanos-derived bytes only if /dev/urandom is unreadable.
fn rand_hex16() -> String {
    use std::io::Read;
    let mut buf = [0u8; 16];
    let ok = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut buf))
        .is_ok();
    if !ok {
        // Extremely unlikely fallback; mix pid + nanos so the name is still unique.
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mix = (std::process::id() as u128) ^ nanos;
        buf.copy_from_slice(&mix.to_le_bytes());
    }
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// Create a fresh private (0700) temp dir with an unpredictable name, atomically and
/// fail-closed: errors (AlreadyExists) if anything is squatting the name, and the mode is
/// applied at creation time (no 0755 window). Returns the created path.
fn make_private_tmp() -> std::io::Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    let dir = std::env::temp_dir().join(format!("mmterm-upd-{}", rand_hex16()));
    // Non-recursive create => fails if the dir already exists (fail-closed against squatting).
    // .mode(0o700) sets the directory mode atomically at mkdir() time.
    std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
    Ok(dir)
}

/// Linux: download the tarball, verify, extract `mmterm`, atomically replace `exe`.
#[cfg(target_os = "linux")]
pub fn apply_linux_update(exe: &Path) -> std::io::Result<()> {
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };
    let file = format!("mmterm-linux-{arch}.tar.gz");
    let dir = make_private_tmp()?;
    let result = (|| {
        let (tarball, expected) = download_and_verify(&file, &dir)?;
        // Re-verify right before use to close any swap window (defense in depth).
        if sha256_of(&tarball).as_deref() != Some(expected.as_str()) {
            return Err(std::io::Error::other("artifact changed after verification"));
        }
        let status = Command::new("tar")
            .arg("-xzf")
            .arg(&tarball)
            .arg("-C")
            .arg(&dir)
            .status()?;
        if !status.success() {
            return Err(std::io::Error::other("extract failed"));
        }
        let extracted = dir.join("mmterm");
        let meta = std::fs::symlink_metadata(&extracted)?;
        if !meta.file_type().is_file() {
            return Err(std::io::Error::other("archive entry is not a regular file"));
        }
        atomic_replace(exe, &extracted)
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// macOS: download the dmg, verify, move to ~/Downloads, open it for drag-install.
#[cfg(target_os = "macos")]
pub fn apply_macos_update() -> std::io::Result<()> {
    let file = "mmterm-macos-aarch64.dmg";
    let dir = make_private_tmp()?;
    let result = (|| {
        let (dmg, _hash) = download_and_verify(file, &dir)?;
        let dest = dirs_next::download_dir()
            .or_else(dirs_next::home_dir)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(file);
        std::fs::rename(&dmg, &dest).or_else(|_| std::fs::copy(&dmg, &dest).map(|_| ()))?;
        Command::new("open").arg(&dest).status()?;
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[cfg(test)]
#[path = "update_test.rs"]
mod tests;
