//! CPE-1964 — the executable reproduction of "the swarm mission directory is a guessable path in
//! the OS temp directory, and `create_dir_all` succeeds straight onto a pre-existing junction".
//!
//! ## What is being demonstrated
//!
//! `handle_swarm_run` used to open a mission at
//! `std::env::temp_dir().join(format!("cpe-swarm-{}", now_millis()))` and materialise it with
//! `std::fs::create_dir_all` (inside `write_members` / `write_mcp_config`). Three properties
//! compound, exactly as at CPE-1952's staging site:
//!
//! * **guessable** — a millisecond timestamp, so an attacker watching the temp directory learns the
//!   cadence and can pre-create the next few names; the 55 leaked directories CPE-1964 measured were
//!   publishing that cadence in plain sight;
//! * **outside the project**, so none of CPE-1896 / CPE-1913 / CPE-1937's containment covers it;
//! * **`create_dir_all` traverses a reparse point** — it treats a junction or symlink as the
//!   directory it points at, so "the path does not exist yet" becomes "the attacker picked the
//!   destination", and the mission's roster, mailbox, memory notes, per-agent MCP configs and
//!   Windows task files all land there.
//!
//! ## Threat model, stated in halves
//!
//! On **Unix** `std::env::temp_dir()` is `$TMPDIR`/`/tmp` — a shared, world-writable namespace, so
//! any local user can plant the link. On **Windows** it is the per-user `%LOCALAPPDATA%\Temp`, so
//! the attack needs a process already running as the same user. Both are real; they are not the same
//! claim, and CPE-1952's "predictable path in a shared namespace" framing must not be inherited
//! wholesale.
//!
//! ## The shape of this file, and why the first test is the *attack succeeding*
//!
//! [`the_old_mission_primitive_writes_through_a_planted_link`] is the **sensitivity control**
//! (CPE-1937's lesson, and PR #1075's model): it runs the pre-fix primitive verbatim and asserts the
//! mission's files land in the attacker's directory. Without it the containment test proves nothing
//! — a harness that cannot demonstrate the attack cannot demonstrate its absence either. It is an
//! ordinary `#[test]`, not `#[ignore]`d, and runs on all three OSes in the `sidecar` CI job. If it
//! ever passes by *not* escaping, the fix is not "delete it", it is "find out what changed".
//!
//! [`the_hardened_mission_primitive_refuses_a_planted_link`] is the fix: the same planted link, at
//! the same real path in the real temp directory, driven through the real production creation
//! primitive — and the assertions are on the **filesystem**: the victim gains nothing and the link
//! still leads to an empty directory. Every assertion here is about where bytes ended up, never
//! about a returned verdict; this family's whole history is reports that read `ok: true` while files
//! landed somewhere else.
//!
//! ## Why the planted path is the real one, and why `create_mission_dir_at` is the seam
//!
//! The fix gives mission directories 32 random hex characters, so nobody — including this test — can
//! plant at the name production will actually pick. What production *does* is call
//! `create_mission_dir_at(<temp>/cpe-swarm-<name>)`, and that is the function under test here, at a
//! real `<temp>/cpe-swarm-…` path this process planted a link at. It is deliberately **not** a
//! stand-in path inside a `tempfile::tempdir()`: a stand-in is unreachable by any regression of the
//! code under test, so every assertion about it would be unfalsifiable — safe-looking and worthless
//! at once, which is what CPE-1929 is about.
//!
//! ## Deterministic, so CI actually runs it
//!
//! There is no race to win. The link is planted *before* the code under test runs, which is exactly
//! the attacker's position against the pre-fix code: the name was computable in advance, so they
//! never needed to win anything.
//!
//! ## Privileges — and why a link that cannot be planted is RED, never a skip
//!
//! Windows uses a **junction** (`junction::create`, the same primitive `mklink /J` uses), which
//! needs no administrator rights and no Developer Mode; Unix uses `symlink(2)`, likewise. Every
//! platform this crate is built for is one or the other, so a failure to plant is a broken runner,
//! not an environment quirk, and [`Scene::planted`] **panics** rather than returning.
//!
//! That is `cpe_server::fsutil::require_staged`'s CPE-1717 policy applied by hand — a sidecar may
//! not depend on `cpe-server` (ADR 0001), so the helper is unreachable from here — and one notch
//! stricter, because creating a link inside one's own temp directory is not a mechanism a
//! developer's environment can legitimately lack. PR #1075's round 2 is the reason this paragraph
//! exists: its first draft returned early with an `eprintln!` skip notice, libtest swallowed the
//! macro's output for the passing test, and both CI legs announced "verified nothing" to nobody.

use std::path::{Path, PathBuf};

use ai_console::swarm_mission_dir::{
    create_mission_dir_at, is_mission_name, sweep_stale_mission_dirs, MISSION_MARKER,
    MISSION_PREFIX, SWEEP_RETENTION,
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
    // There is no third platform: this crate builds for Windows, macOS and Linux, and the `sidecar`
    // CI job runs exactly those three. A fourth would need its own recorded decision here — the
    // platform named and the reason stated — rather than joining the others in one shared silent
    // early return.
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

/// The scene: the attacker's link, planted at a **real** `<temp>/cpe-swarm-…` path.
///
/// `predictable` is `std::env::temp_dir().join(format!("cpe-swarm-{}", <this pid>))` — the real OS
/// temp directory, the real mission-name shape (`is_mission_name` is asserted over it below, from
/// the production module, so the fixture cannot drift into a shape production would never produce).
/// The pid stands in for the millisecond timestamp the pre-fix code used: both are small integers
/// any local process can compute, and using the pid makes the name unique per test process so
/// parallel `cargo test` invocations do not collide with each other.
///
/// Both tests plant at the same name, so [`SCENE_LOCK`] serialises them; cargo runs a test binary's
/// tests on parallel threads by default. [`Drop`] removes the link on every exit path including a
/// panic — `remove_dir_all` on a junction/symlink removes **the link**, not the directory it points
/// at (Rust's std has refused to recurse into a reparse point since 1.77), so the cleanup cannot
/// itself escape. A stray junction left in `%TEMP%` pointing at a real directory would be a hazard
/// for whoever runs next.
struct Scene {
    _lock: std::sync::MutexGuard<'static, ()>,
    _root: tempfile::TempDir,
    /// Where the attacker's link points.
    victim: PathBuf,
    /// The real predictable mission path, and so the link's own location.
    predictable: PathBuf,
}

/// Serialises the two tests that plant at the same real path. Poisoning is ignored deliberately: the
/// control test asserting an escape may panic on a future regression, and the second test must still
/// run and report rather than be masked by a poison error.
static SCENE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl Scene {
    /// Build the tree and plant the link.
    ///
    /// **Panics when the link cannot be planted**, rather than reporting an unstaged leg as a pass.
    /// See this file's "Privileges" section for the argument.
    fn planted() -> Self {
        let lock = SCENE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let name = format!("{MISSION_PREFIX}{}", std::process::id());
        // The fixture must be a name production could mint, or the assertions are about a shape no
        // regression would ever reach. Derived from the production predicate rather than asserted by
        // eye (CPE-1933).
        assert!(is_mission_name(&name), "the fixture name must be a real mission-id shape: {name}");
        let predictable = std::env::temp_dir().join(&name);
        // A leftover from an earlier run (or from the other test) must not be mistaken for the code
        // under test having created one.
        let _ = std::fs::remove_dir_all(&predictable);
        let root = tempfile::tempdir().expect("scratch root");
        let victim = root.path().join("victim");
        std::fs::create_dir_all(&victim).expect("victim");
        if let Err(step) = stage_dir_link(&victim, &predictable) {
            // The panic happens BEFORE the `Scene` exists, so its `Drop` guard cannot run — and
            // `stage_dir_link` can fail with the link already created (the second arm: created, but
            // not resolving to the target). Clean up on this path too, before saying anything.
            let _ = std::fs::remove_dir_all(&predictable);
            panic!(
                "[CPE-1964] the attacker's directory link could not be planted, so this leg \
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
        // Never panic in `Drop` (it would mask the test's own failure), but do not go quiet either:
        // a junction left behind in the temp directory is precisely the hazard this ticket is about.
        // `writeln!` straight to the process's stderr handle, NOT `eprintln!`: libtest installs its
        // capture inside the print macros and discards what it captured when the test passes, so the
        // macro would route this warning to nobody on the one harness that matters. This is what
        // `cpe_server::skip_notice!` expands to, written longhand because ADR 0001 puts the macro
        // out of a sidecar's reach.
        if self.predictable.exists() {
            use std::io::Write as _;
            let _ = writeln!(
                std::io::stderr(),
                "WARNING: CPE-1964's test could not remove its planted link at {} — remove it by hand",
                self.predictable.display()
            );
        }
    }
}

/// **SENSITIVITY CONTROL.** The pre-CPE-1964 mission primitive, run verbatim against a planted link.
///
/// This must FAIL to contain — the mission's files must land in the victim directory. It is the
/// evidence that the scene is a real attack and that the containment test below is measuring
/// something. If the link cannot be planted, [`Scene::planted`] panics: a control that could not
/// stage its own attack has verified nothing, and the honest outcome for that is red.
#[test]
fn the_old_mission_primitive_writes_through_a_planted_link() {
    let scene = Scene::planted();
    assert_eq!(scene.victim_entries(), Vec::<String>::new(), "the victim starts empty");

    // What `handle_swarm_run` → `write_members` / `seed_kickoff` / `write_mcp_config` used to do.
    // `create_dir_all` is the whole defect: it walks the junction/symlink like an ordinary directory
    // instead of refusing the entry that is there.
    std::fs::create_dir_all(&scene.predictable)
        .expect("create_dir_all succeeds onto a pre-existing link — that IS the bug");
    std::fs::write(scene.predictable.join(MISSION_MARKER), b"MISSION-ROSTER").expect("roster");
    std::fs::write(scene.predictable.join("mcp-claude-builder1.json"), b"MISSION-MCP-CONFIG")
        .expect("mcp config");

    // The on-disk evidence: the bytes are in the attacker's directory, not in a mission directory.
    assert_eq!(
        scene.victim_entries(),
        // `victim_entries` sorts, and `mcp-` sorts before `members.json`.
        vec!["mcp-claude-builder1.json".to_string(), MISSION_MARKER.to_string()],
        "the mission scaffolding landed inside the attacker's directory at {}",
        scene.victim.display()
    );
    assert_eq!(
        std::fs::read(scene.victim.join(MISSION_MARKER)).expect("escaped roster readable"),
        b"MISSION-ROSTER",
        "and it is our bytes, not a coincidence"
    );
}

/// **THE FIX.** The same scene — same planted link, same real path — driven through the real
/// production creation primitive.
///
/// Assertions are on the filesystem: the call refuses, the attacker's directory stays empty, and the
/// link still leads nowhere useful.
#[test]
fn the_hardened_mission_primitive_refuses_a_planted_link() {
    let scene = Scene::planted();
    assert_eq!(scene.victim_entries(), Vec::<String>::new(), "the victim starts empty");

    let err = create_mission_dir_at(&scene.predictable)
        .expect_err("an occupied path must be refused, not walked through");
    assert_eq!(
        err.kind(),
        std::io::ErrorKind::AlreadyExists,
        "the refusal is `mkdir`'s own AlreadyExists — atomic with the create, so there is no \
         check-then-use window; got {err:?}"
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
        "something was written through the mission path"
    );

    // 3. And `scene.predictable` is not a stand-in: it is under the real `std::env::temp_dir()` and
    //    carries a name the production predicate accepts, so (1) and (2) are statements about the
    //    exact kind of path a regression would reach for. Asserted rather than left to the reader,
    //    because "the fixture is realistic" is a claim and CPE-1933 says derive claims instead of
    //    making them.
    assert_eq!(
        scene.predictable.parent(),
        Some(std::env::temp_dir().as_path()),
        "the scene must be planted in the real temp directory or it proves nothing"
    );
    assert!(is_mission_name(&scene.predictable.file_name().unwrap().to_string_lossy()));
}

/// "Refuses everything" must not be able to pass the test above, so: the same primitive on an
/// unoccupied path in the real temp directory creates a real, usable directory.
#[test]
fn the_hardened_primitive_still_opens_a_real_mission_directory() {
    let path = std::env::temp_dir().join(format!("{MISSION_PREFIX}live{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    create_mission_dir_at(&path).expect("an unoccupied path must succeed");
    assert!(path.is_dir(), "and it is a real directory we can write a roster into");
    std::fs::write(path.join(MISSION_MARKER), b"[]").expect("writable");
    let _ = std::fs::remove_dir_all(&path);
}

/// The sweep must never follow or remove a planted link, however old and however well-named — the
/// destructive half of CPE-1964 is a new operation over a directory other processes share, so its
/// refusals get the same on-disk treatment as the creation half.
///
/// The **root is injected** here, unlike the creation tests above, and that is a deliberate
/// difference rather than a stand-in: running the shipped sweep over the real `std::env::temp_dir()`
/// with a forced clock would delete this machine's actual mission directories, including one another
/// `ai-console` might be using right now. A test must not *be* the destructive operation it is
/// testing. Everything else is real — the link is a real junction / `symlink(2)`, the names are real
/// mission names (`is_mission_name` is asserted over them, from the production module), and
/// `sweep_stale_mission_dirs` is the exact function `sweep_stale_mission_dirs_now` calls; only the
/// directory it is pointed at differs.
#[test]
fn the_sweep_never_follows_or_removes_a_planted_link() {
    let root = tempfile::tempdir().expect("scratch root");
    let victim = root.path().join("victim");
    std::fs::create_dir_all(&victim).expect("victim");
    // Make the victim look maximally like a stale mission of ours, so nothing but the reparse-point
    // refusal can be what saves it: our roster inside, and a `now` far enough ahead that every age
    // test passes.
    std::fs::write(victim.join(MISSION_MARKER), b"[]").expect("roster in the victim");

    let bait = root.path().join(format!("{MISSION_PREFIX}deadbeef"));
    assert!(is_mission_name(&bait.file_name().unwrap().to_string_lossy()), "a real mission name");
    if let Err(step) = stage_dir_link(&victim, &bait) {
        let _ = std::fs::remove_dir_all(&bait);
        panic!(
            "[CPE-1964] the attacker's directory link could not be planted, so this leg verified \
             NOTHING — going red rather than passing quietly.\n  failed step: {step}\n  link: {}\n  \
             target: {}\nA junction (Windows) and `symlink(2)` (Unix) need no elevated privilege; \
             a failure here means the runner changed. Do not soften this into a skip.",
            bait.display(),
            victim.display()
        );
    }
    // And one genuinely stale mission of ours beside it, so "the sweep did nothing at all" cannot
    // pass this test either.
    let ours = root.path().join(format!("{MISSION_PREFIX}0011223344556677"));
    std::fs::create_dir(&ours).expect("ours");
    std::fs::write(ours.join(MISSION_MARKER), b"[]").expect("our roster");

    let far_future = std::time::SystemTime::now() + std::time::Duration::from_secs(400 * 24 * 3600);
    let report = sweep_stale_mission_dirs(root.path(), SWEEP_RETENTION, far_future);

    assert_eq!(report.removed, vec![ours.clone()], "our own stale mission is the only thing removed");
    assert!(!ours.exists());
    assert!(
        victim.join(MISSION_MARKER).exists(),
        "the sweep deleted through the planted link into {}",
        victim.display()
    );
    let mut left: Vec<String> = std::fs::read_dir(&victim)
        .expect("victim readable")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(left, vec![MISSION_MARKER.to_string()], "the attacker's directory is untouched");

    // The bait link is left in place by the sweep — removing it would be the sweep acting on
    // something it could not prove was ours. Removed here because this test is what planted it;
    // `remove_dir_all` on a link removes the link, not what it points at (re-asserted below, since
    // that property is the reason the sweep's own cleanup cannot escape either).
    let _ = std::fs::remove_dir_all(&bait);
    assert!(!bait.exists(), "the planted link is cleaned up");
    assert!(victim.join(MISSION_MARKER).exists(), "and removing the link did not follow it");
}

/// The case where the sweep's delete **actually runs**: a link nested *inside* a genuine mission
/// directory of ours.
///
/// The test above stops at the top of the tree — all five conditions refuse, so `remove_dir_all` is
/// never reached. Here every condition passes on the parent, the removal runs for real, and the
/// question is whether it walks a link on the way down. Two positions are covered because CPE-1964
/// round 2 made them two different code paths:
///
/// * a link **directly inside** the mission directory now goes through the module's own
///   `remove_link` (roster-last removal deletes each child itself), which must unlink the link — and
///   on Windows a junction needs `RemoveDirectory`, not `DeleteFile`, so this is the leg that would
///   catch getting that backwards;
/// * a link **one level deeper** is inside a real subdirectory, which is handed whole to
///   `std::fs::remove_dir_all`, so this leg is the derivation of the module header's claim that std
///   refuses to recurse through a reparse point (Rust >= 1.77) rather than a restatement of it.
///
/// Both targets hold a `secret.txt`. If either survives the sweep only because the sweep did nothing,
/// the final assertion that the mission directory is gone goes red.
#[test]
fn the_sweep_does_not_walk_a_link_nested_inside_a_real_mission_directory() {
    let root = tempfile::tempdir().expect("scratch root");

    // Two attacker directories, outside the mission, each holding something worth keeping.
    let secret_top = root.path().join("secret-top");
    let secret_deep = root.path().join("secret-deep");
    for d in [&secret_top, &secret_deep] {
        std::fs::create_dir_all(d).expect("attacker directory");
        std::fs::write(d.join("secret.txt"), b"KEEP-ME").expect("secret");
    }

    // A genuine, stale mission of ours: real name, our roster, ordinary files, a real subdirectory.
    let mission = root.path().join(format!("{MISSION_PREFIX}00ff11ee22dd33cc"));
    assert!(is_mission_name(&mission.file_name().unwrap().to_string_lossy()), "a real mission name");
    std::fs::create_dir(&mission).expect("mission");
    std::fs::write(mission.join(MISSION_MARKER), b"[]").expect("roster");
    std::fs::write(mission.join("mailbox.jsonl"), b"{}").expect("mailbox");
    let memory = mission.join("memory");
    std::fs::create_dir(&memory).expect("memory");
    std::fs::write(memory.join("note-abc.md"), b"note").expect("note");

    let link_top = mission.join("escape-top");
    let link_deep = memory.join("escape-deep");
    for (target, link) in [(&secret_top, &link_top), (&secret_deep, &link_deep)] {
        if let Err(step) = stage_dir_link(target, link) {
            let _ = std::fs::remove_dir_all(link);
            panic!(
                "[CPE-1964] the nested directory link could not be planted, so this leg verified \
                 NOTHING — going red rather than passing quietly.\n  failed step: {step}\n  \
                 link: {}\n  target: {}\nA junction (Windows) and `symlink(2)` (Unix) need no \
                 elevated privilege; a failure here means the runner changed. Do not soften this \
                 into a skip.",
                link.display(),
                target.display()
            );
        }
    }

    let far_future = std::time::SystemTime::now() + std::time::Duration::from_secs(400 * 24 * 3600);
    let report = sweep_stale_mission_dirs(root.path(), SWEEP_RETENTION, far_future);

    // The delete really ran — without this the two survivals below would prove nothing.
    assert_eq!(report.removed, vec![mission.clone()], "the mission itself is removed");
    assert_eq!(report.failed, 0, "and cleanly");
    assert!(!mission.exists(), "the whole mission directory is gone");

    // And it took the links, not what they pointed at.
    for (label, target) in [("top-level", &secret_top), ("nested", &secret_deep)] {
        assert!(
            target.join("secret.txt").exists(),
            "the sweep followed the {label} link and deleted through it into {}",
            target.display()
        );
        assert_eq!(
            std::fs::read(target.join("secret.txt")).expect("secret readable"),
            b"KEEP-ME",
            "the {label} link's target must be untouched, not merely present"
        );
    }
}
