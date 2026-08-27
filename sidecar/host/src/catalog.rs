//! Signed agent-catalog index (CPE-308 part 2, slice 1).
//!
//! Host-authoritative trust for runtime catalog updates (design decision D1). A catalog **index**
//! is a signed document that lists agent-manifest entries; the host verifies the index against a
//! trusted first-party key (reusing the CPE-295 trust engine), binds each entry to its manifest
//! **content** by SHA-256, and enforces **monotonic versions** (anti-rollback) before any manifest
//! is handed to the sidecar for loading. The sidecar's own signature check (CPE-371) then remains
//! as defence-in-depth.
//!
//! The index signature is detached — over the exact index bytes — mirroring the per-manifest `.sig`
//! convention from CPE-371, so there is no JSON-canonicalisation ambiguity.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::trust;

/// The index-schema version this build understands.
pub const CATALOG_SCHEMA_VERSION: u16 = 1;

/// One agent manifest named by the index. `sha256` binds the entry to exact manifest content;
/// `version` is a monotonic counter used for anti-rollback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    /// The manifest's own agent-schema version (CPE-278/300), carried for migration decisions.
    pub schema_version: u16,
    /// Hex SHA-256 of the manifest bytes this entry names.
    pub sha256: String,
    /// Monotonic catalog version for this entry — a fetched entry must be strictly newer than the
    /// installed one to be accepted (anti-rollback).
    pub version: u64,
}

/// A signed list of catalog entries. Verified as a whole via a detached signature over its bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CatalogIndex {
    pub schema_version: u16,
    #[serde(default)]
    pub entries: Vec<CatalogEntry>,
}

impl CatalogIndex {
    pub fn from_json(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| e.to_string())
    }
    pub fn get(&self, id: &str) -> Option<&CatalogEntry> {
        self.entries.iter().find(|e| e.id == id)
    }
    /// Whether this build understands the index schema (unknown-future is refused, as elsewhere).
    pub fn is_supported(&self) -> bool {
        self.schema_version != 0 && self.schema_version <= CATALOG_SCHEMA_VERSION
    }
}

/// How a fetched entry's `version` stands against the installed one.
///
/// This is the **one** place the "is this an upgrade?" question is answered (CPE-1924). Both the
/// trust decision ([`Self::is_upgrade`]) and the two *reporting* reasons ([`Self::refusal`]) are
/// derived from this single comparison, so they can never disagree about what is applyable: there
/// is exactly one variant that isn't refused, and it is the same variant in both methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionStanding {
    /// Strictly newer than what's installed — or nothing is installed yet (first install).
    /// **The only standing that may be applied.**
    Newer,
    /// Exactly the version already installed. Routine and healthy: you already have the latest
    /// published catalog. Still refused (there is nothing to apply), but it is not a regression.
    Same,
    /// Older than what's installed — the published index has gone *backwards*. Refused, and unlike
    /// [`Self::Same`] this genuinely means something is wrong upstream.
    Older,
}

impl VersionStanding {
    /// The anti-rollback rule itself: **only** [`Self::Newer`] may be applied.
    pub fn is_upgrade(self) -> bool {
        matches!(self, VersionStanding::Newer)
    }
    /// Why an entry with this standing was refused — `None` exactly when [`Self::is_upgrade`] is
    /// true. Splitting `==` from `<` is a *reporting* refinement only: both arms still return a
    /// refusal, so no standing that isn't `Newer` can reach `Accept`.
    /// (Invariant pinned by `refusal_and_is_upgrade_agree_on_what_is_applyable`.)
    pub fn refusal(self) -> Option<EntryVerdict> {
        match self {
            VersionStanding::Newer => None,
            VersionStanding::Same => Some(EntryVerdict::AlreadyCurrent),
            VersionStanding::Older => Some(EntryVerdict::Rollback),
        }
    }
}

impl CatalogEntry {
    /// Whether `manifest_bytes` is exactly the content this entry names (content binding).
    pub fn matches(&self, manifest_bytes: &[u8]) -> bool {
        trust::content_hash(manifest_bytes).eq_ignore_ascii_case(self.sha256.trim())
    }
    /// Where this entry's version sits relative to `installed` — the single version comparison in
    /// the trust engine. A first install (`None`) counts as [`VersionStanding::Newer`].
    pub fn version_standing(&self, installed: Option<u64>) -> VersionStanding {
        match installed {
            None => VersionStanding::Newer,
            Some(v) => match self.version.cmp(&v) {
                std::cmp::Ordering::Greater => VersionStanding::Newer,
                std::cmp::Ordering::Equal => VersionStanding::Same,
                std::cmp::Ordering::Less => VersionStanding::Older,
            },
        }
    }
    /// Anti-rollback: accept only a strictly newer version than what's installed (or a first install).
    /// Derived from [`Self::version_standing`] — never a second, independent comparison.
    pub fn is_upgrade_over(&self, installed: Option<u64>) -> bool {
        self.version_standing(installed).is_upgrade()
    }
}

/// Verify a detached signature over the index bytes against any trusted key (CPE-295 format).
/// Fail-closed: false on any malformed input or if no trusted key matches.
pub fn verify_index(index_bytes: &[u8], signature_hex: &str, trusted_keys: &[String]) -> bool {
    trusted_keys.iter().any(|pk| trust::verify_signature(index_bytes, signature_hex, pk))
}

/// The result of gating one incoming manifest against a (signature-verified) index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryVerdict {
    /// Content matches the index and it is a strict upgrade — safe to load.
    Accept,
    /// The id is not listed in the index.
    Unlisted,
    /// Listed, but the content hash does not match (tamper).
    ContentMismatch,
    /// Listed at **exactly** the installed version — you already have the latest published entry.
    /// Refused like any non-upgrade (nothing is applied), but routine and healthy rather than a
    /// regression (CPE-1924).
    AlreadyCurrent,
    /// Listed at an **older** version than what is installed — the published catalog has gone
    /// backwards (rollback attempt / regressed publish).
    Rollback,
}

/// Gate one manifest by id + content against an **already signature-verified** index and the
/// installed version. Callers MUST call [`verify_index`] first; this enforces content binding and
/// anti-rollback only.
pub fn gate_manifest(
    index: &CatalogIndex,
    id: &str,
    manifest_bytes: &[u8],
    installed_version: Option<u64>,
) -> EntryVerdict {
    gate_manifest_opt(index, id, manifest_bytes, installed_version, false)
}

/// As [`gate_manifest`], but with an explicit `allow_downgrade` override (CPE-383). When `true`, the
/// anti-rollback check is deliberately bypassed for **this** entry so a user-chosen *specific prior
/// version* can be reinstalled — a rollback. **Every other gate still applies**: the index must
/// already be signature-verified (caller's job) and the manifest content must match the index hash
/// (`ContentMismatch` on tamper). The override only relaxes the "must be strictly newer" rule; it
/// never accepts unsigned or tampered content. Intended to be enabled per-agent, never blanket.
pub fn gate_manifest_opt(
    index: &CatalogIndex,
    id: &str,
    manifest_bytes: &[u8],
    installed_version: Option<u64>,
    allow_downgrade: bool,
) -> EntryVerdict {
    let Some(entry) = index.get(id) else {
        return EntryVerdict::Unlisted;
    };
    if !entry.matches(manifest_bytes) {
        return EntryVerdict::ContentMismatch;
    }
    // Anti-rollback — ONE decision, taken from ONE comparison. `VersionStanding::refusal()` returns
    // `Some(..)` for every standing except `Newer`, so the only route past this point is a strict
    // upgrade (or the audited per-agent downgrade override below). The `==` / `<` split changes the
    // *reason* reported, never whether the entry is applied (CPE-1924).
    if !allow_downgrade {
        if let Some(refused) = entry.version_standing(installed_version).refusal() {
            return refused;
        }
    }
    EntryVerdict::Accept
}

/// The installed catalog `version` per agent id — persisted so anti-rollback survives restarts.
pub type VersionMap = BTreeMap<String, u64>;

/// Load the persisted version map (empty on any error — a missing/corrupt map just means
/// "nothing installed yet", never a failure).
pub fn load_versions(path: &Path) -> VersionMap {
    std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

/// Persist the version map.
pub fn save_versions(path: &Path, versions: &VersionMap) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string(versions).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// Why one entry in a bundle was or wasn't applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyOutcome {
    Applied,
    ContentMismatch,
    /// Refused because the entry names exactly the installed version — the routine "you already
    /// have the latest published catalog" outcome. A **rejection**, never an apply (CPE-1924).
    AlreadyCurrent,
    /// Refused because the entry names an **older** version than the installed one — the published
    /// catalog has regressed.
    Rollback,
    MissingManifest,
    MissingSignature,
    BadSignature,
    /// The user pinned this agent, so the update was skipped (CPE-378).
    Pinned,
}

/// The result of applying a catalog bundle. `index_ok == false` means the index didn't verify and
/// **nothing was touched** (last-known-good).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ApplyReport {
    pub index_ok: bool,
    pub applied: Vec<String>,
    pub rejected: Vec<(String, ApplyOutcome)>,
}

/// Apply a catalog **bundle** staged at `staging` into the sidecar catalog dir `out`, gating every
/// entry against its signed index (CPE-308 part 2). A bundle is `index.json` + `index.json.sig` +
/// per-entry `<id>.json` + `<id>.json.sig`, all signed by a trusted key.
///
/// - If the **index** doesn't verify (bad/missing signature, unsupported schema) nothing is written
///   — the previous catalog stands (**last-known-good**).
/// - Each entry needs a present, trusted-key-signed manifest whose content matches the index
///   (`gate_manifest`) and whose `version` strictly upgrades `installed` (anti-rollback); otherwise
///   it's rejected and its previously-applied copy is left untouched.
/// - Accepted manifests + their `.sig` are written to `out` and `installed` is bumped. Offline by
///   construction (reads local staging); the remote fetch that fills `staging` is a separate wrapper.
pub fn apply_bundle(
    staging: &Path,
    out: &Path,
    trusted_keys: &[String],
    installed: &mut VersionMap,
    pinned: &[String],
) -> ApplyReport {
    apply_bundle_with(staging, out, trusted_keys, installed, pinned, &[])
}

/// As [`apply_bundle`], but with an audited per-agent **downgrade override** (CPE-383): ids listed
/// in `allow_downgrade` may be applied even when the bundle's `version` is not newer than the
/// installed one — used to roll an agent back to a specific previously-published version. The
/// override is deliberately narrow: it applies **only** to the named ids and only relaxes the
/// anti-rollback rule; index-signature, per-manifest signature, and content-hash binding are all
/// still enforced, and any id **not** in `allow_downgrade` still gets full anti-rollback. On accept,
/// `installed` is set to the (older) version so subsequent normal fetches upgrade from there.
/// A pinned agent is still frozen (pin wins over a rollback request).
pub fn apply_bundle_with(
    staging: &Path,
    out: &Path,
    trusted_keys: &[String],
    installed: &mut VersionMap,
    pinned: &[String],
    allow_downgrade: &[String],
) -> ApplyReport {
    let mut report = ApplyReport::default();

    // 1. Verify the index (governs the whole set). Any failure ⇒ touch nothing.
    let Ok(index_bytes) = std::fs::read(staging.join("index.json")) else { return report };
    let Ok(index_sig) = std::fs::read_to_string(staging.join("index.json.sig")) else { return report };
    if !verify_index(&index_bytes, index_sig.trim(), trusted_keys) {
        return report;
    }
    let Ok(index_text) = String::from_utf8(index_bytes) else { return report };
    let Ok(index) = CatalogIndex::from_json(&index_text) else { return report };
    if !index.is_supported() {
        return report;
    }
    report.index_ok = true;

    // 2. Gate + apply each entry.
    for entry in &index.entries {
        // A pinned agent is intentionally frozen at its installed version (CPE-378).
        if pinned.iter().any(|p| p == &entry.id) {
            report.rejected.push((entry.id.clone(), ApplyOutcome::Pinned));
            continue;
        }
        let Ok(bytes) = std::fs::read(staging.join(format!("{}.json", entry.id))) else {
            report.rejected.push((entry.id.clone(), ApplyOutcome::MissingManifest));
            continue;
        };
        let Ok(sig) = std::fs::read_to_string(staging.join(format!("{}.json.sig", entry.id))) else {
            report.rejected.push((entry.id.clone(), ApplyOutcome::MissingSignature));
            continue;
        };
        // The manifest itself must be signed by a trusted key (the sidecar re-checks on load,
        // CPE-371 — refuse early here too).
        if !trusted_keys.iter().any(|pk| trust::verify_signature(&bytes, sig.trim(), pk)) {
            report.rejected.push((entry.id.clone(), ApplyOutcome::BadSignature));
            continue;
        }
        let dg = allow_downgrade.iter().any(|a| a == &entry.id);
        match gate_manifest_opt(&index, &entry.id, &bytes, installed.get(&entry.id).copied(), dg) {
            EntryVerdict::Accept => {
                if write_entry(out, &entry.id, &bytes, sig.trim()).is_ok() {
                    installed.insert(entry.id.clone(), entry.version);
                    report.applied.push(entry.id.clone());
                }
            }
            EntryVerdict::ContentMismatch => {
                report.rejected.push((entry.id.clone(), ApplyOutcome::ContentMismatch))
            }
            // Both non-upgrade verdicts land in `rejected` — the split only names the reason.
            EntryVerdict::AlreadyCurrent => {
                report.rejected.push((entry.id.clone(), ApplyOutcome::AlreadyCurrent))
            }
            EntryVerdict::Rollback => report.rejected.push((entry.id.clone(), ApplyOutcome::Rollback)),
            EntryVerdict::Unlisted => {} // impossible: we iterate the index's own entries
        }
    }
    report
}

fn write_entry(out: &Path, id: &str, bytes: &[u8], sig: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(out)?;
    std::fs::write(out.join(format!("{id}.json")), bytes)?;
    std::fs::write(out.join(format!("{id}.json.sig")), sig)?;
    Ok(())
}

/// Build + sign a catalog bundle from agent manifests (CPE-377) — the release-side counterpart to
/// [`apply_bundle`]. Given `(id, manifest_bytes)` pairs, a 32-byte ed25519 seed (hex), and a
/// monotonic `version` stamped on every entry, returns the files to publish as release assets:
/// `catalog-index.json` (+ `.sig`) and each `<id>.json` (+ `.sig`). The output verifies under
/// [`verify_index`] / [`gate_manifest`] with the seed's public key.
pub fn sign_bundle(
    manifests: &[(String, Vec<u8>)],
    signing_key_hex: &str,
    version: u64,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    use ed25519_dalek::{Signer, SigningKey};

    let seed = hex::decode(signing_key_hex.trim()).map_err(|e| format!("bad key hex: {e}"))?;
    let seed: [u8; 32] = seed.try_into().map_err(|_| "signing key must be a 32-byte seed".to_string())?;
    let key = SigningKey::from_bytes(&seed);
    let sign = |bytes: &[u8]| hex::encode(key.sign(bytes).to_bytes());

    let mut entries = Vec::with_capacity(manifests.len());
    let mut files = Vec::new();
    for (id, bytes) in manifests {
        // The manifest's declared agent-schema version (default 1 if absent).
        let schema_version = serde_json::from_slice::<serde_json::Value>(bytes)
            .ok()
            .and_then(|v| v.get("schema_version").and_then(|s| s.as_u64()))
            .unwrap_or(1) as u16;
        entries.push(CatalogEntry {
            id: id.clone(),
            schema_version,
            sha256: trust::content_hash(bytes),
            version,
        });
        files.push((format!("{id}.json"), bytes.clone()));
        files.push((format!("{id}.json.sig"), sign(bytes).into_bytes()));
    }

    let index = CatalogIndex { schema_version: CATALOG_SCHEMA_VERSION, entries };
    let index_bytes = serde_json::to_vec(&index).map_err(|e| e.to_string())?;
    files.push(("catalog-index.json.sig".into(), sign(&index_bytes).into_bytes()));
    files.push(("catalog-index.json".into(), index_bytes));
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn keypair(seed: u8) -> (SigningKey, String) {
        let k = SigningKey::from_bytes(&[seed; 32]);
        (k.clone(), hex::encode(k.verifying_key().to_bytes()))
    }
    fn sign(k: &SigningKey, msg: &[u8]) -> String {
        hex::encode(k.sign(msg).to_bytes())
    }
    fn index_json(id: &str, sha: &str, version: u64) -> String {
        format!(
            r#"{{"schema_version":1,"entries":[{{"id":"{id}","schema_version":1,"sha256":"{sha}","version":{version}}}]}}"#
        )
    }

    #[test]
    fn a_valid_index_signature_verifies_and_a_tampered_one_does_not() {
        let (k, pk) = keypair(1);
        let bytes = index_json("claude", "deadbeef", 3);
        let sig = sign(&k, bytes.as_bytes());
        assert!(verify_index(bytes.as_bytes(), &sig, std::slice::from_ref(&pk)));
        assert!(!verify_index(b"tampered", &sig, &[pk]));
        assert!(!verify_index(bytes.as_bytes(), &sig, &[keypair(9).1])); // untrusted key
    }

    #[test]
    fn gate_accepts_matching_content_that_is_an_upgrade() {
        let manifest = br#"{"schema_version":1,"id":"claude"}"#;
        let sha = trust::content_hash(manifest);
        let index = CatalogIndex::from_json(&index_json("claude", &sha, 5)).unwrap();
        assert_eq!(gate_manifest(&index, "claude", manifest, Some(4)), EntryVerdict::Accept);
        assert_eq!(gate_manifest(&index, "claude", manifest, None), EntryVerdict::Accept);
    }

    #[test]
    fn gate_rejects_unlisted_tampered_and_rollback() {
        let manifest = br#"{"schema_version":1,"id":"claude"}"#;
        let sha = trust::content_hash(manifest);
        let index = CatalogIndex::from_json(&index_json("claude", &sha, 5)).unwrap();
        // Unknown id.
        assert_eq!(gate_manifest(&index, "aider", manifest, None), EntryVerdict::Unlisted);
        // Right id, wrong content.
        assert_eq!(
            gate_manifest(&index, "claude", b"different bytes", None),
            EntryVerdict::ContentMismatch
        );
        // Neither "same as installed" nor "older than installed" is an upgrade — neither is
        // accepted. CPE-1924: they are reported as *different* refusals (see the split tests below).
        assert_eq!(gate_manifest(&index, "claude", manifest, Some(5)), EntryVerdict::AlreadyCurrent);
        assert_eq!(gate_manifest(&index, "claude", manifest, Some(6)), EntryVerdict::Rollback);
    }

    // --- "already current" vs. "the index regressed" (CPE-1924) --------------------------

    /// The two non-upgrade cases must be **distinguishable**, and neither may be accepted. Before
    /// CPE-1924 both collapsed into `Rollback`, so the console could not tell the routine, healthy
    /// "you already have the latest published catalog" (`==` — the normal outcome of every check
    /// between releases, given `release.yml`'s `VERSION=$(date +%s)` stamping) from the genuinely
    /// broken "the published index has gone backwards" (`<`).
    #[test]
    fn gate_tells_already_current_apart_from_a_regressed_index_and_accepts_neither() {
        let manifest = br#"{"schema_version":1,"id":"claude"}"#;
        let sha = trust::content_hash(manifest);
        let index = CatalogIndex::from_json(&index_json("claude", &sha, 5)).unwrap();

        let same = gate_manifest(&index, "claude", manifest, Some(5)); // published == installed
        let older = gate_manifest(&index, "claude", manifest, Some(9)); // published <  installed
        assert_eq!(same, EntryVerdict::AlreadyCurrent);
        assert_eq!(older, EntryVerdict::Rollback);
        assert_ne!(same, older, "== and < must not collapse into one verdict again");
        // Neither is an accept — the split is reporting only.
        assert_ne!(same, EntryVerdict::Accept);
        assert_ne!(older, EntryVerdict::Accept);
        // …and a genuine upgrade / first install still is.
        assert_eq!(gate_manifest(&index, "claude", manifest, Some(4)), EntryVerdict::Accept);
        assert_eq!(gate_manifest(&index, "claude", manifest, None), EntryVerdict::Accept);
    }

    /// The security-critical invariant of the split: the *reason* map and the *trust* rule are two
    /// views of one comparison and can never disagree. `refusal()` returns `None` exactly for the
    /// standings `is_upgrade()` calls applyable — i.e. only `Newer`.
    #[test]
    fn refusal_and_is_upgrade_agree_on_what_is_applyable() {
        for standing in [VersionStanding::Newer, VersionStanding::Same, VersionStanding::Older] {
            assert_eq!(
                standing.refusal().is_none(),
                standing.is_upgrade(),
                "{standing:?}: a refusal-free standing must be exactly an upgrade"
            );
        }
        // Only `Newer` is applyable; the other two both refuse (with different reasons).
        assert_eq!(VersionStanding::Newer.refusal(), None);
        assert_eq!(VersionStanding::Same.refusal(), Some(EntryVerdict::AlreadyCurrent));
        assert_eq!(VersionStanding::Older.refusal(), Some(EntryVerdict::Rollback));

        // And the standings themselves, straight off a real entry.
        let e = CatalogEntry {
            id: "claude".into(),
            schema_version: 1,
            sha256: "deadbeef".into(),
            version: 5,
        };
        assert_eq!(e.version_standing(None), VersionStanding::Newer);
        assert_eq!(e.version_standing(Some(4)), VersionStanding::Newer);
        assert_eq!(e.version_standing(Some(5)), VersionStanding::Same);
        assert_eq!(e.version_standing(Some(6)), VersionStanding::Older);
        // `is_upgrade_over` is derived from the same comparison — anti-rollback is unchanged.
        assert!(e.is_upgrade_over(None) && e.is_upgrade_over(Some(4)));
        assert!(!e.is_upgrade_over(Some(5)) && !e.is_upgrade_over(Some(6)));
    }

    #[test]
    fn unsupported_index_schema_is_flagged() {
        let idx = CatalogIndex { schema_version: 99, entries: vec![] };
        assert!(!idx.is_supported());
        assert!(CatalogIndex { schema_version: 1, entries: vec![] }.is_supported());
    }

    // --- Bundle apply (CPE-373) --------------------------------------------------------
    use std::path::Path;

    /// Stage a signed bundle: index.json (+ .sig) and each `<id>.json` (+ .sig), all signed by `k`.
    /// `entries` = (id, manifest_bytes, version). If `corrupt_content` names an id, its manifest is
    /// written as different bytes than the index hash (a tamper).
    fn stage_bundle(dir: &Path, entries: &[(&str, &[u8], u64)], k: &SigningKey) {
        let index = CatalogIndex {
            schema_version: 1,
            entries: entries
                .iter()
                .map(|(id, bytes, v)| CatalogEntry {
                    id: id.to_string(),
                    schema_version: 1,
                    sha256: trust::content_hash(bytes),
                    version: *v,
                })
                .collect(),
        };
        let index_json = serde_json::to_string(&index).unwrap();
        std::fs::write(dir.join("index.json"), &index_json).unwrap();
        std::fs::write(dir.join("index.json.sig"), sign(k, index_json.as_bytes())).unwrap();
        for (id, bytes, _) in entries {
            std::fs::write(dir.join(format!("{id}.json")), bytes).unwrap();
            std::fs::write(dir.join(format!("{id}.json.sig")), sign(k, bytes)).unwrap();
        }
    }

    #[test]
    fn apply_accepts_an_upgrade_writes_it_and_bumps_the_version() {
        let (k, pk) = keypair(1);
        let stage = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        stage_bundle(stage.path(), &[("claude", br#"{"id":"claude"}"#, 5)], &k);

        let mut installed = VersionMap::new();
        let report = apply_bundle(stage.path(), out.path(), &[pk], &mut installed, &[]);
        assert!(report.index_ok);
        assert_eq!(report.applied, vec!["claude".to_string()]);
        assert!(out.path().join("claude.json").exists());
        assert!(out.path().join("claude.json.sig").exists());
        assert_eq!(installed.get("claude"), Some(&5));
    }

    #[test]
    fn apply_rejects_rollback_and_tamper_without_touching_the_good_copy() {
        let (k, pk) = keypair(1);
        let out = tempfile::tempdir().unwrap();
        // A good copy is already installed at v5.
        std::fs::write(out.path().join("claude.json"), b"GOOD").unwrap();
        let mut installed = VersionMap::from([("claude".to_string(), 5u64)]);

        // Rollback: an OLDER version (4) than the installed 5. (The same-version case is its own
        // outcome now — see `apply_reports_already_current_and_a_regressed_publish_separately`.)
        let s1 = tempfile::tempdir().unwrap();
        stage_bundle(s1.path(), &[("claude", br#"{"id":"claude"}"#, 4)], &k);
        let r1 = apply_bundle(s1.path(), out.path(), std::slice::from_ref(&pk), &mut installed, &[]);
        assert_eq!(r1.rejected, vec![("claude".to_string(), ApplyOutcome::Rollback)]);
        assert_eq!(std::fs::read(out.path().join("claude.json")).unwrap(), b"GOOD"); // untouched

        // Tamper: index says v9 over bytesA, but ship different manifest bytes.
        let s2 = tempfile::tempdir().unwrap();
        stage_bundle(s2.path(), &[("claude", br#"{"id":"claude","v":"A"}"#, 9)], &k);
        std::fs::write(s2.path().join("claude.json"), br#"{"id":"claude","v":"EVIL"}"#).unwrap();
        std::fs::write(s2.path().join("claude.json.sig"), sign(&k, br#"{"id":"claude","v":"EVIL"}"#))
            .unwrap();
        let r2 = apply_bundle(s2.path(), out.path(), &[pk], &mut installed, &[]);
        assert_eq!(r2.rejected, vec![("claude".to_string(), ApplyOutcome::ContentMismatch)]);
        assert_eq!(std::fs::read(out.path().join("claude.json")).unwrap(), b"GOOD"); // still untouched
    }

    /// End-to-end through `apply_bundle`: a same-version publish and an older-version publish must
    /// produce **different, asserted** outcomes — and *both* must land in `rejected`, never in
    /// `applied`, with the installed copy and the version map untouched. This is the test that goes
    /// red if the reporting split is ever allowed to become a trust change (CPE-1924).
    #[test]
    fn apply_reports_already_current_and_a_regressed_publish_separately_and_applies_neither() {
        let (k, pk) = keypair(1);

        // Leg 1: the published entry is EXACTLY what's installed (v5) — routine "you're current".
        let out = tempfile::tempdir().unwrap();
        std::fs::write(out.path().join("claude.json"), b"GOOD").unwrap();
        let mut installed = VersionMap::from([("claude".to_string(), 5u64)]);
        let s1 = tempfile::tempdir().unwrap();
        stage_bundle(s1.path(), &[("claude", br#"{"id":"claude","v":"NEW"}"#, 5)], &k);
        let r1 = apply_bundle(s1.path(), out.path(), std::slice::from_ref(&pk), &mut installed, &[]);
        assert!(r1.index_ok);
        // Asserted FIRST and on its own: if the reporting split ever becomes a trust change, this
        // is the line that names the violation.
        assert!(r1.applied.is_empty(), "an already-current entry must NEVER be applied");
        assert!(!r1.applied.contains(&"claude".to_string()));
        assert_eq!(r1.rejected, vec![("claude".to_string(), ApplyOutcome::AlreadyCurrent)]);
        assert_eq!(std::fs::read(out.path().join("claude.json")).unwrap(), b"GOOD"); // untouched
        assert_eq!(installed.get("claude"), Some(&5)); // version map untouched

        // Leg 2: the published entry is OLDER (v3) than installed — the index has gone backwards.
        let s2 = tempfile::tempdir().unwrap();
        stage_bundle(s2.path(), &[("claude", br#"{"id":"claude","v":"OLD"}"#, 3)], &k);
        let r2 = apply_bundle(s2.path(), out.path(), &[pk], &mut installed, &[]);
        assert!(r2.index_ok);
        assert!(r2.applied.is_empty(), "a regressed entry must NEVER be applied");
        assert_eq!(r2.rejected, vec![("claude".to_string(), ApplyOutcome::Rollback)]);
        assert_eq!(std::fs::read(out.path().join("claude.json")).unwrap(), b"GOOD"); // untouched
        assert_eq!(installed.get("claude"), Some(&5));

        // The whole point: the two situations are reported differently.
        assert_ne!(r1.rejected, r2.rejected);
    }

    #[test]
    fn a_bad_index_signature_touches_nothing_last_known_good() {
        let (k, pk) = keypair(1);
        let stage = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        std::fs::write(out.path().join("claude.json"), b"GOOD").unwrap();
        stage_bundle(stage.path(), &[("claude", br#"{"id":"claude"}"#, 5)], &k);
        // Corrupt the index signature.
        std::fs::write(stage.path().join("index.json.sig"), sign(&k, b"not the index")).unwrap();

        let mut installed = VersionMap::new();
        let report = apply_bundle(stage.path(), out.path(), &[pk], &mut installed, &[]);
        assert!(!report.index_ok);
        assert!(report.applied.is_empty());
        assert_eq!(std::fs::read(out.path().join("claude.json")).unwrap(), b"GOOD");
        assert!(installed.is_empty());
    }

    #[test]
    fn a_missing_manifest_signature_is_rejected() {
        let (k, pk) = keypair(1);
        let stage = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        stage_bundle(stage.path(), &[("claude", br#"{"id":"claude"}"#, 1)], &k);
        std::fs::remove_file(stage.path().join("claude.json.sig")).unwrap();

        let mut installed = VersionMap::new();
        let report = apply_bundle(stage.path(), out.path(), &[pk], &mut installed, &[]);
        assert!(report.index_ok);
        assert_eq!(report.rejected, vec![("claude".to_string(), ApplyOutcome::MissingSignature)]);
        assert!(!out.path().join("claude.json").exists());
    }

    #[test]
    fn sign_bundle_output_verifies_and_applies() {
        // Sign a bundle with a seed, then confirm it verifies + applies under the seed's pubkey.
        let seed = [42u8; 32];
        let seed_hex = hex::encode(seed);
        let pk = hex::encode(SigningKey::from_bytes(&seed).verifying_key().to_bytes());
        let manifests = vec![
            ("claude".to_string(), br#"{"schema_version":1,"id":"claude"}"#.to_vec()),
            ("aider".to_string(), br#"{"schema_version":1,"id":"aider"}"#.to_vec()),
        ];
        let files = sign_bundle(&manifests, &seed_hex, 7).unwrap();

        // Write the produced files to a staging dir and apply them.
        let stage = tempfile::tempdir().unwrap();
        for (name, bytes) in &files {
            std::fs::write(stage.path().join(name), bytes).unwrap();
        }
        // apply_bundle expects `index.json`, not `catalog-index.json` — mirror the fetch, which
        // saves the index under that name.
        // CPE-1710: test fixture — renames files this test just wrote into its own tempdir.
        #[allow(clippy::disallowed_methods)]
        std::fs::rename(stage.path().join("catalog-index.json"), stage.path().join("index.json")).unwrap();
        #[allow(clippy::disallowed_methods)]
        std::fs::rename(
            stage.path().join("catalog-index.json.sig"),
            stage.path().join("index.json.sig"),
        )
        .unwrap();

        let out = tempfile::tempdir().unwrap();
        let mut installed = VersionMap::new();
        let report = apply_bundle(stage.path(), out.path(), &[pk], &mut installed, &[]);
        assert!(report.index_ok);
        assert_eq!(report.applied.len(), 2);
        assert_eq!(installed.get("claude"), Some(&7));
        assert!(report.rejected.is_empty());
    }

    #[test]
    fn a_pinned_agent_is_skipped_even_when_an_upgrade_is_available() {
        let (k, pk) = keypair(1);
        let stage = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        stage_bundle(stage.path(), &[("claude", br#"{"id":"claude"}"#, 9)], &k);
        let mut installed = VersionMap::new();
        let report =
            apply_bundle(stage.path(), out.path(), &[pk], &mut installed, &["claude".to_string()]);
        assert!(report.index_ok);
        assert_eq!(report.rejected, vec![("claude".to_string(), ApplyOutcome::Pinned)]);
        assert!(report.applied.is_empty());
        assert!(!out.path().join("claude.json").exists());
        assert!(!installed.contains_key("claude")); // pin froze it — nothing recorded
    }

    // --- Audited downgrade override (CPE-383) ------------------------------------------

    #[test]
    fn gate_opt_allows_a_downgrade_only_when_opted_in_and_never_relaxes_content() {
        let manifest = br#"{"schema_version":1,"id":"claude"}"#;
        let sha = trust::content_hash(manifest);
        let index = CatalogIndex::from_json(&index_json("claude", &sha, 3)).unwrap();
        // Installed v7; the bundle names v3 (older). Without opt-in → Rollback; with opt-in → Accept.
        assert_eq!(gate_manifest_opt(&index, "claude", manifest, Some(7), false), EntryVerdict::Rollback);
        assert_eq!(gate_manifest_opt(&index, "claude", manifest, Some(7), true), EntryVerdict::Accept);
        // The override never accepts tampered content, even when downgrade is allowed.
        assert_eq!(
            gate_manifest_opt(&index, "claude", b"EVIL", Some(7), true),
            EntryVerdict::ContentMismatch
        );
    }

    #[test]
    fn apply_with_downgrade_rolls_the_chosen_agent_back_and_leaves_others_on_anti_rollback() {
        let (k, pk) = keypair(1);
        let out = tempfile::tempdir().unwrap();
        // Both agents already installed at v7.
        std::fs::write(out.path().join("claude.json"), b"OLD-CLAUDE").unwrap();
        std::fs::write(out.path().join("aider.json"), b"OLD-AIDER").unwrap();
        let mut installed = VersionMap::from([("claude".to_string(), 7u64), ("aider".to_string(), 7u64)]);

        // A signed *older* (v3) bundle for both agents — a specific prior version.
        let stage = tempfile::tempdir().unwrap();
        stage_bundle(
            stage.path(),
            &[("claude", br#"{"id":"claude","v":3}"#, 3), ("aider", br#"{"id":"aider","v":3}"#, 3)],
            &k,
        );

        // Only claude is opted into the downgrade.
        let report = apply_bundle_with(
            stage.path(),
            out.path(),
            &[pk],
            &mut installed,
            &[],
            &["claude".to_string()],
        );
        assert!(report.index_ok);
        // claude rolled back and written; installed set to the older version.
        assert_eq!(report.applied, vec!["claude".to_string()]);
        assert_eq!(installed.get("claude"), Some(&3));
        assert_eq!(std::fs::read(out.path().join("claude.json")).unwrap(), br#"{"id":"claude","v":3}"#);
        // aider was NOT opted in → still anti-rollback → rejected, its good copy untouched.
        assert_eq!(report.rejected, vec![("aider".to_string(), ApplyOutcome::Rollback)]);
        assert_eq!(installed.get("aider"), Some(&7));
        assert_eq!(std::fs::read(out.path().join("aider.json")).unwrap(), b"OLD-AIDER");
    }

    #[test]
    fn a_pin_still_wins_over_a_downgrade_request() {
        let (k, pk) = keypair(1);
        let stage = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        stage_bundle(stage.path(), &[("claude", br#"{"id":"claude","v":2}"#, 2)], &k);
        let mut installed = VersionMap::from([("claude".to_string(), 7u64)]);
        // Pinned AND asked to downgrade → the pin freezes it (safety), so nothing is applied.
        let report = apply_bundle_with(
            stage.path(),
            out.path(),
            &[pk],
            &mut installed,
            &["claude".to_string()],
            &["claude".to_string()],
        );
        assert_eq!(report.rejected, vec![("claude".to_string(), ApplyOutcome::Pinned)]);
        assert!(report.applied.is_empty());
        assert_eq!(installed.get("claude"), Some(&7)); // unchanged
    }

    #[test]
    fn version_map_round_trips_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("versions.json");
        assert!(load_versions(&path).is_empty()); // missing → empty
        let map = VersionMap::from([("claude".to_string(), 7u64), ("aider".to_string(), 2)]);
        save_versions(&path, &map).unwrap();
        assert_eq!(load_versions(&path), map);
    }
}
