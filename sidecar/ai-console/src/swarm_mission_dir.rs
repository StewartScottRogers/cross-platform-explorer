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
//! 5. `remove_dir_all` succeeds. On Unix `/tmp` the sticky bit means another user's mission
//!    directory fails here rather than being removed, which is the correct outcome and is counted,
//!    not hidden.
//!
//! `remove_dir_all` cannot itself escape: since Rust 1.77 std refuses to recurse into a reparse
//! point and removes the link instead — and condition 2 has already excluded a link at the top.

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

/// What one sweep did. Returned rather than logged so tests assert on it and the caller can trace it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SweepReport {
    /// Mission directories removed, in the order they were removed.
    pub removed: Vec<PathBuf>,
    /// Entries examined and deliberately left alone (wrong name, not a plain directory, no roster,
    /// too new, or unreadable — see the module header; every one of those is a refusal).
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
        // 1. The name must be one we could have minted.
        let named_ours = entry.file_name().to_str().map(is_mission_name).unwrap_or(false);
        if !named_ours {
            report.skipped += 1;
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
        // 5. Remove it. `remove_dir_all` will not recurse into a reparse point (Rust >= 1.77) and
        //    (2) already established the top of the tree is not one.
        match std::fs::remove_dir_all(&path) {
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
        let r = sweep_stale_mission_dirs(root.path(), SWEEP_RETENTION, later);
        assert_eq!(r.removed, Vec::<PathBuf>::new(), "none of those is a mission name");
        assert_eq!(r.skipped, 5);

        assert!(!is_mission_name("cpe-swarm-.."), "a traversal must never read as a mission id");
        assert!(!is_mission_name("cpe-swarm-a/b"));
        assert!(is_mission_name("cpe-swarm-1755300000000"), "the old millis names are still ours");
        assert!(is_mission_name("cpe-swarm-0f1e2d3c4b5a69788796a5b4c3d2e1f0"), "and the new ones");
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
