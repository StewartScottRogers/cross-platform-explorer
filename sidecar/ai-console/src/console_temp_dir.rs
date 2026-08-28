//! CPE-1975 — the AI Console's **rendezvous directory** under the OS temp directory, and the one
//! primitive that opens it.
//!
//! ## The defect
//!
//! Three sites built `std::env::temp_dir().join("cpe-ai-console")` by hand and two of them
//! materialised it with `std::fs::create_dir_all`:
//!
//! * `session_diag::log_path` / `session_diag::trace` — builds it, `create_dir_all`s it, appends the
//!   CPE-309 I/O trace log into it;
//! * `session_supervisor::default_port_file` / `write_port_file` — builds it, `create_dir_all`s it,
//!   writes the session daemon's **port file** into it;
//! * `sidecar_host::reaper::default_session_daemon_port_file` — builds it from **a second, duplicate
//!   spelling of the path** and `remove_file`s the port file at startup. (Round 1 of this ticket
//!   explained that duplicate as forced, "because ADR 0001 forbids the host depending on this crate".
//!   **That was false** — see the constants below — and the duplicate is gone: both names now live
//!   once, in `sidecar-contract`.)
//!
//! `create_dir_all` is the primitive CPE-1952 established will walk a pre-existing junction
//! (Windows) or symlink (Unix) as though it were the directory it points at. So an attacker who
//! plants a link at that path chooses where the trace log and the port file land — and where the
//! host's startup reaper deletes.
//!
//! And unlike CPE-1952's `cpe-catalog-stage-<pid>` or CPE-1964's `cpe-swarm-<millis>`, this name is
//! **a constant**. There is no window to guess inside; it is the same string on every machine, for
//! every release, forever.
//!
//! ## Reproduced before it was fixed, on both platforms
//!
//! `tests/console_temp_dir_containment.rs` is the executable Windows/macOS/Linux reproduction and
//! the sensitivity control. The Unix half was additionally measured by hand on **real ext4** (a WSL
//! Ubuntu home directory — `/tmp` there is tmpfs, so `TMPDIR` was overridden; the repo checkout on
//! `/mnt/z` is a 9p mount and no good for link semantics either), 2026-08-28:
//!
//! ```text
//! TMPDIR filesystem: /dev/sdd  ext4
//! planted symlink  ~/cpe-1975-tmp/cpe-ai-console -> ~/cpe-1975-tmp/victim
//! mkdir -p on the planted symlink            -> exit 0
//! victim after     : ['session-daemon.port', 'session-diag.log']
//! exists() through the link                  -> True
//! victim after unlink('session-daemon.port') : ['session-diag.log']
//! BARE mkdir on the planted symlink          -> errno 17 File exists (EEXIST)
//! lstat says is_dir: False   is_symlink: True
//! ```
//!
//! The last two lines are the fix, measured on the same filesystem: `mkdir(2)` without `-p` refuses
//! the planted link atomically, and `lstat` — `symlink_metadata` — is what tells a real directory
//! from a link afterwards.
//!
//! ## Threat model, in halves — the two platforms are not the same claim
//!
//! * **Unix.** `std::env::temp_dir()` is `$TMPDIR` or `/tmp`, a genuinely shared, world-writable
//!   namespace. Any local user can plant the link, and the name is a constant they never have to
//!   guess. This is the strong form.
//! * **Windows.** `std::env::temp_dir()` resolves to the per-user `%LOCALAPPDATA%\Temp`, so the
//!   attack needs a process already running as the same user. Weaker — but this app exists to launch
//!   other people's agent binaries as that same user, which is exactly the position required.
//!
//! ## What the consequence actually is today — measured, and smaller than the ticket assumed
//!
//! The ticket's headline is that the port file is a **control channel**: redirect the directory and
//! the console connects to the attacker's "daemon". That consequence is **not reachable in the
//! shipped product**, and the evidence is a call graph rather than an argument:
//!
//! * the only writer and the only reader of the port file are
//!   [`crate::session_supervisor::SessionDaemonHandle::discover_or_spawn`]'s `write_port_file` /
//!   `read_port_file`, and `discover_or_spawn` has **zero callers** — in this crate, in the host, in
//!   `src-tauri`, and in every test (`rg discover_or_spawn` over the whole tree, 2026-08-28: one
//!   hit, its own definition);
//! * the production path never consults a file at all. The host spawns the daemon as its own child
//!   (`AiConsoleState::ensure_session_daemon`, `src-tauri/src/lib.rs`), reads `PORT <n>` off that
//!   **child's stdout pipe**, and injects `CPE_AICONSOLE_SESSION_DAEMON_ADDR` into the sidecar's
//!   environment; `main.rs` parses that env var and calls `SessionDaemonHandle::external(port)`. A
//!   pipe from a process you spawned and an environment variable you set are not substitutable by a
//!   filesystem plant;
//! * corroborated on disk: this machine's `%LOCALAPPDATA%\Temp\cpe-ai-console` (last write
//!   2026-08-20) holds `session-diag.log` and **no `session-daemon.port`** — months of real use and
//!   the port file has never been written.
//!
//! So the live consequences of the redirect are the two smaller ones, both real:
//!
//! 1. **Write.** `session_diag::trace` appends the CPE-309 trace log through the link — an
//!    attacker-chosen destination for a file that records session ids, byte counts and pids
//!    (CPE-1952's class: a leak).
//! 2. **Delete.** The host's startup reaper `remove_file`s `<link>/session-daemon.port`, so a planted
//!    link turns it into an unlink of a fixed filename inside an attacker-chosen directory.
//!
//! That is a **downgrade from the ticket's stated priority**, and it is stated here rather than
//! quietly enjoyed. It is *not* an argument for leaving the primitive as it was: `discover_or_spawn`
//! is the second half of CPE-309 S4, written and waiting to be wired up, and when it is wired the
//! control channel becomes live. Hardening now costs nothing and means that step does not have to
//! remember.
//!
//! **And the reader would not catch it.** `discover_or_spawn` calls `daemon_answers(port)`, which
//! writes `{"op":"list"}` to `127.0.0.1:<port>` and returns true if *anything* answers a byte. It
//! authenticates nothing. So a redirected port file naming an attacker's socket passes that check.
//! Making the reader safe needs a shared secret the console and the daemon both hold (the daemon
//! could mint a token, print it beside `PORT <n>`, and require it on every op) — not attempted here,
//! because a defence for a call path with no callers cannot be exercised, and unexercised security
//! code is CPE-1929's shadowed guard in a different costume. Named so the wiring step inherits it.
//!
//! ## Why CPE-1952's stronger answer — "delete the directory" — is unavailable
//!
//! CPE-1952 could keep unverified bytes off the disk entirely because exactly one process read them.
//! This directory cannot go: it **is** a rendezvous. Its whole purpose is that a process which did
//! not create it can find it later by name — the restarted console looking for a daemon, the host
//! reaper looking for a stale port file, the user opening `session-diag.log` after a black-terminal
//! run. A rendezvous with a private or random name is not a rendezvous.
//!
//! So the honest statement of what the hardening buys: **the directory still exists at a constant
//! path in a shared namespace, and it always will.** What changes is that this process will no
//! longer *create* it through a link, no longer *write* into one, and (in the host) no longer
//! *delete* through one.
//!
//! ## And what it does not buy: the residual TOCTOU, stated rather than implied
//!
//! CPE-1964's mission directory got the clean property — exclusive create, refusal atomic with the
//! create, no check-then-use window — because each mission directory is brand new. A rendezvous
//! directory is expected to be **already there** on every run after the first, so `AlreadyExists`
//! cannot be a failure; it has to be followed by "…and is it a real directory?", and that second
//! question is asked after the first is answered. Between the `symlink_metadata` and the file open,
//! a same-user attacker on Unix can `rmdir` and `symlink`. The window is short and needs a process
//! already running as the victim, but it is not zero, and no path-based design closes it. Closing it
//! needs a handle-based one — open the directory once (`O_DIRECTORY|O_NOFOLLOW`, or
//! `NtCreateFile` with `FILE_FLAG_OPEN_REPARSE_POINT`) and reach the files through `openat`-style
//! calls on that handle. Out of scope here; named so the next reader is not misled by the green
//! tests into thinking the window was closed.
//!
//! ## No sweep, and the reason is not laziness
//!
//! CPE-1964 swept because `cpe-swarm-<millis>` mints a new directory per mission and 55 had piled
//! up. This name is a **constant**: there is exactly one, ever, per temp directory — measured on
//! this machine, one directory, 14,843 bytes of trace log. It is not litter, it is the rendezvous,
//! and deleting it is not a cleanup but a denial of the thing being defended.
//!
//! A user whose `cpe-ai-console` *is already* a planted link is handled by refusing it, not by
//! removing it. Removing it would mean this code deciding to unlink something at a shared path it
//! cannot prove anything about — which is CPE-1972 exactly: an absence of information must never
//! license a delete. The refusal is loud (the diag log stops being written, `write_port_file`
//! returns an error) and leaves the evidence in place for the user to look at.

use std::io;
use std::path::{Path, PathBuf};

/// The rendezvous directory's name under `std::env::temp_dir()`, and the port file inside it.
///
/// **There is exactly one copy of each, in `sidecar-contract`**, re-exported here so callers in this
/// crate keep reading them from `console_temp_dir`. Round 1 of this ticket spelled them a second time
/// in `sidecar_host::reaper` and justified the duplicate with "ADR 0001 forbids the host depending on
/// a sidecar crate, and CI fails the build if it tries" — **a false claim of exactly the class this
/// ticket set out to kill**, sitting beside a green test that vouched only for the two string
/// literals matching. ADR 0001's rule and its CI guard both point the other way (a *sidecar* must not
/// depend on the *explorer app*), so the experiment would have passed; it was never run. See
/// `sidecar_contract::CONSOLE_DIR_NAME`'s own doc for the full record.
///
/// The contract crate is where a host↔sidecar rendezvous name belongs anyway, and both crates already
/// depend on it, so removing the duplication added **no new dependency edge**.
pub use sidecar_contract::{CONSOLE_DIR_NAME, PORT_FILE_NAME};

/// The CPE-309 I/O trace log, inside [`CONSOLE_DIR_NAME`].
pub const DIAG_LOG_NAME: &str = "session-diag.log";

/// `<temp>/cpe-ai-console` — the path itself, without touching the filesystem.
pub fn console_temp_dir() -> PathBuf {
    std::env::temp_dir().join(CONSOLE_DIR_NAME)
}

/// Open (or adopt) the rendezvous directory at exactly `dir`, refusing to act through a link.
///
/// Two cases, and the split is the whole design:
///
/// * **not there yet** — `std::fs::create_dir` is one `mkdir(2)` / `CreateDirectoryW`. It fails with
///   [`io::ErrorKind::AlreadyExists`] when *anything* occupies the path, a reparse point included,
///   and that refusal is atomic with the create. This is the case `create_dir_all` used to walk
///   straight through;
/// * **already there** — which is the normal case for a rendezvous, so `AlreadyExists` cannot be an
///   error. It is instead the question "…is it a real directory?", answered by `symlink_metadata`,
///   which does **not** follow. `FileType::is_dir()` is `is_directory() && !is_symlink()` on
///   Windows, and std reports a name-surrogate reparse point (a junction) as a symlink, so this one
///   predicate excludes junctions, Unix symlinks, files, and anything else that is not a plain
///   directory. Measured on real ext4 (see the module header): `lstat` on a planted symlink reports
///   `is_dir: False, is_symlink: True`.
///
/// There is deliberately **no `dir.exists()` pre-check** in front of the `create_dir`. It would be a
/// shadowed guard in CPE-1929's exact sense — every input that trips it trips `create_dir` first,
/// and racily — and a shadowed guard reads as coverage.
///
/// ## CPE-1929 sabotage pair on the [`console_dir_is_real`] refusal
///
/// Measured 2026-08-28 on **Windows**, `cargo test --locked --no-fail-fast` in `sidecar/ai-console`
/// (`--no-fail-fast` because without it cargo stops after the first failing binary and the totals are
/// not comparable). The `Compiling ai-console` line was confirmed present in every run below, so none
/// of them is a stale-binary pass. Baseline: **423 passed / 0 failed**.
///
/// The numbers below are the **round-3 re-run against the shipping code**, and the baseline moved
/// with it. Rounds 1 and 2 measured 420/2 and 421/1 against a **422** baseline; round 3 added
/// `session_diag::tests::tracing_is_off_by_default_so_it_cannot_carry_a_must_see_message`, which
/// shifts every total by one. The *deltas* never changed and the *named tests* never changed — but
/// the absolute figures did, and a stale absolute figure beside a green suite is the whole failure
/// mode this file is about, so they were re-measured rather than adjusted on paper.
///
/// Re-run three times in all: round 2 after the predicate was extracted into
/// [`console_dir_is_real`] (a behaviour-preservation *claim*, so both legs were run rather than
/// carried forward), and round 3 after `discover_or_spawn`'s reporting changed and again after the
/// new test moved the baseline. (The `sidecar/host` pairs are **not** included in the round-3
/// re-run — that crate is untouched by rounds 3's edits and its baseline is still 153. Stated so the
/// two are not read as having had equal treatment.)
///
/// * **disabled** (`if false && !console_dir_is_real(dir)`) → **RED**, 421 passed / **2 failed**:
///   `console_temp_dir::tests::ensure_console_dir_at_refuses_a_plain_file_at_the_path` and
///   `the_hardened_primitive_refuses_a_planted_link`. So the refusal is reachable, and those two
///   tests are what reach it.
/// * **predicate made to lie** (`std::fs::metadata(dir)` — the *following* stat — in place of
///   `symlink_metadata` inside [`console_dir_is_real`], so a junction reports `is_dir() == true`) →
///   **RED**, 422 passed / **1 failed**: `the_hardened_primitive_refuses_a_planted_link`. One rather
///   than two, and the
///   difference is informative: a plain *file* at the path is refused by either stat, so only the
///   planted-link test can tell the two apart — which is exactly why the link test has to exist.
///
/// Both red ⇒ live, not shadowed. Note which side that was measured on: **Windows only**. The Linux
/// and macOS legs of the same pair were not run here (no C linker is available in this shift's WSL,
/// so the crate cannot be built there), and this shift's own notes record a pair that came out green
/// on one platform and red on the other — so treat the Unix side as unmeasured rather than as
/// implied by the Windows result. What *is* measured on Unix is the underlying primitive: bare
/// `mkdir(2)` returns `EEXIST` on a planted symlink and `lstat` reports it as a symlink, both on real
/// ext4 (module header).
pub fn ensure_console_dir_at(dir: &Path) -> io::Result<()> {
    match std::fs::create_dir(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            if !console_dir_is_real(dir) {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!(
                        "{} is not a plain directory (it is a symlink, junction or file), so the AI \
                         Console will not write through it — see CPE-1975",
                        dir.display()
                    ),
                ));
            }
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Is `dir` a **plain directory** — not a symlink, not a Windows junction, not a file, not missing?
///
/// The refusal [`ensure_console_dir_at`] rides on, split out so a **read** path can ask the question
/// without the `mkdir` side effect (CPE-1975 round 2: `read_port_file` used to call
/// `ensure_console_dir_at`, so a lookup created a directory). One predicate, two callers, so there is
/// a single place to sabotage and a single place to audit.
///
/// `symlink_metadata` does **not** follow. `FileType::is_dir()` is `is_directory() && !is_symlink()`
/// on Windows, and std reports a name-surrogate reparse point (a junction) as a symlink, so this one
/// predicate excludes junctions, Unix symlinks, files, and anything else that is not a plain
/// directory. Measured on real ext4 (module header): `lstat` on a planted symlink reports
/// `is_dir: False, is_symlink: True`.
///
/// Every error is `false` — an unreadable entry is refused, never assumed benign. For the read path
/// that also makes "not there yet" a clean `false`, which is the correct answer on a first run.
/// (One deliberate consequence for [`ensure_console_dir_at`]: an unreadable `symlink_metadata` used
/// to propagate its own `io::Error` and now surfaces as the `AlreadyExists` refusal below. Both
/// refuse; only the message differs.)
pub fn console_dir_is_real(dir: &Path) -> bool {
    std::fs::symlink_metadata(dir).map(|m| m.is_dir()).unwrap_or(false)
}

/// [`ensure_console_dir_at`] at the real `<temp>/cpe-ai-console`, returning the path on success.
pub fn ensure_console_dir() -> io::Result<PathBuf> {
    let dir = console_temp_dir();
    ensure_console_dir_at(&dir)?;
    Ok(dir)
}

/// Is `path` safe for this crate to open by name — a plain regular file, or nothing at all?
///
/// The directory check in [`ensure_console_dir_at`] stops a redirected *directory*; this stops a
/// redirected *file* inside a directory that is genuinely ours. Both are needed: the port file and
/// the trace log are opened by name with `create(true)`, which follows a symlink sitting at that
/// name and writes to its target.
///
/// Absent is fine — that is the first run. Anything else, including an unreadable entry, is refused:
/// an absence of information is not a licence to write (CPE-1972's rule, in the other direction).
///
/// ## CPE-1929 sabotage pair
///
/// Measured 2026-08-28 on **Windows**, same command and baseline as [`ensure_console_dir_at`]'s
/// (`cargo test --locked --no-fail-fast`, baseline **423 passed / 0 failed**, `Compiling ai-console`
/// confirmed in both runs). Re-measured in round 3 against the shipping code for the same reason
/// given there — the new `session_diag` gate test moved every total by one.
///
/// * **disabled** (`Ok(_meta) => true`, i.e. any existing entry accepted) → **RED**, 421 passed /
///   **2 failed**: `console_temp_dir::tests::regular_file_or_absent_accepts_absent_and_regular_only`
///   and `a_link_at_the_port_file_name_is_refused`.
/// * **predicate made to lie** (`std::fs::metadata(path)` instead of `symlink_metadata`, so a
///   symlink to a regular file reports `is_file() == true`) → **RED**, 422 passed / **1 failed**:
///   `a_link_at_the_port_file_name_is_refused`. Only the link test separates the two stats, which is
///   the same asymmetry the other pair shows.
///
/// Both red ⇒ live, and **not shadowed by [`ensure_console_dir_at`]'s check**, which was the outcome
/// to watch for here: that one answers "is the *directory* real?", this one answers "is the *leaf*
/// real?", and `a_link_at_the_port_file_name_is_refused` reaches this one through a directory that is
/// genuine. (CPE-1964's third pair is the cautionary case — an `is_symlink()` arm that `!is_dir()`
/// had already answered, written, measured and deleted.)
///
/// Windows-measured only; see the note on the other pair.
pub fn regular_file_or_absent(path: &Path) -> bool {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => meta.file_type().is_file(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => true,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_console_dir_at_creates_then_adopts() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(CONSOLE_DIR_NAME);
        ensure_console_dir_at(&dir).expect("first call creates it");
        assert!(dir.is_dir());
        // A rendezvous is found again by name on every later run, so the second call must succeed —
        // this is the property that makes `create_dir` alone (CPE-1964's answer) inapplicable here.
        std::fs::write(dir.join(PORT_FILE_NAME), b"1234").unwrap();
        ensure_console_dir_at(&dir).expect("second call adopts the existing directory");
        assert_eq!(std::fs::read(dir.join(PORT_FILE_NAME)).unwrap(), b"1234", "and does not clear it");
    }

    #[test]
    fn ensure_console_dir_at_refuses_a_plain_file_at_the_path() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(CONSOLE_DIR_NAME);
        std::fs::write(&dir, b"not a directory").unwrap();
        let err = ensure_console_dir_at(&dir).expect_err("a file at the rendezvous path is refused");
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn the_real_rendezvous_path_is_temp_dir_joined_with_the_constant() {
        let dir = console_temp_dir();
        assert_eq!(dir.parent(), Some(std::env::temp_dir().as_path()));
        assert_eq!(dir.file_name().unwrap().to_string_lossy(), CONSOLE_DIR_NAME);
    }

    #[test]
    fn regular_file_or_absent_accepts_absent_and_regular_only() {
        let root = tempfile::tempdir().unwrap();
        assert!(regular_file_or_absent(&root.path().join("nothing-here")), "absent is the first run");
        let f = root.path().join(PORT_FILE_NAME);
        std::fs::write(&f, b"1").unwrap();
        assert!(regular_file_or_absent(&f));
        let d = root.path().join("a-directory");
        std::fs::create_dir(&d).unwrap();
        assert!(!regular_file_or_absent(&d), "a directory at the file's name is refused");
    }
}
