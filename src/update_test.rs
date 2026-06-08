use super::*;
use std::time::{Duration, SystemTime};

#[test]
fn version_parse_variants() {
    assert_eq!(
        Version::parse("v0.7.0"),
        Some(Version {
            major: 0,
            minor: 7,
            patch: 0
        })
    );
    assert_eq!(
        Version::parse("0.7.0"),
        Some(Version {
            major: 0,
            minor: 7,
            patch: 0
        })
    );
    assert_eq!(
        Version::parse("0.6.1+abc123"),
        Some(Version {
            major: 0,
            minor: 6,
            patch: 1
        })
    );
    assert_eq!(
        Version::parse("1.2.3-rc1"),
        Some(Version {
            major: 1,
            minor: 2,
            patch: 3
        })
    );
    assert_eq!(Version::parse("v1.2"), None);
    assert_eq!(Version::parse("garbage"), None);
    assert_eq!(Version::parse(""), None);
}

#[test]
fn version_ordering() {
    let v = |s: &str| Version::parse(s).unwrap();
    assert!(v("0.7.0") > v("0.6.9"));
    assert!(v("1.0.0") > v("0.99.99"));
    assert!(v("0.6.2") > v("0.6.1"));
    assert!(!(v("0.6.1") > v("0.6.1")));
    assert!(!(v("0.6.1+hash") > v("0.6.1")));
    assert!(v("0.7.0") > v("0.6.1+hash"));
}

#[test]
fn parse_tag_from_resolved_url() {
    assert_eq!(
        parse_latest_tag("https://github.com/roramirez/mmterm/releases/tag/v0.7.0"),
        Some(Version {
            major: 0,
            minor: 7,
            patch: 0
        })
    );
    assert_eq!(
        parse_latest_tag("https://github.com/roramirez/mmterm/releases/tag/v0.7.0/"),
        Some(Version {
            major: 0,
            minor: 7,
            patch: 0
        })
    );
    assert_eq!(
        parse_latest_tag("https://github.com/roramirez/mmterm/releases/latest"),
        None
    );
    assert_eq!(parse_latest_tag("garbage"), None);
}

#[test]
fn should_check_throttle() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    let day = Duration::from_secs(24 * 60 * 60);
    assert!(should_check(None, now, day));
    assert!(should_check(
        Some(now - day - Duration::from_secs(1)),
        now,
        day
    ));
    assert!(!should_check(Some(now - Duration::from_secs(60)), now, day));
    assert!(!should_check(Some(now + day), now, day));
}

#[test]
fn update_state_round_trip_and_atomic() {
    let dir = std::env::temp_dir().join(format!("mmterm-upd-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("update-check.toml");

    let s = UpdateState {
        last_check: Some(123),
        latest_version: Some("0.7.0".into()),
    };
    s.save(&path).unwrap();
    let loaded = UpdateState::load(&path);
    assert_eq!(loaded.last_check, Some(123));
    assert_eq!(loaded.latest_version.as_deref(), Some("0.7.0"));
    let leftovers = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| e.file_name().to_string_lossy().contains(".tmp"));
    assert!(!leftovers);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn update_state_missing_or_garbage_is_default() {
    let missing = UpdateState::load(Path::new("/nonexistent/mmterm/x.toml"));
    assert_eq!(missing.last_check, None);
    assert_eq!(missing.latest_version, None);

    let dir = std::env::temp_dir().join(format!("mmterm-upd-bad-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bad.toml");
    std::fs::write(&path, b"not valid toml ===").unwrap();
    let s = UpdateState::load(&path);
    assert_eq!(s.last_check, None);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn detect_install_target_cases() {
    assert!(matches!(
        detect_install_target(&std::env::temp_dir().join("mmterm"), "0.6.1+abc"),
        InstallTarget::DevBuild
    ));
    let tp = Path::new("/home/u/mmterm/target/release/mmterm");
    assert!(matches!(
        detect_install_target(tp, "0.6.1"),
        InstallTarget::DevBuild
    ));
    let dir = std::env::temp_dir().join(format!("mmterm-it-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("mmterm");
    std::fs::write(&exe, b"x").unwrap();
    assert!(matches!(
        detect_install_target(&exe, "0.6.1"),
        InstallTarget::Writable(_)
    ));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn find_checksum_matches_and_rejects() {
    let body = "\
ABCDEF1234  artifacts/mmterm-linux-x86_64.tar.gz
00ff00ff  artifacts/mmterm-macos-aarch64.dmg
deadbeef  mmterm-linux-aarch64.tar.gz
";
    // path-prefixed entry, basename match, lowercased
    assert_eq!(
        find_checksum(body, "mmterm-linux-x86_64.tar.gz").as_deref(),
        Some("abcdef1234")
    );
    // dmg entry
    assert_eq!(
        find_checksum(body, "mmterm-macos-aarch64.dmg").as_deref(),
        Some("00ff00ff")
    );
    // bare (no path prefix) entry still matches on basename
    assert_eq!(
        find_checksum(body, "mmterm-linux-aarch64.tar.gz").as_deref(),
        Some("deadbeef")
    );
    // no matching entry -> None (this is the mismatch-rejection gate)
    assert_eq!(find_checksum(body, "mmterm-windows-x86_64.zip"), None);
    // empty body -> None
    assert_eq!(find_checksum("", "mmterm-linux-x86_64.tar.gz"), None);
}

#[test]
fn atomic_replace_preserves_and_cleans() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("mmterm-ar-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("mmterm");
    std::fs::write(&target, b"OLD").unwrap();
    let newbin = dir.join("new");
    std::fs::write(&newbin, b"NEWCONTENT").unwrap();

    atomic_replace(&target, &newbin).unwrap();

    assert_eq!(std::fs::read(&target).unwrap(), b"NEWCONTENT");
    let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o755);
    let leftovers = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| e.file_name().to_string_lossy().contains(".tmp"));
    assert!(!leftovers);
    std::fs::remove_dir_all(&dir).ok();
}
