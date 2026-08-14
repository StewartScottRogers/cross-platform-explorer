//! Batch **execute** runner (CPE-1084, epic CPE-723): the real-filesystem counterpart to
//! [`batch_media::plan`] and [`batch_transform::apply_ops`]. `plan` only computes *where* each output
//! goes (pure, no I/O); `apply_ops` only transforms bytes→bytes (pure, no I/O). This module is the glue
//! that actually reads each planned input off disk, runs the job's ops over its bytes, and writes the
//! result to the planned output — **skip-on-error, never fatal**, mirroring the `list_dir` /
//! `revert_engine` ethos: one bad file must never abort the whole batch.
//!
//! Non-destructive **only when `job.non_destructive` is true**: `batch_media::plan` then guarantees
//! every output path differs from its input, so this module only ever reads an input and writes to a
//! *different* planned output. When the caller sets `job.non_destructive = false`, `plan` drops that
//! guarantee — for an op combo with no dedicated output-renaming suffix (a lone Compress, Strip
//! metadata, or Watermark) the planned output can equal the input, and this module then **overwrites
//! the source file's bytes in place**, same as any other planned write (CPE-1590).
//!
//! **The engine itself refuses that, not just the UI (CPE-1599).** Before touching any bytes,
//! [`execute_plan_walk`] scans `items` for one whose planned output is the same file as its input per
//! [`crate::batch_media::same_file`] — **not** raw string equality (CPE-1613): `plan` lower-cases a `Convert` target's
//! extension, so a planned output can be textually different yet the SAME file on a case-insensitive
//! filesystem, and that must be caught too. If it finds one and `job.confirmed_overwrite` is not set, it
//! returns `Err` and writes nothing at all — a clean, specific refusal, not a panic and not a silent
//! no-op that quietly skips the dangerous files. This closes the gap a purely frontend confirm
//! (`BatchMediaDialog.svelte`'s "Overwrite N files" panel) can't: a devtools call, a future
//! automation/agent feature, or a new UI surface that invokes `batch_media_execute_stream` directly no
//! longer gets an in-place overwrite for free just by setting `non_destructive: false` — it must ALSO
//! carry `confirmed_overwrite: true`, which should only ever be set by that one confirm panel after
//! showing its warning. See [`BatchJob::confirmed_overwrite`]'s doc for the ownership of that promise.
//!
//! **Broadened beyond self-overwrite (CPE-1623).** The refusal used to fire only for `output == input`
//! (per item, comparing a planned item against its OWN input). A planned output that instead resolves
//! onto some OTHER real file on disk — one never submitted as any of this batch's own inputs — is just as
//! destructive to overwrite unconfirmed: an ordinary rename landing on an unrelated pre-existing file's
//! name, or the directory-traversal case a security audit demonstrated. [`any_in_place`] catches both; see
//! its own doc. **This alone was NOT the "second, independent layer" an earlier cut of this doc claimed
//! it was** — see the containment paragraph immediately below for why, and what actually closes that gap.
//!
//! **The engine — not just `batch_media::plan()` — is the actual containment enforcement point (IPC-bypass
//! follow-up, PR #828).** `PlannedItem` is a plain public struct: `Serialize`/`Deserialize`, no invariants
//! of its own, nothing that enforces "this came from `plan()`". Before this fix, EVERY containment check
//! (a Rename template can't walk `output` outside `input`'s own folder) lived entirely inside
//! `batch_media::validate()`/`plan()` — and `batch_media_execute_stream`, the Tauri command backing this
//! module, took `items: Vec<PlannedItem>` straight off the IPC wire. [`is_foreign_overwrite`]'s only
//! question was "does something already exist at this output?" — never "does this output stay inside the
//! input's own folder?" — so a caller that hand-built a `PlannedItem` (skipping `plan()` entirely: a
//! devtools call, a compromised webview, a future automation surface) pointing `output` at a **path
//! nothing occupied yet** sailed straight through, `confirmed_overwrite` or not: `is_foreign_overwrite`
//! returns `false` the instant `Path::is_file()` is false, and [`execute_one`] then does an unconditional
//! `fs::write`. Demonstrated: hand-build a `PlannedItem` with `output` pointing outside `input`'s
//! directory, skip `plan()`, call [`execute_plan`] → `Ok(BatchReport { written: 1, .. })`, a file created
//! at the arbitrary path — and, with a real file already sitting there plus caller-supplied
//! `confirmed_overwrite: true`, its bytes replaced. [`execute_plan_walk`] now re-derives containment
//! itself, per item, before ANY byte is read or written — using
//! [`crate::batch_media::classify_output_containment`],
//! the identical check `plan()` uses (not a fresh, potentially-drifting reimplementation) — and refuses the
//! whole batch, nothing written, if any item's output would leave its own input's folder. This runs
//! **regardless of `confirmed_overwrite`**: that flag only ever authorises overwriting the user's OWN
//! input in place (or a foreign file the user's own plan happened to land on) — it was never meant to, and
//! must not, license writing to a folder the user never selected. The engine is now the actual enforcement
//! point end-to-end, not just this one IPC command's two former layers.
//!
//! **TOCTOU: the write-time decision is made on the HANDLE the bytes go through (CPE-1624, corrected by
//! the PR #848 security audit).** The checks above run once per batch, before any bytes are touched, so
//! for item N they are an answer about a filesystem that no longer exists. An earlier cut of this fix
//! re-asked them *by path* just before calling [`execute_one`] — and the audit executed straight through
//! it: the path check passed in 25 µs, `execute_one` then spent 528 ms reading and transforming the
//! image, and only then wrote, so an unprivileged `mklink /H` landing anywhere in that window redirected
//! the write onto a file **outside the selected folder** (`written = 1`, `skipped = []`, victim
//! 35 → 17120 bytes, `confirmed_overwrite` false and never consulted). Re-checking a *path* cannot fix
//! that: a path is a name, and the whole attack is substituting what the name denotes.
//!
//! So there is deliberately **no path-based per-item pre-check** here to be defeated. The transform runs
//! first (touching nothing), and then [`crate::batch_media::open_output_verified`] opens the output
//! atomically, refuses to follow any link at it, settles identity/hard-links/directoriness **on that
//! handle** against a freshly-scanned census, and the bytes are written **through that same handle**. See
//! [`crate::batch_media::VerifiedOutput`] for the mechanism, what it closes outright, and the one
//! microsecond-wide residual it does not.
//!
//! A refusal at that point **skips the item with a reported reason** rather than writing it, and rather
//! than aborting a batch that has already written files — the up-front scan keeps its all-or-nothing
//! refusal for a plan that arrives wrong.

use std::fs;
use std::path::Path;

use crate::batch_media::{
    classify_output_containment, path_key, same_file, BatchJob, Containment, ParentCache, PathKey,
    PlannedItem,
};
use crate::batch_transform;
use crate::model::OpResult;

/// Outcome of running a plan: how many items were written successfully, and which were skipped (with a
/// short human reason each). Skipped items are never fatal to the batch.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BatchReport {
    pub written: usize,
    pub skipped: Vec<(String /* input */, String /* why */)>,
}

/// The batch's own input identities, computed **once per batch** — the "against what" side of
/// [`is_foreign_overwrite`]'s foreign-file question, as a `HashSet<PathKey>` rather than the
/// `&[PlannedItem]` slice it used to take (CPE-1667).
///
/// **Why this exists.** `is_foreign_overwrite` used to answer "is the output one of this batch's own
/// inputs?" by scanning `items` pairwise with [`same_file`] — `O(n)` [`same_file`] calls per item, each
/// building its own throwaway `ParentCache` and paying up to two `canonicalize` syscalls, so the whole
/// question cost `O(n)` *per item checked*. Called from [`execute_one`], that scan ran **inside** the
/// verify-to-write window ([`crate::batch_media::VerifiedOutput`]'s doc), so a bigger batch meant a wider
/// window on every one of its own items — the opposite of what a security-critical window wants. Building
/// the set once, up front (outside any window: nothing has been opened or written yet), turns each
/// in-window check into a single `HashSet` lookup — `O(1)` regardless of batch size.
///
/// **Where these keys are CONSUMED, and why a stale memo is acceptable here — read this before reusing
/// the pattern (PR #856 review).** This file's whole history is memo-staleness findings, and
/// [`crate::batch_media::ParentCache`]'s own doc codifies the opposing rule: *the write-time authority
/// builds its own cache, per item, and never receives one*. This set breaks that rule deliberately, so the
/// reasoning is recorded rather than left in a reviewer's head. Item 300 consults keys computed ~n
/// transforms earlier — but the set only ever answers "was this name one of the batch's **own inputs**",
/// which is a question about the *batch*, not about what is on disk right now; the in-place arm is still
/// computed fresh at write time; and `open_output_verified` re-checks links, reparse tags, containment and
/// link count on a fresh cache **before** this is consulted. The only reach an attacker gains over the old
/// code is that having transiently aliased a batch input onto an in-folder victim, they may now revert the
/// alias instead of having to leave it in place. A future change that makes this set answer anything about
/// current on-disk state must move it back inside the per-item authority.
///
/// See
/// `cpe_1667_is_foreign_overwrite_costs_a_bounded_number_of_canonicalize_calls_regardless_of_batch_size`
/// (deterministic) and `cpe_1667_the_not_created_branch_window_stays_narrow_across_a_chained_batch`
/// (wall-clock) for the measurement.
fn input_path_keys(items: &[PlannedItem]) -> std::collections::HashSet<PathKey> {
    let mut cache = ParentCache::new();
    items.iter().map(|it| path_key(&it.input, &mut cache)).collect()
}

/// Lazily builds, and memoizes, [`input_path_keys`] on first use (CPE-1674).
///
/// **Why this exists.** CPE-1667 got the `n`-canonicalize-call cost of building this set out of the
/// per-item verify-to-write window, but the PR #856 re-review found it still ran the build
/// **unconditionally**, before the batch's first item is even touched — including when
/// `job.confirmed_overwrite` is `true`, where nothing ever consults it (both call sites below are gated on
/// `!job.confirmed_overwrite`, directly or via [`is_foreign_overwrite`]'s `!created` guard in
/// [`execute_one`]). On a large batch over a network share that delays **time to first written file**,
/// cutting against this repo's streaming-liveness convention (`docs/design/STREAMING.md`) for no reason —
/// the confirmed-overwrite path was going to write item 0 immediately regardless.
///
/// Wrapping the memo in a [`std::cell::OnceCell`] fixes that without touching `input_path_keys` or
/// [`is_foreign_overwrite`]'s own signature (both are still exercised directly, unchanged, by
/// `cpe_1667_is_foreign_overwrite_costs_a_bounded_number_of_canonicalize_calls_regardless_of_batch_size`):
/// a caller that never calls [`LazyInputKeys::get`] never pays for the build at all, and a caller that does
/// pays for it exactly once no matter how many times it asks. For `confirmed_overwrite = true`, `.get()` is
/// never called anywhere in this module, so the build genuinely never runs — not "runs cheaply", zero calls
/// (measured in `cpe_1674_a_confirmed_overwrite_batch_never_builds_the_input_key_set`). For
/// `confirmed_overwrite = false`, the up-front all-or-nothing refusal scan (below, in
/// [`execute_plan_walk`]) still has to see every item before the loop starts — that part of the shape is
/// unchanged from CPE-1667 and is not what this ticket is about — so `.get()` there builds it once, still
/// before any byte is written, and every later call (including the one inside [`execute_one`]'s window)
/// reuses the same cached set.
struct LazyInputKeys<'a> {
    items: &'a [PlannedItem],
    cell: std::cell::OnceCell<std::collections::HashSet<PathKey>>,
}

impl<'a> LazyInputKeys<'a> {
    fn new(items: &'a [PlannedItem]) -> Self {
        Self { items, cell: std::cell::OnceCell::new() }
    }

    /// Build (once) and return the batch's input key set. Only ever call this from a branch that can run
    /// while `job.confirmed_overwrite` is `false` — see this struct's doc for why calling it unconditionally
    /// would undo the whole point of CPE-1674.
    fn get(&self) -> &std::collections::HashSet<PathKey> {
        self.cell.get_or_init(|| input_path_keys(self.items))
    }
}

/// What currently occupies a planned output path, as far as a `stat` can tell (CPE-1696). Three states,
/// not two: the whole point is that "I could not find out" is **not** the same answer as "nothing is
/// there", and only the second one is safe to treat as a free path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputOccupancy {
    /// The path genuinely does not exist, or exists as something other than a regular file (a directory —
    /// which the pre-CPE-1696 `Path::is_file()` gate also answered "not an overwrite" for, and which a
    /// write to that path will fail on anyway).
    Free,
    /// A real regular file occupies the path.
    File,
    /// The `stat` failed for a reason other than the path being absent — permission denied along the
    /// resolved path, a dead network mount, an I/O error. We do **not** know whether bytes are there.
    Unknown,
}

/// The pure classifier behind [`output_occupancy`], split out (mirroring
/// `crate::dispatch::classify_path_error`'s own rationale) so the `NotFound`-vs-everything-else taxonomy
/// is unit-testable without a real filesystem: permission bits are platform- and privilege-dependent —
/// inert as root, and on Windows `Path::exists()` is not refused by a deny ACE at all — so an ACL-based
/// test alone would leave this taxonomy unverified on some machines.
///
/// `exists` is the outcome of [`Path::try_exists`] (which, unlike [`Path::exists`], returns
/// `io::Result<bool>` instead of folding every failure into `false`); `metadata` is called only when
/// `exists` says the path is there, to distinguish a regular file from a directory.
fn classify_output_occupancy(
    exists: std::io::Result<bool>,
    metadata: impl FnOnce() -> std::io::Result<std::fs::Metadata>,
) -> OutputOccupancy {
    match exists {
        Ok(false) => OutputOccupancy::Free,
        Ok(true) => match metadata() {
            Ok(m) if m.is_file() => OutputOccupancy::File,
            // Exists but isn't a regular file (a directory): identical to what the old `!is_file()` gate
            // concluded, and a write onto it fails on its own.
            Ok(_) => OutputOccupancy::Free,
            // A TOCTOU vanish between the two calls is a genuine absence.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => OutputOccupancy::Free,
            Err(_) => OutputOccupancy::Unknown,
        },
        // `Path::try_exists` already folds a genuine `NotFound` into `Ok(false)`, but be explicit: only an
        // absence is an absence.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => OutputOccupancy::Free,
        Err(_) => OutputOccupancy::Unknown,
    }
}

/// Stat `output` and classify what sits there, via [`classify_output_occupancy`].
fn output_occupancy(output: &Path) -> OutputOccupancy {
    classify_output_occupancy(output.try_exists(), || std::fs::metadata(output))
}

/// True when `item`'s planned write would replace bytes belonging to a file that was never submitted as
/// one of the batch's own inputs — the original CPE-1613 in-place case (`item.output` is [`same_file`] as
/// `item.input`), OR (CPE-1623) `item.output` resolves onto some OTHER real, pre-existing file that isn't
/// any input in this batch. The second check is gated behind a cheap [`output_occupancy`] stat — `Free`
/// for the overwhelmingly common case (a freshly-computed non-destructive name that doesn't exist yet) —
/// so only a genuine collision pays for the [`path_key`]/`HashSet` lookup, and even that lookup is `O(1)`
/// against `input_keys` (CPE-1667; see [`input_path_keys`]), not the `O(n)` pairwise scan this used to
/// run.
fn is_foreign_overwrite(item: &PlannedItem, input_keys: &std::collections::HashSet<PathKey>) -> bool {
    // **Security audit finding 4 (PR #848).** Standalone, this function fails OPEN for an alternate data
    // stream: a never-before-existing stream path makes the `Path::is_file()` arm below false, so it
    // reports "nothing sits there — not an overwrite of anything" and the hidden write is permitted with
    // no confirmation. (The "output is one of this batch's own inputs, so it's permitted" arm is a second
    // route to the same place once stream paths fuse onto their host.) That was previously safe only
    // because `classify_output_containment` refuses a stream path first — an unenforced call-ordering
    // convention, which is not a guarantee. Enforced here now, so the function is correct on its own and
    // no caller can reach the fail-open by calling it in the wrong order.
    if crate::batch_media::names_alternate_stream(&item.output) {
        return true;
    }
    if same_file(&item.input, &item.output) {
        return true;
    }
    // **CPE-1696 — the second fail-open route.** This used to be `if !Path::new(&item.output).is_file() {
    // return false }`, and `Path::is_file()` collapses EVERY `stat` failure into `false`: a
    // permission-denied output path therefore reported "nothing sits there — not an overwrite of
    // anything", `execute_plan_walk`'s refusal check passed, and the write proceeded with no confirmation.
    // An unknown must fail CLOSED here (`true` = "treat as a foreign overwrite"), because the only cost of
    // being wrong that way is a confirmation prompt the user can accept, whereas being wrong the other way
    // is unconfirmed data loss.
    match output_occupancy(Path::new(&item.output)) {
        OutputOccupancy::Free => return false, // genuinely nothing there — not an overwrite of anything
        OutputOccupancy::Unknown => return true, // we cannot prove it's empty; refuse without consent
        OutputOccupancy::File => {}
    }
    // A real file already occupies the output path. It's only a refusable overwrite if it's not one of
    // THIS batch's own inputs — a batch is always allowed to write into paths it explicitly selected.
    // `cache` is fresh per call (never threaded across items), matching `open_output_verified`'s own rule
    // for anything computed at write time — this isn't a live/dynamic fact `path_key`'s `Resolved` tier
    // could serve stale (it always canonicalizes `item.output` itself, never memoizes it), but keeping the
    // same discipline here avoids relying on that distinction staying true.
    let mut cache = ParentCache::new();
    !input_keys.contains(&path_key(&item.output, &mut cache))
}

/// True when running `items` would overwrite at least one file's bytes that isn't explicitly part of this
/// batch's own input set — decided by [`is_foreign_overwrite`] (CPE-1613/CPE-1623), **not** raw string
/// equality: `batch_media::plan` lower-cases a `Convert` target's extension, so a planned `output` can be
/// textually different from `input` yet be the SAME file on a case-insensitive filesystem (Windows,
/// default macOS). Shared by the [`execute_plan_walk`] refusal check and available to any future caller
/// that wants to ask "would this plan be destructive?" without duplicating the comparison.
///
/// **Test-only as of security audit finding 4 (PR #848).** This is a *predicate*, not a gate: it answers
/// "would this plan overwrite something?", and a caller outside the engine could easily read a `false` as
/// permission to write — which is exactly the fail-open shape the finding is about, since a
/// never-before-existing alternate data stream used to answer `false`. It was `pub`, and **nothing
/// anywhere used it**: not the app adapter, not another crate, not even `execute_plan_walk` (which calls
/// [`is_foreign_overwrite`] directly). Rather than keep a public advisory predicate that invites being
/// mistaken for the enforcement point, it is now compiled only for this module's own tests. The engine's
/// real write-time authority is [`crate::batch_media::open_output_verified`], which cannot be bypassed by
/// asking a question, and a future caller has to reach for that.
#[cfg(test)]
fn any_in_place(items: &[PlannedItem]) -> bool {
    let input_keys = input_path_keys(items);
    items.iter().any(|it| is_foreign_overwrite(it, &input_keys))
}

/// Execute a planned batch, calling `flush` with each file's [`OpResult`] as it completes — the shared
/// walker behind both the blocking [`execute_plan`] (test/no-progress path) and the streaming Tauri
/// command (`batch_media_execute_stream`), per the "one walker, both callers" streaming convention. For
/// each `PlannedItem`, reads its input bytes, applies `job.ops` via [`batch_transform::apply_ops`], and
/// writes the result to the item's planned output (creating the output's parent directory as needed). A
/// failing item — unreadable input, a non-image or otherwise rejected transform, or a failed write — is
/// recorded in the returned report and reported to `flush` with a reason; it never aborts the run.
///
/// **Refuses up front** ([`Err`], nothing written, `flush` never called) in two independent ways, checked
/// in this order:
///
/// 1. **Containment (IPC-bypass follow-up, unconditional — `confirmed_overwrite` has no effect on this
///    one).** Any item whose `output` would leave its own `input`'s directory, per
///    [`crate::batch_media::classify_output_containment`] — the same check `batch_media::plan()` itself
///    uses, re-derived here because a `PlannedItem` reaching this fn may never have gone through `plan()`
///    at all (see the module doc for the demonstrated bypass). This can't be waived by any flag: it isn't
///    asking "did the user consent to an overwrite?", it's asking "is this even a place the batch was
///    allowed to touch?". Reports its two refusal reasons **separately** (CPE-1642): an output whose
///    identity couldn't be established has not been shown to leave the folder, and saying otherwise would
///    tell the user something false about their own files.
/// 2. **Foreign overwrite (CPE-1590/CPE-1599/CPE-1623).** Any planned item that [`is_foreign_overwrite`]
///    — its output is the same file as its own input, OR resolves onto some other real file this batch
///    never selected — when `job.confirmed_overwrite` is `false`. Otherwise, those files ARE overwritten
///    wherever the plan says to.
pub fn execute_plan_walk(
    items: &[PlannedItem],
    job: &BatchJob,
    mut flush: impl FnMut(OpResult),
) -> Result<BatchReport, String> {
    let mut parent_cache = ParentCache::new();
    // CPE-1642: count the two refusal reasons SEPARATELY. An output whose identity could not be resolved
    // has not been shown to leave the folder — reporting it as one that "would land outside" tells the
    // user something factually untrue about their own files.
    let mut escaping = 0usize;
    let mut rejected: Vec<&'static str> = Vec::new();
    let mut unverifiable: Vec<&'static str> = Vec::new();
    for it in items {
        match classify_output_containment(&it.input, &it.output, &mut parent_cache) {
            Containment::Inside => {}
            Containment::Escapes => escaping += 1,
            Containment::Refused(why) => rejected.push(why),
            Containment::Unverifiable(why) => unverifiable.push(why),
        }
    }
    if escaping > 0 || !rejected.is_empty() || !unverifiable.is_empty() {
        let mut reasons: Vec<String> = Vec::new();
        if escaping > 0 {
            reasons.push(format!(
                "{escaping} planned output{} would land outside its own input's folder",
                if escaping == 1 { "" } else { "s" }
            ));
        }
        if !rejected.is_empty() {
            reasons.push(format!(
                "{} planned output{} {}",
                rejected.len(),
                if rejected.len() == 1 { "" } else { "s" },
                rejected[0]
            ));
        }
        if !unverifiable.is_empty() {
            let why = unverifiable[0];
            reasons.push(format!(
                "{} planned output{} couldn't be verified to stay inside its own input's folder ({why})",
                unverifiable.len(),
                if unverifiable.len() == 1 { "" } else { "s" }
            ));
        }
        return Err(format!(
            "refusing to run: {} — this can happen when a PlannedItem is supplied without going through \
             batch_media::plan() (which normally refuses this itself); nothing was written, and this \
             cannot be overridden by confirmed_overwrite, which only ever authorises overwriting a file \
             inside the selected folder",
            reasons.join(", and ")
        ));
    }

    // Built lazily, at most once for the whole batch (CPE-1667's O(1)-per-check shape, made lazy by
    // CPE-1674) — every `is_foreign_overwrite` call below, including the one inside `execute_one`'s
    // per-item verify-to-write window, probes this same `HashSet` in O(1) instead of re-scanning `items`
    // pairwise, and a `confirmed_overwrite` batch never builds it at all. See `LazyInputKeys`'s doc.
    let input_keys = LazyInputKeys::new(items);

    if !job.confirmed_overwrite {
        let count = items.iter().filter(|it| is_foreign_overwrite(it, input_keys.get())).count();
        if count > 0 {
            return Err(format!(
                "refusing to run: this plan would overwrite {count} file{} not explicitly part of this \
                 batch (either the same file as its own input, or an existing file this batch never \
                 selected), and `confirmed_overwrite` was not set on the batch job — re-plan with an \
                 explicit confirmation or change the job so every output stays clear of existing files",
                if count == 1 { "" } else { "s" }
            ));
        }
    }

    let mut report = BatchReport::default();
    for item in items {
        // Every write-time question is asked inside `execute_one`, on the handle the bytes go through —
        // NOT here. An earlier cut re-checked by path at this point; the security audit showed that gave
        // false assurance, because `execute_one` then read and transformed the image (528 ms measured)
        // before writing, and a link planted anywhere in that window redirected the write. There is
        // deliberately no path-based per-item pre-check here to be defeated.
        match execute_one(item, job, &input_keys) {
            Ok(()) => {
                report.written += 1;
                flush(OpResult::ok(Path::new(&item.output)));
            }
            Err(reason) => {
                flush(OpResult::err(Path::new(&item.input), &reason));
                report.skipped.push((item.input.clone(), reason));
            }
        }
    }
    Ok(report)
}

/// Execute a planned batch without streaming per-file progress — the cargo-test correctness path.
/// Delegates to [`execute_plan_walk`] with a no-op flush so there is exactly one implementation.
pub fn execute_plan(items: &[PlannedItem], job: &BatchJob) -> Result<BatchReport, String> {
    execute_plan_walk(items, job, |_| {})
}

/// Transform one item and write it — with **every** write-time safety question answered on the handle the
/// bytes actually go through, in the last moment before they do (security audit finding 1, PR #848).
///
/// Ordering is load-bearing and is the whole fix:
///
/// 1. `fs::read` + `apply_ops` — the slow part (the audit measured 528 ms), deliberately **before** the
///    output is opened or verified. Nothing has been created or touched yet, so a transform that fails
///    still leaves no output behind, and the expensive work is outside the window rather than inside it.
/// 2. [`crate::batch_media::open_output_verified`] — claims the name atomically, refuses to follow a link
///    at it, and settles identity/hard-links/directoriness on the resulting handle against a freshly
///    scanned census. This is the enforcement point.
/// 3. The foreign-overwrite question, answered from `created` (an atomic fact from step 2) rather than a
///    `Path::is_file()` stat that could already be stale.
/// 4. Truncate + write **through the handle from step 2** — the same object that was verified, which
///    cannot have been swapped for another in between because a handle names an object, not a name.
///
/// Steps 2-4 are a handful of syscalls; the previously exploitable window was the entire transform.
///
/// `input_keys` is the batch's lazily-built [`LazyInputKeys`] memo (CPE-1667, made lazy by CPE-1674), not
/// the raw `items` slice — step 3's `is_foreign_overwrite` call sits inside the verify-to-write window, so
/// it must cost O(1), not an O(n) scan over the batch, once the set has been built. `input_keys.get()` is
/// only reached below when `!job.confirmed_overwrite` is also true (short-circuit `&&`), so a
/// `confirmed_overwrite` batch never builds the set from here either.
fn execute_one(
    item: &PlannedItem,
    job: &BatchJob,
    input_keys: &LazyInputKeys,
) -> Result<(), String> {
    #[cfg(test)]
    WINDOW_TRACE.with(|c| c.set(WindowTrace::default()));
    #[cfg(test)]
    trace_mark(|t| t.transform_start = Some(std::time::Instant::now()));
    let input_bytes = fs::read(&item.input).map_err(|e| format!("could not read input: {e}"))?;
    let output_bytes = batch_transform::apply_ops(&input_bytes, &job.ops)?;
    #[cfg(test)]
    trace_mark(|t| t.transform_end = Some(std::time::Instant::now()));

    if let Some(parent) = Path::new(&item.output).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| format!("could not create output dir: {e}"))?;
        }
    }

    let verified = crate::batch_media::open_output_verified(&item.input, &item.output)?;
    #[cfg(test)]
    trace_mark(|t| t.window_start = Some(std::time::Instant::now()));

    // Something was already at this name. `confirmed_overwrite` is the only thing that can authorise
    // replacing it — and `created` is an atomic fact from the open, not a stat that can go stale.
    //
    // `&&` short-circuits left to right, so `input_keys.get()` (CPE-1674) is only ever evaluated once both
    // `!verified.created()` and `!job.confirmed_overwrite` are true — a `confirmed_overwrite` batch never
    // reaches it, from here or from `execute_plan_walk`'s up-front scan, so it never builds the set.
    if !verified.created() && !job.confirmed_overwrite && is_foreign_overwrite(item, input_keys.get()) {
        verified.abandon(&item.output);
        return Err(format!(
            "refusing at write time: \"{}\" would overwrite a file this batch never selected — it \
             appeared after the batch's up-front check, and `confirmed_overwrite` was not set. Nothing \
             was written for this file",
            item.output
        ));
    }

    // **CPE-1725 inventoried this site as an `fs::write` sibling of the two whole-file save paths and
    // found the premise wrong in the safe direction: there is no `fs::write` here at all.** The bytes go
    // through the handle `open_output_verified` already opened with `O_NOFOLLOW` /
    // `FILE_FLAG_OPEN_REPARSE_POINT` and then re-verified (`symlink_metadata`, plus the handle's reparse
    // bit), so **any** link at the output — live or dangling — is refused before a byte is written. That
    // is stricter than either save path, which *resolve* a link and edit its target; a batch never writes
    // through one, because its output name is claimed rather than opened by the user. No change was needed
    // here, and this note is so the next sweep does not have to re-derive that.
    let result = verified.write_all(&output_bytes, &item.output);
    #[cfg(test)]
    trace_mark(|t| t.window_end = Some(std::time::Instant::now()));
    result
}

/// Four timestamps recorded from **inside a real [`execute_one`] run** (PR #856 review): the transform's
/// start/end and the verify-to-write window's start/end. Ordering claims built from these (does the
/// transform's interval lie entirely before the window's?) are exact and immune to both build profile and
/// runner contention — unlike a ratio of measured durations, which cannot be made to work: under
/// `--release` the window is roughly two orders of magnitude smaller than under `cargo test` (a debug
/// build's image transform is far slower), and a shared CI runner can add two more orders of magnitude of
/// noise on top of either. A fixed ratio bound that survives all of that does not exist — see the tests
/// that read this trace for what a ratio bound got wrong in practice (a real CI failure on a saturated
/// runner, 9.1× against a 10× gate, that turned out to be pure contention).
///
/// **What this does NOT cover, stated plainly because the alternative is a guard whose reputation exceeds
/// its reach (PR #856 re-review).** The ordering assertion pins exactly one property: *the instrumented
/// transform* does not overlap the window. It does **not** bound the window's size. The reviewer
/// demonstrated the gap by adding a whole second transform inside the window — both tests stayed green
/// while printing a **444 ms verify→write window**, which is the same shape and very nearly the same
/// magnitude as the ~528 ms window the CPE-1624 audit actually exploited.
///
/// That coverage was traded away deliberately, because both walls have now been measured and no fixed
/// ratio fits between them: a 10× gate failed on a saturated CI runner at 9.1× (pure contention), and a
/// 100× gate fails under `--release`, where the window is ~405,000 ns against a ~35.9 ms transform. The
/// ratio also never caught the regression this ticket was filed for — the reviewer restored the genuine
/// O(n) scan and the ratio passed with a 24× margin. The `canonicalize`-count seam cannot cover the
/// residual either: a second `apply_ops` plus an `fs::read` make zero canonicalize calls.
///
/// **So: new work introduced between `window_start` and `write_all` is a CODE-REVIEW obligation with no
/// automated net.** If you are adding anything there, that is the check — there is no test that will stop
/// you.
///
/// `Instant` is `Copy`, so a plain `Cell` (not `RefCell`) suffices — no borrow to hold across the
/// read-modify-write. Thread-local for the same reason the other test counters in this module are: tests
/// run on separate threads and must not see each other's runs. The trace is reset at the top of every
/// [`execute_one`], so in a multi-item batch [`assert_transform_precedes_window`] necessarily inspects
/// only the **last** item — harmless for today's callers (SEC-7 plans exactly one input; the CPE-1667
/// test calls `execute_one` directly), but an ordering regression affecting only item 0 of an n-item
/// batch would go unseen by a future reuse.
#[cfg(test)]
#[derive(Debug, Clone, Copy, Default)]
struct WindowTrace {
    transform_start: Option<std::time::Instant>,
    transform_end: Option<std::time::Instant>,
    window_start: Option<std::time::Instant>,
    window_end: Option<std::time::Instant>,
}

#[cfg(test)]
thread_local! {
    static WINDOW_TRACE: std::cell::Cell<WindowTrace> = std::cell::Cell::new(WindowTrace::default());
}

/// Read-modify-write one field of the current thread's [`WindowTrace`]. `Cell::get`/`set` (not `RefCell`)
/// because `WindowTrace` is `Copy` — no borrow needs to live across the mutation.
#[cfg(test)]
fn trace_mark(set: impl FnOnce(&mut WindowTrace)) {
    WINDOW_TRACE.with(|c| {
        let mut t = c.get();
        set(&mut t);
        c.set(t);
    });
}

/// The deterministic replacement for a ratio bound (PR #856 review, following CI red on a saturated
/// runner: 9.1× against a 10× gate, pure contention — see [`WindowTrace`]'s doc for why no fixed ratio
/// survives both build profile and runner load). Reads the trace left by the [`execute_one`] call the
/// caller just made and asserts an **ordering** property instead of a duration comparison: the transform's
/// whole interval must end at or before the window opens, i.e. it is provably not overlapping the window
/// at all, regardless of how fast or slow either one ran. `Instant` is monotonic, so this is exact on every
/// build profile and every runner, loaded or not — there is no bound to tune.
///
/// **Read [`WindowTrace`]'s doc for what this deliberately does not cover** — it pins the ordering of the
/// instrumented transform, not the window's size, and a reviewer demonstrated a 444 ms window passing
/// green. Do not cite this assertion as evidence that the window is bounded.
///
/// Red→green evidence for this assertion is the **faithful** mutation, not a synthetic one: moving
/// `fs::read` + `apply_ops` bodily to after `open_output_verified`, with the trace marks travelling with
/// the code exactly as a real regression would move them, turns both tests red deterministically
/// (`the transform's interval ends 480.4241ms AFTER the window opened`). Moving `window_start` instead
/// mutates the *instrumentation* and only proves this function works — that was the original proof and it
/// was the weaker one.
///
/// Prints both measured durations (still genuinely useful context) but gates on ordering alone.
#[cfg(test)]
fn assert_transform_precedes_window(context: &str) {
    let t = WINDOW_TRACE.with(|c| c.get());
    let transform_start =
        t.transform_start.expect("transform_start was never recorded — this test verified NOTHING");
    let transform_end =
        t.transform_end.expect("transform_end was never recorded — this test verified NOTHING");
    let window_start =
        t.window_start.expect("window_start was never recorded — this test verified NOTHING");
    let window_end = t.window_end.expect("window_end was never recorded — this test verified NOTHING");

    println!(
        "{context}: transform took {:?}; the verify->write window took {:?}",
        transform_end.duration_since(transform_start),
        window_end.duration_since(window_start)
    );

    assert!(
        transform_end <= window_start,
        "{context}: THE INSTRUMENTED TRANSFORM MOVED INTO THE WINDOW — its interval ends {:?} AFTER the \
         window opened. This is an exact ordering check on monotonic `Instant`s, not a duration ratio, so \
         it cannot flake on build profile or runner load; a failure here means the ordering itself \
         regressed. NOTE it pins only THIS transform's ordering — it does not bound the window's size, \
         and other work added inside the window passes it silently (see `WindowTrace`'s doc).",
        transform_end.duration_since(window_start)
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch_media::{plan, MediaOp};
    use image::ImageFormat;
    use std::io::Cursor;
    use std::path::PathBuf;

    /// Process-unique scratch dir (mirrors `snapshot_capture`/`revert_engine`'s test pattern): a
    /// `std::env::temp_dir()` subdir keyed by tag + pid + an atomic counter, so parallel test threads and
    /// parallel CI runs never collide, and no OS-permission trickery is needed for the Windows leg.
    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-batchexec-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        image::RgbImage::from_pixel(w, h, image::Rgb([10u8, 20, 30]))
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn executes_a_resize_and_convert_plan_on_real_files() {
        let d = scratch("ok");
        let a = d.join("a.png");
        let b = d.join("b.png");
        let t = d.join("c.txt");
        fs::write(&a, png_bytes(64, 32)).unwrap();
        fs::write(&b, png_bytes(50, 50)).unwrap();
        fs::write(&t, b"not an image").unwrap();

        let orig_a = fs::read(&a).unwrap();
        let orig_b = fs::read(&b).unwrap();
        let orig_t = fs::read(&t).unwrap();

        let job = BatchJob::new(vec![
            MediaOp::Resize { max_px: 16 },
            MediaOp::Convert { to_ext: "jpg".into() },
        ]);
        let inputs = vec![
            a.to_string_lossy().to_string(),
            b.to_string_lossy().to_string(),
            t.to_string_lossy().to_string(),
        ];
        let items = plan(&job, &inputs).unwrap();
        assert_eq!(items.len(), 3);

        let report = execute_plan(&items, &job).unwrap();

        assert_eq!(report.written, 2, "the two PNGs should succeed");
        assert_eq!(report.skipped.len(), 1, "the .txt should be skipped, not fatal");
        assert_eq!(report.skipped[0].0, t.to_string_lossy().to_string());

        // Both image outputs exist, are JPEGs, and respect the resize cap.
        for item in items.iter().take(2) {
            let out_bytes = fs::read(&item.output)
                .unwrap_or_else(|e| panic!("expected output at {}: {e}", item.output));
            assert_eq!(image::guess_format(&out_bytes).unwrap(), ImageFormat::Jpeg);
            let decoded = image::load_from_memory(&out_bytes).unwrap();
            assert!(decoded.width() <= 16 && decoded.height() <= 16);
        }

        // The rejected .txt item's planned output must NOT have been created.
        assert!(!Path::new(&items[2].output).exists());

        // Non-destructive: every input is byte-for-byte unchanged.
        assert_eq!(fs::read(&a).unwrap(), orig_a);
        assert_eq!(fs::read(&b).unwrap(), orig_b);
        assert_eq!(fs::read(&t).unwrap(), orig_t);

        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_input_is_skipped_with_a_reason_not_fatal() {
        let d = scratch("missing");
        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 16 }]);
        let missing = d.join("nope.png").to_string_lossy().to_string();
        let items = plan(&job, std::slice::from_ref(&missing)).unwrap();

        let report = execute_plan(&items, &job).unwrap();

        assert_eq!(report.written, 0);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].0, missing);
        assert!(!report.skipped[0].1.is_empty());

        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_watermark_overlay_is_skipped_with_a_reason_not_fatal() {
        // The INPUT file is real and valid; only the watermark's overlay path is missing. This must
        // still be a per-file skip (with a reason), not a fatal error for the whole batch.
        let d = scratch("watermark-missing-overlay");
        let a = d.join("a.png");
        fs::write(&a, png_bytes(20, 20)).unwrap();

        let job = BatchJob::new(vec![MediaOp::Watermark {
            image: d.join("nope-logo.png").to_string_lossy().to_string(),
            position: crate::batch_media::Corner::BottomRight,
            opacity: 50,
        }]);
        let inputs = vec![a.to_string_lossy().to_string()];
        let items = plan(&job, &inputs).unwrap();

        let report = execute_plan(&items, &job).unwrap();
        assert_eq!(report.written, 0);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].0, a.to_string_lossy().to_string());
        assert!(!report.skipped[0].1.is_empty(), "the skip must carry a reason");
        // The valid input itself must be untouched.
        assert_eq!(fs::read(&a).unwrap(), png_bytes(20, 20));

        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn empty_plan_yields_an_empty_report_with_no_panic() {
        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 16 }]);
        let report = execute_plan(&[], &job).unwrap();
        assert_eq!(report, BatchReport::default());
    }

    #[test]
    fn execute_plan_walk_empty_input_never_panics_and_never_flushes() {
        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 16 }]);
        let mut flushed = 0usize;
        let report = execute_plan_walk(&[], &job, |_| flushed += 1).unwrap();
        assert_eq!(report, BatchReport::default());
        assert_eq!(flushed, 0);
    }

    // ---- CPE-1599: engine-side refusal of an unconfirmed in-place overwrite ---------------------------

    /// The core defence-in-depth guarantee: a plan whose planned `output == input` for at least one item
    /// is REFUSED — `Err`, nothing written, `flush` never called — when `job.confirmed_overwrite` is
    /// left at its default `false`. This must hold even though `job.non_destructive` was explicitly set
    /// to `false` (the only way `plan()` ever produces an in-place output in the first place): setting
    /// `non_destructive: false` alone is no longer sufficient to get an in-place write out of the engine.
    #[test]
    fn refuses_an_in_place_plan_without_confirmed_overwrite() {
        let d = scratch("refuse-unconfirmed");
        let a = d.join("a.jpg");
        fs::write(&a, png_bytes(10, 10)).unwrap();
        let orig = fs::read(&a).unwrap();

        let mut job = BatchJob::new(vec![MediaOp::Compress { quality: 80 }]); // no rename suffix
        job.non_destructive = false; // the only way plan() can resolve output == input
        assert!(!job.confirmed_overwrite, "confirmed_overwrite must default to false");

        let inputs = vec![a.to_string_lossy().to_string()];
        let items = plan(&job, &inputs).unwrap();
        assert_eq!(items[0].input, items[0].output, "sanity: this plan IS in-place");

        let mut flushed = 0usize;
        let err = execute_plan_walk(&items, &job, |_| flushed += 1)
            .expect_err("an unconfirmed in-place plan must be refused, not executed");

        assert!(!err.is_empty(), "the refusal must carry a specific reason");
        assert!(err.to_lowercase().contains("confirm"), "refusal reason: {err}");
        assert_eq!(flushed, 0, "flush must never fire on a refused plan");
        // The original file must be COMPLETELY untouched — the refusal is a no-op, not a partial write.
        assert_eq!(fs::read(&a).unwrap(), orig);

        let _ = fs::remove_dir_all(&d);
    }

    /// The flip side: the identical in-place plan proceeds and actually overwrites the input once
    /// `confirmed_overwrite` is explicitly set — proving the flag is load-bearing, not decorative.
    #[test]
    fn an_in_place_plan_proceeds_once_confirmed_overwrite_is_set() {
        let d = scratch("refuse-confirmed");
        let a = d.join("a.png");
        fs::write(&a, png_bytes(12, 12)).unwrap();

        let mut job = BatchJob::new(vec![MediaOp::StripMetadata]); // no rename suffix
        job.non_destructive = false;
        job.confirmed_overwrite = true;

        let inputs = vec![a.to_string_lossy().to_string()];
        let items = plan(&job, &inputs).unwrap();
        assert_eq!(items[0].input, items[0].output, "sanity: this plan IS in-place");

        let report = execute_plan(&items, &job).expect("a confirmed in-place plan must be allowed to run");
        assert_eq!(report.written, 1);
        assert!(report.skipped.is_empty());
        // The file still exists at the same path (it was overwritten in place, not left alone).
        assert!(a.exists());

        let _ = fs::remove_dir_all(&d);
    }

    /// A non-destructive plan (every output != input) is unaffected by `confirmed_overwrite` either
    /// way — the refusal check only ever fires for a plan that actually contains an in-place item.
    #[test]
    fn a_non_destructive_plan_runs_regardless_of_confirmed_overwrite() {
        for confirmed in [false, true] {
            let d = scratch(if confirmed { "nondestructive-confirmed" } else { "nondestructive-unconfirmed" });
            let a = d.join("a.png");
            fs::write(&a, png_bytes(8, 8)).unwrap();

            let mut job = BatchJob::new(vec![MediaOp::Resize { max_px: 4 }]); // default non_destructive: true
            job.confirmed_overwrite = confirmed;
            assert!(job.non_destructive);

            let inputs = vec![a.to_string_lossy().to_string()];
            let items = plan(&job, &inputs).unwrap();
            assert_ne!(items[0].input, items[0].output, "sanity: this plan is NOT in-place");

            let report = execute_plan(&items, &job)
                .unwrap_or_else(|e| panic!("a non-destructive plan must never be refused (confirmed={confirmed}): {e}"));
            assert_eq!(report.written, 1);

            let _ = fs::remove_dir_all(&d);
        }
    }

    /// Streamed outcomes (`execute_plan_walk` + a collecting `flush`) must match a direct `execute_plan`
    /// run byte-for-byte: same written count, same skipped set — proving the streaming command and the
    /// blocking command run the identical per-file logic (no drift between the two callers).
    #[test]
    fn streamed_walk_matches_direct_execute_plan_same_written_and_skips() {
        let d = scratch("streamed-vs-direct");
        let a = d.join("a.png");
        let b = d.join("b.png");
        let t = d.join("c.txt");
        fs::write(&a, png_bytes(40, 20)).unwrap();
        fs::write(&b, png_bytes(30, 30)).unwrap();
        fs::write(&t, b"not an image").unwrap();

        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 16 }, MediaOp::StripMetadata]);
        let inputs = vec![
            a.to_string_lossy().to_string(),
            b.to_string_lossy().to_string(),
            t.to_string_lossy().to_string(),
        ];
        let items_direct = plan(&job, &inputs).unwrap();
        let direct_report = execute_plan(&items_direct, &job).unwrap();

        // Re-plan into a sibling scratch dir so the streamed run doesn't clash with the direct run's
        // already-written outputs (both plans are non-destructive but share the same source dir).
        let d2 = scratch("streamed-vs-direct-2");
        let a2 = d2.join("a.png");
        let b2 = d2.join("b.png");
        let t2 = d2.join("c.txt");
        fs::write(&a2, png_bytes(40, 20)).unwrap();
        fs::write(&b2, png_bytes(30, 30)).unwrap();
        fs::write(&t2, b"not an image").unwrap();
        let inputs2 = vec![
            a2.to_string_lossy().to_string(),
            b2.to_string_lossy().to_string(),
            t2.to_string_lossy().to_string(),
        ];
        let items_streamed = plan(&job, &inputs2).unwrap();

        let mut streamed_results: Vec<OpResult> = Vec::new();
        let streamed_report = execute_plan_walk(&items_streamed, &job, |r| streamed_results.push(r)).unwrap();

        assert_eq!(streamed_report.written, direct_report.written);
        assert_eq!(streamed_report.skipped.len(), direct_report.skipped.len());
        assert_eq!(streamed_report.written, 2, "the two PNGs should succeed");
        assert_eq!(streamed_report.skipped.len(), 1, "the .txt should be skipped, not fatal");

        // One OpResult flushed per planned item, and the ok/err split matches the report.
        assert_eq!(streamed_results.len(), items_streamed.len());
        let ok_count = streamed_results.iter().filter(|r| r.ok).count();
        let err_count = streamed_results.iter().filter(|r| !r.ok).count();
        assert_eq!(ok_count, streamed_report.written);
        assert_eq!(err_count, streamed_report.skipped.len());
        for r in streamed_results.iter().filter(|r| !r.ok) {
            assert!(!r.error.is_empty(), "a skipped OpResult must carry a reason");
        }

        let _ = fs::remove_dir_all(&d);
        let _ = fs::remove_dir_all(&d2);
    }

    /// QA-Architect burndown (CPE-1115 follow-up): pin the exact scenario the user hit by hand on 0.57.36 —
    /// a file with a valid image *extension* (and even a valid JPEG SOI marker) but **no decodable image
    /// data** must be SKIPPED-with-a-reason (never fatal), while the valid images in the same batch still
    /// produce outputs. The pre-existing skip test uses a `.txt` (obviously not an image); this covers the
    /// subtler "looks like an image, isn't" case that made "2 selected → 1 output" look like a lost-file bug.
    #[test]
    fn a_real_looking_but_undecodable_image_is_skipped_while_valid_files_still_succeed() {
        let d = scratch("undecodable");
        let good_png = d.join("pixel.png");
        let good_jpg = d.join("real.jpg");
        let bad_jpg = d.join("photo.jpg"); // valid .jpg ext + JPEG SOI/EOI, but no frame/scan → undecodable
        fs::write(&good_png, png_bytes(4, 4)).unwrap();
        {
            let mut buf = Cursor::new(Vec::new());
            image::RgbImage::from_pixel(8, 8, image::Rgb([120u8, 130, 140]))
                .write_to(&mut buf, ImageFormat::Jpeg)
                .unwrap();
            fs::write(&good_jpg, buf.into_inner()).unwrap();
        }
        fs::write(&bad_jpg, [0xFFu8, 0xD8, 0xFF, 0xD9]).unwrap(); // SOI + EOI, zero image content

        let job = BatchJob::new(vec![MediaOp::Compress { quality: 80 }]);
        let inputs = vec![
            good_png.to_string_lossy().to_string(),
            good_jpg.to_string_lossy().to_string(),
            bad_jpg.to_string_lossy().to_string(),
        ];
        let items = plan(&job, &inputs).unwrap();
        assert_eq!(items.len(), 3, "one planned item per input");

        let report = execute_plan(&items, &job).unwrap();
        assert_eq!(report.written, 2, "the two valid images are written");
        assert_eq!(report.skipped.len(), 1, "the undecodable jpg is skipped, not fatal to the batch");
        assert_eq!(report.skipped[0].0, bad_jpg.to_string_lossy().to_string());
        assert!(!report.skipped[0].1.is_empty(), "the skip must carry a human reason");

        // Both valid outputs exist and decode; the skipped input produced NO output file.
        for item in items.iter().take(2) {
            let bytes = fs::read(&item.output).unwrap_or_else(|e| panic!("expected output {}: {e}", item.output));
            assert!(image::load_from_memory(&bytes).is_ok(), "valid output should decode");
        }
        assert!(!Path::new(&items[2].output).exists(), "no output for the skipped file");

        let _ = fs::remove_dir_all(&d);
    }

    // ---- CPE-1613: same-file detection, not raw string equality ---------------------------------------

    /// The ticket's exact worked example, end-to-end with a REAL file on disk: `IMG_1.JPG` + Convert→jpg
    /// with "write to new files" OFF. `plan()` lower-cases only the extension, so the planned output is
    /// the textually different `"IMG_1.jpg"` — but `any_in_place`/`execute_plan_walk`'s refusal must
    /// recognise it as the SAME file on a case-insensitive filesystem (Windows, default macOS) and refuse
    /// without `confirmed_overwrite`, leaving the original byte-for-byte untouched. On a case-sensitive
    /// filesystem (Linux) these really are two different possible files, so no refusal is needed there —
    /// per CPE-1613, the check must NOT be unconditionally case-insensitive on Linux.
    #[test]
    fn cpe_1613_worked_example_real_file_in_place_detection_is_platform_gated() {
        let d = scratch("cpe1613-worked-example");
        let input = d.join("IMG_1.JPG");
        {
            let mut buf = Cursor::new(Vec::new());
            image::RgbImage::from_pixel(6, 6, image::Rgb([1u8, 2, 3]))
                .write_to(&mut buf, ImageFormat::Jpeg)
                .unwrap();
            fs::write(&input, buf.into_inner()).unwrap();
        }

        let mut job = BatchJob::new(vec![MediaOp::Convert { to_ext: "jpg".into() }]);
        job.non_destructive = false;
        let items = plan(&job, &[input.to_string_lossy().to_string()]).unwrap();
        assert_eq!(items[0].output, input.with_file_name("IMG_1.jpg").to_string_lossy());

        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            assert!(
                any_in_place(&items),
                "a case-only extension difference must be flagged in-place on {}",
                std::env::consts::OS
            );
            let orig = fs::read(&input).unwrap();
            let err = execute_plan_walk(&items, &job, |_| {})
                .expect_err("must refuse an unconfirmed in-place overwrite even though the strings differ");
            assert!(err.to_lowercase().contains("confirm"), "refusal reason: {err}");
            assert_eq!(fs::read(&input).unwrap(), orig, "a refused plan must never touch the original");

            // Confirmed, it proceeds and genuinely overwrites the original in place.
            job.confirmed_overwrite = true;
            let report = execute_plan(&items, &job).expect("a confirmed in-place plan must be allowed to run");
            assert_eq!(report.written, 1);
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            assert!(
                !any_in_place(&items),
                "a case-only extension difference is a DIFFERENT file on a case-sensitive filesystem"
            );
            let report = execute_plan(&items, &job).expect("a genuinely distinct output needs no refusal");
            assert_eq!(report.written, 1);
        }

        let _ = fs::remove_dir_all(&d);
    }

    // ---- CPE-1623: broadened destructive-overwrite refusal beyond the batch's own input set -----------

    /// Fix #3 from the ticket: an output that resolves onto an EXISTING file this batch never selected as
    /// one of its own inputs must be treated exactly like an in-place self-overwrite (CPE-1599) — refused
    /// without `confirmed_overwrite`, allowed to proceed once it's set. Overwrite mode (`non_destructive:
    /// false`) is used here because that's the case `plan()` itself does no disambiguation for — see
    /// `batch_media.rs`'s `cpe_1623_non_destructive_mode_steps_around_a_real_pre_existing_unrelated_file`
    /// for the non-destructive-mode side of the fix, where `plan()` just picks a different name instead.
    #[test]
    fn cpe_1623_output_landing_on_a_foreign_existing_file_is_refused_then_allowed_once_confirmed() {
        let d = scratch("cpe1623-foreign-overwrite");
        let input = d.join("photo.png");
        fs::write(&input, png_bytes(10, 10)).unwrap();
        let foreign = d.join("vacation.png"); // real file, NOT one of this batch's inputs
        let foreign_original = png_bytes(4, 4);
        fs::write(&foreign, &foreign_original).unwrap();

        let mut job = BatchJob::new(vec![MediaOp::Rename { template: "vacation".into() }]);
        job.non_destructive = false; // explicit "write to this literal location" mode — no disambiguation
        let items = plan(&job, &[input.to_string_lossy().to_string()]).unwrap();
        assert_eq!(
            items[0].output,
            foreign.to_string_lossy(),
            "sanity: the rename lands exactly on the foreign file"
        );
        assert!(!same_file(&items[0].input, &items[0].output), "sanity: this is NOT the self-overwrite case");

        assert!(any_in_place(&items), "an output landing on a real foreign file must be flagged");
        let err = execute_plan_walk(&items, &job, |_| {})
            .expect_err("must refuse an unconfirmed overwrite of a file outside this batch");
        assert!(err.to_lowercase().contains("confirm"), "refusal reason: {err}");
        assert_eq!(
            fs::read(&foreign).unwrap(),
            foreign_original,
            "the foreign file must be untouched by a refused plan"
        );

        // Confirmed, it proceeds and genuinely overwrites the foreign file.
        job.confirmed_overwrite = true;
        let report = execute_plan(&items, &job).expect("a confirmed overwrite must be allowed to run");
        assert_eq!(report.written, 1);
        assert_ne!(fs::read(&foreign).unwrap(), foreign_original, "the confirmed write actually replaced the bytes");

        let _ = fs::remove_dir_all(&d);
    }

    /// The flip side, proving no new false alarms: a plan whose output does NOT already exist on disk is
    /// never flagged, confirmed_overwrite or not — mirrors the pre-CPE-1623
    /// `a_non_destructive_plan_runs_regardless_of_confirmed_overwrite` coverage above, but for overwrite
    /// mode specifically (where `plan()` does no disambiguation at all).
    #[test]
    fn cpe_1623_overwrite_mode_with_a_genuinely_new_output_name_needs_no_confirmation() {
        let d = scratch("cpe1623-no-false-alarm");
        let input = d.join("photo.png");
        fs::write(&input, png_bytes(10, 10)).unwrap();

        let mut job = BatchJob::new(vec![MediaOp::Rename { template: "brand-new-name".into() }]);
        job.non_destructive = false;
        let items = plan(&job, &[input.to_string_lossy().to_string()]).unwrap();

        assert!(!any_in_place(&items), "a genuinely new output name must not be flagged");
        let report = execute_plan(&items, &job).expect("no refusal expected for a non-colliding rename");
        assert_eq!(report.written, 1);

        let _ = fs::remove_dir_all(&d);
    }

    // ---- IPC-bypass follow-up: execute_plan_walk re-derives containment itself ------------------------
    //
    // The security-audit finding this closes: `PlannedItem` is a plain public struct with zero invariants
    // of its own — `Serialize`/`Deserialize`, nothing enforcing "this came from `plan()`". Every
    // containment guarantee lived in `batch_media::plan()`/`validate()`, but `batch_media_execute_stream`
    // (the Tauri command) took `items: Vec<PlannedItem>` straight off the wire, and this module's only
    // gate (`is_foreign_overwrite`) asked "does something already exist at this output?" — never "does
    // this stay inside the input's own folder?". A caller that skips `plan()` entirely (devtools, a
    // compromised webview, a future automation surface) could hand-build a `PlannedItem` pointing `output`
    // at ANY path the process can write, and a brand-new file at that path sailed straight through with no
    // refusal, `confirmed_overwrite` or not. These tests hand-build exactly such an item WITHOUT ever
    // calling `plan()`, proving `execute_plan` now refuses it — verified by reading bytes off disk, not
    // trusting the `Result`.

    /// The core PoC turned into a permanent regression test: a hand-built `PlannedItem` whose `output`
    /// resolves outside its `input`'s own directory must be refused — nothing written at all, proven by
    /// asserting the target path never came into existence.
    #[test]
    fn ipc_bypass_hand_built_escaping_planned_item_is_refused_and_writes_nothing() {
        let d = scratch("ipc-bypass-escape");
        let workdir = d.join("a").join("traversal_workdir");
        let victim_dir = d.join("cpe1613_traversal_victim");
        fs::create_dir_all(&workdir).unwrap();
        fs::create_dir_all(&victim_dir).unwrap();
        let input = workdir.join("innocuous.jpg");
        fs::write(&input, png_bytes(8, 8)).unwrap();
        let victim_target = victim_dir.join("important.jpg"); // does NOT exist yet — the attack scenario

        // Hand-built PlannedItem: NEVER goes through batch_media::plan(), so none of plan()'s own
        // containment checks ever run — this simulates a caller invoking the execute IPC surface directly.
        let items = vec![PlannedItem {
            input: input.to_string_lossy().to_string(),
            output: victim_target.to_string_lossy().to_string(),
            summary: "hand-built, bypassing plan()".into(),
        }];
        let job = BatchJob::new(vec![MediaOp::StripMetadata]);

        let err = execute_plan(&items, &job)
            .expect_err("an output escaping the input's own directory must be refused even without plan()");
        assert!(err.to_lowercase().contains("folder"), "refusal reason: {err}");

        // Byte-level proof, not a trust-the-return-value check: the target must never have been created.
        assert!(!victim_target.exists(), "the escaping write must never have happened");

        let _ = fs::remove_dir_all(&d);
    }

    /// The same hand-built escape, but with `confirmed_overwrite: true` — confirmation must not buy an
    /// escape. It only ever authorises overwriting the user's OWN input in place (CPE-1599/1613); it was
    /// never meant to, and must not, license writing to an arbitrary path outside the selected folder.
    #[test]
    fn ipc_bypass_hand_built_escaping_planned_item_is_refused_even_with_confirmed_overwrite() {
        let d = scratch("ipc-bypass-escape-confirmed");
        let workdir = d.join("a").join("traversal_workdir");
        let victim_dir = d.join("cpe1613_traversal_victim");
        fs::create_dir_all(&workdir).unwrap();
        fs::create_dir_all(&victim_dir).unwrap();
        let input = workdir.join("innocuous.jpg");
        fs::write(&input, png_bytes(8, 8)).unwrap();
        let victim_target = victim_dir.join("important.jpg");
        let victim_original = b"VICTIM ORIGINAL CONTENT - must not be touched".to_vec();
        fs::write(&victim_target, &victim_original).unwrap(); // a REAL pre-existing file this time

        let items = vec![PlannedItem {
            input: input.to_string_lossy().to_string(),
            output: victim_target.to_string_lossy().to_string(),
            summary: "hand-built, bypassing plan()".into(),
        }];
        let mut job = BatchJob::new(vec![MediaOp::StripMetadata]);
        job.confirmed_overwrite = true; // the caller-supplied "confirmation" from the finding

        let err = execute_plan(&items, &job).expect_err(
            "confirmed_overwrite must not authorise escaping the input's own folder — only an in-place \
             overwrite of the user's own input",
        );
        assert!(err.to_lowercase().contains("folder"), "refusal reason: {err}");

        // Byte-for-byte proof the victim's real bytes are untouched.
        assert_eq!(
            fs::read(&victim_target).unwrap(),
            victim_original,
            "confirmed_overwrite must never buy an escape — the victim file must be byte-for-byte untouched"
        );

        let _ = fs::remove_dir_all(&d);
    }

    /// No new false refusals: an ordinary plan produced BY `plan()` itself (output correctly contained)
    /// must still execute normally through the new pre-write containment re-check.
    #[test]
    fn ipc_bypass_containment_recheck_does_not_disturb_an_ordinary_contained_plan() {
        let d = scratch("ipc-bypass-no-false-alarm");
        let a = d.join("a.png");
        let b = d.join("b.png");
        fs::write(&a, png_bytes(20, 10)).unwrap();
        fs::write(&b, png_bytes(15, 15)).unwrap();

        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 8 }]);
        let inputs = vec![a.to_string_lossy().to_string(), b.to_string_lossy().to_string()];
        let items = plan(&job, &inputs).unwrap();

        let report = execute_plan(&items, &job)
            .unwrap_or_else(|e| panic!("an ordinary contained plan must not be refused: {e}"));
        assert_eq!(report.written, 2);

        let _ = fs::remove_dir_all(&d);
    }

    // ---- Link-as-final-component: the directory-text check alone isn't enough (reviewer, PR #828 -------
    // ---- attempt 3) ---------------------------------------------------------------------------------
    //
    // `output_escapes_input_dir`'s `out_dir == dir` fast path used to return `false` the instant the two
    // directories' TEXT matched — never asking what `output`'s final path component actually IS on disk. A
    // link whose *name* sits inside the input's own directory can alias data physically outside it. Both
    // shapes below were demonstrated on the real filesystem (see the module's link-as-final-component doc
    // paragraph); these are the permanent regression tests, proven by reading bytes off disk, not by
    // trusting a `Result`. **Negative control:** both failed against the pre-fix branch HEAD (verified
    // manually before landing this fix — `escapes == false` for the hard-link case, and the dangling-symlink
    // case sailed through `is_foreign_overwrite` with `confirmed_overwrite` still at its default `false`).

    /// **Hard link (needs no privilege on any platform, any Windows account, same volume).** A hand-built
    /// `PlannedItem` whose `output` is a hard link to a file OUTSIDE the selected folder, even though
    /// `output`'s own directory text is textually identical to `input`'s — the exact shape that fooled the
    /// old fast path. Run WITH `confirmed_overwrite: true` deliberately: this is the finding that falsified
    /// the earlier claim that flag can never license an out-of-folder write.
    #[test]
    fn link_as_final_component_hard_link_alias_is_refused_even_with_confirmed_overwrite() {
        let d = scratch("link-final-hardlink");
        let selected = d.join("selected");
        let outside = d.join("outside");
        fs::create_dir_all(&selected).unwrap();
        fs::create_dir_all(&outside).unwrap();

        let victim = outside.join("important.jpg");
        let victim_original = b"VICTIM ORIGINAL CONTENT - must not be touched".to_vec();
        fs::write(&victim, &victim_original).unwrap();

        // link.jpg's NAME lives inside `selected` (so its directory TEXT matches `input`'s), but its DATA
        // is the same file as `victim`, which lives OUTSIDE `selected`.
        let link = selected.join("link.jpg");
        crate::links::create_hard_link(&victim.to_string_lossy(), &link.to_string_lossy())
            .expect("hard link creation needs no elevation on any platform");

        let input = selected.join("photo.jpg");
        fs::write(&input, png_bytes(8, 8)).unwrap();

        let items = vec![PlannedItem {
            input: input.to_string_lossy().to_string(),
            output: link.to_string_lossy().to_string(),
            summary: "hand-built, aliasing a hard link".into(),
        }];
        let mut job = BatchJob::new(vec![MediaOp::StripMetadata]);
        job.confirmed_overwrite = true; // the finding's own demonstrated bypass

        let err = execute_plan(&items, &job).expect_err(
            "an output whose directory text matches the input's own, but whose data is hard-linked to a \
             file outside it, must be refused — even with confirmed_overwrite",
        );
        assert!(!err.is_empty(), "the refusal must carry a specific reason");

        // Byte-level proof, not a trust-the-Result check: read the victim's ACTUAL bytes back off disk.
        assert_eq!(
            fs::read(&victim).unwrap(),
            victim_original,
            "the hard-linked victim file must be byte-for-byte untouched"
        );

        let _ = fs::remove_dir_all(&d);
    }

    /// **Dangling symlink (needs zero batch-job flags).** `Path::is_file()` on a dangling symlink is
    /// `false`, so `is_foreign_overwrite` sees "nothing there yet" and `confirmed_overwrite` never needs to
    /// be set — this must still be refused by the containment re-check alone. Symlink creation needs
    /// Developer Mode / elevation on Windows; this test skips cleanly (not fails) when that's unavailable,
    /// matching `links.rs`'s own test pattern — this dev machine has Developer Mode enabled, CI may not.
    #[test]
    fn link_as_final_component_dangling_symlink_is_refused_with_no_confirmation_needed() {
        let d = scratch("link-final-dangling-symlink");
        let selected = d.join("selected");
        let outside = d.join("outside");
        fs::create_dir_all(&selected).unwrap();
        fs::create_dir_all(&outside).unwrap();

        let victim = outside.join("newly-planted.jpg"); // deliberately does NOT exist yet
        let link = selected.join("link.jpg");
        if crate::links::create_symlink(&victim.to_string_lossy(), &link.to_string_lossy()).is_err() {
            crate::skip_notice!(
                "skipping dangling-symlink containment test: could not create a symlink in this \
                 environment (Windows needs Developer Mode or elevation) — same skip pattern links.rs uses"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }
        assert!(!victim.exists(), "sanity: the symlink's target must not exist yet — the dangling case");

        let input = selected.join("photo.jpg");
        fs::write(&input, png_bytes(8, 8)).unwrap();

        let items = vec![PlannedItem {
            input: input.to_string_lossy().to_string(),
            output: link.to_string_lossy().to_string(),
            summary: "hand-built, aliasing a dangling symlink".into(),
        }];
        let job = BatchJob::new(vec![MediaOp::StripMetadata]); // confirmed_overwrite left at its default false
        assert!(!job.confirmed_overwrite, "sanity: no confirmation given — this must be refused regardless");

        let err = execute_plan(&items, &job).expect_err(
            "a dangling symlink whose name sits inside the selected folder but whose stored target names a \
             path outside it must be refused — no batch-job flag should even be necessary",
        );
        assert!(!err.is_empty(), "the refusal must carry a specific reason");

        // Byte-level proof: the victim path must never have come into existence at all.
        assert!(!victim.exists(), "the escaping write through a dangling symlink must never have happened");

        let _ = fs::remove_dir_all(&d);
    }

    // ---- CPE-1642: identity resolution, end-to-end and byte-proven ----------------------------------
    //
    // The two escapes CPE-1623's shape-matching left open, reproduced here as full `execute_plan` runs and
    // proven by reading the victim's ACTUAL bytes back off disk. **Negative control:** both were run
    // against the pre-fix code first — the chain case returned `Ok(BatchReport { written: 1 })` with the
    // outside victim's bytes changed, and the contended hard-link case likewise; see the ticket's work log.

    /// **Finding A — symlink CHAIN.** `linkA → linkB` (relative, same folder) → `outside/important.jpg`.
    /// Reading one hop saw `linkB` sitting textually inside the selected folder and waved it through.
    /// Run with `confirmed_overwrite: true`, exactly as demonstrated.
    #[test]
    fn cpe_1642_symlink_chain_alias_is_refused_and_the_victim_bytes_are_untouched() {
        let d = scratch("cpe1642-chain");
        let selected = d.join("selected");
        let outside = d.join("outside");
        fs::create_dir_all(&selected).unwrap();
        fs::create_dir_all(&outside).unwrap();

        let victim = outside.join("important.jpg");
        let victim_original = b"VICTIM ORIGINAL CONTENT - must survive a two-hop chain".to_vec();
        fs::write(&victim, &victim_original).unwrap();

        let link_b = selected.join("linkB.jpg");
        let link_a = selected.join("linkA.jpg");
        if crate::links::create_symlink(&victim.to_string_lossy(), &link_b.to_string_lossy()).is_err() {
            crate::skip_notice!(
                "SKIPPING cpe_1642_symlink_chain_alias_is_refused: could not create a symlink here \
                 (Windows needs Developer Mode or elevation) — this test verified NOTHING"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }
        if crate::links::create_symlink("linkB.jpg", &link_a.to_string_lossy()).is_err() {
            crate::skip_notice!("SKIPPING cpe_1642_symlink_chain_alias_is_refused: second hop could not be created");
            let _ = fs::remove_dir_all(&d);
            return;
        }

        let input = selected.join("photo.jpg");
        fs::write(&input, png_bytes(8, 8)).unwrap();

        let items = vec![PlannedItem {
            input: input.to_string_lossy().to_string(),
            output: link_a.to_string_lossy().to_string(),
            summary: "hand-built, aliasing a two-hop symlink chain".into(),
        }];
        let mut job = BatchJob::new(vec![MediaOp::StripMetadata]);
        job.confirmed_overwrite = true;

        let err = execute_plan(&items, &job).expect_err(
            "a symlink CHAIN whose far end lands outside the selected folder must be refused",
        );
        assert!(!err.is_empty(), "the refusal must carry a specific reason");
        assert_eq!(
            fs::read(&victim).unwrap(),
            victim_original,
            "the chain's outside victim must be byte-for-byte untouched"
        );

        let _ = fs::remove_dir_all(&d);
    }

    /// **Finding B — a contended hard link used to fail OPEN.** Same hard-link alias the CPE-1623 test
    /// above covers, but with an ordinary unprivileged process holding an exclusive handle on it, which
    /// made the old `GENERIC_READ` link-count read fail and default to "one link, nothing to see".
    #[cfg(windows)]
    #[test]
    fn cpe_1642_contended_hard_link_alias_is_refused_and_the_victim_bytes_are_untouched() {
        use std::os::windows::fs::OpenOptionsExt;

        let d = scratch("cpe1642-contended");
        let selected = d.join("selected");
        let outside = d.join("outside");
        fs::create_dir_all(&selected).unwrap();
        fs::create_dir_all(&outside).unwrap();

        let victim = outside.join("important.jpg");
        let victim_original = b"VICTIM ORIGINAL CONTENT - must survive a contended read".to_vec();
        fs::write(&victim, &victim_original).unwrap();
        let link = selected.join("link.jpg");
        crate::links::create_hard_link(&victim.to_string_lossy(), &link.to_string_lossy())
            .expect("hard link creation needs no elevation on any platform");

        let input = selected.join("photo.jpg");
        fs::write(&input, png_bytes(8, 8)).unwrap();

        // Any concurrent holder — another process, an AV scanner, a second thread of the same batch.
        let hold = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&link)
            .expect("an exclusive handle needs no privilege");

        let items = vec![PlannedItem {
            input: input.to_string_lossy().to_string(),
            output: link.to_string_lossy().to_string(),
            summary: "hand-built, aliasing a contended hard link".into(),
        }];
        let mut job = BatchJob::new(vec![MediaOp::StripMetadata]);
        job.confirmed_overwrite = true;

        let err = execute_plan(&items, &job).expect_err(
            "a hard-linked output must stay refused while the file is held exclusively — a failed read \
             must never be reported as \"not linked\"",
        );
        assert!(!err.is_empty(), "the refusal must carry a specific reason");
        drop(hold);
        assert_eq!(
            fs::read(&victim).unwrap(),
            victim_original,
            "the hard-linked victim must be byte-for-byte untouched"
        );

        let _ = fs::remove_dir_all(&d);
    }

    /// **REV-G2 (reviewer, PR #840 round 1) — the probe and the WRITER must address the same files.**
    /// Every write in this crate goes through `std::fs`, which applies Windows' `\\?\` verbatim prefix and
    /// therefore reaches paths past `MAX_PATH`. A raw `CreateFileW` does not. When the identity probe was
    /// a raw Win32 call on the unprefixed path, an over-`MAX_PATH` output failed to open with
    /// `ERROR_PATH_NOT_FOUND`, which classified as "nothing is there" — so a planted symlink in a deep
    /// folder was judged contained and the outside victim's bytes really were replaced, a case the
    /// pre-CPE-1642 code refused correctly. Byte-proven end to end.
    ///
    /// Windows-only: `MAX_PATH` is the Windows limit, and Linux/macOS `symlink_metadata` has no such
    /// truncation, so there is nothing to regress there.
    #[cfg(windows)]
    #[test]
    fn cpe_1642_over_max_path_symlink_alias_is_refused_and_the_victim_bytes_are_untouched() {
        let d = scratch("cpe1642-longpath");
        // Pad the selected folder past MAX_PATH (260). `create_dir_all` gets there because std prefixes.
        let mut deep = d.clone();
        while deep.to_string_lossy().chars().count() < 300 {
            deep = deep.join("padpadpadpadpadpadpadpadpadpadpadpadpadpad");
        }
        let selected = deep.join("selected");
        let outside = deep.join("outside");
        fs::create_dir_all(&selected).unwrap();
        fs::create_dir_all(&outside).unwrap();
        assert!(
            selected.to_string_lossy().chars().count() > 260,
            "sanity: the selected folder must sit past MAX_PATH or this test proves nothing (len {})",
            selected.to_string_lossy().chars().count()
        );

        let victim = outside.join("important.jpg");
        let victim_original = b"VICTIM ORIGINAL CONTENT - must survive a >MAX_PATH probe".to_vec();
        fs::write(&victim, &victim_original).unwrap();

        // A RELATIVE target, resolved against the link's own parent exactly as the OS does — which means
        // the probe has to cope with a `..` segment inside an over-MAX_PATH path too.
        let link_a = selected.join("linkA.jpg");
        if crate::links::create_symlink("..\\outside\\important.jpg", &link_a.to_string_lossy()).is_err() {
            crate::skip_notice!(
                "SKIPPING cpe_1642_over_max_path_symlink_alias: could not create a symlink here \
                 (Windows needs Developer Mode or elevation) — this test verified NOTHING"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        let input = selected.join("photo.jpg");
        fs::write(&input, png_bytes(8, 8)).unwrap();

        let items = vec![PlannedItem {
            input: input.to_string_lossy().to_string(),
            output: link_a.to_string_lossy().to_string(),
            summary: "hand-built, aliasing a symlink inside an over-MAX_PATH folder".into(),
        }];
        let mut job = BatchJob::new(vec![MediaOp::StripMetadata]);
        job.confirmed_overwrite = true;

        let outcome = execute_plan(&items, &job);
        // Byte-level proof FIRST, so a wrong-reason failure can't be mistaken for the right one.
        assert_eq!(
            fs::read(&victim).unwrap(),
            victim_original,
            "the victim behind an over-MAX_PATH symlink must be byte-for-byte untouched"
        );
        let err = outcome.expect_err(
            "a symlink alias inside an over-MAX_PATH folder must be refused — the probe must reach every \
             path the writer reaches",
        );
        assert!(!err.is_empty(), "the refusal must carry a specific reason");

        let _ = fs::remove_dir_all(&d);
    }

    /// **Refusal messages must state what is actually true (CPE-1642).** An output whose identity could
    /// not be established has NOT been shown to leave the folder — saying "would land outside its own
    /// input's folder" about it tells the user something false about their own files. A symlink cycle is
    /// the deterministic unverifiable case: nothing escaped, the chain simply has no real end.
    #[test]
    fn cpe_1642_unverifiable_output_is_refused_without_claiming_it_left_the_folder() {
        let d = scratch("cpe1642-message");
        let selected = d.join("selected");
        fs::create_dir_all(&selected).unwrap();
        let link_a = selected.join("linkA.jpg");
        let link_b = selected.join("linkB.jpg");
        if crate::links::create_symlink("linkB.jpg", &link_a.to_string_lossy()).is_err()
            || crate::links::create_symlink("linkA.jpg", &link_b.to_string_lossy()).is_err()
        {
            crate::skip_notice!(
                "SKIPPING cpe_1642_unverifiable_output_message: could not create a symlink here \
                 (Windows needs Developer Mode or elevation) — this test verified NOTHING"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        let input = selected.join("photo.jpg");
        fs::write(&input, png_bytes(8, 8)).unwrap();
        let items = vec![PlannedItem {
            input: input.to_string_lossy().to_string(),
            output: link_a.to_string_lossy().to_string(),
            summary: "hand-built, aliasing a cyclic symlink".into(),
        }];

        let err = execute_plan(&items, &BatchJob::new(vec![MediaOp::StripMetadata]))
            .expect_err("a cyclic link chain must be refused, not followed forever");
        assert!(
            err.contains("couldn't be verified"),
            "the refusal must say the output could not be VERIFIED: {err}"
        );
        assert!(
            !err.contains("would land outside"),
            "an unverifiable output must NOT be reported as a proven escape: {err}"
        );

        let _ = fs::remove_dir_all(&d);
    }

    /// Perf regression guard for `execute_plan_walk`'s OWN copy of the containment check (mirrors
    /// `batch_media::cpe_1623_containment_check_for_a_directory_changing_rename_stays_bounded`, which only
    /// ever covered `plan()`'s copy — reviewer finding, PR #828 attempt 3's "ALSO" follow-up: there was no
    /// equivalent guard on the execute side, so a regression there wouldn't be caught by CI). A Rename
    /// template of `"./{stem}"` changes `out_dir` TEXTUALLY (adds `"./"`) but resolves to the exact same
    /// real directory, so `execute_plan_walk`'s pre-write containment re-check genuinely runs `path_key`'s
    /// full resolution for every item (the fast path is skipped), not the near-zero-cost common case.
    #[test]
    fn cpe_1623_execute_plan_walk_containment_recheck_stays_bounded() {
        let d = scratch("cpe1623-execute-containment-perf-guard");
        let n = 300usize;
        let inputs: Vec<String> = (0..n)
            .map(|i| {
                let p = d.join(format!("photo{i:04}.jpg"));
                fs::write(&p, png_bytes(4, 4)).unwrap();
                p.to_string_lossy().to_string()
            })
            .collect();

        // "./{stem}-renamed" changes `out_dir` TEXTUALLY (adds "./", resolving to the same real directory)
        // AND genuinely renames the file, so every output is both non-in-place (no `confirmed_overwrite`
        // needed) and non-colliding — isolates the containment re-check's own cost from `is_foreign_overwrite`.
        let job = BatchJob::new(vec![MediaOp::Rename { template: "./{stem}-renamed".into() }]);
        let items = plan(&job, &inputs).unwrap();
        assert_eq!(items.len(), n);

        // Only count canonicalize calls made by execute_plan_walk itself, not plan()'s own (already
        // separately guarded) resolution above.
        crate::batch_media::reset_canonicalize_call_count();
        let report = execute_plan(&items, &job)
            .unwrap_or_else(|e| panic!("a same-directory rename must not be refused: {e}"));
        let calls = crate::batch_media::canonicalize_call_count();
        assert_eq!(report.written, n);

        // O(n) allows a small constant number of canonicalize calls per file (one `path_key` resolution
        // per item for each of out_dir/dir, both memoized after the first). O(n²) would blow far past
        // this even at n=300.
        //
        // **Raised from 10 to 14 by CPE-1624** and kept there through the PR #848 security-audit
        // rework. Measured on this worst-case shape (the `"./"` template defeats every fast path):
        // **5 calls/item on base `main`, 10 with the original path-based re-check, 7 with the
        // handle-based verification that replaced it** — the write-time check now resolves the path once
        // per item instead of twice, because the second question is answered from the open handle rather
        // than from another path resolution. The headroom is for platform variation in how many tiers
        // `path_key` falls through, not for a future doubling; a regression to per-item uncached
        // resolution would still be orders of magnitude past it.
        assert!(
            calls <= n * 14,
            "expected O(n) canonicalize calls for execute_plan_walk's containment re-check (bound {}), got \
             {calls} for n={n} files — it may have regressed to a per-item uncached resolution",
            n * 14
        );
        println!("execute_plan_walk canonicalize calls for n={n}: {calls} ({} per item)", calls / n);

        let _ = fs::remove_dir_all(&d);
    }

    /// Manual-only measurement of the write-time verification's per-item overhead. `#[ignore]`d like the
    /// other timing tests (wall-clock assertions are flaky in CI); run with
    /// `cargo test --release -- --ignored --nocapture cpe_1624_write_time_recheck_overhead`.
    ///
    /// The **control** is the same run with the verification neutralised (see the PR's
    /// guard-neutralisation table), so the two runs are directly comparable.
    #[test]
    #[ignore]
    fn cpe_1624_write_time_recheck_overhead() {
        let d = scratch("cpe1624-overhead");
        let n = 1000usize;
        let inputs: Vec<String> = (0..n)
            .map(|i| {
                let p = d.join(format!("photo{i:04}.png"));
                fs::write(&p, png_bytes(16, 16)).unwrap();
                p.to_string_lossy().to_string()
            })
            .collect();
        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 8 }]);
        let items = plan(&job, &inputs).unwrap();

        // The verification alone — open + decide on the handle, then abandon without writing.
        let start = std::time::Instant::now();
        for item in &items {
            crate::batch_media::open_output_verified(&item.input, &item.output)
                .unwrap()
                .abandon(&item.output);
        }
        let verify_only = start.elapsed();

        let start = std::time::Instant::now();
        let report = execute_plan(&items, &job).unwrap();
        let whole_walk = start.elapsed();
        assert_eq!(report.written, n);

        println!(
            "write-time verification over n={n}: verify alone {verify_only:?} ({:?}/item); whole \
             execute_plan_walk {whole_walk:?} ({:?}/item)",
            verify_only / n as u32,
            whole_walk / n as u32
        );

        let _ = fs::remove_dir_all(&d);
    }

    // ---- CPE-1624 finding A: the guards are re-asked immediately before each write -------------------

    /// **The TOCTOU, demonstrated without needing any link privilege.** Both guards used to run once,
    /// before the loop; a file that appears at a planned output *after* that scan but before its own
    /// item's write was therefore clobbered with no confirmation, because the only question ever asked
    /// about it was asked while it did not exist.
    ///
    /// The `flush` callback is the seam that makes the race deterministic: it fires after item 0 is
    /// written and before item 1 is, which is exactly the window. **Red on base `main`:** `written == 2`
    /// and the foreign file's bytes are replaced by PNG data. Green here: item 1 is skipped with a
    /// reason, the foreign file is byte-for-byte intact, and item 0 still succeeds (a late detection
    /// skips the ITEM, it does not abort a batch that has already written files).
    #[test]
    fn cpe_1624_a_file_appearing_at_a_planned_output_mid_batch_is_skipped_not_clobbered() {
        let d = scratch("cpe1624-toctou-appearing-file");
        let a = d.join("a.png");
        let b = d.join("b.png");
        fs::write(&a, png_bytes(32, 32)).unwrap();
        fs::write(&b, png_bytes(32, 32)).unwrap();

        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 16 }]);
        let inputs = vec![a.to_string_lossy().to_string(), b.to_string_lossy().to_string()];
        let items = plan(&job, &inputs).unwrap();
        assert_eq!(items.len(), 2);
        let victim_path = items[1].output.clone();
        assert!(!Path::new(&victim_path).exists(), "the second output must be free when the batch starts");

        // Plant a stranger's file at item 1's planned output the instant item 0 finishes — after the
        // up-front scan said "nothing is there", and before item 1's own write.
        let victim_bytes = b"a stranger's file, planted mid-batch".to_vec();
        let planted = std::cell::Cell::new(false);
        let report = execute_plan_walk(&items, &job, |_| {
            if !planted.get() {
                planted.set(true);
                fs::write(&victim_path, &victim_bytes).unwrap();
            }
        })
        .expect("a late collision must not fail the whole batch — the first file was already written");

        assert!(planted.get(), "the test never planted the file, so it verified nothing");
        assert_eq!(report.written, 1, "only the first item may be written");
        assert_eq!(report.skipped.len(), 1, "the second item must be skipped, not written");
        assert_eq!(report.skipped[0].0, items[1].input);
        assert!(
            report.skipped[0].1.contains("write time"),
            "the skip reason must say this was caught at write time: {}",
            report.skipped[0].1
        );
        assert_eq!(
            fs::read(&victim_path).unwrap(),
            victim_bytes,
            "the planted file's bytes must be untouched — this is the clobber the re-check prevents"
        );

        let _ = fs::remove_dir_all(&d);
    }

    /// The ticket's own worded scenario: a link whose target is swapped **mid-batch**, so a check made
    /// before the swap is stale for what actually gets written afterwards. Same `flush` seam. Skips
    /// (rather than aborts) and leaves the outside victim's bytes untouched.
    ///
    /// Skipped with a visible message where symlinks can't be created (Windows without Developer Mode),
    /// exactly like this module's other link tests — never a silent pass.
    #[test]
    fn cpe_1624_a_link_repointed_mid_batch_is_caught_at_write_time() {
        let d = scratch("cpe1624-toctou-link-swap");
        let selected = d.join("selected");
        let outside = d.join("outside");
        fs::create_dir_all(&selected).unwrap();
        fs::create_dir_all(&outside).unwrap();

        let a = selected.join("a.png");
        let b = selected.join("b.png");
        fs::write(&a, png_bytes(32, 32)).unwrap();
        fs::write(&b, png_bytes(32, 32)).unwrap();
        let inside_target = selected.join("inside-target.png");
        fs::write(&inside_target, png_bytes(8, 8)).unwrap();
        let victim = outside.join("important.png");
        let victim_bytes = b"the outside victim's original bytes".to_vec();
        fs::write(&victim, &victim_bytes).unwrap();

        // b's planned output is a symlink that, at scan time, points INSIDE the selected folder — so the
        // up-front containment check correctly says "contained".
        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 16 }]);
        let items = plan(&job, &[a.to_string_lossy().to_string(), b.to_string_lossy().to_string()]).unwrap();
        let link_path = PathBuf::from(&items[1].output);
        if crate::links::create_symlink(&inside_target.to_string_lossy(), &link_path.to_string_lossy())
            .is_err()
        {
            crate::skip_notice!(
                "SKIPPING cpe_1624_a_link_repointed_mid_batch_is_caught_at_write_time: could not create \
                 a symlink here (Windows needs Developer Mode or elevation) — this test verified NOTHING"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        // The batch is allowed to write onto a name it planned, so confirm the overwrite — this isolates
        // the containment re-check from the foreign-overwrite guard.
        let mut job = job;
        job.confirmed_overwrite = true;
        let swapped = std::cell::Cell::new(false);
        let report = execute_plan_walk(&items, &job, |_| {
            if !swapped.get() {
                swapped.set(true);
                fs::remove_file(&link_path).unwrap();
                crate::links::create_symlink(&victim.to_string_lossy(), &link_path.to_string_lossy())
                    .unwrap();
            }
        })
        .expect("a mid-batch swap must skip the item, not fail the whole batch");

        assert!(swapped.get(), "the test never swapped the link, so it verified nothing");
        assert_eq!(report.written, 1, "only the first item may be written");
        assert_eq!(report.skipped.len(), 1);
        assert!(
            report.skipped[0].1.contains("write time"),
            "the skip reason must say this was caught at write time: {}",
            report.skipped[0].1
        );
        assert_eq!(
            fs::read(&victim).unwrap(),
            victim_bytes,
            "the outside victim's bytes must be untouched — this is the write the re-check prevents"
        );

        let _ = fs::remove_dir_all(&d);
    }

    /// The negative control for both tests above: with nothing changing underneath it, an ordinary batch
    /// must run to completion. A re-check that refused everything would pass the two tests above and be
    /// useless.
    #[test]
    fn cpe_1624_the_write_time_recheck_does_not_disturb_an_ordinary_batch() {
        let d = scratch("cpe1624-negative-control");
        let inputs: Vec<String> = (0..8)
            .map(|i| {
                let p = d.join(format!("photo{i}.png"));
                fs::write(&p, png_bytes(24, 24)).unwrap();
                p.to_string_lossy().to_string()
            })
            .collect();
        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 8 }]);
        let items = plan(&job, &inputs).unwrap();

        let report = execute_plan(&items, &job).expect("an ordinary batch must not be refused");
        assert_eq!(report.written, 8, "every item must still be written: {:?}", report.skipped);
        assert!(report.skipped.is_empty(), "no false alarms: {:?}", report.skipped);

        let _ = fs::remove_dir_all(&d);
    }

    /// **CPE-1624 finding B, end to end through the real engine entry point.** The audit's PoC:
    /// `plan()` computed `…\workdir\C:foo.png` — same directory, contained — and `execute_plan` returned
    /// `Ok(written: 1)`, with 120 bytes of transformed PNG readable at the `foo.png` stream of the
    /// unrelated file `C`, whose visible size and content never changed. `Path::is_file()` on a
    /// never-before-existing stream returns `false`, so no confirmation was ever required.
    ///
    /// Hand-built `PlannedItem` on purpose: the template-level colon rule cannot see this path, which is
    /// the whole reason the refusal had to move to the shared engine boundary. **Red on base `main`:**
    /// `Ok(written: 1)` and readable bytes at the stream. Windows-only — an alternate data stream is an
    /// NTFS concept, and off Windows this same path is an ordinary (legal) filename.
    #[cfg(windows)]
    #[test]
    fn cpe_1624_a_hand_built_alternate_data_stream_output_is_refused_and_writes_nothing() {
        let d = scratch("cpe1624-ads-engine-boundary");
        let photo = d.join("photo.png");
        fs::write(&photo, png_bytes(32, 32)).unwrap();
        // The unrelated, never-selected file whose hidden stream the write would land on.
        let host = d.join("C");
        let host_bytes = b"an unrelated file the user can see".to_vec();
        fs::write(&host, &host_bytes).unwrap();

        let stream_path = format!("{}:foo.png", host.to_string_lossy());
        let items = vec![PlannedItem {
            input: photo.to_string_lossy().to_string(),
            output: stream_path.clone(),
            summary: "hand-built, targeting an NTFS alternate data stream".into(),
        }];

        for confirmed in [false, true] {
            let mut job = BatchJob::new(vec![MediaOp::Resize { max_px: 16 }]);
            job.confirmed_overwrite = confirmed;
            let err = execute_plan(&items, &job).expect_err(
                "an alternate-data-stream output must be refused — confirmed_overwrite authorises \
                 overwriting a FILE, never writing hidden bytes onto an unrelated one",
            );
            assert!(
                err.contains("alternate data stream"),
                "the refusal must name the real reason: {err}"
            );
            assert!(
                !err.contains("would land outside"),
                "an ADS output does NOT leave the folder — saying so would be untrue: {err}"
            );
        }

        assert!(fs::read(&stream_path).is_err(), "no bytes may exist at the stream path");
        assert_eq!(fs::read(&host).unwrap(), host_bytes, "the host file must be untouched");

        let _ = fs::remove_dir_all(&d);
    }

    /// **Pins the one direction in which CPE-1624's ADS-aware identity could *relax* a rule.** Making
    /// `X.png:evil` and `X.png` compare as the same file is conservative everywhere except
    /// [`is_foreign_overwrite`]'s "the output is one of this batch's OWN inputs, so it's permitted" arm —
    /// where fusing them would license the hidden write with no confirmation at all. It is unreachable
    /// because the containment refusal is co-gated with the stripping and runs first; this test is what
    /// keeps that true if either gate is ever moved. See `strip_stream_suffix`'s reach audit.
    #[cfg(windows)]
    #[test]
    fn cpe_1624_an_alternate_stream_of_the_batchs_own_input_is_still_refused() {
        let d = scratch("cpe1624-ads-of-own-input");
        let photo = d.join("photo.png");
        fs::write(&photo, png_bytes(32, 32)).unwrap();
        let photo_bytes = fs::read(&photo).unwrap();

        // The stream's host file IS this batch's own (only) input — the case the "permitted" arm covers.
        let items = vec![PlannedItem {
            input: photo.to_string_lossy().to_string(),
            output: format!("{}:evil.png", photo.to_string_lossy()),
            summary: "hand-built, ADS of the batch's own input".into(),
        }];
        let err = execute_plan(&items, &BatchJob::new(vec![MediaOp::Resize { max_px: 16 }]))
            .expect_err("an ADS output must be refused even when its host is one of the batch's inputs");
        assert!(err.contains("alternate data stream"), "refusal reason: {err}");
        assert!(fs::read(&items[0].output).is_err(), "no bytes may exist at the stream path");
        assert_eq!(fs::read(&photo).unwrap(), photo_bytes, "the input must be untouched");

        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1652 finding B through the real engine entry point: past the census cap the verdict degrades
    /// to `Unverifiable`, and `execute_plan_walk` refuses — **fail closed, not fail slow, and never
    /// "allowed because we stopped looking"**. The uncapped run in the same test is the positive control.
    #[test]
    fn cpe_1652_a_census_past_the_cap_refuses_the_write_rather_than_allowing_it() {
        let d = scratch("cpe1652-census-cap-engine");
        let photo = d.join("photo.png");
        fs::write(&photo, png_bytes(32, 32)).unwrap();
        let a = d.join("shared.png");
        fs::write(&a, png_bytes(8, 8)).unwrap();
        let b = d.join("shared-2.png");
        if fs::hard_link(&a, &b).is_err() {
            crate::skip_notice!(
                "SKIPPING cpe_1652_a_census_past_the_cap_refuses_the_write: no hard-link support here \
                 — this test verified NOTHING"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }
        for i in 0..6 {
            fs::write(d.join(format!("filler{i}.png")), b"filler").unwrap();
        }

        let items = vec![PlannedItem {
            input: photo.to_string_lossy().to_string(),
            output: b.to_string_lossy().to_string(),
            summary: "writes onto a multiply-linked name inside the folder".into(),
        }];
        let mut job = BatchJob::new(vec![MediaOp::Resize { max_px: 16 }]);
        job.confirmed_overwrite = true; // the overwrite itself is authorised; only the census is at issue

        // Positive control: uncapped, every name is accounted for inside the folder, so this is allowed.
        crate::batch_media::set_census_cap_for_test(None);
        let ok = execute_plan(&items, &job);
        assert!(ok.is_ok(), "an uncapped census must still allow a wholly-inside hard link: {ok:?}");

        // Capped below the folder's entry count: refuse rather than guess.
        crate::batch_media::set_census_cap_for_test(Some(1));
        let err = execute_plan(&items, &job);
        crate::batch_media::set_census_cap_for_test(None);
        let err = err.expect_err("past the cap the engine must refuse, not write");
        assert!(
            err.contains("too many entries"),
            "the refusal must say the folder was too big to account for, not blame a lock: {err}"
        );

        let _ = fs::remove_dir_all(&d);
    }

    // ==== SECURITY AUDIT (PR #848) — end-to-end attack attempts ======================================

    /// **Security audit finding 4.** `is_foreign_overwrite` was correct only *in the order the engine
    /// happens to call it* — `classify_output_containment` refuses an alternate-data-stream path first.
    /// That is an unenforced convention, not a guarantee, and on its own the function fails open for a
    /// stream: a never-before-existing stream path makes `Path::is_file()` false, so the early return
    /// says "nothing sits there — not an overwrite of anything", and the write is permitted with no
    /// confirmation. It now refuses a stream itself, so it is correct **standalone, in any order**.
    ///
    /// The host here is deliberately a *different* file from `item.input` and does not exist, so neither
    /// the same-file arm nor the `is_file` arm can mask the check being tested. Red with the stream
    /// refusal neutralised (B6); green with it.
    #[cfg(windows)]
    #[test]
    fn secaudit_is_foreign_overwrite_refuses_a_stream_output_without_relying_on_call_order() {
        let items = vec![PlannedItem {
            input: r"C:\pics\photo.png".into(),
            output: r"C:\pics\unrelated.png:evil.png".into(),
            summary: "an alternate data stream of an unrelated, non-existent file".into(),
        }];
        assert!(
            is_foreign_overwrite(&items[0], &input_path_keys(&items)),
            "a stream-named output must be refused by this function alone, with no other check having \
             run and nothing existing at the path to trip the is_file() arm"
        );
        assert!(any_in_place(&items), "and therefore by the predicate built on it");
    }

    // ---- CPE-1696: the stat-collapse fail-open into a silent overwrite ------------------------------
    //
    // `is_foreign_overwrite`'s second gate was `if !Path::new(&item.output).is_file() { return false }`.
    // `Path::is_file()` is `metadata().map(|m| m.is_file()).unwrap_or(false)` — it folds EVERY stat
    // failure into `false`, i.e. into "nothing sits there — not an overwrite of anything". So an output
    // path the process could not stat passed `execute_plan_walk`'s up-front refusal check and the write
    // proceeded with no confirmation. Same shape as CPE-1678/1687/1692; this is the fifth round.

    /// The deterministic half (runs on every OS and account, no privilege needed) — same role as
    /// `crate::dispatch::classify_path_error`'s own unit tests. Pins the taxonomy the wiring below
    /// depends on: an absence is `Free`, a real file is `File`, and **every** other stat failure is
    /// `Unknown`, never `Free`.
    #[test]
    fn cpe_1696_only_a_genuine_absence_reads_as_a_free_output_path() {
        let never_called = || panic!("metadata must not be consulted when try_exists already answered");
        assert_eq!(
            classify_output_occupancy(Ok(false), never_called),
            OutputOccupancy::Free,
            "a genuinely absent output path is free"
        );
        for kind in [
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::Other,
            std::io::ErrorKind::TimedOut,
        ] {
            assert_eq!(
                classify_output_occupancy(Err(std::io::Error::new(kind, "Access is denied.")), never_called),
                OutputOccupancy::Unknown,
                "{kind:?} on the existence probe must never read as a free path — that is the fail-open"
            );
            // The same must hold when it's the *second* call that fails: try_exists said the path is
            // there, so we already know it is NOT free, and the type probe failing cannot make it free.
            assert_eq!(
                classify_output_occupancy(Ok(true), || Err(std::io::Error::new(kind, "Access is denied."))),
                OutputOccupancy::Unknown,
                "{kind:?} on the type probe must never read as a free path either"
            );
        }
        assert_eq!(
            classify_output_occupancy(
                Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
                never_called
            ),
            OutputOccupancy::Free,
            "an explicit NotFound is a genuine absence"
        );
        assert_eq!(
            classify_output_occupancy(Ok(true), || Err(std::io::Error::from(std::io::ErrorKind::NotFound))),
            OutputOccupancy::Free,
            "a TOCTOU vanish between the two calls is a genuine absence"
        );
        // Real-filesystem legs for the two `Ok` arms, so the classifier's meaning is pinned against actual
        // syscalls and not only against synthesised results.
        let d = scratch("cpe1696-occupancy");
        let f = d.join("real.png");
        fs::write(&f, png_bytes(4, 4)).unwrap();
        assert_eq!(output_occupancy(&f), OutputOccupancy::File, "a real file occupies the path");
        assert_eq!(
            output_occupancy(&d.join("nope.png")),
            OutputOccupancy::Free,
            "a path that isn't there is free"
        );
        assert_eq!(
            output_occupancy(&d),
            OutputOccupancy::Free,
            "a directory answers exactly what the pre-CPE-1696 `!is_file()` gate answered for it"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// Drives `is_foreign_overwrite` with a **real, OS-refused** output path, on **every** platform.
    ///
    /// **Why the output path is a DIRECTORY.** It has to be, for the Windows leg to have any power over
    /// the bug. `fsutil::deny_stat_of` makes `try_exists()` on the target fail while `fs::metadata()` on
    /// the identical target still succeeds (PR #874's measurement: on Windows the two are different
    /// syscalls, and only the attributes query is refused by a deny ACE). If the denied output were a
    /// regular *file*, the pre-fix `!is_file()` gate would still see `metadata → Ok(is_file: true)` and
    /// refuse — the right answer by the wrong route, i.e. a test that passes against the bug and proves
    /// nothing. Against a denied *directory* the two answers diverge: the old gate reads `is_file: false`
    /// → "free" → not an overwrite, while the fixed gate reads `Err` → `Unknown` → refuse.
    ///
    /// **Why input and output sit in SEPARATE parent directories.** This is what lets the Unix leg run
    /// for real instead of skipping, and it is the whole reason this test is not Windows-only.
    /// `deny_stat_of`'s Unix mechanism is `chmod 0o000` on the target's *parent* (CPE-1687: POSIX `stat()`
    /// needs `+x` on the parent, not on the file), so a shared parent would deny the input too and the
    /// test would be measuring the wrong thing. `is_foreign_overwrite` reads only `same_file(input,
    /// output)` and the output's own occupancy — it never compares the two directories — so separate
    /// parents exercise it faithfully. (The *caller* does require a shared directory, via
    /// `classify_output_containment`; that is the end-to-end sibling's problem, not this one's.)
    ///
    /// **Asserted on `is_foreign_overwrite` DIRECTLY, deliberately.** The precedent is this module's own
    /// `secaudit_is_foreign_overwrite_refuses_a_stream_output_without_relying_on_call_order`: PR #848's
    /// finding 4 was that this function must be correct *standalone*, because "some other check happens to
    /// run first" is an unenforced call-ordering convention, not a guarantee. At today's two live entry
    /// points `execute_plan_walk`'s step-1 containment check reaches a denied output first (measured — see
    /// the PR body), so the fail-open is *mitigated* there, exactly as the ADS route was before #848 closed
    /// it. That mitigation is reported rather than relied on: it is a property of the caller, and a third
    /// caller in the wrong order would reopen it.
    #[test]
    fn cpe_1696_is_foreign_overwrite_refuses_an_output_it_cannot_stat() {
        use std::io::Write;
        let d = scratch("cpe1696-denied-output");
        // Separate parents on purpose — see the doc comment. `out_parent` is what the Unix deny chmods.
        let in_parent = d.join("in");
        let out_parent = d.join("out");
        fs::create_dir_all(&in_parent).unwrap();
        fs::create_dir_all(&out_parent).unwrap();
        let input = in_parent.join("in.png");
        fs::write(&input, png_bytes(8, 8)).unwrap();
        let output = out_parent.join("occupied");
        fs::create_dir_all(&output).unwrap();

        // Armed BEFORE the deny so cleanup runs on every exit path, panic or not (mirrors split_join.rs's
        // `Restore` pattern — Evidence Rules: a red run must never leave debris behind).
        struct Restore<'a>(&'a Path, &'a Path, &'a Path);
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                crate::fsutil::undo_deny_stat_of(self.0, self.1);
                let _ = fs::remove_dir_all(self.2);
            }
        }
        let _restore = Restore(&output, &out_parent, &d);

        if !crate::fsutil::deny_stat_of(&output) {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1696] SKIPPED the is_foreign_overwrite denied-output leg: could not deny stat of {} \
                 on this machine (running elevated/root, or a filesystem that ignores ACLs/mode bits). \
                 NOTHING in this test covered CPE-1696 for is_foreign_overwrite on this run; the taxonomy \
                 is covered on every platform by \
                 cpe_1696_only_a_genuine_absence_reads_as_a_free_output_path.",
                output.display()
            );
            return;
        }
        // The input must still be readable, or the deny landed on the wrong thing and the assertion below
        // would be measuring a broken tree rather than the guard.
        assert!(input.try_exists().unwrap_or(false), "sanity: the input is still readable");

        let items = vec![PlannedItem {
            input: input.to_string_lossy().to_string(),
            output: output.to_string_lossy().to_string(),
            summary: "an output path whose stat is refused".into(),
        }];
        assert!(
            is_foreign_overwrite(&items[0], &input_path_keys(&items)),
            "an output path whose stat we were refused must be treated as a possible overwrite by this \
             function alone — the pre-CPE-1696 `!Path::is_file()` gate read the refusal as `false`, i.e. \
             \"nothing sits there\", and permitted the write with no confirmation"
        );
        assert!(any_in_place(&items), "and therefore by the predicate built on it");
    }

    /// The end-to-end half: the real [`execute_plan`] entry point must refuse a batch whose output path
    /// cannot be stat'd, and write nothing. Also documents *which* guard gets there first today — see the
    /// sibling above for why that ordering is reported rather than depended on.
    ///
    /// **Windows-only, and the whole body is `#[cfg]`'d rather than early-returned**, because a
    /// `#[cfg(not(windows))] { ..; return; }` block makes every following statement an `unreachable
    /// statement` error under CI's `-D warnings` on Linux and macOS — invisible from a Windows dev box.
    /// Unlike its `is_foreign_overwrite` sibling, this one cannot use separate parent directories to unlock
    /// the Unix mechanism: `execute_plan`'s own `classify_output_containment` requires the output to sit in
    /// the input's directory, so `deny_stat_of`'s Unix `chmod 0o000` on that shared parent would deny the
    /// input and the canary read too, and the test would be measuring a broken tree instead of the guard.
    #[test]
    fn cpe_1696_execute_plan_refuses_a_denied_output_path() {
        #[cfg(not(windows))]
        {
            use std::io::Write;
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1696] SKIPPED the execute_plan denied-output leg on this platform: no Unix \
                 stat-deny exists that leaves the input's own directory readable, and \
                 `classify_output_containment` requires input and output to share a directory. NOTHING \
                 in this test covered CPE-1696 on this run; the taxonomy and the standalone-function \
                 legs cover it on every platform."
            );
        }
        #[cfg(windows)]
        {
            use std::io::Write;
            let d = scratch("cpe1696-denied-e2e");
            let holder = d.join("holder");
            fs::create_dir_all(&holder).unwrap();
            let input = holder.join("in.png");
            fs::write(&input, png_bytes(8, 8)).unwrap();
            let output = holder.join("occupied");
            fs::create_dir_all(&output).unwrap();
            let canary = output.join("canary.txt");
            fs::write(&canary, b"MUST SURVIVE").unwrap();

            struct Restore<'a>(&'a Path, &'a Path, &'a Path);
            impl Drop for Restore<'_> {
                fn drop(&mut self) {
                    crate::fsutil::undo_deny_stat_of(self.0, self.1);
                    let _ = fs::remove_dir_all(self.2);
                }
            }
            let _restore = Restore(&output, &holder, &d);

            if !crate::fsutil::deny_stat_of(&output) {
                let _ = writeln!(
                    std::io::stderr(),
                    "[CPE-1696] SKIPPED the execute_plan denied-output leg: could not deny stat of {} \
                     on this machine (running elevated, or a filesystem that ignores ACLs). NOTHING in \
                     this test covered CPE-1696 on this run.",
                    output.display()
                );
                return;
            }

            let items = vec![PlannedItem {
                input: input.to_string_lossy().to_string(),
                output: output.to_string_lossy().to_string(),
                summary: "an output path whose stat is refused".into(),
            }];
            // confirmed_overwrite left at its default false
            let job = BatchJob::new(vec![MediaOp::StripMetadata]);
            assert!(!job.confirmed_overwrite, "sanity: no confirmation was given");

            let err = execute_plan(&items, &job).expect_err(
                "an output path we could not stat must be refused, not waved through as an empty slot",
            );
            assert!(!err.is_empty(), "the refusal must carry a specific reason");
            // Byte-level proof, not a trust-the-Result check.
            assert_eq!(
                fs::read(&canary).unwrap(),
                b"MUST SURVIVE".to_vec(),
                "nothing under the denied output path may have been touched"
            );
        }
    }

    /// The honest case, at the same real entry point (Evidence Rules: a guard that refuses everything is
    /// not a guard). Runs on every OS with no privilege needed.
    #[test]
    fn cpe_1696_a_genuinely_free_output_path_still_runs_without_confirmation() {
        let d = scratch("cpe1696-free-output");
        let input = d.join("in.png");
        fs::write(&input, png_bytes(8, 8)).unwrap();
        let output = d.join("out.png"); // deliberately does not exist
        assert!(!output.exists(), "sanity: the output slot really is empty");

        let items = vec![PlannedItem {
            input: input.to_string_lossy().to_string(),
            output: output.to_string_lossy().to_string(),
            summary: "a fresh, non-colliding output".into(),
        }];
        let job = BatchJob::new(vec![MediaOp::StripMetadata]);
        let report = execute_plan(&items, &job)
            .expect("a genuinely free output path must not be refused as a possible overwrite");
        assert_eq!(report.written, 1, "and the write must actually happen: {report:?}");
        let _ = fs::remove_dir_all(&d);
    }

    /// **SEC-8 (executed by the audit, now a permanent regression test).** The stale `dir_scans` memo let
    /// an attacker swap an *inside* hard link for an *outside* one mid-batch: the link count is unchanged
    /// at 2, so only a census can tell the difference, and the write-time check replayed the pre-swap
    /// census. Measured against the vulnerable build: `written = 2`, `skipped = []`, and
    /// `outside\exfil.png` went 34 → 168 bytes holding the batch's output.
    ///
    /// **The auditor's two assertions are deliberately inverted here.** Theirs asserted the exploit
    /// (`assert_ne!(landed, original)` — "the outside bytes DID change"), which is how a demonstration
    /// proves itself. As a regression test the security property is the opposite: the outside file must
    /// be byte-for-byte untouched, and the item must be skipped. The `skipped.len() == 1` assertion is
    /// unchanged — it was already the right way round and was the half that went red.
    #[test]
    fn secaudit_e2e_stale_census_memo_writes_outside_the_selected_folder() {
        let d = scratch("secaudit-memo-e2e");
        let selected = d.join("selected");
        let outside = d.join("outside");
        fs::create_dir_all(&selected).unwrap();
        fs::create_dir_all(&outside).unwrap();

        let first = selected.join("first.png");
        let photo = selected.join("photo.png");
        fs::write(&first, png_bytes(32, 32)).unwrap();
        fs::write(&photo, png_bytes(64, 64)).unwrap();
        // Resize names its output "<stem>-<max_px>.<ext>", so photo.png's output is photo-16.png. The
        // attacker pre-plants that name PLUS a second INSIDE hard link to it: links = 2, census sees 2.
        let target = selected.join("photo-16.png");
        fs::write(&target, b"the attacker's planted target file").unwrap();
        let decoy = selected.join("decoy.png");
        if fs::hard_link(&target, &decoy).is_err() {
            crate::skip_notice!("SKIP secaudit_e2e: no hard links on this fs — VERIFIED NOTHING");
            return;
        }
        let original = fs::read(&target).unwrap();
        let original_len = original.len();

        let mut job = BatchJob::new(vec![MediaOp::Resize { max_px: 16 }]);
        job.confirmed_overwrite = true; // the user confirmed overwriting an existing name
        job.non_destructive = false;
        let items =
            plan(&job, &[first.to_string_lossy().to_string(), photo.to_string_lossy().to_string()])
                .unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].output, target.to_string_lossy().to_string(), "planned output is the target");

        // The attacker's window: after the up-front scan (which memoized the census) and before item 1's
        // own write, drop the inside second name and create an outside one. Link count is UNCHANGED at 2.
        let outside_name = outside.join("exfil.png");
        let swapped = std::cell::Cell::new(false);
        let report = execute_plan_walk(&items, &job, |_| {
            if !swapped.get() {
                swapped.set(true);
                fs::remove_file(&decoy).unwrap();
                fs::hard_link(&target, &outside_name).unwrap();
            }
        })
        .expect("the batch runs");

        assert!(swapped.get(), "the swap never happened — this test verified NOTHING");
        let landed = fs::read(&outside_name).unwrap();
        println!( // NOT-A-SKIP-NOTICE: "skipped" here is the batch report's own field; this test asserts unconditionally above and declines nothing
            "SEC-8 report: written={} skipped={:?}; outside file now {} bytes (was {})",
            report.written,
            report.skipped,
            landed.len(),
            original_len
        );
        let photo_now = fs::read(&target).unwrap();
        println!(
            "SEC-8 target now {} bytes (was {}); outside link {} bytes; same_bytes_as_target={}",
            photo_now.len(),
            original_len,
            landed.len(),
            photo_now == landed
        );
        assert_eq!(
            landed, original,
            "EXPLOIT: the file OUTSIDE the selected folder now holds different bytes — the batch wrote \
             through a name it was never allowed to touch ({outside_name:?})"
        );
        assert_eq!(
            report.skipped.len(),
            1,
            "EXPLOIT: item 1 was written even though one of its names now lives outside the selected \
             folder — {outside_name:?} now holds the batch's output bytes"
        );
        assert!(
            report.skipped[0].1.contains("write time"),
            "the skip must be attributed to the write-time verification: {}",
            report.skipped[0].1
        );

        let _ = fs::remove_dir_all(&d);
    }

    /// **SEC-7 (the audit's window measurement, kept and inverted).** The auditor asked how wide the gap
    /// is between the safety decision and the actual write, and measured the vulnerable answer: the check
    /// passed in 25 µs and `execute_one` then spent 528 ms reading and transforming the image before
    /// writing, so the entire transform sat inside the window. Their assertion (`transform < guard`) was
    /// written to fail on that build and is meaningless once the ordering is fixed.
    ///
    /// The regression form asserts the property the fix actually establishes: **the transform happens
    /// before verification, so the window no longer contains it.** This used to be a duration ratio
    /// (`window * 10 < transform`); a ratio bound cannot survive both build profile and a shared CI
    /// runner (PR #856 review — this bound went red at 9.1× on a saturated runner while the branch's own
    /// logic was fine), so it is now the exact claim the property actually is: an **ordering** check on
    /// [`WindowTrace`]'s monotonic timestamps via [`assert_transform_precedes_window`] — the transform's
    /// whole interval must end at or before the window opens. That is immune to both concerns and cannot
    /// flake for either reason; it can only fail if the ordering itself regresses.
    #[test]
    fn secaudit_the_transform_is_no_longer_inside_the_verify_to_write_window() {
        let d = scratch("secaudit-gap");
        let photo = d.join("photo.png");
        fs::write(&photo, png_bytes(2000, 2000)).unwrap();
        let mut job = BatchJob::new(vec![MediaOp::Resize { max_px: 1000 }]);
        job.confirmed_overwrite = true;
        let items = plan(&job, &[photo.to_string_lossy().to_string()]).unwrap();

        // The trace is recorded from inside `execute_one`, driven through the real `execute_plan`.
        let report = execute_plan(&items, &job).expect("the batch runs");
        assert_eq!(report.written, 1);

        assert_transform_precedes_window("SEC-7");

        let _ = fs::remove_dir_all(&d);
    }

    /// **CPE-1667 — SEC-7 covered only `created == true`.** A 1-item batch with `confirmed_overwrite =
    /// true` always takes that branch: `!verified.created()` is false, so `is_foreign_overwrite` short
    /// circuits away and never runs at all. It pinned the branch that was already fine and said nothing
    /// about the other one, where `is_foreign_overwrite` sits **inside** the window.
    ///
    /// This forces `!created && !confirmed_overwrite`: item 0's planned output is made to be a real,
    /// pre-existing file — chained onto the batch's OWN last input (see [`input_path_keys`]'s doc for why
    /// that shape, not an arbitrary existing file, is what's needed) — so `open_output_verified` reports
    /// `created == false`, and `confirmed_overwrite` is left at its default `false` so the flag doesn't
    /// short-circuit `is_foreign_overwrite` away.
    ///
    /// **The trap this avoids (per the ticket: the auditor's worktree was destroyed mid-measurement).** If
    /// item 0's output is made to collide with a file that is NOT one of the batch's own inputs,
    /// `is_foreign_overwrite` correctly answers "foreign" and `execute_one` refuses before ever reaching
    /// `write_all` — `.unwrap_or_else(|e| panic!(...))` below turns that refusal into a hard test failure
    /// rather than a silent "pass" having measured nothing, exactly as SEC-7's own copy guards the same
    /// trap (see [`assert_transform_precedes_window`]'s own `.expect`s for the second line of defence).
    ///
    /// **Not a duration ratio, for the same reason SEC-7 no longer is one (PR #856 review).** This used to
    /// assert `window * 10 < transform`; the reviewer restored the GENUINE pre-CPE-1667 pairwise scan and
    /// that assertion still passed with a 24× margin under `cargo test` (a debug build's transform is slow
    /// enough to hide even the O(n) cost it was meant to catch) — the exact defect this ticket exists to
    /// fix, re-introduced in a new assertion. The deterministic canonicalize-count test below is what owns
    /// the O(1) claim (it caught that same restoration at exactly 898 calls); this test's job is only the
    /// ordering claim [`assert_transform_precedes_window`] checks, which a ratio was never suited to prove
    /// or disprove in the first place.
    #[test]
    fn cpe_1667_the_not_created_branch_window_stays_narrow_across_a_chained_batch() {
        let d = scratch("cpe1667-not-created-window");
        let n = 300usize;

        let photo = d.join("photo0000.png");
        fs::write(&photo, png_bytes(2000, 2000)).unwrap();

        // The batch's own last input — the real object item 0's output is chained onto.
        let last_input = d.join(format!("photo{:04}.png", n - 1));
        let last_input_orig = png_bytes(4, 4);
        fs::write(&last_input, &last_input_orig).unwrap();

        let mut items: Vec<PlannedItem> = vec![PlannedItem {
            input: photo.to_string_lossy().to_string(),
            output: last_input.to_string_lossy().to_string(),
            summary: "item under test — output chained onto the batch's own last input".into(),
        }];
        for i in 1..n {
            items.push(PlannedItem {
                input: d.join(format!("photo{i:04}.png")).to_string_lossy().to_string(),
                output: d.join(format!("photo{i:04}-out.png")).to_string_lossy().to_string(),
                summary: "chain filler — never executed, only present so the batch is genuinely n items \
                          wide for is_foreign_overwrite's own scan"
                    .into(),
            });
        }
        assert_eq!(items.len(), n);

        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 1000 }]);
        // Lazily built, memoized on first use — exactly like `execute_plan_walk` does (CPE-1667/CPE-1674).
        let input_keys = LazyInputKeys::new(&items);

        // The trace is recorded from inside this real `execute_one` call.
        execute_one(&items[0], &job, &input_keys).unwrap_or_else(|e| {
            panic!(
                "the batch's own last input must be a permitted write target for item 0's output, not a \
                 refused foreign overwrite: {e}"
            )
        });

        // The overwrite genuinely happened — this is the "return false, item actually written" the ticket
        // asked for, not a refusal that happened to also leave the trace unset.
        let written = fs::read(&last_input).unwrap();
        assert_ne!(written, last_input_orig, "item 0 must actually have been written, not skipped");

        assert_transform_precedes_window("CPE-1667 !created branch (n=300, match at the far end)");

        let _ = fs::remove_dir_all(&d);
    }

    /// **CPE-1667.** The wall-clock test above is loose on purpose (it can't afford to flake on a loaded
    /// CI runner), so it cannot by itself prove the fix is `O(1)` rather than merely "fast enough at
    /// n=300". This is the precise, deterministic companion: it counts real `canonicalize` syscalls
    /// (the same seam [`crate::batch_media::canonicalize_call_count`] backs other perf-regression guards
    /// in this crate) rather than timing anything, isolates `is_foreign_overwrite` from everything else
    /// `execute_one` does, and asserts a small constant bound that an `O(n)` scan reaching all the way to
    /// the far end of a 300-item batch could not possibly hit.
    ///
    /// Same chain shape as the test above and for the same reason: item 0's output equals the batch's
    /// LAST item's input, so the pairwise scan `is_foreign_overwrite` used to run would have had to walk
    /// past every other item before finding its match — the worst case for that algorithm, not one that
    /// happens to terminate early no matter which algorithm runs.
    #[test]
    fn cpe_1667_is_foreign_overwrite_costs_a_bounded_number_of_canonicalize_calls_regardless_of_batch_size(
    ) {
        let d = scratch("cpe1667-foreign-overwrite-o1");
        let n = 300usize;

        let first_input = d.join("photo0000.png");
        fs::write(&first_input, png_bytes(4, 4)).unwrap();
        let last_input = d.join(format!("photo{:04}.png", n - 1));
        fs::write(&last_input, png_bytes(4, 4)).unwrap();

        let mut items: Vec<PlannedItem> = vec![PlannedItem {
            input: first_input.to_string_lossy().to_string(),
            output: last_input.to_string_lossy().to_string(),
            summary: "item under test".into(),
        }];
        for i in 1..n {
            items.push(PlannedItem {
                input: d.join(format!("photo{i:04}.png")).to_string_lossy().to_string(),
                output: d.join(format!("photo{i:04}-out.png")).to_string_lossy().to_string(),
                summary: "chain filler".into(),
            });
        }
        assert_eq!(items.len(), n);

        let input_keys = input_path_keys(&items);
        crate::batch_media::reset_canonicalize_call_count();
        let foreign = is_foreign_overwrite(&items[0], &input_keys);
        let calls = crate::batch_media::canonicalize_call_count();

        assert!(!foreign, "item 0's output IS one of the batch's own inputs, so this must be permitted");
        println!(
            "CPE-1667 is_foreign_overwrite canonicalize calls for n={n}, match at the far end: {calls}"
        );
        assert!(
            calls <= 4,
            "EXPECTED O(1): is_foreign_overwrite made {calls} canonicalize call(s) against a {n}-item \
             batch whose matching input sits at the far end — a bound this small is unreachable for an \
             O(n) pairwise scan that has to walk the whole batch to find it"
        );

        let _ = fs::remove_dir_all(&d);
    }

    /// **CPE-1674 — the PR #856 re-review, second round.** `input_path_keys` used to be built
    /// unconditionally, before `execute_plan_walk`'s loop even starts — including when
    /// `job.confirmed_overwrite` is `true`, where nothing ever consults it (the up-front refusal scan is
    /// skipped entirely, and `execute_one`'s in-window check short-circuits on `!job.confirmed_overwrite`
    /// before ever reaching it). That delayed the first file being written on a large batch for zero
    /// reason. [`LazyInputKeys`] fixes it by making the build a `OnceCell`, populated only by a caller that
    /// actually needs an answer.
    ///
    /// Deterministic, like its CPE-1667 sibling above: every one of these `n` items has a fresh,
    /// never-before-existing, SAME-DIRECTORY output, so `classify_output_containment`'s fast path (textually
    /// identical directories) and `open_output_verified`'s own re-check never touch `path_key` either —
    /// the only path to a `canonicalize_path` call anywhere in this run is `input_path_keys` itself. A
    /// confirmed-overwrite batch must make **zero**.
    ///
    /// Proved red without the fix by reverting `execute_plan_walk` to call `input_path_keys(items)`
    /// unconditionally (CPE-1667's original shape): this test then measured `calls = 300` (one
    /// `canonicalize_path` per item's input) against the `== 0` assertion below.
    #[test]
    fn cpe_1674_a_confirmed_overwrite_batch_never_builds_the_input_key_set() {
        let d = scratch("cpe1674-lazy-input-keys");
        let n = 300usize;

        let mut items: Vec<PlannedItem> = Vec::with_capacity(n);
        for i in 0..n {
            let input = d.join(format!("photo{i:04}.png"));
            fs::write(&input, png_bytes(4, 4)).unwrap();
            let output = d.join(format!("photo{i:04}-out.png")); // same dir; never exists beforehand
            items.push(PlannedItem {
                input: input.to_string_lossy().to_string(),
                output: output.to_string_lossy().to_string(),
                summary: "item under test — fresh, same-directory output".into(),
            });
        }
        assert_eq!(items.len(), n);

        let mut job = BatchJob::new(vec![MediaOp::Resize { max_px: 1000 }]);
        job.confirmed_overwrite = true;

        crate::batch_media::reset_canonicalize_call_count();
        let report = execute_plan(&items, &job).expect("a confirmed batch runs");
        let calls = crate::batch_media::canonicalize_call_count();

        assert_eq!(report.written, n, "every item must actually have been written, not skipped");
        println!("CPE-1674 canonicalize calls for a confirmed-overwrite {n}-item batch: {calls}");
        assert_eq!(
            calls, 0,
            "EXPECTED ZERO: a confirmed_overwrite batch must never build `input_path_keys` — {calls} \
             canonicalize call(s) were made against a {n}-item batch whose keys are never consulted; \
             non-zero here is exactly the eager, unconditional pre-loop build this ticket removes"
        );

        let _ = fs::remove_dir_all(&d);
    }

    /// **SEC-9 (executed by the audit, now a permanent regression test) — the highest-severity finding.**
    /// A racing, unprivileged process creates the planned output name as a **hard link** to a file outside
    /// the selected folder (`mklink /H`, no elevation, no symlink privilege) after the safety check has
    /// passed but while the transform is still running. On the vulnerable build the write followed that
    /// name: `written = 1`, `skipped = []`, victim 35 → 17120 bytes, with `confirmed_overwrite` false and
    /// never consulted, because at check time the output did not exist at all.
    ///
    /// **Two independent defences now cover it, and which one fires depends on when the racer lands.**
    /// The test asserts the security property either way and reports which:
    /// - Racer lands **before** the output is opened (the usual case here — it sleeps 300 ms, the
    ///   transform takes ~500 ms): the open finds an existing file whose link count is 2 while the
    ///   selected folder holds only one of those names, so it is refused and the item is skipped.
    /// - Racer lands **after**: it fails outright, because the atomic `create_new` already claimed the
    ///   name. Nothing to detect — the attack cannot be staged at all.
    ///
    /// The auditor's `assert!(linked)` precondition is therefore relaxed into a branch: with the fix, a
    /// failed `hard_link` is itself the defence rather than a test that verified nothing. The victim-bytes
    /// assertion — the one that went red — is unchanged.
    #[test]
    fn secaudit_race_between_the_guard_and_the_write_clobbers_an_outside_file() {
        let d = scratch("secaudit-race");
        let selected = d.join("selected");
        let outside = d.join("outside");
        fs::create_dir_all(&selected).unwrap();
        fs::create_dir_all(&outside).unwrap();

        let photo = selected.join("photo.png");
        fs::write(&photo, png_bytes(2000, 2000)).unwrap();
        let victim = outside.join("victim.txt");
        let victim_bytes = b"the outside victim's original bytes".to_vec();
        fs::write(&victim, &victim_bytes).unwrap();

        let mut job = BatchJob::new(vec![MediaOp::Resize { max_px: 1000 }]);
        job.non_destructive = false; // confirmed_overwrite deliberately left FALSE
        let items = plan(&job, &[photo.to_string_lossy().to_string()]).unwrap();
        let out_path = PathBuf::from(&items[0].output);
        assert!(!out_path.exists(), "the output must be free when the batch starts");

        // The racing process: wait until the guard has passed and the transform is under way, then make
        // the planned output a second NAME for the outside victim.
        let v = victim.clone();
        let o = out_path.clone();
        let racer = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            fs::hard_link(&v, &o).map(|_| true).unwrap_or(false)
        });

        let report = execute_plan(&items, &job).expect("the batch runs");
        let linked = racer.join().unwrap();

        let victim_now = fs::read(&victim).unwrap();
        println!( // NOT-A-SKIP-NOTICE: "skipped" is the batch report's own field; the security assertion below runs either way
            "SEC-9 report: racer_linked={linked} written={} skipped={:?}; victim {} -> {} bytes",
            report.written,
            report.skipped,
            victim_bytes.len(),
            victim_now.len()
        );
        // THE security property, asserted whichever defence fired.
        assert_eq!(
            victim_now, victim_bytes,
            "EXPLOIT: a file OUTSIDE the selected folder was overwritten by a race that landed between \
             the safety check and the write"
        );
        if linked {
            // The racer got the name in before the open: the handle check must have caught the
            // multiply-linked object and skipped the item.
            assert_eq!(
                report.written, 0,
                "the racer's link landed first, so the item must have been refused, not written"
            );
            assert_eq!(report.skipped.len(), 1, "the refused item must be reported, not silently dropped");
            println!("SEC-9 defence: the handle check refused a multiply-linked output ({})", report.skipped[0].1); // NOT-A-SKIP-NOTICE: the PRODUCT skipped an item; this test declined nothing
        } else {
            // The atomic create claimed the name first, so the attack could not be staged at all.
            assert_eq!(report.written, 1, "with the attack impossible, the ordinary write must succeed");
            println!("SEC-9 defence: the atomic create claimed the name before the racer could link it");
        }

        let _ = fs::remove_dir_all(&d);
    }
}
