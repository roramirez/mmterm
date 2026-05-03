use crossbeam_channel::Sender;
use portable_pty::{CommandBuilder, NativePtySystem, PtyPair, PtySize, PtySystem};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::thread;

pub struct PtySession {
    pub writer: Box<dyn Write + Send>,
    _pair: PtyPair,
    child_pid: Option<u32>,
}

impl PtySession {
    #[allow(dead_code)]
    pub fn spawn(cols: u16, rows: u16, output_tx: Sender<Vec<u8>>) -> anyhow::Result<Self> {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        Self::spawn_with_shell(cols, rows, output_tx, &shell, None)
    }

    pub fn spawn_with_shell(
        cols: u16,
        rows: u16,
        output_tx: Sender<Vec<u8>>,
        shell: &str,
        cwd: Option<&PathBuf>,
    ) -> anyhow::Result<Self> {
        let pty_system = NativePtySystem::default();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = CommandBuilder::new(shell);
        cmd.env("TERM", "xterm-256color");
        if let Some(dir) = cwd {
            cmd.cwd(dir);
        }

        let child = pair.slave.spawn_command(cmd)?;
        let child_pid = child.process_id();

        let mut reader = pair.master.try_clone_reader()?;
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if output_tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        let writer = pair.master.take_writer()?;

        Ok(Self {
            writer,
            _pair: pair,
            child_pid,
        })
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
        self._pair.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }
}
