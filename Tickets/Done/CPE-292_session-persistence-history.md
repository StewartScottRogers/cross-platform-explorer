---
id: CPE-292
title: Session persistence & history
type: Feature
status: Done
priority: Low
component: Multiple
estimate: 2-3h
created: 2026-07-13
closed: 2026-07-13
---

## Summary

Remember console sessions and transcripts across restarts, per repo, in the
sidecar's own storage namespace ([[CPE-269]]) — so you can reopen where you left
off and review past runs. Secrets are never written to history.

## Acceptance Criteria

- [ ] Sessions (agent, provider, model, cwd) and scrollback persisted per repo in
      the sidecar storage.
- [ ] Reopen restores recent sessions/history; a "clear history" action exists.
- [ ] Transcripts redact any secret values.
- [ ] Bounded/rotated storage so history can't grow unbounded.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-280]], [[CPE-269]]. **Phase:** C6. **Epic:** [[CPE-261]].

## Resolution

Implemented `history` in ai-console: `SessionRecord` (agent/provider/model/cwd/started_at + transcript) and `SessionHistory` (schema-versioned, JSON-persisted via the storage capability). `record()` **redacts secrets** from the transcript (a sidecar-local `redact` scrubbing the injected key values — sidecars can't depend on the host redactor), trims it to a byte cap, and rotates out the oldest past a session cap so history can't grow unbounded. `recent()` (newest first), `clear()`. 4 tests (redaction, rotation, transcript cap, JSON round-trip + clear). 50 crate tests + clippy green.

**Deferred:** binding this to the live storage capability + the history UI land with the launcher/UI ([[CPE-289]]/[[CPE-271]]).

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Implemented session history with redaction + bounds during dayshift. Done.
2026-07-14 — The deferred wiring (live storage capability + history UI) landed in **CPE-370**:
`SessionHistory` is now recorded on session end via `BrokerHistory`, exposed at `/api/history`,
and surfaced in the launcher's "Recent sessions" panel with relaunch.
