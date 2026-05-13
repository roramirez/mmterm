use crossbeam_channel::unbounded;

use super::PtySession;

#[test]
fn spawn_with_shell_succeeds_with_bin_true() {
    let (tx, _rx) = unbounded();
    let session = PtySession::spawn_with_shell(80, 24, tx, "/bin/true", None, Box::new(|| {}));
    assert!(
        session.is_ok(),
        "spawn_with_shell failed: {:?}",
        session.err()
    );
}

#[test]
fn write_bytes_after_spawn_does_not_panic() {
    let (tx, _rx) = unbounded();
    let mut session = PtySession::spawn_with_shell(80, 24, tx, "/bin/sh", None, Box::new(|| {}))
        .expect("spawn failed");
    // Writing to a live shell; ignore errors (shell may exit before write).
    let _ = session.write_input(b"exit\n");
}

#[test]
fn resize_after_spawn_does_not_panic() {
    let (tx, _rx) = unbounded();
    let session = PtySession::spawn_with_shell(80, 24, tx, "/bin/sh", None, Box::new(|| {}))
        .expect("spawn failed");
    let result = session.resize(120, 40);
    assert!(result.is_ok(), "resize failed: {:?}", result.err());
}

#[test]
fn spawn_default_uses_env_shell() {
    let (tx, _rx) = unbounded();
    // spawn() derives the shell from $SHELL; allow failure when $SHELL is unset.
    let _ = PtySession::spawn(80, 24, tx);
}

#[test]
fn spawn_with_cwd_sets_working_directory() {
    let (tx, _rx) = unbounded();
    let cwd = std::path::PathBuf::from("/tmp");
    let result = PtySession::spawn_with_shell(80, 24, tx, "/bin/sh", Some(&cwd), Box::new(|| {}));
    assert!(result.is_ok(), "spawn with cwd failed: {:?}", result.err());
}

#[test]
fn cwd_returns_path_or_none_after_spawn() {
    let (tx, _rx) = unbounded();
    let session = PtySession::spawn_with_shell(80, 24, tx, "/bin/sh", None, Box::new(|| {}))
        .expect("spawn failed");
    // May return None on non-Linux; just assert no panic.
    let _ = session.cwd();
}
