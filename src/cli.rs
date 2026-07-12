/// Returns `true` if `--version` or `-V` appears in the given argument iterator.
pub(crate) fn version_requested(mut args: impl Iterator<Item = String>) -> bool {
    args.any(|a| a == "--version" || a == "-V")
}

/// Returns `true` if `--help` or `-h` appears in the given argument iterator.
pub(crate) fn help_requested(mut args: impl Iterator<Item = String>) -> bool {
    args.any(|a| a == "--help" || a == "-h")
}

pub(crate) fn print_help() {
    println!(
        "mmterm {version}

A cross-platform CPU-rendered terminal emulator.

Usage: mmterm [OPTIONS]

Options:
  --version, -V       Print version and exit
  --help,    -h       Print this help and exit
  --debug             Enable debug logging to ~/.mmterm/debug-<ts>.log
  --scope <name>      Use a named session scope (~/.config/mmterm/sessions/<name>.toml)
  --scope=<name>      Same as --scope <name>
  -s <name>           Short form of --scope
  --list-scopes       Print all saved scope names and exit
  --maximized         Start with the window maximized
  --fullscreen        Start with the window in fullscreen (wins over --maximized)",
        version = env!("MMTERM_VERSION")
    );
}

/// How the window should be sized at startup, when requested on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartupWindowMode {
    Maximized,
    Fullscreen,
}

/// Extracts the startup window mode from `--maximized` / `--fullscreen`.
///
/// `--fullscreen` takes precedence when both are present.
pub(crate) fn startup_window_mode(args: impl Iterator<Item = String>) -> Option<StartupWindowMode> {
    let mut mode = None;
    for a in args {
        match a.as_str() {
            "--fullscreen" => return Some(StartupWindowMode::Fullscreen),
            "--maximized" => mode = Some(StartupWindowMode::Maximized),
            _ => {}
        }
    }
    mode
}

/// Extracts the `--scope <name>` / `--scope=<name>` / `-s <name>` value from args.
pub(crate) fn scope_from_args(args: impl Iterator<Item = String>) -> Option<String> {
    let args: Vec<String> = args.collect();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--scope" || args[i] == "-s" {
            return args.get(i + 1).cloned();
        }
        if let Some(val) = args[i].strip_prefix("--scope=") {
            return Some(val.to_string());
        }
        i += 1;
    }
    None
}

/// Returns `true` if `--list-scopes` appears in the given argument iterator.
pub(crate) fn list_scopes_requested(mut args: impl Iterator<Item = String>) -> bool {
    args.any(|a| a == "--list-scopes")
}

/// Returns the debug log path when `--debug` is in argv, otherwise `None`.
pub fn debug_log_path() -> Option<String> {
    if !std::env::args().any(|a| a == "--debug") {
        return None;
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let dir = format!("{home}/.mmterm");
    std::fs::create_dir_all(&dir).ok()?;
    let ts = chrono::Local::now().format("%Y%m%dT%H%M%S");
    Some(format!("{dir}/debug-{ts}.log"))
}
