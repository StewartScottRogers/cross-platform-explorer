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
- [ ] With `session: None`, `checkpoint_preview_revert` produces byte-identical results to today (existing tests
      at lines ~378/399/420 still pass unchanged, adjusted only for the new arg).
- [ ] With `session: Some(sess)` and a seeded audit journal, a path the agent `sess` mutated at/after the
      checkpoint ts is **NOT** counted as drift, while a path changed by a *different* session (or no session)
      **IS** flagged as drift — proven by a new unit test that seeds the journal + a live-tree divergence.
- [ ] A missing/empty audit journal degrades to the conservative behaviour (no panic; treated as empty
      touched-set).
- [ ] All `crates/server` tests pass; `cargo clippy --all-targets -D warnings` clean.

## Notes
- Queued in `.claude/workshift-metrics/CHECKPOINT.md` as "CPE-732 optional headless follow-up — thread
  `revert_attribution` into `checkpoint_preview_revert` so drift flags only *truly-outside* changes."
- Distinct from CPE-1127 (manifest-id validation, already Done) — this is the attribution wiring, not path
  hardening.
