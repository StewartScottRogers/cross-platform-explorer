//! CPE-1975 — the executable reproduction of "the AI Console's rendezvous directory is a **constant**
//! path in the OS temp directory, and `create_dir_all` succeeds straight onto a pre-existing
//! junction".
//!
//! ## What is being demonstrated
//!
//! Two sites built `std::env::temp_dir().join("cpe-ai-console")` and materialised it with
//! `std::fs::create_dir_all`: `session_diag::trace` (which then appends the CPE-309 I/O trace log)
//! and `session_supervisor::write_port_file` (which then writes the session daemon's port). Three
//! properties compound:
//!
//! * **constant** — not `<pid>`, not `<millis>`, not random. `cpe-ai-console`, on every machine, for
//!   every release. There is no window to guess inside;
//! * **outside the project**, so none of CPE-1896 / CPE-1913 / CPE-1937's containment covers it;
//! * **`create_dir_all` traverses a reparse point** — it treats a junction or symlink as the
//!   directory it points at, so "the path is not there yet" becomes "the attacker picked the
//!   destination".
//!
//! ## Threat model, stated in halves
//!
//! On **Unix** `std::env::temp_dir()` is `$TMPDIR`/`/tmp` — a shared, world-writable namespace, so
//! any local user can plant the link, and the constant name means they need no timing at all. On
//! **Windows** it is the per-user `%LOCALAPPDATA%\Temp`, so the attack needs a process already
//! running as the same user. Both are real; they are not the same claim, and CPE-1952's "predictable
//! path in a shared namespace" framing must not be inherited wholesale.
//!
//! ## What the consequence is, honestly
//!
//! The ticket's headline was that the port file is a control channel. It is not reachable as one
//! today — `SessionDaemonHandle::discover_or_spawn` is the port file's only reader and writer and has
//! zero callers, and production learns the daemon's port from the child's own stdout pipe instead.
//! The full evidence is in `console_temp_dir`'s module header. What *is* live is the trace-log write
//! (this file's control test) and the host reaper's delete (`sidecar/host`'s companion file).
//!
//! ## Why the first test is the attack *succeeding*
//!
//! [`the_old_console_dir_primitive_writes_through_a_planted_link`] is the **sensitivity control**
//! (CPE-1937's lesson, PR #1075's model): it runs the pre-fix primitive verbatim and asserts the
//! console's files land in the attacker's directory. Without it the containment test proves nothing —
//! a harness that cannot demonstrate the attack cannot demonstrate its absence either. It is an
//! ordinary `#[test]`, not `#[ignore]`d, and runs on all three OSes in the `sidecar` CI job. If it
//! ever passes by *not* escaping, the fix is not "delete it", it is "find out what changed".
//!
//! ## Privileges — and why a link that cannot be planted is RED, never a skip
//!
//! Windows uses a **junction** (`junction::create`, the primitive behind `mklink /J`), which needs no
//! administrator rights and no Developer Mode; Unix uses `symlink(2)`, likewise. Every platform this
//! crate is built for is one or the other, so a failure to plant is a broken runner, not an
//! environment quirk, and [`Scene::planted`] **panics**. PR #1075's round 2 is why this paragraph
//! exists: its first draft returned early with a skip notice, libtest swallowed the macro's output
//! for the passing test, and both CI legs announced "verified nothing" to nobody. Where a notice is
//! genuinely the right answer (the Windows *file* symlink leg, which does need Developer Mode) it is
//! written with `writeln!` straight to the stderr handle — `cpe_server::skip_notice!` is unreachable
//! from a sidecar under ADR 0001.
//!
//! ## Where the links are planted, and why not at the literal production path
//!
//! Production's name is a **constant**, so unlike CPE-1964's random mission name this test *could*
//! plant at it. It deliberately does not, and the reasons are about the test being safe rather than
//! weaker: cargo runs a binary's tests on parallel threads, so two tests at one constant path race;
//! and `<temp>/cpe-ai-console` on a developer's machine is a **live rendezvous directory** holding
//! the trace log — a test must not be the destructive operation it is testing.
//!
//! Everything a regression could reach through is preserved: the plant is under the **real**
//! `std::env::temp_dir()` (asserted), its name is derived from the production constant
//! [`CONSOLE_DIR_NAME`] so a rename moves the fixture with it, and the function under test is the
//! exact one production calls — [`ensure_console_dir_at`], not a copy. It is deliberately **not** a
//! path inside a `tempfile::tempdir()`, which would be unreachable by any regression of the code
//! under test and so unfalsifiable (CPE-1929).
//! [`the_hardened_primitive_opens_the_real_rendezvous_directory`] then exercises the no-argument
//! production entry point at the literal real path, non-destructively.

use std::path::{Path, PathBuf};

use ai_console::console_temp_dir::{
    console_temp_dir, ensure_console_dir, ensure_console_dir_at, regular_file_or_absent,
    CONSOLE_DIR_NAME, DIAG_LOG_NAME, PORT_FILE_NAME,
};

/// Plant a directory link at `link` pointing at `target` — a junction on Windows, a symlink on Unix.
///
/// `Err` names the **step** that failed rather than only reporting that one did, so a red log on a
/// runner nobody can log into says which half broke.
fn stage_dir_link(target: &Path, link: &Path) -> Result<(), &'static str> {
    #[cfg(windows)]
    let made = junction::create(target, link).is_ok();
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(target, link).is_ok();
    // There is no third platform: this crate builds for Windows, macOS and Linux and the `sidecar`
    // CI job runs exactly those three. A fourth would need its own recorded decision here rather
    // than joining the others in one shared silent early return.
    #[cfg(not(any(windows, unix)))]
    let made = false;
    if !made {
        return Err("creating the directory link (junction on Windows, symlink(2) on Unix)");
    }
    // The premise, asserted rather than assumed: something is at `link`, and it leads to `target`.
    if std::fs::canonicalize(link).ok() != std::fs::canonicalize(target).ok() {
        return Err("the planted link does not resolve to the target directory");
    }
    Ok(())
}

/// Plant a **file** link. On Windows this needs Developer Mode or `SeCreateSymbolicLinkPrivilege`,
/// unlike a junction — so its caller reports instead of panicking, out loud.
#[cfg(windows)]
fn stage_file_link(target: &Path, link: &Path) -> Result<(), &'static str> {
    if std::os::windows::fs::symlink_file(target, link).is_err() {
        return Err("creating the file symlink (needs Developer Mode / SeCreateSymbolicLinkPrivilege)");
    }
    Ok(())
}

#[cfg(unix)]
fn stage_file_link(target: &Path, link: &Path) -> Result<(), &'static str> {
    if std::os::unix::fs::symlink(target, link).is_err() {
        return Err("creating the file symlink");
    }
    Ok(())
}

/// The scene: the attacker's link, planted at a real `<temp>/cpe-ai-console-…` path.
///
/// Both link-planting tests use the same name, so [`SCENE_LOCK`] serialises them. [`Drop`] removes
/// the link on every exit path including a panic — `remove_dir_all` on a junction/symlink removes
/// **the link**, not what it points at (std has refused to recurse into a reparse point since 1.77),
/// so the cleanup cannot itself escape. A stray junction left in `%TEMP%` pointing at a real
/// directory would be a hazard for whoever runs next, which is the thing this ticket is about.
struct Scene {
    _lock: std::sync::MutexGuard<'static, ()>,
    _root: tempfile::TempDir,
    /// Where the attacker's link points.
    victim: PathBuf,
    /// The real predictable rendezvous path, and so the link's own location.
    predictable: PathBuf,
}

/// Serialises the tests that plant at the same real path. Poisoning is ignored deliberately: the
/// control test asserting an escape may panic on a future regression, and the containment test must
/// still run and report rather than be masked by a poison error.
static SCENE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl Scene {
    /// Build the tree and plant the link.
    ///
    /// **Panics when the link cannot be planted**, rather than reporting an unstaged leg as a pass.
    /// See this file's "Privileges" section for the argument.
    fn planted() -> Self {
        let lock = SCENE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Derived from the production constant, so renaming it moves the fixture too; the suffix is
        // what keeps this off the machine's live rendezvous directory (see the header).
        let name = format!("{CONSOLE_DIR_NAME}-cpe1975-{}", std::process::id());
        let predictable = std::env::temp_dir().join(&name);
        // A leftover from an earlier run (or the other test) must not be mistaken for the code under
        // test having created one.
        let _ = std::fs::remove_dir_all(&predictable);
        let root = tempfile::tempdir().expect("scratch root");
        let victim = root.path().join("victim");
        std::fs::create_dir_all(&victim).expect("victim");
        if let Err(step) = stage_dir_link(&victim, &predictable) {
            // The panic happens BEFORE the `Scene` exists, so its `Drop` guard cannot run — and
            // `stage_dir_link` can fail with the link already created (the second arm: created, but
            // not resolving). Clean up on this path too, before saying anything.
            let _ = std::fs::remove_dir_all(&predictable);
            panic!(
                "[CPE-1975] the attacker's directory link could not be planted, so this leg \
                 verified NOTHING — going red rather than passing quietly.\n  \
                 failed step: {step}\n  link:        {}\n  target:      {}\n\
                 A junction (Windows) and `symlink(2)` (Unix) both need no elevated privilege and \
                 no Developer Mode, so this means the runner or its temp filesystem changed, not \
                 that the environment is unusual. Fix the runner; do not soften this back into a \
                 skip. (CPE-1717's policy, applied by hand: `cpe_server::fsutil::require_staged` is \
                 unreachable from a sidecar under ADR 0001.)",
                predictable.display(),
                victim.display(),
            );
        }
        Scene { _lock: lock, _root: root, victim, predictable }
    }

    /// Everything the attacker's directory holds right now, sorted — the on-disk evidence both tests
    /// assert against.
    fn victim_entries(&self) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(&self.victim)
            .expect("victim readable")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }
}

impl Drop for Scene {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.predictable);
        // Never panic in `Drop` (it would mask the test's own failure), but do not go quiet either.
        // `writeln!` straight to the process's stderr handle, NOT `eprintln!`: libtest installs its
        // capture inside the print macros and discards what it captured when the test passes, so the
        // macro would route this warning to nobody on the one harness that matters.
        if self.predictable.exists() {
            use std::io::Write as _;
            let _ = writeln!(
                std::io::stderr(),
                "WARNING: CPE-1975's test could not remove its planted link at {} — remove it by hand",
                self.predictable.display()
            );
        }
    }
}

/// **SENSITIVITY CONTROL.** The pre-CPE-1975 primitive, run verbatim against a planted link.
///
/// This must FAIL to contain — the trace log and the port file must land in the victim directory.
#[test]
fn the_old_console_dir_primitive_writes_through_a_planted_link() {
    let scene = Scene::planted();
    assert_eq!(scene.victim_entries(), Vec::<String>::new(), "the victim starts empty");

    // What `session_diag::trace` and `session_supervisor::write_port_file` used to do.
    // `create_dir_all` is the whole defect: it walks the junction/symlink like an ordinary directory
    // instead of refusing the entry that is there.
    std::fs::create_dir_all(&scene.predictable)
        .expect("create_dir_all succeeds onto a pre-existing link — that IS the bug");
    {
        use std::io::Write as _;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(scene.predictable.join(DIAG_LOG_NAME))
            .expect("trace log");
        writeln!(f, "DIAG-LINE").expect("append");
    }
    std::fs::write(scene.predictable.join(PORT_FILE_NAME), b"65001").expect("port file");

    // The on-disk evidence: the bytes are in the attacker's directory.
    assert_eq!(
        scene.victim_entries(),
        vec![PORT_FILE_NAME.to_string(), DIAG_LOG_NAME.to_string()],
        "the console's rendezvous files landed inside the attacker's directory at {}",
        scene.victim.display()
    );
    assert_eq!(
        std::fs::read(scene.victim.join(PORT_FILE_NAME)).expect("escaped port file readable"),
        b"65001",
        "and it is our bytes, not a coincidence"
    );
}

/// **THE FIX.** The same scene — same planted link, same real temp directory — driven through the
/// real production primitive.
///
/// Assertions are on the filesystem: the call refuses, the attacker's directory stays empty, and the
/// link still leads nowhere useful.
#[test]
fn the_hardened_primitive_refuses_a_planted_link() {
    let scene = Scene::planted();
    assert_eq!(scene.victim_entries(), Vec::<String>::new(), "the victim starts empty");

    let err = ensure_console_dir_at(&scene.predictable)
        .expect_err("a path occupied by a link must be refused, not adopted");
    assert_eq!(
        err.kind(),
        std::io::ErrorKind::AlreadyExists,
        "the refusal rides `mkdir`'s own AlreadyExists; got {err:?}"
    );

    // 1. Nothing reached the attacker.
    assert_eq!(
        scene.victim_entries(),
        Vec::<String>::new(),
        "the attacker's directory must stay empty; found {:?}",
        scene.victim_entries()
    );

    // 2. Nothing was written through the path either. (`read_dir` follows the link, so an empty
    //    listing here is the same fact as (1) seen from the other side; both are asserted because a
    //    future regression could reach one without the other.)
    assert!(
        std::fs::read_dir(&scene.predictable).expect("the link still resolves").next().is_none(),
        "something was written through the rendezvous path"
    );

    // 3. And `scene.predictable` is not a stand-in: it is under the real `std::env::temp_dir()` and
    //    carries the production directory name as its prefix, so (1) and (2) are statements about
    //    the kind of path a regression would reach for. Asserted rather than left to the reader,
    //    because "the fixture is realistic" is a claim and CPE-1933 says derive claims (CPE-1964's
    //    same leg, one file over).
    assert_eq!(
        scene.predictable.parent(),
        Some(std::env::temp_dir().as_path()),
        "the scene must be planted in the real temp directory or it proves nothing"
    );
    assert!(scene
        .predictable
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with(CONSOLE_DIR_NAME));
}

/// A link at the **trace log's / port file's own name**, inside a directory that really is ours.
///
/// The directory refusal above cannot see this: the directory is genuine and only the leaf is a link.
/// This is [`regular_file_or_absent`], the second refusal, and it is a different fact — which is why
/// it is not shadowed by the first.
///
/// A file symlink on Windows needs Developer Mode, unlike a junction, so this is the one leg that can
/// genuinely be unavailable. It reports on the **real stderr handle** rather than panicking, and says
/// which leg still ran; the directory legs above run everywhere.
#[test]
fn a_link_at_the_port_file_name_is_refused() {
    let root = tempfile::tempdir().expect("scratch root");
    let victim = root.path().join("victim");
    std::fs::create_dir_all(&victim).expect("victim");
    let secret = victim.join("secret.txt");
    std::fs::write(&secret, b"KEEP-ME").expect("secret");

    let dir = root.path().join(CONSOLE_DIR_NAME);
    ensure_console_dir_at(&dir).expect("a real rendezvous directory");
    let port_file = dir.join(PORT_FILE_NAME);
    if let Err(step) = stage_file_link(&secret, &port_file) {
        use std::io::Write as _;
        let _ = writeln!(
            std::io::stderr(),
            "[CPE-1975] NOT VERIFIED on this runner: the file symlink could not be planted ({step}); \
             a Windows *file* symlink needs Developer Mode, unlike the junction the directory legs \
             use. This leg checked nothing here — the directory legs did run."
        );
        return;
    }

    assert!(!regular_file_or_absent(&port_file), "a symlink at the port file's name must be refused");
    assert_eq!(std::fs::read(&secret).expect("readable"), b"KEEP-ME", "and nothing wrote through it");
}

/// "Refuses everything" must not be able to pass the tests above, so: the real no-argument production
/// entry point, at the literal real `<temp>/cpe-ai-console`.
///
/// Non-destructive on purpose — it creates the directory if absent and adopts it if present, which is
/// exactly the rendezvous contract, and never removes it. That is what lets this test point at the
/// machine's live directory when the link-planting tests must not.
#[test]
fn the_hardened_primitive_opens_the_real_rendezvous_directory() {
    let dir = ensure_console_dir().expect("the real rendezvous directory must open");
    assert_eq!(dir, console_temp_dir());
    assert!(dir.is_dir(), "and it is a real directory");
    assert_eq!(dir.parent(), Some(std::env::temp_dir().as_path()));
    assert_eq!(dir.file_name().unwrap().to_string_lossy(), CONSOLE_DIR_NAME);
    // Idempotent: a rendezvous is adopted on every later run, not re-created.
    ensure_console_dir().expect("the second call adopts it");
}
