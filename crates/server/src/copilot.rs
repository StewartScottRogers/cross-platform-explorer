//! AI **file-copilot command-layer glue** (CPE-1275, epic CPE-977): the safe plan → confirm → execute →
//! undo pipeline that sits between the risky [`crate::copilot_planner`] (an LLM proposes ops) and the disk.
//!
//! This module owns the SAFETY CHAIN, and nothing here executes without it:
//! 1. **plan** ([`plan_with`]) — list the target folder, ask the planner for a candidate [`FileOpPlan`],
//!    then [`crate::op_plan::validate`] it against the scope envelope ([`PlanLimits`]: every path under
//!    `root`, op-count capped at [`COPILOT_MAX_OPS`]). Returns the plan + a dry-run [`PlanSummary`] +ALL
//!    violations. **No filesystem change.** The UI (CPE-1276) previews this for the human to confirm.
//! 2. **confirm** — the human's job (the UI only calls execute after the previewed plan is confirmed).
//! 3. **execute** ([`execute_with`]) — **RE-VALIDATE** the (possibly stale/tampered) plan against the same
//!    envelope; if it no longer validates, **nothing runs and no checkpoint is taken**. Otherwise take a
//!    [`crate::checkpoint_store`] checkpoint FIRST (one-click undo for the whole plan), then apply the
//!    whitelisted ops. **Deletes go to the OS trash** (recoverable) via the [`TrashBin`] seam, never a hard
//!    delete. Per-op [`OpResult`]s are returned (skip-on-error, never all-or-nothing).
//! 4. **undo** — [`crate::checkpoint_store::checkpoint_revert`] against the returned checkpoint id.
//!
//! Two layers of recoverability back a delete: the pre-execute checkpoint AND the trash. The op set is a
//! closed whitelist by construction ([`crate::op_plan::FileOp`]) — there is no shell/free-form escape — so a
//! plan is always inspectable, and validate + re-validate guarantee no path escapes `root`.
//!
//! The [`crate::copilot_planner::LlmPlanner`] and [`TrashBin`] are seams, so the whole chain is tested with
//! a [`crate::copilot_planner::FakePlanner`] + a fake trash — no network, no real recycle bin.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::checkpoint_store::{checkpoint_create, CheckpointCreated};
use crate::copilot_planner::{LlmPlanner, PlanEntry};
use crate::ctx::ServerCtx;
use crate::listing;
use crate::model::OpResult;
use crate::op_plan::{self, FileOp, FileOpPlan, PlanLimits, PlanSummary};

/// Conservative hard cap on how many operations one copilot plan may carry. A plan touching more than this
/// should be re-planned or split rather than run in one confirm — safety over ambition, since an LLM emits
/// the ops. Deliberately far below [`PlanLimits::DEFAULT_MAX_OPS`]. Both plan + execute validate against it,
/// so the two envelopes are identical (execute can never accept what plan rejected).
pub const COPILOT_MAX_OPS: usize = 100;

/// The label the pre-execute checkpoint is recorded under.
const CHECKPOINT_LABEL: &str = "Before AI copilot plan";

/// Persisted selection of the model the copilot uses (CPE-1275, epic CPE-977) — mirrors
/// [`crate::content_index::ContentEmbedderConfig`]. Off by default. When `enabled` with a `base_url` +
/// `model`, an OpenAI-compatible [`crate::copilot_planner::HttpPlanner`] is used. The API **key is NOT
/// here** — it lives in the OS keychain (service `cpe.copilot`), fetched separately at the command boundary,
/// so a key never persists in plaintext settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CopilotConfig {
    /// When false (the default), the copilot is disabled and no plan can be produced.
    pub enabled: bool,
    /// The model server base URL, with or without `/v1` (e.g. `http://localhost:1234/v1` for LM Studio, or
    /// `https://api.openai.com/v1`). See [`crate::copilot_planner::chat_completions_url`].
    pub base_url: String,
    /// The chat model name the server expects (e.g. `gpt-4o-mini`, or a local model id).
    pub model: String,
}

/// The result of [`plan_with`]: the validated candidate plan, its dry-run summary, and every scope/cap
/// violation (empty ⇒ safe to offer for execution). A non-empty `violations` means the UI must NOT offer
/// execute — execute would re-validate and refuse anyway.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CopilotPlanResult {
    pub plan: FileOpPlan,
    pub summary: PlanSummary,
    pub violations: Vec<String>,
}

/// The result of [`execute_with`]. Either the plan ran — `checkpoint` is `Some` (the undo handle) and
/// `results` holds a per-op outcome — or re-validation refused the plan: `checkpoint` is `None`, `results`
/// is empty, and `violations` explains why nothing ran. The two states are mutually exclusive.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CopilotExecuteResult {
    /// The checkpoint captured immediately before any op ran — revert to it to undo the whole plan. `None`
    /// iff re-validation refused the plan (nothing ran).
    pub checkpoint: Option<CheckpointCreated>,
    /// One outcome per op, in plan order. Empty iff re-validation refused the plan.
    pub results: Vec<OpResult>,
    /// The scope/cap violations that made execute refuse the plan. Empty iff the plan ran.
    pub violations: Vec<String>,
}

/// The OS-trash seam: send the entry at `path` to the recycle bin / trash (recoverable), or return a clear
/// error. Abstracted off this crate so `cpe-server` stays Tauri- and `trash`-crate-free — the app adapter
/// implements it with the `trash` crate (the same primitive `delete_to_trash` uses), exactly as
/// [`crate::vault_manager::SecretAccess`] keeps the keychain out of this crate.
pub trait TrashBin: Send + Sync {
    fn trash(&self, path: &str) -> Result<(), String>;
}

/// List `root`'s direct children as [`PlanEntry`]s for the planner's prompt. Bubbles a listing error (a
/// missing/unreadable folder) up as `Err` — no plan can be made without knowing the folder.
pub fn list_plan_entries(root: &str) -> Result<Vec<PlanEntry>, String> {
    Ok(listing::list_dir(root)?
        .into_iter()
        .map(|e| PlanEntry { name: e.name, is_dir: e.is_dir })
        .collect())
}

/// Slice-1 step 1: build a validated plan for `instruction` over `root`, without touching the filesystem.
/// Lists the folder, asks `planner`, then validates against the scope+cap envelope and summarises. A
/// model/planner failure is a clear `Err`; a produced-but-unsafe plan comes back with a non-empty
/// `violations` (so the UI can show the problem and withhold execute) rather than an error.
pub fn plan_with(
    planner: &dyn LlmPlanner,
    root: &str,
    instruction: &str,
) -> Result<CopilotPlanResult, String> {
    let entries = list_plan_entries(root)?;
    let plan = planner.plan(root, instruction, &entries)?;
    let limits = PlanLimits::new(root, COPILOT_MAX_OPS);
    let violations = op_plan::validate(&plan, &limits).err().unwrap_or_default();
    let summary = op_plan::summarize(&plan);
    Ok(CopilotPlanResult { plan, summary, violations })
}

/// Slice-1 step 3: execute a (human-confirmed) plan against `root`. **Re-validates first** against the same
/// envelope [`plan_with`] used — a stale or tampered plan whose paths now escape `root`, or that exceeds the
/// cap, is refused with nothing run and no checkpoint taken. Otherwise a checkpoint is captured **before**
/// any op, then the whitelisted ops are applied top-to-bottom (skip-on-error), deletes routed to `trash`.
///
/// # Scope: breadth vs. traversal
/// `root` is **caller-supplied**. [`op_plan::validate`]'s textual check confines every path *under* `root`,
/// and the per-op [`parent_confined`] check below defends against a symlink/junction component **resolving
/// out** of `root` at kernel time (the data-loss guard). What the backend can NOT floor is the *breadth* of
/// `root` itself — a compromised frontend could pass `root = C:\`. The human-confirm step (the UI previews
/// the folder + the plan before execute is ever called) is the guard on that: a person is choosing which
/// folder the copilot may touch. This is documented, deliberate, and mirrors every other folder-scoped
/// command in the app (organize/checkpoint/…), which are likewise scoped to a caller-chosen folder.
pub fn execute_with(
    ctx: &dyn ServerCtx,
    trash: &dyn TrashBin,
    root: &str,
    plan: &FileOpPlan,
) -> Result<CopilotExecuteResult, String> {
    let limits = PlanLimits::new(root, COPILOT_MAX_OPS);
    if let Err(violations) = op_plan::validate(plan, &limits) {
        // Never trust a stale/tampered plan: refuse without checkpointing or touching disk.
        return Ok(CopilotExecuteResult { checkpoint: None, results: Vec::new(), violations });
    }
    // Resolve the real root ONCE so the per-op confinement check ([`parent_confined`]) can compare each
    // op's kernel-resolved parent against it. A root that can't be canonicalized (missing/unreadable) is a
    // hard error — nothing runs, nothing is checkpointed.
    let canonical_root = std::fs::canonicalize(root)
        .map_err(|e| format!("cannot resolve the target folder {root:?}: {e}"))?;
    // Checkpoint BEFORE any mutation so the whole plan is one-click undoable even on partial failure.
    let checkpoint = checkpoint_create(ctx, root, CHECKPOINT_LABEL)?;
    let results = plan.ops.iter().map(|op| apply_op(op, &canonical_root, trash)).collect();
    Ok(CopilotExecuteResult { checkpoint: Some(checkpoint), results, violations: Vec::new() })
}

/// The path-typed fields of an op as `(field, value)` pairs — the paths that must be symlink-confined
/// before the op mutates. `new_name` is deliberately excluded (a bare name; its location is `path`'s
/// parent, which IS checked). Mirrors `op_plan::FileOp::path_fields` (private there).
fn op_path_fields(op: &FileOp) -> Vec<(&'static str, &str)> {
    match op {
        FileOp::Move { src, dst } => vec![("src", src), ("dst", dst)],
        FileOp::Copy { src, dst } => vec![("src", src), ("dst", dst)],
        FileOp::Rename { path, .. } => vec![("path", path)],
        FileOp::Delete { path } => vec![("path", path)],
        FileOp::Mkdir { path } => vec![("path", path)],
    }
}

/// Is `path`'s parent directory, **after kernel symlink/junction resolution**, still within
/// `canonical_root`? This is the traversal guard [`op_plan::validate`] (purely textual — no filesystem
/// access) cannot provide: an intermediate component under `root` that is a symlink/junction pointing
/// OUTSIDE (common with OneDrive, NTFS junctions, `C:\Users\Public`) would let `rename`/`create_dir_all`/
/// `copy`/`trash::delete` act on a real location outside the confirmed folder — and outside the pre-execute
/// checkpoint, so undo could not restore it. We canonicalize the **deepest existing ancestor** of `path`'s
/// parent (the not-yet-created remainder is created UNDER that confined ancestor, so it stays confined) and
/// require it to start with `canonical_root`. Mirrors `archive.rs`'s zip-slip defence (canonicalize +
/// canonical-containment). A path with no canonicalizable in-root ancestor is treated as unconfined.
fn parent_confined(canonical_root: &Path, path: &Path) -> bool {
    let mut cur = path.parent();
    while let Some(dir) = cur {
        if dir.as_os_str().is_empty() {
            break;
        }
        match dir.canonicalize() {
            Ok(canon) => return canon.starts_with(canonical_root),
            // This component doesn't exist yet — walk up to the deepest ancestor that does.
            Err(_) => cur = dir.parent(),
        }
    }
    false
}

/// Apply one whitelisted op, returning its [`OpResult`]. Never all-or-nothing: a failure (locked file,
/// collision, unreadable source) is a failed result for that op and the caller runs the rest. Move/copy
/// **refuse to overwrite** an existing destination (a safer default than clobbering); deletes go to trash.
///
/// **Symlink/junction confinement (data-loss guard):** before any mutation, every path field's parent must
/// resolve within `canonical_root` ([`parent_confined`]). An op that would resolve outside — via a
/// symlinked intermediate component that passed the textual [`op_plan::validate`] — is **refused** as a
/// failed result and never executed, so no mutation ever lands outside the confirmed folder.
fn apply_op(op: &FileOp, canonical_root: &Path, trash: &dyn TrashBin) -> OpResult {
    for (field, value) in op_path_fields(op) {
        if !parent_confined(canonical_root, Path::new(value)) {
            return OpResult::err(
                Path::new(value),
                format!("refused: {field} {value:?} resolves outside the folder (symlink/junction escape)"),
            );
        }
    }
    match op {
        FileOp::Mkdir { path } => {
            let p = Path::new(path);
            match std::fs::create_dir_all(p) {
                Ok(()) => OpResult::ok(p),
                Err(e) => OpResult::err(p, e),
            }
        }
        FileOp::Move { src, dst } => transfer_entry(src, dst, false),
        FileOp::Copy { src, dst } => transfer_entry(src, dst, true),
        FileOp::Rename { path, new_name } => {
            let p = Path::new(path);
            let Some(parent) = p.parent() else {
                return OpResult::err(p, "cannot rename a path with no parent directory");
            };
            let dst = parent.join(new_name);
            // CPE-1705: was `if dst.exists()` in front of an `fs::rename`, which replaces its
            // destination silently. CPE-1710: and it got only HALF the guard — `clobber_refusal` alone,
            // which follows links, so a **dangling** symlink at `dst` read as a free name and the rename
            // destroyed the link. `rename_slot_refusal` is both halves in one call.
            match crate::fsutil::rename_into_slot(p, &dst, &format!("\"{new_name}\" already exists")) {
                Ok(()) => OpResult::ok(&dst),
                Err(e) => OpResult::err(p, e),
            }
        }
        FileOp::Delete { path } => {
            let p = Path::new(path);
            match trash.trash(path) {
                Ok(()) => OpResult::ok(p),
                Err(e) => OpResult::err(p, e),
            }
        }
    }
}

/// Shared move/copy: ensure `dst`'s parent exists, refuse to overwrite an existing `dst`, then either
/// `rename` (move) or recursively copy. Move relies on same-volume `rename` (both paths are under one
/// `root`, so they share a volume); copy handles a file or a whole subtree.
fn transfer_entry(src: &str, dst: &str, copy: bool) -> OpResult {
    let s = Path::new(src);
    let d = Path::new(dst);
    if let Some(parent) = d.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return OpResult::err(d, e);
        }
    }
    // CPE-1705: was `if d.exists()`, guarding BOTH a silent-replacing `fs::rename` and a `copy_recursive`
    // whose leaf `fs::copy` truncates whatever it lands on.
    //
    // CPE-1710: `clobber_refusal` on its own let a **dangling** link at `d` through — it follows links, so
    // a link to a missing target reads as "nothing there", and `fs::rename` (which does not follow the
    // final component) then destroyed the link itself. The copy branch is guarded by the same call on
    // purpose: `fs::copy` DOES follow the final component, so it would instead materialise the link's
    // absent target — a different surprise, equally unasked-for.
    // Refuse first, so a refusal is still reported against the DESTINATION path (the thing in the way),
    // which is what this command has always reported and what the UI shows.
    if let Some(e) = crate::fsutil::rename_slot_refusal(d, "destination already exists") {
        return OpResult::err(d, e);
    }
    let outcome = if copy {
        copy_recursive(s, d).map_err(|e| e.to_string())
    } else {
        // …and the move re-checks inside `rename_into_slot` (CPE-1710 round 3): the guard and the
        // destructive call are one function, so nothing can be inserted between them later.
        crate::fsutil::rename_into_slot(s, d, "destination already exists")
    };
    match outcome {
        Ok(()) => OpResult::ok(d),
        Err(e) => OpResult::err(s, e),
    }
}

/// Recursively copy `src` to `dst` (a file, or a whole directory tree). Uses `symlink_metadata` so a
/// symlink is not silently followed into an unbounded copy of its target's tree; a symlink's own bytes are
/// copied via `fs::copy` (best-effort — good enough for the copilot's in-root file operations).
fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(src)?;
    if meta.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(src, dst).map(|_| ())
    }
}

// ---------------------------------------------------------------------------
// Real HTTP planner resolution + connection test — feature-gated (`copilot`).
// ---------------------------------------------------------------------------

/// Build the [`LlmPlanner`] the copilot commands should use from the persisted [`CopilotConfig`] + the
/// keychain `api_key`. Returns a clear `Err` when the copilot is disabled, unconfigured (blank URL/model),
/// or this build lacks the `copilot` feature — never a panic. Feature-gated: without `copilot` the real
/// HTTP planner isn't compiled, so any enabled config reports the honest "not built with copilot support".
#[cfg(feature = "copilot")]
pub fn resolve_planner(
    config: &CopilotConfig,
    api_key: Option<String>,
) -> Result<Box<dyn LlmPlanner>, String> {
    if !config.enabled {
        return Err("the AI copilot is disabled".to_string());
    }
    if config.base_url.trim().is_empty() || config.model.trim().is_empty() {
        return Err("the AI copilot needs a server URL and a model name".to_string());
    }
    Ok(Box::new(crate::copilot_planner::connect(&config.base_url, &config.model, api_key)))
}

#[cfg(not(feature = "copilot"))]
pub fn resolve_planner(
    _config: &CopilotConfig,
    _api_key: Option<String>,
) -> Result<Box<dyn LlmPlanner>, String> {
    Err("this build was compiled without AI copilot support".to_string())
}

/// Test a model endpoint (the Settings "Test connection" button): send a trivial planning request and
/// confirm a parseable plan comes back, or return a clear error (unreachable, bad key, bad response) —
/// never a panic. Ignores `config.enabled` (you test before turning it on). Feature-gated; without
/// `copilot` this is a clear "not built" error.
#[cfg(feature = "copilot")]
pub fn probe_planner(base_url: &str, model: &str, api_key: Option<String>) -> Result<(), String> {
    if base_url.trim().is_empty() || model.trim().is_empty() {
        return Err("the AI copilot needs a server URL and a model name".to_string());
    }
    let planner = crate::copilot_planner::connect(base_url, model, api_key);
    // A tiny, side-effect-free probe over an empty folder; we only need a parseable plan back.
    planner
        .plan("/", "Reply with an empty plan.", &[])
        .map(|_| ())
}

#[cfg(not(feature = "copilot"))]
pub fn probe_planner(_base_url: &str, _model: &str, _api_key: Option<String>) -> Result<(), String> {
    Err("this build was compiled without AI copilot support".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::copilot_planner::FakePlanner;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;

    /// A minimal in-memory `ServerCtx`: everything lives under one scratch dir, no events (mirrors
    /// `organize_apply`'s test ctx).
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

    /// A fake trash: instead of deleting, it MOVES the entry into a holding dir and records the path — so a
    /// test can assert both "it left its place" and "it is recoverable" (the whole point of trash-not-hard-
    /// delete). A missing source is a clear error, like the real recycle bin.
    struct FakeTrash {
        holding: std::path::PathBuf,
        trashed: Mutex<Vec<String>>,
    }
    impl FakeTrash {
        fn new(holding: std::path::PathBuf) -> Self {
            fs::create_dir_all(&holding).unwrap();
            FakeTrash { holding, trashed: Mutex::new(Vec::new()) }
        }
    }
    impl TrashBin for FakeTrash {
        fn trash(&self, path: &str) -> Result<(), String> {
            let src = Path::new(path);
            let name = src.file_name().ok_or_else(|| "no file name".to_string())?;
            let dst = self.holding.join(name);
            // CPE-1710: test double for the recycle bin — moves into a holding dir this fake owns.
            #[allow(clippy::disallowed_methods)]
            fs::rename(src, &dst).map_err(|e| e.to_string())?;
            self.trashed.lock().unwrap().push(path.to_string());
            Ok(())
        }
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-copilot-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn ctx_for(root: &Path) -> TestCtx {
        TestCtx { data_dir: root.join(".cpe-data") }
    }

    fn root_str(p: &Path) -> String {
        p.to_string_lossy().to_string()
    }

    #[test]
    fn plan_with_lists_folder_validates_and_summarizes() {
        let root = scratch("plan");
        fs::write(root.join("a.txt"), b"x").unwrap();
        let dst = root.join("Archive/a.txt");
        let plan = FileOpPlan {
            ops: vec![
                FileOp::Mkdir { path: root_str(&root.join("Archive")) },
                FileOp::Move {
                    src: root_str(&root.join("a.txt")),
                    dst: root_str(&dst),
                },
            ],
        };
        let planner = FakePlanner::returning(plan.clone());
        let out = plan_with(&planner, &root_str(&root), "archive a.txt").unwrap();
        assert!(out.violations.is_empty(), "{:?}", out.violations);
        assert_eq!(out.summary.mkdirs, 1);
        assert_eq!(out.summary.moves, 1);
        assert_eq!(out.plan, plan);
    }

    #[test]
    fn plan_with_reports_out_of_root_violation_without_erroring() {
        let root = scratch("escape");
        let plan = FileOpPlan { ops: vec![FileOp::Delete { path: "/etc/passwd".into() }] };
        let out = plan_with(&FakePlanner::returning(plan), &root_str(&root), "delete passwd").unwrap();
        assert!(!out.violations.is_empty());
        assert!(out.violations.iter().any(|v| v.contains("escapes the scope root")));
    }

    #[test]
    fn plan_with_reports_over_cap_violation() {
        let root = scratch("cap");
        let ops = (0..COPILOT_MAX_OPS + 1)
            .map(|i| FileOp::Mkdir { path: root_str(&root.join(format!("d{i}"))) })
            .collect();
        let out = plan_with(&FakePlanner::returning(FileOpPlan { ops }), &root_str(&root), "many").unwrap();
        assert!(out.violations.iter().any(|v| v.contains("exceeds the cap")));
    }

    #[test]
    fn plan_with_propagates_model_failure_as_err() {
        let root = scratch("modelfail");
        let out = plan_with(&FakePlanner::failing("model unreachable"), &root_str(&root), "x");
        assert_eq!(out.unwrap_err(), "model unreachable");
    }

    #[test]
    fn execute_applies_ops_checkpoints_first_and_trashes_deletes() {
        let root = scratch("exec");
        fs::write(root.join("keep.txt"), b"keep").unwrap();
        fs::write(root.join("old.log"), b"old").unwrap();
        let ctx = ctx_for(&root);
        let trash = FakeTrash::new(scratch("hold"));

        let plan = FileOpPlan {
            ops: vec![
                FileOp::Mkdir { path: root_str(&root.join("Sub")) },
                FileOp::Copy {
                    src: root_str(&root.join("keep.txt")),
                    dst: root_str(&root.join("Sub/keep.txt")),
                },
                FileOp::Delete { path: root_str(&root.join("old.log")) },
            ],
        };

        let out = execute_with(&ctx, &trash, &root_str(&root), &plan).unwrap();
        assert!(out.violations.is_empty());
        assert_eq!(out.results.len(), 3);
        assert!(out.results.iter().all(|r| r.ok), "{:?}", out.results);

        // A checkpoint was taken BEFORE any op.
        let cp = out.checkpoint.as_ref().unwrap();
        assert!(!cp.checkpoint.manifest_id.is_empty());
        let list = crate::checkpoint_store::checkpoint_list(&ctx, &root_str(&root)).unwrap();
        assert_eq!(list.len(), 1);

        // Ops actually applied.
        assert!(root.join("Sub").is_dir());
        assert!(root.join("Sub/keep.txt").exists());
        // Delete went to TRASH (recoverable), not a hard delete.
        assert!(!root.join("old.log").exists(), "the deleted file left its place");
        assert_eq!(trash.trashed.lock().unwrap().len(), 1);
        assert!(trash.holding.join("old.log").exists(), "trashed file is recoverable from the holding dir");

        // Undo restores the pre-execute state (recreates old.log, drops Sub/).
        let revert = crate::checkpoint_store::checkpoint_revert(
            &ctx,
            &root_str(&root),
            &cp.checkpoint.manifest_id,
        )
        .unwrap();
        assert!(revert.applied > 0, "{:?}", revert);
        assert!(root.join("old.log").exists(), "undo recreated the deleted file");
    }

    #[test]
    fn execute_revalidates_and_refuses_a_tampered_plan_without_touching_disk() {
        let root = scratch("tamper");
        fs::write(root.join("safe.txt"), b"safe").unwrap();
        let ctx = ctx_for(&root);
        let trash = FakeTrash::new(scratch("hold-tamper"));

        // A plan whose delete path was tampered to escape the root — as if a compromised frontend re-sent
        // it. Even though a human may have confirmed some earlier plan, execute must independently refuse.
        let tampered = FileOpPlan { ops: vec![FileOp::Delete { path: "/etc/passwd".into() }] };
        let out = execute_with(&ctx, &trash, &root_str(&root), &tampered).unwrap();

        assert!(out.checkpoint.is_none(), "no checkpoint for a refused plan");
        assert!(out.results.is_empty(), "nothing executed");
        assert!(!out.violations.is_empty(), "the refusal is explained");
        // Nothing was trashed, and NO checkpoint store was created (execute bailed before checkpointing).
        assert_eq!(trash.trashed.lock().unwrap().len(), 0);
        let list = crate::checkpoint_store::checkpoint_list(&ctx, &root_str(&root)).unwrap();
        assert!(list.is_empty(), "a refused plan takes no checkpoint");
    }

    #[test]
    fn execute_is_skip_on_error_not_all_or_nothing() {
        let root = scratch("skip");
        fs::write(root.join("real.txt"), b"1").unwrap();
        let ctx = ctx_for(&root);
        let trash = FakeTrash::new(scratch("hold-skip"));

        let plan = FileOpPlan {
            ops: vec![
                // This move fails: the source doesn't exist.
                FileOp::Move {
                    src: root_str(&root.join("missing.txt")),
                    dst: root_str(&root.join("Sub/missing.txt")),
                },
                // This mkdir still runs and succeeds.
                FileOp::Mkdir { path: root_str(&root.join("Made")) },
            ],
        };
        let out = execute_with(&ctx, &trash, &root_str(&root), &plan).unwrap();
        assert_eq!(out.results.len(), 2);
        assert!(!out.results[0].ok, "the bad move failed");
        assert!(out.results[1].ok, "the later mkdir still ran");
        assert!(root.join("Made").is_dir());
    }

    #[test]
    fn execute_refuses_to_overwrite_existing_destination() {
        let root = scratch("nooverwrite");
        fs::write(root.join("a.txt"), b"new").unwrap();
        fs::write(root.join("b.txt"), b"existing").unwrap();
        let ctx = ctx_for(&root);
        let trash = FakeTrash::new(scratch("hold-ow"));

        let plan = FileOpPlan {
            ops: vec![FileOp::Move {
                src: root_str(&root.join("a.txt")),
                dst: root_str(&root.join("b.txt")),
            }],
        };
        let out = execute_with(&ctx, &trash, &root_str(&root), &plan).unwrap();
        assert!(!out.results[0].ok);
        // The existing file is untouched and the source is still there.
        assert_eq!(fs::read(root.join("b.txt")).unwrap(), b"existing");
        assert!(root.join("a.txt").exists());
    }

    /// CPE-1705, staging **real byte loss** through the real `execute_with` entry point.
    /// `transfer_entry`'s guard was `if d.exists()`, and a `Move` op's outcome is `fs::rename`, which
    /// replaces its destination silently. A destination the copilot could not stat therefore read as
    /// free and the user's file was replaced — by an *AI-generated* plan the user approved on the
    /// understanding that "destination already exists" would stop it.
    ///
    /// Windows-only: on Unix a `chmod` that makes `stat` fail also denies `rename(2)` on the same
    /// directory, so the byte loss is not constructible there and the assertion would pass for the wrong
    /// reason. `execute_refuses_to_overwrite_existing_destination` above carries the honest case on all
    /// three CI legs.
    #[test]
    fn cpe_1705_execute_never_renames_over_a_destination_it_cannot_stat() {
        use std::io::Write;
        #[cfg(not(windows))]
        {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1705] SKIPPED the copilot execute byte-loss leg on this platform: `stat` and \
                 `rename(2)` on a destination share one directory's permission bits on Unix. NOTHING in \
                 this test covered CPE-1705's overwrite route on this run."
            );
        }
        #[cfg(windows)]
        {
            let root = scratch("cpe1705-denied");
            fs::write(root.join("a.txt"), b"NEW CONTENT").unwrap();
            let victim = root.join("b.txt");
            fs::write(&victim, b"VICTIM ORIGINAL").unwrap();
            let ctx = ctx_for(&root);
            let trash = FakeTrash::new(scratch("cpe1705-hold"));

            struct Restore<'a>(&'a Path, &'a Path);
            impl Drop for Restore<'_> {
                fn drop(&mut self) {
                    crate::fsutil::undo_deny_stat_of(self.0, self.1);
                }
            }
            let _r = Restore(&victim, &root);

            // `(R)` on the victim only. The root's `(DC)` is deliberately left intact — denying it too
            // would block the rename outright and the test would prove nothing about the guard.
            if !crate::fsutil::deny_stat_of(&victim) {
                let _ = writeln!(
                    std::io::stderr(),
                    "[CPE-1705] SKIPPED the copilot denied-destination leg: could not deny stat of {} on \
                     this machine. NOTHING in this test covered the overwrite route on this run.",
                    victim.display()
                );
                return;
            }

            let plan = FileOpPlan {
                ops: vec![FileOp::Move {
                    src: root_str(&root.join("a.txt")),
                    dst: root_str(&victim),
                }],
            };
            let out = execute_with(&ctx, &trash, &root_str(&root), &plan).unwrap();

            crate::fsutil::undo_deny_stat_of(&victim, &root);

            assert_eq!(
                fs::read(&victim).unwrap(),
                b"VICTIM ORIGINAL".to_vec(),
                "a destination whose stat we were refused must NEVER be renamed over"
            );
            assert!(!out.results[0].ok, "the op must be reported failed: {:?}", out.results[0]);
            assert!(
                out.results[0].error.contains("could not check what is at"),
                "must name the uncertainty rather than claim a collision: {}",
                out.results[0].error
            );
        }
    }

    /// CPE-1710, **site 1 of 2**: `apply_op`'s `Rename` arm. CPE-1705 gave it `clobber_refusal` and stopped
    /// there, so the destination name was probed with a check that **follows links** — and a *dangling*
    /// link resolves to nothing, so the slot read as free and `fs::rename` (which does not follow the final
    /// component) destroyed the link itself.
    ///
    /// **The assertion is on the slot, not on the `Result`.** The reviewer's reproduction returned
    /// `ok: true`: the op reported success while quietly deleting something the user had made. An
    /// assertion on the returned error would have passed pre-fix on the `ok` alone.
    ///
    /// No ACLs and no privileges are needed for the *bug*; a dangling link is an ordinary thing to have.
    /// Only *creating* the link can be refused (Windows without Developer Mode, where the junction
    /// fallback in `make_dangling_link` covers most machines), and that is a loud skip, never a silent one.
    #[test]
    fn cpe_1710_execute_never_renames_over_a_dangling_link_at_the_new_name() {
        use std::io::Write;
        let root = scratch("cpe1710-rename");
        fs::write(root.join("a.txt"), b"NEW CONTENT").unwrap();
        let link = root.join("b.txt");
        if !crate::fsutil::make_dangling_link(&link) {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1710] SKIPPED the copilot Rename dangling-link leg: this machine could not create a \
                 link at {} (Windows without Developer Mode / admin, and no junction either). NOTHING in \
                 this test covered the link-destruction route on this run.",
                link.display()
            );
            return;
        }
        let ctx = ctx_for(&root);
        let trash = FakeTrash::new(scratch("cpe1710-hold-rename"));

        let plan = FileOpPlan {
            ops: vec![FileOp::Rename {
                path: root_str(&root.join("a.txt")),
                new_name: "b.txt".to_string(),
            }],
        };
        let out = execute_with(&ctx, &trash, &root_str(&root), &plan).unwrap();

        assert!(
            fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()),
            "the dangling link at the new name was DESTROYED — `clobber_refusal` alone follows the link, \
             finds nothing, reads the slot as free, and `fs::rename` replaces the link itself"
        );
        assert!(!out.results[0].ok, "the op must be reported failed, not silently succeeded: {:?}", out.results[0]);
        assert!(
            out.results[0].error.contains("is a link"),
            "and it must say what is in the way: {}",
            out.results[0].error
        );
        // The source is still where it started — a refusal that moved the file anyway is not a refusal.
        assert_eq!(fs::read(root.join("a.txt")).unwrap(), b"NEW CONTENT".to_vec());
    }

    /// CPE-1710, **site 2 of 2**: `transfer_entry`, reached here through a `Move` op — the arm whose
    /// outcome is `fs::rename`. Same missing half of the same guard, a separate function and a separate
    /// test, so breaking one guard reds one test.
    #[test]
    fn cpe_1710_execute_never_moves_over_a_dangling_link_at_the_destination() {
        use std::io::Write;
        let root = scratch("cpe1710-move");
        fs::write(root.join("a.txt"), b"NEW CONTENT").unwrap();
        let link = root.join("dest.txt");
        if !crate::fsutil::make_dangling_link(&link) {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1710] SKIPPED the copilot Move dangling-link leg: this machine could not create a \
                 link at {} (Windows without Developer Mode / admin, and no junction either). NOTHING in \
                 this test covered the link-destruction route on this run.",
                link.display()
            );
            return;
        }
        let ctx = ctx_for(&root);
        let trash = FakeTrash::new(scratch("cpe1710-hold-move"));

        let plan = FileOpPlan {
            ops: vec![FileOp::Move {
                src: root_str(&root.join("a.txt")),
                dst: root_str(&link),
            }],
        };
        let out = execute_with(&ctx, &trash, &root_str(&root), &plan).unwrap();

        assert!(
            fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()),
            "the dangling link at the destination was DESTROYED by the move — the same half-guard, at the \
             site that carries every Move and Copy the copilot plans"
        );
        assert!(!out.results[0].ok, "the op must be reported failed, not silently succeeded: {:?}", out.results[0]);
        assert!(
            out.results[0].error.contains("is a link"),
            "and it must say what is in the way: {}",
            out.results[0].error
        );
        assert_eq!(fs::read(root.join("a.txt")).unwrap(), b"NEW CONTENT".to_vec());
    }

    /// Create `link` pointing at directory `target` without needing admin: an NTFS **junction** on Windows
    /// (no privilege) and a symlink on Unix. Returns whether it was created (some CI/sandbox envs forbid
    /// even junctions — the pure `parent_confined` unit test still covers the confinement logic there).
    fn make_dir_link(target: &Path, link: &Path) -> bool {
        #[cfg(windows)]
        {
            junction::create(target, link).is_ok()
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link).is_ok()
        }
        #[cfg(not(any(windows, unix)))]
        {
            let _ = (target, link);
            false
        }
    }

    #[test]
    fn parent_confined_accepts_in_root_and_rejects_escapes() {
        let root = scratch("confine");
        let canon = root.canonicalize().unwrap();
        // A normal in-root leaf (parent is root).
        assert!(parent_confined(&canon, &root.join("a.txt")));
        // A deep not-yet-existing path: the deepest existing ancestor is root → confined.
        assert!(parent_confined(&canon, &root.join("new/deep/x")));
        // A sibling entirely outside root.
        let outside = scratch("confine-out");
        assert!(!parent_confined(&canon, &outside.join("y")));
        // Through a symlink/junction to outside → rejected (the core traversal guard).
        let link = root.join("out");
        if make_dir_link(&outside, &link) {
            assert!(
                !parent_confined(&canon, &link.join("x")),
                "a path resolving out of root via a symlinked component must be rejected"
            );
        }
    }

    #[test]
    fn execute_refuses_ops_that_escape_root_through_a_symlinked_component() {
        let root = scratch("symlink-escape");
        let outside = scratch("outside-target");
        fs::write(outside.join("victim.txt"), b"precious").unwrap();

        let link = root.join("link"); // root/link -> outside (junction/symlink, no admin needed)
        if !make_dir_link(&outside, &link) {
            eprintln!("skipping symlink-escape exec test: could not create a link in this environment \
(the parent_confined unit test still covers the confinement logic)");
            return;
        }
        let ctx = ctx_for(&root);
        let trash = FakeTrash::new(scratch("hold-symlink"));

        // Both ops pass the TEXTUAL validate (they start with root) but resolve through the link to OUTSIDE.
        let plan = FileOpPlan {
            ops: vec![
                FileOp::Delete { path: root_str(&link.join("victim.txt")) },
                FileOp::Mkdir { path: root_str(&link.join("newdir")) },
            ],
        };
        let out = execute_with(&ctx, &trash, &root_str(&root), &plan).unwrap();

        // Every op refused, with a clear escape reason, and NOTHING outside root was touched.
        assert_eq!(out.results.len(), 2);
        assert!(out.results.iter().all(|r| !r.ok), "{:?}", out.results);
        assert!(
            out.results.iter().all(|r| r.error.contains("symlink") || r.error.contains("outside")),
            "{:?}",
            out.results
        );
        assert!(outside.join("victim.txt").exists(), "the out-of-root file must NOT be deleted");
        assert!(!outside.join("newdir").exists(), "no directory must be created out of root");
        assert_eq!(trash.trashed.lock().unwrap().len(), 0, "nothing was trashed");
    }

    #[test]
    fn config_default_is_disabled_and_carries_no_key() {
        let c = CopilotConfig::default();
        assert!(!c.enabled);
        assert!(c.base_url.is_empty());
        assert!(c.model.is_empty());
        // The struct has no key field at all — a key can only live in the keychain.
        let json = serde_json::to_string(&c).unwrap();
        assert!(!json.contains("key"), "config JSON must not carry a key: {json}");
    }

    #[test]
    fn resolve_planner_rejects_disabled_or_unconfigured() {
        assert!(resolve_planner(&CopilotConfig::default(), None).is_err());
        let enabled_no_url = CopilotConfig { enabled: true, base_url: "  ".into(), model: "m".into() };
        assert!(resolve_planner(&enabled_no_url, None).is_err());
        let enabled_no_model = CopilotConfig { enabled: true, base_url: "http://h/v1".into(), model: "".into() };
        assert!(resolve_planner(&enabled_no_model, None).is_err());
    }
}
