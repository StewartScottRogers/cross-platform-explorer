//! Orphan session-daemon reaper (CPE-483).
//!
//! The Agent Deck can run a long-lived `ai-console --session-daemon` process so agent PTYs survive
//! a console restart (CPE-309). By design those daemons **outlive** the app — which bit us twice:
//! a leftover daemon held `sidecars/ai-console.exe` file-locked so the NSIS installer silently
//! skipped updating the sidecar (a new host left running a *stale* sidecar), and a surviving daemon
//! kept serving old, output-less sessions.
//!
//! This module sweeps such orphans at host startup. It runs **before** the host spawns any daemon
//! of its own, so by construction every matching daemon is one the current host does not own — safe
//! to terminate. The match is scoped tightly to *this app's* sidecar binary path(s): an unrelated
//! `ai-console.exe` elsewhere (a dev build, another install) is never touched.

use std::path::{Path, PathBuf};

/// What a sweep did — returned so the caller can log it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ReapReport {
    /// PIDs of orphan daemons we asked the OS to terminate.
    pub killed_pids: Vec<u32>,
    /// Whether a stale daemon port file was found and removed.
    pub port_file_removed: bool,
}

/// True iff `proc_cmd` is a `--session-daemon` invocation of one of *our* sidecar binaries.
///
/// Both conditions must hold: the command line carries the `--session-daemon` flag AND the process
/// executable is (path-)equal to one of `our_exes`. Split out from the OS scan so the matching rule
/// is unit-testable without spawning processes.
pub fn is_our_session_daemon(proc_exe: Option<&Path>, proc_cmd: &[String], our_exes: &[PathBuf]) -> bool {
    if !proc_cmd.iter().any(|a| a == "--session-daemon") {
        return false;
    }
    let Some(exe) = proc_exe else { return false };
    our_exes.iter().any(|ours| same_exe(exe, ours))
}

/// Compare two executable paths for identity. Prefers `canonicalize` (resolves `.`/`..`/symlinks and,
/// on Windows, short 8.3 names); falls back to a normalized string compare (case-insensitive on
/// Windows) when a path can't be canonicalized — e.g. the binary was already moved by an installer.
fn same_exe(a: &Path, b: &Path) -> bool {
    if let (Ok(ca), Ok(cb)) = (a.canonicalize(), b.canonicalize()) {
        return ca == cb;
    }
    norm(a) == norm(b)
}

fn norm(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}

/// The well-known session-daemon port file. Mirrors `ai-console`'s
/// `session_supervisor::default_port_file()` — the host can't depend on the sidecar crate
/// (one-way dependency rule), so the path is duplicated here with this note. Keep them in sync.
pub fn default_session_daemon_port_file() -> PathBuf {
    std::env::temp_dir().join("cpe-ai-console").join("session-daemon.port")
}

/// Terminate every orphan `--session-daemon` process spawned from one of `our_exes`, and delete a
/// stale `port_file` if present. Best-effort: a process that refuses to die or a port file that
/// can't be removed is skipped, never fatal — a failed sweep must not stop the app from starting.
pub fn reap_orphan_session_daemons(our_exes: &[PathBuf], port_file: Option<&Path>) -> ReapReport {
    let mut killed_pids = Vec::new();

    let mut sys = sysinfo::System::new();
    sys.refresh_processes();
    for (pid, proc_) in sys.processes() {
        if is_our_session_daemon(proc_.exe(), proc_.cmd(), our_exes) && proc_.kill() {
            killed_pids.push(pid.as_u32());
        }
    }

    let port_file_removed = match port_file {
        Some(pf) if pf.exists() => std::fs::remove_file(pf).is_ok(),
        _ => false,
    };

    ReapReport { killed_pids, port_file_removed }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exe(p: &str) -> PathBuf {
        PathBuf::from(p)
    }

    #[test]
    fn matches_our_daemon_by_exe_and_flag() {
        let ours = vec![exe("/app/sidecars/ai-console")];
        let cmd = vec!["/app/sidecars/ai-console".to_string(), "--session-daemon".to_string()];
        assert!(is_our_session_daemon(Some(Path::new("/app/sidecars/ai-console")), &cmd, &ours));
    }

    #[test]
    fn ignores_non_daemon_invocation() {
        // Same binary, but not the daemon mode — must be left alone (it's the live console).
        let ours = vec![exe("/app/sidecars/ai-console")];
        let cmd = vec!["/app/sidecars/ai-console".to_string(), "--serve".to_string()];
        assert!(!is_our_session_daemon(Some(Path::new("/app/sidecars/ai-console")), &cmd, &ours));
    }

    #[test]
    fn ignores_a_daemon_from_a_different_binary() {
        // A `--session-daemon` from some *other* ai-console (a dev build, another install) is out of
        // scope — the sweep must never touch unrelated processes.
        let ours = vec![exe("/app/sidecars/ai-console")];
        let cmd = vec!["/other/place/ai-console".to_string(), "--session-daemon".to_string()];
        assert!(!is_our_session_daemon(Some(Path::new("/other/place/ai-console")), &cmd, &ours));
    }

    #[test]
    fn ignores_process_with_no_exe_path() {
        let ours = vec![exe("/app/sidecars/ai-console")];
        let cmd = vec!["ai-console".to_string(), "--session-daemon".to_string()];
        assert!(!is_our_session_daemon(None, &cmd, &ours));
    }

    #[cfg(windows)]
    #[test]
    fn windows_path_match_is_case_and_separator_insensitive() {
        let ours = vec![exe(r"C:\App\sidecars\ai-console.exe")];
        let cmd = vec!["ai-console".to_string(), "--session-daemon".to_string()];
        // Backslash vs forward slash and upper vs lower must still match on Windows.
        assert!(is_our_session_daemon(Some(Path::new(r"c:/app/sidecars/AI-CONSOLE.EXE")), &cmd, &ours));
    }

    #[test]
    fn port_file_path_is_under_temp() {
        let pf = default_session_daemon_port_file();
        assert!(pf.ends_with("cpe-ai-console/session-daemon.port") || pf.ends_with(r"cpe-ai-console\session-daemon.port"));
    }
}
