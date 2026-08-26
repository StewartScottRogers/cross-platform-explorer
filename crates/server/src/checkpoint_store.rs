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
//!   checkpoints.json        THIS module's checkpoint index (append-only JSON-lines, retention-reconciled)
//! ```
//! `<root_key>` is the SHA-256 of the absolute root path, so two different roots never collide and no
//! user path ever leaks into a directory name (the same "safe single segment" concern
//! [`crate::audit_journal::record`] solves by sanitising a session id).
//!
//! ## Why `checkpoints.json` is JSON-**lines** and tolerant-read
//! Like the audit journal it is append-only in the common case: [`checkpoint_create`] writes exactly one
//! flushed line per checkpoint, and [`read_checkpoints`] reads them back **skipping any malformed line** —
//! a torn/partial trailing write (or a hand-edit) degrades to "ignore that one record", never a crash or a
//! lost index, exactly as [`crate::audit_journal::read_session`] degrades. Missing file → empty list.
//! Newest-first on read for the UI.
//!
//! **CPE-1862 — reconciled, not purely append-only.** Nothing about this file's rows tracks whether the
//! manifest they name is still on disk, and [`snapshot_prune::apply`] (reached from
//! [`checkpoint_prune_apply`]) deletes manifests without this module in the loop at all — so a retention
//! pass used to leave rows here naming manifests that no longer existed, and the UI listed a checkpoint
//! that would error the moment the user tried to restore it. [`checkpoint_prune_apply`] now rewrites this
//! file after every pass to drop what retention just removed, **and** [`checkpoint_list`] independently
//! filters every read against [`snapshot_capture::list_manifests`]'s live/loadable set — the backstop for
//! a manifest that's present but fails CPE-1861's identity checks, which retention deliberately never
//! prunes (leak over corruption) and so the write-time reconciliation alone could never catch. See both
//! functions' docs for the full reasoning and the trade each makes.
//!
//! Std + serde only — no new dependencies, not feature-gated (like the engines it drives). The revert
//! path preserves the engine's skip-on-error guarantee: a single unreadable/locked file is reported in
//! [`RevertOutcome::skipped`], not fatal to the rest of the revert. Since CPE-1845 each such entry also
//! carries an [`OpOutcome`] saying whether it FAILED or was deliberately HELD BACK (and, if held back,
//! whether re-running can help), with the shared explanation stated once in [`RevertOutcome::held_back`].

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::audit_journal;
use crate::ctx::ServerCtx;
use crate::fsutil::to_epoch_ms;
use crate::model::{OpOutcome, OpResult};
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
/// not, preserving the engine's skip-on-error guarantee.
///
/// **CPE-1845.** Two things live in `skipped` that are not the same thing: an action that was *attempted
/// and failed*, and a delete the engine *deliberately declined to perform* because it could not trust the
/// checkpoint. Every entry now carries [`cpe_server::model::OpOutcome`](crate::model::OpOutcome) saying
/// which, so a consumer branches on a field rather than on the wording of `error`; and the explanation
/// shared by a whole group of hold-backs is stated **once**, in [`RevertOutcome::held_back`], instead of
/// being copied onto every path.
#[derive(Debug, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RevertOutcome {
    /// Actions applied successfully.
    pub applied: u32,
    /// Actions that did not apply — genuine failures **and** deliberate hold-backs, in that order.
    /// Read each entry's `outcome` to tell them apart; `error` carries only what is specific to that
    /// path (empty for most hold-backs — their shared explanation is in
    /// [`held_back`](RevertOutcome::held_back)). Never fatal to the rest of the revert.
    pub skipped: Vec<OpResult>,
    /// Present when the revert deliberately held its deletions back: the single explanation, the count,
    /// and a next step honest about whether re-running can help. `None` when nothing was held back.
    pub held_back: Option<HeldBackSummary>,
}

/// The one statement behind a whole group of held-back deletes (CPE-1845), so 500 hold-backs cost one
/// paragraph rather than 500 copies of it (~185 KB, measured in CPE-1847).
#[derive(Debug, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct HeldBackSummary {
    /// Which hold-back this is — `skipped_by_plan` (retryable) or `held_back_by_checkpoint` (not
    /// retryable on this platform). The same discriminant every entry in `skipped` carries.
    pub outcome: OpOutcome,
    /// How many deletes this covers. Pair it with `reason` for the "one statement plus a count" the UI
    /// renders instead of N identical rows.
    pub count: u32,
    /// The shared explanation, stated once.
    pub reason: String,
    /// What the user can actually do next. For `held_back_by_checkpoint` this states that re-running
    /// cannot help and gives the alternative; it never says "re-run".
    pub next_step: String,
    /// Convenience mirror of [`OpOutcome::retryable`] so a template can branch without re-deriving it.
    pub retryable: bool,
    /// **CPE-1869.** `true` when `next_step` actually tells the user to go delete these paths themselves
    /// — mirrors [`crate::revert_engine::HeldBack::advises_manual_delete`], see its doc for why a consumer
    /// must read this field rather than infer it from `next_step`'s wording. A "copy every held-back path"
    /// affordance belongs behind this, not behind `outcome == held_back_by_checkpoint` alone: that
    /// discriminant is also true of the alias/collision hold-back, where the paths are the checkpoint's
    /// OWN content under another spelling and offering to delete them would be the bug this field exists
    /// to prevent.
    pub advises_manual_delete: bool,
}

impl RevertOutcome {
    fn from_report(report: RestoreReport) -> Self {
        // Genuine failures first, then the hold-backs, so `skipped.len()` keeps meaning "actions that
        // did not happen" for every existing caller while the *kind* of each is now readable.
        let mut skipped: Vec<OpResult> = report
            .skipped
            .into_iter()
            .map(|(path, error)| OpResult::err(std::path::Path::new(&path), error))
            .collect();
        let held_back = report.held_back.map(|group| {
            for (path, detail) in &group.paths {
                skipped.push(OpResult::held_back(path, group.outcome, detail.clone()));
            }
            HeldBackSummary {
                outcome: group.outcome.as_outcome(),
                count: group.paths.len() as u32,
                reason: group.reason,
                next_step: group.next_step,
                retryable: group.outcome.retryable(),
                advises_manual_delete: group.advises_manual_delete,
            }
        });
        Self { applied: report.applied as u32, skipped, held_back }
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

/// **CPE-1862.** Rewrite `store_dir`'s checkpoint index so it names only `keep_ids`, dropping any row
/// whose `manifest_id` retention just removed (or that was already dangling from before this function
/// existed — see the module doc). Preserves on-disk (append/oldest-first) order for the rows that
/// survive; a malformed line is left in place untouched (the same line [`read_checkpoints`] already
/// skips on read, so leaving it costs nothing and a rewrite is not the place to also silently drop
/// unrelated garbage). Crash-safe temp-file + rename, mirroring [`trim_failures`]. A missing index, or
/// one where nothing needs dropping, is a no-op — no write, no rename.
fn reconcile_checkpoints(store_dir: &Path, keep_ids: &BTreeSet<String>) -> Result<(), String> {
    let file = index_file(store_dir);
    let content = match fs::read_to_string(&file) {
        Ok(c) => c,
        Err(_) => return Ok(()), // nothing recorded yet — nothing to reconcile
    };
    let mut dropped_any = false;
    let mut kept_lines: Vec<&str> = Vec::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Checkpoint>(line) {
            Ok(cp) if !keep_ids.contains(&cp.manifest_id) => dropped_any = true, // dangling — drop the row
            _ => kept_lines.push(line), // still live, or unparseable (leave as-is; read skips it anyway)
        }
    }
    if !dropped_any {
        return Ok(());
    }
    let tmp = file.with_extension("json.tmp");
    let mut body = kept_lines.join("\n");
    if !kept_lines.is_empty() {
        body.push('\n');
    }
    fs::write(&tmp, body).map_err(|e| e.to_string())?;
    // CPE-1710: app-private store, same as `trim_failures` below — a deliberate atomic replace of our
    // own file, never a user-supplied path.
    #[allow(clippy::disallowed_methods)]
    fs::rename(&tmp, &file).map_err(|e| e.to_string())?;
    Ok(())
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
///
/// **CPE-1862.** Filtered against [`snapshot_capture::list_manifests`] — the same "fit to steer a
/// retention decision" set `snapshot_prune::apply` itself plans against — so a row is only ever
/// returned if its manifest both exists **and** is one [`snapshot_capture::load_manifest`] will accept.
/// That excludes two things `checkpoints.json` alone cannot tell apart from a good row:
/// - a manifest retention has since pruned (the row [`checkpoint_prune_apply`] otherwise reconciles
///   away already, but this catches anything from before that existed, or any other way the two files
///   could drift);
/// - a manifest that is *present but unloadable* per CPE-1861's identity rules (inner id disagreeing
///   with its filename, a crafted stem, a `file_count`/hash that contradicts the file's own content).
///   Retention deliberately never prunes that file (leak-over-corruption), so nothing ever removes its
///   `checkpoints.json` row either — this is the only place that can stop it from looking selectable.
///   The manifest itself is left on disk untouched; only the misleading "you can restore this" listing
///   is suppressed. There is no further UI text to add here: nothing can be done with it, and its
///   presence on disk is preserved for the same reason `prune` leaks it — recoverable by hand, never by
///   silently and speculatively "fixing" a record that might be evidence of tampering.
///
/// A checkpoint a user can see is therefore always one they can act on (the acceptance bar this ticket
/// sets): [`checkpoint_preview_revert`]/[`checkpoint_revert`]/[`checkpoint_revert_one`] all resolve the
/// same `manifest_id` through the same [`snapshot_capture::load_manifest`], so nothing offered here can
/// diverge from what a click will actually do.
pub fn checkpoint_list(ctx: &dyn ServerCtx, root: &str) -> Result<Vec<Checkpoint>, String> {
    let store_dir = store_dir_for(ctx, root)?;
    let store = store_dir.to_string_lossy().to_string();
    let live: BTreeSet<String> =
        snapshot_capture::list_manifests(&store)?.into_iter().map(|m| m.id).collect();
    Ok(read_checkpoints(&store_dir).into_iter().filter(|c| live.contains(&c.manifest_id)).collect())
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
///
/// **CPE-1862.** `snapshot_prune::apply` deletes manifest files; until now nothing told
/// `checkpoints.json` a row's manifest was gone, so the UI kept listing checkpoints that would error on
/// `load_manifest` the moment the user tried to act on one. Reconciled here, right after the deletion,
/// because retention is already the thing mutating the manifest store for this root — making it also
/// retire the index rows that named what it just removed keeps `checkpoints.json` bounded to what is
/// actually still on disk, rather than growing forever with dead rows nothing will ever restore. (The
/// alternative the ticket poses, filtering only at read time, was rejected as the *sole* fix for exactly
/// that reason — see [`checkpoint_list`]'s doc for why a read-time filter still exists too, as the
/// backstop for the one case reconciliation here structurally cannot reach: a manifest that is present
/// but never pruned at all.)
///
/// Reconciliation failure is deliberately **not** propagated: the manifests are already deleted by the
/// time this runs (retention's destructive part is done and must not be undone by a bookkeeping write
/// failing), and a `checkpoints.json` this call couldn't rewrite still can't mislead the user —
/// [`checkpoint_list`]'s own live-manifest filter hides any row whose manifest isn't
/// [`snapshot_capture::list_manifests`]-fit regardless of whether this reconciliation succeeded.
pub fn checkpoint_prune_apply(
    ctx: &dyn ServerCtx,
    root: &str,
    policy: &RetentionPolicy,
    max_total_bytes: Option<u64>,
) -> Result<RetentionApplyResult, String> {
    let store_dir = store_dir_for(ctx, root)?;
    let store = store_dir.to_string_lossy().to_string();
    let result = snapshot_prune::apply(&store, policy, max_total_bytes)?;
    let survivors: BTreeSet<String> = result.kept.iter().cloned().collect();
    let _ = reconcile_checkpoints(&store_dir, &survivors); // best-effort — see doc above
    Ok(result)
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
            // CPE-1845 made this structural. Was: every entry had to *start with* the prose
            // `"not deleted:"`, and the count had to be findable inside that same prose — repeated
            // verbatim on all five. Now the state is a typed field on each entry and the shared
            // explanation is a single summary carrying the count as a number.
            for op in &outcome.skipped {
                assert_eq!(
                    op.outcome,
                    OpOutcome::HeldBackByCheckpoint,
                    "a held-back delete must be flagged structurally, and an empty checkpoint is the \
                     NOT-retryable kind — its bytes read the same on every re-run: {op:?}"
                );
                assert!(!op.ok, "a hold-back is not an applied action: {op:?}");
            }
            let summary = outcome
                .held_back
                .as_ref()
                .expect("a revert that held deletes back must carry the one-statement summary");
            assert_eq!(summary.outcome, OpOutcome::HeldBackByCheckpoint);
            assert!(!summary.retryable, "an empty checkpoint cannot be fixed by re-running: {summary:?}");
            assert_eq!(
                summary.count as usize, expected_held,
                "the summary must carry the count of what would have been deleted: {summary:?}"
            );
            assert!(
                summary.reason.contains(&format!("{expected_held} file")),
                "the one statement must still say how many: {summary:?}"
            );
            assert!(
                !summary.next_step.is_empty() && !summary.next_step.contains("run the revert again"),
                "a non-retryable hold-back must offer a real next step and must NOT say re-run: \
                 {summary:?}"
            );

            let _ = fs::remove_dir_all(&root);
            let _ = fs::remove_dir_all(&app);
        }
    }

    /// **CPE-1845 — the acceptance test: a consumer separates all four states with every message
    /// string erased.**
    ///
    /// Before this ticket the only way to tell a deliberate hold-back from a genuine failure was
    /// `error.starts_with("not deleted:")`, and the two *kinds* of hold-back — one retryable, one
    /// permanently unfixable on this platform — were the same string channel with different prose. So
    /// this test blanks **every** human-readable field on the wire shape (`error`, `reason`,
    /// `next_step`) and then asks a consumer to classify. If the discriminant is real, erasing the prose
    /// costs nothing; if any two states share one discriminant, the buckets collapse and this goes red.
    ///
    /// The four states come from three real runs of the production [`execute_restore`], mapped to the
    /// wire by the production [`RevertOutcome::from_report`] — nothing here is hand-built.
    #[test]
    fn cpe_1845_a_consumer_tells_the_four_states_apart_with_every_message_erased() {
        use crate::restore_plan::{FileState, RestoreOp, Snapshot};

        let store = scratch("1845-store");
        let blobs = store.join("blobs");
        fs::create_dir_all(&blobs).unwrap();
        // One real blob (so a write can succeed) and one hash with NO blob (so a write must fail).
        let present = "1845aaaa";
        let missing = "1845bbbb";
        fs::write(blobs.join(present), b"restored bytes").unwrap();
        assert!(
            !blobs.join(missing).exists(),
            "fixture is inert: the missing-blob hash must NOT exist, or the failure leg never fails \
             and this test certifies nothing"
        );

        // --- run 1: everything works → Applied, and nothing else. ---
        let root_ok = scratch("1845-ok");
        let mut cp_ok = Snapshot::new();
        cp_ok.insert("restored.txt".to_string(), FileState::new(present, 14));
        let applied_run = RevertOutcome::from_report(execute_restore(
            &[RestoreAction { path: "restored.txt".to_string(), op: RestoreOp::Create }],
            &root_ok.to_string_lossy(),
            &store.to_string_lossy(),
            &cp_ok,
        ));
        assert_eq!(applied_run.applied, 1, "fixture is inert: run 1 applied nothing: {applied_run:?}");

        // --- run 2: a write whose blob is gone, WITH a delete in the plan → Failed (the write) and
        // SkippedByPlan (the delete, held back because the write's failure makes its premise unproven).
        let root_mixed = scratch("1845-mixed");
        fs::write(root_mixed.join("added.txt"), b"user file").unwrap();
        let mut cp_mixed = Snapshot::new();
        cp_mixed.insert("gone.txt".to_string(), FileState::new(missing, 9));
        let mixed_run = RevertOutcome::from_report(execute_restore(
            &[
                RestoreAction { path: "gone.txt".to_string(), op: RestoreOp::Create },
                RestoreAction { path: "added.txt".to_string(), op: RestoreOp::Delete },
            ],
            &root_mixed.to_string_lossy(),
            &store.to_string_lossy(),
            &cp_mixed,
        ));
        assert!(
            root_mixed.join("added.txt").exists(),
            "fixture is inert: run 2's delete actually ran, so nothing was held back: {mixed_run:?}"
        );

        // --- run 3: an empty checkpoint with a delete planned → HeldBackByCheckpoint (permanent). ---
        let root_empty = scratch("1845-empty");
        fs::write(root_empty.join("added.txt"), b"user file").unwrap();
        let held_run = RevertOutcome::from_report(execute_restore(
            &[RestoreAction { path: "added.txt".to_string(), op: RestoreOp::Delete }],
            &root_empty.to_string_lossy(),
            &store.to_string_lossy(),
            &Snapshot::new(),
        ));
        assert!(
            root_empty.join("added.txt").exists(),
            "fixture is inert: run 3's delete actually ran: {held_run:?}"
        );

        // ---- ERASE EVERY MESSAGE. From here on there is no prose left to match. ----
        let mut runs = vec![applied_run, mixed_run, held_run];
        for run in &mut runs {
            for op in &mut run.skipped {
                op.error = String::new();
            }
            if let Some(h) = run.held_back.as_mut() {
                h.reason = String::new();
                h.next_step = String::new();
            }
        }

        /// A consumer of the wire shape, written the way a UI must be: it reads `outcome` and nothing
        /// else. Returns every state present in this outcome.
        fn states_seen(o: &RevertOutcome) -> std::collections::BTreeSet<String> {
            let mut seen = std::collections::BTreeSet::new();
            if o.applied > 0 {
                seen.insert(format!("{:?}", OpOutcome::Applied));
            }
            for op in &o.skipped {
                seen.insert(format!("{:?}", op.outcome));
            }
            seen
        }

        let applied_states = states_seen(&runs[0]);
        let mixed_states = states_seen(&runs[1]);
        let held_states = states_seen(&runs[2]);

        assert_eq!(
            applied_states,
            ["Applied"].map(String::from).into_iter().collect(),
            "run 1 must read as applied-only with no prose: {:?}",
            runs[0]
        );
        assert_eq!(
            mixed_states,
            ["Failed", "SkippedByPlan"].map(String::from).into_iter().collect(),
            "run 2 must separate the genuine failure from the retryable hold-back with no prose: {:?}",
            runs[1]
        );
        assert_eq!(
            held_states,
            ["HeldBackByCheckpoint"].map(String::from).into_iter().collect(),
            "run 3 must read as the NON-retryable hold-back with no prose: {:?}",
            runs[2]
        );

        // All four states seen, all four distinct — this is the claim the ticket makes.
        let all: std::collections::BTreeSet<String> =
            applied_states.union(&mixed_states).cloned().collect::<std::collections::BTreeSet<_>>()
                .union(&held_states).cloned().collect();
        assert_eq!(
            all.len(),
            4,
            "the four states must be four discriminants, not two states sharing one: {all:?}"
        );

        // And the retryable/non-retryable split — the second half of the ticket — is also structural,
        // and is the one the "re-run after fixing" wording depends on.
        assert!(
            runs[1].held_back.as_ref().is_some_and(|h| h.retryable),
            "a locked/missing-blob hold-back IS retryable: {:?}",
            runs[1]
        );
        assert!(
            runs[2].held_back.as_ref().is_some_and(|h| !h.retryable),
            "an empty-checkpoint hold-back is NOT retryable on this platform: {:?}",
            runs[2]
        );

        // Finally, the volume claim: one statement for the whole group, never one copy per path.
        let big_root = scratch("1845-volume");
        let mut plan = Vec::new();
        for i in 0..200 {
            let name = format!("added-{i}.txt");
            fs::write(big_root.join(&name), b"x").unwrap();
            plan.push(RestoreAction { path: name, op: RestoreOp::Delete });
        }
        let big = RevertOutcome::from_report(execute_restore(
            &plan,
            &big_root.to_string_lossy(),
            &store.to_string_lossy(),
            &Snapshot::new(),
        ));
        assert_eq!(big.skipped.len(), 200, "all 200 deletes must be accounted for: {:?}", big.held_back);
        let prose: usize = big.skipped.iter().map(|o| o.error.len()).sum();
        assert_eq!(
            prose, 0,
            "the shared explanation must NOT be copied onto each path — that is the ~185 KB CPE-1847 \
             measured; {prose} bytes of per-path prose found"
        );
        let summary = big.held_back.as_ref().expect("200 hold-backs must carry one summary");
        assert_eq!(summary.count, 200, "one statement plus a count: {summary:?}");
        assert!(
            summary.reason.len() < 1024,
            "the one statement must be one statement: {} bytes",
            summary.reason.len()
        );

        for d in [store, root_ok, root_mixed, root_empty, big_root] {
            let _ = fs::remove_dir_all(&d);
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

    /// CPE-1861 end-to-end through the **registered commands** — `checkpoint_create`,
    /// `checkpoint_prune_apply`, `checkpoint_revert`, the three `snapshot_run_due` and the UI actually
    /// drive. The unit fixtures assert the store's state; this one asserts the thing the user has:
    /// a file on disk with the right bytes in it, after a retention pass they never asked for.
    ///
    /// CPE-1847's reverted fix measured here as `prune_apply -> kept: [id], pruned: [id-backup]`,
    /// `blobs = []`, and then `checkpoint_revert -> applied: 0, skipped: [a.txt: blobs/…: cannot find
    /// the file]` with `a.txt` still reading `"damaged"` — content destroyed, complete success reported,
    /// unattended.
    #[test]
    fn cpe_1861_a_duplicated_manifest_file_cannot_cost_a_revert_its_content() {
        let app = scratch("app-data-dup");
        let ctx = HeadlessCtx::new(app.to_path_buf());
        let root = scratch("root-dup");
        let root_s = root.to_string_lossy().to_string();
        fs::write(root.join("a.txt"), b"original").unwrap();

        let created = checkpoint_create(&ctx, &root_s, "before").unwrap();
        let id = created.checkpoint.manifest_id.clone();
        let store_dir = store_dir_for(&ctx, &root_s).unwrap();
        let mdir = store_dir.join("manifests");

        // The trigger: a second copy of the manifest file. Explorer copy/paste, a cloud-sync conflict
        // copy, a backup script — no user action inside this app at all.
        fs::copy(mdir.join(format!("{id}.json")), mdir.join(format!("{id} - Copy.json"))).unwrap();

        // FIXTURE LIVENESS — the copy is on disk, parses as a manifest, and names the same blob as the
        // original (which is what made pruning it destructive).
        assert!(mdir.join(format!("{id} - Copy.json")).exists(), "LIVE: the copy is not on disk");
        let copy_doc: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(mdir.join(format!("{id} - Copy.json"))).unwrap())
                .unwrap();
        let hash = copy_doc["files"]["a.txt"]["hash"].as_str().unwrap().to_string();
        assert!(store_dir.join("blobs").join(&hash).exists(), "LIVE: the shared blob is missing");

        let policy = RetentionPolicy { hourly: 2, daily: 0, weekly: 0, monthly: 0 };
        let retained = checkpoint_prune_apply(&ctx, &root_s, &policy, None).unwrap();
        assert!(!retained.kept.is_empty());

        // Now the ordinary thing the checkpoint exists for: the tree is damaged and reverted.
        fs::write(root.join("a.txt"), b"damaged").unwrap();
        let kept = retained.kept[0].clone();
        let outcome = checkpoint_revert(&ctx, &root_s, &kept).unwrap();

        // Content first, Result second — the order CPE-1847 established, because "complete success
        // reported" was the failure mode.
        assert_eq!(
            fs::read(root.join("a.txt")).unwrap(),
            b"original",
            "HARM: the checkpoint retention says it kept could not put the file back"
        );
        assert_eq!(outcome.applied, 1);
        assert!(outcome.skipped.is_empty(), "HARM: {:?}", outcome.skipped);

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }

    // ---- CPE-1844, through the registered commands -------------------------------------------------

    /// Rewrite `root`'s snapshot-store `index.json` so every blob claims `size` bytes, and read it back
    /// so a tamper that failed to land cannot be mistaken for a guard that worked. Returns the total the
    /// file now claims — which is exactly what `store_total_bytes` used to hand the byte cap.
    fn tamper_index_sizes(store_dir: &std::path::Path, size: u64) -> u64 {
        let p = store_dir.join("index.json");
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
        let claimed: u64 = blobs.values().map(|m| m["size"].as_u64().unwrap()).sum();
        assert_eq!(
            crate::snapshot_capture::load_store(store_dir).unwrap().total_bytes(),
            claimed,
            "LIVE: the tamper never reached index.json as the production reader sees it"
        );
        claimed
    }

    /// **CPE-1844 end-to-end through the registered commands** — `checkpoint_create`,
    /// `checkpoint_prune_apply` and `checkpoint_revert`, which is what `snapshot_prune_apply` and
    /// `snapshot_run_due` actually drive. The unit fixtures assert the store's state; this one asserts
    /// the thing the user has: their earlier checkpoints still there, and still able to put a file back,
    /// after a retention pass whose byte cap was handed a number out of a hand-edited file.
    ///
    /// Measured on `origin/main` before anything changed, five checkpoints, real footprint 45 bytes,
    /// cap 1,000,000:
    ///
    /// ```text
    /// index.json: every blob's "size" -> 1000000000
    ///   CMD prune_apply  kept=[newest]  pruned=[the other four]  bytes_freed=4000000000
    ///   manifests left on disk = 1 of 5
    /// ```
    #[test]
    fn cpe_1844_a_hand_edited_index_cannot_prune_the_users_other_checkpoints() {
        let app = scratch("app-data-1844");
        let ctx = HeadlessCtx::new(app.to_path_buf());
        let root = scratch("root-1844");
        let root_s = root.to_string_lossy().to_string();

        let mut ids = Vec::new();
        for i in 0..5u64 {
            fs::write(root.join("a.txt"), format!("version {i}").as_bytes()).unwrap();
            ids.push(checkpoint_create(&ctx, &root_s, &format!("c{i}")).unwrap().checkpoint.manifest_id);
        }
        let store_dir = store_dir_for(&ctx, &root_s).unwrap();
        let store_s = store_dir.to_string_lossy().to_string();

        // A day between captures, so the GFS pass keeps all five and the byte cap is the only thing in
        // this test that can delete anything.
        for (i, id) in ids.iter().enumerate() {
            let p = store_dir.join("manifests").join(format!("{id}.json"));
            let mut doc: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
            doc["created_ms"] = serde_json::json!(1_700_000_000_000u64 + (i as u64) * 86_400_000);
            fs::write(&p, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
        }

        let policy = RetentionPolicy { hourly: 0, daily: 100, weekly: 0, monthly: 0 };
        let cap = 1_000_000u64;
        let before = checkpoint_prune_preview(&ctx, &root_s, &policy).unwrap();
        assert!(
            before.prune.is_empty(),
            "LIVE: the GFS pass would prune on its own, so this fixture does not isolate the byte cap"
        );
        assert!(before.total_bytes < cap, "LIVE: the honest store is not under the cap");

        let claimed = tamper_index_sizes(&store_dir, 1_000_000_000);
        assert!(claimed > cap, "LIVE: the tampered claim does not even exceed the cap");
        assert_eq!(
            crate::snapshot_capture::list_manifests(&store_s).unwrap().len(),
            5,
            "LIVE: the planner no longer sees five checkpoints, so the cap would have nothing to delete"
        );

        let applied = checkpoint_prune_apply(&ctx, &root_s, &policy, Some(cap)).unwrap();

        // HARM FIRST — the user's earlier checkpoints are still on disk, and the oldest can still put
        // its file back. Only then the Result.
        let mut left: Vec<String> = fs::read_dir(store_dir.join("manifests"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        left.sort();
        assert_eq!(left.len(), 5, "HARM: a hand-edited index.json deleted checkpoints — left {left:?}");

        fs::write(root.join("a.txt"), b"damaged").unwrap();
        let out = checkpoint_revert(&ctx, &root_s, &ids[0]).unwrap();
        assert_eq!(
            fs::read(root.join("a.txt")).unwrap(),
            b"version 0",
            "HARM: the oldest checkpoint could not put the file back after the retention pass"
        );
        assert_eq!(out.applied, 1);
        assert!(applied.pruned.is_empty(), "HARM: pruned {:?}", applied.pruned);
        assert_eq!(applied.kept.len(), 5);
        assert_eq!(applied.bytes_freed, 0, "nothing was deleted, so nothing was freed");

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }

    /// **CPE-1844's dedup sink, through the registered commands.** An `index.json` entry is a claim that
    /// the store holds those bytes; `capture` used to honour it by writing nothing. With the blob file
    /// gone and its index entry left behind — `prune`'s own documented leak-over-corruption residue, or
    /// a partial restore-from-backup of a store — a fresh checkpoint of that content stored none of it:
    ///
    /// ```text
    /// checkpoint_create        -> Ok, a checkpoint recorded; blobs/<hash> still absent
    /// checkpoint_revert(it)    -> Ok(applied: 0, skipped: [a.txt: stored copy (blob <hash>) could not be read])
    /// a.txt still reads "damaged"
    /// ```
    #[test]
    fn cpe_1844_a_checkpoint_taken_over_a_missing_blob_can_still_put_the_file_back() {
        let app = scratch("app-data-1844-dedup");
        let ctx = HeadlessCtx::new(app.to_path_buf());
        let root = scratch("root-1844-dedup");
        let root_s = root.to_string_lossy().to_string();

        fs::write(root.join("a.txt"), b"the user only copy").unwrap();
        checkpoint_create(&ctx, &root_s, "first").unwrap();
        let store_dir = store_dir_for(&ctx, &root_s).unwrap();
        let doc: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(store_dir.join("index.json")).unwrap()).unwrap();
        let hash = doc["blobs"].as_object().unwrap().keys().next().unwrap().clone();

        fs::remove_file(store_dir.join("blobs").join(&hash)).unwrap();
        assert!(!store_dir.join("blobs").join(&hash).exists(), "LIVE: the blob file is still on disk");
        let back: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(store_dir.join("index.json")).unwrap()).unwrap();
        assert!(
            back["blobs"].as_object().unwrap().contains_key(&hash),
            "LIVE: the index no longer claims the blob, so dedup would not fire"
        );

        let second = checkpoint_create(&ctx, &root_s, "second").unwrap();
        fs::write(root.join("a.txt"), b"damaged").unwrap();
        let out = checkpoint_revert(&ctx, &root_s, &second.checkpoint.manifest_id).unwrap();

        assert_eq!(
            fs::read(root.join("a.txt")).unwrap(),
            b"the user only copy",
            "HARM: a checkpoint reported as created held none of the file's content"
        );
        assert_eq!(out.applied, 1);
        assert!(out.skipped.is_empty(), "HARM: {:?}", out.skipped);

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }

    /// **CPE-1844 round 2, through the registered commands** — the security audit's B1/B5, which
    /// measured the pre-witness fix reproducing this ticket's own opening outcome from a file dropped in
    /// `blobs/`, with no `index.json` edit:
    ///
    /// ```text
    /// File::create("blobs/dead") + set_len(2_000_000_000)
    ///   preview.total_bytes 45 -> 2000000045
    ///   CMD prune_apply kept=1 pruned=4 bytes_freed=36 manifests_left=1
    ///   revert(oldest) = Err(cannot find the file);  a.txt = "damaged"
    /// orphan blob of 4 MB (capture's own crash residue, no attacker)
    ///   preview.total_bytes 45 -> 4000045; same destruction, and PERMANENT — an index tamper
    ///   self-heals when save_store rewrites honest sizes, a stray file does not
    /// ```
    #[test]
    fn cpe_1844_a_file_dropped_in_blobs_cannot_prune_the_users_checkpoints() {
        for (mode, name, len) in
            [("planted", "dead", 2_000_000_000u64), ("orphan", "00ff00ff", 4_000_000), ("hardlink", "beef", 500_000_000)]
        {
            let app = scratch(&format!("app-1844-w-{mode}"));
            let ctx = HeadlessCtx::new(app.to_path_buf());
            let root = scratch(&format!("root-1844-w-{mode}"));
            let root_s = root.to_string_lossy().to_string();

            let mut ids = Vec::new();
            for i in 0..5u64 {
                fs::write(root.join("a.txt"), format!("version {i}").as_bytes()).unwrap();
                ids.push(checkpoint_create(&ctx, &root_s, &format!("c{i}")).unwrap().checkpoint.manifest_id);
            }
            let store_dir = store_dir_for(&ctx, &root_s).unwrap();
            for (i, id) in ids.iter().enumerate() {
                let p = store_dir.join("manifests").join(format!("{id}.json"));
                let mut doc: serde_json::Value =
                    serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
                doc["created_ms"] = serde_json::json!(1_700_000_000_000u64 + (i as u64) * 86_400_000);
                fs::write(&p, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
            }
            let policy = RetentionPolicy { hourly: 0, daily: 100, weekly: 0, monthly: 0 };
            let cap = 1_000_000u64;
            let honest = checkpoint_prune_preview(&ctx, &root_s, &policy).unwrap();
            assert!(honest.prune.is_empty(), "LIVE[{mode}]: GFS would prune on its own");
            assert!(honest.total_bytes < cap, "LIVE[{mode}]: the honest store is not under the cap");

            let blobs = store_dir.join("blobs");
            let victim = store_dir.join("cpe1844-victim.bin");
            if mode == "hardlink" {
                let vf = std::fs::File::create(&victim).unwrap();
                vf.set_len(len).unwrap();
                drop(vf);
                if std::fs::hard_link(&victim, blobs.join(name)).is_err() {
                    // Hard links are unavailable on this filesystem; the other two legs still run.
                    let _ = fs::remove_dir_all(&root);
                    let _ = fs::remove_dir_all(&app);
                    continue;
                }
            } else {
                let f = std::fs::File::create(blobs.join(name)).unwrap();
                f.set_len(len).unwrap();
                drop(f);
            }

            // LIVE: the plant is on disk, is huge, and passes the name/type filter — so anything that
            // excludes it is the witness and nothing else. `blob_files_on_disk` is the stage before it.
            let sum: u64 = crate::snapshot_capture::blob_files_on_disk(&blobs)
                .unwrap()
                .values()
                .sum();
            assert!(
                sum >= len,
                "LIVE[{mode}]: the plant never reached the measurement — {sum} < {len}"
            );
            assert!(sum > cap, "LIVE[{mode}]: the plant does not even exceed the cap");

            let applied = checkpoint_prune_apply(&ctx, &root_s, &policy, Some(cap)).unwrap();

            // HARM FIRST — the checkpoints are still there and the oldest still puts its file back.
            let left = fs::read_dir(store_dir.join("manifests")).unwrap().flatten().count();
            assert_eq!(left, 5, "HARM[{mode}]: a file dropped in blobs/ deleted checkpoints — {left} left");
            fs::write(root.join("a.txt"), b"damaged").unwrap();
            let out = checkpoint_revert(&ctx, &root_s, &ids[0]).unwrap();
            assert_eq!(
                fs::read(root.join("a.txt")).unwrap(),
                b"version 0",
                "HARM[{mode}]: the oldest checkpoint could not put the file back"
            );
            assert_eq!(out.applied, 1);
            assert!(applied.pruned.is_empty(), "HARM[{mode}]: pruned {:?}", applied.pruned);

            let _ = fs::remove_file(&victim);
            let _ = fs::remove_dir_all(&root);
            let _ = fs::remove_dir_all(&app);
        }
    }

    // ---- CPE-1862 — checkpoints.json vs. manifests retention just deleted ---------------------------

    /// Hand-edit manifest `id`'s `created_ms` in `store_dir` to `epoch_s * 1000`, same pattern
    /// `snapshot_prune`'s own tests use to place captures at arbitrary spread timestamps without
    /// sleeping. Reads the value back so a tamper that never landed can't be mistaken for one that did.
    fn set_manifest_created_ms(store_dir: &std::path::Path, id: &str, epoch_s: u64) {
        let path = store_dir.join("manifests").join(format!("{id}.json"));
        let mut doc: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        doc["created_ms"] = serde_json::json!(epoch_s * 1000);
        fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
        let back: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back["created_ms"], serde_json::json!(epoch_s * 1000), "LIVE: created_ms tamper never landed");
    }

    /// **The core CPE-1862 fixture, produced by running real retention — not by hand-editing
    /// `checkpoints.json`.** Three ordinary checkpoints land in the same hourly bucket (`hourly: 1`
    /// keeps only the newest), so `checkpoint_prune_apply` genuinely deletes two manifest files. Before
    /// this ticket's fix, their `checkpoints.json` rows survived that deletion untouched: the file kept
    /// listing two checkpoints whose manifest was gone, and clicking either would error out of
    /// `load_manifest` instead of never having been offered — the exact user experience the ticket
    /// records. This asserts both halves of the repair: the index file itself is rewritten (not just
    /// filtered on the way out), and every row `checkpoint_list` still returns genuinely loads.
    #[test]
    fn cpe_1862_retention_reconciles_checkpoints_json_and_every_listed_row_still_loads() {
        let app = scratch("app-data-reconcile");
        let ctx = HeadlessCtx::new(app.to_path_buf());
        let root = scratch("root-reconcile");
        let root_s = root.to_string_lossy().to_string();
        let store_dir = store_dir_for(&ctx, &root_s).unwrap();

        fs::write(root.join("a.txt"), b"v1").unwrap();
        let id1 = checkpoint_create(&ctx, &root_s, "v1").unwrap().checkpoint.manifest_id;
        set_manifest_created_ms(&store_dir, &id1, 1_000);

        fs::write(root.join("a.txt"), b"v2").unwrap();
        let id2 = checkpoint_create(&ctx, &root_s, "v2").unwrap().checkpoint.manifest_id;
        set_manifest_created_ms(&store_dir, &id2, 2_000);

        fs::write(root.join("a.txt"), b"v3").unwrap();
        let id3 = checkpoint_create(&ctx, &root_s, "v3").unwrap().checkpoint.manifest_id;
        set_manifest_created_ms(&store_dir, &id3, 3_000);

        // LIVE: all three rows are really in the index before anything is pruned.
        assert_eq!(
            read_checkpoints(&store_dir).len(),
            3,
            "LIVE: setup didn't record three checkpoints"
        );

        // All three timestamps (1000s/2000s/3000s) fall in hour bucket 0 — `hourly: 1` keeps only the
        // newest of that bucket, so this is a real, ordinary GFS prune, not a contrived corner case.
        let policy = RetentionPolicy { hourly: 1, daily: 0, weekly: 0, monthly: 0 };
        let result = checkpoint_prune_apply(&ctx, &root_s, &policy, None).unwrap();
        assert_eq!(result.kept, vec![id3.clone()], "the newest of the shared hour bucket survives");
        let mut pruned_sorted = result.pruned.clone();
        pruned_sorted.sort();
        let mut want_pruned = vec![id1.clone(), id2.clone()];
        want_pruned.sort();
        assert_eq!(pruned_sorted, want_pruned, "HARM: retention did not actually prune id1/id2");

        // FIXTURE LIVENESS — the manifests really are gone from disk, not merely absent from a report.
        let mdir = store_dir.join("manifests");
        assert!(!mdir.join(format!("{id1}.json")).exists(), "LIVE: id1's manifest was not actually deleted");
        assert!(!mdir.join(format!("{id2}.json")).exists(), "LIVE: id2's manifest was not actually deleted");
        assert!(mdir.join(format!("{id3}.json")).exists(), "LIVE: id3's manifest should still be on disk");

        // THE GUARD (write-time half): the index file itself no longer names what retention deleted.
        // Read with the raw, unfiltered reader — this is `checkpoints.json` on disk, not a view.
        let raw = read_checkpoints(&store_dir);
        assert_eq!(
            raw.iter().map(|c| c.manifest_id.as_str()).collect::<Vec<_>>(),
            vec![id3.as_str()],
            "HARM: checkpoints.json still names a manifest retention deleted"
        );

        // THE GUARD (read-time half, AC2): every row the UI would actually be shown loads cleanly —
        // list, then act on every row listed, exactly as the ticket demands.
        let listed = checkpoint_list(&ctx, &root_s).unwrap();
        assert_eq!(listed.len(), 1, "HARM: the UI would still list a pruned checkpoint");
        for cp in &listed {
            let preview = checkpoint_preview_revert(&ctx, &root_s, &cp.manifest_id, None);
            assert!(
                preview.is_ok(),
                "HARM: a checkpoint the UI lists errored on load_manifest: {:?}",
                preview.err()
            );
        }

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }

    /// **AC3.** A manifest that is *present* but fails CPE-1861's identity rules (here: its inner `id`
    /// disagrees with its own filename) is never pruned at all — `list_manifests` excludes it from the
    /// planner's view entirely, so it never appears in a `RetentionApplyResult`'s `kept` or `pruned`. Its
    /// `checkpoints.json` row therefore cannot be cleaned up by [`checkpoint_prune_apply`]'s reconcile,
    /// which only ever sees ids retention actually decided about. This is the case the read-time filter
    /// in [`checkpoint_list`] exists for: the row must never be handed to the UI as actionable, even
    /// though nothing about `checkpoints.json` itself changed.
    #[test]
    fn cpe_1862_a_present_but_unloadable_manifest_is_never_listed() {
        let app = scratch("app-data-unloadable");
        let ctx = HeadlessCtx::new(app.to_path_buf());
        let root = scratch("root-unloadable");
        let root_s = root.to_string_lossy().to_string();
        let store_dir = store_dir_for(&ctx, &root_s).unwrap();

        fs::write(root.join("a.txt"), b"keep me").unwrap();
        let id1 = checkpoint_create(&ctx, &root_s, "good").unwrap().checkpoint.manifest_id;

        fs::write(root.join("a.txt"), b"tampered").unwrap();
        let id2 = checkpoint_create(&ctx, &root_s, "bad").unwrap().checkpoint.manifest_id;

        // The tamper: id2's manifest is rewritten to claim id1's identity (CPE-1861's "inner id -> a
        // sibling's id" shape) — a self-inconsistent file `list_manifests` refuses to hand to the
        // planner at all, the same way an Explorer copy or a cloud-sync conflict copy would.
        let mdir = store_dir.join("manifests");
        let id2_path = mdir.join(format!("{id2}.json"));
        let mut doc: serde_json::Value = serde_json::from_str(&fs::read_to_string(&id2_path).unwrap()).unwrap();
        doc["id"] = serde_json::json!(id1.clone());
        fs::write(&id2_path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
        // LIVE: the tamper landed and the file still parses.
        let back: serde_json::Value = serde_json::from_str(&fs::read_to_string(&id2_path).unwrap()).unwrap();
        assert_eq!(back["id"], serde_json::json!(id1.clone()), "LIVE: the id tamper never landed");

        // LIVE, and the crux of AC3: the planner's own view already excludes id2 before any listing
        // decision is made — this is not a filter this ticket bolts on afterwards, it is the existing
        // CPE-1861 rule this ticket must respect rather than route around.
        let planner_ids: BTreeSet<String> =
            snapshot_capture::list_manifests(&store_dir.to_string_lossy()).unwrap().into_iter().map(|m| m.id).collect();
        assert!(planner_ids.contains(&id1), "LIVE: id1 should still be planner-visible");
        assert!(!planner_ids.contains(&id2), "LIVE: the tamper never reached the planner");

        // checkpoints.json itself is untouched so far — both rows are still there, from two ordinary
        // captures. Nothing has pruned anything yet.
        assert_eq!(read_checkpoints(&store_dir).len(), 2, "LIVE: both rows should still be recorded");

        // THE GUARD: the UI-facing list must not offer id2, even though its row is still on disk and its
        // manifest file still parses.
        let listed = checkpoint_list(&ctx, &root_s).unwrap();
        assert_eq!(
            listed.iter().map(|c| c.manifest_id.as_str()).collect::<Vec<_>>(),
            vec![id1.as_str()],
            "HARM: a checkpoint whose manifest fails CPE-1861's identity check was listed as actionable"
        );

        // And it stays that way even after an ordinary retention pass that would keep everything: the
        // tampered manifest is never pruned (leak over corruption, CPE-1861's own documented direction),
        // so its file survives on disk untouched — but reconciliation still retires its now-orphaned
        // checkpoints.json row, since `result.kept` (drawn only from the planner-visible set) never
        // named it either.
        let generous = RetentionPolicy { hourly: 5, daily: 5, weekly: 5, monthly: 5 };
        let result = checkpoint_prune_apply(&ctx, &root_s, &generous, None).unwrap();
        assert!(!result.kept.contains(&id2), "id2 was never planner-visible, so retention cannot keep it");
        assert!(!result.pruned.contains(&id2), "id2 was never planner-visible, so retention cannot prune it either");
        assert!(id2_path.exists(), "the tampered manifest file itself must survive untouched on disk");

        let raw_after = read_checkpoints(&store_dir);
        assert_eq!(
            raw_after.iter().map(|c| c.manifest_id.as_str()).collect::<Vec<_>>(),
            vec![id1.as_str()],
            "the orphaned row should be reconciled away on the next real prune pass"
        );

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }
}
