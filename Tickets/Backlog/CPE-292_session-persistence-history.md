---
id: CPE-292
title: Session persistence & history
type: Feature
status: Open
priority: Low
component: Multiple
estimate: 2-3h
created: 2026-07-13
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

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
