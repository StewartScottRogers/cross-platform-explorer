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

/// The rendezvous directory's name, and the port file inside it — **one copy, in the contract
/// crate**, re-exported here (CPE-1975).
///
/// This used to be a second literal spelling, under a bare "Keep them in sync" comment. Round 1 of
/// CPE-1975 replaced the comment with a derived test and then justified keeping the duplicate with
/// "ADR 0001's one-way rule means the host may not depend on a sidecar crate, and CI fails the build
/// if it tries" — **which is false**, and is the same untested-provenance defect the round was
/// closing, one level up. ADR 0001's rule and its CI guard are about a *sidecar* depending on the
/// *explorer app*: the guard greps `sidecar/*/Cargo.toml` for `^(app_lib|cross-platform-explorer)\b`
/// or `path = "../../src-tauri"`, neither of which a host→sidecar edge matches.
///
/// The duplication was removable all along and is now gone: the name is a host↔sidecar rendezvous,
/// so it belongs in `sidecar-contract`, which **both** crates already depend on — no new dependency
/// edge, no effect on the one-way rule or the delete-test. CPE-1950: where the duplication is
/// removable, remove it.
pub use sidecar_contract::{CONSOLE_DIR_NAME, PORT_FILE_NAME};

/// The well-known session-daemon port file: `<temp>/cpe-ai-console/session-daemon.port`.
pub fn default_session_daemon_port_file() -> PathBuf {
    std::env::temp_dir().join(CONSOLE_DIR_NAME).join(PORT_FILE_NAME)
}

/// Delete the stale daemon port file — refusing to act **through** a planted link (CPE-1975).
///
/// This is the third of the ticket's three sites. It never created the directory, so it had no
/// `create_dir_all` to fix; what it had was `port_file.exists()` followed by `std::fs::remove_file`,
/// and both of those resolve the whole path. A junction (Windows) or symlink (Unix) planted at
/// `<temp>/cpe-ai-console` therefore turned the host's startup sweep into an unlink of
/// `<attacker's directory>/session-daemon.port`. Measured before the fix on real ext4:
/// `exists()` through the link returned `True` and `unlink` removed the target's file (see
/// `ai_console::console_temp_dir`'s module header for the full transcript).
///
/// Two refusals, and they answer **different** questions — which is why both are here and neither is
/// shadowed by the other:
///
/// * the **parent** must be a plain directory. Path resolution always follows intermediate
///   components, so a junction at `cpe-ai-console` makes the port file inside it look like a perfectly
///   ordinary regular file to the second check. This is the one that stops the escape;
/// * the **port file itself** must be a plain regular file. This is the one that stops a symlink
///   planted at `session-daemon.port` inside a directory that really is ours.
///
/// Every failure returns `false` — "did not remove" — never a delete. An unreadable entry is a skip
/// (CPE-1972: an absence of information must never license a delete), and `ReapReport` reports it as
/// "no port file removed", which is the truth.
///
/// There is no `exists()` pre-check: `symlink_metadata` answers existence and kind in one call, and
/// adding `exists()` in front of it would be a shadowed guard.
///
/// ## The residual, stated HERE and not only one crate over
///
/// Both checks are **verify-then-use**, so a window remains between the last `symlink_metadata` and
/// the `remove_file`: a same-user attacker who wins that race can still have the unlink land through
/// a link swapped in after the checks. It is much narrower than the pre-fix "compute the path in
/// advance and plant at leisure" — it needs a process already running as the victim, and it needs to
/// win a race measured in microseconds — but it is not zero, and no path-based design closes it.
/// Closing it needs a handle-based one: open the directory once (`O_DIRECTORY|O_NOFOLLOW`, or
/// `NtCreateFile` with `FILE_FLAG_OPEN_REPARSE_POINT`) and reach the file through `openat`-style
/// calls on that handle. Out of scope for CPE-1975.
///
/// Written out at this site deliberately. `ai_console::console_temp_dir`'s module header makes the
/// same disclosure for the write half, and a reader of `reaper.rs` never sees it — a residual
/// declared only in the other crate is, for this file's reader, not declared at all.
///
/// ## CPE-1929 sabotage pairs, both refusals, measured 2026-08-28 on **Windows**
///
/// `cargo test --locked --no-fail-fast` in `sidecar/host` (`--no-fail-fast` because otherwise cargo
/// stops after the first failing binary and the totals are not comparable), with the `Compiling
/// sidecar-host` line confirmed present in every sabotage run so none is a stale-binary pass.
/// Baseline: **153 passed / 0 failed**.
///
/// Two of the four (the parent-directory *disabled* leg and the port-file *lie* leg) were **re-run in
/// round 2**, after this file changed to re-export the path constants from `sidecar-contract`; both
/// reproduce their round-1 numbers exactly, naming the same tests. Round 2 also independently
/// reproduced all four of these numbers during review.
///
/// * **parent-directory refusal disabled** (`if false && !parent_is_real_dir(port_file)`) → **RED**,
///   152 passed / **1 failed**: `the_reaper_does_not_delete_through_a_planted_directory_link`.
/// * **parent-directory predicate made to lie** (`std::fs::metadata` — the following stat — instead
///   of `symlink_metadata`, so a junction reports `is_dir() == true`) → **RED**, 152 passed /
///   **1 failed**, the same test.
/// * **port-file refusal disabled** (`Ok(_meta) => true` in place of the `is_file()` test) → **RED**,
///   152 passed / **1 failed**: `the_reaper_does_not_delete_through_a_link_at_the_port_file_name`.
/// * **port-file predicate made to lie** (`std::fs::metadata` instead of `symlink_metadata`, so a
///   symlink to a regular file reports `is_file() == true`) → **RED**, 152 passed / **1 failed**,
///   the same test.
///
/// Four reds, and each pair reds a **different** test ⇒ both refusals are live and neither shadows
/// the other. That was the outcome to watch for — CPE-1964's third pair is the cautionary case, an
/// `is_symlink()` arm written, measured and deleted because `!is_dir()` had already answered the same
/// fact. Here the two ask different questions (is the *directory* real / is the *leaf* real) and each
/// has a test that reaches only it. **Windows-measured only**: this
/// shift had no C linker available under WSL, so the Linux/macOS legs of these pairs were not run,
/// and this shift's own notes record a pair that came out green on one platform and red on the
/// other. The tests themselves are ordinary `#[test]`s and do run on all three OSes in the `sidecar`
/// CI job, but that is the *tests* running green, not the *sabotages* having been run there.
///
/// The last two entries depend on a Windows **file** symlink, which needs Developer Mode; it was
/// plantable on the measuring machine — the leg's "NOT VERIFIED" notice appeared in none of the runs,
/// and it could not have reddened otherwise. On a runner without it that leg reports and returns, and
/// the directory leg (the escape that matters) still runs.
fn remove_stale_port_file(port_file: &Path) -> bool {
    if !parent_is_real_dir(port_file) {
        return false;
    }
    let is_regular_file = match std::fs::symlink_metadata(port_file) {
        Ok(meta) => meta.file_type().is_file(),
        Err(_) => false,
    };
    if !is_regular_file {
        return false;
    }
    std::fs::remove_file(port_file).is_ok()
}

/// Is `path`'s parent directory a plain directory rather than a link? See [`remove_stale_port_file`].
fn parent_is_real_dir(path: &Path) -> bool {
    match path.parent() {
        Some(dir) => std::fs::symlink_metadata(dir).map(|m| m.is_dir()).unwrap_or(false),
        None => false,
    }
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

    // CPE-1975: was `Some(pf) if pf.exists() => std::fs::remove_file(pf).is_ok()`, which followed a
    // junction/symlink planted at `<temp>/cpe-ai-console` and deleted inside the attacker's
    // directory. See [`remove_stale_port_file`].
    let port_file_removed = match port_file {
        Some(pf) => remove_stale_port_file(pf),
        None => false,
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
