//! CPE-1964 — how a swarm mission directory is **created** and how stale ones are **swept**.
//!
//! ## The two defects that shared one line
//!
//! `handle_swarm_run` used to open a mission with
//!
//! ```text
//! let mission_dir = std::env::temp_dir().join(format!("cpe-swarm-{}", now_millis()));
//! ```
//!
//! and materialise it (via `write_members` / `write_mcp_config`) with `std::fs::create_dir_all`.
//! That is two separate bugs wearing one line:
//!
//! 1. **Escape.** `create_dir_all` treats a pre-existing junction (Windows) or symlink (Unix) as
//!    the directory it points at and happily creates whatever is missing beyond it, so an attacker
//!    who plants a link at the path we are about to use chooses where the mission's roster, mailbox,
//!    memory notes, per-agent MCP configs and (on Windows) task files land. The name was a
//!    millisecond timestamp — guessable inside a narrow window rather than random.
//! 2. **Leak.** Nothing ever removed the directory. 55 of them were counted in one real user's
//!    `%TEMP%` (CPE-1964's field measurement), against the 9 at the `cpe-catalog-stage-<pid>` site
//!    that got CPE-1952.
//!
//! ## Threat model — stated in halves, because the two platforms differ
//!
//! * **Unix.** `std::env::temp_dir()` is `$TMPDIR` or `/tmp`: a genuinely **shared, world-writable**
//!   namespace. Any local user can plant a symlink at a name we are about to use. This is the strong
//!   form of the attack.
//! * **Windows.** `std::env::temp_dir()` resolves to `%LOCALAPPDATA%\Temp`, which is **per-user**.
//!   The attack therefore needs a process already running as the same user (a compromised or
//!   sandboxed helper, a malicious installer, another agent). Weaker, but not nothing — the whole
//!   point of this app is that it launches other people's agent binaries.
//!
//! Do **not** repeat CPE-1952's framing wholesale: "predictable path in a shared namespace" is fully
//! true of Unix and only half true of Windows.
//!
//! ## Why the CPE-1952 fix shape ("delete the directory") is unavailable here
//!
//! CPE-1952's strongest property was that unverified bytes never reached the disk at all: the
//! catalog bundle could be assembled in memory because only one process ever read it. A mission
//! directory cannot be: it **is** the swarm's shared substrate. Each launched agent spawns its own
//! `ai-console --swarm-mcp --dir <mission>` host in a **separate process**, and those hosts
//! coordinate through files in that directory — `members.json`, `mailbox.jsonl`, `memory/*.md`, one
//! `mcp-<agent>.json` per agent, plus `task-*.txt` on Windows (CPE-588 delivers the prompt by stdin
//! redirect because `cmd /c` mangles argv). An in-memory mission directory would have to be shared
//! across process boundaries, which is the thing the filesystem is being used for. So the directory
//! must exist, and the two defects need two answers instead of one.
//!
//! ## Answer 1 — creation: exclusive create, and an unguessable name behind it
//!
//! [`create_mission_dir_at`] uses **`std::fs::create_dir`**, not `create_dir_all`. `mkdir(2)` and
//! `CreateDirectoryW` both fail with `AlreadyExists`/`ERROR_ALREADY_EXISTS` when *anything* is at
//! the path — a real directory, a file, or a reparse point. The refusal is atomic with the create,
//! so there is no check-then-use window. That is the load-bearing property; the random name in
//! [`create_mission_dir`] is defence in depth on top of it (it denies the attacker the pre-emption
//! target in the first place, and it removes the "watch `%TEMP%`, learn the timestamp cadence,
//! pre-create the next few names" play the 55 leaked names were publishing).
//!
//! **There is deliberately no `path.exists()` pre-check in front of `create_dir`.** It would be a
//! *shadowed guard* in CPE-1929's exact sense — every input that trips it trips `create_dir` first
//! (and later, and racily), so it would be safe, unverifiable, and read as coverage. `create_dir`'s
//! own `AlreadyExists` is the only refusal here, and the tests in
//! `tests/swarm_mission_dir_containment.rs` red-proof it as such.
//!
//! ## Answer 2 — the leak: a startup sweep with a stated retention
//!
//! An RAII guard that deleted the mission directory when the mission thread ends was the first
//! design and was **rejected**: the live coordination panel (CPE-592) polls
//! `/api/swarm/activity?mission=<id>` out of that same directory, and the moment you most want to
//! read what the agents said to each other is right after the mission finishes. Deleting on
//! completion would blank the panel exactly then.
//!
//! So [`sweep_stale_mission_dirs`] runs once at console startup with a **24-hour retention**
//! ([`SWEEP_RETENTION`]). Retention rather than "delete everything we did not create" because a
//! second `ai-console` (the user runs the app and a `--session-daemon`; two app windows; a
//! developer build alongside the installed one) may have a mission running right now, and its
//! directory is not ours to reason about. A day is far longer than any mission and far shorter than
//! the two months the leaked directories had been accumulating.
//!
//! ### A sweep is a new destructive operation over a shared namespace, so it refuses by default
//!
//! Five conditions must **all** hold before a directory is removed, and every one of them fails
//! **closed** — an error reading anything is a skip, never a delete. That is CPE-1972's rule
//! directly: *an absence of information must never license a delete.*
//!
//! 1. the name is exactly `cpe-swarm-` + a non-empty run of ASCII alphanumerics (no separators, no
//!    dots, so nothing here can name a path we did not mean);
//! 2. `symlink_metadata` says it is a **real directory** — that one predicate excludes symlinks and
//!    Windows junctions too, because std reports a name-surrogate reparse point as a symlink and
//!    never as a directory, so the sweep can neither delete through a planted link nor be tricked
//!    into removing an attacker's link-shaped bait. (A separate `is_symlink()` arm was written, its
//!    CPE-1929 pair run, and deleted as shadowed — see the site.);
//! 3. it contains `members.json` **as a regular file** (again by `symlink_metadata`) — the roster
//!    `swarm_mcp_server::write_members` writes into every mission and nothing else in `%TEMP%`
//!    writes. This is the "plainly ours" evidence; a `cpe-swarm-xxxx` directory somebody else made
//!    is left alone;
//! 4. its mtime is readable and at least `retention` in the past (a future mtime — clock skew, a
//!    hostile touch — is a skip, not a delete);
//! 5. [`remove_mission_dir`] succeeds. On Unix `/tmp` the sticky bit means another user's mission
//!    directory fails here rather than being removed, which is the correct outcome and is counted,
//!    not hidden.
//!
//! The removal cannot itself escape, and that is **tested rather than asserted** — condition 2 stops
//! the sweep at the top of the tree, so it says nothing about what happens on the way *down*, and the
//! interesting case is a link nested inside a directory that is genuinely ours.
//! `the_sweep_does_not_walk_a_link_nested_inside_a_real_mission_directory` runs the real sweep over
//! exactly that (a link directly inside the mission, and one a level deeper inside a real
//! subdirectory, each pointing at an attacker directory holding a `secret.txt`) and asserts the
//! mission is removed while both secrets survive. Its two legs cover the two code paths: the
//! top-level link goes through [`remove_link`], the nested one through std's `remove_dir_all`, which
//! since Rust 1.77 removes a reparse point as a link rather than recursing into it. Deliberately
//! sabotaged once — a `canonicalize()` before the delete, i.e. the removal made to follow — and it
//! went RED, so the leg is live and not decorative.
//!
//! ### The marker is deleted LAST, because the sweep runs on a detached thread
//!
//! [`sweep_stale_mission_dirs_now`] is spawned and never joined, so a console that exits mid-removal
//! takes the process down with a **half-deleted** mission directory. A plain `remove_dir_all` picks
//! its own order, so the survivor can be one that has lost `members.json` — and condition 3 then
//! refuses it *forever*. That is precisely the shape of the one directory this sweep already cannot
//! reclaim (`console.rs`'s rosterless unit-test leftover): the cleanup would be manufacturing more of
//! the litter it exists to remove.
//!
//! So [`remove_mission_dir`] removes every other entry first and the roster last. Any torn state is
//! then **self-healing**: whatever survives still carries the ownership evidence, still satisfies all
//! five conditions, and the next startup sweep finishes the job.
//!
//! Joining the thread at shutdown was the alternative and is worse twice over: it puts an unbounded
//! walk of the OS temp directory (2,127 reparse points on the machine CPE-1974 measured) into the
//! console's exit path, and it still loses to `taskkill /f`, `SIGKILL` or a power cut — it *narrows*
//! the window rather than closing it, and pays shutdown latency for the narrowing. Ordering closes it
//! for every cause of a tear and costs nothing.
//!
//! ## The residual: `create_dir_all` on the mission directory, and why the window is bounded
//!
//! Four helpers still call `std::fs::create_dir_all` on a mission directory — `write_members`
//! (`swarm_mcp_server.rs:195`), `seed_kickoff` (`:207`), `seed_memory` (`:237`, called one line after
//! `seed_kickoff` in `handle_swarm_run`) and `write_mcp_config` (`swarm_plan.rs:139`). They are left
//! alone deliberately: they are public helpers other callers hand an existing directory to, and on
//! the swarm path every one of them runs **after** [`create_mission_dir`] has already made a real
//! directory there, so the primitive is unreachable with a link in place.
//!
//! The bound that matters is therefore unchanged by the count: `write_members` is the **first** of
//! the four to run, so a same-user attacker who wants a `create_dir_all` to walk a link of theirs
//! must `rmdir` our freshly created directory and plant one inside the window between
//! [`create_mission_dir`] returning and `write_members` writing the roster. Much narrower than the
//! pre-fix "compute the name in advance and plant at leisure", but not closed; closing it needs a
//! handle-based design rather than a path-based one.
//!
//! (That list is a point-in-time enumeration — `rg -n 'create_dir_all' sidecar/ai-console/src/`,
//! re-run in round 2 after it shipped one site short. Nothing tests it, so treat it as a pointer to
//! re-derive rather than as a fact; the **bound** is the load-bearing half and it holds for any
//! number of post-create `create_dir_all` callers.)

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Every mission directory is `<temp>/cpe-swarm-<suffix>`. The prefix is the sweep's first filter and
/// the mission-id form `/api/swarm/activity` accepts, so it is spelled once, here.
pub const MISSION_PREFIX: &str = "cpe-swarm-";

/// The roster `swarm_mcp_server::write_members` writes into every mission directory. The sweep uses
/// it as proof of ownership — see condition 3 in this module's header.
pub const MISSION_MARKER: &str = "members.json";

/// How long a mission directory is kept after its last write before the startup sweep may remove it.
/// 24h: longer than any mission, shorter than the months of accumulation CPE-1964 measured.
pub const SWEEP_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);

/// How many names [`create_mission_dir`] will try before giving up. A collision needs a 128-bit
/// coincidence or an attacker who guessed one, so >1 attempt is already unreachable in practice; the
/// loop exists so that a *deliberate* pre-emption cannot turn into a hard failure of the feature.
const NAME_ATTEMPTS: usize = 8;

/// Create the mission directory at exactly `path`, refusing anything already there.
///
/// This is the whole containment: `std::fs::create_dir` is a single `mkdir(2)` /
/// `CreateDirectoryW`, which fails with [`io::ErrorKind::AlreadyExists`] when the path is occupied —
/// **including when it is occupied by a junction or a symlink**, which is the case `create_dir_all`
/// used to walk straight through. No pre-flight `exists()` test guards this call; see the module
/// header for why adding one would be a shadowed guard rather than a second line of defence.
///
/// Separated from [`create_mission_dir`] so a test can drive the production creation primitive at a
/// path it planted a link at — the name [`create_mission_dir`] picks is random by design and so
/// cannot be planted at in advance.
pub fn create_mission_dir_at(path: &Path) -> io::Result<()> {
    std::fs::create_dir(path)
}

/// Open a fresh mission directory under the OS temp directory and return its path.
///
/// The name is `cpe-swarm-<32 hex chars>`; the containment is [`create_mission_dir_at`]'s exclusive
/// create, and the randomness denies an attacker the target to pre-empt.
pub fn create_mission_dir() -> Result<PathBuf, String> {
    create_mission_dir_in(&std::env::temp_dir())
}

/// [`create_mission_dir`] with the root injected, so tests can exercise the retry loop without
/// writing into the real temp directory. Production always passes `std::env::temp_dir()`.
pub fn create_mission_dir_in(root: &Path) -> Result<PathBuf, String> {
    let mut last: Option<(PathBuf, io::Error)> = None;
    for _ in 0..NAME_ATTEMPTS {
        let path = root.join(format!("{MISSION_PREFIX}{}", random_suffix()));
        match create_mission_dir_at(&path) {
            Ok(()) => return Ok(path),
            Err(e) => last = Some((path, e)),
        }
    }
    match last {
        Some((path, e)) => Err(format!(
            "could not open a mission directory after {NAME_ATTEMPTS} attempts (last: {} — {e}). \
             The directory is created with `create_dir`, which refuses any pre-existing entry \
             including a planted junction or symlink; that refusal is deliberate (CPE-1964).",
            path.display()
        )),
        // Unreachable while NAME_ATTEMPTS > 0, and stated rather than `unwrap`ed so a future edit
        // to the constant cannot turn this into a panic in a request handler.
        None => Err("could not open a mission directory: no attempt was made".to_string()),
    }
}

/// 32 hex characters of per-process entropy.
///
/// The key material is `std::collections::hash_map::RandomState`, which std seeds from the operating
/// system — the same entropy `tempfile` reaches for when it names a temporary directory. It is
/// **not** claimed to be a CSPRNG stream: two hashes from one `RandomState` are two keyed hashes,
/// not independent draws, and successive `RandomState::new()` calls on a thread are related by
/// construction (std increments one key), which is why exactly one is built here and fed two
/// different messages. The security property this function is responsible for is only "an attacker
/// watching `%TEMP%` cannot compute the next name", and for that it is ample; the property that
/// stops the attack outright is [`create_mission_dir_at`]'s exclusive create.
fn random_suffix() -> String {
    use std::hash::{BuildHasher, Hasher};
    /// Distinguishes two names minted in the same nanosecond by the same process.
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let pid = u64::from(std::process::id());
    let state = std::collections::hash_map::RandomState::new();
    let mut a = state.build_hasher();
    a.write_u64(seq);
    a.write_u64(pid);
    a.write_u128(nanos);
    let mut b = state.build_hasher();
    b.write_u64(!seq);
    b.write_u64(pid.rotate_left(32));
    b.write_u128(!nanos);
    format!("{:016x}{:016x}", a.finish(), b.finish())
}

/// Is `name` a mission-directory name this code could have minted — `cpe-swarm-` plus a non-empty
/// run of ASCII alphanumerics?
///
/// Shared by the sweep and by `/api/swarm/activity`'s id check, so the two cannot drift apart about
/// what a mission id is. The alphanumeric rule is what makes `<temp>/<name>` safe to join: no
/// separator, no `.`, no `..`, nothing that can leave the temp directory.
pub fn is_mission_name(name: &str) -> bool {
    match name.strip_prefix(MISSION_PREFIX) {
        Some(rest) => !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_alphanumeric()),
        None => false,
    }
}

/// Remove one mission directory, deleting the ownership marker **last**.
///
/// The order is the whole point. The sweep runs on a detached thread that is never joined, so the
/// process can die part-way through this function; with a plain `remove_dir_all` the survivor may be
/// a directory that has lost `members.json`, which condition 3 then refuses forever. Marker-last
/// makes every torn state self-healing. The module header argues this against the alternative
/// (joining the sweep thread at shutdown) and says why ordering wins.
///
/// Failure is returned, not swallowed: the caller counts it in [`SweepReport::failed`], and the
/// marker is still on disk because phase 1 skipped it, so the next sweep retries.
fn remove_mission_dir(path: &Path) -> io::Result<()> {
    remove_all_but_marker(path)?;
    std::fs::remove_file(path.join(MISSION_MARKER))?;
    // Now empty, so `remove_dir` — no second recursive walk, and it cannot take anything with it.
    std::fs::remove_dir(path)
}

/// Phase 1: everything in `dir` except [`MISSION_MARKER`].
///
/// Split out so a test can stage the exact state a kill mid-[`remove_mission_dir`] leaves — the
/// marker, and nothing else — without having to kill anything.
fn remove_all_but_marker(dir: &Path) -> io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_name() == std::ffi::OsStr::new(MISSION_MARKER) {
            continue;
        }
        let child = entry.path();
        // `DirEntry::file_type` does not follow links, so a nested junction or symlink is
        // `is_symlink()` and never `is_dir()` — the same std property condition 2 rests on.
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            // A real directory: hand the subtree to std, which since 1.77 removes a nested reparse
            // point as a link rather than recursing through it.
            std::fs::remove_dir_all(&child)?;
        } else if file_type.is_symlink() {
            remove_link(&child)?;
        } else {
            std::fs::remove_file(&child)?;
        }
    }
    Ok(())
}

/// Remove a link itself, never what it points at.
///
/// Windows needs `remove_dir` for a **directory** reparse point (a junction, or `mklink /D`) and
/// `remove_file` for a file one, and the two are told apart from the link's own attributes — read off
/// `symlink_metadata`, so nothing here follows the link to ask.
#[cfg(windows)]
fn remove_link(path: &Path) -> io::Result<()> {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
    let meta = std::fs::symlink_metadata(path)?;
    if meta.file_attributes() & FILE_ATTRIBUTE_DIRECTORY != 0 {
        std::fs::remove_dir(path)
    } else {
        std::fs::remove_file(path)
    }
}

/// `unlink(2)` removes a symlink of either kind without following it.
#[cfg(not(windows))]
fn remove_link(path: &Path) -> io::Result<()> {
    std::fs::remove_file(path)
}

/// What one sweep did. Returned rather than logged so tests assert on it and the caller can trace it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SweepReport {
    /// Mission directories removed, in the order they were removed.
    pub removed: Vec<PathBuf>,
    /// **Mission-named** entries deliberately left alone: the name matched `cpe-swarm-<alnum>` but a
    /// later condition refused it (not a plain directory, no roster, too new, or unreadable — see the
    /// module header; every one of those is a refusal).
    ///
    /// Entries whose *name* is not ours are not counted at all. Counting them was the round-1 shape
    /// and it made the number unreadable: `%TEMP%` on the machine CPE-1974 measured holds thousands
    /// of unrelated entries, so a log line saying "left 2,140" would have been read as "left 2,140
    /// mission directories" while meaning "the temp directory has a lot in it".
    pub skipped: usize,
    /// Entries that met every condition but whose removal failed (another user's directory under
    /// `/tmp`'s sticky bit, a file held open on Windows). Counted, never hidden.
    pub failed: usize,
}

/// Remove mission directories under `root` that are plainly ours and older than `retention`.
///
/// `now` is injected so the retention edge is testable without sleeping. Every condition fails
/// closed; see this module's header for the list and the argument.
pub fn sweep_stale_mission_dirs(root: &Path, retention: Duration, now: SystemTime) -> SweepReport {
    let mut report = SweepReport::default();
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        // No temp directory to read is not a licence to do anything (CPE-1972).
        Err(_) => return report,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // 1. The name must be one we could have minted. An entry that fails this is not a mission
        //    directory at all, so it is not counted as one — `skipped` reports only entries this
        //    sweep considered and then refused. See [`SweepReport::skipped`].
        let named_ours = entry.file_name().to_str().map(is_mission_name).unwrap_or(false);
        if !named_ours {
            continue;
        }
        // 2. A real directory. `symlink_metadata` does NOT follow, and std reports a name-surrogate
        //    reparse point (a Unix symlink, a Windows junction) as a symlink rather than a directory
        //    — `FileType::is_dir()` is `!is_symlink() && is_directory()` on Windows — so this one
        //    predicate excludes links, files, and anything else that is not a plain directory. The
        //    link is never followed and never removed.
        //
        //    CPE-1929 pair, measured 2026-08-27 on Windows against
        //    `the_sweep_never_follows_or_removes_a_planted_link`:
        //      * disabled (`if false && !meta.is_dir()`) → RED (1 failed): the sweep deleted the
        //        planted junction, so the guard is reachable and this test is what reaches it;
        //      * predicate made to lie (`std::fs::metadata(&path)` — the *following* stat — instead
        //        of `symlink_metadata`, so the junction reports `is_dir() == true`) → RED (1 failed).
        //    Both red, so the refusal is live rather than shadowed.
        //
        //    A third arm, `meta.file_type().is_symlink() ||`, WAS written here and has been deleted:
        //    disabling the pair of them was red, but forcing `is_symlink()` alone to lie left the
        //    suite GREEN (4 passed) — `!meta.is_dir()` answers the same fact first on both platforms,
        //    which is CPE-1929's shadowed guard exactly. Deleted rather than left in, because a
        //    shadowed guard reads as coverage.
        let meta = match std::fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => {
                report.skipped += 1;
                continue;
            }
        };
        if !meta.is_dir() {
            report.skipped += 1;
            continue;
        }
        // 3. The roster we write into every mission, as a regular file. This is the ownership
        //    evidence: no marker, no delete.
        let marker_ok = std::fs::symlink_metadata(path.join(MISSION_MARKER))
            .map(|m| m.file_type().is_file())
            .unwrap_or(false);
        if !marker_ok {
            report.skipped += 1;
            continue;
        }
        // 4. Old enough. An unreadable mtime, or one in the future, is a skip.
        let old_enough = meta
            .modified()
            .ok()
            .and_then(|m| now.duration_since(m).ok())
            .map(|age| age >= retention)
            .unwrap_or(false);
        if !old_enough {
            report.skipped += 1;
            continue;
        }
        // 5. Remove it, roster last, so a console that exits mid-removal leaves something the next
        //    sweep can still finish. See [`remove_mission_dir`] and the module header.
        match remove_mission_dir(&path) {
            Ok(()) => report.removed.push(path),
            Err(_) => report.failed += 1,
        }
    }
    report
}

/// The startup sweep, in the real temp directory with the shipped retention. Called once from
/// `main` on the console path; separated so the policy (`where`, `how long`) is stated in one place.
pub fn sweep_stale_mission_dirs_now() -> SweepReport {
    sweep_stale_mission_dirs(&std::env::temp_dir(), SWEEP_RETENTION, SystemTime::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The retention edge, from both sides, with the clock injected.
    #[test]
    fn sweep_removes_only_directories_older_than_the_retention() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("cpe-swarm-aaaa");
        let fresh = root.path().join("cpe-swarm-bbbb");
        for d in [&old, &fresh] {
            std::fs::create_dir(d).unwrap();
            std::fs::write(d.join(MISSION_MARKER), b"[]").unwrap();
        }
        // "Now" is two days after the real mtimes, so both are two days old; then one hour after,
        // so neither is.
        let two_days = SystemTime::now() + Duration::from_secs(2 * 24 * 3600);
        let one_hour = SystemTime::now() + Duration::from_secs(3600);

        let kept = sweep_stale_mission_dirs(root.path(), SWEEP_RETENTION, one_hour);
        assert_eq!(kept.removed, Vec::<PathBuf>::new(), "nothing inside the retention window goes");
        assert!(old.exists() && fresh.exists());

        let swept = sweep_stale_mission_dirs(root.path(), SWEEP_RETENTION, two_days);
        assert_eq!(swept.removed.len(), 2, "both are past the retention window");
        assert!(!old.exists() && !fresh.exists());
    }

    /// The ownership evidence is load-bearing: a `cpe-swarm-`-named directory that is not a mission
    /// (no roster) is left alone however old it is.
    #[test]
    fn sweep_refuses_a_directory_without_our_roster() {
        let root = tempfile::tempdir().unwrap();
        let theirs = root.path().join("cpe-swarm-cccc");
        std::fs::create_dir(&theirs).unwrap();
        std::fs::write(theirs.join("something-else.json"), b"{}").unwrap();
        let later = SystemTime::now() + Duration::from_secs(30 * 24 * 3600);

        let r = sweep_stale_mission_dirs(root.path(), SWEEP_RETENTION, later);
        assert_eq!(r.removed, Vec::<PathBuf>::new(), "no roster, no delete");
        assert_eq!(r.skipped, 1);
        assert!(theirs.exists());
    }

    /// Names outside the exact `cpe-swarm-<alnum>` shape are not touched, and the shape cannot name
    /// anything outside `root`.
    #[test]
    fn sweep_refuses_every_name_that_is_not_exactly_ours() {
        let root = tempfile::tempdir().unwrap();
        let later = SystemTime::now() + Duration::from_secs(30 * 24 * 3600);
        for name in ["cpe-swarm", "cpe-swarms-1", "cpe-swarm-", "cpe-swarm-a.b", "other-1"] {
            let d = root.path().join(name);
            std::fs::create_dir(&d).unwrap();
            std::fs::write(d.join(MISSION_MARKER), b"[]").unwrap();
        }
        // A real mission alongside them, so "removed nothing" cannot pass by the loop never running.
        let ours = root.path().join("cpe-swarm-eeee");
        std::fs::create_dir(&ours).unwrap();
        std::fs::write(ours.join(MISSION_MARKER), b"[]").unwrap();

        let r = sweep_stale_mission_dirs(root.path(), SWEEP_RETENTION, later);
        assert_eq!(r.removed, vec![ours], "only the one whose name we could have minted");
        // Not `5`: an entry whose name is not ours is not a mission directory the sweep considered
        // and left alone, so it is not counted as one (CPE-1964 round 2 — `%TEMP%` holds thousands
        // of unrelated entries and "left N" must not report them).
        assert_eq!(r.skipped, 0);

        assert!(!is_mission_name("cpe-swarm-.."), "a traversal must never read as a mission id");
        assert!(!is_mission_name("cpe-swarm-a/b"));
        assert!(is_mission_name("cpe-swarm-1755300000000"), "the old millis names are still ours");
        assert!(is_mission_name("cpe-swarm-0f1e2d3c4b5a69788796a5b4c3d2e1f0"), "and the new ones");
    }

    /// A torn delete must be **self-healing**.
    ///
    /// The sweep is a detached thread, so the process can die part-way through a removal. The state
    /// that leaves is exactly what [`remove_all_but_marker`] produces, so this stages it directly
    /// rather than trying to kill something mid-call — then runs the real sweep over it and asserts
    /// the leftover is finished off instead of becoming permanent litter.
    ///
    /// CPE-1929 pair, measured 2026-08-27 on Windows against `cargo test --locked --lib`, with the
    /// `Compiling ai-console` line checked in both runs (a `touch` on `/mnt/z` does not reliably
    /// force a rebuild, and a stale-binary run is how a sabotage comes back falsely green):
    ///   * refusal **disabled** — `if false && …` on the marker skip, i.e. a plain "delete everything
    ///     in one pass" — → **RED**, 388 passed / **3 failed**: this test (the torn state has no
    ///     roster), plus both sweep tests, because `remove_mission_dir`'s `remove_file` then fails
    ///     `NotFound` and nothing gets removed at all;
    ///   * predicate made to **lie** — the skip comparing against `"mailbox.jsonl"` instead of
    ///     `MISSION_MARKER`, so the *wrong* file survives phase 1 — → **RED**, 388 passed /
    ///     **3 failed**, same three.
    ///
    /// So the ordering is live, not decorative: it is the only reason a torn directory stays
    /// sweepable, and it is reachable from a test.
    #[test]
    fn a_torn_delete_leaves_a_directory_the_next_sweep_still_removes() {
        let root = tempfile::tempdir().unwrap();
        let mission = root.path().join("cpe-swarm-dddd");
        std::fs::create_dir(&mission).unwrap();
        std::fs::write(mission.join(MISSION_MARKER), b"[]").unwrap();
        std::fs::write(mission.join("mailbox.jsonl"), b"{}").unwrap();
        std::fs::create_dir(mission.join("memory")).unwrap();
        std::fs::write(mission.join("memory").join("note-abc.md"), b"note").unwrap();

        // The state a kill mid-`remove_mission_dir` leaves behind.
        remove_all_but_marker(&mission).unwrap();
        let mut left: Vec<String> = std::fs::read_dir(&mission)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left,
            vec![MISSION_MARKER.to_string()],
            "the roster is the last thing standing, so the torn directory is still plainly ours"
        );

        let later = SystemTime::now() + Duration::from_secs(30 * 24 * 3600);
        let r = sweep_stale_mission_dirs(root.path(), SWEEP_RETENTION, later);
        assert_eq!(r.removed, vec![mission.clone()], "the next sweep finishes what the tear started");
        assert_eq!(r.failed, 0);
        assert!(!mission.exists());
    }

    /// Two mission directories minted back-to-back must not be guessable from each other, and must
    /// not be the old timestamp shape.
    #[test]
    fn minted_names_are_random_rather_than_a_timestamp() {
        let root = tempfile::tempdir().unwrap();
        let a = create_mission_dir_in(root.path()).unwrap();
        let b = create_mission_dir_in(root.path()).unwrap();
        assert_ne!(a, b);
        let suffix = |p: &PathBuf| {
            p.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .strip_prefix(MISSION_PREFIX)
                .expect("minted with the mission prefix")
                .to_string()
        };
        let (sa, sb) = (suffix(&a), suffix(&b));
        assert_eq!((sa.len(), sb.len()), (32, 32), "32 hex characters each");
        assert!(a.is_dir() && b.is_dir(), "and the directories really exist");
        // Two consecutive *timestamps* differ in one or two trailing characters. Two independent
        // 128-bit draws agree in half their positions by chance; requiring 16 differences out of 32
        // is failed by any counter-like sequence and passed by real entropy with probability
        // 1 - ~1e-13, so this is a live assertion rather than a flaky one.
        let differing = sa.bytes().zip(sb.bytes()).filter(|(x, y)| x != y).count();
        assert!(
            differing >= 16,
            "consecutive names must not be a countable sequence (the `<millis>` shape CPE-1964 \
             removed); {sa} vs {sb} differ in only {differing} of 32 positions"
        );
    }
}
