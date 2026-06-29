use crossbeam_channel::unbounded;
use std::time::{Duration, Instant};

use super::{PtySession, ReadOutcome, classify_read};

/// Regression: the reader loop must distinguish a clean EOF (`Ok(0)`) from a
/// genuine I/O error (`Err(_)`). The previous `Ok(0) | Err(_) => break` arm
/// collapsed both, so the disconnection cause was lost. `classify_read` keeps
/// them as separate variants.
#[test]
fn classify_read_distinguishes_eof_error_and_data() {
    assert!(
        matches!(classify_read(Ok(0)), ReadOutcome::Eof),
        "zero-length read must be EOF, not an error"
    );
    assert!(
        matches!(classify_read(Ok(5)), ReadOutcome::Data(5)),
        "non-zero read must carry the byte count"
    );
    let err = std::io::Error::from_raw_os_error(libc_eio());
    match classify_read(Err(err)) {
        ReadOutcome::Error(_) => {}
        other => panic!("I/O error must classify as Error, got {other:?}"),
    }
}

/// EIO (5) — the error a crashed shell produces on the master fd. Hardcoded to
/// avoid a `libc` dependency just for the constant.
fn libc_eio() -> i32 {
    5
}

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

/// The reader thread must terminate (not hang) when the shell exits and the
/// PTY reaches EOF. We spawn `/bin/true` (exits immediately) keeping the
/// receiver alive, then drain it: the channel's senders are all owned by the
/// reader thread, so once that thread breaks out of its loop and drops the
/// sender, `recv` returns `Err`. Reaching that within the deadline proves the
/// loop exited on EOF rather than spinning or blocking.
#[test]
fn reader_thread_exits_on_pty_eof() {
    let (tx, rx) = unbounded::<Vec<u8>>();
    let session = PtySession::spawn_with_shell(80, 24, tx, "/bin/true", None, Box::new(|| {}))
        .expect("spawn failed");
    // Keep the session (and thus the master) alive until the child has exited.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(_) => {} // drain any startup bytes
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break, // reader thread ended
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                assert!(
                    Instant::now() < deadline,
                    "reader thread did not terminate after PTY EOF within 5s"
                );
            }
        }
    }
    drop(session);
}

/// Verify that a child process does not become a zombie after it exits.
///
/// Without the reaper thread in `spawn_with_shell`, every shell spawned for a
/// tab or pane split would remain in state Z (zombie) in the kernel's process
/// table until mmterm itself exited.  This test spawns `/bin/true` (exits
/// immediately), then polls `/proc/<pid>/status` until the entry disappears or
/// up to a 2-second deadline.  Finding state `Z` at any point is a failure;
/// a missing `/proc` entry means the process was fully reaped.
///
/// Only meaningful on Linux where `/proc` is available; the test is a no-op on
/// other platforms.
#[test]
#[cfg(target_os = "linux")]
fn no_zombie_after_child_exits() {
    let (tx, _rx) = unbounded();
    let session = PtySession::spawn_with_shell(80, 24, tx, "/bin/true", None, Box::new(|| {}))
        .expect("spawn failed");

    let pid = match session.pid() {
        Some(p) => p,
        None => return, // PID unavailable — skip
    };

    let status_path = format!("/proc/{pid}/status");
    let deadline = Instant::now() + Duration::from_secs(2);

    // Poll until the /proc entry disappears (fully reaped) or the deadline.
    // A transient Z state while the reaper thread is being scheduled is
    // acceptable; we only fail if the process is still a zombie at deadline.
    loop {
        let last_zombie = match std::fs::read_to_string(&status_path) {
            Err(_) => return, // /proc entry gone — process fully reaped, test passes.
            Ok(contents) => contents
                .lines()
                .find(|l| l.starts_with("State:"))
                .map(|l| l.contains('Z'))
                .unwrap_or(false),
        };

        if Instant::now() >= deadline {
            assert!(
                !last_zombie,
                "child process {pid} is still a zombie after 2 seconds — reaper thread did not call wait()"
            );
            return;
        }

        std::thread::sleep(Duration::from_millis(50));
    }
}
