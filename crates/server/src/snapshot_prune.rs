//! Retention-prune command layer (CPE-1196, epic CPE-735): wires the pure grandfather-father-son policy
//! ([`crate::snapshot_retention::thin`]) to the real on-disk snapshot store
//! ([`crate::snapshot_capture`]) — enumerate a root's manifests, decide keep/prune under a
//! [`RetentionPolicy`], and (only in [`apply`]) actually [`crate::snapshot_capture::prune`] the losers.
//!
//! Store-dir-based, like [`crate::snapshot_capture`] itself (no [`crate::ctx::ServerCtx`] here) — the
//! per-root store-dir resolution stays in [`crate::checkpoint_store`], which is the natural place for a
//! ctx-aware `root -> store_dir` wrapper around these functions.
//!
//! [`preview`] is read-only: it never calls [`crate::snapshot_capture::prune`], so it's always safe to call
//! before showing the user what would happen. [`apply`] is the only function here that touches disk, and it
//! reuses [`crate::snapshot_capture::prune`] verbatim — the manifest-deleted-first / leak-over-corruption
//! invariant documented at `snapshot_capture.rs:218-247` is untouched; this module never opens a manifest,
//! an index, or a blob file directly.

use std::collections::BTreeMap;

use crate::snapshot_capture;
use crate::snapshot_retention::{thin, RetentionPolicy, Snapshot as RetentionSnapshot};

/// The keep/prune verdict for a store under a policy, plus the store's current footprint — non-destructive,
/// safe to compute and show before [`apply`] touches anything.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RetentionPreview {
    /// Manifest ids the policy would keep, newest-first.
    pub keep: Vec<String>,
    /// Manifest ids the policy would prune, oldest-first.
    pub prune: Vec<String>,
    /// The store's current total footprint in bytes (informational only — `preview` never mutates it).
    /// Measured from the blob files on disk, not read out of `index.json` — see
    /// [`crate::snapshot_capture::store_total_bytes`] (CPE-1844).
    pub total_bytes: u64,
}

/// What became of the optional byte cap in one [`apply`] — a **pass-level** verdict on a budget, not a
/// per-item one (CPE-1863).
///
/// It exists because "the cap was met" and "the cap could not be met and we destroyed checkpoints
/// discovering that" used to be the same answer: an `Ok(RetentionApplyResult)` with a non-empty `pruned`
/// list. A caller reading `pruned` alone reports success either way, which is the whole of CPE-1863.
///
/// | variant | what happened | what the caller can say |
/// |---|---|---|
/// | [`NotRequested`](ByteCapOutcome::NotRequested) | no cap was passed (`None`, or `Some(0)`) | nothing — the GFS policy alone decided |
/// | [`Met`](ByteCapOutcome::Met) | the store's measured footprint is at or under the cap | the cap holds |
/// | [`StoppedNoProgress`](ByteCapOutcome::StoppedNoProgress) | an eviction reclaimed **nothing**, so the loop stopped | the cap was **not** met, and deleting more checkpoints would not have helped |
/// | [`StoppedAtFloor`](ByteCapOutcome::StoppedAtFloor) | the loop ran out of evictable survivors (a store is never pruned below one snapshot) | the cap was **not** met; the store cannot get smaller by thinning |
///
/// **Why this is not [`crate::model::OpOutcome`]** (CPE-1845's discriminant), checked rather than
/// assumed: that enum answers "what happened to *this one item*, and can the user retry it" for a bulk
/// per-path operation, and every manifest this loop deletes is unambiguously `Applied` — the item
/// succeeded. What is unresolved is the *budget the deletions were justified by*, which has no item.
/// Mapping a missed cap onto `SkippedByPlan`/`HeldBackByCheckpoint` would mean reporting a hold-back for
/// operations that were in fact performed. Its *conventions* are reused deliberately, because those are
/// the reusable part: a discriminated union rather than a prose prefix, `snake_case` on the wire, and
/// variants chosen by the user-facing decision they drive rather than by internal control flow.
///
/// Serialised `snake_case`, so TS reads `"not_requested" | "met" | "stopped_no_progress" |
/// "stopped_at_floor"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum ByteCapOutcome {
    /// No byte cap was requested, so there was nothing to meet. The default, and what every caller in
    /// the app gets today — `snapshot_schedule::snapshot_run_due` passes `None`.
    #[default]
    NotRequested,
    /// The store's measured footprint is at or under the cap. This includes the common case where it
    /// already was before any eviction.
    Met,
    /// **The cap was not met.** An eviction reclaimed nothing — the store's re-measured footprint did not
    /// fall — so the loop stopped rather than keep deleting checkpoints that cannot help. See the
    /// no-progress rule on [`apply`].
    StoppedNoProgress,
    /// **The cap was not met.** Only one snapshot is left — a store is never thinned to zero — and the
    /// footprint is still over the cap. Also the safe label on [`apply`]'s structurally unreachable
    /// out-of-candidates arm; see the comment there before reading that as a second real cause.
    StoppedAtFloor,
}

impl ByteCapOutcome {
    /// Was a cap requested and *not* met? The two `Stopped*` variants, i.e. exactly the cases where
    /// [`apply`] returned `Ok` having failed to do what it was asked. A consumer that reports a
    /// retention pass as a success must not do so without consulting this.
    ///
    /// This is a convenience for phrasing, never the discriminant: which of the two it is decides what
    /// the user can do about it, and that distinction is the variant.
    pub fn cap_missed(self) -> bool {
        matches!(self, ByteCapOutcome::StoppedNoProgress | ByteCapOutcome::StoppedAtFloor)
    }
}

/// The outcome of actually pruning a store: which manifests survived, which were removed (by the GFS
/// policy and/or the optional byte cap), how many bytes were freed, and whether the byte cap was met.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RetentionApplyResult {
    /// Manifest ids still present after pruning, newest-first.
    pub kept: Vec<String>,
    /// Manifest ids actually removed — the GFS losers, plus any byte-cap eviction beyond them.
    pub pruned: Vec<String>,
    /// Total bytes freed across every prune in this apply — the lengths of the blob files actually
    /// removed (CPE-1844), not the sizes `index.json` recorded for them.
    ///
    /// `bytes_freed == 0` with a **non-empty `pruned`** is the anomaly CPE-1863 is about: checkpoints
    /// were destroyed and nothing was reclaimed. When the GFS policy alone drove it, it is not an anomaly
    /// at all — the user asked for fewer checkpoints, not for fewer bytes.
    ///
    /// **It does not imply [`ByteCapOutcome::StoppedNoProgress`], and an earlier draft of this comment
    /// said it did.** The two are measured in different currencies on purpose (see the no-progress rule
    /// on [`apply`]): `bytes_freed` counts blob *files removed*, `byte_cap` turns on the *re-measured
    /// footprint*. In precisely the divergence case the rule exists to serve — a blob whose last namer
    /// was pruned and whose file could not be deleted — `prune` credits 0 while `total` falls, so the
    /// loop correctly keeps going and can finish at [`ByteCapOutcome::Met`] or
    /// [`ByteCapOutcome::StoppedAtFloor`] with `bytes_freed == 0` and a non-empty `pruned`. Read the two
    /// fields together; neither derives the other.
    pub bytes_freed: u64,
    /// What became of the byte cap (CPE-1863). [`ByteCapOutcome::NotRequested`] whenever no cap was
    /// passed, which is every caller in the app today.
    pub byte_cap: ByteCapOutcome,
}

/// Load every manifest in `store_dir` as a [`RetentionSnapshot`] (`id` + `epoch_s`) for
/// [`crate::snapshot_retention::thin`] — `created_ms` truncated to seconds, since retention buckets
/// (hourly/daily/weekly/monthly) don't need sub-second resolution.
fn manifests_as_snapshots(store_dir: &str) -> Result<Vec<RetentionSnapshot>, String> {
    Ok(snapshot_capture::list_manifests(store_dir)?
        .into_iter()
        .map(|m| RetentionSnapshot { id: m.id, epoch_s: m.created_ms / 1000 })
        .collect())
}

/// Preview what [`apply`] would do to `store_dir` under `policy`, without touching disk: enumerate its
/// manifests, run [`thin`], and report the keep/prune split plus the store's current footprint.
pub fn preview(store_dir: &str, policy: &RetentionPolicy) -> Result<RetentionPreview, String> {
    let snaps = manifests_as_snapshots(store_dir)?;
    let result = thin(&snaps, policy);
    let total_bytes = snapshot_capture::store_total_bytes(store_dir)?;
    Ok(RetentionPreview { keep: result.keep, prune: result.prune, total_bytes })
}

/// Actually prune `store_dir` to `policy`: run the same [`thin`] decision [`preview`] would, then
/// [`crate::snapshot_capture::prune`] every losing manifest. If `max_total_bytes` is `Some` (and nonzero),
/// after the GFS pass the survivors are further thinned **oldest-first** — protecting the newest snapshots
/// last — until one of three things happens:
///
/// - the store's re-measured footprint is at or under the cap ([`ByteCapOutcome::Met`]);
/// - an eviction **reclaims nothing**, so the loop stops rather than keep deleting checkpoints that
///   cannot help ([`ByteCapOutcome::StoppedNoProgress`], CPE-1863);
/// - only one survivor remains ([`ByteCapOutcome::StoppedAtFloor`]) — a policy is never allowed to prune
///   a store down to zero snapshots; that would make `max_total_bytes` a silent full-wipe knob instead
///   of a cap.
///
/// **Only the first of those met the cap**, and [`RetentionApplyResult::byte_cap`] says which one it was.
/// A caller that reads `pruned` alone and reports success is the bug CPE-1863 records; the reasoning for
/// the no-progress rule, and what it costs, is on the loop itself.
pub fn apply(
    store_dir: &str,
    policy: &RetentionPolicy,
    max_total_bytes: Option<u64>,
) -> Result<RetentionApplyResult, String> {
    let snaps = manifests_as_snapshots(store_dir)?;
    let mut result = thin(&snaps, policy);

    // Floor: NEVER prune a non-empty store down to zero snapshots. `thin()` returns an empty `keep`
    // whenever the policy retains 0 across all four GFS tiers ({0,0,0,0}), which the GFS loop below
    // would then execute as a full wipe — silent, irreversible data loss (a zeroed schedule-rule
    // retention would erase every snapshot it just captured). This guard mirrors the `kept.len() <= 1`
    // floor already present in the byte-cap branch: keep the single NEWEST manifest so at least one
    // snapshot always survives. (Ties broken by id for determinism.)
    if result.keep.is_empty() {
        if let Some(newest) = snaps.iter().max_by_key(|s| (s.epoch_s, s.id.clone())) {
            let id = newest.id.clone();
            result.prune.retain(|p| p != &id);
            result.keep.push(id);
        }
    }

    let mut bytes_freed = 0u64;
    let mut pruned = Vec::new();
    // No cap unless one is asked for, and the byte-cap branch below is the only thing that may change it.
    let mut byte_cap = ByteCapOutcome::NotRequested;
    // This loop is where the quadratic term lives: each `prune` re-scans the surviving manifests to
    // check nothing else still names the blobs it is about to free (CPE-1861). Fine for the scheduled
    // shape — one snapshot ageing out — and measurable on a bulk thin. Before optimising the retention
    // pass, read the cost note on `snapshot_capture::manifests_naming`: hoisting that scan out of the
    // per-manifest call is the fix, weakening it is not.
    for id in &result.prune {
        bytes_freed += snapshot_capture::prune(store_dir, id)?;
        pruned.push(id.clone());
    }

    let mut kept = result.keep;

    if let Some(cap) = max_total_bytes {
        if cap > 0 {
            // Oldest-first among survivors: `created_ms` looked up from the snapshots already loaded
            // above (before any pruning changed the on-disk set).
            let created_by_id: BTreeMap<&str, u64> =
                snaps.iter().map(|s| (s.id.as_str(), s.epoch_s)).collect();
            let mut oldest_first = kept.clone();
            oldest_first.sort_by_key(|id| created_by_id.get(id.as_str()).copied().unwrap_or(0));

            // CPE-1844 — **every figure this loop steers on is re-measured from disk, never carried
            // forward as arithmetic over `index.json`'s claims.** Two of them were claims:
            //
            // 1. `store_total_bytes` used to sum the `size` fields recorded in `index.json`. One text
            //    edit therefore drove this loop, which deletes the user's other checkpoints oldest-first
            //    down to a single survivor. Fixed at the source — that function now measures the blob
            //    files a manifest still names — so nothing is needed here for it.
            // 2. `total = total.saturating_sub(freed)` was the second, and it is fixed here: `freed`
            //    came from `release`, i.e. the same recorded sizes. A *deflated* index made each prune
            //    look like it reclaimed almost nothing and kept the loop deleting. Re-measuring instead
            //    of subtracting removes the input rather than bounding it, which is this ticket's whole
            //    rule, and it also means the loop stops the moment the store is genuinely under the cap
            //    even if the accounting disagrees.
            //
            // **No test will catch you if you delete the `total = store_total_bytes(..)` re-measure
            // at the foot of this loop, on its own.** Once `prune` reports the bytes of the files it
            // actually removed, that value and a fresh measurement
            // agree on every fixture in the suite, so breaking only this line leaves the suite green.
            // Its red-proof is the *pair* regression — restoring both the old `Ok(release(..))` in
            // `prune` and the subtraction here — which reds
            // `cpe_1844_a_deflated_index_cannot_keep_the_cap_loop_deleting_past_the_cap` at "1 left" of
            // four. Written here rather than only in the ticket because every other guard in this module
            // carries its own red-proof in a comment, and a maintainer tidying this line would otherwise
            // see a green suite and no warning. It is kept for two reasons: it removes a carried-forward
            // number rather than bounding it, and the two are *not* equal by construction — they diverge
            // whenever the initial measurement under-counts something `prune` then removes and credits
            // (a `metadata()` that failed once and succeeded later), where the re-measure is the safer
            // side.
            //
            // **Interaction with CPE-1863, which is now fixed below rather than deferred.** CPE-1844
            // left it standing with the note that when a prune really frees nothing, the re-measured
            // `total` really has not moved — so this re-measure describes reality rather than an
            // accounting artifact, but the loop still ran to its floor on it. That re-measure is exactly
            // what CPE-1863's no-progress rule now reads: see the block above the loop.
            //
            // **Scoped claim, because an earlier draft of this comment over-broadened it and was
            // measurably false.** It read "Nothing here makes it worse", stated as if it covered the
            // whole change; it is established only of *the subtraction*, where every case in which the
            // old arithmetic advanced `total` faster than the disk did was an over-credit that ended the
            // loop early. It was **not** established of the change of basis, and the security audit
            // falsified that half: the first version of `store_total_bytes` summed the whole `blobs/`
            // directory, so an **orphan** blob — `capture`'s own partial-write residue, which no prune
            // can ever reclaim — counted toward the cap and drove this loop to its floor. Measured at
            // `preview.total_bytes 45 -> 4000045`, `pruned 4 of 5`, `bytes_freed = 36`, on a store with
            // no attacker in it, where before the change that residue contributed 0 and the pass did
            // nothing. That is a shape the old code could not see at all, not an over-credit. It is
            // closed by the witness in `store_total_bytes` rather than here, and it is why that function
            // measures *reclaimable* footprint instead of disk usage.
            //
            // Cost: one `read_dir` of `blobs/` plus a stat per blob file, plus the witness walk over
            // `manifests/`, per iteration, on top of the one this loop always paid. The loop only runs
            // when a caller passes a cap at all — `snapshot_schedule::snapshot_run_due` passes `None`,
            // so the scheduled path pays nothing — and it iterates only while survivors are still being
            // deleted, which is bounded by `kept.len()`. The witness walk is the same scan
            // `snapshot_capture::manifests_naming` already performs inside every `prune` this loop
            // makes, so the loop's per-iteration cost is roughly doubled rather than newly incurred.
            //
            // **CPE-1863 — the no-progress rule, and the definition it turns on.** This loop used to
            // have exactly one exit besides the cap being met: the `kept.len() <= 1` floor. So a store
            // where evicting a checkpoint reclaims *nothing* was thinned all the way down to a single
            // survivor and then reported `Ok`. Measured on a store with no tamper in it at all — six
            // identical captures, so one blob is shared by every manifest and pruning any of them frees
            // nothing:
            //
            // ```text
            // apply(cap = total - 1)  ->  kept = 1, pruned = 5, bytes_freed = 0
            // ```
            //
            // Five checkpoints destroyed, zero bytes reclaimed, reported as success. The deletions were
            // not merely useless: they could not have worked, because the survivors were not what was
            // using the space.
            //
            // **"Progress" is measured in the cap's own currency — the re-measured `total` — not in
            // `prune`'s return value.** The two normally agree, and the `freed == 0` case is the common
            // instance, but they are not the same question and the difference decides a real case: a
            // blob whose last namer was pruned but whose *file* could not be deleted (this module's
            // documented leak-over-corruption direction) is credited 0 by `prune` while genuinely
            // leaving the reclaimable footprint — which is what the cap is compared against — smaller.
            // Stopping on `freed == 0` there would abandon a loop that was working. The cap is a
            // statement about `total`, so progress has to be too.
            //
            // **What it costs, stated rather than discovered later: one checkpoint.** The first eviction
            // still happens, because nothing here can know a prune's yield without performing it — the
            // yield is "blobs no other manifest *file* still names", which is `prune`'s own witness scan.
            // So the fixture above now loses one checkpoint instead of five. The strictly better fix is a
            // predictor — ask what evicting a candidate *would* free and skip a zero-yield one without
            // deleting it — which needs a witness-with-exclusion query alongside `prune`'s, and a mirror
            // of that predicate that drifts is worse than the bounded loss. Left as the follow-up; the
            // residual is not silent, it is `StoppedNoProgress` with `bytes_freed == 0`.
            //
            // **And the accepted cost of NOT continuing: a later candidate might have freed bytes.**
            // `m1={A}, m2={A,B}, m3={A}` — evicting m1 frees nothing while evicting m2 would free B. The
            // loop stops at m1 and reports the cap unmet. That is the deliberate direction: continuing
            // spends *certain* destruction of the user's history on a *speculative* reclaim, and the
            // usual reason an eviction freed nothing is that the blobs are shared with everything, in
            // which case continuing destroys the lot for nothing. Reporting honestly costs the user a
            // decision; guessing costs them their checkpoints.
            //
            // **The rule is PER PASS. It holds no state across invocations, and that bounds what
            // "at most one checkpoint" is honestly a claim about.** Per invocation it is exact. Across a
            // *repeating schedule* with a cap wired up, each pass is entitled to its own fruitless
            // eviction, so the six-identical-captures fixture above still ends at the floor — five
            // evictions over five passes instead of five in one. Materially weaker than the headline
            // framing, and written here rather than only in the ticket because this loop is where a
            // maintainer will look. `cpe_1863_an_invisible_manifest_pinning_blobs_stops_the_cap_without_
            // stalling_it` pins the two-pass shape deliberately. Removing the erosion needs the predictor
            // above, not more state: remembering "this store made no progress last time" would have to be
            // invalidated by every capture, and a stale memory that suppresses a *needed* eviction is a
            // worse failure than a slow one.
            //
            // **Interaction with CPE-1861's accepted leak, checked because that ticket's residual makes
            // `freed == 0` the *expected* outcome.** A manifest file `list_manifests` refuses — an
            // Explorer "- Copy.json", a 122-byte witness that is invisible and permanent — still counts
            // as a namer to `prune`'s witness and to `store_total_bytes`, so its snapshot's blobs are
            // pinned, counted toward the cap, and reclaimable by no prune this loop can make. Before this
            // rule that store was thinned to its floor every pass. It now stops after one eviction and
            // says `StoppedNoProgress`, which is the honest answer: the space is held by something
            // retention cannot reach. That is a stop, not a stall — `apply` returns `Ok`, the GFS pass
            // above has already run in full, and nothing about the next pass is wedged.
            let mut total = snapshot_capture::store_total_bytes(store_dir)?;
            let mut candidates = oldest_first.iter();
            byte_cap = loop {
                if total <= cap {
                    break ByteCapOutcome::Met;
                }
                // The floor: a policy is never allowed to prune a store down to zero snapshots.
                if kept.len() <= 1 {
                    break ByteCapOutcome::StoppedAtFloor;
                }
                // **Structurally unreachable, and kept as defence rather than as a case.** `oldest_first`
                // is a clone of `kept`, and the arm above breaks at `kept.len() <= 1`, so one candidate is
                // always left unconsumed. Only an internal inconsistency between the two — `kept` holding
                // an id `oldest_first` never did, or a future edit that stops them being the same set —
                // could arrive here, which is a bug in this function, not a store that ran out of
                // snapshots. `StoppedAtFloor` is the safe *label* for it (it is the honest "nothing left
                // to evict" answer and never reports a cap as met) but it is the wrong *diagnosis*: do
                // not read this arm as evidence that running out of candidates is a real outcome.
                let Some(id) = candidates.next() else {
                    break ByteCapOutcome::StoppedAtFloor;
                };
                // CPE-1871 — **both of the choices this block makes are argued above and pinned by
                // `cpe_1871_an_undeletable_blobs_freed_bytes_still_count_as_progress`.** That test stages
                // a blob whose last namer is this very eviction but whose FILE cannot be removed (the
                // OS holds it open / its directory is read-only), so `prune`'s own `freed_now` reads 0
                // while the re-measured store footprint genuinely falls. Swapping either line below for
                // its rejected alternative — `let progressed = freed_now > 0;` (CPE-1863's rejected
                // form) or `let after = total.saturating_sub(freed_now);` (CPE-1844's, in place of the
                // `store_total_bytes` re-measure) — reds that test with `StoppedNoProgress` where the cap
                // was in fact met. Before CPE-1871 neither swap was caught by anything in this suite.
                let freed_now = snapshot_capture::prune(store_dir, id)?;
                bytes_freed += freed_now;
                let after = snapshot_capture::store_total_bytes(store_dir)?;
                kept.retain(|k| k != id);
                pruned.push(id.clone());
                let progressed = after < total;
                total = after;
                if !progressed {
                    break ByteCapOutcome::StoppedNoProgress;
                }
            };
        }
    }

    Ok(RetentionApplyResult { kept, pruned, bytes_freed, byte_cap })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::CaptureBudget;
    use crate::snapshot_capture::capture;
    use std::fs;

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-snapprune-{tag}"))
    }

    /// Capture `src` into `store` and then hand-edit the resulting manifest's `created_ms` to `epoch_s *
    /// 1000`, so tests can place captures at arbitrary spread timestamps without sleeping. Returns the
    /// manifest id.
    fn capture_at(src: &std::path::Path, store: &std::path::Path, epoch_s: u64) -> String {
        let outcome =
            capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED).unwrap();
        let path = store.join("manifests").join(format!("{}.json", outcome.manifest_id));
        let mut doc: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        doc["created_ms"] = serde_json::json!(epoch_s * 1000);
        fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
        outcome.manifest_id
    }

    fn all_pol() -> RetentionPolicy {
        RetentionPolicy { hourly: 2, daily: 0, weekly: 0, monthly: 0 }
    }

    // ---- CPE-1861 helpers ---------------------------------------------------------------------------

    /// The ids `list_manifests` currently hands the planner, sorted — i.e. exactly what
    /// [`crate::snapshot_retention::thin`] gets to decide about. Every CPE-1861 fixture asserts on this
    /// before and after its tamper, so "the tamper reached the planner" is proved rather than assumed.
    fn planner_view(store: &std::path::Path) -> Vec<String> {
        let mut v: Vec<String> = snapshot_capture::list_manifests(&store.to_string_lossy())
            .unwrap()
            .into_iter()
            .map(|m| m.id)
            .collect();
        v.sort();
        v
    }

    fn manifest_files(store: &std::path::Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(store.join("manifests"))
            .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect())
            .unwrap_or_default();
        v.sort();
        v
    }

    /// Hand-edit `<file_id>.json`'s inner `id` field to `new_id`, and read it straight back so a tamper
    /// that silently failed to land can never be mistaken for a guard that worked.
    fn set_inner_id(store: &std::path::Path, file_id: &str, new_id: &str) {
        let path = store.join("manifests").join(format!("{file_id}.json"));
        let mut doc: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        doc["id"] = serde_json::json!(new_id);
        fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
        let back: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back["id"].as_str().unwrap(), new_id, "LIVE: the id tamper never landed on disk");
    }

    /// Copy `<file_id>.json` to `<new_name>.json`, optionally rewriting the copy's inner `id`. Returns
    /// the copy's path. Verified on disk before returning.
    fn plant_copy(
        store: &std::path::Path,
        file_id: &str,
        new_name: &str,
        inner_id: Option<&str>,
    ) -> std::path::PathBuf {
        let dir = store.join("manifests");
        let src = dir.join(format!("{file_id}.json"));
        let dst = dir.join(format!("{new_name}.json"));
        fs::copy(&src, &dst).unwrap();
        if let Some(id) = inner_id {
            let mut doc: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&dst).unwrap()).unwrap();
            doc["id"] = serde_json::json!(id);
            fs::write(&dst, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
        }
        let back: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&dst).unwrap()).unwrap();
        assert!(back["files"].is_object(), "LIVE: the planted copy is not a parseable manifest");
        dst
    }

    /// Every id in `kept` must name a manifest that still restores. Before CPE-1861 a retention pass
    /// could report `kept: ["no-such-manifest"]` — a checkpoint it says it protected and that does not
    /// exist — and `kept: [m3, m2, m3]`, the same real checkpoint counted twice.
    fn assert_kept_ids_all_restore(store: &std::path::Path, kept: &[String], tag: &str) {
        let mut seen = std::collections::BTreeSet::new();
        for id in kept {
            assert!(seen.insert(id.clone()), "{tag}: kept names {id} twice: {kept:?}");
            let dest = scratch(&format!("kept-{tag}"));
            assert!(
                snapshot_capture::restore(&store.to_string_lossy(), id, &dest.to_string_lossy()).is_ok(),
                "{tag}: retention reported keeping {id}, which does not restore"
            );
            let _ = fs::remove_dir_all(&dest);
        }
    }

    #[test]
    fn preview_is_non_destructive() {
        let src = scratch("prev-src");
        let store = scratch("prev-store");
        fs::write(src.join("a.txt"), b"a").unwrap();
        let m1 = capture_at(&src, &store, 3600);
        fs::write(src.join("a.txt"), b"a2").unwrap();
        let _m2 = capture_at(&src, &store, 2 * 3600);
        fs::write(src.join("a.txt"), b"a3").unwrap();
        let m3 = capture_at(&src, &store, 3 * 3600);

        let before = snapshot_capture::list_manifests(&store.to_string_lossy()).unwrap().len();
        let preview = preview(&store.to_string_lossy(), &all_pol()).unwrap();
        assert_eq!(preview.prune, vec![m1.clone()]);
        assert!(preview.keep.contains(&m3));

        // Nothing on disk changed: same manifest count, and the "pruned" one still restores.
        let after = snapshot_capture::list_manifests(&store.to_string_lossy()).unwrap().len();
        assert_eq!(before, after, "preview must not touch the manifests dir");
        assert!(
            snapshot_capture::restore(&store.to_string_lossy(), &m1, &scratch("prev-restore").to_string_lossy())
                .is_ok(),
            "the manifest preview marked for pruning is untouched by preview() and still restores"
        );

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    #[test]
    fn apply_keeps_gfs_survivors_and_they_still_restore_byte_for_byte() {
        let src = scratch("apply-src");
        let store = scratch("apply-store");
        fs::write(src.join("a.txt"), b"v1").unwrap();
        let m1 = capture_at(&src, &store, 3600);
        fs::write(src.join("a.txt"), b"v2").unwrap();
        let m2 = capture_at(&src, &store, 2 * 3600);
        fs::write(src.join("a.txt"), b"v3").unwrap();
        let m3 = capture_at(&src, &store, 3 * 3600);

        let bytes_before = snapshot_capture::store_total_bytes(&store.to_string_lossy()).unwrap();

        // hourly=2 keeps the two newest hour buckets (m2, m3), prunes m1.
        let result = apply(&store.to_string_lossy(), &all_pol(), None).unwrap();
        assert_eq!(result.pruned, vec![m1.clone()]);
        assert!(result.kept.contains(&m2) && result.kept.contains(&m3));
        assert!(result.bytes_freed > 0);

        // Removed from manifests/ + index.json: a restore of the pruned manifest now fails.
        assert!(snapshot_capture::restore(&store.to_string_lossy(), &m1, &scratch("gone").to_string_lossy())
            .is_err());
        let manifests_after = snapshot_capture::list_manifests(&store.to_string_lossy()).unwrap();
        assert!(!manifests_after.iter().any(|m| m.id == m1), "pruned manifest file is gone");

        // Store bytes dropped.
        let bytes_after = snapshot_capture::store_total_bytes(&store.to_string_lossy()).unwrap();
        assert!(bytes_after < bytes_before, "store footprint shrank after pruning the loser");

        // Survivors still restore byte-for-byte.
        let dest2 = scratch("restore-m2");
        snapshot_capture::restore(&store.to_string_lossy(), &m2, &dest2.to_string_lossy()).unwrap();
        assert_eq!(fs::read(dest2.join("a.txt")).unwrap(), b"v2");
        let dest3 = scratch("restore-m3");
        snapshot_capture::restore(&store.to_string_lossy(), &m3, &dest3.to_string_lossy()).unwrap();
        assert_eq!(fs::read(dest3.join("a.txt")).unwrap(), b"v3");

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    #[test]
    fn apply_with_a_total_byte_cap_further_thins_survivors_oldest_first_but_never_to_zero() {
        let src = scratch("cap-src");
        let store = scratch("cap-store");
        // Distinct, sizeable content per capture so the byte cap actually bites.
        fs::write(src.join("a.txt"), vec![1u8; 1000]).unwrap();
        let m1 = capture_at(&src, &store, 3600);
        fs::write(src.join("a.txt"), vec![2u8; 1000]).unwrap();
        let m2 = capture_at(&src, &store, 2 * 3600);
        fs::write(src.join("a.txt"), vec![3u8; 1000]).unwrap();
        let m3 = capture_at(&src, &store, 3 * 3600);

        // A very generous hourly count keeps all three via GFS; the byte cap is the only pressure.
        let generous = RetentionPolicy { hourly: 10, daily: 0, weekly: 0, monthly: 0 };
        let total = snapshot_capture::store_total_bytes(&store.to_string_lossy()).unwrap();
        assert!(total >= 3000, "three distinct ~1000-byte blobs");

        // Cap tight enough to force dropping the oldest survivor but not everything.
        let cap = total - 1000;
        let result = apply(&store.to_string_lossy(), &generous, Some(cap)).unwrap();
        assert!(result.pruned.contains(&m1), "oldest survivor is evicted first for the byte cap");
        assert!(result.kept.contains(&m3), "newest survivor is protected");
        assert!(!result.kept.is_empty(), "byte cap must never prune every survivor");
        let _ = m2; // m2 may or may not survive depending on exact sizes; not asserted either way.

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    #[test]
    fn apply_never_prunes_the_last_survivor_even_under_a_tiny_cap() {
        let src = scratch("last-src");
        let store = scratch("last-store");
        fs::write(src.join("a.txt"), vec![9u8; 500]).unwrap();
        let only = capture_at(&src, &store, 3600);

        let pol = RetentionPolicy { hourly: 10, daily: 0, weekly: 0, monthly: 0 };
        let result = apply(&store.to_string_lossy(), &pol, Some(1)).unwrap();
        assert_eq!(result.kept, vec![only], "the single remaining snapshot is never wiped by the byte cap");
        assert!(result.pruned.is_empty());

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    #[test]
    fn apply_with_an_all_zero_policy_keeps_the_newest_survivor_not_a_full_wipe() {
        // Regression (CPE-1196 review): a policy that retains 0 in EVERY GFS tier made `thin()` return an
        // empty keep, and the GFS prune loop executed that as a full wipe — silent, irreversible loss of
        // the entire snapshot history (a zeroed schedule-rule retention was the real hazard). The floor
        // must keep the single newest snapshot.
        let src = scratch("zero-src");
        let store = scratch("zero-store");
        fs::write(src.join("a.txt"), b"v1").unwrap();
        let m1 = capture_at(&src, &store, 3600);
        fs::write(src.join("a.txt"), b"v2").unwrap();
        let m2 = capture_at(&src, &store, 2 * 3600);

        let zero = RetentionPolicy { hourly: 0, daily: 0, weekly: 0, monthly: 0 };
        let result = apply(&store.to_string_lossy(), &zero, None).unwrap();

        assert_eq!(result.kept, vec![m2.clone()], "the newest snapshot always survives an all-zero policy");
        assert_eq!(result.pruned, vec![m1.clone()]);
        // The survivor still restores byte-for-byte; the store is not empty.
        let dest = scratch("zero-restore");
        snapshot_capture::restore(&store.to_string_lossy(), &m2, &dest.to_string_lossy()).unwrap();
        assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"v2");
        assert!(!snapshot_capture::list_manifests(&store.to_string_lossy()).unwrap().is_empty());

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
        let _ = fs::remove_dir_all(&dest);
    }

    // ---- CPE-1861: a manifest's inner id vs the filename it is stored under -------------------------
    //
    // Every test below drives the real `apply` — the function `snapshot_run_due` calls after every
    // scheduled capture — and asserts its fixture is live in BOTH directions (the tamper landed on disk,
    // and the planner's own view changed because of it) before asserting any harm.

    /// Three captures, spread one hour apart, with the oldest one's inner `id` rewritten to name its
    /// newest sibling. Before: the phantom entry collapsed the whole decision — `Ok(kept: [m3, m2, m3],
    /// pruned: [])`, the same checkpoint reported kept twice and **nothing thinned at all**, with the
    /// tampered file immortal. After: the liar is simply not a checkpoint, the rest of the store is
    /// decided normally, and every id the result reports as kept is one that actually restores.
    #[test]
    fn cpe_1861_an_inner_id_naming_a_sibling_cannot_steer_or_stall_retention() {
        let src = scratch("steer-src");
        let store = scratch("steer-store");
        fs::write(src.join("a.txt"), b"v1").unwrap();
        let m1 = capture_at(&src, &store, 3600);
        fs::write(src.join("a.txt"), b"v2").unwrap();
        let m2 = capture_at(&src, &store, 2 * 3600);
        fs::write(src.join("a.txt"), b"v3").unwrap();
        let m3 = capture_at(&src, &store, 3 * 3600);

        // FIXTURE LIVENESS 1 — the planner sees all three before the tamper.
        let before = planner_view(&store);
        assert_eq!(before.len(), 3, "fixture: {before:?}");
        assert!(before.contains(&m1));

        set_inner_id(&store, &m1, &m3);

        // FIXTURE LIVENESS 2 — the tamper reached the planner: its view changed, and the file is still
        // there (so a "no such file" could never be what this test is really measuring).
        let after = planner_view(&store);
        assert_ne!(before, after, "LIVE: the tamper never reached the planner");
        assert!(
            manifest_files(&store).contains(&format!("{m1}.json")),
            "LIVE: the tampered manifest file must still be on disk"
        );

        let result = apply(&store.to_string_lossy(), &all_pol(), None).unwrap();
        assert_kept_ids_all_restore(&store, &result.kept, "steer");
        assert!(!after.contains(&m1), "the liar is not a checkpoint: {after:?}");
        assert!(result.kept.contains(&m2) && result.kept.contains(&m3));

        // The two honest survivors still restore byte-for-byte — the phantom stole nothing.
        for (id, want) in [(&m2, &b"v2"[..]), (&m3, &b"v3"[..])] {
            let dest = scratch("steer-restore");
            snapshot_capture::restore(&store.to_string_lossy(), id, &dest.to_string_lossy()).unwrap();
            assert_eq!(fs::read(dest.join("a.txt")).unwrap(), want);
            let _ = fs::remove_dir_all(&dest);
        }

        // Recorded cost, pinned so it is a decision and not a surprise: a self-inconsistent manifest is
        // never reclaimed. A leak — this module's chosen failure direction — not a delete.
        assert!(result.pruned.is_empty());
        assert!(manifest_files(&store).contains(&format!("{m1}.json")));

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    /// The worse of the two original shapes: an inner `id` naming nothing at all. Before, `apply`
    /// returned `Err(".../no-such-manifest.json: cannot find the file specified")` — and because
    /// `snapshot_run_due` retention-prunes after every scheduled capture, **no checkpoint in that store
    /// was ever thinned again**. After, the pass succeeds, thins normally, and keeps succeeding.
    #[test]
    fn cpe_1861_an_inner_id_naming_nothing_cannot_wedge_the_retention_pass() {
        let src = scratch("wedge-src");
        let store = scratch("wedge-store");
        let mut ids = Vec::new();
        for (n, body) in [(1u64, "v1"), (2, "v2"), (3, "v3"), (4, "v4")] {
            fs::write(src.join("a.txt"), body).unwrap();
            ids.push(capture_at(&src, &store, n * 3600));
        }
        let (m1, m2) = (ids[0].clone(), ids[1].clone());

        let before = planner_view(&store);
        assert_eq!(before.len(), 4, "fixture: {before:?}");

        set_inner_id(&store, &m1, "no-such-manifest");

        // FIXTURE LIVENESS — the tamper reached the enumerator the planner reads.
        let after = planner_view(&store);
        assert_ne!(before, after, "LIVE: the tamper never reached the planner");
        assert!(
            manifest_files(&store).contains(&format!("{m1}.json")),
            "LIVE: the tampered manifest file must still be on disk"
        );

        // hourly=2 keeps the two newest hour buckets; m2 is the oldest listed loser.
        let result = apply(&store.to_string_lossy(), &all_pol(), None).unwrap_or_else(|e| {
            panic!("HARM: one manifest whose inner id names nothing killed the whole retention pass: {e}")
        });
        assert_eq!(result.pruned, vec![m2], "retention still thins the rest of the store");
        assert_kept_ids_all_restore(&store, &result.kept, "wedge");
        assert!(!after.contains(&"no-such-manifest".to_string()), "a phantom id is never listed: {after:?}");
        assert!(!after.contains(&m1), "{after:?}");

        // And it is not a one-shot recovery: the next scheduled pass succeeds too.
        assert!(apply(&store.to_string_lossy(), &all_pol(), None).is_ok(), "the pass stays alive");

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    /// A crafted **filename**, which is where CPE-1847's fix relocated the wedge rather than removing
    /// it. Both shapes are covered: a copy whose inner id still names the original (caught by the
    /// agreement rule) and a self-*consistent* one whose inner id is the crafted name too (caught only
    /// by `validate_manifest_id`, which is why that second condition exists).
    #[test]
    fn cpe_1861_a_crafted_manifest_filename_cannot_wedge_the_retention_pass() {
        let names: &[&str] = if cfg!(unix) { &["a..b", "a:b", "a\\b"] } else { &["a..b"] };
        for name in names {
            for self_consistent in [false, true] {
                let src = scratch("name-src");
                let store = scratch("name-store");
                fs::write(src.join("a.txt"), b"v1").unwrap();
                let m1 = capture_at(&src, &store, 3600);
                fs::write(src.join("a.txt"), b"v2").unwrap();
                let m2 = capture_at(&src, &store, 2 * 3600);
                fs::write(src.join("a.txt"), b"v3").unwrap();
                let _m3 = capture_at(&src, &store, 3 * 3600);

                let before = planner_view(&store);
                assert_eq!(before.len(), 3, "fixture: {before:?}");
                let planted =
                    plant_copy(&store, &m1, name, if self_consistent { Some(name) } else { None });

                // FIXTURE LIVENESS — the file really is there under the crafted name, and the planner
                // still refuses to list it (so a filesystem that rejected the name outright, which is
                // what would silently make this test inert on Windows, cannot pass for a guard).
                assert!(planted.exists(), "LIVE: {name}.json is not on disk");
                assert!(
                    manifest_files(&store).contains(&format!("{name}.json")),
                    "LIVE: {name}.json is not in manifests/"
                );
                let after = planner_view(&store);
                let result = apply(&store.to_string_lossy(), &all_pol(), None).unwrap_or_else(|e| {
                    panic!("HARM: planting {name}.json killed the whole retention pass: {e}")
                });
                assert_eq!(result.pruned, vec![m1.clone()], "retention still thins normally ({name})");
                assert!(result.kept.contains(&m2));
                assert_kept_ids_all_restore(&store, &result.kept, "name");
                assert_eq!(after, before, "a crafted filename is never a checkpoint: {after:?}");

                let _ = fs::remove_dir_all(&src);
                let _ = fs::remove_dir_all(&store);
            }
        }
    }

    /// CPE-1847 added the `file_count` self-consistency refusal and recorded, next to it, that one
    /// tampered manifest therefore wedged the whole retention pass — the same permanent stall as an
    /// unresolvable id, since `apply` propagates `prune`'s error with `?`. Closed from the enumeration
    /// end: `list_manifests` applies the *same* predicate as a skip, so the pass never names it. The
    /// refusal itself is unchanged and still fires on every read route.
    #[test]
    fn cpe_1861_a_manifest_contradicting_its_own_count_no_longer_wedges_the_pass() {
        let src = scratch("count-src");
        let store = scratch("count-store");
        fs::write(src.join("a.txt"), b"v1").unwrap();
        fs::write(src.join("b.txt"), b"b").unwrap();
        let m1 = capture_at(&src, &store, 3600);
        fs::write(src.join("a.txt"), b"v2").unwrap();
        let m2 = capture_at(&src, &store, 2 * 3600);
        fs::write(src.join("a.txt"), b"v3").unwrap();
        let _m3 = capture_at(&src, &store, 3 * 3600);

        let before = planner_view(&store);
        assert_eq!(before.len(), 3, "fixture: {before:?}");

        // Remove one entry from the map and leave the count behind — the tamper the count exists to
        // catch.
        let path = store.join("manifests").join(format!("{m1}.json"));
        let mut doc: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        doc["files"].as_object_mut().unwrap().remove("b.txt").unwrap();
        fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();

        // FIXTURE LIVENESS — the refusal really fires for this file (so the skip below is skipping a
        // manifest that would genuinely have errored), and the planner's view changed.
        assert!(
            snapshot_capture::manifest_snapshot(&store.to_string_lossy(), &m1)
                .unwrap_err()
                .contains("contradicts itself"),
            "LIVE: the count tamper did not make this manifest unloadable"
        );
        // Deliberately NOT "the planner's view changed" here, the way the id fixtures above prove their
        // tamper reached the enumerator: this tamper's only visible effect on that view *is* the guard,
        // so asserting it before the harm would make a removed guard red on a proxy instead of on the
        // stall itself. The liveness above is the independent one — the file really is unloadable now.
        let after = planner_view(&store);
        let result = apply(&store.to_string_lossy(), &all_pol(), None).unwrap_or_else(|e| {
            panic!("HARM: a manifest contradicting its own count killed the whole retention pass: {e}")
        });
        assert!(result.kept.contains(&m2));
        assert_kept_ids_all_restore(&store, &result.kept, "count");
        assert!(!after.contains(&m1), "a self-contradicting manifest is not a checkpoint: {after:?}");
        assert_eq!(after.len(), 2);

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    /// The fourth gate, found by the round-3 security audit after the first three had shipped in
    /// review: `prune` refuses a manifest whose entry hash is not a plain hex blob name (CPE-1823's
    /// `validate_blob_name`), and that refusal fires **before** the manifest is deleted, so it recurs
    /// on every scheduled pass forever. One hand-edited `hash` in an otherwise *perfectly*
    /// self-describing manifest — inner id agrees with the stem, stem valid, `file_count` correct —
    /// therefore stalled the pass exactly as a missing file did:
    ///
    /// ```text
    /// pass 1 -> Err("…: refusing this manifest entry — its content hash \"not-a-hex-hash\" is not a
    ///                plain hex blob name")
    /// pass 2 -> Err(same); all three manifests still listed
    /// ```
    ///
    /// Identical on `main`, so never a regression — but the same grammar at the same tamper cost, and
    /// my enumeration missed it by writing off `prune` as "unchanged" instead of walking its gate list.
    #[test]
    fn cpe_1861_a_hand_edited_entry_hash_no_longer_wedges_the_pass() {
        let src = scratch("hash-src");
        let store = scratch("hash-store");
        let mut ids = Vec::new();
        for (n, body) in [(1u64, "v1"), (2, "v2"), (3, "v3")] {
            fs::write(src.join("a.txt"), body).unwrap();
            ids.push(capture_at(&src, &store, n * 3600));
        }
        let (m1, m2) = (ids[0].clone(), ids[1].clone());

        let before = planner_view(&store);
        assert_eq!(before.len(), 3, "fixture: {before:?}");

        let path = store.join("manifests").join(format!("{m1}.json"));
        let mut doc: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        doc["files"]["a.txt"]["hash"] = serde_json::json!("not-a-hex-hash");
        fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();

        // FIXTURE LIVENESS 1 — the tamper landed, and the manifest is otherwise flawless: the three
        // conditions that shipped in review all pass it, so this can only be caught by the fourth.
        let back: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back["files"]["a.txt"]["hash"].as_str().unwrap(), "not-a-hex-hash", "LIVE: no tamper");
        assert_eq!(back["id"].as_str().unwrap(), m1, "LIVE: the inner id must still agree with the stem");
        assert_eq!(
            back["file_count"].as_u64(),
            Some(back["files"].as_object().unwrap().len() as u64),
            "LIVE: the count must still be honest, or a different guard would be doing the work"
        );

        // FIXTURE LIVENESS 2 — this really is a manifest `prune` refuses, so a listed id would stall.
        assert!(
            snapshot_capture::prune(&store.to_string_lossy(), &m1)
                .unwrap_err()
                .contains("not a plain hex blob name"),
            "LIVE: the tampered hash does not actually make this manifest unprunable"
        );

        let result = apply(&store.to_string_lossy(), &all_pol(), None).unwrap_or_else(|e| {
            panic!("HARM: one hand-edited entry hash killed the whole retention pass: {e}")
        });
        assert!(result.kept.contains(&m2));
        assert_kept_ids_all_restore(&store, &result.kept, "hash");
        // Not a one-shot recovery — the stall recurred on every pass, so the next one must be Ok too.
        assert!(apply(&store.to_string_lossy(), &all_pol(), None).is_ok(), "the pass stays alive");
        assert!(!planner_view(&store).contains(&m1), "a manifest prune would refuse is not a checkpoint");

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    /// **The acceptance gate.** A second copy of a manifest file — Explorer copy/paste, a cloud-sync
    /// conflict copy, a backup script, a partial restore-from-backup — must never cost the surviving
    /// checkpoint its content. CPE-1847's reverted fix turned this ordinary event into silent
    /// unattended data loss: two copies got two ids, retention pruned one, `release` dropped the
    /// **shared** blob refcounts to zero, and the manifest reported as `kept` could restore nothing.
    ///
    /// Two independent guards now stand between that fixture and the harm, and this test asserts the
    /// outcome rather than either mechanism: the copy is not a checkpoint (`list_manifests`), and even
    /// if it were, pruning it could not free a blob the survivor still names (`prune`).
    #[test]
    fn cpe_1861_a_duplicated_manifest_file_never_costs_the_survivor_its_content() {
        let src = scratch("dup-src");
        let store = scratch("dup-store");
        fs::write(src.join("a.txt"), b"the only copy of this content").unwrap();
        let id = capture_at(&src, &store, 3600);

        let before = planner_view(&store);
        assert_eq!(before, vec![id.clone()], "fixture: {before:?}");
        plant_copy(&store, &id, &format!("{id}-backup"), None);

        // FIXTURE LIVENESS — the copy is on disk, and it genuinely names the same blob the original
        // does, which is the whole reason pruning it was able to destroy the original.
        assert_eq!(manifest_files(&store).len(), 2, "LIVE: the copy is not in manifests/");
        let copy_doc: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(store.join("manifests").join(format!("{id}-backup.json"))).unwrap(),
        )
        .unwrap();
        let hash = copy_doc["files"]["a.txt"]["hash"].as_str().unwrap().to_string();
        assert!(store.join("blobs").join(&hash).exists(), "LIVE: the shared blob is missing");

        // FIXTURE LIVENESS, and the measurement behind the whole design choice: **the refcount cannot
        // answer "does another manifest still name this blob".** `index.json` says one reference while
        // two files on disk name it — a copy adds a namer without ever going through a capture, so the
        // counter and the manifest set disagree by construction.
        let index: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(store.join("index.json")).unwrap()).unwrap();
        assert_eq!(
            index["blobs"][&hash]["refs"].as_u64(),
            Some(1),
            "LIVE: the refcount/namer drift this ticket turns on is not present in the fixture"
        );

        let result = apply(&store.to_string_lossy(), &all_pol(), None).unwrap();

        // THE GATE, asserted on content before anything else: whatever retention says it kept, restores.
        assert!(!result.kept.is_empty());
        for kept in &result.kept {
            let dest = scratch("dup-restore");
            snapshot_capture::restore(&store.to_string_lossy(), kept, &dest.to_string_lossy())
                .unwrap_or_else(|e| panic!("HARM: retention kept {kept} and it cannot restore: {e}"));
            assert_eq!(
                fs::read(dest.join("a.txt")).unwrap(),
                b"the only copy of this content",
                "HARM: the kept checkpoint's tree did not come back"
            );
            let _ = fs::remove_dir_all(&dest);
        }
        assert!(store.join("blobs").join(&hash).exists(), "HARM: the shared blob was deleted");

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    // ---- CPE-1844: a hand-edited index.json steers the byte cap ------------------------------------

    /// Rewrite every `size` recorded in `store`'s `index.json` to `size`, reading it back so a tamper
    /// that silently failed to land can never be mistaken for a guard that worked. Returns the total the
    /// file now claims — which is exactly what `store_total_bytes` used to return.
    fn set_index_sizes(store: &std::path::Path, size: u64) -> u64 {
        let p = store.join("index.json");
        let mut doc: serde_json::Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        for (_h, m) in doc["blobs"].as_object_mut().unwrap().iter_mut() {
            m["size"] = serde_json::json!(size);
        }
        fs::write(&p, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
        let back: serde_json::Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        let blobs = back["blobs"].as_object().unwrap();
        assert!(
            blobs.values().all(|m| m["size"].as_u64() == Some(size)),
            "LIVE: the index.json size tamper never landed on disk"
        );
        let claimed: u64 = blobs.values().map(|m| m["size"].as_u64().unwrap()).sum();
        assert_eq!(
            crate::snapshot_capture::load_store(store).unwrap().total_bytes(),
            claimed,
            "LIVE: the tamper never reached index.json as the production reader sees it"
        );
        claimed
    }

    fn real_blob_bytes(store: &std::path::Path) -> u64 {
        fs::read_dir(store.join("blobs"))
            .map(|rd| {
                rd.flatten()
                    .filter_map(|e| e.metadata().ok())
                    .filter(|m| m.is_file())
                    .map(|m| m.len())
                    .sum()
            })
            .unwrap_or(0)
    }

    /// **CPE-1844's headline harm, at the layer that does the deleting.** `apply`'s byte cap thins
    /// survivors oldest-first until the store's footprint is under the cap or one checkpoint remains, and
    /// the footprint used to be read out of `index.json`. Reproduced on `origin/main` through
    /// `checkpoint_prune_apply` before anything was changed — a 45-byte store, a 1,000,000-byte cap, a
    /// GFS policy keeping all five:
    ///
    /// ```text
    /// index.json: every blob's "size" -> 1000000000      (one text edit; no bytes written)
    ///   preview.total_bytes  45 -> 5000000000
    ///   prune_apply  kept=[newest]  pruned=[the other 4]  bytes_freed=4000000000
    ///   manifests left on disk = 1 of 5
    /// ```
    #[test]
    fn cpe_1844_an_inflated_index_cannot_drive_the_byte_cap_into_deleting_checkpoints() {
        let src = scratch("1844-cap-src");
        let store = scratch("1844-cap-store");
        let store_s = store.to_string_lossy().to_string();

        // Five captures, a day apart, each with its own content — so the GFS pass keeps every one of
        // them and the byte cap is the ONLY thing in this test that can delete anything.
        let mut ids = Vec::new();
        for i in 0..5u64 {
            fs::write(src.join("a.txt"), format!("version {i}").as_bytes()).unwrap();
            ids.push(capture_at(&src, &store, 1_700_000_000 + i * 86_400));
        }
        let policy = RetentionPolicy { hourly: 0, daily: 100, weekly: 0, monthly: 0 };
        let cap = 1_000_000u64;

        let real = real_blob_bytes(&store);
        assert!(real > 0 && real < cap, "LIVE: the honest store is not comfortably under the cap");
        assert!(
            preview(&store_s, &policy).unwrap().prune.is_empty(),
            "LIVE: the GFS pass wants to prune something, so this fixture does not isolate the byte cap"
        );

        let claimed = set_index_sizes(&store, 1_000_000_000);
        assert!(claimed > cap, "LIVE: the tampered claim does not even exceed the cap");
        assert!(
            snapshot_capture::list_manifests(&store_s).unwrap().len() == 5,
            "LIVE: the planner no longer sees all five checkpoints, so the cap has nothing to delete"
        );

        let applied = apply(&store_s, &policy, Some(cap)).unwrap();

        // HARM FIRST: the checkpoints the real on-disk state says should be kept are still there, and
        // still usable. Only then the Result.
        let mut left: Vec<String> = fs::read_dir(store.join("manifests"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        left.sort();
        assert_eq!(
            left.len(),
            5,
            "HARM: a hand-edited index.json deleted the user's other checkpoints — {left:?}"
        );
        for id in &ids {
            snapshot_capture::manifest_snapshot(&store_s, id)
                .unwrap_or_else(|e| panic!("HARM: checkpoint {id} no longer loads: {e}"));
        }
        assert!(applied.pruned.is_empty(), "HARM: pruned {:?}", applied.pruned);
        assert_eq!(applied.kept.len(), 5);
        assert_eq!(applied.bytes_freed, 0, "nothing was freed because nothing was deleted");

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    /// The byte cap must still *work*: a store genuinely over its cap is thinned oldest-first, and stops
    /// as soon as the real footprint is under it. This is the over-tightening pin — a "measure the disk"
    /// fix that simply never pruned would satisfy the test above and be useless.
    #[test]
    fn cpe_1844_the_byte_cap_still_thins_a_store_that_is_genuinely_over_it() {
        let src = scratch("1844-honest-src");
        let store = scratch("1844-honest-store");
        let store_s = store.to_string_lossy().to_string();

        let mut ids = Vec::new();
        for i in 0..4u64 {
            fs::write(src.join("a.txt"), vec![b'a' + i as u8; 200]).unwrap();
            ids.push(capture_at(&src, &store, 1_700_000_000 + i * 86_400));
        }
        let policy = RetentionPolicy { hourly: 0, daily: 100, weekly: 0, monthly: 0 };
        assert!(preview(&store_s, &policy).unwrap().prune.is_empty(), "LIVE: GFS would prune on its own");
        assert_eq!(real_blob_bytes(&store), 800, "LIVE: the fixture's real footprint is not 4 x 200");

        // Cap 700: one prune takes the store to 600, which is under it — so exactly one is deleted.
        let applied = apply(&store_s, &policy, Some(700)).unwrap();
        assert_eq!(applied.pruned, vec![ids[0].clone()], "the oldest, and only the oldest");
        assert_eq!(applied.kept.len(), 3, "HARM: the cap deleted more than it needed to");
        assert_eq!(real_blob_bytes(&store), 600, "the real footprint is now under the cap");
        assert_eq!(applied.bytes_freed, 200, "the figure describes the file that was removed");

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    /// **CPE-1844's second steering input, and the reason the loop re-measures rather than subtracts.**
    /// `total = total.saturating_sub(freed)` carried `index.json`'s recorded sizes forward through the
    /// loop's own arithmetic. With the sizes deflated to 1 the loop believed each prune reclaimed a
    /// single byte and kept deleting long after the store was genuinely under its cap — all the way to
    /// the one-survivor floor. Re-measuring removes the input instead of bounding it.
    #[test]
    fn cpe_1844_a_deflated_index_cannot_keep_the_cap_loop_deleting_past_the_cap() {
        let src = scratch("1844-deflate-src");
        let store = scratch("1844-deflate-store");
        let store_s = store.to_string_lossy().to_string();

        let mut ids = Vec::new();
        for i in 0..4u64 {
            fs::write(src.join("a.txt"), vec![b'a' + i as u8; 200]).unwrap();
            ids.push(capture_at(&src, &store, 1_700_000_000 + i * 86_400));
        }
        let policy = RetentionPolicy { hourly: 0, daily: 100, weekly: 0, monthly: 0 };
        assert!(preview(&store_s, &policy).unwrap().prune.is_empty(), "LIVE: GFS would prune on its own");
        assert_eq!(real_blob_bytes(&store), 800, "LIVE: the fixture's real footprint is not 4 x 200");

        // The tamper: every blob claims one byte. The real footprint is untouched at 800, so the loop
        // legitimately fires — but each prune's *recorded* yield is now 1 instead of 200.
        let claimed = set_index_sizes(&store, 1);
        assert_eq!(claimed, 4, "LIVE: the deflated claim is not 4 x 1");
        assert_eq!(real_blob_bytes(&store), 800, "LIVE: the tamper moved real bytes, which it must not");

        let applied = apply(&store_s, &policy, Some(700)).unwrap();

        // HARM FIRST: three of four checkpoints are what the deflated index used to buy.
        let left = fs::read_dir(store.join("manifests")).unwrap().flatten().count();
        assert_eq!(
            left, 3,
            "HARM: a deflated index.json kept the byte-cap loop deleting past the cap — {} left",
            left
        );
        assert_eq!(applied.pruned, vec![ids[0].clone()]);
        assert_eq!(real_blob_bytes(&store), 600);

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    /// **CPE-1844 — an unreadable `index.json` used to cost one checkpoint per retention pass.**
    /// `load_store` is fail-closed by design (CPE-1705), but it sat *below* `prune`'s point of no
    /// return, so every pass deleted a manifest file and then refused. Measured on `origin/main` through
    /// `checkpoint_prune_apply` over four checkpoints, `index.json` truncated to `{"blobs": {`:
    /// `Err`/3 left, `Err`/2 left, `Err`/1 left, then `Ok`. Walking `prune`'s own gate list — the axis
    /// CPE-1861 recorded as the one its enumeration failed to walk — is what found it.
    #[test]
    fn cpe_1844_an_unreadable_index_refuses_totally_instead_of_costing_a_checkpoint_a_pass() {
        let src = scratch("1844-corrupt-src");
        let store = scratch("1844-corrupt-store");
        let store_s = store.to_string_lossy().to_string();

        let mut ids = Vec::new();
        for i in 0..4u64 {
            fs::write(src.join("a.txt"), format!("version {i}").as_bytes()).unwrap();
            ids.push(capture_at(&src, &store, 1_700_000_000 + i * 3_600));
        }
        // A policy that DOES want to thin, so the pass genuinely reaches `prune`.
        let policy = RetentionPolicy { hourly: 1, daily: 0, weekly: 0, monthly: 0 };
        assert!(!preview(&store_s, &policy).unwrap().prune.is_empty(), "LIVE: nothing would be pruned");

        let idx = store.join("index.json");
        fs::write(&idx, b"{\"blobs\": {").unwrap();
        assert!(
            serde_json::from_str::<serde_json::Value>(&fs::read_to_string(&idx).unwrap()).is_err(),
            "LIVE: index.json still parses, so load_store would not refuse it"
        );
        assert_eq!(
            snapshot_capture::list_manifests(&store_s).unwrap().len(),
            4,
            "LIVE: the planner no longer sees the checkpoints, so nothing would reach prune"
        );

        for pass in 1..=4 {
            let r = apply(&store_s, &policy, None);
            let left = fs::read_dir(store.join("manifests")).unwrap().flatten().count();
            assert_eq!(
                left, 4,
                "HARM: pass {pass} destroyed a checkpoint on an unreadable index.json — {left} left"
            );
            assert!(r.is_err(), "an unreadable refcount ledger must refuse, loudly");
        }
        // And every checkpoint is still usable.
        for id in &ids {
            snapshot_capture::manifest_snapshot(&store_s, id)
                .unwrap_or_else(|e| panic!("HARM: checkpoint {id} no longer loads: {e}"));
        }
        // The footprint no longer goes through `index.json` at all, so a preview still answers honestly
        // on a store whose index is corrupt.
        assert_eq!(preview(&store_s, &policy).unwrap().total_bytes, real_blob_bytes(&store));

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    // ---- CPE-1863: the byte-cap loop must not delete checkpoints that cannot help ------------------

    /// **Every CPE-1863 fixture's liveness, in one place.** These tests are all assertions about the
    /// byte-cap loop, and the loop only reaches its own logic if the GFS pass leaves it something to do
    /// and the store measures as non-empty. This proves that before any harm is asserted: the planner
    /// hands `thin` exactly `expected` checkpoints, the policy would prune **none** of them (so the byte
    /// cap is the only pressure in the test), and the store has a real measured footprint — returned,
    /// because that is what every cap here is expressed relative to.
    ///
    /// Folded into a helper rather than written out per test **on CPE-1861's evidence**: three of that
    /// ticket's tests certified nothing under a decoy-sibling trap, because each carried its own
    /// hand-written liveness checks and the claim inverted from 2-passed/9-failed to 9-passed/2-failed
    /// without anyone noticing. One helper cannot rot in three places.
    fn live_cap_fixture(store: &std::path::Path, policy: &RetentionPolicy, expected: usize) -> u64 {
        let store_s = store.to_string_lossy().to_string();
        let seen = planner_view(store);
        assert_eq!(
            seen.len(),
            expected,
            "LIVE: the planner sees {} checkpoints, not {expected} — {seen:?}",
            seen.len()
        );
        assert!(
            preview(&store_s, policy).unwrap().prune.is_empty(),
            "LIVE: the GFS pass wants to prune on its own, so this fixture does not isolate the byte cap"
        );
        let total = snapshot_capture::store_total_bytes(&store_s).unwrap();
        assert!(total > 0, "LIVE: the store measures as empty, so no cap can bite it");
        total
    }

    /// The other half of a no-progress fixture: the store's blobs are **shared**, which is the condition
    /// under which evicting a checkpoint reclaims nothing. Strictly fewer blob files than checkpoints
    /// proves at least one blob has more than one namer. Returns the blob-file count.
    ///
    /// Paired with [`live_cap_fixture`] so a no-progress test can never quietly become a test of a store
    /// where pruning would have freed bytes anyway — which would pass for the wrong reason.
    fn assert_blobs_are_shared(store: &std::path::Path, checkpoints: usize) -> usize {
        let blobs: Vec<String> = fs::read_dir(store.join("blobs"))
            .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect())
            .unwrap_or_default();
        assert!(!blobs.is_empty(), "LIVE: the store has no blobs, so there is nothing to share");
        assert!(
            blobs.len() < checkpoints,
            "LIVE: {} blob files for {checkpoints} checkpoints — nothing is shared, so a prune WOULD \
             free bytes and this fixture does not test no-progress",
            blobs.len()
        );
        blobs.len()
    }

    /// **CPE-1863's headline harm, with no tamper of any kind in the fixture.** Six identical captures —
    /// an ordinary store where nothing changed between snapshots — so one blob is shared by every
    /// manifest and evicting any of them reclaims nothing. `total = total.saturating_sub(freed)` never
    /// saw the cap met, and the loop's only other exit was its one-survivor floor. Measured on
    /// `origin/main`:
    ///
    /// ```text
    /// apply(cap = total - 1)  ->  kept = 1, pruned = 5, bytes_freed = 0
    /// ```
    ///
    /// Five checkpoints destroyed, zero bytes reclaimed, reported as `Ok`. The cap was never going to be
    /// met by deleting them, because they were not what was using the space.
    #[test]
    fn cpe_1863_a_cap_no_eviction_can_meet_stops_instead_of_emptying_the_store() {
        let src = scratch("1863-shared-src");
        let store = scratch("1863-shared-store");
        let store_s = store.to_string_lossy().to_string();

        // Six captures of the SAME content, a day apart. No tamper: this is what a store looks like when
        // a scheduled capture runs over a folder nobody edited.
        fs::write(src.join("a.txt"), vec![7u8; 400]).unwrap();
        let mut ids = Vec::new();
        for i in 0..6u64 {
            ids.push(capture_at(&src, &store, 1_700_000_000 + i * 86_400));
        }
        let policy = RetentionPolicy { hourly: 0, daily: 100, weekly: 0, monthly: 0 };

        let total = live_cap_fixture(&store, &policy, 6);
        assert_eq!(assert_blobs_are_shared(&store, 6), 1, "LIVE: six identical captures are not one blob");

        // A cap one byte under the store's real footprint: satisfiable only by freeing something, and
        // nothing here can be freed while any manifest survives.
        let applied = apply(&store_s, &policy, Some(total - 1)).unwrap();

        // HARM FIRST, on disk, before the Result is consulted at all.
        let left = manifest_files(&store);
        assert_eq!(
            left.len(),
            5,
            "HARM: the byte cap destroyed {} of 6 checkpoints to reclaim nothing — {left:?}",
            6 - left.len()
        );
        assert_eq!(
            snapshot_capture::store_total_bytes(&store_s).unwrap(),
            total,
            "HARM: the footprint the cap was chasing did not move, which is the whole point"
        );
        assert_kept_ids_all_restore(&store, &applied.kept, "1863-shared");

        // And the Result says so rather than reading as a success.
        assert_eq!(applied.byte_cap, ByteCapOutcome::StoppedNoProgress);
        assert!(applied.byte_cap.cap_missed(), "a cap that was not met must not read as met");
        assert_eq!(applied.bytes_freed, 0, "no eviction here can free a byte");
        assert_eq!(applied.pruned.len(), 1, "the loop stops after the first fruitless eviction");
        assert_eq!(applied.pruned, vec![ids[0].clone()], "and it is the oldest, as always");
        assert_eq!(applied.kept.len(), 5);

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    /// **The over-tightening pin.** A "stop when a prune frees nothing" rule that stopped a loop which
    /// *is* reclaiming bytes would satisfy the test above and make the byte cap useless. Four distinct
    /// 200-byte captures, a cap of 450: the loop must evict **two** — each eviction genuinely freeing
    /// 200 — and stop the moment the store is under the cap, not before and not at the floor.
    #[test]
    fn cpe_1863_the_cap_still_evicts_more_than_once_while_each_eviction_frees_bytes() {
        let src = scratch("1863-progress-src");
        let store = scratch("1863-progress-store");
        let store_s = store.to_string_lossy().to_string();

        let mut ids = Vec::new();
        for i in 0..4u64 {
            fs::write(src.join("a.txt"), vec![b'a' + i as u8; 200]).unwrap();
            ids.push(capture_at(&src, &store, 1_700_000_000 + i * 86_400));
        }
        let policy = RetentionPolicy { hourly: 0, daily: 100, weekly: 0, monthly: 0 };
        assert_eq!(live_cap_fixture(&store, &policy, 4), 800, "LIVE: the fixture is not 4 x 200 bytes");

        // 800 -> 600 (still over) -> 400 (under). Two evictions, and the third must not happen.
        let applied = apply(&store_s, &policy, Some(450)).unwrap();

        assert_eq!(
            applied.pruned,
            vec![ids[0].clone(), ids[1].clone()],
            "HARM: the no-progress rule stopped a loop that was reclaiming bytes"
        );
        assert_eq!(manifest_files(&store).len(), 2, "HARM: the cap deleted more than it needed to");
        assert_eq!(real_blob_bytes(&store), 400, "the real footprint is now under the cap");
        assert_eq!(applied.bytes_freed, 400);
        assert_eq!(applied.byte_cap, ByteCapOutcome::Met);
        assert!(!applied.byte_cap.cap_missed());
        assert_kept_ids_all_restore(&store, &applied.kept, "1863-progress");

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    /// A cap that is genuinely unreachable because there is nothing left to evict — every eviction here
    /// frees its 200 bytes, so the loop is working, and it still cannot get a 4-checkpoint store under 1
    /// byte. It stops at the one-survivor floor and **says so**. Before CPE-1863 this returned an `Ok`
    /// indistinguishable from a cap that was met.
    #[test]
    fn cpe_1863_a_cap_no_store_can_reach_reports_the_floor_rather_than_success() {
        let src = scratch("1863-floor-src");
        let store = scratch("1863-floor-store");
        let store_s = store.to_string_lossy().to_string();

        let mut ids = Vec::new();
        for i in 0..4u64 {
            fs::write(src.join("a.txt"), vec![b'a' + i as u8; 200]).unwrap();
            ids.push(capture_at(&src, &store, 1_700_000_000 + i * 86_400));
        }
        let policy = RetentionPolicy { hourly: 0, daily: 100, weekly: 0, monthly: 0 };
        assert_eq!(live_cap_fixture(&store, &policy, 4), 800, "LIVE: the fixture is not 4 x 200 bytes");

        let applied = apply(&store_s, &policy, Some(1)).unwrap();

        assert_eq!(applied.kept.len(), 1, "the floor still holds: a store is never thinned to zero");
        assert_eq!(applied.pruned, vec![ids[0].clone(), ids[1].clone(), ids[2].clone()]);
        assert_eq!(applied.bytes_freed, 600, "every one of those evictions did reclaim its bytes");
        assert_eq!(
            applied.byte_cap,
            ByteCapOutcome::StoppedAtFloor,
            "HARM: a cap the store cannot reach was reported as met"
        );
        assert!(applied.byte_cap.cap_missed());
        assert_kept_ids_all_restore(&store, &applied.kept, "1863-floor");

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    /// A pass with no cap at all — every caller in the app today, `snapshot_schedule::snapshot_run_due`
    /// included — must not claim a cap outcome it was never given.
    #[test]
    fn cpe_1863_a_pass_with_no_cap_reports_no_cap() {
        let src = scratch("1863-nocap-src");
        let store = scratch("1863-nocap-store");
        let store_s = store.to_string_lossy().to_string();
        fs::write(src.join("a.txt"), b"v1").unwrap();
        let m1 = capture_at(&src, &store, 3600);
        fs::write(src.join("a.txt"), b"v2").unwrap();
        let m2 = capture_at(&src, &store, 2 * 3600);
        fs::write(src.join("a.txt"), b"v3").unwrap();
        let m3 = capture_at(&src, &store, 3 * 3600);
        assert_eq!(planner_view(&store).len(), 3, "LIVE: the planner does not see three checkpoints");

        // A policy that DOES thin, so the GFS pass really deletes something — and still no cap verdict.
        let applied = apply(&store_s, &all_pol(), None).unwrap();
        assert_eq!(applied.pruned, vec![m1], "LIVE: the GFS pass pruned nothing, so this proves nothing");
        assert_eq!(applied.byte_cap, ByteCapOutcome::NotRequested);
        assert!(!applied.byte_cap.cap_missed());
        let _ = (m2, m3);

        // `Some(0)` is documented as "no cap", not "a cap of zero bytes", and must read the same way.
        let applied = apply(&store_s, &all_pol(), Some(0)).unwrap();
        assert_eq!(applied.byte_cap, ByteCapOutcome::NotRequested);

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    /// **The CPE-1861 interaction, which the fix must not turn into a stall.** CPE-1861 accepted a
    /// residual: a manifest file `list_manifests` refuses — an Explorer `"<id> - Copy.json"`, ~122 bytes,
    /// invisible to the planner and permanent — still counts as a namer to `prune`'s witness and to
    /// `store_total_bytes`. Its snapshot's blobs are therefore pinned, counted toward the cap, and
    /// reclaimable by no prune retention can make, so `freed == 0` is the *expected* outcome there.
    ///
    /// Three things are asserted, in this order: the pass still returns `Ok` and the GFS half of it still
    /// ran (a stop, not a stall); it stops after one eviction instead of walking to the floor; and a
    /// **second pass is not wedged** — it evicts a candidate that does free bytes, so the rule stops a
    /// fruitless walk without stopping a productive one.
    ///
    /// It also pins this fix's accepted cost, in the open: pruning `m2` *would* have freed 200 bytes,
    /// and pass 1 does not find that out. Continuing would spend certain destruction of the user's
    /// history on a speculative reclaim, and the usual reason an eviction freed nothing is that the
    /// blobs are shared with everything — in which case continuing destroys the lot for nothing.
    #[test]
    fn cpe_1863_an_invisible_manifest_pinning_blobs_stops_the_cap_without_stalling_it() {
        let src = scratch("1863-pinned-src");
        let store = scratch("1863-pinned-store");
        let store_s = store.to_string_lossy().to_string();

        let mut ids = Vec::new();
        for i in 0..3u64 {
            fs::write(src.join("a.txt"), vec![b'a' + i as u8; 200]).unwrap();
            ids.push(capture_at(&src, &store, 1_700_000_000 + i * 86_400));
        }
        let policy = RetentionPolicy { hourly: 0, daily: 100, weekly: 0, monthly: 0 };
        assert_eq!(live_cap_fixture(&store, &policy, 3), 600, "LIVE: the fixture is not 3 x 200 bytes");

        // The residual, exactly as CPE-1861 leaves it: an ordinary Explorer copy of the OLDEST
        // checkpoint. The planner never sees it; `prune`'s witness always does.
        let copy = plant_copy(&store, &ids[0], &format!("{} - Copy", ids[0]), None);
        assert!(copy.exists() && fs::metadata(&copy).unwrap().len() > 0, "LIVE: the copy is not on disk");
        assert_eq!(
            live_cap_fixture(&store, &policy, 3),
            600,
            "LIVE: the copy became visible to the planner, so this is not CPE-1861's residual"
        );
        assert_eq!(manifest_files(&store).len(), 4, "LIVE: the copy is not in manifests/");

        let pass1 = apply(&store_s, &policy, Some(1)).unwrap();

        // HARM FIRST: two checkpoints are what the walk to the floor used to cost here.
        assert_eq!(
            planner_view(&store).len(),
            2,
            "HARM: the cap walked to the floor over blobs no prune of it could ever free"
        );
        assert_eq!(pass1.pruned, vec![ids[0].clone()]);
        assert_eq!(pass1.bytes_freed, 0, "the copy still names m1's blob, so nothing was reclaimed");
        assert_eq!(pass1.byte_cap, ByteCapOutcome::StoppedNoProgress);
        assert_kept_ids_all_restore(&store, &pass1.kept, "1863-pinned-1");

        // A STOP, NOT A STALL: pass 2 runs, and evicting m2 does free its 200 bytes, so the no-progress
        // rule does not freeze a store it once fired on.
        let pass2 = apply(&store_s, &policy, Some(1)).unwrap();
        assert_eq!(pass2.pruned, vec![ids[1].clone()], "HARM: the store is wedged — pass 2 did nothing");
        assert_eq!(pass2.bytes_freed, 200, "HARM: a productive eviction was refused");
        assert_eq!(pass2.byte_cap, ByteCapOutcome::StoppedAtFloor, "one survivor left, cap still unmet");
        assert_kept_ids_all_restore(&store, &pass2.kept, "1863-pinned-2");

        // The pinned blob is still there, still unreclaimable, and still counted — CPE-1861's residual,
        // unchanged by this ticket and now *named* by the outcome rather than paid for in checkpoints.
        assert!(
            snapshot_capture::store_total_bytes(&store_s).unwrap() >= 200,
            "CPE-1861's pinned blob is still part of the measured footprint"
        );

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    // ---- CPE-1871: pin the two argued-but-undefended design decisions -----------------------------

    /// RAII guard staging CPE-1871's fixture: makes `path` — a blob file directly under `blobs_dir`
    /// (`<store>/blobs/`) — impossible for `fs::remove_file` to remove, while leaving its bytes fully
    /// readable (`fs::metadata`/`fs::read_dir` inside `prune`/`store_total_bytes` are unaffected). This
    /// is `snapshot_capture.rs:741`'s `if fs::remove_file(&path).is_ok()` failing on purpose, so `prune`
    /// credits the blob 0 bytes freed while `store_total_bytes`'s witness — which re-scans `manifests/`,
    /// not `blobs/` alone — still excludes it the moment its last namer is pruned. That gap between
    /// "`prune`'s own return value" and "the re-measured reachable footprint" is exactly what the two
    /// design decisions on the byte-cap loop turn on.
    ///
    /// One mechanism per OS the 3-way CI matrix runs, per the ticket's own portability notes:
    ///
    /// - **Windows**: an open handle without `FILE_SHARE_DELETE` — "the ordinary cause in the field".
    ///   `std::fs::File::open` cannot stage this; it always adds `FILE_SHARE_DELETE` (see `fsutil.rs`'s
    ///   `cpe_1739_windows_a_foreign_share_read_write_handle_still_blocks_the_save`, which measures the
    ///   same construction against a save instead of a delete). `DeleteFileW` — what `remove_file` goes
    ///   through — needs to open the target for delete access, and that open fails with a sharing
    ///   violation while this handle is outstanding.
    /// - **Unix (Linux/macOS)**: the containing `blobs/` directory loses its write bit. POSIX `unlink`
    ///   is a mutation of the *directory entry*, so it needs write+execute on the PARENT, never on the
    ///   target file itself — a read-only directory blocks the delete while every read this fixture
    ///   still needs (`fs::metadata`, `fs::read_dir`) keeps working, because those only need read+execute
    ///   on the directory. This is the ticket's "read-only parent directory" leg, deliberately chosen
    ///   over `chattr +i`/`chflags uchg` — no elevated capability or root is required, so it stages the
    ///   same way in an ordinary CI runner as it does on a developer machine.
    ///
    /// `stage` returns `None` if the mechanism could not be constructed; callers MUST route that through
    /// [`crate::fsutil::require_staged`] rather than skipping silently — every OS this module's tests run
    /// under is one where this is supposed to work.
    struct Undeletable {
        #[cfg(windows)]
        handle: windows::Win32::Foundation::HANDLE,
        #[cfg(unix)]
        dir: std::path::PathBuf,
    }

    impl Undeletable {
        #[cfg(windows)]
        fn stage(path: &std::path::Path, _blobs_dir: &std::path::Path) -> Option<Self> {
            use windows::core::PCWSTR;
            use windows::Win32::Storage::FileSystem::{
                CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
            };
            fn wide(p: &std::path::Path) -> Vec<u16> {
                use std::os::windows::ffi::OsStrExt;
                p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
            }
            let handle = unsafe {
                CreateFileW(
                    PCWSTR(wide(path).as_ptr()),
                    0x8000_0000, // GENERIC_READ
                    FILE_SHARE_READ | FILE_SHARE_WRITE, // deliberately NOT FILE_SHARE_DELETE
                    None,
                    OPEN_EXISTING,
                    FILE_FLAGS_AND_ATTRIBUTES(0),
                    None,
                )
            }
            .ok()?;
            Some(Self { handle })
        }

        #[cfg(unix)]
        fn stage(_path: &std::path::Path, blobs_dir: &std::path::Path) -> Option<Self> {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(blobs_dir, std::fs::Permissions::from_mode(0o555)).ok()?;
            Some(Self { dir: blobs_dir.to_path_buf() })
        }

        #[cfg(not(any(windows, unix)))]
        fn stage(_path: &std::path::Path, _blobs_dir: &std::path::Path) -> Option<Self> {
            None
        }
    }

    impl Drop for Undeletable {
        fn drop(&mut self) {
            #[cfg(windows)]
            unsafe {
                let _ = windows::Win32::Foundation::CloseHandle(self.handle);
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                // Restore write+execute so the scratch dir can be torn down afterwards.
                let _ = std::fs::set_permissions(&self.dir, std::fs::Permissions::from_mode(0o755));
            }
        }
    }

    /// **CPE-1871's headline fixture, and the one test both design decisions on the byte-cap loop turn
    /// on.** Three distinct 200-byte captures, no sharing, so the GFS pass keeps all three and only the
    /// byte cap can evict anything. The oldest checkpoint's blob is staged undeletable ([`Undeletable`])
    /// before the cap runs.
    ///
    /// `prune` still deletes the oldest MANIFEST — that `fs::remove_file` is a different file, at a
    /// different point-of-no-return earlier in `prune`, and nothing here touches it — so the eviction
    /// genuinely happens and the blob genuinely loses its only namer. But the blob's own file cannot be
    /// removed, so `prune` returns `freed_now == 0` for this eviction: `snapshot_capture.rs`'s
    /// `if fs::remove_file(&path).is_ok() { freed = freed.saturating_add(size); }` never adds anything.
    ///
    /// `store_total_bytes`, re-measured immediately after, tells a different story: it re-scans
    /// `manifests/` for who still names each blob on disk, and with the oldest manifest gone nothing
    /// names this one any more — so it is excluded from the reachable footprint regardless of whether its
    /// file was actually removed. The real, re-measured total genuinely falls by 200 even though `prune`
    /// credited the eviction nothing.
    ///
    /// **This is the exact shape both decisions defend.** The current loop reads progress off the
    /// re-measured total (`let progressed = after < total;` fed by `store_total_bytes`), so it sees the
    /// genuine fall and reports the cap `Met`. Either rejected alternative — reading progress off
    /// `freed_now` (CPE-1863) or reconstructing `after` by subtracting `freed_now` instead of
    /// re-measuring (CPE-1844) — sees `freed_now == 0`, concludes nothing happened, and stops the loop
    /// with `StoppedNoProgress` one eviction short of a cap that was, in fact, met.
    #[test]
    fn cpe_1871_an_undeletable_blobs_freed_bytes_still_count_as_progress() {
        let src = scratch("1871-src");
        let store = scratch("1871-store");
        let store_s = store.to_string_lossy().to_string();

        let mut ids = Vec::new();
        for i in 0..3u64 {
            fs::write(src.join("a.txt"), vec![b'a' + i as u8; 200]).unwrap();
            ids.push(capture_at(&src, &store, 1_700_000_000 + i * 86_400));
        }
        let policy = RetentionPolicy { hourly: 0, daily: 100, weekly: 0, monthly: 0 };
        assert_eq!(live_cap_fixture(&store, &policy, 3), 600, "LIVE: the fixture is not 3 x 200 bytes");

        // The blob about to be orphaned: the oldest checkpoint's own unique content.
        let hash0 = snapshot_capture::manifest_snapshot(&store_s, &ids[0])
            .unwrap()
            .values()
            .next()
            .unwrap()
            .hash
            .clone();
        let blobs_dir = store.join("blobs");
        let blob0_path = blobs_dir.join(&hash0);
        assert!(blob0_path.exists(), "LIVE: the oldest checkpoint's blob is not where this test expects");

        let guard = Undeletable::stage(&blob0_path, &blobs_dir);
        assert!(
            crate::fsutil::require_staged("cpe_1871_undeletable_blob", true, guard.is_some()),
            "could not stage an undeletable blob on this platform"
        );

        // Cap 500: satisfiable by one eviction of the (nominally) 200-byte oldest checkpoint — but only
        // if the loop trusts the re-measured footprint over `prune`'s own bytes_freed.
        let applied = apply(&store_s, &policy, Some(500)).unwrap();

        // HARM/LIVE FIRST, on disk, before the Result is trusted at all.
        assert!(
            !store.join("manifests").join(format!("{}.json", ids[0])).exists(),
            "LIVE: prune's point-of-no-return manifest delete did not happen — this fixture never reached \
             the blob-delete step at all"
        );
        assert!(
            blob0_path.exists(),
            "LIVE: the blob file was removed anyway — this fixture did not stage an undeletable blob, so \
             it proves nothing about the divergence it exists to test"
        );
        assert_eq!(
            applied.pruned,
            vec![ids[0].clone()],
            "LIVE: the byte cap did not evict the oldest checkpoint at all"
        );
        assert_eq!(
            applied.bytes_freed, 0,
            "LIVE: prune's own remove_file must have failed for this hash — bytes_freed should read 0, \
             not the blob's nominal size"
        );
        let real_after = snapshot_capture::store_total_bytes(&store_s).unwrap();
        assert_eq!(
            real_after, 400,
            "LIVE: the re-measured footprint did not genuinely fall by the orphaned blob's size"
        );

        // THE PIN. Current code: Met. `freed_now > 0` (CPE-1863's rejected form) or
        // `total.saturating_sub(freed_now)` in place of the re-measure (CPE-1844's rejected form): both
        // read this eviction as having made no progress and stop at StoppedNoProgress instead — even
        // though the cap genuinely was met.
        assert_eq!(
            applied.byte_cap,
            ByteCapOutcome::Met,
            "CPE-1871: a prune that reports bytes_freed == 0 must still count as progress when the \
             re-measured store footprint genuinely fell — got {:?}",
            applied.byte_cap
        );
        assert!(!applied.byte_cap.cap_missed());
        assert_eq!(applied.kept.len(), 2, "HARM: the cap evicted more than the one checkpoint it needed to");
        assert_kept_ids_all_restore(&store, &applied.kept, "1871");

        drop(guard);
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }
}
