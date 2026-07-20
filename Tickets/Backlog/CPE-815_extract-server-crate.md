---
id: CPE-815
title: Extract a pure `server` crate (Tauri-free domain logic)
type: refactor
component: Backend
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-20
epic: CPE-810
estimate: 4h+
---

## Summary
Child of CPE-810. Move the domain logic (the ~9k-line command bodies) out of the Tauri app into a
standalone `server` crate that depends only on `ServerCtx` (CPE-814) and the `contract` envelope
(CPE-811) — no Tauri. The Tauri app becomes a **thin adapter**: register handlers, build `TauriCtx`,
dispatch envelopes to the server. This is what makes the Server runnable headless and, later, remote.
Prereqs: CPE-814, CPE-811.

## Acceptance Criteria
- [ ] `server` crate holds the command logic; depends on `contract` + `ServerCtx`, not `tauri`.
- [ ] Tauri app is a thin shim dispatching to the server crate; local behaviour byte-for-byte unchanged.
- [ ] The server crate builds and its logic is unit-testable headless (no Tauri runtime).
- [ ] Local explorer no slower (spot-check vs benchmark); clippy clean both modes; GUI-verified.

## Work Log
