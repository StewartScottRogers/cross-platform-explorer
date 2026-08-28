//! CPE-1954 — `catalog-sign verify` reads its index through `VerifiedIndex`, like every other
//! path-forming index reader.
//!
//! ## What the pre-fix path actually did (measured, not inferred)
//!
//! `verify` ran **three** of the checks and skipped **two**:
//!
//! | check | pre-fix |
//! |---|---|
//! | index signature verifies under the operator-supplied key (`verify_index`) | **ran** |
//! | per-manifest signature verifies under that key (`trust::verify_signature`) | **ran** |
//! | sha256 content binding, index entry → manifest bytes (`CatalogEntry::matches`) | **ran** |
//! | index **schema** is one this build understands (`CatalogIndex::is_supported`) | *skipped* |
//! | `entry.id` is a single safe path component (`is_valid_entry_id`, CPE-1949) | *skipped* |
//!
//! There is no filename binding to skip: this subsystem's signatures are raw detached ed25519 over
//! the exact bytes (`trust::verify_signature`), with no trusted comment and no `file:<name>` field —
//! that is the *updater's* minisign scheme, a different subsystem. And there is no version floor to
//! skip either: anti-rollback lives in `apply_bundle_with`, and `verify` is a read-only diagnostic
//! that never applies anything.
//!
//! The skipped `is_valid_entry_id` is the one that mattered, because the very next line was
//! `dir.join(format!("{}.json", entry.id))`. Measured before the fix, by running the real binary
//! (`the_verified_manifests_all_came_from_inside_the_named_directory` below):
//!
//! ```text
//! $ catalog-sign verify <root>/bundle <attacker-pubkey>
//! OK: index + 1 manifest(s) verify under the key
//! $ ls <root>/bundle
//! catalog-index.json  catalog-index.json.sig      <-- no manifest here at all
//! ```
//!
//! Every manifest it reported as verified was read out of `<root>/outside/`, one level up. The
//! operator asked a question about a directory and got an answer about a different one. That is a
//! **path-formation** step, and "the operator supplied the bundle" does not cover it: they chose the
//! directory, not the filenames inside a document they are asking this tool to appraise.
//!
//! Severity stays low — the read is read-only and maintainer-run. What it buys is disclosure (an
//! error message names an absolute path outside the bundle) and, as above, a confident OK about the
//! wrong bytes.
//!
//! This input is exactly the one `sign_bundle` never sees. `sign_bundle` refuses a hostile id at
//! publish time, so nothing **this repo** publishes can carry one; `catalog-sign verify` exists to
//! appraise a bundle **someone else** built, under a key the operator names on the command line.
//!
//! ## The shape of this file
//!
//! Every test drives the real binary via `CARGO_BIN_EXE_catalog-sign` — no in-process re-creation of
//! the CLI's logic, because the defect was in the CLI's own wiring rather than in the library. The
//! hostile-id test asserts **on the filesystem** (what is and is not inside the directory named on
//! the command line), not merely on the exit code, and it is paired with a control that differs from
//! it in one respect only: the id. If the control did not verify OK, "refused" would prove nothing.

use std::path::Path;
use std::process::Command;

use ed25519_dalek::{Signer, SigningKey};

const BIN: &str = env!("CARGO_BIN_EXE_catalog-sign");

/// The key the bundle under test is signed with. This is *not* a first-party key: the whole point is
/// that the operator names a key on the command line, so a third party's bundle verifies under a
/// third party's key.
const THIRD_PARTY_SEED: [u8; 32] = [0x2c; 32];

fn key(seed: [u8; 32]) -> SigningKey {
    SigningKey::from_bytes(&seed)
}

fn pubkey(seed: [u8; 32]) -> String {
    hex::encode(key(seed).verifying_key().to_bytes())
}

fn sig(seed: [u8; 32], bytes: &[u8]) -> String {
    hex::encode(key(seed).sign(bytes).to_bytes())
}

struct Run {
    ok: bool,
    stdout: String,
    stderr: String,
}

impl Run {
    /// Everything the tool said, for assertion messages that show what actually happened.
    fn said(&self) -> String {
        format!("exit ok={} stdout={:?} stderr={:?}", self.ok, self.stdout, self.stderr)
    }
}

fn verify(dir: &Path, pubkey_hex: &str) -> Run {
    let out = Command::new(BIN)
        .arg("verify")
        .arg(dir)
        .arg(pubkey_hex)
        .output()
        .expect("run catalog-sign");
    Run {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn write(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent");
    }
    std::fs::write(path, bytes).expect("write fixture");
}

/// A hand-built index. Hand-built on purpose: `sign_bundle` refuses every id below, which is
/// precisely why the verify path is the only one that ever meets them.
fn index_json(schema_version: u16, id: &str, sha256: &str, version: u64) -> Vec<u8> {
    format!(
        r#"{{"schema_version":{schema_version},"entries":[{{"id":"{id}","schema_version":1,"sha256":"{sha256}","version":{version}}}]}}"#
    )
    .into_bytes()
}

fn sha256_of(bytes: &[u8]) -> String {
    sidecar_host::trust::content_hash(bytes)
}

/// Lay down a bundle directory holding only the index and its signature, plus a *separately placed*
/// manifest at `manifest_at`, named by `id`. Returns the bundle dir.
fn third_party_bundle(root: &Path, id: &str, manifest_at: &Path) -> std::path::PathBuf {
    let manifest = br#"{"schema_version":1,"id":"evil","run":"whatever"}"#;
    write(manifest_at, manifest);
    write(
        &manifest_at.with_extension("json.sig"),
        sig(THIRD_PARTY_SEED, manifest).as_bytes(),
    );

    let bundle = root.join("bundle");
    let index = index_json(1, id, &sha256_of(manifest), 7);
    write(&bundle.join("catalog-index.json"), &index);
    write(&bundle.join("catalog-index.json.sig"), sig(THIRD_PARTY_SEED, &index).as_bytes());
    bundle
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read bundle dir")
        .map(|e| e.expect("dir entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// **The demonstration.** A third-party bundle whose single entry id is `../outside/evil`. The
/// directory the operator names on the command line contains no manifest at all; the manifest and
/// its signature sit one level up, where only a `join` of that id can reach them.
///
/// Asserted on the filesystem: the named directory holds exactly the index and its signature, and
/// the manifest exists only outside it. So a report of "1 manifest(s) verify" about that directory
/// can only have come from reading outside it.
#[test]
fn the_verified_manifests_all_came_from_inside_the_named_directory() {
    let root = tempfile::tempdir().expect("root");
    let outside = root.path().join("outside").join("evil.json");
    let bundle = third_party_bundle(root.path(), "../outside/evil", &outside);

    // Filesystem facts, established before the tool runs.
    assert_eq!(
        entries(&bundle),
        vec!["catalog-index.json".to_string(), "catalog-index.json.sig".to_string()],
        "the bundle the operator names must hold no manifest — otherwise this proves nothing",
    );
    assert!(outside.exists(), "the manifest must exist outside the bundle for the read to reach it");

    let run = verify(&bundle, &pubkey(THIRD_PARTY_SEED));

    assert!(
        !run.stdout.contains("OK:"),
        "verify reported a bundle OK while every manifest it checked was read from OUTSIDE the \
         directory named on the command line — the `dir.join(format!(\"{{}}.json\", entry.id))` \
         traversal (CPE-1954). Got: {}",
        run.said(),
    );
    assert!(!run.ok, "a refused verify must exit non-zero. Got: {}", run.said());
    assert!(
        run.stderr.contains("entry id"),
        "the refusal must name the entry id as the reason, so an operator can tell a hostile bundle \
         from a corrupt one. Got: {}",
        run.said(),
    );
}

/// The sensitivity control for the test above: the *same* bundle, differing only in that the id is a
/// plain filename and the manifest therefore sits inside the directory. This must verify OK. Without
/// it, a `verify` that refused everything would pass the test above.
#[test]
fn the_same_bundle_with_an_ordinary_id_verifies() {
    let root = tempfile::tempdir().expect("root");
    let bundle_dir = root.path().join("bundle");
    let bundle = third_party_bundle(root.path(), "evil", &bundle_dir.join("evil.json"));

    let run = verify(&bundle, &pubkey(THIRD_PARTY_SEED));
    assert!(run.ok, "an ordinary third-party bundle must still verify. Got: {}", run.said());
    assert!(run.stdout.contains("OK: index + 1 manifest(s)"), "got: {}", run.said());
}

/// Every id `is_valid_entry_id` refuses must be refused *here* too, with the same reason. The
/// traversal spellings are the ones that formed a path pre-fix; the rest are refused because the
/// rule is the id rule, not a hand-rolled list of the escapes someone remembered (CPE-1949).
#[test]
fn every_hostile_entry_id_is_refused_by_the_same_rule() {
    // A backslash id is written with a Rust escape; on Windows it is a second separator.
    for id in ["../outside/evil", "..", ".", "sub/evil", "sub\\\\evil", "", "a:b"] {
        let root = tempfile::tempdir().expect("root");
        let bundle = third_party_bundle(root.path(), id, &root.path().join("outside/evil.json"));
        let run = verify(&bundle, &pubkey(THIRD_PARTY_SEED));
        assert!(!run.ok, "id {id:?} must be refused. Got: {}", run.said());
        assert!(
            run.stderr.contains("entry id"),
            "id {id:?} must be refused *as an entry id*. Got: {}",
            run.said(),
        );
        assert!(!run.stdout.contains("OK:"), "id {id:?} reported OK. Got: {}", run.said());
    }
    // An over-long id, built rather than typed so it tracks MAX_ENTRY_ID_LEN.
    let long = "a".repeat(sidecar_host::catalog::MAX_ENTRY_ID_LEN + 1);
    let root = tempfile::tempdir().expect("root");
    let bundle = third_party_bundle(root.path(), &long, &root.path().join("outside/evil.json"));
    let run = verify(&bundle, &pubkey(THIRD_PARTY_SEED));
    assert!(!run.ok, "an over-long id must be refused. Got: {}", run.said());
    assert!(run.stderr.contains("entry id"), "got: {}", run.said());
}

/// The reason this was deferred out of PR #1063: folding the schema check in must not turn a
/// future-schema bundle into a bare "no index". The message has to say the schema is unsupported and
/// name both numbers, or the operator of a *newer* published catalog is told their bundle is broken.
#[test]
fn a_future_schema_bundle_says_the_schema_is_unsupported() {
    let root = tempfile::tempdir().expect("root");
    let bundle_dir = root.path().join("bundle");
    let manifest = br#"{"schema_version":1,"id":"evil"}"#;
    write(&bundle_dir.join("evil.json"), manifest);
    write(&bundle_dir.join("evil.json.sig"), sig(THIRD_PARTY_SEED, manifest).as_bytes());
    let future = sidecar_host::catalog::CATALOG_SCHEMA_VERSION + 9;
    let index = index_json(future, "evil", &sha256_of(manifest), 7);
    write(&bundle_dir.join("catalog-index.json"), &index);
    write(&bundle_dir.join("catalog-index.json.sig"), sig(THIRD_PARTY_SEED, &index).as_bytes());

    let run = verify(&bundle_dir, &pubkey(THIRD_PARTY_SEED));
    assert!(!run.ok, "an unsupported schema must be refused. Got: {}", run.said());
    assert!(
        run.stderr.contains("schema"),
        "the refusal must say the SCHEMA is the problem, not report a generic bad index — that \
         regression is the only reason this fix was deferred. Got: {}",
        run.said(),
    );
    assert!(
        run.stderr.contains(&future.to_string())
            && run.stderr.contains(&sidecar_host::catalog::CATALOG_SCHEMA_VERSION.to_string()),
        "the refusal must name both the schema found and the newest one understood. Got: {}",
        run.said(),
    );
}

/// A bundle produced by the real publish path still verifies — the no-regression leg. This is the
/// input `catalog-sign verify` is pointed at in the release workflow's own round-trips.
#[test]
fn a_bundle_from_the_real_signer_still_verifies() {
    let root = tempfile::tempdir().expect("root");
    let out = root.path().join("catalog-out");
    let manifests = vec![
        ("claude".to_string(), br#"{"schema_version":1,"id":"claude"}"#.to_vec()),
        ("opencode".to_string(), br#"{"schema_version":1,"id":"opencode"}"#.to_vec()),
    ];
    let files = sidecar_host::catalog::sign_bundle(
        &manifests,
        &hex::encode(THIRD_PARTY_SEED),
        1_784_951_108,
    )
    .expect("sign_bundle");
    for (name, data) in &files {
        write(&out.join(name), data);
    }

    let run = verify(&out, &pubkey(THIRD_PARTY_SEED));
    assert!(run.ok, "the real signer's own output must verify. Got: {}", run.said());
    assert!(run.stdout.contains("OK: index + 2 manifest(s)"), "got: {}", run.said());
}

/// Fail closed on every way the check can fail to *run*, not just on a clean "no" — CLAUDE.md's rule
/// that a wrapper must distinguish "ran and found nothing" from "did not run". Each arm below is a
/// different way the index can be unusable, and every one must refuse.
#[test]
fn every_unusable_index_is_refused_rather_than_waved_through() {
    let manifest = br#"{"schema_version":1,"id":"evil"}"#;

    // A bundle that is well-formed except for the one thing each arm breaks.
    let lay = |root: &Path| -> std::path::PathBuf {
        let bundle = root.join("bundle");
        write(&bundle.join("evil.json"), manifest);
        write(&bundle.join("evil.json.sig"), sig(THIRD_PARTY_SEED, manifest).as_bytes());
        let index = index_json(1, "evil", &sha256_of(manifest), 7);
        write(&bundle.join("catalog-index.json"), &index);
        write(&bundle.join("catalog-index.json.sig"), sig(THIRD_PARTY_SEED, &index).as_bytes());
        bundle
    };

    // Sanity: unbroken, it verifies. Otherwise every arm below passes vacuously.
    let root = tempfile::tempdir().expect("root");
    let bundle = lay(root.path());
    assert!(verify(&bundle, &pubkey(THIRD_PARTY_SEED)).ok, "the unbroken control must verify");

    struct Arm {
        what: &'static str,
        break_it: fn(&Path),
        key_seed: [u8; 32],
    }
    let arms = [
        Arm {
            what: "signed by a key the operator did not name",
            break_it: |_| {},
            key_seed: [0x99; 32],
        },
        Arm {
            what: "no index signature at all (empty file)",
            break_it: |b| write(&b.join("catalog-index.json.sig"), b""),
            key_seed: THIRD_PARTY_SEED,
        },
        Arm {
            what: "a signature that is not hex",
            break_it: |b| write(&b.join("catalog-index.json.sig"), b"not a signature"),
            key_seed: THIRD_PARTY_SEED,
        },
        Arm {
            what: "a signature file that is not UTF-8",
            break_it: |b| write(&b.join("catalog-index.json.sig"), &[0xff, 0xfe, 0x00]),
            key_seed: THIRD_PARTY_SEED,
        },
        Arm {
            what: "index bytes tampered after signing",
            break_it: |b| write(&b.join("catalog-index.json"), b"{\"schema_version\":1}"),
            key_seed: THIRD_PARTY_SEED,
        },
        Arm {
            what: "a missing index",
            break_it: |b| std::fs::remove_file(b.join("catalog-index.json")).expect("rm"),
            key_seed: THIRD_PARTY_SEED,
        },
        Arm {
            what: "a missing index signature",
            break_it: |b| std::fs::remove_file(b.join("catalog-index.json.sig")).expect("rm"),
            key_seed: THIRD_PARTY_SEED,
        },
        Arm {
            what: "an operator pubkey that is not a key at all",
            break_it: |_| {},
            key_seed: [0; 32], // replaced below by a junk string
        },
    ];

    for arm in &arms {
        let root = tempfile::tempdir().expect("root");
        let bundle = lay(root.path());
        (arm.break_it)(&bundle);
        let k = if arm.what.starts_with("an operator pubkey") {
            "zzz-not-a-key".to_string()
        } else {
            pubkey(arm.key_seed)
        };
        let run = verify(&bundle, &k);
        assert!(!run.ok, "{}: must exit non-zero. Got: {}", arm.what, run.said());
        assert!(!run.stdout.contains("OK:"), "{}: reported OK. Got: {}", arm.what, run.said());
    }
}

/// Signed, trusted key, supported schema, safe id — and the index bytes are still not UTF-8. The
/// signature is over bytes, so an attacker signing with their own key can produce this; the parse
/// must refuse rather than lossily reinterpreting the bytes into something that parses.
#[test]
fn an_index_that_is_not_utf8_is_refused_rather_than_lossily_reinterpreted() {
    let root = tempfile::tempdir().expect("root");
    let bundle = root.path().join("bundle");
    let mut index = index_json(1, "evil", &sha256_of(b"x"), 7);
    // Splice an invalid byte inside the JSON. Under `from_utf8_lossy` this becomes U+FFFD and the
    // document is *still* parseable JSON; under a strict `str::from_utf8` it is refused outright.
    let at = index.len() - 2;
    index.insert(at, 0xff);
    write(&bundle.join("catalog-index.json"), &index);
    write(&bundle.join("catalog-index.json.sig"), sig(THIRD_PARTY_SEED, &index).as_bytes());

    let run = verify(&bundle, &pubkey(THIRD_PARTY_SEED));
    assert!(!run.ok, "a non-UTF-8 index must be refused. Got: {}", run.said());
    assert!(!run.stdout.contains("OK:"), "got: {}", run.said());
}
