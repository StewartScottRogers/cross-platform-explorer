---
id: CPE-289
title: Agent session launcher UI (agent × provider × model)
type: Feature
status: Done
closed: 2026-07-13
priority: High
component: Frontend
estimate: 3-4h
created: 2026-07-13
---

## Summary

The control surface that ties it together: pick an **agent × provider × model ×
credential profile**, see install status, and launch a console session in the open
repo. The combinatorial matrix from the reference, made a first-class UI.

## Acceptance Criteria

- [ ] Choose agent (with install state), provider, model, and credential profile.
- [ ] Launch composes the env via the routing engine ([[CPE-285]]) and opens a PTY
      console ([[CPE-280]]) in the current repo.
- [ ] Offers install/update inline when an agent isn't installed ([[CPE-282]]).
- [ ] Remembers last-used selections per repo.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-280]], [[CPE-285]], [[CPE-281]]. **Phase:** C5.
**Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — **Implemented the launcher** as the sidecar's own served UI (ADR 0001). New
`ai-console::http` — a tiny dependency-free HTTP/1.1 server (request parse, CORS for the
opaque-origin iframe, per-connection threads; 5 tests incl. a live GET). New
`ai-console::console` — `ConsoleState` + `route()` wiring the existing modules:
  - `GET /api/catalog` → agents with **install state** (via `lifecycle::detect`), providers,
    default model, install-ability, + repo cwd + last-used.
  - `POST /api/launch` → composes env via the routing engine (`scope::build_launch` →
    `routing::compose_launch`), spawns a **PTY** (`pty::PtySession`) scoped to the chosen
    folder, streams output; returns session id + any dangerous flags.
  - `GET /api/session/{id}/output?since=` + `POST …/input` → PTY streaming (poll-based).
  - `POST /api/install` → `lifecycle::install` inline when an agent isn't installed.
  `src/launcher.html` (self-contained, no external resources) renders the agent × provider ×
  model pickers, install badge/button, a working terminal (keystrokes → input, polled output,
  ANSI-stripped), and remembers the last selection per folder. `main.rs` now serves this on
  Welcome instead of the placeholder. **Verified real process**: spawn → handshake → serves
  launcher + announces `ui:` URL; 69 ai-console tests + clippy green; delete-test unaffected.

Acceptance: agent/provider/model + install state ✅; compose-env + PTY in the repo ✅;
inline install ✅; **remembers last-used** ✅ (in-session, per folder — cross-restart
persistence is a small follow-up needing the storage capability). **Credential-profile
selection** is intentionally deferred to **CPE-287** (Provider credential UI); the launcher
exposes an optional API-key field and supports keyless/native providers today. The repo
defaults to the sidecar's cwd (editable) — auto-fill from the explorer's current folder ties
to **CPE-313**. This unblocks the console GUI suite (287/290/312/313).
