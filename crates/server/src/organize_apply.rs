//! Rules-based auto-organize: propose → checkpoint → apply (CPE-1142, epic CPE-979 "rules-based" slice).
//!
//! [`crate::organize`] is a pure, filesystem-free planner: give it a flat list of [`crate::organize::
//! OrganizeEntry`] and a rule, it returns the moves to make. This module supplies the missing pieces
//! around that pure core:
//! - [`organize_plan`] — list a directory, map its entries, and hand them to `organize::plan_organize`.
//!   Read-only; moves nothing.
//! - [`organize_apply`] — the safe executor: it **first** takes a [`crate::checkpoint_store::
//!   checkpoint_create`] snapshot of the directory (so the whole reorg is a single one-click undo), then
//!   creates each proposal's `target_subdir` and moves the file into it. Skip-on-error per file, matching
//!   the rest of the app's bulk file operations — a single locked file never sinks the whole run.
//! - [`organize_clutter`] — the same directory → `OrganizeEntry` mapping, handed to
//!   `organize::find_clutter` instead (a secondary, read-only surface; no apply counterpart — clutter is
//!   only ever a suggestion the user acts on manually).
//!
//! Nothing here runs unless a caller (the Tauri command layer) invokes `organize_apply` explicitly —
//! `organize_plan`/`organize_clutter` are pure reads over the listing.

use std::path::Path;

use crate::checkpoint_store::{checkpoint_create, CheckpointCreated};
use crate::ctx::ServerCtx;
use crate::listing;
use crate::model::{DirEntry, OpResult};
use crate::organize::{find_clutter, plan_organize, ClutterFinding, MoveProposal, OrganizeEntry, OrganizeRule};

/// Map a listed [`DirEntry`] to the pure engine's [`OrganizeEntry`]: `modified` is milliseconds (or
/// absent), the engine wants whole UTC seconds, so absent becomes `0` (the epoch — never `ByModifiedYear`
/// mis-sorts a file with unknown mtime into a plausible year; it lands in "1970" instead, visibly odd
/// rather than silently wrong).
fn to_organize_entry(e: &DirEntry) -> OrganizeEntry {
    OrganizeEntry::new(e.name.clone(), e.is_dir, e.extension.clone(), e.size, e.modified.unwrap_or(0) / 1000)
}

fn list_as_organize_entries(dir: &str) -> Result<Vec<OrganizeEntry>, String> {
    Ok(listing::list_dir(dir)?.iter().map(to_organize_entry).collect())
}

/// List `dir` and compute the move plan for `rule`. Pure proposal: moves nothing on disk.
pub fn organize_plan(dir: &str, rule: OrganizeRule) -> Result<Vec<MoveProposal>, String> {
    Ok(plan_organize(&list_as_organize_entries(dir)?, rule))
}

/// List `dir` and flag likely-clutter files (CPE-994's heuristics). Read-only, like `organize_plan`; the
/// caller decides what (if anything) to do about a finding.
pub fn organize_clutter(dir: &str) -> Result<Vec<ClutterFinding>, String> {
    Ok(find_clutter(&list_as_organize_entries(dir)?))
}

/// The outcome of an apply: the checkpoint captured **before** any file moved, plus the per-file move
/// results (never all-or-nothing — see [`apply_proposals`]).
#[derive(Debug, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct OrganizeApplyOutcome {
    /// The checkpoint taken over `dir` immediately before any move — revert to it (`checkpoint_revert`)
    /// to undo the whole reorg in one action.
    pub checkpoint: CheckpointCreated,
    pub results: Vec<OpResult>,
}

/// Checkpoint `dir` (one-undo for the whole reorg), then execute the plan for `rule`: create each
/// `dir/<target_subdir>/` and move the file into it. The checkpoint is captured **before** the plan is
/// (re-)computed and applied, so even a mid-run failure leaves an undo point that predates every move.
pub fn organize_apply(ctx: &dyn ServerCtx, dir: &str, rule: OrganizeRule) -> Result<OrganizeApplyOutcome, String> {
    let checkpoint = checkpoint_create(ctx, dir, "Before auto-organize")?;
    let proposals = organize_plan(dir, rule)?;
    let results = apply_proposals(dir, &proposals);
    Ok(OrganizeApplyOutcome { checkpoint, results })
}

/// Execute one proposal per file: create its target subfolder (if needed) and rename the file into it.
/// Skip-on-error: a locked file, a name collision, or a subfolder that can't be created is reported as a
/// failed [`OpResult`] for that file and the rest of the plan still runs.
fn apply_proposals(dir: &str, proposals: &[MoveProposal]) -> Vec<OpResult> {
    let root = Path::new(dir);
    proposals
        .iter()
        .map(|p| {
            let target_dir = root.join(&p.target_subdir);
            let src = root.join(&p.name);
            let dst = target_dir.join(&p.name);
            if let Err(e) = std::fs::create_dir_all(&target_dir) {
                return OpResult::err(&dst, e.to_string());
            }
            // CPE-1705: was `if dst.exists()`. Auto-organize moves every matching file in a folder in one
            // batch, so a single unreadable destination slot silently replaced a real file and the run
            // reported success for it — the checkpoint taken above is the only thing that would have got
            // it back, and only if the user noticed in time to revert.
            if let Some(e) = crate::fsutil::clobber_refusal(
                &dst,
                &format!("\"{}\" already exists in {}", p.name, p.target_subdir),
            ) {
                return OpResult::err(&src, e);
            }
            match std::fs::rename(&src, &dst) {
                Ok(()) => OpResult::ok(&dst),
                Err(e) => OpResult::err(&src, e.to_string()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A minimal in-memory `ServerCtx` for tests: everything lives under one scratch dir, no events.
    struct TestCtx {
        data_dir: std::path::PathBuf,
    }

    impl ServerCtx for TestCtx {
        fn app_data_dir(&self) -> Result<std::path::PathBuf, String> {
            Ok(self.data_dir.clone())
        }
        fn app_config_dir(&self) -> Result<std::path::PathBuf, String> {
            Ok(self.data_dir.clone())
        }
        fn app_cache_dir(&self) -> Result<std::path::PathBuf, String> {
            Ok(self.data_dir.clone())
        }
        fn emit_json(&self, _event: &str, _payload: serde_json::Value) -> Result<(), String> {
            Ok(())
        }
    }

    /// A fresh, uniquely-named per-test scratch dir (CPE-1450). Backed by [`tempfile::TempDir`] rather
    /// than a hand-rolled `(pid, atomic counter)` name: that older scheme was unique only *within one
    /// test-binary run* — under full-suite parallelism the process is short-lived and the crate never
    /// deleted its scratch dirs, so a later `cargo test` invocation whose OS reused the same pid could
    /// revisit a stale, non-empty leftover directory from an earlier run of the exact same test (e.g. a
    /// `Documents/b.pdf` left behind from a prior pass would make this test's "the pdf move must
    /// succeed" assertion fail against pre-existing state). `tempfile::TempDir` names each dir with a
    /// random, collision-resistant suffix (no pid/counter dependence) and removes it on `Drop`, so
    /// nothing is ever left behind to collide with a future run.
    fn scratch(tag: &str) -> tempfile::TempDir {
        tempfile::Builder::new().prefix(&format!("cpe-organize-apply-{tag}-")).tempdir().unwrap()
    }

    fn ctx_for(root: &std::path::Path) -> TestCtx {
        TestCtx { data_dir: root.join(".cpe-data") }
    }

    #[test]
    fn organize_plan_maps_a_listed_dir_for_each_rule() {
        let dir = scratch("plan");
        fs::write(dir.path().join("photo.png"), b"x").unwrap();
        fs::write(dir.path().join("report.pdf"), b"xx").unwrap();
        fs::create_dir_all(dir.path().join("subdir")).unwrap(); // directories are never proposed

        let by_kind = organize_plan(&dir.path().to_string_lossy(), OrganizeRule::ByKind).unwrap();
        let mut names: Vec<&str> = by_kind.iter().map(|p| p.name.as_str()).collect();
        names.sort();
        assert_eq!(names, vec!["photo.png", "report.pdf"]);
        assert!(by_kind.iter().any(|p| p.name == "photo.png" && p.target_subdir == "Images"));
        assert!(by_kind.iter().any(|p| p.name == "report.pdf" && p.target_subdir == "Documents"));

        let by_ext = organize_plan(&dir.path().to_string_lossy(), OrganizeRule::ByExtension).unwrap();
        assert!(by_ext.iter().any(|p| p.name == "photo.png" && p.target_subdir == "PNG"));

        let by_size = organize_plan(&dir.path().to_string_lossy(), OrganizeRule::BySizeBucket).unwrap();
        assert!(by_size.iter().all(|p| p.target_subdir == "Tiny"));
    }

    #[test]
    fn organize_plan_on_a_missing_dir_is_an_error() {
        // The scratch dir itself is dropped (and removed) at the end of this statement, which is fine —
        // `does-not-exist` was never created either way, so the path is missing regardless.
        let missing = scratch("missing").path().join("does-not-exist");
        assert!(organize_plan(&missing.to_string_lossy(), OrganizeRule::ByKind).is_err());
    }

    #[test]
    fn organize_plan_empty_dir_yields_no_proposals() {
        let dir = scratch("empty");
        let plan = organize_plan(&dir.path().to_string_lossy(), OrganizeRule::ByKind).unwrap();
        assert!(plan.is_empty());
    }

    #[test]
    fn organize_clutter_flags_zero_byte_files() {
        let dir = scratch("clutter");
        fs::write(dir.path().join("empty.log"), b"").unwrap();
        fs::write(dir.path().join("keep.rs"), b"fn main() {}").unwrap();
        let findings = organize_clutter(&dir.path().to_string_lossy()).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].name, "empty.log");
    }

    #[test]
    fn organize_apply_checkpoints_first_then_moves_into_target_subdirs() {
        let dir = scratch("apply");
        fs::write(dir.path().join("photo.png"), b"pixels").unwrap();
        fs::write(dir.path().join("report.pdf"), b"paperwork").unwrap();
        let ctx = ctx_for(dir.path());

        let outcome = organize_apply(&ctx, &dir.path().to_string_lossy(), OrganizeRule::ByKind).unwrap();

        // A checkpoint was captured before anything moved.
        assert!(!outcome.checkpoint.checkpoint.manifest_id.is_empty());
        let checkpoints =
            crate::checkpoint_store::checkpoint_list(&ctx, &dir.path().to_string_lossy()).unwrap();
        assert_eq!(checkpoints.len(), 1);
        assert_eq!(checkpoints[0].manifest_id, outcome.checkpoint.checkpoint.manifest_id);

        // Both files actually moved into their target subfolders.
        assert_eq!(outcome.results.len(), 2);
        assert!(outcome.results.iter().all(|r| r.ok), "{:?}", outcome.results);
        assert!(dir.path().join("Images/photo.png").exists());
        assert!(dir.path().join("Documents/report.pdf").exists());
        assert!(!dir.path().join("photo.png").exists());
        assert!(!dir.path().join("report.pdf").exists());

        // The checkpoint really can restore the pre-organize layout.
        let revert = crate::checkpoint_store::checkpoint_revert(
            &ctx,
            &dir.path().to_string_lossy(),
            &outcome.checkpoint.checkpoint.manifest_id,
        )
        .unwrap();
        assert!(revert.applied > 0, "{:?}", revert);
        assert!(dir.path().join("photo.png").exists());
        assert!(dir.path().join("report.pdf").exists());
    }

    #[test]
    fn organize_apply_skips_on_name_collision_without_failing_the_rest() {
        let dir = scratch("collision");
        fs::write(dir.path().join("a.png"), b"1").unwrap();
        fs::write(dir.path().join("b.pdf"), b"2").unwrap();
        // Pre-create a colliding target so the png move must fail while the pdf move still succeeds.
        fs::create_dir_all(dir.path().join("Images")).unwrap();
        fs::write(dir.path().join("Images/a.png"), b"already here").unwrap();
        let ctx = ctx_for(dir.path());

        let outcome = organize_apply(&ctx, &dir.path().to_string_lossy(), OrganizeRule::ByKind).unwrap();

        let png_result = outcome.results.iter().find(|r| r.path.ends_with("a.png")).unwrap();
        assert!(!png_result.ok);
        let pdf_result = outcome.results.iter().find(|r| r.path.ends_with("b.pdf")).unwrap();
        assert!(pdf_result.ok);
        // The un-moved file is still where it started.
        assert!(dir.path().join("a.png").exists());
    }
}
