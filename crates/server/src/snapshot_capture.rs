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

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
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
    // CPE-1844 — **`plan.reused` is walked here too, and that is the fix, not a tidy-up.** `reused` is
    // `plan_capture`'s dedup verdict, and dedup asks exactly one question: `BlobStore::contains(hash)`,
    // i.e. *does `index.json` list this hash*. That is a claim in a hand-editable file, and this loop
    // used to act on it by writing nothing at all — so an index entry naming a blob whose file is not on
    // disk made the next capture of that content store none of it, while reporting a checkpoint created.
    // Reproduced through the registered commands before this changed, with no attacker in it: the index
    // entry left behind is precisely `prune`'s own documented leak-over-corruption residue (blob files
    // removed, then a failure before `save_store`), and it is also what a partial restore-from-backup of
    // a store leaves — this module's stated threat premise.
    //
    // ```text
    // blobs/<hash> deleted, index.json's entry for <hash> left in place
    //   checkpoint_create        -> Ok, "second" checkpoint recorded, blob file still absent
    //   checkpoint_revert(second)-> Ok(applied: 0, skipped: [a.txt: stored copy (blob <hash>) could not be read])
    //   a.txt still reads "damaged"
    // ```
    //
    // The repair is to ask the disk instead of the index, and the existing slot probe below already *is*
    // that question — a blob whose file is there is `Occupied` and skipped, costing one stat. So a reused
    // blob is written when, and only when, its bytes are genuinely **absent**. `added_bytes` deliberately
    // still reports the plan's figure: it is "what this capture added that the store did not have", and a
    // repair write is content the store was already supposed to be holding.
    //
    // **Absent is the whole of it, and an earlier draft of this comment over-claimed by saying dedup goes
    // back to being "an optimisation rather than a promise".** It does not. A blob file that is *present*
    // but whose bytes were replaced in place is `Occupied` by the probe below and skipped, so its content
    // is still taken on trust and a restore hands back whatever is now in the file — measured by the
    // security audit as `restored bytes = "PLANTED BYTES"`. That is pre-existing (the probe's
    // occupied-is-already-there policy is CPE-1705's and CPE-1769's, and re-hashing every reused blob on
    // every capture is a different ticket's cost), but the sentence claimed a property this repair does
    // not deliver. What it does deliver: a claim about content the store does **not** hold is no longer
    // acted on by writing nothing.
    for blob in plan.to_store.iter().chain(plan.reused.iter()) {
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
        // CPE-1847: the map's one falsifiable claim about itself — see `PersistedManifest::file_count`.
        // Computed here from the map that is about to be written, so the two cannot drift apart.
        file_count: Some(files.len()),
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
///
/// # Judge everything, then write — the abort is total
///
/// Two passes. Pass 1 runs all three guards over every entry and creates nothing; pass 2 does the writes,
/// and by then no entry can be refused. So a refused manifest leaves the destination **exactly as it was**
/// — the same "the refusal costs nothing" property [`prune`] gets by validating before its point of no
/// return — and one message names every offending entry instead of surfacing them one re-run at a time.
///
/// Two refusals are necessarily *not* total, and both say so in their own message: the pass-2
/// re-validation (a component swapped mid-restore) and the pass-2 collision check (two entries that
/// resolve onto one file, which in a fresh destination is invisible until the first of them exists).
/// Both are still `Err` — the alternative is a tree that looks complete and is not.
///
/// This replaced a refuse-as-you-go loop justified by the claim that "nothing legitimate can produce a
/// refused entry". That claim was **false twice over**, which is why the structure changed instead of the
/// wording. First within a platform: `safe_segments` refused `:` and `\` everywhere, so an ordinary Linux
/// or macOS filename (`2026-08-21 10:30 notes.txt`, or any Finder name containing `/`, which macOS stores
/// as `:`) aborted a restore mid-tree — fixed at the source by `cfg!(windows)`-gating it. Then across
/// platforms, where the same argument fails for the rules that gating introduced: `NUL`, `notes. ` and
/// `a\b` are all names a Linux or macOS [`capture`] stores happily and a Windows restore must refuse, and
/// a store carried between machines is this ticket's own threat premise, not an exotic case. A partial
/// tree was therefore always reachable through legitimate use. Now it is not reachable at all.
///
/// # Windows-only rules stay on Windows
///
/// Two rules apply on Windows and nowhere else, and **both live inside `safe_segments`**, not here:
/// its refusal of `:` and `\`, and its refusal of a reserved device name or a trailing dot/space. On
/// Linux and macOS every one of those is an ordinary byte in an ordinary filename that [`capture`] will
/// happily store, and refusing them there would break a working round trip to defend against a hazard
/// that exists only on the third platform — the mistake [`crate::fsutil::win32_name_is_unstable`]'s doc
/// records being made and reverted once already.
///
/// **They are in `safe_segments` because this function is not where the risk is.** CPE-1823 twice landed
/// a guard here, on a function with no production caller, while `revert_engine`'s `apply_write` and
/// `apply_delete` — reached from the registered `checkpoint_revert` commands — went unguarded and, in
/// the trailing-space case, *deleted the user's file* while reporting complete success. Every rule that
/// belongs to "resolve a manifest-supplied relative path under a root" now sits in the one helper all
/// four call sites share, so the two paths cannot drift again.
///
/// The gates are a property of the **host**, not of the manifest: a manifest captured on Linux carrying
/// `a\b` or `notes. ` restores as a literal name there and is refused here. Right direction, but it means
/// one manifest is legal on one machine and not another — the round-trip asymmetry is reduced, not gone.
pub fn restore(store_dir: &str, manifest_id: &str, dest: &str) -> Result<(), String> {
    let store_path = Path::new(store_dir);
    let dest_path = Path::new(dest);
    let manifest = load_manifest(store_path, manifest_id)?;
    let blobs_dir_path = blobs_dir(store_path);
    // The caller's own destination, created before pass 1 because `safe_target`'s containment check
    // resolves against it. This is the one thing a fully-refused restore leaves behind, and it is not
    // manifest-controlled: it is the empty directory the caller named in the call.
    fs::create_dir_all(dest_path).map_err(|e| format!("{}: {e}", dest_path.display()))?;

    // ---- pass 1: the ABORT DECISION only. Judges every entry, touches nothing. ---------------------
    let mut refusals: Vec<String> = Vec::new();
    // **CPE-1823 round 5 — two entries that ADDRESS ONE FILE.** Every rule before this one judges an
    // entry on its own, and this hazard has no single offending entry: `A.txt` and `a.txt` are both
    // perfectly legal names on every platform, so no per-entry rule may refuse either, and on a
    // case-folding volume they are one file. Restoring both wrote one file holding whichever content
    // was copied last, lost the other, and returned `Ok(())` — a restore is *believed*, so a manifest
    // entry that silently never arrived is exactly the failure this module's own doc exists to forbid.
    //
    // Asked of the resolved path rather than of the spelling, per [`crate::fsutil::confined_to`]'s
    // principle, so it covers trailing spaces and dots, 8.3 short names, Unicode-folding volumes and a
    // link inside `dest` without naming any of them.
    let mut lands_on: HashMap<PathBuf, &String> = HashMap::new();
    for (rel, file) in &manifest.files {
        if let Err(why) = resolve_entry(dest_path, &blobs_dir_path, rel, &file.hash) {
            refusals.push(why);
        }
        if let Some(at) = crate::revert_engine::landing(dest_path, rel) {
            if let Some(first) = lands_on.insert(at, rel) {
                refusals.push(refusal(
                    rel,
                    &format!(
                        "it addresses the same file as the entry {first:?} — two manifest entries, one \
                         surviving file, and whichever is copied last silently wins"
                    ),
                ));
            }
        }
    }
    if !refusals.is_empty() {
        return Err(refusal_summary(&refusals));
    }

    // ---- pass 2: re-judge each entry IMMEDIATELY before its own write ------------------------------
    //
    // **Pass 1's verdicts are deliberately not carried here, and that is the whole point (round 4).**
    // The first version of this split kept the `(target, blob)` pairs pass 1 resolved and copied those,
    // which reintroduced this ticket's original arbitrary write: `confined_to`'s answer for entry #1 was
    // reached before every *other* entry was validated and written, so the window stopped being one
    // `create_dir_all` and became the whole pass plus the whole preceding write run. An attacker need
    // not race that blindly — the first byte hitting disk is the signal that pass 1 is over and its
    // verdicts are stale — and swapping an already-blessed interior component for a junction then landed
    // the write outside the folder **5 runs out of 5**, with `Ok(())` returned.
    //
    // Re-resolving costs one extra canonicalise per entry, on a path about to be written anyway, and
    // restores the property the original per-entry loop had: the check a write relies on is the most
    // recent thing that happened before it. Blind racers have failed tens of thousands of swaps against
    // that shape across three rounds without an escape.
    //
    // Pass 1 is kept for what it is genuinely good at — the all-or-nothing decision, so a manifest with
    // a refused entry writes *nothing* rather than a partial tree. Neither pass is sufficient alone.
    //
    // **Why not `fsutil::copy_file_into_claimed_slot` (CPE-1765) — asked, and answered no.** It closes
    // the final component properly, claiming the name with `create_new` rather than writing to a name a
    // probe pronounced free, and it is exactly right where the name is *picked*. This name is **chosen
    // by the caller**, not picked, and `create_new` refuses a name that already exists: restoring a
    // snapshot over a tree that still holds files, and `revert_engine`'s first-class `Overwrite` op,
    // both depend on writing onto an existing file. Claiming the slot would turn those into refusals.
    //
    // **CPE-1846 took the other half of that pattern, which is the half a restore can use.** The
    // rejection above covered only step 1 (claim the name atomically). Step 2 — *never follow a link at
    // the final component* — costs a restore nothing, because opening an existing regular file for
    // truncate-and-write is exactly what an overwrite is. `fsutil::copy_file_onto_no_follow` below opens
    // the target with `batch_media`'s own `O_NOFOLLOW` / `FILE_FLAG_OPEN_REPARSE_POINT` open (the same
    // per-target constants, not a second spelling), refuses the handle if it addresses a link or a
    // directory, and streams the blob's bytes through it. So the final-component swap is no longer a
    // race at all: the object written is the object opened.
    //
    // What that residual *was*, stated accurately because an earlier draft overstated it in both
    // directions: it was never that the final component is unprotected — `confined_to` canonicalises the
    // final component too, and an independent audit planted 17,488 symlinks at it for **zero** writes
    // through — it was that the check and the copy were two syscalls, checked but not atomically. That
    // gap is now gone.
    //
    // **The interior-component race is NOT closed and is still the recorded residual.** `safe_target`
    // resolves the directories above the final component by path, and the open below is by path too, so
    // a directory link swapped into an interior component between them still redirects the write.
    // Closing that needs `openat`-relative resolution, which `std` does not expose. Pass 2 re-resolving
    // immediately before each write is what keeps that window one open wide instead of a whole pass.
    //
    // The Windows cost of writing through a handle rather than `CopyFileExW` — alternate data streams
    // are no longer carried — is measured and argued in `copy_file_onto_no_follow`'s own doc.
    let mut written: HashSet<PathBuf> = HashSet::new();
    for (rel, file) in &manifest.files {
        let (target, blob) = resolve_entry(dest_path, &blobs_dir_path, rel, &file.hash).map_err(|why| {
            format!(
                "{why} (detected immediately before writing it — the destination changed during the \
                 restore, so entries written before this one may already be on disk)"
            )
        })?;
        // Pass 1's collision check can only see entries that already resolve to something, so in a
        // *fresh* destination it sees neither `A.txt` nor `a.txt` until one of them exists. This closes
        // that half by observation instead of prediction: after each copy the file's real identity goes
        // in `written`, and a later entry that resolves onto one of them is refused **before** its copy
        // rather than silently overwriting a sibling entry's content.
        //
        // Unlike a pass-1 refusal this one is not total — earlier entries are already on disk — but the
        // alternative is not a clean tree, it is a clean-looking tree with a file missing and `Ok(())`
        // returned. Deliberately checked here and not carried from pass 1, for round 4's reason: the
        // only verdict a write may rely on is the one taken immediately before it.
        if let Some(at) = crate::revert_engine::landing(dest_path, rel) {
            if written.contains(&at) {
                return Err(refusal(
                    rel,
                    "it resolves onto a file this same restore has already written — two manifest \
                     entries address one file, so one of them would silently vanish (entries written \
                     before this one are already on disk)",
                ));
            }
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        crate::fsutil::copy_file_onto_no_follow(&blob, &target)
            .map_err(|why| format!("{}: {why}", blob.display()))?;
        if let Ok(at) = fs::canonicalize(&target) {
            written.insert(at);
        }
    }
    Ok(())
}

/// The three guards for one manifest entry, yielding the `(target, blob)` pair pass 2 will use. Pure
/// judgement — it creates nothing, which is what lets [`restore`] run it over the whole manifest first.
fn resolve_entry(
    dest_path: &Path,
    blobs_dir_path: &Path,
    rel: &str,
    hash: &str,
) -> Result<(PathBuf, PathBuf), String> {
    // Both the textual rules and the resolved-containment check now live inside `safe_target`, so this
    // function and `revert_engine`'s two sinks cannot disagree about what is safe (CPE-1823 round 3).
    let target = crate::revert_engine::safe_target(dest_path, rel).map_err(|why| refusal(rel, &why))?;
    let blob = blob_source(blobs_dir_path, hash).map_err(|why| refusal(rel, &why))?;
    Ok((target, blob))
}

/// How many refused entries a [`restore`] failure names before summarising the rest. A planted manifest
/// can carry thousands; a multi-megabyte error string helps nobody, and the first few establish the shape.
const MAX_NAMED_REFUSALS: usize = 10;

/// Every refusal in one message. The user gets the whole picture from a single run instead of discovering
/// the next bad entry only after fixing the previous one — which, for a manifest that is planted rather
/// than merely damaged, is the difference between seeing an attack and seeing a typo.
fn refusal_summary(refusals: &[String]) -> String {
    let mut out = format!(
        "this manifest cannot be restored: {} of its entries were refused, and nothing was written.\n",
        refusals.len()
    );
    for why in refusals.iter().take(MAX_NAMED_REFUSALS) {
        out.push_str("  - ");
        out.push_str(why);
        out.push('\n');
    }
    if refusals.len() > MAX_NAMED_REFUSALS {
        out.push_str(&format!("  …and {} more\n", refusals.len() - MAX_NAMED_REFUSALS));
    }
    out
}

/// The per-entry refusal message (CPE-1823), in one place so every rejected shape reads the same and all
/// of them name the manifest path that was rejected. The path is what the user has to act on: it is the
/// only thing tying the refusal back to a line in the JSON, and it is also the evidence that the manifest
/// was tampered with rather than merely unlucky.
/// An empty `rel` is named explicitly rather than producing the headless `": refusing …"` the first cut
/// emitted — a message whose subject is invisible is barely a message.
pub(crate) fn refusal(rel: &str, why: &str) -> String {
    let named = if rel.is_empty() { "(a manifest entry with an empty path)" } else { rel };
    format!("{named}: refusing this manifest entry — {why}")
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
/// **`pub(crate)` because this is the whole crate's rule for that field, not this module's.** The same
/// manifest `hash` is joined onto `blobs/` at three other sinks the shipped app actually reaches —
/// [`crate::revert_engine`]'s `apply_write`, [`crate::checkpoint_store::checkpoint_diff_file`], and
/// [`prune`] — and the first version of this fix hardened only [`restore`], which has no production
/// caller at all. Three of those four call **this** function; [`prune`] deliberately calls only
/// [`validate_blob_name`], because the containment half answers "could this read pull bytes from outside
/// the store" and a `remove_file` on a planted link removes the link, never its target — see its call
/// site.
///
/// # What containment does NOT catch here, stated because an earlier draft of this doc overclaimed
///
/// - A **hard link** planted at `blobs/<hash>`. It needs no privilege on Windows, and `canonicalize`
///   resolves it to *itself* — a hard link is not a redirection the filesystem can report — so both this
///   check and [`crate::fsutil::confined_to`] see an ordinary file inside the store and pass it. The
///   earlier sentence "a link planted at a blob's name cannot substitute another file's bytes" was true
///   only of symlinks and junctions, which *are* both refused. Rated follow-up rather than blocker
///   because a hard link does not survive the copy or sync step in this threat model — a planted store
///   arrives as ordinary files — but the limit is real and is recorded rather than papered over.
/// - Replacing **`blobs/` itself** with a directory link. `confined_to` canonicalises the root too, so a
///   relocated store is self-consistently "contained" in its new location. Guarding it means pinning the
///   store root at open time, which is a different ticket's shape.
///
/// The length bound is a sanity cap, not the security property: it keeps a 4 MB "hash" out of a path
/// buffer without hard-coding sha256's 64 characters into a format that has already been described as
/// swappable in this module's own header.
pub(crate) fn blob_source(blobs_dir: &Path, hash: &str) -> Result<PathBuf, String> {
    validate_blob_name(hash)?;
    // Distinguished from the containment failure below on purpose: an absent `blobs/` is an incomplete
    // or half-deleted store, and reporting that as "does not resolve inside the blob store" reads as
    // tampering and sends the user hunting for an attack that isn't there. `confined_to` fails closed on
    // an unresolvable root (correctly), so without this the two causes are indistinguishable.
    // The message names no path: since CPE-1845 these reasons are rendered in the revert panel, and the
    // checkpoint store's on-disk layout is the app's private business, not something to put in a dialog.
    if let Err(e) = std::fs::metadata(blobs_dir) {
        return Err(format!("this checkpoint's blob store could not be opened: {e}"));
    }
    let blob = blobs_dir.join(hash);
    if !crate::fsutil::confined_to(&blob, blobs_dir) {
        return Err(format!("blob {hash:?} does not resolve inside this checkpoint's blob store"));
    }
    Ok(blob)
}

/// The name half of [`blob_source`]'s rule, split out because [`prune`] needs exactly this and not the
/// containment half (see its call site). A blob file is named by a content address and nothing else.
pub(crate) fn validate_blob_name(hash: &str) -> Result<(), String> {
    if hash.is_empty() || hash.len() > MAX_BLOB_NAME_LEN || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("its content hash {hash:?} is not a plain hex blob name"));
    }
    Ok(())
}

/// Longest blob name [`validate_blob_name`] will entertain. Comfortably past sha256's 64 hex characters
/// (and sha512's 128) without pinning the digest this module happens to use today.
const MAX_BLOB_NAME_LEN: usize = 128;

/// Drop manifest `manifest_id`'s hold on its blobs via [`release`] and remove the now-unreferenced blob
/// files from disk. Returns the bytes freed — **measured from the files this call actually removed**
/// (CPE-1844), not from the sizes `index.json` records for them. The manifest file itself is deleted; a
/// manifest no longer on
/// disk cannot be [`restore`]d.
///
/// Ordering matters and is deliberate: the manifest file is deleted **first** — that single, atomic
/// `remove_file` is the point of no return. `release` decrements refcounts, so running it twice on the
/// same manifest would double-decrement a shared blob to 0 and delete content another snapshot still
/// needs (silent data loss). Deleting the manifest up front makes a retry-after-failure always safe: if
/// this `remove_file` fails, nothing else has changed → a clean retry; if it succeeds but a later step
/// (`release`/`save_store`) fails, the manifest is already gone so no second `release` can
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

    // CPE-1844 — **`load_store` is read here, ABOVE the point of no return, and that placement is the
    // fix.** It used to sit just below the `remove_file`. `load_store` is fail-closed by design
    // (CPE-1705: an `index.json` that is unparseable, or is not a regular file, or cannot be stat'd, is
    // a refusal — reading it as a fresh store is that ticket's cross-snapshot data loss). Refusing after
    // the manifest file is already deleted meant the refusal cost a checkpoint. Walking `prune`'s own
    // gate list — the axis CPE-1861's enumeration recorded as the one it failed to walk — this was the
    // one gate on the wrong side of the line. Measured through `checkpoint_prune_apply`, four
    // checkpoints, `index.json` truncated to `{"blobs": {`:
    //
    // ```text
    // pass 1  Err(index.json: EOF while parsing…)   manifests left = 3
    // pass 2  Err(same)                             manifests left = 2
    // pass 3  Err(same)                             manifests left = 1
    // pass 4  Ok                                    manifests left = 1
    // ```
    //
    // One checkpoint destroyed per retention pass, each pass reporting failure, their blobs leaked
    // because `release` never ran — the store thinned to the one-survivor floor by an unreadable ledger.
    // A torn write or a truncation is enough; no attacker is required. Hoisted, the refusal is total:
    // nothing has been touched at this line, so a store whose index cannot be read is simply not pruned.
    //
    // The stall that leaves behind is deliberate and is the safe direction — there is nothing sound to
    // do with a refcount ledger you cannot read except stop, and unlike CPE-1861's per-manifest stalls
    // this one cannot be mirrored into `list_manifests`, because it is a property of the store rather
    // than of any one manifest. It is also loud, recoverable, and no longer destructive.
    let mut store = load_store(store_path)?;

    let mpath = manifest_path(store_path, manifest_id);
    fs::remove_file(&mpath).map_err(|e| format!("{}: {e}", mpath.display()))?; // point of no return

    // CPE-1861 — **one manifest, one refcount: a release must not drop a blob another manifest still
    // names.** That invariant is what the rest of this module assumes and what `index.json` alone
    // cannot enforce, because a refcount is a *counter bumped at capture time*, not a count of the
    // manifests on disk. Every way the two can disagree is a way for this function to delete content a
    // surviving manifest still points at:
    //
    // - a manifest **file** copied in by something other than a capture (Explorer copy/paste, a
    //   cloud-sync conflict copy, a backup script, a partial restore-from-backup) adds a namer without
    //   ever bumping a ref — measured: two manifest files, `refs: 1`;
    // - a manifest file removed by hand drops a namer without dropping a ref (the harmless direction);
    // - `prune`'s own documented leak-over-corruption retry window can leave a ref behind.
    //
    // So the index is asked only for the *cheap* question — which of this manifest's hashes are even in
    // danger of hitting zero — and the authoritative question ("does anything still name it?") is
    // answered by **recomputing from the manifests actually on disk**, the shape CPE-1823 kept landing
    // on. The index lookup is the cheap gate: a hash whose `refs` is 2 or more cannot reach zero here,
    // so it costs one map lookup and nothing more.
    //
    // **That gate does not usually spare the scan, and it should not be read as if it did.** The
    // ordinary scheduled prune removes a snapshot holding unique blobs nothing else references, so at
    // least one hash is at risk and `manifests/` is walked on essentially every pass — which is what
    // the 6.2 ms → 6.8 ms figure in `manifests_naming`'s cost note actually measures. The scan is the
    // normal cost of a prune, not an exceptional one.
    let at_risk: BTreeSet<String> = hashes
        .iter()
        // `map_or(true, ..)`: a hash the index doesn't know about is at risk too — the delete loop
        // below keys off `!store.contains(hash)`, so an absent entry is exactly the case that removes a
        // blob file with no refcount ever having said it was free.
        .filter(|h| store.get(h).map_or(true, |m| m.refs <= 1))
        .cloned()
        .collect();
    // The pruned manifest's own file is already gone (the point of no return above), so this scan sees
    // precisely the survivors.
    let still_named = manifests_naming(store_path, &at_risk);

    // Skip the decrement rather than decrement-and-restore: the survivor's hold is what the count
    // *should* have been all along, so leaving it at its current value is the repair. As a rule about
    // this function it is self-correcting — prune the last namer and nothing protects the blob any
    // more, so it is freed then — and the honest two-captures-share-a-blob case is untouched (its
    // `refs` is 2, so it is never even at risk).
    //
    // **"Prune the last namer" is not reachable through retention, though, and an earlier version of
    // this comment claimed "no permanent leak" on the strength of it.** That was wrong, because the two
    // halves of CPE-1861 interact and neither one says so on its own: `list_manifests` refuses to list
    // a manifest file that does not describe itself, so the very file protecting this blob is one
    // retention can never name. Measured, driving the real `snapshot_prune::apply`:
    //
    // ```text
    // 3 captures (m1 oldest, unique 12-byte blob) + an Explorer copy "<m1> - Copy.json"; hourly=2
    //   pass 1       pruned=[m1]  kept=[m3, m2]  bytes_freed=0
    //                m1's unique blob still on disk after its owner was pruned: true   (index refs: 1)
    //   passes 2-4   pruned=[]    freed=0        every time
    //   final        blob present; manifests/ = ["<m1> - Copy.json", "<m2>.json", "<m3>.json"]
    //   prune("<m1> - Copy") by id  ->  freed=12, blob gone
    // ```
    //
    // So a copy pins the whole pruned snapshot's unique content for as long as it sits in the store,
    // and a *recurring* copier (a sync client leaving one per cycle) grows it without bound — measured
    // at `list_manifests`, where the size of the residue is stated in full. Reclaiming it needs the file
    // removed by hand, or `prune` called with that id directly — the last line above, which is exactly
    // what `cpe_1861_prune_never_frees_a_blob_another_manifest_file_still_names` exercises.
    let to_release: BTreeSet<String> = hashes.difference(&still_named).cloned().collect();
    release(&mut store, &to_release);
    let blobs_dir_path = blobs_dir(store_path);
    // CPE-1844 — **the returned figure is the bytes whose files this loop actually removed**, measured
    // one `metadata()` ahead of each `remove_file`, not `release`'s return value. `release` sums the
    // `size` fields of the index entries it garbage-collected, which is `index.json`'s claim about how
    // big those blobs are; on the fixture that opens this ticket it reported `bytes_freed=4000000000`
    // for a store holding 45 bytes. That figure is reported to the caller as
    // `RetentionApplyResult::bytes_freed` and shown as a result of a destructive operation, so it is
    // held to the same rule as every other count in this tree (CPE-1803/1804/1805/1816): a number the
    // user reads must describe what happened. An entry the index lists but whose file is already gone
    // now contributes 0 rather than its recorded size, which is also simply true.
    let mut freed = 0u64;
    for hash in &hashes {
        if !store.contains(hash) && !still_named.contains(hash) {
            let path = blobs_dir_path.join(hash);
            let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            if fs::remove_file(&path).is_ok() {
                // best-effort; index is the source of truth for *what* is free, the disk for how much
                freed = freed.saturating_add(size);
            }
        }
    }
    save_store(store_path, &store)?;
    Ok(freed)
}

/// Which of `wanted` are still named by a manifest file on disk under `store_dir` — the recomputed
/// witness behind [`prune`]'s CPE-1861 invariant. Empty `wanted` never touches the disk.
///
/// **Deliberately more permissive than [`list_manifests`], and the difference is the whole point.**
/// `list_manifests` answers "may this file *steer* a destructive decision?" and so demands a
/// self-consistent record. This answers "would deleting these bytes destroy something that still points
/// at them?", where any parseable manifest file counts — including the duplicate copy, the id-liar, and
/// the crafted filename that `list_manifests` refuses to list. Applying the strict rule here would
/// re-open exactly the hole this closes.
///
/// Failure is conservative in the same direction: an unreadable `manifests/` directory answers "all of
/// them are still named", so the blobs are kept and the residue is a space leak — never a delete of
/// content nothing could prove was free. Same leak-over-corruption tradeoff [`prune`] itself documents.
/// A single manifest file that can't be read or parsed *is* skipped, matching [`list_manifests`]: its
/// own capture already put a ref in the index, so it is protected by the refcount rather than by this.
///
/// **Cost, measured rather than assumed** (release build, CPE-1861). The scheduled shape —
/// [`crate::snapshot_schedule::snapshot_run_due`] pruning the one capture that just aged out — pays
/// 6.2 → 6.8 ms on a 50-manifest store and 9.6 → 18.3 ms on a 200-manifest × 50-file one. The worst
/// case, a single pass thinning 197 of 200 manifests, is 1.08 s → 1.81 s: `prune` rescans the survivors
/// per call, so a bulk thin is quadratic in manifest count.
///
/// The round-3 security audit fitted the delta and the model is worth keeping, because it says when to
/// care: `3.2 µs · n(n−1)/2 + 2.9 ms · n`, within about 1% at n = 50/100/200. The quadratic term is
/// real but the **linear** one dominates until n ≈ 1800, and the shipped default `RetentionPolicy`
/// (24/7/4/12) caps a store at roughly 47 manifests — so the worst case is not reachable under
/// defaults. It *is* reachable on the **first pass after this fix un-wedges a long-stalled store**,
/// which is exactly the situation this ticket creates: ~1.6 s, inside `spawn_blocking`, unattended.
/// That is a considered deferral rather than a hope. If a store ever does get big enough for it to
/// bite, the fix is to hoist the scan out of the per-manifest call — not to weaken it.
///
/// Generous on an unreadable `manifests/` — see [`manifests_naming_strict`], which carries the actual
/// scan and is the only thing that opens the directory. This wrapper is [`prune`]'s policy: a directory
/// it cannot open answers "all of them are still named" (keep the blobs, leak rather than destroy).
/// [`store_total_bytes`] needs the opposite policy and calls the strict variant directly instead — see
/// its own doc comment (CPE-1867) for why the same predicate needs two callers standing on opposite
/// sides of this failure branch.
fn manifests_naming(store_dir: &Path, wanted: &BTreeSet<String>) -> BTreeSet<String> {
    manifests_naming_strict(store_dir, wanted).unwrap_or_else(|_| wanted.clone())
}

/// [`manifests_naming`]'s scan, minus its generous fallback: an unreadable `manifests/` is returned as
/// the `read_dir` error instead of being silently answered "all of them are still named". The *only*
/// `read_dir` of `manifests/` in this predicate lives here — [`manifests_naming`] wraps it rather than
/// re-checking readability itself first, and [`store_total_bytes`] calls this directly rather than
/// probing readability and then calling [`manifests_naming`] (CPE-1867).
///
/// **Why the two-open shape was a real, measured window and not a theoretical one.** The round-2 audit
/// raced a thread renaming `manifests/` away and back against `store_total_bytes`'s old check-then-call
/// pair — a probing `read_dir` to confirm readability, then a second, independent `read_dir` inside
/// `manifests_naming` to do the scan — and out of 30,000 calls landed the rename in the gap between
/// them: `worst Ok value under the race = 2000000000, errs = 0`, the full pre-witness directory sum. The
/// pre-check answered "readable" honestly; the directory was gone by the time the second call ran, so
/// `manifests_naming`'s own fallback fired and answered "all of them are named" — safe for `prune`,
/// wrong here. A single `read_dir` closes the window outright rather than narrowing it: there is no
/// second call left for the race to land in. See the `cpe_1867_*` test below for the harness and the
/// worst-`Ok` figure measured against this fix.
///
/// **Compared case-INsensitively against `wanted` (CPE-1864).** Windows and macOS open
/// `blobs/05c2…b8` and `blobs/05C2…B8` as the same file; `validate_blob_name` accepts uppercase hex on
/// purpose (see its own doc comment — nothing in this app ever *writes* one, but refusing the format
/// would break restoring a store that already contains one, from an import, a sync, or a hand edit). A
/// manifest is exactly that kind of trusted-but-external input — CPE-1861 already documents a plain file
/// copy as a legitimate second namer of a blob — so a survivor manifest that happens to spell its hash in
/// a different case than the candidate set was, before this fix, invisible to this witness: `f.hash ==`
/// (or `wanted.contains`) compared byte-for-byte, uppercase against lowercase never matched, and the blob
/// that survivor still needed could be freed out from under it by pruning any other manifest sharing that
/// content. Measured by the independent Security Auditor: `keeper restores AFTER the prune:
/// Err("...\blobs\05C200FE…B8: cannot find the file")`.
///
/// **What was decided about `validate_blob_name`, and why the fix is here instead.** Tightening
/// `validate_blob_name` to refuse uppercase would not close this hole — the mismatch is between two hash
/// *strings*, not about whether either one is individually well-formed — and it would open a worse one:
/// `blob_source` (the read half `restore` uses) calls the same validator, so refusing uppercase would
/// make `restore` fail a manifest that legitimately names an existing uppercase-spelled blob, on the
/// store shapes the ticket calls out (an imported store, a different capture tool, a hand-recovered
/// manifest) — turning a working restore into a refused one. Left permissive; the comparison is fixed
/// instead.
///
/// **The return value is normalised back to `wanted`'s own spelling, not the disk manifest's.** Every
/// caller does `BTreeSet` algebra (`difference`, `contains`) between this return value and a set built in
/// `wanted`'s casing (`prune`'s `hashes`, `store_total_bytes`'s on-disk filenames — both always lowercase,
/// since capture only ever writes lowercase). Matching case-insensitively but returning the disk
/// manifest's own spelling would just move the exact-string mismatch one call further up instead of
/// closing it — `hashes.difference(&still_named)` would fail to cancel out the shared hash again, only
/// now because the two sides disagree in case rather than because one side never matched at all.
///
/// Every other hash comparison in this module was checked against this same shape and needs no change:
/// `contains`/`get`/`release` on [`BlobStore`] and the delete loop in [`prune`] all key off hashes that
/// are either this function's own (now-normalised) return value or drawn straight from `index.json`,
/// which only this app's own capture ever writes (always lowercase) — there is no second place an
/// externally-supplied spelling reaches a case-sensitive lookup.
fn manifests_naming_strict(store_dir: &Path, wanted: &BTreeSet<String>) -> std::io::Result<BTreeSet<String>> {
    let mut found = BTreeSet::new();
    if wanted.is_empty() {
        return Ok(found);
    }
    // Lowercase spelling -> `wanted`'s own original member, so a disk manifest's hash is matched
    // case-insensitively but `found` still holds exactly `wanted`'s spelling (see the case-insensitivity
    // note above for why that direction matters). If `wanted` itself somehow held two case-variant
    // spellings of the same hash (only possible on a case-sensitive filesystem carrying two distinct blob
    // files that differ only by case — not a shape this store ever produces), the later one in sorted
    // order wins; harmless, because on such a filesystem the two are genuinely different files and this
    // witness's case-insensitive matching is not what distinguishes them.
    let by_lower: BTreeMap<String, &String> = wanted.iter().map(|h| (h.to_ascii_lowercase(), h)).collect();
    let dir = manifests_dir(store_dir);
    let entries = fs::read_dir(&dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(data) = fs::read_to_string(&path) else { continue };
        let Ok(m) = serde_json::from_str::<PersistedManifest>(&data) else { continue };
        for f in m.files.values() {
            if let Some(&orig) = by_lower.get(&f.hash.to_ascii_lowercase()) {
                found.insert(orig.clone());
            }
        }
        if found.len() == wanted.len() {
            break; // every candidate already accounted for
        }
    }
    Ok(found)
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

/// Every manifest currently on disk under `store_dir`'s `manifests/` directory **that is fit to steer a
/// retention decision**, unordered (callers sort as needed — [`crate::snapshot_retention::thin`] sorts
/// internally). A missing `manifests/` dir (a store that has never captured) yields an empty list, not an
/// error. A file that fails to parse as a manifest (torn write, hand-edit) is skipped rather than failing
/// the whole enumeration — mirrors this module's other skip-on-error guardrails, and CPE-1861 extends
/// that same skip to three further ways a file can fail to describe itself (see the body).
///
/// **The invariant this function now carries (CPE-1861): every id it hands out is one
/// [`load_manifest`] will accept, and one [`prune`] can carry past its point of no return**
/// (with one store-level exception, below). Its only
/// caller, [`crate::snapshot_prune::apply`], feeds these ids straight to [`prune`] and propagates any
/// error with `?` — so an id [`prune`] refuses does not fail *one* manifest, it kills the whole
/// retention pass and every pass after it, until someone deletes the file by hand.
///
/// That makes this function's condition list a **mirror of [`prune`]'s fail-closed gates**, and the two
/// must be kept in lockstep: [`validate_manifest_id`], [`load_manifest`]'s parse, [`load_manifest`]'s
/// `file_count` cross-check, and CPE-1823's [`validate_blob_name`] on every entry's hash. The fourth was
/// missing until the round-3 security audit found a hand-edited hash still stalling the pass — if you
/// add a refusal to [`prune`] ahead of its `remove_file`, add its predicate here too, or you have
/// re-opened the permanent stall.
///
/// **One exception, and it is deliberate (CPE-1844).** [`prune`] now also reads [`load_store`] ahead of
/// its `remove_file`, and that refusal is **not** mirrored here — it cannot be, because it is a property
/// of the store's one `index.json` rather than of any manifest, so there is no per-file predicate to
/// skip on. An unreadable index therefore does stall the retention pass. That is the right direction:
/// the whole store's refcount ledger is unreadable, so no prune in it is sound. Before that hoist the
/// same condition was *destructive* instead of stalling — it cost one checkpoint per pass, measured in
/// [`prune`]'s own comment.
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
        // ---- CPE-1861: three skips, one rule ------------------------------------------------------
        //
        // **A file may take part in a decision to delete a checkpoint only if it is a record
        // `save_manifest` could actually have written at this name, and one `load_manifest` will
        // accept.** Everything below is that sentence, checked.
        //
        // Before this, the summary reported `m.id` — the manifest's own account of itself — while every
        // other entry point in the module (`load_manifest`, `restore`, `prune`, `manifest_snapshot`)
        // resolves by **filename**. A hand-edited inner `id` therefore steered a destructive decision
        // onto a different file, measured through `snapshot_prune::apply`:
        //
        // ```text
        // inner id -> a sibling's id     Ok(kept: [m3, m2, m3], pruned: [])   nothing thinned, ever
        // inner id -> "no-such-manifest" Err(".../no-such-manifest.json: cannot find the file")
        // ```
        //
        // The second kills the pass permanently: `snapshot_prune::apply` propagates `prune`'s error with
        // `?`, so no checkpoint in that store is ever thinned again.
        //
        // **Why this is a skip and not the obvious fix.** CPE-1847 fixed it by deriving the id from the
        // filename and had to revert: the filename is chosen by whoever put the file there, so trusting
        // it *invents a checkpoint* out of a stray file. Measured, both regressions:
        //
        // ```text
        // cp <id>.json <id>-backup.json   (Explorer copy/paste, a cloud-sync conflict copy, a backup
        //                                  script, a partial restore-from-backup)
        //   file_stem: apply pruned=[id-backup]  blobs=[]  restore(id)=Err(blob missing)  tree=[]
        // plant a..b.json
        //   file_stem: apply Err("a..b: not a valid manifest id")   -- the original wedge, relocated
        // ```
        //
        // Requiring the two to **agree** keeps the filename authoritative (it is what everything else
        // resolves by) without ever letting a name invent an identity: a copy, a liar and a crafted
        // name are all simply not checkpoints. Retention then thins the rest of the store normally,
        // which the duplicate-id collapse used to prevent — it stopped the whole pass, not just the
        // liar.
        //
        // **The cost, stated plainly — and corrected in review, because the first version of it was
        // wrong in the flattering direction.** A file that fails these checks is never reclaimed, and
        // *neither is any blob it names*. That is a leak, and it is the failure direction this module
        // chooses everywhere else (`prune`'s "leak over corruption", `capture`'s skip-on-error) — but
        // it is **not** "one small JSON file", which is what this comment used to say.
        //
        // The two halves of CPE-1861 interact to make it larger than either half implies. `prune`
        // refuses to free a blob any manifest file still names — *including a file this function
        // declines to list* — and because this function declines to list it, retention can never prune
        // it and so can never reach the "last namer" case that would free those blobs. Measured
        // through the real `snapshot_prune::apply`:
        //
        // ```text
        // 3 captures (m1 oldest, unique 12-byte blob) + an Explorer copy "<m1> - Copy.json"; hourly=2
        //   pass 1      pruned=[m1] kept=[m3, m2] bytes_freed=0; m1's unique blob present, index refs 1
        //   passes 2-4  pruned=[] freed=0, every time
        //   final       manifests/ = ["<m1> - Copy.json", "<m2>.json", "<m3>.json"], blob still present
        // ```
        //
        // So the residue is **that snapshot's stored content**, pinned for as long as the file sits in
        // the store, not a few hundred bytes. Reclaiming it needs the file removed by hand, or `prune`
        // driven by that id directly.
        //
        // **And for a recurring copier there is no bound at all** — the round-3 security audit measured
        // the case with no attacker in it, a sync client leaving one `<id> - Copy.json` per cycle:
        //
        // ```text
        // cycle  1: listed=1  manifest files= 2  blob files= 1
        // cycle  6: listed=1  manifest files= 7  blob files= 6
        // cycle 12: listed=1  manifest files=13  blob files=12    apply Ok, bytes_freed=0 every pass
        // ```
        //
        // Linear, unbounded, and **nothing surfaces it**: no `src/` code consumes
        // `RetentionApplyResult`, and `snapshot_schedule::snapshot_run_due` runs headless. So this is
        // "bounded per file, unbounded per copier", and calling it bounded full stop was the error.
        //
        // Traded knowingly and in that direction anyway, and the honest comparison is what settles it:
        // on `main` the *same* fixture gives a permanent `Err` wedge plus 23 phantom checkpoints. This
        // converts a loud wedge into a silent leak — better on every axis except discoverability, and a
        // leak is recoverable by deleting files where a store that can never be thinned is not. But
        // whoever reconsiders this should meet the real number rather than the reassuring one; that is
        // the whole value of pinning the decision here, and the first version of this comment defeated
        // it by being wrong.
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
        if m.id != stem
            // The name must also be one this module could hand out and resolve. Without this, a file
            // crafted to be self-*consistent* — `a..b.json` whose inner id is also `"a..b"` — would pass
            // the agreement check and then wedge the pass at `validate_manifest_id` inside `prune`,
            // which is the original harm with an extra step.
            || validate_manifest_id(stem).is_err()
            // CPE-1847 left this one open and recorded it: a manifest whose `file_count` contradicts its
            // `files` map is refused by `load_manifest`, and that refusal wedged the retention pass
            // exactly as a missing file did. Same predicate, applied here as a skip, so the pass never
            // names it. Shared with `load_manifest` so the two cannot drift.
            || file_count_disagreement(&m).is_some()
            // **The fourth gate, found by the round-3 security audit and missed by my own enumeration.**
            // `prune` has FOUR fail-closed refusals before its point of no return —
            // `validate_manifest_id`, `load_manifest`'s parse, `load_manifest`'s `file_count`, and
            // CPE-1823's `validate_blob_name` on every hash — and the three conditions above mirrored
            // only the first three. So one hand-edited `hash` in an otherwise *perfectly*
            // self-describing manifest still stalled the pass permanently, at the same tamper cost as
            // the shapes this ticket exists to remove:
            //
            // ```text
            // "hash": "not-a-hex-hash"   (inner id agrees with the stem, stem valid, file_count correct)
            //   pass 1 -> Err("…: refusing this manifest entry — its content hash \"not-a-hex-hash\" is
            //                  not a plain hex blob name")
            //   pass 2 -> Err(same)      the refusal fires BEFORE the manifest is deleted, so it recurs
            //                            on every scheduled pass, forever
            // ```
            //
            // Identical on `main`, so it was never a regression — but it is the same grammar, and my
            // enumeration wrote off `restore`/`prune`/`manifest_snapshot` as "unchanged" and never
            // walked `prune`'s own gate list. Mirrored here in the same shape as the other three, which
            // is cheaper than caveating the invariant this function's doc states. The hashes are already
            // deserialized by this point, so it is a map walk and no extra I/O.
            || m.files.values().any(|f| validate_blob_name(&f.hash).is_err())
        {
            continue;
        }
        out.push(ManifestSummary { id: m.id, created_ms: m.created_ms });
    }
    Ok(out)
}

/// The store's current total footprint in bytes — **measured from the blob files actually on disk, and
/// only those a manifest on disk still names**. Never read out of `index.json`. A store with no blobs,
/// or none that anything names, reads as `0`, not an error.
///
/// # CPE-1844 — this number authorises deletions, so it is measured rather than claimed
///
/// This was `load_store(store_dir)?.total_bytes()`: the sum of the `size` fields recorded in
/// `index.json`, an ordinary hand-editable JSON file in the store — exactly as editable as the manifest
/// CPE-1823 spent five rounds hardening, and receiving none of that validation. Its one consumer is
/// [`crate::snapshot_prune::apply`]'s byte cap, which prunes survivors **oldest-first until the figure
/// is under the cap or one checkpoint remains**. So one text edit turned a claim into real deletions of
/// the user's other checkpoints. Reproduced through `checkpoint_prune_apply` before this was changed,
/// on a store holding **45 bytes** against a **1,000,000-byte** cap, with a GFS policy that kept all
/// five:
///
/// ```text
/// index.json: every blob's "size" -> 1000000000       (one edit; no bytes written anywhere)
///   preview.total_bytes  45  ->  5000000000
///   CMD prune_apply  kept=[newest]  pruned=[4 others]  bytes_freed=4000000000
///   manifests left on disk = 1 of 5
/// ```
///
/// **Recomputed, not validated** — the lesson CPE-1823 paid for twice (its diff cap gated on the
/// manifest's claimed `size` and fell to a manifest claiming `size: 1`; the fix was to measure the real
/// file) and CPE-1861 reached independently (`manifests_naming` recomputes from the manifests on disk
/// because the refcount structurally cannot answer the question). There is nothing to sanity-check in a
/// recorded size: it *is* the claim, so any bound on it is another claim.
///
/// # The witness, and why measuring the directory was only half the move
///
/// **A first version of this counted every hex-named regular file in `blobs/`, and that was a lateral
/// move rather than a fix.** It swapped one hand-editable steering input for another: the new surface
/// trusted any correctly-named file, with nothing saying it was a blob *of anything*. Measured by the
/// security audit through the registered commands, against the very fixture this ticket opens with, and
/// reproduced here before this second version was written:
///
/// ```text
/// File::create("blobs/dead") + set_len(2_000_000_000)      -- no index.json edit at all
///   preview.total_bytes  45 -> 2000000045
///   CMD prune_apply  kept=1  pruned=4  bytes_freed=36  manifests left = 1 of 5
///   revert(oldest) -> Err(cannot find the file);  a.txt still reads "damaged"
/// ```
///
/// Byte-for-byte the pre-fix outcome. So the size question has a prior question — *whose content is
/// this?* — and the store already knows how to answer it: [`manifests_naming`], CPE-1861's
/// recompute-from-disk witness, which [`prune`] already calls before it frees anything. Only blobs some
/// manifest file on disk still names are this store's footprint. The planted file contributes **0**
/// because nothing names `dead`.
///
/// **This also fixes a regression the directory-sum version introduced, with no attacker in it.** An
/// *orphan* blob — a file on disk that no index entry and no manifest records — is [`capture`]'s own
/// documented partial-write residue (the `fs::copy` loop runs, then a crash before `save_store`), and
/// what a restore-from-backup with a stale index leaves. Nothing can ever reclaim it, because [`prune`]
/// only removes hashes named by the manifest it is pruning. Counting it toward a cap that is *enforced
/// by deleting checkpoints* is a category error, and it measured as one: a 4 MB crash residue deleted
/// four of five checkpoints, honestly reporting `bytes_freed = 36`, permanently (the pre-fix index
/// tamper self-heals when `save_store` rewrites honest sizes; an orphan file does not). Before the
/// directory-sum it contributed 0 and the pass did nothing. It contributes 0 again.
///
/// So the figure this returns is **reclaimable footprint** — the bytes that deleting checkpoints could
/// actually free — which is the only basis on which a delete-driven cap means anything.
///
/// # What it still costs an attacker, at full strength
///
/// Inflating this figure now requires a **matched pair**: a file in `blobs/` with a large *logical*
/// length under a plain hex name, **and** a manifest file naming that same hash. Neither half alone
/// does anything. That is a real increase over editing one number, and it is **not a barrier**:
///
/// - The cheap way to get a large logical length is **a hard link**, not a sparse file. It needs no
///   sparse API, no flag and no privilege — measured at `500,000,000` from a hard link named `beef`
///   pointing at a file outside the store, on an unelevated Windows session where a *symlink* required
///   elevation. An earlier draft of this note named only the sparse file, which understated it.
///   `fsutil sparse` / `truncate -s` work too, with near-zero allocation.
/// - [`manifests_naming`] is deliberately permissive — any parseable manifest counts, including a
///   planted one — so the manifest half is a file to write, not a gate to defeat.
///
/// **And the pair is cheaper and quieter than "two files" suggests — both halves measured by the
/// round-2 audit, and both understated by an earlier draft of this note.**
///
/// - **The witness manifest does not scale with the plant.** It is **122 bytes** for one hash, and one
///   manifest can name any number of them: a single 8 KB manifest validated **200** planted blobs,
///   i.e. 200 GB of claimed footprint. The cost of the second half is therefore essentially fixed no
///   matter how large the inflation, and "a matched pair" must not be read as "a pair per blob".
/// - **It is invisible and permanent.** Give the planted manifest an inner `id` that disagrees with
///   its filename and CPE-1861's rule makes [`list_manifests`] skip it — so it never appears in the
///   UI and is never a prune candidate — while [`manifests_naming`] still honours it, because that
///   function is deliberately the permissive one. The two CPE-1861 halves compose into a witness
///   nothing can see and nothing can remove. Measured: six manifest files in, four pruned, **two
///   left** — one real survivor plus the planted witness, which re-pins the one-survivor floor on
///   every future pass.
///
/// That is not a reason to tighten [`manifests_naming`] — see the shared-predicate hazard below,
/// where tightening it re-opens CPE-1861's blob-deletion hole for [`prune`]. It is a reason to state
/// the residual at its real size: an attacker who can already write into the store gets an arbitrary
/// footprint for about 8 KB, undetectably.
///
/// What the witness buys is that the tamper is now two coordinated files inside the store rather than
/// one number, and that every *accidental* shape (crash residue, stale-index restore, a stray file) is
/// worth zero. What it can still buy is bounded by CPE-1863, which owns the byte-cap loop's willingness
/// to run to its one-survivor floor when pruning makes no progress.
///
/// # The shared-predicate hazard, written down because the two callers want opposite things
///
/// [`prune`] asks [`manifests_naming`] *"would deleting these bytes destroy something that still points
/// at them?"* and is safe when the answer is **generous** — a blob wrongly reported as named is merely
/// leaked. This asks *"is this content the store accounts for?"* and is safe when the answer is
/// **stingy** — a blob wrongly reported as named inflates a figure that deletes checkpoints. Same
/// predicate, opposite failure directions. Tightening [`manifests_naming`] would help here and re-open
/// CPE-1861's blob-deletion hole there; loosening it does the reverse. **Do not tune it for one caller.**
/// If this site ever needs a stricter witness, give it its own and say why — that is exactly the
/// "one predicate, two meanings" drift CPE-1861 warns about, and it now has two callers standing on
/// opposite sides of it.
///
/// # Failure directions
///
/// **Deliberate under-counting, all of it safe.** Only regular files whose names pass
/// [`validate_blob_name`] are candidates. A directory, a symlink (a `DirEntry::metadata` does not follow
/// one, so it is not `is_file`), a sync client's `hash (1)` conflict copy, a stray `Thumbs.db` — none is
/// a content-addressed blob this store wrote, and letting an attacker-or-OS-chosen filename feed a
/// delete-driving total is the defect being fixed. An entry whose `metadata()` fails is skipped for the
/// same reason. Under-counting a cap prunes less, the only direction in which being wrong is not
/// destructive.
///
/// **Unreadable is an error, absent is a zero.** An absent `blobs/` is `Ok(0)` (never captured, or all
/// blobs freed); an unreadable one is `Err`. An absent `manifests/` is `Ok(0)` — nothing names anything,
/// so nothing is accounted for — while an **unreadable** `manifests/` is `Err` rather than being handed
/// to [`manifests_naming`], whose own conservative branch answers "all of them are named" and would
/// therefore over-count at precisely the site where over-counting deletes checkpoints.
/// [`crate::snapshot_prune::apply`] propagates either `Err` and deletes nothing.
///
/// **That pre-check is check-then-walk, and the window is measured reachable rather than
/// theoretical.** An earlier draft of this paragraph said the generous branch was unreachable
/// "except in a race", which reads as a dismissal. The round-2 audit ran it: with a thread renaming
/// `manifests/` away and back, out of 30,000 calls the fallback was hit and returned the full 2 GB
/// directory sum — the pre-witness behaviour. The bound stated here held exactly (at most the
/// directory sum), and it grants an attacker nothing they do not already have: anyone able to rename
/// `manifests/` can plant the 122-byte witness manifest instead, which is quieter and
/// deterministic. Filed separately rather than widened into this change; the close is a
/// [`manifests_naming`] variant that returns its `read_dir` failure instead of falling back, so this
/// site opens the directory once instead of twice.
///
/// # Cost, measured rather than assumed — and it went UP
///
/// **The driver is total manifest JSON bytes — manifests x files-per-tree — not blob count.** Two
/// earlier figures in this ticket were both internally correct and neither was plannable: the audit's
/// ~1.16x measured the witness-less directory sum, with no manifest parsing in it at all, and my own
/// 8.35x used 2,500 manifests, a shape retention can never produce. Measured instead on the shape the
/// shipped default `RetentionPolicy` (24/7/4/12) actually produces — **47 manifests, each listing a
/// whole tree, blobs shared between them**:
///
/// ```text
/// manifests x files   manifest JSON   witness    ratio to the index.json read it replaces
///     47 x     20          68 KB        105 us    2.6x
///     47 x    200         648 KB        343 us    3.7x
///     47 x  2,000         6.5 MB       3199 us    4.3x
///     47 x 10,000        32.9 MB      15541 us    5.1x
/// ```
///
/// **The number to plan against is ~16 ms**, for a 10,000-file tree under the default policy: about
/// 5x the index read it replaces, and just under the 18.3 ms this crate already accepts for
/// [`manifests_naming`]'s scan inside a single `prune`.
///
/// **Why it does not short-circuit.** [`manifests_naming`] stops early once every hash in `wanted` is
/// accounted for. [`prune`] asks about a handful of at-risk hashes, so it usually stops after a few
/// files; this asks about **every blob in the store**, so it reads every manifest, every time. That is
/// inherent to the question rather than an implementation choice.
///
/// **Not gated, and here is the arithmetic behind that.** ~16 ms is the *worst* row a default-policy
/// store can reach, and it needs a 10,000-file tree; an ordinary tree is the 105 us row. The one
/// caller that runs unattended, [`crate::snapshot_schedule::snapshot_run_due`], passes
/// `max_total_bytes: None`, so it pays this once per `preview` and never inside the byte-cap loop,
/// all of it inside `spawn_blocking`. If a store ever does grow past what retention permits, the fix
/// is the one CPE-1861 records for its own scan: hoist the manifest walk out so
/// [`crate::snapshot_prune::preview`] — which already parses every manifest via
/// [`list_manifests`] — shares one pass with this. Not weaken the witness.
///
/// Note this no longer opens `index.json` at all. Recorded rather than left to be discovered:
/// [`crate::snapshot_prune::preview`] and `apply` therefore now *disagree* about a store whose index is
/// corrupt — `preview` succeeds (it has no other reader of that file) while `apply` refuses inside
/// [`prune`]. Before this change both refused. Non-destructive in both directions, and the honest
/// preview is the more useful half, but it is a new asymmetry rather than an intended design.
///
/// **`manifests/` is opened exactly once (CPE-1867).** This used to probe readability with its own
/// `read_dir`, then hand the question to [`manifests_naming`], whose *own* `read_dir` did the actual
/// scan — two opens with a window between them for `manifests/` to change. It now calls
/// [`manifests_naming_strict`] directly: that function's single `read_dir` is both the readability check
/// and the scan, so there is no second open left for a race to land in. See that function's doc comment
/// for the measured race this closes.
pub fn store_total_bytes(store_dir: &str) -> Result<u64, String> {
    let store_path = Path::new(store_dir);
    let present = blob_files_on_disk(&blobs_dir(store_path))?;
    if present.is_empty() {
        return Ok(0);
    }
    let candidates: BTreeSet<String> = present.keys().cloned().collect();
    let named = match manifests_naming_strict(store_path, &candidates) {
        Ok(named) => named,
        // An absent `manifests/` means nothing is named yet (a store that has never captured, or one
        // whose only manifests were already pruned) — `Ok(0)`, same as `manifests_naming`'s caller-facing
        // behaviour elsewhere. Any other failure to read it is refused rather than handed to the generous
        // fallback: see the failure-directions note above for why "all of them are named" is the wrong
        // answer at this call site.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(format!("{}: {e}", manifests_dir(store_path).display())),
    };
    Ok(present
        .iter()
        .filter(|(hash, _)| named.contains(*hash))
        .fold(0u64, |acc, (_, len)| acc.saturating_add(*len)))
}

/// Every content-addressed blob file directly under `dir`, as `hash -> length`. The *name and type*
/// half of [`store_total_bytes`]'s rule; the witness half is applied by that function, which carries all
/// the reasoning. Absent directory is an empty map, not an error.
pub(crate) fn blob_files_on_disk(dir: &Path) -> Result<BTreeMap<String, u64>, String> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        // The one answer that means "no blobs here": nothing at that path. Everything else is a
        // directory we were told about and could not read, which is not a footprint of zero.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    };
    let mut out = BTreeMap::new();
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(|s| s.to_string()) else { continue };
        if validate_blob_name(&name).is_err() {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        out.insert(name, meta.len());
    }
    Ok(out)
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
    /// How many entries [`capture`] put in `files` — written by the capture, cross-checked by
    /// [`load_manifest`] (CPE-1847).
    ///
    /// **What it is for, stated precisely, because it is easy to over-claim.** A revert derives
    /// *destruction* from *absence*: a path in the tree but not in `files` is planned as a `Delete`. An
    /// absence is unfalsifiable — an entry someone removed and an entry that was never written are the
    /// same bytes — so removing entries from this map silently converts a revert into a delete of
    /// exactly those paths, measured at `applied: 4, survivors: ["f1.txt"]` for 4 of 5 entries removed.
    /// This field gives the map one falsifiable claim about itself, so the *cheapest* form of that
    /// tamper — deleting text and nothing else — is refused at load on every route rather than silently
    /// re-read as a smaller tree.
    ///
    /// **What it is NOT, stated at full strength because the first version of this record overstated
    /// it.** It lives in the same hand-editable JSON as the map it describes, so an attacker who edits
    /// both is not stopped — and, worse than that framing suggested, **editing both is not even
    /// required**. The field is `Option` with `#[serde(default)]` and the cross-check in
    /// [`load_manifest`] is gated on `Some`, because manifests written before this field existed must
    /// keep loading. Deleting the `"file_count"` line is therefore exactly as cheap as deleting entries
    /// from `files`, and the check simply does not run. Measured through the registered commands, with
    /// no number rewritten anywhere:
    ///
    /// ```text
    /// 4 of 5 entries removed + "file_count" key deleted, each leg on a FRESH five-file tree
    ///   checkpoint_revert_one(f3) -> Ok(RevertOutcome { applied: 1, skipped: [] })  survivors f1,f2,f4,f5
    ///   checkpoint_revert         -> Ok(RevertOutcome { applied: 4, skipped: [] })  survivors ["f1.txt"]
    /// ```
    ///
    /// **Three bypasses, all measured here, none of them requiring a number to be rewritten:**
    ///
    /// 1. **Delete the field** — as above. `#[serde(default)]` makes it `None` and the check is skipped.
    /// 2. **Null the field** — `"file_count": null` deserializes to `None` for an `Option`, so it is
    ///    exactly as good as deleting the line: `applied: 4, skipped: []`, four files destroyed.
    /// 3. **Replace entries instead of removing them** — the count stays *honest* and the check passes
    ///    while the map describes a different tree. Removing `f2..f5` and adding `z1..z4` all pointing at
    ///    `f1`'s blob, with `file_count: 5` untouched, measured
    ///    `Ok(RevertOutcome { applied: 8, skipped: [] })` — four user files deleted and four attacker-named
    ///    files created, survivors `["f1.txt", "z1.txt", "z2.txt", "z3.txt", "z4.txt"]`.
    ///
    /// And it is **size-shaped, not content-shaped**: substituting one entry's `hash` for another's is
    /// count-neutral and per-entry-guard-neutral. Pointing `f1.txt` at `f2.txt`'s blob measured
    /// `Ok(RevertOutcome { applied: 1, skipped: [] })` with `f1.txt`'s content on disk replaced by
    /// `f2`'s. That is within the manifest's trust model, but it shows the field gives **zero**
    /// protection against content substitution — only against a change in the number of entries.
    ///
    /// So this raises **no** cost against an attacker who knows the field exists; it catches a tamper
    /// that removes entries and leaves the count behind, and nothing else. It is a **consistency check
    /// on a record that may have been edited**, not a cost-raiser and not a guard — an earlier draft
    /// called it the former in three places, which was false and is corrected rather than softened.
    ///
    /// It is correspondingly never allowed to *authorise* anything:
    /// [`crate::revert_engine::execute_restore`]'s zero-entry stand-down does not consult it, and a
    /// manifest asserting `file_count: 0` unlocks no deletes. That is what keeps the Critical shape
    /// closed regardless of the above — `files: {}` with the `file_count` line deleted is still held
    /// back, because emptiness is read from the map itself.
    ///
    /// **On the keyed-signature ceiling, precisely.** The repo does hold signing keys — the updater
    /// pubkey in `src-tauri/tauri.conf.json` and `TAURI_SIGNING_PRIVATE_KEY` plus a catalog key in
    /// `.github/workflows/release.yml` — but every one of them is a **publisher** key held in CI
    /// secrets, signing centrally-produced artifacts. A checkpoint manifest is written on the user's own
    /// machine at capture time, so no publisher key can ever sign it. The honest ceiling is therefore
    /// "no key that helps against a **same-user** attacker", not "no key at all". Note the one vector
    /// where a key *would* be a real boundary and is not ruled out by that argument: for the
    /// store-synced-or-copied-from-another-machine case this module's own threat premise names, a
    /// **per-machine key in the OS keychain** would make a manifest from elsewhere detectable. Not
    /// attempted here; recorded so the ceiling is not read as lower than it is.
    ///
    /// `Option`, `#[serde(default)]`: manifests written before this field existed are absent-not-zero
    /// and must keep loading — refusing them would destroy access to every checkpoint already on disk,
    /// which is the over-tightening this ticket's sibling spent four rounds learning to avoid. That
    /// exemption is exactly what costs the check its teeth, above; the trade was made knowingly and in
    /// that direction. Every manifest written from now on carries the field.
    #[serde(default)]
    file_count: Option<usize>,
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
pub(crate) fn load_store(store_dir: &Path) -> Result<BlobStore, String> {
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
    let manifest: PersistedManifest =
        serde_json::from_str(&data).map_err(|e| format!("{}: {e}", path.display()))?;
    // CPE-1847 — the `files` map's declared size, checked against the map. See
    // `PersistedManifest::file_count` for what this does and does not claim.
    //
    // Here rather than at any one reader, for the reason `validate_manifest_id` above is here: this is
    // the single chokepoint every caller-supplied manifest id funnels through — `restore`, `prune`,
    // `manifest_snapshot` (and through it `checkpoint_store`'s preview / diff / revert / revert_one) —
    // so preview, diff and both revert routes refuse together instead of each deciding separately. That
    // matters more than usual for this shape: the cherry-revert route never consults the preview at all,
    // so a check that only guarded the preview would guard the route nobody is attacked through.
    //
    // A mismatch is `Err`, never a silent repair or a skip. No writer can produce one — `capture`
    // computes the count from the map it is writing in the same expression — so a disagreement means
    // the file was edited after it was written, and a store whose records contradict themselves is not
    // a store to quietly act on. The refusal names both numbers so the user can see what changed.
    //
    // **CPE-1847 recorded this as an unfixed wedge; CPE-1861 closed it from the other end.** The refusal
    // here made a tampered manifest unprunable, and `snapshot_prune::apply` propagates `prune`'s error
    // with `?`, so one such file stopped the whole retention pass for good. Pruning a manifest whose
    // file list is known wrong is worse (it releases the wrong blob refs), so the refusal stays — what
    // changed is that `list_manifests` now applies the **same** predicate as a *skip*, so retention
    // never names such a manifest in the first place and never reaches this line with it.
    if let Some((declared, actual)) = file_count_disagreement(&manifest) {
        return Err(format!(
            "{}: this manifest says it holds {declared} file{} but its file list has {actual} — it has \
             been edited since it was written, and a checkpoint that contradicts itself cannot be \
             used to decide what to restore or delete",
            path.display(),
            if declared == 1 { "" } else { "s" },
        ));
    }
    Ok(manifest)
}

/// Whether `manifest`'s declared `file_count` contradicts the map it describes — the one falsifiable
/// claim a manifest makes about itself (see [`PersistedManifest::file_count`]). `Some((declared,
/// actual))` on a disagreement.
///
/// Factored out so [`load_manifest`] (which **refuses**) and [`list_manifests`] (which **skips**) can
/// never drift apart. That pairing is what makes CPE-1861's invariant hold by construction: *every id
/// [`list_manifests`] hands out is one [`load_manifest`] will accept*, so a retention pass can no longer
/// be handed an id that then errors — the shape that wedged it permanently.
fn file_count_disagreement(manifest: &PersistedManifest) -> Option<(usize, usize)> {
    match manifest.file_count {
        Some(declared) if declared != manifest.files.len() => Some((declared, manifest.files.len())),
        _ => None,
    }
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
                file_count: Some(0),
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
                file_count: Some(0),
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

    /// **CPE-1846, both halves in one test, because they are the same trade.**
    ///
    /// Half 1 — restoring a snapshot **over a tree that still holds files** must keep working. This is
    /// the constraint that made CPE-1823 decline `copy_file_into_claimed_slot`: `create_new` refuses a
    /// name that already exists, so claiming the slot would turn every restore-over-a-tree into a pile
    /// of refusals. CPE-1846 takes only step 2 of that pattern (never follow a link), so this half must
    /// stay green, and it is asserted first so a fix that closes the link by refusing existing names
    /// fails here rather than looking like a pass.
    ///
    /// Half 2 — a link planted at an entry's final component, pointing at a bystander file **inside the
    /// same destination**, must not be written through. `confined_to` admits that link, and correctly:
    /// the resolved target *is* contained. Containment was never the question at the final component;
    /// atomicity was. Before this, the copy re-resolved the name and the bystander took the bytes.
    #[test]
    fn cpe_1846_restore_over_a_tree_overwrites_but_never_through_a_link_at_the_final_component() {
        let src = scratch("cpe1846-src");
        let store = scratch("cpe1846-store");
        let dest = scratch("cpe1846-dest");
        fs::write(src.join("a.txt"), b"CHECKPOINT A").unwrap();
        fs::write(src.join("b.txt"), b"CHECKPOINT B").unwrap();
        let out = capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::default())
            .expect("capture must succeed");

        // ---- half 1: the legitimate overwrite ----------------------------------------------------
        fs::write(dest.join("a.txt"), b"LIVE A").unwrap();
        fs::write(dest.join("b.txt"), b"LIVE B").unwrap();
        fs::write(dest.join("bystander.txt"), b"BYSTANDER").unwrap();
        restore(&store.to_string_lossy(), &out.manifest_id, &dest.to_string_lossy())
            .expect("restoring over a tree that still holds files must succeed");
        assert_eq!(fs::read(dest.join("a.txt")).ok().as_deref(), Some(&b"CHECKPOINT A"[..]));
        assert_eq!(fs::read(dest.join("b.txt")).ok().as_deref(), Some(&b"CHECKPOINT B"[..]));

        // ---- half 2: the planted link ------------------------------------------------------------
        fs::remove_file(dest.join("a.txt")).unwrap();
        if !crate::fsutil::make_file_link(&dest.join("bystander.txt"), &dest.join("a.txt")) {
            use std::io::Write;
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1846] SKIPPED the planted-link restore leg: no file symlink privilege on this \
                 machine. NOTHING on this run covered the final-component swap in `restore`."
            );
            return;
        }
        // Liveness, a second way: following the link must reach the bystander's bytes.
        assert_eq!(
            fs::read(dest.join("a.txt")).ok().as_deref(),
            Some(&b"BYSTANDER"[..]),
            "fixture is inert: the planted link does not lead to the bystander"
        );

        let outcome = restore(&store.to_string_lossy(), &out.manifest_id, &dest.to_string_lossy());

        // HARM FIRST, on the filesystem.
        assert_eq!(
            fs::read(dest.join("bystander.txt")).ok().as_deref(),
            Some(&b"BYSTANDER"[..]),
            "HARM: the restore wrote a manifest entry's bytes through a link at the final component, \
             onto a file the manifest never named — restore returned {outcome:?}"
        );
        assert!(
            fs::symlink_metadata(dest.join("a.txt")).is_ok_and(|m| m.file_type().is_symlink()),
            "the planted link must still be there, not replaced by bytes written over it"
        );
        assert!(outcome.is_err(), "writing through a link at the final component must be refused: {outcome:?}");
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
        let manifest = PersistedManifest {
            id: id.to_string(),
            created_ms: 0,
            file_count: Some(files.len()),
            files,
            skipped: Vec::new(),
        };
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

        // The entry carries a **directory** component on purpose: `create_dir_all(parent)` is the other
        // thing a manifest entry makes the process do, and it is attacker-chosen directory creation
        // anywhere on the volume. `files_under` enumerates files only, so it structurally cannot see a
        // stray directory — without the `planted-dir` assertion below, moving the three guards to *after*
        // `create_dir_all` passes every other test in this file while creating that directory outside the
        // restore folder.
        let rel = format!("../{}/planted-dir/pwned.txt", dir_name(&outside));
        let escape = outside.join("planted-dir").join("pwned.txt");
        let escape_dir = outside.join("planted-dir");
        plant_blob(&store, GOOD_HASH);
        plant_manifest(&store, "planted", &[(rel.as_str(), GOOD_HASH)]);

        let r = restore(&store.to_string_lossy(), "planted", &dest.to_string_lossy());

        assert!(
            !escape.exists(),
            "HARM: a `..` manifest path wrote {} — arbitrary file write from a hand-edited manifest",
            escape.display()
        );
        assert!(
            !escape_dir.exists(),
            "HARM: a `..` manifest path created the directory {} — every guard must run BEFORE the \
             entry's create_dir_all, not after it",
            escape_dir.display()
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

    /// **The pairing of guard 1 and guard 2 is not held together by platform luck.** Breaking guard 1
    /// alone reds nothing on Windows (guard 2 catches every shape the other tests stage there), so
    /// without this a Windows-only developer could delete `safe_target` and see local green. These two
    /// entries are refusable by guard 1 and by nothing else on any platform: `confined_to(dest, dest)`
    /// answers *true* for the empty path — the root is contained in itself, by design — and `a//b`
    /// resolves to the perfectly-contained `dest/a/b`, so the filesystem has no objection to either.
    /// What is wrong with them is structural, and only the textual guard can say so.
    #[test]
    fn cpe_1823_only_the_textual_guard_can_refuse_an_empty_or_doubled_separator_path() {
        for rel in ["", "a//b"] {
            let store = scratch("cpe1823-textual-store");
            let dest = scratch("cpe1823-textual-dest");
            plant_blob(&store, GOOD_HASH);
            plant_manifest(&store, "planted", &[(rel, GOOD_HASH)]);

            let r = restore(&store.to_string_lossy(), "planted", &dest.to_string_lossy());

            assert!(files_under(&dest).is_empty(), "a refused entry must write nothing at all");
            let err = r.unwrap_err();
            // **The discriminating assertion, and the empty leg had none until now.** With guard 1
            // broken, `rel = ""` gives `target == dest`, `fs::copy` fails because dest is a directory,
            // `files_under` is empty and the error starts `C:\…` — so both assertions above passed and
            // all the power sat in the `a//b` leg. The refusal must come from the *textual* guard,
            // named, and for an empty path it must still identify its subject (the first cut emitted a
            // headless `": refusing this manifest entry — empty path"`). This also pins `refusal()`'s
            // empty-name handling, which nothing else covered.
            assert!(
                err.contains("empty path"),
                "the refusal must be the textual guard's, not an incidental I/O failure: {err:?}"
            );
            if rel.is_empty() {
                assert!(
                    err.contains("(a manifest entry with an empty path)"),
                    "an empty path must still be named as the subject of its own refusal: {err:?}"
                );
            }
        }
    }

    /// **Blocker 3 — an entry that is neither refused nor written, returning `Ok(())`.** Both shapes were
    /// observed doing exactly that. They are Windows-only *hazards* (and the guard is `cfg!(windows)`),
    /// but the aliasing one is the more serious: three distinct entries collapse onto one file, so two
    /// vanish and the survivor holds content the user never asked for at that name — a restore that
    /// reports success and hands back a tree that is wrong in a way nothing announces.
    #[cfg(windows)]
    #[test]
    fn cpe_1823_a_win32_aliasing_or_device_name_entry_is_refused_not_silently_swallowed() {
        // A reserved device name: `fs::copy` "succeeds" into the null device and nothing lands on disk.
        {
            let store = scratch("cpe1823-nul-store");
            let dest = scratch("cpe1823-nul-dest");
            plant_blob(&store, GOOD_HASH);
            plant_manifest(&store, "planted", &[("sub/NUL", GOOD_HASH)]);

            let r = restore(&store.to_string_lossy(), "planted", &dest.to_string_lossy());

            let err = r.expect_err(
                "a device-name entry writes nothing to disk, so returning Ok reports a restore that did \
                 not happen",
            );
            assert!(err.contains("sub/NUL"), "the refusal must name the entry, got: {err}");
            assert!(!dest.join("sub").exists(), "and it must refuse before creating the parent");
        }
        // Trailing-space aliasing: three entries, one surviving file, content from whichever won.
        {
            let store = scratch("cpe1823-alias-store");
            let dest = scratch("cpe1823-alias-dest");
            plant_blob(&store, GOOD_HASH);
            plant_manifest(
                &store,
                "planted",
                &[("a.txt", GOOD_HASH), ("a.txt ", GOOD_HASH), ("a.txt.", GOOD_HASH)],
            );

            let r = restore(&store.to_string_lossy(), "planted", &dest.to_string_lossy());

            // Harm before `Result`, which is this file's convention and which the first cut of this leg
            // skipped: the point of the aliasing shape is not that an entry is lost, it is that THREE
            // entries collapse onto ONE file holding whichever content was copied last. Asserting only
            // the `Err` never observes that, so it would pass against a fix that refused for some
            // unrelated reason while leaving the collapse in place.
            let landed = files_under(&dest);
            assert!(
                landed.is_empty(),
                "HARM: three distinct manifest entries collapsed onto {landed:?} — two vanished and the \
                 survivor holds content the user never asked for at that name"
            );
            let err = r.expect_err("entries that Win32 collapses onto one name must be refused");
            assert!(
                err.contains("a.txt ") || err.contains("a.txt."),
                "the refusal must name the aliasing entry, got: {err}"
            );
        }
    }


    /// **CPE-1823 round 5 — two entries, one file, and it returned `Ok`.** `A.txt` and `a.txt` are legal
    /// names on every platform, so no per-entry rule may refuse either; on a case-folding volume they are
    /// one file. Restoring a manifest holding both wrote one file with whichever content was copied last,
    /// lost the other entry entirely, and reported success. A restore is *believed*, so a manifest entry
    /// that silently never arrived is the exact failure this module's doc forbids.
    ///
    /// Both legs are staged because they are caught by different halves of the fix: a destination that
    /// **already holds** the file is visible to pass 1 (total abort, nothing written at all), while a
    /// **fresh** destination is invisible until the first entry exists and is caught in pass 2, before
    /// the second copy rather than after it.
    #[cfg(windows)]
    #[test]
    fn cpe_1823_two_manifest_entries_that_resolve_to_one_file_are_refused() {
        const OTHER_HASH: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

        // Leg 1 — fresh destination. Nothing resolves until the first entry lands.
        {
            let store = scratch("cpe1823-collapse-store");
            let dest = scratch("cpe1823-collapse-dest");
            plant_blob(&store, GOOD_HASH);
            plant_blob(&store, OTHER_HASH);
            plant_manifest(&store, "planted", &[("A.txt", GOOD_HASH), ("a.txt", OTHER_HASH)]);

            // Fixture is inert on a case-SENSITIVE volume: there the two entries really are two files
            // and there is nothing to collapse, so this leg would certify nothing.
            let probe = scratch("cpe1823-collapse-probe");
            fs::write(probe.join("A.txt"), b"probe").unwrap();
            assert_eq!(
                fs::read(probe.join("a.txt")).ok().as_deref(),
                Some(&b"probe"[..]),
                "fixture is inert: this volume must fold case, or the two entries are two files and \
                 this test certifies nothing"
            );
            let _ = fs::remove_dir_all(&probe);

            let r = restore(&store.to_string_lossy(), "planted", &dest.to_string_lossy());

            // Harm before `Result`, this file's convention: the point is not that an entry errors, it is
            // that TWO entries became ONE file and the caller was told everything arrived.
            let landed = files_under(&dest);
            assert!(
                !(r.is_ok() && landed.len() < 2),
                "HARM: a manifest with two entries restored as {landed:?} and returned {r:?} — one \
                 captured file silently never arrived"
            );
            let err = r.expect_err("two entries addressing one file cannot be reported as a full restore");
            assert!(
                err.contains("A.txt") || err.contains("a.txt"),
                "the refusal must name the colliding entry, got: {err}"
            );
        }

        // Leg 2 — a destination that already holds the file, which pass 1 can see: the abort is total.
        {
            const LIVE: &[u8] = b"already in the destination";
            let store = scratch("cpe1823-collapse2-store");
            let dest = scratch("cpe1823-collapse2-dest");
            plant_blob(&store, GOOD_HASH);
            plant_blob(&store, OTHER_HASH);
            plant_manifest(&store, "planted", &[("A.txt", GOOD_HASH), ("a.txt", OTHER_HASH)]);
            fs::write(dest.join("a.txt"), LIVE).unwrap();

            let r = restore(&store.to_string_lossy(), "planted", &dest.to_string_lossy());

            assert_eq!(
                fs::read(dest.join("a.txt")).ok().as_deref(),
                Some(LIVE),
                "HARM: a manifest refused for a collision still overwrote the destination — the abort \
                 must be total when pass 1 can see the collision. Result was {r:?}"
            );
            let err = r.expect_err("a collision pass 1 can see must abort before anything is written");
            assert!(err.contains("nothing was written"), "and it must say so, got: {err}");
        }
    }

    /// **The abort is total: a refused entry anywhere means nothing is written anywhere.** The good entry
    /// sorts *first* in the manifest's `BTreeMap`, so the refuse-as-you-go loop this replaced wrote it to
    /// disk and only then hit the bad one — leaving a half-restored tree the user has no way to tell from
    /// a complete one. Ordering is the whole test: with the names reversed it would pass against either
    /// implementation.
    ///
    /// Also pins that one run names *every* offending entry rather than surfacing them one re-run at a
    /// time, which for a planted manifest is the difference between seeing an attack and seeing a typo.
    #[test]
    fn cpe_1823_one_refused_entry_means_nothing_at_all_is_written() {
        let store = scratch("cpe1823-total-store");
        let dest = scratch("cpe1823-total-dest");
        plant_blob(&store, GOOD_HASH);
        plant_manifest(
            &store,
            "planted",
            &[
                ("aaa-good.txt", GOOD_HASH),   // sorts first — written by a refuse-as-you-go loop
                ("bbb-bad.txt", "../nope"),    // sorts second — the refusal
                ("ccc-also-bad.txt", "zzzz!"), // and a second refusal, to prove one run names both
            ],
        );

        let err = restore(&store.to_string_lossy(), "planted", &dest.to_string_lossy())
            .expect_err("a manifest with refused entries must not restore");

        assert!(
            files_under(&dest).is_empty(),
            "HARM: entries sorting before the refused one were already written — the abort must be \
             total, not partial: {:?}",
            files_under(&dest)
        );
        assert!(err.contains("bbb-bad.txt"), "the first refusal must be named: {err}");
        assert!(
            err.contains("ccc-also-bad.txt"),
            "and so must the second — one run, every offending entry: {err}"
        );
    }

    /// **CPE-1823 round 4 — the window the pre-pass opened, and the reason pass 2 re-judges.** When
    /// `restore` validated everything and then wrote everything, `confined_to`'s verdict for a late entry
    /// was reached before every earlier entry had been written. The attacker does not race blindly: the
    /// first file appearing under the destination *is* the signal that pass 1 has finished and its
    /// verdicts are stale.
    ///
    /// Staged the cheap way — the swap only has to **create** a junction at a name pass 1 already
    /// blessed. `zzz` does not exist during pass 1, so `confined_to` walks up to the destination and
    /// passes it; creating the junction afterwards means pass 2's `create_dir_all` finds a directory
    /// already there and the copy follows it straight out of the folder. Nothing has to be deleted or
    /// won by timing beyond "after the first write, before the last".
    ///
    /// The `aaa/` entries exist to make the write run long enough to interleave with, and `zzz` sorts
    /// last in the manifest's `BTreeMap` so it is written after them.
    #[test]
    fn cpe_1823_a_component_swapped_after_validation_is_caught_before_its_own_write() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let store = scratch("cpe1823-toctou-store");
        let dest = scratch("cpe1823-toctou-dest");
        let outside = scratch("cpe1823-toctou-outside");
        plant_blob(&store, GOOD_HASH);

        let mut entries: Vec<(String, &str)> =
            (0..400).map(|i| (format!("aaa/f{i:04}.txt"), GOOD_HASH)).collect();
        entries.push(("zzz/target.txt".to_string(), GOOD_HASH));
        let pairs: Vec<(&str, &str)> = entries.iter().map(|(p, h)| (p.as_str(), *h)).collect();
        plant_manifest(&store, "planted", &pairs);

        let stop = Arc::new(AtomicBool::new(false));
        let attacker = {
            let (dest_p, outside_p, stop) = (dest.to_path_buf(), outside.to_path_buf(), Arc::clone(&stop));
            std::thread::spawn(move || {
                // Wait for proof that pass 1 is over: a byte on disk. Then plant the junction at the
                // already-blessed name.
                while !stop.load(Ordering::Relaxed) {
                    if dest_p.join("aaa").join("f0000.txt").exists() {
                        return crate::fsutil::make_dir_link(&outside_p, &dest_p.join("zzz"));
                    }
                    std::thread::yield_now();
                }
                false
            })
        };

        let r = restore(&store.to_string_lossy(), "planted", &dest.to_string_lossy());
        stop.store(true, Ordering::Relaxed);
        let swap_landed = attacker.join().unwrap_or(false);

        if !swap_landed {
            crate::skip_notice!(
                "[CPE-1823] SKIPPED the pre-pass TOCTOU leg: the swap never landed (no link privilege, \
                 or the restore finished first). NOTHING on this run covered a component swapped \
                 between validation and its own write."
            );
            return;
        }
        // The harm is a file OUTSIDE the destination. Asserting on `r` alone would miss it entirely:
        // the vulnerable version returned `Ok(())`.
        assert!(
            !outside.join("target.txt").exists(),
            "HARM: a component swapped after pass 1 blessed it took the write outside the restore \
             folder — pass 2 must re-judge each entry immediately before its own copy (restore returned \
             {r:?})"
        );
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
        // The reported regression was in **this** function — `restore` aborts the whole manifest where
        // `revert_engine` skips one file — so the colon name is exercised here and not only through
        // `execute_restore`. The shared predicate covers both today; if `restore` ever stops going
        // through `safe_segments`, only this assertion notices. `\` too: also a legal Unix byte, also
        // refused by the ungated rule, and on macOS a Finder name containing `/` is stored as `:`.
        #[cfg(unix)]
        for legal_here in ["2026-08-21 10:30 notes.txt", r"Q1\Q2 report.txt", "NUL", "notes. "] {
            fs::write(src.join(legal_here), b"ordinary on this platform").unwrap();
        }

        let outcome =
            capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED).unwrap();
        restore(&store.to_string_lossy(), &outcome.manifest_id, &dest.to_string_lossy())
            .expect("a legal filename that merely starts with `..` must still restore");
        assert_eq!(fs::read(dest.join("sub").join("..evil")).unwrap(), b"perfectly ordinary bytes");
        #[cfg(unix)]
        for legal_here in ["2026-08-21 10:30 notes.txt", r"Q1\Q2 report.txt", "NUL", "notes. "] {
            assert_eq!(
                fs::read(dest.join(legal_here)).unwrap(),
                b"ordinary on this platform",
                "{legal_here:?} is an ordinary filename here — a Windows-only rule must not refuse it"
            );
        }
    }

    /// **CPE-1847 — the `file_count` cross-check, and the three things it must NOT refuse.**
    ///
    /// `load_manifest` is the single chokepoint every caller-supplied manifest id funnels through, so
    /// this one check covers `restore`, `prune`, and `manifest_snapshot` (and through it the command
    /// layer's preview, diff, revert and cherry-revert). The over-tightening legs are the point of the
    /// test as much as the refusal leg is: a manifest written before this field existed, and a genuine
    /// capture of an empty folder, must both keep loading, or the fix destroys access to real
    /// checkpoints — the failure mode CPE-1823 spent four rounds learning to avoid.
    #[test]
    fn cpe_1847_load_manifest_refuses_a_file_list_that_contradicts_its_own_count() {
        let store = scratch("cpe1847-count");
        let dir = manifests_dir(&store);
        fs::create_dir_all(&dir).unwrap();
        let one_entry = r#""a.txt": { "hash": "abcd", "size": 4 }"#;
        let plant = |id: &str, count_field: &str, files: &str| {
            fs::write(
                manifest_path(&store, id),
                format!(r#"{{ "id": "{id}", "created_ms": 0, "files": {{ {files} }}, "skipped": []{count_field} }}"#),
            )
            .unwrap();
        };

        // The tamper: four of five entries removed, count left alone. Same refusal for a map emptied
        // entirely — both are "the list contradicts the number written beside it".
        plant("tampered", r#", "file_count": 5"#, one_entry);
        let err = load_manifest(&store, "tampered")
            .err()
            .expect("a file list that contradicts its own count must be refused, not read as a smaller tree");
        assert!(err.contains("5 file"), "the refusal must name the count claimed: {err}");
        assert!(err.contains("has 1"), "and what the list actually holds: {err}");

        // A manifest written before this field existed: absent is not zero. Refusing these would make
        // every checkpoint already on disk unusable.
        plant("legacy", "", one_entry);
        assert_eq!(
            load_manifest(&store, "legacy").expect("a legacy manifest must still load").files.len(),
            1,
            "a manifest with no `file_count` at all is exempt, not treated as claiming zero"
        );

        // A genuine capture of an empty folder: `files: {}` with the count positively asserting 0.
        plant("genuinely-empty", r#", "file_count": 0"#, "");
        assert!(
            load_manifest(&store, "genuinely-empty").unwrap().files.is_empty(),
            "a real capture of an empty directory must load — this is the constraint that makes a \
             naive refusal of `files: {{}}` wrong"
        );

        // And the ordinary agreeing case, so the check is not simply refusing everything.
        plant("agreeing", r#", "file_count": 1"#, one_entry);
        assert!(load_manifest(&store, "agreeing").is_ok());

        let _ = fs::remove_dir_all(&store);
    }

    /// **CPE-1847 — the round trip a naive refusal would break, at the `capture`/`restore` level.**
    /// Capturing an empty directory is legal, produces `files: {}` (the very shape the attack produces),
    /// and must still restore. `checkpoint_store`'s command-level test pins the revert half.
    #[test]
    fn cpe_1847_capture_of_an_empty_directory_round_trips_through_restore() {
        let src = scratch("cpe1847-empty-src");
        let store = scratch("cpe1847-empty-store");
        let dest = scratch("cpe1847-empty-dest");

        let outcome =
            capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED).unwrap();
        assert_eq!(outcome.new_blobs, 0, "fixture is inert: an empty capture must store no blobs");
        let manifest = load_manifest(&store, &outcome.manifest_id).unwrap();
        assert!(
            manifest.files.is_empty(),
            "fixture is inert: the capture must have produced the zero-entry shape this test is about"
        );
        assert_eq!(
            manifest.file_count,
            Some(0),
            "the capture must positively assert zero rather than leave the count absent"
        );

        restore(&store.to_string_lossy(), &outcome.manifest_id, &dest.to_string_lossy())
            .expect("a genuine capture of an empty directory must still restore");
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 0, "and restore to an empty tree");

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

    /// CPE-1861, the `prune` half, driven directly rather than through retention: **one manifest, one
    /// refcount — a release must not drop a blob another manifest still names.**
    ///
    /// The refcount alone cannot enforce that, and this test measures why before it measures anything
    /// else: a manifest file that arrives by *copy* rather than by capture adds a namer without ever
    /// bumping a ref, so `index.json` reads `refs: 1` while two files on disk name the blob. Pruning
    /// either one therefore used to take the count to zero and delete content the other still needed.
    #[test]
    fn cpe_1861_prune_never_frees_a_blob_another_manifest_file_still_names() {
        let src = scratch("shared-src");
        let store = scratch("shared-store");
        fs::write(src.join("a.txt"), b"irreplaceable").unwrap();
        let first =
            capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED).unwrap();
        let id = first.manifest_id.clone();

        let mdir = manifests_dir(&store);
        fs::copy(mdir.join(format!("{id}.json")), mdir.join(format!("{id}-backup.json"))).unwrap();

        // FIXTURE LIVENESS — the copy is a real second namer of the same blob, and the index disagrees
        // with the manifest set. If either of these stopped being true this test would be measuring
        // nothing.
        let hash = load_manifest(&store, &format!("{id}-backup")).unwrap().files["a.txt"].hash.clone();
        assert!(blobs_dir(&store).join(&hash).exists(), "LIVE: the shared blob is missing");
        assert_eq!(
            load_store(&store).unwrap().get(&hash).unwrap().refs,
            1,
            "LIVE: the refcount/namer drift this guard exists for is absent from the fixture"
        );
        assert_eq!(files_under(&mdir).len(), 2, "LIVE: the copy is not in manifests/");

        // Prune the copy. The original still names the blob, so the blob must survive.
        prune(&store.to_string_lossy(), &format!("{id}-backup")).unwrap();
        assert!(
            blobs_dir(&store).join(&hash).exists(),
            "HARM: pruning one of two manifest files naming a blob deleted the blob"
        );
        assert!(load_store(&store).unwrap().contains(&hash), "the index still holds the blob");
        let dest = scratch("shared-dest");
        restore(&store.to_string_lossy(), &id, &dest.to_string_lossy())
            .unwrap_or_else(|e| panic!("HARM: the surviving checkpoint can no longer restore: {e}"));
        assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"irreplaceable");

        // Self-correcting, not a permanent leak: prune the last namer and the blob is freed as usual.
        let freed = prune(&store.to_string_lossy(), &id).unwrap();
        assert!(freed > 0, "the last namer's prune still frees the blob");
        assert!(!blobs_dir(&store).join(&hash).exists(), "the blob file is gone once nothing names it");

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
        let _ = fs::remove_dir_all(&dest);
    }

    /// CPE-1864 — **the witness compared hashes byte-for-byte, so a survivor spelling its hash in a
    /// different case than the victim's own manifest was invisible to it.** Same shape as
    /// `cpe_1861_prune_never_frees_a_blob_another_manifest_file_still_names` (a second manifest file
    /// naming the same content is a legitimate namer this witness must see) — except here the second
    /// namer's hash is the SAME content hash, merely re-spelled uppercase, which `validate_blob_name`
    /// accepts and which Windows/macOS resolve to the identical `blobs/<hash>` file. The bug this guards
    /// needs no case-insensitive filesystem to fail: the blob is a real `fs::remove_file`, gone entirely,
    /// not merely mis-spelled — so this test is deterministic on every platform in the 3-OS matrix.
    ///
    /// **What the user loses today, stated plainly (per the ticket).** This is a false "blob missing"
    /// from the witness's point of view — the dangerous direction, because it is the direction that
    /// deletes content a live checkpoint still names. `prune` reports success and frees bytes; the harm
    /// is silent until the survivor's own restore later fails to find a file that should still be there.
    /// The opposite mistake — a false "blob present" — is not this bug's shape: nothing here makes an
    /// absent blob look present, so it never causes `restore` to hand back content that silently
    /// resolves to the wrong bytes.
    #[test]
    fn cpe_1864_a_survivor_spelling_its_hash_uppercase_still_protects_the_shared_blob() {
        let src = scratch("1864-src");
        let store = scratch("1864-store");
        fs::write(src.join("a.txt"), b"irreplaceable, byte for byte").unwrap();
        let victim =
            capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED).unwrap();
        let victim_id = victim.manifest_id.clone();

        // The survivor: a second manifest file naming the SAME content, but with its hash re-spelled
        // uppercase — "editing the survivor's own manifest", the ticket's own threat model, and the same
        // plant-a-copy shape CPE-1861's own `-backup` test uses for a legitimate second namer.
        let mdir = manifests_dir(&store);
        let survivor_id = format!("{victim_id}-upper");
        let doc_path = mdir.join(format!("{victim_id}.json"));
        let mut doc: serde_json::Value = serde_json::from_str(&fs::read_to_string(&doc_path).unwrap()).unwrap();
        doc["id"] = serde_json::json!(survivor_id);
        let lower_hash = doc["files"]["a.txt"]["hash"].as_str().unwrap().to_string();
        let upper_hash = lower_hash.to_ascii_uppercase();
        doc["files"]["a.txt"]["hash"] = serde_json::json!(upper_hash);
        fs::write(mdir.join(format!("{survivor_id}.json")), serde_json::to_string_pretty(&doc).unwrap())
            .unwrap();

        // FIXTURE LIVENESS — the uppercase spelling really reached disk, really differs from the lowercase
        // spelling the actual blob file is named after, and the blob really is on disk under that
        // lowercase name.
        assert_ne!(upper_hash, lower_hash, "LIVE: sanity — the case flip must actually change the string");
        assert!(
            upper_hash.chars().any(|c| c.is_ascii_uppercase()),
            "LIVE: the plant is not actually uppercase"
        );
        let on_disk = load_manifest(&store, &survivor_id).unwrap().files["a.txt"].hash.clone();
        assert_eq!(on_disk, upper_hash, "LIVE: the uppercase spelling did not survive the round trip to disk");
        assert!(blobs_dir(&store).join(&lower_hash).exists(), "LIVE: the shared blob must exist on disk");

        // Prune the victim. The survivor's own manifest still names the exact same content — merely
        // spelled differently — so the blob must survive.
        prune(&store.to_string_lossy(), &victim_id).unwrap();

        assert!(
            blobs_dir(&store).join(&lower_hash).exists(),
            "HARM: pruning the victim deleted a blob the survivor's manifest still names (uppercase \
             spelling) — a false \"blob missing\" from the witness deleted content a live checkpoint \
             still needs"
        );

        let dest = scratch("1864-dest");
        restore(&store.to_string_lossy(), &survivor_id, &dest.to_string_lossy())
            .unwrap_or_else(|e| panic!("HARM: the surviving checkpoint can no longer restore: {e}"));
        assert_eq!(
            fs::read(dest.join("a.txt")).unwrap(),
            b"irreplaceable, byte for byte",
            "HARM: the survivor did not restore byte-for-byte"
        );

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
        let _ = fs::remove_dir_all(&dest);
    }

    /// CPE-1861's invariant, as a property over a store carrying every tamper shape at once: **every id
    /// `list_manifests` hands out is one `load_manifest` accepts.** Its only caller feeds those ids
    /// straight to `prune` and propagates the error with `?`, so an id that cannot be loaded does not
    /// fail one manifest — it stops the retention pass for that store permanently.
    #[test]
    fn cpe_1861_every_id_list_manifests_hands_out_can_still_be_loaded() {
        let src = scratch("inv-src");
        let store = scratch("inv-store");
        fs::write(src.join("a.txt"), b"v1").unwrap();
        fs::write(src.join("b.txt"), b"b").unwrap();
        let good1 =
            capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED).unwrap()
                .manifest_id;
        fs::write(src.join("a.txt"), b"v2").unwrap();
        let good2 =
            capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED).unwrap()
                .manifest_id;

        let mdir = manifests_dir(&store);
        let edit = |name: &str, f: &dyn Fn(&mut serde_json::Value)| {
            let p = mdir.join(format!("{name}.json"));
            let mut doc: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
            f(&mut doc);
            fs::write(&p, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
        };
        let copy = |from: &str, to: &str| {
            fs::copy(mdir.join(format!("{from}.json")), mdir.join(format!("{to}.json"))).unwrap();
        };

        copy(&good1, &format!("{good1}-backup")); // a plain duplicate
        copy(&good1, "liar"); // filename/id disagreement
        copy(&good1, "a..b"); // crafted filename, inner id still the original's
        // A crafted name that AGREES with its own inner id — the shape the agreement rule alone would
        // let through and `validate_manifest_id` catches.
        copy(&good1, "a..c");
        edit("a..c", &|d| d["id"] = serde_json::json!("a..c"));
        copy(&good1, "countliar");
        edit("countliar", &|d| {
            d["id"] = serde_json::json!("countliar");
            d["files"].as_object_mut().unwrap().remove("b.txt");
        });
        fs::write(mdir.join("garbage.json"), b"{ not json").unwrap();

        // FIXTURE LIVENESS — every planted file is on disk, and each one is genuinely a way to break the
        // invariant: fed to `load_manifest` by name, each planted id fails.
        let planted = files_under(&mdir);
        assert_eq!(planted.len(), 8, "LIVE: planted files missing: {planted:?}");
        // The wedge shapes: unloadable at their own name, so listing one kills the pass outright.
        for bad in ["a..b", "a..c", "countliar", "garbage"] {
            assert!(
                load_manifest(&store, bad).is_err(),
                "LIVE: {bad} was supposed to be unloadable, so listing it would wedge the pass"
            );
        }
        // The steering shapes: perfectly loadable at their own name, but each reports a *foreign* id, so
        // listing one puts a second, phantom entry for `good1` in front of the retention policy.
        for steerer in ["liar", &format!("{good1}-backup")] {
            let doc: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(mdir.join(format!("{steerer}.json"))).unwrap())
                    .unwrap();
            assert_eq!(
                doc["id"].as_str().unwrap(),
                good1,
                "LIVE: {steerer} does not actually claim another manifest's id"
            );
        }

        let listed: BTreeSet<String> =
            list_manifests(&store.to_string_lossy()).unwrap().into_iter().map(|m| m.id).collect();
        assert_eq!(
            listed,
            [good1.clone(), good2.clone()].into_iter().collect::<BTreeSet<_>>(),
            "only self-describing manifests may steer a retention decision"
        );
        for id in &listed {
            load_manifest(&store, id)
                .unwrap_or_else(|e| panic!("INVARIANT BROKEN: list_manifests handed out {id}: {e}"));
        }

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    // ---- CPE-1844: index.json is hand-editable, and it steers deletions ----------------------------

    /// Sum every blob file's real length under `store`'s `blobs/`, independently of the code under test.
    fn real_blob_bytes(store: &std::path::Path) -> u64 {
        fs::read_dir(blobs_dir(store))
            .map(|rd| {
                rd.flatten()
                    .filter_map(|e| e.metadata().ok())
                    .filter(|m| m.is_file())
                    .map(|m| m.len())
                    .sum()
            })
            .unwrap_or(0)
    }

    /// Rewrite every `size` recorded in `store`'s `index.json` to `size`, and read it back so a tamper
    /// that failed to land can never be mistaken for a guard that worked. Returns the total it now claims.
    fn set_index_sizes(store: &std::path::Path, size: u64) -> u64 {
        let p = index_path(store);
        let mut doc: serde_json::Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        for (_h, m) in doc["blobs"].as_object_mut().unwrap().iter_mut() {
            m["size"] = serde_json::json!(size);
        }
        fs::write(&p, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
        let back: serde_json::Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        let blobs = back["blobs"].as_object().unwrap();
        assert!(
            blobs.values().all(|m| m["size"].as_u64() == Some(size)),
            "LIVE: the index.json size tamper never landed on disk"
        );
        blobs.values().map(|m| m["size"].as_u64().unwrap()).sum()
    }

    /// **CPE-1844, the headline.** `store_total_bytes` is the figure
    /// [`crate::snapshot_prune::apply`]'s byte cap deletes checkpoints against. It used to be the sum of
    /// the `size` fields in `index.json` — an ordinary hand-editable file in the store. This asserts the
    /// figure now describes the blob files that are actually there.
    #[test]
    fn cpe_1844_store_total_bytes_measures_the_blobs_instead_of_believing_the_index() {
        let src = scratch("1844-total-src");
        let store = scratch("1844-total-store");
        fs::write(src.join("a.txt"), b"twenty-nine bytes of content!").unwrap();
        capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED).unwrap();

        let real = real_blob_bytes(&store);
        assert!(real > 0, "LIVE: the fixture stored no blobs at all");
        assert_eq!(store_total_bytes(&store.to_string_lossy()).unwrap(), real, "the honest store");

        // The tamper: one text edit, no bytes written anywhere on disk.
        let claimed = set_index_sizes(&store, 1_000_000_000);
        assert_eq!(
            load_store(&store).unwrap().total_bytes(),
            claimed,
            "LIVE: the tamper did not reach the record the old figure was read from"
        );
        assert_ne!(claimed, real, "LIVE: the fixture's claim and its reality are the same number");

        assert_eq!(
            store_total_bytes(&store.to_string_lossy()).unwrap(),
            real,
            "HARM: a hand-edited index.json still dictates the store's reported footprint"
        );

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    /// The measurement's deliberate exclusions, each of which under-counts — the only direction in which
    /// being wrong about a delete-driving total is not destructive. A name that is not a plain hex blob
    /// address is not something this store wrote, and letting an attacker-or-OS-chosen filename
    /// contribute to the figure is the defect being fixed one layer down.
    #[test]
    fn cpe_1844_only_hex_named_regular_files_count_towards_the_footprint() {
        let store = scratch("1844-measure");
        let blobs = blobs_dir(&store);
        fs::create_dir_all(&blobs).unwrap();

        // An absent `blobs/` is a store that has never captured, not an error.
        let empty = scratch("1844-measure-empty");
        assert!(blob_files_on_disk(&blobs_dir(&empty)).unwrap().is_empty(), "an absent blobs/ is empty, not Err");

        fs::write(blobs.join("deadbeef"), b"0123456789").unwrap(); // 10 bytes, counted
        fs::write(blobs.join("BEEF00"), b"01234").unwrap(); // 5 bytes; uppercase hex is still hex
        fs::write(blobs.join("Thumbs.db"), vec![0u8; 5_000]).unwrap(); // not a blob name
        fs::write(blobs.join("deadbeef (1)"), vec![0u8; 5_000]).unwrap(); // a sync client's conflict copy
        fs::write(blobs.join("not-hex-at-all"), vec![0u8; 5_000]).unwrap();
        fs::create_dir_all(blobs.join("cafe")).unwrap(); // a hex-named DIRECTORY

        // LIVE: every decoy really is on disk and really is large, so a measurement that counted them
        // would be visible as a number in the thousands rather than 15.
        assert_eq!(blobs.join("Thumbs.db").metadata().unwrap().len(), 5_000, "LIVE: decoy is not staged");
        assert_eq!(blobs.join("deadbeef (1)").metadata().unwrap().len(), 5_000, "LIVE: decoy is not staged");
        assert!(blobs.join("cafe").is_dir(), "LIVE: the hex-named directory is not staged");

        assert_eq!(
            blob_files_on_disk(&blobs).unwrap().values().sum::<u64>(),
            15,
            "HARM: something that is not a content-addressed blob file contributed to the footprint"
        );

        let _ = fs::remove_dir_all(&store);
        let _ = fs::remove_dir_all(&empty);
    }

    /// **CPE-1844 — `prune` reports the bytes it removed, not the bytes `index.json` claimed.** The
    /// figure is surfaced as `RetentionApplyResult::bytes_freed` after a destructive operation, so it is
    /// held to this tree's rule that a number the user reads must describe what happened
    /// (CPE-1803/1804/1805/1816). On the fixture that opens this ticket the old figure read
    /// `bytes_freed = 4000000000` for a store holding 45 bytes.
    #[test]
    fn cpe_1844_prune_reports_the_bytes_it_actually_removed() {
        let src = scratch("1844-freed-src");
        let store = scratch("1844-freed-store");
        fs::write(src.join("a.txt"), b"twenty-nine bytes of content!").unwrap();
        let id = capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED)
            .unwrap()
            .manifest_id;
        let real = real_blob_bytes(&store);
        assert!(real > 0, "LIVE: nothing was stored, so nothing can be freed");

        let claimed = set_index_sizes(&store, 1_000_000_000);
        assert_eq!(
            load_store(&store).unwrap().total_bytes(),
            claimed,
            "LIVE: the tamper did not reach the record `release` sums"
        );

        let freed = prune(&store.to_string_lossy(), &id).unwrap();
        assert_eq!(real_blob_bytes(&store), 0, "LIVE: the prune removed no blob file, so it freed nothing");
        assert_ne!(freed, claimed, "HARM: prune reported index.json's claim as bytes freed");
        assert_eq!(freed, real, "HARM: prune's freed figure does not describe the files it removed");

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    /// **CPE-1844 — an index entry is a claim about content, and `capture` used to act on it by writing
    /// nothing.** Dedup asks `BlobStore::contains(hash)`, i.e. "does `index.json` list this hash", and a
    /// `reused` verdict meant the blob's bytes were never written. An entry whose blob file is gone —
    /// `prune`'s own documented leak-over-corruption residue, or a partial restore-from-backup of a
    /// store — therefore made the next capture of that content store none of it.
    #[test]
    fn cpe_1844_a_reused_blob_whose_file_is_missing_is_written_not_assumed() {
        let src = scratch("1844-dedup-src");
        let store = scratch("1844-dedup-store");
        fs::write(src.join("a.txt"), b"the user only copy").unwrap();
        capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED).unwrap();

        let hash = load_store(&store).unwrap().iter().next().unwrap().0.clone();
        fs::remove_file(blobs_dir(&store).join(&hash)).unwrap();

        // LIVE: the torn state really is staged — the file is gone and the index still claims it.
        assert!(!blobs_dir(&store).join(&hash).exists(), "LIVE: the blob file is still on disk");
        assert!(load_store(&store).unwrap().contains(&hash), "LIVE: the index no longer claims the blob");

        let second =
            capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED).unwrap();
        // LIVE: the dedup verdict really was "reused" — this fixture reaches the sink under test, and
        // does not merely re-store the content as new.
        assert_eq!(second.reused_blobs, 1, "LIVE: the capture did not take the dedup path");
        assert_eq!(second.new_blobs, 0, "LIVE: the capture treated the blob as new, not reused");

        let dest = scratch("1844-dedup-dest");
        restore(&store.to_string_lossy(), &second.manifest_id, &dest.to_string_lossy())
            .unwrap_or_else(|e| panic!("HARM: the checkpoint just taken cannot restore its content: {e}"));
        assert_eq!(
            fs::read(dest.join("a.txt")).unwrap(),
            b"the user only copy",
            "HARM: a checkpoint reported as created holds none of the file's content"
        );
        assert!(blobs_dir(&store).join(&hash).exists(), "the missing blob was repaired on disk");

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
        let _ = fs::remove_dir_all(&dest);
    }

    /// **CPE-1844 enumeration — the `refs` field, both directions, pinned rather than argued.**
    /// Deflating a shared blob's refcount is the direction that could destroy content, and CPE-1861's
    /// recomputed `manifests_naming` witness is what stops it; inflating it can only leak. Neither can
    /// cost a surviving checkpoint its content, and this is the test that says so.
    #[test]
    fn cpe_1844_a_hand_edited_refcount_cannot_cost_a_surviving_checkpoint_its_content() {
        let src = scratch("1844-refs-src");
        let store = scratch("1844-refs-store");
        fs::write(src.join("a.txt"), b"irreplaceable").unwrap();
        let first = capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED)
            .unwrap()
            .manifest_id;
        fs::write(src.join("b.txt"), b"second capture, same a.txt").unwrap();
        let second = capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED)
            .unwrap()
            .manifest_id;
        let hash = load_manifest(&store, &first).unwrap().files["a.txt"].hash.clone();
        assert_eq!(load_store(&store).unwrap().get(&hash).unwrap().refs, 2, "LIVE: the blob is not shared");

        // The tamper: the shared blob's refcount now says one snapshot holds it, when two do.
        let p = index_path(&store);
        let mut doc: serde_json::Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        doc["blobs"][&hash]["refs"] = serde_json::json!(1);
        fs::write(&p, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
        assert_eq!(
            load_store(&store).unwrap().get(&hash).unwrap().refs,
            1,
            "LIVE: the refcount tamper never landed on disk"
        );

        prune(&store.to_string_lossy(), &first).unwrap();
        let dest = scratch("1844-refs-dest");
        restore(&store.to_string_lossy(), &second, &dest.to_string_lossy())
            .unwrap_or_else(|e| panic!("HARM: a deflated refcount cost the surviving checkpoint: {e}"));
        assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"irreplaceable");

        // The other direction: an inflated refcount can only keep a blob alive past its last namer — a
        // space leak, never a delete. Asserted so the enumeration's "harmless" verdict is measured.
        let leak_src = scratch("1844-refs-leak-src");
        let leak_store = scratch("1844-refs-leak-store");
        fs::write(leak_src.join("a.txt"), b"solo").unwrap();
        let solo =
            capture(&leak_src.to_string_lossy(), &leak_store.to_string_lossy(), &CaptureBudget::UNLIMITED)
                .unwrap()
                .manifest_id;
        let solo_hash = load_manifest(&leak_store, &solo).unwrap().files["a.txt"].hash.clone();
        let lp = index_path(&leak_store);
        let mut ldoc: serde_json::Value = serde_json::from_str(&fs::read_to_string(&lp).unwrap()).unwrap();
        ldoc["blobs"][&solo_hash]["refs"] = serde_json::json!(9);
        fs::write(&lp, serde_json::to_string_pretty(&ldoc).unwrap()).unwrap();
        assert_eq!(load_store(&leak_store).unwrap().get(&solo_hash).unwrap().refs, 9, "LIVE: refs tamper");
        prune(&leak_store.to_string_lossy(), &solo).unwrap();
        assert!(
            blobs_dir(&leak_store).join(&solo_hash).exists(),
            "an inflated refcount leaks the blob — that is the recorded, non-destructive direction"
        );

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
        let _ = fs::remove_dir_all(&dest);
        let _ = fs::remove_dir_all(&leak_src);
        let _ = fs::remove_dir_all(&leak_store);
    }

    /// **CPE-1844 round 2 — the witness.** Measuring `blobs/` closed the `index.json` steering input and
    /// opened another: any correctly-named file in `blobs/` steered the same deletions, with nothing
    /// saying it was a blob of anything. The security audit measured the pre-witness fix at
    /// `preview.total_bytes 45 -> 2000000045` and `pruned 4 of 5` from `File::create("blobs/dead") +
    /// set_len(2_000_000_000)` — byte-for-byte the outcome this ticket opens with, with no `index.json`
    /// edit at all. Only content a manifest on disk still names is this store's footprint.
    #[test]
    fn cpe_1844_a_blob_file_no_manifest_names_is_not_the_stores_footprint() {
        let src = scratch("1844-witness-src");
        let store = scratch("1844-witness-store");
        fs::write(src.join("a.txt"), b"twenty-nine bytes of content!").unwrap();
        capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED).unwrap();
        let honest = store_total_bytes(&store.to_string_lossy()).unwrap();
        assert!(honest > 0, "LIVE: the fixture stored no blobs at all");

        // Three shapes, one rule. `planted` is the audit's; `orphan` is `capture`'s own partial-write
        // residue and needs no attacker; `hardlink` is the cheapest way to a huge logical length —
        // no sparse API, no flag, no privilege.
        let planted = std::fs::File::create(blobs_dir(&store).join("dead")).unwrap();
        planted.set_len(2_000_000_000).unwrap();
        drop(planted);
        fs::write(blobs_dir(&store).join("00ff00ff"), vec![0u8; 4_000_000]).unwrap();
        let victim = store.join("..").join("cpe1844-witness-victim.bin");
        let vf = std::fs::File::create(&victim).unwrap();
        vf.set_len(500_000_000).unwrap();
        drop(vf);
        let linked = std::fs::hard_link(&victim, blobs_dir(&store).join("beef")).is_ok();

        // FIXTURE LIVENESS — every plant is on disk, is huge, and passes the name/type filter, so the
        // only thing that can be excluding it is the witness. Asserted through `blob_files_on_disk`,
        // which is the stage *before* the guard under test.
        let on_disk = blob_files_on_disk(&blobs_dir(&store)).unwrap();
        assert_eq!(on_disk.get("dead").copied(), Some(2_000_000_000), "LIVE: the planted file is inert");
        assert_eq!(on_disk.get("00ff00ff").copied(), Some(4_000_000), "LIVE: the orphan blob is inert");
        if linked {
            assert_eq!(on_disk.get("beef").copied(), Some(500_000_000), "LIVE: the hard link is inert");
        }
        assert!(
            on_disk.values().sum::<u64>() > 2_000_000_000,
            "LIVE: the pre-witness measurement would not even have been inflated by this fixture"
        );

        assert_eq!(
            store_total_bytes(&store.to_string_lossy()).unwrap(),
            honest,
            "HARM: a blob file no manifest names steers the store's footprint"
        );

        let _ = fs::remove_file(&victim);
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    /// The witness's own over-tightening pin, and it is the CPE-1861 interaction. A blob named **only by
    /// a duplicate manifest file** — the copy CPE-1861 refuses to *list* — is still real, still on disk,
    /// and still reclaimable by deleting that file, so it must keep counting. A witness that asked
    /// `list_manifests` instead of `manifests_naming` would drop it, and would then under-report a store
    /// that a recurring copier grows without bound.
    #[test]
    fn cpe_1844_a_blob_named_only_by_a_duplicate_manifest_still_counts() {
        let src = scratch("1844-dupname-src");
        let store = scratch("1844-dupname-store");
        fs::write(src.join("a.txt"), b"irreplaceable").unwrap();
        let id = capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED)
            .unwrap()
            .manifest_id;
        let full = store_total_bytes(&store.to_string_lossy()).unwrap();
        assert!(full > 0);

        let mdir = manifests_dir(&store);
        fs::copy(mdir.join(format!("{id}.json")), mdir.join(format!("{id} - Copy.json"))).unwrap();
        assert!(mdir.join(format!("{id} - Copy.json")).exists(), "LIVE: the duplicate manifest is not on disk");
        prune(&store.to_string_lossy(), &id).unwrap();
        // LIVE: without the copy this prune would have freed the blob, so the fixture is the
        // unlisted-namer case and not an ordinary surviving checkpoint.
        assert_eq!(
            blob_files_on_disk(&blobs_dir(&store)).unwrap().len(),
            1,
            "LIVE: the duplicate never protected the blob, so there is nothing here to count"
        );

        // LIVE: the original is gone, the copy is the only namer left, and the blob survived (CPE-1861).
        assert!(!mdir.join(format!("{id}.json")).exists(), "LIVE: the original manifest is still there");
        assert_eq!(
            list_manifests(&store.to_string_lossy()).unwrap().len(),
            0,
            "LIVE: the copy is being listed, so this is not the unlisted-namer case"
        );
        let hash = blob_files_on_disk(&blobs_dir(&store)).unwrap().keys().next().unwrap().clone();
        assert!(blobs_dir(&store).join(&hash).exists(), "LIVE: CPE-1861 did not protect the blob");

        assert_eq!(
            store_total_bytes(&store.to_string_lossy()).unwrap(),
            full,
            "HARM: content a manifest file still names stopped counting, so the store under-reports"
        );

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    /// **CPE-1844 — an unreadable witness must refuse, not wave everything through.**
    /// [`manifests_naming`]'s own failure branch answers "all of them are still named", which is the
    /// right answer for [`prune`] (keep the blobs, leak rather than destroy) and exactly the wrong one
    /// here, where "all of them are named" is the maximal footprint and the maximal footprint deletes
    /// checkpoints. Same predicate, opposite safe directions — so `store_total_bytes` checks the
    /// directory is readable itself rather than handing the question over.
    ///
    /// Staged the way `classify_store_index` stages its own untestable-by-permissions case: a
    /// **non-directory** at `manifests/`. `read_dir` fails with something that is not `NotFound`, on
    /// every platform, without any ACL or `chmod` that Windows and Unix disagree about.
    #[test]
    fn cpe_1844_an_unreadable_manifests_dir_refuses_instead_of_counting_everything() {
        let src = scratch("1844-nowitness-src");
        let store = scratch("1844-nowitness-store");
        fs::write(src.join("a.txt"), b"twenty-nine bytes of content!").unwrap();
        capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED).unwrap();

        // A big plant, so "count everything" is visibly different from "count what is named".
        let planted = std::fs::File::create(blobs_dir(&store).join("dead")).unwrap();
        planted.set_len(2_000_000_000).unwrap();
        drop(planted);

        let mdir = manifests_dir(&store);
        fs::remove_dir_all(&mdir).unwrap();
        fs::write(&mdir, b"not a directory").unwrap();

        // FIXTURE LIVENESS — the witness really is unreadable in a way that is not "absent", the plant
        // really is on disk, and the directory sum really would be the inflated figure.
        let err = fs::read_dir(&mdir).unwrap_err();
        assert_ne!(err.kind(), std::io::ErrorKind::NotFound, "LIVE: read_dir says absent, not unreadable");
        let dir_sum: u64 = blob_files_on_disk(&blobs_dir(&store)).unwrap().values().sum();
        assert!(dir_sum > 2_000_000_000, "LIVE: the plant never reached the measurement — {dir_sum}");

        let got = store_total_bytes(&store.to_string_lossy());

        // HARM FIRST: whatever happens, the one answer that must never come back is the maximal one.
        assert_ne!(
            got.as_ref().ok().copied(),
            Some(dir_sum),
            "HARM: an unreadable witness counted every blob file, which is the figure that deletes \
             checkpoints"
        );
        assert!(got.is_err(), "an unreadable witness must refuse: {got:?}");

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    /// CPE-1867 — **the racing-rename harness, against the single-open fix.** The round-2 audit ran this
    /// same shape against the old two-open `store_total_bytes` (a probing `read_dir`, then
    /// `manifests_naming`'s own separate `read_dir`): a thread renaming `manifests/` away and back landed
    /// in the gap between the two calls and hit the generous fallback — `worst Ok value under the race =
    /// 2000000000, errs = 0` out of 30,000 calls, the full directory sum including an unnamed decoy blob.
    ///
    /// The fix removes the second open rather than narrowing the gap: `store_total_bytes` now calls
    /// [`manifests_naming_strict`] directly, and that function's own single `read_dir` is the ONLY place
    /// this call asks whether `manifests/` is readable. There is no second call left for a racer to land
    /// in — either the one `read_dir` sees the directory (scan proceeds normally) or it doesn't (`Ok(0)`
    /// or `Err`, both handled explicitly by `store_total_bytes`), never "readable, then not, so count
    /// everything".
    ///
    /// A big, unnamed decoy blob makes the harm visible (same shape as
    /// `cpe_1844_an_unreadable_manifests_dir_refuses_instead_of_counting_everything`): if the race is ever
    /// won by the generous path, the reported total jumps by the decoy's size. `Ok(0)` results — proof
    /// the rename actually landed mid-call and was handled honestly rather than generously — are counted
    /// as the run's liveness evidence instead of a separate single-shot race.
    #[test]
    fn cpe_1867_a_racing_rename_of_manifests_never_returns_the_generous_directory_sum() {
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        use std::sync::Arc;

        let src = scratch("1867-race-src");
        let store = scratch("1867-race-store");
        fs::write(src.join("a.txt"), b"twenty-nine bytes of content!").unwrap();
        capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED).unwrap();
        let honest = store_total_bytes(&store.to_string_lossy()).unwrap();
        assert!(honest > 0, "LIVE: capture must have produced a named, counted blob");

        // A decoy nothing names, big enough that "counted everything" cannot be mistaken for the honest
        // figure.
        let decoy = std::fs::File::create(blobs_dir(&store).join("d".repeat(64))).unwrap();
        decoy.set_len(2_000_000_000).unwrap();
        drop(decoy);
        let dir_sum: u64 = blob_files_on_disk(&blobs_dir(&store)).unwrap().values().sum();
        assert!(dir_sum > 2_000_000_000, "LIVE: the decoy never reached the measurement — {dir_sum}");
        assert_ne!(dir_sum, honest, "LIVE: the decoy must not already be named");

        let mdir = manifests_dir(&store);
        let away = store.join("manifests-away");

        let stop = Arc::new(AtomicBool::new(false));
        let toggles = Arc::new(AtomicU64::new(0));
        let racer = {
            let (mdir, away, stop, toggles) =
                (mdir.clone(), away.clone(), Arc::clone(&stop), Arc::clone(&toggles));
            std::thread::spawn(move || {
                // Test-only race harness, not a production write: `disallowed_methods` exists to keep an
                // unguarded destructive rename out of real call paths, and this rename's whole job here IS
                // to be destructive-and-unguarded, on a throwaway scratch directory, to prove the
                // production code survives it.
                #[allow(clippy::disallowed_methods)]
                while !stop.load(Ordering::Relaxed) {
                    if fs::rename(&mdir, &away).is_ok() {
                        toggles.fetch_add(1, Ordering::Relaxed);
                        let _ = fs::rename(&away, &mdir);
                    }
                }
            })
        };

        const ITERS: u32 = 20_000;
        let mut worst_ok = 0u64;
        let mut errs = 0u32;
        let mut zeros = 0u32; // Ok(0): the race landed and was handled honestly, not generously
        for _ in 0..ITERS {
            match store_total_bytes(&store.to_string_lossy()) {
                Ok(v) => {
                    worst_ok = worst_ok.max(v);
                    if v == 0 {
                        zeros += 1;
                    }
                    assert_ne!(
                        v,
                        dir_sum,
                        "HARM: a racing rename of manifests/ made store_total_bytes count every blob \
                         file (the generous fallback), which is the figure that deletes checkpoints"
                    );
                }
                Err(_) => errs += 1,
            }
        }
        stop.store(true, Ordering::Relaxed);
        racer.join().unwrap();

        eprintln!(
            "[CPE-1867] {ITERS} calls under a racing rename: worst Ok = {worst_ok}, errs = {errs}, \
             Ok(0)-from-a-landed-race = {zeros}, toggles observed = {}",
            toggles.load(Ordering::Relaxed)
        );

        assert!(
            toggles.load(Ordering::Relaxed) > 0,
            "LIVE: the racer thread never won a single rename — the test never actually raced anything"
        );
        assert!(
            zeros > 0,
            "LIVE: not one call observed manifests/ actually missing — the race pressure never reached \
             store_total_bytes, so the harm assertion above proves nothing"
        );
        assert_eq!(
            worst_ok, honest,
            "the single-open fix must never inflate the total beyond the honest, fully-witnessed figure"
        );

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }
}
