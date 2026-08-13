//! Disk-backed snapshot capture & restore engine (CPE-1011, epic CPE-735 "local snapshots").
//!
//! [`crate::snapshot`] (CPE-969) is only the **pure in-memory bookkeeping half** — a refcounted
//! [`crate::snapshot::BlobStore`] plus `plan_capture`/`apply_capture`/`release` — and its own docs say
//! "the bytes behind each hash are the caller's to persist." Nothing persisted them until this module:
//! it walks a folder ([`scan_dir`]), hashes each file (reusing [`crate::checksum::hash_file`], sha256),
//! plans the capture against a disk-persisted [`crate::snapshot::BlobStore`], writes new blobs
//! content-addressed under `store_dir/blobs/<hash>` (a blob whose file already exists is a no-op — the
//! dedup win extends to disk, not just the in-memory index), and records a JSON manifest of
//! `path → {hash,size}` plus the skipped-file list. [`restore`] replays a manifest back onto a directory
//! byte-for-byte.
//!
//! On-disk layout under a store directory:
//! ```text
//! store_dir/
//!   blobs/<hash>              one file per unique content hash
//!   index.json                the persisted BlobStore (hash -> {size,refs})
//!   manifests/<manifest_id>.json   one per capture: id, time, path->hash map, skipped files
//! ```
//!
//! Std + serde only (serde_json is already a workspace dependency) — no new crates, not feature-gated,
//! always available like `snapshot`/`snapshot_retention`/`restore_plan`.
//!
//! Design notes (documented deliberate choices):
//! - **Symlinks are not followed.** [`scan_dir`] uses the same technique as [`crate::checksum`]'s
//!   `checksum_folder`: `DirEntry::metadata()` does not traverse symlinks, so a symlinked file or
//!   directory is neither `is_dir()` nor `is_file()` and is skipped outright — loop-safe by
//!   construction, with no separate cycle check needed.
//! - **Skip-on-error.** An entry the walk can't `read_dir`/`metadata`/hash is skipped, not fatal —
//!   mirrors `list_dir`'s guardrail. A file that becomes unreadable between `scan_dir` and the blob-copy
//!   step in [`capture`] *does* fail that capture (rather than silently persisting a manifest that
//!   references a blob nothing ever wrote); this only matters for a narrow race, and no writes to the
//!   index/manifest happen until every blob copy for this capture has already succeeded.
//! - **Manifest ids** are the capture's wall-clock epoch-ms, with a `-N` suffix appended on collision
//!   (two captures inside the same millisecond) so ids stay both sortable and unique.
//! - CoW/reflink/hardlink store optimisation is **out of scope for v1** — blobs are plain-copied in and
//!   out; a later ticket can swap the copy primitive without touching this module's public API.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::checksum::hash_file;
use crate::fsutil::to_epoch_ms;
use crate::restore_plan::{FileState, Snapshot};
use crate::snapshot::{apply_capture, plan_capture, release, BlobMeta, BlobStore, CaptureBudget, SkipReason, SkippedFile};

/// Walk `root` recursively into a [`Snapshot`]: every regular file, keyed by its path relative to `root`
/// with forward-slash separators (stable across OSes, sorted by [`Snapshot`]'s `BTreeMap` iteration).
/// `root` must be a directory. Unreadable directories/entries/files are skipped rather than failing the
/// whole walk (mirrors `list_dir`'s guardrail); symlinked files and directories are not followed (see the
/// module doc).
pub fn scan_dir(root: &str) -> Result<Snapshot, String> {
    let root_path = Path::new(root);
    if !root_path.is_dir() {
        return Err(format!("{root}: not a folder"));
    }
    let mut out = Snapshot::new();
    scan_walk(root_path, root_path, &mut out);
    Ok(out)
}

fn scan_walk(root: &Path, dir: &Path, out: &mut Snapshot) {
    let Ok(entries) = fs::read_dir(dir) else { return }; // unreadable dir: skip, don't fail the walk
    for entry in entries.flatten() {
        // DirEntry::metadata() does not traverse symlinks, so a symlinked file or directory is neither
        // is_dir() nor is_file() here and simply falls through unhandled below (matches checksum.rs).
        let Ok(meta) = entry.metadata() else { continue }; // unreadable entry: skip, don't fail the walk
        let path = entry.path();
        if meta.is_dir() {
            scan_walk(root, &path, out);
        } else if meta.is_file() {
            let Some(rel) = relative_slash_path(root, &path) else { continue };
            if let Ok(hash) = hash_file(&path.to_string_lossy()) {
                out.insert(rel, FileState::new(hash, meta.len()));
            }
            // else: became unreadable between listing and hashing — skip, don't fail the walk
        }
    }
}

/// `path`, relative to `root`, joined with `/` regardless of the host OS's native separator. `None` if
/// `path` isn't under `root` or is `root` itself.
fn relative_slash_path(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let parts: Vec<String> =
        rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

/// The inverse of [`relative_slash_path`]: rebuild an absolute path under `root` from a `/`-joined
/// relative path, using the host OS's native separator. `pub(crate)` so sibling command-layer modules
/// (e.g. [`crate::checkpoint_store`]'s per-file diff) can resolve the same live-file path this module's
/// own [`capture`]/[`restore`] use, without duplicating the join logic.
pub(crate) fn root_relative_to_abs(root: &Path, rel: &str) -> PathBuf {
    let mut p = root.to_path_buf();
    for part in rel.split('/') {
        p.push(part);
    }
    p
}

/// The outcome of one [`capture`]: which manifest it produced, how many blobs were newly written vs.
/// reused (the dedup win), the bytes added to the store, and which files were left out (never silently
/// dropped — surfaced so the caller can warn that a checkpoint is incomplete).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureOutcome {
    /// The id of the manifest this capture wrote — pass to [`restore`] or [`prune`].
    pub manifest_id: String,
    /// New blobs written to `store_dir/blobs/`.
    pub new_blobs: usize,
    /// Blobs already present in the store — nothing written (dedup).
    pub reused_blobs: usize,
    /// Bytes this capture added to the store's footprint.
    pub added_bytes: u64,
    /// Files whose content was skipped (oversize or over budget) and so isn't in this manifest.
    pub skipped: Vec<SkippedFile>,
}

/// Capture `root` into the content-addressed store at `store_dir` under `budget`: [`scan_dir`] → plan
/// against the store loaded from disk → write each new blob's bytes to `store_dir/blobs/<hash>` (a blob
/// already on disk is a no-op) → commit the plan to the store index → persist the index and a new
/// manifest. Returns a summary; see [`CaptureOutcome`].
///
/// Transactional-ish: blob bytes are written *before* the store index or manifest are persisted, so a
/// mid-capture I/O failure (e.g. disk full) never leaves a persisted index/manifest referencing a blob
/// that was never actually written. It may leave stray blob files on disk from a partial write — those
/// aren't referenced by any manifest, so a later [`prune`]-style GC pass (or just retrying the capture)
/// cleans them up naturally.
pub fn capture(root: &str, store_dir: &str, budget: &CaptureBudget) -> Result<CaptureOutcome, String> {
    let root_path = Path::new(root);
    let store_path = Path::new(store_dir);

    let scan = scan_dir(root)?;
    let mut store = load_store(store_path)?;
    let plan = plan_capture(&store, &scan, budget);

    // Map each new blob's hash to a source path holding that content (first one seen in the scan; any
    // other path sharing the hash is byte-identical content, so any source works).
    let mut source_for_hash: BTreeMap<&str, &str> = BTreeMap::new();
    for (path, state) in &scan {
        source_for_hash.entry(state.hash.as_str()).or_insert(path.as_str());
    }

    let blobs_dir_path = blobs_dir(store_path);
    fs::create_dir_all(&blobs_dir_path).map_err(|e| format!("{}: {e}", blobs_dir_path.display()))?;
    for blob in &plan.to_store {
        let dest = blobs_dir_path.join(&blob.hash);
        // CPE-1705: was `if dest.exists() { continue }`. The overwrite this guard's collapse permits is
        // benign — the blob store is content-addressed, so `dest`'s name IS the hash of the bytes about
        // to be written and re-copying writes identical content. It is fixed anyway, for two reasons
        // worth stating rather than shrugging at: the shape is the same fail-open-into-overwrite as the
        // dangerous sites and leaving one behind is how the next sweep mis-sorts it (this file already
        // supplied that exact lesson — `load_store` above), and the property that makes it benign is an
        // invariant of the *caller*, not of this line. `Unknown` re-copies rather than skipping: writing
        // the same bytes again is harmless and strictly safer than assuming a blob we cannot see is
        // intact, since the manifest saved below will reference it.
        if crate::fsutil::classify_target_slot(&dest.try_exists()) == crate::fsutil::TargetSlot::Occupied {
            continue; // already on disk (e.g. left over from a prior capture of the same content)
        }
        let Some(rel) = source_for_hash.get(blob.hash.as_str()) else {
            return Err(format!("internal: no source path recorded for new blob {}", blob.hash));
        };
        let src = root_relative_to_abs(root_path, rel);
        fs::copy(&src, &dest).map_err(|e| format!("{}: {e}", src.display()))?;
    }

    apply_capture(&mut store, &plan);
    save_store(store_path, &store)?;

    // A path's content is retrievable iff its hash ended up new or reused (never skipped) — this also
    // correctly excludes *every* path sharing a skipped hash, not just the one `plan.skipped` blames.
    let referenced = plan.referenced_hashes();
    let files: BTreeMap<String, PersistedFileState> = scan
        .iter()
        .filter(|(_, state)| referenced.contains(&state.hash))
        .map(|(p, state)| (p.clone(), PersistedFileState { hash: state.hash.clone(), size: state.size }))
        .collect();

    let manifest_id = fresh_manifest_id(store_path)?;
    let manifest = PersistedManifest {
        id: manifest_id.clone(),
        created_ms: to_epoch_ms(SystemTime::now()).unwrap_or(0),
        files,
        skipped: plan
            .skipped
            .iter()
            .map(|s| PersistedSkipped { path: s.path.clone(), size: s.size, reason: skip_reason_str(s.reason).to_string() })
            .collect(),
    };
    save_manifest(store_path, &manifest)?;

    Ok(CaptureOutcome {
        manifest_id,
        new_blobs: plan.to_store.len(),
        reused_blobs: plan.reused.len(),
        added_bytes: plan.added_bytes,
        skipped: plan.skipped,
    })
}

/// Recreate every file recorded in manifest `manifest_id` (from the store at `store_dir`) under `dest`,
/// byte-for-byte, creating parent directories as needed. `dest` need not exist yet. Files the original
/// capture skipped (oversize/budget) are absent from the manifest and so are not recreated.
pub fn restore(store_dir: &str, manifest_id: &str, dest: &str) -> Result<(), String> {
    let store_path = Path::new(store_dir);
    let dest_path = Path::new(dest);
    let manifest = load_manifest(store_path, manifest_id)?;
    fs::create_dir_all(dest_path).map_err(|e| format!("{}: {e}", dest_path.display()))?;
    let blobs_dir_path = blobs_dir(store_path);
    for (rel, file) in &manifest.files {
        let target = root_relative_to_abs(dest_path, rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let blob = blobs_dir_path.join(&file.hash);
        fs::copy(&blob, &target).map_err(|e| format!("{}: {e}", blob.display()))?;
    }
    Ok(())
}

/// Drop manifest `manifest_id`'s hold on its blobs via [`release`] and remove the now-unreferenced blob
/// files from disk. Returns the bytes freed. The manifest file itself is deleted; a manifest no longer on
/// disk cannot be [`restore`]d.
///
/// Ordering matters and is deliberate: the manifest file is deleted **first** — that single, atomic
/// `remove_file` is the point of no return. `release` decrements refcounts, so running it twice on the
/// same manifest would double-decrement a shared blob to 0 and delete content another snapshot still
/// needs (silent data loss). Deleting the manifest up front makes a retry-after-failure always safe: if
/// this `remove_file` fails, nothing else has changed → a clean retry; if it succeeds but a later step
/// (`load_store`/`release`/`save_store`) fails, the manifest is already gone so no second `release` can
/// happen — the residue is only a refcount/space leak, never data loss. Same "leak over corruption"
/// tradeoff [`capture`] makes.
pub fn prune(store_dir: &str, manifest_id: &str) -> Result<u64, String> {
    let store_path = Path::new(store_dir);
    let manifest = load_manifest(store_path, manifest_id)?; // read what we need before anything mutates
    let hashes: BTreeSet<String> = manifest.files.values().map(|f| f.hash.clone()).collect();

    let mpath = manifest_path(store_path, manifest_id);
    fs::remove_file(&mpath).map_err(|e| format!("{}: {e}", mpath.display()))?; // point of no return

    let mut store = load_store(store_path)?;
    let freed = release(&mut store, &hashes);
    let blobs_dir_path = blobs_dir(store_path);
    for hash in &hashes {
        if !store.contains(hash) {
            let _ = fs::remove_file(blobs_dir_path.join(hash)); // best-effort; index is the source of truth
        }
    }
    save_store(store_path, &store)?;
    Ok(freed)
}

/// Load the checkpoint [`Snapshot`] (`path → FileState`) recorded in manifest `manifest_id` from the
/// store at `store_dir`. The read-back view of [`capture`]: it reconstructs the "checkpoint" side of the
/// two-map input [`crate::restore_plan::plan_restore`] needs, so a command layer can diff a captured
/// checkpoint against a fresh [`scan_dir`] of the live tree without re-reading any blob bytes. Files the
/// original capture skipped (oversize/budget) are absent — they were never in the manifest.
pub fn manifest_snapshot(store_dir: &str, manifest_id: &str) -> Result<Snapshot, String> {
    let store_path = Path::new(store_dir);
    let manifest = load_manifest(store_path, manifest_id)?;
    Ok(manifest
        .files
        .into_iter()
        .map(|(path, file)| (path, FileState::new(file.hash, file.size)))
        .collect())
}

/// A manifest's identifying info without loading its full path→hash map — enough for a retention/listing
/// pass (CPE-1196) to decide keep/prune without paying for every file entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestSummary {
    pub id: String,
    /// When the capture was taken, epoch milliseconds (same clock as [`Checkpoint::ts`] /
    /// [`PersistedManifest::created_ms`]).
    pub created_ms: u64,
}

/// Every manifest currently on disk under `store_dir`'s `manifests/` directory, unordered (callers sort as
/// needed — [`crate::snapshot_retention::thin`] sorts internally). A missing `manifests/` dir (a store that
/// has never captured) yields an empty list, not an error. A file that fails to parse as a manifest (torn
/// write, hand-edit) is skipped rather than failing the whole enumeration — mirrors this module's other
/// skip-on-error guardrails.
pub fn list_manifests(store_dir: &str) -> Result<Vec<ManifestSummary>, String> {
    let dir = manifests_dir(Path::new(store_dir));
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Ok(Vec::new()),
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(data) = fs::read_to_string(&path) else { continue };
        let Ok(m) = serde_json::from_str::<PersistedManifest>(&data) else { continue };
        out.push(ManifestSummary { id: m.id, created_ms: m.created_ms });
    }
    Ok(out)
}

/// The store's current total footprint in bytes (sum of unique blob sizes — see
/// [`crate::snapshot::BlobStore::total_bytes`]). A store that has never captured (no `index.json` yet)
/// reads as `0`, not an error.
pub fn store_total_bytes(store_dir: &str) -> Result<u64, String> {
    Ok(load_store(Path::new(store_dir))?.total_bytes())
}

// ---- on-disk layout + persistence -----------------------------------------------------------------

fn blobs_dir(store_dir: &Path) -> PathBuf {
    store_dir.join("blobs")
}

fn index_path(store_dir: &Path) -> PathBuf {
    store_dir.join("index.json")
}

fn manifests_dir(store_dir: &Path) -> PathBuf {
    store_dir.join("manifests")
}

fn manifest_path(store_dir: &Path, id: &str) -> PathBuf {
    manifests_dir(store_dir).join(format!("{id}.json"))
}

/// The persisted shape of a [`BlobStore`]'s index — hash → `{size,refs}` — since `BlobStore`'s fields
/// aren't `Serialize`. Reloaded via [`BlobStore::from_index`].
#[derive(Serialize, Deserialize)]
struct PersistedIndex {
    blobs: BTreeMap<String, PersistedBlobMeta>,
}

#[derive(Serialize, Deserialize)]
struct PersistedBlobMeta {
    size: u64,
    refs: u32,
}

/// The persisted shape of one capture's manifest.
#[derive(Serialize, Deserialize)]
struct PersistedManifest {
    id: String,
    created_ms: u64,
    /// Path (relative, `/`-joined) → content identity, for every file this capture actually stored or
    /// reused. A path whose content was skipped is absent (see [`capture`]'s filtering).
    files: BTreeMap<String, PersistedFileState>,
    skipped: Vec<PersistedSkipped>,
}

#[derive(Serialize, Deserialize)]
struct PersistedFileState {
    hash: String,
    size: u64,
}

#[derive(Serialize, Deserialize)]
struct PersistedSkipped {
    path: String,
    size: u64,
    reason: String,
}

fn skip_reason_str(reason: SkipReason) -> &'static str {
    match reason {
        SkipReason::Oversize => "oversize",
        SkipReason::Budget => "budget",
    }
}

/// What [`load_store`] found at `store_dir/index.json` (CPE-1705).
#[derive(Debug, PartialEq, Eq)]
enum StoreIndexState {
    /// Provably nothing there — a genuine first capture. An empty store is the right answer.
    Fresh,
    /// A regular file — read it.
    Present,
    /// Anything else. **Never silently `Fresh`**, which is the bug.
    Refuse(String),
}

/// The pure decision behind [`load_store`], split out (mirroring [`crate::dispatch`]'s
/// `classify_path_error` and [`crate::split_join`]'s `part_stat_error`) so the taxonomy is unit-testable
/// on every OS and CI account.
///
/// `stat` is `Ok(is_file)` when the stat succeeded, or `Err(kind)` when it failed.
///
/// **Splitting this out is load-bearing here, not stylistic — and the reason is worth recording so the
/// next person does not go looking for the test that "should" exist.** Unlike the rename sites in this
/// ticket, the end-to-end damage at this site **cannot be staged from file permissions on any platform**,
/// measured rather than assumed:
///
/// - The damage needs the *stat* to fail while the later `save_store` *write* succeeds.
/// - **Unix:** making `metadata(store_dir/index.json)` fail with `EACCES` requires denying `+x` on
///   `store_dir` — and that same denial refuses `fs::write` to a path inside it. Both halves die together.
/// - **Windows:** no deny ACE refuses `fs::metadata` at all (it opens with a desired-access mask of `0`),
///   so the stat cannot be made to fail in the first place.
///
/// A transient network stat failure on a store directory — the QNAP case that makes this real — is
/// precisely the condition neither OS's permission model will simulate. So the classifier carries the
/// evidence for the failure branch, and the end-to-end test drives the one bad state a real filesystem
/// *can* hold: a non-file at `index.json`.
fn classify_store_index(stat: Result<bool, std::io::ErrorKind>, path: &Path) -> StoreIndexState {
    match stat {
        Ok(true) => StoreIndexState::Present,
        Ok(false) => StoreIndexState::Refuse(format!(
            "{}: the snapshot store index is not a regular file. Refusing to treat this as a fresh \
             store, which would overwrite it and orphan every existing snapshot's blobs",
            path.display()
        )),
        // The ONLY answer that means "no store here yet".
        Err(std::io::ErrorKind::NotFound) => StoreIndexState::Fresh,
        Err(kind) => StoreIndexState::Refuse(format!(
            "{}: could not read the snapshot store index ({kind:?}), so this capture was abandoned. \
             Refusing to continue as if this were the first capture — doing so would rewrite the index \
             with only this capture's blobs and permanently orphan every existing snapshot's",
            path.display()
        )),
    }
}

/// Load the store index from `store_dir/index.json`. A store directory that doesn't exist yet (first
/// capture) yields an empty store, not an error.
///
/// # CPE-1705 — the most destructive site in the whole stat-collapse chain
///
/// This was `if !path.is_file() { return Ok(BlobStore::new()) }`. `Path::is_file()` is
/// `metadata().map(|m| m.is_file()).unwrap_or(false)`, so **a stat that merely failed read as "first
/// capture ever"** and this returned an *empty* store. That is not a wrong error message and not even a
/// single overwrite:
///
/// 1. [`capture`] loads the empty store,
/// 2. applies this capture's plan to it, and
/// 3. `save_store`s it back over `index.json` —
///
/// so the real index, holding every other snapshot's blob refcounts, is replaced by one listing only this
/// capture's blobs. The next `delete_snapshot`/GC then frees blobs that older snapshots still reference,
/// and **those snapshots become permanently unrestorable.** One transient stat, cross-snapshot data loss,
/// no error anywhere. A network-backed store directory is a plausible trigger and an explicitly supported
/// one — the QNAP target.
///
/// The distinction that had to be restored is precisely *absent* vs *unreadable*: the first is a genuine
/// first capture and an empty store is right; the second must **fail**, because there is no safe thing to
/// do with a ledger you cannot read except stop. This is also the site that motivated the ticket's triage
/// rule — *a type-check whose false branch discards state is an absence claim, not a type claim* — which
/// is why it survived an "exhaustive" sweep that had correctly *enumerated* it and then filed it under
/// harmless type checks. Note the type check is kept and still enforced separately: a directory (or a
/// device node) at `index.json` is a corrupt store, not a fresh one.
///
/// See [`classify_store_index`] for the decision itself, split out so it is testable without a
/// filesystem that can be made to fail on demand — which, for *this* site, matters more than usual: see
/// that function's note on why the end-to-end damage is not constructible from permissions.
fn load_store(store_dir: &Path) -> Result<BlobStore, String> {
    let path = index_path(store_dir);
    let stat = fs::metadata(&path).map(|m| m.is_file()).map_err(|e| e.kind());
    match classify_store_index(stat, &path) {
        StoreIndexState::Fresh => return Ok(BlobStore::new()),
        StoreIndexState::Refuse(e) => return Err(e),
        StoreIndexState::Present => {}
    }
    let data = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let persisted: PersistedIndex =
        serde_json::from_str(&data).map_err(|e| format!("{}: {e}", path.display()))?;
    let blobs = persisted.blobs.into_iter().map(|(h, m)| (h, BlobMeta { size: m.size, refs: m.refs })).collect();
    Ok(BlobStore::from_index(blobs))
}

fn save_store(store_dir: &Path, store: &BlobStore) -> Result<(), String> {
    fs::create_dir_all(store_dir).map_err(|e| format!("{}: {e}", store_dir.display()))?;
    let blobs: BTreeMap<String, PersistedBlobMeta> =
        store.iter().map(|(h, m)| (h.clone(), PersistedBlobMeta { size: m.size, refs: m.refs })).collect();
    let json = serde_json::to_string_pretty(&PersistedIndex { blobs }).map_err(|e| e.to_string())?;
    let path = index_path(store_dir);
    fs::write(&path, json).map_err(|e| format!("{}: {e}", path.display()))
}

fn save_manifest(store_dir: &Path, manifest: &PersistedManifest) -> Result<(), String> {
    let dir = manifests_dir(store_dir);
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let json = serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    let path = manifest_path(store_dir, &manifest.id);
    fs::write(&path, json).map_err(|e| format!("{}: {e}", path.display()))
}

fn load_manifest(store_dir: &Path, manifest_id: &str) -> Result<PersistedManifest, String> {
    validate_manifest_id(manifest_id)?;
    let path = manifest_path(store_dir, manifest_id);
    let data = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&data).map_err(|e| format!("{}: {e}", path.display()))
}

/// Reject a caller-supplied `manifest_id` that isn't a single safe path segment, so
/// [`manifest_path`] can never resolve outside `store_dir/manifests/`.
///
/// `load_manifest` is the single chokepoint every caller-supplied manifest id funnels through —
/// [`restore`], [`prune`], and [`manifest_snapshot`] (in turn used by `checkpoint_store`'s
/// preview/revert/revert_one) — so validating here covers every read-path entry point in one
/// place. Read-only defense-in-depth: the write/delete side of a revert is already independently
/// guarded by [`crate::revert_engine`]'s `safe_segments`, so a crafted id here cannot corrupt or
/// delete anything — at worst it would have let the read resolve to an arbitrary `.json` file
/// outside the store, which this closes off. Mirrors `safe_segments`'s checks (empty, `.`/`..`,
/// separators, drive-letter colon) plus a NUL guard, since a manifest id is a single segment, not
/// a `/`-joined relative path.
fn validate_manifest_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id == "."
        || id == ".."
        || id.contains('/')
        || id.contains('\\')
        || id.contains("..")
        || id.contains(':')
        || id.contains('\0')
    {
        return Err(format!("{id}: not a valid manifest id"));
    }
    Ok(())
}

/// How many candidate manifest ids in a row may be unreadable before [`fresh_manifest_id`] gives up
/// (CPE-1705) — see `MAX_CONSECUTIVE_UNKNOWN_OUTPUT_SLOTS` in [`crate::batch_media`] for why treating
/// unknown-as-occupied without a bound converts a silent overwrite into a hang.
const MAX_CONSECUTIVE_UNKNOWN_IDS: usize = 8;

/// A fresh, unique manifest id for a capture happening now: the wall-clock epoch-ms, with a `-N` suffix
/// appended if a manifest with that id already exists (two captures inside the same millisecond) — keeps
/// ids both roughly time-sortable and guaranteed unique.
fn fresh_manifest_id(store_dir: &Path) -> Result<String, String> {
    let dir = manifests_dir(store_dir);
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let ms = to_epoch_ms(SystemTime::now()).unwrap_or(0);
    let mut candidate = ms.to_string();
    let mut n = 0u32;
    // CPE-1705: was `while manifest_path(..).exists()`, the exact `unique_target` shape. An unreadable
    // manifests directory answered `false` on the first probe, so this handed back an id whose file
    // `save_manifest` then `fs::write`s — truncating a real manifest and losing a whole snapshot's file
    // list. Unknown is skipped like occupied, bounded (see `MAX_CONSECUTIVE_UNKNOWN_IDS`) so an
    // unreadable directory refuses instead of spinning: unlike a copy target there is no fallback name
    // worth guessing at, and a capture that cannot allocate an id has written nothing yet.
    let mut unknown_run = 0usize;
    loop {
        let p = manifest_path(store_dir, &candidate);
        let stat = p.try_exists();
        match crate::fsutil::classify_target_slot(&stat) {
            crate::fsutil::TargetSlot::Free => break,
            crate::fsutil::TargetSlot::Occupied => unknown_run = 0,
            crate::fsutil::TargetSlot::Unknown => {
                unknown_run += 1;
                if unknown_run >= MAX_CONSECUTIVE_UNKNOWN_IDS {
                    return Err(format!(
                        "{}: could not check what is at a snapshot manifest id is free \
                         ({MAX_CONSECUTIVE_UNKNOWN_IDS} candidates in a row were unreadable), so nothing \
                         was captured — refusing to guess an id rather than risk overwriting an existing \
                         snapshot's manifest",
                        dir.display()
                    ));
                }
            }
        }
        n += 1;
        candidate = format!("{ms}-{n}");
    }
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-snapcap-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Create a symlink at `link` pointing at `target`; returns false when creation isn't permitted (e.g.
    /// unprivileged Windows), so the test can skip gracefully — same pattern as `disk_usage`'s tests.
    fn symlink_dir(target: &Path, link: &Path) -> bool {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link).is_ok()
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(target, link).is_ok()
        }
    }

    fn symlink_file(target: &Path, link: &Path) -> bool {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link).is_ok()
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(target, link).is_ok()
        }
    }

    // ---- scan_dir ------------------------------------------------------------------------------

    #[test]
    fn scan_dir_rejects_a_non_folder_or_missing_root() {
        let d = scratch("scan-bad-root");
        assert!(scan_dir(&d.join("nope").to_string_lossy()).is_err());
        let file = d.join("f.txt");
        fs::write(&file, b"x").unwrap();
        assert!(scan_dir(&file.to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn scan_dir_keys_by_forward_slash_relative_path() {
        let d = scratch("scan-rel");
        fs::create_dir_all(d.join("nested/deeper")).unwrap();
        fs::write(d.join("a.txt"), b"top").unwrap();
        fs::write(d.join("nested/deeper/b.txt"), b"deep").unwrap();
        let scan = scan_dir(&d.to_string_lossy()).unwrap();
        assert_eq!(scan.len(), 2);
        assert!(scan.contains_key("a.txt"));
        assert!(scan.contains_key("nested/deeper/b.txt"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn scan_does_not_follow_symlinked_directories() {
        let d = scratch("scan-symloop");
        fs::write(d.join("real.txt"), b"data").unwrap();
        fs::create_dir_all(d.join("sub")).unwrap();
        if !symlink_dir(&d, &d.join("sub").join("loop")) {
            let _ = fs::remove_dir_all(&d);
            return; // unprivileged Windows: symlink creation gated — skip
        }
        // Without the no-follow guard, `sub/loop` -> d would send the walk back into d forever (a hang)
        // and, if it terminated, would double-count real.txt under sub/loop/real.txt.
        let scan = scan_dir(&d.to_string_lossy()).unwrap();
        assert_eq!(scan.len(), 1, "the symlinked loop back to the root is never descended");
        assert!(scan.contains_key("real.txt"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn scan_skips_dangling_symlinks_without_failing_the_walk() {
        let d = scratch("scan-dangling");
        fs::write(d.join("real.txt"), b"kept").unwrap();
        if !symlink_file(&d.join("does-not-exist"), &d.join("broken-link")) {
            let _ = fs::remove_dir_all(&d);
            return; // unprivileged Windows: symlink creation gated — skip
        }
        let scan = scan_dir(&d.to_string_lossy()).unwrap();
        assert_eq!(scan.len(), 1, "the dangling symlink is skipped, not fatal to the walk");
        assert!(scan.contains_key("real.txt"));
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn scan_skips_unreadable_files_without_failing_the_whole_scan() {
        use std::os::unix::fs::PermissionsExt;
        let d = scratch("scan-unreadable");
        fs::write(d.join("ok.txt"), b"visible").unwrap();
        let blocked = d.join("blocked.txt");
        fs::write(&blocked, b"secret").unwrap();
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000)).unwrap();

        let scan = scan_dir(&d.to_string_lossy()).unwrap();

        // Restore perms before asserting/cleanup so a failing assert never leaves an undeletable file.
        let _ = fs::set_permissions(&blocked, fs::Permissions::from_mode(0o644));

        assert!(scan.contains_key("ok.txt"));
        assert!(!scan.contains_key("blocked.txt"), "unreadable file is skipped, not fatal to the scan");
        let _ = fs::remove_dir_all(&d);
    }

    // ---- CPE-1705: a stat failure on the store index is NOT "first capture ever" -----------------

    /// The taxonomy, on every OS and account. The `Err(_)` row is the whole ticket: `is_file()` folded it
    /// into `false`, `false` meant "fresh store", and a fresh store gets **written back over the real
    /// one**, orphaning every other snapshot's blobs.
    #[test]
    fn cpe_1705_only_a_genuine_absence_means_a_fresh_snapshot_store() {
        let p = Path::new("/store/index.json");
        assert_eq!(classify_store_index(Ok(true), p), StoreIndexState::Present, "a real file is loaded");
        assert_eq!(
            classify_store_index(Err(std::io::ErrorKind::NotFound), p),
            StoreIndexState::Fresh,
            "an absent index — and ONLY an absent index — is a genuine first capture"
        );
        // A non-file at index.json is a corrupt store, not a fresh one: the type check is kept, and its
        // false branch no longer discards the ledger.
        assert!(
            matches!(classify_store_index(Ok(false), p), StoreIndexState::Refuse(_)),
            "a directory at index.json must refuse, not read as a fresh store"
        );
        for kind in [
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::TimedOut,
            std::io::ErrorKind::Other,
        ] {
            match classify_store_index(Err(kind), p) {
                StoreIndexState::Refuse(msg) => {
                    assert!(
                        msg.contains("abandoned") && msg.contains("orphan"),
                        "the refusal must say what was at stake: {msg}"
                    );
                }
                other => panic!(
                    "{kind:?} must REFUSE, not {other:?} — reading it as a fresh store is what rewrites \
                     the index with only this capture's blobs"
                ),
            }
        }
    }

    /// The one bad state a real filesystem can actually hold at `index.json`, driven through the real
    /// `capture()` entry point on **every** OS: a directory sitting where the index should be.
    ///
    /// **Asserts on WHICH error, not merely that one occurred.** Pre-CPE-1705 this also failed — but from
    /// `save_store`'s `fs::write`, *after* `load_store` had already decided the store was fresh and
    /// `apply_capture` had built a replacement index from nothing. A bare `expect_err` would have passed
    /// against the bug (Evidence Rules, `Ticketing/wiki.md`, and the exact vacuous shape the CPE-1705
    /// ticket warns about). The distinguishing string is `load_store`'s own refusal.
    #[test]
    fn cpe_1705_capture_refuses_a_store_index_that_is_not_a_regular_file() {
        let src = scratch("cpe1705-src");
        let store = scratch("cpe1705-store");
        fs::write(src.join("a.txt"), b"payload").unwrap();
        // A directory where index.json belongs. `metadata()` succeeds, `is_file()` is false — the
        // "type-check whose false branch discards state" the ticket's triage rule is named after.
        fs::create_dir_all(index_path(&store)).unwrap();

        let err = capture(
            &src.to_string_lossy(),
            &store.to_string_lossy(),
            &CaptureBudget::default(),
        )
        .expect_err("a store index that is not a regular file must abort the capture");
        assert!(
            err.contains("Refusing to treat this as a fresh store"),
            "the refusal must come from load_store's guard, not incidentally from the later write — \
             those are the same red for opposite reasons: {err}"
        );

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    /// The honest case, ungated: a store directory that genuinely does not exist yet still captures
    /// cleanly. A guard that refused everything would make every first capture fail, and that is as
    /// broken as the overwrite it replaced.
    #[test]
    fn cpe_1705_a_genuinely_absent_store_index_still_captures() {
        let src = scratch("cpe1705-fresh-src");
        let store = scratch("cpe1705-fresh-store");
        fs::write(src.join("a.txt"), b"payload").unwrap();
        // Nothing at all in the store dir — the real first-capture case.
        assert!(!index_path(&store).exists());

        let out = capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::default())
            .expect("a genuine first capture must still succeed");
        assert!(out.new_blobs > 0, "the first capture must actually store the file's blob");
        assert!(index_path(&store).is_file(), "…and write a real index");

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    /// `fresh_manifest_id`'s own guard, broken out from the store-index one above so neutralising either
    /// reds exactly one test. Windows-only for the real-syscall leg: `deny_stat_of` refuses `try_exists`
    /// there by denying the target itself, whereas Unix's mechanism denies the whole parent directory —
    /// which would make `create_dir_all`/`fs::write` fail first and prove nothing about this loop.
    #[test]
    fn cpe_1705_a_manifest_id_is_never_handed_out_from_an_unreadable_slot() {
        use std::io::Write;
        #[cfg(not(windows))]
        {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1705] SKIPPED the fresh_manifest_id denied-slot leg on this platform: the Unix deny \
                 mechanism is a chmod on the PARENT directory, which fails the surrounding \
                 create_dir_all/write before this loop is reached. NOTHING in this test covered the \
                 unreadable-slot route on this run; the fsutil taxonomy tests cover the classification on \
                 every OS."
            );
        }
        #[cfg(windows)]
        {
            let store = scratch("cpe1705-mid");
            let dir = manifests_dir(&store);
            fs::create_dir_all(&dir).unwrap();
            // Occupy the id this call will compute first, then make that file unreadable.
            let ms = to_epoch_ms(SystemTime::now()).unwrap_or(0);
            let first = manifest_path(&store, &ms.to_string());
            fs::write(&first, b"REAL MANIFEST").unwrap();

            struct Restore<'a>(&'a Path, &'a Path);
            impl Drop for Restore<'_> {
                fn drop(&mut self) {
                    crate::fsutil::undo_deny_stat_of(self.0, self.1);
                    let _ = fs::remove_dir_all(self.1);
                }
            }
            let _r = Restore(&first, &store);

            if !crate::fsutil::deny_stat_of(&first) {
                let _ = writeln!(
                    std::io::stderr(),
                    "[CPE-1705] SKIPPED the fresh_manifest_id denied-slot leg: could not deny stat of {} \
                     on this machine. NOTHING in this test covered the unreadable-slot route on this run.",
                    first.display()
                );
                return;
            }

            let id = fresh_manifest_id(&store).expect("an unreadable slot must be skipped, not fatal");
            assert_ne!(
                manifest_path(&store, &id),
                first,
                "an id whose manifest file could not be stat'ed must NEVER be handed back as free — \
                 save_manifest would fs::write straight over a real snapshot's file list"
            );
            crate::fsutil::undo_deny_stat_of(&first, &store);
            assert_eq!(
                fs::read(&first).unwrap(),
                b"REAL MANIFEST".to_vec(),
                "and the real manifest must be byte-for-byte intact"
            );
        }
    }

    // ---- capture / restore round trip -----------------------------------------------------------

    #[test]
    fn capture_then_restore_round_trips_a_nested_tree_byte_for_byte() {
        let src = scratch("rt-src");
        let store = scratch("rt-store");
        let dest = scratch("rt-dest");
        fs::write(src.join("a.txt"), b"hello world").unwrap();
        fs::create_dir_all(src.join("nested/deeper")).unwrap();
        fs::write(src.join("nested/b.txt"), b"nested content").unwrap();
        fs::write(src.join("nested/deeper/c.bin"), vec![7u8; 300]).unwrap();

        let outcome =
            capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED).unwrap();
        assert_eq!(outcome.new_blobs, 3);
        assert_eq!(outcome.reused_blobs, 0);
        assert!(outcome.skipped.is_empty());

        restore(&store.to_string_lossy(), &outcome.manifest_id, &dest.to_string_lossy()).unwrap();

        assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"hello world");
        assert_eq!(fs::read(dest.join("nested").join("b.txt")).unwrap(), b"nested content");
        assert_eq!(fs::read(dest.join("nested").join("deeper").join("c.bin")).unwrap(), vec![7u8; 300]);

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn restore_of_an_unknown_manifest_is_an_error() {
        let store = scratch("rt-unknown");
        let dest = scratch("rt-unknown-dest");
        assert!(restore(&store.to_string_lossy(), "does-not-exist", &dest.to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&store);
        let _ = fs::remove_dir_all(&dest);
    }

    // ---- manifest_id validation (CPE-1127) ------------------------------------------------------

    #[test]
    fn load_manifest_rejects_traversal_and_separator_ids_before_reading_outside_manifests_dir() {
        let store = scratch("validate-traversal");
        // A valid-looking manifest planted OUTSIDE manifests/ that a `..`-id would otherwise reach if
        // validation were missing: manifest_path(store, "../outside") == store/manifests/../outside.json
        // == store/outside.json.
        let outside = store.join("outside.json");
        fs::write(&outside, br#"{"id":"outside","created_ms":0,"files":{},"skipped":[]}"#).unwrap();

        for bad in ["../outside", "..\\outside", "sub/id", "sub\\id", "..", ".", "", "a:b", "a\0b"] {
            assert!(
                load_manifest(&store, bad).is_err(),
                "expected manifest id {bad:?} to be refused, not read"
            );
        }
        // The unsafe entry-point call sites reject it too (they all funnel through load_manifest).
        assert!(restore(&store.to_string_lossy(), "../outside", &scratch("validate-dest").to_string_lossy())
            .is_err());
        assert!(prune(&store.to_string_lossy(), "../outside").is_err());

        // A normal id — the shape `fresh_manifest_id` actually produces — still loads exactly as before.
        let src = scratch("validate-src");
        fs::write(src.join("f.txt"), b"hi").unwrap();
        let outcome =
            capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED).unwrap();
        assert!(load_manifest(&store, &outcome.manifest_id).is_ok());

        let _ = fs::remove_dir_all(&store);
        let _ = fs::remove_dir_all(&src);
    }

    // ---- dedup -----------------------------------------------------------------------------------

    #[test]
    fn capture_dedups_identical_content_and_a_recapture_writes_nothing_new() {
        let src = scratch("dedup-src");
        let store = scratch("dedup-store");
        fs::write(src.join("a.txt"), b"same bytes").unwrap();
        fs::write(src.join("b.txt"), b"same bytes").unwrap();
        fs::write(src.join("c.txt"), b"different").unwrap();

        let first =
            capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED).unwrap();
        assert_eq!(first.new_blobs, 2, "a.txt+b.txt share one blob; c.txt is a second");
        assert_eq!(first.reused_blobs, 0);
        let footprint_after_first = load_store(&store).unwrap().total_bytes();
        let blob_count_after_first = load_store(&store).unwrap().blob_count();
        assert_eq!(blob_count_after_first, 2);

        // Re-capture the *unchanged* tree: everything reused, nothing new written, footprint unchanged.
        let second =
            capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED).unwrap();
        assert_eq!(second.new_blobs, 0);
        assert_eq!(second.reused_blobs, 2);
        assert_eq!(load_store(&store).unwrap().total_bytes(), footprint_after_first);
        assert_eq!(load_store(&store).unwrap().blob_count(), blob_count_after_first);
        assert_ne!(first.manifest_id, second.manifest_id, "each capture gets its own manifest id");

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    // ---- budget / skip -----------------------------------------------------------------------------

    #[test]
    fn oversize_file_is_skipped_but_the_rest_of_the_capture_succeeds() {
        let src = scratch("budget-src");
        let store = scratch("budget-store");
        fs::write(src.join("small.txt"), b"tiny").unwrap();
        fs::write(src.join("huge.bin"), vec![9u8; 5_000]).unwrap();

        let budget = CaptureBudget { max_blob_bytes: 1_000, max_total_bytes: 0 };
        let outcome = capture(&src.to_string_lossy(), &store.to_string_lossy(), &budget).unwrap();

        assert_eq!(outcome.new_blobs, 1, "only small.txt's content is stored");
        assert_eq!(outcome.skipped.len(), 1);
        assert_eq!(outcome.skipped[0].path, "huge.bin");
        assert_eq!(outcome.skipped[0].reason, SkipReason::Oversize);

        // Restoring still recreates whatever WAS captured; the skipped file is simply absent.
        let dest = scratch("budget-dest");
        restore(&store.to_string_lossy(), &outcome.manifest_id, &dest.to_string_lossy()).unwrap();
        assert_eq!(fs::read(dest.join("small.txt")).unwrap(), b"tiny");
        assert!(!dest.join("huge.bin").exists(), "skipped file has no stored content to restore");

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
        let _ = fs::remove_dir_all(&dest);
    }

    // ---- prune -------------------------------------------------------------------------------------

    #[test]
    fn prune_gcs_blobs_no_longer_referenced_and_keeps_shared_ones() {
        let src = scratch("prune-src");
        let store = scratch("prune-store");
        fs::write(src.join("shared.txt"), b"shared").unwrap();
        fs::write(src.join("only-in-first.txt"), b"first only").unwrap();

        let first =
            capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED).unwrap();

        fs::remove_file(src.join("only-in-first.txt")).unwrap();
        let second =
            capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED).unwrap();
        assert_eq!(second.new_blobs, 0, "shared.txt's content is reused, nothing new");

        let blobs_before = load_store(&store).unwrap().blob_count();
        let freed = prune(&store.to_string_lossy(), &first.manifest_id).unwrap();
        assert!(freed > 0, "only-in-first.txt's blob, held only by the pruned manifest, is freed");
        let blobs_after = load_store(&store).unwrap().blob_count();
        assert_eq!(blobs_after, blobs_before - 1, "only the unique blob is GC'd; shared.txt's survives");

        // The pruned manifest is gone; the surviving one still restores fine.
        assert!(restore(&store.to_string_lossy(), &first.manifest_id, &scratch("prune-gone").to_string_lossy())
            .is_err());
        let dest = scratch("prune-dest");
        restore(&store.to_string_lossy(), &second.manifest_id, &dest.to_string_lossy()).unwrap();
        assert_eq!(fs::read(dest.join("shared.txt")).unwrap(), b"shared");

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
        let _ = fs::remove_dir_all(&dest);
    }
}
