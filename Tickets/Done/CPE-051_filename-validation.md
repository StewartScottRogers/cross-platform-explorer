---
id: CPE-051
title: Validate file names on rename with a friendly message
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 1h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

`commitRename()` only guards against empty/unchanged names. Names containing characters that are
illegal on Windows (`< > : " / \ | ? *`), Windows reserved device names (CON, PRN, NUL, COM1…,
LPT1…), or a trailing dot/space are passed straight to the backend, which fails with a raw OS error
string. Explorer instead blocks the rename up front with a clear message. Add a pure `validateFileName`
helper and use it to show a friendly notice and abort before invoking the backend.

## Acceptance Criteria

- [ ] Names with any of `< > : " / \ | ? *` are rejected with a message naming the illegal characters
- [ ] Windows reserved device names (case-insensitive, with or without extension) are rejected
- [ ] Trailing space or dot is rejected
- [ ] Empty / whitespace-only is rejected
- [ ] A valid name returns no error (null)
- [ ] `commitRename` shows the message and aborts instead of hitting the backend
- [ ] Unit tests added; `npm run check` clean; full suite green

## Resolution

Added `validateFileName(name)` in `src/lib/filename.ts` returning a friendly message (or null): rejects
empty/whitespace, the Windows illegal chars `< > : " / \ | ? *`, control chars, trailing space/period,
and reserved device names (CON/PRN/NUL/COM1-9/LPT1-9, any casing, with/without extension). Wired into
`commitRename` to show a notice and abort before hitting the backend. 7 unit tests. `npm run check` 0
errors; suite 120 passed; `vite build` clean. Committed on branch, merged to `main`, pushed. GUI verify
(try renaming to `a/b`, see the message) DEFERRED — user present.

## Work Log

2026-07-11 — Nightshift loop: research found commitRename does no character validation; illegal names produce a raw OS error. Decision: validate against the Windows-illegal set (the strict union) even on POSIX, matching Explorer and keeping behavior consistent cross-platform. Logged as a deliberate product assumption since some of these chars are legal on Linux.

## Notes

Backend `rename_entry` remains the final authority; this is a friendlier front-line guard.
