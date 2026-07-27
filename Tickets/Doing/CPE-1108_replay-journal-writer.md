---
id: CPE-1108
title: "Activity replay: persist fs-activity to the audit journal (writer)"
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-26
epic: CPE-728
---

## Summary
CPE-728 slice **a** (lowest risk, unblocks everything). The event-replay reconstruction backend is already
built + unit-tested (`replay::state_at`, `replay_view::children_at`, `replay_session::load_replay`) but has
**no durable log to replay** — live `fs-activity` in `flush_fs_batch` emits to the UI but is never persisted.
Add a **host-side writer** that appends each watched-session fs batch to the existing **audit journal**
(CPE-733), so a session's full ordered event log survives beyond the in-memory 300-cap. Design:
`.claude/research-library/entries/activity-replay-event-reconstruction-plan.md` (READ IT).

## Context (verified — file:line)
- `crates/server/src/audit_journal.rs` — append-only JSONL, one file/session, bounded/rotated
  (`MAX_EVENTS_PER_SESSION=10_000`); `AuditEvent{ts,session,kind,path,detail}` (:21-33); `record`/`read_session`/
  `list_sessions`. Fronted by `audit_record`/`audit_sessions`/`audit_read` commands (src-tauri/src/lib.rs:2377-2417),
  dir via `audit_dir(app)` (:2370). `AuditEvent` crosses into `bindings.gen.ts` via `audit_read`.
- `src-tauri/src/lib.rs` `flush_fs_batch` (~:4569-4628) — the per-session pump drains coalesced items and emits
  `ai-console://fs-activity` (~:4625) with `{kind,path,actor}` (+ the `session_id` the pump owns). This is the
  writer seam — `actor`, `kind`, `path`, `session_id`, and `app` are all in hand here.
- Off-means-off: `flush_fs_batch` runs ONLY on the pump thread spawned by `agent_watch_start` (~:4660); no watch
  ⇒ no pump ⇒ no append ⇒ the `audit/` dir need not exist.

## Design (buildable)
1. **Add `actor` to `AuditEvent`** (audit_journal.rs:21): `actor: Option<String>` with `#[serde(default,
   skip_serializing_if="Option::is_none")]` — backward-compatible (old journal lines → `None`; the replay fold
   ignores it). This also lets replay/attribution carry the CPE-1101 actor. Regenerate bindings (AuditEvent is
   bound); drift-guard passes.
2. **`audit_journal::record_many(base, session, &[AuditEvent], max) -> Result<(),String>`** — a thin loop over
   the existing `record` (or record-then-trim-once) so a 50-file batch is one call, not 50. Keep the existing
   per-session cap/rotation.
3. **Writer in `flush_fs_batch`** — where the batch is drained + emitted (~:4625): stamp `ts = epoch_ms(now)`
   (reuse the helper at ~:2387), build one `AuditEvent{ts, session: session_id, kind, path, actor, detail:None}`
   per drained item, and `record_many(&audit_dir(app), &session_id, &events, MAX)`. Do it alongside the existing
   emit (persist AND emit). Reads (`fs-read:` bridge, a different path) are a no-op for listing reconstruction —
   do NOT journal them here (optional follow-up).
4. Errors from `record_many` must NOT break the pump — log/swallow (a failed journal write shouldn't kill the
   live activity view). 

## ⚠ Guardrails
- **Off-means-off**: the append lives only in the pump thread (only exists while a session is armed). No watch
  ⇒ no writes ⇒ no `audit/` dir. `agent_watch_stop_all` drops pumps ⇒ zero residual cost. Confirm nothing
  appends when idle.
- No new deps (audit_journal + std + serde already present). Cross-platform-safe (the journal already sanitizes
  filenames + temp-file+rename). Bounded/rotated by the existing cap.
- `actor` field is additive + backward-compatible — old journals still read.

## Acceptance Criteria
- [ ] `AuditEvent` gains optional `actor`; `record_many` appends a batch; `flush_fs_batch` persists each watched
      fs batch to the session's audit journal (in addition to emitting it). After a watched session,
      `audit_read(session)` returns the full ordered log incl. `actor`; nothing is written when unwatched.
- [ ] A journal-write error doesn't break the live pump/activity view (logged/swallowed).
- [ ] `cargo test -p cpe-server` green (add: `record_many` appends N + respects the cap; `AuditEvent` round-trips
      with and without `actor`); clippy clean (default + `--features index` + sidecar-platform); bindings
      regenerated + drift-guard passes; `npm run check` clean; no new deps.

## Work Log
2026-07-26 (workshift) — CPE-728 slice a, from the filed plan (the replay backend is ~80% built+unwired; this
adds the missing persistence writer). Unblocks 728b (baseline), 728c (replay_load + fold port), 728d (listing
UI). Coordinates with CPE-731b (which gets a SIBLING per-session metrics_journal, not this per-event one).
