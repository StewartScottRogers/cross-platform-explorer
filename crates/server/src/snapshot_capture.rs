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

/// The inverse of [`relative_slash_path`] **for a name this process just scanned off the disk**: rebuild
/// an absolute path under `root` from a `/`-joined relative path, using the host OS's native separator.
///
/// **CPE-1823 — this is deliberately the unvalidated join, and it has exactly one caller.** Until that
/// ticket it was also what [`restore`] used to turn a *manifest-supplied* path into a write target, which
/// was the whole bug: `Path::push` with an absolute component **replaces** the accumulated path, and a
/// `..` component walks up out of the restore root, so a hand-edited manifest chose where the bytes
/// landed. The untrusted side now goes through [`crate::revert_engine::safe_target`] instead (see
/// [`restore`]); nothing here may be reused for a caller-supplied path.
///
/// It survives for [`capture`]'s blob-copy loop, whose `rel` is not input at all: it was produced moments
/// earlier by [`relative_slash_path`] `strip_prefix`ing a real [`std::fs::DirEntry`] path under this same
/// `root`, so every segment is by construction one real, existing directory entry, and the path is used
/// to **read** a file the scan just hashed. Routing that through `safe_target` would be strictly worse
/// than useless: its (correct, for untrusted input) blanket refusal of `:` and `\` in a segment would
/// abort a whole capture on Linux or macOS because the user owns a file legitimately named
/// `2026-08-21 10:30 notes.txt` — breaking a working operation on two platforms to re-check a name the
/// filesystem itself just handed us.
fn scan_source_path(root: &Path, rel: &str) -> PathBuf {
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
        //
        // CPE-1769: `dest.try_exists()` FOLLOWS the final component, so an attacker who plants a dangling
        // link at a blob's hashed name has it read as `Free` here — `Occupied` is skipped, the `if` falls
        // through, and `fs::copy` below writes the blob's bytes straight through the link to wherever it
        // points, outside the content-addressed store. Low harm in the sense the ticket recorded (the
        // store is content-addressed, so nothing sensitive is chosen by an attacker here — only which
        // pre-existing file's bytes get clobbered by *this capture's* blob content), but the shape is
        // identical to the dangerous sites, so it gets the same shared probe: `name_pick_slot_probe`
        // folds a dangling link (or an NTFS junction) into `Occupied`, same as a real blob file already on
        // disk, so this site's existing "treat occupied as already-there, skip" policy now also covers it
        // — the copy is skipped and the link is left exactly as it was, never written through.
        if crate::fsutil::classify_target_slot(&crate::fsutil::name_pick_slot_probe(&dest))
            == crate::fsutil::TargetSlot::Occupied
        {
            continue; // already on disk (e.g. left over from a prior capture of the same content), or a
                      // link occupies the name — either way, nothing is written through it
        }
        let Some(rel) = source_for_hash.get(blob.hash.as_str()) else {
            return Err(format!("internal: no source path recorded for new blob {}", blob.hash));
        };
        let src = scan_source_path(root_path, rel);
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
///
/// # The manifest is INPUT, not a trusted record (CPE-1823)
///
/// A manifest is an ordinary JSON file in an ordinary directory — hand-editable, copyable from another
/// machine, restorable from a shared drive, syncable by a cloud client, and unsigned. Both paths it
/// carries per entry used to be joined straight onto a root:
///
/// - the **write target** (`dest` + the entry's `/`-joined path) via [`scan_source_path`]'s `push` loop,
///   where an absolute segment *replaces* the whole path and `..` walks up — arbitrary file **write**;
/// - the **read source** (`store_dir/blobs/` + the entry's `hash`), where the same two shapes in a field
///   nothing validated made it an arbitrary file **read**.
///
/// Three guards now stand between the JSON and the filesystem, all applied **before** this entry creates
/// any directory:
///
/// 1. [`crate::revert_engine::safe_target`] — the crate's existing "resolve a caller-supplied relative
///    path safely under a root" helper, already guarding [`crate::revert_engine`]'s writes against
///    manifests from this same store. Reused rather than re-implemented, so a restore and a revert of the
///    same manifest cannot disagree about which entries are legal.
/// 2. [`crate::fsutil::confined_to`] on the resolved target — `safe_target` is a *textual* check, blind
///    to a symlink or junction planted at an interior component, which needs neither `..` nor an absolute
///    segment and redirects the write just as effectively. `confined_to` canonicalises and fails closed.
/// 3. [`blob_source`] on the read side — a plain hex content hash, then the same containment check
///    against `blobs/`, so a link planted at a blob's name cannot feed some other file's bytes into the
///    restored tree.
///
/// # A rejected entry fails the restore — it is never skipped
///
/// The refusal is loud, per-entry, and names the offending path. Skipping it and returning `Ok` would
/// hand the user a restore that reports success while a file they asked for is missing, which is the
/// worse failure: a restore is *believed*, and a silently absent file is discovered later, if ever.
/// Nothing legitimate produces such an entry — [`capture`] only ever writes plain relative paths and
/// 64-hex hashes — so a manifest containing one is corrupt or planted, and neither is a thing to partly
/// apply.
///
/// Entries are applied in `BTreeMap` order, so a refusal may leave earlier entries already written. That
/// is the same partial state any mid-restore I/O error leaves, and it is reported rather than hidden.
///
/// # The one inherited over-refusal, recorded rather than forked
///
/// [`crate::revert_engine::safe_segments`] rejects `:` and `\` in a segment on **every** platform, because
/// on Windows either can re-root or re-stream a path. On Linux and macOS both are ordinary filename
/// bytes, so a file the user genuinely owns — `2026-08-21 10:30 notes.txt` — captures fine and then
/// cannot be restored on the machine it came from. That is a real gap and it is **pre-existing**:
/// `revert_engine` has always refused such an entry (as a per-file skip-with-reason) for manifests from
/// this same store. It is inherited here on purpose rather than fixed by forking a second, more lenient
/// copy of the rule: two containment predicates disagreeing about the same manifest is how the next hole
/// gets found. The fix belongs in `safe_segments`, gating the refusal on `cfg!(windows)` the way
/// [`crate::fsutil::win32_name_is_unstable`]'s callers already must, so restore and revert move together.
pub fn restore(store_dir: &str, manifest_id: &str, dest: &str) -> Result<(), String> {
    let store_path = Path::new(store_dir);
    let dest_path = Path::new(dest);
    let manifest = load_manifest(store_path, manifest_id)?;
    fs::create_dir_all(dest_path).map_err(|e| format!("{}: {e}", dest_path.display()))?;
    let blobs_dir_path = blobs_dir(store_path);
    for (rel, file) in &manifest.files {
        let target = crate::revert_engine::safe_target(dest_path, rel).map_err(|why| refusal(rel, &why))?;
        if !crate::fsutil::confined_to(&target, dest_path) {
            return Err(refusal(
                rel,
                &format!("it resolves outside the restore folder {}", dest_path.display()),
            ));
        }
        let blob = blob_source(&blobs_dir_path, &file.hash).map_err(|why| refusal(rel, &why))?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        fs::copy(&blob, &target).map_err(|e| format!("{}: {e}", blob.display()))?;
    }
    Ok(())
}

/// The per-entry refusal message (CPE-1823), in one place so every rejected shape reads the same and all
/// of them name the manifest path that was rejected. The path is what the user has to act on: it is the
/// only thing tying the refusal back to a line in the JSON, and it is also the evidence that the manifest
/// was tampered with rather than merely unlucky.
fn refusal(rel: &str, why: &str) -> String {
    format!("{rel}: refusing this manifest entry — {why}")
}

/// Resolve a manifest entry's `hash` to the blob file it names, or refuse (CPE-1823).
///
/// `hash` is a manifest field, so it is input: `blobs_dir.join(hash)` with `hash = "../../../etc/passwd"`
/// (or, on Windows, an absolute `C:\…`, which `join` lets *replace* the whole path) made [`restore`] copy
/// any file the app could read into the restored tree, under a name the same manifest chose.
///
/// Two checks, in this order:
/// - the name must be a plain hex content address — which is all [`capture`] ever writes, since every
///   hash comes from [`crate::checksum::hash_file`]'s lowercase sha256 hex. Hex alone already forbids
///   `.`, `/`, `\`, `:` and `..`, so no path shape survives it;
/// - and the join must still land inside `blobs_dir` per [`crate::fsutil::confined_to`], because the hex
///   check only constrains the *spelling*. A symlink or junction planted at `blobs/<hash>` is a legal hex
///   name whose bytes come from somewhere else entirely — and [`capture`]'s CPE-1769 blob loop
///   deliberately leaves such a slot alone (it reads as `Occupied` and is skipped, never written
///   through), so a planted link can still be sitting there at restore time.
///
/// The length bound is a sanity cap, not the security property: it keeps a 4 MB "hash" out of a path
/// buffer without hard-coding sha256's 64 characters into a format that has already been described as
/// swappable in this module's own header.
fn blob_source(blobs_dir: &Path, hash: &str) -> Result<PathBuf, String> {
    validate_blob_name(hash)?;
    let blob = blobs_dir.join(hash);
    if !crate::fsutil::confined_to(&blob, blobs_dir) {
        return Err(format!(
            "its blob {hash:?} does not resolve inside the blob store {}",
            blobs_dir.display()
        ));
    }
    Ok(blob)
}

/// The name half of [`blob_source`]'s rule, split out because [`prune`] needs exactly this and not the
/// containment half (see its call site). A blob file is named by a content address and nothing else.
fn validate_blob_name(hash: &str) -> Result<(), String> {
    if hash.is_empty() || hash.len() > MAX_BLOB_NAME_LEN || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("its content hash {hash:?} is not a plain hex blob name"));
    }
    Ok(())
}

/// Longest blob name [`validate_blob_name`] will entertain. Comfortably past sha256's 64 hex characters
/// (and sha512's 128) without pinning the digest this module happens to use today.
const MAX_BLOB_NAME_LEN: usize = 128;

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

    // CPE-1823: the loop below `remove_file`s `blobs/<hash>` for every hash the store no longer refs, so
    // the same unvalidated manifest field that gave `restore` an arbitrary read gives this an arbitrary
    // **delete**. Validated here, before the point of no return, so a planted manifest costs nothing at
    // all rather than costing the manifest file and then failing: at this line nothing has been touched,
    // so the refusal is total. Reuses `validate_blob_name`, where the "what may a hash name" rule lives.
    // The name check alone, deliberately — `blob_source`'s second, `confined_to` half answers "could this
    // read pull bytes from outside the store", and a `remove_file` on a planted link removes the link,
    // never its target, so there is nothing here for it to protect. Requiring it would also make `prune`
    // refuse on a store whose `blobs/` directory is already gone, which is a legitimate thing to prune.
    for hash in &hashes {
        validate_blob_name(hash).map_err(|why| refusal(manifest_id, &why))?;
    }

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

/// **CPE-1765 — this is a name-picking write, so it CLAIMS the name.** Its one production caller runs
/// [`fresh_manifest_id`] and then this, so the id was proved free by a probe and then handed to a plain
/// `fs::write`: the exact probe-then-write shape the copy/move sites carried. Two things could get
/// between them — a concurrent capture in another window landing on the same millisecond, and a link
/// planted at `<id>.json`, which `fs::write` follows out of the store — and both ended with a truncated
/// file and an `Ok`.
///
/// [`crate::fsutil::claim_file_slot`] makes the create its own existence check, so an id taken in that
/// gap is a refusal naming the path instead of a lost manifest. Safe here precisely because the id is
/// always fresh: this function has never had a legitimate reason to overwrite an existing manifest, and
/// now it cannot. (Its sibling `save_store` writes a single fixed `index.json` the app owns and rewrites
/// every capture — not a picked name, deliberately left alone.)
fn save_manifest(store_dir: &Path, manifest: &PersistedManifest) -> Result<(), String> {
    use std::io::Write as _;
    let dir = manifests_dir(store_dir);
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let json = serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    let path = manifest_path(store_dir, &manifest.id);
    let mut f = crate::fsutil::claim_file_slot(&path)?;
    f.write_all(json.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))
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
    pick_manifest_id(store_dir, to_epoch_ms(SystemTime::now()).unwrap_or(0))
}

/// The candidate walk behind [`fresh_manifest_id`], with the clock passed in (CPE-1705).
///
/// Split out purely so a test can **name the candidate ids in advance**. `fresh_manifest_id` reads
/// `SystemTime::now()` inside itself, so a test that pre-creates `<ms>`, `<ms>-1`, … computes a *different*
/// millisecond than the call under test does and stages files the walk never looks at — it then passes
/// trivially, having exercised nothing. (Observed: the first version of this ticket's test reded with
/// `must REFUSE, never hand back a guessed name: "1786642033625"` — the very first candidate, free,
/// because every staged file was named for an earlier millisecond.)
fn pick_manifest_id(store_dir: &Path, ms: u64) -> Result<String, String> {
    let dir = manifests_dir(store_dir);
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
            // A real collision breaks the run: `unknown_run` counts CONSECUTIVE unknowns, because a *run*
            // is the only evidence that the directory itself is unreadable. Without this reset, unknowns
            // scattered among genuine same-millisecond collisions accumulate and refuse a capture that
            // should simply have walked to the next id (PR #893 review — broken, it redded nothing).
            crate::fsutil::TargetSlot::Occupied => unknown_run = 0,
            crate::fsutil::TargetSlot::Unknown => {
                unknown_run += 1;
                if unknown_run >= MAX_CONSECUTIVE_UNKNOWN_IDS {
                    return Err(format!(
                        "{}: could not find a free snapshot manifest id \
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

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-snapcap-{tag}"))
    }

    /// **CPE-1765.** `fresh_manifest_id` proves an id free and `save_manifest` then writes to it; a second
    /// capture landing on the same millisecond, or a link planted at `<id>.json`, fits between them. The
    /// pre-fix `fs::write` truncated whatever was there and returned `Ok`, losing a whole snapshot's file
    /// list. The bytes are asserted **before** the `Result`, because that is exactly the outcome the bug
    /// produced.
    #[test]
    fn cpe_1765_save_manifest_refuses_an_id_taken_in_the_gap_instead_of_truncating_it() {
        let d = scratch("cpe1765_manifest_gap");
        let store = d.path();
        std::fs::create_dir_all(manifests_dir(store)).unwrap();
        let taken = manifest_path(store, "1786642033625");
        std::fs::write(&taken, b"SOMEONE ELSE'S MANIFEST").unwrap();

        let r = save_manifest(
            store,
            &PersistedManifest {
                id: "1786642033625".to_string(),
                created_ms: 0,
                files: Default::default(),
                skipped: Vec::new(),
            },
        );

        assert_eq!(
            std::fs::read_to_string(&taken).unwrap(),
            "SOMEONE ELSE'S MANIFEST",
            "the save truncated a manifest that took the id in the gap"
        );
        let e = r.expect_err("an id taken in the gap must not report success");
        assert!(e.contains(&taken.display().to_string()), "the refusal must name the path: {e}");
    }

    /// Parity for the same change: a fresh id still writes a manifest that round-trips. A "fix" that
    /// refused every save would pass the test above and break every capture.
    #[test]
    fn cpe_1765_save_manifest_still_writes_a_manifest_at_a_free_id() {
        let d = scratch("cpe1765_manifest_ok");
        let store = d.path();
        save_manifest(
            store,
            &PersistedManifest {
                id: "42".to_string(),
                created_ms: 7,
                files: Default::default(),
                skipped: Vec::new(),
            },
        )
        .expect("a free id must save");
        assert_eq!(load_manifest(store, "42").unwrap().created_ms, 7);
    }

    /// **CPE-1769.** `capture`'s blob-write loop used a bare `dest.try_exists()`: FOLLOWS the final
    /// component, so a dangling link planted at a blob's hashed destination name reads as `Free`, the
    /// `Occupied => continue` skip above never fires, and `fs::copy` writes the blob's bytes straight
    /// through the link to wherever it points — outside the content-addressed store. Staged with
    /// [`crate::fsutil::make_dangling_link`], which falls back to an NTFS junction on an unprivileged
    /// Windows runner, so this test covers both legs via its own fallback rather than branching here.
    ///
    /// Drop guards (`src`/`store`, both [`crate::fsutil::ScratchDir`]) are armed at construction, before
    /// any assertion runs — no trailing `remove_dir_all`.
    #[test]
    fn cpe_1769_capture_does_not_write_a_blobs_bytes_through_a_dangling_link_at_its_hashed_name() {
        let src = scratch("cpe1769-src");
        let store = scratch("cpe1769-store");
        let file = src.join("payload.txt");
        fs::write(&file, b"CONTENT").unwrap();
        let hash = crate::checksum::hash_file(&file.to_string_lossy()).expect("hashing the source must succeed");

        fs::create_dir_all(blobs_dir(&store)).unwrap();
        let blob_dest = blobs_dir(&store).join(&hash);
        if !crate::fsutil::make_dangling_link(&blob_dest) {
            use std::io::Write;
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1769] SKIPPED the capture dangling-link blob leg: this machine could not stage a \
                 link or junction at all. NOTHING in this test covered CPE-1769 on this run."
            );
            return;
        }
        let phantom_target = crate::fsutil::dangling_link_target(&blob_dest);
        assert!(!phantom_target.exists(), "sanity: the link really is dangling before capture runs");

        let out = capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::default());

        // THE HARM, on the filesystem, before trusting whatever `capture` returned: nothing must have
        // appeared at the link's phantom target (a copy following the link lands exactly there), and the
        // blob's hashed name must still hold the link, not bytes written through it.
        assert!(
            !phantom_target.exists(),
            "the blob copy must not have followed the dangling link to its (nonexistent) target"
        );
        assert!(
            std::fs::symlink_metadata(&blob_dest).is_ok_and(|m| m.file_type().is_symlink()),
            "the blob's hashed name must still hold the link, not the file's bytes written through it"
        );
        // This site's established policy (CPE-1705's own doc comment above the call site) is that a
        // provably-occupied slot is a benign skip, not a refusal — re-copying identical content is
        // harmless, so the fix folds a link into that same bucket rather than inventing a new refusal
        // here. The capture as a whole is therefore still allowed to succeed.
        assert!(out.is_ok(), "an occupied (link) blob slot must be skipped, not turned into a hard failure: {out:?}");
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

    /// A single unreadable candidate id must never be handed back as free — `save_manifest` would
    /// `fs::write` straight over a real snapshot's file list.
    ///
    /// **This test was deleted once as "vacuous" and restored; the deletion was the error.** Under a
    /// target-only deny it did pass against the unfixed `while manifest_path(..).exists()` loop, because
    /// `fs::metadata` falls back to `FindFirstFileW` and reads the entry out of the parent directory.
    /// `deny_stat_of` now also denies `(RD)` on the parent, killing that fallback, so `exists()` answers
    /// `false` on a manifest that is really there and the `assert_ne!` fires as intended.
    ///
    /// Driven through the `pick_manifest_id` seam so the candidate ids are known in advance —
    /// `fresh_manifest_id` reads its own clock, and a test that pre-creates `<ms>` files computes a
    /// different millisecond and stages files the walk never probes.
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
                 unreadable-slot route on this run."
            );
        }
        #[cfg(windows)]
        {
            let store = scratch("cpe1705-mid-one");
            fs::create_dir_all(manifests_dir(&store)).unwrap();
            let ms = 1_700_000_000_000u64;
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

            let id = pick_manifest_id(&store, ms).expect("one unreadable slot must be skipped, not fatal");
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

    /// `fresh_manifest_id`'s **bound** — the second half of the same fix, tested separately so
    /// neutralising either reds exactly one test.
    ///
    /// Treating unknown-as-occupied without a bound is what turns a silent overwrite into an unbounded
    /// loop: with an unreadable directory *every* candidate is unknown, and against a dead mount each
    /// stat blocks for seconds. Past [`MAX_CONSECUTIVE_UNKNOWN_IDS`] the fixed loop refuses, while the
    /// `.exists()` original sails past all of them and hands back a guessed name.
    #[test]
    fn cpe_1705_an_unreadable_manifests_directory_refuses_instead_of_guessing_an_id() {
        use std::io::Write;
        #[cfg(not(windows))]
        {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1705] SKIPPED the fresh_manifest_id bound leg on this platform: the Unix deny \
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
            // Occupy AND deny the first `MAX_CONSECUTIVE_UNKNOWN_IDS` candidate ids this call will walk:
            // `<ms>`, `<ms>-1`, … Each must be a real file for the ACE to attach to.
            // A FIXED ms, driven through the `pick_manifest_id` seam — `fresh_manifest_id` reads its own
            // clock, so pre-created files would be named for a different millisecond and never probed.
            let ms = 1_700_000_000_000u64;
            let mut denied: Vec<PathBuf> = Vec::new();
            for n in 0..MAX_CONSECUTIVE_UNKNOWN_IDS {
                let id = if n == 0 { ms.to_string() } else { format!("{ms}-{n}") };
                let p = manifest_path(&store, &id);
                fs::write(&p, b"REAL MANIFEST").unwrap();
                denied.push(p);
            }

            struct Restore<'a>(&'a [PathBuf], &'a Path);
            impl Drop for Restore<'_> {
                fn drop(&mut self) {
                    for p in self.0 {
                        crate::fsutil::undo_deny_stat_of(p, self.1);
                    }
                    let _ = fs::remove_dir_all(self.1);
                }
            }
            let _r = Restore(&denied, &store);

            if !denied.iter().all(|p| crate::fsutil::deny_stat_of(p)) {
                let _ = writeln!(
                    std::io::stderr(),
                    "[CPE-1705] SKIPPED the fresh_manifest_id bound leg: could not deny stat of the \
                     candidate manifest files on this machine (running elevated, or an ACL-less \
                     filesystem). NOTHING in this test covered the unreadable-slot route on this run."
                );
                return;
            }

            // Pre-fix (`while ..exists()`) this returned `Ok("<ms>-8")` — a guessed id in a directory it
            // could not read, which `save_manifest` would then `fs::write` into.
            let err = pick_manifest_id(&store, ms).expect_err(
                "a run of unreadable candidate ids must REFUSE, never hand back a guessed name",
            );
            assert!(
                err.contains("could not find a free snapshot manifest id") && err.contains("unreadable"),
                "the refusal must name the uncertainty: {err}"
            );

            for p in &denied {
                crate::fsutil::undo_deny_stat_of(p, &store);
                assert_eq!(
                    fs::read(p).unwrap(),
                    b"REAL MANIFEST".to_vec(),
                    "every real manifest must be byte-for-byte intact"
                );
            }
        }
    }

    /// **The `Occupied => unknown_run = 0` reset, made load-bearing.** Interleave unreadable manifest ids
    /// with readable ones so the *total* unknown count exceeds [`MAX_CONSECUTIVE_UNKNOWN_IDS`] while no
    /// run of them ever does: the walk must find a free id, not refuse.
    ///
    /// Written because the PR #893 review broke this reset and found it redded **nothing** — the bound
    /// test only ever presents an uninterrupted run. Without it, a store with a handful of unreadable
    /// manifests scattered among real ones refuses every capture.
    #[test]
    fn cpe_1705_scattered_unreadable_manifest_ids_do_not_accumulate_into_a_refusal() {
        use std::io::Write;
        #[cfg(not(windows))]
        {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1705] SKIPPED the interleaved-unknowns leg on this platform: the Unix deny mechanism \
                 chmods the PARENT, making every candidate unreadable rather than alternating ones. \
                 NOTHING in this test covered the unknown_run reset on this run."
            );
        }
        #[cfg(windows)]
        {
            let store = scratch("cpe1705-mid-interleaved");
            fs::create_dir_all(manifests_dir(&store)).unwrap();
            let ms = 1_700_000_000_000u64;
            // Ids `<ms>`, `<ms>-1` … `<ms>-19` all occupied; every other one additionally denied. That is
            // 10 unknowns in total — more than MAX_CONSECUTIVE_UNKNOWN_IDS — but never two in a row.
            let mut denied: Vec<PathBuf> = Vec::new();
            for n in 0..20u32 {
                let id = if n == 0 { ms.to_string() } else { format!("{ms}-{n}") };
                let p = manifest_path(&store, &id);
                fs::write(&p, b"REAL MANIFEST").unwrap();
                if n % 2 == 1 {
                    denied.push(p);
                }
            }

            struct Restore<'a>(&'a [PathBuf], &'a Path);
            impl Drop for Restore<'_> {
                fn drop(&mut self) {
                    for p in self.0 {
                        crate::fsutil::undo_deny_stat_of(p, self.1);
                    }
                    let _ = fs::remove_dir_all(self.1);
                }
            }
            let _r = Restore(&denied, &store);

            if !denied.iter().all(|p| crate::fsutil::deny_stat_of(p)) {
                let _ = writeln!(
                    std::io::stderr(),
                    "[CPE-1705] SKIPPED the interleaved-unknowns leg: could not deny stat of the candidate \
                     manifests on this machine. NOTHING in this test covered the unknown_run reset on this \
                     run."
                );
                return;
            }

            let id = pick_manifest_id(&store, ms).expect(
                "unknowns broken up by real collisions must NOT accumulate into a refusal — the bound \
                 means \"this directory is unreadable\", and one with readable manifests in it is not",
            );
            assert_eq!(id, format!("{ms}-20"), "it must walk to the first genuinely free id");
        }
    }

    /// The ungated sibling: the ordinary collision walk still works on every OS. A `fresh_manifest_id`
    /// that refused whenever anything was in the way would break the two-captures-in-one-millisecond case
    /// the `-N` suffix exists for.
    #[test]
    fn cpe_1705_fresh_manifest_id_still_walks_past_ordinary_collisions() {
        let store = scratch("cpe1705-mid-ok");
        fs::create_dir_all(manifests_dir(&store)).unwrap();
        let ms = 1_700_000_000_000u64;
        // Two readable manifests already sitting on the first two candidate ids.
        fs::write(manifest_path(&store, &ms.to_string()), b"{}").unwrap();
        fs::write(manifest_path(&store, &format!("{ms}-1")), b"{}").unwrap();

        let id = pick_manifest_id(&store, ms).expect("readable collisions must still be walked past");
        assert_eq!(id, format!("{ms}-2"), "it must walk to the first genuinely free id");
        assert!(!manifest_path(&store, &id).exists(), "the chosen id must be genuinely free");
        // …and the real entry point still works end to end, clock and all.
        assert!(fresh_manifest_id(&store).is_ok());

        let _ = fs::remove_dir_all(&store);
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

    // ---- CPE-1823: a planted manifest is input, not a trusted record ----------------------------
    //
    // Every test below stages a manifest a hand-editor could write and asserts **the harm did not
    // happen** — the escape target is untouched, and nothing appeared under the restore folder — BEFORE
    // it looks at the `Result`. Order matters: the unfixed code returns `Ok(())` for most of these, so a
    // test that checked the `Result` first would report "expected Err, got Ok" and say nothing about
    // where the bytes went; and `!dest.join(x).exists()` at the *intended* location passes happily while
    // the escape succeeds, which is why the assertion is on the escape target.

    /// A 64-character lowercase hex string — the shape [`crate::checksum::hash_file`] produces, so the
    /// manifests planted below differ from a real one only in the field under test.
    const GOOD_HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    /// The bytes a successful attack would deposit. Distinctive so a stray copy is identifiable.
    const PAYLOAD: &[u8] = b"PWNED BY A PLANTED MANIFEST";

    /// Write a manifest straight to `store/manifests/<id>.json`, exactly as someone editing the JSON in a
    /// text editor would. Deliberately **not** via [`save_manifest`]: the attack is on the read side, and
    /// a manifest is only ever a file on disk to `load_manifest`.
    fn plant_manifest(store: &Path, id: &str, entries: &[(&str, &str)]) {
        let dir = manifests_dir(store);
        fs::create_dir_all(&dir).unwrap();
        let files: BTreeMap<String, PersistedFileState> = entries
            .iter()
            .map(|(path, hash)| {
                ((*path).to_string(), PersistedFileState { hash: (*hash).to_string(), size: PAYLOAD.len() as u64 })
            })
            .collect();
        let manifest = PersistedManifest { id: id.to_string(), created_ms: 0, files, skipped: Vec::new() };
        fs::write(manifest_path(store, id), serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
    }

    /// Put real bytes at `store/blobs/<hash>`. Without this the copy the attack depends on would fail for
    /// want of a source, and a test asserting "nothing was written" would pass on the wrong reason.
    fn plant_blob(store: &Path, hash: &str) {
        let dir = blobs_dir(store);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(hash), PAYLOAD).unwrap();
    }

    /// Every regular file currently under `dir`, using this module's own walker. Used to assert a refused
    /// restore wrote **nothing at all**, not merely nothing at the path the manifest asked for — the
    /// absolute-component shape resolves differently per platform, and this catches the write wherever it
    /// would have landed inside the tree.
    fn files_under(dir: &Path) -> Vec<String> {
        scan_dir(&dir.to_string_lossy()).unwrap().into_keys().collect()
    }

    /// The temp-directory name of a scratch dir, for building a `../<sibling>` escape that actually
    /// reaches it.
    fn dir_name(p: &Path) -> String {
        p.file_name().unwrap().to_string_lossy().into_owned()
    }

    /// `..` in a manifest path walks straight up out of the restore folder: `Path::push("..")` appends a
    /// `ParentDir` component that the filesystem then resolves. The staged escape lands in a **sibling
    /// scratch directory**, so this asserts on a real file appearing somewhere it must never appear.
    #[test]
    fn cpe_1823_a_dotdot_manifest_path_writes_nothing_outside_the_restore_folder() {
        let store = scratch("cpe1823-dotdot-store");
        let dest = scratch("cpe1823-dotdot-dest");
        let outside = scratch("cpe1823-dotdot-outside");
        assert_eq!(dest.parent(), outside.parent(), "the escape below assumes the two are siblings");

        let rel = format!("../{}/pwned.txt", dir_name(&outside));
        let escape = outside.join("pwned.txt");
        plant_blob(&store, GOOD_HASH);
        plant_manifest(&store, "planted", &[(rel.as_str(), GOOD_HASH)]);

        let r = restore(&store.to_string_lossy(), "planted", &dest.to_string_lossy());

        assert!(
            !escape.exists(),
            "HARM: a `..` manifest path wrote {} — arbitrary file write from a hand-edited manifest",
            escape.display()
        );
        assert!(files_under(&dest).is_empty(), "a refused entry must write nothing at all");
        let err = r.expect_err("a refused entry must fail the restore, never be skipped into an Ok");
        assert!(err.contains(&rel), "the refusal must name the offending manifest path, got: {err}");
    }

    /// An **absolute** component: `Path::push` with one *replaces* everything accumulated so far, so a
    /// single segment relocates the write anywhere the process can reach.
    ///
    /// The two platforms fail differently and both legs are real. On Windows the whole native path
    /// (`C:\…\outside`) survives `split('/')` as one segment, `push` replaces, and the bytes land in
    /// `outside` — the escape assertion is the live one. On Unix the same string splits on its leading
    /// `/` into an **empty** first segment, so `push` never replaces and the unfixed code instead
    /// materialises the absolute path *inside* the restore folder (`dest/tmp/…/pwned.txt`) — which the
    /// `files_under` assertion is what catches. Asserting only the escape target would have made this
    /// test pass on Linux and macOS while proving nothing there.
    #[test]
    fn cpe_1823_an_absolute_manifest_path_writes_nothing_outside_the_restore_folder() {
        let store = scratch("cpe1823-abs-store");
        let dest = scratch("cpe1823-abs-dest");
        let outside = scratch("cpe1823-abs-outside");

        let rel = format!("{}/pwned.txt", outside.display());
        let escape = outside.join("pwned.txt");
        plant_blob(&store, GOOD_HASH);
        plant_manifest(&store, "planted", &[(rel.as_str(), GOOD_HASH)]);

        let r = restore(&store.to_string_lossy(), "planted", &dest.to_string_lossy());

        assert!(
            !escape.exists(),
            "HARM: an absolute manifest component wrote {} — the push replaced the restore root",
            escape.display()
        );
        assert!(
            files_under(&dest).is_empty(),
            "a refused entry must write nothing at all, and wrote: {:?}",
            files_under(&dest)
        );
        let err = r.expect_err("a refused entry must fail the restore, never be skipped into an Ok");
        assert!(err.contains(&rel), "the refusal must name the offending manifest path, got: {err}");
    }

    /// A **drive-relative** component (`Z:name`) — a Windows shape with no Unix analogue: it carries a
    /// prefix but no root, which `PathBuf::push` documents as replacing `self` entirely. It then resolves
    /// against that drive's *current directory*.
    ///
    /// The drive is taken from the process's own working directory rather than hard-coded to `C:`,
    /// deliberately: the per-drive current directory for the drive the process is already on **is** the
    /// process CWD, so the landing site is known exactly. `C:` would resolve against a per-drive CWD this
    /// test does not control (and may not be able to write to), and a write that merely failed for want
    /// of permission would look like a passing guard.
    #[cfg(windows)]
    #[test]
    fn cpe_1823_a_drive_relative_manifest_path_never_writes_to_the_working_directory() {
        let store = scratch("cpe1823-drive-store");
        let dest = scratch("cpe1823-drive-dest");

        let cwd = std::env::current_dir().unwrap();
        let cwd_str = cwd.to_string_lossy().into_owned();
        assert_eq!(cwd_str.as_bytes()[1], b':', "expected a drive-lettered CWD, got {cwd_str}");
        let drive = &cwd_str[..2]; // ASCII drive letter + colon
        let name = "cpe1823-drive-relative-pwned.txt";
        let rel = format!("{drive}{name}");
        let landing = cwd.join(name);
        assert!(!landing.exists(), "sanity: {} must not already exist", landing.display());

        plant_blob(&store, GOOD_HASH);
        plant_manifest(&store, "planted", &[(rel.as_str(), GOOD_HASH)]);

        let r = restore(&store.to_string_lossy(), "planted", &dest.to_string_lossy());

        let landed = landing.exists();
        let _ = fs::remove_file(&landing); // never leave the repo dirty, even on the failing path
        assert!(
            !landed,
            "HARM: a drive-relative manifest path wrote {} — into the app's working directory",
            landing.display()
        );
        assert!(files_under(&dest).is_empty(), "a refused entry must write nothing at all");
        let err = r.expect_err("a refused entry must fail the restore, never be skipped into an Ok");
        assert!(err.contains(&rel), "the refusal must name the offending manifest path, got: {err}");
    }

    /// A link planted at an **interior** component. Every segment here (`link/pwned.txt`) is a plain,
    /// textually innocent name — no `..`, no absolute component — so the string check alone passes it and
    /// only resolving the path on disk reveals that `dest/link` leads out of the restore folder. This is
    /// the leg that covers the `confined_to` half of the fix; the textual guard cannot see it.
    ///
    /// A **directory** link, so the leg runs on Windows too (a junction needs no privilege), and the
    /// shape is right: an interior component is a directory by definition.
    #[test]
    fn cpe_1823_a_link_at_an_interior_component_never_redirects_the_restore_out_of_the_folder() {
        let store = scratch("cpe1823-link-store");
        let dest = scratch("cpe1823-link-dest");
        let outside = scratch("cpe1823-link-outside");

        let link = dest.join("link");
        if !crate::fsutil::make_dir_link(&outside, &link) {
            crate::skip_notice!(
                "[CPE-1823] SKIPPED the interior-link restore leg: this machine could not create a \
                 directory link at {} (no symlink privilege and no junction). NOTHING on this run \
                 covered a restore whose path runs THROUGH a link out of the restore folder.",
                link.display()
            );
            return;
        }

        let escape = outside.join("pwned.txt");
        plant_blob(&store, GOOD_HASH);
        plant_manifest(&store, "planted", &[("link/pwned.txt", GOOD_HASH)]);

        let r = restore(&store.to_string_lossy(), "planted", &dest.to_string_lossy());

        assert!(
            !escape.exists(),
            "HARM: the restore followed the planted link and wrote {} — outside the restore folder",
            escape.display()
        );
        let err = r.expect_err("a refused entry must fail the restore, never be skipped into an Ok");
        assert!(err.contains("link/pwned.txt"), "the refusal must name the offending path, got: {err}");
    }

    /// The **read** side: `hash` is joined onto `blobs/` with nothing checking it, so a manifest could
    /// name any file the app can read and have its bytes copied into the restored tree under a name the
    /// same manifest chose. Both escaping shapes are staged — a `..` climb and an absolute path, which
    /// `Path::join` lets replace the whole base on Unix as well as Windows.
    ///
    /// The harm assertion is on the restored file's **existence and content**: here the intended location
    /// *is* where the stolen bytes land, so `!dest/stolen.txt` is the escape assertion, not a decoy.
    #[test]
    fn cpe_1823_an_escaping_hash_never_reads_a_file_outside_the_blob_store() {
        const SECRET: &[u8] = b"this is the victim's private file";
        let secrets = scratch("cpe1823-hash-secrets");
        let secret = secrets.join("secret.txt");
        fs::write(&secret, SECRET).unwrap();

        // `blobs/` is `<store>/blobs`, so `../../<sibling>/secret.txt` climbs store → temp and back down.
        let climb = format!("../../{}/secret.txt", dir_name(&secrets));
        let absolute = secret.to_string_lossy().into_owned();

        for hash in [climb.as_str(), absolute.as_str()] {
            let store = scratch("cpe1823-hash-store");
            let dest = scratch("cpe1823-hash-dest");
            fs::create_dir_all(blobs_dir(&store)).unwrap();
            plant_manifest(&store, "planted", &[("stolen.txt", hash)]);

            let r = restore(&store.to_string_lossy(), "planted", &dest.to_string_lossy());

            let stolen = dest.join("stolen.txt");
            let leaked = fs::read(&stolen).unwrap_or_default();
            assert!(
                !stolen.exists() && leaked != SECRET,
                "HARM: hash {hash:?} pulled {} bytes from outside the blob store into the restored tree",
                leaked.len()
            );
            assert!(files_under(&dest).is_empty(), "a refused entry must write nothing at all");
            let err = r.expect_err("a refused entry must fail the restore, never be skipped into an Ok");
            assert!(err.contains("stolen.txt"), "the refusal must name the offending path, got: {err}");
        }
    }

    /// The same unvalidated `hash` reaches a `remove_file` in [`prune`] — an arbitrary **delete**. The
    /// refusal has to come before the manifest file itself is unlinked (that step is documented as the
    /// point of no return), so this asserts the victim survives *and* that the store is intact enough to
    /// prune legitimately afterwards.
    #[test]
    fn cpe_1823_an_escaping_hash_never_deletes_a_file_outside_the_blob_store() {
        let store = scratch("cpe1823-prune-store");
        let victims = scratch("cpe1823-prune-victims");
        let victim = victims.join("important.txt");
        fs::write(&victim, b"the user's only copy").unwrap();

        fs::create_dir_all(blobs_dir(&store)).unwrap();
        let hash = format!("../../{}/important.txt", dir_name(&victims));
        plant_manifest(&store, "planted", &[("a.txt", hash.as_str())]);

        let r = prune(&store.to_string_lossy(), "planted");

        assert!(victim.exists(), "HARM: prune deleted {} — outside the blob store", victim.display());
        assert!(
            manifest_path(&store, "planted").exists(),
            "the refusal must land before the manifest is unlinked, so the refusal costs nothing"
        );
        let err = r.expect_err("a manifest naming a path outside the store must be refused loudly");
        assert!(err.contains("planted"), "the refusal must name the manifest, got: {err}");
    }

    /// The counterweight, and it is not decoration: the *other* helper CPE-1823 considered —
    /// [`crate::transfer::is_safe_name`] — rejects any leaf beginning with `..` and any leaf containing
    /// `:`, which would have made a file the capture happily stored **unrestorable**. `..evil` is an
    /// ordinary, legal filename on all three platforms. A future tightening that reaches for the stricter
    /// predicate reds here instead of silently breaking a round trip.
    #[test]
    fn cpe_1823_a_legal_dotdot_prefixed_filename_still_round_trips() {
        let src = scratch("cpe1823-ok-src");
        let store = scratch("cpe1823-ok-store");
        let dest = scratch("cpe1823-ok-dest");
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::write(src.join("sub/..evil"), b"perfectly ordinary bytes").unwrap();

        let outcome =
            capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED).unwrap();
        restore(&store.to_string_lossy(), &outcome.manifest_id, &dest.to_string_lossy())
            .expect("a legal filename that merely starts with `..` must still restore");
        assert_eq!(fs::read(dest.join("sub").join("..evil")).unwrap(), b"perfectly ordinary bytes");
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
