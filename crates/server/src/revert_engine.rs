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
    // which cleanups were held back and why, and re-running after fixing the manifest performs them.
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
    //   spelling will change on this platform — which is why this branch's message deliberately does
    //   **not** say "re-run once that is resolved" the way the `report.skipped` branch below does. The
    //   `report.skipped` branch is the transient one (a locked file, a missing blob): there a
    //   500-delete revert with one locked file holds back all 500, and re-running after fixing it does
    //   perform them. `RevertOutcome::from_report` carries each path and reason, so the UI has what it
    //   needs — but it can only tell a deliberate hold-back from a failure by string-matching
    //   `"not deleted:"`, which is a structural gap in `OpResult` and its own ticket.
    // - **Finer granularity is not available *from the spelling*, which is why round 5 stopped asking
    //   the spelling.** Pairing each delete with the write that would have covered it is precisely what
    //   the aliasing case makes invisible — `a.txt ` and `a.txt` look like different paths, which is the
    //   bug. The rule that *can* tell them apart is the per-delete resolution check further down: it
    //   asks the filesystem which file a delete addresses and holds back only that one. This blanket
    //   rule stays as the coarse backstop for shapes resolution cannot answer — a device name resolves
    //   to `\\?\NUL`, which is not a file any checkpoint entry can collide with — so the two are
    //   complementary, not redundant.
    //
    // **One other destructive shape a planted manifest still has, and it is NOT the widest.** An emptied
    // `"files": {}` turns a whole-tree revert into "delete every file", with no writes to stand down and
    // no error; measured, an empty checkpoint against a five-file tree gives `applied: 5, skipped: []`
    // and no survivors. Bigger blast radius than anything else here — but the widest shape is measured by
    // *reach*, not by count, and on reach this one is the narrower of the two: it needs the user to
    // confirm a whole-tree revert whose `checkpoint_preview_revert` says "delete 5 files, restore 0", and
    // an empty checkpoint is a legal capture of an empty folder, so refusing it would refuse a real one.
    //
    // The wider shape was the **resolution collision** below: one planted key, one live file, no reliance
    // on the user confirming a mass delete, reachable through *cherry*-revert where the preview shows a
    // single file — and it destroys a file the attacker names. It is closed in this function as of round
    // 5. (Round 4's comment here asserted the opposite ranking, which was wrong on both halves: the
    // alias case is not "not this one", and it was not then closed. Recorded so the correction is
    // visible rather than quietly swapped.) The empty-`files` shape is its own ticket.
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
    let hold = if !unrestorable.is_empty() {
        let named: Vec<String> =
            unrestorable.iter().take(NAMED_CAUSES).map(|k| format!("{k:?}")).collect();
        let more = unrestorable.len().saturating_sub(named.len());
        Some(format!(
            "not deleted: {} of this checkpoint's entries cannot be restored on this platform ({}{}), \
             so \"this file is not in the checkpoint\" cannot be trusted — deleting it may destroy a \
             file the checkpoint does hold, under a name spelled differently here",
            unrestorable.len(),
            named.join(", "),
            if more > 0 { format!(", and {more} more") } else { String::new() }
        ))
    } else if !report.skipped.is_empty() {
        // Name the entries, not just the count. Both lines land in the same `skipped` list so a UI that
        // renders all of it makes the cause discoverable — but a user looking at one held-back file
        // should not have to scan the rest of the list to find out what blocked it.
        let held = report.skipped.len();
        let named: Vec<&str> =
            report.skipped.iter().take(NAMED_CAUSES).map(|(path, _)| path.as_str()).collect();
        let more = held.saturating_sub(named.len());
        Some(format!(
            "not deleted: {held} checkpoint entr{} could not be restored ({}{}), so \"this file is not \
             in the checkpoint\" cannot be trusted — re-run once that is resolved and this cleanup will \
             apply",
            if held == 1 { "y" } else { "ies" },
            named.join(", "),
            if more > 0 { format!(", and {more} more") } else { String::new() }
        ))
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
                    report.skipped.push((
                        action.path.clone(),
                        format!(
                            "not deleted: this path resolves to the same file as the checkpoint entry \
                             {key:?}, so \"this file is not in the checkpoint\" is true of the spelling \
                             but false of the file — deleting it would destroy content the checkpoint \
                             holds"
                        ),
                    ));
                    continue;
                }
                match apply_delete(action, dest_root_path) {
                    Ok(()) => report.applied += 1,
                    Err(reason) => report.skipped.push((action.path.clone(), reason)),
                }
            }
        }
        Some(reason) => {
            for action in &deletes {
                report.skipped.push((action.path.clone(), reason.clone()));
            }
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
) -> Result<(), String> {
    let target = safe_target(dest_root, &action.path)?;
    let Some(state) = checkpoint.get(&action.path) else {
        return Err("no checkpoint entry for this path".to_string());
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
    let blob = crate::snapshot_capture::blob_source(blobs_dir, &state.hash)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
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
    // can invalidate before the write it is protecting. Nothing may sit between this and `fs::copy`.
    //
    // What remains is the final-component link swap in the gap between this check and the copy —
    // narrow, and **reducible**, not irreducible: this crate already ships the pattern that closes it
    // (`batch_media`'s "never follow a link at the final component" open, `O_NOFOLLOW` on Unix and
    // `FILE_FLAG_OPEN_REPARSE_POINT` on Windows, with no libc dependency, already used by
    // `batch_execute`). Adopting it means opening the target and writing through the handle instead of
    // `fs::copy`, which changes attribute-preserving behaviour on Windows — its own ticket, deliberately
    // not folded in here.
    if action.op == RestoreOp::Create && fs::symlink_metadata(&target).is_ok() {
        return Err(format!(
            "this entry restores a file the plan read as absent, but {} already answers to that name — \
             the spelling resolves to a file the scan did not see under it, and writing would overwrite \
             content nothing in this plan accounted for",
            target.display()
        ));
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
        assert!(
            report.skipped.iter().any(|(p, why)| p == "a.txt" && why.contains("not deleted")),
            "the paired delete must be held back and said so: {report:?}"
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
        assert!(
            report.skipped.iter().any(|(p, why)| p == "added.txt" && why.contains("not deleted")),
            "and the held-back delete must be reported with its reason, never silently dropped: {report:?}"
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
        assert!(
            report.skipped.iter().any(|(p, why)| p == "a.txt" && why.contains("not deleted")),
            "and the paired delete must be held back and said so: {report:?}"
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
        assert!(
            report.skipped.iter().any(|(p, why)| p == "alias/f.txt" && why.contains("not deleted")),
            "and the held-back delete must be reported with its reason: {report:?}"
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
