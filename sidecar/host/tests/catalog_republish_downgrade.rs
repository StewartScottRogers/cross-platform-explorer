//! CPE-1941 — the executable reproduction of "re-running an old tag's release republishes stale
//! manifests under a *newer* version, and anti-rollback accepts them".
//!
//! ## What is being demonstrated
//!
//! `.github/workflows/release.yml`'s `catalog` job used to compute the version it stamps on every
//! catalog entry as `VERSION=$(date +%s)` — a wall-clock reading taken **at publish time**. The
//! trust engine in `sidecar/host/src/catalog.rs` enforces anti-rollback by comparing exactly that
//! number and nothing else: `VersionStanding::refusal()` returns `None` (the only route to
//! `Accept`) precisely when the incoming `version` is strictly greater than the installed one.
//!
//! Put those two together and the engine's core assumption — *a higher version means fresher
//! content* — is simply false, because the number is produced by the act of publishing rather than
//! by the thing published. Re-running the workflow on an old tag signs that tag's **old** manifests
//! with **today's** timestamp. Everything else about the bundle is impeccable: the index signature
//! verifies, each manifest's own signature verifies, every sha256 binds, the schema is supported.
//! The engine has no way to notice the content went backwards. And it needs no key compromise —
//! only the ability to press "Re-run jobs" on an old tag.
//!
//! ## The shape of this file
//!
//! Every test here drives the **real** publish path (`catalog::sign_bundle`, the same function
//! `catalog-sign` calls) and the **real** apply path (`catalog::apply_bundle`, the same function
//! the host runs on a fetched bundle). Nothing is stubbed; the only thing that varies between the
//! "before" and "after" tests is the *one number* the release workflow passes to `sign_bundle`:
//!
//!   * `publish_time_version(...)`  — the old scheme, `date +%s` when the job ran.
//!   * `commit_time_version(...)`   — the new scheme, the committer timestamp of the tagged commit
//!     (`.github/workflows/scripts/catalog-version.sh`).
//!
//! `republishing_an_old_tag_is_accepted_under_publish_time_versions` is the bug, red-proofed by
//! asserting the on-disk manifest actually reverts to the old bytes — not merely that a verdict
//! enum said `Applied`. `republishing_an_old_tag_is_refused_under_commit_time_versions` is the same
//! sequence after the fix, and `a_genuinely_newer_release_is_still_accepted_...` is the other
//! direction, so "refuses everything" cannot pass for a fix.

use std::collections::BTreeMap;
use std::path::Path;

use sidecar_host::catalog::{
    apply_bundle, sign_bundle, ApplyOutcome, ApplyReport, CatalogIndex, VersionMap,
};

/// The ed25519 seed the "release pipeline" signs with in these tests. Fixed, so every bundle below
/// is genuinely signed by the same trusted key the "client" trusts — the point of the reproduction
/// is that a downgrade rides in on *correctly signed* content.
const SEED: [u8; 32] = [0x5a; 32];

fn trusted_key() -> String {
    hex::encode(ed25519_dalek::SigningKey::from_bytes(&SEED).verifying_key().to_bytes())
}

/// One agent id, present in every bundle below, so the sequence is about versions and content and
/// nothing else.
const AGENT: &str = "claude";

/// The manifest bytes a given "tag" of the repo would have published.
fn manifest_at_tag(tag: &str) -> Vec<u8> {
    format!(r#"{{"schema_version":1,"id":"{AGENT}","run":"{tag}"}}"#).into_bytes()
}

/// Stage a signed bundle for `tag`'s content at `version`, exactly as the release job produces it,
/// then apply it as the host would. Returns the apply report; `installed` is mutated in place, so
/// a sequence of calls models a client that keeps its version map across fetches.
fn publish_then_apply(
    tag: &str,
    version: u64,
    out: &Path,
    installed: &mut VersionMap,
) -> ApplyReport {
    let stage = tempfile::tempdir().expect("staging dir");
    let bytes = manifest_at_tag(tag);
    let files = sign_bundle(&[(AGENT.to_string(), bytes)], &hex::encode(SEED), version)
        .expect("sign_bundle");
    for (name, data) in &files {
        // The fetch saves the index under `index.json`; sign_bundle emits it as the release asset
        // name `catalog-index.json`. Map the two names here rather than renaming on disk after the
        // fact, so the staging dir only ever holds what the host would actually have fetched.
        let staged = match name.as_str() {
            "catalog-index.json" => "index.json",
            "catalog-index.json.sig" => "index.json.sig",
            other => other,
        };
        std::fs::write(stage.path().join(staged), data).expect("stage file");
    }
    apply_bundle(stage.path(), out, &[trusted_key()], installed, &[])
}

/// What the manifest on disk says right now — the thing that actually decides which install/run
/// recipe the sidecar will execute. Asserting on this (rather than only on `ApplyReport`) is what
/// makes the reproduction a red-proof: it shows the *content* moving backwards, not just a verdict.
fn installed_tag(out: &Path) -> String {
    let bytes = std::fs::read(out.join(format!("{AGENT}.json"))).expect("installed manifest");
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("manifest json");
    v.get("run").and_then(|s| s.as_str()).expect("run field").to_string()
}

// --- The two version schemes, as the release workflow computes them ------------------------------

/// OLD scheme: `VERSION=$(date +%s)` — whatever the clock said when the job ran. The argument is
/// the wall-clock instant of that run, and note what is *absent*: the tag being published.
fn publish_time_version(publish_instant: u64) -> u64 {
    publish_instant
}

/// NEW scheme: the committer timestamp of the tagged commit. The argument is a property of the
/// content; the publish instant is not an input at all, which is the entire fix.
fn commit_time_version(tag_commit_instant: u64) -> u64 {
    tag_commit_instant
}

// Fabricated but realistically-ordered instants (epoch seconds). Read: v1 was committed, then
// published; v2 was committed, then published; the attacker's re-run happens later still.
const V1_COMMITTED: u64 = 1_787_100_000;
const V1_PUBLISHED: u64 = 1_787_101_800; // 30 min after the commit — a normal release
const V2_COMMITTED: u64 = 1_787_200_000;
const V2_PUBLISHED: u64 = 1_787_201_800;
const RERUN_OF_V1_HAPPENS_AT: u64 = 1_787_300_000; // someone re-runs the v1 tag, much later
const V3_COMMITTED: u64 = 1_787_400_000;

// --- 1. The bug ----------------------------------------------------------------------------------

#[test]
fn republishing_an_old_tag_is_accepted_under_publish_time_versions() {
    let out = tempfile::tempdir().unwrap();
    let mut installed: VersionMap = BTreeMap::new();

    // Release v1, then release v2. Ordinary history.
    let r1 = publish_then_apply("v1", publish_time_version(V1_PUBLISHED), out.path(), &mut installed);
    assert_eq!(r1.applied, vec![AGENT.to_string()]);
    let r2 = publish_then_apply("v2", publish_time_version(V2_PUBLISHED), out.path(), &mut installed);
    assert_eq!(r2.applied, vec![AGENT.to_string()]);
    assert_eq!(installed_tag(out.path()), "v2");
    assert_eq!(installed.get(AGENT), Some(&V2_PUBLISHED));

    // Now press "Re-run jobs" on the **v1 tag**. The checkout is v1's, so the manifests signed are
    // v1's; the version stamped is `date +%s` *now*, which is larger than v2's.
    let stale = publish_time_version(RERUN_OF_V1_HAPPENS_AT);
    assert!(stale > V2_PUBLISHED, "the re-run's number is newer than the newest real release");
    let r3 = publish_then_apply("v1", stale, out.path(), &mut installed);

    // THE BUG: accepted. Index signature ok, manifest signature ok, sha256 binds, schema supported,
    // anti-rollback satisfied — and the installed manifest has silently gone back to v1's content.
    assert!(r3.index_ok);
    assert_eq!(r3.applied, vec![AGENT.to_string()], "old content accepted as an upgrade");
    assert!(r3.rejected.is_empty());
    assert_eq!(installed_tag(out.path()), "v1", "the CONTENT went backwards, not just the verdict");
    assert_eq!(
        installed.get(AGENT),
        Some(&RERUN_OF_V1_HAPPENS_AT),
        "and the client now records a version higher than the real newest release, so the genuine \
         v2 bundle can never be re-applied either",
    );
}

// --- 2. The fix, both directions -----------------------------------------------------------------

#[test]
fn republishing_an_old_tag_is_refused_under_commit_time_versions() {
    let out = tempfile::tempdir().unwrap();
    let mut installed: VersionMap = BTreeMap::new();

    publish_then_apply("v1", commit_time_version(V1_COMMITTED), out.path(), &mut installed);
    publish_then_apply("v2", commit_time_version(V2_COMMITTED), out.path(), &mut installed);
    assert_eq!(installed_tag(out.path()), "v2");

    // The identical re-run. The workflow no longer reads a clock: it reads v1's commit, so it
    // reproduces v1's own number no matter when the re-run happens.
    let r3 = publish_then_apply("v1", commit_time_version(V1_COMMITTED), out.path(), &mut installed);
    assert!(r3.index_ok, "the bundle is still perfectly signed — it is refused on freshness alone");
    assert!(r3.applied.is_empty());
    assert_eq!(r3.rejected, vec![(AGENT.to_string(), ApplyOutcome::Rollback)]);
    assert_eq!(installed_tag(out.path()), "v2", "content stayed put");
    assert_eq!(installed.get(AGENT), Some(&V2_COMMITTED), "and so did the recorded version");
}

#[test]
fn a_genuinely_newer_release_is_still_accepted_under_commit_time_versions() {
    // The other direction. A fix that refuses everything is not a fix.
    let out = tempfile::tempdir().unwrap();
    let mut installed: VersionMap = BTreeMap::new();

    publish_then_apply("v1", commit_time_version(V1_COMMITTED), out.path(), &mut installed);
    publish_then_apply("v2", commit_time_version(V2_COMMITTED), out.path(), &mut installed);
    let r3 = publish_then_apply("v3", commit_time_version(V3_COMMITTED), out.path(), &mut installed);

    assert_eq!(r3.applied, vec![AGENT.to_string()]);
    assert!(r3.rejected.is_empty());
    assert_eq!(installed_tag(out.path()), "v3");
    assert_eq!(installed.get(AGENT), Some(&V3_COMMITTED));
}

#[test]
fn re_running_the_current_tag_is_already_current_not_a_rollback() {
    // Re-running the *newest* tag is a legitimate operation (a flaky upload, a re-signed asset). It
    // must be a no-op that reads as healthy, not as a regression — the `Same` / `Older` split
    // CPE-1924 added exists exactly so this case is distinguishable in the report.
    let out = tempfile::tempdir().unwrap();
    let mut installed: VersionMap = BTreeMap::new();

    publish_then_apply("v2", commit_time_version(V2_COMMITTED), out.path(), &mut installed);
    let again = publish_then_apply("v2", commit_time_version(V2_COMMITTED), out.path(), &mut installed);

    assert!(again.applied.is_empty());
    assert_eq!(again.rejected, vec![(AGENT.to_string(), ApplyOutcome::AlreadyCurrent)]);
    assert_eq!(installed_tag(out.path()), "v2");
}

// --- 3. The installed-base transition ------------------------------------------------------------

/// The real, measured high-water mark of the installed base: the `version` on every one of the 12
/// entries in the last published `catalog-index.json` (release `v0.57.32`, fetched 2026-08-27) —
/// `1784951108`, i.e. 2026-07-25T03:45:08Z. Any client that ever refreshed its catalog holds this.
///
/// This is the number a scheme change must clear. It is hard-coded on purpose: it is a historical
/// fact about what is already out there, not a value that should track anything in the repo.
const HIGHEST_INSTALLED_UNDER_THE_OLD_SCHEME: u64 = 1_784_951_108;

/// Read `CATALOG_VERSION_FLOOR` out of the shell script that is the single definition of it, so
/// this test is a ratchet on the real value rather than a copy that can drift away from it.
fn catalog_version_floor() -> u64 {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.github/workflows/scripts/catalog-version.sh");
    let text = std::fs::read_to_string(&script)
        .unwrap_or_else(|e| panic!("read {}: {e}", script.display()));
    let line = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("CATALOG_VERSION_FLOOR="))
        .expect("catalog-version.sh must define CATALOG_VERSION_FLOOR");
    line.split('#').next().unwrap().trim().parse().expect("floor must be an integer")
}

#[test]
fn the_floor_clears_the_installed_base_so_the_first_commit_derived_release_is_an_upgrade() {
    let floor = catalog_version_floor();

    // (a) The floor is above everything the old scheme can have left installed. If this ever stops
    //     holding, the release job's own floor check stops protecting anything.
    assert!(
        floor > HIGHEST_INSTALLED_UNDER_THE_OLD_SCHEME,
        "floor {floor} must exceed the highest known installed version \
         {HIGHEST_INSTALLED_UNDER_THE_OLD_SCHEME}",
    );

    // (b) The floor is in the past, so a commit made today satisfies it. A floor set into the
    //     future would fail every release instead — the opposite failure, equally fatal.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock after the epoch")
        .as_secs();
    assert!(floor <= now, "floor {floor} must not be in the future (now {now})");

    // (c) The consequence, run through the real engine: a client sitting on the last publish-time
    //     version accepts the first commit-derived release. This is the transition, executed.
    let out = tempfile::tempdir().unwrap();
    let mut installed: VersionMap = BTreeMap::new();
    installed.insert(AGENT.to_string(), HIGHEST_INSTALLED_UNDER_THE_OLD_SCHEME);

    let first_post_fix = publish_then_apply("first-post-fix", floor, out.path(), &mut installed);
    assert_eq!(
        first_post_fix.applied,
        vec![AGENT.to_string()],
        "an installed base holding timestamp versions in the billions must not be permanently \
         newer than every future commit-derived version",
    );
    assert_eq!(installed_tag(out.path()), "first-post-fix");
    assert_eq!(installed.get(AGENT), Some(&floor));
}

#[test]
fn a_repo_committed_counter_would_have_bricked_the_installed_base() {
    // The red-proof for the scheme choice, not a guard on shipped behaviour: the ticket names a
    // repo-committed counter as the other viable option. Run it through the same engine and it
    // fails exactly where a scheme change is most dangerous — a counter restarts at a small
    // integer while every installed client holds ~1.79 billion, so the first release under it is
    // refused as a rollback, and so is every release after it. Permanently.
    let out = tempfile::tempdir().unwrap();
    let mut installed: VersionMap = BTreeMap::new();
    installed.insert(AGENT.to_string(), HIGHEST_INSTALLED_UNDER_THE_OLD_SCHEME);

    for counter in [1u64, 2, 3] {
        let r = publish_then_apply("counter-scheme", counter, out.path(), &mut installed);
        assert!(r.index_ok);
        assert_eq!(
            r.rejected,
            vec![(AGENT.to_string(), ApplyOutcome::Rollback)],
            "counter {counter} would be refused by every already-installed client",
        );
        assert!(!out.path().join(format!("{AGENT}.json")).exists(), "nothing was ever installed");
    }
}

// --- 4. Why sha256 alone cannot carry freshness --------------------------------------------------

#[test]
fn the_index_sha256_identifies_content_but_cannot_order_it() {
    // The ticket asks whether the manifests' existing content identity could let freshness stop
    // resting on a published number. This test states precisely what that identity does and does
    // not give you, so the answer in the PR body is evidenced rather than asserted.
    //
    // It DOES pin content: an entry names one exact byte string and nothing else can be swapped in
    // (covered exhaustively by catalog.rs's own ContentMismatch tests).
    //
    // It does NOT order two contents. Below, an old bundle and a new bundle have plainly different
    // hashes — and that difference says only "not the same", never "older". Nothing in the engine's
    // persisted state (`VersionMap`, an id → u64 map) records which hashes it has already seen, so
    // there is no history to rank them against either. A downgrade is therefore invisible to the
    // hash and visible only to the number, which is why CPE-1941 fixes the number.
    let old = sign_bundle(&[(AGENT.to_string(), manifest_at_tag("v1"))], &hex::encode(SEED), 10)
        .expect("sign old");
    let new = sign_bundle(&[(AGENT.to_string(), manifest_at_tag("v2"))], &hex::encode(SEED), 20)
        .expect("sign new");

    let sha_of = |files: &[(String, Vec<u8>)]| -> String {
        let bytes = files
            .iter()
            .find(|(n, _)| n == "catalog-index.json")
            .map(|(_, b)| b.clone())
            .expect("index present");
        let index = CatalogIndex::from_json(&String::from_utf8(bytes).unwrap()).expect("parse");
        index.get(AGENT).expect("entry").sha256.clone()
    };

    let (old_sha, new_sha) = (sha_of(&old), sha_of(&new));
    assert_ne!(old_sha, new_sha, "different content, different identity");
    // ...and that is the whole of what it tells you. The only field carrying an ORDER is `version`.
    assert!(
        old_sha.len() == 64 && new_sha.len() == 64,
        "sha256 hex — an identity, with no more or less ordering information than any other hash",
    );
}
