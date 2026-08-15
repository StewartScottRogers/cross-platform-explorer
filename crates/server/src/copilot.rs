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
//! plan is always inspectable, and validate + re-validate keep every path **textually** under `root`.
//!
//! Textual is not enough, and that is [`crate::fsutil::confined_to`]'s job: a component under `root` that
//! is a symlink or an NTFS junction can resolve *out* of it while spelling the path exactly like an
//! in-root one. Every path field of every op is put through that one primitive in [`apply_op`] before any
//! mutation. **CPE-1750** rewrote this paragraph: it used to be a local `parent_confined` that inspected
//! only the *parent* chain, walked *past* a dangling link, and therefore answered "confined" for
//! `root/dangling` and `root/dangling/x.txt` — while `create_dir_all`/`fs::copy` follow that link and act
//! at its target. The guard is now the same function the protocol rigs use, and there is one answer to
//! "is this inside the folder?" in this crate rather than three.
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
/// and the per-op [`crate::fsutil::confined_to`] check in [`apply_op`] defends against a symlink/junction
/// component **resolving out** of `root` at kernel time (the data-loss guard). What the backend can NOT floor is the *breadth* of
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
    // Resolve the real root ONCE so the per-op confinement check ([`apply_op`]) can compare each
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
/// before the op mutates. `new_name` is deliberately excluded: it is a bare name, and the slot it names
/// (`path`'s parent joined with it) is reached only after `path` itself has been confined **in full**
/// (CPE-1750 — which transitively confines its parent), and is then guarded at the primitive by
/// [`crate::fsutil::rename_into_slot`], which refuses a link sitting in that slot. Mirrors
/// `op_plan::FileOp::path_fields` (private there).
fn op_path_fields(op: &FileOp) -> Vec<(&'static str, &str)> {
    match op {
        FileOp::Move { src, dst } => vec![("src", src), ("dst", dst)],
        FileOp::Copy { src, dst } => vec![("src", src), ("dst", dst)],
        FileOp::Rename { path, .. } => vec![("path", path)],
        FileOp::Delete { path } => vec![("path", path)],
        FileOp::Mkdir { path } => vec![("path", path)],
    }
}

// ─────────────────────────────────────────────────────────────────────────────────────────────────
// CPE-1750 — there is deliberately NO containment primitive in this file
//
// This module used to carry its own `parent_confined`. It is gone, and nothing like it may come back:
// containment has ONE answer in this crate, `fsutil::confined_to`, and every caller asks it.
// The deleted version was wrong in two independent ways, both measured (CPE-1750, from PR #909's
// review):
//
//   * its `Err(_) => cur = dir.parent()` walked **past a dangling link** — `canonicalize` reports
//     `NotFound` for one, which reads identically to "this name does not exist yet", so
//     `root/dangling -> <outside>/soon` looked like an ordinary not-yet-created folder; and
//   * it only ever inspected `path.parent()`, so a link at the **final component** was invisible to it.
//
// Together those made it answer `true` for `root/dangling`, `root/dangling/x.txt` and `root/live`
// (a live link out of the root) — the exact three inputs `confined_to` answers `false` for, and the
// exact three where `create_dir_all`/`fs::copy`/`trash::delete` act at the link's target rather than
// at the name the human confirmed.
//
// `fsutil::contained_under` is the other neighbour and is NOT interchangeable: it fails **open** on a
// path that does not exist yet, which is right for its removal-side callers and wrong for every field
// here (a `Copy` destination or an `Mkdir` name not existing yet is the ordinary case).
// ─────────────────────────────────────────────────────────────────────────────────────────────────

/// Say why a path failed [`crate::fsutil::confined_to`] **without over-claiming** (CPE-1750).
///
/// `confined_to` answers one bit, and its "no" covers two different truths: the path really does resolve
/// outside the confirmed folder, **or** the OS would not say where it resolves (`EACCES`, `ELOOP`, a
/// Windows sharing violation) and the guard fails closed. Refusing is right for both. *Saying* the first
/// when the truth is the second is the confident-false-statement failure this repo has now filed four
/// tickets about (CPE-1687, CPE-1705, CPE-1710, CPE-1716) — "it says the file is outside the folder, but
/// I can see that it isn't" sends a user looking for a problem that does not exist.
///
/// So a refusal asks `try_exists` — the same probe [`crate::fsutil::clobber_refusal`] classifies, and
/// nothing more; no second containment walk is derived here — purely to choose the wording, and the
/// uncertain case is phrased by [`crate::fsutil::unknown_slot_message`], which exists precisely so the
/// sites that cannot call `clobber_refusal` wholesale still word the unknown identically.
fn confinement_refusal(field: &str, path: &Path) -> String {
    let stat = path.try_exists();
    if stat.is_err() {
        return format!("refused: {field} — {}", crate::fsutil::unknown_slot_message(path, &stat));
    }
    format!("refused: {field} {path:?} resolves outside the folder (symlink/junction escape)")
}

/// Apply one whitelisted op, returning its [`OpResult`]. Never all-or-nothing: a failure (locked file,
/// collision, unreadable source) is a failed result for that op and the caller runs the rest. Move/copy
/// **refuse to overwrite** an existing destination (a safer default than clobbering); deletes go to trash.
///
/// # Symlink/junction confinement — the data-loss guard
///
/// Before any mutation, **every path field, in full — final component included** — must resolve within
/// `canonical_root` per [`crate::fsutil::confined_to`]. An op that resolves outside (via a symlinked or
/// junctioned component that passed the purely textual [`op_plan::validate`], or via a link *at* the name
/// itself) is **refused** as a failed result and never reaches a primitive. That matters here more than
/// almost anywhere else in the app: a mutation that lands outside the confirmed folder is also outside the
/// pre-execute checkpoint, so the app's own one-click undo cannot take it back.
///
/// `confined_to` fails **closed**: an `EACCES`/`ELOOP`/sharing-violation it cannot resolve is refused, not
/// waved through. For a guard on a path about to be created or written, "I could not tell" must not mean
/// "go ahead", and a refused op is a reported failure the human can act on.
///
/// # What this does NOT cover — recorded here, where a reader of the Copilot path will hit it
///
/// - **It is not atomic with the primitive.** Between this check and the `fs::rename`/`fs::copy`/
///   `create_dir_all`/`trash::delete` below, another process could replace a component with a link out of
///   the tree (a TOCTOU swap) and the mutation would follow it. Closing that needs
///   `openat2(RESOLVE_BENEATH)` on Linux or an `O_NOFOLLOW` walk on each component, neither of which
///   `std` offers, so it is recorded rather than solved — the same residual [`crate::fsutil::confined_to`]
///   states about itself. The window is small and the attacker must already have write access inside the
///   folder the human confirmed; it is nonetheless real, and this is not a security boundary against a
///   local adversary racing the app.
/// - **It says nothing about what the primitive then does to a link the path resolves *inside* the root.**
///   A contained link may still be written *through* or destroyed; that is the separate CPE-1710/CPE-1716
///   question, answered by [`crate::fsutil::rename_into_slot`]/[`crate::fsutil::rename_slot_refusal`],
///   which the move/copy/rename arms below call.
/// - It says nothing about the **breadth** of `root` itself — see [`execute_with`]'s scope note.
///
/// # Ordering: this runs BEFORE the slot guards, and that shadows one of their messages
///
/// Containment is asked first, because a path that may resolve outside the folder must not be probed,
/// stat'd or written at all. One consequence, measured by CPE-1705's own copilot leg: a destination the
/// OS refuses to `stat` used to reach [`crate::fsutil::rename_slot_refusal`] and get its
/// "could not check what is at …" wording from there. It now stops here instead, because `confined_to`
/// fails closed on `EACCES`. [`confinement_refusal`] therefore borrows the very same shared wording, so
/// the user sees no difference and CPE-1705's property still holds at this seam.
fn apply_op(op: &FileOp, canonical_root: &Path, trash: &dyn TrashBin) -> OpResult {
    for (field, value) in op_path_fields(op) {
        // Argument order is (path, root) here and (root, path) on the deleted `parent_confined` —
        // deliberately not aliased behind a local wrapper, so the one primitive is called by name.
        let p = Path::new(value);
        if !crate::fsutil::confined_to(p, canonical_root) {
            return OpResult::err(p, confinement_refusal(field, p));
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
    ///
    /// **CPE-1750 moved which guard says so, and left the property alone.** The confinement check now
    /// runs before the slot check and `confined_to` fails closed on the `EACCES` this test stages, so the
    /// refusal comes from `apply_op`'s guard rather than from `rename_slot_refusal`. `confinement_refusal`
    /// borrows `fsutil::unknown_slot_message` for exactly that case, so the wording asserted below is
    /// unchanged — and the extra assertion added here pins the thing that could quietly go wrong: the
    /// containment guard must not answer an "I could not read it" with a confident "it is outside the
    /// folder". `rename_slot_refusal`'s own unknown branch keeps its direct coverage in `fsutil`'s tests.
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
            // CPE-1750: …and must not have swapped one confident false claim for another. The path is
            // an ordinary file directly inside the confirmed folder; the only true statement about it is
            // that it could not be read.
            assert!(
                !out.results[0].error.contains("resolves outside the folder"),
                "an unreadable in-root destination must NOT be reported as a containment escape — it is \
                 a file the user can see sitting in the folder they confirmed: {}",
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

    /// Every op in `plan` was refused, and refused *by the containment guard* rather than by whatever
    /// the primitive would have said next.
    ///
    /// The second half is what makes the guard non-deletable. Several of these inputs also happen to
    /// make `create_dir_all` fail with `EEXIST`/`ENOENT` on some platforms, so a test asserting only
    /// "the op failed" would stay green with the guard removed — precisely the shape that shipped a
    /// deletable guard earlier this sprint. Asserting the *reason* pins the decision at the seam that
    /// makes it.
    fn assert_all_refused_for_escaping(results: &[OpResult], plan: &FileOpPlan) {
        assert_eq!(results.len(), plan.ops.len(), "one result per op: {results:?}");
        for (r, op) in results.iter().zip(&plan.ops) {
            assert!(!r.ok, "{op:?} must be refused, not reported successful: {r:?}");
            assert!(
                r.error.contains("resolves outside the folder"),
                "{op:?} must be refused BY THE CONFINEMENT GUARD — an incidental primitive failure \
                 (\"File exists\", \"cannot find the path\") means the guard could be deleted with this \
                 test still green. Got: {}",
                r.error
            );
        }
    }

    #[test]
    fn execute_refuses_ops_that_escape_root_through_a_symlinked_component() {
        let root = scratch("symlink-escape");
        let outside = scratch("outside-target");
        fs::write(outside.join("victim.txt"), b"precious").unwrap();
        fs::write(root.join("a.txt"), b"payload").unwrap();

        let link = root.join("link"); // root/link -> outside (junction/symlink, no admin needed)
        if !crate::fsutil::make_dir_link(&outside, &link) {
            crate::skip_notice!(
                "[CPE-1750] SKIPPED the symlinked-intermediate-component exec leg: this machine could \
                 not create a directory link at {} (no symlink privilege and no junction). NOTHING on \
                 this run covered a Copilot op reaching a real primitive THROUGH a link out of the \
                 confirmed folder.",
                link.display()
            );
            return;
        }
        let ctx = ctx_for(&root);
        let trash = FakeTrash::new(scratch("hold-symlink"));

        // Every op passes the TEXTUAL validate (they all start with root) but resolves through the link
        // to OUTSIDE. Copy and Move are here as well as Delete/Mkdir because they are the arms that call
        // `create_dir_all` on the destination's parent and then `fs::copy`/`fs::rename` — the three
        // primitives that would have materialised something out of the folder the human confirmed.
        let plan = FileOpPlan {
            ops: vec![
                FileOp::Delete { path: root_str(&link.join("victim.txt")) },
                FileOp::Mkdir { path: root_str(&link.join("newdir")) },
                FileOp::Copy { src: root_str(&root.join("a.txt")), dst: root_str(&link.join("copied.txt")) },
                FileOp::Move { src: root_str(&root.join("a.txt")), dst: root_str(&link.join("moved.txt")) },
            ],
        };
        let out = execute_with(&ctx, &trash, &root_str(&root), &plan);

        // The EFFECT first: this family fails by *succeeding*, so anything asserted after an `unwrap()`
        // is unreachable on a build that has the bug.
        assert!(
            outside.join("victim.txt").exists(),
            "\"{}\" — a file OUTSIDE the confirmed folder {} — was destroyed by a Copilot Delete that \
             reached it through the link at {}",
            outside.join("victim.txt").display(),
            root.display(),
            link.display()
        );
        for escaped in ["newdir", "copied.txt", "moved.txt"] {
            assert!(
                !outside.join(escaped).exists(),
                "\"{}\" was CREATED outside the confirmed folder {} — the Copilot wrote through the link \
                 at {}, and being outside the folder it is also outside the pre-execute checkpoint, so \
                 the app's own undo cannot take it back",
                outside.join(escaped).display(),
                root.display(),
                link.display()
            );
        }
        let trashed = trash.trashed.lock().unwrap().clone();
        assert!(trashed.is_empty(), "the trash seam was handed out-of-root paths: {trashed:?}");

        let out = out.unwrap();
        assert_all_refused_for_escaping(&out.results, &plan);
        assert_eq!(fs::read(root.join("a.txt")).unwrap(), b"payload".to_vec(), "the source stayed put");
    }

    /// CPE-1750, defect **2 of 2**: the old `parent_confined` inspected only `path.parent()`, so a link
    /// **at the final component** was invisible to it. `Mkdir` then reached `create_dir_all`, which
    /// follows the link and reports success about a directory outside the confirmed folder, and `Delete`
    /// reached the [`TrashBin`] seam — `trash::delete` on the user's real filesystem in the shipped app —
    /// with a path resolving outside it.
    ///
    /// The second half of this test is the discrimination check: a link at the leaf that resolves back
    /// *inside* the root must still be allowed, so the fix is containment and not a blanket "no links".
    #[test]
    fn cpe_1750_execute_refuses_an_op_whose_own_final_component_links_out_of_the_root() {
        let root = scratch("cpe1750-leaf");
        let outside = scratch("cpe1750-leaf-outside");
        fs::write(outside.join("victim.txt"), b"precious").unwrap();
        fs::create_dir_all(root.join("sub")).unwrap();

        let out_link = root.join("outlink"); // root/outlink -> <outside>
        let in_link = root.join("inlink"); // root/inlink  -> root/sub
        if !crate::fsutil::make_dir_link(&outside, &out_link)
            || !crate::fsutil::make_dir_link(&root.join("sub"), &in_link)
        {
            crate::skip_notice!(
                "[CPE-1750] SKIPPED the leaf-link exec leg: this machine could not create a directory \
                 link at {} (no symlink privilege and no junction). NOTHING on this run covered a \
                 Copilot op whose OWN final component resolves out of the confirmed folder, nor the \
                 discrimination check that an in-root link is still allowed.",
                out_link.display()
            );
            return;
        }
        let ctx = ctx_for(&root);
        let trash = FakeTrash::new(scratch("cpe1750-leaf-hold"));

        let plan = FileOpPlan {
            ops: vec![
                FileOp::Mkdir { path: root_str(&out_link) },
                FileOp::Delete { path: root_str(&out_link) },
            ],
        };
        let out = execute_with(&ctx, &trash, &root_str(&root), &plan);

        // EFFECT BEFORE UNWRAP. `Delete` here is the one that actually removes something: the trash seam
        // takes the link away from the confirmed folder while its target sits outside.
        assert!(
            fs::symlink_metadata(&out_link).is_ok_and(|m| m.file_type().is_symlink()),
            "the link at \"{}\" was DESTROYED — the Copilot's Delete reached the trash seam with a path \
             that resolves to \"{}\", outside the confirmed folder \"{}\", because the guard never looked \
             at the op path's own final component",
            out_link.display(),
            outside.display(),
            root.display()
        );
        let trashed = trash.trashed.lock().unwrap().clone();
        assert!(
            trashed.is_empty(),
            "the trash seam — `trash::delete` on the user's real filesystem in the shipped app — was \
             handed {trashed:?}, which resolves to \"{}\", outside the confirmed folder \"{}\"",
            outside.display(),
            root.display()
        );
        assert!(outside.join("victim.txt").exists(), "the out-of-root file must survive");

        let out = out.unwrap();
        assert_all_refused_for_escaping(&out.results, &plan);

        // …and the discrimination check: same shape, but the link stays inside the root, so containment
        // has nothing to say about it and the op runs.
        let inside_plan = FileOpPlan { ops: vec![FileOp::Mkdir { path: root_str(&in_link) }] };
        let inside = execute_with(&ctx, &trash, &root_str(&root), &inside_plan).unwrap();
        assert!(
            inside.results[0].ok,
            "a leaf link resolving back INSIDE the confirmed folder must not be refused as an escape — \
             the guard is containment, not a ban on links: {:?}",
            inside.results[0]
        );
    }

    /// CPE-1750, defect **1 of 2**: `canonicalize` reports `NotFound` for a **dangling** link exactly as
    /// it does for a name that simply is not there, and the old guard's `Err(_) => cur = dir.parent()`
    /// could not tell those apart — so it walked straight past `root/dangling -> <outside>/soon` and
    /// called everything under it confined.
    ///
    /// Some of these inputs are also refused further down by an incidental `create_dir_all` failure on
    /// some platforms; that is why [`assert_all_refused_for_escaping`] insists on the containment
    /// *reason*. The hazard is not hypothetical: `create_dir_all`'s behaviour on a dangling reparse point
    /// differs between Windows and POSIX, so "some platform happens to stop it" is not a guard.
    #[test]
    fn cpe_1750_execute_refuses_ops_under_a_dangling_link_pointing_out_of_the_root() {
        let root = scratch("cpe1750-dangling");
        let outside = scratch("cpe1750-dangling-outside");
        let soon = outside.join("soon"); // exists only long enough to hang the link on
        fs::create_dir_all(&soon).unwrap();
        fs::write(root.join("a.txt"), b"payload").unwrap();

        let dangling = root.join("dangling"); // root/dangling -> <outside>/soon, then `soon` is removed
        if !crate::fsutil::make_dir_link(&soon, &dangling) {
            crate::skip_notice!(
                "[CPE-1750] SKIPPED the dangling-link exec leg: this machine could not create a directory \
                 link at {} (no symlink privilege and no junction). NOTHING on this run covered a Copilot \
                 op whose path runs through a link that dangles OUT of the confirmed folder.",
                dangling.display()
            );
            return;
        }
        fs::remove_dir(&soon).unwrap();
        assert!(
            fs::symlink_metadata(&dangling).is_ok_and(|m| m.file_type().is_symlink())
                && !matches!(dangling.try_exists(), Ok(true)),
            "staging premise: \"{}\" must be a link, and it must dangle",
            dangling.display()
        );

        let ctx = ctx_for(&root);
        let trash = FakeTrash::new(scratch("cpe1750-dangling-hold"));

        let plan = FileOpPlan {
            ops: vec![
                // The leaf itself…
                FileOp::Mkdir { path: root_str(&dangling) },
                FileOp::Delete { path: root_str(&dangling) },
                // …and the ticket's measured pair, one component deeper.
                FileOp::Mkdir { path: root_str(&dangling.join("newdir")) },
                FileOp::Copy { src: root_str(&root.join("a.txt")), dst: root_str(&dangling.join("x.txt")) },
            ],
        };
        let out = execute_with(&ctx, &trash, &root_str(&root), &plan);

        // EFFECT BEFORE UNWRAP: nothing may have been materialised at the link's target, which is where
        // `create_dir_all`/`fs::copy` would have acted.
        assert!(
            !soon.exists(),
            "\"{}\" was CREATED outside the confirmed folder \"{}\" — the Copilot followed the dangling \
             link at \"{}\" and materialised its target, which is also outside the pre-execute \
             checkpoint, so the app's own undo cannot take it back",
            soon.display(),
            root.display(),
            dangling.display()
        );
        for escaped in ["newdir", "x.txt"] {
            assert!(
                !soon.join(escaped).exists(),
                "\"{}\" landed outside the confirmed folder \"{}\" through the dangling link at \"{}\"",
                soon.join(escaped).display(),
                root.display(),
                dangling.display()
            );
        }
        let trashed = trash.trashed.lock().unwrap().clone();
        assert!(
            trashed.is_empty(),
            "the trash seam was handed {trashed:?}, which resolves to \"{}\", outside the confirmed \
             folder \"{}\"",
            soon.display(),
            root.display()
        );

        let out = out.unwrap();
        assert_all_refused_for_escaping(&out.results, &plan);
        assert_eq!(fs::read(root.join("a.txt")).unwrap(), b"payload".to_vec(), "the source stayed put");
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
