//! CPE-1952 — the executable reproduction of "the catalog staging dir is a predictable path outside
//! the project, and `create_dir_all` succeeds straight onto a pre-existing junction".
//!
//! ## What is being demonstrated
//!
//! `do_fetch_catalog` used to stage the downloaded bundle at
//! `std::env::temp_dir().join(format!("cpe-catalog-stage-{}", std::process::id()))` and materialise
//! it with `std::fs::create_dir_all`. Three properties compound:
//!
//! * **predictable** — a shared namespace plus a pid, both computable by any local process;
//! * **outside the project**, so none of CPE-1896 / CPE-1913 / CPE-1937's containment covers it;
//! * **`create_dir_all` traverses a reparse point** — it treats a junction or symlink as the
//!   directory it points at, and creates whatever components are missing, so "the path does not
//!   exist yet" becomes "the attacker picked the destination".
//!
//! ## The shape of this file, and why the first test is the *attack succeeding*
//!
//! [`the_old_staging_primitive_writes_through_a_planted_link`] is the **sensitivity control**
//! (CPE-1937's lesson): it runs the pre-fix primitive verbatim and asserts the staged bytes land in
//! the attacker's directory. Without it, the containment test below proves nothing — a harness that
//! cannot demonstrate the attack cannot demonstrate its absence either, and this repo has shipped
//! that mistake more than once. If it ever goes green-by-passing (no escape) the fix is not
//! "delete it", it is "find out what changed", because the containment test's meaning depends on it.
//!
//! [`the_fetched_bundle_never_touches_the_filesystem`] is the fix: the same planted link, at the
//! same real predictable path, a **real signed bundle applied through the real production entry point**
//! (`catalog::apply_bundle_source_at`, which is what `do_fetch_catalog` calls) — and the assertions
//! are on the **filesystem**: the predictable path is never created, the victim directory gains
//! nothing, and the catalog still installs correctly into `out`.
//!
//! Every assertion here is about where bytes ended up, never about a returned verdict. That is
//! deliberate: this family's whole history is reports that read `ok: true` while files landed
//! somewhere else.
//!
//! ## Deterministic, so CI actually runs it
//!
//! There is no race to win. The link is planted *before* the code under test runs, which is exactly
//! the attacker's position — the staging path was computable in advance, so they never needed to win
//! anything. Both tests are ordinary `#[test]`s and run on all three OSes in the `sidecar` CI job.
//!
//! ## Privileges — and why a link that cannot be planted is RED, never a skip
//!
//! Windows uses a **junction** (`junction::create`, the same primitive `mklink /J` uses), which
//! needs no administrator rights and no Developer Mode; Unix uses `symlink(2)`, likewise. Every
//! platform this crate is built for is one or the other, and on both the mechanism is *supposed* to
//! work — so a failure to plant is a broken runner, not an environment quirk, and [`Scene::planted`]
//! panics rather than returning.
//!
//! That is `cpe_server::fsutil::require_staged`'s CPE-1717 policy, applied here **by hand and one
//! notch stricter**. By hand because a sidecar may not depend on `cpe-server` (ADR 0001), so the
//! helper itself is unreachable from this crate — the policy travels, the call does not. Stricter
//! because `require_staged` is deliberately lenient off CI, for legs whose mechanism a developer's
//! environment might legitimately lack (a deny ACE, a root Docker shell, an ACL-less filesystem);
//! creating a junction or a symlink inside one's own temp directory is not such a mechanism, so
//! there is no environment to be lenient about here and no `LegitimateSkip` arm to write.
//!
//! The stakes are why this is worth spelling out. The first test below is a **sensitivity control**:
//! its whole job is to show the escape still happens with the fix disabled. A control that returns
//! green because it could not plant its link proves nothing, and proves it invisibly — which is
//! worse than not having it, because the green reads as coverage. The first draft of this file
//! returned early with an `eprintln!` skip notice and `fsutil`'s
//! `skip_notices_never_use_a_captured_print_macro` scan failed the build over it: libtest swallows
//! that macro's output for a passing test, so both legs would have announced "verified nothing" to
//! nobody. It is the same argument that put the planted link at the **real** predictable path rather
//! than a stand-in, taken one step further out.

use std::path::{Path, PathBuf};

use sidecar_host::catalog::{apply_bundle_source_at, load_versions, sign_bundle, MemBundle};

/// The ed25519 seed the "release pipeline" signs with, so the bundle applied below is genuinely
/// signed by the key the "client" trusts. The point is containment, not signature behaviour.
const SEED: [u8; 32] = [0x3c; 32];

const AGENT: &str = "claude";

fn trusted_key() -> String {
    hex::encode(ed25519_dalek::SigningKey::from_bytes(&SEED).verifying_key().to_bytes())
}

/// Plant a directory link at `link` pointing at `target` — a junction on Windows, a symlink on Unix.
///
/// `Err` names the **step** that failed rather than only reporting that one did, so a red log on a
/// runner nobody can log into says which half broke. (Same reasoning as
/// `cpe_server::fsutil::require_staged_reason`, which exists for exactly that; unreachable from a
/// sidecar under ADR 0001.)
fn stage_dir_link(target: &Path, link: &Path) -> Result<(), &'static str> {
    #[cfg(windows)]
    let made = junction::create(target, link).is_ok();
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(target, link).is_ok();
    // There is no third platform: this crate builds for Windows, macOS and Linux, and the `sidecar`
    // CI job runs exactly those three. A fourth would need its own recorded decision here — the
    // platform named and the reason stated — rather than joining the others in one shared silent
    // early return, which is what this arm used to feed.
    #[cfg(not(any(windows, unix)))]
    let made = false;
    if !made {
        return Err("creating the directory link (junction on Windows, symlink(2) on Unix)");
    }
    // The premise, asserted rather than assumed: something is at `link`, it is not a real directory
    // we made ourselves, and it leads to `target`.
    if std::fs::canonicalize(link).ok() != std::fs::canonicalize(target).ok() {
        return Err("the planted link does not resolve to the target directory");
    }
    Ok(())
}

/// The scene: the attacker's link, planted at **the real pre-fix staging path**.
///
/// `predictable` is `std::env::temp_dir().join(format!("cpe-catalog-stage-{}", std::process::id()))`
/// — the exact expression CPE-1952 deleted from `do_fetch_catalog`, evaluated here, in the real temp
/// directory, with this process's real pid. It is deliberately **not** a stand-in path inside a
/// `tempfile::tempdir()`: a stand-in cannot be reached by any regression of the code under test, so
/// every assertion about it would be unfalsifiable — safe-looking and worthless at once, which is
/// what CPE-1929 is about. The real path is reachable, so the assertions are live.
///
/// Both tests plant at the same name (same pid), so [`SCENE_LOCK`] serialises them; cargo runs a
/// test binary's tests on parallel threads by default. [`Drop`] removes the link on every exit path
/// including a panic — `remove_dir_all` on a junction/symlink removes **the link**, not the
/// directory it points at (measured on Windows and on real ext4; Rust's std has refused to recurse
/// into a reparse point since 1.77), so the cleanup cannot itself escape. A stray junction left in
/// `%TEMP%` pointing at a real directory would be a hazard for whoever runs next.
struct Scene {
    _lock: std::sync::MutexGuard<'static, ()>,
    _root: tempfile::TempDir,
    /// Where the attacker's link points.
    victim: PathBuf,
    /// The real pre-fix staging path, and so the link's own location.
    predictable: PathBuf,
    /// The installed-catalog directory (`catalog_dir(app)` in production).
    out: PathBuf,
}

/// Serialises the two tests that plant at the same real path. Poisoning is ignored deliberately: the
/// control test asserting an escape may panic on a future regression, and the second test must still
/// run and report rather than be masked by a poison error.
static SCENE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl Scene {
    /// Build the tree and plant the link.
    ///
    /// **Panics when the link cannot be planted**, rather than reporting an unstaged leg as a pass.
    /// See this file's "Privileges" section for the argument: on every platform this crate builds
    /// for, the mechanism needs no privilege, so a failure means the runner changed.
    fn planted() -> Self {
        let lock = SCENE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // The pre-fix path expression, written once so nothing here can drift about what "the
        // predictable staging path" is.
        let predictable =
            std::env::temp_dir().join(format!("cpe-catalog-stage-{}", std::process::id()));
        // A leftover from an earlier run (or from the other test) must not be mistaken for the code
        // under test having created one.
        let _ = std::fs::remove_dir_all(&predictable);
        let root = tempfile::tempdir().expect("scratch root");
        let victim = root.path().join("victim");
        let out = root.path().join("catalog");
        std::fs::create_dir_all(&victim).expect("victim");
        std::fs::create_dir_all(&out).expect("out");
        if let Err(step) = stage_dir_link(&victim, &predictable) {
            // The panic happens BEFORE the `Scene` exists, so its `Drop` guard cannot run — and
            // `stage_dir_link` can fail with the link already created (the second arm: created, but
            // not resolving to the target). Measured while red-proofing this very change: the
            // sabotaged run left a live junction behind in `%TEMP%`, which is the exact hazard this
            // file exists about. Clean up on this path too, before saying anything.
            let _ = std::fs::remove_dir_all(&predictable);
            panic!(
                "[CPE-1952] the attacker's directory link could not be planted, so this leg \
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
        Scene { _lock: lock, _root: root, victim, predictable, out }
    }

    /// Everything the attacker's directory holds right now, sorted — the on-disk evidence both
    /// tests assert against.
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
        // a junction left behind in the shared temp directory is precisely the hazard this ticket is
        // about, so say so loudly enough to be found in a CI log. `writeln!` straight to the
        // process's stderr handle, NOT `eprintln!`: libtest installs its capture inside the print
        // macros and discards what it captured when the test passes, so the macro would route this
        // warning to nobody on the one harness that matters. The emitter is load-bearing — this is
        // what `cpe_server::skip_notice!` expands to, written out longhand because ADR 0001 puts
        // the macro out of a sidecar's reach.
        if self.predictable.exists() {
            use std::io::Write as _;
            let _ = writeln!(
                std::io::stderr(),
                "WARNING: CPE-1952's test could not remove its planted link at {} — remove it by hand",
                self.predictable.display()
            );
        }
    }
}

/// A real signed bundle, as `catalog-sign` produces it, with the release asset names mapped to the
/// member names the apply engine reads.
fn signed_bundle(version: u64) -> MemBundle {
    let manifest = format!(r#"{{"schema_version":1,"id":"{AGENT}","run":"r{version}"}}"#).into_bytes();
    let files = sign_bundle(&[(AGENT.to_string(), manifest)], &hex::encode(SEED), version)
        .expect("sign_bundle");
    let mut bundle = MemBundle::new();
    for (name, data) in files {
        let member = match name.as_str() {
            "catalog-index.json" => "index.json".to_string(),
            "catalog-index.json.sig" => "index.json.sig".to_string(),
            other => other.to_string(),
        };
        bundle.insert(member, data);
    }
    bundle
}

/// **SENSITIVITY CONTROL.** The pre-CPE-1952 staging primitive, run verbatim against a planted link.
///
/// This must FAIL to contain — the staged bytes must land in the victim directory. It is the
/// evidence that the scene is a real attack and that the containment test below is measuring
/// something. If the link cannot be planted, [`Scene::planted`] panics: a control that could not
/// stage its own attack has verified nothing, and the honest outcome for that is red.
#[test]
fn the_old_staging_primitive_writes_through_a_planted_link() {
    let scene = Scene::planted();
    assert_eq!(scene.victim_entries(), Vec::<String>::new(), "the victim starts empty");

    // The three lines `do_fetch_catalog` used to run. `create_dir_all` is the whole defect: it walks
    // the junction/symlink like an ordinary directory instead of refusing the entry that is there.
    std::fs::create_dir_all(&scene.predictable)
        .expect("create_dir_all succeeds onto a pre-existing link — that IS the bug");
    std::fs::write(scene.predictable.join("index.json"), b"STAGED-CATALOG-INDEX").expect("stage index");
    std::fs::write(scene.predictable.join("index.json.sig"), b"STAGED-SIG").expect("stage sig");

    // The on-disk evidence: the bytes are in the attacker's directory, not in staging.
    assert_eq!(
        scene.victim_entries(),
        vec!["index.json".to_string(), "index.json.sig".to_string()],
        "the staged bundle landed inside the attacker's directory at {}",
        scene.victim.display()
    );
    assert_eq!(
        std::fs::read(scene.victim.join("index.json")).expect("escaped index readable"),
        b"STAGED-CATALOG-INDEX",
        "and it is our bytes, not a coincidence"
    );
}

/// **THE FIX.** The same scene — same planted link, same predictable path — driven through the real
/// production apply entry point with the bundle in memory.
///
/// Assertions are on the filesystem, in both directions: nothing reaches the attacker, and the
/// catalog still installs. "Refuses everything" must not be able to pass this.
#[test]
fn the_fetched_bundle_never_touches_the_filesystem() {
    let scene = Scene::planted();
    assert_eq!(scene.victim_entries(), Vec::<String>::new(), "the victim starts empty");

    let vpath = scene.out.join("versions.json");
    let report = apply_bundle_source_at(
        &signed_bundle(7),
        &scene.out,
        &[trusted_key()],
        &vpath,
        &[],
        &[],
    )
    .expect("a first-run baseline is absent, not corrupt");

    // 1. Nothing reached the attacker.
    assert_eq!(
        scene.victim_entries(),
        Vec::<String>::new(),
        "the attacker's directory must stay empty; found {:?}",
        scene.victim_entries()
    );

    // 2. The predictable path was never even opened for writing — the link the attacker planted is
    //    still just their link, with nothing behind it. (`read_dir` follows the link, so an empty
    //    listing here is the same fact as (1) seen from the other side; both are asserted because a
    //    future regression could reach one without the other.)
    assert!(
        std::fs::read_dir(&scene.predictable).expect("the link still resolves").next().is_none(),
        "something was written through the predictable staging path"
    );

    // 3. And `scene.predictable` is not a stand-in: it is `temp_dir()/cpe-catalog-stage-<pid>` for
    //    the real temp dir and this process's real pid, so (1) and (2) are statements about the
    //    exact path a regression would reach for. Asserted rather than left to the reader, because
    //    "the fixture is realistic" is a claim and CPE-1933 says derive claims instead of making
    //    them.
    assert_eq!(
        scene.predictable,
        std::env::temp_dir().join(format!("cpe-catalog-stage-{}", std::process::id())),
        "the scene must be planted at the real pre-fix staging path or it proves nothing"
    );

    // 4. And the catalog actually installed — so "contains everything by doing nothing" cannot pass.
    assert!(report.index_ok, "the index verified");
    assert_eq!(report.applied, vec![AGENT.to_string()], "the entry applied");
    let installed = std::fs::read(scene.out.join(format!("{AGENT}.json"))).expect("manifest installed");
    assert!(
        String::from_utf8_lossy(&installed).contains(r#""run":"r7""#),
        "the installed manifest is the bundle's content"
    );
    assert_eq!(
        load_versions(&vpath).expect("baseline readable").get(AGENT).copied(),
        Some(7),
        "the anti-rollback baseline advanced, so the whole cycle ran"
    );
}

/// The same bundle, applied twice, to show the memory arm behaves identically to the directory arm
/// on the one property a reviewer would worry a refactor broke: anti-rollback still refuses a
/// replay. Filesystem-asserted (the installed manifest does not change).
#[test]
fn the_memory_arm_still_enforces_anti_rollback() {
    let out = tempfile::tempdir().expect("out");
    let vpath = out.path().join("versions.json");
    let keys = [trusted_key()];

    let first = apply_bundle_source_at(&signed_bundle(9), out.path(), &keys, &vpath, &[], &[])
        .expect("baseline absent");
    assert_eq!(first.applied, vec![AGENT.to_string()]);

    // An older bundle, correctly signed, must not be installed.
    let second = apply_bundle_source_at(&signed_bundle(4), out.path(), &keys, &vpath, &[], &[])
        .expect("baseline readable");
    assert!(second.applied.is_empty(), "an older version must not apply");
    let installed = std::fs::read(out.path().join(format!("{AGENT}.json"))).expect("manifest");
    assert!(
        String::from_utf8_lossy(&installed).contains(r#""run":"r9""#),
        "the on-disk manifest must still be the newer content"
    );
    assert_eq!(load_versions(&vpath).expect("baseline").get(AGENT).copied(), Some(9));
}
