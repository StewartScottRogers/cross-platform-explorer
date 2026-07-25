---
id: CPE-370
title: "Wire session history + relaunch (complete CPE-292; slice of CPE-309)"
type: Feature
status: Done
priority: Medium
component: Backend
tags: [ready]
estimate: 2-3h
created: 2026-07-14
closed: 2026-07-14
---

## Summary

`history.rs` (CPE-292) already implements a redacted, capped, JSON-persistable `SessionHistory`
— but it is **not wired into the console**: nothing records sessions, nothing loads them on
startup, no API exposes them, and the launcher has no history/relaunch UI (verified 2026-07-14 —
zero references to `SessionHistory` outside its own module/tests). Connecting it delivers the
achievable value of [[CPE-309]] — after a sidecar restart the user's sessions + transcripts are
preserved and one-click relaunchable — without the (impossible-here) live PTY reattach.

## Acceptance Criteria

- [x] `Session` carries its launch metadata (agent, provider, model, cwd, started_at) so a
      `SessionRecord` can be built later.
- [x] A persisted history backend (mirror `BrokerPresets` → `history.json` via the storage
      capability) + a `MemHistory` for dev/tests; loaded on startup.
- [x] On session end (the PTY reader-thread EOF), snapshot the `ring` and record a redacted
      `SessionRecord` (reuse `history::redact` with the injected secrets) → persist.
- [x] `GET /api/history` returns recent sessions (id, agent, provider, model, cwd, started_at);
      a detail route returns the stored transcript.
- [x] Launcher "Recent sessions" panel lists them with **Relaunch** (reuses agent+provider+
      model+cwd through the existing launch path). Panel gets the standard visible border.
- [x] Tests: record-on-end, startup load, `/api/history` shape, relaunch reuses metadata.
- [x] Update [[CPE-309]] and [[CPE-292]]: mark the persistence value delivered; leave the live-
      reattach core (needs PTY re-parenting) on CPE-309.

## Notes
Additive — must not change live session I/O. The end-hook runs on the reader thread and does
persistence, so keep it panic-safe and non-blocking. Launcher UI needs a GUI eyeball (can't be
verified headlessly), so land behind the existing platform gate and QA visually.

## Work Log
2026-07-14 — Filed while working the backlog: found CPE-292's `SessionHistory` built but unwired;
this is the implementable value-slice of CPE-309.
2026-07-14 — Implemented & landed. `HistoryBackend` (+ `MemHistory`, `BrokerHistory` over
`history.json`); `SessionMeta` captured at launch and recorded (redacted against the injected key)
on the PTY reader-thread EOF; `GET /api/history` (list) + `/api/history/{id}` (transcript); launcher
"Recent sessions" panel with Relaunch (reuses agent/provider/model/cwd via the shared `launchWith`)
and Transcript, standard visible border. 113 ai-console tests pass (record→list→redacted-detail,
`from_json`, mem backend); clippy clean; build ok.

**Note on acceptance item 1:** metadata is captured in `SessionMeta` and recorded, rather than
stored as a live field on `Session` (that field would be dead — the launcher already holds the id
of the pane it opened, so no running-session listing is needed). Intent (a `SessionRecord` can be
built at end) is met.

**Visual QA pending:** the "Recent sessions" panel can't be verified headlessly — needs a GUI
eyeball (relaunch + transcript render), like other launcher UI.
