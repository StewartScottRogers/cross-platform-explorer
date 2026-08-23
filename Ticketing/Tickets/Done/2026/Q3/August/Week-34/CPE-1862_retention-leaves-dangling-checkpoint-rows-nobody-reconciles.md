---
id: CPE-1862
title: retention prunes manifests but nobody reconciles checkpoints.json, leaving dangling rows
type: bug
priority: Medium
status: Done
tags: ready
estimate: M
created: 2026-08-22
closed: 2026-08-23
---

## Problem

`checkpoints.json` is an **append-only index that nothing reconciles**. Retention prunes manifests
(`snapshot_prune::apply`, reached from `checkpoint_prune_apply` at
`crates/server/src/checkpoint_store.rs:500-508`), but the rows naming those manifests stay.

So the UI lists checkpoints whose manifest is gone. Selecting one gets an error from `load_manifest`
rather than a row that was never offered.

## Why it is filed separately

CPE-1861's review found this while confirming the change did not touch it. It was originally noted as
belonging with **CPE-1845**, and the reviewer disagreed with that home for a good reason:

- CPE-1845 is about `OpResult` lacking a structural flag to separate a deliberate hold-back from a real
  failure — a **result-shape** defect.
- This is an **index nobody reconciles** — a different defect that happens to live in the same file.

More practically: CPE-1845's own ticket carried no record of it, so the note existed only in CPE-1861's
Work Log and PR body and would have been lost. A search of the open queues found no existing ticket
mentioning `checkpoints.json`.

## Acceptance criteria

- [x] After a retention pass, `checkpoints.json` contains no row whose manifest is gone — either
      reconciled at prune time, or filtered at read time. Decide which and record why; a filter at read
      time leaves the file growing, a reconcile at prune time makes retention a writer of a file it
      currently only reads through.
- [x] A checkpoint the user can see is a checkpoint they can act on. Pin that: list, then act on every
      row listed, and assert none errors with a missing manifest.
- [x] Check what happens to a row whose manifest is present but **unloadable** — CPE-1861 established
      that `list_manifests` now skips a manifest disagreeing with its filename, failing
      `validate_manifest_id`, or contradicting its own `file_count`. Such a manifest is never pruned, so
      its row stays valid-looking while retention ignores the file entirely. Say whether the UI should
      show it, and what it should say.
- [x] Red-proof each test with the minimal realistic change, observe red, revert, record the line.
- [x] Assert each fixture is live — that the prune actually removed the manifest — before asserting the
      row's state. Six inert tests were caught on CPE-1823 and one ordering bug on CPE-1861, both because
      an assertion stood in for the thing it was meant to observe.

## Notes

Confirmed pre-existing and untouched by CPE-1861: `checkpoint_prune_apply` is a two-line pass-through and
is not in that diff at all; its whole `checkpoint_store.rs` change sits inside `mod tests`.

Read CPE-1861's Work Log before starting — it carries the identity rules that decide which manifests
retention will and will not act on, which is exactly what determines whether a row can go stale.

Related: CPE-1845 (the `OpResult` discriminant, same file, different defect), CPE-1861 (the identity
rules), CPE-1844 (`index.json` steering prune — the other unreconciled store file).

## Work Log

### 2026-08-23 — fixed, branch `cpe-1862-reconcile-checkpoints-json`

**What the user experiences with a dangling entry today, confirmed rather than assumed** (and this is
what an existing test, `snapshot_schedule::run_due_applies_retention_pruning_to_each_captured_root`,
already pins on `main`): a dangling row is neither silent nor a partial restore. `checkpoint_revert` /
`checkpoint_preview_revert` both resolve `manifest_id` through `snapshot_capture::load_manifest`, which
does a plain `fs::read_to_string` on the (now-deleted) manifest file and returns `Err(".../<id>.json:
The system cannot find the file specified.")`. So today the failure mode is: the checkpoint stays listed
after retention silently removed it, and clicking it produces a clear-but-surprising load error for
something the panel just told the user it could restore. The ticket's own framing ("gets an error from
`load_manifest` rather than a row that was never offered") is exactly this, confirmed by the existing
test rather than re-derived.

**The fix, and the decision the first acceptance criterion asks for.** Both mechanisms are implemented,
because they answer different questions and one cannot cover the other's case:

1. **Reconcile at prune time — the primary answer, in `checkpoint_prune_apply`.** Right after
   `snapshot_prune::apply` deletes manifests, a new `reconcile_checkpoints` rewrites `checkpoints.json`
   (crash-safe temp-file + rename, mirroring `trim_failures`) to keep only rows whose `manifest_id` is in
   `result.kept` — i.e. the exact post-apply on-disk/loadable set. Chosen over "filter at read time only"
   because retention is already the actor mutating this root's manifest store; making it also retire the
   index rows that named what it just deleted keeps `checkpoints.json` bounded to what's actually
   restorable, instead of growing forever with rows nothing will ever act on again. It's also
   self-healing: because it keeps *only* what's currently in `kept` (not merely subtracting this pass's
   `pruned`), every prune pass reconciles the whole file against the current live set, cleaning up any
   row that drifted stale by any means — including from before this fix existed on an install that
   already has dangling rows. Reconciliation failure is deliberately swallowed (best-effort): the
   manifest deletion is already done and irreversible by the time this runs, and a `checkpoints.json`
   this call couldn't rewrite still can't mislead the user, because of mechanism 2.
2. **Filter at read time — the backstop, in `checkpoint_list`.** Every read is now filtered against
   `snapshot_capture::list_manifests`'s "fit to steer a retention decision" set — the same set
   `snapshot_prune::apply` itself plans against. This is not redundant with (1): it is the *only*
   mechanism that can address AC3's case. A manifest that is **present but unloadable** per CPE-1861's
   identity rules (inner id disagreeing with its filename, a crafted stem, a `file_count`/hash
   contradiction) is *never* pruned at all — CPE-1861's deliberate leak-over-corruption direction — so it
   never appears in `result.kept` or `result.pruned`, and reconciliation (1) has no signal to act on
   during the pass that first sees it. Only a read-time check, re-derived on every list, can keep such a
   row off the UI. (It does, however, get swept up by the *next* prune pass's reconciliation, since it's
   absent from `kept` there too — demonstrated in the second test below.)

**AC3's answer, stated plainly: the UI should not show it, and there is nothing further to say.** A
present-but-unloadable manifest is excluded from `checkpoint_list` exactly like a pruned one — nothing
distinguishes "gone" from "present but untrustworthy" in what the user is offered, because neither is
actionable. No new UI copy is warranted: there is nothing the user can do about a manifest failing
CPE-1861's identity checks, and the manifest file itself is deliberately left on disk untouched (not
deleted, not "fixed") for the same reason `prune`'s own leak-over-corruption tradeoff leaves such files
alone — recoverable by hand inspection, never silently discarded.

**Tests — both red-proofed against the real retention path, never a hand-edited `checkpoints.json`:**

- `cpe_1862_retention_reconciles_checkpoints_json_and_every_listed_row_still_loads` — three real
  captures placed in the same hourly bucket, a genuine `checkpoint_prune_apply(hourly: 1)` that deletes
  two manifest files, then asserts (a) the manifests are actually gone from disk (fixture liveness), (b)
  the raw `checkpoints.json` (via the unfiltered `read_checkpoints`) names only the survivor, and (c)
  every row `checkpoint_list` returns loads cleanly through `checkpoint_preview_revert` (AC2, "list, then
  act on every row listed").
  - Red-proof: commented out the `reconcile_checkpoints` call in `checkpoint_prune_apply`. Observed:
    `assertion left == right failed: HARM: checkpoints.json still names a manifest retention
    deleted / left: ["...562", "...539", "...511"] / right: ["...562"]`. Reverted.
- `cpe_1862_a_present_but_unloadable_manifest_is_never_listed` — two real captures, then one manifest's
  inner `id` is rewritten to a sibling's id (CPE-1861's own "inner id → a sibling's id" shape — the same
  tamper an Explorer copy or a cloud-sync conflict would produce). Asserts the tamper reaches
  `list_manifests`'s exclusion before any listing decision (fixture liveness), that `checkpoint_list`
  excludes it while `checkpoints.json` itself is still untouched, and then runs a *generous* retention
  pass (`hourly: 5, daily: 5, weekly: 5, monthly: 5`, i.e. one that would keep everything it can see) to
  show the tampered manifest is neither kept nor pruned — it's simply invisible to retention — yet its
  now-orphaned `checkpoints.json` row is swept up by that pass's reconciliation anyway, while the
  tampered manifest file itself survives on disk untouched.
  - Red-proof: changed `checkpoint_list`'s filter predicate to `|c| true || live.contains(...)` (i.e.
    no filtering). Observed: `assertion left == right failed: HARM: a checkpoint whose manifest fails
    CPE-1861's identity check was listed as actionable / left: ["...985", "...972"] / right: ["...972"]`.
    Reverted.

**Evidence — full suite, both feature modes, from `crates/server`:**

```text
cargo clippy --all-targets -- -D warnings                      clean, 0 warnings
cargo test --lib                                                2367 passed; 0 failed; 8 ignored
cargo clippy --all-targets --features index -- -D warnings      clean, 0 warnings
cargo test --lib --features index                               2415 passed; 0 failed; 8 ignored
cargo clippy --all-targets --features specta -- -D warnings      clean, 0 warnings (bindings-codegen mode; no
                                                                  specta::Type struct was touched, so no
                                                                  regen was needed)

# src-tauri (its own test caught a latent bug my fix exposed — see the entry below)
cargo clippy --all-targets -- -D warnings   (src-tauri)          clean, 0 warnings
cargo test --lib                            (src-tauri)          214 passed; 0 failed
```

CPE-1871's just-merged byte-cap pin test
(`snapshot_prune::tests::cpe_1871_an_undeletable_blobs_freed_bytes_still_count_as_progress`) passes
unmodified — this change never touches `snapshot_prune.rs`'s `apply`, only its caller in
`checkpoint_store.rs`.

### 2026-08-23 — CI (`Backend`, all three OSes) caught a pre-existing latent test bug my fix exposed

`src-tauri/src/lib.rs`'s
`tests::snapshot_schedule_tick_captures_a_due_enabled_root_then_holds_off_within_interval` failed on all
three `Backend` OS legs after the first push: `assertion left == right failed: interval elapsed ⇒ second
capture / left: 1 / right: 2`.

Root cause, traced rather than patched over: `snapshot_run_due` retention-prunes after every scheduled
capture using each manifest's **real** `created_ms` (`SystemTime::now()`), but this test injects a **fake**
`now` (1000s, then 5000s) only for the due/hold-off scheduling clock — the two clocks were never the same
clock. Both real captures in the test happen milliseconds apart in wall-clock time, so under
`RetentionPolicy::default()` (`hourly: 24`) they always collided into the *same* real hourly bucket, and
the second scheduled prune genuinely thinned them to one survivor on disk — even though the test's fake
clock says they're 4000s (more than an hour) apart. Before this ticket's fix, `checkpoint_list` was blind
to that: it just echoed `checkpoints.json`'s raw append count (2), so the test's `== 2` assertion passed by
coincidence, never having actually verified two *restorable* checkpoints existed. That is CPE-1862's bug,
caught in the wild by CI the moment `checkpoint_list` started telling the truth.

Fixed the test, not routed around: after tick 1's capture, its manifest's `created_ms` is hand-edited to
match the tick's own injected `now` (mirroring the `set_manifest_created_ms` pattern already used in
`checkpoint_store.rs`'s own tests), via a small `find_manifest` helper that locates the file by name under
the `HeadlessCtx`'s app-data dir (`checkpoint_store::store_dir_for`'s hashing is private to that module,
so a src-tauri test can't recompute the path directly). That makes the fake scheduling clock and the real
retention clock agree — as they would in the actual running app — so the two captures land in genuinely
different hourly buckets (0 and 1) and both legitimately survive, restoring the test's original assertion
value as a fact instead of an accident. Verified: `cargo test --lib` in `src-tauri` — 214 passed, 0 failed;
`cargo clippy --all-targets -- -D warnings` — clean.

**Assumptions logged:**
- CI's "both feature modes" for `crates/server` are `default` and `--features index` (per
  `.github/workflows/ci.yml`'s `server — clippy + test` step); I ran those plus `--features specta`
  (the bindings-codegen mode) for extra coverage since no `specta::Type` struct changed.
- Reconciliation swallows its own write failure (`let _ = reconcile_checkpoints(...)`) rather than
  propagating it as an error from `checkpoint_prune_apply`. Judgment call: the actual retention (deleting
  manifests) already succeeded and must not be reported as failed over a secondary bookkeeping write, and
  `checkpoint_list`'s independent read-time filter means a failed rewrite here still can't mislead the
  user — it only means the next successful prune pass, or a future `checkpoint_list` call, does the
  cleanup instead.
- Scope held to the prune/capture/manifest side per the sprint brief — no frontend changes. CPE-1857 is
  independently touching the revert/restore write path; nothing here overlaps it (I only changed
  `checkpoint_list` and `checkpoint_prune_apply`, never `checkpoint_revert`/`checkpoint_revert_one`/
  `execute_restore`).
