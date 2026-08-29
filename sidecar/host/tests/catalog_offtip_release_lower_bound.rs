//! CPE-1951 — the executable reproduction of "a release cut from an **older** commit publishes a
//! fully green catalog that every client silently refuses".
//!
//! ## What is being demonstrated
//!
//! CPE-1941 replaced `VERSION=$(date +%s)` with the **committer timestamp of the tagged commit**
//! (`.github/workflows/scripts/catalog-version.sh`). That closed the old-tag republish and opened
//! this: the version now tracks **commit order, not release order**.
//!
//! Cut a release from a commit older than the last released one — a hotfix off a maintenance
//! branch, a revert branch, `git tag` on a non-tip commit — and the derived version is BELOW the one
//! already live. Nothing on the publish side objects:
//!
//!   * `CATALOG_VERSION_FLOOR` is a **static** constant (1787000000 at the time of writing) and an
//!     off-tip commit from this year clears it by a mile. `the_publish_side_is_entirely_green_for_
//!     the_off_tip_release` below reads that constant out of the real script and asserts it.
//!   * The future-date check passes (the commit is in the past, not the future).
//!   * The index signature verifies, each manifest signature verifies, every sha256 binds.
//!   * `gh release upload` succeeds.
//!
//! And then every client answers [`ApplyOutcome::Rollback`], writes nothing, and logs nothing —
//! because from a client's side a lower version is indistinguishable from a stale republish. It is
//! an **availability** failure, not a security one: nothing unsafe is accepted. It surfaces months
//! later as "why has nobody's agent catalog updated".
//!
//! ## The shape of this file
//!
//! Every test drives the **real** publish path (`catalog::sign_bundle`, what `catalog-sign` calls)
//! and the **real** apply path (`catalog::apply_bundle_at`, which owns the load / apply / save cycle
//! since CPE-1940), and asserts on the **client outcome and the bytes on disk** — never on a verdict
//! enum alone. Nothing is stubbed. It is a sibling of `catalog_republish_downgrade.rs` (CPE-1941),
//! which demonstrates the *opposite* direction (an old tag stamped with a *newer* number).
//!
//! ## The half this file deliberately does NOT cover
//!
//! The fix is a publish-time lower-bound check — `.github/workflows/scripts/catalog-lower-bound.sh`,
//! wired into `release.yml`'s `catalog` job. That is a shell script, and the Rust matrix runs on
//! three operating systems where invoking bash from a test is a portability trap rather than a
//! guard. So it is executed, against a real git fixture and with stubbed `gh`/`curl`, in
//! `src/lib/catalogPublishLowerBound.test.ts`, which also derives the fetched URL out of
//! `catalog_url()` in `src-tauri/src/lib.rs` and asserts the wiring in `release.yml`.
//!
//! The seam between the two files is the acceptance boundary, and it is **measured here rather than
//! asserted**: `the_clients_acceptance_boundary_is_strictly_greater_than_the_installed_version`
//! sweeps the engine across the boundary and shows the client accepts exactly `candidate >
//! installed`. That is the predicate the shell guard has to implement, and the shell test executes
//! the shell guard across the same boundary. Neither file claims the other is correct; each measures
//! its own side of one number.

use std::path::Path;

use sidecar_host::catalog::{
    apply_bundle_at, load_versions, save_versions, sign_bundle, ApplyOutcome, ApplyReport,
    VersionMap,
};

/// The ed25519 seed the "release pipeline" signs with. Fixed, so every bundle below is genuinely
/// signed by the key the "client" trusts — the whole point is that the refusal happens to
/// impeccably-signed content.
const SEED: [u8; 32] = [0x5a; 32];

fn trusted_key() -> String {
    hex::encode(ed25519_dalek::SigningKey::from_bytes(&SEED).verifying_key().to_bytes())
}

const AGENT: &str = "claude";

fn manifest_at_tag(tag: &str) -> Vec<u8> {
    format!(r#"{{"schema_version":1,"id":"{AGENT}","run":"{tag}"}}"#).into_bytes()
}

/// Stage a signed bundle for `tag`'s content at `version` — exactly what the release job produces —
/// and apply it as the host would, against the anti-rollback baseline persisted at `versions_path`.
fn publish_then_apply(tag: &str, version: u64, out: &Path, versions_path: &Path) -> ApplyReport {
    let stage = tempfile::tempdir().expect("staging dir");
    let files = sign_bundle(&[(AGENT.to_string(), manifest_at_tag(tag))], &hex::encode(SEED), version)
        .expect("sign_bundle");
    for (name, data) in &files {
        // The fetch saves the index as `index.json`; `sign_bundle` emits the release asset name.
        let staged = match name.as_str() {
            "catalog-index.json" => "index.json",
            "catalog-index.json.sig" => "index.json.sig",
            other => other,
        };
        std::fs::write(stage.path().join(staged), data).expect("stage file");
    }
    apply_bundle_at(stage.path(), out, &[trusted_key()], versions_path, &[], &[])
        .expect("baseline readable")
}

fn recorded_version(versions_path: &Path) -> Option<u64> {
    load_versions(versions_path).expect("version map readable").get(AGENT).copied()
}

fn fresh_client() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("client dir");
    let versions = dir.path().join("versions.json");
    (dir, versions)
}

/// What the manifest ON DISK says right now — the thing that actually decides which install/run
/// recipe the sidecar executes. Asserting on this, not only on `ApplyReport`, is what makes the
/// reproduction a red-proof.
fn installed_tag(out: &Path) -> String {
    let bytes = std::fs::read(out.join(format!("{AGENT}.json"))).expect("installed manifest");
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("manifest json");
    v.get("run").and_then(|s| s.as_str()).expect("run field").to_string()
}

/// Read `CATALOG_VERSION_FLOOR` out of the shell script that is the single definition of it, so the
/// "the publish side is green" claim below is derived from the shipped constant rather than from a
/// number copied into this file (CLAUDE.md → "Derive provenance, don't claim it"). Change the
/// script's floor and the assertions that use this move with it.
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

// Committer timestamps, in the order the commits were made. `HOTFIX` is the whole ticket: a commit
// that exists BETWEEN the two mainline releases — a maintenance branch cut from just after v1, or a
// revert branch, or `git tag` on a non-tip commit — released AFTER v2.
const V1_COMMITTED: u64 = 1_787_100_000;
const HOTFIX_OFF_OLDER_BASE_COMMITTED: u64 = 1_787_150_000;
const V2_COMMITTED: u64 = 1_787_200_000;
const V3_COMMITTED: u64 = 1_787_300_000;

// --- 1. The bug ----------------------------------------------------------------------------------

#[test]
fn a_release_cut_from_an_older_commit_is_refused_by_every_client_and_changes_nothing_on_disk() {
    let (client, versions) = fresh_client();
    let out = client.path();

    // Ordinary history: v1, then v2. Both applied.
    let r1 = publish_then_apply("v1", V1_COMMITTED, out, &versions);
    assert_eq!(r1.applied, vec![AGENT.to_string()]);
    let r2 = publish_then_apply("v2", V2_COMMITTED, out, &versions);
    assert_eq!(r2.applied, vec![AGENT.to_string()]);
    assert_eq!(installed_tag(out), "v2");
    assert_eq!(recorded_version(&versions), Some(V2_COMMITTED));

    // Now the hotfix. Its content is NEW — it is a real fix nobody has ever shipped — but it was
    // committed off a base older than v2, so CPE-1941's rule stamps a number below v2's.
    const _: () = assert!(HOTFIX_OFF_OLDER_BASE_COMMITTED < V2_COMMITTED);
    let hotfix = publish_then_apply("hotfix", HOTFIX_OFF_OLDER_BASE_COMMITTED, out, &versions);

    // THE BUG, on the client side. The bundle is impeccable — `index_ok` is the engine saying the
    // index signature verified — and the entry is still refused.
    assert!(hotfix.index_ok, "the bundle is perfectly signed; it is refused on the number alone");
    assert!(hotfix.applied.is_empty(), "nothing was applied");
    assert_eq!(hotfix.rejected, vec![(AGENT.to_string(), ApplyOutcome::Rollback)]);

    // ...and the ON-DISK state, which is what actually matters and what nobody looks at: the new
    // fix is simply not there. Users keep running v2's recipe forever.
    assert_eq!(installed_tag(out), "v2", "the hotfix's content never reached the disk");
    assert_eq!(recorded_version(&versions), Some(V2_COMMITTED), "and the baseline did not move");

    // The failure is PERMANENT, not transient: re-fetching does not eventually take. Every
    // subsequent fetch of the same published catalog replays the identical refusal, silently.
    for _ in 0..3 {
        let again = publish_then_apply("hotfix", HOTFIX_OFF_OLDER_BASE_COMMITTED, out, &versions);
        assert_eq!(again.rejected, vec![(AGENT.to_string(), ApplyOutcome::Rollback)]);
        assert_eq!(installed_tag(out), "v2");
    }
}

#[test]
fn the_publish_side_is_entirely_green_for_the_off_tip_release() {
    // The other half of "silently": there is nothing wrong with what the release job produced, so
    // no check in it fires. Stated against the SHIPPED floor rather than a copied number.
    let floor = catalog_version_floor();
    assert!(
        HOTFIX_OFF_OLDER_BASE_COMMITTED > floor,
        "the off-tip version {HOTFIX_OFF_OLDER_BASE_COMMITTED} clears CATALOG_VERSION_FLOOR \
         ({floor}) — the static ratchet cannot see this class of defect at all",
    );

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock after the epoch")
        .as_secs();
    assert!(
        HOTFIX_OFF_OLDER_BASE_COMMITTED <= now + 86_400,
        "and it clears the future-date check too, so that does not fire either",
    );

    // And the bundle itself is valid: `sign_bundle` produces a full, signed set of assets. There is
    // no step in the catalog job that this release fails.
    let files =
        sign_bundle(&[(AGENT.to_string(), manifest_at_tag("hotfix"))], &hex::encode(SEED),
            HOTFIX_OFF_OLDER_BASE_COMMITTED)
        .expect("the off-tip release signs cleanly — the job goes green");
    let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    assert!(names.contains(&"catalog-index.json"));
    assert!(names.contains(&"catalog-index.json.sig"));
    for (_, bytes) in &files {
        assert!(!bytes.is_empty(), "no empty asset — the bundle-verify step passes too");
    }
}

// --- 2. The boundary the publish-time guard has to implement --------------------------------------

#[test]
fn the_clients_acceptance_boundary_is_strictly_greater_than_the_installed_version() {
    // MEASURED, not asserted. The publish-time lower-bound guard
    // (.github/workflows/scripts/catalog-lower-bound.sh) refuses `candidate <= published`. This
    // sweeps the engine across that boundary and shows why "strictly greater" — not ">=", not "!=" —
    // is the right comparison: at equality the client answers AlreadyCurrent and writes nothing, so
    // a `>=` guard would let a release publish that reaches no user.
    let installed = V2_COMMITTED;

    for (candidate, expected) in [
        (installed - 1, Some(ApplyOutcome::Rollback)),
        (installed, Some(ApplyOutcome::AlreadyCurrent)),
        (installed + 1, None), // applied
    ] {
        let (client, versions) = fresh_client();
        let out = client.path();
        publish_then_apply("v2", installed, out, &versions);
        assert_eq!(installed_tag(out), "v2");

        let r = publish_then_apply("candidate", candidate, out, &versions);
        match expected {
            Some(outcome) => {
                assert!(r.applied.is_empty(), "candidate {candidate} must not apply");
                assert_eq!(r.rejected, vec![(AGENT.to_string(), outcome)]);
                assert_eq!(installed_tag(out), "v2", "disk unchanged for candidate {candidate}");
                assert_eq!(recorded_version(&versions), Some(installed));
            }
            None => {
                assert_eq!(r.applied, vec![AGENT.to_string()], "candidate {candidate} must apply");
                assert!(r.rejected.is_empty());
                assert_eq!(installed_tag(out), "candidate");
                assert_eq!(recorded_version(&versions), Some(candidate));
            }
        }
    }
}

// --- 3. The other direction: the fix must not refuse legitimate releases ---------------------------

#[test]
fn a_hotfix_re_cut_from_a_newer_commit_reaches_every_client() {
    // What the guard's error message tells a release engineer to do: re-cut the tag from a commit
    // newer than the one already released. Run that through the real engine, so "refuses
    // everything" cannot pass for a fix.
    let (client, versions) = fresh_client();
    let out = client.path();

    publish_then_apply("v1", V1_COMMITTED, out, &versions);
    publish_then_apply("v2", V2_COMMITTED, out, &versions);

    // Same fix, this time committed on top of v2 (a merge or cherry-pick forward, which is what
    // `%ct` tracks — committer time, refreshed by a rebase/cherry-pick, per catalog-version.sh).
    let re_cut = publish_then_apply("hotfix-re-cut", V3_COMMITTED, out, &versions);
    assert_eq!(re_cut.applied, vec![AGENT.to_string()]);
    assert!(re_cut.rejected.is_empty());
    assert_eq!(installed_tag(out), "hotfix-re-cut");
    assert_eq!(recorded_version(&versions), Some(V3_COMMITTED));
}

#[test]
fn a_client_that_never_saw_the_newer_release_still_takes_the_off_tip_one() {
    // Precision about the blast radius, so the PR body does not overstate it. The refusal is
    // per-client and depends on what that client already holds: a machine still on v1 accepts the
    // off-tip hotfix quite happily, because for IT the number went up. So the observable symptom is
    // a SPLIT installed base — some machines updated, the ones that were current did not — which is
    // even harder to notice than a uniform freeze, and is why the fix belongs at publish time
    // rather than in the client.
    let (client, versions) = fresh_client();
    let out = client.path();
    let mut seed = VersionMap::new();
    seed.insert(AGENT.to_string(), V1_COMMITTED);
    save_versions(&versions, &seed).expect("seed the baseline on disk");

    let r = publish_then_apply("hotfix", HOTFIX_OFF_OLDER_BASE_COMMITTED, out, &versions);
    assert_eq!(r.applied, vec![AGENT.to_string()]);
    assert_eq!(installed_tag(out), "hotfix");
    assert_eq!(recorded_version(&versions), Some(HOTFIX_OFF_OLDER_BASE_COMMITTED));
}
