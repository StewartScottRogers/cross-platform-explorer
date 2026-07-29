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
use crate::revert_engine::{execute_restore, RestoreReport};
use crate::revert_safety::{classify_plan, summarize_conflicts};
use crate::snapshot::{CaptureBudget, SkipReason};
use crate::snapshot_capture;

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
///   missing/empty journal, or a `manifest_id` absent from the index (defensive; falls back to `ts:
///   0`, i.e. "every event counts"), folds to a touched-set that is empty or a superset — never a
///   panic, never less safe than `None`.
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
            let since_ts = read_checkpoints(&store_dir)
                .into_iter()
                .find(|c| c.manifest_id == manifest_id)
                .map(|c| c.ts)
                .unwrap_or(0);
            let events = audit_journal::read_session(&audit_base(ctx)?, sess);
            revert_attribution::agent_touched(&events, sess, since_ts, root)
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
    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-ckpt-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
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
        let ctx = HeadlessCtx::new(&app);
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
        let ctx = HeadlessCtx::new(&app);
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
        let ctx = HeadlessCtx::new(&app);
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
        let ctx = HeadlessCtx::new(&app);
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

    /// CPE-1134: a missing (never-written) or empty audit journal for the requested session degrades to
    /// the conservative behaviour — no panic, and every diverging path still counted as drift, exactly
    /// as if `session` had been `None`.
    #[test]
    fn preview_with_session_and_no_journal_degrades_to_conservative_behaviour() {
        let app = scratch("app-data-no-journal");
        let ctx = HeadlessCtx::new(&app);
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
}
