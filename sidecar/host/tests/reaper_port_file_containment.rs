//! CPE-1975 — the executable reproduction of "the host's startup reaper consults and **unlinks** the
//! session-daemon port file at a fixed `<temp>/cpe-ai-console/` path, following whatever link is
//! planted there".
//!
//! ## What is being demonstrated
//!
//! `reap_orphan_session_daemons` used to finish with
//!
//! ```text
//! Some(pf) if pf.exists() => std::fs::remove_file(pf).is_ok(),
//! ```
//!
//! Neither call refuses a link: `Path::exists` stats through every component and `remove_file`
//! resolves the path the same way. The path is `std::env::temp_dir().join("cpe-ai-console")
//! .join("session-daemon.port")` — a **constant**, not even a timestamp, so an attacker never has to
//! guess a window. Plant a junction (Windows) or symlink (Unix) at `cpe-ai-console` and the host's
//! startup sweep deletes `<their directory>/session-daemon.port` on every launch.
//!
//! This is the third of the ticket's three sites and the only one with no `create_dir_all` in it: it
//! is a **reader and deleter** of the same redirectable path, which is why "fix the two that create"
//! would have been the enumeration defect (CPE-1932) one more time.
//!
//! ## Threat model, in halves
//!
//! On **Unix** `std::env::temp_dir()` is `$TMPDIR`/`/tmp`, a shared world-writable namespace — any
//! local user plants the link. On **Windows** it is the per-user `%LOCALAPPDATA%\Temp`, so the attack
//! needs a process already running as the same user. Both are real and they are not the same claim.
//!
//! ## Why the first test is the attack *succeeding*
//!
//! [`the_old_reaper_primitive_deletes_through_a_planted_link`] is the **sensitivity control**
//! (CPE-1937's lesson, PR #1075's model): it runs the pre-fix primitive verbatim and asserts the
//! attacker's file is gone. Without it, the containment test below proves nothing — a harness that
//! cannot demonstrate the attack cannot demonstrate its absence. It is an ordinary `#[test]`, not
//! `#[ignore]`d, and runs on all three OSes in the `sidecar` CI job.
//!
//! ## Privileges — and why a link that cannot be planted is RED, never a skip
//!
//! A **junction** (`junction::create`, the primitive behind `mklink /J`) needs no administrator
//! rights and no Developer Mode; `symlink(2)` likewise. Every platform this crate builds for is one
//! or the other, so a failure to plant is a broken runner, not an environment quirk, and
//! [`plant_or_panic`] **panics**. PR #1075 lost this leg twice: a skip notice printed from a passing
//! test is swallowed by libtest's capture, so both CI legs announced "verified nothing" to nobody.
//! The panic message is written with `writeln!` to the real stderr handle where it has to survive a
//! pass — `cpe_server::skip_notice!` is unreachable from a sidecar crate under ADR 0001.
//!
//! ## Where the links are planted, and why it is not the literal production path
//!
//! The production path's last component is a **constant**, so — unlike CPE-1964's random mission
//! name — this test *could* plant at it. It deliberately does not, for two reasons that are about
//! the test being safe rather than about it being weaker:
//!
//! * cargo runs a test binary's tests on parallel threads, and two tests planting at one constant
//!   path race each other; and
//! * `<temp>/cpe-ai-console` on a developer's machine is a **live rendezvous directory** holding the
//!   CPE-309 trace log. A test must not be the destructive operation it is testing.
//!
//! What is preserved is everything a regression could reach through: the link is a real junction /
//! `symlink(2)`, on the same filesystem semantics, and the function under test is the exact one
//! `src-tauri` calls at startup — `reap_orphan_session_daemons`, not a copy of it. The port file's
//! own name comes from the production constant [`PORT_FILE_NAME`], so a rename moves the fixture
//! with it, and [`the_production_port_file_path_is_the_one_under_test`] pins the real path itself.

use std::path::{Path, PathBuf};

use sidecar_host::reaper::{
    default_session_daemon_port_file, reap_orphan_session_daemons, CONSOLE_DIR_NAME, PORT_FILE_NAME,
};

/// Plant a directory link at `link` pointing at `target` — a junction on Windows, a symlink on Unix.
///
/// `Err` names the **step** that failed, so a red log on a runner nobody can log into says which half
/// broke.
fn stage_dir_link(target: &Path, link: &Path) -> Result<(), &'static str> {
    #[cfg(windows)]
    let made = junction::create(target, link).is_ok();
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(target, link).is_ok();
    // There is no third platform: this crate builds for Windows, macOS and Linux and the `sidecar`
    // CI job runs exactly those three. A fourth would need its own recorded decision here rather
    // than joining the others in a shared silent early return.
    #[cfg(not(any(windows, unix)))]
    let made = false;
    if !made {
        return Err("creating the directory link (junction on Windows, symlink(2) on Unix)");
    }
    if std::fs::canonicalize(link).ok() != std::fs::canonicalize(target).ok() {
        return Err("the planted link does not resolve to the target directory");
    }
    Ok(())
}

/// Plant a **file** link at `link` pointing at the file `target`.
///
/// On Windows a *file* symlink needs Developer Mode or `SeCreateSymbolicLinkPrivilege`, which a
/// junction does not — so this one can legitimately be unavailable, and it is the single place in
/// this file that reports rather than panics. Its caller states what it does when the link cannot be
/// planted, out loud, on the real stderr handle.
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

/// Plant a directory link or die loudly. See this file's "Privileges" section.
fn plant_or_panic(target: &Path, link: &Path) {
    if let Err(step) = stage_dir_link(target, link) {
        // The link may exist but not resolve (the second arm), so clean up before saying anything.
        let _ = std::fs::remove_dir_all(link);
        panic!(
            "[CPE-1975] the attacker's directory link could not be planted, so this leg verified \
             NOTHING — going red rather than passing quietly.\n  failed step: {step}\n  link:   {}\n  \
             target: {}\nA junction (Windows) and `symlink(2)` (Unix) both need no elevated \
             privilege and no Developer Mode, so this means the runner or its temp filesystem \
             changed, not that the environment is unusual. Fix the runner; do not soften this back \
             into a skip. (CPE-1717's policy applied by hand — `cpe_server::fsutil::require_staged` \
             is unreachable from a sidecar under ADR 0001.)",
            link.display(),
            target.display(),
        );
    }
}

/// The scene: an attacker's directory holding a `session-daemon.port`, and a directory link planted
/// at the rendezvous name in front of it.
struct Scene {
    _root: tempfile::TempDir,
    victim: PathBuf,
    /// The link, standing where `<temp>/cpe-ai-console` stands in production.
    rendezvous: PathBuf,
    /// The path the reaper is handed: `<link>/session-daemon.port`.
    port_file: PathBuf,
}

impl Scene {
    fn planted() -> Self {
        let root = tempfile::tempdir().expect("scratch root");
        let victim = root.path().join("victim");
        std::fs::create_dir_all(&victim).expect("victim");
        std::fs::write(victim.join(PORT_FILE_NAME), b"THEIRS").expect("the attacker's own file");
        let rendezvous = root.path().join(CONSOLE_DIR_NAME);
        plant_or_panic(&victim, &rendezvous);
        let port_file = rendezvous.join(PORT_FILE_NAME);
        Scene { _root: root, victim, rendezvous, port_file }
    }
}

/// **SENSITIVITY CONTROL.** The pre-CPE-1975 reaper primitive, run verbatim against a planted link.
///
/// This must FAIL to contain — the attacker's file must be deleted. It is the evidence that the
/// scene is a real attack and that the containment test below measures something. If it ever passes
/// by *not* escaping, the fix is not "delete it", it is "find out what changed".
#[test]
fn the_old_reaper_primitive_deletes_through_a_planted_link() {
    let scene = Scene::planted();
    assert!(scene.victim.join(PORT_FILE_NAME).exists(), "the victim's file is there to lose");

    // Exactly what `reap_orphan_session_daemons` used to run:
    //     Some(pf) if pf.exists() => std::fs::remove_file(pf).is_ok(),
    assert!(scene.port_file.exists(), "`exists()` follows the link — that is half the bug");
    assert!(std::fs::remove_file(&scene.port_file).is_ok(), "`remove_file` follows it too");

    assert!(
        !scene.victim.join(PORT_FILE_NAME).exists(),
        "the reaper deleted a file inside the attacker's directory at {}",
        scene.victim.display()
    );
}

/// **THE FIX.** The same scene, driven through the real production entry point.
///
/// `our_exes` is empty on purpose: `is_our_session_daemon` requires a match against that list, so no
/// process on the runner can match and the sweep's process half is a no-op. The port-file half — the
/// half under test — runs exactly as it does at app startup.
///
/// Every assertion is on the **filesystem**, never on the returned verdict; `port_file_removed` is
/// checked too, but as a second fact, because this family's whole history is reports that read fine
/// while bytes went somewhere else.
#[test]
fn the_reaper_does_not_delete_through_a_planted_directory_link() {
    let scene = Scene::planted();

    let report = reap_orphan_session_daemons(&[], Some(&scene.port_file));

    assert!(
        scene.victim.join(PORT_FILE_NAME).exists(),
        "the sweep deleted through the planted link into {}",
        scene.victim.display()
    );
    assert_eq!(
        std::fs::read(scene.victim.join(PORT_FILE_NAME)).expect("readable"),
        b"THEIRS",
        "the attacker's file must be untouched, not merely present"
    );
    assert!(!report.port_file_removed, "and the report says so rather than claiming a removal");
    assert!(scene.rendezvous.exists(), "the link itself is left alone — refusing is not deleting");
}

/// A link at the **port file's own name**, inside a directory that really is ours.
///
/// The parent-directory refusal above cannot see this one: the directory is genuine, and only the
/// leaf is a link. This is the second refusal in `remove_stale_port_file` and it is a different fact.
///
/// A file symlink on Windows needs Developer Mode, unlike a junction — so this is the one leg that
/// can be genuinely unavailable. It does not panic; it says so on the **real stderr handle** (not
/// `eprintln!`, which libtest swallows on a pass) so an unstaged leg is never silent. The directory
/// half above still runs everywhere and is the leg that covers the escape that matters.
#[test]
fn the_reaper_does_not_delete_through_a_link_at_the_port_file_name() {
    let root = tempfile::tempdir().expect("scratch root");
    let victim = root.path().join("victim");
    std::fs::create_dir_all(&victim).expect("victim");
    let secret = victim.join("secret.txt");
    std::fs::write(&secret, b"KEEP-ME").expect("secret");

    // A real directory of ours, with only the leaf redirected.
    let real_dir = root.path().join(CONSOLE_DIR_NAME);
    std::fs::create_dir(&real_dir).expect("real rendezvous directory");
    let port_file = real_dir.join(PORT_FILE_NAME);
    if let Err(step) = stage_file_link(&secret, &port_file) {
        use std::io::Write as _;
        let _ = writeln!(
            std::io::stderr(),
            "[CPE-1975] NOT VERIFIED on this runner: the file symlink could not be planted ({step}); \
             a Windows *file* symlink needs Developer Mode, unlike the junction the directory leg \
             uses. This leg checked nothing here — the directory leg \
             (`the_reaper_does_not_delete_through_a_planted_directory_link`) did run."
        );
        return;
    }

    let report = reap_orphan_session_daemons(&[], Some(&port_file));

    assert!(secret.exists(), "the sweep deleted through the leaf link into {}", secret.display());
    assert_eq!(std::fs::read(&secret).expect("readable"), b"KEEP-ME");
    assert!(!report.port_file_removed);
}

/// "Refuses everything" must not be able to pass the tests above: a real, plain port file in a real
/// directory is still removed, which is the whole point of the sweep (CPE-483).
#[test]
fn the_reaper_still_removes_a_real_stale_port_file() {
    let root = tempfile::tempdir().expect("scratch root");
    let dir = root.path().join(CONSOLE_DIR_NAME);
    std::fs::create_dir(&dir).expect("dir");
    let port_file = dir.join(PORT_FILE_NAME);
    std::fs::write(&port_file, b"65001").expect("port file");

    let report = reap_orphan_session_daemons(&[], Some(&port_file));

    assert!(report.port_file_removed, "a genuine stale port file is still swept");
    assert!(!port_file.exists());
}

/// The scenes above plant at a sibling name rather than at the literal production path (see this
/// file's header for why). So the production path is pinned here instead, from the production
/// constants — otherwise "the fixture resembles production" would be a claim rather than a
/// derivation (CPE-1933).
#[test]
fn the_production_port_file_path_is_the_one_under_test() {
    let pf = default_session_daemon_port_file();
    assert_eq!(pf.file_name().unwrap().to_string_lossy(), PORT_FILE_NAME);
    let dir = pf.parent().expect("the port file lives in the rendezvous directory");
    assert_eq!(dir.file_name().unwrap().to_string_lossy(), CONSOLE_DIR_NAME);
    assert_eq!(dir.parent(), Some(std::env::temp_dir().as_path()), "under the REAL temp directory");
}
