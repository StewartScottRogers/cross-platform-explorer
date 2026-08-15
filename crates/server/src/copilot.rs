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
//! mutation, together with a second question `confined_to` deliberately does not answer — *is this path
//! the confirmed folder itself?* — asked via [`crate::fsutil::same_place`], because an op on the folder
//! acts in the folder's parent.
//!
//! Those two questions are asked of the op's own path **fields**. A `Copy` of a folder then walks that
//! folder's children, and **CPE-1756** added the same question there, per entry that is a link: a child
//! link out of the folder used to be followed by `fs::copy`, pulling outside content *in* — no mutation
//! escaped, but the confirmed folder ended up holding a file the human never chose. Which entries need
//! asking, and why an ordinary link-free tree pays nothing for it, is argued on [`copy_recursive`].
//!
//! **CPE-1750** rewrote this paragraph: it used to be a local `parent_confined` that inspected
//! only the *parent* chain, walked *past* a dangling link, and therefore answered "confined" for
//! `root/dangling` and `root/dangling/x.txt` — while `create_dir_all`/`fs::copy` follow that link and act
//! at its target. The guard is now the same function the protocol rigs use, and there is one answer to
//! "is this inside the folder?" in this crate rather than three.
//!
//! The [`crate::copilot_planner::LlmPlanner`] and [`TrashBin`] are seams, so the whole chain is tested with
//! a [`crate::copilot_planner::FakePlanner`] + a fake trash — no network, no real recycle bin.

use std::path::{Path, PathBuf};

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

/// Where a [`FileOp::Rename`] will actually land: `path`'s lexical parent joined with the bare
/// `new_name`. `None` when `path` has no parent, which the rename arm reports as its own error.
///
/// **One computation, two callers** — [`op_path_fields`] puts the result through the guard and
/// [`apply_op`]'s rename arm hands the same result to [`crate::fsutil::rename_into_slot`]. Deriving it
/// twice is how a guard and the call it guards drift apart, which is the shape this file has already
/// been bitten by three times (CPE-1705, CPE-1710, CPE-1750).
fn rename_destination(path: &Path, new_name: &str) -> Option<PathBuf> {
    Some(path.parent()?.join(new_name))
}

/// Every path an op will act on, as `(field, value)` pairs — the complete list the guards in
/// [`apply_op`] iterate. Mirrors `op_plan::FileOp::path_fields` (private there), **plus the rename's
/// computed destination**, which that one does not carry.
///
/// # Why the rename destination is in here (CPE-1750, attempt 2)
///
/// The first attempt left it out, reasoning that confining `path` transitively confines `path`'s parent
/// and therefore the slot `parent.join(new_name)` names. **That reasoning is false at exactly one
/// input, and it is the dangerous one:** when `path` *is* the confirmed root, its parent is outside the
/// root by definition, so `Rename { path: <root>, new_name: "away" }` computed a destination in the
/// root's *parent* — and [`crate::fsutil::rename_into_slot`] guards only **what is sitting in the slot**,
/// never **where the slot is**, so an empty name outside the folder sailed straight through and the
/// confirmed folder was relocated out of itself. Measured on PR #916 by the reviewer.
///
/// `new_name` itself still is not listed: `op_plan::validate` (which `execute_with` re-runs before
/// anything) rejects it if it is empty, contains `/` or `\`, or is `.`/`..`, so it cannot relocate — it
/// can only name a slot, and that slot is now what gets guarded.
fn op_path_fields(op: &FileOp) -> Vec<(&'static str, PathBuf)> {
    match op {
        FileOp::Move { src, dst } => vec![("src", PathBuf::from(src)), ("dst", PathBuf::from(dst))],
        FileOp::Copy { src, dst } => vec![("src", PathBuf::from(src)), ("dst", PathBuf::from(dst))],
        FileOp::Rename { path, new_name } => {
            let p = PathBuf::from(path);
            let dst = rename_destination(&p, new_name);
            let mut fields = vec![("path", p)];
            fields.extend(dst.map(|d| ("dst", d)));
            fields
        }
        FileOp::Delete { path } => vec![("path", PathBuf::from(path))],
        FileOp::Mkdir { path } => vec![("path", PathBuf::from(path))],
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
    // Deliberately does NOT name a link as the cause. A path reaches here for either of two reasons: a
    // link (or junction) along it redirects out, or the path simply leads out on its own — the rename
    // destination of an op naming the confirmed folder is the second kind, and it involves no link at
    // all. Naming a "symlink/junction escape" there was measured as misleading by CPE-1750's UAT: true
    // that it resolves outside, wrong about why, and this module has already spent three review rounds
    // on messages that were confident about the wrong cause. State what was established — it resolves
    // outside — and stop there.
    format!("refused: {field} {path:?} resolves outside the folder")
}

/// Say why an op was refused for naming **the confirmed folder itself** (CPE-1750, attempt 2).
///
/// A different sentence from [`confinement_refusal`] on purpose: this path is *not* outside the folder,
/// so saying it is would be false, and the user's next question ("which part of my folder is outside my
/// folder?") has no answer. What is true is that the operation's subject is the folder rather than
/// something in it, so carrying it out would act at the folder's parent — outside.
fn root_itself_refusal(field: &str, path: &Path) -> String {
    format!(
        "refused: {field} {path:?} IS the folder you confirmed, not something inside it — moving, \
         renaming or deleting the folder itself would act in its parent, outside the folder, and \
         outside what Undo can restore"
    )
}

/// Apply one whitelisted op, returning its [`OpResult`]. Never all-or-nothing: a failure (locked file,
/// collision, unreadable source) is a failed result for that op and the caller runs the rest. Move/copy
/// **refuse to overwrite** an existing destination (a safer default than clobbering); deletes go to trash.
///
/// # Two questions, asked about EVERY field in [`op_path_fields`], before anything mutates
///
/// **1. Does it stay inside the folder?** Every path field, in full — final component included — must
/// resolve within `canonical_root` per [`crate::fsutil::confined_to`]. An op that resolves outside (via a
/// symlinked or junctioned component that passed the purely textual [`op_plan::validate`], or via a link
/// *at* the name itself) is **refused** as a failed result and never reaches a primitive. That matters
/// here more than almost anywhere else in the app: a mutation that lands outside the confirmed folder is
/// also outside the pre-execute checkpoint, so the app's own one-click undo cannot take it back.
///
/// `confined_to` fails **closed**: an `EACCES`/`ELOOP`/sharing-violation it cannot resolve is refused, not
/// waved through. For a guard on a path about to be created or written, "I could not tell" must not mean
/// "go ahead", and a refused op is a reported failure the human can act on.
///
/// **2. Is it the folder itself?** [`crate::fsutil::confined_to`] answers `true` for `root` — deliberately,
/// documented on itself, and it explicitly hands this second question back to the caller: *"the caller
/// still decides whether the root itself is an acceptable answer … 'Not the root itself' is `same_place`'s
/// question, and the rename sites ask both."* So this asks both, via
/// [`crate::fsutil::same_place`].
///
/// **CPE-1750 attempt 1 asked only the first, and that was a regression**, because the deleted
/// `parent_confined` had answered the second one by accident: it inspected `path.parent()`, and the root's
/// parent is outside the root by definition. Measured on PR #916 —
/// `Delete { path: <root> }` sent the entire confirmed folder to the trash seam and reported the op
/// **successful**, and `Rename { path: <root>, new_name: X }` relocated the folder to
/// `<parent-of-root>/X`. `op_plan::validate` does not stop either: its `within_root` is a `>=`-length
/// prefix test, so it is true for equality.
///
/// The refusal is uniform across all five ops rather than a list of "the destructive ones", because
/// "which arms need it" is the judgement this repo keeps getting wrong (a guard on three of four call
/// sites). `Mkdir { path: <root> }` was a no-op reported as success and is now an honest refusal; the
/// others were live hazards, including `Copy { src: <root> }`, which recursed a folder into itself.
///
/// The two questions run as **two passes**, containment over every field first. That ordering is not
/// cosmetic: it is what makes each guard's removal red a *different* test, since an op that trips both
/// (`Rename { path: <root> }` trips containment on its computed `dst` and identity on its `path`) must
/// report the stronger, more specific reason.
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
/// - **It asks about the op's own path fields, not about what lives UNDER them.** A `Copy` whose `src`
///   is a directory is confined here exactly once, and [`copy_recursive`] then walks its children. That
///   walk asks its own containment question, per entry, for the reason recorded on it (CPE-1756): a
///   child that is a link out of the folder was followed by `fs::copy` and pulled outside content *in*.
///   The answer still comes from the one primitive — see [`copy_recursive`] for the induction that says
///   which entries need asking.
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
    let fields = op_path_fields(op);
    // Pass 1 — does it stay inside the folder? Argument order is (path, root) here and (root, path) on
    // the deleted `parent_confined`; deliberately not aliased behind a local wrapper, so the one
    // primitive is called by name.
    for (field, value) in &fields {
        if !crate::fsutil::confined_to(value, canonical_root) {
            return OpResult::err(value, confinement_refusal(field, value));
        }
    }
    // Pass 2 — is it the folder ITSELF? `confined_to` says yes to the root by design and leaves this to
    // the caller; a whole pass later so a field that fails both is reported by the more specific reason.
    for (field, value) in &fields {
        if crate::fsutil::same_place(value, canonical_root) {
            return OpResult::err(value, root_itself_refusal(field, value));
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
        FileOp::Move { src, dst } => transfer_entry(src, dst, false, canonical_root),
        FileOp::Copy { src, dst } => transfer_entry(src, dst, true, canonical_root),
        FileOp::Rename { path, new_name } => {
            let p = Path::new(path);
            // The SAME computation the guards above ran on (CPE-1750 attempt 2) — if this were derived
            // separately, the guard and the call could come to disagree about where the rename lands.
            let Some(dst) = rename_destination(p, new_name) else {
                return OpResult::err(p, "cannot rename a path with no parent directory");
            };
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
///
/// `canonical_root` is carried through purely for [`copy_recursive`]'s per-entry containment question
/// (CPE-1756). The **move** branch does not need it: `fs::rename` moves the directory entry itself and
/// never descends, so a link inside a moved subtree stays a link and nothing is dereferenced.
fn transfer_entry(src: &str, dst: &str, copy: bool, canonical_root: &Path) -> OpResult {
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
        copy_recursive(s, d, canonical_root).map_err(|e| e.to_string())
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

/// Recursively copy `src` to `dst` (a file, or a whole directory tree), **refusing any entry that is a
/// link resolving outside `canonical_root`** (CPE-1756). Uses `symlink_metadata` so a link to a directory
/// is not silently followed into an unbounded copy of its target's tree.
///
/// # Why the walk asks containment again, and why it asks it ONLY at a link
///
/// [`apply_op`] confines the op's own `src` and `dst`, and CPE-1750 made that the single containment
/// answer in this crate. It says nothing about what lives *under* `src`, and this walk used to descend
/// un-asked: a child that is a live file link was handed straight to `fs::copy`, which **follows** the
/// final component, so "tidy up this folder" could deposit a byte-for-byte copy of a file from elsewhere
/// on the disk inside the folder the human confirmed.
///
/// Nothing lands *outside* — the destination side stays contained, because a `read_dir` name is a single
/// component and [`transfer_entry`]'s `rename_slot_refusal` means `dst` is always freshly created — so
/// CPE-1750's claim ("a refusal never reaches a primitive", for the op's own fields) was never false.
/// This is a read **inflow** rather than an escaping mutation. It is still content the user never chose,
/// now sitting in a folder they may share, sync, back up or hand to someone else.
///
/// The question is asked **only of entries that are links**, and that is a soundness argument, not a
/// saving taken on faith:
///
/// - `src` itself is confined — [`apply_op`] asked, before this ran.
/// - A `read_dir` name is one component: never `.`/`..`, never separator-bearing. So for a child that is
///   *not* a link, `canonicalize(child) == canonicalize(parent).join(name)`, which `starts_with` the real
///   root exactly when the parent's does. Its containment is settled by its parent's.
/// - The walk recurses only where `symlink_metadata().is_dir()` holds, and that is false for every link
///   (a junction included — Rust reports one as `is_symlink`). Every directory descended into is
///   therefore a non-link child of a confined directory, and the induction carries.
///
/// So a link is the only entry whose containment is not already decided, and a link is the only entry
/// asked. A link-free tree — the ordinary case — pays **zero** extra `canonicalize` calls, which is
/// `PURPOSE.md`'s fast/small/predictable tiebreaker honoured rather than traded away; the per-entry
/// `confined_to` the ticket floated as one option would have paid a full resolve on every node of a deep
/// tree to re-derive an answer the induction already gives.
///
/// # What it refused BEFORE — enumerated, so the change can be checked as additive
///
/// Nothing here refused anything *by name*; every stop was an incidental primitive error. Listing them is
/// CPE-1750's own lesson applied: replacing a check means enumerating what the old one refused, not only
/// what the new one refuses better.
///
/// | entry | before | now |
/// |---|---|---|
/// | a **directory** link (`symlink_metadata().is_dir()` is false for one) | not descended into, so no unbounded copy of its target's tree — then `fs::copy` fails on a directory and aborts the whole copy | refused **by name** when it resolves outside; the same incidental abort when it resolves inside |
/// | a **dangling** link | `fs::copy` fails `NotFound`, aborting the copy | refused by name when the dangle leads outside ([`crate::fsutil::confined_to`] fails closed there); the same `NotFound` when it leads back inside |
/// | a **live file** link | **followed** — the hole this closes | refused when it resolves outside; still followed when it resolves inside |
/// | an unreadable `src` or entry | `Err` from `symlink_metadata`/`read_dir` | unchanged |
///
/// Every row moves in one direction: something previously allowed is now refused, and nothing previously
/// refused is now allowed. The `symlink_metadata` call is kept for its own sake and not merely reused —
/// it is what stops a directory link being *descended*, which is a separate property from this guard.
///
/// # What this does NOT cover — recorded here, where a reader of the walk will hit it
///
/// - **A link resolving back INSIDE the folder is still dereferenced.** `fs::copy` copies the target's
///   bytes, so the copy holds a regular file where the original held a link. That is a real difference
///   from what "copy" might suggest — but the bytes are bytes the user already has inside the folder, and
///   reproducing the link would need `symlink_file`, which an unprivileged Windows session cannot create;
///   a copy that fails on Windows is worse than one that flattens. Deliberate, and pinned by CPE-1756's
///   discrimination leg so it cannot drift into "refuses every link" unnoticed — a guard that refuses
///   everything looks perfect.
/// - **A HARD link still pulls outside content in, and the op reports success.** Measured by CPE-1756's
///   review: a hard link inside the folder whose data lives outside it copies those bytes into the folder,
///   `ok = true`. Not a regression — identical before this guard — and, more interestingly, **not something
///   the primitive could have answered**: `confined_to` returns `true` for it, because a hard link *is* the
///   file and has a real directory entry inside the folder; `canonicalize` resolves it to its own in-root
///   name. So the rejected "ask `confined_to` for every entry" option would not have caught it either, and
///   narrowing to links loses nothing. Recorded because a reader of this walk would otherwise reasonably
///   conclude all outside-content inflow is now closed. It is not.
/// - **A refusal mid-walk leaves the partial copy behind.** The op is reported failed and `dst` holds
///   whatever was copied before the link was reached — all of it from inside the folder. That is this
///   function's behaviour for *any* mid-walk error and predates this guard; recorded, not changed.
/// - **It is not atomic with `fs::copy`.** The TOCTOU residual [`apply_op`] and
///   [`crate::fsutil::confined_to`] both state applies here too, now once per link: a component could be
///   swapped between `read_dir` and the copy.
/// - **The pre-execute checkpoint covers no link either way.** [`crate::snapshot_capture::scan_dir`] uses
///   `DirEntry::metadata()`, which does not traverse, so a link is neither `is_dir` nor `is_file` there
///   and is skipped outright — Undo cannot restore one. Established while answering CPE-1756's "does any
///   other recursive walk share this shape?": `scan_dir` is the Copilot's only other recursive walk of the
///   user's tree, and skipping links is exactly why it has **no** inflow of this shape. `list_plan_entries`
///   is one level deep and reads names only. This walk was the only one.
fn copy_recursive(src: &Path, dst: &Path, canonical_root: &Path) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(src)?;
    if meta.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dst.join(entry.file_name()), canonical_root)?;
        }
        Ok(())
    } else {
        // The one containment primitive, by name, as CPE-1750 requires — not a second local answer.
        if meta.file_type().is_symlink() && !crate::fsutil::confined_to(src, canonical_root) {
            return Err(std::io::Error::other(link_inflow_refusal(src)));
        }
        std::fs::copy(src, dst).map(|_| ())
    }
}

/// Say why a copy stopped at a **link that leads out of the confirmed folder** (CPE-1756).
///
/// A different sentence from [`confinement_refusal`] on purpose, and the difference is not decoration:
/// that one is about a path the *plan* named, this one about a path the *walk* found, and the user did
/// not write it down anywhere. So it names the entry, and it says what would have happened — otherwise
/// "resolves outside the folder" reads as a complaint about the folder the user did choose.
///
/// Like [`confinement_refusal`] it deliberately does not claim *where* the link goes. `confined_to` fails
/// closed, so this sentence is also reached when the OS would not say where the link resolves (`EACCES`,
/// `ELOOP`, a sharing violation) — and stating a destination the guard never established is the
/// confident-false-statement failure CPE-1687/1705/1710/1716/1750 have between them filed five tickets
/// about.
fn link_inflow_refusal(entry: &Path) -> String {
    format!(
        "refused: {entry:?} is a link that does not resolve inside the folder — copying it would follow \
         the link and bring content from outside the folder you confirmed into it"
    )
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

    // -----------------------------------------------------------------------------------------------
    // CPE-1756 — the walk under a confined `src`, which CPE-1750 deliberately did not close
    // -----------------------------------------------------------------------------------------------

    /// The bytes staged outside the confirmed folder. A distinctive literal so an assertion can say
    /// *which* outside content arrived, rather than "something did".
    const OUTSIDE_BYTES: &[u8] = b"CPE-1756 OUTSIDE-THE-FOLDER SECRET";

    /// Stage a **live file** link at `link` pointing at the existing file `victim`; `false` if this
    /// machine cannot.
    ///
    /// A live *file* link is the one construction this repo cannot fake — a junction is directory-only
    /// and a hard link answers `is_symlink() == false` (CPE-1716) — and it is the only shape that makes
    /// `fs::copy` pull a target's **bytes** in. So every caller pairs it with
    /// [`crate::fsutil::require_staged`], and a runner that is supposed to manage one goes red rather
    /// than covering nothing quietly (CPE-1717). Same body as `archive.rs`'s `stage_live_link`, which is
    /// private to that module's own test mod; a third copy in `fsutil` would be a `pub` staging helper
    /// added for one ticket, and [`crate::fsutil::make_dir_link`]'s doc records why only the
    /// cross-*crate* helpers earned that.
    fn stage_live_file_link(victim: &Path, link: &Path) -> bool {
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(victim, link).is_ok()
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(victim, link).is_ok()
        }
    }

    /// The first path under `dir` whose bytes are exactly `needle`, or `None`. Recurses with
    /// `symlink_metadata`, so it walks the **copy's own** entries and never wanders out of the tree it is
    /// inspecting. Returning the path (not a bool) is what lets the caller's assertion name where the
    /// outside content landed.
    fn tree_holding(dir: &Path, needle: &[u8]) -> Option<PathBuf> {
        for entry in fs::read_dir(dir).ok()?.flatten() {
            let p = entry.path();
            let Ok(md) = fs::symlink_metadata(&p) else { continue };
            if md.is_dir() {
                if let Some(hit) = tree_holding(&p, needle) {
                    return Some(hit);
                }
            } else if fs::read(&p).is_ok_and(|b| b == needle) {
                return Some(p);
            }
        }
        None
    }

    /// Every op in `results` was refused **by [`copy_recursive`]'s link-inflow guard**, not by the
    /// containment guard at [`apply_op`] and not by an incidental primitive error.
    ///
    /// The reason matters more here than almost anywhere: [`apply_op`] already refuses an op whose own
    /// `src` escapes, and `fs::copy` already fails on a dangling or directory link. Both would keep a
    /// "the op failed" assertion green with this guard deleted, which is precisely the shape CPE-1750
    /// round 2 had to go back and fix.
    fn assert_all_refused_for_link_inflow(results: &[OpResult], plan: &FileOpPlan) {
        assert_eq!(results.len(), plan.ops.len(), "one result per op: {results:?}");
        for (r, op) in results.iter().zip(&plan.ops) {
            assert!(!r.ok, "{op:?} must be refused, not reported successful: {r:?}");
            assert!(
                r.error.contains("is a link that does not resolve inside the folder"),
                "{op:?} must be refused BY THE LINK-INFLOW GUARD IN `copy_recursive`. The op's own `src` \
                 is confined, so `apply_op` has nothing to say; and `fs::copy`'s own \"cannot find the \
                 file\"/\"is a directory\" would keep a failure-only assertion green with the guard \
                 deleted. Got: {}",
                r.error
            );
        }
    }

    /// CPE-1756, the hole itself: `src` is confined, so [`apply_op`] passes it, and [`copy_recursive`]
    /// then descends into a child that is a **live file link out of the folder**. `fs::copy` follows the
    /// final component, so the outside file's bytes are written into the folder the human confirmed.
    ///
    /// Nothing lands *outside* — this is a read **inflow** — which is why CPE-1750's claim stayed true
    /// and this needed its own ticket. It is still content the user never chose, now sitting in a folder
    /// they may share, sync or hand on.
    ///
    /// Both levels the ticket asks for are here in one plan: a link that is a **direct child** of `src`,
    /// and one **three levels down**, because "the walk asks at the top" is a plausible half-fix.
    #[test]
    fn cpe_1756_copy_refuses_a_child_link_that_would_pull_outside_content_in() {
        let root = scratch("cpe1756-inflow");
        let outside = scratch("cpe1756-inflow-outside");
        let secret = outside.join("secret.txt");
        fs::write(&secret, OUTSIDE_BYTES).unwrap();

        let flat = root.join("flat"); // the link is a DIRECT child of the copied folder
        let nest = root.join("nest"); // …and here it is three levels down instead
        let deep = nest.join("a/b");
        fs::create_dir_all(&flat).unwrap();
        fs::create_dir_all(&deep).unwrap();
        fs::write(flat.join("mine.txt"), b"MINE").unwrap();
        fs::write(deep.join("mine.txt"), b"MINE").unwrap();

        let flat_link = flat.join("leak.txt");
        let deep_link = deep.join("leak.txt");
        let staged =
            stage_live_file_link(&secret, &flat_link) && stage_live_file_link(&secret, &deep_link);
        if !crate::fsutil::require_staged("live_file_symlink", true, staged) {
            crate::skip_notice!(
                "[CPE-1756] SKIPPED the copy-inflow leg: this machine could not create a live FILE \
                 symlink at {} (no SeCreateSymbolicLinkPrivilege; a junction is directory-only and \
                 cannot stage this). NOTHING on this run covered a Copilot recursive copy following a \
                 child link and pulling content from OUTSIDE the confirmed folder into it.",
                flat_link.display()
            );
            return;
        }
        let ctx = ctx_for(&root);
        let trash = FakeTrash::new(scratch("cpe1756-inflow-hold"));

        let plan = FileOpPlan {
            ops: vec![
                FileOp::Copy {
                    src: root_str(&flat),
                    dst: root_str(&root.join("copy-of-flat")),
                },
                FileOp::Copy {
                    src: root_str(&nest),
                    dst: root_str(&root.join("copy-of-nest")),
                },
            ],
        };
        let out = execute_with(&ctx, &trash, &root_str(&root), &plan);

        // EFFECT BEFORE THE UNWRAP. This family fails by *succeeding* — with the guard gone the copy
        // returns `Ok` and the op is reported green — so anything asserted after `unwrap()` is
        // unreachable on a build that has the bug.
        for (dst, where_) in [
            (root.join("copy-of-flat"), "a DIRECT child of"),
            (root.join("copy-of-nest"), "three levels below"),
        ] {
            let landed = tree_holding(&dst, OUTSIDE_BYTES);
            assert!(
                landed.is_none(),
                "the bytes of \"{}\" — a file OUTSIDE the confirmed folder \"{}\" — were copied INTO \
                 that folder at \"{}\". `copy_recursive` followed a link {} the copied folder, and \
                 `fs::copy` dereferences the final component. Nothing landed outside, so CPE-1750's \
                 guard is intact; this is the inflow it does not cover.",
                secret.display(),
                root.display(),
                landed.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
                where_
            );
        }

        let out = out.unwrap();
        assert_all_refused_for_link_inflow(&out.results, &plan);
        assert_eq!(
            fs::read(&secret).unwrap(),
            OUTSIDE_BYTES.to_vec(),
            "the outside file itself must be untouched — this guard is about reading it, not writing it"
        );
    }

    /// CPE-1756, the **dangling** child: `root/src/dangling -> <outside>/soon`, with `soon` removed.
    ///
    /// `fs::copy` fails `NotFound` on one, so the op ends up failed either way and an
    /// effect-only assertion proves nothing — which is exactly why
    /// [`assert_all_refused_for_link_inflow`] insists on the *reason*. What changes with the guard is
    /// that the user is told a link leads out of their folder instead of "the system cannot find the
    /// file specified", and that `confined_to`'s fail-closed handling of a dangling link is the thing
    /// deciding it rather than the platform's `fs::copy` behaviour — which differs between Windows and
    /// POSIX for reparse points, so "some platform happens to stop it" is not a guard.
    ///
    /// Staged with [`crate::fsutil::make_dir_link`] (junction fallback, no privilege needed), so this
    /// leg runs on every runner even when the live-file-link leg above must skip.
    #[test]
    fn cpe_1756_copy_refuses_a_child_link_that_dangles_out_of_the_folder() {
        let root = scratch("cpe1756-dangle");
        let outside = scratch("cpe1756-dangle-outside");
        let soon = outside.join("soon"); // exists only long enough to hang the link on
        fs::create_dir_all(&soon).unwrap();

        let src = root.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("mine.txt"), b"MINE").unwrap();

        let dangling = src.join("dangling");
        if !crate::fsutil::make_dir_link(&soon, &dangling) {
            crate::skip_notice!(
                "[CPE-1756] SKIPPED the dangling-child leg: this machine could not create a directory \
                 link at {} (no symlink privilege and no junction). NOTHING on this run covered a \
                 Copilot recursive copy meeting a child link that dangles OUT of the confirmed folder.",
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
        let trash = FakeTrash::new(scratch("cpe1756-dangle-hold"));

        let dst = root.join("copy-of-src");
        let plan = FileOpPlan {
            ops: vec![FileOp::Copy { src: root_str(&src), dst: root_str(&dst) }],
        };
        let out = execute_with(&ctx, &trash, &root_str(&root), &plan);

        // EFFECT BEFORE THE UNWRAP: nothing may have been materialised at the link's target, which is
        // where a `fs::copy` that dereferenced it would have reached.
        assert!(
            !soon.exists(),
            "\"{}\" was created outside the confirmed folder \"{}\" by way of the dangling child link \
             at \"{}\"",
            soon.display(),
            root.display(),
            dangling.display()
        );

        let out = out.unwrap();
        assert_all_refused_for_link_inflow(&out.results, &plan);
    }

    /// CPE-1756's discrimination + regression leg, in one test because they answer one question: is the
    /// guard *containment*, or did it just become "no links, no walking"?
    ///
    /// 1. An ordinary tree — real files, real subfolders, no links at all — still copies whole. This is
    ///    also the leg that would catch a guard implemented as a blanket refusal of anything it could
    ///    not cheaply prove.
    /// 2. A child link resolving back **inside** the confirmed folder is still copied. Without this,
    ///    a guard that refuses every link would look perfect — and its `fs::copy` dereferences the link,
    ///    which is the documented, deliberate flattening in [`copy_recursive`]'s "does NOT cover" note.
    #[test]
    fn cpe_1756_ordinary_copy_and_an_in_root_child_link_still_work() {
        let root = scratch("cpe1756-ordinary");
        let ctx = ctx_for(&root);
        let trash = FakeTrash::new(scratch("cpe1756-ordinary-hold"));

        let src = root.join("src");
        fs::create_dir_all(src.join("a/b")).unwrap();
        fs::write(src.join("top.txt"), b"TOP").unwrap();
        fs::write(src.join("a/mid.txt"), b"MID").unwrap();
        fs::write(src.join("a/b/leaf.txt"), b"LEAF").unwrap();

        let dst = root.join("copy-of-src");
        let plan = FileOpPlan {
            ops: vec![FileOp::Copy { src: root_str(&src), dst: root_str(&dst) }],
        };
        let out = execute_with(&ctx, &trash, &root_str(&root), &plan).unwrap();
        assert!(out.results[0].ok, "an ordinary link-free recursive copy must still run: {:?}", out.results[0]);
        for (rel, bytes) in [("top.txt", &b"TOP"[..]), ("a/mid.txt", &b"MID"[..]), ("a/b/leaf.txt", &b"LEAF"[..])] {
            assert_eq!(
                fs::read(dst.join(rel)).unwrap(),
                bytes.to_vec(),
                "\"{rel}\" is missing or wrong in the copy — the guard broke ordinary recursion"
            );
        }

        // …and the discrimination leg: same shape, but the child link stays inside the confirmed folder,
        // so containment has nothing to say about it and the copy runs.
        let inside_target = root.join("inside-target.txt");
        fs::write(&inside_target, b"INSIDE THE FOLDER").unwrap();
        let linked_src = root.join("linked-src");
        fs::create_dir_all(&linked_src).unwrap();
        let in_link = linked_src.join("inlink.txt");
        if !crate::fsutil::require_staged(
            "live_file_symlink",
            true,
            stage_live_file_link(&inside_target, &in_link),
        ) {
            crate::skip_notice!(
                "[CPE-1756] SKIPPED the in-root-child-link discrimination leg: this machine could not \
                 create a live FILE symlink at {}. NOTHING on this run checked that the copy guard is \
                 containment rather than a blanket ban on links — a guard that refuses everything looks \
                 perfect without this leg.",
                in_link.display()
            );
            return;
        }
        let linked_dst = root.join("copy-of-linked-src");
        let inside_plan = FileOpPlan {
            ops: vec![FileOp::Copy { src: root_str(&linked_src), dst: root_str(&linked_dst) }],
        };
        let inside = execute_with(&ctx, &trash, &root_str(&root), &inside_plan).unwrap();
        assert!(
            inside.results[0].ok,
            "a child link resolving back INSIDE the confirmed folder must not be refused as an inflow — \
             the guard is containment, not a ban on links: {:?}",
            inside.results[0]
        );
        assert_eq!(
            fs::read(linked_dst.join("inlink.txt")).unwrap(),
            b"INSIDE THE FOLDER".to_vec(),
            "and `fs::copy` dereferences it, so the copy holds the target's bytes — the documented, \
             deliberate flattening, pinned here so it cannot change unnoticed"
        );
    }

    /// CPE-1750, **attempt 2, blocker 1** — found by PR #916's reviewer and reproduced against both
    /// commits before it was fixed.
    ///
    /// `confined_to` answers `true` for the root itself, by design, and hands "is it the root?" back to
    /// the caller. Attempt 1 never asked, so `Delete { path: <root> }` walked past the guard and reached
    /// the [`TrashBin`] seam — `trash::delete` on the user's real filesystem in the shipped app — with
    /// the entire confirmed folder as its argument, and reported the op **successful**. The deleted
    /// `parent_confined` had refused this by accident, because the root's parent is outside the root by
    /// definition, so the swap made this operation strictly *more* permissive.
    ///
    /// `op_plan::validate` is no help: its `within_root` is a `>=`-length prefix test, true for equality.
    #[test]
    fn cpe_1750_execute_never_trashes_the_confirmed_folder_itself() {
        let root = scratch("cpe1750-root-delete");
        fs::write(root.join("keep.txt"), b"the user's files").unwrap();
        let ctx = ctx_for(&root);
        let trash = FakeTrash::new(scratch("cpe1750-root-delete-hold"));

        let plan = FileOpPlan { ops: vec![FileOp::Delete { path: root_str(&root) }] };
        let out = execute_with(&ctx, &trash, &root_str(&root), &plan);

        // EFFECT BEFORE UNWRAP — the call still returns `Ok` here; this op fails by *succeeding*, so an
        // assertion below an `unwrap()` would never run on the build that has the bug.
        let trashed = trash.trashed.lock().unwrap().clone();
        assert!(
            trashed.is_empty(),
            "the trash seam — `trash::delete` on the user's real filesystem in the shipped app — was \
             handed {trashed:?}, which IS the confirmed folder \"{}\" itself. The whole folder the human \
             approved went to the Recycle Bin, reported as a successful operation",
            root.display()
        );
        assert!(
            root.join("keep.txt").exists(),
            "\"{}\" is gone — the confirmed folder \"{}\" was removed out from under the user's files",
            root.join("keep.txt").display(),
            root.display()
        );

        let out = out.unwrap();
        assert!(!out.results[0].ok, "the op must be refused, not reported successful: {:?}", out.results[0]);
        assert!(
            out.results[0].error.contains("IS the folder you confirmed"),
            "and it must be refused BY THE ROOT-IDENTITY GUARD, naming the real problem — the path is not \
             outside the folder, it *is* the folder: {}",
            out.results[0].error
        );
    }

    /// CPE-1750, **attempt 2, blocker 2** — the same reviewer, the same root input, a different route out.
    ///
    /// `Rename`'s destination is `path`'s parent joined with `new_name`, and attempt 1 never guarded it,
    /// reasoning that confining `path` transitively confines its parent. That is false at `path == root`:
    /// the destination lands in the root's **parent**, and `rename_into_slot` guards only *what is
    /// sitting in* the slot, never *where the slot is*, so an empty name outside the folder sailed
    /// through and the confirmed folder was relocated to `<parent-of-root>/away`.
    ///
    /// Two guards can now stop this, which is deliberate defence in depth — and it is why the assertion
    /// below pins the **reason**. Containment runs as a whole pass before identity, so the destination
    /// guard is what answers here; drop the `dst` field from `op_path_fields` and identity still refuses
    /// the op, but with the other message, and this test reds. Without that, the destination guard could
    /// be deleted outright with every test still green.
    #[test]
    fn cpe_1750_execute_never_renames_the_confirmed_folder_out_of_itself() {
        let root = scratch("cpe1750-root-rename");
        fs::write(root.join("keep.txt"), b"the user's files").unwrap();
        let away = root.parent().expect("scratch dirs have a parent").join("cpe1750-relocated-root");
        let _ = fs::remove_dir_all(&away); // a previous run must not decide this one
        let ctx = ctx_for(&root);
        let trash = FakeTrash::new(scratch("cpe1750-root-rename-hold"));

        let plan = FileOpPlan {
            ops: vec![FileOp::Rename {
                path: root_str(&root),
                new_name: "cpe1750-relocated-root".into(),
            }],
        };
        let out = execute_with(&ctx, &trash, &root_str(&root), &plan);

        // EFFECT BEFORE UNWRAP — same reason as above: this one fails by succeeding.
        assert!(
            !away.exists(),
            "the confirmed folder \"{}\" was RELOCATED to \"{}\" — outside the folder, and outside what \
             the pre-execute checkpoint took, so the app's own Undo cannot bring it back",
            root.display(),
            away.display()
        );
        assert!(
            root.join("keep.txt").exists(),
            "\"{}\" is gone — the confirmed folder was renamed out from under the user's files",
            root.join("keep.txt").display()
        );

        let out = out.unwrap();
        assert!(!out.results[0].ok, "the op must be refused, not reported successful: {:?}", out.results[0]);
        assert!(
            out.results[0].error.contains("resolves outside the folder"),
            "and it must be refused BY THE DESTINATION-CONFINEMENT GUARD — if identity catches it first, \
             that guard is unreachable and deletable with every test still green: {}",
            out.results[0].error
        );
    }

    /// The other half of blocker 1: the root-identity refusal must not become a blanket refusal of
    /// anything *near* the root. Every ordinary op names something **inside** the folder, and the
    /// pre-existing suite would notice a regression there.
    ///
    /// The input a sloppy `starts_with`/string comparison gets wrong — a sibling whose name merely
    /// *starts with* the root's name — is deliberately **not** here: it lies outside the root, so
    /// `op_plan::validate` rejects it before this guard is ever asked. It is covered where it can
    /// actually occur, in `fsutil`'s own `confined_to` probe. (An earlier version of this comment
    /// claimed the case as covered by this test; it never was. Reported by the CPE-1750 round-2 review.)
    #[test]
    fn cpe_1750_root_identity_refusal_does_not_catch_ordinary_in_root_work() {
        let root = scratch("cpe1750-discriminate");
        fs::write(root.join("a.txt"), b"payload").unwrap();
        let ctx = ctx_for(&root);
        let trash = FakeTrash::new(scratch("cpe1750-discriminate-hold"));

        let plan = FileOpPlan {
            ops: vec![
                // A child of the root, and a not-yet-existing one two levels down — the ordinary shapes.
                FileOp::Mkdir { path: root_str(&root.join("Archive/2026")) },
                FileOp::Rename { path: root_str(&root.join("a.txt")), new_name: "b.txt".into() },
            ],
        };
        let out = execute_with(&ctx, &trash, &root_str(&root), &plan).unwrap();

        assert!(
            out.results.iter().all(|r| r.ok),
            "ordinary in-root work must still run — the guard is 'not the folder itself', not 'nothing \
             near the folder': {:?}",
            out.results
        );
        assert!(root.join("Archive/2026").is_dir());
        assert_eq!(fs::read(root.join("b.txt")).unwrap(), b"payload".to_vec());
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
