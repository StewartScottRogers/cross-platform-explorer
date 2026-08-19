//! Checkpoint & rollback: engine round-trip integration test (CPE-1124, epic CPE-732).
//!
//! The checkpoint/rollback engines already exist and are unit-tested individually in
//! `crates/server/src/{snapshot_capture,restore_plan,revert_safety,revert_engine}.rs`. This test proves
//! they COMPOSE end-to-end: capture a real baseline tree on disk, mutate it every way the plan vocabulary
//! understands (create/overwrite/delete/rename), then drive it back through `plan_restore` →
//! `revert_safety::classify_plan` → `revert_engine::execute_restore` and assert the tree is byte-for-byte
//! back to the baseline. A final case pins the skip-on-error guardrail: an action whose blob can't be
//! read is skipped, not a hard failure, and the rest of the plan still applies.
//!
//! Independent of the command-layer wiring (CPE-1123) — this exercises the engines directly, not any
//! `#[tauri::command]` or store-index seam.

use std::collections::BTreeSet;
use std::fs;

use cpe_server::restore_plan::{plan_restore, RestoreOp};
use cpe_server::revert_engine::execute_restore;
use cpe_server::revert_safety::{classify_plan, summarize_conflicts, ConflictKind, RevertSafety};
use cpe_server::snapshot_capture::{capture, scan_dir};
use cpe_server::snapshot::CaptureBudget;

/// A unique scratch directory under the OS temp dir, mirroring the pattern already used by
/// `snapshot_capture`'s and `revert_engine`'s own `#[cfg(test)]` blocks.
fn scratch(tag: &str) -> cpe_server::fsutil::ScratchDir {
    cpe_server::fsutil::scratch_dir(&format!("cpe-checkpoint-roundtrip-{tag}"))
}

/// End-to-end: capture a baseline tree with nested dirs, mutate it via create/overwrite/delete/rename,
/// compute the restore plan + drift classification, execute the restore, and assert the tree is back to
/// the baseline byte-for-byte with no stray files.
#[test]
fn capture_mutate_plan_drift_revert_round_trips_the_tree_byte_for_byte() {
    let root = scratch("root");
    let store = scratch("store");

    // ---- 1. Build the baseline tree: several files (varied content) + a nested subdir. ----
    fs::write(root.join("top.txt"), b"top level content").unwrap();
    fs::write(root.join("keep.txt"), b"unchanged throughout").unwrap();
    fs::write(root.join("overwrite_me.txt"), b"original content").unwrap();
    fs::write(root.join("delete_me.txt"), b"about to be deleted").unwrap();
    fs::write(root.join("rename_me.txt"), b"will move to a new path").unwrap();
    fs::create_dir_all(root.join("nested/deeper")).unwrap();
    fs::write(root.join("nested/child.txt"), b"nested content").unwrap();
    fs::write(root.join("nested/deeper/grandchild.txt"), b"deeply nested content").unwrap();

    // ---- 2. Capture the baseline via the disk-backed engine (scan + hash + blob-store write). ----
    let outcome = capture(&root.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED)
        .expect("capture baseline");
    assert_eq!(outcome.new_blobs, 7, "one blob per distinct file content in the baseline");
    assert!(outcome.skipped.is_empty(), "nothing should be skipped capturing a plain baseline");

    // The checkpoint snapshot is exactly what scan_dir sees right after capture (both are pure functions
    // of the same tree state) — this is the `Snapshot` `plan_restore`/`execute_restore` consume.
    let checkpoint = scan_dir(&root.to_string_lossy()).expect("scan baseline");
    assert_eq!(checkpoint.len(), 7);

    // ---- 3. Mutate the tree: create, overwrite, delete, rename. ----
    fs::write(root.join("brand_new.txt"), b"created after the checkpoint").unwrap(); // create
    fs::write(root.join("overwrite_me.txt"), b"mutated content").unwrap(); // overwrite
    fs::remove_file(root.join("delete_me.txt")).unwrap(); // delete
    // CPE-1710: test fixture — renames a file this test created in its own scratch tree.
    #[allow(clippy::disallowed_methods)]
    fs::rename(root.join("rename_me.txt"), root.join("renamed.txt")).unwrap(); // rename (delete+create pair)

    let current = scan_dir(&root.to_string_lossy()).expect("scan mutated tree");
    // top.txt, keep.txt, overwrite_me.txt, brand_new.txt, renamed.txt, nested/child.txt,
    // nested/deeper/grandchild.txt = 7 (delete_me.txt gone, rename_me.txt gone, both replaced by one each).
    assert_eq!(current.len(), 7);

    // ---- 4. plan_restore: assert the plan reverses each mutation. ----
    let plan = plan_restore(&checkpoint, &current);

    // brand_new.txt: created since checkpoint -> plan deletes it.
    let brand_new = plan.iter().find(|a| a.path == "brand_new.txt").expect("brand_new.txt in plan");
    assert_eq!(brand_new.op, RestoreOp::Delete);

    // overwrite_me.txt: content differs -> plan overwrites it back.
    let overwrite = plan.iter().find(|a| a.path == "overwrite_me.txt").expect("overwrite_me.txt in plan");
    assert_eq!(overwrite.op, RestoreOp::Overwrite);

    // delete_me.txt: gone from current, present in checkpoint -> plan recreates it.
    let deleted = plan.iter().find(|a| a.path == "delete_me.txt").expect("delete_me.txt in plan");
    assert_eq!(deleted.op, RestoreOp::Create);

    // rename_me.txt -> renamed.txt: the engine has no rename concept, so the "undo" is the pair
    // (recreate the old path, delete the new one) — assert both halves of the rename are undone.
    let rename_old = plan.iter().find(|a| a.path == "rename_me.txt").expect("rename_me.txt (old path) in plan");
    assert_eq!(rename_old.op, RestoreOp::Create);
    let rename_new = plan.iter().find(|a| a.path == "renamed.txt").expect("renamed.txt (new path) in plan");
    assert_eq!(rename_new.op, RestoreOp::Delete);

    // Unchanged files (top.txt, keep.txt, nested/*) never appear in the plan (content addressing).
    assert!(plan.iter().all(|a| a.path != "top.txt" && a.path != "keep.txt"));
    assert!(plan.iter().all(|a| a.path != "nested/child.txt" && a.path != "nested/deeper/grandchild.txt"));

    // Exactly the 5 touched paths, nothing more.
    assert_eq!(plan.len(), 5, "plan: brand_new(delete), overwrite_me(overwrite), delete_me(create), rename_me(create), renamed(delete)");

    // ---- revert_safety: drift report counts the out-of-checkpoint changes. ----
    // No agent activity recorded at all -> every plan action is a conflict (drift outside any tracked
    // agent), which is exactly the "out-of-checkpoint changes" the drift report exists to surface.
    let agent_touched: BTreeSet<String> = BTreeSet::new();
    let classified = classify_plan(&plan, &checkpoint, &current, &agent_touched);
    assert_eq!(classified.len(), 5);
    let drift = summarize_conflicts(&classified);
    assert_eq!(drift.safe, 0);
    assert_eq!(drift.conflicts, 5, "all 5 mutations are drift the agent isn't recorded as having made");
    assert_eq!(drift.conflict_ratio, Some(1.0));
    let conflict_paths: BTreeSet<&str> = drift.conflict_paths.iter().map(|s| s.as_str()).collect();
    assert_eq!(
        conflict_paths,
        BTreeSet::from(["brand_new.txt", "overwrite_me.txt", "delete_me.txt", "rename_me.txt", "renamed.txt"])
    );

    // Now the same plan classified as agent-attributed (every mutated path recorded as agent activity)
    // should be entirely Safe — proves the drift classifier flips with attribution, not just a static
    // "everything unexplained is a conflict" default.
    let agent_touched_all: BTreeSet<String> = plan.iter().map(|a| a.path.clone()).collect();
    let classified_safe = classify_plan(&plan, &checkpoint, &current, &agent_touched_all);
    assert!(classified_safe.iter().all(|c| c.safety == RevertSafety::Safe));
    let drift_safe = summarize_conflicts(&classified_safe);
    assert_eq!((drift_safe.safe, drift_safe.conflicts), (5, 0));

    // Spot-check conflict-kind classification lines up with the checkpoint/current shape at each path.
    let kind_of = |path: &str| -> &RevertSafety {
        &classified.iter().find(|c| c.action.path == path).unwrap().safety
    };
    assert!(matches!(kind_of("brand_new.txt"), RevertSafety::Conflict(ConflictKind::RecreatedOutsideAgent)));
    assert!(matches!(kind_of("overwrite_me.txt"), RevertSafety::Conflict(ConflictKind::ChangedOutsideAgent)));
    assert!(matches!(kind_of("delete_me.txt"), RevertSafety::Conflict(ConflictKind::DeletedOutsideAgent)));

    // ---- 5. execute_restore: apply the plan for real, then assert content + membership match. ----
    let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);
    assert!(report.skipped.is_empty(), "nothing should be skipped: {:?}", report.skipped);
    assert_eq!(report.applied, 5);

    // Tree membership: exactly the baseline's 7 paths, no strays, deleted ones back.
    let restored = scan_dir(&root.to_string_lossy()).expect("scan restored tree");
    let restored_paths: BTreeSet<&String> = restored.keys().collect();
    let checkpoint_paths: BTreeSet<&String> = checkpoint.keys().collect();
    assert_eq!(restored_paths, checkpoint_paths, "restored tree membership matches the baseline exactly");
    assert!(!root.join("brand_new.txt").exists(), "the created-after-checkpoint file is gone");
    assert!(!root.join("renamed.txt").exists(), "the renamed-to path is gone");
    assert!(root.join("rename_me.txt").exists(), "the renamed-from path is back");

    // Content: every baseline file's bytes match the original, byte-for-byte.
    assert_eq!(fs::read(root.join("top.txt")).unwrap(), b"top level content");
    assert_eq!(fs::read(root.join("keep.txt")).unwrap(), b"unchanged throughout");
    assert_eq!(fs::read(root.join("overwrite_me.txt")).unwrap(), b"original content");
    assert_eq!(fs::read(root.join("delete_me.txt")).unwrap(), b"about to be deleted");
    assert_eq!(fs::read(root.join("rename_me.txt")).unwrap(), b"will move to a new path");
    assert_eq!(fs::read(root.join("nested/child.txt")).unwrap(), b"nested content");
    assert_eq!(fs::read(root.join("nested/deeper/grandchild.txt")).unwrap(), b"deeply nested content");

    // The full snapshot equality is the strongest byte-for-byte statement available at this layer: same
    // paths, same content hashes as the checkpoint (scan_dir hashes real file bytes, so equal hashes
    // over an identical path set means the restored tree's content is identical to the baseline's).
    assert_eq!(restored, checkpoint, "restored tree's content hashes match the baseline exactly");

}

/// Skip-unreadable guardrail: when a plan action's checkpoint content can't actually be read back from
/// the blob store (the file that would supply the bytes is missing/unreadable), `execute_restore` skips
/// just that one action — recording why — rather than failing the whole restore. Every other action in
/// the same plan still applies. This mirrors the crate-wide skip-on-error guardrail also enforced by
/// `list_dir`/`scan_dir` and is exercised here as part of the same capture->mutate->plan->revert
/// pipeline as the main round-trip, not a standalone unit test of `execute_restore` alone.
#[test]
fn unreadable_content_is_skipped_not_a_hard_failure_and_the_rest_of_the_plan_still_applies() {
    let root = scratch("skip-root");
    let store = scratch("skip-store");

    fs::write(root.join("readable.txt"), b"checkpoint readable").unwrap();
    fs::write(root.join("will_lose_its_blob.txt"), b"checkpoint content that becomes unreadable").unwrap();

    let outcome = capture(&root.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED)
        .expect("capture baseline");
    assert_eq!(outcome.new_blobs, 2);
    let checkpoint = scan_dir(&root.to_string_lossy()).expect("scan baseline");

    // Simulate the checkpoint content becoming unreadable: remove its blob from the store after the
    // capture recorded it (a corrupt/evicted store entry, or a race — either way, the bytes the plan
    // needs are gone). This is a portable stand-in for "can't be read" that behaves identically on every
    // OS in the 3-OS CI matrix (permission-based unreadability is unix-only and would silently no-op on
    // Windows CI, which the existing `snapshot_capture`/`revert_engine` unit tests already avoid for the
    // same reason).
    let lost_hash = checkpoint.get("will_lose_its_blob.txt").unwrap().hash.clone();
    fs::remove_file(store.join("blobs").join(&lost_hash)).unwrap();

    // Mutate both files so the plan wants to overwrite each of them.
    fs::write(root.join("readable.txt"), b"mutated readable").unwrap();
    fs::write(root.join("will_lose_its_blob.txt"), b"mutated unreadable").unwrap();
    let current = scan_dir(&root.to_string_lossy()).expect("scan mutated tree");

    let plan = plan_restore(&checkpoint, &current);
    assert_eq!(plan.len(), 2);

    let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);

    // The readable file restores fine; the one whose blob vanished is skipped with a reason, not a
    // panic/hard failure that would abort the rest of the plan.
    assert_eq!(report.applied, 1, "readable.txt still restores despite the sibling failure");
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].0, "will_lose_its_blob.txt");
    assert!(!report.skipped[0].1.is_empty(), "a reason is recorded, not a silent drop");

    assert_eq!(fs::read(root.join("readable.txt")).unwrap(), b"checkpoint readable");
    // The failed restore leaves the mutated content untouched (never partially written / corrupted).
    assert_eq!(fs::read(root.join("will_lose_its_blob.txt")).unwrap(), b"mutated unreadable");

}
