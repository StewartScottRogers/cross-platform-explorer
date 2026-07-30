---
id: CPE-1134
title: "Checkpoint preview: thread revert_attribution so drift flags only truly-outside changes"
type: enhancement
component: Backend
priority: medium
status: Backlog
tags: ready
created: 2026-07-29
epic: CPE-732
---

## Summary
`checkpoint_preview_revert` in `crates/server/src/checkpoint_store.rs` (line ≈ 240) currently classifies drift
against an **empty touched-set** (lines 250-252):

```rust
// No agent attribution at this layer → classify against an empty touched-set so every diverging path
// is reported as drift (conservative warn). See `RevertPreview`'s doc.
let classified = classify_plan(&plan, &checkpoint, &current, &std::collections::BTreeSet::new());
```

So the preview conservatively warns about **every** diverging path, even ones the agent itself changed. The
`revert_attribution` module (`crates/server/src/revert_attribution.rs`, CPE-1079) already computes exactly the
set needed — `agent_touched(events, session, since_ts, root) -> BTreeSet<String>` — but it is not wired into the
preview. This ticket threads it in so drift flags only **truly-outside** changes (someone/something other than
the reverting agent), which is the whole point of the attribution work.

The epic CPE-732 work log records this as an explicit **optional headless refinement** of the checkpoint engine;
it is pure logic, testable headlessly, with no GUI dependency (the restore panel that would surface it is the
deferred GUI cap CPE-1126).

## Design (headless, backward-compatible signature change)
- Add an **optional session** parameter to `checkpoint_preview_revert` — e.g.
  `session: Option<&str>`. When `None`, keep today's conservative behaviour exactly (empty touched-set → warn
  about everything), so no existing caller breaks. When `Some(sess)`:
  1. Load the session's audit events via `audit_journal::read_session(<audit_base>, sess)`.
     - **Design question for the worker:** confirm the audit-journal base dir. `checkpoint_store` already
       derives its own dir from `ctx.app_data_dir()` (`checkpoints_base`, line 156); find where
       `audit_journal::record` is written from (grep `record(` / `record_many(` callers in `lib.rs` and the
       app adapter) and reuse the same `ctx.app_data_dir()`-relative base. Add a private
       `audit_base(ctx)` helper mirroring `checkpoints_base`.
  2. Compute `let touched = revert_attribution::agent_touched(&events, sess, checkpoint.ts, root);`
     (`checkpoint.ts` is the checkpoint timestamp — the manifest's `Checkpoint.ts: u64`).
  3. Pass `&touched` to `classify_plan` instead of the empty set.
- Update the doc comment on `checkpoint_preview_revert` and on `RevertPreview` to describe the new behaviour
  (attribution-aware when a session is supplied; conservative when not).
- Update the thin dispatcher / any existing caller to pass `None` for now (no live GUI caller yet — CPE-1126 is
  deferred), so this is a pure backward-compatible enablement.
- No new deps. Std-only. Behaviour identical across the three CI OSes (paths are `/`-segment keys, per
  `revert_attribution`'s module doc).

## Acceptance Criteria
- [x] With `session: None`, `checkpoint_preview_revert` produces byte-identical results to today (existing tests
      at lines ~378/399/420 still pass unchanged, adjusted only for the new arg).
- [x] With `session: Some(sess)` and a seeded audit journal, a path the agent `sess` mutated at/after the
      checkpoint ts is **NOT** counted as drift, while a path changed by a *different* session (or no session)
      **IS** flagged as drift — proven by a new unit test that seeds the journal + a live-tree divergence.
- [x] A missing/empty audit journal degrades to the conservative behaviour (no panic; treated as empty
      touched-set).
- [x] All `crates/server` tests pass; `cargo clippy --all-targets -D warnings` clean.

## Work Log (2026-07-29)
- **Audit-base-dir finding:** grepped `audit_journal::record(`/`record_many(` callers in `src-tauri/src/lib.rs`
  (`audit_record` / `audit_record_batch`-style commands around line 2392/4977) and found the writer's base dir
  is `audit_dir(app) = server_ctx::TauriCtx::new(app).app_data_dir()?.join("audit")` (line ~2373). That's the
  exact same `ServerCtx::app_data_dir()` seam `checkpoint_store`'s own `checkpoints_base` uses
  (`ctx.app_data_dir()?.join("checkpoints")`), so the new private `audit_base(ctx)` helper mirrors it 1:1:
  `ctx.app_data_dir()?.join("audit")`. Confirmed this is also what the module doc already called "the audit
  journal's `audit/`" (sibling of `checkpoints/`) — no new base-dir invention needed, just reusing the seam.
- Added `session: Option<&str>` to `checkpoint_preview_revert`. `None` keeps the pre-existing empty-touched-set
  path byte-for-byte; `Some(sess)` reads `audit_journal::read_session(&audit_base(ctx)?, sess)`, looks up this
  `manifest_id`'s recorded `Checkpoint.ts` from the on-disk index (the captured `Snapshot` itself is just a
  path→state map with no timestamp field of its own — the ticket's "`checkpoint.ts`" refers to the index
  entry, not the snapshot), and folds via `revert_attribution::agent_touched(&events, sess, ts, root)`.
- Updated doc comments on `checkpoint_preview_revert` and `RevertPreview`. Updated the one live caller
  (`src-tauri/src/lib.rs`'s `checkpoint_preview_revert` Tauri command) to pass `None` — no GUI caller supplies
  a session yet (CPE-1126 deferred), so the Tauri-facing command signature is unchanged.
- Tests: adjusted the 3 existing `checkpoint_store` tests to pass `None`; added
  `preview_with_session_excludes_only_that_sessions_own_changes_from_drift` (seeds two sessions' audit events,
  asserts the reverting session's own edit is excluded from drift while the other session's edit is not) and
  `preview_with_session_and_no_journal_degrades_to_conservative_behaviour` (no audit dir ever created, no
  panic, same conservative drift count as `None`).
- Verified: `cargo build`, `cargo test` (1072 tests total across `crates/server`, 0 failed), and
  `cargo clippy --all-targets -- -D warnings` all clean.

## Review fix (2026-07-29, Foreman-applied per opus reviewer CHANGES REQUESTED)
- **Safety defect fixed:** the `since_ts` lookup fell back to `.unwrap_or(0)` when a `manifest_id` was absent
  from the on-disk index (a torn/corrupt index row can leave the manifest present but its index entry gone).
  `ts: 0` makes `agent_touched` keep the session's ENTIRE history → a *superset* touched-set → *fewer* drift
  warnings → **strictly less safe than `None`** (a false-negative in a destructive-rollback warning). Replaced
  with a `match` that degrades to the conservative empty touched-set (`BTreeSet::new()`, warn about every
  diverging path) when the index entry is missing — every branch is now ≥ as safe as `None`. Corrected the
  now-false doc lines on `checkpoint_preview_revert` accordingly.
- **Test gap closed:** added `preview_with_session_ignores_pre_checkpoint_events` — seeds a session event dated
  *before* the checkpoint and asserts the post-checkpoint divergence is still flagged as drift, proving the
  real `Checkpoint.ts` bounds attribution (this test would fail under the old `unwrap_or(0)`).
- Re-verified `crates/server`: 1063 lib tests pass (0 failed), `cargo clippy --all-targets -- -D warnings`
  clean. `src-tauri` signature unchanged (reviewer already confirmed it compiles).

## Notes
- Queued in `.claude/workshift-metrics/CHECKPOINT.md` as "CPE-732 optional headless follow-up — thread
  `revert_attribution` into `checkpoint_preview_revert` so drift flags only *truly-outside* changes."
- Distinct from CPE-1127 (manifest-id validation, already Done) — this is the attribution wiring, not path
  hardening.
