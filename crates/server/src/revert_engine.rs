//! Revert-plan execution engine (CPE-1081, epic CPE-732 "checkpoint & rollback"): apply a minimal
//! [`RestoreAction`] plan (as computed by [`crate::restore_plan::plan_restore`]) to a REAL directory —
//! surgical, current-state-aware execution, distinct from [`crate::snapshot_capture::restore`] (CPE-735),
//! which only replays a whole manifest onto a (possibly fresh) directory and never deletes or considers
//! what's already there.
//!
//! Reuses [`crate::restore_plan::RestoreAction`]/[`crate::restore_plan::RestoreOp`]/
//! [`crate::restore_plan::Snapshot`] and [`crate::snapshot_capture`]'s on-disk blob layout
//! (`store_dir/blobs/<hash>`, one file per content hash) without modifying either module.
//!
//! Design notes (deliberate choices):
//! - **Skip-on-error, never fatal.** Mirrors `list_dir`/`snapshot_capture`'s guardrail: any op that fails
//!   (missing blob, permission denied, path-safety refusal) is recorded in
//!   [`RestoreReport::skipped`] with a reason and the rest of the plan still runs.
//! - **Deletes apply deepest-first** (descending `/`-segment depth) so a directory empties out before
//!   anything tries to touch it — matches `restore_plan`'s "execution note" doc comment.
//! - **Path safety.** Every action's `path` is a `/`-joined relative path (same convention as
//!   `snapshot_capture::scan_dir`). Before touching disk we split it on `/` and reject (skip, don't
//!   panic or write) any segment that is empty, `.`, `..`, or that makes the path absolute/drive-rooted —
//!   this is a pure string/segment guard, not `Path::starts_with` (which is defeated by `..` components
//!   that never get resolved against the filesystem). The real target is then rebuilt with
//!   `Path::join` over the validated segments (portable — no manual separator concatenation).
//! - **No recursion.** The plan is iterated; deletes are sorted once up front.

use std::fs;
use std::path::{Path, PathBuf};

use crate::restore_plan::{RestoreAction, RestoreOp, Snapshot};

/// Outcome of [`execute_restore`]: how many ops applied cleanly, and which were skipped (with why).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RestoreReport {
    /// Number of actions that applied successfully.
    pub applied: usize,
    /// Actions that could not be applied: `(path, reason)`. Never fatal — the rest of the plan still runs.
    pub skipped: Vec<(String, String)>,
}

/// Execute `plan` (as produced by [`crate::restore_plan::plan_restore`]) against the real directory
/// `dest_root`, pulling restored content for `Create`/`Overwrite` from `store_dir`'s blob store using each
/// path's hash recorded in `checkpoint`. `Delete` actions are applied deepest-first so a directory is
/// empty by the time anything might look at it (nothing in this engine removes directories itself —
/// only the files the plan names).
///
/// Every action is independent: a failing op is skipped and recorded rather than aborting the rest of the
/// plan (mirrors the `list_dir`/`snapshot_capture` skip-on-error guardrail). A plan path that escapes
/// `dest_root` (`..`, an absolute path, or a drive-rooted path) is refused the same way — skipped with a
/// reason, nothing is ever written outside `dest_root`.
pub fn execute_restore(
    plan: &[RestoreAction],
    dest_root: &str,
    store_dir: &str,
    checkpoint: &Snapshot,
) -> RestoreReport {
    let dest_root_path = Path::new(dest_root);
    let blobs_dir = Path::new(store_dir).join("blobs");
    let mut report = RestoreReport::default();

    // Partition into writes (Create/Overwrite) and deletes; order deletes deepest-first (most `/`
    // segments first) so nested files are removed before anything above them is touched.
    let mut writes: Vec<&RestoreAction> = Vec::new();
    let mut deletes: Vec<&RestoreAction> = Vec::new();
    for action in plan {
        match action.op {
            RestoreOp::Create | RestoreOp::Overwrite => writes.push(action),
            RestoreOp::Delete => deletes.push(action),
        }
    }
    deletes.sort_by_key(|a| std::cmp::Reverse(segment_depth(&a.path)));

    for action in &writes {
        match apply_write(action, dest_root_path, &blobs_dir, checkpoint) {
            Ok(()) => report.applied += 1,
            Err(reason) => report.skipped.push((action.path.clone(), reason)),
        }
    }
    for action in &deletes {
        match apply_delete(action, dest_root_path) {
            Ok(()) => report.applied += 1,
            Err(reason) => report.skipped.push((action.path.clone(), reason)),
        }
    }

    report
}

/// Split `rel` on `/` and validate every segment is a plain path component: non-empty, not `.`/`..`, and
/// not itself absolute/drive-rooted (guards a Windows `C:\...`-style segment slipping in as the first
/// piece). Returns the validated segments, or an error naming why the path was refused. This is a pure
/// string check over the `/`-joined convention `scan_dir`/`plan_restore` use — deliberately not
/// `Path::starts_with`, which only compares an already-joined path and can be fooled by `..` components
/// that are never resolved against the real filesystem.
pub(crate) fn safe_segments(rel: &str) -> Result<Vec<&str>, String> {
    if rel.is_empty() {
        return Err("empty path".to_string());
    }
    if Path::new(rel).is_absolute() {
        return Err("escapes dest_root: absolute path".to_string());
    }
    let mut segments = Vec::new();
    for seg in rel.split('/') {
        if seg.is_empty() {
            return Err("escapes dest_root: empty path segment".to_string());
        }
        if seg == "." || seg == ".." {
            return Err("escapes dest_root: `.`/`..` segment".to_string());
        }
        // A segment containing a drive letter (`C:`) or a backslash would, on Windows, be
        // reinterpreted as rooting/escaping once joined — reject it the same way.
        if seg.contains(':') || seg.contains('\\') {
            return Err("escapes dest_root: drive-rooted or backslash segment".to_string());
        }
        segments.push(seg);
    }
    Ok(segments)
}

/// Rebuild the real, validated target path under `root` from `rel`'s `/`-segments via [`Path::join`]
/// (portable — never string concatenation), or an error if `rel` escapes `root`. `pub(crate)` so sibling
/// command-layer modules needing the same "resolve a caller-supplied relative path safely under a root"
/// guard (e.g. [`crate::checkpoint_store`]'s per-file diff, CPE-1197) reuse this rather than duplicating
/// the segment validation.
pub(crate) fn safe_target(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let segments = safe_segments(rel)?;
    let mut p = root.to_path_buf();
    for seg in segments {
        p.push(seg);
    }
    Ok(p)
}

/// Number of `/`-separated segments in `rel` — used only to order deletes deepest-first. A malformed
/// path (caught later by [`safe_segments`]) simply sorts by its raw segment count; it will be refused
/// before anything is written regardless of where it lands in delete order.
fn segment_depth(rel: &str) -> usize {
    rel.split('/').count()
}

fn apply_write(
    action: &RestoreAction,
    dest_root: &Path,
    blobs_dir: &Path,
    checkpoint: &Snapshot,
) -> Result<(), String> {
    let target = safe_target(dest_root, &action.path)?;
    let Some(state) = checkpoint.get(&action.path) else {
        return Err("no checkpoint entry for this path".to_string());
    };
    let blob = blobs_dir.join(&state.hash);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    fs::copy(&blob, &target).map_err(|e| format!("{}: {e}", blob.display()))?;
    Ok(())
}

fn apply_delete(action: &RestoreAction, dest_root: &Path) -> Result<(), String> {
    let target = safe_target(dest_root, &action.path)?;
    fs::remove_file(&target).map_err(|e| format!("{}: {e}", target.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::restore_plan::plan_restore;
    use crate::snapshot_capture::scan_dir;

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-revert-{tag}"))
    }

    /// Write every file in `snapshot` (paths relative, `/`-joined) as a blob in `store_dir/blobs/<hash>`,
    /// content taken from `contents` (path -> bytes). Minimal hand-rolled store setup for these tests —
    /// deliberately not reusing `snapshot_capture::capture` so the test doesn't depend on its internals.
    fn write_blobs(store_dir: &Path, checkpoint: &Snapshot, contents: &[(&str, &[u8])]) {
        let blobs = store_dir.join("blobs");
        fs::create_dir_all(&blobs).unwrap();
        for (path, bytes) in contents {
            let hash = &checkpoint.get(*path).unwrap().hash;
            fs::write(blobs.join(hash), bytes).unwrap();
        }
    }

    #[test]
    fn round_trips_a_mutated_tree_back_to_the_checkpoint() {
        let root = scratch("rt-root");
        let store = scratch("rt-store");

        // Build the checkpoint state on disk.
        fs::write(root.join("a.txt"), b"checkpoint a").unwrap();
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("nested").join("b.txt"), b"checkpoint b").unwrap();
        fs::write(root.join("gone_later.txt"), b"will be deleted after checkpoint").unwrap();

        let checkpoint = scan_dir(&root.to_string_lossy()).unwrap();
        write_blobs(
            &store,
            &checkpoint,
            &[
                ("a.txt", b"checkpoint a"),
                ("nested/b.txt", b"checkpoint b"),
                ("gone_later.txt", b"will be deleted after checkpoint"),
            ],
        );

        // Mutate: edit a.txt, delete gone_later.txt, add a new file.
        fs::write(root.join("a.txt"), b"edited a").unwrap();
        fs::remove_file(root.join("gone_later.txt")).unwrap();
        fs::write(root.join("added.txt"), b"new since checkpoint").unwrap();

        let current = scan_dir(&root.to_string_lossy()).unwrap();
        let plan = plan_restore(&checkpoint, &current);
        assert_eq!(plan.len(), 3, "overwrite a.txt, create gone_later.txt, delete added.txt");

        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);
        assert!(report.skipped.is_empty(), "skipped: {:?}", report.skipped);
        assert_eq!(report.applied, 3);

        let restored = scan_dir(&root.to_string_lossy()).unwrap();
        assert_eq!(restored, checkpoint, "tree matches the checkpoint content-for-content");
        assert_eq!(fs::read(root.join("a.txt")).unwrap(), b"checkpoint a");
        assert_eq!(fs::read(root.join("gone_later.txt")).unwrap(), b"will be deleted after checkpoint");
        assert!(!root.join("added.txt").exists());

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&store);
    }

    #[test]
    fn a_missing_blob_is_skipped_while_the_rest_of_the_plan_applies() {
        let root = scratch("missing-blob-root");
        let store = scratch("missing-blob-store");

        fs::write(root.join("ok.txt"), b"checkpoint ok").unwrap();
        fs::write(root.join("broken.txt"), b"checkpoint broken").unwrap();
        let checkpoint = scan_dir(&root.to_string_lossy()).unwrap();
        // Only store the blob for ok.txt — broken.txt's hash points at a blob that never gets written,
        // a portable stand-in for "the blob store is missing/corrupt for one file" that doesn't require
        // OS-specific permission tricks.
        write_blobs(&store, &checkpoint, &[("ok.txt", b"checkpoint ok")]);

        fs::write(root.join("ok.txt"), b"edited ok").unwrap();
        fs::write(root.join("broken.txt"), b"edited broken").unwrap();
        let current = scan_dir(&root.to_string_lossy()).unwrap();
        let plan = plan_restore(&checkpoint, &current);
        assert_eq!(plan.len(), 2);

        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);
        assert_eq!(report.applied, 1, "ok.txt restores");
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].0, "broken.txt");
        assert_eq!(fs::read(root.join("ok.txt")).unwrap(), b"checkpoint ok");
        assert_eq!(
            fs::read(root.join("broken.txt")).unwrap(),
            b"edited broken",
            "the failed restore leaves the current content untouched"
        );

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&store);
    }

    #[test]
    fn an_escaping_plan_path_is_refused_and_writes_nothing_outside_dest_root() {
        let root = scratch("escape-root");
        let store = scratch("escape-store");
        fs::create_dir_all(store.join("blobs")).unwrap();

        let mut checkpoint = Snapshot::new();
        checkpoint.insert(
            "../escape.txt".to_string(),
            crate::restore_plan::FileState::new("deadbeef", 4),
        );
        fs::write(store.join("blobs").join("deadbeef"), b"evil").unwrap();

        let plan = vec![RestoreAction { path: "../escape.txt".to_string(), op: RestoreOp::Create }];
        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);

        assert_eq!(report.applied, 0);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].0, "../escape.txt");
        // The escape target, one level above root, must not have been written.
        assert!(!root.parent().unwrap().join("escape.txt").exists());

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&store);
    }

    #[test]
    fn an_absolute_plan_path_is_refused() {
        let root = scratch("absolute-root");
        let store = scratch("absolute-store");
        fs::create_dir_all(store.join("blobs")).unwrap();

        #[cfg(unix)]
        let abs_path = "/etc/passwd";
        #[cfg(windows)]
        let abs_path = "C:/evil.txt";

        let mut checkpoint = Snapshot::new();
        checkpoint.insert(abs_path.to_string(), crate::restore_plan::FileState::new("deadbeef", 4));

        let plan = vec![RestoreAction { path: abs_path.to_string(), op: RestoreOp::Create }];
        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);

        assert_eq!(report.applied, 0);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].0, abs_path);

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&store);
    }

    #[test]
    fn deletes_apply_deepest_first() {
        let root = scratch("deep-delete-root");
        let store = scratch("deep-delete-store");
        fs::create_dir_all(store.join("blobs")).unwrap();
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("a/b/c.txt"), b"deep").unwrap();
        fs::write(root.join("top.txt"), b"shallow").unwrap();

        let checkpoint = Snapshot::new(); // both files are new-since-checkpoint -> deletes
        let plan = vec![
            RestoreAction { path: "top.txt".to_string(), op: RestoreOp::Delete },
            RestoreAction { path: "a/b/c.txt".to_string(), op: RestoreOp::Delete },
        ];

        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);
        assert_eq!(report.applied, 2);
        assert!(report.skipped.is_empty());
        assert!(!root.join("a/b/c.txt").exists());
        assert!(!root.join("top.txt").exists());

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&store);
    }
}
