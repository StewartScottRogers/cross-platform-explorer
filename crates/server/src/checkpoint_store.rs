//! Per-watched-root checkpoint store (CPE-1123, epic CPE-732 "checkpoint & rollback").
//!
//! The command-layer glue that finally wires the checkpoint/rollback **engine** — already built and
//! cargo-tested but until now bound to zero commands — into a live, per-root store. It owns nothing
//! algorithmic: every capture, plan, drift, and revert is delegated to the existing modules
//! ([`crate::snapshot_capture`], [`crate::restore_plan`], [`crate::revert_safety`],
//! [`crate::revert_engine`]); this module only decides *where* a root's snapshots live on disk and keeps
//! a small human-facing index of the checkpoints taken there.
//!
//! ## On-disk layout (mirrors the [`crate::audit_journal`] pattern)
//! Everything lives under the app-data dir, keyed per watched root:
//! ```text
//! <app_data>/checkpoints/<root_key>/
//!   blobs/                  content-addressed blobs        ] owned by
//!   index.json              the persisted BlobStore        ] snapshot_capture
//!   manifests/<id>.json     one per capture                ]
//!   checkpoints.json        THIS module's checkpoint index (append-only JSON-lines)
//! ```
//! `<root_key>` is the SHA-256 of the absolute root path, so two different roots never collide and no
//! user path ever leaks into a directory name (the same "safe single segment" concern
//! [`crate::audit_journal::record`] solves by sanitising a session id).
//!
//! ## Why `checkpoints.json` is JSON-**lines** and tolerant-read
//! Like the audit journal it is append-only: [`checkpoint_create`] writes exactly one flushed line per
//! checkpoint, and [`read_checkpoints`] reads them back **skipping any malformed line** — a torn/partial
//! trailing write (or a hand-edit) degrades to "ignore that one record", never a crash or a lost index,
//! exactly as [`crate::audit_journal::read_session`] degrades. Missing file → empty list. Newest-first on
//! read for the UI.
//!
//! Std + serde only — no new dependencies, not feature-gated (like the engines it drives). The revert
//! path preserves the engine's skip-on-error guarantee: a single unreadable/locked file is reported in
//! [`RevertOutcome::skipped`], not fatal to the rest of the revert.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::audit_journal;
use crate::ctx::ServerCtx;
use crate::fsutil::to_epoch_ms;
use crate::model::OpResult;
use crate::restore_plan::{self, plan_restore, summarize_plan, RestoreAction};
use crate::revert_attribution;
use crate::revert_engine::{execute_restore, safe_target, RestoreReport};
use crate::revert_safety::{classify_plan, summarize_conflicts};
use crate::snapshot::{CaptureBudget, SkipReason};
use crate::snapshot_capture;
use crate::snapshot_prune::{self, RetentionApplyResult, RetentionPreview};
use crate::snapshot_retention::RetentionPolicy;

/// One recorded checkpoint's index entry: which manifest holds its captured tree, the user's label, and
/// when it was taken (epoch ms). This is the row appended to / read from `checkpoints.json`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Checkpoint {
    /// The [`snapshot_capture`] manifest id this checkpoint captured into — pass to preview/revert.
    pub manifest_id: String,
    /// The user-supplied label ("before refactor", …). May be empty.
    pub label: String,
    /// When the checkpoint was taken, epoch milliseconds.
    pub ts: u64,
}

/// A best-effort pre-write checkpoint that was **attempted and failed** — the row appended to /
/// read from `checkpoint_failures.json` (CPE-1600). Every caller of the "checkpoint before an
/// irreversible batch" pattern (Batch Media, Metadata Studio, Declutter, Similar Images) takes a
/// best-effort [`checkpoint_create`] before it overwrites/moves originals; a checkpoint failure never
/// blocks that write (it's a bonus safety net, not a gate), but until now the ONLY trace of "I tried to
/// protect this folder and couldn't" was a `showNotice` banner that auto-dismisses in ~5s. This gives it
/// a durable home next to the checkpoints that did succeed.
///
/// Deliberately **not** a [`Checkpoint`]: it carries no `manifest_id` because nothing was captured, so
/// it can never be passed to `checkpoint_preview_revert`/`checkpoint_revert`/`checkpoint_revert_one` —
/// there is nothing there to revert to. The frontend keys off this distinct shape to render a failed
/// attempt with no restore affordances at all, so it can never be mistaken for a real restore point.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CheckpointFailure {
    /// What prompted the attempt ("Before batch media overwrite", "Before metadata edit", …) — the
    /// same label convention [`Checkpoint::label`] uses for a successful checkpoint's caller, so the
    /// panel reads "tried to protect this folder before X and couldn't".
    pub operation: String,
    /// Why the attempt failed — the error string from the failed `checkpoint_create` call, verbatim.
    pub reason: String,
    /// When the attempt was made, epoch milliseconds.
    pub ts: u64,
}

/// A file the capture left out (oversize / over budget), surfaced so the caller can warn a checkpoint is
/// incomplete rather than silently dropping content. The string form of [`crate::snapshot::SkipReason`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SkippedInfo {
    pub path: String,
    pub size: u64,
    /// `"oversize"` (larger than the per-file cap) or `"budget"` (would breach the store cap).
    pub reason: String,
}

/// Outcome of [`checkpoint_create`]: the index entry plus the dedup accounting from the capture.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CheckpointCreated {
    /// The checkpoint just recorded (also now the newest entry [`checkpoint_list`] returns).
    pub checkpoint: Checkpoint,
    /// Blobs newly written to the store by this capture.
    pub new_blobs: u32,
    /// Blobs already present and reused (the dedup win — nothing written).
    pub reused_blobs: u32,
    /// Bytes this capture added to the store's footprint.
    pub added_bytes: u64,
    /// Files whose content was skipped (never silently dropped).
    pub skipped: Vec<SkippedInfo>,
}

/// A preview of what reverting to a checkpoint would do: the restore-plan summary plus a **drift** report.
///
/// Drift = files that differ from the checkpoint but that this layer cannot attribute to the watched
/// agent. [`checkpoint_preview_revert`] takes an optional session id to resolve this:
/// - `session: None` (conservative default) — the classifier runs against an **empty** "agent-touched"
///   set, so every diverging path is surfaced as drift ("changed outside since checkpoint").
/// - `session: Some(sess)` (attribution-aware) — [`crate::revert_attribution::agent_touched`] computes
///   the set of paths `sess` itself mutated at/after the checkpoint, from the durable audit journal;
///   only paths **outside** that set are surfaced as drift, since the agent's own changes are expected,
///   not a warning-worthy conflict.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RevertPreview {
    /// Files present in the checkpoint but gone now → would be recreated.
    pub creates: u32,
    /// Files present in both but changed → would be overwritten with the checkpoint content.
    pub overwrites: u32,
    /// Files created since the checkpoint → would be deleted.
    pub deletes: u32,
    /// Bytes the revert would write back (Create + Overwrite checkpoint sizes; deletes free space).
    pub bytes_written: u64,
    /// Total paths the revert would touch (`creates + overwrites + deletes`).
    pub total: u32,
    /// How many of those touched paths are drift (see the struct doc).
    pub drift_count: u32,
    /// The drifted paths, in plan order, so the UI can list them.
    pub drift_paths: Vec<String>,
}

/// Outcome of a revert — an `OpResult`-style summary: how many actions applied, plus the ones that were
/// skipped (each carried as a failed [`OpResult`] with the skip reason), preserving the engine's
/// skip-on-error guarantee.
#[derive(Debug, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RevertOutcome {
    /// Actions applied successfully.
    pub applied: u32,
    /// Actions skipped (missing blob, locked/permission-denied, path-safety refusal): `ok:false` +
    /// `error` = the reason. Never fatal to the rest of the revert.
    pub skipped: Vec<OpResult>,
}

impl RevertOutcome {
    fn from_report(report: RestoreReport) -> Self {
        Self {
            applied: report.applied as u32,
            skipped: report
                .skipped
                .into_iter()
                .map(|(path, error)| OpResult { path, ok: false, error })
                .collect(),
        }
    }
}

// ---- disk location ---------------------------------------------------------------------------------

/// The SHA-256 (lowercase hex) of a root path — a collision-free, path-safe single directory segment for
/// that root's store, so no user path is ever reflected into a directory name.
fn root_key(root: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(root.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

/// The base checkpoints directory under the app-data dir (sibling of the audit journal's `audit/`).
fn checkpoints_base(ctx: &dyn ServerCtx) -> Result<PathBuf, String> {
    Ok(ctx.app_data_dir()?.join("checkpoints"))
}

/// The base audit-journal directory under the app-data dir — mirrors `checkpoints_base` above, and
/// must resolve to the exact same path the app adapter writes to. That writer is `audit_dir` in
/// `src-tauri/src/lib.rs` (`TauriCtx::new(app).app_data_dir()?.join("audit")`, the base every
/// `audit_journal::record`/`record_many` call in this codebase is given); `ServerCtx::app_data_dir`
/// is the same seam `TauriCtx` implements, so `<app_data>/audit` here lines up with what a real
/// session actually wrote — reading anywhere else would make [`revert_attribution::agent_touched`]'s
/// touched-set silently empty for a real session.
fn audit_base(ctx: &dyn ServerCtx) -> Result<PathBuf, String> {
    Ok(ctx.app_data_dir()?.join("audit"))
}

/// The per-root store directory: `<app_data>/checkpoints/<root_key>`.
fn store_dir_for(ctx: &dyn ServerCtx, root: &str) -> Result<PathBuf, String> {
    Ok(checkpoints_base(ctx)?.join(root_key(root)))
}

/// The checkpoint index file inside a per-root store dir.
fn index_file(store_dir: &Path) -> PathBuf {
    store_dir.join("checkpoints.json")
}

/// The failed-checkpoint-attempts index file inside a per-root store dir — deliberately a SEPARATE file
/// from `checkpoints.json` (not a differently-shaped row in the same file): a torn/malformed line in one
/// can never affect the other, and [`checkpoint_list`] (the restore surface) never has to filter out
/// failure rows from what it returns — it simply never sees them.
fn failures_file(store_dir: &Path) -> PathBuf {
    store_dir.join("checkpoint_failures.json")
}

/// How many failed-attempt rows a root's `checkpoint_failures.json` retains before the oldest are
/// rotated out (CPE-1600). Deliberately far smaller than [`audit_journal::MAX_EVENTS_PER_SESSION`]: a
/// checkpoint attempt happens at most once per user-initiated irreversible batch (never per-file, never
/// in a tight retry loop — see [`record_checkpoint_failure`]'s doc), so even a persistently broken root
/// (e.g. a read-only drive hit repeatedly over days) accumulates one row per attempt, not a flood. 50 is
/// generous headroom for that while keeping the panel a "what needs my attention" list rather than an
/// unbounded log a user has to scroll past.
pub const MAX_CHECKPOINT_FAILURES: usize = 50;

// ---- tolerant JSON-lines index (pure over an explicit store dir) -----------------------------------

/// Append one checkpoint row to `store_dir`'s index (creating the dir if needed), flushed before return.
/// One JSON object per line — the append-only, torn-write-tolerant shape [`read_checkpoints`] reads back.
pub fn append_checkpoint(store_dir: &Path, cp: &Checkpoint) -> Result<(), String> {
    fs::create_dir_all(store_dir).map_err(|e| e.to_string())?;
    let line = serde_json::to_string(cp).map_err(|e| e.to_string())?;
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(index_file(store_dir))
        .map_err(|e| e.to_string())?;
    writeln!(f, "{line}").map_err(|e| e.to_string())?;
    f.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// Read `store_dir`'s checkpoint index back **newest-first**, skipping malformed lines (robust to a
/// partial trailing write or a hand-edit). A missing index → empty list.
pub fn read_checkpoints(store_dir: &Path) -> Vec<Checkpoint> {
    let content = match fs::read_to_string(index_file(store_dir)) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut out: Vec<Checkpoint> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Checkpoint>(l).ok())
        .collect();
    out.reverse(); // append order is oldest-first; the UI wants newest-first
    out
}

/// Append one failed-attempt row to `store_dir`'s failures index (creating the dir if needed), then trim
/// the file to its last [`MAX_CHECKPOINT_FAILURES`] lines (oldest rotated out first) — mirrors
/// `audit_journal::trim`'s crash-safe temp-file + rename rewrite so a rotation can never leave a
/// half-written file. Flushed before return, same durability as [`append_checkpoint`].
pub fn append_checkpoint_failure(store_dir: &Path, cf: &CheckpointFailure) -> Result<(), String> {
    fs::create_dir_all(store_dir).map_err(|e| e.to_string())?;
    let file = failures_file(store_dir);
    let line = serde_json::to_string(cf).map_err(|e| e.to_string())?;
    {
        let mut f = OpenOptions::new().create(true).append(true).open(&file).map_err(|e| e.to_string())?;
        writeln!(f, "{line}").map_err(|e| e.to_string())?;
        f.flush().map_err(|e| e.to_string())?;
    }
    trim_failures(&file, MAX_CHECKPOINT_FAILURES)
}

/// Keep only the last `max` non-empty lines of a failures file (rotate the oldest out first).
fn trim_failures(file: &Path, max: usize) -> Result<(), String> {
    let content = match fs::read_to_string(file) {
        Ok(c) => c,
        Err(_) => return Ok(()), // nothing written yet — nothing to trim
    };
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() <= max {
        return Ok(());
    }
    let keep = &lines[lines.len() - max..];
    let tmp = file.with_extension("json.tmp");
    let mut body = keep.join("\n");
    body.push('\n');
    fs::write(&tmp, body).map_err(|e| e.to_string())?;
    // CPE-1710: app-private store. The path is `app_data_dir()/checkpoints/<hex digest of the root>` —
    // the user's root is hashed, never joined — and this is a deliberate atomic replace of our own file.
    #[allow(clippy::disallowed_methods)]
    fs::rename(&tmp, file).map_err(|e| e.to_string())?;
    Ok(())
}

/// Read `store_dir`'s failed-attempts index back **newest-first**, skipping malformed lines — same
/// tolerant-read contract as [`read_checkpoints`]. A missing file → empty list.
pub fn read_checkpoint_failures(store_dir: &Path) -> Vec<CheckpointFailure> {
    let content = match fs::read_to_string(failures_file(store_dir)) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut out: Vec<CheckpointFailure> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<CheckpointFailure>(l).ok())
        .collect();
    out.reverse();
    out
}

// ---- ServerCtx entry points (the thin Tauri commands are one-liners into these) --------------------

/// Capture the tree under `root` into its per-root store and record a `{manifest_id, label, ts}` row in
/// the index. Reuses [`snapshot_capture::capture`] (content-addressed, deduped) — v1 captures with an
/// unbounded budget; a later ticket can thread a cap through without touching this signature.
pub fn checkpoint_create(
    ctx: &dyn ServerCtx,
    root: &str,
    label: &str,
) -> Result<CheckpointCreated, String> {
    let store_dir = store_dir_for(ctx, root)?;
    let store = store_dir.to_string_lossy().to_string();
    let outcome = snapshot_capture::capture(root, &store, &CaptureBudget::UNLIMITED)?;
    let ts = to_epoch_ms(SystemTime::now()).unwrap_or(0);
    let checkpoint = Checkpoint { manifest_id: outcome.manifest_id, label: label.to_string(), ts };
    append_checkpoint(&store_dir, &checkpoint)?;
    Ok(CheckpointCreated {
        checkpoint,
        new_blobs: outcome.new_blobs as u32,
        reused_blobs: outcome.reused_blobs as u32,
        added_bytes: outcome.added_bytes,
        skipped: outcome
            .skipped
            .into_iter()
            .map(|s| SkippedInfo { path: s.path, size: s.size, reason: skip_reason_str(s.reason).to_string() })
            .collect(),
    })
}

/// The checkpoints recorded for `root`, newest-first. Missing store → empty.
pub fn checkpoint_list(ctx: &dyn ServerCtx, root: &str) -> Result<Vec<Checkpoint>, String> {
    Ok(read_checkpoints(&store_dir_for(ctx, root)?))
}

/// Record that a best-effort pre-write checkpoint of `root` was **attempted and failed** (CPE-1600) —
/// called by every "checkpoint before an irreversible batch" caller from its own `catch`, alongside the
/// `console.error` it already logs, so the failure gets a durable home in `root`'s store next to the
/// checkpoints that did succeed. `operation` is the caller-supplied label ("Before batch media
/// overwrite", …); `reason` is the error text from the failed `checkpoint_create` call.
///
/// This function itself is infallible-in-spirit but still returns `Result` because it touches disk (the
/// store dir could be unwritable — plausibly the SAME root cause as the checkpoint that just failed);
/// every caller treats a failure here as best-effort too (log and move on), so recording a failure can
/// never itself block or surface a second error to the user.
pub fn record_checkpoint_failure(
    ctx: &dyn ServerCtx,
    root: &str,
    operation: &str,
    reason: &str,
) -> Result<(), String> {
    let store_dir = store_dir_for(ctx, root)?;
    let ts = to_epoch_ms(SystemTime::now()).unwrap_or(0);
    append_checkpoint_failure(
        &store_dir,
        &CheckpointFailure { operation: operation.to_string(), reason: reason.to_string(), ts },
    )
}

/// The failed checkpoint attempts recorded for `root`, newest-first. Missing store → empty. Kept
/// strictly separate from [`checkpoint_list`] — the Checkpoints panel calls both and renders the two
/// shapes distinctly rather than this layer conflating them into one list.
pub fn checkpoint_failures_list(ctx: &dyn ServerCtx, root: &str) -> Result<Vec<CheckpointFailure>, String> {
    Ok(read_checkpoint_failures(&store_dir_for(ctx, root)?))
}

/// Preview reverting `root` to checkpoint `manifest_id`: diff the captured checkpoint against a fresh scan
/// of the live tree ([`plan_restore`] + [`summarize_plan`]) and classify the drift ([`classify_plan`] +
/// [`summarize_conflicts`]) so the UI can warn before touching disk. Reads nothing destructive.
///
/// `session` selects how drift is attributed (see [`RevertPreview`]'s doc):
/// - `None` — conservative default, byte-identical to the pre-attribution behaviour: every diverging
///   path is drift.
/// - `Some(sess)` — attribution-aware: `sess`'s durable audit journal (under [`audit_base`]) is read
///   and folded by [`revert_attribution::agent_touched`] into the set of paths `sess` itself mutated
///   at/after this checkpoint's index `ts` (the `Checkpoint.ts` this `manifest_id` was recorded under
///   — looked up via [`read_checkpoints`], since the captured [`restore_plan::Snapshot`] itself is
///   just a path→state map with no timestamp of its own); only paths outside that set are drift. A
///   missing/empty journal folds to an empty touched-set (every diverging path is drift). If the
///   `manifest_id` is absent from the on-disk index (defensive: a torn/corrupt index row can leave
///   the manifest present but its entry missing) we cannot bound `sess`'s history to this checkpoint,
///   so we **degrade to the conservative empty touched-set** — warn about every diverging path, exactly
///   like `None`. Every branch is at least as safe as `None` and never panics.
pub fn checkpoint_preview_revert(
    ctx: &dyn ServerCtx,
    root: &str,
    manifest_id: &str,
    session: Option<&str>,
) -> Result<RevertPreview, String> {
    let store_dir = store_dir_for(ctx, root)?;
    let store = store_dir.to_string_lossy().to_string();
    let checkpoint = snapshot_capture::manifest_snapshot(&store, manifest_id)?;
    let current = snapshot_capture::scan_dir(root)?;
    let plan = plan_restore(&checkpoint, &current);
    let summary = summarize_plan(&plan, &checkpoint);
    let touched = match session {
        // No session supplied → classify against an empty touched-set so every diverging path is
        // reported as drift (conservative warn). See `RevertPreview`'s doc.
        None => std::collections::BTreeSet::new(),
        Some(sess) => {
            // Attribution needs this checkpoint's recorded timestamp so only mutations at/after it
            // count as the agent's own. If the `manifest_id` is absent from the on-disk index
            // (defensive: a torn/corrupt index row can leave the manifest present but its entry
            // missing), we cannot bound `sess`'s history to this checkpoint — so degrade to the
            // conservative empty touched-set (warn about every diverging path), exactly like `None`.
            // Falling back to `ts: 0` would instead keep the session's ENTIRE history, attributing
            // more paths away and suppressing real outside-drift warnings — strictly less safe.
            match read_checkpoints(&store_dir)
                .into_iter()
                .find(|c| c.manifest_id == manifest_id)
                .map(|c| c.ts)
            {
                Some(since_ts) => {
                    let events = audit_journal::read_session(&audit_base(ctx)?, sess);
                    revert_attribution::agent_touched(&events, sess, since_ts, root)
                }
                None => std::collections::BTreeSet::new(),
            }
        }
    };
    let classified = classify_plan(&plan, &checkpoint, &current, &touched);
    let drift = summarize_conflicts(&classified);
    Ok(RevertPreview {
        creates: summary.creates as u32,
        overwrites: summary.overwrites as u32,
        deletes: summary.deletes as u32,
        bytes_written: summary.bytes_written,
        total: summary.total() as u32,
        drift_count: drift.conflicts as u32,
        drift_paths: drift.conflict_paths,
    })
}

/// Revert the whole tree under `root` to checkpoint `manifest_id`: plan the diff and execute it with
/// [`execute_restore`] (skip-on-error honoured — a locked/unreadable file is reported, not fatal).
pub fn checkpoint_revert(
    ctx: &dyn ServerCtx,
    root: &str,
    manifest_id: &str,
) -> Result<RevertOutcome, String> {
    let store = store_dir_for(ctx, root)?.to_string_lossy().to_string();
    let checkpoint = snapshot_capture::manifest_snapshot(&store, manifest_id)?;
    let current = snapshot_capture::scan_dir(root)?;
    let plan = plan_restore(&checkpoint, &current);
    let report = execute_restore(&plan, root, &store, &checkpoint);
    Ok(RevertOutcome::from_report(report))
}

/// Cherry-revert a single `path` under `root` to its checkpoint state — the one-file counterpart of
/// [`checkpoint_revert`], using [`restore_plan::revert_one`]. A path already at the checkpoint (or absent
/// from both) produces an empty plan → a no-op outcome.
pub fn checkpoint_revert_one(
    ctx: &dyn ServerCtx,
    root: &str,
    manifest_id: &str,
    path: &str,
) -> Result<RevertOutcome, String> {
    let store = store_dir_for(ctx, root)?.to_string_lossy().to_string();
    let checkpoint = snapshot_capture::manifest_snapshot(&store, manifest_id)?;
    let current = snapshot_capture::scan_dir(root)?;
    let plan: Vec<RestoreAction> = restore_plan::revert_one(path, &checkpoint, &current).into_iter().collect();
    let report = execute_restore(&plan, root, &store, &checkpoint);
    Ok(RevertOutcome::from_report(report))
}

// ---- Retention prune (CPE-1196, epic CPE-735) --------------------------------------------------------
// Thin ctx-aware wrappers around `snapshot_prune`'s store-dir-based `preview`/`apply` — resolve `root`'s
// per-root store dir the same way every other entry point in this module does, then delegate. `preview`
// never touches disk; `apply` is the only one of the two that does, and it goes through
// `snapshot_capture::prune` internally, so the manifest-deleted-first invariant is preserved unchanged.

/// Preview retention-thinning `root`'s checkpoints under `policy`: which manifests would be kept vs.
/// pruned, and the store's current footprint. Read-only.
pub fn checkpoint_prune_preview(
    ctx: &dyn ServerCtx,
    root: &str,
    policy: &RetentionPolicy,
) -> Result<RetentionPreview, String> {
    let store = store_dir_for(ctx, root)?.to_string_lossy().to_string();
    snapshot_prune::preview(&store, policy)
}

/// Actually retention-prune `root`'s checkpoints to `policy` (+ an optional total-store-byte cap — see
/// [`snapshot_prune::apply`]'s doc for the oldest-first/never-to-zero eviction rule beyond the GFS pass).
pub fn checkpoint_prune_apply(
    ctx: &dyn ServerCtx,
    root: &str,
    policy: &RetentionPolicy,
    max_total_bytes: Option<u64>,
) -> Result<RetentionApplyResult, String> {
    let store = store_dir_for(ctx, root)?.to_string_lossy().to_string();
    snapshot_prune::apply(&store, policy, max_total_bytes)
}

// ---- Per-file diff (CPE-1197 backend half, epic CPE-735) ---------------------------------------------

/// A cap on how much text [`checkpoint_diff_file`] will diff on either side. Generous for a "what changed
/// in this file" panel but never a giant blob — mirrors `read_file_text`'s "error rather than truncate"
/// preview-skip semantics in `src-tauri/src/lib.rs` (a huge or binary file gets a clean error, not a
/// silently truncated/garbled diff).
const DIFF_MAX_BYTES: u64 = 5 * 1024 * 1024;

/// The before/after text of one file for the restore-preview's "Open diff" affordance.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FileDiff {
    /// The file's content as captured in the checkpoint's manifest.
    pub before: String,
    /// The file's current content on disk.
    pub after: String,
}

/// Diff `rel_path` between checkpoint `manifest_id` (the "before") and its live state under `root` (the
/// "after"), for a single-file preview alongside [`checkpoint_preview_revert`]'s folder-level summary.
/// Reads the checkpointed blob straight from the store (bypassing a full [`snapshot_capture::restore`])
/// and the live file from disk. Errors — cleanly, never a silent truncation or replacement character glob
/// — when: `rel_path` isn't in the checkpoint, `rel_path` escapes `root` (path-safety guard, reusing
/// [`revert_engine::safe_target`]), either side is over [`DIFF_MAX_BYTES`], or either side isn't valid
/// UTF-8 text (a binary file has no meaningful line diff here — mirrors `read_file_text`'s behaviour).
/// Read at most `cap` bytes from `path` in **one** open: `Ok(None)` means the file is longer than `cap`,
/// `Ok(Some(bytes))` that it fitted (CPE-1823 follow-up).
///
/// The point is that the limit is structural. A `metadata().len()` check followed by a `read` is two
/// opens with an unbounded window between them, and a file that grows in that window is read in full —
/// measured at 3× the cap. Reading `cap + 1` through a `Take` cannot exceed the cap no matter what the
/// file does, so nothing has to be true about timing for the bound to hold.
///
/// Recorded, deliberately not handled: a *directory* in the blob slot has `len() == 0` on some platforms,
/// so it would pass any size check — it then fails loudly at the read, which is the right outcome and
/// needs no special case here.
fn read_capped(path: &Path, cap: u64) -> std::io::Result<Option<Vec<u8>>> {
    use std::io::Read as _;
    let mut buf = Vec::new();
    fs::File::open(path)?.take(cap + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > cap {
        return Ok(None);
    }
    Ok(Some(buf))
}

pub fn checkpoint_diff_file(
    ctx: &dyn ServerCtx,
    root: &str,
    manifest_id: &str,
    rel_path: &str,
) -> Result<FileDiff, String> {
    let store_dir = store_dir_for(ctx, root)?;
    let store = store_dir.to_string_lossy().to_string();
    let checkpoint = snapshot_capture::manifest_snapshot(&store, manifest_id)?;
    let state = checkpoint
        .get(rel_path)
        .ok_or_else(|| format!("{rel_path}: not present in checkpoint {manifest_id}"))?;

    // CPE-1823, and this is the higher-impact of the two live sinks: `state.hash` is a manifest field,
    // and `blobs/`.join(it) then `fs::read` **displayed** the bytes to the user through `FileDiff.before`
    // — the command is registered and called from the frontend. `../../…/id_rsa` read any file the app
    // could read. Same shared validator as every other blob join in the crate.
    let blob_path = snapshot_capture::blob_source(&store_dir.join("blobs"), &state.hash)
        .map_err(|why| format!("{rel_path}: {why}"))?;
    // The cap is enforced on the READ, never on `state.size` (CPE-1823 follow-up 5). `size` is the
    // manifest's *claim* about the content, from the same hand-editable JSON as the hash: an entry
    // claiming `size: 1` sailed past the old check and the `fs::read` under it was then unbounded.
    // Paired with the sink above, that made the read both arbitrary in target and unlimited in length.
    //
    // **One open, bounded by construction — not stat-then-read.** The first fix measured
    // `fs::metadata().len()` and then `fs::read`, which is two separate opens with nothing holding the
    // file: measured, a concurrent appender got `15728645` bytes past a `5242880` cap, because the real
    // bound was whatever the file reached before the second open finished. `take(DIFF_MAX_BYTES + 1)`
    // cannot be raced — the window is gone rather than narrowed — and it drops a redundant traversal.
    // The `+ 1` is what distinguishes "exactly at the cap" from "over it".
    let before_bytes = read_capped(&blob_path, DIFF_MAX_BYTES)
        .map_err(|e| format!("{}: {e}", blob_path.display()))?
        .ok_or_else(|| format!("{rel_path}: checkpoint content too large to diff (over the {DIFF_MAX_BYTES} byte limit)."))?;
    let before = String::from_utf8(before_bytes).map_err(|_| {
        format!("{rel_path}: checkpoint content is not valid UTF-8 text (binary diff isn't supported).")
    })?;

    let live_path = safe_target(Path::new(root), rel_path)?;
    // Same one-open bound as the checkpoint half above. The live file is the one an attacker does NOT
    // need to plant a manifest to grow — it is a file in the user's own tree that any process may be
    // appending to right now — so leaving this half as stat-then-read would have kept the measured
    // 3×-the-cap read available through the very command the other half just closed it in. Fixing one
    // half of a symmetric pair is this ticket's own recurring mistake.
    let after_bytes = read_capped(&live_path, DIFF_MAX_BYTES)
        .map_err(|e| format!("{}: {e}", live_path.display()))?
        .ok_or_else(|| format!("{rel_path}: live file too large to diff (over the {DIFF_MAX_BYTES} byte limit)."))?;
    let after = String::from_utf8(after_bytes).map_err(|_| {
        format!("{rel_path}: live file is not valid UTF-8 text (binary diff isn't supported).")
    })?;

    Ok(FileDiff { before, after })
}

/// String form of a capture skip reason for the wire (mirrors `snapshot_capture`'s private mapping).
fn skip_reason_str(reason: SkipReason) -> &'static str {
    match reason {
        SkipReason::Oversize => "oversize",
        SkipReason::Budget => "budget",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::HeadlessCtx;

    /// A unique scratch dir per call (parallel-test-safe: atomic counter, not a timestamp).
    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-ckpt-{tag}"))
    }

    #[test]
    fn read_index_is_newest_first_and_skips_malformed_lines() {
        let store = scratch("index");
        append_checkpoint(&store, &Checkpoint { manifest_id: "m1".into(), label: "first".into(), ts: 1 }).unwrap();
        // A torn/garbage line spliced in must be skipped, not crash the read.
        {
            let mut f = OpenOptions::new().append(true).open(index_file(&store)).unwrap();
            writeln!(f, "{{ not valid json").unwrap();
        }
        append_checkpoint(&store, &Checkpoint { manifest_id: "m2".into(), label: "second".into(), ts: 2 }).unwrap();

        let got = read_checkpoints(&store);
        assert_eq!(got.iter().map(|c| c.manifest_id.as_str()).collect::<Vec<_>>(), vec!["m2", "m1"]);
        // A missing index degrades to empty.
        assert!(read_checkpoints(&scratch("empty")).is_empty());
        let _ = fs::remove_dir_all(&store);
    }

    #[test]
    fn distinct_roots_get_distinct_stores() {
        assert_ne!(root_key("/a/root"), root_key("/b/root"));
        assert_eq!(root_key("/a/root"), root_key("/a/root")); // deterministic
    }

    /// The full command-level lifecycle: create → mutate the tree → preview (plan + drift) → revert →
    /// assert restored. Exercises every entry point + the store index through the ServerCtx seam.
    #[test]
    fn create_mutate_preview_revert_round_trips_the_tree() {
        let app = scratch("app-data");
        let ctx = HeadlessCtx::new(app.to_path_buf());
        let root = scratch("root");

        // Initial tree.
        fs::write(root.join("keep.txt"), b"keep me").unwrap();
        fs::write(root.join("edit.txt"), b"original content").unwrap();
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("nested").join("gone.txt"), b"will be deleted after checkpoint").unwrap();
        let root_s = root.to_string_lossy().to_string();

        // 1) Create a checkpoint.
        let created = checkpoint_create(&ctx, &root_s, "before edits").unwrap();
        assert_eq!(created.checkpoint.label, "before edits");
        assert!(created.new_blobs >= 3, "keep/edit/gone each store a blob");
        assert!(created.skipped.is_empty());

        // It shows up in the list, newest-first.
        let list = checkpoint_list(&ctx, &root_s).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].manifest_id, created.checkpoint.manifest_id);
        let manifest_id = created.checkpoint.manifest_id.clone();

        // 2) Mutate: edit one file, delete another, add a new one.
        fs::write(root.join("edit.txt"), b"changed after checkpoint").unwrap();
        fs::remove_file(root.join("nested").join("gone.txt")).unwrap();
        fs::write(root.join("added.txt"), b"new since checkpoint").unwrap();

        // 3) Preview: overwrite edit.txt, create nested/gone.txt, delete added.txt = 3 paths, all drift
        //    (no attribution wired → every change is surfaced).
        let preview = checkpoint_preview_revert(&ctx, &root_s, &manifest_id, None).unwrap();
        assert_eq!(preview.overwrites, 1);
        assert_eq!(preview.creates, 1);
        assert_eq!(preview.deletes, 1);
        assert_eq!(preview.total, 3);
        assert_eq!(preview.drift_count, 3, "no attribution → all three diverging paths are drift");
        assert!(preview.drift_paths.contains(&"edit.txt".to_string()));

        // 4) Revert and assert the tree is restored content-for-content.
        let outcome = checkpoint_revert(&ctx, &root_s, &manifest_id).unwrap();
        assert_eq!(outcome.applied, 3);
        assert!(outcome.skipped.is_empty(), "skipped: {:?}", outcome.skipped);
        assert_eq!(fs::read(root.join("edit.txt")).unwrap(), b"original content");
        assert_eq!(
            fs::read(root.join("nested").join("gone.txt")).unwrap(),
            b"will be deleted after checkpoint"
        );
        assert!(!root.join("added.txt").exists(), "the post-checkpoint file was removed");
        assert_eq!(fs::read(root.join("keep.txt")).unwrap(), b"keep me");

        // A re-preview now finds nothing to do (tree matches the checkpoint).
        let after = checkpoint_preview_revert(&ctx, &root_s, &manifest_id, None).unwrap();
        assert_eq!(after.total, 0);
        assert_eq!(after.drift_count, 0);

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }

    /// CPE-1127: every store entry point that takes a caller-supplied `manifest_id` refuses a
    /// traversal/separator id (they funnel through `snapshot_capture::load_manifest`, which validates).
    #[test]
    fn revert_entry_points_refuse_a_traversal_manifest_id() {
        let app = scratch("app-data-traversal");
        let ctx = HeadlessCtx::new(app.to_path_buf());
        let root = scratch("root-traversal");
        fs::write(root.join("a.txt"), b"a").unwrap();
        let root_s = root.to_string_lossy().to_string();
        // A real checkpoint exists, but every call below is attacked with a bad id, not the real one.
        checkpoint_create(&ctx, &root_s, "cp").unwrap();

        for bad in ["../../etc/foo", "..\\secrets", "nested/id"] {
            assert!(checkpoint_preview_revert(&ctx, &root_s, bad, None).is_err(), "preview_revert({bad:?})");
            assert!(checkpoint_revert(&ctx, &root_s, bad).is_err(), "revert({bad:?})");
            assert!(checkpoint_revert_one(&ctx, &root_s, bad, "a.txt").is_err(), "revert_one({bad:?})");
        }

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }

    #[test]
    fn cherry_revert_one_restores_a_single_path_only() {
        let app = scratch("app-data-one");
        let ctx = HeadlessCtx::new(app.to_path_buf());
        let root = scratch("root-one");
        fs::write(root.join("a.txt"), b"a original").unwrap();
        fs::write(root.join("b.txt"), b"b original").unwrap();
        let root_s = root.to_string_lossy().to_string();

        let created = checkpoint_create(&ctx, &root_s, "cp").unwrap();
        // Edit both, then cherry-revert only a.txt.
        fs::write(root.join("a.txt"), b"a edited").unwrap();
        fs::write(root.join("b.txt"), b"b edited").unwrap();

        let outcome =
            checkpoint_revert_one(&ctx, &root_s, &created.checkpoint.manifest_id, "a.txt").unwrap();
        assert_eq!(outcome.applied, 1);
        assert!(outcome.skipped.is_empty());
        assert_eq!(fs::read(root.join("a.txt")).unwrap(), b"a original", "a.txt reverted");
        assert_eq!(fs::read(root.join("b.txt")).unwrap(), b"b edited", "b.txt left untouched");

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }

    /// CPE-1134: with `session: Some(sess)` and a seeded audit journal, a path `sess` itself mutated
    /// at/after the checkpoint is attributed away (not drift), while a path a *different* session
    /// touched is still surfaced — proving `revert_attribution::agent_touched` is actually wired in,
    /// not just accepted and ignored.
    #[test]
    fn preview_with_session_excludes_only_that_sessions_own_changes_from_drift() {
        let app = scratch("app-data-attrib");
        let ctx = HeadlessCtx::new(app.to_path_buf());
        let root = scratch("root-attrib");
        fs::write(root.join("a.txt"), b"a original").unwrap();
        fs::write(root.join("b.txt"), b"b original").unwrap();
        let root_s = root.to_string_lossy().to_string();

        let created = checkpoint_create(&ctx, &root_s, "cp").unwrap();
        let manifest_id = created.checkpoint.manifest_id.clone();
        let checkpoint_ts = created.checkpoint.ts;

        // Both files diverge after the checkpoint: a.txt by "agent-1" (the reverting session), b.txt
        // by a different session entirely.
        fs::write(root.join("a.txt"), b"a edited by agent-1").unwrap();
        fs::write(root.join("b.txt"), b"b edited by someone else").unwrap();

        let base = audit_base(&ctx).unwrap();
        audit_journal::record(
            &base,
            &audit_journal::AuditEvent {
                ts: checkpoint_ts + 1,
                session: "agent-1".into(),
                kind: "modified".into(),
                path: format!("{root_s}/a.txt"),
                actor: None,
                detail: None,
            },
            audit_journal::MAX_EVENTS_PER_SESSION,
        )
        .unwrap();
        audit_journal::record(
            &base,
            &audit_journal::AuditEvent {
                ts: checkpoint_ts + 1,
                session: "other-session".into(),
                kind: "modified".into(),
                path: format!("{root_s}/b.txt"),
                actor: None,
                detail: None,
            },
            audit_journal::MAX_EVENTS_PER_SESSION,
        )
        .unwrap();

        // Without a session, both diverging paths are drift (unchanged conservative default).
        let conservative = checkpoint_preview_revert(&ctx, &root_s, &manifest_id, None).unwrap();
        assert_eq!(conservative.total, 2);
        assert_eq!(conservative.drift_count, 2, "no attribution → both diverging paths are drift");

        // With the reverting agent's session, its own change (a.txt) is attributed away; the other
        // session's change (b.txt) is still drift.
        let attributed =
            checkpoint_preview_revert(&ctx, &root_s, &manifest_id, Some("agent-1")).unwrap();
        assert_eq!(attributed.total, 2, "still two paths to restore");
        assert_eq!(attributed.drift_count, 1, "only b.txt (a different session) is drift");
        assert_eq!(attributed.drift_paths, vec!["b.txt".to_string()]);
        assert!(!attributed.drift_paths.contains(&"a.txt".to_string()), "agent-1's own edit isn't drift");

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }

    /// CPE-1134 (review follow-up): attribution is bounded by the checkpoint's *real* recorded `ts`, not
    /// `0`. A session event that predates the checkpoint must NOT be attributed away — the agent
    /// touching a path *before* the checkpoint says nothing about who diverged it *after*. This guards
    /// the earlier `unwrap_or(0)` hazard (a `0` floor would keep pre-checkpoint events and wrongly
    /// suppress the drift warning).
    #[test]
    fn preview_with_session_ignores_pre_checkpoint_events() {
        let app = scratch("app-data-pre-cp");
        let ctx = HeadlessCtx::new(app.to_path_buf());
        let root = scratch("root-pre-cp");
        fs::write(root.join("a.txt"), b"a original").unwrap();
        let root_s = root.to_string_lossy().to_string();

        let created = checkpoint_create(&ctx, &root_s, "cp").unwrap();
        let manifest_id = created.checkpoint.manifest_id.clone();
        let checkpoint_ts = created.checkpoint.ts;
        assert!(checkpoint_ts >= 1, "epoch-ms checkpoint ts should be well above 0");

        // a.txt diverges after the checkpoint, but the ONLY journal event for the reverting session is
        // dated BEFORE the checkpoint — so it must not attribute a.txt away.
        fs::write(root.join("a.txt"), b"a edited after checkpoint").unwrap();
        let base = audit_base(&ctx).unwrap();
        audit_journal::record(
            &base,
            &audit_journal::AuditEvent {
                ts: checkpoint_ts - 1,
                session: "agent-1".into(),
                kind: "modified".into(),
                path: format!("{root_s}/a.txt"),
                actor: None,
                detail: None,
            },
            audit_journal::MAX_EVENTS_PER_SESSION,
        )
        .unwrap();

        let preview =
            checkpoint_preview_revert(&ctx, &root_s, &manifest_id, Some("agent-1")).unwrap();
        assert_eq!(preview.total, 1);
        assert_eq!(
            preview.drift_count, 1,
            "a pre-checkpoint event must not attribute the post-checkpoint divergence away"
        );
        assert_eq!(preview.drift_paths, vec!["a.txt".to_string()]);

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }

    /// CPE-1134: a missing (never-written) or empty audit journal for the requested session degrades to
    /// the conservative behaviour — no panic, and every diverging path still counted as drift, exactly
    /// as if `session` had been `None`.
    #[test]
    fn preview_with_session_and_no_journal_degrades_to_conservative_behaviour() {
        let app = scratch("app-data-no-journal");
        let ctx = HeadlessCtx::new(app.to_path_buf());
        let root = scratch("root-no-journal");
        fs::write(root.join("a.txt"), b"a original").unwrap();
        let root_s = root.to_string_lossy().to_string();

        let created = checkpoint_create(&ctx, &root_s, "cp").unwrap();
        let manifest_id = created.checkpoint.manifest_id.clone();
        fs::write(root.join("a.txt"), b"a edited").unwrap();

        // No audit dir has ever been created under this app-data dir — `read_session` must return an
        // empty Vec rather than erroring, and the preview must not panic.
        let preview =
            checkpoint_preview_revert(&ctx, &root_s, &manifest_id, Some("nobody")).unwrap();
        assert_eq!(preview.total, 1);
        assert_eq!(preview.drift_count, 1, "unattributed session → still conservative drift");

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }

    // ---- retention prune (CPE-1196) ---------------------------------------------------------------

    #[test]
    fn prune_preview_and_apply_go_through_the_roots_own_store() {
        let app = scratch("app-data-prune");
        let ctx = HeadlessCtx::new(app.to_path_buf());
        let root = scratch("root-prune");
        let root_s = root.to_string_lossy().to_string();
        fs::write(root.join("a.txt"), b"v1").unwrap();
        let first = checkpoint_create(&ctx, &root_s, "one").unwrap();
        fs::write(root.join("a.txt"), b"v2").unwrap();
        let second = checkpoint_create(&ctx, &root_s, "two").unwrap();

        // Retention that only ever keeps 1 hourly bucket: since both checkpoints happen within the same
        // wall-clock hour in a fast test run, thin() keeps just the newest of that bucket.
        let pol = crate::snapshot_retention::RetentionPolicy { hourly: 1, daily: 0, weekly: 0, monthly: 0 };
        let preview = checkpoint_prune_preview(&ctx, &root_s, &pol).unwrap();
        assert_eq!(preview.keep.len() + preview.prune.len(), 2);

        let applied = checkpoint_prune_apply(&ctx, &root_s, &pol, None).unwrap();
        assert_eq!(applied.kept.len() + applied.pruned.len(), 2);
        // Whichever the policy dropped no longer lists in the checkpoint index... no wait, the index is
        // separate from manifests; assert instead that a pruned manifest can no longer be reverted to.
        for id in &applied.pruned {
            assert!(
                checkpoint_preview_revert(&ctx, &root_s, id, None).is_err(),
                "a pruned manifest {id} must no longer be readable"
            );
        }
        for id in &applied.kept {
            assert!(checkpoint_preview_revert(&ctx, &root_s, id, None).is_ok());
        }
        let _ = (first, second);

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }

    // ---- per-file diff (CPE-1197 backend half) ----------------------------------------------------

    #[test]
    fn diff_file_returns_checkpoint_and_live_content_for_a_changed_file() {
        let app = scratch("app-data-diff");
        let ctx = HeadlessCtx::new(app.to_path_buf());
        let root = scratch("root-diff");
        let root_s = root.to_string_lossy().to_string();
        fs::write(root.join("a.txt"), b"original content").unwrap();

        let created = checkpoint_create(&ctx, &root_s, "cp").unwrap();
        fs::write(root.join("a.txt"), b"changed content").unwrap();

        let diff = checkpoint_diff_file(&ctx, &root_s, &created.checkpoint.manifest_id, "a.txt").unwrap();
        assert_eq!(diff.before, "original content");
        assert_eq!(diff.after, "changed content");

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }

    /// Rewrite one entry of a checkpoint's manifest on disk, exactly as someone editing the JSON would
    /// (CPE-1823). The manifest is an ordinary unsigned file in the app's data directory; nothing signs
    /// it and nothing checks it was written by us.
    fn tamper(store: &Path, id: &str, key: &str, hash: Option<&str>, size: Option<u64>) {
        let p = store.join("manifests").join(format!("{id}.json"));
        let mut v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        if let Some(h) = hash {
            v["files"][key]["hash"] = serde_json::Value::String(h.to_string());
        }
        if let Some(s) = size {
            v["files"][key]["size"] = serde_json::json!(s);
        }
        fs::write(&p, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    }

    /// **CPE-1823 round 2 — the higher-impact live sink.** `checkpoint_diff_file` is a registered command
    /// the frontend calls, and it joined the manifest's `hash` onto `blobs/` and `fs::read` it straight
    /// into `FileDiff.before`, which is **displayed**. A climbing hash exfiltrated any file the app could
    /// read to the screen.
    ///
    /// The harm assertion is on the returned content, not on the `Result`: the unfixed code returns
    /// `Ok(FileDiff { .. })` and looks entirely normal — the secret simply appears in the diff pane.
    #[test]
    fn cpe_1823_diff_file_never_reads_a_blob_from_outside_the_store() {
        const SECRET: &str = "THE VICTIM PRIVATE KEY FROM OUTSIDE THE STORE";
        let app = scratch("cpe1823-diff-app");
        let ctx = HeadlessCtx::new(app.to_path_buf());
        let root = scratch("cpe1823-diff-root");
        let root_s = root.to_string_lossy().to_string();
        fs::write(root.join("a.txt"), b"original").unwrap();

        let created = checkpoint_create(&ctx, &root_s, "cp").unwrap();
        let id = created.checkpoint.manifest_id.clone();
        let store = store_dir_for(&ctx, &root_s).unwrap();

        // The victim is planted as a SIBLING OF THE REAL STORE, and the climb is derived from that —
        // never guessed. The first version of this test aimed `../../<a temp sibling>/id_rsa` at a
        // scratch directory while `blobs/` actually sits at `<app>/data/checkpoints/<64-hex>/blobs`, so
        // the real climb is five, not two. At the wrong depth the target simply does not exist, the raw
        // join returns NotFound, and `!shown.contains(SECRET)` could not fail before OR after the fix —
        // the test certified nothing while appearing to pass. (It did red under sabotage, but only on
        // the error-message assertion, which any differently-worded error would have satisfied too.)
        let secret_file = store.parent().unwrap().join("cpe1823-victim-id_rsa");
        fs::write(&secret_file, SECRET).unwrap();

        // Both escaping shapes, and the absolute one is the belt to the climb's braces — it reaches the
        // victim regardless of how deep the store turns out to be, which is precisely what went wrong.
        for hash in ["../../cpe1823-victim-id_rsa".to_string(), secret_file.to_string_lossy().into_owned()] {
            // **The fixture is asserted LIVE before the guard is asked about it.** Without this the test
            // could not fail on its harm axis at all: at the wrong depth the raw join is `NotFound`, so
            // `!shown.contains(SECRET)` is true before and after the fix, and the whole certification is
            // inert while looking green.
            assert_eq!(
                fs::read(store.join("blobs").join(&hash)).unwrap(),
                SECRET.as_bytes(),
                "fixture is inert: {hash:?} must actually reach the victim through a raw join, or this \
                 test certifies nothing"
            );
            tamper(&store, &id, "a.txt", Some(&hash), None);
            fs::write(root.join("a.txt"), b"changed").unwrap();

            let got = checkpoint_diff_file(&ctx, &root_s, &id, "a.txt");

            let shown = got.as_ref().map(|d| d.before.clone()).unwrap_or_default();
            assert!(
                !shown.contains(SECRET),
                "HARM: hash {hash:?} put a file from outside the store on screen: {shown:?}"
            );
            let err = got.expect_err("a manifest naming a blob outside the store must be refused");
            // Naming the entry is not enough to pin this: prefixing the raw io error with `rel_path` —
            // something this crate does routinely — would satisfy `err.contains("a.txt")` with the sink
            // wide open. The assertion is on the *rule* that refused it.
            assert!(
                err.contains("plain hex blob name"),
                "the refusal must come from the blob-name rule, not from an incidental I/O error that \
                 happens to mention the path: {err}"
            );
        }

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }

    /// **CPE-1823 follow-up 5.** The cap was applied to `state.size` — the manifest's *claim* about the
    /// content — and the `fs::read` beneath it was then unbounded. An entry claiming `size: 1` sailed
    /// through and read the whole file. Paired with the sink above, that made the read both arbitrary in
    /// target and unlimited in length; alone it is still an unbounded read into memory from a file the
    /// user never asked to diff.
    ///
    /// **The live file is shrunk to a few bytes after the capture, and that is the whole reason this test
    /// says anything.** The first cut left the oversize file on disk, so with the guard sabotaged the
    /// function still returned `Err` — from the *live* half's cap, which has always measured the real
    /// file — and the assertion on the byte count matched that message just as happily. It passed under
    /// sabotage: a test proving the sibling check works while claiming to prove this one. With the live
    /// side small, only the checkpoint half can trip a cap, and the message is asserted to be that half's.
    #[test]
    fn cpe_1823_the_diff_cap_is_measured_on_the_blob_not_on_the_manifests_claim() {
        let app = scratch("cpe1823-size-app");
        let ctx = HeadlessCtx::new(app.to_path_buf());
        let root = scratch("cpe1823-size-root");
        let root_s = root.to_string_lossy().to_string();
        fs::write(root.join("huge.txt"), vec![b'x'; (DIFF_MAX_BYTES + 1) as usize]).unwrap();

        let created = checkpoint_create(&ctx, &root_s, "cp").unwrap();
        let id = created.checkpoint.manifest_id.clone();
        let store = store_dir_for(&ctx, &root_s).unwrap();
        // The blob on disk is unchanged and over the cap; only the manifest's claim about it shrinks.
        tamper(&store, &id, "huge.txt", None, Some(1));
        fs::write(root.join("huge.txt"), b"small now").unwrap(); // so only the checkpoint half can cap

        let got = checkpoint_diff_file(&ctx, &root_s, &id, "huge.txt");

        let served = got.as_ref().map(|d| d.before.len()).unwrap_or(0) as u64;
        assert!(
            served <= DIFF_MAX_BYTES,
            "HARM: {served} bytes were read and returned past the {DIFF_MAX_BYTES} cap, on the strength \
             of a manifest claiming size: 1"
        );
        let err = got.expect_err("a lie about the size must not unlock an unbounded read of the blob");
        assert!(
            err.contains("checkpoint content too large"),
            "the refusal must come from the CHECKPOINT half, not the live one: {err}"
        );
        assert!(
            err.contains(&DIFF_MAX_BYTES.to_string()),
            "and it must name the limit that was exceeded: {err}"
        );

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }

    /// **CPE-1823 round 4, and this one needs no attacker at all.** A macOS or Linux capture holding
    /// `a.txt ` — a name those platforms store happily — cherry-reverted on Windows destroyed the user's
    /// `a.txt`. `revert_one` asks `checkpoint.get("a.txt")`, gets `None` because the checkpoint spells it
    /// with a trailing space, and plans a lone `Delete`; on Windows the two are the same file, so the
    /// checkpoint *does* hold it. Measured before the fix:
    ///
    /// ```text
    /// plan   = [("a.txt", "Delete")]
    /// report = RestoreReport { applied: 1, skipped: [] }; a.txt = Err(NotFound)
    /// ```
    ///
    /// Round 3's stand-down could not catch it: a one-action cherry-revert plan contains **no write**, so
    /// nothing could be skipped and the condition never armed. Keying it on the checkpoint's keys instead
    /// of on the plan's outcome is what closes it.
    ///
    /// Driven through the registered command rather than through `execute_restore`, because the whole
    /// defect is that this *path* through the code produces a plan the guards never see.
    #[cfg(windows)]
    #[test]
    fn cpe_1823_cherry_reverting_never_deletes_a_file_the_checkpoint_holds_under_an_aliased_name() {
        const LIVE: &[u8] = b"the user's only copy";
        let app = scratch("cpe1823-revone-app");
        let ctx = HeadlessCtx::new(app.to_path_buf());
        let root = scratch("cpe1823-revone-root");
        let root_s = root.to_string_lossy().to_string();
        fs::write(root.join("a.txt"), LIVE).unwrap();
        fs::write(root.join("other.txt"), b"untouched").unwrap();

        // A capture made where `a.txt ` is a legal, distinct filename — reproduced here by renaming the
        // key in the manifest, which is exactly what a store carried over from macOS or Linux contains.
        let created = checkpoint_create(&ctx, &root_s, "cp").unwrap();
        let id = created.checkpoint.manifest_id.clone();
        let store = store_dir_for(&ctx, &root_s).unwrap();
        let p = store.join("manifests").join(format!("{id}.json"));
        let mut v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        // `remove`, not `take` — `take` leaves the key behind holding `null`, which fails to deserialize
        // into `PersistedFileState`, so the command errors before it ever plans anything and the test
        // panics at this `unwrap` having proved nothing about deletes.
        let entry = v["files"].as_object_mut().unwrap().remove("a.txt").unwrap();
        v["files"]["a.txt "] = entry;
        fs::write(&p, serde_json::to_string_pretty(&v).unwrap()).unwrap();

        let outcome = checkpoint_revert_one(&ctx, &root_s, &id, "a.txt").unwrap();

        assert_eq!(
            fs::read(root.join("a.txt")).ok().as_deref(),
            Some(LIVE),
            "HARM: cherry-revert deleted the user's only copy of a file the checkpoint holds under an \
             aliased name — outcome was {outcome:?}"
        );

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }


    /// **CPE-1823 round 5, through the registered commands, both of them.** Round 4's stand-down keys on
    /// SPELLING — `checkpoint.keys().filter(|k| safe_segments(k).is_err())` — and `A.txt` passes
    /// `safe_segments` just as `a.txt` does, so on a case-alias the filter is empty and nothing arms.
    /// Measured before this fix:
    ///
    /// ```text
    /// CMD revert[case-alias]     -> applied=2 skipped=0; a.txt = Err(NotFound)
    /// CMD revert_one[case-alias] -> applied=1 skipped=0; a.txt = Err(NotFound)
    /// ```
    ///
    /// Byte-for-byte the round-3 harm with `A.txt` substituted for `a.txt `, and it needs no name a
    /// platform disallows: both spellings are legal everywhere, which is why the fix had to move to the
    /// resolved path. Driven through `checkpoint_revert` **and** `checkpoint_revert_one` because this
    /// ticket has three times fixed one of a pair and left the other reachable.
    #[cfg(windows)]
    #[test]
    fn cpe_1823_neither_revert_command_destroys_a_file_reached_under_a_case_alias() {
        const CAPTURED: &[u8] = b"what the checkpoint captured";
        const LIVE: &[u8] = b"the user's current work, which this revert must not silently destroy";
        for whole_tree in [true, false] {
            let app = scratch("cpe1823-casecmd-app");
            let ctx = HeadlessCtx::new(app.to_path_buf());
            let root = scratch("cpe1823-casecmd-root");
            let root_s = root.to_string_lossy().to_string();
            fs::write(root.join("a.txt"), CAPTURED).unwrap();
            // Fixture is inert on a case-sensitive volume: there `A.txt` is a different file, the plan is
            // legitimate, and this leg certifies nothing.
            assert_eq!(
                fs::read(root.join("A.txt")).ok().as_deref(),
                Some(CAPTURED),
                "fixture is inert: `A.txt` must already address the user's `a.txt` on this volume"
            );

            // A capture made where `A.txt` and `a.txt` are distinct — reproduced by renaming the key in
            // the manifest, which is what a store carried from a case-sensitive machine contains, and
            // what a hand-edited one contains regardless.
            let created = checkpoint_create(&ctx, &root_s, "cp").unwrap();
            let id = created.checkpoint.manifest_id.clone();
            let store = store_dir_for(&ctx, &root_s).unwrap();
            let p = store.join("manifests").join(format!("{id}.json"));
            let mut v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
            let entry = v["files"].as_object_mut().unwrap().remove("a.txt").unwrap();
            v["files"]["A.txt"] = entry;
            fs::write(&p, serde_json::to_string_pretty(&v).unwrap()).unwrap();
            // The user's current work, distinct from the captured bytes on purpose: it makes the write
            // half destructive too, so the `Create` guard and the delete guard are each independently
            // load-bearing here rather than covering for one another.
            fs::write(root.join("a.txt"), LIVE).unwrap();

            let outcome = if whole_tree {
                checkpoint_revert(&ctx, &root_s, &id).unwrap()
            } else {
                checkpoint_revert_one(&ctx, &root_s, &id, "a.txt").unwrap()
            };

            assert_eq!(
                fs::read(root.join("a.txt")).ok().as_deref(),
                Some(LIVE),
                "HARM: {} destroyed the user's file through a case alias — the aliased manifest names \
                 `A.txt`, so nothing here may be applied to `a.txt` at all. Outcome was {outcome:?}",
                if whole_tree { "checkpoint_revert" } else { "checkpoint_revert_one" }
            );
            assert_eq!(outcome.applied, 0, "nothing may be counted applied: {outcome:?}");

            let _ = fs::remove_dir_all(&root);
            let _ = fs::remove_dir_all(&app);
        }
    }

    /// Rewrite manifest `id`'s on-disk JSON through `edit` — a hand-edit, a store synced from another
    /// machine, or anything running as the user — and hand back what is actually on disk afterwards, so
    /// a test can assert its **fixture is live** before it asserts any harm. Reading it back rather than
    /// returning the edited value in memory is the point: it proves the write landed.
    fn tamper_manifest(
        ctx: &HeadlessCtx,
        root_s: &str,
        id: &str,
        edit: impl FnOnce(&mut serde_json::Value),
    ) -> serde_json::Value {
        let p = store_dir_for(ctx, root_s).unwrap().join("manifests").join(format!("{id}.json"));
        let mut v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        edit(&mut v);
        fs::write(&p, serde_json::to_string_pretty(&v).unwrap()).unwrap();
        serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap()
    }

    /// **CPE-1847, through both registered commands.** A manifest whose `files` map is empty describes a
    /// tree with nothing in it, so `plan_restore` reads every live file as "added since the checkpoint"
    /// and plans a `Delete` for all of them. Measured on this branch before the fix, reproducing the
    /// ticket's figures exactly:
    ///
    /// ```text
    /// CMD revert[empty manifest]:     applied=5 skipped=0   survivors = []
    /// CMD revert_one[empty manifest]: applied=1 skipped=0   survivors = [f1, f2, f4, f5]
    /// ```
    ///
    /// Complete success reported, whole tree gone, from deleting three characters. Every CPE-1823 guard
    /// is structurally blind to it: there is no write to fail, no key to judge, and no entry to resolve.
    ///
    /// `revert_one` is not a courtesy leg. It is the route that **evades the mitigation everyone
    /// assumed** — it never consults `checkpoint_preview_revert`, so its per-file confirm says nothing
    /// about a mass delete, and CPE-1823 needed four rounds largely because guards kept landing on a
    /// path with no callers while the shipping path went unguarded.
    ///
    /// The tamper here restates `file_count` as well, so this exercises the **stand-down** rather than
    /// `load_manifest`'s cross-check — the cheap one-field version is the next test. Two edited fields
    /// is the most this ever costs an attacker, which is exactly why the fix cannot rest on the count.
    #[test]
    fn cpe_1847_neither_revert_command_deletes_a_populated_tree_on_a_zero_entry_checkpoint() {
        const CAPTURED: &[u8] = b"what the checkpoint captured";
        const LIVE: &[u8] = b"the user's work since the checkpoint, which no revert here may touch";
        // Cherry-revert first, deliberately: it is the route with no preview in front of it, and the
        // one four rounds of CPE-1823 kept leaving for last.
        for whole_tree in [false, true] {
            let app = scratch("cpe1847-zero-app");
            let ctx = HeadlessCtx::new(app.to_path_buf());
            let root = scratch("cpe1847-zero-root");
            let root_s = root.to_string_lossy().to_string();
            for i in 1..=5 {
                fs::write(root.join(format!("f{i}.txt")), CAPTURED).unwrap();
            }

            let created = checkpoint_create(&ctx, &root_s, "cp").unwrap();
            let id = created.checkpoint.manifest_id.clone();

            // f1 is made to diverge from the checkpoint AFTER the capture, on purpose: it is what makes
            // a **dead tamper** red instead of passing quietly. If the edit below failed to land, the
            // checkpoint still holds all five entries and the plan is a legitimate single `Overwrite` of
            // f1.txt — a different real change, which the `LIVE` assertion below then catches, because
            // f1 would be back to `CAPTURED`. A test that only asserted "all five files exist" would
            // pass on an inert fixture here, which is the trap CPE-1823 fell into six times.
            fs::write(root.join("f1.txt"), LIVE).unwrap();

            let after = tamper_manifest(&ctx, &root_s, &id, |v| {
                v["files"] = serde_json::json!({});
                v["file_count"] = serde_json::json!(0);
            });
            assert_eq!(
                after["files"].as_object().map(|o| o.len()),
                Some(0),
                "fixture is inert: the manifest's `files` map must actually be empty on disk, or this \
                 test certifies nothing"
            );

            // …and it must have reached the PLANNER as a whole-tree delete, not merely sat in the file.
            // This also pins that a zero-entry checkpoint still previews: refusing to load it would
            // refuse a genuine capture of an empty folder.
            let preview = checkpoint_preview_revert(&ctx, &root_s, &id, None)
                .expect("a zero-entry checkpoint must still load and preview");
            assert_eq!(
                (preview.creates, preview.overwrites, preview.deletes),
                (0, 0, 5),
                "fixture is inert: the emptied manifest must plan five deletes and no writes, or this \
                 test certifies nothing. preview = {preview:?}"
            );

            let outcome = if whole_tree {
                checkpoint_revert(&ctx, &root_s, &id).unwrap()
            } else {
                checkpoint_revert_one(&ctx, &root_s, &id, "f3.txt").unwrap()
            };

            // THE HARM, asserted before the `Result` is looked at at all.
            for i in 1..=5 {
                assert!(
                    root.join(format!("f{i}.txt")).exists(),
                    "HARM: {} deleted f{i}.txt on a checkpoint that records no files — an emptied \
                     `files` map turns a revert into a whole-tree delete reported as complete success. \
                     Outcome was {outcome:?}",
                    if whole_tree { "checkpoint_revert" } else { "checkpoint_revert_one" }
                );
            }
            assert_eq!(
                fs::read(root.join("f1.txt")).ok().as_deref(),
                Some(LIVE),
                "fixture is inert: f1.txt was reverted to its captured bytes, so the manifest still \
                 held its entry and the tamper never took — this certifies nothing about empty \
                 manifests"
            );

            assert_eq!(outcome.applied, 0, "nothing may be counted applied: {outcome:?}");
            let expected_held = if whole_tree { 5 } else { 1 };
            assert_eq!(
                outcome.skipped.len(),
                expected_held,
                "every delete must be held back and named, never silently dropped: {outcome:?}"
            );
            for op in &outcome.skipped {
                assert!(
                    op.error.starts_with("not deleted:"),
                    "a held-back delete must use the one hold-back channel the UI can match on \
                     (CPE-1845 owns making that structural): {op:?}"
                );
                assert!(
                    op.error.contains(&format!("{expected_held} file")),
                    "the reason must carry the count of what would have been deleted: {op:?}"
                );
            }

            let _ = fs::remove_dir_all(&root);
            let _ = fs::remove_dir_all(&app);
        }
    }

    /// **CPE-1847, the cheapest tamper and its wider sibling.** Deleting entries from `files` and
    /// touching nothing else is the whole attack: the removed paths become `Delete`s. Emptying the map
    /// entirely is the ticket's shape; removing *four of five* is strictly wider, because it evades any
    /// zero-entry rule while destroying almost as much — measured on this branch before the fix at
    /// `applied: 4, survivors: ["f1.txt"]`.
    ///
    /// Both are refused at `load_manifest`, by the count the capture wrote, so **every** route refuses
    /// together — preview, diff, and both revert commands. That placement matters for exactly the reason
    /// this ticket exists: `checkpoint_revert_one` never consults the preview, so a check that guarded
    /// only the preview would guard the one route nobody is attacked through.
    ///
    /// **What this test does NOT show, spelled out because an earlier version of this doc claimed the
    /// opposite.** It called the count a "cost-raiser". It is not one: `file_count` is
    /// `#[serde(default)] Option<usize>` and the check is gated on `Some`, so an attacker who also
    /// **deletes the `"file_count"` line** — no number rewritten, just more text removed — bypasses it
    /// entirely, and the removed entries become `Delete`s again:
    ///
    /// ```text
    /// 4 of 5 entries removed + "file_count" key deleted, each leg on a FRESH five-file tree
    ///   checkpoint_revert_one(f3) -> Ok(RevertOutcome { applied: 1, skipped: [] })  survivors f1,f2,f4,f5
    ///   checkpoint_revert         -> Ok(RevertOutcome { applied: 4, skipped: [] })  survivors ["f1.txt"]
    /// ```
    ///
    /// So the scope of this test is exactly the scope of the check: a tamper that removes entries and
    /// **leaves the count behind**. That is a consistency check on a possibly-edited record, not a bar
    /// an attacker has to clear. The Critical shape stays closed anyway, by the stand-down — which does
    /// not consult the count, so `files: {}` with the count deleted is still held back — and that is the
    /// previous test's job, not this one's.
    #[test]
    fn cpe_1847_a_files_map_edited_out_from_under_its_own_count_is_refused_on_every_route() {
        const CAPTURED: &[u8] = b"the user's five files";
        const LIVE: &[u8] = b"work done since the checkpoint";
        // Partial first, and cherry-revert first: the wider shape and the unprevewed route lead, so a
        // regression surfaces on the leg that matters most rather than on the easiest one.
        for empty_it_entirely in [false, true] {
            for whole_tree in [false, true] {
                let app = scratch("cpe1847-count-app");
                let ctx = HeadlessCtx::new(app.to_path_buf());
                let root = scratch("cpe1847-count-root");
                let root_s = root.to_string_lossy().to_string();
                for i in 1..=5 {
                    fs::write(root.join(format!("f{i}.txt")), CAPTURED).unwrap();
                }
                let created = checkpoint_create(&ctx, &root_s, "cp").unwrap();
                let id = created.checkpoint.manifest_id.clone();
                // Same dead-tamper insurance as the previous test: if the edit fails to land, the plan
                // becomes a real `Overwrite` of f1.txt and the `LIVE` assertion catches it.
                fs::write(root.join("f1.txt"), LIVE).unwrap();

                let after = tamper_manifest(&ctx, &root_s, &id, |v| {
                    if empty_it_entirely {
                        v["files"] = serde_json::json!({});
                    } else {
                        let obj = v["files"].as_object_mut().unwrap();
                        for i in 2..=5 {
                            obj.remove(&format!("f{i}.txt"));
                        }
                    }
                });
                assert_eq!(
                    after["files"].as_object().map(|o| o.len()),
                    Some(if empty_it_entirely { 0 } else { 1 }),
                    "fixture is inert: the entries must actually be gone from the file on disk"
                );
                assert_eq!(
                    after["file_count"],
                    serde_json::json!(5),
                    "fixture is inert: the count must be left at its captured value — restating it is \
                     the OTHER test's tamper, and this one certifies nothing if the count moved too"
                );

                let outcome = if whole_tree {
                    checkpoint_revert(&ctx, &root_s, &id)
                } else {
                    checkpoint_revert_one(&ctx, &root_s, &id, "f3.txt")
                };

                // THE HARM, before the `Result`.
                for i in 1..=5 {
                    assert!(
                        root.join(format!("f{i}.txt")).exists(),
                        "HARM: entries deleted from a manifest's `files` map turned a revert into a \
                         delete of f{i}.txt — the removal is invisible to every per-entry guard, \
                         because an absence cannot be told from an entry that was never written. \
                         Outcome was {outcome:?}"
                    );
                }
                assert_eq!(
                    fs::read(root.join("f1.txt")).ok().as_deref(),
                    Some(LIVE),
                    "fixture is inert: f1.txt was reverted to its captured bytes, so the entries were \
                     still in the manifest and the tamper never took"
                );

                let err = outcome.expect_err("a file list that contradicts its own count must refuse");
                assert!(err.contains(" 5 file"), "the refusal must name the count claimed: {err}");
                assert!(
                    err.contains(if empty_it_entirely { "has 0" } else { "has 1" }),
                    "the refusal must name what the list actually holds: {err}"
                );
                // The same refusal, on the read-only routes, so a user is told the store is wrong
                // rather than shown a smaller tree and left to wonder.
                assert!(checkpoint_preview_revert(&ctx, &root_s, &id, None).is_err(), "preview too");
                assert!(checkpoint_diff_file(&ctx, &root_s, &id, "f1.txt").is_err(), "diff too");

                let _ = fs::remove_dir_all(&root);
                let _ = fs::remove_dir_all(&app);
            }
        }
    }

    /// **CPE-1847's binding constraint: a genuine capture of an empty directory is a real manifest, and
    /// it is byte-identical to an emptied one.** This is what makes a naive "refuse `files: {}`" fix
    /// wrong, so it is pinned rather than argued about.
    ///
    /// The second half pins the deliberate **cost** of the stand-down — the one legitimate flow it
    /// changes — so nobody restores the old behaviour as a "bug fix" without meeting this test and the
    /// reasoning attached to it. Before the fix that flow measured `applied: 3` with the folder emptied;
    /// now the deletes are held back and named. That is a lost convenience against unrecoverable,
    /// silently-reported loss of a whole tree, and the checkpoint was never restoring anything in that
    /// flow — only authorising deletion.
    #[test]
    fn cpe_1847_a_genuine_capture_of_an_empty_directory_still_round_trips() {
        let app = scratch("cpe1847-genuine-app");
        let ctx = HeadlessCtx::new(app.to_path_buf());
        let root = scratch("cpe1847-genuine-root");
        let root_s = root.to_string_lossy().to_string();

        let created = checkpoint_create(&ctx, &root_s, "an empty folder").unwrap();
        let id = created.checkpoint.manifest_id.clone();
        assert_eq!(created.new_blobs, 0, "fixture is inert: an empty capture must store no blobs");
        assert!(created.skipped.is_empty(), "fixture is inert: nothing may have been skipped here");

        let p = store_dir_for(&ctx, &root_s).unwrap().join("manifests").join(format!("{id}.json"));
        let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(
            v["files"].as_object().map(|o| o.len()),
            Some(0),
            "fixture is inert: a capture of an empty folder must produce the very shape the attack \
             produces, or this test is not pinning the constraint it claims to"
        );
        assert_eq!(
            v["file_count"],
            serde_json::json!(0),
            "a genuine empty capture asserts zero rather than omitting the count"
        );

        // It loads, it previews, and reverting the UNCHANGED empty tree succeeds with nothing held
        // back. Any of these erroring would be the naive refusal this test exists to forbid.
        let preview = checkpoint_preview_revert(&ctx, &root_s, &id, None)
            .expect("a genuine empty capture must still preview");
        assert_eq!(preview.total, 0, "nothing to do: {preview:?}");
        let outcome = checkpoint_revert(&ctx, &root_s, &id)
            .expect("a genuine empty capture must still revert");
        assert_eq!((outcome.applied, outcome.skipped.len()), (0, 0), "{outcome:?}");

        // The recorded cost, pinned.
        for i in 1..=3 {
            fs::write(root.join(format!("g{i}.txt")), b"added after the checkpoint").unwrap();
        }
        let outcome = checkpoint_revert(&ctx, &root_s, &id).expect("still not an error");
        for i in 1..=3 {
            assert!(
                root.join(format!("g{i}.txt")).exists(),
                "a zero-entry checkpoint may not delete, even a genuine one — it holds nothing to \
                 restore in exchange, and it is indistinguishable from an emptied map: {outcome:?}"
            );
        }
        assert_eq!(outcome.applied, 0, "{outcome:?}");
        assert_eq!(outcome.skipped.len(), 3, "each held-back delete is reported: {outcome:?}");

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }

    #[test]
    fn diff_file_errors_cleanly_on_binary_content_and_oversize_and_unknown_path() {
        let app = scratch("app-data-diff-err");
        let ctx = HeadlessCtx::new(app.to_path_buf());
        let root = scratch("root-diff-err");
        let root_s = root.to_string_lossy().to_string();
        fs::write(root.join("bin.dat"), [0xFFu8, 0xFE, 0x00, 0x01]).unwrap(); // invalid UTF-8
        fs::write(root.join("huge.txt"), vec![b'x'; (DIFF_MAX_BYTES + 1) as usize]).unwrap();
        fs::write(root.join("ok.txt"), b"fine").unwrap();

        let created = checkpoint_create(&ctx, &root_s, "cp").unwrap();
        let id = &created.checkpoint.manifest_id;

        assert!(
            checkpoint_diff_file(&ctx, &root_s, id, "bin.dat").is_err(),
            "non-UTF-8 checkpoint content must error, not garble"
        );
        assert!(
            checkpoint_diff_file(&ctx, &root_s, id, "huge.txt").is_err(),
            "over-cap content must error, not truncate"
        );
        assert!(
            checkpoint_diff_file(&ctx, &root_s, id, "nope.txt").is_err(),
            "a path absent from the checkpoint must error"
        );
        assert!(checkpoint_diff_file(&ctx, &root_s, id, "ok.txt").is_ok());

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }

    #[test]
    fn diff_file_refuses_a_path_that_escapes_root() {
        let app = scratch("app-data-diff-escape");
        let ctx = HeadlessCtx::new(app.to_path_buf());
        let root = scratch("root-diff-escape");
        let root_s = root.to_string_lossy().to_string();
        fs::write(root.join("a.txt"), b"content").unwrap();
        let created = checkpoint_create(&ctx, &root_s, "cp").unwrap();

        // Not in the checkpoint (never scanned as "../secret"), so this is refused on lookup already, but
        // also exercises that a traversal-shaped rel_path never reaches `safe_target`'s live-file read.
        assert!(checkpoint_diff_file(&ctx, &root_s, &created.checkpoint.manifest_id, "../secret").is_err());

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }

    // ---- failed checkpoint attempts (CPE-1600) --------------------------------------------------

    #[test]
    fn recorded_failure_is_listed_newest_first_and_is_separate_from_the_checkpoint_list() {
        let app = scratch("app-data-fail");
        let ctx = HeadlessCtx::new(app.to_path_buf());
        let root = scratch("root-fail");
        let root_s = root.to_string_lossy().to_string();
        fs::write(root.join("a.txt"), b"a").unwrap();

        // A real checkpoint succeeds too, so the failure list and the checkpoint list can be told apart.
        let created = checkpoint_create(&ctx, &root_s, "ok one").unwrap();

        record_checkpoint_failure(&ctx, &root_s, "Before batch media overwrite", "disk is read-only")
            .unwrap();
        record_checkpoint_failure(&ctx, &root_s, "Before removing clutter", "permission denied").unwrap();

        let failures = checkpoint_failures_list(&ctx, &root_s).unwrap();
        assert_eq!(failures.len(), 2, "both attempts recorded");
        // Newest first.
        assert_eq!(failures[0].operation, "Before removing clutter");
        assert_eq!(failures[0].reason, "permission denied");
        assert_eq!(failures[1].operation, "Before batch media overwrite");
        assert_eq!(failures[1].reason, "disk is read-only");

        // The success and the failures never mix: `checkpoint_list` still shows only the one real
        // checkpoint, and nothing about it resembles a `Checkpoint` (no `manifest_id` field at all —
        // this is a compile-time guarantee, not just a runtime one, since `CheckpointFailure` has no
        // such field to check).
        let checkpoints = checkpoint_list(&ctx, &root_s).unwrap();
        assert_eq!(checkpoints.len(), 1);
        assert_eq!(checkpoints[0].manifest_id, created.checkpoint.manifest_id);

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }

    #[test]
    fn a_missing_failures_file_degrades_to_an_empty_list() {
        let app = scratch("app-data-fail-empty");
        let ctx = HeadlessCtx::new(app.to_path_buf());
        let root = scratch("root-fail-empty");
        let root_s = root.to_string_lossy().to_string();

        // Never recorded a failure for this root — no store dir exists at all yet.
        assert!(checkpoint_failures_list(&ctx, &root_s).unwrap().is_empty());

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }

    #[test]
    fn repeated_failures_rotate_at_the_cap_keeping_the_newest() {
        // CPE-1600 volume guard: a persistently broken root (e.g. a read-only drive hit on every batch
        // run) must not grow the failures index without limit — it rotates at `MAX_CHECKPOINT_FAILURES`,
        // same shape as the audit journal's per-session cap.
        let app = scratch("app-data-fail-rotate");
        let ctx = HeadlessCtx::new(app.to_path_buf());
        let root = scratch("root-fail-rotate");
        let root_s = root.to_string_lossy().to_string();

        for i in 0..(MAX_CHECKPOINT_FAILURES + 10) {
            record_checkpoint_failure(&ctx, &root_s, "Before batch media overwrite", &format!("attempt {i}"))
                .unwrap();
        }

        let failures = checkpoint_failures_list(&ctx, &root_s).unwrap();
        assert_eq!(failures.len(), MAX_CHECKPOINT_FAILURES, "capped, oldest rotated out");
        // Newest-first: the very last attempt recorded is first in the list.
        assert_eq!(failures[0].reason, format!("attempt {}", MAX_CHECKPOINT_FAILURES + 9));
        // The oldest surviving entry is exactly the cap's worth back from the newest — the first 10
        // attempts (0..10) were rotated out.
        assert_eq!(failures.last().unwrap().reason, "attempt 10");

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }

    #[test]
    fn a_malformed_failure_line_is_skipped_not_fatal() {
        let app = scratch("app-data-fail-torn");
        let ctx = HeadlessCtx::new(app.to_path_buf());
        let root = scratch("root-fail-torn");
        let root_s = root.to_string_lossy().to_string();

        record_checkpoint_failure(&ctx, &root_s, "Before metadata edit", "locked file").unwrap();
        let store_dir = store_dir_for(&ctx, &root_s).unwrap();
        {
            let mut f = OpenOptions::new().append(true).open(failures_file(&store_dir)).unwrap();
            writeln!(f, "{{ not valid json").unwrap();
        }
        record_checkpoint_failure(&ctx, &root_s, "Before removing similar images", "network drive gone")
            .unwrap();

        let failures = checkpoint_failures_list(&ctx, &root_s).unwrap();
        assert_eq!(failures.len(), 2, "the torn line is skipped, not counted or fatal");
        assert_eq!(failures[0].operation, "Before removing similar images");
        assert_eq!(failures[1].operation, "Before metadata edit");

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }
}
