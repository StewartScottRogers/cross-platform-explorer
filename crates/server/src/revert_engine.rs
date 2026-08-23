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
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::model::HeldBackOutcome;
#[cfg(test)]
use crate::model::OpOutcome;
use crate::restore_plan::{RestoreAction, RestoreOp, Snapshot};

/// Outcome of [`execute_restore`]: how many ops applied cleanly, which genuinely could not be applied,
/// and — kept structurally apart since CPE-1845 — which deletes were **deliberately held back**.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RestoreReport {
    /// Number of actions that applied successfully.
    pub applied: usize,
    /// Actions that were **attempted and could not be applied**: `(path, reason)`. Never fatal — the
    /// rest of the plan still runs. These are genuine failures; a deliberate hold-back is **not** here,
    /// it is in [`RestoreReport::held_back`].
    pub skipped: Vec<(String, String)>,
    /// Deletes this engine chose not to perform, and why — **one** explanation for the whole group
    /// (CPE-1845). `None` when nothing was held back.
    pub held_back: Option<HeldBack>,
}

/// One stand-down, covering every delete it held back (CPE-1845).
///
/// **Why this is a group and not a per-path reason.** The pre-CPE-1845 engine pushed the same paragraph
/// onto every held-back delete. CPE-1847's audit measured it: 500 held-back deletes emitted 500 copies of
/// a ~370-character paragraph — roughly **185 KB** in one `RevertOutcome` — and CPE-1823's review measured
/// the shape that produces it (`applied=1 skipped=201`, 200 of those 201 carrying an identical
/// paragraph). The reason belongs to the *decision*, which happened once, not to each path it covers.
///
/// [`HeldBack::detail`] carries only what genuinely differs per path, and is usually empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldBack {
    /// Which state this is: [`HeldBackOutcome::SkippedByPlan`] (retryable — fix the blocker, run
    /// again) or [`HeldBackOutcome::HeldBackByCheckpoint`] (**not** retryable on this platform). The
    /// narrow enum, so a hold-back carrying `Applied` or `Failed` cannot be built at all.
    pub outcome: HeldBackOutcome,
    /// The single explanation, stated **once**. Never copied per path.
    pub reason: String,
    /// What the user can actually do — a real next step, or an explicit statement that there is none on
    /// this platform. For [`OpOutcome::HeldBackByCheckpoint`] this must **not** say "re-run": re-running
    /// here cannot change the verdict (a capture holding a name this platform cannot write never will).
    pub next_step: String,
    /// The held-back paths, in the order the plan reached them, each with the detail specific to that
    /// path (empty when the group [`reason`](HeldBack::reason) says everything).
    pub paths: Vec<(String, String)>,
}

impl HeldBack {
    fn new(outcome: HeldBackOutcome, reason: String, next_step: &str) -> Self {
        Self { outcome, reason, next_step: next_step.to_string(), paths: Vec::new() }
    }
}

/// One entry's refusal, plus **whether running the revert again on this machine could come out
/// differently** (CPE-1845, review round 2).
///
/// This exists because the first cut of CPE-1845 inferred that answer from *which branch* produced the
/// hold-back, and got it wrong. `RestoreReport::skipped` is fed by every write refusal, and only some of
/// them are transient: a locked file and a missing stored blob clear if you fix them, while
/// `escapes dest_root`, a Win32-unstable name, a containment failure and the Create-premise refusal are
/// judgements about the checkpoint's own stored spelling that this platform reaches identically every
/// time. The branch nevertheless labelled the whole group retryable and told the user to run the revert
/// again — reproduced through the production `execute_restore` with a plan of `Create "a/../evil.txt"`
/// plus `Delete "added.txt"`:
///
/// ```text
/// PROBE skipped=[("a/../evil.txt", "escapes dest_root: `.`/`..` segment")]
/// PROBE outcome=SkippedByPlan retryable=true
///       next_step=This one is temporary: … and run the revert again …
/// ```
///
/// That is this ticket's own acceptance criterion — *it must not say "re-run"* — surviving on a narrower
/// set of shapes inside the change filed to remove it. So the answer is now **carried from the point of
/// refusal**, where it is known, instead of inferred later from prose or from which branch fired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refused {
    /// The human reason, unchanged — this is what lands in [`RestoreReport::skipped`].
    reason: String,
    /// `true` when this refusal will be reached again, identically, on every re-run here.
    permanent: bool,
}

impl Refused {
    /// An attempt that was made and failed: a locked file, a missing stored blob, a permission error, a
    /// directory that could not be created. Fix the cause and the same run succeeds.
    fn transient(reason: impl Into<String>) -> Self {
        Self { reason: reason.into(), permanent: false }
    }
    /// A judgement about the checkpoint's stored content — a path that escapes the root, a name this
    /// platform cannot address stably, a spelling that resolves onto a file the plan did not account
    /// for. Nothing about re-running changes a stored spelling.
    fn permanent(reason: impl Into<String>) -> Self {
        Self { reason: reason.into(), permanent: true }
    }
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

    // Whether ANY refusal so far is one this platform will reach again identically. Tracked here rather
    // than re-derived from the reason strings later — deriving it from prose is exactly the coupling
    // this ticket removes, and deriving it from "which branch fired" is what got it wrong first time.
    let mut any_permanent_refusal = false;
    for action in &writes {
        match apply_write(action, dest_root_path, &blobs_dir, checkpoint) {
            Ok(()) => report.applied += 1,
            Err(refused) => {
                any_permanent_refusal |= refused.permanent;
                report.skipped.push((action.path.clone(), refused.reason));
            }
        }
    }

    // **A delete is only safe once the checkpoint state has actually been established (CPE-1823 round 3,
    // keyed on the CHECKPOINT rather than on the plan in round 4).**
    //
    // Refusing the write alone did not save the user's file. The reported shape: a planted entry
    // `a.txt ` against a live `a.txt`. Refusing the Create is right — but `plan_restore` had also
    // emitted `Delete a.txt`, because on its reading `a.txt` is a file added since the checkpoint. So
    // the guard skipped the write, the delete ran, and the file was gone anyway; the report just said
    // `applied: 1, skipped: 1` instead of `applied: 2, skipped: []`. A visible refusal next to the same
    // destroyed file is not a fix.
    //
    // The rule is deliberately about the *class*, not that one alias. A delete's whole justification is
    // "this path is not in the checkpoint" — a judgement that depends on having read the checkpoint
    // correctly and applied it. Any skipped write means we did not, so no delete's premise can be
    // trusted, and the destructive half of the revert stands down. That also covers aliasing shapes this
    // engine cannot see coming (the case-insensitive `A.txt`/`a.txt` collapse recorded on
    // `win32_addresses_a_different_path`, for one).
    //
    // Conservative in the safe direction and reported per path, never silent: the user is told exactly
    // which cleanups were held back and why. Whether re-running performs them depends on WHICH hold-back
    // it is, and since CPE-1845 the two are separate `OpOutcome` variants carrying their own next step —
    // `SkippedByPlan` says "fix it and run again", `HeldBackByCheckpoint` says re-running cannot help.
    //
    // Three consequences, recorded rather than fixed, so none of them reads later as an engine bug:
    //
    // - **It is attacker-triggerable as denial-of-revert, and round 4 made it CHEAPER, not dearer.** The
    //   round-3 shape needed a write that actually failed. This one needs **one checkpoint key with a
    //   trailing space** — no blob, no write attempted, no I/O at all, judged textually — and every
    //   delete in every revert of that checkpoint is held back from then on. Accepted anyway: the
    //   direction is fail-safe, the reason names the offending key so the cause is discoverable rather
    //   than mysterious, and the restorable half still applies. Recorded at its real cost, not its old
    //   one.
    // - **It is blunt in a way users will feel, and on this branch the hold is PERMANENT, not
    //   transient.** Measured at scale: a checkpoint with one unrestorable key, 200 files added since,
    //   and one restorable entry gives `applied: 1, skipped: 201` with all 200 survivors intact and the
    //   restorable half correct. Re-running changes nothing, because nothing about the checkpoint's
    //   spelling will change on this platform — which is why this branch is
    //   [`OpOutcome::HeldBackByCheckpoint`] and offers a next step that is **not** "re-run".
    //   **CPE-1845 closed the reporting gap this used to carry**: a hold-back and a failure are now
    //   separate `OpOutcome` variants on a typed field, so a consumer never string-matches
    //   `"not deleted:"` to tell them apart, and the shared paragraph is stated once in
    //   [`HeldBack::reason`] instead of copied onto all 200 paths.
    //
    //   **Correction, review round 2 — the `report.skipped` branch below is NOT "the transient one",**
    //   and an earlier revision of this very comment said it was. That branch is fed by *every* write
    //   refusal, and only an attempt that was made and failed (a locked file, a pruned blob) is
    //   transient; `escapes dest_root`, the Win32-unstable name rule, the containment failure and the
    //   Create-premise refusal are all verdicts on the checkpoint's stored spelling and recur forever.
    //   It therefore splits on [`Refused::permanent`], carried from the point of refusal. The
    //   500-locked-file example still holds — for the transient half of it.
    // - **Finer granularity is not available *from the spelling*, which is why round 5 stopped asking
    //   the spelling.** Pairing each delete with the write that would have covered it is precisely what
    //   the aliasing case makes invisible — `a.txt ` and `a.txt` look like different paths, which is the
    //   bug. The rule that *can* tell them apart is the per-delete resolution check further down: it
    //   asks the filesystem which file a delete addresses and holds back only that one. This blanket
    //   rule stays as the coarse backstop for shapes resolution cannot answer — a device name resolves
    //   to `\\?\NUL`, which is not a file any checkpoint entry can collide with — so the two are
    //   complementary, not redundant.
    //
    // **The widest destructive shape a planted manifest had was an emptied `"files": {}` (CPE-1847),
    // and it is closed just below by the first `hold` branch.** It turned a revert into "delete every
    // file", with no writes to stand down and no error. Both routes measured on this branch before the
    // fix, reproducing CPE-1823's round-5 figures exactly:
    //
    // ```text
    // C1 CMD revert[empty manifest]:     applied=5 skipped=0   survivors = []
    // C2 CMD revert_one[empty manifest]: applied=1 skipped=0   survivors = [f1, f2, f4, f5]
    // ```
    //
    // C2 is the row that settled the ranking, and it is why this was wider than either checker first
    // said. The same emptied manifest destroys files **one at a time** through `checkpoint_revert_one`,
    // behind a per-file confirm that says nothing about a mass delete and never consults
    // `checkpoint_preview_revert` at all. An earlier draft of this comment ranked it narrower *because*
    // of that preview — measured false, and stated here rather than quietly dropped, since this passage
    // exists to correct false claims and had become one.
    //
    // The **resolution collision** below — one planted key, one live file, reachable through
    // cherry-revert, destroying a file the attacker names — was the widest shape while it was open, and
    // rounds 3 and 4 both mis-ranked it (round 4's comment here said the empty-`files` shape was "not
    // this one", and that the alias was out of scope; both wrong). It is closed in this function as of
    // round 5. What CPE-1847 leaves standing as the widest **remaining** shape is a `files` map emptied
    // only *partially* — see that first `hold` branch's doc, which records it measured.
    //
    // The condition is a property of the **checkpoint**, not of this plan. Round 3 keyed it on
    // `report.skipped`, which reads "some write failed" — and a *cherry-revert* plan contains no write
    // at all, so nothing could be skipped and the stand-down never armed:
    //
    // ```text
    // REVERT_ONE[trailing space] plan   = [("a.txt", "Delete")]
    //                            report = RestoreReport { applied: 1, skipped: [] }; a.txt = NotFound
    // ```
    //
    // The user cherry-reverts `a.txt`; `revert_one` asks `checkpoint.get("a.txt")`, gets `None` because
    // the checkpoint spells it `a.txt `, and plans a lone Delete. On Windows those are the same file, so
    // the checkpoint *does* hold it — and their only copy is deleted, reported as complete success.
    //
    // **This needs no attacker.** A macOS or Linux capture holding `a.txt ` — a name those systems store
    // happily — does it to a Windows user with an ordinary cross-platform checkpoint. That is what makes
    // it a data-loss bug rather than only a planted-manifest hazard.
    //
    // Asking `safe_segments` about every checkpoint key covers both shapes with one rule, and makes the
    // sentence above literally true: the justification for a delete is "this path is not in the
    // checkpoint", which presupposes having read the checkpoint correctly. If any key is one this
    // platform cannot restore, we have not, so no delete's premise holds. Textual and I/O-free, so it
    // costs nothing on the common path.
    //
    // **Deliberately NOT fixed by refusing the checkpoint upstream** (the other candidate): rejecting a
    // manifest containing such a key at `manifest_snapshot` would close the same class, but it would
    // make a *legitimate* Linux checkpoint containing `a.txt ` entirely unusable on Windows — no
    // preview, no diff, no partial revert — trading a data-loss bug for a total-refusal bug against a
    // real user with a real capture. This keeps the restorable half working.
    let unrestorable: Vec<&String> =
        checkpoint.keys().filter(|key| safe_segments(key).is_err()).collect();
    let hold = if checkpoint.is_empty() {
        // **CPE-1847 — a checkpoint that records no files cannot justify deleting any.**
        //
        // This is the shape every rule above and below is structurally blind to, because there is
        // nothing for them to look at. `plan_restore` reads an empty `checkpoint` as "the tree was empty
        // then", so every live file becomes a `Delete`; there is no write to fail, no key to judge, and
        // no entry to resolve. Measured through the registered commands on this branch before this
        // branch existed — the ticket's figures, reproduced exactly:
        //
        // ```text
        // CMD revert[empty manifest]:     applied=5 skipped=0   survivors = []
        // CMD revert_one[empty manifest]: applied=1 skipped=0   survivors = [f1, f2, f4, f5]
        // ```
        //
        // Complete success reported, whole tree gone, from the cheapest tamper there is: **deleting
        // text**. Every other manifest attack CPE-1823 closed needed a crafted key that survived a
        // guard; this one needs three characters (`{}`) and reaches through `checkpoint_revert_one`,
        // where nothing consults `checkpoint_preview_revert` at all.
        //
        // # Why the fix is a stand-down and not a refusal
        //
        // **An empty checkpoint is legitimately representable** — capturing an empty directory produces
        // exactly this manifest, measured: `new_blobs: 0, added_bytes: 0, files: {}`. So refusing to
        // *load* it, or erroring here, would refuse a real capture. It is also byte-for-byte
        // indistinguishable from an emptied map: there is no evidence anywhere on disk that separates a
        // genuine empty capture from one whose entries were removed, which is why this ticket was a
        // judgement call rather than a detection problem. (`file_count` in
        // [`crate::snapshot_capture`]'s persisted manifest raises the cost of producing the emptied
        // shape, but it is a field in the same hand-editable file, so it cannot be the thing that makes
        // the harm impossible. It is deliberately **not** consulted here, and asserting `file_count: 0`
        // unlocks nothing.)
        //
        // So the rule is about what a zero-entry checkpoint can *authorise*, and it costs the legitimate
        // reading nothing the checkpoint could have given back:
        //
        // - A delete's whole justification — this function's standing premise since CPE-1823 round 3 —
        //   is "this path is not in the checkpoint". A checkpoint holding nothing says that of every
        //   path in the universe. It is an absence, and an absence is unfalsifiable: a removed entry and
        //   an entry that was never written are the same bytes.
        // - **A zero-entry checkpoint has no constructive half at all.** It can restore nothing, so
        //   every delete it authorises destroys content it cannot give back, and holding the destructive
        //   half back forfeits no restorable state. That asymmetry is what makes standing down
        //   proportionate here and not merely cautious.
        //
        // # What this costs, measured, and why it is the right trade anyway
        //
        // The one legitimate flow it changes: capture an empty folder, let something fill it, revert to
        // clean it out. Measured before this branch — `applied: 3`, tree emptied. Now those deletes are
        // **held back and named**, one reason per path with the count, on the same loud channel as every
        // other hold-back; the user deletes them themselves. That is a lost convenience against total,
        // unrecoverable, silently-reported loss of an entire tree — and the checkpoint was never
        // restoring anything in that flow, only authorising deletion.
        //
        // Not changed, and pinned by test: a genuine empty capture still loads, still previews, still
        // diffs, and reverting an **unchanged** empty tree still succeeds — the plan is empty, so
        // nothing is held back and `applied: 0` with no error. The round trip the ticket names as the
        // constraint that makes a naive refusal wrong is intact.
        //
        // # The residual, measured rather than assumed
        //
        // A **partially** emptied map evades this branch entirely and is strictly wider: removing 4 of 5
        // entries measured `applied: 4, survivors: ["f1.txt"]`. No rule in this engine can close it —
        // the surviving entry makes the checkpoint look ordinary, and the plan it produces is exactly
        // what a real revert of a real checkpoint looks like. It is *detected* one layer up, at the only
        // place the tamper is visible: `load_manifest`'s `file_count` cross-check, which refuses a
        // `files` map whose size disagrees with the count the capture wrote.
        //
        // **That check raises no cost, and an earlier draft of this comment claimed it did.** The claim
        // was "delete text becomes delete text and rewrite a number". It is false, because the field is
        // `#[serde(default)] Option<usize>` and the cross-check is gated on `Some` — manifests written
        // before the field existed must keep loading, so an absent count is exempt by design. The
        // attacker's cheapest partial tamper is therefore **delete text and delete more text**: remove
        // entries from `files`, remove the `"file_count"` line, and the check never runs. Measured
        // through the registered commands with no number rewritten anywhere:
        //
        // ```text
        // 4 of 5 entries removed + "file_count" key deleted, each leg on a FRESH five-file tree
        //   checkpoint_revert_one(f3) -> Ok(RevertOutcome { applied: 1, skipped: [] })  survivors f1,f2,f4,f5
        //   checkpoint_revert         -> Ok(RevertOutcome { applied: 4, skipped: [] })  survivors ["f1.txt"]
        // ```
        //
        // (The whole-tree figure is **4**, not the 3 first reported in review: a 3 is what a shared
        // fixture yields when the cherry-revert leg has already taken `f3` out. Re-measured on a fresh
        // tree per leg, because this record has already paid once for repeating a number instead of
        // running it.)
        //
        // Three bypasses, none needing a number rewritten: **delete** the field, **null** it
        // (`"file_count": null` is `None` for an `Option`, measured `applied: 4`), or **replace** entries
        // rather than removing them so the count stays honest — removing `f2..f5` and adding `z1..z4`
        // pointing at `f1`'s blob, count untouched, measured `applied: 8, skipped: []` with four user
        // files destroyed and four attacker-named ones created. The field is also size-shaped, not
        // content-shaped: substituting one entry's `hash` for another's is count-neutral and gave
        // `applied: 1` with `f1.txt`'s content replaced by `f2`'s.
        //
        // So `file_count` stops only an attacker who does not know the field exists, and the
        // partial-tamper residual is **cheaper than the first version of this record said**. Stated here
        // rather than softened, on this ticket's own standard — see `snapshot_prune`'s
        // `cpe_1847_retention_prunes_by_where_a_manifest_is_not_by_what_it_calls_itself`, where a false
        // claim of mine was corrected for exactly this reason: a false claim in a security record is
        // worse than an honest smaller one.
        //
        // **The security posture is unchanged by that correction, and that was measured too, not
        // assumed.** The Critical shape this ticket exists to close — a zero-entry checkpoint — is
        // closed by the branch above, which does not consult `file_count` at all. With the map emptied
        // *and* the `"file_count"` line deleted, both routes still stand down completely:
        //
        // ```text
        // files: {} + "file_count" key deleted
        //   checkpoint_revert_one(f3) -> applied: 0, skipped: 1   all five survive
        //   checkpoint_revert         -> applied: 0, skipped: 5   all five survive
        // ```
        //
        // Emptiness is read from the map itself, so nothing the attacker does to the count reaches it.
        // What is left standing is the partial tamper, and no rule available here closes it — a keyed
        // signature would, and see the field's own doc for why none of the repo's existing keys is that
        // key.
        //
        // # Reporting (rewritten by CPE-1845)
        //
        // Was: this paragraph copied onto every held-back delete, on the `"not deleted:"` prose channel,
        // because that was the only channel there was. Now it is stated **once** in [`HeldBack::reason`]
        // with [`OpOutcome::HeldBackByCheckpoint`] as the structural flag, so a consumer branches on a
        // field and 500 deletes cost one paragraph rather than 500 copies of it (~185 KB, CPE-1847).
        //
        // Not retryable: the emptiness is a property of the stored checkpoint, and re-running the revert
        // on this machine reads the same bytes and reaches the same verdict. So the next step is a real
        // one — do it yourself — not "try again".
        Some(HeldBack::new(
            HeldBackOutcome::HeldBackByCheckpoint,
            format!(
                "This checkpoint records no files at all, so it cannot say that anything is \"not in \
                 the checkpoint\" — and it holds nothing to restore in exchange. An emptied `files` map \
                 and a genuine capture of an empty folder are the same bytes on disk, so this revert \
                 would have deleted {} file{} and restored none.",
                deletes.len(),
                if deletes.len() == 1 { "" } else { "s" }
            ),
            "Re-running will not change this — the checkpoint is empty on disk and will read the same \
             way every time. Delete these files yourself if that is what you meant, or pick a \
             checkpoint that actually holds the content you want back.",
        ))
    } else if !unrestorable.is_empty() {
        let named: Vec<String> =
            unrestorable.iter().take(NAMED_CAUSES).map(|k| format!("{k:?}")).collect();
        let more = unrestorable.len().saturating_sub(named.len());
        // **The branch CPE-1845 exists for.** This hold is PERMANENT on this platform: the offending
        // entries are spelled in a way this filesystem cannot write, and no amount of re-running changes
        // a stored name. Telling the user to "re-run after fixing" here — which is what the one shared
        // wording used to do — points them at something that cannot succeed.
        Some(HeldBack::new(
            HeldBackOutcome::HeldBackByCheckpoint,
            format!(
                "{} of this checkpoint's entries cannot be restored on this computer ({}{}), so \"this \
                 file is not in the checkpoint\" cannot be trusted — deleting it may destroy a file the \
                 checkpoint does hold, under a name spelled differently here.",
                unrestorable.len(),
                named.join(", "),
                if more > 0 { format!(", and {more} more") } else { String::new() }
            ),
            &format!(
                "There is no fix for this on this computer: those names are stored in the checkpoint \
                 and this filesystem cannot write them, so re-running the revert will hold the same \
                 files back again. {} Delete these files yourself if you want them gone, or finish \
                 the revert on the system the checkpoint was captured on.",
                // **The completeness claim is CONDITIONAL, and that is not cosmetic** (CPE-1845 UAT):
                // it is the PREMISE of the sentence after it. Both halves are reachable in one run — an
                // unrestorable-name checkpoint AND a locked file or a missing blob, which is what an
                // ordinary cross-platform backup restored on a machine with a file open looks like. The
                // unconditional wording printed "Everything restorable has already been restored" three
                // lines above two entries that had just failed to restore, and a user who believes it
                // deletes the leftovers on a restore that did not finish.
                if report.skipped.is_empty() {
                    "Everything restorable has already been restored."
                } else {
                    "The restorable half is restored only where it could be: check the failures listed \
                     above first."
                }
            ),
        ))
    } else if !report.skipped.is_empty() {
        // **The branch that is NOT one branch** (CPE-1845 review round 2). Its first cut labelled every
        // hold-back derived from a non-empty `report.skipped` retryable and told the user to run the
        // revert again. But `report.skipped` is fed by every write refusal, and most of them recur
        // forever: `escapes dest_root` in its four forms, the Win32-unstable name rule, the resolved-
        // containment failure, and the Create-premise refusal are all verdicts on the checkpoint's own
        // stored spelling. Only an attempt that was made and failed — a locked file, a pruned blob — is
        // transient. So the split is on `Refused::permanent`, carried from the point of refusal, and
        // NOT on which branch fired or on what the reason says.
        let held = report.skipped.len();
        let named: Vec<&str> =
            report.skipped.iter().take(NAMED_CAUSES).map(|(path, _)| path.as_str()).collect();
        let more = held.saturating_sub(named.len());
        let listed = format!(
            "({}{})",
            named.join(", "),
            if more > 0 { format!(", and {more} more") } else { String::new() }
        );
        let entries = if held == 1 { "entry" } else { "entries" };
        if any_permanent_refusal {
            // At least one refusal will be reached again identically, so re-running cannot clear the
            // hold even if the user fixes everything that IS fixable.
            Some(HeldBack::new(
                HeldBackOutcome::HeldBackByCheckpoint,
                format!(
                    "{held} checkpoint {entries} could not be restored {listed}, so \"this file \
                     is not in the checkpoint\" cannot be trusted — and at least one of those \
                     refusals is about the checkpoint itself rather than about this attempt."
                ),
                "Re-running will not clear this on its own: at least one of the entries above is \
                 refused for what it IS, not for what happened this time, and this computer will \
                 refuse it the same way every run. Fix anything transient listed above if you can, \
                 then delete these files yourself if you want them gone — or finish the revert on the \
                 system the checkpoint was captured on.",
            ))
        } else {
            // Every refusal was an attempt that failed. This is the one branch entitled to say "again".
            Some(HeldBack::new(
                HeldBackOutcome::SkippedByPlan,
                format!(
                    "{held} checkpoint {entries} could not be restored this time {listed}, so \"this \
                     file is not in the checkpoint\" cannot be trusted yet."
                ),
                "This one is temporary: close whatever is holding those files (or restore the missing \
                 stored content) and run the revert again — the held-back cleanups will then apply.",
            ))
        }
    } else {
        None
    };

    match hold {
        None => {
            // **CPE-1823 round 5 — ask the premise of the FILE, not of the spelling.**
            //
            // Every rule above this line keys on how an entry is *spelled*, and the hazard is not a
            // spelling. `A.txt` and `a.txt` both pass [`safe_segments`] — neither is a device name,
            // neither ends in a dot or space — so the blanket stand-down's filter is empty and nothing
            // arms, while on a case-folding volume the two are one file. Measured through the registered
            // commands before this check existed:
            //
            // ```text
            // CMD revert[case-alias]     -> applied=2 skipped=0; a.txt = Err(NotFound)
            // CMD revert_one[case-alias] -> applied=1 skipped=0; a.txt = Err(NotFound)
            // ```
            //
            // Byte-for-byte the round-3 harm with `A.txt` substituted for `a.txt `. **No name-based rule
            // can see it**: both spellings are legal on every platform, so refusing either would break a
            // legitimate capture on the platform it came from — the objection that (correctly) killed the
            // upstream name-refusal in round 4. The problem is not the name.
            //
            // So this asks [`crate::fsutil::confined_to`]'s own principle, applied one level up: *assert
            // on the resolved path, never on the spelling that produced it.* A delete's whole
            // justification is "this path is not in the checkpoint". Asked of the spelling that can be
            // true while the *file* it addresses is one the checkpoint holds under a different spelling —
            // and then the revert destroys the very content it exists to protect. Resolve both sides and
            // the question answers itself.
            //
            // **What this subsumes without enumerating any of it:** trailing space, trailing dot, case
            // folding, 8.3 short names, Unicode-folding volumes, and — the leg that is nothing to do with
            // Windows — a directory link inside the tree giving one file two legal spellings on Linux and
            // macOS too (`sub/f.txt` and `alias/f.txt`, both of which `confined_to` admits, correctly,
            // because both resolve inside the tree). Whatever produces the next one is covered as well,
            // because the check never looks at how the path is written.
            //
            // **It fixes cherry-revert for free**, which is the shape round 4 had to key on the whole
            // checkpoint to reach: the collision is visible in `checkpoint` versus this one delete, with
            // no write anywhere in the plan for a plan-outcome rule to notice.
            //
            // **And it refuses nothing by name**, so a legitimate Linux capture stays as usable here as
            // it was: only a delete whose resolved target *is* a checkpoint entry is held back, and only
            // that one.
            //
            // Cost, measured and accepted rather than optimised: one `canonicalize` per checkpoint key
            // plus one per delete, computed only when the plan actually contains a delete. On the
            // measured numbers the textual pass over 20,000 keys is ~32 ms in a debug build and is
            // dwarfed by these walks — so the honest statement is that a large revert pays a resolution
            // walk per entry, on a destructive operation, on local disk. Unmeasured against a network
            // share, same caveat as [`safe_target`]'s.
            let checkpoint_lands_on: HashMap<PathBuf, &String> = if deletes.is_empty() {
                HashMap::new()
            } else {
                checkpoint.keys().filter_map(|key| landing(dest_root_path, key).map(|at| (at, key))).collect()
            };
            for action in &deletes {
                let collides = landing(dest_root_path, &action.path)
                    .and_then(|at| checkpoint_lands_on.get(&at).copied())
                    .filter(|key| key.as_str() != action.path.as_str());
                if let Some(key) = collides {
                    // A hold-back, not a failure, and **not** retryable: the two spellings resolve to
                    // one file on this volume and will do so on every re-run. The group statement is
                    // shared; the checkpoint entry that each path collides with is the one thing that
                    // genuinely differs per path, so that — and only that — goes in the detail.
                    let group = report.held_back.get_or_insert_with(|| {
                        HeldBack::new(
                            HeldBackOutcome::HeldBackByCheckpoint,
                            "These paths resolve to the same files as entries the checkpoint already \
                             holds, spelled differently. \"This file is not in the checkpoint\" is true \
                             of the spelling and false of the file, so deleting them would destroy \
                             content the checkpoint is there to protect."
                                .to_string(),
                            "Nothing needs doing and re-running will not change it: these files ARE \
                             the checkpoint's own content, reached under another spelling on this \
                             volume, so they are already in the state the revert was asking for.",
                        )
                    });
                    group
                        .paths
                        .push((action.path.clone(), format!("same file as checkpoint entry {key:?}")));
                    continue;
                }
                match apply_delete(action, dest_root_path) {
                    Ok(()) => report.applied += 1,
                    Err(refused) => report.skipped.push((action.path.clone(), refused.reason)),
                }
            }
        }
        Some(mut group) => {
            // The 200-identical-paragraphs case (CPE-1823's measurement) collapses HERE: the paragraph
            // lives once on `group.reason`; each path contributes only its name.
            for action in &deletes {
                group.paths.push((action.path.clone(), String::new()));
            }
            report.held_back = Some(group);
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
        if let Some(why) = win32_addresses_a_different_path(seg) {
            return Err(why);
        }
        // A segment containing a drive letter (`C:`) or a backslash would, on Windows, be
        // reinterpreted as rooting/escaping once joined — reject it the same way.
        //
        // **`cfg!(windows)`-gated (CPE-1823 round 2), and the gate is load-bearing.** On Linux and macOS
        // neither character roots anything: `:` and `\` are ordinary filename bytes, `a\b` is one real
        // directory entry, and `2026-08-21 10:30 notes.txt` is a name users type on purpose. macOS makes
        // it routine rather than exotic — a Finder name containing `/` is stored on disk as `:`, so any
        // "Q1/Q2 report" becomes a colon on the volume.
        //
        // Refusing those everywhere was survivable while this rule only served `apply_write`, which
        // *skips one file and continues* with the reason in its report. `snapshot_capture::restore` now
        // shares the rule and **aborts the whole manifest** at the first refusal, so an unqualified
        // colon check would turn one ordinary Unix filename into a half-restored tree — a file that
        // restored fine before CPE-1823 touched anything. Same reasoning, and the same `cfg!(windows)`
        // shape, as `crate::backup`'s `refuses_unstable_names` and `fsutil::win32_name_is_unstable`'s
        // doc: apply the Windows rule where Windows is, and nowhere else.
        //
        // Containment on Unix is unaffected — it never rested on this line. `..`, `.`, empty segments and
        // absolute paths are all rejected above, and every surviving segment is pushed as one component.
        if cfg!(windows) && (seg.contains(':') || seg.contains('\\')) {
            return Err("escapes dest_root: drive-rooted or backslash segment".to_string());
        }
        // CPE-1823 round 2: a segment Win32 resolves to something OTHER than what it spells.
        // Lives HERE, not at a call site, so every `safe_target` caller inherits it — see below.
        segments.push(seg);
    }
    Ok(segments)
}

/// Names that Win32 resolves to **something other than what they spell**, so acting on one neither
/// refuses nor does what it says (CPE-1823 round 2). Two shapes, both measured:
///
/// - a **reserved DOS device name** (`sub/NUL`, `CON.txt`) — the write "succeeds" into the device and
///   leaves nothing on disk, so a revert reported `applied: 1, skipped: []` against an empty tree;
/// - a **trailing dot or space** (`evil.txt `) — Win32 strips it on any non-verbatim path, so the entry
///   addresses `evil.txt`. In this engine that is **destructive**, not merely wrong: `plan_restore` sees
///   `a.txt ` and `a.txt` as two distinct keys and plans Create `a.txt ` + Delete `a.txt`. Writes run
///   first, so the Create lands *on* `a.txt`; the Delete then removes it. The user's real file is gone,
///   the tree is empty, and the report says `applied: 2, skipped: []` — complete success, from a planted
///   manifest, through a registered command.
///
/// **Why it lives in [`safe_segments`] rather than at a call site.** CPE-1823 twice put a guard on
/// `snapshot_capture::restore` — which has no production caller — while `apply_write` and `apply_delete`,
/// reached from `checkpoint_revert`/`checkpoint_revert_one`, went unguarded. All four `safe_target`
/// callers now inherit this by construction, and a fifth cannot forget it.
///
/// **`cfg!(windows)`-gated**, like the `:`/`\` rule above it and for the same reason: on Linux and macOS
/// `NUL` and `notes. ` are ordinary distinct filenames that a capture will store and a revert must
/// restore. Reuses [`crate::fsutil::win32_name_is_unstable`] and
/// [`crate::transfer::is_windows_device_name`] — the crate's existing predicates, not a third spelling.
///
/// **A host property, not a manifest property — recorded rather than assumed away.** The gate asks about
/// the machine doing the restoring, so a manifest captured on Linux carrying `a\b` or `notes. ` restores
/// as a literal name there and is refused here. That is the correct direction — refuse where the hazard
/// is — but it does mean the same manifest is legal on one machine and not another: the round-trip
/// asymmetry is *reduced* by these gates, not eliminated.
///
/// **Where this predicate stops, so nobody reads it as "the class is shut".** Rounds 2–4 recorded a
/// remaining member here — a capture holding both `A.txt` and `a.txt` collapsing onto one file on a
/// case-folding volume — as "pre-existing, out of scope". That framing was **wrong twice** and is
/// corrected rather than deleted, because getting it wrong is what cost round 5. It was not one case
/// class but two, and the second is strictly worse: a **single** planted key — `A.txt` against a live
/// `a.txt` — needs no second capture entry, destroys a specific named file the checkpoint does hold,
/// and reaches through cherry-revert where nothing else in this engine can see it.
///
/// Both are now closed, and **not here**: no name-based predicate can close them, because `A.txt` and
/// `a.txt` are legal on every platform and refusing either would break a legitimate capture on the
/// machine it came from. They are closed at the *resolution* level instead — [`landing`] and its two
/// callers in [`execute_restore`] and [`apply_write`]. This predicate keeps only what resolution cannot
/// answer: a device name resolves to `\\?\NUL`, which no checkpoint entry can collide with, and a
/// trailing-dot name may address a file no entry names either.
fn win32_addresses_a_different_path(seg: &str) -> Option<String> {
    if !cfg!(windows) {
        return None;
    }
    if crate::fsutil::win32_name_is_unstable(seg) {
        return Some(format!(
            "the component {seg:?} ends in a dot or space, which Windows strips — it would address a \
             different path than it names, silently colliding with whatever answers to the stripped one"
        ));
    }
    if crate::transfer::is_windows_device_name(seg) {
        return Some(format!(
            "the component {seg:?} is a reserved DOS device name — the write would succeed into the \
             device and leave nothing on disk, reporting work that did not happen"
        ));
    }
    None
}

/// How many blocking entries a held-back-delete reason names before falling back to a count. Enough to
/// identify the cause without turning one skipped path's reason into a wall of text when a whole
/// checkpoint is unrestorable.
///
/// (Sits below the function on purpose: dropped in between `win32_addresses_a_different_path`'s doc
/// block and its signature, it silently *stole* that doc — rustdoc rendered four rounds of argument on a
/// `usize` and left the function undocumented.)
const NAMED_CAUSES: usize = 3;

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
    // **Resolved containment, for every caller (CPE-1823 round 3).** [`safe_segments`] is textual, and a
    // textual check cannot see a symlink or junction planted at an *interior* component: `sub/x.txt` is
    // four innocent characters and a slash, and if `root/sub` leads out of the tree then the write, or
    // the `remove_file`, lands outside it. Nothing about the spelling gives that away.
    //
    // This was closed in `snapshot_capture::restore` — which has no production caller — and left open in
    // `apply_write` and `apply_delete`, which the registered `checkpoint_revert` commands reach. That is
    // the third time in this ticket a guard landed on the function with no callers, and it was found by
    // walking every manifest-derived value to its sink rather than by another review. Putting it here
    // rather than at the call sites is the same conclusion as the Win32 rule above: the callers cannot
    // drift if they do not each own a copy.
    //
    // `confined_to` canonicalises and fails closed, and it handles a target that does not exist yet by
    // walking up to the nearest ancestor that does — which is why `restore` can still be handed a
    // destination it is about to create, provided the destination itself exists by the time this runs.
    //
    // **Cost, recorded so it is a known trade rather than a surprise, and counted honestly.** Every
    // `safe_target` call canonicalises, plus an ancestor walk for each name that does not exist yet.
    // The per-entry multiplier is no longer 1: `snapshot_capture::restore` resolves each entry **twice**
    // (pass 1 for the abort decision, pass 2 immediately before its own write — round 4), so a
    // 10k-entry restore is 20k+ walks, not 10k+; and `execute_restore` adds [`landing`] resolutions on
    // top — one per checkpoint key and one per delete, but only when the plan contains a delete
    // (round 5). The textual rules are not the cost: `safe_segments` over 20,000 keys measured ~32 ms
    // in an unoptimised debug build, which these walks dwarf, so there is nothing to cache there.
    // The right trade for a destructive operation on local disk, but still unmeasured against a network
    // share — worth one run against SMB or the QNAP before someone meets it on a slow mount.
    if !crate::fsutil::confined_to(&p, root) {
        return Err(format!("escapes dest_root: {rel:?} resolves outside {}", root.display()));
    }
    Ok(p)
}

/// Where `rel` **lands** under `root`: the file it addresses as the filesystem resolves it, rather than
/// the spelling that produced it (CPE-1823 round 5). `None` when nothing answers to it yet.
///
/// This is the identity half of the same principle [`crate::fsutil::confined_to`]'s doc states for
/// containment — *assert on the resolved path, never on the spelling* — and it exists because the
/// spelling is structurally unable to answer the question `execute_restore` needs. `A.txt` and `a.txt`
/// are both perfectly legal names on **every** platform, so no per-name rule may refuse either; on a
/// case-folding volume they are nonetheless one file. Trailing spaces, trailing dots, 8.3 short names,
/// Unicode-folding volumes and a directory link inside the tree are all the same shape, and the next one
/// will be too. Resolving both sides and comparing costs nothing in enumeration and covers all of them.
///
/// **Deliberately not a safety check and deliberately not [`safe_target`].** It answers "which file is
/// this?", not "may we touch it?" — the escape question is `safe_target`'s and stays there. So the
/// spellings `safe_target` refuses are *still resolved* here (that is the whole point: `a.txt ` has to
/// resolve to `a.txt` for the collision to be visible), and only the shapes that are not identity
/// questions at all are declined: an absolute path, an empty segment, `.` and `..`. Those are escapes,
/// answered by `safe_target`, and canonicalising them here would put this helper in the containment
/// business by accident.
///
/// Returning `None` is always the safe direction for its callers: an unresolvable path yields no
/// collision, and the action then faces `safe_target`'s judgement exactly as before.
pub(crate) fn landing(root: &Path, rel: &str) -> Option<PathBuf> {
    if rel.is_empty() || Path::new(rel).is_absolute() {
        return None;
    }
    let mut p = root.to_path_buf();
    for seg in rel.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return None;
        }
        p.push(seg);
    }
    fs::canonicalize(&p).ok()
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
) -> Result<(), Refused> {
    // Every `safe_target` refusal — `escapes dest_root` in its four textual forms, the Win32-unstable
    // name rule, and the resolved-containment failure — is a verdict on the checkpoint's own stored
    // spelling. It recurs identically on every run here, so none of them may be reported as retryable.
    let target = safe_target(dest_root, &action.path).map_err(Refused::permanent)?;
    let Some(state) = checkpoint.get(&action.path) else {
        return Err(Refused::permanent("no checkpoint entry for this path"));
    };
    // CPE-1823: `state` comes from `snapshot_capture::manifest_snapshot` — the same planted JSON — and
    // `hash` was joined onto `blobs_dir` with nothing checking it, so `../../<anywhere>/secret` copied
    // any readable file into the reverted tree and the report counted it **applied**. This is the sink
    // the app actually ships: `checkpoint_revert` / `checkpoint_revert_one` reach it from registered
    // Tauri commands, where `snapshot_capture::restore` has no production caller at all.
    //
    // One shared validator (`snapshot_capture::blob_source`), never a second spelling of the hex rule.
    // The refusal lands in this action's `Err`, which `apply_restore` records as a per-file
    // skip-with-reason in the report — this engine's existing loud channel, not a silent drop.
    // A stored hash that is not a plain hex name, and a blob path that does not resolve inside the
    // store, are both properties of the checkpoint file: permanent. (The far commoner "the blob FILE is
    // missing" is not this — it surfaces at the `fs::copy` below, and it is genuinely transient.)
    let blob =
        crate::snapshot_capture::blob_source(blobs_dir, &state.hash).map_err(Refused::permanent)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| Refused::transient(format!("{}: {e}", parent.display())))?;
    }
    // **CPE-1823 round 5 — a destructive NON-delete, structurally outside the delete stand-down.**
    // [`RestoreOp`] has three variants and every rule before this one guarded the third. A single
    // `Create` for `PAYROLL.CSV` against a live `payroll.csv` rewrote the user's file and reported
    // complete success — one action, nothing skipped, no delete anywhere to stand down:
    //
    // ```text
    // R4-OVERWRITE report = RestoreReport { applied: 1, skipped: [] }
    //              payroll.csv = "ATTACKER CHOSEN BYTES"
    // ```
    //
    // The rule is about the op's **premise**, not about the name — which is what lets it be strict
    // without refusing any spelling. `plan_restore` emits `Create` only for a path present in the
    // checkpoint and absent from the scan of the live tree. If something already answers to that name,
    // the plan's reading of the tree and the filesystem's resolution disagree, and that disagreement is
    // the aliasing signal itself. `Overwrite` is untouched: it means "this file is there and its content
    // differs", so writing onto an existing file is exactly what it is for.
    //
    // Also the reason two `Create`s that collapse onto one file cannot be applied as "complete success":
    // the first writes, the second then finds its target occupied and is refused with a reason, rather
    // than silently overwriting a sibling entry and counting two.
    //
    // **The one non-aliasing way to reach this, recorded so it does not read later as a false positive.**
    // `scan_dir` honours the crate-wide skip-unreadable guardrail, so a file it could not hash is absent
    // from `current` and its checkpoint entry is planned as a `Create` even though the file is right
    // there. This now refuses it instead of overwriting it. That is the correct direction — a file we
    // could not read is a file we cannot say is safe to clobber — and it is reported per path with this
    // reason, not dropped. The same applies to a path a symlink has since taken over: `DirEntry::metadata`
    // does not traverse, so the scan does not see it as a file, and following the link to overwrite
    // whatever it points at is not what a revert of that path means.
    //
    // **Placed here, immediately before the copy, not in a pre-pass** — round 4's lesson, paid for once
    // already: a verdict reached before `create_dir_all` and `blob_source` is a verdict the destination
    // can invalidate before the write it is protecting. Nothing may sit between this and the write.
    //
    // **CPE-1846 closed the final-component link swap structurally.** The write below is no longer
    // `fs::copy` but `fsutil::copy_file_onto_no_follow`, which opens `target` with `batch_media`'s
    // never-follow-a-link open (`O_NOFOLLOW` / `FILE_FLAG_OPEN_REPARSE_POINT`, the same per-target
    // constants, not a second spelling), refuses the handle if it addresses a link or a directory, and
    // streams the blob through it. The object written is the object opened, so a link swapped into that
    // name after this check has nothing left to redirect. `Overwrite` is unaffected: opening an existing
    // regular file for truncate-and-write is exactly what it means.
    //
    // **Still open, and deliberately still recorded:** the *interior*-component race. `safe_target`
    // resolves the directories above the final component by path and the open is by path too, so a
    // directory link swapped into an interior component in between still redirects the write. `std`
    // exposes no `openat`-relative resolution to close it with.
    if action.op == RestoreOp::Create && fs::symlink_metadata(&target).is_ok() {
        // **Permanent** (review round 2). The shape that reaches this is a case fold or another alias —
        // `A.txt` against a live `a.txt` on a case-insensitive volume — and the volume does not stop
        // folding between runs. Reporting it as "temporary, run the revert again" sends the user round a
        // loop that ends in the same place.
        return Err(Refused::permanent(format!(
            "this entry restores a file the plan read as absent, but {} already answers to that name — \
             the spelling resolves to a file the scan did not see under it, and writing would overwrite \
             content nothing in this plan accounted for",
            target.display()
        )));
    }
    // The reason reaches the USER — CPE-1845 renders it in the revert panel — so it names the blob by
    // its hash, already validated as plain hex by `blob_source`, rather than by its absolute path inside
    // the app's private checkpoint store. `copy_file_onto_no_follow` is written to hold that line from
    // its side too: it never names its *source* in a message, only the destination, which is the user's
    // own tree and the half they need to see.
    //
    // **CPE-1845 and CPE-1846 meet here, and the classification is the whole of the merge.** CPE-1845
    // split refusals into transient ("fix the cause and re-running works") and permanent ("nothing about
    // re-running changes this"). CPE-1846 added a refusal that is emphatically **permanent**: a link or a
    // directory sitting at the destination name. A planted link does not stop being a link because the
    // user runs the revert again, and telling them to try again is precisely the loop CPE-1845 exists to
    // stop sending people round. Everything else here — a pruned or unreadable blob, a locked
    // destination, a full disk — is transient, exactly as before.
    //
    // **Decided by asking the filesystem, never by parsing the message.** A string match on our own
    // wording is the kind of coupling that silently stops working the first time someone improves a
    // sentence. Re-resolving the destination by path is safe *here and only here*: by this point the
    // write has already happened or already been refused, so this decides WORDING and can no longer
    // decide where a byte goes. That is the one property that made the same call unsafe before the copy.
    crate::fsutil::copy_file_onto_no_follow(&blob, &target).map_err(|why| {
        let name_is_taken_by_a_link_or_dir = fs::symlink_metadata(&target)
            .is_ok_and(|m| m.file_type().is_symlink() || m.is_dir());
        if name_is_taken_by_a_link_or_dir {
            Refused::permanent(why)
        } else {
            Refused::transient(format!(
                "the checkpoint's stored copy of this file (blob {}) could not be written: {why}",
                state.hash
            ))
        }
    })?;
    Ok(())
}

fn apply_delete(action: &RestoreAction, dest_root: &Path) -> Result<(), Refused> {
    let target = safe_target(dest_root, &action.path).map_err(Refused::permanent)?;
    // A locked file or a permission error — transient by nature.
    fs::remove_file(&target)
        .map_err(|e| Refused::transient(format!("{}: {e}", target.display())))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::restore_plan::plan_restore;
    use crate::snapshot_capture::scan_dir;

    /// **CPE-1845 — the assertion helper that replaced `why.contains("not deleted")`.**
    ///
    /// Every test below used to prove a delete was held back by matching the *prose* of its reason.
    /// That is exactly the coupling this ticket removes, and it also could not tell a deliberate
    /// hold-back from a failure that happened to be worded similarly. This asks the structure instead:
    /// is `path` in the report's hold-back group, and which of the two hold-back states is it?
    ///
    /// It deliberately does **not** look at `report.skipped` — an entry there is a genuine failure by
    /// construction now, so a hold-back landing in it would be the bug, not a pass.
    fn held_back_as(report: &RestoreReport, path: &str) -> Option<OpOutcome> {
        let group = report.held_back.as_ref()?;
        group.paths.iter().find(|(p, _)| p == path).map(|_| group.outcome.as_outcome())
    }

    /// Asserts the group carried by `report` is coherent, and returns it. Folded into a helper rather
    /// than repeated per test so a fixture that never armed the hold-back cannot pass by omission —
    /// the CPE-1844 lesson (a liveness claim that inverted under a decoy).
    fn live_hold_back(report: &RestoreReport) -> &HeldBack {
        let group = report
            .held_back
            .as_ref()
            .unwrap_or_else(|| panic!("fixture is inert: nothing was held back at all: {report:?}"));
        assert!(!group.paths.is_empty(), "a hold-back with no paths certifies nothing: {group:?}");
        assert!(!group.reason.is_empty(), "a hold-back must state its reason once: {group:?}");
        assert!(!group.next_step.is_empty(), "a hold-back must offer a next step: {group:?}");
        group
    }

    /// **The CPE-1845 x CPE-1846 merge, pinned.** The two tickets landed against each other: CPE-1845
    /// classifies every write refusal as transient ("fix it and re-run") or permanent ("re-running
    /// changes nothing"), and CPE-1846 added a brand-new refusal — a link or a directory sitting at the
    /// destination name — that CPE-1845 never saw. Resolving the conflict meant choosing a class for it,
    /// and the wrong choice is invisible: a planted link reported as transient tells the user to run the
    /// revert again, which produces the identical refusal forever. That is the exact loop CPE-1845
    /// exists to stop, arriving through a door it did not know about.
    ///
    /// The plan is built by hand with `Overwrite` rather than through `plan_restore`, deliberately: a
    /// scan run *after* the link is planted would not see `a.txt` as a file, the plan would say `Create`,
    /// and the Create-premise refusal above would fire first — a different (also permanent) rule, so the
    /// test would pass while covering nothing about the one under test.
    #[test]
    fn cpe_1846_a_link_at_the_destination_is_reported_permanent_not_as_run_it_again() {
        let store = scratch("1846-perm-store");
        fs::create_dir_all(store.join("blobs")).unwrap();
        fs::write(store.join("blobs").join("1846aaaa"), b"ok").unwrap();

        let root = scratch("1846-perm-root");
        fs::write(root.join("bystander.txt"), b"BYSTANDER").unwrap();
        fs::write(root.join("added.txt"), b"user file").unwrap();
        // Liveness is asserted inside `make_file_link` (the slot holds a link AND resolves to the
        // bystander); `require_staged` turns a staging failure on a platform that supports the mechanism
        // into a red rather than a silent skip.
        if !crate::fsutil::make_file_link(&root.join("bystander.txt"), &root.join("a.txt")) {
            use std::io::Write;
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1846] SKIPPED the permanent-classification leg: no file symlink privilege here. \
                 NOTHING on this run covered how a link refusal is reported."
            );
            return;
        }

        let mut checkpoint = Snapshot::new();
        checkpoint.insert("a.txt".to_string(), crate::restore_plan::FileState::new("1846aaaa", 2));
        let report = execute_restore(
            &[
                RestoreAction { path: "a.txt".to_string(), op: RestoreOp::Overwrite },
                RestoreAction { path: "added.txt".to_string(), op: RestoreOp::Delete },
            ],
            &root.to_string_lossy(),
            &store.to_string_lossy(),
            &checkpoint,
        );

        // HARM FIRST, before any claim about how it was reported.
        assert_eq!(
            fs::read(root.join("bystander.txt")).ok().as_deref(),
            Some(&b"BYSTANDER"[..]),
            "HARM: the revert wrote through a link at the final component: {report:?}"
        );
        // Fixture liveness for the BRANCH: it must be the link refusal that armed the hold, not the
        // checkpoint-key stand-down above it and not some other rule.
        assert_eq!(report.skipped.len(), 1, "fixture is inert: nothing was refused: {report:?}");
        // **The refusal READS differently per platform, and that asymmetry is load-bearing, not noise.**
        // On Unix `O_NOFOLLOW` fails the **open itself**, so the post-open refusals never run and the
        // message is the generic open error; on Windows the open succeeds on the reparse point and the
        // post-open check is what refuses, in words. `copy_file_onto_no_follow`'s own doc says exactly
        // this, and this test is where it was PROVEN: the first push asserted the Windows wording
        // unconditionally and reddened `Server crates` on ubuntu **and** macOS with
        // `could not open the destination for writing: Too many levels of symbolic links (os error 40)`.
        // That is the ELOOP claim the Work Log had listed as "not verified locally", verified by CI in
        // the strongest available way — a test that could not pass unless it were true.
        //
        // The errno's *text* is deliberately not matched (Linux and macOS need not word it alike, and a
        // libc could reword it); the prefix plus the asserted-live planted link is what identifies the
        // refusal. Asserting merely "something was refused" would let any earlier rule satisfy this.
        let refusal = &report.skipped[0].1;
        let is_the_link_refusal = if cfg!(windows) {
            refusal.contains("never writes through one")
        } else {
            refusal.contains("could not open the destination for writing")
        };
        assert!(
            is_the_link_refusal,
            "fixture is inert: the refusal is not the link one this test is about: {report:?}"
        );
        assert!(
            checkpoint.keys().all(|k| safe_segments(k).is_ok()),
            "fixture is inert: a checkpoint KEY is unrestorable, so the branch above this one armed"
        );
        // The refusal text reaches the revert panel, so it must not carry the private store's layout.
        assert!(
            !report.skipped[0].1.contains("blobs"),
            "a user-visible refusal must not name the app's private checkpoint store: {report:?}"
        );

        let group = live_hold_back(&report);
        assert_eq!(
            group.outcome,
            HeldBackOutcome::HeldBackByCheckpoint,
            "a link at the destination is still a link on the next run — this is NOT retryable: {group:?}"
        );
        assert!(!group.outcome.retryable(), "{group:?}");
        assert!(
            !group.next_step.to_lowercase().contains("run the revert again"),
            "telling the user to re-run produces the identical refusal forever: {group:?}"
        );

        let _ = fs::remove_dir_all(&store);
        let _ = fs::remove_dir_all(&root);
    }

    /// **CPE-1845 review round 2 — a permanent refusal must not be reported as "run it again".**
    ///
    /// `RestoreReport::skipped` is fed by EVERY write refusal, and most of them recur forever: the four
    /// `escapes dest_root` forms, the Win32-unstable name rule, the resolved-containment failure, and the
    /// Create-premise refusal are all verdicts on the checkpoint's own stored spelling. The first cut of
    /// this ticket labelled the whole group `SkippedByPlan`, `retryable: true`, "…and run the revert
    /// again" — which is this ticket's own acceptance criterion violated on a narrower set of shapes.
    /// Reproduced by the independent Reviewer through the production `execute_restore`:
    ///
    /// ```text
    /// PROBE skipped=[("a/../evil.txt", "escapes dest_root: `.`/`..` segment")]
    /// PROBE outcome=SkippedByPlan retryable=true
    ///       next_step=This one is temporary: … and run the revert again …
    /// ```
    ///
    /// Both legs run here, from the same fixture shape, so neither can pass by the other's accident.
    #[test]
    fn cpe_1845_a_permanent_write_refusal_is_never_reported_as_retryable() {
        let store = scratch("1845-perm-store");
        fs::create_dir_all(store.join("blobs")).unwrap();
        // A real blob, so the RESTORABLE entry genuinely restores and `unrestorable` stays empty — this
        // has to reach the `report.skipped` branch, not the checkpoint-key branch above it.
        fs::write(store.join("blobs").join("1845aaaa"), b"ok").unwrap();

        // ---- leg 1: a PERMANENT refusal (the Reviewer's probe, verbatim in shape). ----
        let root = scratch("1845-perm-root");
        fs::write(root.join("added.txt"), b"user file").unwrap();
        // The checkpoint holds ONLY restorable keys, so the checkpoint-key stand-down above this branch
        // stays disarmed and the `report.skipped` branch is genuinely the one under test. The bad path is
        // in the PLAN, where `apply_write`'s own `safe_target` refuses it.
        let mut checkpoint = Snapshot::new();
        checkpoint.insert("fine.txt".to_string(), crate::restore_plan::FileState::new("1845aaaa", 2));
        let report = execute_restore(
            &[
                RestoreAction { path: "a/../evil.txt".to_string(), op: RestoreOp::Create },
                RestoreAction { path: "added.txt".to_string(), op: RestoreOp::Delete },
            ],
            &root.to_string_lossy(),
            &store.to_string_lossy(),
            &checkpoint,
        );
        // Fixture liveness, both halves: the refusal happened, AND it is the `report.skipped` branch
        // that armed the hold — if `unrestorable` had caught it first this would certify nothing about
        // the branch under test.
        assert_eq!(report.skipped.len(), 1, "fixture is inert: nothing was refused: {report:?}");
        assert!(
            report.skipped[0].1.contains("escapes dest_root"),
            "fixture is inert: the refusal is not the permanent one this test is about: {report:?}"
        );
        assert!(
            checkpoint.keys().all(|k| safe_segments(k).is_ok()),
            "fixture is inert: a checkpoint KEY is unrestorable, so the branch above this one armed and              the `report.skipped` branch under test never ran: {report:?}"
        );
        let group = live_hold_back(&report);
        assert_eq!(
            group.outcome,
            HeldBackOutcome::HeldBackByCheckpoint,
            "a refusal this platform reaches identically every run is NOT retryable: {group:?}"
        );
        assert!(!group.outcome.retryable(), "{group:?}");
        assert!(
            !group.next_step.to_lowercase().contains("run the revert again"),
            "and it must not send the user round a loop that ends in the same place: {group:?}"
        );

        // ---- leg 2: the same shape with a TRANSIENT refusal still says "again". ----
        let root2 = scratch("1845-transient-root");
        fs::write(root2.join("added.txt"), b"user file").unwrap();
        let mut cp2 = Snapshot::new();
        // Valid hex, no such blob file → the refusal lands at `fs::copy`, which is genuinely transient.
        cp2.insert("gone.txt".to_string(), crate::restore_plan::FileState::new("1845cccc", 2));
        let report2 = execute_restore(
            &[
                RestoreAction { path: "gone.txt".to_string(), op: RestoreOp::Create },
                RestoreAction { path: "added.txt".to_string(), op: RestoreOp::Delete },
            ],
            &root2.to_string_lossy(),
            &store.to_string_lossy(),
            &cp2,
        );
        assert_eq!(report2.skipped.len(), 1, "fixture is inert: nothing was refused: {report2:?}");
        let group2 = live_hold_back(&report2);
        assert_eq!(
            group2.outcome,
            HeldBackOutcome::SkippedByPlan,
            "an attempt that merely failed IS retryable — the split must cut both ways or it is just a              blanket rename: {group2:?}"
        );
        assert!(group2.next_step.contains("run the revert again"), "{group2:?}");

        let _ = fs::remove_dir_all(&store);
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&root2);
    }

    /// **CPE-1845 UAT — the false sentence in the mixed case.**
    ///
    /// An unrestorable-name checkpoint AND a genuine restore failure in the same run is not exotic: a
    /// cross-platform backup restored on a machine with one file open reaches it. The independent UAT
    /// stood the shape up and quoted the panel:
    ///
    /// ```text
    /// Applied 0 changes, 2 failed, 1 deletion held back.
    /// 1 of this checkpoint's entries cannot be restored on this computer ("a/../evil.txt") …
    /// There is no fix … Everything restorable has already been restored — delete these files yourself …
    ///   gone1.txt — …blobs\11111111: The system cannot find the file specified. (os error 2)
    ///   gone2.txt — …blobs\22222222: The system cannot find the file specified. (os error 2)
    /// ```
    ///
    /// The completeness claim sat three lines above two entries that had just failed to restore — and it
    /// is the **premise** of the clause after it, so a user who trusts it deletes the leftovers believing
    /// the restore half finished. This asserts it is absent, and that what replaces it points at the
    /// failures instead.
    #[test]
    fn cpe_1845_the_completeness_claim_is_absent_when_something_actually_failed() {
        let store = scratch("1845-mixed-store");
        fs::create_dir_all(store.join("blobs")).unwrap();
        let root = scratch("1845-mixed-root");
        fs::write(root.join("added.txt"), b"user file").unwrap();

        // Both halves at once: one checkpoint key this platform cannot restore (the permanent hold-back)
        // plus two entries whose stored content is missing (two genuine failures).
        let mut checkpoint = Snapshot::new();
        checkpoint.insert("a/../evil.txt".to_string(), crate::restore_plan::FileState::new("1845dead", 3));
        checkpoint.insert("gone1.txt".to_string(), crate::restore_plan::FileState::new("11111111", 3));
        checkpoint.insert("gone2.txt".to_string(), crate::restore_plan::FileState::new("22222222", 3));
        let report = execute_restore(
            &[
                RestoreAction { path: "gone1.txt".to_string(), op: RestoreOp::Create },
                RestoreAction { path: "gone2.txt".to_string(), op: RestoreOp::Create },
                RestoreAction { path: "added.txt".to_string(), op: RestoreOp::Delete },
            ],
            &root.to_string_lossy(),
            &store.to_string_lossy(),
            &checkpoint,
        );

        // Fixture liveness: BOTH halves must actually be present, or this certifies nothing. A run with
        // no failures would pass a naive "the sentence is absent" assertion for the wrong reason.
        assert_eq!(report.skipped.len(), 2, "fixture is inert: nothing failed to restore: {report:?}");
        assert!(
            root.join("added.txt").exists(),
            "fixture is inert: the delete ran, so nothing was held back: {report:?}"
        );
        let group = live_hold_back(&report);
        assert_eq!(
            group.outcome,
            HeldBackOutcome::HeldBackByCheckpoint,
            "the unrestorable KEY must win over the transient branch — it is the permanent cause and \
             the branch order encodes that: {group:?}"
        );

        assert!(
            !group.next_step.contains("Everything restorable has already been restored"),
            "the completeness claim is FALSE here — two entries failed to restore in this same run — and \
             it is the premise of the clause telling the user to delete the leftovers: {group:?}"
        );
        assert!(
            group.next_step.contains("check the failures listed above"),
            "and what replaces it must point at the failures, not merely omit the claim: {group:?}"
        );
        // The advice that is still true either way survives.
        assert!(group.next_step.to_lowercase().contains("delete these files yourself"), "{group:?}");

        // And the internal checkpoint-store path stays out of user-facing text (UAT orange 2).
        for (path, reason) in &report.skipped {
            assert!(
                !reason.contains("blobs"),
                "a reason shown to the user must not expose the private blob-store layout: {path}: \
                 {reason}"
            );
            assert!(
                reason.contains("blob 11111111") || reason.contains("blob 22222222"),
                "but it must still name WHICH stored copy is missing: {path}: {reason}"
            );
        }

        let _ = fs::remove_dir_all(&store);
        let _ = fs::remove_dir_all(&root);
    }

    /// **CPE-1845, the second half of the ticket — the wording must match the state.**
    ///
    /// The recorded UI wording was *"held back, re-run after fixing"*, applied to every hold-back. It is
    /// right for [`OpOutcome::SkippedByPlan`] and **wrong** for [`OpOutcome::HeldBackByCheckpoint`],
    /// where re-running on this platform can never help: the user is sent to do something that cannot
    /// succeed, with no alternative offered. This asserts the two branches say opposite things, and
    /// that the permanent one still offers a real next step rather than a dead end.
    ///
    /// Both fixtures are proved live by [`live_hold_back`] before anything is read off them.
    #[test]
    fn cpe_1845_only_the_retryable_hold_back_may_tell_the_user_to_run_it_again() {
        let store = scratch("1845-wording-store");
        fs::create_dir_all(store.join("blobs")).unwrap();

        // Both permanent branches, checked identically. Leg 1 is the empty checkpoint (CPE-1847); leg 2
        // is a checkpoint key this platform cannot restore (CPE-1823) — armed portably with a `..`
        // segment, which `safe_segments` refuses on every OS, so both CI legs cover it.
        let mut unrestorable = Snapshot::new();
        unrestorable
            .insert("a/../b.txt".to_string(), crate::restore_plan::FileState::new("1845-none", 3));
        for (leg, checkpoint) in [("empty", Snapshot::new()), ("unrestorable-key", unrestorable)] {
            let root = scratch(&format!("1845-wording-{leg}"));
            fs::write(root.join("added.txt"), b"user file").unwrap();
            let report = execute_restore(
                &[RestoreAction { path: "added.txt".to_string(), op: RestoreOp::Delete }],
                &root.to_string_lossy(),
                &store.to_string_lossy(),
                &checkpoint,
            );
            assert!(
                root.join("added.txt").exists(),
                "fixture is inert: the {leg} leg's delete actually ran, so nothing was held back and \
                 the wording under test was never produced: {report:?}"
            );
            let permanent = live_hold_back(&report);
            assert_eq!(permanent.outcome, HeldBackOutcome::HeldBackByCheckpoint, "{leg}");
            assert!(
                !permanent.outcome.retryable(),
                "the {leg} hold-back cannot be fixed by running the revert again: {permanent:?}"
            );
            // The instruction forms only. The text may — and should — *mention* re-running in order to
            // say it will not help; what it must never do is send the user to do it.
            for imperative in [
                "run the revert again",
                "run it again",
                "re-run the revert",
                "re-run it",
                "try again",
                "please retry",
            ] {
                assert!(
                    !permanent.next_step.to_lowercase().contains(imperative),
                    "the {leg} hold-back must not tell the user to {imperative:?} — that is the \
                     recorded wording CPE-1845 was filed to remove, and here it cannot succeed: \
                     {permanent:?}"
                );
            }
            assert!(
                permanent.next_step.to_lowercase().contains("will not change")
                    || permanent.next_step.to_lowercase().contains("no fix for this"),
                "the {leg} hold-back must say plainly that re-running cannot help, rather than leave \
                 the user guessing: {permanent:?}"
            );
            assert!(
                permanent.next_step.to_lowercase().contains("delete these files yourself"),
                "and it must offer a real next step, not just a refusal: {permanent:?}"
            );
            // Both legs here are the PURE case — nothing failed to restore. The unrestorable-key branch
            // is the only one that makes a completeness claim at all (the empty-checkpoint branch says
            // "restored none"), and with nothing failed that claim is true and must be present. The
            // mixed-case test above is what proves it is not printed unconditionally.
            assert!(report.skipped.is_empty(), "this leg is the pure case by construction: {report:?}");
            if leg == "unrestorable-key" {
                assert!(
                    permanent.next_step.contains("Everything restorable has already been restored"),
                    "with nothing failed, this hold-back may and should say the restore half finished: {permanent:?}"
                );
            }
            let _ = fs::remove_dir_all(&root);
        }

        // Retryable: a missing blob. Put the blob back and the delete applies — so "run it again" is
        // exactly the right advice, and this branch is the one allowed to say it.
        let root_retryable = scratch("1845-wording-retryable");
        fs::write(root_retryable.join("added.txt"), b"user file").unwrap();
        let mut cp = Snapshot::new();
        // A VALID hex blob name whose file is absent. `"1845-no-such-blob"` would be refused by the
        // hex-name rule instead, which is a property of the checkpoint and therefore permanent — the
        // wrong leg entirely, and precisely the conflation review round 2 blocked this PR on.
        cp.insert("gone.txt".to_string(), crate::restore_plan::FileState::new("1845bbbb", 9));
        let retryable = execute_restore(
            &[
                RestoreAction { path: "gone.txt".to_string(), op: RestoreOp::Create },
                RestoreAction { path: "added.txt".to_string(), op: RestoreOp::Delete },
            ],
            &root_retryable.to_string_lossy(),
            &store.to_string_lossy(),
            &cp,
        );
        let retryable = live_hold_back(&retryable);
        assert_eq!(retryable.outcome, HeldBackOutcome::SkippedByPlan);
        assert!(retryable.outcome.retryable());
        assert!(
            retryable.next_step.to_lowercase().contains("run the revert again"),
            "the RETRYABLE hold-back must keep the advice that actually works: {retryable:?}"
        );

        let _ = fs::remove_dir_all(&store);
        let _ = fs::remove_dir_all(&root_retryable);
    }

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

    /// **CPE-1846 — the shipping sink's final-component link swap, and the overwrite it must not cost.**
    ///
    /// This is the one shape none of CPE-1823's five rounds closed. `safe_target`'s `confined_to`
    /// correctly *admits* a link that resolves back **inside** the reverted tree — it is a containment
    /// check, and the target is contained — and round 5's `Create`-premise rule deliberately leaves
    /// `Overwrite` alone, because writing onto an existing file is what `Overwrite` means. So a link
    /// planted at an `Overwrite` target after the plan was made, pointing at a bystander file in the same
    /// tree, took the checkpoint's bytes straight onto the bystander and reported success.
    ///
    /// The plan is computed **before** the link is planted, deliberately: `scan_dir` does not traverse
    /// links, so planting first would make `current` lose the path and the plan would say `Create` — a
    /// different rule, already covered, and not the one this test exists for.
    ///
    /// The second half is the constraint that killed `create_new`: `b.txt`'s ordinary overwrite must
    /// still apply in the same run. A fix that closes the link by refusing existing names would red here.
    #[test]
    fn cpe_1846_a_link_planted_at_an_overwrite_target_is_refused_and_the_other_overwrite_still_applies() {
        let root = scratch("cpe1846-link-overwrite");
        let store = scratch("cpe1846-link-store");

        fs::write(root.join("a.txt"), b"checkpoint a").unwrap();
        fs::write(root.join("b.txt"), b"checkpoint b").unwrap();
        fs::write(root.join("bystander.txt"), b"BYSTANDER").unwrap();
        let checkpoint = scan_dir(&root.to_string_lossy()).unwrap();
        write_blobs(
            &store,
            &checkpoint,
            &[("a.txt", b"checkpoint a"), ("b.txt", b"checkpoint b"), ("bystander.txt", b"BYSTANDER")],
        );

        fs::write(root.join("a.txt"), b"edited a").unwrap();
        fs::write(root.join("b.txt"), b"edited b").unwrap();
        let current = scan_dir(&root.to_string_lossy()).unwrap();
        let plan = plan_restore(&checkpoint, &current);
        assert_eq!(plan.len(), 2, "fixture is inert: expected two overwrites, got {plan:?}");
        assert!(
            plan.iter().all(|a| a.op == RestoreOp::Overwrite),
            "fixture is inert: this test is about Overwrite, and the plan is {plan:?}"
        );

        // The swap, after the plan and before the write. Liveness is asserted inside `make_file_link`
        // (the slot holds a link AND it resolves to the bystander) and again here by following it.
        fs::remove_file(root.join("a.txt")).unwrap();
        if !crate::fsutil::make_file_link(&root.join("bystander.txt"), &root.join("a.txt")) {
            let _ = std::io::Write::write_all(
                &mut std::io::stderr(),
                b"[CPE-1846] SKIPPED the planted-link overwrite leg: no file symlink privilege here. \
                  NOTHING on this run covered the final-component swap in the shipping revert sink.\n",
            );
            return;
        }
        assert_eq!(
            fs::read(root.join("a.txt")).ok().as_deref(),
            Some(&b"BYSTANDER"[..]),
            "fixture is inert: the planted link does not lead to the bystander"
        );

        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);

        // HARM FIRST.
        assert_eq!(
            fs::read(root.join("bystander.txt")).ok().as_deref(),
            Some(&b"BYSTANDER"[..]),
            "HARM: the revert wrote the checkpoint's bytes through a link at the final component, onto a \
             file the plan never named — report was {report:?}"
        );
        assert!(
            fs::symlink_metadata(root.join("a.txt")).is_ok_and(|m| m.file_type().is_symlink()),
            "the planted link must still be there, not replaced by bytes written over it"
        );
        // …and the legitimate overwrite in the same plan must be unaffected.
        assert_eq!(
            fs::read(root.join("b.txt")).ok().as_deref(),
            Some(&b"checkpoint b"[..]),
            "an ordinary Overwrite of an existing regular file must still apply: {report:?}"
        );
        assert_eq!(report.applied, 1, "{report:?}");
        assert_eq!(report.skipped.len(), 1, "{report:?}");
        assert_eq!(report.skipped[0].0, "a.txt", "{report:?}");
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

    /// **CPE-1823 round 2 — the sink the shipped app actually reaches.** `snapshot_capture::restore` has
    /// no production caller; *this* function does, via `checkpoint_revert` / `checkpoint_revert_one`.
    /// `state` comes from `manifest_snapshot` — the same hand-editable JSON — and its `hash` was joined
    /// onto `blobs/` unvalidated, so a climbing path pulled any readable file into the reverted tree and
    /// the report counted it **applied**.
    ///
    /// The harm assertion is on the file's *content*: the target path is one the plan legitimately names,
    /// so its existence proves nothing — only whether it holds the victim's bytes does.
    #[test]
    fn cpe_1823_an_escaping_hash_never_reads_a_file_outside_the_blob_store() {
        const SECRET: &[u8] = b"THE VICTIM PRIVATE KEY FROM OUTSIDE THE STORE";
        const LIVE: &[u8] = b"the user's real current content";
        let root = scratch("cpe1823-hash-root");
        let store = scratch("cpe1823-hash-store");
        let secrets = scratch("cpe1823-hash-secrets");
        fs::write(secrets.join("id_rsa"), SECRET).unwrap();
        fs::create_dir_all(store.join("blobs")).unwrap();

        // `blobs/` is `<store>/blobs`, so `../../<sibling>/id_rsa` climbs store → temp and back down.
        let hash = format!("../../{}/id_rsa", secrets.file_name().unwrap().to_string_lossy());
        let mut checkpoint = Snapshot::new();
        checkpoint
            .insert("a.txt".to_string(), crate::restore_plan::FileState::new(hash.as_str(), SECRET.len() as u64));

        fs::write(root.join("a.txt"), LIVE).unwrap();
        let current = scan_dir(&root.to_string_lossy()).unwrap();
        let plan = plan_restore(&checkpoint, &current);
        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);

        let landed = fs::read(root.join("a.txt")).unwrap();
        assert_ne!(
            landed, SECRET,
            "HARM: the revert pulled {} bytes from outside the blob store into the user's tree",
            SECRET.len()
        );
        assert_eq!(landed, LIVE, "a refused entry must leave the live file exactly as it was");
        assert_eq!(report.applied, 0, "a refused entry must never be counted applied");
        assert_eq!(report.skipped.len(), 1, "and it must be reported, not dropped: {report:?}");
        assert_eq!(report.skipped[0].0, "a.txt");
        assert!(
            report.skipped[0].1.contains("hex"),
            "the skip reason must say what was wrong, got: {}",
            report.skipped[0].1
        );

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&store);
    }

    /// **CPE-1823 round 2.** `safe_segments` refused `:` and `\` on every platform. That was survivable
    /// while this engine was the only caller — it skips one file and continues — but
    /// `snapshot_capture::restore` now shares the rule and aborts the whole manifest, so an ordinary
    /// Unix filename would have turned into a half-restored tree. On macOS this is routine, not exotic:
    /// a Finder name containing `/` is stored on disk as `:`.
    ///
    /// Nothing covered this before, which is why the ubuntu CI leg was green while the regression was
    /// live. Containment on Unix never rested on the colon rule — `..`, `.`, empty and absolute segments
    /// are all still refused above it, and `an_escaping_plan_path_is_refused_and_writes_nothing_outside_dest_root`
    /// still proves that on the same platform.
    #[cfg(unix)]
    #[test]
    fn cpe_1823_a_colon_or_backslash_is_an_ordinary_unix_filename_and_still_reverts() {
        for name in ["2026-08-21 10:30 notes.txt", r"Q1\Q2 report.txt"] {
            let root = scratch("cpe1823-unix-name-root");
            let store = scratch("cpe1823-unix-name-store");

            fs::write(root.join(name), b"checkpoint content").unwrap();
            let checkpoint = scan_dir(&root.to_string_lossy()).unwrap();
            assert!(checkpoint.contains_key(name), "the scan must key it verbatim: {checkpoint:?}");
            write_blobs(&store, &checkpoint, &[(name, b"checkpoint content")]);

            fs::write(root.join(name), b"edited since the checkpoint").unwrap();
            let current = scan_dir(&root.to_string_lossy()).unwrap();
            let plan = plan_restore(&checkpoint, &current);
            let report =
                execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);

            assert!(report.skipped.is_empty(), "{name:?} is a legal Unix filename: {:?}", report.skipped);
            assert_eq!(report.applied, 1);
            assert_eq!(fs::read(root.join(name)).unwrap(), b"checkpoint content");

            let _ = fs::remove_dir_all(&root);
            let _ = fs::remove_dir_all(&store);
        }
    }

    /// **CPE-1823 round 3 — the destructive one, and the reason this guard is not in `restore`.** A
    /// manifest entry `a.txt ` (one trailing space) against a live `a.txt` holding the user's real
    /// content. `plan_restore` sees two distinct keys, so it plans Create `a.txt ` + Delete `a.txt`.
    /// Writes run before deletes, Win32 strips the space so the Create lands **on** `a.txt`, and the
    /// Delete then removes it. Measured before the fix:
    ///
    /// ```text
    /// report = RestoreReport { applied: 2, skipped: [] }
    /// tree after revert = []
    /// a.txt = Err(NotFound)
    /// ```
    ///
    /// The user's file is gone and the command reports complete success. The harm assertion is that the
    /// file still holds its bytes — asserting on the report alone would pass on the unfixed code, which
    /// reports success precisely while destroying the file.
    #[cfg(windows)]
    #[test]
    fn cpe_1823_a_trailing_space_entry_never_destroys_the_file_it_aliases() {
        const LIVE: &[u8] = b"the user's only copy of their real content";
        let root = scratch("cpe1823-alias-root");
        let store = scratch("cpe1823-alias-store");
        fs::create_dir_all(store.join("blobs")).unwrap();
        fs::write(store.join("blobs").join("deadbeef"), b"attacker bytes").unwrap();

        fs::write(root.join("a.txt"), LIVE).unwrap();
        let mut checkpoint = Snapshot::new();
        checkpoint.insert("a.txt ".to_string(), crate::restore_plan::FileState::new("deadbeef", 14));

        let current = scan_dir(&root.to_string_lossy()).unwrap();
        let plan = plan_restore(&checkpoint, &current);
        assert_eq!(plan.len(), 2, "the premise: a Create for `a.txt ` and a Delete for `a.txt`");

        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);

        assert_eq!(
            fs::read(root.join("a.txt")).ok().as_deref(),
            Some(LIVE),
            "HARM: the revert destroyed the user's file via a trailing-space alias — report was {report:?}"
        );
        assert!(
            report.skipped.iter().any(|(p, _)| p == "a.txt "),
            "the aliasing entry must be reported as skipped, not applied: {report:?}"
        );
        // Refusing the write alone did NOT save the file — the paired Delete finished the job. The
        // stand-down has to be visible in the report too, or the next reader will "simplify" it away.
        assert_eq!(
            held_back_as(&report, "a.txt"),
            Some(OpOutcome::HeldBackByCheckpoint),
            "the paired delete must be held back and said so — structurally, not in prose (CPE-1845), \
             and as the NOT-retryable kind because a stored trailing-space name never becomes writable \
             here: {report:?}"
        );

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&store);
    }

    /// **CPE-1823 round 3.** A reserved device name through the *shipping* path. Before the fix:
    /// `RestoreReport { applied: 1, skipped: [] }` with `tree=[]` — the copy "succeeded" into the null
    /// device, so the engine reported restoring a file that is not on disk. `restore()` refused the same
    /// entry, which is exactly the asymmetry: the guard was on the function with no callers.
    #[cfg(windows)]
    #[test]
    fn cpe_1823_a_device_name_entry_is_never_reported_applied_with_nothing_on_disk() {
        let root = scratch("cpe1823-dev-root");
        let store = scratch("cpe1823-dev-store");
        fs::create_dir_all(store.join("blobs")).unwrap();
        fs::write(store.join("blobs").join("deadbeef"), b"evil").unwrap();

        let mut checkpoint = Snapshot::new();
        checkpoint.insert("sub/NUL".to_string(), crate::restore_plan::FileState::new("deadbeef", 4));
        let plan = vec![RestoreAction { path: "sub/NUL".to_string(), op: RestoreOp::Create }];

        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);

        assert_eq!(
            report.applied, 0,
            "a write into the null device leaves nothing on disk, so counting it applied reports work \
             that did not happen: {report:?}"
        );
        assert_eq!(report.skipped.len(), 1, "and it must be reported: {report:?}");
        assert_eq!(report.skipped[0].0, "sub/NUL");
        assert!(!root.join("sub").exists(), "and it must refuse before creating the parent directory");

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&store);
    }

    /// The stand-down rule itself, on **every** platform — the Windows aliasing leg above is the reason
    /// it exists, but the rule is not about aliasing and must not be pinned only where the alias is.
    /// A delete's justification is "this path is not in the checkpoint"; a skipped write means the
    /// checkpoint was not established, so that justification is unavailable for every delete in the plan.
    ///
    /// Uses a missing blob as the portable stand-in for a skipped write (the same device
    /// `a_missing_blob_is_skipped_while_the_rest_of_the_plan_applies` uses), so this runs on all three
    /// CI legs rather than only where a device name or trailing space means something.
    #[test]
    fn cpe_1823_a_skipped_write_holds_back_every_delete_in_the_plan() {
        const KEEP: &[u8] = b"a file the user added after the checkpoint";
        let root = scratch("cpe1823-standdown-root");
        let store = scratch("cpe1823-standdown-store");

        fs::write(root.join("restored.txt"), b"checkpoint content").unwrap();
        let checkpoint = scan_dir(&root.to_string_lossy()).unwrap();
        fs::create_dir_all(store.join("blobs")).unwrap(); // …but never write the blob: the write will skip

        fs::write(root.join("restored.txt"), b"edited since").unwrap();
        fs::write(root.join("added.txt"), KEEP).unwrap();
        let current = scan_dir(&root.to_string_lossy()).unwrap();
        let plan = plan_restore(&checkpoint, &current);
        assert_eq!(plan.len(), 2, "the premise: one Overwrite that will skip, and one Delete");

        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);

        assert_eq!(
            fs::read(root.join("added.txt")).ok().as_deref(),
            Some(KEEP),
            "a delete must not run while any checkpoint entry could not be restored: {report:?}"
        );
        assert_eq!(report.applied, 0, "nothing was applied: {report:?}");
        assert_eq!(
            held_back_as(&report, "added.txt"),
            Some(OpOutcome::SkippedByPlan),
            "and the held-back delete must be reported with its reason, never silently dropped — and \
             as the RETRYABLE kind, because the blocker here is a missing blob that can be put back \
             (CPE-1845): {report:?}"
        );

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&store);
    }

    /// **CPE-1823 round 3 — found by walking every manifest-derived value to its sink, not by review.**
    /// `safe_segments` is textual, and `sub/x.txt` is textually spotless; if `root/sub` is a junction
    /// leading out of the tree then both the write and the delete land outside it. `restore` had the
    /// resolved-containment check and these two — the ones the registered `checkpoint_revert` commands
    /// actually reach — did not. Third instance of the same asymmetry in this ticket.
    ///
    /// Both directions are staged, because `apply_delete` is the one that destroys something that was
    /// never ours: a `remove_file` through the link deletes the victim's file outright.
    #[test]
    fn cpe_1823_a_link_at_an_interior_component_never_redirects_a_revert_out_of_the_tree() {
        const VICTIM: &[u8] = b"a file that has nothing to do with this checkpoint";
        let root = scratch("cpe1823-revlink-root");
        let store = scratch("cpe1823-revlink-store");
        let outside = scratch("cpe1823-revlink-outside");
        fs::write(outside.join("victim.txt"), VICTIM).unwrap();
        fs::create_dir_all(store.join("blobs")).unwrap();
        fs::write(store.join("blobs").join("deadbeef"), b"attacker bytes").unwrap();

        let link = root.join("sub");
        if !crate::fsutil::make_dir_link(&outside, &link) {
            crate::skip_notice!(
                "[CPE-1823] SKIPPED the revert interior-link leg: this machine could not create a \
                 directory link at {} (no symlink privilege and no junction). NOTHING on this run \
                 covered a revert reaching THROUGH a link out of the reverted tree.",
                link.display()
            );
            return;
        }

        let mut checkpoint = Snapshot::new();
        checkpoint.insert("sub/planted.txt".to_string(), crate::restore_plan::FileState::new("deadbeef", 14));
        let plan = vec![
            RestoreAction { path: "sub/planted.txt".to_string(), op: RestoreOp::Create },
            RestoreAction { path: "sub/victim.txt".to_string(), op: RestoreOp::Delete },
        ];

        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);

        assert!(
            !outside.join("planted.txt").exists(),
            "HARM: the revert wrote through the planted link, outside the reverted tree: {report:?}"
        );
        assert_eq!(
            fs::read(outside.join("victim.txt")).ok().as_deref(),
            Some(VICTIM),
            "HARM: the revert DELETED a file outside the reverted tree through the planted link: {report:?}"
        );
        assert_eq!(report.applied, 0, "neither action may be counted applied: {report:?}");

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&store);
        let _ = fs::remove_dir_all(&outside);
    }


    /// **CPE-1823 round 5 — the stand-down keyed on SPELLING; the hazard is RESOLUTION.** `A.txt` and
    /// `a.txt` both pass `safe_segments` — neither is a device name, neither ends in a dot or space — so
    /// round 4's `checkpoint.keys().filter(…is_err())` filter is **empty** and nothing arms. On a
    /// case-insensitive volume the two are one file, so `plan_restore` emits Create `A.txt` +
    /// Delete `a.txt`, the Create lands *on* the user's file and the Delete removes it. Measured through
    /// the registered command before this fix:
    ///
    /// ```text
    /// CMD revert[case-alias] -> applied=2 skipped=0; a.txt = Err(NotFound)
    /// ```
    ///
    /// Byte-for-byte the round-3 harm with `A.txt` substituted for `a.txt `. No name-based rule can see
    /// it: both spellings are legal on **every** platform, so the problem is not the name.
    #[cfg(windows)]
    #[test]
    fn cpe_1823_a_case_aliased_entry_never_destroys_the_file_it_resolves_onto() {
        const LIVE: &[u8] = b"the user's only copy of their real content";
        let root = scratch("cpe1823-case-root");
        let store = scratch("cpe1823-case-store");
        fs::create_dir_all(store.join("blobs")).unwrap();
        fs::write(store.join("blobs").join("deadbeef"), b"attacker bytes").unwrap();

        fs::write(root.join("a.txt"), LIVE).unwrap();
        // Fixture is inert unless this volume really folds case: on a case-SENSITIVE mount `A.txt` is a
        // different file, the Create is legitimate, and this test certifies nothing.
        assert_eq!(
            fs::read(root.join("A.txt")).ok().as_deref(),
            Some(LIVE),
            "fixture is inert: `A.txt` must already address the user's `a.txt` on this volume, or this \
             test certifies nothing"
        );

        let mut checkpoint = Snapshot::new();
        checkpoint.insert("A.txt".to_string(), crate::restore_plan::FileState::new("deadbeef", 14));
        let current = scan_dir(&root.to_string_lossy()).unwrap();
        let plan = plan_restore(&checkpoint, &current);
        assert_eq!(plan.len(), 2, "the premise: a Create for `A.txt` and a Delete for `a.txt`");

        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);

        assert_eq!(
            fs::read(root.join("a.txt")).ok().as_deref(),
            Some(LIVE),
            "HARM: the revert destroyed the user's file via a case alias — report was {report:?}"
        );
        assert!(
            report.skipped.iter().any(|(p, _)| p == "A.txt"),
            "the aliasing write must be reported as skipped, not applied: {report:?}"
        );
        // **NOT retryable, and that correction is the point** (CPE-1845 review round 2). The refusal
        // that arms this hold-back is the Create-premise rule firing on a case fold: `A.txt` against a
        // live `a.txt` on a case-insensitive volume. The volume does not stop folding between runs, so
        // the refusal recurs forever and the user must not be told to run the revert again. This
        // assertion read `SkippedByPlan` in the first cut of CPE-1845 and would have locked the wrong
        // answer in — a later correct fix would have read as a regression here.
        assert_eq!(
            held_back_as(&report, "a.txt"),
            Some(OpOutcome::HeldBackByCheckpoint),
            "and the paired delete must be held back and said so, structurally (CPE-1845): {report:?}"
        );

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&store);
    }

    /// **CPE-1823 round 5 — a destructive NON-delete, structurally outside the delete stand-down.** A
    /// single `Create` for `PAYROLL.CSV` silently rewrites the user's `payroll.csv`. This is the shape
    /// `checkpoint_revert_one` produces for a checkpoint key the current scan does not hold under that
    /// spelling: a one-action plan, no delete to stand down, and `RestoreOp` has three variants where the
    /// round-4 stand-down guarded one. Measured before the fix:
    ///
    /// ```text
    /// R4-OVERWRITE report = RestoreReport { applied: 1, skipped: [] }
    ///              payroll.csv = "ATTACKER CHOSEN BYTES"
    /// ```
    ///
    /// The rule that catches it is about the *premise*, not the name: a `Create` means the plan read this
    /// path as absent. If something already answers to it, the plan's reading of the tree and the
    /// filesystem's resolution disagree — which is the aliasing signal itself.
    #[cfg(windows)]
    #[test]
    fn cpe_1823_a_lone_create_never_silently_overwrites_the_file_it_resolves_onto() {
        const LIVE: &[u8] = b"the user's real payroll";
        let root = scratch("cpe1823-overwrite-root");
        let store = scratch("cpe1823-overwrite-store");
        fs::create_dir_all(store.join("blobs")).unwrap();
        fs::write(store.join("blobs").join("deadbeef"), b"ATTACKER CHOSEN BYTES").unwrap();

        fs::write(root.join("payroll.csv"), LIVE).unwrap();
        assert_eq!(
            fs::read(root.join("PAYROLL.CSV")).ok().as_deref(),
            Some(LIVE),
            "fixture is inert: `PAYROLL.CSV` must already address the user's file on this volume, or \
             this test certifies nothing"
        );

        let mut checkpoint = Snapshot::new();
        checkpoint.insert("PAYROLL.CSV".to_string(), crate::restore_plan::FileState::new("deadbeef", 21));
        let plan = vec![RestoreAction { path: "PAYROLL.CSV".to_string(), op: RestoreOp::Create }];

        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);

        assert_eq!(
            fs::read(root.join("payroll.csv")).ok().as_deref(),
            Some(LIVE),
            "HARM: a lone Create rewrote the user's file under an aliased spelling — report was {report:?}"
        );
        assert_eq!(report.applied, 0, "nothing may be counted applied: {report:?}");
        assert_eq!(report.skipped.len(), 1, "and it must be reported: {report:?}");
        assert_eq!(report.skipped[0].0, "PAYROLL.CSV");

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&store);
    }

    /// **CPE-1823 round 5, and the reason the fix is at the RESOLUTION level rather than the spelling
    /// level: this leg has nothing to do with Windows.** A directory link inside the reverted tree gives
    /// one file two perfectly legal spellings on *every* platform — `sub/f.txt` and `alias/f.txt` — and
    /// `confined_to` admits both, correctly, because both resolve inside the tree.
    ///
    /// A delete's whole justification is "this path is not in the checkpoint". Asked of the *spelling*
    /// that is true; asked of the *file* it is false, and the checkpoint's own copy is deleted. This is
    /// the cherry-revert shape (`checkpoint_revert_one`), so there is no write in the plan to skip and
    /// nothing for a plan-outcome rule to notice.
    #[test]
    fn cpe_1823_a_delete_that_resolves_onto_a_checkpoint_entry_is_held_back() {
        const LIVE: &[u8] = b"a file the checkpoint holds under its other spelling";
        let root = scratch("cpe1823-aliasdel-root");
        let store = scratch("cpe1823-aliasdel-store");
        fs::create_dir_all(store.join("blobs")).unwrap();
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("sub").join("f.txt"), LIVE).unwrap();

        let link = root.join("alias");
        if !crate::fsutil::make_dir_link(&root.join("sub"), &link) {
            crate::skip_notice!(
                "[CPE-1823] SKIPPED the resolution-alias delete leg: this machine could not create a \
                 directory link at {} (no symlink privilege and no junction). NOTHING on this run \
                 covered a delete whose RESOLVED target is a file the checkpoint holds.",
                link.display()
            );
            return;
        }
        // Fixture is inert unless the two spellings really are one file.
        assert_eq!(
            fs::read(root.join("alias").join("f.txt")).ok().as_deref(),
            Some(LIVE),
            "fixture is inert: `alias/f.txt` must address the same file as `sub/f.txt`, or this test \
             certifies nothing"
        );

        let mut checkpoint = Snapshot::new();
        checkpoint.insert("sub/f.txt".to_string(), crate::restore_plan::FileState::new("deadbeef", 14));
        // Exactly what `restore_plan::revert_one("alias/f.txt", …)` yields: the checkpoint has no key of
        // that spelling, so the file reads as "added since the checkpoint" and is planned for deletion.
        let plan = vec![RestoreAction { path: "alias/f.txt".to_string(), op: RestoreOp::Delete }];

        let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);

        assert_eq!(
            fs::read(root.join("sub").join("f.txt")).ok().as_deref(),
            Some(LIVE),
            "HARM: the revert deleted a file the checkpoint holds, reached under its other spelling — \
             report was {report:?}"
        );
        assert_eq!(report.applied, 0, "nothing may be counted applied: {report:?}");
        assert_eq!(
            held_back_as(&report, "alias/f.txt"),
            Some(OpOutcome::HeldBackByCheckpoint),
            "and the held-back delete must be reported with its reason, structurally (CPE-1845) — the \
             NOT-retryable kind, since the two spellings resolve to one file on every re-run: {report:?}"
        );

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
        fs::write(root.join("keep.txt"), b"unchanged since the checkpoint").unwrap();

        // Both deleted files are new-since-checkpoint. `keep.txt` is in the checkpoint and unchanged, so
        // it contributes no action — it is here solely so the checkpoint is not **zero-entry**, which
        // CPE-1847 stands every delete down on. This test was written with `Snapshot::new()`, i.e. with
        // the exact shape that ticket is about; keeping it that way would have made it a test of the
        // attack rather than of delete ordering.
        let mut checkpoint = Snapshot::new();
        checkpoint.insert(
            "keep.txt".to_string(),
            crate::restore_plan::FileState::new("deadbeef", 30),
        );
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

    // ============================================================================================
    // CPE-1870 — the committed triggered-race harness CPE-1846 acknowledged it owed
    // ============================================================================================
    //
    // CPE-1846 measured this attack with a harness that was never committed, and its own Work Log
    // records why that is a gap: the auditor "could not verify my self-inflict fix — only my account
    // of it". Worse, that first uncommitted harness reported **2,681 escapes** which were entirely its
    // own doing — its per-round setup wrote fixture content with `fs::write`, which *follows* a link
    // the racer had already planted, so it poisoned its own victims before the code under test ran.
    //
    // This one is built so that failure is **structurally impossible**:
    //
    //  1. **The racer only ever unlinks and symlinks. It never writes a byte of content.** So no bytes
    //     the harness itself produced can ever be mistaken for an escape.
    //  2. **Every victim is asserted byte-pristine immediately before the call**, as a hard panic. A
    //     victim already damaged by setup fails the run instead of being counted as a finding.
    //  3. **The racer is armed on an observed effect of the code under test — the first restored byte —
    //     never on "start".** CPE-1823 planted from the start of the run: the first plant landed before
    //     the pre-pass had finished judging, the pre-pass refused, the abort was total and *no write
    //     ever happened*. 23,340 plants against zero writes is a clean-looking zero that proves nothing.
    //  4. **The denominator is published and asserted.** A run that wrote nothing, or that planted
    //     nothing live, FAILS — it is not allowed to report "0 escapes". Two false negatives on
    //     CPE-1823 were exactly "N plants, 0 escapes" with the count of entries actually written left
    //     out of the record.
    //  5. **A plant counts only if the name really holds a link naming the victim**, checked with
    //     `read_link` — structurally, never by reading *through* the link, which would race the very
    //     write under test and could mis-file a successful escape as "not a live plant".
    //
    // `#[ignore]`d, and that is deliberate rather than shyness: it takes tens of seconds, it needs the
    // symlink privilege, and it is a **probabilistic attack, not a pin**. The deterministic pins for
    // this property are the `cpe_1846_*` tests; this is the instrument that says how hard they were
    // pushed. Run it with
    // `cargo test --release -- --ignored --nocapture cpe_1870_triggered_race`.
    //
    // **Its positive control is a manual sabotage, and a zero from it is worthless without one.**
    // Swap `crate::fsutil::copy_file_onto_no_follow(&blob, &target)` in `apply_write` for
    // `fs::copy(&blob, &target)` and re-run: the numbers in CPE-1870's Work Log show that turns this
    // harness's zero into escapes on the same machine, in the same run shape.

    /// Plant a symlink at `at` pointing to `victim`, and report whether the name really holds one.
    ///
    /// Removes first, because the point is to replace a name the restore is about to write to. Both
    /// steps are allowed to fail — the writer may be holding the name at this instant — and a failure
    /// simply is not counted.
    fn cpe1870_plant(victim: &Path, at: &Path) -> bool {
        let _ = fs::remove_file(at);
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(victim, at).is_ok();
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(victim, at).is_ok();
        #[cfg(not(any(windows, unix)))]
        let made = false;
        made && fs::read_link(at).ok().as_deref() == Some(victim)
    }

    /// What the race measured. Every field is reported, including the denominators.
    #[derive(Default, Debug)]
    struct RaceTally {
        rounds: u64,
        live_plants: u64,
        entries_written: u64,
        writes_through: u64,
    }

    /// The racer thread: waits for the first restored byte at `trigger`, then plants links at the
    /// remaining target names until told to stop. Never writes content.
    fn cpe1870_spawn_racer(
        targets: Vec<PathBuf>,
        victims: Vec<PathBuf>,
        trigger: PathBuf,
        restored: Vec<u8>,
    ) -> (
        std::sync::Arc<std::sync::atomic::AtomicBool>,
        std::sync::Arc<std::sync::atomic::AtomicU64>,
        std::thread::JoinHandle<()>,
    ) {
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        use std::sync::Arc;
        let stop = Arc::new(AtomicBool::new(false));
        let plants = Arc::new(AtomicU64::new(0));
        let (s, p) = (stop.clone(), plants.clone());
        let handle = std::thread::spawn(move || {
            // ARM ON AN OBSERVED EFFECT OF THE CODE UNDER TEST. Not a sleep, not "start": the first
            // restored byte is the signal that the judging pass is over and the write window is open.
            while !s.load(Ordering::Relaxed) {
                if fs::read(&trigger).ok().as_deref() == Some(restored.as_slice()) {
                    break;
                }
                std::thread::yield_now();
            }
            let mut i = 0usize;
            while !s.load(Ordering::Relaxed) {
                // Never the trigger name itself (index 0) — poisoning the arming signal would arm the
                // racer against its own plant rather than against the restore.
                let at = &targets[1 + (i % (targets.len() - 1))];
                let victim = &victims[i % victims.len()];
                if cpe1870_plant(victim, at) {
                    p.fetch_add(1, Ordering::Relaxed);
                    // FLICKER, and it is what makes this a measurement rather than a wall. Leaving the
                    // link in place turns every name hostile, so the restore refuses ~99% of its
                    // entries and the run reports "0 escapes" against a denominator of a couple of
                    // hundred writes — a strong-looking attack that mostly never enters the window it
                    // is aiming at. Withdrawing the link again (an unlink, never a write of content)
                    // keeps the names writable, so the restore keeps writing, and each plant becomes a
                    // brief hostile pulse exactly where the check-then-write window is. It only ever
                    // removes a name it has just confirmed holds its OWN link, so a legitimately
                    // restored file is never deleted out from under the denominator.
                    if fs::symlink_metadata(at).is_ok_and(|m| m.file_type().is_symlink()) {
                        let _ = fs::remove_file(at);
                    }
                }
                i += 1;
            }
        });
        (stop, plants, handle)
    }

    /// Assert every victim still holds exactly `pristine`, and return how many did not. Called
    /// immediately BEFORE the run (where any failure means the fixture poisoned itself, a hard panic)
    /// and again after (where a difference is the finding).
    fn cpe1870_victims_intact(victims: &[PathBuf], pristine: &[u8], before: bool) -> u64 {
        let mut damaged = 0;
        for v in victims {
            let now = fs::read(v).unwrap_or_default();
            if now != pristine {
                assert!(
                    !before,
                    "FIXTURE POISONED: {} was already not pristine before the restore ran — the \
                     harness damaged its own victim and any 'escape' it reports would be its own \
                     (this is the CPE-1846 2,681-false-escape shape)",
                    v.display()
                );
                damaged += 1;
            }
        }
        damaged
    }

    /// Stage `count` victims OUTSIDE the restored tree with `create_new`, so staging itself can never
    /// follow a link the racer left behind.
    fn cpe1870_victims(outside: &Path, count: usize, pristine: &[u8]) -> Vec<PathBuf> {
        (0..count)
            .map(|i| {
                let v = outside.join(format!("victim{i:03}.txt"));
                let mut f = fs::OpenOptions::new().write(true).create_new(true).open(&v).unwrap();
                std::io::Write::write_all(&mut f, pristine).unwrap();
                v
            })
            .collect()
    }

    /// The verdict, with the denominators asserted before the zero is believed.
    fn cpe1870_verdict(sink: &str, tally: &RaceTally) {
        let _ = std::io::Write::write_all(
            &mut std::io::stderr(),
            format!(
                "\n[CPE-1870] {sink}: {} rounds, {} live plants, {} ENTRIES WRITTEN, {} writes through\n",
                tally.rounds, tally.live_plants, tally.entries_written, tally.writes_through
            )
            .as_bytes(),
        );
        // THE DENOMINATORS, ASSERTED. A run that planted nothing live, or wrote nothing, has not
        // exercised the window at all and must not be allowed to report a zero — that is precisely the
        // shape of both CPE-1823 false negatives.
        assert!(
            tally.live_plants > 0,
            "{sink}: NOT A RESULT — zero live plants, so nothing raced the write window. If this \
             machine withholds the symlink privilege, this harness cannot measure anything here."
        );
        assert!(
            tally.entries_written > 0,
            "{sink}: NOT A RESULT — zero entries written, so the restore never entered its write \
             window and a zero here proves nothing (CPE-1823's own false negative)."
        );
        assert_eq!(
            tally.writes_through, 0,
            "{sink}: a manifest entry's bytes landed on a file OUTSIDE the restored tree, against {} \
             entries actually written under {} live plants",
            tally.entries_written, tally.live_plants
        );
    }

    /// The SHIPPING sink — `execute_restore`, reached from `checkpoint_revert` /
    /// `checkpoint_revert_one`. CPE-1846's own race drove `snapshot_capture::restore`, which has no
    /// production caller; its auditor's headline finding came from driving this one instead.
    #[test]
    #[ignore = "CPE-1870 race harness: tens of seconds, needs the symlink privilege, probabilistic"]
    fn cpe_1870_triggered_race_against_the_shipping_revert_sink() {
        use std::sync::atomic::Ordering;
        const ROUNDS: u64 = 10;
        const ENTRIES: usize = 3000;
        const VICTIMS: usize = 64;
        const PRISTINE: &[u8] = b"PRISTINE";
        const RESTORED: &[u8] = b"RESTORED CONTENT";

        let mut tally = RaceTally::default();
        for round in 0..ROUNDS {
            let root = scratch(&format!("cpe1870-race-revert-root-{round}"));
            let store = scratch(&format!("cpe1870-race-revert-store-{round}"));
            let outside = scratch(&format!("cpe1870-race-revert-outside-{round}"));

            let targets: Vec<PathBuf> = (0..ENTRIES).map(|i| root.join(format!("t{i:05}.txt"))).collect();
            for t in &targets {
                fs::write(t, RESTORED).unwrap();
            }
            let checkpoint = scan_dir(&root.to_string_lossy()).unwrap();
            let names: Vec<String> = (0..ENTRIES).map(|i| format!("t{i:05}.txt")).collect();
            let contents: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), RESTORED)).collect();
            write_blobs(&store, &checkpoint, &contents);
            for t in &targets {
                fs::write(t, b"edited").unwrap();
            }
            let current = scan_dir(&root.to_string_lossy()).unwrap();
            let plan = plan_restore(&checkpoint, &current);
            assert_eq!(plan.len(), ENTRIES, "fixture is inert: the plan must name every entry");

            let victims = cpe1870_victims(&outside, VICTIMS, PRISTINE);
            cpe1870_victims_intact(&victims, PRISTINE, true);

            let trigger = targets[0].clone();
            let (stop, plants, handle) =
                cpe1870_spawn_racer(targets.clone(), victims.clone(), trigger, RESTORED.to_vec());

            let report = execute_restore(&plan, &root.to_string_lossy(), &store.to_string_lossy(), &checkpoint);

            stop.store(true, Ordering::Relaxed);
            handle.join().unwrap();

            let through = cpe1870_victims_intact(&victims, PRISTINE, false);
            tally.rounds += 1;
            tally.live_plants += plants.load(Ordering::Relaxed);
            tally.entries_written += report.applied as u64;
            tally.writes_through += through;
            let _ = std::io::Write::write_all(
                &mut std::io::stderr(),
                format!(
                    "[CPE-1870] execute_restore round {round}: live plants {}, applied {} (skipped {}), \
                     writes through {through}\n",
                    plants.load(Ordering::Relaxed),
                    report.applied,
                    report.skipped.len()
                )
                .as_bytes(),
            );
        }

        cpe1870_verdict("execute_restore (the shipping sink)", &tally);
    }

    /// The other sink, `snapshot_capture::restore`, kept because the shared copier's guard lives in
    /// both and a change to it has to be shown safe at both.
    #[test]
    #[ignore = "CPE-1870 race harness: tens of seconds, needs the symlink privilege, probabilistic"]
    fn cpe_1870_triggered_race_against_snapshot_restore() {
        use std::sync::atomic::Ordering;
        const ROUNDS: u64 = 10;
        const ENTRIES: usize = 1000;
        const VICTIMS: usize = 64;
        const PRISTINE: &[u8] = b"PRISTINE";
        const RESTORED: &[u8] = b"RESTORED CONTENT";

        let mut tally = RaceTally::default();
        for round in 0..ROUNDS {
            let src = scratch(&format!("cpe1870-race-snap-src-{round}"));
            let store = scratch(&format!("cpe1870-race-snap-store-{round}"));
            let dest = scratch(&format!("cpe1870-race-snap-dest-{round}"));
            let outside = scratch(&format!("cpe1870-race-snap-outside-{round}"));

            for i in 0..ENTRIES {
                fs::write(src.join(format!("t{i:05}.txt")), RESTORED).unwrap();
            }
            let out = crate::snapshot_capture::capture(
                &src.to_string_lossy(),
                &store.to_string_lossy(),
                &crate::snapshot::CaptureBudget::default(),
            )
            .expect("capture must succeed");

            let targets: Vec<PathBuf> = (0..ENTRIES).map(|i| dest.join(format!("t{i:05}.txt"))).collect();
            for t in &targets {
                fs::write(t, b"edited").unwrap();
            }
            let victims = cpe1870_victims(&outside, VICTIMS, PRISTINE);
            cpe1870_victims_intact(&victims, PRISTINE, true);

            let trigger = targets[0].clone();
            let (stop, plants, handle) =
                cpe1870_spawn_racer(targets.clone(), victims.clone(), trigger, RESTORED.to_vec());

            let outcome = crate::snapshot_capture::restore(
                &store.to_string_lossy(),
                &out.manifest_id,
                &dest.to_string_lossy(),
            );

            stop.store(true, Ordering::Relaxed);
            handle.join().unwrap();

            // The denominator for THIS sink cannot come from a report — `restore` returns
            // `Result<(), String>`. It is counted off the filesystem: names that are still ordinary
            // files (not the racer's links) holding the restored bytes.
            let written = targets
                .iter()
                .filter(|t| {
                    fs::symlink_metadata(t).is_ok_and(|m| m.file_type().is_file())
                        && fs::read(t).ok().as_deref() == Some(RESTORED)
                })
                .count() as u64;

            let through = cpe1870_victims_intact(&victims, PRISTINE, false);
            tally.rounds += 1;
            tally.live_plants += plants.load(Ordering::Relaxed);
            tally.entries_written += written;
            tally.writes_through += through;
            let _ = std::io::Write::write_all(
                &mut std::io::stderr(),
                format!(
                    "[CPE-1870] snapshot restore round {round}: live plants {}, entries written \
                     {written}, writes through {through} (outcome {})\n",
                    plants.load(Ordering::Relaxed),
                    if outcome.is_ok() { "Ok" } else { "Err" }
                )
                .as_bytes(),
            );
        }

        cpe1870_verdict("snapshot_capture::restore", &tally);
    }

    /// **Handle pinning, re-proved for CPE-1870 rather than inherited from CPE-1846's account of it.**
    /// A link is planted at the destination name *strictly after* the copier has opened it, while a
    /// large blob is still streaming through the handle. Nothing after the open re-opens by path, so
    /// the bytes must land in the pinned object and the victim must be untouched.
    #[test]
    #[ignore = "CPE-1870 race harness: needs the symlink privilege, probabilistic"]
    fn cpe_1870_a_link_planted_after_the_open_cannot_redirect_the_copy() {
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        use std::sync::Arc;
        const VICTIM: &[u8] = b"SAFE!!";
        let d = scratch("cpe1870-postopen");
        let outside = scratch("cpe1870-postopen-outside");
        let blob = d.join("blob.bin");
        fs::write(&blob, vec![b'B'; 64 * 1024 * 1024]).unwrap();
        let victim = outside.join("victim.txt");
        {
            let mut f = fs::OpenOptions::new().write(true).create_new(true).open(&victim).unwrap();
            std::io::Write::write_all(&mut f, VICTIM).unwrap();
        }
        let dst = d.join("target.bin");
        fs::write(&dst, b"OLD").unwrap();

        // The plant starts only once the copy is demonstrably under way: the destination's length has
        // grown past the old content, which can only happen after the open and after the first write.
        let stop = Arc::new(AtomicBool::new(false));
        let plants = Arc::new(AtomicU64::new(0));
        let (s, p, dc, vc) = (stop.clone(), plants.clone(), dst.clone(), victim.clone());
        let racer = std::thread::spawn(move || {
            while !s.load(Ordering::Relaxed) {
                if fs::metadata(&dc).map(|m| m.len()).unwrap_or(0) > 4 * 1024 * 1024 {
                    break;
                }
                std::thread::yield_now();
            }
            while !s.load(Ordering::Relaxed) {
                if cpe1870_plant(&vc, &dc) {
                    p.fetch_add(1, Ordering::Relaxed);
                }
                i_yield();
            }
        });

        let outcome = crate::fsutil::copy_file_onto_no_follow(&blob, &dst);
        stop.store(true, Ordering::Relaxed);
        racer.join().unwrap();

        let landed = plants.load(Ordering::Relaxed);
        let _ = std::io::Write::write_all(
            &mut std::io::stderr(),
            format!("[CPE-1870] post-open plants: {landed}, copy returned {outcome:?}\n").as_bytes(),
        );
        assert!(
            landed > 0,
            "NOT A RESULT — no link was planted after the open, so nothing tested handle pinning"
        );
        assert_eq!(
            fs::read(&victim).ok().as_deref(),
            Some(VICTIM),
            "HARM: a link planted AFTER the open redirected the copy onto a file outside the tree — \
             the write re-resolved the destination by path"
        );
        assert_eq!(outcome, Ok(64 * 1024 * 1024), "the copy must complete into the pinned object");
    }

    fn i_yield() {
        std::thread::yield_now();
    }
}
