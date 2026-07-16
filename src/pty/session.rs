use crossbeam_channel::Sender;
use portable_pty::{CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::thread;

pub struct PtySession {
    pub writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    child_pid: Option<u32>,
}

/// Classification of a single `Read::read` result from the PTY master.
///
/// The reader loop must distinguish a clean EOF (the shell exited normally and
/// closed its end of the PTY) from a genuine I/O error (e.g. EIO when the shell
/// crashed, EBADF on a closed fd). Collapsing both into a bare `break` discards
/// the cause and makes disconnections invisible under `RUST_LOG`.
#[derive(Debug)]
enum ReadOutcome {
    /// `read` returned 0 bytes — the shell closed the PTY (normal exit).
    Eof,
    /// `read` returned an I/O error — abnormal termination.
    Error(std::io::Error),
    /// `read` returned `n > 0` bytes to forward to the parser.
    Data(usize),
}

/// Maps a raw `read` result to a [`ReadOutcome`]. Pure and side-effect free so
/// the EOF-vs-error distinction can be unit-tested without a live PTY.
fn classify_read(r: std::io::Result<usize>) -> ReadOutcome {
    match r {
        Ok(0) => ReadOutcome::Eof,
        Ok(n) => ReadOutcome::Data(n),
        Err(e) => ReadOutcome::Error(e),
    }
}

/// Body of the PTY reader thread: forward bytes to the parser until EOF, an I/O
/// error, or a closed output channel — logging the cause of every termination.
fn pty_reader_loop(
    mut reader: Box<dyn Read + Send>,
    output_tx: Sender<Vec<u8>>,
    wakeup: Box<dyn Fn() + Send + 'static>,
) {
    let mut buf = [0u8; 4096];
    loop {
        match classify_read(reader.read(&mut buf)) {
            ReadOutcome::Eof => return log::debug!("PTY EOF (shell exited)"),
            ReadOutcome::Error(e) => return log::debug!("PTY read failed: {e}"),
            ReadOutcome::Data(n) if output_tx.send(buf[..n].to_vec()).is_ok() => wakeup(),
            ReadOutcome::Data(_) => return log::debug!("PTY output channel closed"),
        }
    }
}

impl PtySession {
    #[allow(dead_code)]
    pub fn spawn(cols: u16, rows: u16, output_tx: Sender<Vec<u8>>) -> anyhow::Result<Self> {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        Self::spawn_with_shell(cols, rows, output_tx, &shell, None, &[], Box::new(|| {}))
    }

    pub fn spawn_with_shell(
        cols: u16,
        rows: u16,
        output_tx: Sender<Vec<u8>>,
        shell: &str,
        cwd: Option<&PathBuf>,
        env: &[(String, String)],
        wakeup: Box<dyn Fn() + Send + 'static>,
    ) -> anyhow::Result<Self> {
        let pty_system = NativePtySystem::default();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let portable_pty::PtyPair { master, slave } = pair;

        let mut cmd = CommandBuilder::new(shell);
        cmd.env("TERM", "xterm-256color");
        for (k, v) in env {
            cmd.env(k, v);
        }
        if let Some(dir) = cwd {
            cmd.cwd(dir);
        }

        let mut child = slave.spawn_command(cmd)?;
        let child_pid = child.process_id();

        // Drop the slave fd now — once the shell process exits and closes its
        // copy, the master reader will get EIO/EOF, which lets us detect exit.
        drop(slave);

        // --- Zombie reaper ---
        // A process becomes a zombie (state Z in /proc/<pid>/status) when it
        // exits but its parent has not yet called waitpid(2) to collect the
        // exit status.  The kernel keeps the process table entry alive until
        // the parent reaps it, so an un-reaped child occupies a PID slot
        // indefinitely.
        //
        // In mmterm each tab and each pane split spawns one child shell.
        // Without an explicit wait the shells linger as zombies for the entire
        // lifetime of the application.
        //
        // We cannot call wait() on the main thread (it would block the event
        // loop) and we do not own a SIGCHLD handler, so we dedicate one
        // lightweight thread per session whose only job is to block on
        // child.wait() and then exit.  The thread is cheap — it spends almost
        // all of its life parked in the kernel — and it guarantees the child
        // is reaped promptly when the shell exits for any reason (user `exit`,
        // Ctrl-D, pane close, tab close, or crash).
        thread::spawn(move || {
            let _ = child.wait();
        });

        let reader = master.try_clone_reader()?;
        thread::spawn(move || pty_reader_loop(reader, output_tx, wakeup));

        let writer = master.take_writer()?;

        Ok(Self {
            writer,
            master,
            child_pid,
        })
    }

    /// Returns the OS PID of the child shell, if known.
    pub fn pid(&self) -> Option<u32> {
        self.child_pid
    }

    /// Returns the CWD of the shell process by reading /proc/<pid>/cwd.
    pub fn cwd(&self) -> Option<PathBuf> {
        let pid = self.child_pid?;
        std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
    }

    pub fn write_input(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()
    }

    pub fn resize(&self, cols: u16, rows: u16) -> anyhow::Result<()> {
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "session_test.rs"]
mod tests;
